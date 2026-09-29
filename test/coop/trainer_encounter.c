#include "global.h"
#include "battle.h"
#include "battle_setup.h"
#include "battle_util.h"
#include "coop/battle_consent.h"
#include "coop/battle_runtime.h"
#include "coop/generated_regional_identities.h"
#include "coop/net_bridge.h"
#include "coop/region.h"
#include "coop/save.h"
#include "coop/trainer_rewards.h"
#include "constants/battle_setup.h"
#include "constants/event_objects.h"
#include "constants/layouts.h"
#include "constants/opponents.h"
#include "constants/region_map_sections.h"
#include "constants/trainers.h"
#include "event_data.h"
#include "event_object_lock.h"
#include "event_object_movement.h"
#include "field_message_box.h"
#include "field_player_avatar.h"
#include "fieldmap.h"
#include "main.h"
#include "malloc.h"
#include "money.h"
#include "overworld.h"
#include "palette.h"
#include "pokemon.h"
#include "script.h"
#include "sprite.h"
#include "string_util.h"
#include "task.h"
#include "trainer_see.h"
#include "test/test.h"

/* A parked dotrainerbattle, a grouped cloud session and a partner forced
 * nearby. Presence-based nearness is covered in test/coop/presence_runtime.c;
 * here the partner check is pinned so every other rule can be isolated. */

#define ENCOUNTER_TRAINER TRAINER_CALVIN_1
#define ENCOUNTER_EPOCH 17

_Static_assert(COOP_TRAINER_ENCOUNTER_OFFER_FRAMES == 10 * 60,
               "the partner prompt declines itself after ten seconds");
_Static_assert(COOP_TRAINER_ENCOUNTER_WAIT_FRAMES > COOP_TRAINER_ENCOUNTER_OFFER_FRAMES,
               "the requester outlasts the partner prompt");
_Static_assert(COOP_TRAINER_ENCOUNTER_PARTNER_TILES == 12,
               "partners count as nearby within twelve tiles");

static const u8 sExpectedWaitingText[] = _("Waiting for partner…");

struct EncounterFixture
{
    struct MapHeader map_header;
    TrainerBattleParameter params;
    u8 approaching;
    MainCallback callback1;
    MainCallback callback2;
    MainCallback saved_callback;
    bool8 fade_active;
    bool8 controls_locked;
    u32 frame;
    u32 battle_type_flags;
    u16 partner_trainer_id;
    bool8 tasks[NUM_TASKS];
    struct ObjectEvent object_events[OBJECT_EVENTS_COUNT];
    struct Sprite sprite0;
    struct PlayerAvatar player_avatar;
    struct Pokemon parties[2][PARTY_SIZE];
    u8 party_counts[2];
};

static struct EncounterFixture *sFixture;
static u32 sSequence;

static void DeliverRecord(u16 type, const void *payload, u16 length)
{
    struct CoopBridgeMessage message;

    EXPECT(CoopBridgeMessage_Seal(&message, type, sSequence++, ENCOUNTER_EPOCH,
                                  payload, length));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
}

static void DrainOutbound(void)
{
    struct CoopBridgeMessage message;

    while (CoopNetBridge_DequeueGameToNetwork(&message))
        ;
}

static void SetGrouped(bool8 grouped)
{
    u8 payload[2] = {grouped, 0};

    DeliverRecord(COOP_BRIDGE_MESSAGE_GROUP_STATE_CHANGED, payload, sizeof(payload));
}

static void EstablishGroupedSession(void)
{
    struct CoopBridgeMessage message;

    CoopSave_InitializeCurrent();
    CoopNetBridge_Init();
    DrainOutbound();
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  1, ENCOUNTER_EPOCH, NULL, 0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    DrainOutbound();
    sSequence = 2;
    SetGrouped(TRUE);
    EXPECT(CoopNetBridge_IsGrouped());
    EXPECT(CoopNetBridge_CanSendBattle());
}

static void SetTrainer(u16 trainerId, u8 mode)
{
    InitTrainerBattleParameter();
    TRAINER_BATTLE_PARAM.mode = mode;
    TRAINER_BATTLE_PARAM.opponentA = trainerId;
    gNoOfApproachingTrainers = 1;
}

/* The trainer script has just shown its intro and runs dotrainerbattle. */
static void ParkTrainerScript(void)
{
    gMain.callback1 = CB1_Overworld;
    gMain.callback2 = CB2_Overworld;
    gPaletteFade.active = FALSE;
    ScriptContext_Init();
    if (!ArePlayerFieldControlsLocked())
        LockPlayerFieldControls();
}

