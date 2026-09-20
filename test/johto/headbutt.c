#include "global.h"
#include "battle.h"
#include "battle_setup.h"
#include "bg.h"
#include "event_data.h"
#include "event_object_movement.h"
#include "field_camera.h"
#include "field_message_box.h"
#include "field_name_box.h"
#include "field_screen_effect.h"
#include "field_weather.h"
#include "fieldmap.h"
#include "johto/field_moves.h"
#include "johto/save.h"
#include "load_save.h"
#include "main.h"
#include "menu.h"
#include "overworld.h"
#include "palette.h"
#include "pokemon.h"
#include "script.h"
#include "sprite.h"
#include "task.h"
#include "text.h"
#include "wild_encounter.h"
#include "window.h"
#include "test/test.h"
#include "constants/battle.h"
#include "constants/event_objects.h"
#include "constants/johto_content.h"
#include "constants/maps.h"
#include "constants/metatile_behaviors.h"
#include "constants/vars.h"

extern const u8 Johto_EventScript_Headbutt[];
extern void ShakeCamera(void);
extern void ReinitCallbacks(void);

static EWRAM_DATA struct Pokemon sParty[PARTY_SIZE + 2];

static const u16 sReturnMapData[] = {0};
static const u16 sReturnMapBorder[] = {0, 0, 0, 0};
static const u16 sReturnMapAttributes[NUM_METATILES_IN_PRIMARY] = {0};
static const struct Tileset sReturnMapTileset =
{
    .metatileAttributes = sReturnMapAttributes,
};
static const struct MapLayout sReturnMapLayout =
{
    .width = 1,
    .height = 1,
    .border = sReturnMapBorder,
    .map = sReturnMapData,
    .primaryTileset = &sReturnMapTileset,
    .secondaryTileset = &sReturnMapTileset,
};
static const struct MapEvents sReturnMapEvents = {0};

/* WAITSTATE is 0x27, SETVAR is 0x16 and RELEASEALL is 0x6b in the
 * production script command table.  This continuation makes the camera
 * task's ScriptContext_Enable observable without replacing that task. */
static const u8 sCameraContinuation[] =
{
    0x27,
    0x16, VAR_TEMP_0 & 0xFF, VAR_TEMP_0 >> 8, 1, 0,
    0x6b,
    0x02,
};

static void MakeHeadbuttMon(struct Pokemon *mon, u32 slot)
{
    u32 i;

    CreateMon(mon, SPECIES_TOTODILE, 10, 1, OTID_STRUCT_PLAYER_ID);
    for (i = 0; i < MAX_MON_MOVES; i++)
        SetMonMoveSlot(mon, i == slot ? MOVE_HEADBUTT : MOVE_NONE, i);
}

TEST("Headbutt selects the first non-egg user across every move slot")
{
    u32 moveSlot;

    SetSaveBlocksPointers(0);
    for (moveSlot = 0; moveSlot < MAX_MON_MOVES; moveSlot++)
    {
        memset(sParty, 0, sizeof(sParty));
        MakeHeadbuttMon(&sParty[PARTY_SIZE - 1], moveSlot);
        EXPECT_EQ(JohtoFieldMoves_GetHeadbuttUser(sParty, PARTY_SIZE), PARTY_SIZE - 1);
        MakeHeadbuttMon(&sParty[0], moveSlot);
        EXPECT_EQ(JohtoFieldMoves_GetHeadbuttUser(sParty, PARTY_SIZE), 0);
    }
}

TEST("Headbutt ignores empty and egg slots, allows fainted users, and bounds count")
{
    SetSaveBlocksPointers(0);
    memset(sParty, 0, sizeof(sParty));
    EXPECT_EQ(JohtoFieldMoves_GetHeadbuttUser(sParty, PARTY_SIZE), PARTY_SIZE);

    MakeHeadbuttMon(&sParty[0], 0);
    SetMonData(&sParty[0], MON_DATA_SPECIES, &(u16){SPECIES_NONE});
    MakeHeadbuttMon(&sParty[1], 0);
    SetMonData(&sParty[1], MON_DATA_IS_EGG, &(u8){TRUE});
    MakeHeadbuttMon(&sParty[2], MAX_MON_MOVES);
    EXPECT_EQ(JohtoFieldMoves_GetHeadbuttUser(sParty, PARTY_SIZE), PARTY_SIZE);

    MakeHeadbuttMon(&sParty[3], 0);
    EXPECT_EQ(JohtoFieldMoves_GetHeadbuttUser(sParty, 3), PARTY_SIZE);
    EXPECT_EQ(JohtoFieldMoves_GetHeadbuttUser(sParty, 4), 3);
    SetMonData(&sParty[3], MON_DATA_HP, &(u16){0});
    EXPECT_EQ(JohtoFieldMoves_GetHeadbuttUser(sParty, 4), 3);

    // Keep the out-of-range sentinel distinguishable after the fainted-user
    // eligibility assertion above.  The native clamps scans to PARTY_SIZE.
    memset(&sParty[3], 0, sizeof(sParty[3]));
    MakeHeadbuttMon(&sParty[PARTY_SIZE + 1], 0);
    EXPECT_EQ(JohtoFieldMoves_GetHeadbuttUser(sParty, 0xFFFFFFFF), PARTY_SIZE);
}

