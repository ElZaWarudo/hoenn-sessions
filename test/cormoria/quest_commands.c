#include "global.h"
#include "cormoria/quest_commands.h"
#include "cormoria/quest_state.h"
#include "event_data.h"
#include "script.h"
#include "string_util.h"
#include "test/test.h"
#include "world/event_save.h"

static void InitCommandContext(struct ScriptContext *ctx, const u8 *script)
{
    InitScriptContext(ctx, NULL, NULL);
    ctx->scriptPtr = script;
}

TEST("Cormoria quest command cases mutate and read world-local state")
{
    struct ScriptContext ctx;
    bool8 value;
    const u8 activate[] = {CORMORIA_QUEST_MENU_SET_ACTIVE, 3};
    const u8 checkActive[] = {CORMORIA_QUEST_MENU_CHECK_ACTIVE, 3};
    const u8 reward[] = {CORMORIA_QUEST_MENU_SET_REWARD, 3};
    const u8 complete[] = {CORMORIA_QUEST_MENU_COMPLETE_QUEST, 3};

    WorldEventSave_InitializeCurrent();

    InitCommandContext(&ctx, activate);
    EXPECT(!ScrCmd_CormoriaQuestMenu(&ctx));
    EXPECT(CormoriaQuestState_Get(3, CORMORIA_QUEST_UNLOCKED, &value));
    EXPECT(value);
    EXPECT(CormoriaQuestState_Get(3, CORMORIA_QUEST_ACTIVE, &value));
    EXPECT(value);

    InitCommandContext(&ctx, checkActive);
    EXPECT(!ScrCmd_CormoriaQuestMenu(&ctx));
    EXPECT_EQ(gSpecialVar_Result, TRUE);

    InitCommandContext(&ctx, reward);
    EXPECT(!ScrCmd_CormoriaQuestMenu(&ctx));
    EXPECT(CormoriaQuestState_Get(3, CORMORIA_QUEST_REWARD, &value));
    EXPECT(value);
    EXPECT(CormoriaQuestState_Get(3, CORMORIA_QUEST_ACTIVE, &value));
    EXPECT(!value);

    InitCommandContext(&ctx, complete);
    EXPECT(!ScrCmd_CormoriaQuestMenu(&ctx));
    EXPECT(CormoriaQuestState_Get(3, CORMORIA_QUEST_COMPLETED, &value));
    EXPECT(value);
    EXPECT(CormoriaQuestState_Get(3, CORMORIA_QUEST_REWARD, &value));
    EXPECT(!value);
}

TEST("Cormoria quest conditionals distinguish inactive and completed")
{
    struct ScriptContext ctx;
    const u8 inactive[] = {CORMORIA_QUEST_MENU_CHECK_INACTIVE, 7};
    const u8 completed[] = {CORMORIA_QUEST_MENU_CHECK_COMPLETE, 7};

    WorldEventSave_InitializeCurrent();

    InitCommandContext(&ctx, inactive);
    EXPECT(!ScrCmd_CormoriaQuestMenu(&ctx));
    EXPECT_EQ(gSpecialVar_Result, TRUE);

    EXPECT(CormoriaQuestState_Set(7, CORMORIA_QUEST_COMPLETED, TRUE));
    InitCommandContext(&ctx, inactive);
    EXPECT(!ScrCmd_CormoriaQuestMenu(&ctx));
    EXPECT_EQ(gSpecialVar_Result, FALSE);

    InitCommandContext(&ctx, completed);
    EXPECT(!ScrCmd_CormoriaQuestMenu(&ctx));
    EXPECT_EQ(gSpecialVar_Result, TRUE);
}

TEST("Cormoria subquest command uses the shared linear subquest allocation")
{
    struct ScriptContext ctx;
    bool8 completed;
    const u8 setCompleted[] = {CORMORIA_QUEST_MENU_COMPLETE_QUEST, 0x01, 0x00, 0x07, 0x00};
    const u8 checkCompleted[] = {CORMORIA_QUEST_MENU_CHECK_COMPLETE, 0x01, 0x00, 0x07, 0x00};

    WorldEventSave_InitializeCurrent();

    InitCommandContext(&ctx, setCompleted);
    EXPECT(!ScrCmd_CormoriaSubquestMenu(&ctx));
    EXPECT(CormoriaQuestState_GetSubquest(10, &completed));
    EXPECT(completed);

    InitCommandContext(&ctx, checkCompleted);
    EXPECT(!ScrCmd_CormoriaSubquestMenu(&ctx));
    EXPECT_EQ(gSpecialVar_Result, TRUE);
}

TEST("Cormoria maps each donor parent-local child to its global save bit")
{
    struct ScriptContext ctx;
    const u8 parents[] = {0, 1, 2, 14, 15};
    const u8 globalIds[] = {0, 3, 11, 15, 18};
    bool8 completed;
    u32 i;

    WorldEventSave_InitializeCurrent();
    for (i = 0; i < ARRAY_COUNT(parents); i++)
    {
        const u8 script[] = {CORMORIA_QUEST_MENU_COMPLETE_QUEST, parents[i], 0, 0, 0};
        InitCommandContext(&ctx, script);
        EXPECT(!ScrCmd_CormoriaSubquestMenu(&ctx));
        EXPECT(CormoriaQuestState_GetSubquest(globalIds[i], &completed));
        EXPECT(completed);
    }
}

TEST("Cormoria returnqueststate follows donor result ordering")
{
    struct ScriptContext ctx;
    const u8 query[] = {2};

    WorldEventSave_InitializeCurrent();
    EXPECT(CormoriaQuestState_Set(2, CORMORIA_QUEST_UNLOCKED, TRUE));
    EXPECT(CormoriaQuestState_Set(2, CORMORIA_QUEST_ACTIVE, TRUE));
    EXPECT(CormoriaQuestState_Set(2, CORMORIA_QUEST_REWARD, TRUE));

    InitCommandContext(&ctx, query);
    EXPECT(!ScrCmd_CormoriaReturnQuestState(&ctx));
    EXPECT_EQ(gSpecialVar_Result, CORMORIA_QUEST_STATE_REWARD);

    EXPECT(CormoriaQuestState_Set(2, CORMORIA_QUEST_COMPLETED, TRUE));
    InitCommandContext(&ctx, query);
    EXPECT(!ScrCmd_CormoriaReturnQuestState(&ctx));
    EXPECT_EQ(gSpecialVar_Result, CORMORIA_QUEST_STATE_COMPLETE);
}

TEST("Cormoria quest announcement commands buffer donor quest names")
{
    struct ScriptContext ctx;
    const u8 quest[] = {CORMORIA_QUEST_MENU_BUFFER_QUEST_NAME, 1};
    const u8 subquest[] = {CORMORIA_QUEST_MENU_BUFFER_QUEST_NAME, 1, 0, 0, 0};
    const u8 expectedQuest[] = _("Find the Dreamstone!");
    const u8 expectedSubquest[] = _("The First Dreamstone");

    WorldEventSave_InitializeCurrent();
    InitCommandContext(&ctx, quest);
    EXPECT(!ScrCmd_CormoriaQuestMenu(&ctx));
    EXPECT_EQ(StringCompare(gStringVar1, expectedQuest), 0);

    InitCommandContext(&ctx, subquest);
    EXPECT(!ScrCmd_CormoriaSubquestMenu(&ctx));
    EXPECT_EQ(StringCompare(gStringVar1, expectedSubquest), 0);
}
