#include "global.h"
#include "battle.h"
#include "battle_script_commands.h"
#include "battle_setup.h"
#include "battle_util.h"
#include "caps.h"
#include "coop/trainer_rewards.h"
#include "data.h"
#include "event_data.h"
#include "item.h"
#include "malloc.h"
#include "mastery.h"
#include "money.h"
#include "pokemon.h"
#include "constants/battle.h"
#include "constants/battle_setup.h"
#include "constants/items.h"
#include "constants/opponents.h"
#include "constants/rematches.h"
#include "constants/trainers.h"
#include "constants/vars.h"
#include "test/test.h"

/* The reward module in isolation: a simulated co-op battle records faints
 * against the staged local mons, then the "restored" full party receives
 * the rewards. test/coop/trainer_encounter.c drives the same path through
 * the real co-op end-of-battle callback. */

#define REWARD_TRAINER TRAINER_CALVIN_1

struct RewardFixture
{
    struct Pokemon player[PARTY_SIZE];
    struct Pokemon enemy[PARTY_SIZE];
    u8 player_count;
    u32 money;
    bool8 flag;
    u8 battlers;
    u16 party_indexes[MAX_BATTLERS_COUNT];
    u8 positions[MAX_BATTLERS_COUNT];
    u8 absent;
    u8 leveled;
};

static struct RewardFixture *sFixture;

static void BeginRewardFixture(void)
{
    u32 i;

    sFixture = AllocZeroed(sizeof(*sFixture));
    EXPECT(sFixture != NULL);
    memcpy(sFixture->player, gParties[B_TRAINER_0], sizeof(sFixture->player));
    memcpy(sFixture->enemy, gParties[B_TRAINER_1], sizeof(sFixture->enemy));
    sFixture->player_count = gPartiesCount[B_TRAINER_0];
    sFixture->money = GetMoney(&gSaveBlock1Ptr->money);
    sFixture->flag = HasTrainerBeenFought(REWARD_TRAINER);
    sFixture->battlers = gBattlersCount;
    memcpy(sFixture->party_indexes, gBattlerPartyIndexes, sizeof(sFixture->party_indexes));
    memcpy(sFixture->positions, gBattlerPositions, sizeof(sFixture->positions));
    sFixture->absent = gAbsentBattlerFlags;
    sFixture->leveled = gLeveledUpInBattle;

    for (i = 0; i < PARTY_SIZE; i++)
    {
        ZeroMonData(&gParties[B_TRAINER_0][i]);
        ZeroMonData(&gParties[B_TRAINER_1][i]);
    }
    SetMoney(&gSaveBlock1Ptr->money, 1000);
    ClearTrainerFlag(REWARD_TRAINER);
    gLeveledUpInBattle = 0;
}

static void EndRewardFixture(void)
{
    (void)CoopTrainerRewards_Apply(FALSE, TRAINER_NONE, NULL, 0, FALSE);
    memcpy(gParties[B_TRAINER_0], sFixture->player, sizeof(sFixture->player));
    memcpy(gParties[B_TRAINER_1], sFixture->enemy, sizeof(sFixture->enemy));
    gPartiesCount[B_TRAINER_0] = sFixture->player_count;
    SetMoney(&gSaveBlock1Ptr->money, sFixture->money);
    if (sFixture->flag)
        SetTrainerFlag(REWARD_TRAINER);
    else
        ClearTrainerFlag(REWARD_TRAINER);
    gBattlersCount = sFixture->battlers;
    memcpy(gBattlerPartyIndexes, sFixture->party_indexes, sizeof(sFixture->party_indexes));
    memcpy(gBattlerPositions, sFixture->positions, sizeof(sFixture->positions));
    gAbsentBattlerFlags = sFixture->absent;
    gLeveledUpInBattle = sFixture->leveled;
    Free(sFixture);
    sFixture = NULL;
}

static void CreateUsableMon(struct Pokemon *mon, enum Species species, u8 level, u32 personality)
{
    CreateMon(mon, species, level, personality, OTID_STRUCT_PRESET(1));
    CalculateMonStats(mon);
}

/* The staged sides of a co-op double: local battler 0 and partner battler 2
 * both lead with their own party slot 0; the opponents with slots 0 and 1. */
static void SimulateBattleStart(void)
{
    u32 i;

    gBattlersCount = MAX_BATTLERS_COUNT;
    for (i = 0; i < MAX_BATTLERS_COUNT; i++)
        gBattlerPositions[i] = i;
    gBattlerPartyIndexes[0] = 0;
    gBattlerPartyIndexes[1] = 0;
    gBattlerPartyIndexes[2] = 0;
    gBattlerPartyIndexes[3] = 1;
    gAbsentBattlerFlags = 0;
    CoopTrainerRewards_Begin();
    ResetSentPokesToOpponentValue();
}

