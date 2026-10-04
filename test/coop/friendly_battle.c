#include "global.h"
#include "battle.h"
#include "battle_caps.h"
#include "battle_controllers.h"
#include "battle_setup.h"
#include "coop/battle_consent.h"
#include "coop/battle_runtime.h"
#include "coop/friendly_battle.h"
#include "coop/net_bridge.h"
#include "coop/save.h"
#include "event_data.h"
#include "event_object_lock.h"
#include "event_object_movement.h"
#include "field_message_box.h"
#include "field_player_avatar.h"
#include "main.h"
#include "malloc.h"
#include "money.h"
#include "overworld.h"
#include "palette.h"
#include "pokemon.h"
#include "script.h"
#include "string_util.h"
#include "task.h"
#include "constants/battle.h"
#include "constants/event_objects.h"
#include "constants/items.h"
#include "constants/opponents.h"
#include "constants/trainers.h"
#include "test/test.h"

/*
 * Friendly battles (item 4.6). Two ROMs are simulated in one test by
 * resetting the runtime between the members' views: each view stages its own
 * team as B_TRAINER_0 and the other member's snapshot as B_TRAINER_1, draws
 * its own battlers at the bottom, and must hash the same canonical state.
 */

#define FRIENDLY_EPOCH 23

struct FriendlyFixture
{
    MainCallback callback1;
    MainCallback callback2;
    MainCallback saved_callback;
    bool8 fade_active;
    bool8 controls_locked;
    u32 frame;
    u32 battle_type_flags;
    u16 partner_trainer_id;
    TrainerBattleParameter params;
    bool8 tasks[NUM_TASKS];
    struct ObjectEvent object_events[OBJECT_EVENTS_COUNT];
    struct Sprite sprite0;
    struct PlayerAvatar player_avatar;
    struct Pokemon parties[3][PARTY_SIZE];
    u8 party_counts[3];
    u32 money;
    u8 outcome;
};

static struct FriendlyFixture *sFixture;
static u32 sSequence;

