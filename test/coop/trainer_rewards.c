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

/* A9: Hoenn story battles. */
TEST("Cloud Coop story table maps each story trainer to its battle and kind")
{
    EXPECT_EQ(CoopTrainerRewards_GetStoryBattle(TRAINER_MAY_ROUTE_103_TREECKO), COOP_STORY_RIVAL_ROUTE103);
    EXPECT_EQ(CoopTrainerRewards_GetStoryBattle(TRAINER_BRENDAN_ROUTE_103_MUDKIP), COOP_STORY_RIVAL_ROUTE103);
    EXPECT_EQ(CoopTrainerRewards_GetStoryBattle(TRAINER_BRENDAN_ROUTE_119_TORCHIC), COOP_STORY_RIVAL_ROUTE119);
    EXPECT_EQ(CoopTrainerRewards_GetStoryBattle(TRAINER_WALLY_VR_1), COOP_STORY_WALLY_VICTORY_ROAD);
    EXPECT_EQ(CoopTrainerRewards_GetStoryBattle(TRAINER_SIDNEY), COOP_STORY_SIDNEY);
    EXPECT_EQ(CoopTrainerRewards_GetStoryBattle(TRAINER_WALLACE), COOP_STORY_CHAMPION_WALLACE);
    EXPECT_EQ(CoopTrainerRewards_GetStoryKind(COOP_STORY_MATT_AQUA_HIDEOUT), COOP_STORY_KIND_GRANT);
    EXPECT_EQ(CoopTrainerRewards_GetStoryKind(COOP_STORY_RIVAL_RUSTBORO), COOP_STORY_KIND_HELPER);
    EXPECT_EQ(CoopTrainerRewards_GetStoryKind(COOP_STORY_ARCHIE_SEAFLOOR_CAVERN), COOP_STORY_KIND_HELPER);
    EXPECT_EQ(CoopTrainerRewards_GetStoryKind(COOP_STORY_CHAMPION_WALLACE), COOP_STORY_KIND_HELPER);
    EXPECT_EQ(CoopTrainerRewards_GetStoryKind(COOP_STORY_ADMINS), COOP_STORY_KIND_ORDINARY);
    EXPECT_EQ(CoopTrainerRewards_GetStoryBattle(TRAINER_TABITHA_MT_CHIMNEY), COOP_STORY_ADMINS);
    /* Rematch entries and route trainers are not story battles. */
    EXPECT_EQ(CoopTrainerRewards_GetStoryBattle(TRAINER_WALLY_VR_3), COOP_STORY_NONE);
    EXPECT_EQ(CoopTrainerRewards_GetStoryBattle(TRAINER_CALVIN_1), COOP_STORY_NONE);
    EXPECT_EQ(CoopTrainerRewards_GetStoryBattle(TRAINER_ROXANNE_1), COOP_STORY_NONE);
    EXPECT_EQ(CoopTrainerRewards_GetStoryBattle(TRAINER_NONE), COOP_STORY_NONE);
    /* Party size: story leaders, admins, the Elite Four and the champion keep
     * their team like gym leaders; rivals and grunts keep the cap. */
    EXPECT(CoopTrainerRewards_IsFullTeamClass(TRAINER_CLASS_LEADER));
    EXPECT(CoopTrainerRewards_IsFullTeamClass(TRAINER_CLASS_ELITE_FOUR));
    EXPECT(CoopTrainerRewards_IsFullTeamClass(TRAINER_CLASS_CHAMPION));
    EXPECT(CoopTrainerRewards_IsFullTeamClass(TRAINER_CLASS_MAGMA_LEADER));
    EXPECT(CoopTrainerRewards_IsFullTeamClass(TRAINER_CLASS_AQUA_ADMIN));
    EXPECT(!CoopTrainerRewards_IsFullTeamClass(TRAINER_CLASS_RIVAL));
    EXPECT(!CoopTrainerRewards_IsFullTeamClass(TRAINER_CLASS_TEAM_AQUA));
    EXPECT(!CoopTrainerRewards_IsFullTeamClass(TRAINER_CLASS_YOUNGSTER));
}

