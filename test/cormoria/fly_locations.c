#include "global.h"
#include "event_data.h"
#include "heal_location.h"
#include "region_map.h"
#include "world/event_save.h"
#include "cormoria/heal_locations.h"
#include "constants/cormoria_event_ids.h"
#include "constants/maps.h"
#include "constants/region_map_sections.h"
#include "test/test.h"

#if ROM_WORLD == 2

struct CormoriaFlyCase
{
    u16 map;
    mapsec_u16_t section;
    u16 flag;
    u16 healLocation;
};

static const struct CormoriaFlyCase sFlyCases[] =
{
    {MAP_CORMORIA_CARABRUE_TOWN, MAPSEC_CORMORIA_CARABRUE_TOWN, Cormoria_FLAG_VISITED_CARABRUE_TOWN, HEAL_LOCATION_CORMORIA_CARABRUE_TOWN},
    {MAP_CORMORIA_FENNILAHL_TOWN, MAPSEC_CORMORIA_FENNILAHL_TOWN, Cormoria_FLAG_VISITED_FENNILAHL_TOWN, HEAL_LOCATION_CORMORIA_FENNILAHL_TOWN},
    {MAP_CORMORIA_GASTREE_CITY, MAPSEC_CORMORIA_GASTREE_CITY, Cormoria_FLAG_VISITED_GASTREE_CITY, HEAL_LOCATION_CORMORIA_GASTREE_CITY},
    {MAP_CORMORIA_CERAM_BASE_CAMP, MAPSEC_CORMORIA_CERAM_BASE_CAMP, Cormoria_FLAG_VISITED_CERAM_BASE_CAMP, HEAL_LOCATION_CORMORIA_CERAM_BASE_CAMP},
    {MAP_CORMORIA_GALECREST_CITY, MAPSEC_CORMORIA_GALECREST_CITY, Cormoria_FLAG_VISITED_GALECREST_CITY, HEAL_LOCATION_CORMORIA_GALECREST_CITY},
    {MAP_CORMORIA_SILVERSUN_CITY, MAPSEC_CORMORIA_SILVERSUN_CITY, Cormoria_FLAG_VISITED_SILVERSUN_CITY, HEAL_LOCATION_CORMORIA_SILVERSUN_CITY},
    {MAP_CORMORIA_PELLUCA_CITY, MAPSEC_CORMORIA_PELLUCA_CITY, Cormoria_FLAG_VISITED_PELLUCA_CITY, HEAL_LOCATION_CORMORIA_PELLUCA_CITY},
    {MAP_CORMORIA_MIRROH_BASE_CAMP, MAPSEC_CORMORIA_MIRROH_BASE_CAMP, Cormoria_FLAG_VISITED_MIRROH_BASE_CAMP, HEAL_LOCATION_CORMORIA_MIRROH_BASE_CAMP},
    {MAP_CORMORIA_WINTERLILY_HOLLOW, MAPSEC_CORMORIA_WINTERLILY_HOLLOW, Cormoria_FLAG_VISITED_WINTERLILY_HOLLOW, HEAL_LOCATION_CORMORIA_WINTERLILY_HOLLOW},
    {MAP_CORMORIA_RIVETSHORE_CITY, MAPSEC_CORMORIA_RIVETSHORE_CITY, Cormoria_FLAG_VISITED_RIVETSHORE_CITY, HEAL_LOCATION_CORMORIA_RIVETSHORE_CITY},
    {MAP_CORMORIA_CHAMPIONSHIP, MAPSEC_CORMORIA_VICTORY_CAPE, Cormoria_FLAG_VISITED_VICTORY_CAPE, HEAL_LOCATION_CORMORIA_VICTORY_CAPE},
};

TEST("All 17 Cormoria heal points round-trip through the shared lookup")
{
    u32 id;

    for (id = HEAL_LOCATION_CORMORIA_CARABRUE_TOWN; id < NUM_CORMORIA_HEAL_LOCATIONS; id++)
    {
        const struct HealLocation *heal = GetHealLocation(id);
        struct WarpData warp;

        EXPECT(heal != NULL);
        EXPECT_EQ(GetHealLocationIndexByMap(heal->mapGroup, heal->mapNum), id);
        warp.mapGroup = heal->mapGroup;
        warp.mapNum = heal->mapNum;
        warp.x = heal->x;
        warp.y = heal->y;
        EXPECT_EQ(GetHealLocationIndexByWarpData(&warp), id);
        EXPECT_EQ(GetHealNpcLocalId(id), 0);
    }
    EXPECT(GetHealLocation(NUM_CORMORIA_HEAL_LOCATIONS) == NULL);
}

TEST("Cormoria Fly destinations retain their outdoor heal coordinates")
{
    u32 i;

    SetForcedFlightRegion(REGION_MAP_CORMORIA);
    for (i = 0; i < ARRAY_COUNT(sFlyCases); i++)
    {
        const struct HealLocation *heal = GetHealLocation(sFlyCases[i].healLocation);
        struct RegionMap regionMap = {.mapSecId = sFlyCases[i].section};

        EXPECT(heal != NULL);
        EXPECT_EQ(heal->mapGroup, MAP_GROUP(sFlyCases[i].map));
        EXPECT_EQ(heal->mapNum, MAP_NUM(sFlyCases[i].map));
        EXPECT_EQ(GetHealLocationIndexByMap(heal->mapGroup, heal->mapNum), sFlyCases[i].healLocation);
        EXPECT_EQ(FilterFlyDestination(&regionMap), sFlyCases[i].healLocation);
    }
    ClearForcedFlightRegion();
}

TEST("Cormoria Fly requires each destination's own visited flag")
{
    u32 i;

    WorldEventSave_InitializeCurrent();
    SetForcedFlightRegion(REGION_MAP_CORMORIA);
    for (i = 0; i < ARRAY_COUNT(sFlyCases); i++)
    {
        bool32 previouslyVisited = FlagGet(sFlyCases[i].flag);

        FlagClear(sFlyCases[i].flag);
        EXPECT(!CanFlyToRegionMapSection(sFlyCases[i].section));
        FlagSet(sFlyCases[i].flag);
        EXPECT(CanFlyToRegionMapSection(sFlyCases[i].section));
        if (!previouslyVisited)
            FlagClear(sFlyCases[i].flag);
    }
    ClearForcedFlightRegion();
}
#endif