static void BeginEncounterFixture(void)
{
    u32 i;

    sFixture = AllocZeroed(sizeof(*sFixture));
    EXPECT(sFixture != NULL);

    sFixture->map_header = gMapHeader;
    sFixture->params = gTrainerBattleParameter;
    sFixture->approaching = gNoOfApproachingTrainers;
    sFixture->callback1 = gMain.callback1;
    sFixture->callback2 = gMain.callback2;
    sFixture->saved_callback = gMain.savedCallback;
    sFixture->fade_active = gPaletteFade.active;
    sFixture->controls_locked = ArePlayerFieldControlsLocked();
    sFixture->frame = gMain.vblankCounter1;
    sFixture->battle_type_flags = gBattleTypeFlags;
    sFixture->partner_trainer_id = gPartnerTrainerId;
    for (i = 0; i < NUM_TASKS; i++)
        sFixture->tasks[i] = gTasks[i].isActive;
    for (i = 0; i < OBJECT_EVENTS_COUNT; i++)
        sFixture->object_events[i] = gObjectEvents[i];
    sFixture->sprite0 = gSprites[0];
    sFixture->player_avatar = gPlayerAvatar;
    memcpy(sFixture->parties[0], gParties[B_TRAINER_0], sizeof(sFixture->parties[0]));
    memcpy(sFixture->parties[1], gParties[B_TRAINER_2], sizeof(sFixture->parties[1]));
    sFixture->party_counts[0] = gPartiesCount[B_TRAINER_0];
    sFixture->party_counts[1] = gPartiesCount[B_TRAINER_2];

    /* releaseall and the battle start need a bound player object. */
    for (i = 0; i < OBJECT_EVENTS_COUNT; i++)
        memset(&gObjectEvents[i], 0, sizeof(gObjectEvents[i]));
    memset(&gSprites[0], 0, sizeof(gSprites[0]));
    gObjectEvents[0].active = TRUE;
    gObjectEvents[0].isPlayer = TRUE;
    gObjectEvents[0].localId = LOCALID_PLAYER;
    gObjectEvents[0].spriteId = 0;
    gSprites[0].inUse = TRUE;
    gPlayerAvatar.objectEventId = 0;
    gPlayerAvatar.spriteId = 0;
    gPlayerAvatar.flags = PLAYER_AVATAR_FLAG_ON_FOOT;

    gMapHeader.engineRegion = COOP_MAP_ENGINE_REGION_HOENN;
    gMapHeader.regionMapSectionId = MAPSEC_ROUTE_102;
    gMapHeader.mapLayoutId = LAYOUT_ROUTE102;
    EstablishGroupedSession();
    CoopTrainerEncounter_TestSetPartnerNearby(TRUE);
    SetTrainer(ENCOUNTER_TRAINER, TRAINER_BATTLE_SINGLE);
    ParkTrainerScript();
}

static void EndEncounterFixture(void)
{
    u32 i;

    for (i = 0; i < NUM_TASKS; i++)
        if (gTasks[i].isActive && !sFixture->tasks[i])
            DestroyTask(i);
    StopFieldMessage();
    HideFieldMessageBox();
    CoopTrainerEncounter_TestSetPartnerNearby(-1);
    CoopBattleRuntime_DisarmEngine();
    CoopNetBridge_Init();
    ScriptContext_Init();
    FlagClear(FLAG_SAFE_FOLLOWER_MOVEMENT);
    gMapHeader = sFixture->map_header;
    gTrainerBattleParameter = sFixture->params;
    gNoOfApproachingTrainers = sFixture->approaching;
    gMain.callback1 = sFixture->callback1;
    gMain.callback2 = sFixture->callback2;
    gMain.savedCallback = sFixture->saved_callback;
    gPaletteFade.active = sFixture->fade_active;
    gMain.vblankCounter1 = sFixture->frame;
    gBattleTypeFlags = sFixture->battle_type_flags;
    gPartnerTrainerId = sFixture->partner_trainer_id;
    for (i = 0; i < OBJECT_EVENTS_COUNT; i++)
        gObjectEvents[i] = sFixture->object_events[i];
    gSprites[0] = sFixture->sprite0;
    gPlayerAvatar = sFixture->player_avatar;
    memcpy(gParties[B_TRAINER_0], sFixture->parties[0], sizeof(sFixture->parties[0]));
    memcpy(gParties[B_TRAINER_2], sFixture->parties[1], sizeof(sFixture->parties[1]));
    gPartiesCount[B_TRAINER_0] = sFixture->party_counts[0];
    gPartiesCount[B_TRAINER_2] = sFixture->party_counts[1];
    if (sFixture->controls_locked)
        LockPlayerFieldControls();
    else
        UnlockPlayerFieldControls();
    Free(sFixture);
    sFixture = NULL;
}

/* Runs dotrainerbattle's entry and returns the reservation nonce. */
static u32 BeginParkedEncounter(void)
{
    struct CoopBridgeMessage message;
    u32 nonce;

    BattleSetup_StartTrainerBattle();
    EXPECT(CoopTrainerEncounter_TestIsPending());
    EXPECT(!ScriptContext_IsEnabled());
    EXPECT(ArePlayerFieldControlsLocked());
    EXPECT_EQ(gNoOfApproachingTrainers, 1);
    EXPECT_EQ(GetFieldMessageBoxMode(), FIELD_MESSAGE_BOX_NORMAL);
    EXPECT_EQ(StringCompare(gStringVar4, sExpectedWaitingText), 0);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_TRAINER_BATTLE_RESERVE);
    EXPECT_EQ(message.payload[0], COOP_BATTLE_KIND_COOPERATIVE_TRAINER);
    EXPECT_EQ(message.payload[5], COOP_REGION_HOENN);
    EXPECT_EQ(message.payload[6] | ((u16)message.payload[7] << 8),
              COOP_TRAINER_HOENN_TRAINER_CALVIN_1_ORDINAL);
    nonce = message.payload[1] | ((u32)message.payload[2] << 8)
        | ((u32)message.payload[3] << 16) | ((u32)message.payload[4] << 24);
    EXPECT_NE(nonce, 0);
    return nonce;
}

static void DeliverRequesterOffer(u8 id, u32 nonce)
{
    u8 offer[COOP_BATTLE_JOIN_OFFER_SIZE] = {0};

    offer[0] = id;
    offer[16] = COOP_BATTLE_KIND_COOPERATIVE_TRAINER;
    offer[17] = 0;
    offer[18] = nonce;
    offer[19] = nonce >> 8;
    offer[20] = nonce >> 16;
    offer[21] = nonce >> 24;
    DeliverRecord(COOP_BRIDGE_MESSAGE_BATTLE_JOIN_OFFER, offer, sizeof(offer));
}