TEST("Cloud Coop story requester is paid and leaves the story state to its own script")
{
    u8 staged[1] = {0};
    u32 prize = CoopTrainerRewards_GetPrizeMoney(TRAINER_MAY_ROUTE_110_TREECKO, 1);
    u16 held;

    BeginRewardFixture();
    SnapshotSave();
    VarSet(VAR_ROUTE110_STATE, 0);
    FlagClear(FLAG_HIDE_ROUTE_110_RIVAL);
    ClearTrainerFlag(TRAINER_MAY_ROUTE_110_TREECKO);
    held = CountTotalItemQuantityInBag(ITEM_DOWSING_MACHINE);
    WinWithOneFaint();
    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, TRAINER_MAY_ROUTE_110_TREECKO, staged, 1, TRUE),
              COOP_TRAINER_REWARD_PARTICIPANT);
    EXPECT(HasTrainerBeenFought(TRAINER_MAY_ROUTE_110_TREECKO));
    EXPECT_EQ(GetMoney(&gSaveBlock1Ptr->money), 1000 + prize);
    /* Route110_EventScript_MayDefeated, resumed next, does the rest. */
    EXPECT_EQ(VarGet(VAR_ROUTE110_STATE), 0);
    EXPECT(!FlagGet(FLAG_HIDE_ROUTE_110_RIVAL));
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_DOWSING_MACHINE), held);
    /* Settled once. */
    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, TRAINER_MAY_ROUTE_110_TREECKO, staged, 1, TRUE),
              COOP_TRAINER_REWARD_NONE);
    EXPECT_EQ(GetMoney(&gSaveBlock1Ptr->money), 1000 + prize);
    RestoreSave();
    EndRewardFixture();
}

TEST("Cloud Coop rival partner at the same story point gets the rival grants once")
{
    u8 staged[1] = {0};
    u32 prize = CoopTrainerRewards_GetPrizeMoney(TRAINER_BRENDAN_ROUTE_110_MUDKIP, 1);
    u16 held;

    BeginRewardFixture();
    SnapshotSave();
    VarSet(VAR_ROUTE110_STATE, 0);
    FlagClear(FLAG_HIDE_ROUTE_110_RIVAL);
    FlagClear(FLAG_HIDE_ROUTE_110_RIVAL_ON_BIKE);
    ClearTrainerFlag(TRAINER_BRENDAN_ROUTE_110_MUDKIP);
    held = CountTotalItemQuantityInBag(ITEM_DOWSING_MACHINE);
    if (held != 0)
        EXPECT(RemoveBagItem(ITEM_DOWSING_MACHINE, held));
    EXPECT(CoopTrainerRewards_IsStoryPartnerEligible(COOP_STORY_RIVAL_ROUTE110));
    WinWithOneFaint();
    /* The requester fought Brendan; this partner's own rival may differ. */
    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, TRAINER_BRENDAN_ROUTE_110_MUDKIP, staged, 1, FALSE),
              COOP_TRAINER_REWARD_STORY_PARTNER);
    EXPECT_EQ(VarGet(VAR_ROUTE110_STATE), 1);
    EXPECT(FlagGet(FLAG_HIDE_ROUTE_110_RIVAL));
    EXPECT(FlagGet(FLAG_HIDE_ROUTE_110_RIVAL_ON_BIKE));
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_DOWSING_MACHINE), 1);
    EXPECT(HasTrainerBeenFought(TRAINER_BRENDAN_ROUTE_110_MUDKIP));
    EXPECT_EQ(GetMoney(&gSaveBlock1Ptr->money), 1000 + prize);
    /* The notice shows the item that was given, once. */
    CoopTrainerRewards_LoadStoryNotice(NULL);
    EXPECT_EQ(gSpecialVar_0x8004, FALSE);
    EXPECT_EQ(gSpecialVar_0x8005, TRUE);
    EXPECT_EQ(gSpecialVar_0x8000, ITEM_DOWSING_MACHINE);
    EXPECT_EQ(gSpecialVar_0x8007, TRUE);
    CoopTrainerRewards_LoadStoryNotice(NULL);
    EXPECT_EQ(gSpecialVar_0x8005, FALSE);
    /* No double grant: the story point has passed, so a later win helps. */
    EXPECT(!CoopTrainerRewards_IsStoryPartnerEligible(COOP_STORY_RIVAL_ROUTE110));
    WinWithOneFaint();
    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, TRAINER_BRENDAN_ROUTE_110_MUDKIP, staged, 1, FALSE),
              COOP_TRAINER_REWARD_HELPER);
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_DOWSING_MACHINE), 1);
    EXPECT_EQ(GetMoney(&gSaveBlock1Ptr->money), 1000 + prize);
    RestoreSave();
    EndRewardFixture();
}

