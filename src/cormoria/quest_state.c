#include "global.h"
#include "cormoria/quest_state.h"
#include "load_save.h"
#include "world/event_save.h"

_Static_assert(CORMORIA_QUEST_FLAG_END <= WORLD_EVENT_SAVE_FLAG_BITS_SIZE * 8,
               "Cormoria quest flags exceed world event storage");

static bool8 HasValidRecord(void)
{
    return WorldEventSave_Validate(&gSaveblock1.world_event);
}

static bool8 QuestOrdinal(u16 quest, enum CormoriaQuestBit bit, u16 *ordinal)
{
    if (quest >= CORMORIA_QUEST_COUNT || (u32)bit >= CORMORIA_QUEST_BITS_PER_QUEST)
        return FALSE;
    *ordinal = CORMORIA_QUEST_FLAG_BASE + quest * CORMORIA_QUEST_BITS_PER_QUEST + bit;
    return TRUE;
}

bool8 CormoriaQuestState_Get(u16 quest, enum CormoriaQuestBit bit, bool8 *value)
{
    u16 ordinal;

    if (value == NULL || !QuestOrdinal(quest, bit, &ordinal) || !HasValidRecord())
        return FALSE;
    *value = WorldEventSave_GetFlag(ordinal);
    return TRUE;
}

bool8 CormoriaQuestState_Set(u16 quest, enum CormoriaQuestBit bit, bool8 value)
{
    u16 ordinal;

    return QuestOrdinal(quest, bit, &ordinal) && WorldEventSave_SetFlag(ordinal, value);
}

bool8 CormoriaQuestState_GetSubquest(u16 subquest, bool8 *completed)
{
    if (completed == NULL || subquest >= CORMORIA_SUBQUEST_COUNT || !HasValidRecord())
        return FALSE;
    *completed = WorldEventSave_GetFlag(CORMORIA_SUBQUEST_FLAG_BASE + subquest);
    return TRUE;
}

bool8 CormoriaQuestState_SetSubquest(u16 subquest, bool8 completed)
{
    return subquest < CORMORIA_SUBQUEST_COUNT
        && WorldEventSave_SetFlag(CORMORIA_SUBQUEST_FLAG_BASE + subquest, completed);
}
