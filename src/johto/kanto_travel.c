#include "global.h"
#include "heal_location.h"
#include "load_save.h"
#include "overworld.h"
#include "region_map.h"
#include "johto/events.h"
#include "johto/kanto_travel.h"
#include "johto/save.h"
#include "constants/heal_locations.h"
#include "constants/johto_content.h"

static bool8 IsValidDestination(u16 destination)
{
    return destination >= JOHTO_TRAVEL_DESTINATION_JOHTO
        && destination <= JOHTO_TRAVEL_DESTINATION_KANTO_LATER;
}

static bool8 IsWorldContext(enum JohtoTravelContext context)
{
    return context >= JOHTO_TRAVEL_CONTEXT_JOHTO
        && context <= JOHTO_TRAVEL_CONTEXT_KANTO_LATER;
}

static enum JohtoTravelContext ContextForRegionMap(enum RegionMapType region,
                                                    enum KantoEra era)
{
    if (region == REGION_MAP_JOHTO)
        return JOHTO_TRAVEL_CONTEXT_JOHTO;
    if (region != REGION_MAP_KANTO)
        return JOHTO_TRAVEL_CONTEXT_UNKNOWN;
    if (era == KANTO_ERA_ORIGINAL)
        return JOHTO_TRAVEL_CONTEXT_KANTO_ORIGINAL;
    if (era == KANTO_ERA_LATER)
        return JOHTO_TRAVEL_CONTEXT_KANTO_LATER;
    return JOHTO_TRAVEL_CONTEXT_UNKNOWN;
}

enum JohtoTravelContext JohtoTravel_GetContextForMap(u8 mapGroup, u8 mapNum,
                                                       u16 mapSecId)
{
    return ContextForRegionMap(
        GetRegionMapTypeByMap(mapGroup, mapNum, mapSecId),
        GetKantoEraByMap(mapGroup, mapNum, mapSecId));
}

enum JohtoTravelContext JohtoTravel_GetCurrentContext(void)
{
    return JohtoTravel_GetContextForMap(gSaveBlock1Ptr->location.mapGroup,
                                        gSaveBlock1Ptr->location.mapNum,
                                        gMapHeader.regionMapSectionId);
}

enum JohtoTravelDestination JohtoTravel_GetPendingDestination(void)
{
    u16 value = JohtoEvent_GetVariable(JOHTO_VAR_PENDING_KANTO_DESTINATION);
    return IsValidDestination(value) ? value : JOHTO_TRAVEL_DESTINATION_NONE;
}

bool8 JohtoTravel_SetPendingDestination(enum JohtoTravelDestination destination)
{
    if (!IsValidDestination(destination))
        return FALSE;
    return JohtoEvent_SetVariable(JOHTO_VAR_PENDING_KANTO_DESTINATION, destination);
}

bool8 JohtoTravel_ClearPendingDestination(void)
{
    return JohtoEvent_SetVariable(JOHTO_VAR_PENDING_KANTO_DESTINATION,
                                  JOHTO_TRAVEL_DESTINATION_NONE);
}

bool8 JohtoTravel_Cancel(void)
{
    return JohtoTravel_ClearPendingDestination();
}

static enum JohtoTravelContext ContextForDestination(u16 destination)
{
    switch (destination)
    {
    case JOHTO_TRAVEL_DESTINATION_JOHTO:
        return JOHTO_TRAVEL_CONTEXT_JOHTO;
    case JOHTO_TRAVEL_DESTINATION_KANTO_ORIGINAL:
        return JOHTO_TRAVEL_CONTEXT_KANTO_ORIGINAL;
    case JOHTO_TRAVEL_DESTINATION_KANTO_LATER:
        return JOHTO_TRAVEL_CONTEXT_KANTO_LATER;
    default:
        return JOHTO_TRAVEL_CONTEXT_UNKNOWN;
    }
}

static u16 HealVariableForContext(enum JohtoTravelContext context)
{
    switch (context)
    {
    case JOHTO_TRAVEL_CONTEXT_JOHTO:
        return JOHTO_VAR_LAST_HEAL_JOHTO;
    case JOHTO_TRAVEL_CONTEXT_KANTO_ORIGINAL:
        return JOHTO_VAR_LAST_HEAL_KANTO_ORIGINAL;
    case JOHTO_TRAVEL_CONTEXT_KANTO_LATER:
        return JOHTO_VAR_LAST_HEAL_KANTO_LATER;
    default:
        return 0;
    }
}

static u16 FallbackHealForContext(enum JohtoTravelContext context)
{
    switch (context)
    {
    case JOHTO_TRAVEL_CONTEXT_JOHTO:
        return HEAL_LOCATION_JOHTO_NEW_BARK_TOWN;
    case JOHTO_TRAVEL_CONTEXT_KANTO_ORIGINAL:
        return HEAL_LOCATION_PALLET_TOWN;
    case JOHTO_TRAVEL_CONTEXT_KANTO_LATER:
        return HEAL_LOCATION_KANTO_LATER_PALLET_TOWN;
    default:
        return HEAL_LOCATION_NONE;
    }
}

static enum JohtoTravelContext ContextForHealLocation(u16 healLocationId)
{
    const struct HealLocation *healLocation = GetHealLocation(healLocationId);
    const struct MapHeader *mapHeader;

