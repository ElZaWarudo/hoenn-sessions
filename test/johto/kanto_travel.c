#include "global.h"
#include "heal_location.h"
#include "load_save.h"
#include "region_map.h"
#include "johto/events.h"
#include "johto/kanto_travel.h"
#include "johto/save.h"
#include "constants/heal_locations.h"
#include "constants/johto_content.h"
#include "constants/maps.h"
#include "constants/region_map_sections.h"
#include "test/test.h"

static void SetCurrentMap(u16 map, u16 mapSecId)
{
    gSaveBlock1Ptr->location.mapGroup = MAP_GROUP(map);
    gSaveBlock1Ptr->location.mapNum = MAP_NUM(map);
    gMapHeader.regionMapSectionId = mapSecId;
}

static void SetActiveHeal(u16 healLocationId)
{
    const struct HealLocation *healLocation = GetHealLocation(healLocationId);
    gSaveBlock1Ptr->lastHealLocation.mapGroup = healLocation->mapGroup;
    gSaveBlock1Ptr->lastHealLocation.mapNum = healLocation->mapNum;
    gSaveBlock1Ptr->lastHealLocation.warpId = WARP_ID_NONE;
    gSaveBlock1Ptr->lastHealLocation.x = healLocation->x;
    gSaveBlock1Ptr->lastHealLocation.y = healLocation->y;
}

static struct JohtoSaveV1 ExpectedWithPendingCleared(void)
{
    struct JohtoSaveV1 expected = gSaveblock1.johto;
    expected.variables[JOHTO_VAR_PENDING_KANTO_DESTINATION - JOHTO_VAR_START] = 0;
    EXPECT(JohtoSave_Seal(&expected));
    return expected;
}

static void ExpectActiveHeal(u16 healLocationId)
{
    EXPECT_EQ(GetHealLocationIndexByWarpData(&gSaveBlock1Ptr->lastHealLocation), healLocationId);
}

TEST("Travel runtime allocations are stable and inside the reserved save window")
{
    EXPECT_EQ(JOHTO_FLAG_KANTO_LATER_INITIALIZED, JOHTO_FLAG_END);
    EXPECT_EQ(JOHTO_VAR_PENDING_KANTO_DESTINATION, JOHTO_VAR_END - 3);
    EXPECT_EQ(JOHTO_VAR_LAST_HEAL_JOHTO, JOHTO_VAR_END - 2);
    EXPECT_EQ(JOHTO_VAR_LAST_HEAL_KANTO_ORIGINAL, JOHTO_VAR_END - 1);
    EXPECT_EQ(JOHTO_VAR_LAST_HEAL_KANTO_LATER, JOHTO_VAR_END);
}

TEST("Prepare validates a crossing without mutating heal or Johto bytes")
{
    struct JohtoSaveV1 before;
    struct WarpData healBefore;
    JohtoSave_InitializeCurrent();
    SetCurrentMap(MAP_NEW_BARK_TOWN, MAPSEC_NEW_BARK_TOWN);
    SetActiveHeal(HEAL_LOCATION_JOHTO_CHERRYGROVE_CITY);
    EXPECT(JohtoTravel_SetPendingDestination(JOHTO_TRAVEL_DESTINATION_KANTO_LATER));
    before = gSaveblock1.johto;
    healBefore = gSaveBlock1Ptr->lastHealLocation;
    EXPECT(JohtoTravel_PrepareCrossing());
    EXPECT_EQ(memcmp(&before, &gSaveblock1.johto, sizeof(before)), 0);
    EXPECT_EQ(memcmp(&healBefore, &gSaveBlock1Ptr->lastHealLocation, sizeof(healBefore)), 0);
}