static void DeliverRecord(u16 type, const void *payload, u16 length)
{
    struct CoopBridgeMessage message;

    EXPECT(CoopBridgeMessage_Seal(&message, type, sSequence++, FRIENDLY_EPOCH,
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

static void EstablishGroupedSession(void)
{
    struct CoopBridgeMessage message;
    u8 grouped[2] = {TRUE, 0};

    CoopSave_InitializeCurrent();
    CoopNetBridge_Init();
    DrainOutbound();
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  1, FRIENDLY_EPOCH, NULL, 0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    DrainOutbound();
    sSequence = 2;
    DeliverRecord(COOP_BRIDGE_MESSAGE_GROUP_STATE_CHANGED, grouped, sizeof(grouped));
    EXPECT(CoopNetBridge_IsGrouped());
    EXPECT(CoopNetBridge_CanSendBattle());
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

static void CreateUsableMon(struct Pokemon *mon, enum Species species, u8 level, u32 personality)
{
    CreateMon(mon, species, level, personality, OTID_STRUCT_PRESET(0x1234));
    CalculateMonStats(mon);
}

static void BeginFixture(void)
{
    u32 i;

    sFixture = AllocZeroed(sizeof(*sFixture));
    EXPECT(sFixture != NULL);
    sFixture->callback1 = gMain.callback1;
    sFixture->callback2 = gMain.callback2;
    sFixture->saved_callback = gMain.savedCallback;
    sFixture->fade_active = gPaletteFade.active;
    sFixture->controls_locked = ArePlayerFieldControlsLocked();
    sFixture->frame = gMain.vblankCounter1;
    sFixture->battle_type_flags = gBattleTypeFlags;
    sFixture->partner_trainer_id = gPartnerTrainerId;
    sFixture->params = gTrainerBattleParameter;
    for (i = 0; i < NUM_TASKS; i++)
        sFixture->tasks[i] = gTasks[i].isActive;
    for (i = 0; i < OBJECT_EVENTS_COUNT; i++)
        sFixture->object_events[i] = gObjectEvents[i];
    sFixture->sprite0 = gSprites[0];
    sFixture->player_avatar = gPlayerAvatar;
    for (i = 0; i < 3; i++)
    {
        memcpy(sFixture->parties[i], gParties[i], sizeof(sFixture->parties[i]));
        sFixture->party_counts[i] = gPartiesCount[i];
    }
    sFixture->money = GetMoney(&gSaveBlock1Ptr->money);
    sFixture->outcome = gBattleOutcome;

    /* The battle start needs a bound player object. */
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
    for (i = 0; i < PARTY_SIZE; i++)
        ZeroMonData(&gParties[B_TRAINER_0][i]);
    gPartiesCount[B_TRAINER_0] = 0;
    EstablishGroupedSession();
}

static void EndFixture(void)
{
    u32 i;

    for (i = 0; i < NUM_TASKS; i++)
        if (gTasks[i].isActive && !sFixture->tasks[i])
            DestroyTask(i);
    HideFieldMessageBox();
    CoopBattleRuntime_DisarmEngine();
    CoopNetBridge_Init();
    ScriptContext_Init();
    gMain.callback1 = sFixture->callback1;
    gMain.callback2 = sFixture->callback2;
    gMain.savedCallback = sFixture->saved_callback;
    gPaletteFade.active = sFixture->fade_active;
    gMain.vblankCounter1 = sFixture->frame;
    gBattleTypeFlags = sFixture->battle_type_flags;
    gPartnerTrainerId = sFixture->partner_trainer_id;
    gTrainerBattleParameter = sFixture->params;
    for (i = 0; i < OBJECT_EVENTS_COUNT; i++)
        gObjectEvents[i] = sFixture->object_events[i];
    gSprites[0] = sFixture->sprite0;
    gPlayerAvatar = sFixture->player_avatar;
    for (i = 0; i < 3; i++)
    {
        memcpy(gParties[i], sFixture->parties[i], sizeof(sFixture->parties[i]));
        gPartiesCount[i] = sFixture->party_counts[i];
    }
    SetMoney(&gSaveBlock1Ptr->money, sFixture->money);
    gBattleOutcome = sFixture->outcome;
    if (sFixture->controls_locked)
        LockPlayerFieldControls();
    else
        UnlockPlayerFieldControls();
    Free(sFixture);
    sFixture = NULL;
}

static void SetParty(u8 count)
{
    u8 i;

    for (i = 0; i < PARTY_SIZE; i++)
        ZeroMonData(&gParties[B_TRAINER_0][i]);
    for (i = 0; i < count; i++)
        CreateUsableMon(&gParties[B_TRAINER_0][i], SPECIES_TREECKO + i * 3, 10 + i * 20, 100 + i);
    gPartiesCount[B_TRAINER_0] = count;
}

static void WriteRules(u8 *bytes, u8 format, u8 levels, u8 count)
{
    bytes[0] = format;
    bytes[1] = levels;
    bytes[2] = count;
}

static void DeliverResponderOffer(u8 id, u8 format, u8 levels, u8 count)
{
    u8 offer[COOP_BATTLE_JOIN_OFFER_SIZE] = {0};

    offer[0] = id;
    offer[16] = COOP_BATTLE_KIND_FRIENDLY;
    offer[17] = 1;
    WriteRules(&offer[COOP_BATTLE_JOIN_OFFER_RULES_OFFSET], format, levels, count);
    DeliverRecord(COOP_BRIDGE_MESSAGE_BATTLE_JOIN_OFFER, offer, sizeof(offer));
    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR));
}

static void FreeField(void)
{
    gMain.callback1 = CB1_Overworld;
    gMain.callback2 = CB2_Overworld;
    gPaletteFade.active = FALSE;
    ScriptContext_Init();
    if (ArePlayerFieldControlsLocked())
        UnlockPlayerFieldControls();
}

/* The partner's field is free: the prompt shows and the player says Yes. */
static void AcceptShownOffer(void)
{
    FreeField();
    CoopBattleConsent_Poll();
    Special_CoopBattleConsentGetOffer();
    EXPECT_EQ(gSpecialVar_Result, COOP_BATTLE_KIND_FRIENDLY);
    gSpecialVar_0x8004 = TRUE;
    Special_CoopBattleConsentRespond();
    EXPECT_EQ(gSpecialVar_Result, TRUE);
    ScriptContext_Init();
}

static u32 ReadNonce(const u8 *reserve)
{
    return reserve[1] | ((u32)reserve[2] << 8) | ((u32)reserve[3] << 16) | ((u32)reserve[4] << 24);
}

TEST("Cloud Coop accepted friendly offer is called off at its local start deadline")
{
    struct CoopBridgeMessage message;
    u8 id[COOP_BATTLE_ID_SIZE] = {21};
    u32 start;
    u16 ime;

    BeginFixture();
    SetParty(1);
    ime = FreezeFrameCounter();
    DeliverResponderOffer(21, COOP_BATTLE_FRIENDLY_SINGLES, COOP_BATTLE_FRIENDLY_LEVELS_AS_IS, 1);
    AcceptShownOffer();
    EXPECT(TakeOutbound(COOP_BRIDGE_MESSAGE_BATTLE_JOIN_RESPONSE, &message));
    EXPECT_EQ(message.payload[16], TRUE);
    Special_CoopFriendlyBeginResponderPicks();
    EXPECT_EQ(gSpecialVar_Result, TRUE);
    EXPECT_EQ(CoopFriendly_GetPhase(), COOP_FRIENDLY_PICKING);
    EXPECT(CoopBattleConsent_IsCurrentBattle(id));

    /* Before the fix an accepted friendly consent never expired locally. */
    start = gMain.vblankCounter1;
    gMain.vblankCounter1 = start + COOP_FRIENDLY_START_FRAMES - 1;
    CoopBattleConsent_Poll();
    EXPECT(CoopBattleConsent_IsCurrentBattle(id));
    EXPECT(!TakeOutbound(COOP_BRIDGE_MESSAGE_BATTLE_ABORT_REQUEST, &message));
    gMain.vblankCounter1 = start + COOP_FRIENDLY_START_FRAMES + 1;
    CoopBattleConsent_Poll();
    EXPECT(!CoopBattleConsent_IsCurrentBattle(id));
    EXPECT(CoopBattleConsent_IsIdle());
    EXPECT(TakeOutbound(COOP_BRIDGE_MESSAGE_BATTLE_ABORT_REQUEST, &message));
    EXPECT_EQ(message.payload[0], 21);
    EXPECT_EQ(message.payload[16], COOP_BATTLE_ABORT_CANCELED);
    /* The waiting script is told why. */
    EXPECT_EQ(CoopFriendly_GetPhase(), COOP_FRIENDLY_DONE);
    EXPECT_EQ(CoopFriendly_GetResult(), COOP_FRIENDLY_RESULT_TIMED_OUT);
    CoopFriendly_Finish();
    EXPECT_EQ(CoopFriendly_GetPhase(), COOP_FRIENDLY_IDLE);

    /* A trainer partner's acceptance has a deadline as well. */
    {
        u8 offer[COOP_BATTLE_JOIN_OFFER_SIZE] = {0};

        offer[0] = 22;
        offer[16] = COOP_BATTLE_KIND_COOPERATIVE_TRAINER;
        offer[17] = 1;
        DeliverRecord(COOP_BRIDGE_MESSAGE_BATTLE_JOIN_OFFER, offer, sizeof(offer));
        FreeField();
        CoopBattleConsent_Poll();
        gSpecialVar_0x8004 = TRUE;
        Special_CoopBattleConsentRespond();
        ScriptContext_Init();
        DrainOutbound();
        id[0] = 22;
        EXPECT(CoopBattleConsent_IsCurrentBattle(id));
        start = gMain.vblankCounter1;
        gMain.vblankCounter1 = start + COOP_TRAINER_ENCOUNTER_START_FRAMES + 1;
        LockPlayerFieldControls();
        CoopBattleConsent_Poll();
        EXPECT(!CoopBattleConsent_IsCurrentBattle(id));
        EXPECT(TakeOutbound(COOP_BRIDGE_MESSAGE_BATTLE_ABORT_REQUEST, &message));
        EXPECT_EQ(message.payload[0], 22);
    }
    ThawFrameCounter(ime);
    EndFixture();
}

TEST("Cloud Coop friendly challenge carries its rules and checks the echoed offer")
{
    struct CoopBattleFriendlyRules rules = {COOP_BATTLE_FRIENDLY_DOUBLES, COOP_BATTLE_FRIENDLY_LEVELS_50, 2};
    struct CoopBattleFriendlyRules held;
    struct CoopBridgeMessage message;
    u8 offer[COOP_BATTLE_JOIN_OFFER_SIZE] = {0};
    u8 outcome[COOP_BATTLE_CONSENT_OUTCOME_SIZE] = {0};
    u32 nonce;

    BeginFixture();
    SetParty(1);
    /* Two Pokemon are needed for a two-Pokemon challenge. */
    EXPECT(!CoopFriendly_BeginChallenge(&rules));
    EXPECT_EQ(CoopFriendly_GetResult(), COOP_FRIENDLY_RESULT_UNAVAILABLE);
    CoopFriendly_Finish();
    EXPECT(!TakeOutbound(COOP_BRIDGE_MESSAGE_TRAINER_BATTLE_RESERVE, &message));

    SetParty(2);
    EXPECT(CoopFriendly_CanBegin());
    EXPECT(CoopFriendly_BeginChallenge(&rules));
    EXPECT(!CoopFriendly_CanBegin());
    EXPECT(TakeOutbound(COOP_BRIDGE_MESSAGE_TRAINER_BATTLE_RESERVE, &message));
    EXPECT_EQ(message.length, COOP_BATTLE_RESERVE_SIZE);
    EXPECT_EQ(message.payload[0], COOP_BATTLE_KIND_FRIENDLY);
    EXPECT_EQ(message.payload[5], COOP_BATTLE_FRIENDLY_DOUBLES);
    EXPECT_EQ(message.payload[6], COOP_BATTLE_FRIENDLY_LEVELS_50);
    EXPECT_EQ(message.payload[7], 2);
    nonce = ReadNonce(message.payload);
    CoopFriendly_GetRules(&held);
    EXPECT_EQ(held.format, rules.format);
    EXPECT_EQ(held.level_mode, rules.level_mode);
    EXPECT_EQ(held.count, rules.count);

    /* The server must echo exactly this challenge. */
    offer[0] = 31;
    offer[16] = COOP_BATTLE_KIND_FRIENDLY;
    offer[18] = nonce;
    offer[19] = nonce >> 8;
    offer[20] = nonce >> 16;
    offer[21] = nonce >> 24;
    WriteRules(&offer[COOP_BATTLE_JOIN_OFFER_RULES_OFFSET], COOP_BATTLE_FRIENDLY_SINGLES,
               COOP_BATTLE_FRIENDLY_LEVELS_50, 2);
    EXPECT(!CoopBattleConsent_ReceiveOffer(offer, sizeof(offer)));
    WriteRules(&offer[COOP_BATTLE_JOIN_OFFER_RULES_OFFSET], COOP_BATTLE_FRIENDLY_DOUBLES,
               COOP_BATTLE_FRIENDLY_LEVELS_50, 2);
    EXPECT(CoopBattleConsent_ReceiveOffer(offer, sizeof(offer)));
    EXPECT(CoopBattleConsent_IsCurrentBattle(offer));

    /* A decline ends the challenge for the waiting script. */
    memcpy(outcome, offer, COOP_BATTLE_ID_SIZE);
    memcpy(outcome + 16, offer + 18, 4);
    outcome[20] = COOP_BATTLE_CONSENT_DECLINED;
    DeliverRecord(COOP_BRIDGE_MESSAGE_BATTLE_CONSENT_OUTCOME, outcome, sizeof(outcome));
    EXPECT_EQ(CoopFriendly_GetPhase(), COOP_FRIENDLY_DONE);
    EXPECT_EQ(CoopFriendly_GetResult(), COOP_FRIENDLY_RESULT_DECLINED);
    EXPECT(CoopBattleConsent_IsIdle());
    /* The exact outcome replays harmlessly. */
    DeliverRecord(COOP_BRIDGE_MESSAGE_BATTLE_CONSENT_OUTCOME, outcome, sizeof(outcome));
    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR));
    CoopFriendly_Finish();
    EndFixture();
}

