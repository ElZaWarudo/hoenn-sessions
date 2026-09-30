#include "global.h"
#include "battle.h"
#include "battle_end_turn.h"
#include "battle_main.h"
#include "battle_message.h"
#include "battle_setup.h"
#include "battle_util.h"
#include "coop/battle_runtime.h"
#include "coop/generated_regional_identities.h"
#include "coop/region.h"
#include "fieldmap.h"
#include "malloc.h"
#include "pokemon.h"
#include "random.h"
#include "string_util.h"
#include "constants/battle.h"
#include "constants/battle_end_turn.h"
#include "constants/battle_partner.h"
#include "constants/characters.h"
#include "constants/moves.h"
#include "constants/opponents.h"
#include "constants/region_map_sections.h"
#include "test/battle.h"

/* C3: battler references in co-op trainer battles. Both ROMs are simulated
 * on one: each member's view arms the trainer engine from its own manifest
 * and stages its own Pokemon at B_TRAINER_0 / battler 0 and the partner's at
 * B_TRAINER_2 / battler 2, so member 1's view numbers member 0's battler 2.
 * Effects are set with canonical battlers (member 0 owns 0, member 1 owns 2)
 * translated to each view's local IDs, as the engine would store them. */

#define MEMBER_MONS 2
#define CANON_BATTLE_ID 72

/* Local battler ID of a canonical one on the armed view (the mapping is its
 * own inverse). */
#define LOCAL(canonical) CoopBattleRuntime_CanonicalBattler(canonical)

static void SetCanonRegion(void)
{
    gMapHeader.engineRegion = COOP_MAP_ENGINE_REGION_HOENN;
    gMapHeader.regionMapSectionId = MAPSEC_ROUTE_102;
}

static void CreateOwnedMon(struct Pokemon *mon, enum Species species, u32 personality,
                           const u8 *otName, u8 otGender)
{
    CreateMon(mon, species, 20, personality, OTID_STRUCT_PRESET(1));
    SetMonData(mon, MON_DATA_OT_NAME, otName);
    SetMonData(mon, MON_DATA_OT_GENDER, &otGender);
    CalculateMonStats(mon);
}

static const u8 sMember0Name[] = _("ALICE");
static const u8 sMember1Name[] = _("BOBBY");

static void BuildCanonMembers(struct Pokemon *member0, struct Pokemon *member1, struct Pokemon *foes)
{
    CreateOwnedMon(&member0[0], SPECIES_WOBBUFFET, 301, sMember0Name, FEMALE);
    CreateOwnedMon(&member0[1], SPECIES_WYNAUT, 302, sMember0Name, FEMALE);
    CreateOwnedMon(&member1[0], SPECIES_WOBBUFFET, 401, sMember1Name, MALE);
    CreateOwnedMon(&member1[1], SPECIES_WYNAUT, 402, sMember1Name, MALE);
    CreateOwnedMon(&foes[0], SPECIES_WOBBUFFET, 501, sMember0Name, MALE);
    CreateOwnedMon(&foes[1], SPECIES_WOBBUFFET, 502, sMember0Name, MALE);
}

static void ReceiveCanonManifest(u8 slot, const struct Pokemon *local)
{
    u8 manifest[COOP_BATTLE_MANIFEST_SIZE] = {0};

    manifest[0] = CANON_BATTLE_ID;
    manifest[COOP_BATTLE_MANIFEST_KIND_OFFSET] = COOP_BATTLE_MANIFEST_KIND_TRAINER;
    manifest[COOP_BATTLE_MANIFEST_MEMBER_SLOT_OFFSET] = slot;
    manifest[COOP_BATTLE_MANIFEST_REGION_OFFSET] = COOP_REGION_HOENN;
    manifest[COOP_BATTLE_MANIFEST_TRAINER_ORDINAL_OFFSET] = (u8)COOP_TRAINER_HOENN_TRAINER_CALVIN_1_ORDINAL;
    manifest[COOP_BATTLE_MANIFEST_TRAINER_ORDINAL_OFFSET + 1] = COOP_TRAINER_HOENN_TRAINER_CALVIN_1_ORDINAL >> 8;
    EXPECT(CoopBattleRuntime_ComputePartyDigest(local, MEMBER_MONS,
                                                 &manifest[50 + slot * COOP_BATTLE_DIGEST_SIZE],
                                                 COOP_BATTLE_DIGEST_SIZE));
    EXPECT_EQ(CoopBattleRuntime_ReceiveManifest(manifest, sizeof(manifest)),
              COOP_BATTLE_INBOUND_ACCEPTED);
}

