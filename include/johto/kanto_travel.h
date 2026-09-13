#ifndef GUARD_JOHTO_KANTO_TRAVEL_H
#define GUARD_JOHTO_KANTO_TRAVEL_H

#include "gba/types.h"

enum JohtoTravelDestination
{
    JOHTO_TRAVEL_DESTINATION_NONE,
    JOHTO_TRAVEL_DESTINATION_JOHTO,
    JOHTO_TRAVEL_DESTINATION_KANTO_ORIGINAL,
    JOHTO_TRAVEL_DESTINATION_KANTO_LATER,
};

enum JohtoTravelContext
{
    JOHTO_TRAVEL_CONTEXT_UNKNOWN,
    JOHTO_TRAVEL_CONTEXT_JOHTO,
    JOHTO_TRAVEL_CONTEXT_KANTO_ORIGINAL,
    JOHTO_TRAVEL_CONTEXT_KANTO_LATER,
};

enum JohtoTravelDestination JohtoTravel_GetPendingDestination(void);
bool8 JohtoTravel_SetPendingDestination(enum JohtoTravelDestination destination);
bool8 JohtoTravel_ClearPendingDestination(void);
bool8 JohtoTravel_Cancel(void);
enum JohtoTravelContext JohtoTravel_GetCurrentContext(void);
enum JohtoTravelContext JohtoTravel_GetContextForMap(u8 mapGroup, u8 mapNum,
                                                       u16 mapSecId);
bool8 JohtoTravel_RecordCurrentHeal(u16 healLocationId);
u16 JohtoTravel_GetSavedHeal(enum JohtoTravelContext context);

/* Preparation validates only. Commit runs after arrival and restores that
 * world's independent heal location. */
bool8 JohtoTravel_PrepareCrossing(void);
bool8 JohtoTravel_CommitCrossing(void);

/* The real Later-world initializer marks completion after applying defaults. */
bool8 JohtoTravel_NeedsLaterKantoInitialization(void);
bool8 JohtoTravel_MarkLaterKantoInitialized(void);
bool8 JohtoTravel_IsLaterInitialized(void);

#endif /* GUARD_JOHTO_KANTO_TRAVEL_H */
