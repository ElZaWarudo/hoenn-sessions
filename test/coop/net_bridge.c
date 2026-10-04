#include <stddef.h>

#include "global.h"
#include "battle.h"
#include "battle_setup.h"
#include "coop/battle_consent.h"
#include "coop/battle_runtime.h"
#include "coop/group_travel.h"
#include "coop/net_bridge.h"
#include "coop/presence_runtime.h"
#include "coop/region.h"
#include "coop/generated_regional_identities.h"
#include "coop/save.h"
#include "constants/opponents.h"
#include "constants/johto_content.h"
#include "event_data.h"
#include "event_object_lock.h"
#include "gba/flash_internal.h"
#include "fieldmap.h"
#include "load_save.h"
#include "main.h"
#include "overworld.h"
#include "palette.h"
#include "pokemon.h"
#include "save.h"
#include "script.h"
#include "test/test.h"

static void EstablishTestCloudSession(void);
static void DeliverTestBattleRecord(u16 type, u32 sequence, const u8 *payload, u16 length);

_Static_assert(sizeof(struct CoopBridgeMessage) == 144, "tested message ABI size");
_Static_assert(offsetof(struct CoopBridgeMessage, type) == 0, "tested message type offset");
_Static_assert(offsetof(struct CoopBridgeMessage, length) == 2, "tested message length offset");
_Static_assert(offsetof(struct CoopBridgeMessage, sequence) == 4, "tested message sequence offset");
_Static_assert(offsetof(struct CoopBridgeMessage, session_epoch) == 8, "tested message epoch offset");
_Static_assert(offsetof(struct CoopBridgeMessage, payload) == 12, "tested message payload offset");
_Static_assert(offsetof(struct CoopBridgeMessage, checksum) == 140, "tested message checksum offset");
_Static_assert(sizeof(struct CoopBridgeQueue) == 4612, "tested queue ABI size");
_Static_assert(offsetof(struct CoopBridgeQueue, entries) == 4, "tested queue entries offset");
_Static_assert(offsetof(struct CoopNetBridge, status_flags) == 12, "tested bridge status offset");
_Static_assert(offsetof(struct CoopNetBridge, last_sidecar_heartbeat) == 16, "tested heartbeat offset");
_Static_assert(offsetof(struct CoopNetBridge, game_to_network) == 20, "tested outbound queue offset");
_Static_assert(offsetof(struct CoopNetBridge, network_to_game) == 4632, "tested inbound queue offset");
_Static_assert(sizeof(struct CoopNetBridge) == 9244, "tested bridge ABI size");
_Static_assert(sizeof(struct CoopBridgePlayerState) == COOP_PRESENCE_LOCAL_STATE_SIZE, "tested player-state ABI size");

static struct CoopBridgeQueue *GetTestQueue(void)
{
    return &gCoopNetBridge.game_to_network;
}

static void SealTestMessage(struct CoopBridgeMessage *message, u16 type, u32 sequence, u32 sessionEpoch)
{
    static const u8 sPayload[] = {0x12, 0x34, 0x56, 0x78};

    EXPECT(CoopBridgeMessage_Seal(message,
                                  type,
                                  sequence,
                                  sessionEpoch,
                                  sPayload,
                                  sizeof(sPayload)));
}

static void PopInitialRomReady(void)
{
    struct CoopBridgeMessage message;

    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
}

static void InitTestBridge(void)
{
    CoopSave_InitializeCurrent();
    CoopNetBridge_Init();
}

static void HostWriteInboundUnchecked(const struct CoopBridgeMessage *message)
{
    struct CoopBridgeQueue *queue = &gCoopNetBridge.network_to_game;
    u16 index = queue->write_index & (COOP_NET_BRIDGE_QUEUE_CAPACITY - 1);

    queue->entries[index] = *message;
    queue->write_index++;
}

static u32 sSaveSectorProgramCalls;
#if ROM_WORLD == 1
static EWRAM_DATA u16 sPresenceBridgeMapData[20 * 20];
#endif // ROM_WORLD == 1

static void EstablishTestCloudSession(void);

static u16 CountSaveSectorProgramCalls(u16 sector, u8 *data)
{
    (void)sector;
    (void)data;
    sSaveSectorProgramCalls++;
    return 1;
}

TEST("Cloud Coop wire ABI matches the documented compact layout")
{
    EXPECT_EQ(sizeof(struct CoopBridgeMessage), 144);
    EXPECT_EQ(offsetof(struct CoopBridgeMessage, type), 0);
    EXPECT_EQ(offsetof(struct CoopBridgeMessage, length), 2);
    EXPECT_EQ(offsetof(struct CoopBridgeMessage, sequence), 4);
    EXPECT_EQ(offsetof(struct CoopBridgeMessage, session_epoch), 8);
    EXPECT_EQ(offsetof(struct CoopBridgeMessage, payload), 12);
    EXPECT_EQ(offsetof(struct CoopBridgeMessage, checksum), 140);
    EXPECT_EQ(sizeof(struct CoopBridgeQueue), 4612);
    EXPECT_EQ(offsetof(struct CoopBridgeQueue, entries), 4);
    EXPECT_EQ(offsetof(struct CoopNetBridge, status_flags), 12);
    EXPECT_EQ(offsetof(struct CoopNetBridge, last_sidecar_heartbeat), 16);
    EXPECT_EQ(offsetof(struct CoopNetBridge, game_to_network), 20);
    EXPECT_EQ(offsetof(struct CoopNetBridge, network_to_game), 4632);
    EXPECT_EQ(sizeof(struct CoopNetBridge), 9244);
    EXPECT_EQ(sizeof(struct CoopBridgePlayerState), COOP_PRESENCE_LOCAL_STATE_SIZE);
    EXPECT_EQ(COOP_NET_BRIDGE_GAME_PROTOCOL_VERSION, 5);
    EXPECT_EQ(COOP_BRIDGE_MESSAGE_PAIRING_REQUEST, 18);
    EXPECT_EQ(COOP_BRIDGE_MESSAGE_PROGRESS_OBSERVATION, 19);
    EXPECT_EQ(COOP_BRIDGE_MESSAGE_PORTAL_TRAVEL_REQUEST, 24);
    EXPECT_EQ(COOP_BRIDGE_MESSAGE_ARRIVAL_PROOF, 25);
    EXPECT_EQ(COOP_BRIDGE_MESSAGE_REMOTE_INTERACTION, 0x0111);
    EXPECT_EQ(COOP_BRIDGE_MESSAGE_ARRIVAL_CHALLENGE, 0x011C);
}

TEST("Cloud Coop CRC32 matches the canonical check vector")
{
    static const char sCheckVector[] = "123456789";

    EXPECT_EQ(CoopBridge_Crc32(sCheckVector, sizeof(sCheckVector) - 1), 0xCBF43926u);
    EXPECT_EQ(CoopBridge_Crc32(NULL, 0), 0);
    EXPECT_EQ(CoopBridge_Crc32(NULL, 1), 0);
}

TEST("Cloud Coop Online wire types cross the bridge boundary")
{
    struct CoopBridgeMessage message;
    u8 request[12] = {1};
    u8 status[112] = {1};

    EXPECT(CoopBridgeMessage_Seal(&message, 14, 2, 1, request, sizeof(request)));
    EXPECT(CoopBridgeMessage_Validate(&message));
    EXPECT(CoopBridgeMessage_Seal(&message, 0x010D, 3, 1, status, sizeof(status)));
    EXPECT(CoopBridgeMessage_Validate(&message));
}

static void InitOnlineTestBridge(void)
{
    struct CoopBridgeMessage message;

    InitTestBridge();
    PopInitialRomReady();
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_SESSION_READY,
                                 1, 7, NULL, 0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    while (CoopNetBridge_DequeueGameToNetwork(&message))
        ;
}

TEST("Cloud Coop Online request binds the displayed view and uses fixed bytes")
{
    struct CoopOnlineRequest request = { .request_id = 1, .view_id = 9,
                                        .action = COOP_ONLINE_INVITE, .page = 2 };
    struct CoopBridgeMessage message;

    InitOnlineTestBridge();
    EXPECT(CoopNetBridge_SendOnlineRequest(&request));
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ONLINE_REQUEST);
    EXPECT_EQ(message.length, 12);
    EXPECT_EQ(message.payload[0], 1);
    EXPECT_EQ(message.payload[4], 9);
    EXPECT_EQ(message.payload[8], COOP_ONLINE_INVITE);
    EXPECT_EQ(message.payload[9], 2);
    EXPECT_EQ(message.payload[10], 0);
    EXPECT_EQ(message.payload[11], 0);
    EXPECT(!CoopNetBridge_SendOnlineRequest(&request));
    request.request_id = 2;
    request.view_id = 0;
    EXPECT(!CoopNetBridge_SendOnlineRequest(&request));
    request.action = COOP_ONLINE_REFRESH;
    EXPECT(CoopNetBridge_SendOnlineRequest(&request));
}

TEST("Cloud Coop battle consent matches a reservation nonce before accepting its offer")
{
    struct CoopBridgeMessage message;
    u8 offer[COOP_BATTLE_JOIN_OFFER_SIZE] = {0};

    InitOnlineTestBridge();
    EXPECT(CoopBattleConsent_Begin(COOP_BATTLE_KIND_FRIENDLY));
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_TRAINER_BATTLE_RESERVE);
    EXPECT_EQ(message.length, COOP_BATTLE_RESERVE_SIZE);
    EXPECT_EQ(message.payload[0], COOP_BATTLE_KIND_FRIENDLY);
    EXPECT(message.payload[1] || message.payload[2]
        || message.payload[3] || message.payload[4]);

    offer[0] = 1; // Nonzero battle UUID.
    offer[16] = COOP_BATTLE_KIND_FRIENDLY;
    offer[COOP_BATTLE_JOIN_OFFER_RULES_OFFSET] = COOP_BATTLE_FRIENDLY_SINGLES; offer[COOP_BATTLE_JOIN_OFFER_RULES_OFFSET + 2] = 1;
    offer[17] = 0; // Requester.
    memcpy(&offer[18], &message.payload[1], 4);
    offer[18] ^= 1;
    EXPECT(!CoopBattleConsent_ReceiveOffer(offer, sizeof(offer)));
    offer[18] ^= 1;
    EXPECT(CoopBattleConsent_ReceiveOffer(offer, sizeof(offer)));
    EXPECT(CoopBattleConsent_ReceiveOffer(offer, sizeof(offer)));
    offer[0] = 2;
    EXPECT(!CoopBattleConsent_ReceiveOffer(offer, sizeof(offer)));
}

TEST("Cloud Coop trainer reserve uses the active regional identity ordinal")
{
    struct CoopBridgeMessage message;
    struct MapHeader savedMapHeader = gMapHeader;

    InitOnlineTestBridge();
    gMapHeader.engineRegion = COOP_MAP_ENGINE_REGION_HOENN;
    gMapHeader.regionMapSectionId = MAPSEC_LITTLEROOT_TOWN;
    EXPECT(!CoopBattleConsent_Begin(COOP_BATTLE_KIND_COOPERATIVE_TRAINER));
    EXPECT(!CoopBattleConsent_BeginTrainer(0));
    EXPECT(CoopBattleConsent_BeginTrainer(TRAINER_WALLY_VR_1));
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_TRAINER_BATTLE_RESERVE);
    EXPECT_EQ(message.length, COOP_BATTLE_TRAINER_RESERVE_SIZE);
    EXPECT_EQ(message.payload[0], COOP_BATTLE_KIND_COOPERATIVE_TRAINER);
    EXPECT_EQ(message.payload[5], COOP_REGION_HOENN);
    EXPECT_EQ(message.payload[6] | ((u16)message.payload[7] << 8),
              COOP_TRAINER_HOENN_TRAINER_WALLY_1_ORDINAL);

    InitOnlineTestBridge();
    gMapHeader.engineRegion = COOP_MAP_ENGINE_REGION_KANTO;
    gMapHeader.regionMapSectionId = MAPSEC_PALLET_TOWN;
    EXPECT(!CoopBattleConsent_BeginTrainer(TRAINER_WALLY_VR_1));
    EXPECT(!CoopNetBridge_DequeueGameToNetwork(&message));
    gMapHeader = savedMapHeader;
}

TEST("Cloud Coop battle consent keeps a pending reserve through transport downtime")
{
    struct CoopBridgeMessage message;
    u8 offer[COOP_BATTLE_JOIN_OFFER_SIZE] = {0};

    InitOnlineTestBridge();
    EXPECT(CoopBattleConsent_Begin(COOP_BATTLE_KIND_FRIENDLY));
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    CoopBattleConsent_OnTransportLost();
    gMain.vblankCounter1 += 36 * 60;
    CoopBattleConsent_Poll();
    CoopBattleConsent_OnSessionReady();
    offer[0] = 1;
    offer[16] = COOP_BATTLE_KIND_FRIENDLY;
    offer[COOP_BATTLE_JOIN_OFFER_RULES_OFFSET] = COOP_BATTLE_FRIENDLY_SINGLES; offer[COOP_BATTLE_JOIN_OFFER_RULES_OFFSET + 2] = 1;
    memcpy(&offer[18], &message.payload[1], 4);
    EXPECT(CoopBattleConsent_ReceiveOffer(offer, sizeof(offer)));
}

TEST("Cloud Coop invitation notice is bounded and consumed once")
{
    struct CoopBridgeMessage message;
    u8 name[COOP_ONLINE_NAME_SIZE] = {0};
    InitOnlineTestBridge();
    memcpy(name, "may", 3);
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_GROUP_INVITE_RECEIVED,
                                 2, 7, name, sizeof(name)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_TakeInviteNotice());
    EXPECT(!CoopNetBridge_TakeInviteNotice());
    name[5] = 'x'; // Noncanonical bytes after the zero terminator.
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_GROUP_INVITE_RECEIVED,
                                 3, 7, name, sizeof(name)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(!CoopNetBridge_TakeInviteNotice());
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR);
}

TEST("Cloud Coop progress feed keeps consecutive notices in order")
{
    struct CoopBridgeMessage message;
    u8 first[4] = {1, COOP_REGION_HOENN, 0, 0};
    u8 second[4] = {2, COOP_REGION_HOENN, 25, 0};
    u8 kind, region;
    u16 subjectId;

    InitOnlineTestBridge();
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_PROGRESS_EVENT,
                                 2, 7, first, sizeof(first)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_PROGRESS_EVENT,
                                 3, 7, second, sizeof(second)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_TakeProgressNotice(&kind, &region, &subjectId));
    EXPECT_EQ(kind, 1);
    EXPECT_EQ(region, COOP_REGION_HOENN);
    EXPECT_EQ(subjectId, 0);
    EXPECT(CoopNetBridge_TakeProgressNotice(&kind, &region, &subjectId));
    EXPECT_EQ(kind, 2);
    EXPECT_EQ(subjectId, 25);
    EXPECT(!CoopNetBridge_TakeProgressNotice(&kind, &region, &subjectId));
}

TEST("Cloud Coop observes Kanto badges awarded during the Johto campaign")
{
    struct MapHeader savedMapHeader = gMapHeader;
    struct CoopBridgeMessage message;

    InitOnlineTestBridge();
    gMapHeader.engineRegion = COOP_MAP_ENGINE_REGION_KANTO;
    gMapHeader.regionMapSectionId = MAPSEC_PALLET_TOWN;
    FlagClear(JOHTO_FLAG_BADGE09_GET);
    FlagSet(JOHTO_FLAG_BADGE09_GET);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_PROGRESS_OBSERVATION);
    EXPECT_EQ(message.length, 4);
    EXPECT_EQ(message.payload[0], 1);
    EXPECT_EQ(message.payload[1], COOP_REGION_KANTO);
    EXPECT_EQ(message.payload[2], 0);
    EXPECT_EQ(message.payload[3], 0);
    gMapHeader = savedMapHeader;
}

TEST("Cloud Coop attributes the Johto Champion milestone from the Kanto league")
{
    struct MapHeader savedMapHeader = gMapHeader;
    struct CoopBridgeMessage message;
    u8 kind, region;
    u16 subjectId;
    u8 milestone[4] = {3, COOP_REGION_JOHTO, 1, 0};

    InitOnlineTestBridge();
    gMapHeader.engineRegion = COOP_MAP_ENGINE_REGION_KANTO;
    gMapHeader.regionMapSectionId = MAPSEC_INDIGO_PLATEAU;
    FlagClear(JOHTO_FLAG_IS_CHAMPION);
    FlagSet(JOHTO_FLAG_IS_CHAMPION);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_PROGRESS_OBSERVATION);
    EXPECT_EQ(message.payload[0], 3);
    EXPECT_EQ(message.payload[1], COOP_REGION_JOHTO);
    EXPECT_EQ(message.payload[2], 1);

    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_PROGRESS_EVENT,
                                 2, 7, milestone, sizeof(milestone)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_TakeProgressNotice(&kind, &region, &subjectId));
    EXPECT_EQ(kind, 3);
    EXPECT_EQ(region, COOP_REGION_JOHTO);
    EXPECT_EQ(subjectId, 1);
    gMapHeader = savedMapHeader;
}

TEST("Cloud Coop retains progress observations across a full outbound FIFO")
{
    struct CoopBridgeMessage message;
    u32 i;
    bool8 found = FALSE;

    EstablishTestCloudSession();
    for (i = 0; i < COOP_NET_BRIDGE_QUEUE_CAPACITY; i++)
        EXPECT(CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_ROM_READY, NULL, 0));

    /* The shared FIFO is full, but the edge event is accepted into the
     * dedicated progress queue and must not be reported as a transport
     * failure or discarded. */
    EXPECT(CoopNetBridge_ObserveProgress(2, COOP_REGION_HOENN, 25));
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_QUEUE_CONGESTED);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    CoopNetBridge_Poll();

    while (CoopNetBridge_DequeueGameToNetwork(&message))
    {
        if (message.type == COOP_BRIDGE_MESSAGE_PROGRESS_OBSERVATION)
        {
            EXPECT_EQ(message.length, 4);
            EXPECT_EQ(message.payload[0], 2);
            EXPECT_EQ(message.payload[1], COOP_REGION_HOENN);
            EXPECT_EQ(message.payload[2], 25);
            EXPECT_EQ(message.payload[3], 0);
            found = TRUE;
        }
    }
    EXPECT(found);
}

