#include "global.h"
#include "event_data.h"
#include "region_map.h"
#include "constants/heal_locations.h"
#include "constants/johto_content.h"
#include "constants/flags.h"
#include "constants/maps.h"
#include "constants/region_map_sections.h"
#include "johto/save.h"
#include "save.h"
#include "test/test.h"

static void SetCurrentMap(u8 mapGroup, u8 mapNum, u16 mapSecId)
{
    gSaveBlock1Ptr->location.mapGroup = mapGroup;
    gSaveBlock1Ptr->location.mapNum = mapNum;
    gMapHeader.regionMapSectionId = mapSecId;
}

static void ExpectFlyDestination(u16 mapSecId, u16 healLocation)
{
    struct RegionMap regionMap = { .mapSecId = mapSecId };

    EXPECT_EQ(FilterFlyDestination(&regionMap), healLocation);
}

TEST("Region map identity accepts the complete registered Johto and later Kanto ranges")
{
    EXPECT_EQ(GetRegionMapTypeByMap(MAP_GROUP(MAP_NEW_BARK_TOWN), MAP_NUM(MAP_NEW_BARK_TOWN), MAPSEC_PALLET_TOWN), REGION_MAP_JOHTO);
    EXPECT_EQ(GetRegionMapTypeByMap(MAP_GROUP(MAP_GATE_ILEX_FOREST_ROUTE34), MAP_NUM(MAP_GATE_ILEX_FOREST_ROUTE34), MAPSEC_PALLET_TOWN), REGION_MAP_JOHTO);
    EXPECT_EQ(GetRegionMapTypeByMap(MAP_GROUP(MAP_SAFARI_ZONE_TOP_RIGHT), MAP_NUM(MAP_SAFARI_ZONE_TOP_RIGHT), MAPSEC_LITTLEROOT_TOWN), REGION_MAP_JOHTO);
    EXPECT_EQ(GetRegionMapTypeByMap(MAP_GROUP(MAP_KANTO_LATER_PALLET_TOWN), MAP_NUM(MAP_KANTO_LATER_PALLET_TOWN), MAPSEC_NEW_BARK_TOWN), REGION_MAP_KANTO);
    EXPECT_EQ(GetRegionMapTypeByMap(MAP_GROUP(MAP_KANTO_LATER_MT_MOON_SHOP), MAP_NUM(MAP_KANTO_LATER_MT_MOON_SHOP), MAPSEC_NEW_BARK_TOWN), REGION_MAP_KANTO);
    EXPECT_EQ(GetRegionMapTypeByMap(MAP_GROUP(MAP_KANTO_LATER_ROUTE19_CAVE), MAP_NUM(MAP_KANTO_LATER_ROUTE19_CAVE), MAPSEC_NEW_BARK_TOWN), REGION_MAP_KANTO);
}

TEST("Region map identity rejects unregistered tails in reserved groups")
{
    EXPECT_EQ(GetRegionMapTypeByMap(MAP_GROUP(MAP_GATE_ILEX_FOREST_ROUTE34), MAP_NUM(MAP_GATE_ILEX_FOREST_ROUTE34) + 1, MAPSEC_NEW_BARK_TOWN), REGION_MAP_HOENN);
    EXPECT_EQ(GetRegionMapTypeByMap(MAP_GROUP(MAP_SAFARI_ZONE_TOP_RIGHT), MAP_NUM(MAP_SAFARI_ZONE_TOP_RIGHT) + 1, MAPSEC_NEW_BARK_TOWN), REGION_MAP_HOENN);
    EXPECT_EQ(GetRegionMapTypeByMap(MAP_GROUP(MAP_KANTO_LATER_MT_MOON_SHOP), MAP_NUM(MAP_KANTO_LATER_MT_MOON_SHOP) + 1, MAPSEC_PALLET_TOWN), REGION_MAP_HOENN);
    EXPECT_EQ(GetRegionMapTypeByMap(MAP_GROUP(MAP_KANTO_LATER_ROUTE19_CAVE), MAP_NUM(MAP_KANTO_LATER_ROUTE19_CAVE) + 1, MAPSEC_PALLET_TOWN), REGION_MAP_HOENN);
    EXPECT_EQ(GetKantoEraByMap(MAP_GROUP(MAP_GATE_ILEX_FOREST_ROUTE34), MAP_NUM(MAP_GATE_ILEX_FOREST_ROUTE34) + 1, MAPSEC_NEW_BARK_TOWN), KANTO_ERA_NONE);
    EXPECT_EQ(GetKantoEraByMap(MAP_GROUP(MAP_SAFARI_ZONE_TOP_RIGHT), MAP_NUM(MAP_SAFARI_ZONE_TOP_RIGHT) + 1, MAPSEC_NEW_BARK_TOWN), KANTO_ERA_NONE);
    EXPECT_EQ(GetKantoEraByMap(MAP_GROUP(MAP_KANTO_LATER_MT_MOON_SHOP), MAP_NUM(MAP_KANTO_LATER_MT_MOON_SHOP) + 1, MAPSEC_PALLET_TOWN), KANTO_ERA_NONE);
    EXPECT_EQ(GetKantoEraByMap(MAP_GROUP(MAP_KANTO_LATER_ROUTE19_CAVE), MAP_NUM(MAP_KANTO_LATER_ROUTE19_CAVE) + 1, MAPSEC_PALLET_TOWN), KANTO_ERA_NONE);
}

