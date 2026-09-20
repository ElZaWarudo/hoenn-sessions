#include "global.h"
#include "event_data.h"
#include "event_object_movement.h"
#include "field_effect.h"
#include "pokemon.h"
#include "johto/events.h"
#include "johto/save.h"
#include "load_save.h"
#include "script.h"
#include "constants/johto_events.h"
#include "constants/event_objects.h"
#include "constants/flags.h"
#include "constants/vars.h"
#include "test/test.h"

extern bool8 ScrCmd_setvar(struct ScriptContext *ctx);
extern bool8 ScrCmd_compare_var_to_value(struct ScriptContext *ctx);
extern void GetDirectionToFaceScript(struct ScriptContext *ctx);
extern void IsFollowerFieldMoveUser(struct ScriptContext *ctx);
#if TESTING
extern bool32 FieldControlAvatar_TestShouldTriggerScriptRun(const struct CoordEvent *coordEvent);
#endif

static void PutHalfword(u8 *script, u16 value)
{
    script[0] = value;
    script[1] = value >> 8;
}

TEST("Johto event IDs stay bounded and separate from legacy storage")
{
    JohtoSave_InitializeCurrent();

    EXPECT(JohtoEvent_IsFlagId(JOHTO_FLAG_START));
    EXPECT(JohtoEvent_IsFlagId(JOHTO_FLAG_END));
    EXPECT(!JohtoEvent_IsFlagId(JOHTO_FLAG_END + 1));
    EXPECT(JohtoEvent_IsVariableId(JOHTO_VAR_START));
    EXPECT(JohtoEvent_IsVariableId(JOHTO_VAR_END));
    EXPECT(!JohtoEvent_IsVariableId(JOHTO_VAR_END + 1));
    EXPECT(JohtoEvent_IsReservedId(JOHTO_FLAG_END + 1));
    EXPECT(JohtoEvent_IsReservedId(JOHTO_VAR_END + 1));

    EXPECT_EQ(GetFlagPointer(JOHTO_FLAG_START), NULL);
    EXPECT_EQ(GetVarPointer(JOHTO_VAR_START), NULL);
    EXPECT(JohtoEvent_SetFlag(JOHTO_FLAG_START, TRUE));
    EXPECT(JohtoEvent_GetFlag(JOHTO_FLAG_START));
    EXPECT(JohtoEvent_SetVariable(JOHTO_VAR_END, 0xBEEF));
    EXPECT_EQ(JohtoEvent_GetVariable(JOHTO_VAR_END), 0xBEEF);
    EXPECT_EQ(VarGet(JOHTO_VAR_END), 0xBEEF);
    EXPECT_EQ(VarGetIfExist(JOHTO_VAR_END), 0xBEEF);

    EXPECT(!VarSet(JOHTO_FLAG_START, 1));
    EXPECT(!VarSet(JOHTO_VAR_END + 1, 1));
    EXPECT(!FlagGet(JOHTO_VAR_START));
    EXPECT_EQ(VarGetIfExist(JOHTO_FLAG_START), 65535);
    EXPECT(JohtoSave_Validate(&gSaveblock1.johto));
}

TEST("Johto event setters reject corrupt records without mutation")
{
    struct JohtoSaveV1 snapshot;

    JohtoSave_InitializeCurrent();
    gSaveblock1.johto.flag_bits[0] ^= 1;
    snapshot = gSaveblock1.johto;

    EXPECT(!JohtoEvent_SetFlag(JOHTO_FLAG_START, TRUE));
    EXPECT(!JohtoEvent_SetVariable(JOHTO_VAR_START, 1));
    EXPECT_EQ(memcmp(&snapshot, &gSaveblock1.johto, sizeof(snapshot)), 0);
}