static void DeliverOutcome(u8 id, u32 nonce, u8 outcome)
{
    u8 payload[COOP_BATTLE_CONSENT_OUTCOME_SIZE] = {0};

    payload[0] = id;
    payload[16] = nonce;
    payload[17] = nonce >> 8;
    payload[18] = nonce >> 16;
    payload[19] = nonce >> 24;
    payload[20] = outcome;
    DeliverRecord(COOP_BRIDGE_MESSAGE_BATTLE_CONSENT_OUTCOME, payload, sizeof(payload));
}

static void DeliverRejected(u32 nonce)
{
    u8 payload[COOP_BATTLE_RESERVE_REJECTED_SIZE];

    payload[0] = nonce;
    payload[1] = nonce >> 8;
    payload[2] = nonce >> 16;
    payload[3] = nonce >> 24;
    DeliverRecord(COOP_BRIDGE_MESSAGE_BATTLE_RESERVE_REJECTED, payload, sizeof(payload));
}

static bool8 TakeOutbound(u16 type, struct CoopBridgeMessage *out)
{
    struct CoopBridgeMessage message;
    bool8 found = FALSE;

    memset(out, 0, sizeof(*out));
    while (CoopNetBridge_DequeueGameToNetwork(&message))
    {
        if (message.type == type && !found)
        {
            *out = message;
            found = TRUE;
        }
    }
    return found;
}

/* VBlank advances gMain.vblankCounter1 at any instruction. Deadline tests
 * mask interrupts so the counter moves only when the test sets it. */
static u16 FreezeFrameCounter(void)
{
    u16 ime = REG_IME;

    REG_IME = 0;
    return ime;
}

static void ThawFrameCounter(u16 ime)
{
    REG_IME = ime;
}

/* The deferred vanilla start ran with the parameters the script left. */
static void ExpectVanillaStarted(const TrainerBattleParameter *before)
{
    EXPECT(!CoopTrainerEncounter_TestIsPending());
    EXPECT_EQ(memcmp(&gTrainerBattleParameter, before, sizeof(*before)), 0);
    EXPECT_EQ(gNoOfApproachingTrainers, 0);
    EXPECT_EQ(gBattleTypeFlags, BATTLE_TYPE_TRAINER);
    EXPECT(!CoopBattleRuntime_IsEngineActive());
    EXPECT(IsFieldMessageBoxHidden());
    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR));
}

static bool8 IsEligibleAs(u16 trainerId, u8 mode)
{
    bool8 eligible;

    SetTrainer(trainerId, mode);
    eligible = CoopTrainerEncounter_IsEligible(trainerId);
    SetTrainer(ENCOUNTER_TRAINER, TRAINER_BATTLE_SINGLE);
    return eligible;
}

