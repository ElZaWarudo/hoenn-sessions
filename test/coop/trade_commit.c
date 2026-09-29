#include "global.h"
#include "coop/net_bridge.h"
#include "coop/save.h"
#include "coop/trade_runtime.h"
#include "main.h"
#include "overworld.h"
#include "palette.h"
#include "pokedex.h"
#include "pokemon.h"
#include "script.h"
#include "constants/pokedex.h"
#include "test/test.h"

#define TEST_EPOCH 17
#define OUT_PERSONALITY 0x0BADF00Du
#define OUT_OT_ID 0x22224444u
#define IN_PERSONALITY 0x13579BDFu
#define IN_OT_ID 0x55557777u

struct TradeTestContext
{
    MainCallback callback1;
    MainCallback callback2;
    bool8 paletteFadeActive;
};

static void PumpBridge(void)
{
    gCoopNetBridge.last_sidecar_heartbeat++;
    CoopNetBridge_Poll();
}

/* The overworld may add latest-value PLAYER_STATE frames; skip them. */
static bool8 PopNonPlayerState(struct CoopBridgeMessage *message)
{
    while (CoopNetBridge_DequeueGameToNetwork(message))
    {
        if (message->type != COOP_BRIDGE_MESSAGE_PLAYER_STATE)
            return TRUE;
    }
    return FALSE;
}

static void DeliverInbound(u16 type, u32 sequence, const void *payload, u16 length)
{
    struct CoopBridgeMessage message;

    EXPECT(CoopBridgeMessage_Seal(&message, type, sequence, TEST_EPOCH, payload, length));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    PumpBridge();
}

static void EnterSafeField(struct TradeTestContext *context)
{
    context->callback1 = gMain.callback1;
    context->callback2 = gMain.callback2;
    context->paletteFadeActive = gPaletteFade.active;
    ScriptContext_Init();
    UnlockPlayerFieldControls();
    gMain.callback1 = CB1_Overworld;
    gMain.callback2 = CB2_Overworld;
    gPaletteFade.active = FALSE;
}

static void LeaveSafeField(const struct TradeTestContext *context)
{
    UnlockPlayerFieldControls();
    gMain.callback1 = context->callback1;
    gMain.callback2 = context->callback2;
    gPaletteFade.active = context->paletteFadeActive;
    CoopTradeRuntime_TestSetSaveDryRun(FALSE);
}