TEST("Headbutt selector is limited to the reserved tree behavior")
{
    u16 behavior;

    for (behavior = 0; behavior <= UINT8_MAX; behavior++)
    {
        const u8 *script = JohtoFieldMoves_GetHeadbuttScript((u8)behavior);

        if ((u8)behavior == MB_JOHTO_HEADBUTT_TREE)
            EXPECT_EQ(script, Johto_EventScript_Headbutt);
        else
            EXPECT_EQ(script, NULL);
    }
}

TEST("Headbutt native reports the party index through the V1 effect boundary")
{
    struct ScriptContext ctx;
    u8 program[6];
    u32 i;
    u32 pointer = (uintptr_t)Script_JohtoCheckHeadbutt | 0x0A000000;

    SetSaveBlocksPointers(0);
    InitEventData();
    gMain.inBattle = FALSE;
    gBattleTypeFlags = 0;
    memset(gParties, 0, sizeof(gParties));
    memset(gPartiesCount, 0, sizeof(gPartiesCount));
    memset(sParty, 0, sizeof(sParty));
    gPlayerPartyCount = PARTY_SIZE;
    MakeHeadbuttMon(&gPlayerParty[2], 3);

    program[0] = 0x23; // callnative with requests_effects=1
    for (i = 0; i < 4; i++)
        program[i + 1] = pointer >> (8 * i);
    program[5] = 0x02; // end

    gSpecialVar_Result = 0xBEEF;
    EXPECT(!RunScriptImmediatelyUntilEffect(SCREFF_V1, program, NULL));
    EXPECT_EQ(gSpecialVar_Result, 2);

    InitScriptContext(&ctx, NULL, NULL);
    ctx.scriptPtr = program;
    Script_JohtoCheckHeadbutt(&ctx);
    EXPECT_EQ(ctx.scriptPtr, program);
    EXPECT_EQ(gSpecialVar_Result, 2);

    gPlayerPartyCount = 0;
    Script_JohtoCheckHeadbutt(&ctx);
    EXPECT_EQ(gSpecialVar_Result, PARTY_SIZE);
}

TEST("Headbutt camera feedback completes its bounded task and restores control")
{
    s16 beforeX, beforeY, afterX, afterY;
    u32 frame;

    SetSaveBlocksPointers(0);
    InitEventData();
    ResetTasks();
    VarSet(VAR_TEMP_0, 0);
    InstallCameraPanAheadCallback();
    ScriptContext_SetupScript(sCameraContinuation);
    EXPECT(ScriptContext_RunScript()); // reaches the actual WAITSTATE
    EXPECT(!ScriptContext_IsEnabled());
    EXPECT(ArePlayerFieldControlsLocked());

    GetCameraOffsetWithPan(&beforeX, &beforeY);
    gSpecialVar_0x8004 = 0;
    gSpecialVar_0x8005 = 1;
    gSpecialVar_0x8006 = 4;
    gSpecialVar_0x8007 = 2;
    ShakeCamera();
    EXPECT_EQ(GetTaskCount(), 1);
    EXPECT(!ScriptContext_RunScript()); // the continuation cannot run early
    EXPECT_EQ(VarGet(VAR_TEMP_0), 0);

    for (frame = 0; frame < 8; frame++)
        RunTasks();
    EXPECT_EQ(GetTaskCount(), 0);
    EXPECT(ScriptContext_IsEnabled()); // StopCameraShake enabled the waiter

    GetCameraOffsetWithPan(&afterX, &afterY);
    EXPECT_EQ(afterX, beforeX);
    EXPECT_EQ(afterY, beforeY);
    UpdateCameraPanning();
    EXPECT_EQ(gSpriteCoordOffsetX, gTotalCameraPixelOffsetX - afterX);
    EXPECT_EQ(gSpriteCoordOffsetY, gTotalCameraPixelOffsetY - afterY);

    EXPECT(!ScriptContext_RunScript()); // SETVAR, RELEASEALL and END
    EXPECT_EQ(VarGet(VAR_TEMP_0), 1);
    EXPECT(!ArePlayerFieldControlsLocked());
    ScriptContext_Init();
}

