#include "global.h"
#include "cormoria/quest_commands.h"
#include "cormoria/quest_menu.h"
#include "cormoria/quest_state.h"
#include "event_data.h"
#include "overworld.h"
#include "script.h"
#include "string_util.h"

/* Dreamstone quest notifications are referenced directly by map scripts. */
const u8 Cormoria_gText_QuestAnnounce[] = _("The quest {STR_VAR_1}\n{STR_VAR_2}");
const u8 Cormoria_gText_QuestComplete[] = _("is now complete! Well done!");
const u8 Cormoria_gText_QuestActive[] = _("is now active! Gotta get going!");
const u8 Cormoria_gText_QuestUpdated[] = _("has been updated! Keep it up!");

static const u8 sSubquestNames[][64] =
{
    _("My First Day"),
    _("Lab Supplies"),
    _("Missing Supplies"),
    _("The First Dreamstone"),
    _("Mysterious Area"),
    _("Silversun Sighting"),
    _("Of Drama & Desire"),
    _("Knowledge of a Past Era"),
    _("Showdown at Mt. Mirroh!"),
    _("Stop Melea!"),
    _("No Way Out"),
    _("Reach Rivetshore City"),
    _("Board the S.S. Elegant"),
    _("Get Off the Ship!"),
    _("Explore the Island"),
    _("A Ranger's First Assignment"),
    _("Fieldwork: Mega Evolution"),
    _("The Final Test"),
    _("Help the Mayor!"),
    _("Save the Citizens!"),
};

/* Script operands use a parent-local child index; save bits use global IDs. */
struct CormoriaSubquestGroup
{
    u8 parent;
    u8 first;
    u8 count;
};

static const struct CormoriaSubquestGroup sSubquestGroups[] =
{
    {0, 0, 3},
    {1, 3, 8},
    {2, 11, 4},
    {14, 15, 3},
    {15, 18, 2},
};

static bool8 ValidateQuest(u16 questId)
{
    assertf(questId < CORMORIA_QUEST_COUNT, "invalid Cormoria quest id: %d", questId)
    {
        return FALSE;
    }
    return TRUE;
}

static bool8 ResolveSubquest(u16 parentId, u16 childId, u16 *globalId)
{
    u32 i;

    for (i = 0; i < ARRAY_COUNT(sSubquestGroups); i++)
    {
        if (sSubquestGroups[i].parent == parentId)
        {
            if (childId < sSubquestGroups[i].count)
            {
                *globalId = sSubquestGroups[i].first + childId;
                return TRUE;
            }
            break;
        }
    }
    assertf(FALSE, "invalid Cormoria subquest: parent %d child %d", parentId, childId)
    {
        return FALSE;
    }
    return TRUE;
}

static bool8 GetQuestBit(u16 questId, enum CormoriaQuestBit bit, bool8 *value)
{
    if (!CormoriaQuestState_Get(questId, bit, value))
    {
        assertf(FALSE, "Cormoria quest state is unavailable")
        {
            return FALSE;
        }
    }
    return TRUE;
}

static bool8 SetQuestBit(u16 questId, enum CormoriaQuestBit bit, bool8 value)
{
    if (!CormoriaQuestState_Set(questId, bit, value))
    {
        assertf(FALSE, "Cormoria quest state is unavailable")
        {
            return FALSE;
        }
    }
    return TRUE;
}

static bool8 SetQuestState(u16 questId, u8 caseId)
{
    switch (caseId)
    {
    case CORMORIA_QUEST_MENU_UNLOCK_QUEST:
        return SetQuestBit(questId, CORMORIA_QUEST_UNLOCKED, TRUE);
    case CORMORIA_QUEST_MENU_SET_ACTIVE:
        return SetQuestBit(questId, CORMORIA_QUEST_UNLOCKED, TRUE)
            && SetQuestBit(questId, CORMORIA_QUEST_ACTIVE, TRUE);
    case CORMORIA_QUEST_MENU_SET_REWARD:
        return SetQuestBit(questId, CORMORIA_QUEST_UNLOCKED, TRUE)
            && SetQuestBit(questId, CORMORIA_QUEST_REWARD, TRUE)
            && SetQuestBit(questId, CORMORIA_QUEST_ACTIVE, FALSE);
    case CORMORIA_QUEST_MENU_COMPLETE_QUEST:
        return SetQuestBit(questId, CORMORIA_QUEST_UNLOCKED, TRUE)
            && SetQuestBit(questId, CORMORIA_QUEST_COMPLETED, TRUE)
            && SetQuestBit(questId, CORMORIA_QUEST_ACTIVE, FALSE)
            && SetQuestBit(questId, CORMORIA_QUEST_REWARD, FALSE);
    default:
        return FALSE;
    }
}

static bool8 IsQuestInactive(u16 questId, bool8 *inactive)
{
    bool8 active;
    bool8 reward;
    bool8 completed;

    if (!GetQuestBit(questId, CORMORIA_QUEST_ACTIVE, &active)
        || !GetQuestBit(questId, CORMORIA_QUEST_REWARD, &reward)
        || !GetQuestBit(questId, CORMORIA_QUEST_COMPLETED, &completed))
        return FALSE;
    *inactive = !active && !reward && !completed;
    return TRUE;
}

