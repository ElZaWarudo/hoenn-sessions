#include "global.h"
#include "event_data.h"
#include "heal_location.h"
#include "load_save.h"
#include "overworld.h"
#include "save_location.h"
#include "cormoria/heal_locations.h"
#include "constants/maps.h"
#include "constants/cormoria_event_ids.h"
#include "test/test.h"
#include "world/event_save.h"
#include "main.h"
#include "hall_of_fame.h"
#include "credits.h"
#include "credits_frlg.h"
#include "daycare.h"
#include "malloc.h"
#include "pokemon.h"
#include "constants/battle.h"

int GameClear(void);

#if ROM_WORLD == 2

static void ExpectCarabrueContinueWarp(void)
{
    const struct HealLocation *carabrue = GetHealLocation(HEAL_LOCATION_CORMORIA_CARABRUE_TOWN);
    EXPECT(carabrue != NULL);
    EXPECT_EQ(UseContinueGameWarp(), CONTINUE_GAME_WARP);
    EXPECT_EQ(gSaveBlock1Ptr->continueGameWarp.mapGroup, carabrue->mapGroup);
    EXPECT_EQ(gSaveBlock1Ptr->continueGameWarp.mapNum, carabrue->mapNum);
    EXPECT_EQ(gSaveBlock1Ptr->continueGameWarp.warpId, WARP_ID_NONE);
    EXPECT_EQ(gSaveBlock1Ptr->continueGameWarp.x, carabrue->x);
    EXPECT_EQ(gSaveBlock1Ptr->continueGameWarp.y, carabrue->y);
}

static void ExecuteGameClearWithRunnerRestored(void)
{
    MainCallback runnerCallback = gMain.callback2;
    int result = GameClear();
    MainCallback nextCallback = gMain.callback2;
    // Restore before any assertion can abort the fixture.
    SetMainCallback2(runnerCallback);
    EXPECT_EQ(result, 0);
    EXPECT_EQ(nextCallback, CB2_DoHallOfFameScreen);
}

static void ExpectMonIdentity(struct Pokemon *before, struct Pokemon *after)
{
    static const u8 fields[] = {
        MON_DATA_PERSONALITY, MON_DATA_OT_ID, MON_DATA_SPECIES,
        MON_DATA_LEVEL, MON_DATA_EXP, MON_DATA_HELD_ITEM, MON_DATA_IVS,
        MON_DATA_MOVE1, MON_DATA_MOVE2, MON_DATA_MOVE3, MON_DATA_MOVE4,
        MON_DATA_PP_BONUSES, MON_DATA_SANITY_IS_EGG,
    };
    u32 i;
    u8 beforeName[POKEMON_NAME_LENGTH + 1] = {0};
    u8 afterName[POKEMON_NAME_LENGTH + 1] = {0};
    for (i = 0; i < ARRAY_COUNT(fields); i++)
        EXPECT_EQ(GetMonData(before, fields[i]), GetMonData(after, fields[i]));
    GetMonData(before, MON_DATA_NICKNAME, beforeName);
    GetMonData(after, MON_DATA_NICKNAME, afterName);
    EXPECT_EQ(memcmp(beforeName, afterName, sizeof(beforeName)), 0);
}