static void ResetWildReturnFixture(void)
{
    SetSaveBlocksPointers(0);
    InitEventData();
    ResetTasks();
    memset(gParties, 0, sizeof(gParties));
    memset(gPartiesCount, 0, sizeof(gPartiesCount));
    memset(gObjectEvents, 0, sizeof(gObjectEvents));
    memset(gSprites, 0, sizeof(gSprites));
    gMapHeader = (struct MapHeader){
        .mapLayout = &sReturnMapLayout,
        .events = &sReturnMapEvents,
    };
    gPlayerAvatar.objectEventId = 0;
    gObjectEvents[0].active = TRUE;
    gObjectEvents[0].isPlayer = TRUE;
    gObjectEvents[0].localId = LOCALID_PLAYER;
    gObjectEvents[0].mapNum = 0;
    gObjectEvents[0].mapGroup = 0;
    gObjectEvents[0].spriteId = 0;
    gObjectEvents[0].currentCoords.x = MAP_OFFSET;
    gObjectEvents[0].currentCoords.y = MAP_OFFSET;
    gSprites[0].inUse = TRUE;
    gObjectEvents[1].active = TRUE;
    gObjectEvents[1].localId = 1;
    gObjectEvents[1].mapNum = 0;
    gObjectEvents[1].mapGroup = 0;
    gObjectEvents[1].spriteId = 1;
    gObjectEvents[1].frozen = TRUE;
    gSprites[1].inUse = TRUE;
    gPaletteFade.active = FALSE;
    gMain.callback1 = NULL;
    gMain.callback2 = NULL;
    gMain.savedCallback = NULL;
    gMain.state = 0;
    gMain.inBattle = FALSE;
    gFieldCallback = NULL;
    gFieldCallback2 = NULL;
    InitMap();
}

TEST("Headbutt ordinary wild return uses host field and whiteout callbacks")
{
    ResetWildReturnFixture();
    BattleSetup_StartWildBattle();
    EXPECT(gMain.savedCallback != NULL);
    gBattleOutcome = B_OUTCOME_WON;
    gMain.savedCallback();
    EXPECT_EQ(gMain.callback2, CB2_ReturnToField);
    EXPECT_EQ(gFieldCallback, FieldCB_ReturnToFieldNoScriptCheckMusic);

    // The callback owns the waiting script's return.  Exercise the real field
    // task with a completed fade and verify that it restores controls/objects.
    ResetTasks();
    LockPlayerFieldControls();
    gFieldCallback();
    gPaletteFade.active = FALSE;
    // The return task waits for weather processing, not just the palette flag.
    EXPECT(!IsWeatherNotFadingIn());
    RunTasks();
    EXPECT(ArePlayerFieldControlsLocked());
    EXPECT(gObjectEvents[1].frozen);
    gWeatherPtr->palProcessingState = WEATHER_PAL_STATE_IDLE;
    RunTasks();
    EXPECT(!ArePlayerFieldControlsLocked());
    EXPECT(!gObjectEvents[1].frozen);

    ResetWildReturnFixture();
    BattleSetup_StartWildBattle();
    EXPECT(gMain.savedCallback != NULL);
    gBattleOutcome = B_OUTCOME_LOST;
    gMain.savedCallback();
    EXPECT_EQ(gMain.callback2, CB2_WhiteOut);
    ResetTasks(); // The fixture invokes return directly, bypassing battle start.
    // Resume the harness after observing callbacks that deliberately replace it.
    ReinitCallbacks();
}