TEST("Cloud Coop Brock uses the solo path until save commits are supported")
{
    struct CoopBridgeMessage message;
    struct MapHeader savedMapHeader = gMapHeader;
    bool8 wasDefeated;

    InitOnlineTestBridge();
    gMapHeader.engineRegion = COOP_MAP_ENGINE_REGION_KANTO;
    gMapHeader.regionMapSectionId = MAPSEC_PEWTER_CITY;
    wasDefeated = HasTrainerBeenFought(TRAINER_LEADER_BROCK);
    ClearTrainerFlag(TRAINER_LEADER_BROCK);
    Special_CoopBattleConsentBeginBrock();
    EXPECT_EQ(gSpecialVar_Result, FALSE);
    EXPECT(!CoopNetBridge_DequeueGameToNetwork(&message));

    InitOnlineTestBridge();
    SetTrainerFlag(TRAINER_LEADER_BROCK);
    Special_CoopBattleConsentBeginBrock();
    EXPECT_EQ(gSpecialVar_Result, FALSE);
    EXPECT(!CoopNetBridge_DequeueGameToNetwork(&message));

    if (!wasDefeated)
        ClearTrainerFlag(TRAINER_LEADER_BROCK);
    gMapHeader = savedMapHeader;
}

TEST("Cloud Coop battle offer survives the paused Yes No prompt")
{
    struct CoopBridgeMessage message;
    u8 offer[COOP_BATTLE_JOIN_OFFER_SIZE] = {0};
    MainCallback savedCallback1 = gMain.callback1;
    MainCallback savedCallback2 = gMain.callback2;
    bool8 savedPaletteFadeActive = gPaletteFade.active;
    u32 savedFrame = gMain.vblankCounter1;

    InitOnlineTestBridge();
    ScriptContext_Init();
    UnlockPlayerFieldControls();
    gMain.callback1 = CB1_Overworld;
    gMain.callback2 = CB2_Overworld;
    gPaletteFade.active = FALSE;
    offer[0] = 1;
    offer[16] = COOP_BATTLE_KIND_COOPERATIVE_TRAINER;
    offer[17] = 1; // Responding partner.
    EXPECT(CoopBattleConsent_ReceiveOffer(offer, sizeof(offer)));
    CoopBattleConsent_Poll();
    EXPECT(ArePlayerFieldControlsLocked());
    ScriptContext_Stop(); // A Yes/No box pauses the offer script.
    EXPECT(!ScriptContext_IsEnabled());
    CoopBattleConsent_Poll();

    gSpecialVar_0x8004 = 1;
    Special_CoopBattleConsentRespond();
    EXPECT_EQ(gSpecialVar_Result, TRUE);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_BATTLE_JOIN_RESPONSE);
    EXPECT_EQ(message.payload[COOP_BATTLE_ID_SIZE], 1);

    // The first scenario leaves its synthetic offer script paused. End it
    // before starting a new independent offer in this same test process.
    ScriptContext_Init();
    UnlockPlayerFieldControls();
    InitOnlineTestBridge();
    EXPECT(CoopBattleConsent_ReceiveOffer(offer, sizeof(offer)));
    CoopBattleConsent_Poll();
    EXPECT(ArePlayerFieldControlsLocked());
    ScriptContext_Init(); // Abandoned script releases the field lock.
    UnlockPlayerFieldControls();
    CoopBattleConsent_Poll();
    gSpecialVar_0x8004 = 1;
    Special_CoopBattleConsentRespond();
    EXPECT_EQ(gSpecialVar_Result, FALSE);

    InitOnlineTestBridge();
    EXPECT(CoopBattleConsent_ReceiveOffer(offer, sizeof(offer)));
    CoopBattleConsent_Poll();
    EXPECT(ArePlayerFieldControlsLocked());
    ScriptContext_Stop();
    gMain.vblankCounter1 += COOP_TRAINER_ENCOUNTER_OFFER_FRAMES; // Trainer offers decline at 10 s.
    CoopBattleConsent_Poll();
    EXPECT(!ArePlayerFieldControlsLocked());
    gSpecialVar_0x8004 = 1;
    Special_CoopBattleConsentRespond();
    EXPECT_EQ(gSpecialVar_Result, FALSE);

    InitOnlineTestBridge();
    EXPECT(CoopBattleConsent_ReceiveOffer(offer, sizeof(offer)));
    CoopBattleConsent_Poll();
    EXPECT(ArePlayerFieldControlsLocked());
    ScriptContext_Stop();
    CoopBattleConsent_OnTransportLost();
    EXPECT(!ArePlayerFieldControlsLocked());
    gSpecialVar_0x8004 = 1;
    Special_CoopBattleConsentRespond();
    EXPECT_EQ(gSpecialVar_Result, FALSE);

    ScriptContext_Init();
    UnlockPlayerFieldControls();
    gMain.vblankCounter1 = savedFrame;
    gPaletteFade.active = savedPaletteFadeActive;
    gMain.callback1 = savedCallback1;
    gMain.callback2 = savedCallback2;
}

TEST("Cloud Coop packs distinct progress subjects without changing bridge payloads")
{
    struct CoopBridgeMessage message;
    u32 i;
    static const u8 expected[][4] = {
        {1, COOP_REGION_KANTO, 7, 0},
        {2, COOP_REGION_SEVII, 1, 4},
        {3, COOP_REGION_JOHTO, 1, 0},
        {2, COOP_REGION_HOENN, 1, 4},
    };

    EstablishTestCloudSession();
    for (i = 0; i < COOP_NET_BRIDGE_QUEUE_CAPACITY; i++)
        EXPECT(CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_ROM_READY, NULL, 0));

    EXPECT(CoopNetBridge_ObserveProgress(1, COOP_REGION_KANTO, 7));
    EXPECT(CoopNetBridge_ObserveProgress(2, COOP_REGION_SEVII, 1025));
    EXPECT(CoopNetBridge_ObserveProgress(3, COOP_REGION_JOHTO, 1));
    EXPECT(CoopNetBridge_ObserveProgress(2, COOP_REGION_SEVII, 1025));
    EXPECT(CoopNetBridge_ObserveProgress(2, COOP_REGION_HOENN, 1025));
    EXPECT(!CoopNetBridge_ObserveProgress(1, COOP_REGION_SEVII, 0));
    EXPECT(!CoopNetBridge_ObserveProgress(1, COOP_REGION_KANTO, 8));
    EXPECT(!CoopNetBridge_ObserveProgress(2, COOP_REGION_HOENN, 1026));
    EXPECT(!CoopNetBridge_ObserveProgress(3, COOP_REGION_JOHTO, 2));

    for (i = 0; i < COOP_NET_BRIDGE_QUEUE_CAPACITY; i++)
        EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    CoopNetBridge_Poll();
    for (i = 0; i < ARRAY_COUNT(expected); i++)
    {
        EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
        EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_PROGRESS_OBSERVATION);
        EXPECT_EQ(message.length, 4);
        EXPECT_EQ(message.payload[0], expected[i][0]);
        EXPECT_EQ(message.payload[1], expected[i][1]);
        EXPECT_EQ(message.payload[2], expected[i][2]);
        EXPECT_EQ(message.payload[3], expected[i][3]);
    }
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
}

TEST("Cloud Coop requeues unread progress before ROM_READY on bridge replacement")
{
    struct CoopBridgeMessage message;
    u32 replacement;

    for (replacement = 0; replacement < 2; replacement++)
    {
        u32 i;

        EstablishTestCloudSession();
        EXPECT(CoopNetBridge_ObserveProgress(2, COOP_REGION_HOENN, 25));
        EXPECT(CoopNetBridge_ObserveProgress(2, COOP_REGION_HOENN, 26));
        for (i = 2; i < COOP_NET_BRIDGE_QUEUE_CAPACITY; i++)
            EXPECT(CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_ROM_READY, NULL, 0));
        EXPECT(CoopNetBridge_ObserveProgress(2, COOP_REGION_HOENN, 27));

        EXPECT(CoopBridgeMessage_Seal(&message,
                                      COOP_BRIDGE_MESSAGE_SESSION_READY,
                                      2,
                                      replacement == 0 ? 17 : 18,
                                      NULL,
                                      0));
        EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
        CoopNetBridge_Poll();

        /* A second replacement before the sidecar consumes the first replay
         * must still produce only one copy of each distinct observation. */
        EXPECT(CoopBridgeMessage_Seal(&message,
                                      COOP_BRIDGE_MESSAGE_SESSION_READY,
                                      3,
                                      replacement == 0 ? 17 : 18,
                                      NULL,
                                      0));
        EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
        CoopNetBridge_Poll();

        EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
        EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
        for (i = 25; i <= 27; i++)
        {
            EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
            EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_PROGRESS_OBSERVATION);
            EXPECT_EQ(message.payload[0], 2);
            EXPECT_EQ(message.payload[1], COOP_REGION_HOENN);
            EXPECT_EQ(message.payload[2], i);
            EXPECT_EQ(message.payload[3], 0);
        }
        EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    }
}

TEST("Cloud Coop retries unread progress after heartbeat loss")
{
    struct CoopBridgeMessage message;
    u32 frame;

    EstablishTestCloudSession();
    EXPECT(CoopNetBridge_ObserveProgress(2, COOP_REGION_HOENN, 25));
    for (frame = 0; frame < COOP_NET_BRIDGE_SIDECAR_STALE_INTERVAL; frame++)
        CoopNetBridge_Poll();
    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SESSION_READY));
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));

    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  2,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_PROGRESS_OBSERVATION);
    EXPECT_EQ(message.payload[2], 25);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
}

TEST("Cloud Coop Online ignores stale responses and clears status on rearm")
{
    struct CoopOnlineRequest request = { .request_id = 1, .action = COOP_ONLINE_REFRESH };
    struct CoopOnlineStatus status;
    struct CoopBridgeMessage message;
    u8 payload[COOP_ONLINE_STATUS_SIZE] = {2};

    InitOnlineTestBridge();
    EXPECT(CoopNetBridge_SendOnlineRequest(&request));
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_ONLINE_STATUS,
                                 2, 7, payload, sizeof(payload)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(!CoopNetBridge_GetOnlineStatus(&status));

    payload[0] = 1;
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_ONLINE_STATUS,
                                 3, 7, payload, sizeof(payload)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_GetOnlineStatus(&status));
    EXPECT_EQ(status.request_id, 1);
    EXPECT_EQ(status.result, COOP_ONLINE_READY);

    payload[10] = 5;
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_ONLINE_STATUS,
                                 4, 7, payload, sizeof(payload)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR);
    EXPECT(CoopNetBridge_GetOnlineStatus(&status));
    EXPECT_EQ(status.request_id, 1);

    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_SESSION_READY,
                                 5, 7, NULL, 0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(!CoopNetBridge_GetOnlineStatus(&status));
}

TEST("Known group membership blocks unilateral travel through refresh and disconnect")
{
    struct CoopOnlineRequest request = { .request_id = 1, .action = COOP_ONLINE_REFRESH };
    struct CoopBridgeMessage message;
    u8 payload[COOP_ONLINE_STATUS_SIZE] = {0};

    InitTestBridge();
    EXPECT(!CoopNetBridge_IsOrMayBeGrouped());
    InitOnlineTestBridge();
    /* Before the first authenticated status, the cloud session may resume an
     * existing group; no vanilla travel may run yet. */
    EXPECT(CoopNetBridge_IsOrMayBeGrouped());
    EXPECT(CoopNetBridge_SendOnlineRequest(&request));
    payload[0] = 1;
    payload[5] = COOP_ONLINE_GROUPED;
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_ONLINE_STATUS,
                                 2, 7, payload, sizeof(payload)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_IsGrouped());
    EXPECT(CoopNetBridge_IsOrMayBeGrouped());

    request.request_id = 2;
    EXPECT(CoopNetBridge_SendOnlineRequest(&request));
    EXPECT(CoopNetBridge_IsGrouped());
    EXPECT(CoopNetBridge_IsOrMayBeGrouped());
    payload[0] = 2;
    payload[4] = COOP_ONLINE_UNAVAILABLE;
    payload[5] = 0;
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_ONLINE_STATUS,
                                 3, 7, payload, sizeof(payload)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_IsOrMayBeGrouped());

    gCoopNetBridge.status_flags &= ~COOP_BRIDGE_STATUS_SESSION_READY;
    EXPECT(!CoopNetBridge_IsGrouped());
    EXPECT(CoopNetBridge_IsOrMayBeGrouped());
    gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_SESSION_READY;
    request.request_id = 3;
    EXPECT(CoopNetBridge_SendOnlineRequest(&request));
    payload[0] = 3;
    payload[4] = COOP_ONLINE_READY;
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_ONLINE_STATUS,
                                 4, 7, payload, sizeof(payload)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(!CoopNetBridge_IsOrMayBeGrouped());
}

TEST("Authenticated group state updates clear unknown membership and reject malformed frames")
{
    struct CoopBridgeMessage message;
    u8 payload[2] = {0};

    InitOnlineTestBridge();
    EXPECT(CoopNetBridge_IsOrMayBeGrouped());
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_GROUP_STATE_CHANGED,
                                 2, 7, payload, sizeof(payload)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(!CoopNetBridge_IsOrMayBeGrouped());
    EXPECT(!CoopNetBridge_IsGrouped());

    payload[0] = 1;
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_GROUP_STATE_CHANGED,
                                 3, 7, payload, sizeof(payload)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_IsOrMayBeGrouped());
    EXPECT(CoopNetBridge_IsGrouped());

    payload[0] = 0;
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_GROUP_STATE_CHANGED,
                                 4, 6, payload, sizeof(payload)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_IsOrMayBeGrouped());
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_GROUP_STATE_CHANGED,
                                 3, 7, payload, sizeof(payload)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_IsOrMayBeGrouped());

    payload[0] = 2;
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_GROUP_STATE_CHANGED,
                                 4, 7, payload, sizeof(payload)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR);
    EXPECT(CoopNetBridge_IsOrMayBeGrouped());

    payload[0] = 0;
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_GROUP_STATE_CHANGED,
                                 4, 7, payload, sizeof(payload)));
    message.payload[1] = 1;
    message.payload[2] = 1;
    message.checksum = CoopBridgeMessage_ComputeChecksum(&message);
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_IsOrMayBeGrouped());

    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_GROUP_STATE_CHANGED,
                                 4, 7, payload, sizeof(payload)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(!CoopNetBridge_IsOrMayBeGrouped());
    EXPECT(!CoopNetBridge_IsGrouped());
}

TEST("Pairing redemption keeps group travel guarded until membership refresh")
{
    struct CoopPairingRequest request = { .request_id = 1, .action = COOP_PAIRING_REDEEM,
                                          .code = {'A', 'B', 'C', '-', 'D', 'E', 'F'} };
    struct CoopBridgeMessage message;
    u8 ungrouped[2] = {0};
    u8 joined[COOP_PAIRING_RECORD_SIZE] = {1, 0, 0, 0, COOP_PAIRING_JOINED};

    InitOnlineTestBridge();
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_GROUP_STATE_CHANGED,
                                 2, 7, ungrouped, sizeof(ungrouped)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(!CoopNetBridge_IsOrMayBeGrouped());

    EXPECT(CoopNetBridge_SendPairingRequest(&request));
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_PAIRING_STATUS,
                                 3, 7, joined, sizeof(joined)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_IsOrMayBeGrouped());
}

TEST("Outgoing invite blocks travel before its reply and until the artifact expires")
{
    struct CoopOnlineRequest request = { .request_id = 1, .view_id = 1,
                                         .action = COOP_ONLINE_INVITE };
    struct CoopBridgeMessage message;
    u8 state[2] = {0};
    u8 status[COOP_ONLINE_STATUS_SIZE] = {1};

    InitOnlineTestBridge();
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_GROUP_STATE_CHANGED,
                                 2, 7, state, sizeof(state)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(!CoopNetBridge_IsOrMayBeGrouped());

    EXPECT(CoopNetBridge_SendOnlineRequest(&request));
    EXPECT(CoopNetBridge_IsOrMayBeGrouped());
    /* An earlier watcher result cannot release the request's local latch. */
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_GROUP_STATE_CHANGED,
                                 3, 7, state, sizeof(state)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_IsOrMayBeGrouped());

    status[5] = COOP_ONLINE_REMOTE_JOIN_POSSIBLE;
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_ONLINE_STATUS,
                                 4, 7, status, sizeof(status)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_IsOrMayBeGrouped());
    state[1] = 0;
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_GROUP_STATE_CHANGED,
                                 5, 7, state, sizeof(state)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(!CoopNetBridge_IsOrMayBeGrouped());
}

TEST("Remote invitation artifact guards travel without a formed group")
{
    struct CoopBridgeMessage message;
    u8 state[2] = {0, 1};

    InitOnlineTestBridge();
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_GROUP_STATE_CHANGED,
                                 2, 7, state, sizeof(state)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(!CoopNetBridge_IsGrouped());
    EXPECT(CoopNetBridge_IsOrMayBeGrouped());

    state[1] = 2;
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_GROUP_STATE_CHANGED,
                                 3, 7, state, sizeof(state)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR);
    EXPECT(CoopNetBridge_IsOrMayBeGrouped());

    state[1] = 0;
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_GROUP_STATE_CHANGED,
                                 3, 7, state, sizeof(state)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(!CoopNetBridge_IsOrMayBeGrouped());
}

TEST("Pairing code creation guards travel through delayed watcher status")
{
    struct CoopPairingRequest request = { .request_id = 1, .action = COOP_PAIRING_CREATE };
    struct CoopBridgeMessage message;
    u8 state[2] = {0};
    u8 created[COOP_PAIRING_RECORD_SIZE] = {1, 0, 0, 0, COOP_PAIRING_CREATED,
                                             'A', 'B', 'C', '-', 'D', 'E', 'F'};

    InitOnlineTestBridge();
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_GROUP_STATE_CHANGED,
                                 2, 7, state, sizeof(state)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(!CoopNetBridge_IsOrMayBeGrouped());
    EXPECT(CoopNetBridge_SendPairingRequest(&request));
    EXPECT(CoopNetBridge_IsOrMayBeGrouped());
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_GROUP_STATE_CHANGED,
                                 3, 7, state, sizeof(state)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_IsOrMayBeGrouped());
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_PAIRING_STATUS,
                                 4, 7, created, sizeof(created)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_IsOrMayBeGrouped());
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_GROUP_STATE_CHANGED,
                                 5, 7, state, sizeof(state)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(!CoopNetBridge_IsOrMayBeGrouped());
}