TEST("Cormoria GameClear awards normal party once and keeps host completion independent")
{
    u32 i, fixture = 0;
    u32 status = STATUS1_POISON;
    u32 initialTime, expectedTime;
    u16 hp = 1, heldItem = ITEM_ORAN_BERRY;
    u8 pp = 0;
    struct Pokemon *partySnapshot;
    struct WorldEventSaveV1 *regionalSnapshot;
    for (i = 0; i < 4; i++)
        PARAMETRIZE { fixture = i; }
    partySnapshot = Alloc(sizeof(gPlayerParty));
    regionalSnapshot = Alloc(sizeof(*regionalSnapshot));
    ASSUME(partySnapshot != NULL);
    ASSUME(regionalSnapshot != NULL);
    SetSaveBlocksPointers(0);
    WorldEventSave_InitializeCurrent();
    ResetPokemonStorageSystem();
    ZeroPlayerPartyMons();
    CreateMonWithIVs(&gPlayerParty[0], SPECIES_RATTATA, 17, 0x12345678,
                     OTID_STRUCT_PRESET(0x11223344), 12);
    SetMonMoveSlot(&gPlayerParty[0], MOVE_TACKLE, 0);
    SetMonData(&gPlayerParty[0], MON_DATA_HELD_ITEM, &heldItem);
    SetMonData(&gPlayerParty[0], MON_DATA_HP, &hp);
    SetMonData(&gPlayerParty[0], MON_DATA_STATUS, &status);
    SetMonData(&gPlayerParty[0], MON_DATA_PP1, &pp);
    CreateEgg(&gPlayerParty[1], SPECIES_PIKACHU, FALSE);
    heldItem = ITEM_POTION;
    SetMonData(&gPlayerParty[1], MON_DATA_HELD_ITEM, &heldItem);
    CalculatePlayerPartyCount();
    EXPECT_EQ(gPlayerPartyCount, 2);
    EXPECT(!GetMonData(&gPlayerParty[0], MON_DATA_CHAMPION_RIBBON));
    EXPECT(!GetMonData(&gPlayerParty[1], MON_DATA_CHAMPION_RIBBON));
    memcpy(partySnapshot, gPlayerParty, sizeof(gPlayerParty));
    FlagClear(Cormoria_FLAG_SYS_GAME_CLEAR);
    FlagSet(Cormoria_FLAG_LAB_CALLTOACTION);
    if (fixture & 1)
        FlagSet(FLAG_SYS_GAME_CLEAR);
    else
        FlagClear(FLAG_SYS_GAME_CLEAR);
    FlagClear(FLAG_SYS_RIBBON_GET);
    SetGameStat(GAME_STAT_RECEIVED_RIBBONS, 7);
    initialTime = fixture & 2 ? (9 << 16) | (8 << 8) | 7 : 0;
    SetGameStat(GAME_STAT_FIRST_HOF_PLAY_TIME, initialTime);
    gSaveBlock2Ptr->playTimeHours = 3;
    gSaveBlock2Ptr->playTimeMinutes = 4;
    gSaveBlock2Ptr->playTimeSeconds = 5;
    expectedTime = initialTime != 0 ? initialTime : (3 << 16) | (4 << 8) | 5;
    // Opposite sentinel proves GameClear sets both first-clear indicators.
    gHasHallOfFameRecords = gHasHallOfFameRecordsFrlg = TRUE;

    ExecuteGameClearWithRunnerRestored();
    EXPECT(!gHasHallOfFameRecords);
    EXPECT(!gHasHallOfFameRecordsFrlg);
    EXPECT(FlagGet(Cormoria_FLAG_SYS_GAME_CLEAR));
    EXPECT_EQ(FlagGet(FLAG_SYS_GAME_CLEAR), fixture & 1);
    EXPECT(FlagGet(Cormoria_FLAG_LAB_CALLTOACTION));
    EXPECT(WorldEventSave_Validate(&gSaveblock1.world_event));
    EXPECT_EQ(GetGameStat(GAME_STAT_FIRST_HOF_PLAY_TIME), expectedTime);
    EXPECT_EQ(GetGameStat(GAME_STAT_RECEIVED_RIBBONS), 8);
    EXPECT(FlagGet(FLAG_SYS_RIBBON_GET));
    EXPECT_EQ(gPlayerPartyCount, 2);
    for (i = 0; i < 2; i++)
        ExpectMonIdentity(&partySnapshot[i], &gPlayerParty[i]);
    EXPECT_EQ(GetMonData(&gPlayerParty[0], MON_DATA_HP), GetMonData(&gPlayerParty[0], MON_DATA_MAX_HP));
    EXPECT_EQ(GetMonData(&gPlayerParty[0], MON_DATA_STATUS), STATUS1_NONE);
    EXPECT_EQ(GetMonData(&gPlayerParty[0], MON_DATA_PP1),
              CalculatePPWithBonus(MOVE_TACKLE, GetMonData(&gPlayerParty[0], MON_DATA_PP_BONUSES), 0));
    EXPECT(GetMonData(&gPlayerParty[0], MON_DATA_CHAMPION_RIBBON));
    EXPECT(!GetMonData(&gPlayerParty[1], MON_DATA_CHAMPION_RIBBON));
    EXPECT_EQ(memcmp(&partySnapshot[2], &gPlayerParty[2], sizeof(gPlayerParty[0]) * (PARTY_SIZE - 2)), 0);
    ExpectCarabrueContinueWarp();
    memcpy(partySnapshot, gPlayerParty, sizeof(gPlayerParty));
    *regionalSnapshot = gSaveblock1.world_event;

    gSaveBlock2Ptr->playTimeHours = 11;
    gSaveBlock2Ptr->playTimeMinutes = 12;
    gSaveBlock2Ptr->playTimeSeconds = 13;
    gHasHallOfFameRecords = gHasHallOfFameRecordsFrlg = FALSE;
    ExecuteGameClearWithRunnerRestored();
    EXPECT(gHasHallOfFameRecords);
    EXPECT(gHasHallOfFameRecordsFrlg);
    EXPECT_EQ(GetGameStat(GAME_STAT_FIRST_HOF_PLAY_TIME), expectedTime);
    EXPECT_EQ(GetGameStat(GAME_STAT_RECEIVED_RIBBONS), 8);
    EXPECT_EQ(memcmp(partySnapshot, gPlayerParty, sizeof(gPlayerParty)), 0);
    EXPECT_EQ(memcmp(regionalSnapshot, &gSaveblock1.world_event, sizeof(*regionalSnapshot)), 0);
    EXPECT(WorldEventSave_Validate(&gSaveblock1.world_event));
    EXPECT_EQ(FlagGet(FLAG_SYS_GAME_CLEAR), fixture & 1);
    ExpectCarabrueContinueWarp();
    Free(regionalSnapshot);
    Free(partySnapshot);
}