static void ExerciseLinkedHeadbutt(bool32 unlocked, bool32 knowsMove)
{
    static const struct BgTemplate bg = {.bg = 0, .charBaseIndex = 2, .mapBaseIndex = 28};
    u32 frame;
    bool32 sawMessage = FALSE, sawCamera = FALSE, sawLocked = FALSE;
    s16 initialX, initialY, x, y;
    u16 initialTile;
    u8 oldTextSpeed;

    ResetWildReturnFixture();
    JohtoSave_InitializeCurrent();
    ScriptContext_Init();
    gBattleTypeFlags = 0;
    gPlayerAvatar.tileTransitionState = 0;
    gSaveBlock1Ptr->location.mapGroup = MAP_GROUP(MAP_LITTLEROOT_TOWN);
    gSaveBlock1Ptr->location.mapNum = MAP_NUM(MAP_LITTLEROOT_TOWN);
    EXPECT_EQ(GetCurrentMapWildMonHeaderId(), HEADER_NONE);
    if (unlocked)
        FlagSet(JOHTO_FLAG_GET_HEADBUTT);
    else
        FlagClear(JOHTO_FLAG_GET_HEADBUTT);
    gPlayerPartyCount = 1;
    MakeHeadbuttMon(&gPlayerParty[0], knowsMove ? 0 : MAX_MON_MOVES);
    memcpy(sParty, gPlayerParty, sizeof(gPlayerParty));
    initialTile = MapGridGetMetatileIdAt(MAP_OFFSET, MAP_OFFSET);

    ResetBgsAndClearDma3BusyFlags(0);
    InitBgsFromTemplates(0, &bg, 1);
    InitStandardTextBoxWindows();
    InitFieldMessageBox();
    DeactivateAllTextPrinters();
    gSpeakerName = NULL;
    gMsgIsSignPost = FALSE;
    oldTextSpeed = gSaveBlock2Ptr->optionsTextSpeed;
    gSaveBlock2Ptr->optionsTextSpeed = OPTIONS_TEXT_SPEED_INSTANT;
    InstallCameraPanAheadCallback();
    GetCameraOffsetWithPan(&initialX, &initialY);
    gSpecialVar_0x8004 = gSpecialVar_0x8005 = 0x55;
    gSpecialVar_0x8006 = gSpecialVar_0x8007 = 0x55;
    ScriptContext_SetupScript(Johto_EventScript_Headbutt);
    for (frame = 0; frame < 256; frame++)
    {
        gMain.newKeys = gMain.heldKeys = A_BUTTON;
        ScriptContext_RunScript();
        sawLocked |= ArePlayerFieldControlsLocked();
        sawMessage |= !IsFieldMessageBoxHidden();
        RunTasks(); // The message task itself pumps its text printer.
        GetCameraOffsetWithPan(&x, &y);
        sawCamera |= x != initialX || y != initialY;
        if (!ScriptContext_IsEnabled() && !ArePlayerFieldControlsLocked() && GetTaskCount() == 0)
            break;
    }
    EXPECT(frame < 256);
    EXPECT(sawLocked && sawMessage);
    EXPECT(IsFieldMessageBoxHidden());
    EXPECT(!gObjectEvents[1].frozen);
    EXPECT_EQ(gMain.savedCallback, NULL);
    EXPECT(!gMain.inBattle);
    EXPECT_EQ(sawCamera, unlocked && knowsMove);
    EXPECT_EQ(x, initialX);
    EXPECT_EQ(y, initialY);
    EXPECT_EQ(gSpecialVar_0x8004, unlocked && knowsMove ? 0 : 0x55);
    EXPECT_EQ(gSpecialVar_0x8005, unlocked && knowsMove ? 1 : 0x55);
    EXPECT_EQ(gSpecialVar_0x8006, unlocked && knowsMove ? 4 : 0x55);
    EXPECT_EQ(gSpecialVar_0x8007, unlocked && knowsMove ? 2 : 0x55);
    if (unlocked)
        EXPECT_EQ(gSpecialVar_Result, knowsMove ? FALSE : PARTY_SIZE);
    EXPECT_EQ(FlagGet(JOHTO_FLAG_GET_HEADBUTT), unlocked);
    EXPECT_EQ(MapGridGetMetatileIdAt(MAP_OFFSET, MAP_OFFSET), initialTile);
    EXPECT_EQ(gPlayerPartyCount, 1);
    EXPECT_EQ(memcmp(sParty, gPlayerParty, sizeof(gPlayerParty)), 0);

    gMain.newKeys = gMain.heldKeys = 0;
    gSaveBlock2Ptr->optionsTextSpeed = oldTextSpeed;
    ScriptContext_Init();
    HideFieldMessageBox();
    DeactivateAllTextPrinters();
    ResetTasks();
    FreeAllWindowBuffers();
    ResetBgsAndClearDma3BusyFlags(0);
    ReinitCallbacks();
}

TEST("Headbutt linked script denies a user without the unlock and releases controls")
{
    ExerciseLinkedHeadbutt(FALSE, TRUE);
}

TEST("Headbutt linked script denies an unlocked party without the move")
{
    ExerciseLinkedHeadbutt(TRUE, FALSE);
}

TEST("Headbutt linked script completes feedback and releases when no encounter exists")
{
    ExerciseLinkedHeadbutt(TRUE, TRUE);
}