TEST("Older Online Refresh cannot release a newly created pairing code")
{
    struct CoopPairingRequest pairing = { .request_id = 1, .action = COOP_PAIRING_CREATE };
    struct CoopOnlineRequest refresh = { .request_id = 1, .action = COOP_ONLINE_REFRESH };
    struct CoopBridgeMessage message;
    u8 state[2] = {0};
    u8 status[COOP_ONLINE_STATUS_SIZE] = {1};
    u8 created[COOP_PAIRING_RECORD_SIZE] = {1, 0, 0, 0, COOP_PAIRING_CREATED,
                                             'A', 'B', 'C', '-', 'D', 'E', 'F'};

    InitOnlineTestBridge();
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_GROUP_STATE_CHANGED,
                                 2, 7, state, sizeof(state)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(!CoopNetBridge_IsOrMayBeGrouped());
    EXPECT(CoopNetBridge_SendPairingRequest(&pairing));
    EXPECT(CoopNetBridge_SendOnlineRequest(&refresh));
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_PAIRING_STATUS,
                                 3, 7, created, sizeof(created)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_IsOrMayBeGrouped());
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_ONLINE_STATUS,
                                 4, 7, status, sizeof(status)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_IsOrMayBeGrouped());
    state[1] = 1;
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_GROUP_STATE_CHANGED,
                                 5, 7, state, sizeof(state)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_IsOrMayBeGrouped());
    state[1] = 0;
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_GROUP_STATE_CHANGED,
                                 6, 7, state, sizeof(state)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(!CoopNetBridge_IsOrMayBeGrouped());
}

TEST("Same epoch reconnect waits for fresh group membership")
{
    struct CoopBridgeMessage message;
    u8 ungrouped[2] = {0};

    InitOnlineTestBridge();
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_GROUP_STATE_CHANGED,
                                 2, 7, ungrouped, sizeof(ungrouped)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(!CoopNetBridge_IsOrMayBeGrouped());

    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_SESSION_READY,
                                 3, 7, NULL, 0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_IsOrMayBeGrouped());
    EXPECT(!CoopNetBridge_IsGrouped());

    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_GROUP_STATE_CHANGED,
                                 4, 7, ungrouped, sizeof(ungrouped)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(!CoopNetBridge_IsOrMayBeGrouped());
}

TEST("Cloud Coop Online preserves full names and rejects invalid page indices")
{
    struct CoopOnlineRequest request = { .request_id = 1, .action = COOP_ONLINE_REFRESH };
    struct CoopOnlineStatus status;
    struct CoopBridgeMessage message;
    u8 payload[COOP_ONLINE_STATUS_SIZE] = {1};

    InitOnlineTestBridge();
    EXPECT(CoopNetBridge_SendOnlineRequest(&request));
    memset(payload + 16, 'a', COOP_ONLINE_NAME_SIZE);
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_ONLINE_STATUS,
                                 2, 7, payload, sizeof(payload)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_GetOnlineStatus(&status));
    EXPECT_EQ(status.nearby_name[31], 'a');
    EXPECT_EQ(status.nearby_name[32], 0);
    request.request_id++;
    EXPECT(CoopNetBridge_SendOnlineRequest(&request));
    payload[0] = 2;
    memset(payload + 16, 0, COOP_ONLINE_NAME_SIZE);
    payload[8] = 1;
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_ONLINE_STATUS,
                                 3, 7, payload, sizeof(payload)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(!CoopNetBridge_GetOnlineStatus(&status));
}

TEST("Cloud Coop Online rejects an out-of-catalogue partner location")
{
    struct CoopOnlineRequest request = { .request_id = 1, .action = COOP_ONLINE_REFRESH };
    struct CoopOnlineStatus status;
    struct CoopBridgeMessage message;
    u8 payload[COOP_ONLINE_STATUS_SIZE] = {0};
    InitOnlineTestBridge();
    EXPECT(CoopNetBridge_SendOnlineRequest(&request));
    payload[0] = 1;
    payload[5] = COOP_ONLINE_GROUPED | COOP_ONLINE_HAS_LOCATION;
    payload[12] = 0xFF;
    payload[13] = 0xFF;
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_ONLINE_STATUS,
                                 2, 7, payload, sizeof(payload)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(!CoopNetBridge_GetOnlineStatus(&status));
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR);
}

TEST("Cloud Coop bridge messages seal deterministic metadata and reject corruption")
{
    static const u8 sPayload[] = {0xDE, 0xAD, 0xBE, 0xEF};
    struct CoopBridgeMessage message;
    u32 sealedChecksum;

    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_PLAYER_STATE,
                                  42,
                                  9,
                                  sPayload,
                                  sizeof(sPayload)));
    EXPECT(CoopBridgeMessage_Validate(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_PLAYER_STATE);
    EXPECT_EQ(message.length, sizeof(sPayload));
    EXPECT_EQ(message.sequence, 42);
    EXPECT_EQ(message.session_epoch, 9);
    EXPECT_EQ(memcmp(message.payload, sPayload, sizeof(sPayload)), 0);
    EXPECT_EQ(message.payload[sizeof(sPayload)], 0);

    sealedChecksum = message.checksum;
    EXPECT_EQ(sealedChecksum, CoopBridgeMessage_ComputeChecksum(&message));
    message.payload[COOP_NET_BRIDGE_PAYLOAD_SIZE - 1] ^= 1;
    EXPECT(!CoopBridgeMessage_Validate(&message));
    message.payload[COOP_NET_BRIDGE_PAYLOAD_SIZE - 1] ^= 1;
    EXPECT_EQ(message.checksum, sealedChecksum);
    EXPECT(CoopBridgeMessage_Validate(&message));

    message.type = COOP_BRIDGE_MESSAGE_NONE;
    EXPECT(!CoopBridgeMessage_Validate(&message));
    message.type = 0x7777;
    EXPECT(!CoopBridgeMessage_Validate(&message));
    message.type = COOP_BRIDGE_MESSAGE_PLAYER_STATE;
    message.sequence = 0;
    EXPECT(!CoopBridgeMessage_Validate(&message));
    message.sequence = 42;
    message.length = COOP_NET_BRIDGE_PAYLOAD_SIZE + 1;
    EXPECT(!CoopBridgeMessage_Validate(&message));

    EXPECT(!CoopBridgeMessage_Seal(NULL,
                                   COOP_BRIDGE_MESSAGE_PLAYER_STATE,
                                   1,
                                   0,
                                   NULL,
                                   0));
    EXPECT(!CoopBridgeMessage_Seal(&message,
                                   COOP_BRIDGE_MESSAGE_NONE,
                                   1,
                                   0,
                                   NULL,
                                   0));
    EXPECT(!CoopBridgeMessage_Seal(&message,
                                   0x7777,
                                   1,
                                   0,
                                   NULL,
                                   0));
    EXPECT(!CoopBridgeMessage_Seal(&message,
                                   COOP_BRIDGE_MESSAGE_PLAYER_STATE,
                                   0,
                                   0,
                                   NULL,
                                   0));
    EXPECT(!CoopBridgeMessage_Seal(&message,
                                   COOP_BRIDGE_MESSAGE_PLAYER_STATE,
                                   1,
                                   0,
                                   NULL,
                                   1));
    EXPECT(!CoopBridgeMessage_Seal(&message,
                                   COOP_BRIDGE_MESSAGE_PLAYER_STATE,
                                   1,
                                   0,
                                   sPayload,
                                   COOP_NET_BRIDGE_PAYLOAD_SIZE + 1));
}

TEST("Cloud Coop bridge queue preserves FIFO order across u16 counter wrap")
{
    struct CoopBridgeQueue *queue = GetTestQueue();
    struct CoopBridgeMessage message;
    u32 sequence;

    CoopBridgeQueue_Init(queue);
    queue->read_index = 0xFFF0u;
    queue->write_index = 0xFFF0u;
    EXPECT(CoopBridgeQueue_IsEmpty(queue));
    EXPECT(!CoopBridgeQueue_IsFull(queue));

    for (sequence = 1; sequence <= COOP_NET_BRIDGE_QUEUE_CAPACITY; sequence++)
    {
        SealTestMessage(&message, COOP_BRIDGE_MESSAGE_PLAYER_STATE, sequence, 7);
        EXPECT(CoopBridgeQueue_Push(queue, &message));
    }
    EXPECT(!CoopBridgeQueue_IsEmpty(queue));
    EXPECT(CoopBridgeQueue_IsFull(queue));

    SealTestMessage(&message,
                    COOP_BRIDGE_MESSAGE_PLAYER_STATE,
                    COOP_NET_BRIDGE_QUEUE_CAPACITY + 1,
                    7);
    EXPECT(!CoopBridgeQueue_Push(queue, &message));

    EXPECT(CoopBridgeQueue_Pop(queue, &message));
    EXPECT_EQ(message.sequence, 1);
    SealTestMessage(&message,
                    COOP_BRIDGE_MESSAGE_PLAYER_STATE,
                    COOP_NET_BRIDGE_QUEUE_CAPACITY + 1,
                    7);
    EXPECT(CoopBridgeQueue_Push(queue, &message));
    EXPECT(CoopBridgeQueue_IsFull(queue));

    for (sequence = 2; sequence <= COOP_NET_BRIDGE_QUEUE_CAPACITY + 1; sequence++)
    {
        EXPECT(CoopBridgeQueue_Pop(queue, &message));
        EXPECT_EQ(message.sequence, sequence);
    }
    EXPECT(CoopBridgeQueue_IsEmpty(queue));
    EXPECT(!CoopBridgeQueue_IsFull(queue));
    EXPECT(!CoopBridgeQueue_Pop(queue, &message));
    EXPECT_EQ(queue->read_index, queue->write_index);
}

TEST("Cloud Coop bridge queue rejects malformed producer indexes")
{
    struct CoopBridgeQueue *queue = GetTestQueue();
    struct CoopBridgeMessage message;
    u16 readIndex;
    u16 writeIndex;

    CoopBridgeQueue_Init(queue);
    SealTestMessage(&message, COOP_BRIDGE_MESSAGE_PLAYER_STATE, 1, 7);
    queue->read_index = 100;
    queue->write_index = 100 + COOP_NET_BRIDGE_QUEUE_CAPACITY + 1;
    readIndex = queue->read_index;
    writeIndex = queue->write_index;

    /* Both predicates fail closed: callers can neither consume nor publish. */
    EXPECT(CoopBridgeQueue_IsEmpty(queue));
    EXPECT(CoopBridgeQueue_IsFull(queue));
    EXPECT(!CoopBridgeQueue_Push(queue, &message));
    EXPECT(!CoopBridgeQueue_Pop(queue, &message));
    EXPECT_EQ(queue->read_index, readIndex);
    EXPECT_EQ(queue->write_index, writeIndex);

    queue->read_index = 10;
    queue->write_index = 9;
    EXPECT(CoopBridgeQueue_IsEmpty(queue));
    EXPECT(CoopBridgeQueue_IsFull(queue));
    EXPECT(!CoopBridgeQueue_Push(queue, &message));
    EXPECT(!CoopBridgeQueue_Pop(queue, &message));
}

TEST("Cloud Coop bridge poll discards an impossible inbound queue depth")
{
    InitTestBridge();
    gCoopNetBridge.network_to_game.read_index = 25;
    gCoopNetBridge.network_to_game.write_index = 25 + COOP_NET_BRIDGE_QUEUE_CAPACITY + 1;

    CoopNetBridge_Poll();

    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_QUEUE_ERROR);
    EXPECT_EQ(gCoopNetBridge.network_to_game.read_index,
              gCoopNetBridge.network_to_game.write_index);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.network_to_game));
}

TEST("Cloud Coop session epoch change clears both queues and reissues ROM ready")
{
    struct CoopBridgeMessage message;

    InitTestBridge();
    EXPECT(CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_CHECKPOINT_READY,
                                               NULL,
                                               0));

    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  5,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    SealTestMessage(&message, COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_UPDATE, 6, 17);
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));

    CoopNetBridge_Poll();

    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SESSION_READY);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.network_to_game));
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    EXPECT_EQ(message.sequence, 1);
    EXPECT_EQ(message.session_epoch, 17);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
}

TEST("Cloud Coop reconnect sends ROM ready before active group travel replay")
{
    struct CoopBridgeMessage message;
    struct CoopGroupTravelRecord request = {0};

    InitTestBridge();
    PopInitialRomReady();
    request.kind = COOP_GROUP_TRAVEL_CLIENT_REQUEST;
    request.route = COOP_GROUP_TRAVEL_ROUTE_FERRY_LATER;
    request.era = COOP_GROUP_TRAVEL_ERA_LATER;
    request.departure = COOP_GROUP_TRAVEL_DEPARTURE_FERRY;
    request.destination = COOP_GROUP_TRAVEL_DEST_LATER_VERMILION;
    request.request_id = 77;
    ScriptContext_Init();
    UnlockPlayerFieldControls();
    CoopGroupTravel_TestSetSafe(TRUE);
    CoopGroupTravel_TestSeedRequest(&request);

    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  5,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();

    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_GROUP_TRAVEL_CLIENT);
    EXPECT_EQ(message.length, COOP_GROUP_TRAVEL_RECORD_SIZE);
    EXPECT(CoopGroupTravel_TestSemanticQueued());
    CoopGroupTravel_OnTransportLost();
    CoopGroupTravel_Init();
}

TEST("Cloud Coop same epoch reconnect rejects stale replay and preserves sequences")
{
    struct CoopBridgeMessage message;
    u32 pollCount;
    u16 outboundReadIndex;
    u16 outboundWriteIndex;
    u16 inboundIndex;

    InitTestBridge();
    PopInitialRomReady();

    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  10,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    EXPECT_EQ(message.sequence, 1);
    EXPECT_EQ(message.session_epoch, 17);

    EXPECT(CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_CHECKPOINT_READY,
                                               NULL,
                                               0));
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_CHECKPOINT_READY);
    EXPECT_EQ(message.sequence, 2);

    for (pollCount = 0; pollCount < COOP_NET_BRIDGE_SIDECAR_STALE_INTERVAL; pollCount++)
        CoopNetBridge_Poll();
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SIDECAR_HEARTBEAT_STALE);
    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SESSION_READY));

    EXPECT(CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_CHECKPOINT_READY,
                                               NULL,
                                               0));
    outboundReadIndex = gCoopNetBridge.game_to_network.read_index;
    outboundWriteIndex = gCoopNetBridge.game_to_network.write_index;
    inboundIndex = gCoopNetBridge.network_to_game.read_index;
    EXPECT_EQ(gCoopNetBridge.network_to_game.write_index, inboundIndex);

    /* A replay must be consumed without resetting either queue or re-arming
     * the disconnected session. */
    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  10,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();

    inboundIndex++;
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SIDECAR_HEARTBEAT_STALE);
    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SESSION_READY));
    EXPECT_EQ(gCoopNetBridge.game_to_network.read_index, outboundReadIndex);
    EXPECT_EQ(gCoopNetBridge.game_to_network.write_index, outboundWriteIndex);
    EXPECT_EQ(gCoopNetBridge.network_to_game.read_index, inboundIndex);
    EXPECT_EQ(gCoopNetBridge.network_to_game.write_index, inboundIndex);

    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  9,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();

    inboundIndex++;
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SIDECAR_HEARTBEAT_STALE);
    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SESSION_READY));
    EXPECT_EQ(gCoopNetBridge.game_to_network.read_index, outboundReadIndex);
    EXPECT_EQ(gCoopNetBridge.game_to_network.write_index, outboundWriteIndex);
    EXPECT_EQ(gCoopNetBridge.network_to_game.read_index, inboundIndex);
    EXPECT_EQ(gCoopNetBridge.network_to_game.write_index, inboundIndex);

    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  11,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();

    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SESSION_READY);
    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SIDECAR_HEARTBEAT_STALE));
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    EXPECT_EQ(message.sequence, 4);
    EXPECT_EQ(message.session_epoch, 17);
}

TEST("Cloud Coop rejects unsupported inbound types before later valid session traffic")
{
    struct CoopBridgeMessage message;

    InitTestBridge();
    PopInitialRomReady();

    SealTestMessage(&message, COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_UPDATE, 100, 21);
    message.type = 0x7777;
    message.checksum = CoopBridgeMessage_ComputeChecksum(&message);
    EXPECT(!CoopNetBridge_EnqueueNetworkToGame(&message));
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.network_to_game));

    SealTestMessage(&message, COOP_BRIDGE_MESSAGE_ROM_READY, 101, 21);
    EXPECT(!CoopNetBridge_EnqueueNetworkToGame(&message));
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.network_to_game));

    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  1,
                                  21,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();

    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SESSION_READY);
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    EXPECT_EQ(message.sequence, 1);
    EXPECT_EQ(message.session_epoch, 21);
}

TEST("Cloud Coop raw unsupported inbound traffic cannot suppress a valid new epoch")
{
    struct CoopBridgeMessage message;

    InitTestBridge();
    PopInitialRomReady();

    /* Model mGBA Lua publishing directly into EWRAM, bypassing the C helper. */
    SealTestMessage(&message, COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_UPDATE, 100, 21);
    message.type = 0x7777;
    message.checksum = CoopBridgeMessage_ComputeChecksum(&message);
    HostWriteInboundUnchecked(&message);
    SealTestMessage(&message, COOP_BRIDGE_MESSAGE_ROM_READY, 101, 21);
    HostWriteInboundUnchecked(&message);
    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  1,
                                  21,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));

    CoopNetBridge_Poll();

    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR);
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SESSION_READY);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.network_to_game));
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    EXPECT_EQ(message.sequence, 1);
    EXPECT_EQ(message.session_epoch, 21);
}

TEST("Cloud Coop malformed remote lifecycle payload does not consume its outer sequence")
{
    struct CoopBridgeMessage message;
    u8 malformed_spawn[COOP_PRESENCE_SPAWN_SIZE] = {0};

    InitTestBridge();
    PopInitialRomReady();
    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  1,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);

    /* The outer frame is valid, but a spawn must be exactly 72 bytes.  The
     * bridge must reject it without advancing rx_sequence or mutating the
     * runtime's pending lifecycle queue. */
    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_SPAWN,
                                  2,
                                  17,
                                  malformed_spawn,
                                  COOP_PRESENCE_SPAWN_SIZE - 1));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR);

    /* Reusing the rejected outer sequence as a fresh SESSION_READY proves
     * that malformed remote data did not partially consume the sequence
     * domain.  The accepted reconnect also clears pending bridge queues. */
    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  2,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SESSION_READY);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    EXPECT_EQ(message.session_epoch, 17);
}

