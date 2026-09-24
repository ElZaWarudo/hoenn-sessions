#include "global.h"
#include "event_data.h"
#include "heal_location.h"
#include "constants/event_objects.h"
#include "constants/heal_locations.h"
#include "constants/maps.h"

#include "data/heal_locations.h"
#if ROM_WORLD == 2
#include "cormoria/heal_locations.h"
#include "cormoria/heal_locations_data.h"
STATIC_ASSERT(ARRAY_COUNT(sCormoriaHealLocations) == NUM_CORMORIA_HEAL_LOCATIONS - HEAL_LOCATION_CORMORIA_CARABRUE_TOWN, CormoriaHealLocationCountMismatch);
#endif

u32 GetHealLocationIndexByMap(u16 mapGroup, u16 mapNum)
{
    u32 i;

    for (i = 0; i < ARRAY_COUNT(sHealLocations); i++)
    {
        if (sHealLocations[i].mapGroup == mapGroup && sHealLocations[i].mapNum == mapNum)
            return i + 1;
    }
#if ROM_WORLD == 2
    for (i = 0; i < ARRAY_COUNT(sCormoriaHealLocations); i++)
    {
        if (sCormoriaHealLocations[i].mapGroup == mapGroup && sCormoriaHealLocations[i].mapNum == mapNum)
            return HEAL_LOCATION_CORMORIA_CARABRUE_TOWN + i;
    }
#endif
    return HEAL_LOCATION_NONE;
}

const struct HealLocation *GetHealLocationByMap(u16 mapGroup, u16 mapNum)
{
    u32 index = GetHealLocationIndexByMap(mapGroup, mapNum);

    return GetHealLocation(index);
}

u32 GetHealLocationIndexByWarpData(struct WarpData *warp)
{
    u32 i;
    for (i = 0; i < ARRAY_COUNT(sHealLocations); i++)
    {
        if (sHealLocations[i].mapGroup == warp->mapGroup
        && sHealLocations[i].mapNum == warp->mapNum
        && sHealLocations[i].x == warp->x
        && sHealLocations[i].y == warp->y)
            return i + 1;
    }
#if ROM_WORLD == 2
    for (i = 0; i < ARRAY_COUNT(sCormoriaHealLocations); i++)
    {
        if (sCormoriaHealLocations[i].mapGroup == warp->mapGroup
         && sCormoriaHealLocations[i].mapNum == warp->mapNum
         && sCormoriaHealLocations[i].x == warp->x
         && sCormoriaHealLocations[i].y == warp->y)
            return HEAL_LOCATION_CORMORIA_CARABRUE_TOWN + i;
    }
#endif
    return HEAL_LOCATION_NONE;
}

const struct HealLocation *GetHealLocation(u32 index)
{
    if (index == HEAL_LOCATION_NONE)
        return NULL;
    else if (index <= ARRAY_COUNT(sHealLocations))
        return &sHealLocations[index - 1];
#if ROM_WORLD == 2
    else if (index < NUM_CORMORIA_HEAL_LOCATIONS)
        return &sCormoriaHealLocations[index - HEAL_LOCATION_CORMORIA_CARABRUE_TOWN];
#endif
    return NULL;
}

static bool32 IsLastHealLocation(u32 healLocation)
{
    const struct HealLocation *loc = GetHealLocation(healLocation);
    const struct WarpData *warpData = &gSaveBlock1Ptr->lastHealLocation;

    return warpData->mapGroup == loc->mapGroup
        && warpData->mapNum == loc->mapNum
        && warpData->warpId == WARP_ID_NONE
        && warpData->x == loc->x
        && warpData->y == loc->y;
}

bool32 IsLastHealLocationPlayerHouse()
{
    if (IsLastHealLocation(HEAL_LOCATION_LITTLEROOT_TOWN_MAYS_HOUSE)
        || IsLastHealLocation(HEAL_LOCATION_LITTLEROOT_TOWN_MAYS_HOUSE_2F)
        || IsLastHealLocation(HEAL_LOCATION_LITTLEROOT_TOWN_BRENDANS_HOUSE)
        || IsLastHealLocation(HEAL_LOCATION_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F)
        || IsLastHealLocation(HEAL_LOCATION_PALLET_TOWN))
        return TRUE;

    return FALSE;
}

u32 GetHealNpcLocalId(u32 healLocationId)
{
#if ROM_WORLD == 2
    if (healLocationId >= HEAL_LOCATION_CORMORIA_CARABRUE_TOWN)
        return LOCALID_NONE;
#endif
    if (healLocationId == HEAL_LOCATION_NONE || healLocationId >= NUM_HEAL_LOCATIONS)
        return LOCALID_NONE;

    return sWhiteoutRespawnHealerNpcIds[healLocationId - 1];
}

void SetWhiteoutRespawnWarpAndHealerNPC(struct WarpData *warp)
{
    u32 healLocationId = GetHealLocationIndexByWarpData(&gSaveBlock1Ptr->lastHealLocation);
    u32 healNpcLocalId = GetHealNpcLocalId(healLocationId);

    if (!healNpcLocalId)
    {
        *(warp) = gSaveBlock1Ptr->lastHealLocation;
        return;
    }

    warp->mapGroup = sWhiteoutRespawnHealCenterMapIdxs[healLocationId - 1][0];
    warp->mapNum = sWhiteoutRespawnHealCenterMapIdxs[healLocationId - 1][1];
    warp->warpId = WARP_ID_NONE;
    warp->x = sWhiteoutRespawnHealCenterMapIdxs[healLocationId - 1][2];
    warp->y = sWhiteoutRespawnHealCenterMapIdxs[healLocationId - 1][3];
    gSpecialVar_LastTalked = healNpcLocalId;
    gSpecialVar_0x800B = healNpcLocalId;
}