TEST("Cloud Coop rival partner behind or ahead of the story point helps")
{
    u8 staged[1] = {0};
    u32 before;
    u32 attempt;

    for (attempt = 0; attempt < 3; attempt++)
    {
        BeginRewardFixture();
        SnapshotSave();
        FlagClear(FLAG_DEFEATED_RIVAL_ROUTE103);
        FlagClear(FLAG_HIDE_ROUTE_103_RIVAL);
        VarSet(VAR_BIRCH_LAB_STATE, 3);
        ClearTrainerFlag(TRAINER_MAY_ROUTE_103_TORCHIC);
        if (attempt == 0)
            FlagSet(FLAG_DEFEATED_RIVAL_ROUTE103); // already past it
        else if (attempt == 1)
            FlagSet(FLAG_HIDE_ROUTE_103_RIVAL);    // the rival is not there yet
        WinWithOneFaint();
        before = GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_EXP);
        if (attempt == 2)
        {
            EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, TRAINER_MAY_ROUTE_103_TORCHIC, staged, 1, FALSE),
                      COOP_TRAINER_REWARD_STORY_PARTNER);
            EXPECT(FlagGet(FLAG_DEFEATED_RIVAL_ROUTE103));
            EXPECT(FlagGet(FLAG_HIDE_ROUTE_103_RIVAL));
            EXPECT_EQ(VarGet(VAR_BIRCH_LAB_STATE), 4);
            EXPECT_EQ(VarGet(VAR_OLDALE_RIVAL_STATE), 1);
            EXPECT(!FlagGet(FLAG_HIDE_OLDALE_TOWN_RIVAL));
            EXPECT(!FlagGet(FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_RIVAL));
            CoopTrainerRewards_LoadStoryNotice(NULL);
        }
        else
        {
            EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, TRAINER_MAY_ROUTE_103_TORCHIC, staged, 1, FALSE),
                      COOP_TRAINER_REWARD_HELPER);
            EXPECT_EQ(GetMoney(&gSaveBlock1Ptr->money), 1000);
            EXPECT(!HasTrainerBeenFought(TRAINER_MAY_ROUTE_103_TORCHIC));
            EXPECT_EQ(VarGet(VAR_BIRCH_LAB_STATE), 3);
        }
        EXPECT_GT(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_EXP), before);
        RestoreSave();
        EndRewardFixture();
    }
}

TEST("Cloud Coop admin partner grant is Matt's submarine escape state")
{
    u8 staged[1] = {0};

    BeginRewardFixture();
    SnapshotSave();
    FlagClear(FLAG_TEAM_AQUA_ESCAPED_IN_SUBMARINE);
    FlagClear(FLAG_HIDE_AQUA_HIDEOUT_GRUNTS);
    FlagClear(FLAG_HIDE_LILYCOVE_CITY_AQUA_GRUNTS);
    FlagClear(FLAG_HIDE_AQUA_HIDEOUT_B2F_SUBMARINE_SHADOW);
    ClearTrainerFlag(TRAINER_MATT);
    WinWithOneFaint();
    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, TRAINER_MATT, staged, 1, FALSE),
              COOP_TRAINER_REWARD_STORY_PARTNER);
    EXPECT(FlagGet(FLAG_TEAM_AQUA_ESCAPED_IN_SUBMARINE));
    EXPECT(FlagGet(FLAG_HIDE_LILYCOVE_CITY_AQUA_GRUNTS));
    EXPECT(FlagGet(FLAG_HIDE_AQUA_HIDEOUT_B2F_SUBMARINE_SHADOW));
    /* Matt's own script now shows only his post-battle text. */
    EXPECT(HasTrainerBeenFought(TRAINER_MATT));
    CoopTrainerRewards_LoadStoryNotice(NULL);
    EXPECT_EQ(gSpecialVar_0x8005, FALSE);
    RestoreSave();
    EndRewardFixture();
}