TEST("Kanto era identity remains separate from region map type")
{
    EXPECT_EQ(GetKantoEraByMap(77, 0, MAPSEC_PALLET_TOWN), KANTO_ERA_LATER);
    EXPECT_EQ(GetKantoEraByMap(78, 0, MAPSEC_VIRIDIAN_CITY), KANTO_ERA_LATER);
    EXPECT_EQ(GetKantoEraByMap(MAP_GROUP(MAP_PALLET_TOWN), MAP_NUM(MAP_PALLET_TOWN), MAPSEC_PALLET_TOWN), KANTO_ERA_ORIGINAL);
    EXPECT_EQ(GetKantoEraByMap(MAP_GROUP(MAP_LITTLEROOT_TOWN), MAP_NUM(MAP_LITTLEROOT_TOWN), MAPSEC_LITTLEROOT_TOWN), KANTO_ERA_NONE);
}

TEST("Existing Hoenn and Sevii region map classifications are preserved")
{
    EXPECT_EQ(GetRegionMapTypeByMap(MAP_GROUP(MAP_LITTLEROOT_TOWN), MAP_NUM(MAP_LITTLEROOT_TOWN), MAPSEC_LITTLEROOT_TOWN), REGION_MAP_HOENN);
    EXPECT_EQ(GetRegionMapTypeByMap(MAP_GROUP(MAP_ONE_ISLAND), MAP_NUM(MAP_ONE_ISLAND), MAPSEC_ONE_ISLAND), REGION_MAP_SEVII123);
    EXPECT_EQ(GetKantoEraByMap(MAP_GROUP(MAP_ONE_ISLAND), MAP_NUM(MAP_ONE_ISLAND), MAPSEC_ONE_ISLAND), KANTO_ERA_NONE);
}

TEST("Forced Flight selects era-specific Kanto destinations")
{
    struct RegionMap regionMap = { .mapSecId = MAPSEC_PALLET_TOWN };

    SetForcedFlightRegion(REGION_MAP_KANTO);
    EXPECT_EQ(FilterFlyDestination(&regionMap), HEAL_LOCATION_PALLET_TOWN);

    SetForcedFlightRegionWithKantoEra(REGION_MAP_KANTO, KANTO_ERA_LATER);
    EXPECT_EQ(FilterFlyDestination(&regionMap), HEAL_LOCATION_KANTO_LATER_PALLET_TOWN);

    ClearForcedFlightRegion();
}

TEST("Non-forced Flight uses the current later Kanto identity")
{
    ClearForcedFlightRegion();
    SetCurrentMap(MAP_GROUP(MAP_KANTO_LATER_PALLET_TOWN), MAP_NUM(MAP_KANTO_LATER_PALLET_TOWN), MAPSEC_PALLET_TOWN);
    ExpectFlyDestination(MAPSEC_PALLET_TOWN, HEAL_LOCATION_KANTO_LATER_PALLET_TOWN);
}

TEST("Clearing forced Flight clears both its region and Kanto era")
{
    SetCurrentMap(MAP_GROUP(MAP_PALLET_TOWN), MAP_NUM(MAP_PALLET_TOWN), MAPSEC_PALLET_TOWN);
    SetForcedFlightRegionWithKantoEra(REGION_MAP_KANTO, KANTO_ERA_LATER);
    ExpectFlyDestination(MAPSEC_PALLET_TOWN, HEAL_LOCATION_KANTO_LATER_PALLET_TOWN);
    ClearForcedFlightRegion();
    ExpectFlyDestination(MAPSEC_PALLET_TOWN, HEAL_LOCATION_PALLET_TOWN);

    SetCurrentMap(MAP_GROUP(MAP_KANTO_LATER_PALLET_TOWN), MAP_NUM(MAP_KANTO_LATER_PALLET_TOWN), MAPSEC_PALLET_TOWN);
    SetForcedFlightRegionWithKantoEra(REGION_MAP_KANTO, KANTO_ERA_ORIGINAL);
    ExpectFlyDestination(MAPSEC_PALLET_TOWN, HEAL_LOCATION_PALLET_TOWN);
    ClearForcedFlightRegion();
    ExpectFlyDestination(MAPSEC_PALLET_TOWN, HEAL_LOCATION_KANTO_LATER_PALLET_TOWN);
}

