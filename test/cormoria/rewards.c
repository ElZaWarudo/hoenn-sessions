#include "global.h"
#include "event_data.h"
#include "item.h"
#include "load_save.h"
#include "malloc.h"
#include "script.h"
#include "constants/cormoria_event_ids.h"
#include "constants/sound.h"
#include "test/test.h"
#include "world/event_save.h"

#if ROM_WORLD == 2
extern const u8 Cormoria_CarabrueTown_TenebrisLab_Gardevoir[];
extern const u8 Cormoria_CarabrueTown_TenebrisLab_Gardevoir_ItemFull[];
extern const u8 EventScript_ObtainItemMessage[];
extern ScrCmdFunc gScriptCmdTable[];
extern ScrCmdFunc gScriptCmdTableEnd[];

// This fixture executes imported bytecode, including the real standard gift
// script. Only dialogue, cries and field locking are replaced; it proves the
// native transaction and branches, not visible interaction or save/reload.
static struct Bag *sFullBag;
static u32 sAddItemCalls;
static u32 sGiftCalls;
static u32 sPresentationCalls;
static bool8 sFullExitSeen;

static bool8 UnexpectedCommand(struct ScriptContext *ctx)
{
    EXPECT(FALSE);
    StopScript(ctx);
    return FALSE;
}

static bool8 SkipFieldLock(struct ScriptContext *ctx)
{
    return FALSE;
}

static bool8 SkipCry(struct ScriptContext *ctx)
{
    EXPECT_EQ(VarGet(ScriptReadHalfword(ctx)), SPECIES_GARDEVOIR);
    EXPECT_EQ(VarGet(ScriptReadHalfword(ctx)), CRY_MODE_NORMAL);
    return FALSE;
}

static bool8 PresentationCall(struct ScriptContext *ctx)
{
    // STD_OBTAIN_ITEM calls this presentation-only function after additem
    // and copyvar. Skipping it preserves additem's VAR_RESULT; the full UI
    // script also restores that result before returning.
    EXPECT_EQ((const u8 *)ScriptReadWord(ctx), EventScript_ObtainItemMessage);
    EXPECT_EQ(ctx->stackDepth, 1);
    EXPECT_EQ(gSpecialVar_Result, gSpecialVar_0x8007);
    sPresentationCalls++;
    return FALSE;
}

static bool8 StandardCall(struct ScriptContext *ctx)
{
    u8 standard = ScriptReadByte(ctx);
    if (standard == 0) // STD_OBTAIN_ITEM: preserve its real call and return.
    {
        sGiftCalls++;
        ctx->scriptPtr--;
        return gScriptCmdTable[0x09](ctx);
    }
    EXPECT(standard == 2 || standard == 4); // MSGBOX_NPC / MSGBOX_DEFAULT.
    return FALSE;
}

static bool8 AddRewardItem(struct ScriptContext *ctx)
{
    EXPECT_EQ(ctx->stackDepth, 1); // Inside the real STD_OBTAIN_ITEM.
    EXPECT_EQ(VarGet(ScriptPeekHalfword(ctx)), ITEM_STARF_BERRY);
    sAddItemCalls++;
    return gScriptCmdTable[0x44](ctx);
}

static void RunGardevoirReward(void)
{
    // Opcodes are pinned to data/script_cmd_table.inc. An imported script
    // change must explicitly extend this allowlist rather than touch UI.
    static const u8 realCommands[] = {
        0x02, 0x03, 0x05, 0x06, // end, return, goto, goto_if
        0x0F, 0x16, 0x19, 0x1A, // loadword, setvar, copyvar, setorcopyvar
        0x21, 0x29, 0x2B,       // compare, setflag, checkflag
    };
    struct ScriptContext ctx;
    struct ScriptContext *context = &ctx;
    ScrCmdFunc *commands = Alloc(256 * sizeof(*commands));
    u32 i, steps;
    ASSUME(commands != NULL);
    sAddItemCalls = sGiftCalls = sPresentationCalls = 0;
    sFullExitSeen = FALSE;
    for (i = 0; i < 256; i++)
        commands[i] = UnexpectedCommand;
    for (i = 0; i < ARRAY_COUNT(realCommands); i++)
    {
        EXPECT(realCommands[i] < gScriptCmdTableEnd - gScriptCmdTable);
        commands[realCommands[i]] = gScriptCmdTable[realCommands[i]];
    }
    commands[0x04] = PresentationCall;
    commands[0x09] = StandardCall;
    commands[0x44] = AddRewardItem;
    commands[0x69] = SkipFieldLock;
    commands[0x6B] = SkipFieldLock;
    commands[0xA1] = SkipCry;
    InitScriptContext(&ctx, commands, commands + 256);
    SetupBytecodeScript(&ctx, Cormoria_CarabrueTown_TenebrisLab_Gardevoir);
    // RunScriptCommand loops internally; stepping the same table directly
    // makes the total bytecode budget explicit, including call-stack cycles.
    for (steps = 0; ctx.scriptPtr != NULL && steps < 128; steps++)
    {
        u8 opcode;
        if (ctx.scriptPtr == Cormoria_CarabrueTown_TenebrisLab_Gardevoir_ItemFull)
            sFullExitSeen = TRUE;
        opcode = ScriptReadByte(context);
        EXPECT(!ctx.cmdTable[opcode](&ctx));
    }
    EXPECT(ctx.scriptPtr == NULL);
    EXPECT_EQ(ctx.stackDepth, 0);
    Free(commands);
}

