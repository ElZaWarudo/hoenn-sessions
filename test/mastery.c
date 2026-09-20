#include "global.h"
#include "pokemon.h"
#include "mastery.h"
#include "daycare.h"
#include "event_data.h"
#include "string_util.h"
#include "text.h"
#include "test/test.h"

TEST("Mastery experience survives boxed storage without changing level or stats")
{
    struct Pokemon mon, restored;
    u32 experience = 9000000;
    u32 stats[NUM_STATS];
    u8 nickname[POKEMON_NAME_LENGTH + 1];

    CreateMonWithIVs(&mon, SPECIES_CHARIZARD, MAX_LEVEL, 0, OTID_STRUCT_PRESET(0), 31);
    SetMonData(&mon, MON_DATA_NICKNAME, COMPOUND_STRING("ABCDEFGHIJKL"));
    for (u32 i = 0; i < NUM_STATS; i++)
        stats[i] = GetMonData(&mon, MON_DATA_MAX_HP + i);
    SetMonData(&mon, MON_DATA_EXP, &experience);
    BoxMonToMon(&mon.box, &restored);

    EXPECT_EQ(GetMonData(&restored, MON_DATA_EXP), experience);
    EXPECT_EQ(GetMonData(&restored, MON_DATA_LEVEL), MAX_LEVEL);
    GetMonData(&restored, MON_DATA_NICKNAME, nickname);
    EXPECT_EQ(nickname[10], COMPOUND_STRING("K")[0]);
    EXPECT_EQ(nickname[11], COMPOUND_STRING("L")[0]);
    for (u32 i = 0; i < NUM_STATS; i++)
        EXPECT_EQ(GetMonData(&restored, MON_DATA_MAX_HP + i), stats[i]);
}

TEST("Mastery ranks advance Alpha then Beta then Omega and stop at 300")
{
    struct Pokemon mon;
    u8 text[24];
    u32 level = 0;
    const u8 *expected = NULL;
    PARAMETRIZE { level = 1; expected = COMPOUND_STRING("Alpha 1"); }
    PARAMETRIZE { level = 100; expected = COMPOUND_STRING("Alpha 100"); }
    PARAMETRIZE { level = 101; expected = COMPOUND_STRING("Beta 1"); }
    PARAMETRIZE { level = 200; expected = COMPOUND_STRING("Beta 100"); }
    PARAMETRIZE { level = 201; expected = COMPOUND_STRING("Omega 1"); }
    PARAMETRIZE { level = 300; expected = COMPOUND_STRING("Omega 100"); }
    CreateMonWithIVs(&mon, SPECIES_CHARIZARD, MAX_LEVEL, 0, OTID_STRUCT_PRESET(0), 31);
    AddMonExperience(&mon, level * MASTERY_EXP_PER_LEVEL - 1);
    EXPECT_EQ(GetMonMasteryLevel(&mon), level - 1);
    AddMonExperience(&mon, 1);
    EXPECT_EQ(GetMonMasteryLevel(&mon), level);
    FormatMasteryLevel(text, level, FALSE);
    EXPECT_EQ(StringCompare(text, expected), 0);
    EXPECT_LE(GetStringWidth(FONT_SMALL, text, 0), 55); // Summary gender starts at x=57.
    AddMonExperience(&mon, 0xFFFFFFFF);
    EXPECT_EQ(GetMonMasteryLevel(&mon), MAX_MASTERY_LEVEL);
    EXPECT(!CanMonGainExperience(&mon));
    EXPECT_EQ(GetMonData(&mon, MON_DATA_LEVEL), MAX_LEVEL);
    EXPECT(!TryIncrementMonLevel(&mon));
    EXPECT_EQ(GetMonMasteryLevel(&mon), MAX_MASTERY_LEVEL);
}