TEST("Cloud Coop trainer encounter eligibility requires every condition")
{
    static const u8 sExcludedModes[] = {
        TRAINER_BATTLE_CONTINUE_SCRIPT_NO_MUSIC,
        TRAINER_BATTLE_CONTINUE_SCRIPT,
        TRAINER_BATTLE_REMATCH,
        TRAINER_BATTLE_CONTINUE_SCRIPT_DOUBLE,
        TRAINER_BATTLE_REMATCH_DOUBLE,
        TRAINER_BATTLE_CONTINUE_SCRIPT_DOUBLE_NO_MUSIC,
        TRAINER_BATTLE_TWO_TRAINERS_NO_INTRO,
        TRAINER_BATTLE_EARLY_RIVAL,
    };
    /* The test image replaces gTrainers with the battle-test table, so the
     * class rule is checked on the classes the real trainers use. */
    static const u8 sStoryClasses[] = {
        TRAINER_CLASS_LEADER,        // Roxanne ... Juan
        TRAINER_CLASS_RIVAL,         // Brendan, May and Wally
        TRAINER_CLASS_ELITE_FOUR,
        TRAINER_CLASS_CHAMPION,
        TRAINER_CLASS_AQUA_ADMIN,
        TRAINER_CLASS_MAGMA_ADMIN,
        TRAINER_CLASS_AQUA_LEADER,
        TRAINER_CLASS_MAGMA_LEADER,
        TRAINER_CLASS_PYRAMID_KING,
        TRAINER_CLASS_LEADER_FRLG,
        TRAINER_CLASS_ELITE_FOUR_FRLG,
        TRAINER_CLASS_CHAMPION_FRLG,
        TRAINER_CLASS_RIVAL_EARLY_FRLG,
        TRAINER_CLASS_RIVAL_LATE_FRLG,
        TRAINER_CLASS_BOSS_FRLG,
        TRAINER_CLASS_ROCKET_ADMIN,
    };
    static const u8 sRouteClasses[] = {
        TRAINER_CLASS_YOUNGSTER,
        TRAINER_CLASS_HIKER,
        TRAINER_CLASS_LASS,
        TRAINER_CLASS_TEAM_AQUA,
        TRAINER_CLASS_BUG_CATCHER_FRLG,
    };
    struct CoopBridgeMessage message;
    u32 i;

    BeginEncounterFixture();
    EXPECT(CoopTrainerEncounter_IsEligible(ENCOUNTER_TRAINER));
    EXPECT(IsEligibleAs(ENCOUNTER_TRAINER, TRAINER_BATTLE_SINGLE_NO_INTRO_TEXT));
    EXPECT(IsEligibleAs(ENCOUNTER_TRAINER, TRAINER_BATTLE_DOUBLE));
    EXPECT(IsEligibleAs(TRAINER_TIANA, TRAINER_BATTLE_SINGLE));

    for (i = 0; i < ARRAY_COUNT(sExcludedModes); i++)
        EXPECT(!IsEligibleAs(ENCOUNTER_TRAINER, sExcludedModes[i]));
    for (i = 0; i < ARRAY_COUNT(sStoryClasses); i++)
        EXPECT(!CoopTrainerEncounter_IsPhaseOneClass(sStoryClasses[i]));
    for (i = 0; i < ARRAY_COUNT(sRouteClasses); i++)
        EXPECT(CoopTrainerEncounter_IsPhaseOneClass(sRouteClasses[i]));
    EXPECT(!IsEligibleAs(TRAINER_NONE, TRAINER_BATTLE_SINGLE));
    EXPECT(!IsEligibleAs(TRAINER_SECRET_BASE, TRAINER_BATTLE_SINGLE));
    /* Only the trainer the script is about to battle can be reserved. */
    EXPECT(!CoopTrainerEncounter_IsEligible(TRAINER_TIANA));

    gNoOfApproachingTrainers = 2;
    EXPECT(!CoopTrainerEncounter_IsEligible(ENCOUNTER_TRAINER));
    gNoOfApproachingTrainers = 1;

    gMapHeader.mapLayoutId = LAYOUT_BATTLE_FRONTIER_BATTLE_PYRAMID_FLOOR;
    EXPECT(!CoopTrainerEncounter_IsEligible(ENCOUNTER_TRAINER));
    gMapHeader.mapLayoutId = LAYOUT_ROUTE102;

    /* The trainer must resolve in the region the player stands in. */
    gMapHeader.engineRegion = COOP_MAP_ENGINE_REGION_KANTO;
    gMapHeader.regionMapSectionId = MAPSEC_PALLET_TOWN;
    EXPECT(!CoopTrainerEncounter_IsEligible(ENCOUNTER_TRAINER));
    gMapHeader.engineRegion = COOP_MAP_ENGINE_REGION_HOENN;
    gMapHeader.regionMapSectionId = MAPSEC_ROUTE_102;

    CoopTrainerEncounter_TestSetPartnerNearby(FALSE);
    EXPECT(!CoopTrainerEncounter_IsEligible(ENCOUNTER_TRAINER));
    CoopTrainerEncounter_TestSetPartnerNearby(TRUE);

    SetGrouped(FALSE);
    EXPECT(!CoopTrainerEncounter_IsEligible(ENCOUNTER_TRAINER));
    SetGrouped(TRUE);
    EXPECT(CoopTrainerEncounter_IsEligible(ENCOUNTER_TRAINER));

    CoopBattleConsent_OnTransportLost();
    EXPECT(!CoopTrainerEncounter_IsEligible(ENCOUNTER_TRAINER));
    CoopBattleConsent_OnSessionReady();
    EXPECT(CoopTrainerEncounter_IsEligible(ENCOUNTER_TRAINER));

    gCoopNetBridge.status_flags &= ~COOP_BRIDGE_STATUS_SESSION_READY;
    EXPECT(!CoopNetBridge_CanSendBattle());
    EXPECT(!CoopTrainerEncounter_IsEligible(ENCOUNTER_TRAINER));
    gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_SESSION_READY;
    EXPECT(CoopTrainerEncounter_IsEligible(ENCOUNTER_TRAINER));

    /* A consent already in flight (here a friendly request). */
    EXPECT(CoopBattleConsent_Begin(COOP_BATTLE_KIND_FRIENDLY));
    EXPECT(!CoopTrainerEncounter_IsEligible(ENCOUNTER_TRAINER));
    EndEncounterFixture();

    /* An unread battle record waiting for same-epoch replay. */
    BeginEncounterFixture();
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_BATTLE_ABORT_REQUEST,
                                  sSequence, ENCOUNTER_EPOCH, (const u8[COOP_BATTLE_ABORT_REQUEST_SIZE]){7, [16] = 1},
                                  COOP_BATTLE_ABORT_REQUEST_SIZE));
    EXPECT(CoopBridgeQueue_Push(&gCoopNetBridge.game_to_network, &message));
    CoopBattleRuntime_PreserveOutbound();
    EXPECT(CoopBattleRuntime_HasPendingOutboundReplay());
    EXPECT(!CoopTrainerEncounter_IsEligible(ENCOUNTER_TRAINER));
    EndEncounterFixture();

    /* An ineligible trainer battle starts the vanilla battle at once. */
    BeginEncounterFixture();
    {
        TrainerBattleParameter before;

        CoopTrainerEncounter_TestSetPartnerNearby(FALSE);
        before = gTrainerBattleParameter;
        BattleSetup_StartTrainerBattle();
        EXPECT(!TakeOutbound(COOP_BRIDGE_MESSAGE_TRAINER_BATTLE_RESERVE, &message));
        ExpectVanillaStarted(&before);
    }
    EndEncounterFixture();
}

TEST("Cloud Coop trainer encounter falls back silently when the reserve is rejected")
{
    TrainerBattleParameter before;
    u32 nonce;

    BeginEncounterFixture();
    before = gTrainerBattleParameter;
    nonce = BeginParkedEncounter();
    CoopBattleConsent_Poll();
    EXPECT(CoopTrainerEncounter_TestIsPending());
    DeliverRejected(nonce);
    CoopBattleConsent_Poll();
    ExpectVanillaStarted(&before);

    /* No "Battle request interrupted" notice once the field is free. */
    UnlockPlayerFieldControls();
    CoopBattleConsent_Poll();
    EXPECT(IsFieldMessageBoxHidden());
    /* A replayed rejection for the abandoned request is absorbed. */
    DeliverRejected(nonce);
    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR));
    EndEncounterFixture();
}