/* Cmd_getexp's arithmetic for one sent-in mon against one opponent. */
static u32 ExpectedSentInExp(const struct Pokemon *getter, enum Species fainted, u8 faintedLevel)
{
    struct Pokemon copy = *getter;
    s32 exp = gSpeciesInfo[fainted].expYield * faintedLevel;

    if (B_SCALED_EXP >= GEN_5 && B_SCALED_EXP != GEN_6)
        exp /= 5;
    else
        exp /= 7;
    if (B_TRAINER_EXP_MULTIPLIER <= GEN_7)
        exp = (exp * 150) / 100;
    exp = GetSoftLevelCapExpValue(GetMonData(&copy, MON_DATA_LEVEL), exp);
    ApplyMonExperienceMultipliers(&exp, &copy, faintedLevel);
    return exp;
}

TEST("Cloud Coop trainer rewards give a participant the flag, prize and deferred EXP")
{
    struct Pokemon fought, benched, outside, other;
    struct Pokemon battleParty[PARTY_SIZE];
    u8 staged[2] = {1, 3};
    u32 expected;
    u32 before;

    BeginRewardFixture();
    CreateUsableMon(&fought, SPECIES_CHARMANDER, 5, 1);
    CreateUsableMon(&benched, SPECIES_TREECKO, 5, 2);
    CreateUsableMon(&outside, SPECIES_PIDGEY, 5, 3);
    CreateUsableMon(&other, SPECIES_MUDKIP, 5, 4);
    CreateUsableMon(&gParties[B_TRAINER_1][0], SPECIES_POOCHYENA, 20, 5);
    CreateUsableMon(&gParties[B_TRAINER_1][1], SPECIES_ZIGZAGOON, 20, 6);

    /* In battle the two staged mons sit in slots 0 and 1. */
    gParties[B_TRAINER_0][0] = fought;
    gParties[B_TRAINER_0][1] = benched;
    SimulateBattleStart();
    /* The partner's slot-1 mon comes in: same bit position, not ours. */
    gBattlerPartyIndexes[2] = 1;
    UpdateSentPokesToOpponentValue(2);
    memcpy(battleParty, gParties[B_TRAINER_0], sizeof(battleParty));
    CoopTrainerRewards_RecordFaint(1);
    CoopTrainerRewards_RecordFaint(1); // the fainted-action pass repeats
    EXPECT_EQ(CoopTrainerRewards_TestGetFaintRecord(0), COOP_TRAINER_REWARD_FAINTED | 0x01);
    EXPECT_EQ(CoopTrainerRewards_TestGetFaintRecord(1), 0);
    /* Nothing hashed by the battle digest changed. */
    EXPECT_EQ(memcmp(battleParty, gParties[B_TRAINER_0], sizeof(battleParty)), 0);
    CoopTrainerRewards_OnBattleWon(1);

    /* After the battle the full party is back with staged mons at 1 and 3. */
    gParties[B_TRAINER_0][0] = outside;
    gParties[B_TRAINER_0][1] = fought;
    gParties[B_TRAINER_0][2] = other;
    gParties[B_TRAINER_0][3] = benched;
    ZeroMonData(&gParties[B_TRAINER_0][4]);
    ZeroMonData(&gParties[B_TRAINER_0][5]);
    expected = ExpectedSentInExp(&fought, SPECIES_POOCHYENA, 20);
    EXPECT_GT(expected, 0);
    before = GetMonData(&fought, MON_DATA_EXP);

    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, REWARD_TRAINER, staged, 2, FALSE),
              COOP_TRAINER_REWARD_PARTICIPANT);
    EXPECT(HasTrainerBeenFought(REWARD_TRAINER));
    EXPECT_EQ(GetMoney(&gSaveBlock1Ptr->money),
              1000 + CoopTrainerRewards_GetPrizeMoney(REWARD_TRAINER, 1));
    EXPECT_GT(CoopTrainerRewards_GetPrizeMoney(REWARD_TRAINER, 1), 0);
    EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][1], MON_DATA_EXP), before + expected);
    EXPECT_GT(GetMonData(&gParties[B_TRAINER_0][1], MON_DATA_LEVEL), 5);
    EXPECT(gLeveledUpInBattle & (1u << 1));
    EXPECT(CoopTrainerRewards_HasPendingEvolutions());
    /* Staged but never sent in, and never staged: no EXP. */
    EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][3], MON_DATA_EXP), GetMonData(&benched, MON_DATA_EXP));
    EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_EXP), GetMonData(&outside, MON_DATA_EXP));
    EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][2], MON_DATA_EXP), GetMonData(&other, MON_DATA_EXP));

    /* Settled once: a resumed script or re-entered callback pays nothing. */
    before = GetMonData(&gParties[B_TRAINER_0][1], MON_DATA_EXP);
    EXPECT(!CoopTrainerRewards_TestIsArmed());
    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, REWARD_TRAINER, staged, 2, FALSE), COOP_TRAINER_REWARD_NONE);
    EXPECT_EQ(GetMoney(&gSaveBlock1Ptr->money),
              1000 + CoopTrainerRewards_GetPrizeMoney(REWARD_TRAINER, 1));
    EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][1], MON_DATA_EXP), before);
    EndRewardFixture();
}