TEST("Cloud Coop friendly offer shows the challenge and refuses invalid rules")
{
    static const u8 sDoubles[] = _("Doubles");
    static const u8 sLevels50[] = _("all at Lv. 50");
    static const u8 sThree[] = _("3");
    struct CoopBattleFriendlyRules rules;
    u8 offer[COOP_BATTLE_JOIN_OFFER_SIZE] = {0};

    BeginFixture();
    SetParty(2);
    offer[0] = 41;
    offer[16] = COOP_BATTLE_KIND_FRIENDLY;
    offer[17] = 1;
    /* Doubles needs two Pokemon, the team is 1..6, and a trainer offer never
     * carries rules. */
    WriteRules(&offer[COOP_BATTLE_JOIN_OFFER_RULES_OFFSET], COOP_BATTLE_FRIENDLY_DOUBLES,
               COOP_BATTLE_FRIENDLY_LEVELS_AS_IS, 1);
    EXPECT(!CoopBattleConsent_ReceiveOffer(offer, sizeof(offer)));
    WriteRules(&offer[COOP_BATTLE_JOIN_OFFER_RULES_OFFSET], COOP_BATTLE_FRIENDLY_SINGLES,
               COOP_BATTLE_FRIENDLY_LEVELS_AS_IS, 7);
    EXPECT(!CoopBattleConsent_ReceiveOffer(offer, sizeof(offer)));
    WriteRules(&offer[COOP_BATTLE_JOIN_OFFER_RULES_OFFSET], COOP_BATTLE_FRIENDLY_SINGLES, 2, 3);
    EXPECT(!CoopBattleConsent_ReceiveOffer(offer, sizeof(offer)));
    offer[16] = COOP_BATTLE_KIND_COOPERATIVE_TRAINER;
    WriteRules(&offer[COOP_BATTLE_JOIN_OFFER_RULES_OFFSET], COOP_BATTLE_FRIENDLY_SINGLES, 0, 1);
    EXPECT(!CoopBattleConsent_ReceiveOffer(offer, sizeof(offer)));
    EXPECT(!CoopBattleConsent_ReceiveOffer(offer, sizeof(offer) - 3));

    DeliverResponderOffer(41, COOP_BATTLE_FRIENDLY_DOUBLES, COOP_BATTLE_FRIENDLY_LEVELS_50, 3);
    FreeField();
    CoopBattleConsent_Poll();
    EXPECT(CoopBattleConsent_GetOfferRules(&rules));
    EXPECT_EQ(rules.format, COOP_BATTLE_FRIENDLY_DOUBLES);
    EXPECT_EQ(rules.level_mode, COOP_BATTLE_FRIENDLY_LEVELS_50);
    EXPECT_EQ(rules.count, 3);
    /* Two Pokemon cannot take a three-Pokemon challenge. */
    Special_CoopFriendlyBufferRules();
    EXPECT_EQ(gSpecialVar_Result, FALSE);
    EXPECT_EQ(StringCompare(gStringVar1, sDoubles), 0);
    EXPECT_EQ(StringCompare(gStringVar2, sLevels50), 0);
    EXPECT_EQ(StringCompare(gStringVar3, sThree), 0);
    SetParty(3);
    Special_CoopFriendlyBufferRules();
    EXPECT_EQ(gSpecialVar_Result, TRUE);
    gSpecialVar_0x8004 = FALSE;
    Special_CoopBattleConsentRespond();
    ScriptContext_Init();
    EXPECT(CoopBattleConsent_IsIdle());
    EndFixture();
}