TEST("Cloud Coop trainer encounter falls back to vanilla when the partner declines")
{
    TrainerBattleParameter before;
    u32 nonce;

    BeginEncounterFixture();
    before = gTrainerBattleParameter;
    nonce = BeginParkedEncounter();
    DeliverRequesterOffer(21, nonce);
    CoopBattleConsent_Poll();
    EXPECT(CoopTrainerEncounter_TestIsPending());
    EXPECT_EQ(gNoOfApproachingTrainers, 1);
    DeliverOutcome(21, nonce, COOP_BATTLE_CONSENT_DECLINED);
    CoopBattleConsent_Poll();
    ExpectVanillaStarted(&before);
    EndEncounterFixture();
}

TEST("Cloud Coop trainer encounter falls back and cancels when the offer runs out")
{
    struct CoopBridgeMessage message;
    TrainerBattleParameter before;
    u32 start;
    u32 nonce;
    u16 ime;

    BeginEncounterFixture();
    before = gTrainerBattleParameter;
    ime = FreezeFrameCounter();
    start = gMain.vblankCounter1;
    nonce = BeginParkedEncounter();
    DeliverRequesterOffer(22, nonce);
    gMain.vblankCounter1 = start + COOP_TRAINER_ENCOUNTER_WAIT_FRAMES - 1;
    CoopBattleConsent_Poll();
    EXPECT(CoopTrainerEncounter_TestIsPending());
    gMain.vblankCounter1 = start + COOP_TRAINER_ENCOUNTER_WAIT_FRAMES;
    CoopBattleConsent_Poll();
    ThawFrameCounter(ime);
    ExpectVanillaStarted(&before);
    EXPECT(TakeOutbound(COOP_BRIDGE_MESSAGE_BATTLE_ABORT_REQUEST, &message));
    EXPECT_EQ(message.payload[0], 22);
    EXPECT_EQ(message.payload[COOP_BATTLE_ID_SIZE], COOP_BATTLE_ABORT_CANCELED);

    /* The server's own expiry arrives later and is not a protocol error. */
    DeliverOutcome(22, nonce, COOP_BATTLE_CONSENT_EXPIRED);
    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR));
    EndEncounterFixture();

    /* An offer that arrives after the requester gave up is cancelled too. */
    BeginEncounterFixture();
    before = gTrainerBattleParameter;
    ime = FreezeFrameCounter();
    start = gMain.vblankCounter1;
    nonce = BeginParkedEncounter();
    gMain.vblankCounter1 = start + COOP_TRAINER_ENCOUNTER_WAIT_FRAMES;
    CoopBattleConsent_Poll();
    ThawFrameCounter(ime);
    ExpectVanillaStarted(&before);
    DeliverRequesterOffer(23, nonce);
    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR));
    EXPECT(TakeOutbound(COOP_BRIDGE_MESSAGE_BATTLE_ABORT_REQUEST, &message));
    EXPECT_EQ(message.payload[0], 23);
    EndEncounterFixture();
}

TEST("Cloud Coop trainer encounter falls back to vanilla when transport is lost")
{
    TrainerBattleParameter before;
    u32 nonce;

    BeginEncounterFixture();
    before = gTrainerBattleParameter;
    nonce = BeginParkedEncounter();
    DeliverRequesterOffer(24, nonce);
    CoopBattleConsent_OnTransportLost();
    CoopBattleConsent_Poll();
    ExpectVanillaStarted(&before);
    /* The resumed session does not revive the abandoned request. */
    CoopBattleConsent_OnSessionReady();
    CoopBattleConsent_Poll();
    EXPECT(!CoopTrainerEncounter_TestIsPending());
    EXPECT(CoopTrainerEncounter_IsEligible(ENCOUNTER_TRAINER));
    EndEncounterFixture();
}

/* CreateMon leaves battle HP at zero; CalculateMonStats makes it usable. */
static void CreateUsableMon(struct Pokemon *mon, enum Species species, u32 personality)
{
    CreateMon(mon, species, 5, personality, OTID_STRUCT_PRESET(1));
    CalculateMonStats(mon);
}

static void ReleaseAcceptedEncounter(u8 id)
{
    u8 manifest[COOP_BATTLE_MANIFEST_SIZE] = {0};
    u8 chunk[COOP_BATTLE_PARTY_SNAPSHOT_SIZE] = {0};
    u8 start[COOP_BATTLE_START_SIZE] = {0};
    struct Pokemon peer;

    manifest[0] = id;
    manifest[COOP_BATTLE_MANIFEST_KIND_OFFSET] = COOP_BATTLE_MANIFEST_KIND_TRAINER;
    manifest[COOP_BATTLE_MANIFEST_MEMBER_SLOT_OFFSET] = 0;
    manifest[COOP_BATTLE_MANIFEST_REGION_OFFSET] = COOP_REGION_HOENN;
    manifest[COOP_BATTLE_MANIFEST_TRAINER_ORDINAL_OFFSET] =
        (u8)COOP_TRAINER_HOENN_TRAINER_CALVIN_1_ORDINAL;
    manifest[COOP_BATTLE_MANIFEST_TRAINER_ORDINAL_OFFSET + 1] =
        COOP_TRAINER_HOENN_TRAINER_CALVIN_1_ORDINAL >> 8;
    EXPECT(CoopBattleRuntime_ComputePartyDigest(gParties[B_TRAINER_0],
                                                 gPartiesCount[B_TRAINER_0],
                                                 &manifest[50], COOP_BATTLE_DIGEST_SIZE));
    EXPECT_EQ(CoopBattleRuntime_ReceiveManifest(manifest, sizeof(manifest)),
              COOP_BATTLE_INBOUND_ACCEPTED);
    CreateUsableMon(&peer, SPECIES_BULBASAUR, 2);
    chunk[0] = id;
    chunk[16] = 0;
    chunk[17] = 0;
    chunk[18] = 1;
    chunk[19] = COOP_BATTLE_PARTY_MON_SIZE;
    memcpy(chunk + 20, &peer, sizeof(peer));
    EXPECT_EQ(CoopBattleRuntime_ReceivePeerPartyChunk(chunk, sizeof(chunk)),
              COOP_BATTLE_INBOUND_ACCEPTED);
    CoopBattleRuntime_TestSetReadyForStart();
    start[0] = id;
    EXPECT_EQ(CoopBattleRuntime_ReceiveStart(start, sizeof(start)),
              COOP_BATTLE_INBOUND_ACCEPTED);
    EXPECT(CoopBattleRuntime_IsStartReleased());
}

