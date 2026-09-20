#include "global.h"
#include "battle.h"
#include "battle_caps.h"
#include "battle_setup.h"
#include "config_changes.h"
#include "constants/johto_content.h"
#include "event_data.h"
#include "mastery.h"
#include "test/test.h"

TEST("Badge caps use the permitted evolution stage at the threshold")
{
    EXPECT_EQ(GetSpeciesAtBattleLevelCap(SPECIES_CHARIZARD, 15), SPECIES_CHARMANDER);
    EXPECT_EQ(GetSpeciesAtBattleLevelCap(SPECIES_CHARIZARD, 16), SPECIES_CHARMELEON);
    EXPECT_EQ(GetSpeciesAtBattleLevelCap(SPECIES_CHARIZARD, 35), SPECIES_CHARMELEON);
    EXPECT_EQ(GetSpeciesAtBattleLevelCap(SPECIES_CHARIZARD, 36), SPECIES_CHARIZARD);
    EXPECT_EQ(GetSpeciesAtBattleLevelCap(SPECIES_BUTTERFREE, 6), SPECIES_CATERPIE);
    EXPECT_EQ(GetSpeciesAtBattleLevelCap(SPECIES_BUTTERFREE, 7), SPECIES_METAPOD);
    EXPECT_EQ(GetSpeciesAtBattleLevelCap(SPECIES_LAPRAS, 15), SPECIES_LAPRAS);
    EXPECT_EQ(GetSpeciesAtBattleLevelCap(SPECIES_RAICHU, 15), SPECIES_RAICHU);
}

TEST("Badge caps count only victories from the active region")
{
    static const u16 leaders[] = {
        TRAINER_ROXANNE_1, TRAINER_BRAWLY_1, TRAINER_WATTSON_1, TRAINER_FLANNERY_1,
        TRAINER_NORMAN_1, TRAINER_WINONA_1, TRAINER_TATE_AND_LIZA_1, TRAINER_JUAN_1,
    };
    static const u8 caps[] = {15, 19, 24, 29, 31, 33, 42, 46, 58};
    gMapHeader.regionMapSectionId = MAPSEC_LITTLEROOT_TOWN;
    FlagClear(FLAG_IS_CHAMPION);
    for (u32 i = 0; i < NUM_BADGES; i++)
        ClearTrainerFlag(leaders[i]);
    for (u32 i = 0; i <= NUM_BADGES; i++)
    {
        EXPECT_EQ(GetBadgeBattleLevelCap(), caps[i]);
        if (i < NUM_BADGES)
            SetTrainerFlag(leaders[i]);
    }
    FlagSet(FLAG_IS_CHAMPION);
    EXPECT_EQ(GetBadgeBattleLevelCap(), 100);
    gMapHeader.regionMapSectionId = MAPSEC_PEWTER_CITY;
    for (u32 flag = FLAG_DEFEATED_BROCK; flag <= FLAG_DEFEATED_LEADER_GIOVANNI; flag++)
        FlagClear(flag);
    EXPECT_EQ(GetBadgeBattleLevelCap(), 15);
    FlagSet(FLAG_DEFEATED_BROCK);
    EXPECT_EQ(GetBadgeBattleLevelCap(), 19);
    for (u32 flag = FLAG_DEFEATED_BROCK; flag <= FLAG_DEFEATED_LEADER_GIOVANNI; flag++)
        FlagSet(flag);
    FlagClear(FLAG_KANTO_MASTERY_CHAMPION);
    VarSet(VAR_MAP_SCENE_PALLET_TOWN_OAK, 1);
    EXPECT_EQ(GetBadgeBattleLevelCap(), 58); // Hoenn champion is not Kanto champion.
    VarSet(VAR_MAP_SCENE_PALLET_TOWN_OAK, 3);
    EXPECT_EQ(GetBadgeBattleLevelCap(), 100); // Legacy champion save.

    gMapHeader.regionMapSectionId = MAPSEC_NEW_BARK_TOWN;
    for (u32 flag = JOHTO_FLAG_BADGE01_GET; flag <= JOHTO_FLAG_BADGE08_GET; flag++)
        FlagClear(flag);
    FlagClear(JOHTO_FLAG_IS_CHAMPION);
    EXPECT_EQ(GetBadgeBattleLevelCap(), 15);
    FlagSet(JOHTO_FLAG_BADGE01_GET);
    EXPECT_EQ(GetBadgeBattleLevelCap(), 19);
    for (u32 flag = JOHTO_FLAG_BADGE01_GET; flag <= JOHTO_FLAG_BADGE08_GET; flag++)
        FlagSet(flag);
    EXPECT_EQ(GetBadgeBattleLevelCap(), 58);
    FlagSet(JOHTO_FLAG_IS_CHAMPION);
    EXPECT_EQ(GetBadgeBattleLevelCap(), 100);

    gMapHeader.regionMapSectionId = MAPSEC_PALLET_TOWN;
    gSaveBlock1Ptr->location.mapGroup = 77;
    gSaveBlock1Ptr->location.mapNum = 0;
    for (u32 flag = JOHTO_FLAG_BADGE09_GET; flag <= JOHTO_FLAG_BADGE16_GET; flag++)
        FlagClear(flag);
    FlagClear(JOHTO_FLAG_IS_KANTO_CHAMPION);
    EXPECT_EQ(GetBadgeBattleLevelCap(), 15);
    FlagSet(JOHTO_FLAG_BADGE09_GET);
    EXPECT_EQ(GetBadgeBattleLevelCap(), 19);
    for (u32 flag = JOHTO_FLAG_BADGE09_GET; flag <= JOHTO_FLAG_BADGE16_GET; flag++)
        FlagSet(flag);
    EXPECT_EQ(GetBadgeBattleLevelCap(), 58);
    FlagSet(JOHTO_FLAG_IS_KANTO_CHAMPION);
    EXPECT_EQ(GetBadgeBattleLevelCap(), 100);
}