TEST("Cloud Coop leader partner grant at Mt Chimney and helper at the Magma Hideout")
{
    u8 staged[1] = {0};

    BeginRewardFixture();
    SnapshotSave();
    FlagClear(FLAG_DEFEATED_EVIL_TEAM_MT_CHIMNEY);
    FlagClear(FLAG_HIDE_MT_CHIMNEY_TEAM_MAGMA);
    FlagClear(FLAG_HIDE_MT_CHIMNEY_TEAM_AQUA);
    FlagSet(FLAG_HIDE_FALLARBOR_HOUSE_PROF_COZMO);
    FlagSet(FLAG_HIDE_MT_CHIMNEY_LAVA_COOKIE_LADY);
    ClearTrainerFlag(TRAINER_MAXIE_MT_CHIMNEY);
    WinWithOneFaint();
    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, TRAINER_MAXIE_MT_CHIMNEY, staged, 1, FALSE),
              COOP_TRAINER_REWARD_STORY_PARTNER);
    EXPECT(FlagGet(FLAG_DEFEATED_EVIL_TEAM_MT_CHIMNEY));
    EXPECT(FlagGet(FLAG_HIDE_MT_CHIMNEY_TEAM_MAGMA));
    EXPECT(FlagGet(FLAG_HIDE_MT_CHIMNEY_TEAM_AQUA));
    EXPECT(!FlagGet(FLAG_HIDE_FALLARBOR_HOUSE_PROF_COZMO));
    EXPECT(FlagGet(FLAG_HIDE_METEOR_FALLS_1F_1R_COZMO));
    EXPECT(!FlagGet(FLAG_HIDE_MT_CHIMNEY_LAVA_COOKIE_LADY));
    CoopTrainerRewards_LoadStoryNotice(NULL);

    /* The Magma Hideout battle follows Groudon's awakening, which only the
     * requester saw: the partner helps, whatever its own story point. */
    FlagClear(FLAG_GROUDON_AWAKENED_MAGMA_HIDEOUT);
    ClearTrainerFlag(TRAINER_MAXIE_MAGMA_HIDEOUT);
    SetMoney(&gSaveBlock1Ptr->money, 1000);
    WinWithOneFaint();
    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, TRAINER_MAXIE_MAGMA_HIDEOUT, staged, 1, FALSE),
              COOP_TRAINER_REWARD_HELPER);
    EXPECT(!FlagGet(FLAG_GROUDON_AWAKENED_MAGMA_HIDEOUT));
    EXPECT(!HasTrainerBeenFought(TRAINER_MAXIE_MAGMA_HIDEOUT));
    EXPECT_EQ(GetMoney(&gSaveBlock1Ptr->money), 1000);
    RestoreSave();
    EndRewardFixture();
}