TEST("Cloud Coop friendly team picks follow the pick order and refuse unusable Pokemon")
{
    struct CoopBattleFriendlyRules rules = {COOP_BATTLE_FRIENDLY_SINGLES, COOP_BATTLE_FRIENDLY_LEVELS_AS_IS, 2};
    struct Pokemon team[PARTY_SIZE];
    struct CoopBridgeMessage message;
    u16 zero = 0;
    bool8 egg = TRUE;

    BeginFixture();
    SetParty(4);
    SetMonData(&gParties[B_TRAINER_0][0], MON_DATA_HP, &zero);
    SetMonData(&gParties[B_TRAINER_0][3], MON_DATA_IS_EGG, &egg);
    EXPECT(CoopFriendly_BeginChallenge(&rules));
    DrainOutbound();
    EXPECT_EQ(CoopFriendly_PickMon(0), COOP_FRIENDLY_PICK_REFUSED); // fainted
    EXPECT_EQ(CoopFriendly_PickMon(3), COOP_FRIENDLY_PICK_REFUSED); // egg
    EXPECT_EQ(CoopFriendly_PickMon(4), COOP_FRIENDLY_PICK_REFUSED); // empty
    EXPECT_EQ(CoopFriendly_PickMon(2), COOP_FRIENDLY_PICK_OK);
    EXPECT_EQ(CoopFriendly_PickMon(2), COOP_FRIENDLY_PICK_REFUSED); // already picked
    EXPECT(!CoopFriendly_IsTeamComplete());
    EXPECT_EQ(CoopFriendly_BuildTeam(team), 0);
    EXPECT_EQ(CoopFriendly_PickMon(1), COOP_FRIENDLY_PICK_OK);
    EXPECT(CoopFriendly_IsTeamComplete());
    EXPECT_EQ(CoopFriendly_PickMon(1), COOP_FRIENDLY_PICK_REFUSED);
    EXPECT_EQ(CoopFriendly_BuildTeam(team), 2);
    EXPECT_EQ(memcmp(&team[0], &gParties[B_TRAINER_0][2], sizeof(team[0])), 0);
    EXPECT_EQ(memcmp(&team[1], &gParties[B_TRAINER_0][1], sizeof(team[1])), 0);
    Special_CoopFriendlyBufferPick();
    EXPECT_EQ(gSpecialVar_Result, 2);
    CoopFriendly_Finish();
    EXPECT_EQ(CoopFriendly_GetPhase(), COOP_FRIENDLY_PICKING); // only a finished flow resets

    /* B in the party menu withdraws the challenge. */
    EXPECT_EQ(CoopFriendly_PickMon(PARTY_SIZE), COOP_FRIENDLY_PICK_CANCELLED);
    EXPECT_EQ(CoopFriendly_GetResult(), COOP_FRIENDLY_RESULT_CANCELLED);
    EXPECT(CoopBattleConsent_IsIdle());
    EXPECT(!TakeOutbound(COOP_BRIDGE_MESSAGE_BATTLE_ABORT_REQUEST, &message)); // no battle ID yet
    CoopFriendly_Finish();
    EndFixture();
}

/* ------------------------------------------------------------------------
 * Two ROMs from each other's records.
 */

static void BuildManifest(u8 *manifest, u8 id, u8 slot, const struct CoopBattleFriendlyRules *rules,
                          const struct Pokemon *member0, const struct Pokemon *member1, u8 count)
{
    u8 i;

    memset(manifest, 0, COOP_BATTLE_MANIFEST_SIZE);
    manifest[0] = id;
    for (i = 18; i < 50; i++)
        manifest[i] = 0x5A ^ i;
    EXPECT(CoopBattleRuntime_ComputePartyDigest(member0, count, &manifest[50], COOP_BATTLE_DIGEST_SIZE));
    EXPECT(CoopBattleRuntime_ComputePartyDigest(member1, count, &manifest[82], COOP_BATTLE_DIGEST_SIZE));
    manifest[COOP_BATTLE_MANIFEST_KIND_OFFSET] = COOP_BATTLE_MANIFEST_KIND_FRIENDLY;
    manifest[COOP_BATTLE_MANIFEST_MEMBER_SLOT_OFFSET] = slot;
    CoopBattleRuntime_EncodeFriendlyRules(rules, &manifest[COOP_BATTLE_MANIFEST_RULES_OFFSET]);
}

/* One ROM's runtime after the snapshot exchange: its manifest view and the
 * peer's complete snapshot. */
static void LoadRomView(u8 id, u8 slot, const struct CoopBattleFriendlyRules *rules,
                        const struct Pokemon *member0, const struct Pokemon *member1, u8 count)
{
    u8 manifest[COOP_BATTLE_MANIFEST_SIZE];
    u8 chunk[COOP_BATTLE_PARTY_SNAPSHOT_SIZE] = {0};
    const struct Pokemon *peer = slot == 0 ? member1 : member0;
    u8 i;

    CoopBattleRuntime_Init();
    CoopBattleRuntime_OnSessionReady(FRIENDLY_EPOCH);
    BuildManifest(manifest, id, slot, rules, member0, member1, count);
    EXPECT_EQ(CoopBattleRuntime_ReceiveManifest(manifest, sizeof(manifest)), COOP_BATTLE_INBOUND_ACCEPTED);
    for (i = 0; i < count; i++)
    {
        chunk[0] = id;
        chunk[16] = i;
        chunk[17] = i;
        chunk[18] = count;
        chunk[19] = COOP_BATTLE_PARTY_MON_SIZE;
        memcpy(chunk + 20, &peer[i], sizeof(peer[i]));
        EXPECT_EQ(CoopBattleRuntime_ReceivePeerPartyChunk(chunk, sizeof(chunk)), COOP_BATTLE_INBOUND_ACCEPTED);
    }
}

/* The battle state one ROM holds a few turns in: battler 0 is member 0's
 * lead and battler 1 member 1's on both ROMs; sides and trainer parties are
 * drawn from this ROM's point of view. */