TEST("Ordinary local Flight retains each current world context")
{
    ClearForcedFlightRegion();
    SetCurrentMap(MAP_GROUP(MAP_NEW_BARK_TOWN), MAP_NUM(MAP_NEW_BARK_TOWN), MAPSEC_NEW_BARK_TOWN);
    ExpectFlyDestination(MAPSEC_NEW_BARK_TOWN, HEAL_LOCATION_JOHTO_NEW_BARK_TOWN);

    SetCurrentMap(MAP_GROUP(MAP_PALLET_TOWN), MAP_NUM(MAP_PALLET_TOWN), MAPSEC_PALLET_TOWN);
    ExpectFlyDestination(MAPSEC_PALLET_TOWN, HEAL_LOCATION_PALLET_TOWN);

    SetCurrentMap(MAP_GROUP(MAP_KANTO_LATER_PALLET_TOWN), MAP_NUM(MAP_KANTO_LATER_PALLET_TOWN), MAPSEC_PALLET_TOWN);
    ExpectFlyDestination(MAPSEC_PALLET_TOWN, HEAL_LOCATION_KANTO_LATER_PALLET_TOWN);
}

TEST("Later Kanto Flight filters every city to its later heal location")
{
    SetForcedFlightRegionWithKantoEra(REGION_MAP_KANTO, KANTO_ERA_LATER);
    ExpectFlyDestination(MAPSEC_PALLET_TOWN, HEAL_LOCATION_KANTO_LATER_PALLET_TOWN);
    ExpectFlyDestination(MAPSEC_VIRIDIAN_CITY, HEAL_LOCATION_KANTO_LATER_VIRIDIAN_CITY);
    ExpectFlyDestination(MAPSEC_PEWTER_CITY, HEAL_LOCATION_KANTO_LATER_PEWTER_CITY);
    ExpectFlyDestination(MAPSEC_CERULEAN_CITY, HEAL_LOCATION_KANTO_LATER_CERULEAN_CITY);
    ExpectFlyDestination(MAPSEC_LAVENDER_TOWN, HEAL_LOCATION_KANTO_LATER_LAVENDER_TOWN);
    ExpectFlyDestination(MAPSEC_VERMILION_CITY, HEAL_LOCATION_KANTO_LATER_VERMILION_CITY);
    ExpectFlyDestination(MAPSEC_CELADON_CITY, HEAL_LOCATION_KANTO_LATER_CELADON_CITY);
    ExpectFlyDestination(MAPSEC_FUCHSIA_CITY, HEAL_LOCATION_KANTO_LATER_FUCHSIA_CITY);
    ExpectFlyDestination(MAPSEC_SAFFRON_CITY, HEAL_LOCATION_KANTO_LATER_SAFFRON_CITY);
    ExpectFlyDestination(MAPSEC_CINNABAR_ISLAND, HEAL_LOCATION_KANTO_LATER_CINNABAR_ISLAND);
    ClearForcedFlightRegion();
}

TEST("Johto Flight selection eligibility uses the Johto visit flag")
{
    JohtoSave_InitializeCurrent();
    SetForcedFlightRegion(REGION_MAP_JOHTO);
    FlagClear(JOHTO_FLAG_VISITED_NEWBARK_TOWN);
    EXPECT_EQ(CanFlyToRegionMapSection(MAPSEC_NEW_BARK_TOWN), FALSE);
    FlagSet(JOHTO_FLAG_VISITED_NEWBARK_TOWN);
    EXPECT_EQ(CanFlyToRegionMapSection(MAPSEC_NEW_BARK_TOWN), TRUE);
    FlagClear(JOHTO_FLAG_VISITED_NEWBARK_TOWN);
    ClearForcedFlightRegion();
}

TEST("Later Kanto Flight selection eligibility uses the later visit flag")
{
    JohtoSave_InitializeCurrent();
    SetForcedFlightRegionWithKantoEra(REGION_MAP_KANTO, KANTO_ERA_LATER);
    FlagSet(FLAG_WORLD_MAP_PALLET_TOWN);
    FlagClear(JOHTO_FLAG_VISITED_PALLET_TOWN);
    EXPECT_EQ(CanFlyToRegionMapSection(MAPSEC_PALLET_TOWN), FALSE);
    FlagSet(JOHTO_FLAG_VISITED_PALLET_TOWN);
    EXPECT_EQ(CanFlyToRegionMapSection(MAPSEC_PALLET_TOWN), TRUE);
    FlagClear(JOHTO_FLAG_VISITED_PALLET_TOWN);
    FlagClear(FLAG_WORLD_MAP_PALLET_TOWN);
    ClearForcedFlightRegion();
}

TEST("Original Kanto Flight selection eligibility keeps the original visit flag")
{
    SetForcedFlightRegionWithKantoEra(REGION_MAP_KANTO, KANTO_ERA_ORIGINAL);
    FlagSet(JOHTO_FLAG_VISITED_PALLET_TOWN);
    FlagClear(FLAG_WORLD_MAP_PALLET_TOWN);
    EXPECT_EQ(CanFlyToRegionMapSection(MAPSEC_PALLET_TOWN), FALSE);
    FlagSet(FLAG_WORLD_MAP_PALLET_TOWN);
    EXPECT_EQ(CanFlyToRegionMapSection(MAPSEC_PALLET_TOWN), TRUE);
    FlagClear(FLAG_WORLD_MAP_PALLET_TOWN);
    FlagClear(JOHTO_FLAG_VISITED_PALLET_TOWN);
    ClearForcedFlightRegion();
}