TEST("Badge caps preserve permanent progression and HP through projection")
{
    struct Pokemon *mon = &gParties[B_TRAINER_0][0];
    struct Pokemon expected;
    u32 hpMode = 0;
    PARAMETRIZE { hpMode = 0; }
    PARAMETRIZE { hpMode = 1; }
    PARAMETRIZE { hpMode = 2; }
    ZeroPlayerPartyMons();
    gMapHeader.regionMapSectionId = MAPSEC_LITTLEROOT_TOWN;
    for (u32 trainer = TRAINER_ROXANNE_1; trainer <= TRAINER_JUAN_1; trainer++)
        ClearTrainerFlag(trainer);
    gBattleTypeFlags = 0;
    SetConfig(CONFIG_B_BADGE_BATTLE_CAP, TRUE);
    CreateMonWithIVs(mon, SPECIES_CHARIZARD, MAX_LEVEL, 0, OTID_STRUCT_PRESET(0), 31);
    AddMonExperience(mon, 201 * MASTERY_EXP_PER_LEVEL);
    u32 exp = GetMonData(mon, MON_DATA_EXP);
    u32 hp = hpMode == 2 ? GetMonData(mon, MON_DATA_MAX_HP) / 2 : hpMode;
    SetMonData(mon, MON_DATA_HP, &hp);
    BeginBattleLevelCaps();
    EXPECT_EQ(GetMonData(mon, MON_DATA_SPECIES), SPECIES_CHARMANDER);
    EXPECT_EQ(GetMonData(mon, MON_DATA_LEVEL), 15);
    CreateMonWithIVs(&expected, SPECIES_CHARMANDER, 15, 0, OTID_STRUCT_PRESET(0), 31);
    EXPECT_EQ(GetMonData(mon, MON_DATA_ATK), GetMonData(&expected, MON_DATA_ATK));
    EXPECT_EQ(GetMonData(mon, MON_DATA_MAX_HP), GetMonData(&expected, MON_DATA_MAX_HP));
    EndBattleLevelCaps();
    EXPECT_EQ(GetMonData(mon, MON_DATA_SPECIES), SPECIES_CHARIZARD);
    EXPECT_EQ(GetMonData(mon, MON_DATA_EXP), exp);
    EXPECT_EQ(GetMonMasteryLevel(mon), 201);
    EXPECT_EQ(GetMonData(mon, MON_DATA_HP), hp);
}