static void ArmCanonView(u8 slot, const struct Pokemon *member0, const struct Pokemon *member1)
{
    const struct Pokemon *peer = slot == 0 ? member1 : member0;
    u8 chunk[COOP_BATTLE_PARTY_SNAPSHOT_SIZE];
    u8 id[COOP_BATTLE_ID_SIZE] = {CANON_BATTLE_ID};
    u8 i;

    SetCanonRegion();
    CoopBattleRuntime_Init();
    CoopBattleRuntime_OnSessionReady(CANON_BATTLE_ID);
    ReceiveCanonManifest(slot, slot == 0 ? member0 : member1);
    for (i = 0; i < MEMBER_MONS; i++)
    {
        memset(chunk, 0, sizeof(chunk));
        chunk[0] = CANON_BATTLE_ID;
        chunk[16] = i;
        chunk[17] = i;
        chunk[18] = MEMBER_MONS;
        chunk[19] = COOP_BATTLE_PARTY_MON_SIZE;
        memcpy(chunk + 20, &peer[i], sizeof(peer[i]));
        EXPECT_EQ(CoopBattleRuntime_ReceivePeerPartyChunk(chunk, sizeof(chunk)),
                  COOP_BATTLE_INBOUND_ACCEPTED);
    }
    EXPECT(CoopBattleRuntime_ArmEngine(id));
    EXPECT(CoopBattleRuntime_IsTrainerEngine());
}

static rng_value_t sCanonRng;
static rng_value_t sCanonRng2;
static struct BattleScriptsStack sCanonScripts;
static struct BattleCallbacksStack sCanonCallbacks;

/* The state one member's ROM holds, all four battlers on the field at equal
 * speed (every tie is decided by the order under test). */
