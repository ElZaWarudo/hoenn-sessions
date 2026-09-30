#include "global.h"
#include "coop/net_bridge.h"
#include "coop/save.h"
#include "coop/trade_offer.h"
#include "coop/trade_runtime.h"
#include "main.h"
#include "overworld.h"
#include "palette.h"
#include "pokemon.h"
#include "script.h"
#include "string_util.h"
#include "event_data.h"
#include "constants/characters.h"
#include "constants/items.h"
#include "test/test.h"

#define TEST_EPOCH 17
#define MON0_PERSONALITY 0x01020304u
#define MON0_OT_ID 0x0A0B0C0Du
#define MON1_PERSONALITY 0x0BADF00Du
#define MON1_OT_ID 0x22224444u
#define OFFER_TOKEN 0x55667788u

struct OfferTestContext
{
    MainCallback callback1;
    MainCallback callback2;
    bool8 paletteFadeActive;
};

static u32 sInboundSequence;

static void PumpBridge(void)
{
    gCoopNetBridge.last_sidecar_heartbeat++;
    CoopNetBridge_Poll();
}

static bool8 PopNonPlayerState(struct CoopBridgeMessage *message)
{
    while (CoopNetBridge_DequeueGameToNetwork(message))
    {
        if (message->type != COOP_BRIDGE_MESSAGE_PLAYER_STATE)
            return TRUE;
    }
    return FALSE;
}

static void DeliverInbound(u16 type, const void *payload, u16 length)
{
    struct CoopBridgeMessage message;

    EXPECT(CoopBridgeMessage_Seal(&message, type, ++sInboundSequence, TEST_EPOCH,
                                  payload, length));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    PumpBridge();
}

static void EnterSafeField(struct OfferTestContext *context)
{
    context->callback1 = gMain.callback1;
    context->callback2 = gMain.callback2;
    context->paletteFadeActive = gPaletteFade.active;
    ScriptContext_Init();
    UnlockPlayerFieldControls();
    gMain.callback1 = CB1_Overworld;
    gMain.callback2 = CB2_Overworld;
    gPaletteFade.active = FALSE;
    CoopTradeRuntime_TestSetSaveDryRun(TRUE);
}

static void LeaveSafeField(const struct OfferTestContext *context)
{
    ScriptContext_Init();
    UnlockPlayerFieldControls();
    gMain.callback1 = context->callback1;
    gMain.callback2 = context->callback2;
    gPaletteFade.active = context->paletteFadeActive;
    CoopTradeRuntime_TestSetSaveDryRun(FALSE);
}

static void SetUpParty(void)
{
    ZeroPlayerPartyMons();
    CreateMon(&gPlayerParty[0], SPECIES_TREECKO, 5, MON0_PERSONALITY, OTID_STRUCT_PRESET(MON0_OT_ID));
    CreateMon(&gPlayerParty[1], SPECIES_ZIGZAGOON, 4, MON1_PERSONALITY, OTID_STRUCT_PRESET(MON1_OT_ID));
    gPlayerPartyCount = CalculatePlayerPartyCount();
}