TEST("Cloud Coop accepted trainer encounter starts co-op and an abort releases the trainer")
{
    struct CoopBridgeMessage message;
    MainCallback endBattle;
    bool8 wasFought;
    u32 money;
    u32 nonce;
    u8 i;

    BeginEncounterFixture();
    for (i = 0; i < PARTY_SIZE; i++)
        ZeroMonData(&gParties[B_TRAINER_0][i]);
    CreateUsableMon(&gParties[B_TRAINER_0][0], SPECIES_CHARMANDER, 1);
    gPartiesCount[B_TRAINER_0] = 1;
    wasFought = HasTrainerBeenFought(ENCOUNTER_TRAINER);
    money = GetMoney(&gSaveBlock1Ptr->money);

    nonce = BeginParkedEncounter();
    DeliverRequesterOffer(37, nonce);
    DeliverOutcome(37, nonce, COOP_BATTLE_CONSENT_ACCEPTED);
    CoopBattleConsent_Poll();
    EXPECT(CoopTrainerEncounter_TestIsPending());
    EXPECT_EQ(gNoOfApproachingTrainers, 1);
    ReleaseAcceptedEncounter(37);
    CoopBattleConsent_Poll();

    /* The server-released co-op battle replaced the parked vanilla start. */
    EXPECT(CoopBattleRuntime_IsEngineActive());
    EXPECT(gBattleTypeFlags & BATTLE_TYPE_MULTI);
    EXPECT(gBattleTypeFlags & BATTLE_TYPE_TRAINER);
    EXPECT_EQ(TRAINER_BATTLE_PARAM.opponentA, ENCOUNTER_TRAINER);
    EXPECT_EQ(gNoOfApproachingTrainers, 0);
    EXPECT(CoopTrainerEncounter_TestIsPending());
    EXPECT(IsFieldMessageBoxHidden());
    DrainOutbound();

    /* The battle aborts: no reward, no flag, and the field is released. */
    CoopBattleRuntime_FailEngine();
    endBattle = gMain.savedCallback;
    EXPECT(endBattle != NULL);
    endBattle();
    EXPECT(!CoopTrainerEncounter_TestIsPending());
    EXPECT(!CoopBattleRuntime_IsEngineActive());
    EXPECT_EQ(gMain.callback2, CB2_ReturnToFieldContinueScriptPlayMapMusic);
    EXPECT_EQ(HasTrainerBeenFought(ENCOUNTER_TRAINER), wasFought);
    EXPECT_EQ(GetMoney(&gSaveBlock1Ptr->money), money);
    EXPECT(!CoopTrainerRewards_TestIsArmed());
    EXPECT(TakeOutbound(COOP_BRIDGE_MESSAGE_BATTLE_ABORT_REQUEST, &message));
    EXPECT_EQ(message.payload[0], 37);
    EXPECT(!ScriptContext_IsEnabled());
    gMain.callback2 = CB2_Overworld;
    ScriptContext_Enable();
    EXPECT(!ScriptContext_RunScript()); // releaseall; end
    EXPECT(!ArePlayerFieldControlsLocked());
    EXPECT_EQ(HasTrainerBeenFought(ENCOUNTER_TRAINER), wasFought);

    /* Another trainer is not affected by the cooldown. */
    SetTrainer(TRAINER_TIANA, TRAINER_BATTLE_SINGLE);
    EXPECT(CoopTrainerEncounter_IsEligible(TRAINER_TIANA));

    /* Re-sighting the same trainer right away is a vanilla battle... */
    SetTrainer(ENCOUNTER_TRAINER, TRAINER_BATTLE_SINGLE);
    EXPECT(!CoopTrainerEncounter_IsEligible(ENCOUNTER_TRAINER));
    ParkTrainerScript();
    {
        TrainerBattleParameter before = gTrainerBattleParameter;

        BattleSetup_StartTrainerBattle();
        EXPECT(!TakeOutbound(COOP_BRIDGE_MESSAGE_TRAINER_BATTLE_RESERVE, &message));
        ExpectVanillaStarted(&before);
    }
    /* ...and the cooldown is one-shot. */
    SetTrainer(ENCOUNTER_TRAINER, TRAINER_BATTLE_SINGLE);
    EXPECT(CoopTrainerEncounter_IsEligible(ENCOUNTER_TRAINER));
    EndEncounterFixture();
}