TEST("Mastery starts after level 100 on every growth curve")
{
    enum Species species = SPECIES_NONE;
    for (u32 growth = GROWTH_MEDIUM_FAST; growth <= GROWTH_SLOW; growth++)
    {
        for (u32 candidate = SPECIES_BULBASAUR; candidate < NUM_SPECIES; candidate++)
        {
            if (IsSpeciesEnabled(candidate) && gSpeciesInfo[candidate].growthRate == growth)
            {
                PARAMETRIZE { species = candidate; }
                break;
            }
        }
    }
    struct Pokemon mon;
    CreateMonWithIVs(&mon, species, MAX_LEVEL, 0, OTID_STRUCT_PRESET(0), 31);
    u32 initialExp = GetMonData(&mon, MON_DATA_EXP);
    EXPECT_EQ(GetMonMasteryLevel(&mon), 0);
    EXPECT_EQ(GetProgressLevelNextExp(species, initialExp) - initialExp, MASTERY_EXP_PER_LEVEL);
    AddMonExperience(&mon, MAX_MASTERY_LEVEL * MASTERY_EXP_PER_LEVEL);
    BoxMonToMon(&mon.box, &mon);
    EXPECT_EQ(GetMonData(&mon, MON_DATA_EXP), GetMaxMonExperience(species));
    EXPECT_EQ(GetMonMasteryLevel(&mon), 300);
    EXPECT_EQ(GetMonData(&mon, MON_DATA_LEVEL), MAX_LEVEL);
}

TEST("Mastery Rare Candy advances a mastery level without raising stats")
{
    struct Pokemon mon;
    CreateMonWithIVs(&mon, SPECIES_CHARIZARD, MAX_LEVEL, 0, OTID_STRUCT_PRESET(0), 31);
    u32 attack = GetMonData(&mon, MON_DATA_ATK);
    EXPECT_EQ(ExecuteTableBasedItemEffect(&mon, ITEM_RARE_CANDY, 0, 0), FALSE);
    EXPECT_EQ(GetMonMasteryLevel(&mon), 1);
    EXPECT_EQ(GetMonData(&mon, MON_DATA_ATK), attack);
    EXPECT_EQ(GetMonData(&mon, MON_DATA_LEVEL), MAX_LEVEL);
    AddMonExperience(&mon, 0xFFFFFFFF);
    EXPECT_EQ(ExecuteTableBasedItemEffect(&mon, ITEM_EXP_CANDY_XL, 0, 0), TRUE);
}

TEST("Mastery daycare withdrawal preserves progression and saturates large EXP")
{
    u32 steps = 0;
    PARAMETRIZE { steps = MASTERY_EXP_PER_LEVEL; }
    PARAMETRIZE { steps = 0xFFFFFFFF; }
    struct Pokemon mon;
    ZeroPlayerPartyMons();
    memset(&gSaveBlock1Ptr->daycare, 0, sizeof(gSaveBlock1Ptr->daycare));
    CreateMonWithIVs(&mon, SPECIES_CHARIZARD, MAX_LEVEL, 0, OTID_STRUCT_PRESET(0), 31);
    AddMonExperience(&mon, 100 * MASTERY_EXP_PER_LEVEL);
    u32 stats[NUM_STATS];
    for (u32 stat = 0; stat < NUM_STATS; stat++)
        stats[stat] = GetMonData(&mon, MON_DATA_MAX_HP + stat);
    StorePokemonInDaycare(&mon, &gSaveBlock1Ptr->daycare.mons[0]);
    gSaveBlock1Ptr->daycare.mons[0].steps = steps;
    gSpecialVar_0x8004 = 0;
    EXPECT_EQ(TakePokemonFromDaycare(), SPECIES_CHARIZARD);
    EXPECT_EQ(GetMonMasteryLevel(&gParties[B_TRAINER_0][0]), steps == MASTERY_EXP_PER_LEVEL ? 101 : 300);
    EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_LEVEL), 100);
    for (u32 stat = 0; stat < NUM_STATS; stat++)
        EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_MAX_HP + stat), stats[stat]);
}