TEST("Cloud Coop Elite Four partner in the same room gets only the defeated flag")
{
    u8 staged[1] = {0};
    u32 prize = CoopTrainerRewards_GetPrizeMoney(TRAINER_PHOEBE, 1);

    BeginRewardFixture();
    SnapshotSave();
    FlagSet(FLAG_DEFEATED_ELITE_4_SIDNEY);
    FlagClear(FLAG_DEFEATED_ELITE_4_PHOEBE);
    FlagClear(FLAG_DEFEATED_ELITE_4_GLACIA);
    FlagClear(FLAG_SYS_GAME_CLEAR);
    /* Still in Sidney's room (door closed behind it): not Phoebe's point. */
    VarSet(VAR_ELITE_4_STATE, 1);
    EXPECT(!CoopTrainerRewards_IsStoryPartnerEligible(COOP_STORY_PHOEBE));
    VarSet(VAR_ELITE_4_STATE, 2);
    EXPECT(CoopTrainerRewards_IsStoryPartnerEligible(COOP_STORY_PHOEBE));
    WinWithOneFaint();
    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, TRAINER_PHOEBE, staged, 1, FALSE),
              COOP_TRAINER_REWARD_STORY_PARTNER);
    EXPECT(FlagGet(FLAG_DEFEATED_ELITE_4_PHOEBE));
    EXPECT(!FlagGet(FLAG_DEFEATED_ELITE_4_GLACIA));
    EXPECT_EQ(VarGet(VAR_ELITE_4_STATE), 2); // the next room's walk-in sets 3
    EXPECT(!FlagGet(FLAG_SYS_GAME_CLEAR));
    EXPECT_EQ(GetMoney(&gSaveBlock1Ptr->money), 1000 + prize);
    CoopTrainerRewards_LoadStoryNotice(NULL); // not standing in that room here
    EXPECT_EQ(gSpecialVar_0x8004, FALSE);
    EXPECT(!CoopTrainerRewards_IsStoryPartnerEligible(COOP_STORY_PHOEBE));

    /* The champion's script runs the Hall of Fame: the partner helps. */
    SetMoney(&gSaveBlock1Ptr->money, 1000);
    ClearTrainerFlag(TRAINER_WALLACE);
    WinWithOneFaint();
    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, TRAINER_WALLACE, staged, 1, FALSE),
              COOP_TRAINER_REWARD_HELPER);
    EXPECT(!FlagGet(FLAG_SYS_GAME_CLEAR));
    EXPECT(!HasTrainerBeenFought(TRAINER_WALLACE));
    EXPECT_EQ(GetMoney(&gSaveBlock1Ptr->money), 1000);
    RestoreSave();
    EndRewardFixture();
}

TEST("Cloud Coop Wally partner grant at Victory Road and ordinary story grunts")
{
    u8 staged[1] = {0};

    BeginRewardFixture();
    SnapshotSave();
    VarSet(VAR_VICTORY_ROAD_1F_STATE, 0);
    FlagClear(FLAG_DEFEATED_WALLY_VICTORY_ROAD);
    FlagSet(FLAG_HIDE_VICTORY_ROAD_ENTRANCE_WALLY);
    ClearTrainerFlag(TRAINER_WALLY_VR_1);
    WinWithOneFaint();
    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, TRAINER_WALLY_VR_1, staged, 1, FALSE),
              COOP_TRAINER_REWARD_STORY_PARTNER);
    EXPECT(FlagGet(FLAG_DEFEATED_WALLY_VICTORY_ROAD));
    EXPECT_EQ(VarGet(VAR_VICTORY_ROAD_1F_STATE), 1);
    EXPECT(!FlagGet(FLAG_HIDE_VICTORY_ROAD_ENTRANCE_WALLY));
    EXPECT(HasTrainerBeenFought(TRAINER_WALLY_VR_1));
    CoopTrainerRewards_LoadStoryNotice(NULL);

    /* An Aqua Hideout grunt's continue script only shows text. */
    SetMoney(&gSaveBlock1Ptr->money, 1000);
    ClearTrainerFlag(TRAINER_GRUNT_AQUA_HIDEOUT_2);
    WinWithOneFaint();
    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, TRAINER_GRUNT_AQUA_HIDEOUT_2, staged, 1, FALSE),
              COOP_TRAINER_REWARD_PARTICIPANT);
    EXPECT(HasTrainerBeenFought(TRAINER_GRUNT_AQUA_HIDEOUT_2));
    WinWithOneFaint();
    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, TRAINER_GRUNT_AQUA_HIDEOUT_2, staged, 1, FALSE),
              COOP_TRAINER_REWARD_HELPER);
    RestoreSave();
    EndRewardFixture();
}
