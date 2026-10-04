#include "global.h"
#include "event_data.h"
#include "item.h"
#include "load_save.h"
#include "main.h"
#include "malloc.h"
#include "overworld.h"
#include "pokemon.h"
#include "script.h"
#include "constants/cormoria_event_ids.h"
#include "constants/maps.h"
#include "test/test.h"
#include "world/event_save.h"

#if ROM_WORLD == 2
extern const u8 Cormoria_AncientCormoriaFinal_Dreamstone[];
extern ScrCmdFunc gScriptCmdTable[];
extern ScrCmdFunc gScriptCmdTableEnd[];

static u8 sAnswer;
static u32 sQuestions, sLocks, sReleases, sReleaseAlls, sWarps;
static bool8 sLocked;

static bool8 UnexpectedStoryCommand(struct ScriptContext *ctx)
{
    EXPECT(FALSE);
    StopScript(ctx);
    return FALSE;
}

static bool8 AnswerDreamstoneQuestion(struct ScriptContext *ctx)
{
    EXPECT_EQ(ScriptReadByte(ctx), 5); // MSGBOX_YESNO only.
    EXPECT_EQ(ctx->stackDepth, 0);
    sQuestions++;
    gSpecialVar_Result = sAnswer;
    return FALSE;
}

static bool8 LockStoryField(struct ScriptContext *ctx)
{
    EXPECT(!sLocked);
    sLocked = TRUE;
    sLocks++;
    return FALSE;
}

static bool8 ReleaseQuestionField(struct ScriptContext *ctx)
{
    // The production script releases the question before testing its answer.
    EXPECT(!sLocked);
    sReleases++;
    return FALSE;
}

static bool8 ReleaseStoryField(struct ScriptContext *ctx)
{
    EXPECT(sLocked);
    sLocked = FALSE;
    sReleaseAlls++;
    return FALSE;
}

static bool8 CommitStoryWarp(struct ScriptContext *ctx)
{
    EXPECT(sLocked);
    sWarps++;
    // warp (0x39) and setwarp (0x3E) read identical operands. Execute the
    // native destination parser and commit, without DoWarp scheduling map
    // loading or abandoning the test runner. This proves state and native
    // warp commit, not the visible transition, avatar reset or disk reload.
    EXPECT(!gScriptCmdTable[0x3E](ctx));
    ApplyCurrentWarp();
    return FALSE;
}

static void RunDreamstone(void)
{
    static const u8 realCommands[] = {
        0x02, 0x05, 0x06, 0x0F, // end, goto, goto_if, loadword
        0x16, 0x21, 0x29, 0x2A, // setvar, compare, setflag, clearflag
    };
    struct ScriptContext ctx;
    struct ScriptContext *context = &ctx;
    ScrCmdFunc *commands = Alloc(256 * sizeof(*commands));
    u32 i, steps;
    ASSUME(commands != NULL);
    sQuestions = sLocks = sReleases = sReleaseAlls = sWarps = 0;
    sLocked = FALSE;
    for (i = 0; i < 256; i++)
        commands[i] = UnexpectedStoryCommand;
    for (i = 0; i < ARRAY_COUNT(realCommands); i++)
    {
        EXPECT(realCommands[i] < gScriptCmdTableEnd - gScriptCmdTable);
        commands[realCommands[i]] = gScriptCmdTable[realCommands[i]];
    }
    commands[0x09] = AnswerDreamstoneQuestion;
    commands[0x39] = CommitStoryWarp;
    commands[0x69] = LockStoryField;
    commands[0x6B] = ReleaseStoryField;
    commands[0x6C] = ReleaseQuestionField;
    InitScriptContext(&ctx, commands, commands + 256);
    SetupBytecodeScript(&ctx, Cormoria_AncientCormoriaFinal_Dreamstone);
    for (steps = 0; ctx.scriptPtr != NULL && steps < 128; steps++)
    {
        u8 opcode = ScriptReadByte(context);
        EXPECT(!ctx.cmdTable[opcode](&ctx));
    }
    EXPECT(ctx.scriptPtr == NULL);
    EXPECT_EQ(ctx.stackDepth, 0);
    EXPECT(!sLocked);
    Free(commands);
}