TEST("Cloud Coop trainer rewards make a player who already beat the trainer a helper")
{
    u8 staged[1] = {0};
    u32 before;

    BeginRewardFixture();
    SetTrainerFlag(REWARD_TRAINER);
    CreateUsableMon(&gParties[B_TRAINER_0][0], SPECIES_CHARMANDER, 5, 1);
    CreateUsableMon(&gParties[B_TRAINER_1][0], SPECIES_POOCHYENA, 20, 5);
    before = GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_EXP);
    SimulateBattleStart();
    CoopTrainerRewards_RecordFaint(1);
    CoopTrainerRewards_OnBattleWon(2);

    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, REWARD_TRAINER, staged, 1, FALSE),
              COOP_TRAINER_REWARD_HELPER);
    EXPECT(HasTrainerBeenFought(REWARD_TRAINER));
    EXPECT_EQ(GetMoney(&gSaveBlock1Ptr->money), 1000);
    /* Helpers still earn EXP for what their mons fought. */
    EXPECT_GT(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_EXP), before);
    EndRewardFixture();
}

TEST("Cloud Coop trainer rewards give nothing for a loss or an abort")
{
    u8 staged[1] = {0};
    struct Pokemon party;
    u32 attempt;

    for (attempt = 0; attempt < 2; attempt++)
    {
        BeginRewardFixture();
        CreateUsableMon(&gParties[B_TRAINER_0][0], SPECIES_CHARMANDER, 5, 1);
        CreateUsableMon(&gParties[B_TRAINER_1][0], SPECIES_POOCHYENA, 20, 5);
        party = gParties[B_TRAINER_0][0];
        SimulateBattleStart();
        CoopTrainerRewards_RecordFaint(1);
        /* attempt 0: completed loss; attempt 1: aborted, never won. */
        if (attempt == 0)
            CoopTrainerRewards_OnBattleWon(1);
        EXPECT_EQ(CoopTrainerRewards_Apply(FALSE, REWARD_TRAINER, staged, 1, FALSE),
                  COOP_TRAINER_REWARD_NONE);
        EXPECT(!HasTrainerBeenFought(REWARD_TRAINER));
        EXPECT_EQ(GetMoney(&gSaveBlock1Ptr->money), 1000);
        EXPECT_EQ(memcmp(&party, &gParties[B_TRAINER_0][0], sizeof(party)), 0);
        EXPECT_EQ(gLeveledUpInBattle, 0);
        /* The battle is settled: a late "win" cannot pay either. */
        EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, REWARD_TRAINER, staged, 1, FALSE),
                  COOP_TRAINER_REWARD_NONE);
        EXPECT_EQ(GetMoney(&gSaveBlock1Ptr->money), 1000);
        EndRewardFixture();
    }
}

TEST("Cloud Coop trainer rewards share EXP with a staged EXP Share holder")
{
    u8 staged[2] = {0, 1};
    u16 item = ITEM_EXP_SHARE;
    u32 before;

    BeginRewardFixture();
    CreateUsableMon(&gParties[B_TRAINER_0][0], SPECIES_CHARMANDER, 5, 1);
    CreateUsableMon(&gParties[B_TRAINER_0][1], SPECIES_TREECKO, 5, 2);
    SetMonData(&gParties[B_TRAINER_0][1], MON_DATA_HELD_ITEM, &item);
    CreateUsableMon(&gParties[B_TRAINER_1][0], SPECIES_POOCHYENA, 20, 5);
    before = GetMonData(&gParties[B_TRAINER_0][1], MON_DATA_EXP);
    SimulateBattleStart();
    CoopTrainerRewards_RecordFaint(1);
    EXPECT_EQ(CoopTrainerRewards_TestGetFaintRecord(0),
              COOP_TRAINER_REWARD_FAINTED | 0x01 | (0x02 << COOP_TRAINER_REWARD_SHARE_SHIFT));
    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, REWARD_TRAINER, staged, 2, FALSE),
              COOP_TRAINER_REWARD_PARTICIPANT);
    EXPECT_GT(GetMonData(&gParties[B_TRAINER_0][1], MON_DATA_EXP), before);
    EndRewardFixture();
}