TEST("Johto reserved event IDs cannot access other storage")
{
    static const u16 holes[] = {0x6300, 0x6FFF, 0x7060, 0x7FFF};
    static const u16 endpoints[] = {JOHTO_FLAG_START, JOHTO_FLAG_END};
    static const u16 flags[] = {FLAG_TEMP_1, FLAG_SYS_POKEDEX_GET, FLAG_HIDE_MAP_NAME_POPUP, TESTING_FLAG_UNUSED_3};
    static const u16 vars[] = {VAR_TEMP_0, VAR_OBJ_GFX_ID_0, VAR_0x8000, TESTING_VAR_UNUSED_2};
    struct JohtoSaveV1 snapshot;
    u32 i;

    JohtoSave_InitializeCurrent();
    for (i = 0; i < ARRAY_COUNT(flags); i++)
        FlagSet(flags[i]);
    for (i = 0; i < ARRAY_COUNT(vars); i++)
        EXPECT(VarSet(vars[i], 0xA100 + i));
    snapshot = gSaveblock1.johto;
    for (i = 0; i < ARRAY_COUNT(holes); i++)
    {
        EXPECT_EQ(GetFlagPointer(holes[i]), NULL);
        EXPECT_EQ(GetVarPointer(holes[i]), NULL);
        FlagSet(holes[i]);
        FlagClear(holes[i]);
        FlagToggle(holes[i]);
        EXPECT(!FlagGet(holes[i]));
        EXPECT(!VarSet(holes[i], 0xBEEF));
        EXPECT_EQ(VarGet(holes[i]), holes[i]);
        EXPECT_EQ(VarGetIfExist(holes[i]), 65535);
        EXPECT_EQ(memcmp(&snapshot, &gSaveblock1.johto, sizeof(snapshot)), 0);
    }
    FlagSet(JOHTO_VAR_START);
    FlagClear(JOHTO_VAR_END);
    FlagToggle(JOHTO_VAR_START);
    EXPECT(!VarSet(JOHTO_FLAG_START, 9));
    EXPECT(!VarSet(JOHTO_FLAG_END, 9));
    EXPECT_EQ(memcmp(&snapshot, &gSaveblock1.johto, sizeof(snapshot)), 0);
    for (i = 0; i < ARRAY_COUNT(endpoints); i++)
    {
        FlagSet(endpoints[i]);
        EXPECT(FlagGet(endpoints[i]));
        EXPECT(JohtoSave_Validate(&gSaveblock1.johto));
        FlagToggle(endpoints[i]);
        EXPECT(!FlagGet(endpoints[i]));
        EXPECT(JohtoSave_Validate(&gSaveblock1.johto));
        FlagSet(endpoints[i]);
        FlagClear(endpoints[i]);
        EXPECT(!FlagGet(endpoints[i]));
        EXPECT(JohtoSave_Validate(&gSaveblock1.johto));
    }
    for (i = 0; i < ARRAY_COUNT(flags); i++)
        EXPECT(FlagGet(flags[i]));
    for (i = 0; i < ARRAY_COUNT(vars); i++)
        EXPECT_EQ(VarGet(vars[i]), 0xA100 + i);
}

TEST("Johto script value writes seal the record and preserve operands")
{
    struct ScriptContext ctx;
    u8 script[4];

    JohtoSave_InitializeCurrent();
    PutHalfword(&script[0], JOHTO_VAR_START);
    PutHalfword(&script[2], 0x1234);
    InitScriptContext(&ctx, NULL, NULL);
    ctx.scriptPtr = script;
    EXPECT(!ScrCmd_setvar(&ctx));
    EXPECT_EQ(ctx.scriptPtr, script + sizeof(script));
    EXPECT_EQ(VarGet(JOHTO_VAR_START), 0x1234);
    EXPECT(JohtoSave_Validate(&gSaveblock1.johto));

    PutHalfword(&script[0], JOHTO_VAR_START);
    PutHalfword(&script[2], 0x1234);
    ctx.scriptPtr = script;
    EXPECT(!ScrCmd_compare_var_to_value(&ctx));
    EXPECT_EQ(ctx.comparisonResult, 1);
}