static void ExpectUnrelatedState(void)
{
    EXPECT(FlagGet(Cormoria_FLAG_LAB_CALLTOACTION));
    EXPECT(FlagGet(FLAG_SYS_GAME_CLEAR));
    EXPECT(!FlagGet(FLAG_BADGE02_GET));
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_POTION), 7);
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_ORAN_BERRY), 3);
}

TEST("Cormoria Gardevoir Starf reward retains full-stack claim for retry and delivers once")
{
    SetSaveBlocksPointers(0);
    sFullBag = Alloc(sizeof(*sFullBag));
    ASSUME(sFullBag != NULL);
    WorldEventSave_InitializeCurrent();
    SetBagItemsPointers();
    ClearBag();
    FlagClear(FLAG_STORING_ITEMS_IN_PYRAMID_BAG);
    FlagClear(Cormoria_FLAG_TENEBRIS_GARDEVOIR);
    FlagSet(Cormoria_FLAG_LAB_CALLTOACTION);
    FlagSet(FLAG_SYS_GAME_CLEAR);
    FlagClear(FLAG_BADGE02_GET);
    EXPECT(AddBagItem(ITEM_POTION, 7));
    EXPECT(AddBagItem(ITEM_ORAN_BERRY, 3));
    EXPECT(AddBagItem(ITEM_STARF_BERRY, MAX_BAG_ITEM_CAPACITY));
    // Berry pockets disallow a second stack even when empty slots remain.
    // The generic free-space query counts empty slots, so establish rejection
    // with the actual insertion operation used by STD_OBTAIN_ITEM.
    EXPECT(!AddBagItem(ITEM_STARF_BERRY, 1));
    memcpy(sFullBag, &gSaveBlock1Ptr->bag, sizeof(*sFullBag));

    RunGardevoirReward();
    EXPECT_EQ(sGiftCalls, 1);
    EXPECT_EQ(sAddItemCalls, 1);
    EXPECT_EQ(sPresentationCalls, 1);
    EXPECT_EQ(gSpecialVar_Result, FALSE);
    EXPECT(sFullExitSeen);
    EXPECT(!FlagGet(Cormoria_FLAG_TENEBRIS_GARDEVOIR));
    EXPECT_EQ(memcmp(sFullBag, &gSaveBlock1Ptr->bag, sizeof(*sFullBag)), 0);
    ExpectUnrelatedState();

    EXPECT(RemoveBagItem(ITEM_STARF_BERRY, 1));
    EXPECT(CheckBagHasSpace(ITEM_STARF_BERRY, 1));
    RunGardevoirReward();
    EXPECT_EQ(sGiftCalls, 1);
    EXPECT_EQ(sAddItemCalls, 1);
    EXPECT_EQ(sPresentationCalls, 1);
    EXPECT_EQ(gSpecialVar_Result, TRUE);
    EXPECT(!sFullExitSeen);
    EXPECT(FlagGet(Cormoria_FLAG_TENEBRIS_GARDEVOIR));
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_STARF_BERRY), MAX_BAG_ITEM_CAPACITY);
    EXPECT_EQ(memcmp(sFullBag, &gSaveBlock1Ptr->bag, sizeof(*sFullBag)), 0);
    ExpectUnrelatedState();

    RunGardevoirReward();
    EXPECT_EQ(sGiftCalls, 0);
    EXPECT_EQ(sAddItemCalls, 0);
    EXPECT_EQ(sPresentationCalls, 0);
    EXPECT(!sFullExitSeen);
    EXPECT(FlagGet(Cormoria_FLAG_TENEBRIS_GARDEVOIR));
    EXPECT_EQ(memcmp(sFullBag, &gSaveBlock1Ptr->bag, sizeof(*sFullBag)), 0);
    ExpectUnrelatedState();
    Free(sFullBag);
}
#endif