TEST("Cloud Coop trainer rewards level up, learn into free slots and schedule evolution")
{
    const struct LevelUpMove *learnset = GetSpeciesLevelUpLearnset(SPECIES_CHARMANDER);
    u16 expectedMoves[MAX_MON_MOVES] = {MOVE_SCRATCH, MOVE_NONE, MOVE_NONE, MOVE_NONE};
    u16 none = MOVE_NONE;
    u16 scratch = MOVE_SCRATCH;
    u8 staged[1] = {0};
    struct Pokemon *mon = &gParties[B_TRAINER_0][0];
    bool32 canStopEvo = TRUE;
    u32 exp;
    u32 oldLevel;
    u32 newLevel;
    u32 known = 1;
    u32 i;
    u32 j;

    BeginRewardFixture();
    CreateUsableMon(mon, SPECIES_CHARMANDER, 15, 1);
    exp = gExperienceTables[gSpeciesInfo[SPECIES_CHARMANDER].growthRate][15];
    SetMonData(mon, MON_DATA_EXP, &exp);
    CalculateMonStats(mon);
    SetMonData(mon, MON_DATA_MOVE1, &scratch);
    for (i = 1; i < MAX_MON_MOVES; i++)
        SetMonData(mon, MON_DATA_MOVE1 + i, &none);
    oldLevel = GetMonData(mon, MON_DATA_LEVEL);
    EXPECT_EQ(oldLevel, 15);
    CreateUsableMon(&gParties[B_TRAINER_1][0], SPECIES_BLISSEY, 50, 5);
    SimulateBattleStart();
    CoopTrainerRewards_RecordFaint(1);
    /* Still level 15 until the battle is over. */
    EXPECT_EQ(GetMonData(mon, MON_DATA_LEVEL), 15);

    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, REWARD_TRAINER, staged, 1, FALSE),
              COOP_TRAINER_REWARD_PARTICIPANT);
    newLevel = GetMonData(mon, MON_DATA_LEVEL);
    EXPECT_GT(newLevel, 16);
    EXPECT_EQ(gLeveledUpInBattle, 1u << 0);
    EXPECT_EQ(GetEvolutionTargetSpecies(mon, EVO_MODE_BATTLE_ONLY, 0, NULL, &canStopEvo, CHECK_EVO),
              SPECIES_CHARMELEON);

    /* Each crossed level's moves fill the free slots in order; a full
     * moveset keeps its moves (no replace prompt). */
    for (i = 0; learnset[i].move != LEVEL_UP_MOVE_END && known < MAX_MON_MOVES; i++)
    {
        bool32 duplicate = FALSE;

        if (learnset[i].level <= oldLevel || learnset[i].level > newLevel)
            continue;
        for (j = 0; j < known; j++)
            if (expectedMoves[j] == learnset[i].move)
                duplicate = TRUE;
        if (!duplicate)
            expectedMoves[known++] = learnset[i].move;
    }
    for (i = 0; i < MAX_MON_MOVES; i++)
        EXPECT_EQ(GetMonData(mon, MON_DATA_MOVE1 + i), expectedMoves[i]);
    EndRewardFixture();
}

/* A7/A8 settle roles from the whole save (rematch bookkeeping, badges, bag,
 * gym trainer flags), so these tests put SaveBlock1 and SaveBlock3 back
 * byte for byte. */
static struct SaveBlock1 *sSavedBlock1;
static struct SaveBlock3 *sSavedBlock3;

static void SnapshotSave(void)
{
    sSavedBlock1 = Alloc(sizeof(*gSaveBlock1Ptr));
    sSavedBlock3 = Alloc(sizeof(*gSaveBlock3Ptr));
    EXPECT(sSavedBlock1 != NULL && sSavedBlock3 != NULL);
    memcpy(sSavedBlock1, gSaveBlock1Ptr, sizeof(*gSaveBlock1Ptr));
    memcpy(sSavedBlock3, gSaveBlock3Ptr, sizeof(*gSaveBlock3Ptr));
}

static void RestoreSave(void)
{
    memcpy(gSaveBlock1Ptr, sSavedBlock1, sizeof(*gSaveBlock1Ptr));
    memcpy(gSaveBlock3Ptr, sSavedBlock3, sizeof(*gSaveBlock3Ptr));
    Free(sSavedBlock3);
    Free(sSavedBlock1);
    sSavedBlock1 = NULL;
    sSavedBlock3 = NULL;
}

/* One armed co-op battle in which the local lead knocks out the opponent. */
static void WinWithOneFaint(void)
{
    ZeroMonData(&gParties[B_TRAINER_0][0]);
    ZeroMonData(&gParties[B_TRAINER_1][0]);
    CreateUsableMon(&gParties[B_TRAINER_0][0], SPECIES_CHARMANDER, 5, 1);
    CreateUsableMon(&gParties[B_TRAINER_1][0], SPECIES_POOCHYENA, 20, 5);
    SimulateBattleStart();
    CoopTrainerRewards_RecordFaint(1);
    CoopTrainerRewards_OnBattleWon(1);
}