TEST("Badge caps retain battle damage status PP and item changes")
{
    struct Pokemon *mon = &gParties[B_TRAINER_0][0];
    bool32 fainted = FALSE;
    PARAMETRIZE { fainted = FALSE; }
    PARAMETRIZE { fainted = TRUE; }
    ZeroPlayerPartyMons();
    gMapHeader.regionMapSectionId = MAPSEC_LITTLEROOT_TOWN;
    for (u32 trainer = TRAINER_ROXANNE_1; trainer <= TRAINER_JUAN_1; trainer++)
        ClearTrainerFlag(trainer);
    gBattleTypeFlags = 0;
    SetConfig(CONFIG_B_BADGE_BATTLE_CAP, TRUE);
    CreateMonWithIVs(mon, SPECIES_CHARIZARD, 70, 0, OTID_STRUCT_PRESET(0), 31);
    u32 heldItem = ITEM_ORAN_BERRY;
    SetMonData(mon, MON_DATA_HELD_ITEM, &heldItem);
    SetMonMoveSlot(mon, MOVE_EMBER, 0);
    u32 originalMax = GetMonData(mon, MON_DATA_MAX_HP);
    BeginBattleLevelCaps();
    u32 projectedMax = GetMonData(mon, MON_DATA_MAX_HP);
    u32 hp = fainted ? 0 : projectedMax / 2, status = STATUS1_POISON, pp = 2, item = ITEM_NONE;
    SetMonData(mon, MON_DATA_HP, &hp);
    SetMonData(mon, MON_DATA_STATUS, &status);
    SetMonData(mon, MON_DATA_PP1, &pp);
    SetMonData(mon, MON_DATA_HELD_ITEM, &item);
    EndBattleLevelCaps();
    EXPECT_EQ(GetMonData(mon, MON_DATA_HP), hp * originalMax / projectedMax);
    EXPECT_EQ(GetMonData(mon, MON_DATA_STATUS), status);
    EXPECT_EQ(GetMonData(mon, MON_DATA_PP1), pp);
    EXPECT_EQ(GetMonData(mon, MON_DATA_HELD_ITEM), item);
}

TEST("Badge caps leave eggs and Pokemon at or below the cap unchanged")
{
    ZeroPlayerPartyMons();
    gMapHeader.regionMapSectionId = MAPSEC_LITTLEROOT_TOWN;
    for (u32 trainer = TRAINER_ROXANNE_1; trainer <= TRAINER_JUAN_1; trainer++)
        ClearTrainerFlag(trainer);
    gBattleTypeFlags = 0;
    SetConfig(CONFIG_B_BADGE_BATTLE_CAP, TRUE);
    CreateMonWithIVs(&gParties[B_TRAINER_0][0], SPECIES_CHARMANDER, 15, 0, OTID_STRUCT_PRESET(0), 31);
    CreateMonWithIVs(&gParties[B_TRAINER_0][1], SPECIES_CHARMANDER, 10, 0, OTID_STRUCT_PRESET(0), 31);
    CreateMonWithIVs(&gParties[B_TRAINER_0][2], SPECIES_CHARMANDER, 70, 0, OTID_STRUCT_PRESET(0), 31);
    u32 isEgg = TRUE;
    SetMonData(&gParties[B_TRAINER_0][2], MON_DATA_IS_EGG, &isEgg);
    struct Pokemon originals[3];
    memcpy(originals, gParties[B_TRAINER_0], sizeof(originals));
    BeginBattleLevelCaps();
    EXPECT_EQ(memcmp(originals, gParties[B_TRAINER_0], sizeof(originals)), 0);
    EndBattleLevelCaps();
    EXPECT_EQ(memcmp(originals, gParties[B_TRAINER_0], sizeof(originals)), 0);
}