TEST("Johto and both Kanto eras restore independent heals on round trips")
{
    JohtoSave_InitializeCurrent();
    SetCurrentMap(MAP_NEW_BARK_TOWN, MAPSEC_NEW_BARK_TOWN);
    EXPECT(JohtoTravel_RecordCurrentHeal(HEAL_LOCATION_JOHTO_CHERRYGROVE_CITY));
    EXPECT(JohtoTravel_SetPendingDestination(JOHTO_TRAVEL_DESTINATION_KANTO_ORIGINAL));
    EXPECT(JohtoTravel_PrepareCrossing());
    SetCurrentMap(MAP_PALLET_TOWN, MAPSEC_PALLET_TOWN);
    EXPECT(JohtoTravel_CommitCrossing());
    ExpectActiveHeal(HEAL_LOCATION_PALLET_TOWN);
    EXPECT(JohtoTravel_RecordCurrentHeal(HEAL_LOCATION_CERULEAN_CITY));

    EXPECT(JohtoTravel_SetPendingDestination(JOHTO_TRAVEL_DESTINATION_JOHTO));
    EXPECT(JohtoTravel_PrepareCrossing());
    SetCurrentMap(MAP_NEW_BARK_TOWN, MAPSEC_NEW_BARK_TOWN);
    EXPECT(JohtoTravel_CommitCrossing());
    ExpectActiveHeal(HEAL_LOCATION_JOHTO_CHERRYGROVE_CITY);

    EXPECT(JohtoTravel_SetPendingDestination(JOHTO_TRAVEL_DESTINATION_KANTO_LATER));
    EXPECT(JohtoTravel_PrepareCrossing());
    SetCurrentMap(MAP_KANTO_LATER_PALLET_TOWN, MAPSEC_PALLET_TOWN);
    EXPECT(JohtoTravel_CommitCrossing());
    ExpectActiveHeal(HEAL_LOCATION_KANTO_LATER_PALLET_TOWN);
    EXPECT(JohtoTravel_RecordCurrentHeal(HEAL_LOCATION_KANTO_LATER_VIRIDIAN_CITY));

    EXPECT(JohtoTravel_SetPendingDestination(JOHTO_TRAVEL_DESTINATION_KANTO_ORIGINAL));
    EXPECT(JohtoTravel_PrepareCrossing());
    SetCurrentMap(MAP_PALLET_TOWN, MAPSEC_PALLET_TOWN);
    EXPECT(JohtoTravel_CommitCrossing());
    ExpectActiveHeal(HEAL_LOCATION_CERULEAN_CITY);
    EXPECT(JohtoTravel_SetPendingDestination(JOHTO_TRAVEL_DESTINATION_KANTO_LATER));
    EXPECT(JohtoTravel_PrepareCrossing());
    SetCurrentMap(MAP_KANTO_LATER_PALLET_TOWN, MAPSEC_PALLET_TOWN);
    EXPECT(JohtoTravel_CommitCrossing());
    ExpectActiveHeal(HEAL_LOCATION_KANTO_LATER_VIRIDIAN_CITY);
}

TEST("Wrong-map commit clears pending and leaves every other byte unchanged")
{
    struct JohtoSaveV1 expected;
    struct WarpData healBefore;
    JohtoSave_InitializeCurrent();
    SetCurrentMap(MAP_NEW_BARK_TOWN, MAPSEC_NEW_BARK_TOWN);
    SetActiveHeal(HEAL_LOCATION_JOHTO_NEW_BARK_TOWN);
    EXPECT(JohtoTravel_SetPendingDestination(JOHTO_TRAVEL_DESTINATION_KANTO_LATER));
    expected = ExpectedWithPendingCleared();
    healBefore = gSaveBlock1Ptr->lastHealLocation;
    EXPECT(!JohtoTravel_CommitCrossing());
    EXPECT_EQ(memcmp(&expected, &gSaveblock1.johto, sizeof(expected)), 0);
    EXPECT_EQ(memcmp(&healBefore, &gSaveBlock1Ptr->lastHealLocation, sizeof(healBefore)), 0);
}

TEST("Cancel and failed prepare clear pending without changing other state")
{
    struct JohtoSaveV1 expected;
    struct WarpData healBefore;
    JohtoSave_InitializeCurrent();
    SetCurrentMap(MAP_NEW_BARK_TOWN, MAPSEC_NEW_BARK_TOWN);
    SetActiveHeal(HEAL_LOCATION_JOHTO_NEW_BARK_TOWN);
    EXPECT(JohtoEvent_SetVariable(JOHTO_VAR_LAST_HEAL_JOHTO, HEAL_LOCATION_JOHTO_CHERRYGROVE_CITY));
    EXPECT(JohtoTravel_SetPendingDestination(JOHTO_TRAVEL_DESTINATION_JOHTO));
    expected = ExpectedWithPendingCleared();
    healBefore = gSaveBlock1Ptr->lastHealLocation;
    EXPECT(!JohtoTravel_PrepareCrossing());
    EXPECT_EQ(memcmp(&expected, &gSaveblock1.johto, sizeof(expected)), 0);
    EXPECT_EQ(memcmp(&healBefore, &gSaveBlock1Ptr->lastHealLocation, sizeof(healBefore)), 0);
    EXPECT(JohtoTravel_SetPendingDestination(JOHTO_TRAVEL_DESTINATION_KANTO_ORIGINAL));
    expected = ExpectedWithPendingCleared();
    EXPECT(JohtoTravel_Cancel());
    EXPECT_EQ(memcmp(&expected, &gSaveblock1.johto, sizeof(expected)), 0);
}