static void SetHoennBadges(u8 mask)
{
    u8 i;

    for (i = 0; i < COOP_HOENN_GYM_COUNT; i++)
    {
        if (mask & (1u << i))
            FlagSet(FLAG_BADGE01_GET + i);
        else
            FlagClear(FLAG_BADGE01_GET + i);
    }
}

TEST("Cloud Coop rematch entries resolve to their first-battle trainer")
{
    u16 base = TRAINER_NONE;

    EXPECT(BattleSetup_GetRematchBaseTrainer(TRAINER_CALVIN_2, &base));
    EXPECT_EQ(base, TRAINER_CALVIN_1);
    EXPECT(BattleSetup_GetRematchBaseTrainer(TRAINER_CALVIN_5, &base));
    EXPECT_EQ(base, TRAINER_CALVIN_1);
    EXPECT(BattleSetup_GetRematchBaseTrainer(TRAINER_ROXANNE_3, &base));
    EXPECT_EQ(base, TRAINER_ROXANNE_1);
    /* Cindy's row skips CINDY_2, which stays an ordinary trainer. */
    EXPECT(BattleSetup_GetRematchBaseTrainer(TRAINER_CINDY_3, &base));
    EXPECT_EQ(base, TRAINER_CINDY_1);
    EXPECT(!BattleSetup_GetRematchBaseTrainer(TRAINER_CINDY_2, NULL));
    /* First battles, ordinary trainers and the Elite Four rows are not. */
    EXPECT(!BattleSetup_GetRematchBaseTrainer(TRAINER_CALVIN_1, NULL));
    EXPECT(!BattleSetup_GetRematchBaseTrainer(TRAINER_TIANA, NULL));
    EXPECT(!BattleSetup_GetRematchBaseTrainer(TRAINER_SIDNEY, NULL));
    EXPECT(!BattleSetup_GetRematchBaseTrainer(TRAINER_NONE, NULL));
    EXPECT(IsRematchBattleMode(TRAINER_BATTLE_REMATCH));
    EXPECT(IsRematchBattleMode(TRAINER_BATTLE_REMATCH_DOUBLE));
    EXPECT(!IsRematchBattleMode(TRAINER_BATTLE_SINGLE));
}

TEST("Cloud Coop rematch win records the vanilla rematch bookkeeping on the requester")
{
    u8 staged[1] = {0};
    u32 before;
    u32 prize = CoopTrainerRewards_GetPrizeMoney(TRAINER_CALVIN_2, 1);

    BeginRewardFixture();
    SnapshotSave();
    SetTrainerFlag(TRAINER_CALVIN_1);
    ClearTrainerFlag(TRAINER_CALVIN_2);
    gSaveBlock1Ptr->trainerRematches[REMATCH_CALVIN] = 1; // Calvin wants a rematch
    WinWithOneFaint();
    before = GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_EXP);

    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, TRAINER_CALVIN_2, staged, 1, TRUE),
              COOP_TRAINER_REWARD_PARTICIPANT);
    /* CB2_EndRematchBattle's win: the rematch trainer's flag is set and
     * the "wants a rematch" state is cleared. */
    EXPECT(HasTrainerBeenFought(TRAINER_CALVIN_2));
    EXPECT_EQ(gSaveBlock1Ptr->trainerRematches[REMATCH_CALVIN], 0);
    EXPECT(HasTrainerBeenFought(TRAINER_CALVIN_1));
    EXPECT_GT(prize, 0);
    EXPECT_EQ(GetMoney(&gSaveBlock1Ptr->money), 1000 + prize);
    EXPECT_GT(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_EXP), before);
    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, TRAINER_CALVIN_2, staged, 1, TRUE),
              COOP_TRAINER_REWARD_NONE);
    EXPECT_EQ(GetMoney(&gSaveBlock1Ptr->money), 1000 + prize);

    /* At the last stage the rematch trainer is already beaten; vanilla still
     * pays for the rematch and so does co-op. */
    SetTrainerFlag(TRAINER_CALVIN_5);
    gSaveBlock1Ptr->trainerRematches[REMATCH_CALVIN] = 4;
    WinWithOneFaint();
    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, TRAINER_CALVIN_5, staged, 1, TRUE),
              COOP_TRAINER_REWARD_PARTICIPANT);
    EXPECT_EQ(GetMoney(&gSaveBlock1Ptr->money),
              1000 + prize + CoopTrainerRewards_GetPrizeMoney(TRAINER_CALVIN_5, 1));
    EXPECT_EQ(gSaveBlock1Ptr->trainerRematches[REMATCH_CALVIN], 0);
    RestoreSave();
    EndRewardFixture();
}

