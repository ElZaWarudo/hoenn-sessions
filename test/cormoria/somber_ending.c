#include "global.h"
#include "event_data.h"
#include "heal_location.h"
#include "item.h"
#include "load_save.h"
#include "main.h"
#include "malloc.h"
#include "overworld.h"
#include "pokemon.h"
#include "script.h"
#include "cormoria/heal_locations.h"
#include "cormoria/quest_commands.h"
#include "cormoria/quest_state.h"
#include "cormoria/quests.h"
#include "constants/cormoria_event_ids.h"
#include "constants/maps.h"
#include "test/test.h"
#include "world/event_save.h"

#if ROM_WORLD == 2
extern const u8 Cormoria_UnchartedIsland_Finale_Ending[];
extern const u8 Cormoria_CarabrueTown_TenebrisLab_PostFinale[];
extern ScrCmdFunc gScriptCmdTable[];
extern ScrCmdFunc gScriptCmdTableEnd[];
extern void SetSpeaker(struct ScriptContext *ctx);
// The assembler exports special indices as absolute symbols, not objects.
extern const u8 SPECIAL_ShakeCamera, SPECIAL_SpawnCameraObject, SPECIAL_HealPlayerParty;

static u32 *sCounts;
static u32 sShakes, sCameras, sHeals, sQuestCompletes, sQuestNames;
static u8 sOpcode;
static bool8 sLocked;

static bool8 UnexpectedEndingCommand(struct ScriptContext *ctx)
{
    EXPECT(FALSE);
    StopScript(ctx);
    return FALSE;
}

static bool8 EndingPresentation(struct ScriptContext *ctx)
{
    // Only these presentation operands may be skipped. State commands have
    // native handlers; permanent actor coordinates are native too (0x63).
    sCounts[sOpcode]++;
    switch (sOpcode)
    {
    case 0x09: // MSGBOX_DEFAULT, never a gift or Yes/No standard script.
        EXPECT_EQ(ScriptReadByte(ctx), 4);
        EXPECT_EQ(ctx->stackDepth, 0);
        break;
    case 0x23: // The sole allowed callnative is the namebox renderer.
        EXPECT_EQ(ScriptReadWord(ctx), (uintptr_t)SetSpeaker);
        EXPECT(ScriptReadWord(ctx) != 0);
        break;
    case 0x28: case 0x2F: case 0x31: case 0x51: case 0x53: case 0x55:
        ScriptReadHalfword(ctx);
        break;
    case 0x4F:
        ScriptReadHalfword(ctx);
        EXPECT(ScriptReadWord(ctx) != 0);
        break;
    case 0x57:
        ScriptReadHalfword(ctx);
        ScriptReadHalfword(ctx);
        ScriptReadHalfword(ctx);
        break;
    case 0x67:
        EXPECT(ScriptReadWord(ctx) != 0);
        break;
    case 0x97:
        (void)ScriptReadByte(ctx);
        break;
    case 0xA1:
        ScriptReadHalfword(ctx);
        ScriptReadHalfword(ctx);
        break;
    case 0x69:
        EXPECT(!sLocked);
        sLocked = TRUE;
        break;
    case 0x6B: case 0x6C:
        // completequest releases before the enclosing releaseall.
        sLocked = FALSE;
        break;
    case 0x30: case 0x32: case 0x68: case 0xC5:
        break;
    default:
        return UnexpectedEndingCommand(ctx);
    }
    return FALSE;
}

static bool8 EndingSpecial(struct ScriptContext *ctx)
{
    u16 index = ScriptReadHalfword(ctx);
    if (index == (uintptr_t)&SPECIAL_ShakeCamera)
        sShakes++;
    else if (index == (uintptr_t)&SPECIAL_SpawnCameraObject)
        sCameras++;
    else if (index == (uintptr_t)&SPECIAL_HealPlayerParty)
    {
        sHeals++;
        ctx->scriptPtr -= 2;
        return gScriptCmdTable[0x25](ctx); // Real healing, not a state stub.
    }
    else
        return UnexpectedEndingCommand(ctx);
    return FALSE;
}

static bool8 EndingQuest(struct ScriptContext *ctx)
{
    u8 operation = ScriptReadByte(ctx);
    EXPECT_EQ(ScriptReadByte(ctx), QUEST_STOP_TEAM_SOMBER);
    if (operation == CORMORIA_QUEST_MENU_COMPLETE_QUEST)
        sQuestCompletes++;
    else if (operation == CORMORIA_QUEST_MENU_BUFFER_QUEST_NAME)
        sQuestNames++;
    else
        return UnexpectedEndingCommand(ctx);
    ctx->scriptPtr -= 2;
    return gScriptCmdTable[0xE7](ctx); // Real quest mutation and name buffer.
}