static void StageBattleView(u8 slot, struct Pokemon *member0, struct Pokemon *member1,
                            u8 count, u8 member0Outcome)
{
    u8 i;
    enum BattleSide member0Side = slot == 0 ? B_SIDE_PLAYER : B_SIDE_OPPONENT;

    for (i = 0; i < PARTY_SIZE; i++)
    {
        ZeroMonData(&gParties[B_TRAINER_0][i]);
        ZeroMonData(&gParties[B_TRAINER_1][i]);
    }
    for (i = 0; i < count; i++)
    {
        gParties[slot == 0 ? B_TRAINER_0 : B_TRAINER_1][i] = member0[i];
        gParties[slot == 0 ? B_TRAINER_1 : B_TRAINER_0][i] = member1[i];
    }
    gPartiesCount[B_TRAINER_0] = count;
    gPartiesCount[B_TRAINER_1] = count;
    gBattleTypeFlags = BATTLE_TYPE_TRAINER | BATTLE_TYPE_SECRET_BASE;
    gBattlersCount = 2;
    gBattlerPositions[0] = slot == 0 ? B_POSITION_PLAYER_LEFT : B_POSITION_OPPONENT_LEFT;
    gBattlerPositions[1] = slot == 0 ? B_POSITION_OPPONENT_LEFT : B_POSITION_PLAYER_LEFT;
    gBattlerPartyIndexes[0] = 1;
    gBattlerPartyIndexes[1] = 0;
    memset(gBattleMons, 0, sizeof(gBattleMons));
    gBattleMons[0].species = GetMonData(&member0[1], MON_DATA_SPECIES);
    gBattleMons[0].hp = 17;
    gBattleMons[1].species = GetMonData(&member1[0], MON_DATA_SPECIES);
    gBattleMons[1].hp = 23;
    gBattleMons[1].volatiles.infatuation = 1; // battler references are canonical
    memset(gSideStatuses, 0, sizeof(gSideStatuses));
    memset(gSideTimers, 0, sizeof(gSideTimers));
    memset(gBattleStruct, 0, sizeof(*gBattleStruct));
    gSideStatuses[member0Side] = SIDE_STATUS_REFLECT;
    gSideTimers[member0Side].reflectTimer = 3;
    gBattleStruct->hazardsQueue[member0Side ^ BIT_SIDE][0] = 1;
    gBattleStruct->numHazards[member0Side ^ BIT_SIDE] = 1;
    gBattleStruct->moveTarget[0] = 1;
    gBattleStruct->moveTarget[1] = 0;
    gBattleStruct->partyState[slot == 0 ? B_TRAINER_0 : B_TRAINER_1][1].sentOut = TRUE;
    gBattleStruct->partyState[slot == 0 ? B_TRAINER_1 : B_TRAINER_0][0].sentOut = TRUE;
    /* Each ROM keeps its own outcome; member 0 won here. */
    if (member0Outcome == B_OUTCOME_WON)
        gBattleOutcome = slot == 0 ? B_OUTCOME_WON : B_OUTCOME_LOST;
    else
        gBattleOutcome = member0Outcome;
    /* Only the forfeiting ROM marks its own Run choice. */
    gHitMarker = slot == 1 ? HITMARKER_RUN : 0;
}

TEST("Cloud Coop friendly ROMs stage each other's records and hash one canonical battle")
{
    struct CoopBattleFriendlyRules rules = {COOP_BATTLE_FRIENDLY_SINGLES, COOP_BATTLE_FRIENDLY_LEVELS_AS_IS, 2};
    /* 22 Pokemon on the IWRAM stack left too little of the ~3 KiB test stack
     * for EndFixture's bridge reset, which then overwrote gTasks below the
     * stack. Keep the parties on the heap. */
    struct FriendlyCanonicalParties
    {
        struct Pokemon partyA[PARTY_SIZE], partyB[PARTY_SIZE], teamA[2], teamB[2], copy[PARTY_SIZE];
    } *parties;
    struct Pokemon *partyA, *partyB, *teamA, *teamB, *copy;
    struct CoopBattleStartupPlan *plan;
    struct BattleStruct *savedBattleStruct = gBattleStruct;
    u8 digestA[COOP_BATTLE_DIGEST_SIZE], digestB[COOP_BATTLE_DIGEST_SIZE];
    u8 id[COOP_BATTLE_ID_SIZE] = {51};
    u8 count;
    u32 savedHitMarker = gHitMarker;

    BeginFixture();
    parties = AllocZeroed(sizeof(*parties));
    EXPECT(parties != NULL);
    partyA = parties->partyA;
    partyB = parties->partyB;
    teamA = parties->teamA;
    teamB = parties->teamB;
    copy = parties->copy;
    plan = AllocZeroed(sizeof(*plan));
    gBattleStruct = AllocZeroed(sizeof(*gBattleStruct));
    for (count = 0; count < PARTY_SIZE; count++)
    {
        ZeroMonData(&partyA[count]);
        ZeroMonData(&partyB[count]);
    }
    CreateUsableMon(&partyA[0], SPECIES_TORCHIC, 12, 1);
    CreateUsableMon(&partyA[1], SPECIES_WINGULL, 14, 2);
    CreateUsableMon(&partyA[2], SPECIES_RALTS, 9, 3);
    CreateUsableMon(&partyB[0], SPECIES_MUDKIP, 13, 4);
    CreateUsableMon(&partyB[1], SPECIES_ZIGZAGOON, 11, 5);
    /* Member 0 picked its third and first Pokemon, member 1 both, in order. */
    teamA[0] = partyA[2];
    teamA[1] = partyA[0];
    teamB[0] = partyB[1];
    teamB[1] = partyB[0];

    /* Member 0's ROM. */
    LoadRomView(51, 0, &rules, teamA, teamB, 2);
    EXPECT(CoopBattleRuntime_MakeFriendlyPlan(id, partyA, 3, teamA, 2, plan));
    EXPECT_EQ(plan->local_member_slot, 0);
    EXPECT_EQ(plan->member_party_trainers[0], B_TRAINER_0);
    EXPECT_EQ(plan->member_party_trainers[1], B_TRAINER_1);
    EXPECT_EQ(plan->member_battler_positions[0], B_POSITION_PLAYER_LEFT);
    EXPECT_EQ(plan->staged_local_count, 2);
    EXPECT_EQ(memcmp(plan->original_local, partyA, sizeof(parties->partyA)), 0);
    EXPECT(CoopBattleRuntime_CopyPeerTeam(copy, PARTY_SIZE, &count));
    EXPECT_EQ(count, 2);
    EXPECT_EQ(memcmp(copy, teamB, sizeof(parties->teamB)), 0);
    /* The picked order is the snapshot: another order is not the manifest's. */
    {
        struct Pokemon swapped[2] = {teamA[1], teamA[0]};

        EXPECT(!CoopBattleRuntime_MakeFriendlyPlan(id, partyA, 3, swapped, 2, plan));
        EXPECT(!CoopBattleRuntime_MakeFriendlyPlan(id, partyA, 3, teamA, 1, plan));
    }
    EXPECT(CoopBattleRuntime_ArmEngine(id));
    EXPECT(CoopBattleRuntime_IsFriendlyEngine());
    EXPECT(!CoopBattleRuntime_IsFriendlyDoubles());
    EXPECT_EQ(CoopBattleRuntime_EngineLocalMemberSlot(), 0);
    StageBattleView(0, teamA, teamB, 2, B_OUTCOME_WON);
    EXPECT_EQ(GetBattlerTrainer(0), B_TRAINER_0);
    EXPECT_EQ(GetBattlerTrainer(1), B_TRAINER_1);
    EXPECT(!BattlerHasAi(1));
    EXPECT(CoopBattleRuntime_ComputeBattleDigest(digestA, sizeof(digestA)));
    CoopBattleRuntime_DisarmEngine();

    /* Member 1's ROM: its own team at the bottom, member 0's battlers drawn
     * on the opponent side, the same battler IDs. */
    LoadRomView(51, 1, &rules, teamA, teamB, 2);
    EXPECT(CoopBattleRuntime_MakeFriendlyPlan(id, partyB, 2, teamB, 2, plan));
    EXPECT_EQ(plan->local_member_slot, 1);
    EXPECT_EQ(plan->member_party_trainers[0], B_TRAINER_1);
    EXPECT_EQ(plan->member_party_trainers[1], B_TRAINER_0);
    EXPECT_EQ(plan->member_battler_positions[1], B_POSITION_PLAYER_LEFT);
    EXPECT(CoopBattleRuntime_CopyPeerTeam(copy, PARTY_SIZE, &count));
    EXPECT_EQ(memcmp(copy, teamA, sizeof(parties->teamA)), 0);
    EXPECT(CoopBattleRuntime_ArmEngine(id));
    EXPECT_EQ(CoopBattleRuntime_EngineLocalMemberSlot(), 1);
    EXPECT_EQ(CoopBattleRuntime_CanonicalBattler(0), 0);
    StageBattleView(1, teamA, teamB, 2, B_OUTCOME_WON);
    EXPECT_EQ(GetBattlerTrainer(0), B_TRAINER_1); // member 0's team, drawn at the top
    EXPECT_EQ(GetBattlerTrainer(1), B_TRAINER_0);
    EXPECT(CoopBattleRuntime_ComputeBattleDigest(digestB, sizeof(digestB)));
    EXPECT_EQ(memcmp(digestA, digestB, sizeof(digestA)), 0);

    /* A different battle is a different digest: Reflect on member 1's side. */
    gSideStatuses[B_SIDE_PLAYER] = SIDE_STATUS_REFLECT;
    gSideStatuses[B_SIDE_OPPONENT] = 0;
    EXPECT(CoopBattleRuntime_ComputeBattleDigest(digestB, sizeof(digestB)));
    EXPECT_NE(memcmp(digestA, digestB, sizeof(digestA)), 0);
    /* ... and so is the other member winning. */
    StageBattleView(1, teamA, teamB, 2, B_OUTCOME_WON);
    gBattleOutcome = B_OUTCOME_WON;
    EXPECT(CoopBattleRuntime_ComputeBattleDigest(digestB, sizeof(digestB)));
    EXPECT_NE(memcmp(digestA, digestB, sizeof(digestA)), 0);
    CoopBattleRuntime_DisarmEngine();

    /* A trainer manifest never makes a friendly plan, and vice versa. */
    EXPECT(!CoopBattleRuntime_MakeStartupPlan(id, partyB, 2, plan));
    Free(gBattleStruct);
    gBattleStruct = savedBattleStruct;
    gHitMarker = savedHitMarker;
    Free(plan);
    Free(parties);
    CoopBattleRuntime_Init();
    EndFixture();
}