TEST("Cormoria Dreamstone No preserves finale and Yes commits return to present")
{
    u32 i;
    u8 answer = 0, completion = 0;
    u16 heldItem = ITEM_ORAN_BERRY;
    struct Pokemon *partySnapshot;
    struct Bag *bagSnapshot;
    struct WorldEventSaveV1 *regionalSnapshot;
    struct WarpData locationBefore, lastWarpBefore;
    MainCallback runnerCallback;
    for (i = 0; i < 8; i++)
        PARAMETRIZE { answer = i & 1; completion = i >> 1; }
    partySnapshot = Alloc(sizeof(gPlayerParty));
    bagSnapshot = Alloc(sizeof(*bagSnapshot));
    regionalSnapshot = Alloc(sizeof(*regionalSnapshot));
    ASSUME(partySnapshot != NULL);
    ASSUME(bagSnapshot != NULL);
    ASSUME(regionalSnapshot != NULL);
    SetSaveBlocksPointers(0);
    WorldEventSave_InitializeCurrent();
    ResetPokemonStorageSystem();
    ZeroPlayerPartyMons();
    CreateMonWithIVs(&gPlayerParty[0], SPECIES_RATTATA, 17, 0x12345678,
                     OTID_STRUCT_PRESET(0x11223344), 12);
    SetMonData(&gPlayerParty[0], MON_DATA_HELD_ITEM, &heldItem);
    CalculatePlayerPartyCount();
    SetBagItemsPointers();
    ClearBag();
    FlagClear(FLAG_STORING_ITEMS_IN_PYRAMID_BAG);
    FlagClear(FLAG_SAFE_FOLLOWER_MOVEMENT); // Native end clears this flag.
    EXPECT(AddBagItem(ITEM_POTION, 7));
    EXPECT(AddBagItem(ITEM_ORAN_BERRY, 3));
    if (completion & 1)
        FlagSet(FLAG_SYS_GAME_CLEAR);
    else
        FlagClear(FLAG_SYS_GAME_CLEAR);
    if (completion & 2)
        FlagSet(Cormoria_FLAG_SYS_GAME_CLEAR);
    else
        FlagClear(Cormoria_FLAG_SYS_GAME_CLEAR);
    FlagSet(Cormoria_FLAG_LAB_CALLTOACTION);
    VarSet(Cormoria_VAR_UNCHARTED_ISLAND_CUTSCENES, 3);
    FlagClear(Cormoria_FLAG_HIDE_TENEBRIS_FINALE);
    FlagSet(Cormoria_FLAG_HIDE_ISLAND_FINALE);
    // A valid distinct origin makes a committed destination observable.
    gSaveBlock1Ptr->location.mapGroup = MAP_GROUP(MAP_CORMORIA_UNCHARTED_ISLAND);
    gSaveBlock1Ptr->location.mapNum = MAP_NUM(MAP_CORMORIA_UNCHARTED_ISLAND);
    gSaveBlock1Ptr->location.warpId = WARP_ID_NONE;
    gSaveBlock1Ptr->location.x = 5;
    gSaveBlock1Ptr->location.y = 6;
    locationBefore = gSaveBlock1Ptr->location;
    lastWarpBefore = gLastUsedWarp;
    memcpy(partySnapshot, gPlayerParty, sizeof(gPlayerParty));
    *bagSnapshot = gSaveBlock1Ptr->bag;
    *regionalSnapshot = gSaveblock1.world_event;
    runnerCallback = gMain.callback2;
    sAnswer = answer;

    RunDreamstone();
    EXPECT_EQ(gMain.callback2, runnerCallback);
    EXPECT_EQ(sQuestions, 1);
    EXPECT_EQ(sReleases, 1);
    EXPECT_EQ(sLocks, answer);
    EXPECT_EQ(sReleaseAlls, answer);
    EXPECT_EQ(sWarps, answer);
    EXPECT_EQ(gSpecialVar_Result, answer);
    EXPECT_EQ(VarGet(Cormoria_VAR_UNCHARTED_ISLAND_CUTSCENES), answer ? 4 : 3);
    EXPECT_EQ(FlagGet(Cormoria_FLAG_HIDE_TENEBRIS_FINALE), answer);
    EXPECT_EQ(FlagGet(Cormoria_FLAG_HIDE_ISLAND_FINALE), !answer);
    EXPECT_EQ(FlagGet(FLAG_SYS_GAME_CLEAR), completion & 1);
    EXPECT_EQ(FlagGet(Cormoria_FLAG_SYS_GAME_CLEAR), (completion >> 1) & 1);
    EXPECT(FlagGet(Cormoria_FLAG_LAB_CALLTOACTION));
    EXPECT(WorldEventSave_Validate(&gSaveblock1.world_event));
    EXPECT_EQ(gPlayerPartyCount, 1);
    EXPECT_EQ(memcmp(partySnapshot, gPlayerParty, sizeof(gPlayerParty)), 0);
    EXPECT_EQ(memcmp(bagSnapshot, &gSaveBlock1Ptr->bag, sizeof(*bagSnapshot)), 0);
    if (answer)
    {
        EXPECT_EQ(memcmp(&gLastUsedWarp, &locationBefore, sizeof(locationBefore)), 0);
        EXPECT_EQ(gSaveBlock1Ptr->location.mapGroup, MAP_GROUP(MAP_CORMORIA_UNCHARTED_ISLAND));
        EXPECT_EQ(gSaveBlock1Ptr->location.mapNum, MAP_NUM(MAP_CORMORIA_UNCHARTED_ISLAND));
        EXPECT_EQ(gSaveBlock1Ptr->location.warpId, WARP_ID_NONE);
        EXPECT_EQ(gSaveBlock1Ptr->location.x, 78);
        EXPECT_EQ(gSaveBlock1Ptr->location.y, 24);
    }
    else
    {
        EXPECT_EQ(memcmp(&gSaveBlock1Ptr->location, &locationBefore, sizeof(locationBefore)), 0);
        EXPECT_EQ(memcmp(&gLastUsedWarp, &lastWarpBefore, sizeof(lastWarpBefore)), 0);
        EXPECT_EQ(memcmp(regionalSnapshot, &gSaveblock1.world_event, sizeof(*regionalSnapshot)), 0);
    }
    Free(regionalSnapshot);
    Free(bagSnapshot);
    Free(partySnapshot);
}
#endif
