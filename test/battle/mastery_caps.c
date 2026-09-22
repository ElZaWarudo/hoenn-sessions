#include "global.h"
#include "battle_caps.h"
#include "battle_setup.h"
#include "event_data.h"
#include "mastery.h"
#include "test/battle.h"

static void NoHoennBadges(void)
{
    gMapHeader.regionMapSectionId = MAPSEC_LITTLEROOT_TOWN;
    for (u32 trainer = TRAINER_ROXANNE_1; trainer <= TRAINER_JUAN_1; trainer++)
        ClearTrainerFlag(trainer);
}

WILD_BATTLE_TEST("Mastery gains experience at level 100 without stat levels")
{
    u32 mastery = 0;
    PARAMETRIZE { mastery = 0; }
    PARAMETRIZE { mastery = 299; }
    PARAMETRIZE { mastery = 300; }
    GIVEN {
        PLAYER(SPECIES_CHARIZARD) { Level(100); }
        OPPONENT(SPECIES_BLISSEY) { Level(100); HP(1); }
        AddMonExperience(&PLAYER_PARTY[0], mastery * MASTERY_EXP_PER_LEVEL);
    } WHEN {
        TURN { MOVE(player, MOVE_SCRATCH); }
    } SCENE {
        MESSAGE("Charizard used Scratch!");
        MESSAGE("The wild Blissey fainted!");
    } THEN {
        if (mastery < MAX_MASTERY_LEVEL)
            EXPECT_GT(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_EXP), GetMonData(&PLAYER_PARTY[0], MON_DATA_EXP));
        else
            EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_EXP), GetMaxMonExperience(SPECIES_CHARIZARD));
        EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_LEVEL), 100);
    }
}

WILD_BATTLE_TEST("Badge caps restore real progression after catching a Pokemon")
{
    GIVEN {
        WITH_CONFIG(B_BADGE_BATTLE_CAP, TRUE);
        WITH_CONFIG(B_EXP_CATCH, GEN_6);
        NoHoennBadges();
        PLAYER(SPECIES_CHARIZARD) { Level(70); }
        OPPONENT(SPECIES_CATERPIE) { Level(1); HP(1); }
    } WHEN {
        TURN { USE_ITEM(player, ITEM_ULTRA_BALL, WITH_RNG(RNG_BALLTHROW_SHAKE, 0)); }
    } THEN {
        EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_SPECIES), SPECIES_CHARIZARD);
        EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_LEVEL), 70);
        EXPECT_GT(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_EXP), GetMonData(&PLAYER_PARTY[0], MON_DATA_EXP));
    }
}

WILD_BATTLE_TEST("Badge caps restore the real Pokemon after defeat and keep it fainted")
{
    GIVEN {
        WITH_CONFIG(B_BADGE_BATTLE_CAP, TRUE);
        NoHoennBadges();
        PLAYER(SPECIES_CHARIZARD) { Level(70); }
        OPPONENT(SPECIES_WOBBUFFET) { Level(100); }
    } WHEN {
        TURN { MOVE(player, MOVE_CELEBRATE); MOVE(opponent, MOVE_NIGHT_SHADE); }
    } THEN {
        EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_SPECIES), SPECIES_CHARIZARD);
        EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_LEVEL), 70);
        EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_HP), 0);
    }
}

WILD_BATTLE_TEST("Badge caps use the capped level for damage across switches", s16 first; s16 second)
{
    u32 cap = 0;
    PARAMETRIZE { cap = 15; }
    PARAMETRIZE { cap = 19; }
    GIVEN {
        WITH_CONFIG(B_BADGE_BATTLE_CAP, TRUE);
        NoHoennBadges();
        if (cap == 19)
            SetTrainerFlag(TRAINER_ROXANNE_1);
        PLAYER(SPECIES_CHARIZARD) { Level(70); }
        PLAYER(SPECIES_LAPRAS) { Level(70); }
        OPPONENT(SPECIES_WOBBUFFET) { Level(70); }
    } WHEN {
        TURN { MOVE(player, MOVE_NIGHT_SHADE); }
        TURN { SWITCH(player, 1); }
        TURN { SWITCH(player, 0); }
        TURN { MOVE(player, MOVE_NIGHT_SHADE); }
    } SCENE {
        HP_BAR(opponent, captureDamage: &results[i].first);
        HP_BAR(opponent, captureDamage: &results[i].second);
    } THEN {
        EXPECT_EQ(results[i].first, cap);
        EXPECT_EQ(results[i].second, cap);
        EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_SPECIES), SPECIES_CHARIZARD);
        EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_LEVEL), 70);
    }
}

