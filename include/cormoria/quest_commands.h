#ifndef GUARD_CORMORIA_QUEST_COMMANDS_H
#define GUARD_CORMORIA_QUEST_COMMANDS_H

#include "gba/types.h"

struct ScriptContext;

/* These values are the command cases used by Dreamstone's quest scripts.
 * They are deliberately kept separate from the host's script opcode values:
 * host opcodes 0xe5 and 0xe6 are already assigned. */
enum CormoriaQuestMenuCase
{
    CORMORIA_QUEST_MENU_OPEN,
    CORMORIA_QUEST_MENU_UNLOCK_QUEST,
    CORMORIA_QUEST_MENU_SET_ACTIVE,
    CORMORIA_QUEST_MENU_SET_REWARD,
    CORMORIA_QUEST_MENU_COMPLETE_QUEST,
    CORMORIA_QUEST_MENU_CHECK_UNLOCKED,
    CORMORIA_QUEST_MENU_CHECK_INACTIVE,
    CORMORIA_QUEST_MENU_CHECK_ACTIVE,
    CORMORIA_QUEST_MENU_CHECK_REWARD,
    CORMORIA_QUEST_MENU_CHECK_COMPLETE,
    CORMORIA_QUEST_MENU_BUFFER_QUEST_NAME,
};

/* Values returned by returnqueststate. */
enum CormoriaQuestStateResult
{
    CORMORIA_QUEST_STATE_INACTIVE = 1,
    CORMORIA_QUEST_STATE_ACTIVE = 2,
    CORMORIA_QUEST_STATE_REWARD = 3,
    CORMORIA_QUEST_STATE_COMPLETE = 4,
};

bool8 ScrCmd_CormoriaQuestMenu(struct ScriptContext *ctx);
bool8 ScrCmd_CormoriaSubquestMenu(struct ScriptContext *ctx);
bool8 ScrCmd_CormoriaReturnQuestState(struct ScriptContext *ctx);

#endif // GUARD_CORMORIA_QUEST_COMMANDS_H
