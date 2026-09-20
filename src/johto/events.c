#include "global.h"
#include "johto/events.h"
#include "johto/save.h"
#include "constants/johto_events.h"

bool8 JohtoEvent_IsFlagId(u16 id)
{
    return id >= JOHTO_FLAG_START && id <= JOHTO_FLAG_END;
}

bool8 JohtoEvent_IsVariableId(u16 id)
{
    return id >= JOHTO_VAR_START && id <= JOHTO_VAR_END;
}

bool8 JohtoEvent_IsReservedId(u16 id)
{
    return id >= JOHTO_EVENT_RESERVED_START && id <= JOHTO_EVENT_RESERVED_END;
}

bool8 JohtoEvent_GetFlag(u16 id)
{
    if (!JohtoEvent_IsFlagId(id))
        return FALSE;
    return JohtoSave_GetFlag(id - JOHTO_FLAG_START);
}

bool8 JohtoEvent_SetFlag(u16 id, bool8 value)
{
    if (!JohtoEvent_IsFlagId(id))
        return FALSE;
    return JohtoSave_SetFlag(id - JOHTO_FLAG_START, value);
}

u16 JohtoEvent_GetVariable(u16 id)
{
    if (!JohtoEvent_IsVariableId(id))
        return 0;
    return JohtoSave_GetVariable(id - JOHTO_VAR_START);
}

bool8 JohtoEvent_SetVariable(u16 id, u16 value)
{
    if (!JohtoEvent_IsVariableId(id))
        return FALSE;
    return JohtoSave_SetVariable(id - JOHTO_VAR_START, value);
}