WILD_BATTLE_TEST("Badge caps award experience to real progression and restore after victory")
{
    GIVEN {
        WITH_CONFIG(B_BADGE_BATTLE_CAP, TRUE);
        NoHoennBadges();
        PLAYER(SPECIES_CHARIZARD) { Level(70); }
        OPPONENT(SPECIES_BLISSEY) { Level(100); HP(1); }
    } WHEN {
        TURN { MOVE(player, MOVE_SCRATCH); }
    } THEN {
        EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_SPECIES), SPECIES_CHARIZARD);
        EXPECT_GE(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_LEVEL), 70);
        EXPECT_GT(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_EXP), gExperienceTables[gSpeciesInfo[SPECIES_CHARIZARD].growthRate][70]);
    }
}

WILD_BATTLE_TEST("Badge caps allow real level ups while retaining the capped combat level")
{
    GIVEN {
        WITH_CONFIG(B_BADGE_BATTLE_CAP, TRUE);
        NoHoennBadges();
        PLAYER(SPECIES_CHARIZARD) { Level(70); }
        OPPONENT(SPECIES_BLISSEY) { Level(1); HP(1); }
        u32 exp = gExperienceTables[gSpeciesInfo[SPECIES_CHARIZARD].growthRate][71] - 1;
        SetMonData(&PLAYER_PARTY[0], MON_DATA_EXP, &exp);
    } WHEN {
        TURN { MOVE(player, MOVE_SCRATCH); }
    } THEN {
        EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_SPECIES), SPECIES_CHARIZARD);
        EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_LEVEL), 71);
    }
}

WILD_BATTLE_TEST("Badge caps retain mastery gained in a capped battle")
{
    GIVEN {
        WITH_CONFIG(B_BADGE_BATTLE_CAP, TRUE);
        NoHoennBadges();
        PLAYER(SPECIES_CHARIZARD) { Level(100); }
        OPPONENT(SPECIES_BLISSEY) { Level(100); HP(1); }
        AddMonExperience(&PLAYER_PARTY[0], MASTERY_EXP_PER_LEVEL - 1);
    } WHEN {
        TURN { MOVE(player, MOVE_SCRATCH); }
    } THEN {
        EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_SPECIES), SPECIES_CHARIZARD);
        EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_LEVEL), 100);
        EXPECT_EQ(GetMonMasteryLevel(&gParties[B_TRAINER_0][0]), 1);
    }
}

WILD_BATTLE_TEST("Badge caps restore ordinary HP after Dynamax experience")
{
    u32 level = 0;
    PARAMETRIZE { level = 70; }
    PARAMETRIZE { level = 100; }
    GIVEN {
        WITH_CONFIG(B_BADGE_BATTLE_CAP, TRUE);
        NoHoennBadges();
        PLAYER(SPECIES_LAPRAS) { Level(level); DynamaxLevel(10); }
        OPPONENT(SPECIES_BLISSEY) { Level(1); HP(1); }
    } WHEN {
        TURN { MOVE(player, MOVE_SCRATCH, gimmick: GIMMICK_DYNAMAX); }
    } SCENE {
        MESSAGE("Lapras used Max Strike!");
        MESSAGE("The wild Blissey fainted!");
    } THEN {
        EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_HP), GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_MAX_HP));
        EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_LEVEL), level);
    }
}
