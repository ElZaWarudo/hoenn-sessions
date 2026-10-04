#ifndef GUARD_CORMORIA_QUEST_STATE_H
#define GUARD_CORMORIA_QUEST_STATE_H

#include "gba/types.h"

// These are donor quest bit meanings, stored in the current world's event record.
// The 120-bit interval occupies flag ordinals 0xF00..0xF77 (IDs 0x8F00..0x8F77).
#define CORMORIA_QUEST_FLAG_BASE 0xF00
#define CORMORIA_QUEST_COUNT 20
#define CORMORIA_QUEST_BITS_PER_QUEST 5
#define CORMORIA_SUBQUEST_COUNT 20
#define CORMORIA_SUBQUEST_FLAG_BASE (CORMORIA_QUEST_FLAG_BASE + CORMORIA_QUEST_COUNT * CORMORIA_QUEST_BITS_PER_QUEST)
#define CORMORIA_QUEST_FLAG_END (CORMORIA_SUBQUEST_FLAG_BASE + CORMORIA_SUBQUEST_COUNT)

enum CormoriaQuestBit
{
    CORMORIA_QUEST_UNLOCKED,
    CORMORIA_QUEST_ACTIVE,
    CORMORIA_QUEST_REWARD,
    CORMORIA_QUEST_COMPLETED,
    CORMORIA_QUEST_FAVORITE,
};

// Return FALSE on an invalid index, invalid record, or null output pointer.
// A valid unset bit returns TRUE and writes FALSE to *value.
bool8 CormoriaQuestState_Get(u16 quest, enum CormoriaQuestBit bit, bool8 *value);
bool8 CormoriaQuestState_Set(u16 quest, enum CormoriaQuestBit bit, bool8 value);
bool8 CormoriaQuestState_GetSubquest(u16 subquest, bool8 *completed);
bool8 CormoriaQuestState_SetSubquest(u16 subquest, bool8 completed);

#endif // GUARD_CORMORIA_QUEST_STATE_H