bool8 ScrCmd_CormoriaQuestMenu(struct ScriptContext *ctx)
{
    u8 caseId;
    u8 questId;
    bool8 result;

    caseId = ScriptReadByte(ctx);
    questId = ScriptReadByte(ctx);

    if (caseId == CORMORIA_QUEST_MENU_OPEN)
    {
        Script_RequestEffects(SCREFF_V1 | SCREFF_HARDWARE);
        CormoriaQuestMenu_Init(CB2_ReturnToFieldContinueScript);
        ScriptContext_Stop();
        return TRUE;
    }

    if (!ValidateQuest(questId))
        return FALSE;

    if (caseId == CORMORIA_QUEST_MENU_BUFFER_QUEST_NAME)
    {
        Script_RequestEffects(SCREFF_V1);
        CormoriaQuestMenu_CopyQuestName(gStringVar1, questId);
        return FALSE;
    }

    switch (caseId)
    {
    case CORMORIA_QUEST_MENU_UNLOCK_QUEST:
    case CORMORIA_QUEST_MENU_SET_ACTIVE:
    case CORMORIA_QUEST_MENU_SET_REWARD:
    case CORMORIA_QUEST_MENU_COMPLETE_QUEST:
        Script_RequestEffects(SCREFF_V1 | SCREFF_SAVE);
        if (!SetQuestState(questId, caseId))
            StopScript(ctx);
        return FALSE;
    case CORMORIA_QUEST_MENU_CHECK_UNLOCKED:
        Script_RequestEffects(SCREFF_V1);
        if (!GetQuestBit(questId, CORMORIA_QUEST_UNLOCKED, &result))
        {
            StopScript(ctx);
            return FALSE;
        }
        break;
    case CORMORIA_QUEST_MENU_CHECK_INACTIVE:
        Script_RequestEffects(SCREFF_V1);
        if (!IsQuestInactive(questId, &result))
        {
            StopScript(ctx);
            return FALSE;
        }
        break;
    case CORMORIA_QUEST_MENU_CHECK_ACTIVE:
        Script_RequestEffects(SCREFF_V1);
        if (!GetQuestBit(questId, CORMORIA_QUEST_ACTIVE, &result))
        {
            StopScript(ctx);
            return FALSE;
        }
        break;
    case CORMORIA_QUEST_MENU_CHECK_REWARD:
        Script_RequestEffects(SCREFF_V1);
        if (!GetQuestBit(questId, CORMORIA_QUEST_REWARD, &result))
        {
            StopScript(ctx);
            return FALSE;
        }
        break;
    case CORMORIA_QUEST_MENU_CHECK_COMPLETE:
        Script_RequestEffects(SCREFF_V1);
        if (!GetQuestBit(questId, CORMORIA_QUEST_COMPLETED, &result))
        {
            StopScript(ctx);
            return FALSE;
        }
        break;
    default:
        assertf(FALSE, "unsupported Cormoria quest command case: %d", caseId)
        {
            return FALSE;
        }
    }

    gSpecialVar_Result = result;
    return FALSE;
}

bool8 ScrCmd_CormoriaSubquestMenu(struct ScriptContext *ctx)
{
    u8 caseId;
    u16 parentId;
    u16 childId;
    u16 globalId;
    bool8 result;

    caseId = ScriptReadByte(ctx);
    parentId = VarGet(ScriptReadHalfword(ctx));
    childId = VarGet(ScriptReadHalfword(ctx));

    if (!ResolveSubquest(parentId, childId, &globalId))
        return FALSE;

    if (caseId == CORMORIA_QUEST_MENU_BUFFER_QUEST_NAME)
    {
        Script_RequestEffects(SCREFF_V1);
        StringCopy(gStringVar1, sSubquestNames[globalId]);
        return FALSE;
    }

    switch (caseId)
    {
    case CORMORIA_QUEST_MENU_COMPLETE_QUEST:
        Script_RequestEffects(SCREFF_V1 | SCREFF_SAVE);
        if (!CormoriaQuestState_SetSubquest(globalId, TRUE))
        {
            assertf(FALSE, "Cormoria quest state is unavailable")
            {
                return FALSE;
            }
        }
        return FALSE;
    case CORMORIA_QUEST_MENU_CHECK_COMPLETE:
        Script_RequestEffects(SCREFF_V1);
        if (!CormoriaQuestState_GetSubquest(globalId, &result))
        {
            assertf(FALSE, "Cormoria quest state is unavailable")
            {
                return FALSE;
            }
        }
        gSpecialVar_Result = result;
        return FALSE;
    default:
        assertf(FALSE, "unsupported Cormoria subquest command case: %d", caseId)
        {
            return FALSE;
        }
    }
    return FALSE;
}

bool8 ScrCmd_CormoriaReturnQuestState(struct ScriptContext *ctx)
{
    u8 questId = ScriptReadByte(ctx);
    bool8 active;
    bool8 reward;
    bool8 completed;

    Script_RequestEffects(SCREFF_V1);
    if (!ValidateQuest(questId))
        return FALSE;

    if (!GetQuestBit(questId, CORMORIA_QUEST_ACTIVE, &active)
        || !GetQuestBit(questId, CORMORIA_QUEST_REWARD, &reward)
        || !GetQuestBit(questId, CORMORIA_QUEST_COMPLETED, &completed))
    {
        StopScript(ctx);
        return FALSE;
    }

    if (completed)
        gSpecialVar_Result = CORMORIA_QUEST_STATE_COMPLETE;
    else if (reward)
        gSpecialVar_Result = CORMORIA_QUEST_STATE_REWARD;
    else if (active)
        gSpecialVar_Result = CORMORIA_QUEST_STATE_ACTIVE;
    else
        gSpecialVar_Result = CORMORIA_QUEST_STATE_INACTIVE;
    return FALSE;
}