// Uses Hoenn map 1.3 (Littleroot), whose header only the Main ROM links.
#if ROM_WORLD == 1
TEST("Cloud Coop newer live same epoch SESSION_READY cuts over pending presence")
{
    struct CoopBridgeMessage message;
    struct CoopPresenceSpawn spawn = {
        .handle = 9,
        .server_sequence = 1,
        .state = {
            .pose = {
                .location = {
                    .region = COOP_REGION_HOENN,
                    .reserved = 0,
                    .map_group = 1,
                    .map_number = 3,
                    .x = 4,
                    .y = 5,
                },
                .elevation = ELEVATION_DEFAULT,
                .direction = COOP_PRESENCE_DIRECTION_SOUTH,
                .client_tick = 1,
                .warp_sequence = 1,
                .movement_mode = COOP_PRESENCE_MOVEMENT_IDLE,
                .animation_id = COOP_PRESENCE_ANIMATION_IDLE,
                .avatar_id = COOP_PRESENCE_AVATAR_BRENDAN,
                .player_state = COOP_PRESENCE_PLAYER_OVERWORLD,
            },
            .source_sequence = 1,
        },
        .username = {
            .length = 3,
            .bytes = "rom",
        },
    };
    u8 spawn_bytes[COOP_PRESENCE_SPAWN_SIZE];
    struct MapLayout map_layout = {
        .width = 20,
        .height = 20,
        .map = sPresenceBridgeMapData,
    };
    struct MapHeader saved_map_header = gMapHeader;
    struct BackupMapLayout saved_backup_map_layout = gBackupMapLayout;
    struct CoopSaveV2 saved_coop_save;
    struct PlayerAvatar saved_player_avatar = gPlayerAvatar;
    struct SaveBlock1 *saved_save_block1 = gSaveBlock1Ptr;
    struct ObjectEvent saved_object_event0 = gObjectEvents[0];
    struct ObjectEvent saved_object_event1 = gObjectEvents[1];
    struct Sprite saved_sprite0 = gSprites[0];
    struct Sprite saved_sprite1 = gSprites[1];
    struct Coords16 saved_save_position;
    struct WarpData saved_save_location;
    MainCallback saved_callback1 = gMain.callback1;
    MainCallback saved_callback2 = gMain.callback2;
    bool8 saved_palette_fade_active = gPaletteFade.active;
    u32 i;

    if (saved_save_block1 != NULL)
    {
        saved_save_position = saved_save_block1->pos;
        saved_save_location = saved_save_block1->location;
    }
    if (gSaveBlock3Ptr != NULL)
        saved_coop_save = gSaveBlock3Ptr->coop;
    InitTestBridge();
    PopInitialRomReady();
    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  1,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);

    for (i = 0; i < ARRAY_COUNT(sPresenceBridgeMapData); i++)
        sPresenceBridgeMapData[i] = PACK_ELEVATION(ELEVATION_DEFAULT);
    gMapHeader.mapLayout = &map_layout;
    gMapHeader.events = NULL;
    gMapHeader.engineRegion = COOP_MAP_ENGINE_REGION_HOENN;
    gMapHeader.regionMapSectionId = MAPSEC_LITTLEROOT_TOWN;
    gBackupMapLayout.width = 20;
    gBackupMapLayout.height = 20;
    gBackupMapLayout.map = sPresenceBridgeMapData;
    gSaveBlock1Ptr = &gSaveblock1.block;
    gSaveBlock1Ptr->location.mapGroup = 1;
    gSaveBlock1Ptr->location.mapNum = 3;
    gSaveBlock1Ptr->pos.x = MAP_OFFSET + 4;
    gSaveBlock1Ptr->pos.y = MAP_OFFSET + 5;
    memset(&gObjectEvents[0], 0, sizeof(gObjectEvents[0]));
    memset(&gObjectEvents[1], 0, sizeof(gObjectEvents[1]));
    gObjectEvents[0].active = TRUE;
    gObjectEvents[0].isPlayer = TRUE;
    gObjectEvents[0].localId = LOCALID_PLAYER;
    gObjectEvents[0].mapGroup = 1;
    gObjectEvents[0].mapNum = 3;
    gObjectEvents[0].facingDirection = DIR_SOUTH;
    gObjectEvents[0].currentElevation = ELEVATION_DEFAULT;
    gObjectEvents[0].previousElevation = ELEVATION_DEFAULT;
    gObjectEvents[0].currentCoords.x = MAP_OFFSET + 4;
    gObjectEvents[0].currentCoords.y = MAP_OFFSET + 5;
    gObjectEvents[0].previousCoords = gObjectEvents[0].currentCoords;
    gObjectEvents[0].initialCoords = gObjectEvents[0].currentCoords;
    gObjectEvents[0].spriteId = 0;
    memset(&gSprites[0], 0, sizeof(gSprites[0]));
    memset(&gSprites[1], 0, sizeof(gSprites[1]));
    gSprites[0].inUse = TRUE;
    gSprites[0].data[0] = 0;
    gPlayerAvatar.objectEventId = 0;
    gPlayerAvatar.spriteId = 0;
    gPlayerAvatar.flags = PLAYER_AVATAR_FLAG_ON_FOOT | PLAYER_AVATAR_FLAG_CONTROLLABLE;
    gMain.callback1 = CB1_Overworld;
    gMain.callback2 = CB2_Overworld;
    gPaletteFade.active = FALSE;

    EXPECT(CoopPresence_EncodeSpawn(&spawn, spawn_bytes, sizeof(spawn_bytes)));
    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_SPAWN,
                                  2,
                                  17,
                                  spawn_bytes,
                                  sizeof(spawn_bytes)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    CoopPresenceRuntime_Update();
    EXPECT(CoopPresenceReducer_IsActive(CoopPresenceRuntime_GetReducer()));

    /* This malformed frame is rejected and must not consume sequence 3. */
    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_SPAWN,
                                  3,
                                  17,
                                  spawn_bytes,
                                  sizeof(spawn_bytes) - 1));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  3,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));

    CoopNetBridge_Poll();
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SESSION_READY);
    EXPECT(!CoopPresenceReducer_IsActive(CoopPresenceRuntime_GetReducer()));
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    EXPECT_EQ(message.session_epoch, 17);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.network_to_game));

    CoopPresenceRuntime_TransportLost();
    CoopNetBridge_Init();
    gMapHeader = saved_map_header;
    gBackupMapLayout = saved_backup_map_layout;
    gObjectEvents[0] = saved_object_event0;
    gObjectEvents[1] = saved_object_event1;
    gSprites[0] = saved_sprite0;
    gSprites[1] = saved_sprite1;
    gPlayerAvatar = saved_player_avatar;
    if (saved_save_block1 != NULL)
    {
        saved_save_block1->pos = saved_save_position;
        saved_save_block1->location = saved_save_location;
    }
    if (gSaveBlock3Ptr != NULL)
        gSaveBlock3Ptr->coop = saved_coop_save;
    gSaveBlock1Ptr = saved_save_block1;
    gMain.callback1 = saved_callback1;
    gMain.callback2 = saved_callback2;
    gPaletteFade.active = saved_palette_fade_active;
}
#endif // ROM_WORLD == 1

static void EstablishTestCloudSession(void)
{
    struct CoopBridgeMessage message;

    InitTestBridge();
    PopInitialRomReady();
    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  1,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
}

static void DeliverTestOnlineStatus(u32 requestId, u32 sequence, u8 flags)
{
    struct CoopOnlineRequest request = { .request_id = requestId, .action = COOP_ONLINE_REFRESH };
    struct CoopBridgeMessage message;
    u8 payload[COOP_ONLINE_STATUS_SIZE] = {0};

    EXPECT(CoopNetBridge_SendOnlineRequest(&request));
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ONLINE_REQUEST);
    payload[0] = requestId;
    payload[5] = flags;
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_ONLINE_STATUS,
                                 sequence, 17, payload, sizeof(payload)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
}

static void DeliverTestBattleRecord(u16 type, u32 sequence, const u8 *payload, u16 length)
{
    struct CoopBridgeMessage message;

    EXPECT(CoopBridgeMessage_Seal(&message, type, sequence, 17, payload, length));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
}

TEST("Cloud Coop peer party chunks remain read only and survive same epoch reconnect")
{
    struct Pokemon mon;
    u8 manifest[COOP_BATTLE_MANIFEST_SIZE] = {0};
    u8 chunk[COOP_BATTLE_PARTY_SNAPSHOT_SIZE] = {0};
    u8 copied_id[COOP_BATTLE_ID_SIZE] = {0};
    u8 copied[2 * COOP_BATTLE_PARTY_MON_SIZE] = {0};
    u8 count = 0;
    struct CoopBridgeMessage message;

    EstablishTestCloudSession();
    manifest[0] = 7;
    manifest[COOP_BATTLE_MANIFEST_KIND_OFFSET] = COOP_BATTLE_KIND_FRIENDLY;
    manifest[COOP_BATTLE_MANIFEST_RULES_OFFSET] = COOP_BATTLE_FRIENDLY_SINGLES; manifest[COOP_BATTLE_MANIFEST_RULES_OFFSET + 2] = 1;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_BATTLE_MANIFEST, 2, manifest, sizeof(manifest));
    CreateMon(&mon, SPECIES_BULBASAUR, 5, 1, OTID_STRUCT_PRESET(1));
    memcpy(chunk, manifest, COOP_BATTLE_ID_SIZE);
    chunk[18] = 2;
    chunk[19] = COOP_BATTLE_PARTY_MON_SIZE;
    memcpy(chunk + 20, &mon, sizeof(mon));
    EXPECT(!CoopBattleRuntime_CopyPeerParty(copied_id, &count, copied, sizeof(copied)));
    /* A future slot cannot skip the first chunk. */
    chunk[16] = chunk[17] = 1;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_PEER_PARTY_CHUNK, 3, chunk, sizeof(chunk));
    EXPECT(!CoopBattleRuntime_CopyPeerParty(copied_id, &count, copied, sizeof(copied)));
    chunk[16] = chunk[17] = 0;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_PEER_PARTY_CHUNK, 4, chunk, sizeof(chunk));
    EXPECT(!CoopBattleRuntime_CopyPeerParty(copied_id, &count, copied, sizeof(copied)));
    /* Identical replay is harmless, including across bridge replacement. */
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_PEER_PARTY_CHUNK, 5, chunk, sizeof(chunk));
    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR));
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  6, 17, NULL, 0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    chunk[16] = chunk[17] = 1;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_PEER_PARTY_CHUNK, 7, chunk, sizeof(chunk));
    EXPECT(CoopBattleRuntime_CopyPeerParty(copied_id, &count, copied, sizeof(copied)));
    EXPECT_EQ(count, 2);
    EXPECT_EQ(memcmp(copied_id, manifest, COOP_BATTLE_ID_SIZE), 0);
    EXPECT_EQ(memcmp(copied, &mon, sizeof(mon)), 0);
    EXPECT_EQ(memcmp(copied + sizeof(mon), &mon, sizeof(mon)), 0);
    EXPECT(!CoopBattleRuntime_CopyPeerParty(copied_id, &count, copied, sizeof(copied) - 1));
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_PEER_PARTY_CHUNK, 8, chunk, sizeof(chunk));
    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR));
    manifest[0] = 9;
    manifest[COOP_BATTLE_MANIFEST_KIND_OFFSET] = COOP_BATTLE_KIND_FRIENDLY;
    manifest[COOP_BATTLE_MANIFEST_RULES_OFFSET] = COOP_BATTLE_FRIENDLY_SINGLES; manifest[COOP_BATTLE_MANIFEST_RULES_OFFSET + 2] = 1;
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  9, 18, NULL, 0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(!CoopBattleRuntime_CopyPeerParty(copied_id, &count, copied, sizeof(copied)));
}

TEST("Cloud Coop rejects malformed peer party but ignores stale battle chunks")
{
    struct Pokemon mon;
    u8 manifest[COOP_BATTLE_MANIFEST_SIZE] = {0};
    u8 chunk[COOP_BATTLE_PARTY_SNAPSHOT_SIZE] = {0};

    EstablishTestCloudSession();
    manifest[0] = 7;
    manifest[COOP_BATTLE_MANIFEST_KIND_OFFSET] = COOP_BATTLE_KIND_FRIENDLY;
    manifest[COOP_BATTLE_MANIFEST_RULES_OFFSET] = COOP_BATTLE_FRIENDLY_SINGLES; manifest[COOP_BATTLE_MANIFEST_RULES_OFFSET + 2] = 1;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_BATTLE_MANIFEST, 2, manifest, sizeof(manifest));
    CreateMon(&mon, SPECIES_BULBASAUR, 5, 1, OTID_STRUCT_PRESET(1));
    chunk[0] = 8;
    chunk[18] = 1;
    chunk[19] = COOP_BATTLE_PARTY_MON_SIZE;
    memcpy(chunk + 20, &mon, sizeof(mon));
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_PEER_PARTY_CHUNK, 3, chunk, sizeof(chunk));
    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR));
    chunk[0] = 7;
    chunk[20 + 32] ^= 1;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_PEER_PARTY_CHUNK, 4, chunk, sizeof(chunk));
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR);
    chunk[20 + 32] ^= 1;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_PEER_PARTY_CHUNK, 5, chunk, sizeof(chunk));
    EXPECT_EQ(CoopBattleRuntime_ReceivePeerPartyChunk(chunk, sizeof(chunk)), COOP_BATTLE_INBOUND_IGNORED);
    chunk[18] = 2;
    EXPECT_EQ(CoopBattleRuntime_ReceivePeerPartyChunk(chunk, sizeof(chunk)), COOP_BATTLE_INBOUND_MALFORMED);
}

TEST("Cloud Coop battle snapshot publishes raw mon chunks only for the consent battle")
{
    struct CoopBridgeMessage message;
    struct Pokemon mon;
    struct Pokemon corrupted;
    u8 offer[COOP_BATTLE_JOIN_OFFER_SIZE] = {0};
    u8 empty[COOP_BATTLE_PARTY_MON_SIZE] = {0};
    u8 other_id[COOP_BATTLE_ID_SIZE] = {2};
    u8 i;
    u8 sent = 0;

    InitOnlineTestBridge();
    EXPECT(CoopBattleConsent_Begin(COOP_BATTLE_KIND_FRIENDLY));
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    offer[0] = 1;
    offer[16] = COOP_BATTLE_KIND_FRIENDLY;
    offer[COOP_BATTLE_JOIN_OFFER_RULES_OFFSET] = COOP_BATTLE_FRIENDLY_SINGLES; offer[COOP_BATTLE_JOIN_OFFER_RULES_OFFSET + 2] = 1;
    memcpy(offer + 18, message.payload + 1, 4);
    EXPECT(CoopBattleConsent_ReceiveOffer(offer, sizeof(offer)));
    CreateMon(&mon, SPECIES_BULBASAUR, 5, 1, OTID_STRUCT_PRESET(1));
    corrupted = mon;
    ((u8 *)&corrupted)[32] ^= 1; /* Encrypted substruct checksum no longer matches. */

    EXPECT(!CoopBattleRuntime_SendPartySnapshot(offer, 0, 1, empty, sizeof(empty)));
    EXPECT(!CoopBattleRuntime_SendPartySnapshot(offer, 0, 1, (const u8 *)&corrupted, sizeof(corrupted)));
    EXPECT(!CoopBattleRuntime_SendPartySnapshot(other_id, 0, 1, (const u8 *)&mon, sizeof(mon)));
    EXPECT(!CoopBattleRuntime_SendPartySnapshot(offer, 1, 2, (const u8 *)&mon, sizeof(mon)));
    EXPECT(!CoopBattleRuntime_SendPartySnapshot(offer, 0, 0, (const u8 *)&mon, sizeof(mon)));
    EXPECT(!CoopBattleRuntime_SendPartySnapshot(offer, 0, 1, (const u8 *)&mon, sizeof(mon) - 1));
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    for (i = 0; i < COOP_NET_BRIDGE_QUEUE_CAPACITY; i++)
        EXPECT(CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_ROM_READY, NULL, 0));
    EXPECT(!CoopBattleRuntime_SendPartySnapshot(offer, 0, 2, (const u8 *)&mon, sizeof(mon)));
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT(CoopBattleRuntime_SendPartySnapshot(offer, 0, 2, (const u8 *)&mon, sizeof(mon)));
    EXPECT(!CoopBattleRuntime_SendPartySnapshot(offer, 0, 2, (const u8 *)&mon, sizeof(mon)));
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT(CoopBattleRuntime_SendPartySnapshot(offer, 1, 2, (const u8 *)&mon, sizeof(mon)));
    while (CoopNetBridge_DequeueGameToNetwork(&message))
    {
        if (message.type != COOP_BRIDGE_MESSAGE_PARTY_SNAPSHOT)
            continue;
        EXPECT_EQ(message.length, COOP_BATTLE_PARTY_SNAPSHOT_SIZE);
        EXPECT_EQ(memcmp(message.payload, offer, COOP_BATTLE_ID_SIZE), 0);
        EXPECT_EQ(message.payload[16], message.payload[17]);
        EXPECT_EQ(message.payload[16], i - COOP_NET_BRIDGE_QUEUE_CAPACITY);
        EXPECT_EQ(message.payload[18], 2);
        EXPECT_EQ(message.payload[19], COOP_BATTLE_PARTY_MON_SIZE);
        EXPECT_EQ(memcmp(message.payload + 20, &mon, sizeof(mon)), 0);
        i++;
        sent++;
    }
    EXPECT_EQ(i, COOP_NET_BRIDGE_QUEUE_CAPACITY + 2);
    EXPECT_EQ(sent, 2);
}