TEST("Johto native and coordinate consumers use value APIs")
{
    struct ScriptContext ctx;
    struct CoordEvent coordEvent = {0};
    u8 script[4];

    JohtoSave_InitializeCurrent();
    memset(gObjectEvents, 0, sizeof(gObjectEvents));
    PutHalfword(&script[0], JOHTO_VAR_START);
    script[2] = 1;
    script[3] = 2;
    InitScriptContext(&ctx, NULL, NULL);
    ctx.scriptPtr = script;
    EXPECT(VarSet(JOHTO_VAR_START, 0xBEEF));
    GetDirectionToFaceScript(&ctx);
    EXPECT_EQ(VarGet(JOHTO_VAR_START), DIR_NONE);
    EXPECT_EQ(ctx.scriptPtr, script + 4);
    EXPECT(JohtoSave_Validate(&gSaveblock1.johto));

    gObjectEvents[0].active = TRUE;
    gObjectEvents[0].localId = 1;
    gObjectEvents[0].currentCoords.x = 5;
    gObjectEvents[0].currentCoords.y = 5;
    gObjectEvents[1].active = TRUE;
    gObjectEvents[1].localId = 2;
    gObjectEvents[1].currentCoords.x = 6;
    gObjectEvents[1].currentCoords.y = 5;
    ctx.scriptPtr = script;
    EXPECT(VarSet(JOHTO_VAR_START, 0xBEEF));
    GetDirectionToFaceScript(&ctx);
    EXPECT_EQ(VarGet(JOHTO_VAR_START), DIR_EAST);
    EXPECT_EQ(ctx.scriptPtr, script + 4);
    EXPECT(JohtoSave_Validate(&gSaveblock1.johto));

#if TESTING
    EXPECT(JohtoEvent_SetVariable(JOHTO_VAR_START, 7));
    coordEvent.trigger = JOHTO_VAR_START;
    coordEvent.index = 7;
    EXPECT(FieldControlAvatar_TestShouldTriggerScriptRun(&coordEvent));
    coordEvent.index = 8;
    EXPECT(!FieldControlAvatar_TestShouldTriggerScriptRun(&coordEvent));
#endif
}

TEST("Johto follower native outputs change the destination and seal it")
{
    struct ScriptContext ctx;
    u8 script[2];

    JohtoSave_InitializeCurrent();
    memset(gObjectEvents, 0, sizeof(gObjectEvents));
    memset(gPlayerParty, 0, sizeof(gPlayerParty));
    gFieldEffectArguments[0] = 0;
    PutHalfword(script, JOHTO_VAR_START);
    InitScriptContext(&ctx, NULL, NULL);
    ctx.scriptPtr = script;
    EXPECT(VarSet(JOHTO_VAR_START, 0xBEEF));
    IsFollowerFieldMoveUser(&ctx);
    EXPECT_EQ(VarGet(JOHTO_VAR_START), FALSE);
    EXPECT_EQ(ctx.scriptPtr, script + 2);
    EXPECT(JohtoSave_Validate(&gSaveblock1.johto));

    CreateMon(&gPlayerParty[0], SPECIES_RATTATA, 5, 0, OTID_STRUCT_PLAYER_ID);
    CalculateMonStats(&gPlayerParty[0]);
    gObjectEvents[0].active = TRUE;
    gObjectEvents[0].localId = OBJ_EVENT_ID_FOLLOWER;
    EXPECT_EQ(GetFirstLiveMon(), &gPlayerParty[0]);
    EXPECT_EQ(GetFollowerObject(), &gObjectEvents[0]);
    ctx.scriptPtr = script;
    EXPECT(VarSet(JOHTO_VAR_START, 0xBEEF));
    IsFollowerFieldMoveUser(&ctx);
    EXPECT_EQ(VarGet(JOHTO_VAR_START), TRUE);
    EXPECT_EQ(ctx.scriptPtr, script + 2);
    EXPECT(JohtoSave_Validate(&gSaveblock1.johto));
}