static bool8 CommitEndingWarp(struct ScriptContext *ctx)
{
    EXPECT(sLocked);
    sCounts[0x39]++;
    // Native destination parsing and commit, without DoWarp scheduling map
    // loading or abandoning the runner. No rendering, avatar-reset, disk
    // persistence or live cutscene claim follows from this fixture.
    EXPECT(!gScriptCmdTable[0x3E](ctx));
    ApplyCurrentWarp();
    return FALSE;
}

static void RunSomberScript(const u8 *script, bool8 locked)
{
    static const u8 realCommands[] = {
        0x02, 0x05, 0x06, 0x0F, 0x16, 0x21, 0x29, 0x2A, 0x2B,
        0x47, 0x63, 0x85, 0x9F,
    };
    static const u8 presentationCommands[] = {
        0x09, 0x23, 0x28, 0x2F, 0x30, 0x31, 0x32, 0x4F, 0x51,
        0x53, 0x55, 0x57, 0x67, 0x68, 0x69, 0x6B, 0x6C, 0x97,
        0xA1, 0xC5,
    };
    struct ScriptContext ctx;
    struct ScriptContext *context = &ctx;
    ScrCmdFunc *commands = Alloc(256 * sizeof(*commands));
    u32 i, steps;
    ASSUME(commands != NULL);
    memset(sCounts, 0, 256 * sizeof(*sCounts));
    sShakes = sCameras = sHeals = sQuestCompletes = sQuestNames = 0;
    sLocked = locked;
    for (i = 0; i < 256; i++)
        commands[i] = UnexpectedEndingCommand;
    for (i = 0; i < ARRAY_COUNT(realCommands); i++)
    {
        EXPECT(realCommands[i] < gScriptCmdTableEnd - gScriptCmdTable);
        commands[realCommands[i]] = gScriptCmdTable[realCommands[i]];
    }
    for (i = 0; i < ARRAY_COUNT(presentationCommands); i++)
        commands[presentationCommands[i]] = EndingPresentation;
    commands[0x25] = EndingSpecial;
    commands[0x39] = CommitEndingWarp;
    commands[0xE7] = EndingQuest;
    InitScriptContext(&ctx, commands, commands + 256);
    SetupBytecodeScript(&ctx, script);
    for (steps = 0; ctx.scriptPtr != NULL && steps < 512; steps++)
    {
        sOpcode = ScriptReadByte(context);
        EXPECT(!ctx.cmdTable[sOpcode](&ctx));
    }
    EXPECT(ctx.scriptPtr == NULL);
    EXPECT_EQ(ctx.stackDepth, 0);
    EXPECT(!sLocked);
    Free(commands);
}

static void ExpectSomberQuestBit(enum CormoriaQuestBit bit, bool8 expected)
{
    bool8 value;
    EXPECT(CormoriaQuestState_Get(QUEST_STOP_TEAM_SOMBER, bit, &value));
    EXPECT_EQ(value, expected);
}