TEST("Cloud Coop battle intent and hash fence turns and retry after a full queue")
{
    struct CoopBridgeMessage message;
    struct CoopBattleTurnBundle taken;
    u8 manifest[COOP_BATTLE_MANIFEST_SIZE] = {0};
    u8 bundle[28] = {0};
    u8 action[] = {COOP_BATTLE_ACTION_MOVE, 0, B_POSITION_OPPONENT_LEFT, 0};
    u8 peer_action[] = {COOP_BATTLE_ACTION_AUTO_MOVE, 0, 0, 0};
    u8 digest[COOP_BATTLE_DIGEST_SIZE] = {0x5A};
    u8 other_id[COOP_BATTLE_ID_SIZE] = {2};
    u8 i;
    u8 sent_intents = 0;
    u8 sent_hashes = 0;

    EstablishTestCloudSession();
    manifest[0] = 1;
    manifest[COOP_BATTLE_MANIFEST_KIND_OFFSET] = COOP_BATTLE_KIND_FRIENDLY;
    manifest[COOP_BATTLE_MANIFEST_RULES_OFFSET] = COOP_BATTLE_FRIENDLY_SINGLES; manifest[COOP_BATTLE_MANIFEST_RULES_OFFSET + 2] = 1;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_BATTLE_MANIFEST, 2, manifest, sizeof(manifest));
    EXPECT(!CoopBattleRuntime_SendActionIntent(other_id, 1, action, sizeof(action)));
    EXPECT(!CoopBattleRuntime_SendActionIntent(manifest, 0, action, sizeof(action)));
    EXPECT(!CoopBattleRuntime_SendActionIntent(manifest, 2, action, sizeof(action)));
    EXPECT(!CoopBattleRuntime_SendActionIntent(manifest, 1, action, 0));
    for (i = 0; i < COOP_NET_BRIDGE_QUEUE_CAPACITY; i++)
        EXPECT(CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_ROM_READY, NULL, 0));
    EXPECT(!CoopBattleRuntime_SendActionIntent(manifest, 1, action, sizeof(action)));
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT(CoopBattleRuntime_SendActionIntent(manifest, 1, action, sizeof(action)));
    EXPECT(!CoopBattleRuntime_SendActionIntent(manifest, 1, action, sizeof(action)));
    while (CoopNetBridge_DequeueGameToNetwork(&message))
    {
        if (message.type == COOP_BRIDGE_MESSAGE_ACTION_INTENT)
        {
            EXPECT_EQ(message.length, 19 + sizeof(action));
            EXPECT_EQ(memcmp(message.payload, manifest, COOP_BATTLE_ID_SIZE), 0);
            EXPECT_EQ(message.payload[16], 1);
            EXPECT_EQ(message.payload[17], 0);
            EXPECT_EQ(message.payload[18], sizeof(action));
            EXPECT_EQ(memcmp(message.payload + 19, action, sizeof(action)), 0);
            sent_intents++;
        }
    }
    EXPECT_EQ(sent_intents, 1);
    EXPECT(!CoopBattleRuntime_SendTurnResultHash(manifest, 1, digest, sizeof(digest)));
    bundle[0] = 1;
    bundle[16] = 1;
    bundle[18] = sizeof(action);
    bundle[19] = sizeof(peer_action);
    memcpy(bundle + 20, action, sizeof(action));
    memcpy(bundle + 24, peer_action, sizeof(peer_action));
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_TURN_BUNDLE, 3, bundle, sizeof(bundle));
    EXPECT(CoopBattleRuntime_TakeTurnBundle(&taken));
    EXPECT(!CoopBattleRuntime_SendTurnResultHash(other_id, 1, digest, sizeof(digest)));
    EXPECT(!CoopBattleRuntime_SendTurnResultHash(manifest, 2, digest, sizeof(digest)));
    EXPECT(!CoopBattleRuntime_SendTurnResultHash(manifest, 1, digest, sizeof(digest) - 1));
    for (i = 0; i < COOP_NET_BRIDGE_QUEUE_CAPACITY; i++)
        EXPECT(CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_ROM_READY, NULL, 0));
    EXPECT(!CoopBattleRuntime_SendTurnResultHash(manifest, 1, digest, sizeof(digest)));
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT(CoopBattleRuntime_SendTurnResultHash(manifest, 1, digest, sizeof(digest)));
    EXPECT(!CoopBattleRuntime_SendTurnResultHash(manifest, 1, digest, sizeof(digest)));
    while (CoopNetBridge_DequeueGameToNetwork(&message))
    {
        if (message.type == COOP_BRIDGE_MESSAGE_TURN_RESULT_HASH)
        {
            EXPECT_EQ(message.length, COOP_BATTLE_TURN_RESULT_HASH_SIZE);
            EXPECT_EQ(memcmp(message.payload, manifest, COOP_BATTLE_ID_SIZE), 0);
            EXPECT_EQ(message.payload[16], 1);
            EXPECT_EQ(message.payload[17], 0);
            EXPECT_EQ(memcmp(message.payload + 18, digest, sizeof(digest)), 0);
            sent_hashes++;
        }
    }
    EXPECT_EQ(sent_hashes, 1);
}

TEST("Cloud Coop same epoch replacement replays unread battle records after ROM ready")
{
    struct CoopBridgeMessage message;
    struct CoopBattleTurnBundle taken;
    struct Pokemon mon;
    u8 offer[COOP_BATTLE_JOIN_OFFER_SIZE] = {0};
    u8 manifest[COOP_BATTLE_MANIFEST_SIZE] = {0};
    u8 bundle[28] = {0};
    u8 action[] = {COOP_BATTLE_ACTION_MOVE, 0, B_POSITION_OPPONENT_LEFT, 0};
    u8 peer_action[] = {COOP_BATTLE_ACTION_AUTO_MOVE, 0, 0, 0};
    u8 digest[COOP_BATTLE_DIGEST_SIZE] = {0xC3};

    EstablishTestCloudSession();
    EXPECT(CoopBattleConsent_Begin(COOP_BATTLE_KIND_FRIENDLY));
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    offer[0] = 7;
    offer[16] = COOP_BATTLE_KIND_FRIENDLY;
    offer[COOP_BATTLE_JOIN_OFFER_RULES_OFFSET] = COOP_BATTLE_FRIENDLY_SINGLES; offer[COOP_BATTLE_JOIN_OFFER_RULES_OFFSET + 2] = 1;
    memcpy(offer + 18, message.payload + 1, 4);
    EXPECT(CoopBattleConsent_ReceiveOffer(offer, sizeof(offer)));
    CreateMon(&mon, SPECIES_BULBASAUR, 5, 1, OTID_STRUCT_PRESET(1));
    EXPECT(CoopBattleRuntime_SendPartySnapshot(offer, 0, 1, (const u8 *)&mon, sizeof(mon)));

    manifest[0] = 7;

    manifest[COOP_BATTLE_MANIFEST_KIND_OFFSET] = COOP_BATTLE_KIND_FRIENDLY;
    manifest[COOP_BATTLE_MANIFEST_RULES_OFFSET] = COOP_BATTLE_FRIENDLY_SINGLES; manifest[COOP_BATTLE_MANIFEST_RULES_OFFSET + 2] = 1;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_BATTLE_MANIFEST, 2, manifest, sizeof(manifest));
    EXPECT(CoopBattleRuntime_SendActionIntent(manifest, 1, action, sizeof(action)));
    bundle[0] = 7;
    bundle[16] = 1;
    bundle[18] = sizeof(action);
    bundle[19] = sizeof(peer_action);
    memcpy(bundle + 20, action, sizeof(action));
    memcpy(bundle + 24, peer_action, sizeof(peer_action));
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_TURN_BUNDLE, 3, bundle, sizeof(bundle));
    EXPECT(CoopBattleRuntime_TakeTurnBundle(&taken));
    EXPECT(CoopBattleRuntime_SendTurnResultHash(manifest, 1, digest, sizeof(digest)));

    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  4, 17, NULL, 0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_PARTY_SNAPSHOT);
    EXPECT_EQ(message.length, COOP_BATTLE_PARTY_SNAPSHOT_SIZE);
    EXPECT_EQ(memcmp(message.payload, offer, COOP_BATTLE_ID_SIZE), 0);
    EXPECT_EQ(memcmp(message.payload + 20, &mon, sizeof(mon)), 0);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ACTION_INTENT);
    EXPECT_EQ(message.length, 19 + sizeof(action));
    EXPECT_EQ(memcmp(message.payload + 19, action, sizeof(action)), 0);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_TURN_RESULT_HASH);
    EXPECT_EQ(message.length, COOP_BATTLE_TURN_RESULT_HASH_SIZE);
    EXPECT_EQ(memcmp(message.payload + 18, digest, sizeof(digest)), 0);
    EXPECT(!CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT(!CoopBattleRuntime_SendPartySnapshot(offer, 0, 1, (const u8 *)&mon, sizeof(mon)));
    EXPECT(!CoopBattleRuntime_SendActionIntent(manifest, 1, action, sizeof(action)));
    EXPECT(!CoopBattleRuntime_SendTurnResultHash(manifest, 1, digest, sizeof(digest)));
}

TEST("Cloud Coop partial battle replay survives a second replacement in order")
{
    struct CoopBridgeMessage message;
    const u16 types[] = {
        COOP_BRIDGE_MESSAGE_TRAINER_BATTLE_RESERVE,
        COOP_BRIDGE_MESSAGE_BATTLE_JOIN_RESPONSE,
        COOP_BRIDGE_MESSAGE_PARTY_SNAPSHOT,
    };
    u8 payload[COOP_BATTLE_PARTY_SNAPSHOT_SIZE] = {0};
    u16 i;

    EstablishTestCloudSession();
    CoopBridgeQueue_Init(&gCoopNetBridge.game_to_network);
    for (i = 0; i < ARRAY_COUNT(types); i++)
    {
        payload[0] = i + 1;
        EXPECT(CoopBridgeMessage_Seal(&message, types[i], i + 1, 17,
                                      payload, i == 2 ? sizeof(payload) : 5));
        EXPECT(CoopBridgeQueue_Push(&gCoopNetBridge.game_to_network, &message));
    }
    CoopBattleRuntime_PreserveOutbound();
    CoopBridgeQueue_Init(&gCoopNetBridge.game_to_network);
    EXPECT(CoopBattleRuntime_HasPendingOutboundReplay());
    for (i = 0; i < COOP_NET_BRIDGE_QUEUE_CAPACITY - 1; i++)
    {
        EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_ROM_READY,
                                      i + 4, 17, NULL, 0));
        EXPECT(CoopBridgeQueue_Push(&gCoopNetBridge.game_to_network, &message));
    }
    CoopBattleRuntime_PollOutboundReplay();
    for (i = 0; i < COOP_NET_BRIDGE_QUEUE_CAPACITY - 1; i++)
    {
        EXPECT(CoopBridgeQueue_Pop(&gCoopNetBridge.game_to_network, &message));
        EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    }
    EXPECT(CoopBridgeQueue_Pop(&gCoopNetBridge.game_to_network, &message));
    EXPECT_EQ(message.type, types[0]);
    EXPECT_EQ(message.payload[0], 1);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    CoopBattleRuntime_PreserveOutbound();
    CoopBridgeQueue_Init(&gCoopNetBridge.game_to_network);
    CoopBattleRuntime_PollOutboundReplay();
    for (i = 1; i < ARRAY_COUNT(types); i++)
    {
        EXPECT(CoopBridgeQueue_Pop(&gCoopNetBridge.game_to_network, &message));
        EXPECT_EQ(message.type, types[i]);
        EXPECT_EQ(message.payload[0], i + 1);
    }
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
}

TEST("Cloud Coop heartbeat stale replays unread battle records on reconnect")
{
    struct CoopBridgeMessage message;
    struct CoopBattleTurnBundle taken;
    struct Pokemon mon;
    u8 offer[COOP_BATTLE_JOIN_OFFER_SIZE] = {0};
    u8 manifest[COOP_BATTLE_MANIFEST_SIZE] = {0};
    u8 bundle[28] = {0};
    u8 action[] = {COOP_BATTLE_ACTION_MOVE, 0, B_POSITION_OPPONENT_LEFT, 0};
    u8 peer_action[] = {COOP_BATTLE_ACTION_AUTO_MOVE, 0, 0, 0};
    u8 digest[COOP_BATTLE_DIGEST_SIZE] = {0xC3};
    u32 frame;

    EstablishTestCloudSession();
    EXPECT(CoopBattleConsent_Begin(COOP_BATTLE_KIND_FRIENDLY));
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    offer[0] = 7;
    offer[16] = COOP_BATTLE_KIND_FRIENDLY;
    offer[COOP_BATTLE_JOIN_OFFER_RULES_OFFSET] = COOP_BATTLE_FRIENDLY_SINGLES; offer[COOP_BATTLE_JOIN_OFFER_RULES_OFFSET + 2] = 1;
    memcpy(offer + 18, message.payload + 1, 4);
    EXPECT(CoopBattleConsent_ReceiveOffer(offer, sizeof(offer)));
    CreateMon(&mon, SPECIES_BULBASAUR, 5, 1, OTID_STRUCT_PRESET(1));
    EXPECT(CoopBattleRuntime_SendPartySnapshot(offer, 0, 1, (const u8 *)&mon, sizeof(mon)));
    manifest[0] = 7;
    manifest[COOP_BATTLE_MANIFEST_KIND_OFFSET] = COOP_BATTLE_KIND_FRIENDLY;
    manifest[COOP_BATTLE_MANIFEST_RULES_OFFSET] = COOP_BATTLE_FRIENDLY_SINGLES; manifest[COOP_BATTLE_MANIFEST_RULES_OFFSET + 2] = 1;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_BATTLE_MANIFEST, 2, manifest, sizeof(manifest));
    EXPECT(CoopBattleRuntime_SendActionIntent(manifest, 1, action, sizeof(action)));
    bundle[0] = 7;
    bundle[16] = 1;
    bundle[18] = sizeof(action);
    bundle[19] = sizeof(peer_action);
    memcpy(bundle + 20, action, sizeof(action));
    memcpy(bundle + 24, peer_action, sizeof(peer_action));
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_TURN_BUNDLE, 3, bundle, sizeof(bundle));
    EXPECT(CoopBattleRuntime_TakeTurnBundle(&taken));
    EXPECT(CoopBattleRuntime_SendTurnResultHash(manifest, 1, digest, sizeof(digest)));
    for (frame = 0; frame < COOP_NET_BRIDGE_SIDECAR_STALE_INTERVAL; frame++)
        CoopNetBridge_Poll();
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SIDECAR_HEARTBEAT_STALE);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));

    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  4, 17, NULL, 0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_PARTY_SNAPSHOT);
    EXPECT_EQ(memcmp(message.payload + 20, &mon, sizeof(mon)), 0);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ACTION_INTENT);
    EXPECT_EQ(memcmp(message.payload + 19, action, sizeof(action)), 0);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_TURN_RESULT_HASH);
    EXPECT_EQ(memcmp(message.payload + 18, digest, sizeof(digest)), 0);
}

TEST("Cloud Coop same epoch replacement replays an unread battle reserve")
{
    struct CoopBridgeMessage message;
    u8 reserve[COOP_BATTLE_RESERVE_SIZE];

    EstablishTestCloudSession();
    EXPECT(CoopBattleConsent_Begin(COOP_BATTLE_KIND_FRIENDLY));
    memcpy(reserve, gCoopNetBridge.game_to_network.entries[
        (gCoopNetBridge.game_to_network.write_index - 1)
            & (COOP_NET_BRIDGE_QUEUE_CAPACITY - 1)].payload, sizeof(reserve));
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  2, 17, NULL, 0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_TRAINER_BATTLE_RESERVE);
    EXPECT_EQ(message.length, COOP_BATTLE_RESERVE_SIZE);
    EXPECT_EQ(memcmp(message.payload, reserve, sizeof(reserve)), 0);
}

TEST("Cloud Coop new epoch discards an unread battle reserve")
{
    struct CoopBridgeMessage message;

    EstablishTestCloudSession();
    EXPECT(CoopBattleConsent_Begin(COOP_BATTLE_KIND_FRIENDLY));
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  2, 18, NULL, 0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    EXPECT_EQ(message.session_epoch, 18);
    EXPECT(!CoopNetBridge_DequeueGameToNetwork(&message));
}

TEST("Cloud Coop accepted battle consent survives its offer deadline")
{
    struct CoopBridgeMessage message;
    u8 offer[COOP_BATTLE_JOIN_OFFER_SIZE] = {0};
    MainCallback saved_callback1 = gMain.callback1;
    MainCallback saved_callback2 = gMain.callback2;
    bool8 saved_fade_active = gPaletteFade.active;
    bool8 saved_controls_locked = ArePlayerFieldControlsLocked();
    u32 saved_frame = gMain.vblankCounter1;

    EstablishTestCloudSession();
    offer[0] = 9;
    offer[16] = COOP_BATTLE_KIND_FRIENDLY;
    offer[COOP_BATTLE_JOIN_OFFER_RULES_OFFSET] = COOP_BATTLE_FRIENDLY_SINGLES; offer[COOP_BATTLE_JOIN_OFFER_RULES_OFFSET + 2] = 1;
    offer[17] = 1;
    EXPECT(CoopBattleConsent_ReceiveOffer(offer, sizeof(offer)));
    gMain.callback1 = CB1_Overworld;
    gMain.callback2 = CB2_Overworld;
    gPaletteFade.active = FALSE;
    ScriptContext_Stop();
    if (saved_controls_locked)
        UnlockPlayerFieldControls();
    gMain.vblankCounter1 = saved_frame + 30 * 60 - 1;
    CoopBattleConsent_Poll();
    Special_CoopBattleConsentGetOffer();
    EXPECT_EQ(gSpecialVar_Result, COOP_BATTLE_KIND_FRIENDLY);
    gSpecialVar_0x8004 = TRUE;
    Special_CoopBattleConsentRespond();
    EXPECT_EQ(gSpecialVar_Result, TRUE);
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  2, 17, NULL, 0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_BATTLE_JOIN_RESPONSE);
    EXPECT_EQ(message.length, COOP_BATTLE_JOIN_RESPONSE_SIZE);
    EXPECT_EQ(memcmp(message.payload, offer, COOP_BATTLE_ID_SIZE), 0);
    EXPECT_EQ(message.payload[16], 1);
    gMain.vblankCounter1 += 2;
    ScriptContext_Stop();
    CoopBattleConsent_Poll();
    EXPECT(CoopBattleConsent_IsCurrentBattle(offer));
    gMain.callback1 = saved_callback1;
    gMain.callback2 = saved_callback2;
    gPaletteFade.active = saved_fade_active;
    gMain.vblankCounter1 = saved_frame;
    if (saved_controls_locked)
        LockPlayerFieldControls();
}

TEST("Cloud Coop group ended notice accepts active epoch and exact UUID payload")
{
    const u8 groupId[16] = {1};
    struct CoopBridgeMessage message;

    EstablishTestCloudSession();
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_GROUP_ENDED,
                                  2, 17, groupId, sizeof(groupId)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR));
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_GROUP_ENDED,
                                  3, 18, groupId, sizeof(groupId)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR));
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_GROUP_ENDED,
                                  4, 17, groupId, 15));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR);
}