static void EstablishSession(void)
{
    struct CoopBridgeMessage message;

    CoopSave_InitializeCurrent();
    CoopNetBridge_Init();
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    DeliverInbound(COOP_BRIDGE_MESSAGE_SESSION_READY, 1, NULL, 0);
    EXPECT(PopNonPlayerState(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    EXPECT(!PopNonPlayerState(&message));
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
    gCoopNetBridge.status_flags &= ~COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
}

static void SetUpParty(void)
{
    ZeroPlayerPartyMons();
    CreateMon(&gPlayerParty[0], SPECIES_TREECKO, 5, 0x01020304, OTID_STRUCT_PRESET(0x0A0B0C0D));
    CreateMon(&gPlayerParty[1], SPECIES_ZIGZAGOON, 4, OUT_PERSONALITY, OTID_STRUCT_PRESET(OUT_OT_ID));
    gPlayerPartyCount = CalculatePlayerPartyCount();
}

static void BuildCommit(u8 *payload, u8 idSeed)
{
    struct Pokemon incoming;
    u32 i;

    memset(payload, 0, COOP_TRADE_COMMIT_SIZE);
    for (i = 0; i < COOP_TRADE_COMMIT_ID_SIZE; i++)
        payload[i] = idSeed + i;
    payload[COOP_TRADE_COMMIT_SLOT_OFFSET] = 1;
    for (i = 0; i < 4; i++)
    {
        payload[COOP_TRADE_COMMIT_OUTGOING_PERSONALITY_OFFSET + i] = OUT_PERSONALITY >> (i * 8);
        payload[COOP_TRADE_COMMIT_OUTGOING_OT_ID_OFFSET + i] = OUT_OT_ID >> (i * 8);
    }
    CreateMon(&incoming, SPECIES_WINGULL, 7, IN_PERSONALITY, OTID_STRUCT_PRESET(IN_OT_ID));
    memcpy(payload + COOP_TRADE_COMMIT_RECORD_OFFSET, &incoming, sizeof(incoming));
}

static bool8 SlotHoldsRecord(u8 slot, const u8 *payload)
{
    return memcmp(&gPlayerParty[slot], payload + COOP_TRADE_COMMIT_RECORD_OFFSET,
                  COOP_TRADE_COMMIT_RECORD_SIZE) == 0;
}

static void ExpectAck(const u8 *payload, struct CoopBridgeMessage *message)
{
    EXPECT(PopNonPlayerState(message));
    EXPECT_EQ(message->type, COOP_BRIDGE_MESSAGE_COMMIT_APPLIED);
    EXPECT_EQ(message->length, COOP_TRADE_COMMIT_APPLIED_SIZE);
    EXPECT_EQ(memcmp(message->payload, payload, COOP_TRADE_COMMIT_APPLIED_SIZE), 0);
}

/* Runs the checkpoint the ROM requests after a trade: CHECKPOINT_READY, a
 * grant, the (dry-run) save and its SAVE_DATA_UPDATED. */
static void CompleteTradeCheckpoint(u32 ackSequence, u32 grantSequence)
{
    struct CoopBridgeMessage message;

    PumpBridge();
    EXPECT(PopNonPlayerState(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_CHECKPOINT_READY);
    EXPECT_GT(message.sequence, ackSequence);
    EXPECT_EQ(CoopTradeRuntime_GetState(), COOP_TRADE_STATE_CHECKPOINT_WAITING);
    EXPECT(ArePlayerFieldControlsLocked());

    DeliverInbound(COOP_BRIDGE_MESSAGE_CHECKPOINT_GRANTED, grantSequence, NULL, 0);
    EXPECT_EQ(CoopTradeRuntime_GetState(), COOP_TRADE_STATE_IDLE);
    EXPECT(!ArePlayerFieldControlsLocked());
    EXPECT(PopNonPlayerState(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_SAVE_DATA_UPDATED);
}

TEST("Cloud Coop trade commit applies the exact record, acks, then checkpoints")
{
    struct TradeTestContext context;
    struct CoopBridgeMessage message;
    u8 payload[COOP_TRADE_COMMIT_SIZE];
    struct Pokemon keptFirst;
    u32 ackSequence;

    EstablishSession();
    EnterSafeField(&context);
    CoopTradeRuntime_TestSetSaveDryRun(TRUE);
    SetUpParty();
    keptFirst = gPlayerParty[0];
    BuildCommit(payload, 0xC0);

    DeliverInbound(COOP_BRIDGE_MESSAGE_TRADE_COMMIT, 2, payload, sizeof(payload));
    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR));
    EXPECT(SlotHoldsRecord(1, payload));
    EXPECT_EQ(memcmp(&gPlayerParty[0], &keptFirst, sizeof(keptFirst)), 0);
    EXPECT_EQ(gPlayerPartyCount, 2);
    EXPECT(GetSetPokedexFlag(SpeciesToNationalPokedexNum(SPECIES_WINGULL), FLAG_GET_CAUGHT));
    EXPECT_EQ(CoopTradeRuntime_GetState(), COOP_TRADE_STATE_CHECKPOINT_OWED);

    /* The acknowledgement is the only frame until the sidecar consumes it:
     * no CHECKPOINT_READY may overtake it. */
    PumpBridge();
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
    EXPECT(ArePlayerFieldControlsLocked());
    ExpectAck(payload, &message);
    ackSequence = message.sequence;
    EXPECT(!PopNonPlayerState(&message));

    CompleteTradeCheckpoint(ackSequence, 3);
    EXPECT(SlotHoldsRecord(1, payload));
    LeaveSafeField(&context);
}

TEST("Cloud Coop repeated trade commit is acknowledged again without applying twice")
{
    struct TradeTestContext context;
    struct CoopBridgeMessage message;
    u8 payload[COOP_TRADE_COMMIT_SIZE];
    struct Pokemon applied[2];

    EstablishSession();
    EnterSafeField(&context);
    CoopTradeRuntime_TestSetSaveDryRun(TRUE);
    SetUpParty();
    BuildCommit(payload, 0xC0);

    DeliverInbound(COOP_BRIDGE_MESSAGE_TRADE_COMMIT, 2, payload, sizeof(payload));
    ExpectAck(payload, &message);
    CompleteTradeCheckpoint(message.sequence, 3);
    memcpy(applied, gPlayerParty, sizeof(applied));

    /* Same boot: the remembered header answers the redelivery. */
    DeliverInbound(COOP_BRIDGE_MESSAGE_TRADE_COMMIT, 4, payload, sizeof(payload));
    ExpectAck(payload, &message);
    EXPECT(!PopNonPlayerState(&message));
    EXPECT_EQ(memcmp(gPlayerParty, applied, sizeof(applied)), 0);
    EXPECT_EQ(CoopTradeRuntime_GetState(), COOP_TRADE_STATE_IDLE);

    /* After a reboot the saved party proves the trade already happened. */
    EstablishSession();
    DeliverInbound(COOP_BRIDGE_MESSAGE_TRADE_COMMIT, 2, payload, sizeof(payload));
    ExpectAck(payload, &message);
    EXPECT_EQ(memcmp(gPlayerParty, applied, sizeof(applied)), 0);
    EXPECT_EQ(CoopTradeRuntime_TestGetRejectedCount(), 0);
    CompleteTradeCheckpoint(message.sequence, 3);
    LeaveSafeField(&context);
}

TEST("Cloud Coop trade commit for a different outgoing Pokemon is dropped unacknowledged")
{
    struct TradeTestContext context;
    struct CoopBridgeMessage message;
    u8 payload[COOP_TRADE_COMMIT_SIZE];
    struct Pokemon before[2];

    EstablishSession();
    EnterSafeField(&context);
    SetUpParty();
    memcpy(before, gPlayerParty, sizeof(before));
    BuildCommit(payload, 0xC0);
    payload[COOP_TRADE_COMMIT_OUTGOING_OT_ID_OFFSET] ^= 1;

    DeliverInbound(COOP_BRIDGE_MESSAGE_TRADE_COMMIT, 2, payload, sizeof(payload));
    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR));
    EXPECT(!PopNonPlayerState(&message));
    EXPECT_EQ(memcmp(gPlayerParty, before, sizeof(before)), 0);
    EXPECT_EQ(CoopTradeRuntime_GetState(), COOP_TRADE_STATE_IDLE);
    EXPECT_EQ(CoopTradeRuntime_TestGetRejectedCount(), 1);
    EXPECT(!ArePlayerFieldControlsLocked());

    /* The slot hint does not rescue a Pokemon that left the party. */
    BuildCommit(payload, 0xD0);
    payload[COOP_TRADE_COMMIT_OUTGOING_PERSONALITY_OFFSET] ^= 1;
    payload[COOP_TRADE_COMMIT_SLOT_OFFSET] = 4;
    DeliverInbound(COOP_BRIDGE_MESSAGE_TRADE_COMMIT, 3, payload, sizeof(payload));
    EXPECT(!PopNonPlayerState(&message));
    EXPECT_EQ(memcmp(gPlayerParty, before, sizeof(before)), 0);
    EXPECT_EQ(CoopTradeRuntime_TestGetRejectedCount(), 2);
    LeaveSafeField(&context);
}