/* A ready cloud session in a known group, with a two-Pokemon party. */
static void EstablishGroupedSession(bool8 grouped)
{
    struct CoopBridgeMessage message;
    u8 group[2] = {0};

    CoopSave_InitializeCurrent();
    CoopNetBridge_Init();
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    sInboundSequence = 0;
    DeliverInbound(COOP_BRIDGE_MESSAGE_SESSION_READY, NULL, 0);
    EXPECT(PopNonPlayerState(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    group[0] = grouped;
    DeliverInbound(COOP_BRIDGE_MESSAGE_GROUP_STATE_CHANGED, group, sizeof(group));
    while (PopNonPlayerState(&message))
        ;
    gCoopNetBridge.status_flags &= ~COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
    SetUpParty();
}

static u32 ReadU32(const u8 *bytes)
{
    return (u32)bytes[0] | ((u32)bytes[1] << 8)
         | ((u32)bytes[2] << 16) | ((u32)bytes[3] << 24);
}

static void WriteU32(u8 *bytes, u32 value)
{
    bytes[0] = value;
    bytes[1] = value >> 8;
    bytes[2] = value >> 16;
    bytes[3] = value >> 24;
}

/* Runs the checkpoint a trade offer or accept requests: CHECKPOINT_READY,
 * the grant, the (dry-run) save and its consumed SAVE_DATA_UPDATED. */
static void CompleteCheckpoint(void)
{
    struct CoopBridgeMessage message;

    /* The test field has no encodable player state; a real overworld does. */
    gCoopNetBridge.status_flags &= ~COOP_BRIDGE_STATUS_WORLD_NOT_READY;
    PumpBridge();
    EXPECT(PopNonPlayerState(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_CHECKPOINT_READY);
    DeliverInbound(COOP_BRIDGE_MESSAGE_CHECKPOINT_GRANTED, NULL, 0);
    EXPECT(PopNonPlayerState(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_SAVE_DATA_UPDATED);
    PumpBridge();
}

static void DeliverStatus(u8 role, u8 outcome, u32 request_id, u32 token)
{
    u8 payload[COOP_TRADE_OFFER_STATUS_SIZE] = {0};

    payload[0] = role;
    payload[1] = outcome;
    WriteU32(payload + 4, request_id);
    WriteU32(payload + 8, token);
    DeliverInbound(COOP_BRIDGE_MESSAGE_TRADE_OFFER_STATUS, payload, sizeof(payload));
}

static void DeliverOffer(u8 flags)
{
    u8 payload[COOP_TRADE_OFFER_RECEIVED_SIZE] = {0};
    static const u8 nickname[] = _("SPARKY");

    WriteU32(payload, OFFER_TOKEN);
    payload[4] = SPECIES_WINGULL & 0xFF;
    payload[5] = SPECIES_WINGULL >> 8;
    payload[6] = 12;
    payload[7] = flags;
    memset(payload + 8, EOS, COOP_TRADE_OFFER_NICKNAME_SIZE);
    memcpy(payload + 8, nickname, sizeof(nickname) - 1);
    DeliverInbound(COOP_BRIDGE_MESSAGE_TRADE_OFFER_RECEIVED, payload, sizeof(payload));
}

/* Starts an offer of party slot 1 and returns the request frame's ID. */
static u32 SendOffer(void)
{
    struct CoopBridgeMessage message;
    u32 request_id;

    EXPECT(CoopTradeOffer_BeginOffer(1));
    EXPECT_EQ(CoopTradeOffer_GetState(), COOP_TRADE_OFFER_REQ_CHECKPOINT);
    /* Nothing is offered before the checkpoint that anchors the party. */
    CompleteCheckpoint();
    EXPECT(PopNonPlayerState(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_TRADE_OFFER_REQUEST);
    EXPECT_EQ(message.length, COOP_TRADE_OFFER_REQUEST_SIZE);
    EXPECT_EQ(message.payload[0], COOP_TRADE_OFFER_ACTION_OFFER);
    EXPECT_EQ(message.payload[1], 1);
    EXPECT_EQ(message.payload[2], 0);
    EXPECT_EQ(message.payload[3], 0);
    request_id = ReadU32(message.payload + 4);
    EXPECT_NE(request_id, 0);
    EXPECT_EQ(ReadU32(message.payload + 8), MON1_PERSONALITY);
    EXPECT_EQ(ReadU32(message.payload + 12), MON1_OT_ID);
    EXPECT_EQ(CoopTradeOffer_GetState(), COOP_TRADE_OFFER_REQ_REPLY);
    return request_id;
}

TEST("Cloud Coop trade offer entry needs a grouped idle session and two Pokemon")
{
    struct OfferTestContext context;

    EstablishGroupedSession(FALSE);
    EnterSafeField(&context);
    EXPECT(!CoopTradeOffer_CanBegin());
    EXPECT(!CoopTradeOffer_BeginOffer(1));
    EXPECT_EQ(CoopTradeOffer_GetResult(), COOP_TRADE_OUTCOME_PARTNER_UNAVAILABLE);
    CoopTradeOffer_Finish();

    EstablishGroupedSession(TRUE);
    EXPECT(CoopTradeOffer_CanBegin());
    ZeroMonData(&gPlayerParty[1]);
    gPlayerPartyCount = CalculatePlayerPartyCount();
    EXPECT(!CoopTradeOffer_CanBegin());
    SetUpParty();
    EXPECT(CoopTradeOffer_CanBegin());
    /* One flow at a time. */
    EXPECT(CoopTradeOffer_BeginOffer(1));
    EXPECT(!CoopTradeOffer_CanBegin());
    LeaveSafeField(&context);
}

TEST("Cloud Coop trade offer refuses mail and the last usable Pokemon at selection")
{
    struct OfferTestContext context;
    struct CoopBridgeMessage message;
    u16 mail = ITEM_ORANGE_MAIL;

    EstablishGroupedSession(TRUE);
    EnterSafeField(&context);
    SetMonData(&gPlayerParty[1], MON_DATA_HELD_ITEM, &mail);
    EXPECT(!CoopTradeOffer_BeginOffer(1));
    EXPECT_EQ(CoopTradeOffer_GetResult(), COOP_TRADE_OUTCOME_MAIL);
    EXPECT_EQ(StringCompare(CoopTradeOffer_GetResultText(),
                            COMPOUND_STRING("A POKéMON holding MAIL\ncan't be traded.")), 0);
    CoopTradeOffer_Finish();
    PumpBridge();
    EXPECT(!PopNonPlayerState(&message));

    SetUpParty();
    {
        bool8 egg = TRUE;
        SetMonData(&gPlayerParty[0], MON_DATA_IS_EGG, &egg);
    }
    EXPECT(!CoopTradeOffer_BeginOffer(1));
    EXPECT_EQ(CoopTradeOffer_GetResult(), COOP_TRADE_RESULT_LAST_MON);
    CoopTradeOffer_Finish();
    PumpBridge();
    EXPECT(!PopNonPlayerState(&message));
    LeaveSafeField(&context);
}

TEST("Cloud Coop trade offer checkpoints, sends the exact request and waits without applying")
{
    struct OfferTestContext context;
    struct Pokemon before[2];
    u32 request_id;

    EstablishGroupedSession(TRUE);
    EnterSafeField(&context);
    memcpy(before, gPlayerParty, sizeof(before));
    request_id = SendOffer();

    /* A status for another request changes nothing. */
    DeliverStatus(COOP_TRADE_OFFER_ROLE_REQUESTER, COOP_TRADE_OUTCOME_PENDING,
                  request_id + 1, OFFER_TOKEN);
    EXPECT_EQ(CoopTradeOffer_GetState(), COOP_TRADE_OFFER_REQ_REPLY);
    DeliverStatus(COOP_TRADE_OFFER_ROLE_REQUESTER, COOP_TRADE_OUTCOME_PENDING,
                  request_id, OFFER_TOKEN);
    EXPECT_EQ(CoopTradeOffer_GetState(), COOP_TRADE_OFFER_REQ_PENDING);
    EXPECT(!CoopTradeOffer_WaitStep(0));

    /* Accepted: the ROM waits for the ledger's TradeCommit and never changes
     * the party on its own. */
    DeliverStatus(COOP_TRADE_OFFER_ROLE_REQUESTER, COOP_TRADE_OUTCOME_ACCEPTED,
                  request_id, OFFER_TOKEN);
    EXPECT_EQ(CoopTradeOffer_GetState(), COOP_TRADE_OFFER_COMMIT_WAIT);
    EXPECT(!CoopTradeOffer_WaitStep(0));
    gMain.vblankCounter1 += COOP_TRADE_OFFER_COMMIT_FRAMES + 1;
    PumpBridge();
    EXPECT(CoopTradeOffer_WaitStep(0));
    EXPECT_EQ(CoopTradeOffer_GetResult(), COOP_TRADE_OUTCOME_ACCEPTED);
    EXPECT_EQ(memcmp(before, gPlayerParty, sizeof(before)), 0);
    EXPECT_EQ(CoopTradeRuntime_GetState(), COOP_TRADE_STATE_IDLE);
    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR));
    CoopTradeOffer_Finish();
    EXPECT_EQ(CoopTradeOffer_GetState(), COOP_TRADE_OFFER_IDLE);
    LeaveSafeField(&context);
}

TEST("Cloud Coop trade offer outcomes and failures become short messages")
{
    struct OfferTestContext context;
    u32 request_id;

    EstablishGroupedSession(TRUE);
    EnterSafeField(&context);
    request_id = SendOffer();
    DeliverStatus(COOP_TRADE_OFFER_ROLE_REQUESTER, COOP_TRADE_OUTCOME_PENDING,
                  request_id, OFFER_TOKEN);
    DeliverStatus(COOP_TRADE_OFFER_ROLE_REQUESTER, COOP_TRADE_OUTCOME_DECLINED,
                  request_id, OFFER_TOKEN);
    EXPECT(CoopTradeOffer_WaitStep(0));
    EXPECT_EQ(StringCompare(CoopTradeOffer_GetResultText(),
                            COMPOUND_STRING("Your partner declined the trade.")), 0);
    CoopTradeOffer_Finish();

    /* A launcher failure before any server offer names no token. */
    request_id = SendOffer();
    DeliverStatus(COOP_TRADE_OFFER_ROLE_REQUESTER, COOP_TRADE_OUTCOME_BUSY, request_id, 0);
    EXPECT(CoopTradeOffer_WaitStep(0));
    EXPECT_EQ(StringCompare(CoopTradeOffer_GetResultText(),
                            COMPOUND_STRING("Another trade is in progress.")), 0);
    CoopTradeOffer_Finish();

    request_id = SendOffer();
    DeliverStatus(COOP_TRADE_OFFER_ROLE_REQUESTER, COOP_TRADE_OUTCOME_PARTNER_UNAVAILABLE,
                  request_id, 0);
    EXPECT_EQ(CoopTradeOffer_GetResult(), COOP_TRADE_OUTCOME_PARTNER_UNAVAILABLE);
    CoopTradeOffer_Finish();

    /* No answer from the launcher at all. */
    request_id = SendOffer();
    gMain.vblankCounter1 += COOP_TRADE_OFFER_REPLY_FRAMES + 1;
    PumpBridge();
    EXPECT_EQ(CoopTradeOffer_GetResult(), COOP_TRADE_OUTCOME_UNAVAILABLE);
    CoopTradeOffer_Finish();

    /* A malformed status is a protocol error. */
    {
        u8 payload[COOP_TRADE_OFFER_STATUS_SIZE] = {0};
        payload[0] = COOP_TRADE_OFFER_ROLE_RESPONDER;
        payload[1] = COOP_TRADE_OUTCOME_PENDING;
        WriteU32(payload + 8, OFFER_TOKEN);
        DeliverInbound(COOP_BRIDGE_MESSAGE_TRADE_OFFER_STATUS, payload, sizeof(payload));
        EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR);
    }
    LeaveSafeField(&context);
}

TEST("Cloud Coop trade offer B cancels the pending offer and the partner not answering withdraws it")
{
    struct OfferTestContext context;
    struct CoopBridgeMessage message;
    u32 request_id;

    EstablishGroupedSession(TRUE);
    EnterSafeField(&context);
    request_id = SendOffer();
    /* B does nothing before the server holds the offer. */
    EXPECT(!CoopTradeOffer_WaitStep(B_BUTTON));
    EXPECT_EQ(CoopTradeOffer_GetState(), COOP_TRADE_OFFER_REQ_REPLY);
    DeliverStatus(COOP_TRADE_OFFER_ROLE_REQUESTER, COOP_TRADE_OUTCOME_PENDING,
                  request_id, OFFER_TOKEN);
    EXPECT(!CoopTradeOffer_WaitStep(B_BUTTON));
    EXPECT_EQ(CoopTradeOffer_GetState(), COOP_TRADE_OFFER_REQ_CANCEL);
    EXPECT(PopNonPlayerState(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_TRADE_OFFER_REQUEST);
    EXPECT_EQ(message.payload[0], COOP_TRADE_OFFER_ACTION_CANCEL);
    EXPECT_EQ(message.payload[1], 0);
    EXPECT_EQ(ReadU32(message.payload + 4), request_id);
    EXPECT_EQ(ReadU32(message.payload + 8), 0);
    EXPECT_EQ(ReadU32(message.payload + 12), 0);
    DeliverStatus(COOP_TRADE_OFFER_ROLE_REQUESTER, COOP_TRADE_OUTCOME_CANCELLED,
                  request_id, OFFER_TOKEN);
    EXPECT(CoopTradeOffer_WaitStep(0));
    EXPECT_EQ(StringCompare(CoopTradeOffer_GetResultText(), COMPOUND_STRING("Trade cancelled.")), 0);
    CoopTradeOffer_Finish();

    /* The partner never answers: the ROM gives up and withdraws. */
    request_id = SendOffer();
    DeliverStatus(COOP_TRADE_OFFER_ROLE_REQUESTER, COOP_TRADE_OUTCOME_PENDING,
                  request_id, OFFER_TOKEN);
    gMain.vblankCounter1 += COOP_TRADE_OFFER_WAIT_FRAMES + 1;
    PumpBridge();
    EXPECT_EQ(CoopTradeOffer_GetResult(), COOP_TRADE_RESULT_NO_ANSWER);
    EXPECT(PopNonPlayerState(&message));
    EXPECT_EQ(message.payload[0], COOP_TRADE_OFFER_ACTION_CANCEL);
    EXPECT_EQ(ReadU32(message.payload + 4), request_id);
    CoopTradeOffer_Finish();

    /* Transport loss while waiting: nothing half-applies, and the offer is
     * withdrawn once the session is back. */
    request_id = SendOffer();
    DeliverStatus(COOP_TRADE_OFFER_ROLE_REQUESTER, COOP_TRADE_OUTCOME_PENDING,
                  request_id, OFFER_TOKEN);
    CoopTradeOffer_OnTransportLost();
    EXPECT(CoopTradeOffer_WaitStep(0));
    EXPECT_EQ(CoopTradeOffer_GetResult(), COOP_TRADE_OUTCOME_UNAVAILABLE);
    CoopTradeOffer_Finish();
    PumpBridge();
    EXPECT(PopNonPlayerState(&message));
    EXPECT_EQ(message.payload[0], COOP_TRADE_OFFER_ACTION_CANCEL);
    EXPECT_EQ(ReadU32(message.payload + 4), request_id);
    LeaveSafeField(&context);
}

TEST("Cloud Coop received trade offer prompts on a free field and declines by No")
{
    struct OfferTestContext context;
    struct CoopBridgeMessage message;

    EstablishGroupedSession(TRUE);
    EnterSafeField(&context);
    /* Deferred while the field is busy. */
    LockPlayerFieldControls();
    DeliverOffer(0);
    EXPECT_EQ(CoopTradeOffer_GetState(), COOP_TRADE_OFFER_RSP_RECEIVED);
    PumpBridge();
    EXPECT_EQ(CoopTradeOffer_GetState(), COOP_TRADE_OFFER_RSP_RECEIVED);
    UnlockPlayerFieldControls();
    PumpBridge();
    EXPECT_EQ(CoopTradeOffer_GetState(), COOP_TRADE_OFFER_RSP_PROMPT);
    EXPECT(ScriptContext_IsEnabled());

    Special_CoopTradeBufferOffer();
    EXPECT_EQ(gSpecialVar_Result, 1);
    EXPECT_EQ(StringCompare(gStringVar1, COMPOUND_STRING("SPARKY")), 0);
    EXPECT_EQ(StringCompare(gStringVar2, GetSpeciesName(SPECIES_WINGULL)), 0);
    EXPECT_EQ(StringCompare(gStringVar3, COMPOUND_STRING("12")), 0);

    /* A redelivery of the same offer is absorbed. */
    DeliverOffer(0);
    EXPECT_EQ(CoopTradeOffer_GetState(), COOP_TRADE_OFFER_RSP_PROMPT);

    EXPECT_EQ(CoopTradeOffer_Respond(PARTY_SIZE), 2);
    EXPECT(PopNonPlayerState(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_TRADE_OFFER_DECISION);
    EXPECT_EQ(message.length, COOP_TRADE_OFFER_DECISION_SIZE);
    EXPECT_EQ(message.payload[0], COOP_TRADE_OFFER_DECLINE);
    EXPECT_EQ(message.payload[1], 0);
    EXPECT_EQ(ReadU32(message.payload + 4), OFFER_TOKEN);
    EXPECT_EQ(ReadU32(message.payload + 8), 0);
    EXPECT_EQ(CoopTradeOffer_GetState(), COOP_TRADE_OFFER_IDLE);
    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR));
    LeaveSafeField(&context);
}

TEST("Cloud Coop received trade offer accept checkpoints then sends the chosen Pokemon")
{
    struct OfferTestContext context;
    struct CoopBridgeMessage message;
    struct Pokemon before[2];

    EstablishGroupedSession(TRUE);
    EnterSafeField(&context);
    memcpy(before, gPlayerParty, sizeof(before));
    DeliverOffer(COOP_TRADE_OFFER_FLAG_EGG);
    PumpBridge();
    Special_CoopTradeBufferOffer();
    EXPECT_EQ(gSpecialVar_Result, 2);

    EXPECT_EQ(CoopTradeOffer_Respond(0), 1);
    EXPECT_EQ(CoopTradeOffer_GetState(), COOP_TRADE_OFFER_RSP_CHECKPOINT);
    CompleteCheckpoint();
    EXPECT(PopNonPlayerState(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_TRADE_OFFER_DECISION);
    EXPECT_EQ(message.payload[0], COOP_TRADE_OFFER_ACCEPT);
    EXPECT_EQ(message.payload[1], 0);
    EXPECT_EQ(message.payload[2], 0);
    EXPECT_EQ(message.payload[3], 0);
    EXPECT_EQ(ReadU32(message.payload + 4), OFFER_TOKEN);
    EXPECT_EQ(ReadU32(message.payload + 8), MON0_PERSONALITY);
    EXPECT_EQ(ReadU32(message.payload + 12), MON0_OT_ID);
    EXPECT_EQ(CoopTradeOffer_GetState(), COOP_TRADE_OFFER_RSP_REPLY);

    DeliverStatus(COOP_TRADE_OFFER_ROLE_RESPONDER, COOP_TRADE_OUTCOME_ACCEPTED, 0, OFFER_TOKEN);
    EXPECT_EQ(CoopTradeOffer_GetState(), COOP_TRADE_OFFER_COMMIT_WAIT);
    EXPECT_EQ(memcmp(before, gPlayerParty, sizeof(before)), 0);
    gMain.vblankCounter1 += COOP_TRADE_OFFER_COMMIT_FRAMES + 1;
    PumpBridge();
    EXPECT(CoopTradeOffer_WaitStep(0));
    EXPECT_EQ(StringCompare(CoopTradeOffer_GetResultText(),
                            COMPOUND_STRING("The trade was accepted!")), 0);
    EXPECT_EQ(memcmp(before, gPlayerParty, sizeof(before)), 0);
    CoopTradeOffer_Finish();
    LeaveSafeField(&context);
}

TEST("Cloud Coop received trade offer expires at its deadline and when withdrawn")
{
    struct OfferTestContext context;
    struct CoopBridgeMessage message;

    EstablishGroupedSession(TRUE);
    EnterSafeField(&context);
    DeliverOffer(0);
    PumpBridge();
    EXPECT_EQ(CoopTradeOffer_GetState(), COOP_TRADE_OFFER_RSP_PROMPT);
    gMain.vblankCounter1 += COOP_TRADE_OFFER_PROMPT_FRAMES + 1;
    EXPECT_EQ(CoopTradeOffer_Respond(0), 0);
    EXPECT_EQ(CoopTradeOffer_GetResult(), COOP_TRADE_OUTCOME_EXPIRED);
    EXPECT(PopNonPlayerState(&message));
    EXPECT_EQ(message.payload[0], COOP_TRADE_OFFER_DECLINE);
    CoopTradeOffer_Finish();
    ScriptContext_Init();

    /* Never shown in time: declined silently. */
    LockPlayerFieldControls();
    DeliverOffer(0);
    gMain.vblankCounter1 += COOP_TRADE_OFFER_PROMPT_FRAMES + 1;
    PumpBridge();
    EXPECT_EQ(CoopTradeOffer_GetState(), COOP_TRADE_OFFER_IDLE);
    EXPECT(PopNonPlayerState(&message));
    EXPECT_EQ(message.payload[0], COOP_TRADE_OFFER_DECLINE);
    UnlockPlayerFieldControls();

    /* The requester withdraws while the prompt is up. */
    DeliverOffer(0);
    PumpBridge();
    DeliverStatus(COOP_TRADE_OFFER_ROLE_RESPONDER, COOP_TRADE_OUTCOME_CANCELLED, 0, OFFER_TOKEN);
    Special_CoopTradeBufferOffer();
    EXPECT_EQ(gSpecialVar_Result, 0);
    EXPECT_EQ(StringCompare(CoopTradeOffer_GetResultText(),
                            COMPOUND_STRING("Your partner withdrew the offer.")), 0);
    CoopTradeOffer_Finish();
    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR));
    LeaveSafeField(&context);
}
