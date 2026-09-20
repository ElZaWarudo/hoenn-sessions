#ifndef GUARD_JOHTO_EVENTS_H
#define GUARD_JOHTO_EVENTS_H

#include "gba/types.h"

bool8 JohtoEvent_IsFlagId(u16 id);
bool8 JohtoEvent_IsVariableId(u16 id);
bool8 JohtoEvent_IsReservedId(u16 id);

bool8 JohtoEvent_GetFlag(u16 id);
bool8 JohtoEvent_SetFlag(u16 id, bool8 value);
u16 JohtoEvent_GetVariable(u16 id);
bool8 JohtoEvent_SetVariable(u16 id, u16 value);

#endif /* GUARD_JOHTO_EVENTS_H */