static void StageCanonView(u8 slot, const struct Pokemon *member0, const struct Pokemon *member1,
                           const struct Pokemon *foes)
{
    const struct Pokemon *local = slot == 0 ? member0 : member1;
    const struct Pokemon *peer = slot == 0 ? member1 : member0;
    u8 i;

    for (i = 0; i < PARTY_SIZE; i++)
    {
        ZeroMonData(&gParties[B_TRAINER_0][i]);
        ZeroMonData(&gParties[B_TRAINER_1][i]);
        ZeroMonData(&gParties[B_TRAINER_2][i]);
        ZeroMonData(&gParties[B_TRAINER_3][i]);
    }
    for (i = 0; i < MEMBER_MONS; i++)
    {
        gParties[B_TRAINER_0][i] = local[i];
        gParties[B_TRAINER_2][i] = peer[i];
        gParties[B_TRAINER_1][i] = foes[i];
    }
    gPartiesCount[B_TRAINER_0] = MEMBER_MONS;
    gPartiesCount[B_TRAINER_1] = MEMBER_MONS;
    gPartiesCount[B_TRAINER_2] = MEMBER_MONS;
    gPartiesCount[B_TRAINER_3] = 0;
    gBattleTypeFlags = BATTLE_TYPE_MULTI | BATTLE_TYPE_INGAME_PARTNER
        | BATTLE_TYPE_DOUBLE | BATTLE_TYPE_TRAINER;
    gBattlersCount = MAX_BATTLERS_COUNT;
    for (i = 0; i < MAX_BATTLERS_COUNT; i++)
        gBattlerPositions[i] = i;
    gBattlerPartyIndexes[0] = 0;
    gBattlerPartyIndexes[1] = 0;
    gBattlerPartyIndexes[2] = 0;
    gBattlerPartyIndexes[3] = 1;
    memset(gBattleMons, 0, sizeof(gBattleMons));
    PokemonToBattleMon(&gParties[B_TRAINER_0][0], &gBattleMons[0]);
    PokemonToBattleMon(&gParties[B_TRAINER_1][0], &gBattleMons[1]);
    PokemonToBattleMon(&gParties[B_TRAINER_2][0], &gBattleMons[2]);
    PokemonToBattleMon(&gParties[B_TRAINER_1][1], &gBattleMons[3]);
    for (i = 0; i < MAX_BATTLERS_COUNT; i++)
    {
        gBattleMons[i].speed = 50;
        gBattleMons[i].hp = gBattleMons[i].maxHP = 100;
    }
    memset(gBattleStruct, 0, sizeof(*gBattleStruct));
    memset(gProtectStructs, 0, sizeof(gProtectStructs));
    memset(gSpecialStatuses, 0, sizeof(gSpecialStatuses));
    memset(gSideStatuses, 0, sizeof(gSideStatuses));
    memset(gSideTimers, 0, sizeof(gSideTimers));
    gSideTimers[B_SIDE_PLAYER].stickyWebBattlerId = 0xFF;
    gSideTimers[B_SIDE_OPPONENT].stickyWebBattlerId = 0xFF;
    memset(&gFieldTimers, 0, sizeof(gFieldTimers));
    memset(gLastMoves, 0, sizeof(gLastMoves));
    memset(&sCanonScripts, 0, sizeof(sCanonScripts));
    memset(&sCanonCallbacks, 0, sizeof(sCanonCallbacks));
    gBattleResources->battleScriptsStack = &sCanonScripts;
    gBattleResources->battleCallbackStack = &sCanonCallbacks;
    gFieldStatuses = 0;
    gBattleWeather = 0;
    gAbsentBattlerFlags = 0;
    gBattleOutcome = 0;
    gHitMarker = 0;
    gBattleTurnCounter = 0;
    gRngValue = sCanonRng;
    gRng2Value = sCanonRng2;
}

/* ------------------------------------------------------------------------
 * Digest: every stored battler reference hashes as the canonical battler.
 */

typedef void (*CanonEffect)(enum BattlerId atk, enum BattlerId def);

static void Effect_LeechSeed(enum BattlerId atk, enum BattlerId def)
{
    gBattleMons[def].volatiles.leechSeed = LEECHSEEDED_BY(atk);
}

static void Effect_EncoreDisable(enum BattlerId atk, enum BattlerId def)
{
    gBattleMons[def].volatiles.encoredMove = MOVE_TACKLE;
    gBattleMons[def].volatiles.encoredMovePos = 0;
    gBattleMons[def].volatiles.encoreTimer = 3;
    gBattleMons[def].volatiles.disabledMove = MOVE_GROWL;
    gBattleMons[def].volatiles.disableTimer = 4;
}

static void Effect_Attract(enum BattlerId atk, enum BattlerId def)
{
    gBattleMons[def].volatiles.infatuation = INFATUATED_WITH(atk);
}

static void Effect_FutureSight(enum BattlerId atk, enum BattlerId def)
{
    gBattleStruct->futureSight[def].move = MOVE_FUTURE_SIGHT;
    gBattleStruct->futureSight[def].counter = 3;
    gBattleStruct->futureSight[def].battlerIndex = atk;
    gBattleStruct->futureSight[def].partyIndex = gBattlerPartyIndexes[atk];
}

static void Effect_Wish(enum BattlerId atk, enum BattlerId def)
{
    gBattleStruct->wish[atk].counter = 2;
    gBattleStruct->wish[atk].partyId = gBattlerPartyIndexes[atk];
}