TEST("Semantic corruption with a valid CRC normalizes pending and reloads")
{
    struct JohtoSaveV1 persisted;
    JohtoSave_InitializeCurrent();
    SetCurrentMap(MAP_NEW_BARK_TOWN, MAPSEC_NEW_BARK_TOWN);
    gSaveblock1.johto.variables[JOHTO_VAR_PENDING_KANTO_DESTINATION - JOHTO_VAR_START] = 0xFFFF;
    EXPECT(JohtoSave_Seal(&gSaveblock1.johto));
    EXPECT(!JohtoTravel_PrepareCrossing());
    EXPECT_EQ(JohtoTravel_GetPendingDestination(), JOHTO_TRAVEL_DESTINATION_NONE);
    EXPECT(JohtoSave_Validate(&gSaveblock1.johto));
    persisted = gSaveblock1.johto;
    memset(&gSaveblock1.johto, 0xA5, sizeof(gSaveblock1.johto));
    gSaveblock1.johto = persisted;
    EXPECT_EQ(JohtoSave_Load(), JOHTO_SAVE_LOAD_READY);
    EXPECT_EQ(memcmp(&persisted, &gSaveblock1.johto, sizeof(persisted)), 0);
}

TEST("Per-world travel state survives a save-copy reload with a valid CRC")
{
    struct JohtoSaveV1 persisted;

    JohtoSave_InitializeCurrent();
    EXPECT(JohtoEvent_SetVariable(JOHTO_VAR_LAST_HEAL_JOHTO,
                                   HEAL_LOCATION_JOHTO_CHERRYGROVE_CITY));
    EXPECT(JohtoEvent_SetVariable(JOHTO_VAR_LAST_HEAL_KANTO_ORIGINAL,
                                   HEAL_LOCATION_CERULEAN_CITY));
    EXPECT(JohtoEvent_SetVariable(JOHTO_VAR_LAST_HEAL_KANTO_LATER,
                                   HEAL_LOCATION_KANTO_LATER_VIRIDIAN_CITY));
    EXPECT(JohtoTravel_SetPendingDestination(JOHTO_TRAVEL_DESTINATION_KANTO_LATER));
    persisted = gSaveblock1.johto;
    EXPECT(JohtoSave_Validate(&persisted));

    JohtoSave_InitializeCurrent();
    gSaveblock1.johto = persisted;
    EXPECT_EQ(JohtoSave_Load(), JOHTO_SAVE_LOAD_READY);
    EXPECT_EQ(JohtoTravel_GetPendingDestination(),
              JOHTO_TRAVEL_DESTINATION_KANTO_LATER);
    EXPECT_EQ(JohtoEvent_GetVariable(JOHTO_VAR_LAST_HEAL_JOHTO),
              HEAL_LOCATION_JOHTO_CHERRYGROVE_CITY);
    EXPECT_EQ(JohtoEvent_GetVariable(JOHTO_VAR_LAST_HEAL_KANTO_ORIGINAL),
              HEAL_LOCATION_CERULEAN_CITY);
    EXPECT_EQ(JohtoEvent_GetVariable(JOHTO_VAR_LAST_HEAL_KANTO_LATER),
              HEAL_LOCATION_KANTO_LATER_VIRIDIAN_CITY);
}

TEST("Later initialization is requested and marked only after real setup")
{
    JohtoSave_InitializeCurrent();
    SetCurrentMap(MAP_NEW_BARK_TOWN, MAPSEC_NEW_BARK_TOWN);
    EXPECT(!JohtoTravel_NeedsLaterKantoInitialization());
    EXPECT(!JohtoTravel_MarkLaterKantoInitialized());
    EXPECT(!JohtoTravel_IsLaterInitialized());
    SetCurrentMap(MAP_KANTO_LATER_PALLET_TOWN, MAPSEC_PALLET_TOWN);
    EXPECT(JohtoTravel_NeedsLaterKantoInitialization());
    EXPECT(JohtoTravel_MarkLaterKantoInitialized());
    EXPECT(JohtoTravel_IsLaterInitialized());
    EXPECT(!JohtoTravel_NeedsLaterKantoInitialization());
    EXPECT(JohtoTravel_MarkLaterKantoInitialized());
}