TEST("Cloud Coop end-turn digest waits for capacity and reconnect without duplicate hash")
{
    struct CoopBridgeMessage message;
    struct CoopBattleTurnBundle taken;
    struct Pokemon mon;
    u8 manifest[COOP_BATTLE_MANIFEST_SIZE] = {0};
    u8 chunk[COOP_BATTLE_PARTY_SNAPSHOT_SIZE] = {0};
    u8 bundle[20 + 2 * COOP_BATTLE_ACTION_SIZE] = {0};
    u8 action[COOP_BATTLE_ACTION_SIZE] = {COOP_BATTLE_ACTION_MOVE, 0,
                                        B_POSITION_OPPONENT_LEFT, 0};
    u8 peer_action[COOP_BATTLE_ACTION_SIZE] = {COOP_BATTLE_ACTION_AUTO_MOVE, 0, 0, 0};
    u8 digest[COOP_BATTLE_DIGEST_SIZE] = {0x5A};
    u8 i;
    u8 hash_count = 0;

    EstablishTestCloudSession();
    manifest[0] = 7;
    manifest[COOP_BATTLE_MANIFEST_KIND_OFFSET] = COOP_BATTLE_MANIFEST_KIND_TRAINER;
    manifest[COOP_BATTLE_MANIFEST_REGION_OFFSET] = COOP_REGION_HOENN;
    manifest[COOP_BATTLE_MANIFEST_TRAINER_ORDINAL_OFFSET] =
        COOP_TRAINER_HOENN_TRAINER_WALLY_1_ORDINAL & 0xFF;
    manifest[COOP_BATTLE_MANIFEST_TRAINER_ORDINAL_OFFSET + 1] =
        COOP_TRAINER_HOENN_TRAINER_WALLY_1_ORDINAL >> 8;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_BATTLE_MANIFEST, 2, manifest, sizeof(manifest));
    CreateMon(&mon, SPECIES_BULBASAUR, 5, 1, OTID_STRUCT_PRESET(1));
    chunk[0] = manifest[0];
    chunk[18] = COOP_BATTLE_MULTI_PARTY_SIZE;
    chunk[19] = COOP_BATTLE_PARTY_MON_SIZE;
    memcpy(chunk + 20, &mon, sizeof(mon));
    for (i = 0; i < COOP_BATTLE_MULTI_PARTY_SIZE; i++)
    {
        chunk[16] = i;
        chunk[17] = i;
        EXPECT_EQ(CoopBattleRuntime_ReceivePeerPartyChunk(chunk, sizeof(chunk)),
                  COOP_BATTLE_INBOUND_ACCEPTED);
    }
    EXPECT(CoopBattleRuntime_ArmEngine(manifest));
    EXPECT(CoopBattleRuntime_SendActionIntent(manifest, 1, action, sizeof(action)));
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    memcpy(bundle, manifest, COOP_BATTLE_ID_SIZE);
    bundle[16] = 1;
    bundle[18] = sizeof(action);
    bundle[19] = sizeof(peer_action);
    memcpy(bundle + 20, action, sizeof(action));
    memcpy(bundle + 20 + sizeof(action), peer_action, sizeof(peer_action));
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_TURN_BUNDLE, 3, bundle, sizeof(bundle));
    EXPECT(CoopBattleRuntime_TakeTurnBundle(&taken));
    EXPECT_EQ(CoopBattleRuntime_TestReportTurnDigest(2, 0, digest),
              COOP_BATTLE_TURN_REPORT_INVALID);
    for (i = 0; i < COOP_NET_BRIDGE_QUEUE_CAPACITY; i++)
        EXPECT(CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_ROM_READY, NULL, 0));
    EXPECT_EQ(CoopBattleRuntime_TestReportTurnDigest(1, 0, digest),
              COOP_BATTLE_TURN_REPORT_PENDING);
    EXPECT(CoopBattleRuntime_IsTurnHashPending());
    digest[0] = 0xC3;
    CoopBattleRuntime_OnTransportLost();
    EXPECT_EQ(CoopBattleRuntime_RetryTurnHash(), COOP_BATTLE_TURN_REPORT_PENDING);
    CoopBattleRuntime_OnSessionReady(17);
    EXPECT_EQ(CoopBattleRuntime_RetryTurnHash(), COOP_BATTLE_TURN_REPORT_PENDING);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(CoopBattleRuntime_RetryTurnHash(), COOP_BATTLE_TURN_REPORT_SENT);
    EXPECT(!CoopBattleRuntime_IsTurnHashPending());
    EXPECT_EQ(CoopBattleRuntime_RetryTurnHash(), COOP_BATTLE_TURN_REPORT_INVALID);
    while (CoopNetBridge_DequeueGameToNetwork(&message))
    {
        if (message.type == COOP_BRIDGE_MESSAGE_TURN_RESULT_HASH)
        {
            EXPECT_EQ(message.payload[16], 1);
            EXPECT_EQ(message.payload[18], 0x5A);
            hash_count++;
        }
    }
    EXPECT_EQ(hash_count, 1);
}

TEST("Cloud Coop rejected reserve clears only the matching pending request")
{
    struct CoopBridgeMessage outbound;
    u8 nonce[COOP_BATTLE_RESERVE_REJECTED_SIZE];
    u8 wrong[COOP_BATTLE_RESERVE_REJECTED_SIZE];
    u8 zero[COOP_BATTLE_RESERVE_REJECTED_SIZE] = {0};

    EstablishTestCloudSession();
    EXPECT(CoopBattleConsent_Begin(COOP_BATTLE_KIND_FRIENDLY));
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&outbound));
    memcpy(nonce, outbound.payload + 1, sizeof(nonce));
    memcpy(wrong, nonce, sizeof(wrong));
    wrong[0] = nonce[0] == 1 ? 2 : 1;
    EXPECT(!CoopBattleConsent_ReceiveReserveRejected(NULL, sizeof(nonce)));
    EXPECT(!CoopBattleConsent_ReceiveReserveRejected(nonce, sizeof(nonce) - 1));
    EXPECT(!CoopBattleConsent_ReceiveReserveRejected(wrong, sizeof(wrong)));
    EXPECT(!CoopBattleConsent_Begin(COOP_BATTLE_KIND_FRIENDLY));
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_BATTLE_RESERVE_REJECTED,
                            2, zero, sizeof(zero));
    EXPECT(!CoopBattleConsent_Begin(COOP_BATTLE_KIND_FRIENDLY));
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_BATTLE_RESERVE_REJECTED,
                            2, nonce, sizeof(nonce) - 1);
    EXPECT(!CoopBattleConsent_Begin(COOP_BATTLE_KIND_FRIENDLY));
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_BATTLE_RESERVE_REJECTED,
                            2, nonce, sizeof(nonce));
    EXPECT(CoopBattleConsent_Begin(COOP_BATTLE_KIND_FRIENDLY));
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&outbound));
    memcpy(nonce, outbound.payload + 1, sizeof(nonce));
    memcpy(wrong, nonce, sizeof(wrong));
    wrong[0] = nonce[0] == 1 ? 2 : 1;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_BATTLE_RESERVE_REJECTED,
                            3, wrong, sizeof(wrong));
    EXPECT(!CoopBattleConsent_Begin(COOP_BATTLE_KIND_FRIENDLY));
    EXPECT(CoopBattleConsent_ReceiveReserveRejected(nonce, sizeof(nonce)));
    EXPECT(CoopBattleConsent_Begin(COOP_BATTLE_KIND_FRIENDLY));
}

TEST("Cloud Coop copies responder battle ID only after local accept and clears it on abort")
{
    struct CoopBridgeMessage outbound;
    u8 offer[COOP_BATTLE_JOIN_OFFER_SIZE] = {0};
    u8 abortPayload[COOP_BATTLE_ABORT_SIZE] = {0};
    u8 copied[COOP_BATTLE_ID_SIZE] = {0};
    MainCallback saved_callback1 = gMain.callback1;
    MainCallback saved_callback2 = gMain.callback2;
    bool8 saved_fade_active = gPaletteFade.active;
    bool8 saved_controls_locked = ArePlayerFieldControlsLocked();

    EstablishTestCloudSession();
    offer[0] = 13;
    offer[16] = COOP_BATTLE_KIND_FRIENDLY;
    offer[COOP_BATTLE_JOIN_OFFER_RULES_OFFSET] = COOP_BATTLE_FRIENDLY_SINGLES; offer[COOP_BATTLE_JOIN_OFFER_RULES_OFFSET + 2] = 1;
    offer[17] = 1;
    EXPECT(CoopBattleConsent_ReceiveOffer(offer, sizeof(offer)));
    EXPECT(!CoopBattleConsent_CopyCurrentBattleId(copied, sizeof(copied)));
    gMain.callback1 = CB1_Overworld;
    gMain.callback2 = CB2_Overworld;
    gPaletteFade.active = FALSE;
    ScriptContext_Stop();
    if (saved_controls_locked)
        UnlockPlayerFieldControls();
    CoopBattleConsent_Poll();
    gSpecialVar_0x8004 = TRUE;
    Special_CoopBattleConsentRespond();
    EXPECT_EQ(gSpecialVar_Result, TRUE);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&outbound));
    EXPECT_EQ(outbound.type, COOP_BRIDGE_MESSAGE_BATTLE_JOIN_RESPONSE);
    EXPECT_EQ(CoopBattleConsent_GetOutcome(), 0); // Local decision is not server acceptance.
    EXPECT(!CoopBattleConsent_CopyCurrentBattleId(NULL, sizeof(copied)));
    EXPECT(!CoopBattleConsent_CopyCurrentBattleId(copied, sizeof(copied) - 1));
    EXPECT(CoopBattleConsent_CopyCurrentBattleId(copied, sizeof(copied)));
    EXPECT_EQ(memcmp(copied, offer, sizeof(copied)), 0);
    CoopBattleConsent_OnTransportLost();
    EXPECT(!CoopBattleConsent_CopyCurrentBattleId(copied, sizeof(copied)));
    CoopBattleConsent_OnSessionReady();
    EXPECT(CoopBattleConsent_CopyCurrentBattleId(copied, sizeof(copied)));
    memcpy(abortPayload, offer, COOP_BATTLE_ID_SIZE);
    abortPayload[16] = COOP_BATTLE_ABORT_CANCELED;
    EXPECT(CoopBattleConsent_ReceiveAbort(abortPayload, sizeof(abortPayload)));
    EXPECT(!CoopBattleConsent_CopyCurrentBattleId(copied, sizeof(copied)));
    ScriptContext_Stop();
    CoopBattleConsent_Poll();
    gMain.callback1 = saved_callback1;
    gMain.callback2 = saved_callback2;
    gPaletteFade.active = saved_fade_active;
    if (saved_controls_locked)
        LockPlayerFieldControls();
}

TEST("Cloud Coop requester keeps declined outcome after matching abort")
{
    struct CoopBridgeMessage outbound;
    u8 offer[COOP_BATTLE_JOIN_OFFER_SIZE] = {0};
    u8 outcome[COOP_BATTLE_CONSENT_OUTCOME_SIZE] = {0};
    u8 abortPayload[COOP_BATTLE_ABORT_SIZE] = {0};

    EstablishTestCloudSession();
    EXPECT(CoopBattleConsent_Begin(COOP_BATTLE_KIND_FRIENDLY));
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&outbound));
    EXPECT_EQ(outbound.type, COOP_BRIDGE_MESSAGE_TRAINER_BATTLE_RESERVE);
    offer[0] = 9;
    offer[16] = COOP_BATTLE_KIND_FRIENDLY;
    offer[COOP_BATTLE_JOIN_OFFER_RULES_OFFSET] = COOP_BATTLE_FRIENDLY_SINGLES; offer[COOP_BATTLE_JOIN_OFFER_RULES_OFFSET + 2] = 1;
    memcpy(offer + 18, outbound.payload + 1, 4);
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_BATTLE_JOIN_OFFER, 2, offer, sizeof(offer));
    EXPECT(CoopBattleConsent_IsCurrentBattle(offer));
    memcpy(outcome, offer, COOP_BATTLE_ID_SIZE);
    memcpy(outcome + 16, offer + 18, 4);
    outcome[20] = COOP_BATTLE_CONSENT_DECLINED;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_BATTLE_CONSENT_OUTCOME, 3, outcome, sizeof(outcome));
    Special_CoopBattleConsentGetOutcome();
    EXPECT_EQ(gSpecialVar_Result, COOP_BATTLE_CONSENT_DECLINED);
    memcpy(abortPayload, offer, COOP_BATTLE_ID_SIZE);
    abortPayload[16] = COOP_BATTLE_ABORT_CANCELED;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_ABORT_BATTLE, 4, abortPayload, sizeof(abortPayload));
    Special_CoopBattleConsentGetOutcome();
    EXPECT_EQ(gSpecialVar_Result, COOP_BATTLE_CONSENT_DECLINED);
}

TEST("Cloud Coop matching abort clears stale accepted consent")
{
    struct CoopBridgeMessage outbound;
    u8 offer[COOP_BATTLE_JOIN_OFFER_SIZE] = {0};
    u8 outcome[COOP_BATTLE_CONSENT_OUTCOME_SIZE] = {0};
    u8 abortPayload[COOP_BATTLE_ABORT_SIZE] = {0};

    EstablishTestCloudSession();
    EXPECT(CoopBattleConsent_Begin(COOP_BATTLE_KIND_FRIENDLY));
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&outbound));
    offer[0] = 8;
    offer[16] = COOP_BATTLE_KIND_FRIENDLY;
    offer[COOP_BATTLE_JOIN_OFFER_RULES_OFFSET] = COOP_BATTLE_FRIENDLY_SINGLES; offer[COOP_BATTLE_JOIN_OFFER_RULES_OFFSET + 2] = 1;
    memcpy(offer + 18, outbound.payload + 1, 4);
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_BATTLE_JOIN_OFFER, 2, offer, sizeof(offer));
    memcpy(outcome, offer, COOP_BATTLE_ID_SIZE);
    memcpy(outcome + 16, offer + 18, 4);
    outcome[20] = COOP_BATTLE_CONSENT_ACCEPTED;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_BATTLE_CONSENT_OUTCOME, 3, outcome, sizeof(outcome));
    Special_CoopBattleConsentGetOutcome();
    EXPECT_EQ(gSpecialVar_Result, COOP_BATTLE_CONSENT_ACCEPTED);
    memcpy(abortPayload, offer, COOP_BATTLE_ID_SIZE);
    abortPayload[16] = COOP_BATTLE_ABORT_CANCELED;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_ABORT_BATTLE, 4, abortPayload, sizeof(abortPayload));
    Special_CoopBattleConsentGetOutcome();
    EXPECT_EQ(gSpecialVar_Result, 0);
}

TEST("Cloud Coop battle transport retains a bounded manifest and ordered turns")
{
    u8 manifest[COOP_BATTLE_MANIFEST_SIZE] = {0};
    u8 copied[COOP_BATTLE_MANIFEST_SIZE];
    u8 bundle[28] = {0};
    u8 action[] = {COOP_BATTLE_ACTION_MOVE, 0, B_POSITION_OPPONENT_LEFT, 0};
    u8 peer_action[] = {COOP_BATTLE_ACTION_AUTO_MOVE, 0, 0, 0};
    u8 pause[COOP_BATTLE_PAUSE_FOR_RECONNECT_SIZE] = {0};
    u8 abortPayload[COOP_BATTLE_ABORT_SIZE] = {0};
    struct CoopBridgeMessage message;
    struct CoopBattleTurnBundle taken;
    u16 pauseTurn;
    u8 missingSlot;

    EstablishTestCloudSession();
    manifest[0] = 0xA1;
    manifest[COOP_BATTLE_MANIFEST_KIND_OFFSET] = COOP_BATTLE_KIND_FRIENDLY;
    manifest[COOP_BATTLE_MANIFEST_RULES_OFFSET] = COOP_BATTLE_FRIENDLY_SINGLES; manifest[COOP_BATTLE_MANIFEST_RULES_OFFSET + 2] = 1;
    manifest[18] = 0x52;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_BATTLE_MANIFEST, 2, manifest, sizeof(manifest));
    EXPECT(CoopBattleRuntime_HasManifest());
    EXPECT(CoopBattleRuntime_GetManifest(copied, sizeof(copied)));
    EXPECT_EQ(memcmp(copied, manifest, sizeof(manifest)), 0);

    EXPECT(CoopBattleRuntime_SendActionIntent(manifest, 1, action, sizeof(action)));
    bundle[0] = 0xA1;
    bundle[16] = 1;
    bundle[18] = sizeof(action);
    bundle[19] = sizeof(peer_action);
    memcpy(bundle + 20, action, sizeof(action));
    memcpy(bundle + 24, peer_action, sizeof(peer_action));
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_TURN_BUNDLE, 3, bundle, sizeof(bundle));
    while (CoopNetBridge_DequeueGameToNetwork(&message))
        ;
    bundle[16] = 2;
    EXPECT(CoopBattleRuntime_SendActionIntent(manifest, 2, action, sizeof(action)));
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_TURN_BUNDLE, 4, bundle, sizeof(bundle));
    EXPECT(CoopBattleRuntime_TakeTurnBundle(&taken));
    EXPECT_EQ(taken.turn, 1);
    EXPECT_EQ(taken.first_action[0], COOP_BATTLE_ACTION_MOVE);
    EXPECT_EQ(taken.second_action[1], 0);
    EXPECT(CoopBattleRuntime_TakeTurnBundle(&taken));
    EXPECT_EQ(taken.turn, 2);
    EXPECT(!CoopBattleRuntime_TakeTurnBundle(&taken));
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_TURN_BUNDLE, 5, bundle, sizeof(bundle));
    EXPECT(!CoopBattleRuntime_TakeTurnBundle(&taken));
    bundle[16] = 1;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_TURN_BUNDLE, 6, bundle, sizeof(bundle));
    EXPECT(!CoopBattleRuntime_TakeTurnBundle(&taken));
    EXPECT_EQ(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR, 0);

    pause[0] = 0xA1;
    pause[16] = 3;
    pause[18] = 1;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_PAUSE_FOR_RECONNECT, 7, pause, sizeof(pause));
    EXPECT(CoopBattleRuntime_GetPause(&pauseTurn, &missingSlot));
    EXPECT_EQ(pauseTurn, 3);
    EXPECT_EQ(missingSlot, 1);
    abortPayload[0] = 0xA1;
    abortPayload[16] = COOP_BATTLE_ABORT_CANCELED;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_ABORT_BATTLE, 8, abortPayload, sizeof(abortPayload));
    EXPECT(!CoopBattleRuntime_HasManifest());
}