static void Effect_Yawn(enum BattlerId atk, enum BattlerId def)
{
    gBattleMons[def].volatiles.yawn = 2;
}

static void Effect_PerishSong(enum BattlerId atk, enum BattlerId def)
{
    enum BattlerId i;

    for (i = 0; i < MAX_BATTLERS_COUNT; i++)
    {
        gBattleMons[i].volatiles.perishSong = TRUE;
        gBattleMons[i].volatiles.perishSongTimer = 3;
    }
}

/* Wrap/Bind plus Mean Look/Block (trapping) and Octolock, Syrup Bomb. */
static void Effect_Trap(enum BattlerId atk, enum BattlerId def)
{
    gBattleMons[def].volatiles.wrapped = TRUE;
    gBattleMons[def].volatiles.wrappedBy = atk;
    gBattleMons[def].volatiles.wrapTurns = 4;
    gBattleMons[def].volatiles.wrappedMove = MOVE_WRAP;
    gBattleMons[def].volatiles.escapePrevention = TRUE;
    gBattleMons[def].volatiles.battlerPreventingEscape = atk;
    gBattleMons[def].volatiles.octolock = TRUE;
    gBattleMons[def].volatiles.octolockedBy = atk;
    gBattleMons[def].volatiles.syrupBomb = TRUE;
    gBattleMons[def].volatiles.stickySyrupedBy = atk;
}

/* Lock-On/Mind Reader and Sky Drop, stored on the attacker. */
static void Effect_LockOn(enum BattlerId atk, enum BattlerId def)
{
    gBattleMons[atk].volatiles.lockOn = 2;
    gBattleMons[atk].volatiles.battlerWithSureHit = def + 1;
    gBattleMons[atk].volatiles.skyDropTarget = def + 1;
}

/* Counter/Mirror Coat sources, Revenge/Avalanche, Instruct's last target,
 * the per-target flags of a spread move and Follow Me / Sticky Web. */
static void Effect_Sources(enum BattlerId atk, enum BattlerId def)
{
    gProtectStructs[def].physicalDmg = 31;
    gProtectStructs[def].physicalBattlerId = atk;
    gProtectStructs[def].specialDmg = 12;
    gProtectStructs[def].specialBattlerId = atk;
    gProtectStructs[def].revengeDoubled |= 1u << atk;
    gSpecialStatuses[def].backUpTarget = atk + 1;
    gLastMoves[atk] = MOVE_TACKLE;
    gBattleStruct->battlerState[atk].lastMoveTarget = def;
    gBattleStruct->battlerState[atk].targetsDone[def] = TRUE;
    PushHazardTypeToQueue(GetBattlerSide(def), HAZARDS_STICKY_WEB);
    gSideTimers[GetBattlerSide(def)].stickyWebBattlerId = atk;
    gSideTimers[GetBattlerSide(atk)].followmeTimer = 1;
    gSideTimers[GetBattlerSide(atk)].followmeTarget = atk;
}

static void Effect_ZMove(enum BattlerId atk, enum BattlerId def)
{
    gBattleStruct->zmove.baseMoves[atk] = MOVE_TACKLE;
    gBattleStruct->zmove.healReplacement |= 1u << atk;
}

static void Effect_Dynamax(enum BattlerId atk, enum BattlerId def)
{
    gBattleStruct->dynamax.dynamaxTurns[atk] = 3;
    gBattleStruct->dynamax.baseMoves[atk] = MOVE_TACKLE;
}

struct CanonCase
{
    CanonEffect apply;
    bool8 attackerStored; /* the state names the attacker */
};

static const struct CanonCase sCanonCases[] =
{
    {Effect_LeechSeed, TRUE},
    {Effect_EncoreDisable, FALSE},
    {Effect_Attract, TRUE},
    {Effect_FutureSight, TRUE},
    {Effect_Wish, TRUE},
    {Effect_Yawn, FALSE},
    {Effect_PerishSong, FALSE},
    {Effect_Trap, TRUE},
    {Effect_LockOn, TRUE},
    {Effect_Sources, TRUE},
    {Effect_ZMove, TRUE},
    {Effect_Dynamax, TRUE},
};