TEST("Badge caps keep mastery EXP through repeated experience awards")
{
    struct Pokemon *mon = &gParties[B_TRAINER_0][0];
    ZeroPlayerPartyMons();
    gMapHeader.regionMapSectionId = MAPSEC_LITTLEROOT_TOWN;
    for (u32 trainer = TRAINER_ROXANNE_1; trainer <= TRAINER_JUAN_1; trainer++)
        ClearTrainerFlag(trainer);
    gBattleTypeFlags = 0;
    SetConfig(CONFIG_B_BADGE_BATTLE_CAP, TRUE);
    CreateMonWithIVs(mon, SPECIES_CHARIZARD, 100, 0, OTID_STRUCT_PRESET(0), 31);
    BeginBattleLevelCaps();
    for (u32 i = 0; i < 3; i++)
    {
        u32 damagedHp = 20;
        SetMonData(mon, MON_DATA_HP, &damagedHp);
        EXPECT(BattleCaps_BeginExperience(0));
        EXPECT_EQ(GetMonData(mon, MON_DATA_SPECIES), SPECIES_CHARIZARD);
        AddMonExperience(mon, 100 * MASTERY_EXP_PER_LEVEL);
        EXPECT(BattleCaps_EndExperience(0));
        EXPECT_EQ(GetMonData(mon, MON_DATA_SPECIES), SPECIES_CHARMANDER);
        EXPECT_EQ(GetMonData(mon, MON_DATA_LEVEL), 15);
        EXPECT_EQ(GetMonData(mon, MON_DATA_HP), damagedHp);
    }
    EndBattleLevelCaps();
    EXPECT_EQ(GetMonMasteryLevel(mon), 300);
    EXPECT_EQ(GetMonData(mon, MON_DATA_LEVEL), 100);
}

TEST("Badge caps exclude link and facility battles")
{
    struct Pokemon *mon = &gParties[B_TRAINER_0][0];
    u32 flags = 0;
    PARAMETRIZE { flags = BATTLE_TYPE_LINK; }
    PARAMETRIZE { flags = BATTLE_TYPE_BATTLE_TOWER; }
    PARAMETRIZE { flags = BATTLE_TYPE_RECORDED; }
    PARAMETRIZE { flags = BATTLE_TYPE_CATCH_TUTORIAL; }
    ZeroPlayerPartyMons();
    CreateMonWithIVs(mon, SPECIES_CHARIZARD, 70, 0, OTID_STRUCT_PRESET(0), 31);
    SetConfig(CONFIG_B_BADGE_BATTLE_CAP, TRUE);
    gBattleTypeFlags = flags;
    BeginBattleLevelCaps();
    EXPECT_EQ(GetMonData(mon, MON_DATA_SPECIES), SPECIES_CHARIZARD);
    EXPECT_EQ(GetMonData(mon, MON_DATA_LEVEL), 70);
    EndBattleLevelCaps();
}

TEST("Badge caps preserve HP stat gains from EVs during experience awards")
{
    struct Pokemon *mon = &gParties[B_TRAINER_0][0];
    struct Pokemon expected;
    ZeroPlayerPartyMons();
    gMapHeader.regionMapSectionId = MAPSEC_LITTLEROOT_TOWN;
    for (u32 trainer = TRAINER_ROXANNE_1; trainer <= TRAINER_JUAN_1; trainer++)
        ClearTrainerFlag(trainer);
    gBattleTypeFlags = 0;
    gBattlersCount = 0;
    SetConfig(CONFIG_B_BADGE_BATTLE_CAP, TRUE);
    CreateMonWithIVs(mon, SPECIES_LAPRAS, 100, 0, OTID_STRUCT_PRESET(0), 31);
    BeginBattleLevelCaps();
    u32 hp = 20;
    u32 oldMax = GetMonData(mon, MON_DATA_MAX_HP);
    SetMonData(mon, MON_DATA_HP, &hp);
    EXPECT(BattleCaps_BeginExperience(0));
    expected = *mon;
    u32 ev = 252;
    SetMonData(&expected, MON_DATA_HP_EV, &ev);
    CalculateMonStats(&expected);
    SetMonData(mon, MON_DATA_HP_EV, &ev);
    EXPECT(BattleCaps_EndExperience(0));
    EXPECT_EQ(GetMonData(mon, MON_DATA_HP), hp + GetMonData(mon, MON_DATA_MAX_HP) - oldMax);
    EndBattleLevelCaps();
    EXPECT_EQ(GetMonData(mon, MON_DATA_HP), GetMonData(&expected, MON_DATA_HP));
    EXPECT_EQ(GetMonData(mon, MON_DATA_MAX_HP), GetMonData(&expected, MON_DATA_MAX_HP));
}