TEST("Cloud Coop battle transport distinguishes malformed records from stale battle state")
{
    u8 manifest[COOP_BATTLE_MANIFEST_SIZE] = {0};
    u8 bundle[28] = {0};
    u8 action[] = {COOP_BATTLE_ACTION_MOVE, 0, B_POSITION_OPPONENT_LEFT, 0};
    u8 peer_action[] = {COOP_BATTLE_ACTION_AUTO_MOVE, 0, 0, 0};
    struct CoopBridgeMessage message;

    EstablishTestCloudSession();
    manifest[0] = 1;
    manifest[COOP_BATTLE_MANIFEST_KIND_OFFSET] = COOP_BATTLE_KIND_FRIENDLY;
    manifest[COOP_BATTLE_MANIFEST_RULES_OFFSET] = COOP_BATTLE_FRIENDLY_SINGLES; manifest[COOP_BATTLE_MANIFEST_RULES_OFFSET + 2] = 1;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_BATTLE_MANIFEST, 2, manifest, sizeof(manifest));
    manifest[0] = 2;
    manifest[COOP_BATTLE_MANIFEST_KIND_OFFSET] = COOP_BATTLE_KIND_FRIENDLY;
    manifest[COOP_BATTLE_MANIFEST_RULES_OFFSET] = COOP_BATTLE_FRIENDLY_SINGLES; manifest[COOP_BATTLE_MANIFEST_RULES_OFFSET + 2] = 1;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_BATTLE_MANIFEST, 3, manifest, sizeof(manifest));
    EXPECT_EQ(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR, 0);
    bundle[0] = 2;
    bundle[16] = 1;
    bundle[18] = sizeof(action);
    bundle[19] = sizeof(peer_action);
    memcpy(bundle + 20, action, sizeof(action));
    memcpy(bundle + 24, peer_action, sizeof(peer_action));
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_TURN_BUNDLE, 4, bundle, sizeof(bundle));
    EXPECT_EQ(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR, 0);
    bundle[0] = 1;
    bundle[18] = 49;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_TURN_BUNDLE, 5, bundle, sizeof(bundle));
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR);

    EstablishTestCloudSession();
    manifest[0] = 1;
    manifest[COOP_BATTLE_MANIFEST_KIND_OFFSET] = COOP_BATTLE_KIND_FRIENDLY;
    manifest[COOP_BATTLE_MANIFEST_RULES_OFFSET] = COOP_BATTLE_FRIENDLY_SINGLES; manifest[COOP_BATTLE_MANIFEST_RULES_OFFSET + 2] = 1;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_BATTLE_MANIFEST, 2, manifest, sizeof(manifest));
    EXPECT(CoopBattleRuntime_HasManifest());
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_SESSION_READY, 3, 17, NULL, 0));
    HostWriteInboundUnchecked(&message);
    CoopNetBridge_Poll();
    EXPECT(CoopBattleRuntime_HasManifest());
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_SESSION_READY, 4, 18, NULL, 0));
    HostWriteInboundUnchecked(&message);
    CoopNetBridge_Poll();
    EXPECT(!CoopBattleRuntime_HasManifest());
}

TEST("Cloud Coop battle transport soft-drops skipped turns without losing the expected turn")
{
    u8 manifest[COOP_BATTLE_MANIFEST_SIZE] = {0};
    u8 bundle[28] = {0};
    u8 action[] = {COOP_BATTLE_ACTION_MOVE, 0, B_POSITION_OPPONENT_LEFT, 0};
    u8 peer_action[] = {COOP_BATTLE_ACTION_AUTO_MOVE, 0, 0, 0};
    struct CoopBattleTurnBundle taken;

    EstablishTestCloudSession();
    manifest[0] = 3;
    manifest[COOP_BATTLE_MANIFEST_KIND_OFFSET] = COOP_BATTLE_KIND_FRIENDLY;
    manifest[COOP_BATTLE_MANIFEST_RULES_OFFSET] = COOP_BATTLE_FRIENDLY_SINGLES; manifest[COOP_BATTLE_MANIFEST_RULES_OFFSET + 2] = 1;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_BATTLE_MANIFEST, 2, manifest, sizeof(manifest));
    EXPECT(CoopBattleRuntime_SendActionIntent(manifest, 1, action, sizeof(action)));
    bundle[0] = 3;
    bundle[18] = sizeof(action);
    bundle[19] = sizeof(peer_action);
    memcpy(bundle + 20, action, sizeof(action));
    memcpy(bundle + 24, peer_action, sizeof(peer_action));
    bundle[16] = COOP_BATTLE_MAX_TURN;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_TURN_BUNDLE, 3, bundle, sizeof(bundle));
    bundle[16] = 2;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_TURN_BUNDLE, 4, bundle, sizeof(bundle));
    EXPECT(!CoopBattleRuntime_TakeTurnBundle(&taken));
    bundle[16] = 1;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_TURN_BUNDLE, 5, bundle, sizeof(bundle));
    EXPECT(CoopBattleRuntime_TakeTurnBundle(&taken));
    EXPECT_EQ(taken.turn, 1);
    EXPECT(CoopBattleRuntime_SendActionIntent(manifest, 2, action, sizeof(action)));
    bundle[16] = 2;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_TURN_BUNDLE, 6, bundle, sizeof(bundle));
    EXPECT(CoopBattleRuntime_TakeTurnBundle(&taken));
    EXPECT_EQ(taken.turn, 2);
    EXPECT_EQ(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR, 0);
}

TEST("Cloud Coop battle pause clears when its turn bundle arrives")
{
    u8 manifest[COOP_BATTLE_MANIFEST_SIZE] = {0};
    u8 pause[COOP_BATTLE_PAUSE_FOR_RECONNECT_SIZE] = {0};
    u8 bundle[28] = {0};
    u8 action[] = {COOP_BATTLE_ACTION_MOVE, 0, B_POSITION_OPPONENT_LEFT, 0};
    u8 peer_action[] = {COOP_BATTLE_ACTION_AUTO_MOVE, 0, 0, 0};
    u16 pauseTurn;
    u8 missingSlot;

    EstablishTestCloudSession();
    manifest[0] = 4;
    manifest[COOP_BATTLE_MANIFEST_KIND_OFFSET] = COOP_BATTLE_KIND_FRIENDLY;
    manifest[COOP_BATTLE_MANIFEST_RULES_OFFSET] = COOP_BATTLE_FRIENDLY_SINGLES; manifest[COOP_BATTLE_MANIFEST_RULES_OFFSET + 2] = 1;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_BATTLE_MANIFEST, 2, manifest, sizeof(manifest));
    pause[0] = 4;
    pause[16] = 1;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_PAUSE_FOR_RECONNECT, 3, pause, sizeof(pause));
    EXPECT(CoopBattleRuntime_GetPause(&pauseTurn, &missingSlot));
    EXPECT(CoopBattleRuntime_SendActionIntent(manifest, 1, action, sizeof(action)));
    bundle[0] = 4;
    bundle[16] = 1;
    bundle[18] = sizeof(action);
    bundle[19] = sizeof(peer_action);
    memcpy(bundle + 20, action, sizeof(action));
    memcpy(bundle + 24, peer_action, sizeof(peer_action));
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_TURN_BUNDLE, 4, bundle, sizeof(bundle));
    EXPECT(!CoopBattleRuntime_GetPause(&pauseTurn, &missingSlot));
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_PAUSE_FOR_RECONNECT, 5, pause, sizeof(pause));
    EXPECT(!CoopBattleRuntime_GetPause(&pauseTurn, &missingSlot));
    EXPECT_EQ(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR, 0);
}

TEST("Cloud Coop battle transport rejects nonzero padding and invalid pause fields")
{
    u8 manifest[COOP_BATTLE_MANIFEST_SIZE] = {0};
    u8 pause[COOP_BATTLE_PAUSE_FOR_RECONNECT_SIZE] = {0};
    struct CoopBridgeMessage message;

    EstablishTestCloudSession();
    manifest[0] = 1;
    manifest[COOP_BATTLE_MANIFEST_KIND_OFFSET] = COOP_BATTLE_KIND_FRIENDLY;
    manifest[COOP_BATTLE_MANIFEST_RULES_OFFSET] = COOP_BATTLE_FRIENDLY_SINGLES; manifest[COOP_BATTLE_MANIFEST_RULES_OFFSET + 2] = 1;
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_BATTLE_MANIFEST,
                                  2, 17, manifest, sizeof(manifest)));
    message.payload[sizeof(manifest)] = 1;
    message.checksum = CoopBridgeMessage_ComputeChecksum(&message);
    HostWriteInboundUnchecked(&message);
    CoopNetBridge_Poll();
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR);
    EXPECT(!CoopBattleRuntime_HasManifest());

    EstablishTestCloudSession();
    manifest[0] = 1;
    manifest[COOP_BATTLE_MANIFEST_KIND_OFFSET] = COOP_BATTLE_KIND_FRIENDLY;
    manifest[COOP_BATTLE_MANIFEST_RULES_OFFSET] = COOP_BATTLE_FRIENDLY_SINGLES; manifest[COOP_BATTLE_MANIFEST_RULES_OFFSET + 2] = 1;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_BATTLE_MANIFEST, 2, manifest, sizeof(manifest));
    pause[0] = 1;
    pause[18] = 2;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_PAUSE_FOR_RECONNECT, 3, pause, sizeof(pause));
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR);

    EstablishTestCloudSession();
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_BATTLE_MANIFEST, 2, manifest, sizeof(manifest));
    pause[18] = 0;
    DeliverTestBattleRecord(COOP_BRIDGE_MESSAGE_PAUSE_FOR_RECONNECT, 3, pause, sizeof(pause));
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR);
}

static void StartTestCheckpoint(void)
{
    struct CoopBridgeMessage message;

    EXPECT_EQ(CoopNetBridge_RequestCheckpoint(), COOP_CHECKPOINT_REQUEST_STARTED);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_WAITING_FOR_GRANT);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_CHECKPOINT_READY);
    EXPECT_EQ(message.length, 0);
    EXPECT_EQ(message.session_epoch, 17);
}

static void DeliverTestGrant(u32 sequence, u32 epoch, u16 payloadSize)
{
    struct CoopBridgeMessage message;
    u8 payload = 0xA5;

    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_CHECKPOINT_GRANTED,
                                  sequence,
                                  epoch,
                                  &payload,
                                  payloadSize));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
}

TEST("Cloud Coop checkpoint request is online-only and requires drained queues")
{
    struct CoopBridgeMessage message;

    InitTestBridge();
    EXPECT_EQ(CoopNetBridge_RequestCheckpoint(), COOP_CHECKPOINT_REQUEST_OFFLINE);

    EstablishTestCloudSession();
    EXPECT(CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_PLAYER_STATE, NULL, 0));
    EXPECT_EQ(CoopNetBridge_RequestCheckpoint(), COOP_CHECKPOINT_REQUEST_REJECTED);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(CoopNetBridge_RequestCheckpoint(), COOP_CHECKPOINT_REQUEST_STARTED);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_WAITING_FOR_GRANT);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
}

TEST("Cloud Coop portal request validates stable ID and precedes checkpoint ready")
{
    struct CoopBridgeMessage message;
    char max_id[97];
    u32 i;

    InitTestBridge();
    EXPECT_EQ(CoopNetBridge_RequestPortalTravel("to_cormoria"), COOP_CHECKPOINT_REQUEST_REJECTED);
    EstablishTestCloudSession();
    EXPECT_EQ(CoopNetBridge_RequestPortalTravel(NULL), COOP_CHECKPOINT_REQUEST_REJECTED);
    EXPECT_EQ(CoopNetBridge_RequestPortalTravel(""), COOP_CHECKPOINT_REQUEST_REJECTED);
    EXPECT_EQ(CoopNetBridge_RequestPortalTravel("To_cormoria"), COOP_CHECKPOINT_REQUEST_REJECTED);
    EXPECT_EQ(CoopNetBridge_RequestPortalTravel("to-cormoria"), COOP_CHECKPOINT_REQUEST_REJECTED);
    EXPECT_EQ(CoopNetBridge_RequestPortalTravel("to_cormoria/evil"), COOP_CHECKPOINT_REQUEST_REJECTED);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));

    for (i = 0; i < 96; i++)
        max_id[i] = 'a';
    max_id[96] = '\0';
    EXPECT_EQ(CoopNetBridge_RequestPortalTravel(max_id), COOP_CHECKPOINT_REQUEST_STARTED);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_WAITING_FOR_GRANT);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_PORTAL_TRAVEL_REQUEST);
    EXPECT_EQ(message.length, 96);
    EXPECT_EQ(message.session_epoch, 17);
    EXPECT(memcmp(message.payload, max_id, 96) == 0);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_CHECKPOINT_READY);
    EXPECT_EQ(message.length, 0);
    EXPECT_EQ(message.session_epoch, 17);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    EXPECT_EQ(CoopNetBridge_RequestPortalTravel("to_cormoria"), COOP_CHECKPOINT_REQUEST_REJECTED);
}

TEST("Harbor script portals queue their fixed world routes only in cloud mode")
{
    struct CoopBridgeMessage message;
    struct ScriptContext ctx = {0};

    InitTestBridge();
    CoopNetBridge_ScriptPortalAvailable();
    EXPECT_EQ(gSpecialVar_Result, FALSE);
    CoopNetBridge_ScriptTravelToCormoria(&ctx);
    EXPECT_EQ(gSpecialVar_Result, FALSE);
    /* Offline is final at once: the script is never held for a retry. */
    EXPECT_EQ(ctx.nativePtr, NULL);
    EXPECT(!ctx.waitAfterCallNative);
    /* Only the ROM_READY that bridge init always announces is queued. */
    PopInitialRomReady();

    EstablishTestCloudSession();
    CoopNetBridge_ScriptPortalAvailable();
    EXPECT_EQ(gSpecialVar_Result, TRUE);
    CoopNetBridge_ScriptTravelToCormoria(&ctx);
    EXPECT_EQ(gSpecialVar_Result, TRUE);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_PORTAL_TRAVEL_REQUEST);
    EXPECT_EQ(message.length, 11);
    EXPECT(memcmp(message.payload, "to_cormoria", 11) == 0);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_CHECKPOINT_READY);

    EstablishTestCloudSession();
    CoopNetBridge_ScriptTravelToMain(&ctx);
    EXPECT_EQ(gSpecialVar_Result, TRUE);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_PORTAL_TRAVEL_REQUEST);
    EXPECT_EQ(message.length, 7);
    EXPECT(memcmp(message.payload, "to_main", 7) == 0);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_CHECKPOINT_READY);
}


/* Lua delivers inbound frames at VBlank and CoopNetBridge_Poll consumes them
 * after the frame's callbacks, so the harbor YES runs while an ordinary frame
 * is still in network_to_game. Player a's v8c ferry refusal: */
static void HostDeliverInboundBeforeScript(u32 sequence)
{
    struct CoopBridgeMessage message;
    u8 payload[COOP_ONLINE_STATUS_SIZE] = {0};

    payload[0] = 99; /* nobody's request: consumed and ignored by Poll */
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_ONLINE_STATUS,
                                 sequence, 17, payload, sizeof(payload)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
}

/* One emulated frame after the script step: Poll drains inbound. The test
 * bridge has no overworld, so clear the world-not-ready mark the way a
 * PlayerState from a ready field does. */
static void RunTestBridgeFrame(void)
{
    CoopNetBridge_Poll();
    gCoopNetBridge.status_flags &= ~COOP_BRIDGE_STATUS_WORLD_NOT_READY;
}

static void ExpectQueuedPortal(const char *portal_id, u16 length)
{
    struct CoopBridgeMessage message;

    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_PORTAL_TRAVEL_REQUEST);
    EXPECT_EQ(message.length, length);
    EXPECT(memcmp(message.payload, portal_id, length) == 0);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_CHECKPOINT_READY);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_WAITING_FOR_GRANT);
}

TEST("Harbor YES with an in-flight inbound frame departs on the next frame")
{
    struct ScriptContext ctx = {0};

    EstablishTestCloudSession();
    HostDeliverInboundBeforeScript(2);
    /* The single-frame gate itself still refuses this frame. */
    EXPECT_EQ(CoopNetBridge_RequestPortalTravel("to_cormoria"), COOP_CHECKPOINT_REQUEST_REJECTED);

    gSpecialVar_Result = 0xFFFF;
    CoopNetBridge_ScriptTravelToCormoria(&ctx);
    /* Held, not refused: VAR_RESULT is undecided and nothing is queued. */
    EXPECT_EQ(gSpecialVar_Result, 0xFFFF);
    EXPECT(ctx.nativePtr != NULL);
    EXPECT(ctx.waitAfterCallNative);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));

    RunTestBridgeFrame();
    EXPECT(RunScriptCommand(&ctx));
    EXPECT_EQ(gSpecialVar_Result, TRUE);
    ExpectQueuedPortal("to_cormoria", 11);
}

TEST("Harbor YES with an undrained outbound frame departs once Lua drains it")
{
    struct ScriptContext ctx = {0};
    struct CoopBridgeMessage message;

    EstablishTestCloudSession();
    EXPECT(CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_PLAYER_STATE, NULL, 0));
    gSpecialVar_Result = 0xFFFF;
    CoopNetBridge_ScriptTravelToMain(&ctx);
    EXPECT_EQ(gSpecialVar_Result, 0xFFFF);

    /* A frame where Lua has not sent yet keeps the script waiting. */
    EXPECT(RunScriptCommand(&ctx));
    EXPECT_EQ(gSpecialVar_Result, 0xFFFF);

    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_PLAYER_STATE);
    EXPECT(RunScriptCommand(&ctx));
    EXPECT_EQ(gSpecialVar_Result, TRUE);
    ExpectQueuedPortal("to_main", 7);
}

TEST("Harbor YES refuses after the bounded window and never queues late")
{
    struct ScriptContext ctx = {0};
    u32 frames;

    EstablishTestCloudSession();
    HostDeliverInboundBeforeScript(2);
    gSpecialVar_Result = 0xFFFF;
    CoopNetBridge_ScriptTravelToCormoria(&ctx);
    /* The inbound frame is never drained: the voyage is refused after the
     * window with the existing "can't depart" path, never left hanging. */
    for (frames = 1; gSpecialVar_Result == 0xFFFF && frames < 1000; frames++)
        EXPECT(RunScriptCommand(&ctx));
    EXPECT_EQ(frames, COOP_NET_BRIDGE_PORTAL_REQUEST_FRAMES);
    EXPECT_EQ(gSpecialVar_Result, FALSE);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);

    /* A later YES starts a fresh window and departs once traffic clears. */
    gSpecialVar_Result = 0xFFFF;
    CoopNetBridge_ScriptTravelToCormoria(&ctx);
    EXPECT_EQ(gSpecialVar_Result, 0xFFFF);
    RunTestBridgeFrame();
    EXPECT(RunScriptCommand(&ctx));
    EXPECT_EQ(gSpecialVar_Result, TRUE);
    ExpectQueuedPortal("to_cormoria", 11);
}

TEST("Harbor YES stops retrying when cloud mode is lost")
{
    struct ScriptContext ctx = {0};

    EstablishTestCloudSession();
    HostDeliverInboundBeforeScript(2);
    gSpecialVar_Result = 0xFFFF;
    CoopNetBridge_ScriptTravelToCormoria(&ctx);
    EXPECT_EQ(gSpecialVar_Result, 0xFFFF);
    InitTestBridge();
    EXPECT(RunScriptCommand(&ctx));
    EXPECT_EQ(gSpecialVar_Result, FALSE);
}