TEST("Cloud Coop trainer encounter cooldown expires on its own")
{
    u32 nonce;
    u8 i;

    BeginEncounterFixture();
    for (i = 0; i < PARTY_SIZE; i++)
        ZeroMonData(&gParties[B_TRAINER_0][i]);
    CreateUsableMon(&gParties[B_TRAINER_0][0], SPECIES_CHARMANDER, 1);
    gPartiesCount[B_TRAINER_0] = 1;
    nonce = BeginParkedEncounter();
    DeliverRequesterOffer(38, nonce);
    DeliverOutcome(38, nonce, COOP_BATTLE_CONSENT_ACCEPTED);
    CoopBattleConsent_Poll();
    ReleaseAcceptedEncounter(38);
    CoopBattleConsent_Poll();
    EXPECT(CoopBattleRuntime_IsEngineActive());
    CoopBattleRuntime_FailEngine();
    gMain.savedCallback();
    DrainOutbound();
    SetTrainer(ENCOUNTER_TRAINER, TRAINER_BATTLE_SINGLE);
    EXPECT(!CoopTrainerEncounter_IsEligible(ENCOUNTER_TRAINER));
    gMain.vblankCounter1 += COOP_TRAINER_ENCOUNTER_COOLDOWN_FRAMES;
    gMain.callback2 = CB2_Overworld;
    EXPECT(CoopTrainerEncounter_IsEligible(ENCOUNTER_TRAINER));
    EndEncounterFixture();
}

TEST("Cloud Coop trainer encounter partner prompt declines itself after ten seconds")
{
    struct CoopBridgeMessage message;
    u8 offer[COOP_BATTLE_JOIN_OFFER_SIZE] = {0};
    u32 start;
    u16 ime;

    BeginEncounterFixture();
    /* The partner stands in the free overworld. */
    ScriptContext_Init();
    UnlockPlayerFieldControls();
    offer[0] = 41;
    offer[16] = COOP_BATTLE_KIND_COOPERATIVE_TRAINER;
    offer[17] = 1; // Responding partner.
    ime = FreezeFrameCounter();
    start = gMain.vblankCounter1;
    EXPECT(CoopBattleConsent_ReceiveOffer(offer, sizeof(offer)));
    CoopBattleConsent_Poll();
    EXPECT(ArePlayerFieldControlsLocked());
    ScriptContext_Stop(); // The Yes/No box pauses the offer script.
    gMain.vblankCounter1 = start + COOP_TRAINER_ENCOUNTER_OFFER_FRAMES - 1;
    CoopBattleConsent_Poll();
    Special_CoopBattleConsentGetOffer();
    EXPECT_EQ(gSpecialVar_Result, COOP_BATTLE_KIND_COOPERATIVE_TRAINER);
    EXPECT(!TakeOutbound(COOP_BRIDGE_MESSAGE_BATTLE_JOIN_RESPONSE, &message));

    gMain.vblankCounter1 = start + COOP_TRAINER_ENCOUNTER_OFFER_FRAMES;
    CoopBattleConsent_Poll();
    ThawFrameCounter(ime);
    EXPECT(!ArePlayerFieldControlsLocked());
    EXPECT(TakeOutbound(COOP_BRIDGE_MESSAGE_BATTLE_JOIN_RESPONSE, &message));
    EXPECT_EQ(message.payload[0], 41);
    EXPECT_EQ(message.payload[COOP_BATTLE_ID_SIZE], FALSE);
    gSpecialVar_0x8004 = TRUE;
    Special_CoopBattleConsentRespond();
    EXPECT_EQ(gSpecialVar_Result, FALSE);
    /* A replay of the declined offer is recognised, not re-prompted. */
    EXPECT(CoopBattleConsent_ReceiveOffer(offer, sizeof(offer)));
    Special_CoopBattleConsentGetOffer();
    EXPECT_EQ(gSpecialVar_Result, 0);
    EndEncounterFixture();
}

/* Battle-time state a completed co-op trainer battle leaves behind. */
struct RewardBattleState
{
    struct Pokemon enemy[PARTY_SIZE];
    u32 money;
    bool8 fought;
    u8 outcome;
    u8 battlers;
    u16 party_indexes[MAX_BATTLERS_COUNT];
    u8 positions[MAX_BATTLERS_COUNT];
    u8 absent;
};

/* Party slot 0 is fainted and stays home; slot 1 fights and slot 2 is staged
 * on the bench, so the staged-to-party mapping is exercised. The opponent's
 * first mon faints to the local lead, then the battle ends with outcome. */