    if (healLocation == NULL)
        return JOHTO_TRAVEL_CONTEXT_UNKNOWN;
    mapHeader = Overworld_GetMapHeaderByGroupAndId((u8)healLocation->mapGroup,
                                                    (u8)healLocation->mapNum);
    if (mapHeader == NULL)
        return JOHTO_TRAVEL_CONTEXT_UNKNOWN;
    return JohtoTravel_GetContextForMap((u8)healLocation->mapGroup,
                                        (u8)healLocation->mapNum,
                                        mapHeader->regionMapSectionId);
}

static bool8 IsValidHealForContext(u16 healLocationId,
                                   enum JohtoTravelContext context)
{
    return healLocationId != HEAL_LOCATION_NONE
        && IsWorldContext(context)
        && ContextForHealLocation(healLocationId) == context;
}

u16 JohtoTravel_GetSavedHeal(enum JohtoTravelContext context)
{
    u16 variable = HealVariableForContext(context);
    u16 healLocationId;

    if (variable == 0)
        return HEAL_LOCATION_NONE;
    healLocationId = JohtoEvent_GetVariable(variable);
    return IsValidHealForContext(healLocationId, context)
        ? healLocationId : HEAL_LOCATION_NONE;
}

bool8 JohtoTravel_RecordCurrentHeal(u16 healLocationId)
{
    enum JohtoTravelContext context = JohtoTravel_GetCurrentContext();
    u16 variable = HealVariableForContext(context);

    if (variable == 0 || !IsValidHealForContext(healLocationId, context))
        return FALSE;
    return JohtoEvent_SetVariable(variable, healLocationId);
}

static bool8 SetActiveHealLocation(u16 healLocationId)
{
    const struct HealLocation *healLocation = GetHealLocation(healLocationId);

    if (healLocation == NULL)
        return FALSE;
    gSaveBlock1Ptr->lastHealLocation.mapGroup = healLocation->mapGroup;
    gSaveBlock1Ptr->lastHealLocation.mapNum = healLocation->mapNum;
    gSaveBlock1Ptr->lastHealLocation.warpId = WARP_ID_NONE;
    gSaveBlock1Ptr->lastHealLocation.x = healLocation->x;
    gSaveBlock1Ptr->lastHealLocation.y = healLocation->y;
    return TRUE;
}

static bool8 ResolvePendingTarget(enum JohtoTravelContext *targetContext,
                                  u16 *targetHeal)
{
    u16 pending = JohtoEvent_GetVariable(JOHTO_VAR_PENDING_KANTO_DESTINATION);

    if (!JohtoSave_Validate(&gSaveblock1.johto) || !IsValidDestination(pending))
        return FALSE;
    *targetContext = ContextForDestination(pending);
    *targetHeal = JohtoTravel_GetSavedHeal(*targetContext);
    if (*targetHeal == HEAL_LOCATION_NONE)
        *targetHeal = FallbackHealForContext(*targetContext);
    return IsValidHealForContext(*targetHeal, *targetContext);
}

static bool8 FailCrossing(void)
{
    (void)JohtoTravel_ClearPendingDestination();
    return FALSE;
}

bool8 JohtoTravel_PrepareCrossing(void)
{
    enum JohtoTravelContext targetContext;
    enum JohtoTravelContext currentContext = JohtoTravel_GetCurrentContext();
    u16 targetHeal;

    if (!ResolvePendingTarget(&targetContext, &targetHeal)
        || !IsWorldContext(currentContext)
        || currentContext == targetContext)
        return FailCrossing();
    return TRUE;
}

bool8 JohtoTravel_CommitCrossing(void)
{
    enum JohtoTravelContext targetContext;
    u16 targetHeal;
    struct WarpData previousHeal;

    if (!ResolvePendingTarget(&targetContext, &targetHeal)
        || JohtoTravel_GetCurrentContext() != targetContext)
        return FailCrossing();
    previousHeal = gSaveBlock1Ptr->lastHealLocation;
    if (!SetActiveHealLocation(targetHeal))
        return FailCrossing();
    if (!JohtoTravel_ClearPendingDestination())
    {
        gSaveBlock1Ptr->lastHealLocation = previousHeal;
        return FALSE;
    }
    return TRUE;
}

bool8 JohtoTravel_TryCommitArrival(void)
{
    enum JohtoTravelDestination destination = JohtoTravel_GetPendingDestination();
    enum JohtoTravelContext targetContext = ContextForDestination(destination);

    if (!IsWorldContext(targetContext)
        || JohtoTravel_GetCurrentContext() != targetContext)
        return FALSE;
    return JohtoTravel_CommitCrossing();
}

bool8 JohtoTravel_IsLaterInitialized(void)
{
    return JohtoEvent_GetFlag(JOHTO_FLAG_KANTO_LATER_INITIALIZED);
}

bool8 JohtoTravel_NeedsLaterKantoInitialization(void)
{
    return JohtoTravel_GetCurrentContext() == JOHTO_TRAVEL_CONTEXT_KANTO_LATER
        && !JohtoTravel_IsLaterInitialized();
}

bool8 JohtoTravel_MarkLaterKantoInitialized(void)
{
    if (JohtoTravel_GetCurrentContext() != JOHTO_TRAVEL_CONTEXT_KANTO_LATER)
        return FALSE;
    return JohtoEvent_SetFlag(JOHTO_FLAG_KANTO_LATER_INITIALIZED, TRUE);
}
