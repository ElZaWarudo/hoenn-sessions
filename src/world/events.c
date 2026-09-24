#include "global.h"
#include "world/events.h"
#include "world/event_save.h"
#include "constants/event_bg.h"
#include "constants/flags.h"
#include "constants/johto_events.h"

bool8 WorldEvent_IsFlagId(u16 id)
{
    return id >= WORLD_EVENT_FLAG_START && id <= WORLD_EVENT_FLAG_END;
}

bool8 WorldEvent_IsVariableId(u16 id)
{
    return id >= WORLD_EVENT_VAR_START && id <= WORLD_EVENT_VAR_END;
}

bool8 WorldEvent_IsTrainerId(u16 id)
{
    return id >= WORLD_EVENT_TRAINER_START && id <= WORLD_EVENT_TRAINER_END;
}

bool8 WorldEvent_IsReservedId(u16 id)
{
    return WorldEvent_IsFlagId(id) || WorldEvent_IsVariableId(id);
}

bool8 WorldEvent_GetFlag(u16 id)
{
    return WorldEvent_IsFlagId(id)
        && WorldEventSave_GetFlag(id - WORLD_EVENT_FLAG_START);
}

bool8 WorldEvent_SetFlag(u16 id, bool8 value)
{
    return WorldEvent_IsFlagId(id)
        && WorldEventSave_SetFlag(id - WORLD_EVENT_FLAG_START, value);
}

u16 WorldEvent_GetVariable(u16 id)
{
    if (!WorldEvent_IsVariableId(id))
        return 0;
    return WorldEventSave_GetVariable(id - WORLD_EVENT_VAR_START);
}

bool8 WorldEvent_SetVariable(u16 id, u16 value)
{
    return WorldEvent_IsVariableId(id)
        && WorldEventSave_SetVariable(id - WORLD_EVENT_VAR_START, value);
}

bool8 WorldEvent_GetTrainerDefeated(u16 id)
{
    return WorldEvent_IsTrainerId(id)
        && WorldEventSave_GetTrainerDefeated(id - WORLD_EVENT_TRAINER_START);
}

bool8 WorldEvent_SetTrainerDefeated(u16 id, bool8 value)
{
    return WorldEvent_IsTrainerId(id)
        && WorldEventSave_SetTrainerDefeated(id - WORLD_EVENT_TRAINER_START, value);
}

u16 WorldEvent_GetHiddenItemFlag(const struct BgEvent *event)
{
    u16 packedId;

    if (event == NULL || event->kind != BG_EVENT_HIDDEN_ITEM)
        return 0;
    packedId = event->bgUnion.hiddenItem.hiddenItemId;
    if (packedId & WORLD_EVENT_HIDDEN_ITEM_MARKER)
        return WORLD_EVENT_FLAG_START
             + (packedId & WORLD_EVENT_HIDDEN_ITEM_ORDINAL_MASK);
    if (packedId & WORLD_EVENT_HIDDEN_ITEM_JOHTO_MARKER)
    {
        u16 ordinal = packedId & WORLD_EVENT_HIDDEN_ITEM_JOHTO_MASK;

        if (ordinal >= JOHTO_FLAG_COUNT)
            return 0;
        return JOHTO_FLAG_START + ordinal;
    }
    return FLAG_HIDDEN_ITEMS_START + packedId;
}
