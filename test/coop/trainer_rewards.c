#include "global.h"
#include "battle.h"
#include "battle_script_commands.h"
#include "battle_setup.h"
#include "battle_util.h"
#include "caps.h"
#include "coop/trainer_rewards.h"
#include "data.h"
#include "event_data.h"
#include "malloc.h"
#include "mastery.h"
#include "money.h"
#include "pokemon.h"
#include "constants/battle.h"
#include "constants/items.h"
#include "constants/trainers.h"
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
    (void)CoopTrainerRewards_Apply(FALSE, TRAINER_NONE, NULL, 0);
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

    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, REWARD_TRAINER, staged, 2),
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
    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, REWARD_TRAINER, staged, 2), COOP_TRAINER_REWARD_NONE);
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

    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, REWARD_TRAINER, staged, 1),
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
        EXPECT_EQ(CoopTrainerRewards_Apply(FALSE, REWARD_TRAINER, staged, 1),
                  COOP_TRAINER_REWARD_NONE);
        EXPECT(!HasTrainerBeenFought(REWARD_TRAINER));
        EXPECT_EQ(GetMoney(&gSaveBlock1Ptr->money), 1000);
        EXPECT_EQ(memcmp(&party, &gParties[B_TRAINER_0][0], sizeof(party)), 0);
        EXPECT_EQ(gLeveledUpInBattle, 0);
        /* The battle is settled: a late "win" cannot pay either. */
        EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, REWARD_TRAINER, staged, 1),
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
    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, REWARD_TRAINER, staged, 2),
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

    EXPECT_EQ(CoopTrainerRewards_Apply(TRUE, REWARD_TRAINER, staged, 1),
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