TEST("Cormoria Somber ending and normal finale lab commit regional completion")
{
    u32 i;
    u8 completion = 0;
    u16 heldItem = ITEM_ORAN_BERRY;
    bool8 unrelated;
    struct Pokemon *partySnapshot;
    struct Bag *bagSnapshot;
    struct WarpData origin;
    const struct HealLocation *respawn;
    MainCallback runnerCallback;
    for (i = 0; i < 4; i++)
        PARAMETRIZE { completion = i; }
    partySnapshot = Alloc(sizeof(gPlayerParty));
    bagSnapshot = Alloc(sizeof(*bagSnapshot));
    sCounts = Alloc(256 * sizeof(*sCounts));
    ASSUME(partySnapshot != NULL);
    ASSUME(bagSnapshot != NULL);
    ASSUME(sCounts != NULL);
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
    // Still the first lab cutscene, with Waterfall already owned. This isolates
    // unchanged shared data; the gift/full-pocket retry is a separate test.
    EXPECT(AddBagItem(ITEM_HM07, 1));
    if (completion & 1) FlagSet(FLAG_SYS_GAME_CLEAR);
    else FlagClear(FLAG_SYS_GAME_CLEAR);
    if (completion & 2) FlagSet(Cormoria_FLAG_SYS_GAME_CLEAR);
    else FlagClear(Cormoria_FLAG_SYS_GAME_CLEAR);
    FlagSet(Cormoria_FLAG_LAB_CALLTOACTION);
    VarSet(Cormoria_VAR_UNCHARTED_ISLAND_CUTSCENES, 4);
    FlagClear(Cormoria_FLAG_FINALE_DONE);
    FlagClear(Cormoria_FLAG_POST_FINALE_CUTSCENE);
    FlagClear(Cormoria_FLAG_HIDE_ROUTE1_NORMAL);
    FlagSet(Cormoria_FLAG_HIDE_ROUTE1_STRONG);
    FlagSet(Cormoria_FLAG_HIDE_TENEBRIS_TENEBRIS);
    FlagSet(Cormoria_FLAG_HIDE_TENEBRIS_FINALE);
    FlagClear(Cormoria_FLAG_HIDE_ISLAND_FINALE);
    EXPECT(CormoriaQuestState_Set(QUEST_STOP_TEAM_SOMBER, CORMORIA_QUEST_UNLOCKED, TRUE));
    EXPECT(CormoriaQuestState_Set(QUEST_STOP_TEAM_SOMBER, CORMORIA_QUEST_ACTIVE, TRUE));
    EXPECT(CormoriaQuestState_Set(QUEST_STOP_TEAM_SOMBER, CORMORIA_QUEST_REWARD, TRUE));
    EXPECT(CormoriaQuestState_Set(QUEST_STOP_TEAM_SOMBER, CORMORIA_QUEST_COMPLETED, FALSE));
    EXPECT(CormoriaQuestState_Set(QUEST_STOP_TEAM_SOMBER, CORMORIA_QUEST_FAVORITE, TRUE));
    EXPECT(CormoriaQuestState_SetSubquest(0, TRUE));
    memset(gSaveBlock1Ptr->objectEventTemplates, 0, sizeof(gSaveBlock1Ptr->objectEventTemplates));
    // Real setobjectxyperm writes these three saved templates, even though
    // actor spawning/movement is presentation-adapted rather than rendered.
    gSaveBlock1Ptr->objectEventTemplates[0].localId = 34; // Koraidon.
    gSaveBlock1Ptr->objectEventTemplates[1].localId = 28; // Clefable.
    gSaveBlock1Ptr->objectEventTemplates[2].localId = 27; // Breech.
    gSaveBlock1Ptr->location.mapGroup = MAP_GROUP(MAP_CORMORIA_UNCHARTED_ISLAND);
    gSaveBlock1Ptr->location.mapNum = MAP_NUM(MAP_CORMORIA_UNCHARTED_ISLAND);
    gSaveBlock1Ptr->location.warpId = WARP_ID_NONE;
    gSaveBlock1Ptr->location.x = 78;
    gSaveBlock1Ptr->location.y = 24;
    origin = gSaveBlock1Ptr->location;
    memcpy(partySnapshot, gPlayerParty, sizeof(gPlayerParty));
    *bagSnapshot = gSaveBlock1Ptr->bag;
    runnerCallback = gMain.callback2;

    // Ending is entered from the already locked finale battle/cutscene.
    RunSomberScript(Cormoria_UnchartedIsland_Finale_Ending, TRUE);
    EXPECT_EQ(sCounts[0x09], 17);
    EXPECT_EQ(sCounts[0x23], 17);
    EXPECT_EQ(sCounts[0x28], 35);
    EXPECT_EQ(sCounts[0x4F], 44);
    EXPECT_EQ(sCounts[0x51], 19);
    EXPECT_EQ(sCounts[0x53], 1);
    EXPECT_EQ(sCounts[0x55], 6);
    EXPECT_EQ(sCounts[0x57], 4);
    EXPECT_EQ(sCounts[0x97], 4);
    EXPECT_EQ(sCounts[0xA1], 2);
    EXPECT_EQ(sCounts[0xC5], 1);
    EXPECT_EQ(sCounts[0x2F], 3);
    EXPECT_EQ(sCounts[0x30], 1);
    EXPECT_EQ(sCounts[0x39], 1);
    EXPECT_EQ(sCounts[0x69], 0);
    EXPECT_EQ(sCounts[0x6B], 1);
    EXPECT_EQ(sCounts[0x6C], 0);
    EXPECT_EQ(sCounts[0x31], 0);
    EXPECT_EQ(sCounts[0x32], 0);
    EXPECT_EQ(sCounts[0x67], 0);
    EXPECT_EQ(sCounts[0x68], 0);
    EXPECT_EQ(sShakes, 2);
    EXPECT_EQ(sCameras, 1);
    EXPECT_EQ(sHeals, 0);
    EXPECT_EQ(sQuestCompletes, 0);
    EXPECT_EQ(VarGet(Cormoria_VAR_UNCHARTED_ISLAND_CUTSCENES), 5);
    EXPECT(FlagGet(Cormoria_FLAG_FINALE_DONE));
    EXPECT(!FlagGet(Cormoria_FLAG_POST_FINALE_CUTSCENE));
    EXPECT_EQ(FlagGet(FLAG_SYS_GAME_CLEAR), completion & 1);
    EXPECT_EQ(FlagGet(Cormoria_FLAG_SYS_GAME_CLEAR), (completion >> 1) & 1);
    EXPECT(!FlagGet(Cormoria_FLAG_HIDE_ROUTE1_NORMAL));
    EXPECT(FlagGet(Cormoria_FLAG_HIDE_ROUTE1_STRONG));
    EXPECT(FlagGet(Cormoria_FLAG_HIDE_TENEBRIS_TENEBRIS));
    EXPECT(FlagGet(Cormoria_FLAG_HIDE_TENEBRIS_FINALE));
    EXPECT(!FlagGet(Cormoria_FLAG_HIDE_ISLAND_FINALE));
    EXPECT(FlagGet(Cormoria_FLAG_LAB_CALLTOACTION));
    ExpectSomberQuestBit(CORMORIA_QUEST_UNLOCKED, TRUE);
    ExpectSomberQuestBit(CORMORIA_QUEST_ACTIVE, TRUE);
    ExpectSomberQuestBit(CORMORIA_QUEST_REWARD, TRUE);
    ExpectSomberQuestBit(CORMORIA_QUEST_COMPLETED, FALSE);
    ExpectSomberQuestBit(CORMORIA_QUEST_FAVORITE, TRUE);
    EXPECT_EQ(gMain.callback2, runnerCallback);
    EXPECT_EQ(gSaveBlock1Ptr->objectEventTemplates[0].x, 79);
    EXPECT_EQ(gSaveBlock1Ptr->objectEventTemplates[0].y, 26);
    EXPECT_EQ(gSaveBlock1Ptr->objectEventTemplates[1].x, 78);
    EXPECT_EQ(gSaveBlock1Ptr->objectEventTemplates[1].y, 33);
    EXPECT_EQ(gSaveBlock1Ptr->objectEventTemplates[2].x, 78);
    EXPECT_EQ(gSaveBlock1Ptr->objectEventTemplates[2].y, 34);
    EXPECT_EQ(memcmp(&gLastUsedWarp, &origin, sizeof(origin)), 0);
    EXPECT_EQ(gSaveBlock1Ptr->location.mapGroup, MAP_GROUP(MAP_CORMORIA_CARABRUE_TOWN_TENEBRIS_LAB_FINALE));
    EXPECT_EQ(gSaveBlock1Ptr->location.mapNum, MAP_NUM(MAP_CORMORIA_CARABRUE_TOWN_TENEBRIS_LAB_FINALE));
    EXPECT_EQ(gSaveBlock1Ptr->location.warpId, WARP_ID_NONE);
    EXPECT_EQ(gSaveBlock1Ptr->location.x, 5);
    EXPECT_EQ(gSaveBlock1Ptr->location.y, 4);
    EXPECT_EQ(memcmp(partySnapshot, gPlayerParty, sizeof(gPlayerParty)), 0);
    EXPECT_EQ(memcmp(bagSnapshot, &gSaveBlock1Ptr->bag, sizeof(*bagSnapshot)), 0);
    EXPECT(WorldEventSave_Validate(&gSaveblock1.world_event));

    VarSet(VAR_TEMP_1, 0); // Map on-frame table entry condition.
    RunSomberScript(Cormoria_CarabrueTown_TenebrisLab_PostFinale, FALSE);
    EXPECT_EQ(sCounts[0x09], 19);
    EXPECT_EQ(sCounts[0x23], 19);
    EXPECT_EQ(sCounts[0x28], 37);
    EXPECT_EQ(sCounts[0x4F], 17);
    EXPECT_EQ(sCounts[0x51], 13);
    EXPECT_EQ(sCounts[0x69], 1);
    EXPECT_EQ(sCounts[0x6B], 1);
    EXPECT_EQ(sCounts[0x6C], 1);
    EXPECT_EQ(sCounts[0x68], 1);
    EXPECT_EQ(sCounts[0x67], 1);
    EXPECT_EQ(sCounts[0x31], 1);
    EXPECT_EQ(sCounts[0x32], 1);
    EXPECT_EQ(sCounts[0x39], 0);
    EXPECT_EQ(sCounts[0x53], 0);
    EXPECT_EQ(sCounts[0x55], 0);
    EXPECT_EQ(sCounts[0x57], 0);
    EXPECT_EQ(sCounts[0x97], 0);
    EXPECT_EQ(sCounts[0xA1], 0);
    EXPECT_EQ(sCounts[0xC5], 0);
    EXPECT_EQ(sCounts[0x2F], 0);
    EXPECT_EQ(sCounts[0x30], 0);
    EXPECT_EQ(sShakes, 0);
    EXPECT_EQ(sCameras, 0);
    EXPECT_EQ(sHeals, 1);
    EXPECT_EQ(sQuestCompletes, 1);
    EXPECT_EQ(sQuestNames, 1);
    EXPECT_EQ(VarGet(VAR_TEMP_1), 1);
    EXPECT_EQ(VarGet(Cormoria_VAR_UNCHARTED_ISLAND_CUTSCENES), 5);
    EXPECT(FlagGet(Cormoria_FLAG_FINALE_DONE));
    EXPECT(FlagGet(Cormoria_FLAG_POST_FINALE_CUTSCENE));
    EXPECT(FlagGet(Cormoria_FLAG_HIDE_ROUTE1_NORMAL));
    EXPECT(!FlagGet(Cormoria_FLAG_HIDE_ROUTE1_STRONG));
    EXPECT(!FlagGet(Cormoria_FLAG_HIDE_TENEBRIS_TENEBRIS));
    EXPECT(FlagGet(Cormoria_FLAG_HIDE_TENEBRIS_FINALE));
    EXPECT(!FlagGet(Cormoria_FLAG_HIDE_ISLAND_FINALE));
    EXPECT(FlagGet(Cormoria_FLAG_LAB_CALLTOACTION));
    EXPECT_EQ(FlagGet(FLAG_SYS_GAME_CLEAR), completion & 1);
    EXPECT_EQ(FlagGet(Cormoria_FLAG_SYS_GAME_CLEAR), (completion >> 1) & 1);
    ExpectSomberQuestBit(CORMORIA_QUEST_UNLOCKED, TRUE);
    ExpectSomberQuestBit(CORMORIA_QUEST_ACTIVE, FALSE);
    ExpectSomberQuestBit(CORMORIA_QUEST_REWARD, FALSE);
    ExpectSomberQuestBit(CORMORIA_QUEST_COMPLETED, TRUE);
    ExpectSomberQuestBit(CORMORIA_QUEST_FAVORITE, TRUE);
    EXPECT(CormoriaQuestState_GetSubquest(0, &unrelated));
    EXPECT(unrelated);
    EXPECT_EQ(gMain.callback2, runnerCallback);
    EXPECT_EQ(gPlayerPartyCount, 1);
    EXPECT_EQ(memcmp(partySnapshot, gPlayerParty, sizeof(gPlayerParty)), 0);
    EXPECT_EQ(memcmp(bagSnapshot, &gSaveBlock1Ptr->bag, sizeof(*bagSnapshot)), 0);
    EXPECT(WorldEventSave_Validate(&gSaveblock1.world_event));
    EXPECT_EQ(gSaveBlock1Ptr->location.mapGroup, MAP_GROUP(MAP_CORMORIA_CARABRUE_TOWN_TENEBRIS_LAB_FINALE));
    EXPECT_EQ(gSaveBlock1Ptr->location.mapNum, MAP_NUM(MAP_CORMORIA_CARABRUE_TOWN_TENEBRIS_LAB_FINALE));
    EXPECT_EQ(gSaveBlock1Ptr->location.x, 5);
    EXPECT_EQ(gSaveBlock1Ptr->location.y, 4);
    // Correct regional respawn oracle: donor 0x0303 must resolve to the
    // appended native heal location, never truncate to host index 3.
    respawn = GetHealLocation(HEAL_LOCATION_CORMORIA_CARABRUE_TOWN);
    ASSUME(respawn != NULL);
    EXPECT_EQ(gSaveBlock1Ptr->lastHealLocation.mapGroup, respawn->mapGroup);
    EXPECT_EQ(gSaveBlock1Ptr->lastHealLocation.mapNum, respawn->mapNum);
    EXPECT_EQ(gSaveBlock1Ptr->lastHealLocation.warpId, WARP_ID_NONE);
    EXPECT_EQ(gSaveBlock1Ptr->lastHealLocation.x, respawn->x);
    EXPECT_EQ(gSaveBlock1Ptr->lastHealLocation.y, respawn->y);
    Free(sCounts);
    sCounts = NULL;
    Free(bagSnapshot);
    Free(partySnapshot);
}
#endif