TEST("Cloud Coop friendly doubles exchange two actions per member and a forfeit")
{
    struct CoopBattleFriendlyRules rules = {COOP_BATTLE_FRIENDLY_DOUBLES, COOP_BATTLE_FRIENDLY_LEVELS_AS_IS, 2};
    struct CoopBattleAction local[2] = {
        {COOP_BATTLE_ACTION_MOVE, 1, 3},
        {COOP_BATTLE_ACTION_SWITCH, 5, 0},
    };
    struct CoopBattleAction peer[2];
    struct Pokemon team[2], other[2];
    struct CoopBridgeMessage message;
    u8 manifest[COOP_BATTLE_MANIFEST_SIZE];
    u8 chunk[COOP_BATTLE_PARTY_SNAPSHOT_SIZE] = {0};
    u8 bundle[20 + 2 * COOP_BATTLE_MAX_MEMBER_ACTION_BYTES] = {0};
    u8 id[COOP_BATTLE_ID_SIZE] = {61};
    u8 count;
    u8 i;

    BeginFixture();
    CreateUsableMon(&team[0], SPECIES_TORCHIC, 12, 1);
    CreateUsableMon(&team[1], SPECIES_WINGULL, 14, 2);
    CreateUsableMon(&other[0], SPECIES_MUDKIP, 13, 4);
    CreateUsableMon(&other[1], SPECIES_ZIGZAGOON, 11, 5);
    BuildManifest(manifest, 61, 0, &rules, team, other, 2);
    DeliverRecord(COOP_BRIDGE_MESSAGE_BATTLE_MANIFEST, manifest, sizeof(manifest));
    for (i = 0; i < 2; i++)
    {
        chunk[0] = 61;
        chunk[16] = chunk[17] = i;
        chunk[18] = 2;
        chunk[19] = COOP_BATTLE_PARTY_MON_SIZE;
        memcpy(chunk + 20, &other[i], sizeof(other[i]));
        DeliverRecord(COOP_BRIDGE_MESSAGE_PEER_PARTY_CHUNK, chunk, sizeof(chunk));
    }
    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR));
    EXPECT(CoopBattleRuntime_ArmEngine(id));
    EXPECT(CoopBattleRuntime_IsFriendlyDoubles());
    /* A doubles member always sends both of its battlers' actions. */
    EXPECT(!CoopBattleRuntime_SubmitLocalActions(local, 1));
    EXPECT(CoopBattleRuntime_SubmitLocalActions(local, 2));
    EXPECT(TakeOutbound(COOP_BRIDGE_MESSAGE_ACTION_INTENT, &message));
    EXPECT_EQ(message.length, 19 + 8);
    EXPECT_EQ(message.payload[18], 8);
    EXPECT_EQ(message.payload[19], COOP_BATTLE_ACTION_MOVE);
    EXPECT_EQ(message.payload[21], 3);
    EXPECT_EQ(message.payload[23], COOP_BATTLE_ACTION_SWITCH);
    EXPECT_EQ(message.payload[24], 5);

    bundle[0] = 61;
    bundle[16] = 1;
    bundle[18] = 8;
    bundle[19] = 8;
    memcpy(bundle + 20, message.payload + 19, 8);
    bundle[28] = COOP_BATTLE_ACTION_FORFEIT;
    bundle[32] = COOP_BATTLE_ACTION_FORFEIT;
    /* The local half must be exactly what this ROM sent. */
    bundle[24 + 1] = 4;
    DeliverRecord(COOP_BRIDGE_MESSAGE_TURN_BUNDLE, bundle, sizeof(bundle));
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR);
    gCoopNetBridge.status_flags &= ~COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
    bundle[24 + 1] = 5;
    DeliverRecord(COOP_BRIDGE_MESSAGE_TURN_BUNDLE, bundle, sizeof(bundle));
    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR));
    EXPECT(CoopBattleRuntime_PollPeerActions(peer, &count));
    EXPECT_EQ(count, 2);
    EXPECT_EQ(peer[0].kind, COOP_BATTLE_ACTION_FORFEIT);
    EXPECT_EQ(peer[1].kind, COOP_BATTLE_ACTION_FORFEIT);
    EXPECT_EQ(CoopBattleRuntime_CurrentRound(), 1);
    EXPECT(!CoopBattleRuntime_IsRoundHashed());
    EXPECT(CoopBattleRuntime_IsEngineTurnReady());
    CoopBattleRuntime_FinishEngineTurn();
    EXPECT(!CoopBattleRuntime_IsEngineFaulted());
    CoopBattleRuntime_DisarmEngine();
    EndFixture();
}