TEST("Cloud Coop trade commit follows the outgoing Pokemon after a party reorder")
{
    struct TradeTestContext context;
    struct CoopBridgeMessage message;
    u8 payload[COOP_TRADE_COMMIT_SIZE];
    struct Pokemon kept;
    struct Pokemon applied[2];

    EstablishSession();
    EnterSafeField(&context);
    CoopTradeRuntime_TestSetSaveDryRun(TRUE);
    SetUpParty();
    /* The trade was offered from slot 1; the player then swapped slots. */
    kept = gPlayerParty[0];
    gPlayerParty[0] = gPlayerParty[1];
    gPlayerParty[1] = kept;
    BuildCommit(payload, 0xC0);
    EXPECT_EQ(payload[COOP_TRADE_COMMIT_SLOT_OFFSET], 1);

    DeliverInbound(COOP_BRIDGE_MESSAGE_TRADE_COMMIT, 2, payload, sizeof(payload));
    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR));
    EXPECT(SlotHoldsRecord(0, payload));
    EXPECT_EQ(memcmp(&gPlayerParty[1], &kept, sizeof(kept)), 0);
    EXPECT_EQ(gPlayerPartyCount, 2);
    EXPECT_EQ(CoopTradeRuntime_TestGetRejectedCount(), 0);

    /* The acknowledgement still echoes the original header, slot hint
     * included, so the launcher matches it to the tracked commit. */
    ExpectAck(payload, &message);
    EXPECT_EQ(message.payload[COOP_TRADE_COMMIT_SLOT_OFFSET], 1);
    CompleteTradeCheckpoint(message.sequence, 3);
    memcpy(applied, gPlayerParty, sizeof(applied));

    /* After a reboot the reordered save is recognized as already traded. */
    EstablishSession();
    DeliverInbound(COOP_BRIDGE_MESSAGE_TRADE_COMMIT, 2, payload, sizeof(payload));
    ExpectAck(payload, &message);
    EXPECT_EQ(memcmp(gPlayerParty, applied, sizeof(applied)), 0);
    EXPECT_EQ(CoopTradeRuntime_TestGetRejectedCount(), 0);
    CompleteTradeCheckpoint(message.sequence, 3);

    /* A slot hint past the party end still finds the outgoing Pokemon. */
    EstablishSession();
    SetUpParty();
    BuildCommit(payload, 0xE0);
    payload[COOP_TRADE_COMMIT_SLOT_OFFSET] = 5;
    DeliverInbound(COOP_BRIDGE_MESSAGE_TRADE_COMMIT, 2, payload, sizeof(payload));
    EXPECT(SlotHoldsRecord(1, payload));
    ExpectAck(payload, &message);
    CompleteTradeCheckpoint(message.sequence, 3);
    LeaveSafeField(&context);
}