/* Canonical (attacker, target) pairs: each member on a foe and on the other
 * member, and a foe on each member. */
static const u8 sCanonPairs[][2] =
{
    {0, 1}, {2, 1}, {0, 2}, {2, 0}, {1, 0}, {3, 2}, {2, 3},
};

static void CanonDigest(u8 view, const struct Pokemon *member0, const struct Pokemon *member1,
                        const struct Pokemon *foes, CanonEffect apply, u8 atk, u8 def, u8 *digest)
{
    ArmCanonView(view, member0, member1);
    StageCanonView(view, member0, member1, foes);
    if (apply != NULL)
        apply(LOCAL(atk), LOCAL(def));
    EXPECT(CoopBattleRuntime_ComputeBattleDigest(digest, COOP_BATTLE_DIGEST_SIZE));
    CoopBattleRuntime_DisarmEngine();
}

TEST("Cloud Coop trainer digest hashes battler references canonically on both ROMs")
{
    struct Pokemon member0[MEMBER_MONS], member1[MEMBER_MONS], foes[MEMBER_MONS];
    struct BattleStruct *savedBattleStruct = gBattleStruct;
    struct BattleResources *savedResources = gBattleResources;
    u8 digests[2][COOP_BATTLE_DIGEST_SIZE];
    u8 other[COOP_BATTLE_DIGEST_SIZE];
    u8 c, p, view;

    gBattleStruct = AllocZeroed(sizeof(*gBattleStruct));
    gBattleResources = AllocZeroed(sizeof(*gBattleResources));
    BuildCanonMembers(member0, member1, foes);
    sCanonRng = gRngValue;
    sCanonRng2 = gRng2Value;

    /* Nothing set: unset references (0) must not name a member. */
    for (view = 0; view < 2; view++)
        CanonDigest(view, member0, member1, foes, NULL, 0, 0, digests[view]);
    EXPECT_EQ(memcmp(digests[0], digests[1], COOP_BATTLE_DIGEST_SIZE), 0);

    for (c = 0; c < ARRAY_COUNT(sCanonCases); c++)
    {
        for (p = 0; p < ARRAY_COUNT(sCanonPairs); p++)
        {
            for (view = 0; view < 2; view++)
                CanonDigest(view, member0, member1, foes, sCanonCases[c].apply,
                            sCanonPairs[p][0], sCanonPairs[p][1], digests[view]);
            EXPECT_EQ(memcmp(digests[0], digests[1], COOP_BATTLE_DIGEST_SIZE), 0);
        }
        /* The reference is hashed, not dropped: the same effect on the same
         * target from the other member hashes differently. */
        if (sCanonCases[c].attackerStored)
        {
            CanonDigest(1, member0, member1, foes, sCanonCases[c].apply, 0, 1, digests[0]);
            CanonDigest(1, member0, member1, foes, sCanonCases[c].apply, 2, 1, other);
            EXPECT_NE(memcmp(digests[0], other, COOP_BATTLE_DIGEST_SIZE), 0);
        }
    }

    Free(gBattleResources);
    Free(gBattleStruct);
    gBattleResources = savedResources;
    gBattleStruct = savedBattleStruct;
    CoopBattleRuntime_Init();
}

/* ------------------------------------------------------------------------
 * Behaviour: ordering and picks by battler go through the canonical battler.
 */