/* ------------------------------------------------------------------------
 * The partner's ROM through a whole friendly battle.
 */

struct PartnerBattle
{
    struct Pokemon original[PARTY_SIZE];
    struct Pokemon peer[2];
    u8 id;
};

static void StartBattleAsPartner(struct PartnerBattle *battle, u8 id, u8 levels)
{
    struct Pokemon team[PARTY_SIZE];
    struct CoopBridgeMessage message;
    struct CoopBattleFriendlyRules rules = {COOP_BATTLE_FRIENDLY_SINGLES, levels, 2};
    u8 manifest[COOP_BATTLE_MANIFEST_SIZE];
    u8 chunk[COOP_BATTLE_PARTY_SNAPSHOT_SIZE] = {0};
    u8 start[COOP_BATTLE_START_SIZE] = {0};
    u16 hp;
    u8 i;

    battle->id = id;
    SetParty(3);
    /* Party slot 1 is damaged, poisoned, holds an item and used some PP. */
    hp = GetMonData(&gParties[B_TRAINER_0][1], MON_DATA_MAX_HP) / 2;
    SetMonData(&gParties[B_TRAINER_0][1], MON_DATA_HP, &hp);
    {
        u32 status = STATUS1_POISON;
        u16 item = ITEM_ORAN_BERRY;
        u8 pp = 3;

        SetMonData(&gParties[B_TRAINER_0][1], MON_DATA_STATUS, &status);
        SetMonData(&gParties[B_TRAINER_0][1], MON_DATA_HELD_ITEM, &item);
        SetMonData(&gParties[B_TRAINER_0][1], MON_DATA_PP1, &pp);
    }
    memcpy(battle->original, gParties[B_TRAINER_0], sizeof(battle->original));
    CreateUsableMon(&battle->peer[0], SPECIES_BULBASAUR, 30, 7);
    CreateUsableMon(&battle->peer[1], SPECIES_POOCHYENA, 70, 8);

    DeliverResponderOffer(id, COOP_BATTLE_FRIENDLY_SINGLES, levels, 2);
    AcceptShownOffer();
    DrainOutbound();
    Special_CoopFriendlyBeginResponderPicks();
    EXPECT_EQ(gSpecialVar_Result, TRUE);
    EXPECT_EQ(CoopFriendly_PickMon(1), COOP_FRIENDLY_PICK_OK);
    EXPECT_EQ(CoopFriendly_PickMon(0), COOP_FRIENDLY_PICK_OK);
    CoopFriendly_BeginWaiting();
    EXPECT(CoopFriendly_IsWaiting());
    EXPECT_EQ(CoopFriendly_BuildTeam(team), 2);

    /* Parked: the field is locked and the script waits. */
    gMain.callback1 = CB1_Overworld;
    gMain.callback2 = CB2_Overworld;
    gPaletteFade.active = FALSE;
    ScriptContext_Init();
    if (!ArePlayerFieldControlsLocked())
        LockPlayerFieldControls();
    for (i = 0; i < 2; i++)
    {
        CoopBattleConsent_Poll();
        EXPECT(TakeOutbound(COOP_BRIDGE_MESSAGE_PARTY_SNAPSHOT, &message));
        EXPECT_EQ(message.payload[16], i);
        EXPECT_EQ(message.payload[18], 2);
        EXPECT_EQ(memcmp(message.payload + 20, &team[i], sizeof(team[i])), 0);
    }
    /* This ROM is member 1. */
    BuildManifest(manifest, id, 1, &rules, battle->peer, team, 2);
    DeliverRecord(COOP_BRIDGE_MESSAGE_BATTLE_MANIFEST, manifest, sizeof(manifest));
    for (i = 0; i < 2; i++)
    {
        chunk[0] = id;
        chunk[16] = chunk[17] = i;
        chunk[18] = 2;
        chunk[19] = COOP_BATTLE_PARTY_MON_SIZE;
        memcpy(chunk + 20, &battle->peer[i], sizeof(battle->peer[i]));
        DeliverRecord(COOP_BRIDGE_MESSAGE_PEER_PARTY_CHUNK, chunk, sizeof(chunk));
    }
    CoopBattleConsent_Poll();
    EXPECT(TakeOutbound(COOP_BRIDGE_MESSAGE_BATTLE_READY, &message));
    start[0] = id;
    DeliverRecord(COOP_BRIDGE_MESSAGE_BATTLE_START, start, sizeof(start));
    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR));
    CoopBattleConsent_Poll();
    EXPECT(CoopBattleRuntime_IsFriendlyEngine());
    EXPECT_EQ(CoopFriendly_GetPhase(), COOP_FRIENDLY_IN_BATTLE);
}

