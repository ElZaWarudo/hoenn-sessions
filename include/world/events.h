#ifndef GUARD_WORLD_EVENTS_H
#define GUARD_WORLD_EVENTS_H

#include "gba/types.h"
#include "constants/world_events.h"

/* Added ROMs use these script-ID windows for their own local event record.
 * Each ROM has a distinct regional save image, so future worlds may reuse
 * the windows without sharing quest progress. Never use these IDs as indexes
 * into the legacy flag, variable, or trainer arrays. */
struct BgEvent;

bool8 WorldEvent_IsFlagId(u16 id);
bool8 WorldEvent_IsVariableId(u16 id);
bool8 WorldEvent_IsTrainerId(u16 id);
bool8 WorldEvent_IsReservedId(u16 id);
bool8 WorldEvent_GetFlag(u16 id);
bool8 WorldEvent_SetFlag(u16 id, bool8 value);
u16 WorldEvent_GetVariable(u16 id);
bool8 WorldEvent_SetVariable(u16 id, u16 value);
bool8 WorldEvent_GetTrainerDefeated(u16 id);
bool8 WorldEvent_SetTrainerDefeated(u16 id, bool8 value);
u16 WorldEvent_GetHiddenItemFlag(const struct BgEvent *event);

#endif // GUARD_WORLD_EVENTS_H