static void RunCoopTrainerBattle(struct RewardBattleState *saved, u8 id, u8 outcome)
{
    u32 nonce;
    u16 zero = 0;
    u8 i;

    memcpy(saved->enemy, gParties[B_TRAINER_1], sizeof(saved->enemy));
    saved->money = GetMoney(&gSaveBlock1Ptr->money);
    saved->fought = HasTrainerBeenFought(ENCOUNTER_TRAINER);
    saved->outcome = gBattleOutcome;
    saved->battlers = gBattlersCount;
    memcpy(saved->party_indexes, gBattlerPartyIndexes, sizeof(saved->party_indexes));
    memcpy(saved->positions, gBattlerPositions, sizeof(saved->positions));
    saved->absent = gAbsentBattlerFlags;
    ClearTrainerFlag(ENCOUNTER_TRAINER);
    SetMoney(&gSaveBlock1Ptr->money, 500);

    for (i = 0; i < PARTY_SIZE; i++)
        ZeroMonData(&gParties[B_TRAINER_0][i]);
    CreateUsableMon(&gParties[B_TRAINER_0][0], SPECIES_PIDGEY, 3);
    SetMonData(&gParties[B_TRAINER_0][0], MON_DATA_HP, &zero);
    CreateUsableMon(&gParties[B_TRAINER_0][1], SPECIES_CHARMANDER, 1);
    CreateUsableMon(&gParties[B_TRAINER_0][2], SPECIES_TREECKO, 4);
    gPartiesCount[B_TRAINER_0] = 3;

    nonce = BeginParkedEncounter();
    DeliverRequesterOffer(id, nonce);
    DeliverOutcome(id, nonce, COOP_BATTLE_CONSENT_ACCEPTED);
    CoopBattleConsent_Poll();
    ReleaseAcceptedEncounter(id);
    CoopBattleConsent_Poll();
    EXPECT(CoopBattleRuntime_IsEngineActive());
    EXPECT(CoopTrainerRewards_TestIsArmed());
    DrainOutbound();
    /* Staged in battle: Charmander in slot 0, Treecko in slot 1. */
    EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_SPECIES), SPECIES_CHARMANDER);
    EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][1], MON_DATA_SPECIES), SPECIES_TREECKO);

    CreateMon(&gParties[B_TRAINER_1][0], SPECIES_POOCHYENA, 20, 7, OTID_STRUCT_PRESET(2));
    gBattlersCount = MAX_BATTLERS_COUNT;
    for (i = 0; i < MAX_BATTLERS_COUNT; i++)
    {
        gBattlerPositions[i] = i;
        gBattlerPartyIndexes[i] = 0;
    }
    gAbsentBattlerFlags = 0;
    ResetSentPokesToOpponentValue();
    CoopTrainerRewards_RecordFaint(1);
    /* Nothing is applied while the battle (and its digest) is running. */
    EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_LEVEL), 5);
    if (outcome == B_OUTCOME_WON)
        CoopTrainerRewards_OnBattleWon(1);
    gBattleOutcome = outcome;
    CoopBattleRuntime_TestSetBattleFinishedSent();
    gMain.savedCallback();
    EXPECT(!CoopBattleRuntime_IsEngineActive());
    EXPECT(!CoopTrainerEncounter_TestIsPending());
    EXPECT(!CoopTrainerRewards_TestIsArmed());
    /* The full party is back in its original order. */
    EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_SPECIES), SPECIES_PIDGEY);
    EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][1], MON_DATA_SPECIES), SPECIES_CHARMANDER);
    EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][2], MON_DATA_SPECIES), SPECIES_TREECKO);
}

static void RestoreRewardBattleState(const struct RewardBattleState *saved)
{
    memcpy(gParties[B_TRAINER_1], saved->enemy, sizeof(saved->enemy));
    SetMoney(&gSaveBlock1Ptr->money, saved->money);
    if (saved->fought)
        SetTrainerFlag(ENCOUNTER_TRAINER);
    else
        ClearTrainerFlag(ENCOUNTER_TRAINER);
    gBattleOutcome = saved->outcome;
    gBattlersCount = saved->battlers;
    memcpy(gBattlerPartyIndexes, saved->party_indexes, sizeof(saved->party_indexes));
    memcpy(gBattlerPositions, saved->positions, sizeof(saved->positions));
    gAbsentBattlerFlags = saved->absent;
    gLeveledUpInBattle = 0;
}

TEST("Cloud Coop trainer win sets the flag, pays once and applies EXP after the battle")
{
    struct RewardBattleState saved;
    u32 prize = CoopTrainerRewards_GetPrizeMoney(ENCOUNTER_TRAINER, 1);
    u8 staged[1] = {1};

    BeginEncounterFixture();
    RunCoopTrainerBattle(&saved, 43, B_OUTCOME_WON);
    EXPECT(HasTrainerBeenFought(ENCOUNTER_TRAINER));
    EXPECT_GT(prize, 0);
    EXPECT_EQ(GetMoney(&gSaveBlock1Ptr->money), 500 + prize);
    /* Only the local mon that fought gained EXP, at its own party slot. */
    EXPECT_GT(GetMonData(&gParties[B_TRAINER_0][1], MON_DATA_LEVEL), 5);
    EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][2], MON_DATA_LEVEL), 5);
    EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_LEVEL), 5);
    /* The level-up goes through the post-battle evolution pass, which
     * finds nothing to evolve and resumes the parked trainer script. */
    EXPECT_EQ(gLeveledUpInBattle, 1u << 1);
    EXPECT_EQ(gMain.callback2, CB2_CoopTrainerRewardEvolutions);
    gMain.callback2();
    EXPECT_EQ(gMain.callback2, CB2_ReturnToFieldContinueScriptPlayMapMusic);
    EXPECT_EQ(gLeveledUpInBattle, 0);
    /* The resumed script cannot collect a second prize. */
    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, ENCOUNTER_TRAINER, staged, 1),
              COOP_TRAINER_REWARD_NONE);
    EXPECT_EQ(GetMoney(&gSaveBlock1Ptr->money), 500 + prize);
    RestoreRewardBattleState(&saved);
    EndEncounterFixture();
}

TEST("Cloud Coop trainer loss leaves the trainer, money and EXP untouched")
{
    struct RewardBattleState saved;

    BeginEncounterFixture();
    RunCoopTrainerBattle(&saved, 44, B_OUTCOME_LOST);
    EXPECT(!HasTrainerBeenFought(ENCOUNTER_TRAINER));
    EXPECT_EQ(GetMoney(&gSaveBlock1Ptr->money), 500);
    EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][1], MON_DATA_LEVEL), 5);
    EXPECT_EQ(gLeveledUpInBattle, 0);
    /* Local mons are still standing, so there is no whiteout. */
    EXPECT_EQ(gMain.callback2, CB2_ReturnToFieldContinueScriptPlayMapMusic);
    RestoreRewardBattleState(&saved);
    EndEncounterFixture();
}