TEST("Cloud Coop friendly Lv. 50 battle uses copies and restores the party exactly")
{
    struct PartnerBattle *battle = AllocZeroed(sizeof(*battle));
    u32 money;
    u16 zero = 0;
    u16 noItem = ITEM_NONE;
    u32 savedExp;
    u8 i;

    BeginFixture();
    money = GetMoney(&gSaveBlock1Ptr->money);
    StartBattleAsPartner(battle, 71, COOP_BATTLE_FRIENDLY_LEVELS_50);
    EXPECT_EQ(gBattleTypeFlags, BATTLE_TYPE_TRAINER | BATTLE_TYPE_SECRET_BASE);
    EXPECT_EQ(TRAINER_BATTLE_PARAM.opponentA, TRAINER_SECRET_BASE);
    EXPECT_EQ(CoopBattleRuntime_EngineLocalMemberSlot(), 1);
    EXPECT_EQ(gMain.savedCallback != NULL, TRUE);
    /* Both teams battle as copies set to Lv. 50, in pick order, whatever
     * their own level (30, 70; 10 and 30 before). */
    EXPECT_EQ(gPartiesCount[B_TRAINER_0], 2);
    EXPECT_EQ(gPartiesCount[B_TRAINER_1], 2);
    EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_SPECIES),
              GetMonData(&battle->original[1], MON_DATA_SPECIES));
    EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][1], MON_DATA_SPECIES),
              GetMonData(&battle->original[0], MON_DATA_SPECIES));
    EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][2], MON_DATA_SPECIES), SPECIES_NONE);
    for (i = 0; i < 2; i++)
    {
        EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][i], MON_DATA_LEVEL), COOP_BATTLE_FRIENDLY_LEVEL);
        EXPECT_EQ(GetMonData(&gParties[B_TRAINER_1][i], MON_DATA_LEVEL), COOP_BATTLE_FRIENDLY_LEVEL);
        EXPECT_EQ(GetMonData(&gParties[B_TRAINER_1][i], MON_DATA_SPECIES),
                  GetMonData(&battle->peer[i], MON_DATA_SPECIES));
    }
    /* The half-HP Pokemon keeps its HP fraction; the full one stays full. */
    EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_HP),
              GetMonData(&battle->original[1], MON_DATA_HP)
              * GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_MAX_HP)
              / GetMonData(&battle->original[1], MON_DATA_MAX_HP));
    EXPECT_NE(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_MAX_HP),
              GetMonData(&battle->original[1], MON_DATA_MAX_HP));
    EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][1], MON_DATA_HP),
              GetMonData(&gParties[B_TRAINER_0][1], MON_DATA_MAX_HP));
    EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_STATUS), STATUS1_POISON);
    EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_HELD_ITEM), ITEM_ORAN_BERRY);

    /* The battle faints the lead, eats its berry, uses PP and would give
     * EXP; this member forfeits. */
    savedExp = GetMonData(&gParties[B_TRAINER_0][1], MON_DATA_EXP) + 5000;
    SetMonData(&gParties[B_TRAINER_0][0], MON_DATA_HP, &zero);
    SetMonData(&gParties[B_TRAINER_0][0], MON_DATA_HELD_ITEM, &noItem);
    SetMonData(&gParties[B_TRAINER_0][1], MON_DATA_PP1, &zero);
    SetMonData(&gParties[B_TRAINER_0][1], MON_DATA_EXP, &savedExp);
    gBattleOutcome = B_OUTCOME_LOST;
    CoopBattleRuntime_TestSetBattleFinishedSent();
    gMain.savedCallback();

    EXPECT_EQ(memcmp(gParties[B_TRAINER_0], battle->original, sizeof(battle->original)), 0);
    EXPECT_EQ(gPartiesCount[B_TRAINER_0], 3);
    for (i = 0; i < PARTY_SIZE; i++)
        EXPECT_EQ(GetMonData(&gParties[B_TRAINER_1][i], MON_DATA_SPECIES), SPECIES_NONE);
    EXPECT_EQ(GetMoney(&gSaveBlock1Ptr->money), money);
    EXPECT_EQ(gBattleTypeFlags, sFixture->battle_type_flags);
    EXPECT(!CoopBattleRuntime_IsEngineActive());
    EXPECT(CoopBattleConsent_IsIdle());
    EXPECT_EQ(CoopFriendly_GetPhase(), COOP_FRIENDLY_DONE);
    EXPECT_EQ(CoopFriendly_GetResult(), COOP_FRIENDLY_RESULT_LOST);
    EXPECT_EQ(gMain.callback2, CB2_ReturnToFieldContinueScriptPlayMapMusic);
    Special_CoopFriendlyBufferResult();
    EXPECT_EQ(gSpecialVar_Result, COOP_FRIENDLY_RESULT_LOST);
    CoopFriendly_Finish();
    DrainOutbound();

    /* A win by the partner's forfeit reads as a win here. */
    StartBattleAsPartner(battle, 72, COOP_BATTLE_FRIENDLY_LEVELS_AS_IS);
    /* As-is levels keep each copy's own level. */
    EXPECT_EQ(GetMonData(&gParties[B_TRAINER_1][1], MON_DATA_LEVEL), 70);
    gBattleOutcome = B_OUTCOME_WON;
    CoopBattleRuntime_TestSetBattleFinishedSent();
    gMain.savedCallback();
    EXPECT_EQ(memcmp(gParties[B_TRAINER_0], battle->original, sizeof(battle->original)), 0);
    EXPECT_EQ(CoopFriendly_GetResult(), COOP_FRIENDLY_RESULT_WON);
    EXPECT_EQ(GetMoney(&gSaveBlock1Ptr->money), money);
    CoopFriendly_Finish();
    DrainOutbound();
    Free(battle);
    EndFixture();
}

TEST("Cloud Coop friendly disconnect or desync ends as no contest with the party restored")
{
    struct PartnerBattle *battle = AllocZeroed(sizeof(*battle));
    struct CoopBridgeMessage message;
    u8 abortPayload[COOP_BATTLE_ABORT_SIZE] = {0};
    u16 zero = 0;
    u8 i;

    BeginFixture();
    StartBattleAsPartner(battle, 81, COOP_BATTLE_FRIENDLY_LEVELS_50);
    SetMonData(&gParties[B_TRAINER_0][0], MON_DATA_HP, &zero);
    /* The server gave up on the partner (or saw a desync): ABORT_BATTLE. */
    abortPayload[0] = 81;
    abortPayload[16] = COOP_BATTLE_ABORT_DISCONNECTED;
    DeliverRecord(COOP_BRIDGE_MESSAGE_ABORT_BATTLE, abortPayload, sizeof(abortPayload));
    EXPECT(CoopBattleRuntime_IsEngineFaulted());
    /* The fault path leaves the battle through the end callback. */
    gMain.savedCallback();
    for (i = 0; i < PARTY_SIZE; i++)
    {
        EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][i], MON_DATA_SPECIES),
                  GetMonData(&battle->original[i], MON_DATA_SPECIES));
        EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][i], MON_DATA_HP),
                  GetMonData(&battle->original[i], MON_DATA_HP));
        EXPECT_EQ(memcmp(&gParties[B_TRAINER_0][i], &battle->original[i], sizeof(struct Pokemon)), 0);
    }
    EXPECT_EQ(memcmp(gParties[B_TRAINER_0], battle->original, sizeof(battle->original)), 0);
    EXPECT(!CoopBattleRuntime_IsEngineActive());
    EXPECT_EQ(CoopFriendly_GetResult(), COOP_FRIENDLY_RESULT_NO_CONTEST);
    EXPECT(CoopBattleConsent_IsIdle());
    CoopFriendly_Finish();
    DrainOutbound();

    /* A local fault asks the server to call the battle off. */
    StartBattleAsPartner(battle, 82, COOP_BATTLE_FRIENDLY_LEVELS_AS_IS);
    CoopBattleRuntime_FailEngine();
    gMain.savedCallback();
    EXPECT_EQ(memcmp(gParties[B_TRAINER_0], battle->original, sizeof(battle->original)), 0);
    EXPECT_EQ(CoopFriendly_GetResult(), COOP_FRIENDLY_RESULT_NO_CONTEST);
    EXPECT(TakeOutbound(COOP_BRIDGE_MESSAGE_BATTLE_ABORT_REQUEST, &message));
    EXPECT_EQ(message.payload[0], 82);
    CoopFriendly_Finish();
    Free(battle);
    EndFixture();
}