TEST("Cloud Coop trade commit rejects a bad checksum and nonzero reserved bytes")
{
    struct TradeTestContext context;
    struct CoopBridgeMessage message;
    u8 payload[COOP_TRADE_COMMIT_SIZE];
    struct Pokemon before[2];

    EstablishSession();
    EnterSafeField(&context);
    SetUpParty();
    memcpy(before, gPlayerParty, sizeof(before));

    /* Corrupt one encrypted substructure byte of the incoming record. */
    BuildCommit(payload, 0xC0);
    payload[COOP_TRADE_COMMIT_RECORD_OFFSET + offsetof(struct BoxPokemon, secure) + 5] ^= 0x40;
    DeliverInbound(COOP_BRIDGE_MESSAGE_TRADE_COMMIT, 2, payload, sizeof(payload));
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR);
    EXPECT(!PopNonPlayerState(&message));
    EXPECT_EQ(memcmp(gPlayerParty, before, sizeof(before)), 0);
    EXPECT_EQ(CoopTradeRuntime_GetState(), COOP_TRADE_STATE_IDLE);

    gCoopNetBridge.status_flags &= ~COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
    BuildCommit(payload, 0xC0);
    payload[COOP_TRADE_COMMIT_RESERVED_OFFSET + 1] = 1;
    DeliverInbound(COOP_BRIDGE_MESSAGE_TRADE_COMMIT, 2, payload, sizeof(payload));
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR);
    EXPECT(!PopNonPlayerState(&message));
    EXPECT_EQ(memcmp(gPlayerParty, before, sizeof(before)), 0);

    /* Short frames and an all-zero record are malformed as well. */
    gCoopNetBridge.status_flags &= ~COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
    BuildCommit(payload, 0xC0);
    DeliverInbound(COOP_BRIDGE_MESSAGE_TRADE_COMMIT, 2, payload, sizeof(payload) - 1);
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR);
    gCoopNetBridge.status_flags &= ~COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
    memset(payload + COOP_TRADE_COMMIT_RECORD_OFFSET, 0, COOP_TRADE_COMMIT_RECORD_SIZE);
    DeliverInbound(COOP_BRIDGE_MESSAGE_TRADE_COMMIT, 2, payload, sizeof(payload));
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR);
    EXPECT(!PopNonPlayerState(&message));
    EXPECT_EQ(memcmp(gPlayerParty, before, sizeof(before)), 0);

    /* The rejected sequence was not consumed: a valid commit still applies. */
    gCoopNetBridge.status_flags &= ~COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
    BuildCommit(payload, 0xC0);
    DeliverInbound(COOP_BRIDGE_MESSAGE_TRADE_COMMIT, 2, payload, sizeof(payload));
    EXPECT(SlotHoldsRecord(1, payload));
    ExpectAck(payload, &message);
    LeaveSafeField(&context);
}