TEST("Cloud Coop speed ties, random targets and spread-move targets are canonical on both ROMs")
{
    struct Pokemon member0[MEMBER_MONS], member1[MEMBER_MONS], foes[MEMBER_MONS];
    struct BattleStruct *savedBattleStruct = gBattleStruct;
    struct BattleResources *savedResources = gBattleResources;
    enum BattlerId order[MAX_BATTLERS_COUNT];
    u8 randomTarget[2][8];
    u8 view, i, n;

    gBattleStruct = AllocZeroed(sizeof(*gBattleStruct));
    gBattleResources = AllocZeroed(sizeof(*gBattleResources));
    BuildCanonMembers(member0, member1, foes);
    sCanonRng = gRngValue;
    sCanonRng2 = gRng2Value;
    for (view = 0; view < 2; view++)
    {
        ArmCanonView(view, member0, member1);
        StageCanonView(view, member0, member1, foes);

        /* Equal speeds: canonical order, whatever the input order. */
        for (i = 0; i < MAX_BATTLERS_COUNT; i++)
            order[i] = i;
        SortBattlersBySpeed(order, FALSE);
        for (i = 0; i < MAX_BATTLERS_COUNT; i++)
            EXPECT_EQ(LOCAL(order[i]), i);
        for (i = 0; i < MAX_BATTLERS_COUNT; i++)
            order[i] = MAX_BATTLERS_COUNT - 1 - i;
        SortBattlersBySpeed(order, TRUE);
        for (i = 0; i < MAX_BATTLERS_COUNT; i++)
            EXPECT_EQ(LOCAL(order[i]), i);
        /* A faster member still goes first. */
        gBattleMons[LOCAL(2)].speed = 60;
        for (i = 0; i < MAX_BATTLERS_COUNT; i++)
            order[i] = i;
        SortBattlersBySpeed(order, FALSE);
        EXPECT_EQ(LOCAL(order[0]), 2);
        EXPECT_EQ(LOCAL(order[1]), 0);
        gBattleMons[LOCAL(2)].speed = 50;

        /* A foe's random target: the same member for the same roll. */
        for (n = 0; n < ARRAY_COUNT(randomTarget[0]); n++)
            randomTarget[view][n] = LOCAL(SetRandomTarget(B_BATTLER_1));

        /* A foe's spread move: members in canonical order, then its ally. */
        gBattlerAttacker = B_BATTLER_1;
        gBattlerTarget = MAX_BATTLERS_COUNT;
        EXPECT_EQ(LOCAL(GetNextTarget(TARGET_FOES_AND_ALLY, FALSE)), 0);
        gBattleStruct->battlerState[gBattlerAttacker].targetsDone[LOCAL(0)] = TRUE;
        EXPECT_EQ(LOCAL(GetNextTarget(TARGET_FOES_AND_ALLY, FALSE)), 2);
        gBattleStruct->battlerState[gBattlerAttacker].targetsDone[LOCAL(2)] = TRUE;
        EXPECT_EQ(LOCAL(GetNextTarget(TARGET_FOES_AND_ALLY, FALSE)), 3);
        /* The per-target slots of a foe's move name the same members. */
        EXPECT_EQ(LOCAL(GetTargetBySlot(B_BATTLER_1, B_BATTLER_2)), 0);
        EXPECT_EQ(LOCAL(GetTargetBySlot(B_BATTLER_1, B_BATTLER_3)), 2);
        EXPECT_EQ(LOCAL(GetTargetBySlot(LOCAL(2), B_BATTLER_2)), 1);
        CoopBattleRuntime_DisarmEngine();
    }
    EXPECT_EQ(memcmp(randomTarget[0], randomTarget[1], sizeof(randomTarget[0])), 0);

    Free(gBattleResources);
    Free(gBattleStruct);
    gBattleResources = savedResources;
    gBattleStruct = savedBattleStruct;
    CoopBattleRuntime_Init();
}

struct EndTurnStep
{
    u8 state;
    u8 target;
    u8 attacker;
};

#define END_TURN_STEPS 24

/* Runs the end of turn up to Perish Song, recording each effect that starts
 * a script: the state, and the canonical target and attacker. */