TEST("Cloud Coop rematch partner who beat the first battle earns only the prize")
{
    u8 staged[1] = {0};
    u32 prize = CoopTrainerRewards_GetPrizeMoney(TRAINER_CALVIN_2, 1);

    BeginRewardFixture();
    SnapshotSave();
    SetTrainerFlag(TRAINER_CALVIN_1);
    ClearTrainerFlag(TRAINER_CALVIN_2);
    gSaveBlock1Ptr->trainerRematches[REMATCH_CALVIN] = 1;
    WinWithOneFaint();
    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, TRAINER_CALVIN_2, staged, 1, FALSE),
              COOP_TRAINER_REWARD_PARTICIPANT);
    EXPECT_EQ(GetMoney(&gSaveBlock1Ptr->money), 1000 + prize);
    /* The partner's own match-call state is not its rematch to record. */
    EXPECT(!HasTrainerBeenFought(TRAINER_CALVIN_2));
    EXPECT_EQ(gSaveBlock1Ptr->trainerRematches[REMATCH_CALVIN], 1);
    RestoreSave();
    EndRewardFixture();
}

TEST("Cloud Coop rematch partner who never beat the first battle is a helper")
{
    u8 staged[1] = {0};
    u32 before;

    BeginRewardFixture();
    SnapshotSave();
    ClearTrainerFlag(TRAINER_CALVIN_1);
    ClearTrainerFlag(TRAINER_CALVIN_2);
    gSaveBlock1Ptr->trainerRematches[REMATCH_CALVIN] = 0;
    WinWithOneFaint();
    before = GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_EXP);
    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, TRAINER_CALVIN_2, staged, 1, FALSE),
              COOP_TRAINER_REWARD_HELPER);
    EXPECT_EQ(GetMoney(&gSaveBlock1Ptr->money), 1000);
    EXPECT(!HasTrainerBeenFought(TRAINER_CALVIN_1));
    EXPECT(!HasTrainerBeenFought(TRAINER_CALVIN_2));
    EXPECT_EQ(gSaveBlock1Ptr->trainerRematches[REMATCH_CALVIN], 0);
    EXPECT_GT(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_EXP), before);
    RestoreSave();
    EndRewardFixture();
}

TEST("Cloud Coop gym table follows the badge order and the leader rematches")
{
    static const u16 sLeaders[COOP_HOENN_GYM_COUNT] = {
        TRAINER_ROXANNE_1, TRAINER_BRAWLY_1, TRAINER_WATTSON_1, TRAINER_FLANNERY_1,
        TRAINER_NORMAN_1, TRAINER_WINONA_1, TRAINER_TATE_AND_LIZA_1, TRAINER_JUAN_1,
    };
    u8 i;

    for (i = 0; i < COOP_HOENN_GYM_COUNT; i++)
    {
        EXPECT_EQ(CoopTrainerRewards_GetHoennGym(sLeaders[i]), i);
        EXPECT(CoopTrainerRewards_IsHoennGymLeader(sLeaders[i]));
        EXPECT(CoopTrainerRewards_GetGymNoticeScript(sLeaders[i]) != NULL);
    }
    /* Rematches are leaders for the party size, but grant no badge. */
    EXPECT(CoopTrainerRewards_IsHoennGymLeader(TRAINER_JUAN_5));
    EXPECT_EQ(CoopTrainerRewards_GetHoennGym(TRAINER_JUAN_5), COOP_HOENN_GYM_NONE);
    EXPECT(CoopTrainerRewards_GetGymNoticeScript(TRAINER_JUAN_5) == NULL);
    EXPECT(!CoopTrainerRewards_IsHoennGymLeader(TRAINER_CALVIN_1));
    EXPECT(!CoopTrainerRewards_IsHoennGymLeader(TRAINER_SIDNEY));
    EXPECT_EQ(CoopTrainerRewards_GetHoennGym(TRAINER_NONE), COOP_HOENN_GYM_NONE);
}

TEST("Cloud Coop gym partner eligibility follows the badge order")
{
    BeginRewardFixture();
    SnapshotSave();
    SetHoennBadges(0);
    EXPECT(CoopTrainerRewards_IsGymPartnerEligible(0));
    EXPECT(!CoopTrainerRewards_IsGymPartnerEligible(1));
    SetHoennBadges(0x0F); // four badges: Norman is next
    EXPECT(CoopTrainerRewards_IsGymPartnerEligible(4));
    EXPECT(!CoopTrainerRewards_IsGymPartnerEligible(3)); // already earned
    EXPECT(!CoopTrainerRewards_IsGymPartnerEligible(5)); // Norman skipped
    SetHoennBadges(0x0B); // Flannery missing
    EXPECT(!CoopTrainerRewards_IsGymPartnerEligible(4));
    EXPECT(!CoopTrainerRewards_IsGymPartnerEligible(2)); // has a later badge
    SetHoennBadges(0x13); // Stone, Knuckle and Balance: ahead of Wattson
    EXPECT(!CoopTrainerRewards_IsGymPartnerEligible(2));
    EXPECT(!CoopTrainerRewards_IsGymPartnerEligible(COOP_HOENN_GYM_NONE));
    RestoreSave();
    EndRewardFixture();
}