/* The test bridge has no overworld. The first PlayerState publication after
 * an empty-queue Poll therefore marks the world not ready, and that guard
 * refuses every checkpoint. Prove the guard, then clear it the way a
 * PlayerState published from a ready field does. */
static void ExpectWorldNotReadyThenMarkReady(void)
{
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_WORLD_NOT_READY);
    EXPECT_EQ(CoopNetBridge_RequestPortalTravel("to_cormoria"), COOP_CHECKPOINT_REQUEST_REJECTED);
    EXPECT_EQ(CoopNetBridge_RequestCheckpoint(), COOP_CHECKPOINT_REQUEST_REJECTED);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    gCoopNetBridge.status_flags &= ~COOP_BRIDGE_STATUS_WORLD_NOT_READY;
}

TEST("Grouped cloud sessions enqueue region portals and ordinary checkpoints")
{
    struct CoopBridgeMessage message;

    EstablishTestCloudSession();
    DeliverTestOnlineStatus(1, 2, COOP_ONLINE_GROUPED);
    EXPECT(CoopNetBridge_IsGrouped());
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    ExpectWorldNotReadyThenMarkReady();

    CoopNetBridge_ScriptPortalAvailable();
    EXPECT_EQ(gSpecialVar_Result, TRUE);
    EXPECT_EQ(CoopNetBridge_RequestPortalTravel("to_cormoria"), COOP_CHECKPOINT_REQUEST_STARTED);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_PORTAL_TRAVEL_REQUEST);
    EXPECT_EQ(message.length, 11);
    EXPECT(memcmp(message.payload, "to_cormoria", 11) == 0);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_CHECKPOINT_READY);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));

    EstablishTestCloudSession();
    DeliverTestOnlineStatus(1, 2, COOP_ONLINE_GROUPED);
    ExpectWorldNotReadyThenMarkReady();
    EXPECT_EQ(CoopNetBridge_RequestCheckpoint(), COOP_CHECKPOINT_REQUEST_STARTED);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_CHECKPOINT_READY);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
}

TEST("Cloud Coop portal request rejects overlong ID without publishing intent")
{
    char overlong_id[98];
    u32 i;

    EstablishTestCloudSession();
    for (i = 0; i < 97; i++)
        overlong_id[i] = 'a';
    overlong_id[97] = '\0';
    EXPECT_EQ(CoopNetBridge_RequestPortalTravel(overlong_id), COOP_CHECKPOINT_REQUEST_REJECTED);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
}

TEST("Cloud Coop checkpoint grant accepts only a fresh empty current epoch")
{
    struct CoopBridgeMessage message;

    EstablishTestCloudSession();
    StartTestCheckpoint();

    /* The session-ready sequence is stale, and a different epoch is never a
     * grant for the pending request. Neither may authorize a flash write. */
    DeliverTestGrant(1, 17, 0);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_WAITING_FOR_GRANT);
    DeliverTestGrant(2, 99, 0);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_WAITING_FOR_GRANT);

    /* A nonempty current-epoch grant is malformed and is not freshened. */
    DeliverTestGrant(2, 17, 1);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_WAITING_FOR_GRANT);
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR);

    DeliverTestGrant(2, 17, 0);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_GRANTED);
    EXPECT(!CoopNetBridge_IsCheckpointAuthorizedForSave());
    EXPECT(CoopNetBridge_ConsumeCheckpointGrant());
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_SAVING);
    EXPECT(CoopNetBridge_IsCheckpointAuthorizedForSave());

    /* A second consume cannot cause the normal save callback to run twice. */
    EXPECT(!CoopNetBridge_ConsumeCheckpointGrant());
    (void)message;
}

TEST("Cloud Coop checkpoint timeout never enters the save state")
{
    struct CoopBridgeMessage message;
    u32 frame;

    EstablishTestCloudSession();
    StartTestCheckpoint();
    for (frame = 0; frame < COOP_NET_BRIDGE_CHECKPOINT_TIMEOUT_FRAMES; frame++)
    {
        gCoopNetBridge.last_sidecar_heartbeat++;
        CoopNetBridge_Poll();
    }

    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    EXPECT(CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_PLAYER_STATE, NULL, 0));
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_PLAYER_STATE);
}

TEST("Cloud Coop epoch and heartbeat changes cancel a pending checkpoint")
{
    struct CoopBridgeMessage message;
    u32 frame;

    EstablishTestCloudSession();
    StartTestCheckpoint();
    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  2,
                                  18,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);

    EstablishTestCloudSession();
    StartTestCheckpoint();
    for (frame = 0; frame < COOP_NET_BRIDGE_SIDECAR_STALE_INTERVAL; frame++)
        CoopNetBridge_Poll();
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_OFFLINE);
    EXPECT(!CoopNetBridge_IsCheckpointAuthorizedForSave());
    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SESSION_READY));
    EXPECT(CoopNetBridge_RequestCheckpoint() == COOP_CHECKPOINT_REQUEST_REJECTED);
}

TEST("Cloud Coop successful save emits one generation update and failure emits none")
{
    struct CoopBridgeMessage message;

    EstablishTestCloudSession();
    StartTestCheckpoint();
    DeliverTestGrant(2, 17, 0);
    EXPECT(CoopNetBridge_ConsumeCheckpointGrant());
    CoopNetBridge_NotifySaveResult(FALSE);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));

    StartTestCheckpoint();
    DeliverTestGrant(3, 17, 0);
    EXPECT(CoopNetBridge_ConsumeCheckpointGrant());
    CoopNetBridge_NotifySaveResult(TRUE);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_SAVING);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_SAVE_DATA_UPDATED);
    EXPECT_EQ(message.length, sizeof(u32));
    EXPECT_EQ(message.payload[0], 0);
    EXPECT_EQ(message.payload[1], 0);
    EXPECT_EQ(message.payload[2], 0);
    EXPECT_EQ(message.payload[3], 0);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    CoopNetBridge_NotifySaveResult(TRUE);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
}

TEST("Cloud Coop retries a full critical save update without consuming sequence")
{
    struct CoopBridgeMessage message;
    u16 sequenceBefore;
    u32 i;

    EstablishTestCloudSession();
    StartTestCheckpoint();
    DeliverTestGrant(2, 17, 0);
    EXPECT(CoopNetBridge_ConsumeCheckpointGrant());
    for (i = 0; i < COOP_NET_BRIDGE_QUEUE_CAPACITY; i++)
        EXPECT(CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_ROM_READY, NULL, 0));

    sequenceBefore = gCoopNetBridge.game_to_network.entries[
        (gCoopNetBridge.game_to_network.write_index - 1)
        & (COOP_NET_BRIDGE_QUEUE_CAPACITY - 1)].sequence;
    CoopNetBridge_NotifySaveResult(TRUE);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_SAVING);
    EXPECT(CoopBridgeQueue_IsFull(&gCoopNetBridge.game_to_network));
    EXPECT_EQ(gCoopNetBridge.game_to_network.entries[
        (gCoopNetBridge.game_to_network.write_index - 1)
        & (COOP_NET_BRIDGE_QUEUE_CAPACITY - 1)].sequence, sequenceBefore);

    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    CoopNetBridge_Poll();
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_SAVING);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    while (!CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network))
    {
        EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
        if (message.type == COOP_BRIDGE_MESSAGE_SAVE_DATA_UPDATED)
            break;
    }
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_SAVE_DATA_UPDATED);
    EXPECT_EQ(message.length, sizeof(u32));
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
}

TEST("Cloud Coop retains a pending update across same-epoch heartbeat recovery")
{
    struct CoopBridgeMessage message;
    u32 frame;
    u32 updates = 0;

    EstablishTestCloudSession();
    StartTestCheckpoint();
    DeliverTestGrant(2, 17, 0);
    EXPECT(CoopNetBridge_ConsumeCheckpointGrant());
    for (frame = 0; frame < COOP_NET_BRIDGE_QUEUE_CAPACITY; frame++)
        EXPECT(CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_ROM_READY, NULL, 0));
    CoopNetBridge_NotifySaveResult(TRUE);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_SAVING);

    for (frame = 0; frame < COOP_NET_BRIDGE_SIDECAR_STALE_INTERVAL; frame++)
        CoopNetBridge_Poll();
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_SAVING);
    EXPECT(!CoopNetBridge_IsRecoveryRequired());
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));

    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  3,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_SAVING);

    while (!CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network))
    {
        EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
        if (message.type == COOP_BRIDGE_MESSAGE_SAVE_DATA_UPDATED)
        {
            updates++;
            EXPECT_EQ(message.session_epoch, 17);
            EXPECT_EQ(message.length, sizeof(u32));
        }
    }
    EXPECT_EQ(updates, 1);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
}

TEST("Cloud Coop preserves an enqueued undrained update across same-epoch heartbeat recovery")
{
    struct CoopBridgeMessage message;
    u32 frame;
    u32 updates = 0;

    EstablishTestCloudSession();
    StartTestCheckpoint();
    DeliverTestGrant(2, 17, 0);
    EXPECT(CoopNetBridge_ConsumeCheckpointGrant());
    CoopNetBridge_NotifySaveResult(TRUE);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_SAVING);

    /* The update was accepted into the FIFO, but the sidecar has not
     * advanced read_index yet. A stale reset must make it retryable. */
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.network_to_game));
    for (frame = 0; frame < COOP_NET_BRIDGE_SIDECAR_STALE_INTERVAL; frame++)
        CoopNetBridge_Poll();
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_SAVING);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));

    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  3,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_SAVING);

    while (!CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network))
    {
        EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
        if (message.type == COOP_BRIDGE_MESSAGE_SAVE_DATA_UPDATED)
        {
            updates++;
            EXPECT_EQ(message.session_epoch, 17);
            EXPECT_EQ(message.length, sizeof(u32));
        }
    }
    EXPECT_EQ(updates, 1);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
}

TEST("Cloud Coop does not duplicate a drained update after heartbeat recovery")
{
    struct CoopBridgeMessage message;
    u32 frame;
    u32 updates = 0;

    EstablishTestCloudSession();
    StartTestCheckpoint();
    DeliverTestGrant(2, 17, 0);
    EXPECT(CoopNetBridge_ConsumeCheckpointGrant());
    CoopNetBridge_NotifySaveResult(TRUE);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_SAVE_DATA_UPDATED);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);

    for (frame = 0; frame < COOP_NET_BRIDGE_SIDECAR_STALE_INTERVAL; frame++)
        CoopNetBridge_Poll();
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_OFFLINE);

    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  3,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    while (!CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network))
    {
        EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
        if (message.type == COOP_BRIDGE_MESSAGE_SAVE_DATA_UPDATED)
            updates++;
    }
    EXPECT_EQ(updates, 0);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
}

TEST("Cloud Coop malformed consumer index cannot acknowledge a critical update")
{
    EstablishTestCloudSession();
    StartTestCheckpoint();
    DeliverTestGrant(2, 17, 0);
    EXPECT(CoopNetBridge_ConsumeCheckpointGrant());
    CoopNetBridge_NotifySaveResult(TRUE);

    /* The sidecar owns read_index. An impossible depth must not be treated as
     * proof that the queued SAVE_DATA_UPDATED was consumed. */
    gCoopNetBridge.game_to_network.read_index =
        gCoopNetBridge.game_to_network.write_index
        + COOP_NET_BRIDGE_QUEUE_CAPACITY + 1;
    CoopNetBridge_Poll();

    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_QUEUE_ERROR);
    EXPECT(CoopNetBridge_IsRecoveryRequired());
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_RECOVERY_REQUIRED);
    EXPECT_EQ(CoopNetBridge_RequestCheckpoint(), COOP_CHECKPOINT_REQUEST_REJECTED);
}

TEST("Cloud Coop malformed consumer index without a critical update stays idle")
{
    EstablishTestCloudSession();
    gCoopNetBridge.game_to_network.read_index =
        gCoopNetBridge.game_to_network.write_index
        + COOP_NET_BRIDGE_QUEUE_CAPACITY + 1;
    CoopNetBridge_Poll();

    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_QUEUE_ERROR));
    EXPECT(!CoopNetBridge_IsRecoveryRequired());
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
}

TEST("Cloud Coop enters explicit recovery when a pending update crosses epochs")
{
    struct CoopBridgeMessage message;
    u32 frame;

    EstablishTestCloudSession();
    StartTestCheckpoint();
    DeliverTestGrant(2, 17, 0);
    EXPECT(CoopNetBridge_ConsumeCheckpointGrant());
    for (frame = 0; frame < COOP_NET_BRIDGE_QUEUE_CAPACITY; frame++)
        EXPECT(CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_ROM_READY, NULL, 0));
    CoopNetBridge_NotifySaveResult(TRUE);

    for (frame = 0; frame < COOP_NET_BRIDGE_SIDECAR_STALE_INTERVAL; frame++)
        CoopNetBridge_Poll();
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_SAVING);

    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  3,
                                  18,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_RECOVERY_REQUIRED);
    EXPECT(CoopNetBridge_IsRecoveryRequired());
    EXPECT_EQ(CoopNetBridge_RequestCheckpoint(), COOP_CHECKPOINT_REQUEST_REJECTED);
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
}

TEST("Cloud Coop production save callback waits for grant after confirmation")
{
    EstablishTestCloudSession();
    CoopStartMenu_TestSetSaveDryRun(TRUE);
    CoopStartMenu_TestSetCheckpointRequired(TRUE);

    /* SaveDoSaveCallback is the first production callback after the existing
     * Yes/No and overwrite prompts. It must wait instead of reaching flash. */
    EXPECT_EQ(CoopStartMenu_TestRunSaveSavingMessageCallback(), COOP_START_MENU_TEST_SAVE_IN_PROGRESS);
    EXPECT_EQ(CoopStartMenu_TestRunSaveDoSaveCallback(), COOP_START_MENU_TEST_SAVE_IN_PROGRESS);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_WAITING_FOR_GRANT);

    {
        struct CoopBridgeMessage message;
        EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
        EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_CHECKPOINT_READY);
    }

    DeliverTestGrant(2, 17, 0);
    EXPECT_EQ(CoopStartMenu_TestRunCheckpointWaitCallback(), COOP_START_MENU_TEST_SAVE_SUCCESS);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_SAVING);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    CoopStartMenu_TestSetSaveDryRun(FALSE);
}

TEST("Cloud Coop rejected production save callback returns through interactive recovery")
{
    u32 i;

    EstablishTestCloudSession();
    for (i = 0; i < COOP_NET_BRIDGE_QUEUE_CAPACITY; i++)
        EXPECT(CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_ROM_READY, NULL, 0));

    CoopStartMenu_TestSetSaveDryRun(TRUE);
    CoopStartMenu_TestSetCheckpointRequired(TRUE);
    EXPECT_EQ(CoopStartMenu_TestRunSaveSavingMessageCallback(), COOP_START_MENU_TEST_SAVE_IN_PROGRESS);
    EXPECT_EQ(CoopStartMenu_TestRunSaveDoSaveCallback(), COOP_START_MENU_TEST_SAVE_IN_PROGRESS);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
    EXPECT_EQ(CoopStartMenu_TestRunCheckpointAbortCallback(), COOP_START_MENU_TEST_SAVE_CANCELED);
    CoopStartMenu_TestSetSaveDryRun(FALSE);
}

TEST("Cloud Coop production save callback fails closed before TrySavingData on auth loss")
{
    struct CoopBridgeMessage message;
    u32 frame;

    EstablishTestCloudSession();
    CoopStartMenu_TestSetSaveDryRun(TRUE);
    CoopStartMenu_TestSetCheckpointRequired(TRUE);
    EXPECT_EQ(CoopStartMenu_TestRunSaveSavingMessageCallback(), COOP_START_MENU_TEST_SAVE_IN_PROGRESS);
    EXPECT_EQ(CoopStartMenu_TestRunSaveDoSaveCallback(), COOP_START_MENU_TEST_SAVE_IN_PROGRESS);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_CHECKPOINT_READY);
    DeliverTestGrant(2, 17, 0);
    EXPECT(CoopNetBridge_ConsumeCheckpointGrant());

    for (frame = 0; frame < COOP_NET_BRIDGE_SIDECAR_STALE_INTERVAL; frame++)
        CoopNetBridge_Poll();
    EXPECT(!CoopNetBridge_IsCheckpointAuthorizedForSave());
    EXPECT_EQ(CoopStartMenu_TestRunAuthorizedSaveCallback(), COOP_START_MENU_TEST_SAVE_IN_PROGRESS);
    EXPECT_EQ(CoopStartMenu_TestRunCheckpointAbortCallback(), COOP_START_MENU_TEST_SAVE_CANCELED);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    CoopNetBridge_NotifySaveResult(FALSE);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
    CoopStartMenu_TestSetSaveDryRun(FALSE);
    (void)message;
}

TEST("Cloud Coop SaveFailedScreen retries cannot bypass a revoked checkpoint")
{
    u16 (*programFlashSector)(u16, u8 *) = ProgramFlashSector;
    bool32 flashMemoryPresent = gFlashMemoryPresent;

    EstablishTestCloudSession();
    StartTestCheckpoint();
    DeliverTestGrant(2, 17, 0);
    EXPECT(CoopNetBridge_ConsumeCheckpointGrant());

    /* Simulate a record becoming invalid after negotiation and after the
     * grant was consumed. The cloud epoch remains sticky, but authorization
     * is revoked and the normal callback reports the failed save. */
    gSaveBlock3Ptr->coop.trainer_bits[COOP_SAVE_TRAINER_BITS_SIZE - 1] = 0x80;
    EXPECT(!CoopSave_Seal(&gSaveBlock3Ptr->coop));
    EXPECT(CoopNetBridge_IsCloudMode());
    CoopNetBridge_NotifySaveResult(FALSE);

    sSaveSectorProgramCalls = 0;
    ProgramFlashSector = CountSaveSectorProgramCalls;
    gFlashMemoryPresent = TRUE;
    HandleSavingData(SAVE_NORMAL);
    EXPECT_EQ(sSaveSectorProgramCalls, 0);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);

    ProgramFlashSector = programFlashSector;
    gFlashMemoryPresent = flashMemoryPresent;
}

TEST("Cloud Coop forced save callback bypasses checkpoint negotiation")
{
    EstablishTestCloudSession();
    CoopStartMenu_TestSetSaveDryRun(TRUE);
    CoopStartMenu_TestSetCheckpointRequired(FALSE);

    EXPECT_EQ(CoopStartMenu_TestRunSaveSavingMessageCallback(), COOP_START_MENU_TEST_SAVE_IN_PROGRESS);
    EXPECT_EQ(CoopStartMenu_TestRunAuthorizedSaveCallback(), COOP_START_MENU_TEST_SAVE_SUCCESS);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    CoopStartMenu_TestSetSaveDryRun(FALSE);
}