TEST("Cormoria GameClear stores the Carabrue Town continue warp")
{
    const struct HealLocation *carabrue = GetHealLocation(HEAL_LOCATION_CORMORIA_CARABRUE_TOWN);
    MainCallback runnerCallback = gMain.callback2;
    MainCallback nextCallback;
    int result;

    EXPECT(carabrue != NULL);
    SetSaveBlocksPointers(0);
    WorldEventSave_InitializeCurrent();
    FlagClear(FLAG_SYS_GAME_CLEAR);
    FlagClear(Cormoria_FLAG_SYS_GAME_CLEAR);
    EXPECT_EQ(FlagGet(Cormoria_FLAG_SYS_GAME_CLEAR), FALSE);

    result = GameClear();
    nextCallback = gMain.callback2;
    // Verify the transition without abandoning the test runner for the UI.
    SetMainCallback2(runnerCallback);
    EXPECT_EQ(result, 0);
    EXPECT_EQ(nextCallback, CB2_DoHallOfFameScreen);

    EXPECT_EQ(FlagGet(Cormoria_FLAG_SYS_GAME_CLEAR), TRUE);
    EXPECT_EQ(FlagGet(FLAG_SYS_GAME_CLEAR), FALSE);
    EXPECT_EQ(UseContinueGameWarp(), CONTINUE_GAME_WARP);
    EXPECT_EQ(gSaveBlock1Ptr->continueGameWarp.mapGroup, carabrue->mapGroup);
    EXPECT_EQ(gSaveBlock1Ptr->continueGameWarp.mapNum, carabrue->mapNum);
    EXPECT_EQ(gSaveBlock1Ptr->continueGameWarp.warpId, WARP_ID_NONE);
    EXPECT_EQ(gSaveBlock1Ptr->continueGameWarp.x, carabrue->x);
    EXPECT_EQ(gSaveBlock1Ptr->continueGameWarp.y, carabrue->y);
}

#else

TEST("Main GameClear keeps the Brendan house continue warp")
{
    const struct HealLocation *brendanHouse = GetHealLocation(HEAL_LOCATION_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F);

    EXPECT(brendanHouse != NULL);
    gSaveBlock2Ptr->playerGender = MALE;

    EXPECT_EQ(GameClear(), 0);
    EXPECT_EQ(gSaveBlock1Ptr->continueGameWarp.mapGroup, brendanHouse->mapGroup);
    EXPECT_EQ(gSaveBlock1Ptr->continueGameWarp.mapNum, brendanHouse->mapNum);
    EXPECT_EQ(gSaveBlock1Ptr->continueGameWarp.warpId, WARP_ID_NONE);
    EXPECT_EQ(gSaveBlock1Ptr->continueGameWarp.x, brendanHouse->x);
    EXPECT_EQ(gSaveBlock1Ptr->continueGameWarp.y, brendanHouse->y);
}

#endif