TEST("Cloud Coop gym partner at the same story point gets the vanilla gym grants once")
{
    static const u16 sDewfordTrainers[] = {
        TRAINER_TAKAO, TRAINER_JOCELYN, TRAINER_LAURA,
        TRAINER_BRENDEN, TRAINER_CRISTIAN, TRAINER_LILITH,
    };
    u8 staged[1] = {0};
    u16 held;
    u32 prize = CoopTrainerRewards_GetPrizeMoney(TRAINER_BRAWLY_1, 1);
    u32 i;

    BeginRewardFixture();
    SnapshotSave();
    SetHoennBadges(0x01); // Stone Badge only: Brawly is next
    FlagClear(FLAG_DEFEATED_DEWFORD_GYM);
    FlagClear(FLAG_RECEIVED_TM_BULK_UP);
    FlagClear(FLAG_ENABLE_BRAWLY_MATCH_CALL);
    FlagClear(FLAG_ENABLE_ROXANNE_FIRST_CALL);
    VarSet(VAR_PETALBURG_GYM_STATE, 3);
    VarSet(VAR_ROXANNE_CALL_STEP_COUNTER, 9);
    ClearTrainerFlag(TRAINER_BRAWLY_1);
    for (i = 0; i < ARRAY_COUNT(sDewfordTrainers); i++)
        ClearTrainerFlag(sDewfordTrainers[i]);
    held = CountTotalItemQuantityInBag(ITEM_TM_BULK_UP);
    if (held != 0)
        EXPECT(RemoveBagItem(ITEM_TM_BULK_UP, held));
    WinWithOneFaint();

    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, TRAINER_BRAWLY_1, staged, 1, FALSE),
              COOP_TRAINER_REWARD_GYM_PARTNER);
    /* DewfordTown_Gym_EventScript_BrawlyDefeated and GiveBulkUp. */
    EXPECT(FlagGet(FLAG_BADGE02_GET));
    EXPECT(FlagGet(FLAG_DEFEATED_DEWFORD_GYM));
    EXPECT_EQ(VarGet(VAR_PETALBURG_GYM_STATE), 4);
    for (i = 0; i < ARRAY_COUNT(sDewfordTrainers); i++)
        EXPECT(HasTrainerBeenFought(sDewfordTrainers[i]));
    EXPECT(FlagGet(FLAG_ENABLE_BRAWLY_MATCH_CALL));
    EXPECT(FlagGet(FLAG_ENABLE_ROXANNE_FIRST_CALL));
    EXPECT_EQ(VarGet(VAR_ROXANNE_CALL_STEP_COUNTER), 0);
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_TM_BULK_UP), 1);
    EXPECT(FlagGet(FLAG_RECEIVED_TM_BULK_UP));
    /* CB2_EndTrainerBattle's share: the leader's flag and the prize. */
    EXPECT(HasTrainerBeenFought(TRAINER_BRAWLY_1));
    EXPECT_GT(prize, 0);
    EXPECT_EQ(GetMoney(&gSaveBlock1Ptr->money), 1000 + prize);
    /* Later badges are untouched. */
    EXPECT(!FlagGet(FLAG_BADGE03_GET));

    /* The notice shows the TM that was actually given. */
    gSpecialVar_0x8000 = gSpecialVar_0x8001 = gSpecialVar_0x8007 = 0;
    CoopTrainerRewards_LoadGymNoticeItem(NULL);
    EXPECT_EQ(gSpecialVar_0x8000, ITEM_TM_BULK_UP);
    EXPECT_EQ(gSpecialVar_0x8001, 1);
    EXPECT_EQ(gSpecialVar_0x8007, TRUE);

    /* No double grant: settled, and a later win of the same gym helps. */
    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, TRAINER_BRAWLY_1, staged, 1, FALSE),
              COOP_TRAINER_REWARD_NONE);
    WinWithOneFaint();
    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, TRAINER_BRAWLY_1, staged, 1, FALSE),
              COOP_TRAINER_REWARD_HELPER);
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_TM_BULK_UP), 1);
    EXPECT_EQ(VarGet(VAR_PETALBURG_GYM_STATE), 4);
    EXPECT_EQ(GetMoney(&gSaveBlock1Ptr->money), 1000 + prize);
    RestoreSave();
    EndRewardFixture();
}