TEST("Cloud Coop trade commit waits for a pending checkpoint and a free field")
{
    struct TradeTestContext context;
    struct CoopBridgeMessage message;
    u8 payload[COOP_TRADE_COMMIT_SIZE];
    struct Pokemon before[2];

    EstablishSession();
    EnterSafeField(&context);
    CoopTradeRuntime_TestSetSaveDryRun(TRUE);
    SetUpParty();
    memcpy(before, gPlayerParty, sizeof(before));
    BuildCommit(payload, 0xC0);

    EXPECT_EQ(CoopNetBridge_RequestCheckpoint(), COOP_CHECKPOINT_REQUEST_STARTED);
    EXPECT(PopNonPlayerState(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_CHECKPOINT_READY);

    DeliverInbound(COOP_BRIDGE_MESSAGE_TRADE_COMMIT, 2, payload, sizeof(payload));
    EXPECT_EQ(CoopTradeRuntime_GetState(), COOP_TRADE_STATE_PENDING);
    EXPECT_EQ(memcmp(gPlayerParty, before, sizeof(before)), 0);
    DeliverInbound(COOP_BRIDGE_MESSAGE_CHECKPOINT_GRANTED, 3, NULL, 0);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_GRANTED);
    EXPECT_EQ(CoopTradeRuntime_GetState(), COOP_TRADE_STATE_PENDING);
    EXPECT_EQ(memcmp(gPlayerParty, before, sizeof(before)), 0);
    EXPECT(!PopNonPlayerState(&message));

    /* The unrelated save fails and the checkpoint returns to idle. */
    EXPECT(CoopNetBridge_ConsumeCheckpointGrant());
    CoopNetBridge_NotifySaveResult(FALSE);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);

    /* A running script or locked controls still defer the apply. */
    LockPlayerFieldControls();
    PumpBridge();
    EXPECT_EQ(CoopTradeRuntime_GetState(), COOP_TRADE_STATE_PENDING);
    EXPECT_EQ(memcmp(gPlayerParty, before, sizeof(before)), 0);
    UnlockPlayerFieldControls();
    gMain.callback2 = NULL;
    PumpBridge();
    EXPECT_EQ(CoopTradeRuntime_GetState(), COOP_TRADE_STATE_PENDING);
    gMain.callback2 = CB2_Overworld;

    PumpBridge();
    EXPECT(SlotHoldsRecord(1, payload));
    ExpectAck(payload, &message);
    gCoopNetBridge.status_flags &= ~COOP_BRIDGE_STATUS_WORLD_NOT_READY;
    CompleteTradeCheckpoint(message.sequence, 4);
    LeaveSafeField(&context);
}

TEST("Cloud Coop unconsumed trade ack is re-sent after a same epoch reconnect")
{
    struct TradeTestContext context;
    struct CoopBridgeMessage message;
    u8 payload[COOP_TRADE_COMMIT_SIZE];

    EstablishSession();
    EnterSafeField(&context);
    CoopTradeRuntime_TestSetSaveDryRun(TRUE);
    SetUpParty();
    BuildCommit(payload, 0xC0);

    DeliverInbound(COOP_BRIDGE_MESSAGE_TRADE_COMMIT, 2, payload, sizeof(payload));
    EXPECT(SlotHoldsRecord(1, payload));

    /* The sidecar reconnects before reading the ack. */
    DeliverInbound(COOP_BRIDGE_MESSAGE_SESSION_READY, 3, NULL, 0);
    EXPECT(PopNonPlayerState(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    PumpBridge();
    ExpectAck(payload, &message);
    EXPECT(!PopNonPlayerState(&message));
    gCoopNetBridge.status_flags &= ~COOP_BRIDGE_STATUS_WORLD_NOT_READY;
    CompleteTradeCheckpoint(message.sequence, 4);
    LeaveSafeField(&context);
}