static u8 RunCanonEndTurn(struct EndTurnStep *steps)
{
    u8 count = 0;

    gBattleStruct->eventState.endTurn = ENDTURN_ORDER;
    gBattleStruct->eventState.endTurnBattler = 0;
    while (count < END_TURN_STEPS && gBattleStruct->eventState.endTurn <= ENDTURN_PERISH_SONG)
    {
        sCanonScripts.size = 0;
        sCanonCallbacks.size = 0;
        if (!DoEndTurnEffects())
            break;
        steps[count].state = gBattleStruct->eventState.endTurn;
        steps[count].target = LOCAL(gBattlerTarget);
        steps[count].attacker = LOCAL(gBattlerAttacker);
        count++;
    }
    return count;
}

TEST("Cloud Coop end-of-turn effects landing the same turn resolve in the same order on both ROMs")
{
    struct Pokemon member0[MEMBER_MONS], member1[MEMBER_MONS], foes[MEMBER_MONS];
    struct BattleStruct *savedBattleStruct = gBattleStruct;
    struct BattleResources *savedResources = gBattleResources;
    struct EndTurnStep steps[2][END_TURN_STEPS];
    u8 counts[2];
    u8 digests[2][COOP_BATTLE_DIGEST_SIZE];
    u8 view, i, futureSights, leechSeeds;

    gBattleStruct = AllocZeroed(sizeof(*gBattleStruct));
    gBattleResources = AllocZeroed(sizeof(*gBattleResources));
    BuildCanonMembers(member0, member1, foes);
    sCanonRng = gRngValue;
    sCanonRng2 = gRng2Value;
    for (view = 0; view < 2; view++)
    {
        ArmCanonView(view, member0, member1);
        StageCanonView(view, member0, member1, foes);
        /* Two Future Sights on the members (from the foes) and a Doom Desire
         * from member 0, all landing now; Leech Seeds both ways; a Wish;
         * both members yawning; equal speeds everywhere. */
        Effect_FutureSight(LOCAL(1), LOCAL(0));
        gBattleStruct->futureSight[LOCAL(0)].counter = 1;
        Effect_FutureSight(LOCAL(3), LOCAL(2));
        gBattleStruct->futureSight[LOCAL(2)].counter = 1;
        Effect_FutureSight(LOCAL(0), LOCAL(1));
        gBattleStruct->futureSight[LOCAL(1)].move = MOVE_DOOM_DESIRE;
        gBattleStruct->futureSight[LOCAL(1)].counter = 1;
        Effect_LeechSeed(LOCAL(3), LOCAL(0));
        Effect_LeechSeed(LOCAL(1), LOCAL(2));
        Effect_LeechSeed(LOCAL(2), LOCAL(1));
        Effect_Wish(LOCAL(2), LOCAL(2));
        gBattleStruct->wish[LOCAL(2)].counter = 1;
        gBattleMons[LOCAL(2)].hp = 40;
        gBattleMons[LOCAL(0)].volatiles.yawn = 1;
        gBattleMons[LOCAL(2)].volatiles.yawn = 1;
        counts[view] = RunCanonEndTurn(steps[view]);
        EXPECT(CoopBattleRuntime_ComputeBattleDigest(digests[view], COOP_BATTLE_DIGEST_SIZE));
        CoopBattleRuntime_DisarmEngine();
    }

    EXPECT_EQ(counts[0], counts[1]);
    for (i = 0; i < counts[0]; i++)
    {
        EXPECT_EQ(steps[0][i].state, steps[1][i].state);
        EXPECT_EQ(steps[0][i].target, steps[1][i].target);
        EXPECT_EQ(steps[0][i].attacker, steps[1][i].attacker);
    }
    EXPECT_EQ(memcmp(digests[0], digests[1], COOP_BATTLE_DIGEST_SIZE), 0);

    /* The ties went in canonical order, with the stored attackers. */
    futureSights = leechSeeds = 0;
    for (i = 0; i < counts[0]; i++)
    {
        if (steps[0][i].state == ENDTURN_FUTURE_SIGHT)
        {
            static const u8 expected[][2] = {{0, 1}, {1, 0}, {2, 3}};

            EXPECT_LT(futureSights, ARRAY_COUNT(expected));
            EXPECT_EQ(steps[0][i].target, expected[futureSights][0]);
            EXPECT_EQ(steps[0][i].attacker, expected[futureSights][1]);
            futureSights++;
        }
        else if (steps[0][i].state == ENDTURN_LEECH_SEED)
        {
            /* The seeded battler drains to its seeder. */
            static const u8 expected[][2] = {{3, 0}, {2, 1}, {1, 2}};

            EXPECT_LT(leechSeeds, ARRAY_COUNT(expected));
            EXPECT_EQ(steps[0][i].target, expected[leechSeeds][0]);
            EXPECT_EQ(steps[0][i].attacker, expected[leechSeeds][1]);
            leechSeeds++;
        }
    }
    EXPECT_EQ(futureSights, 3);
    EXPECT_EQ(leechSeeds, 3);

    Free(gBattleResources);
    Free(gBattleStruct);
    gBattleResources = savedResources;
    gBattleStruct = savedBattleStruct;
    CoopBattleRuntime_Init();
}