TEST("Cloud Coop Norman partner grant sets up the walk to Wally's house")
{
    u8 staged[1] = {0};

    BeginRewardFixture();
    SnapshotSave();
    SetHoennBadges(0x0F);
    FlagClear(FLAG_DEFEATED_PETALBURG_GYM);
    FlagSet(FLAG_HIDE_PETALBURG_CITY_WALLYS_DAD);
    VarSet(VAR_PETALBURG_GYM_STATE, 6);
    VarSet(VAR_PETALBURG_CITY_STATE, 3);
    ClearTrainerFlag(TRAINER_NORMAN_1);
    WinWithOneFaint();
    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, TRAINER_NORMAN_1, staged, 1, FALSE),
              COOP_TRAINER_REWARD_GYM_PARTNER);
    EXPECT(FlagGet(FLAG_BADGE05_GET));
    EXPECT(FlagGet(FLAG_DEFEATED_PETALBURG_GYM));
    EXPECT_EQ(VarGet(VAR_PETALBURG_GYM_STATE), 7);
    EXPECT_EQ(VarGet(VAR_PETALBURG_CITY_STATE), 4);
    EXPECT(!FlagGet(FLAG_HIDE_PETALBURG_CITY_WALLYS_DAD));
    EXPECT(FlagGet(FLAG_HIDE_MR_BRINEY_DEWFORD_TOWN)); // EventScript_HideMrBriney
    EXPECT(FlagGet(FLAG_RECEIVED_TM_FACADE));
    RestoreSave();
    EndRewardFixture();
}

TEST("Cloud Coop gym partner behind or ahead of the story helps without grants")
{
    u8 staged[1] = {0};
    u8 attempt;
    u32 before;

    for (attempt = 0; attempt < 2; attempt++)
    {
        BeginRewardFixture();
        SnapshotSave();
        /* 0: no Stone Badge yet (behind); 1: already has the Dynamo Badge. */
        SetHoennBadges(attempt == 0 ? 0x00 : 0x07);
        FlagClear(FLAG_DEFEATED_MAUVILLE_GYM);
        FlagClear(FLAG_RECEIVED_TM_SHOCK_WAVE);
        ClearTrainerFlag(TRAINER_WATTSON_1);
        WinWithOneFaint();
        before = GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_EXP);
        EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, TRAINER_WATTSON_1, staged, 1, FALSE),
                  COOP_TRAINER_REWARD_HELPER);
        EXPECT_EQ(FlagGet(FLAG_BADGE03_GET), attempt == 1);
        EXPECT(!FlagGet(FLAG_DEFEATED_MAUVILLE_GYM));
        EXPECT(!FlagGet(FLAG_RECEIVED_TM_SHOCK_WAVE));
        EXPECT(!HasTrainerBeenFought(TRAINER_WATTSON_1));
        EXPECT_EQ(GetMoney(&gSaveBlock1Ptr->money), 1000);
        EXPECT_GT(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_EXP), before);
        RestoreSave();
        EndRewardFixture();
    }
}

TEST("Cloud Coop gym requester is paid and leaves the badge to the leader's script")
{
    u8 staged[1] = {0};
    u16 held;
    u32 prize = CoopTrainerRewards_GetPrizeMoney(TRAINER_ROXANNE_1, 1);

    BeginRewardFixture();
    SnapshotSave();
    SetHoennBadges(0x00);
    FlagClear(FLAG_DEFEATED_RUSTBORO_GYM);
    FlagClear(FLAG_RECEIVED_TM_ROCK_TOMB);
    ClearTrainerFlag(TRAINER_ROXANNE_1);
    ClearTrainerFlag(TRAINER_JOSH);
    held = CountTotalItemQuantityInBag(ITEM_TM_ROCK_TOMB);
    WinWithOneFaint();
    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, TRAINER_ROXANNE_1, staged, 1, TRUE),
              COOP_TRAINER_REWARD_PARTICIPANT);
    EXPECT(HasTrainerBeenFought(TRAINER_ROXANNE_1));
    EXPECT_EQ(GetMoney(&gSaveBlock1Ptr->money), 1000 + prize);
    /* RustboroCity_Gym_EventScript_RoxanneDefeated still gives all of it. */
    EXPECT(!FlagGet(FLAG_BADGE01_GET));
    EXPECT(!FlagGet(FLAG_DEFEATED_RUSTBORO_GYM));
    EXPECT(!FlagGet(FLAG_RECEIVED_TM_ROCK_TOMB));
    EXPECT(!HasTrainerBeenFought(TRAINER_JOSH));
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_TM_ROCK_TOMB), held);
    RestoreSave();
    EndRewardFixture();
}