/* ------------------------------------------------------------------------
 * The partner's name.
 */

TEST("Cloud Coop trainer battle messages name the partner player, not the in-game partner")
{
    static const u8 sPartnerWithClass[] = {PLACEHOLDER_BEGIN, B_TXT_PARTNER_NAME_WITH_CLASS, EOS};
    static const u8 sPartnerName[] = {PLACEHOLDER_BEGIN, B_TXT_PARTNER_NAME, EOS};
    static const u8 sAttackerWithClass[] = {PLACEHOLDER_BEGIN, B_TXT_ATK_TRAINER_NAME_WITH_CLASS, EOS};
    struct Pokemon member0[MEMBER_MONS], member1[MEMBER_MONS], foes[MEMBER_MONS];
    struct BattleStruct *savedBattleStruct = gBattleStruct;
    struct BattleResources *savedResources = gBattleResources;
    u16 savedPartner = gPartnerTrainerId;
    u8 text[64];
    u8 view;

    gBattleStruct = AllocZeroed(sizeof(*gBattleStruct));
    gBattleResources = AllocZeroed(sizeof(*gBattleResources));
    BuildCanonMembers(member0, member1, foes);
    sCanonRng = gRngValue;
    sCanonRng2 = gRng2Value;
    gPartnerTrainerId = TRAINER_PARTNER(PARTNER_STEVEN);
    for (view = 0; view < 2; view++)
    {
        const u8 *peerName = view == 0 ? sMember1Name : sMember0Name;

        ArmCanonView(view, member0, member1);
        StageCanonView(view, member0, member1, foes);
        BattleStringExpandPlaceholders(sPartnerWithClass, text, sizeof(text));
        EXPECT_EQ(StringCompare(text, peerName), 0);
        BattleStringExpandPlaceholders(sPartnerName, text, sizeof(text));
        EXPECT_EQ(StringCompare(text, peerName), 0);
        /* "<partner> used <item>!" and other attacker-trainer messages. */
        gBattlerAttacker = B_BATTLER_2;
        BattleStringExpandPlaceholders(sAttackerWithClass, text, sizeof(text));
        EXPECT_EQ(StringCompare(text, peerName), 0);
        /* The name fits the 7-character player name buffer. */
        EXPECT_LE(StringLength(text), PLAYER_NAME_LENGTH);
        EXPECT_EQ(CoopBattleRuntime_PartnerGender(), view == 0 ? MALE : FEMALE);
        CoopBattleRuntime_DisarmEngine();

        /* A vanilla in-game partner battle keeps the partner trainer. */
        EXPECT(!CoopBattleRuntime_IsTrainerEngine());
        EXPECT(!CoopBattleRuntime_CopyPartnerName(text));
    }

    gPartnerTrainerId = savedPartner;
    Free(gBattleResources);
    Free(gBattleStruct);
    gBattleResources = savedResources;
    gBattleStruct = savedBattleStruct;
    CoopBattleRuntime_Init();
}
