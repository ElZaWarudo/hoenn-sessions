#include "global.h"
#include "battle_setup.h"
#include "coop/save.h"
#include "coop/identity.h"
#include "event_data.h"
#include "load_save.h"
#include "test/test.h"
#include "world/events.h"
#include "world/event_save.h"
#include "constants/vars.h"
#include "constants/event_bg.h"
#include "constants/flags.h"
#include "constants/johto_events.h"

#if TESTING
extern bool32 FieldControlAvatar_TestShouldTriggerScriptRun(const struct CoordEvent *coordEvent);
#endif

TEST("World event IDs use per-ROM storage without touching special vars")
{
    WorldEventSave_InitializeCurrent();
    EXPECT(VarSet(VAR_0x8000, 0xC0DE));
    EXPECT_EQ(VarGet(VAR_0x8000), 0xC0DE);
    EXPECT_EQ(GetFlagPointer(WORLD_EVENT_FLAG_START), NULL);
    EXPECT_EQ(GetVarPointer(WORLD_EVENT_VAR_START), NULL);
    EXPECT_EQ(GetVarPointer(WORLD_EVENT_TRAINER_START), NULL);
    EXPECT_EQ(GetVarPointer(VARS_END + 1), NULL);
    EXPECT_EQ(GetFlagPointer(FLAGS_COUNT), NULL);
    EXPECT(!VarSet(WORLD_EVENT_TRAINER_START, 9));
    EXPECT_EQ(VarGetIfExist(WORLD_EVENT_TRAINER_START), 65535);
    FlagSet(WORLD_EVENT_FLAG_START);
    FlagSet(WORLD_EVENT_FLAG_END);
    EXPECT(FlagGet(WORLD_EVENT_FLAG_START));
    EXPECT(FlagGet(WORLD_EVENT_FLAG_END));
    FlagToggle(WORLD_EVENT_FLAG_END);
    EXPECT(!FlagGet(WORLD_EVENT_FLAG_END));
    FlagClear(WORLD_EVENT_FLAG_START);
    EXPECT(!FlagGet(WORLD_EVENT_FLAG_START));
    EXPECT(VarSet(WORLD_EVENT_VAR_START, 0x1234));
    EXPECT(VarSet(WORLD_EVENT_VAR_END, 0xBEEF));
    EXPECT_EQ(VarGet(WORLD_EVENT_VAR_START), 0x1234);
    EXPECT_EQ(VarGetIfExist(WORLD_EVENT_VAR_END), 0xBEEF);
    EXPECT_EQ(VarGet(VAR_0x8000), 0xC0DE);
    EXPECT(WorldEventSave_Validate(&gSaveblock1.world_event));
}

TEST("World trainer defeat uses regional bits and rejects corrupt saves")
{
    bool8 defeated = TRUE;

    CoopSave_InitializeCurrent();
    CoopSave_ResetRuntimeState();
    WorldEventSave_InitializeCurrent();
    EXPECT(!CoopSave_IsOnlineEnabled());
    EXPECT(!WorldEvent_GetTrainerDefeated(WORLD_EVENT_TRAINER_START));
    EXPECT_EQ(CoopIdentity_GetTrainerDefeated(WORLD_EVENT_TRAINER_START, &defeated), COOP_IDENTITY_ACCESS_HANDLED);
    EXPECT(!defeated);
    EXPECT(!HasTrainerBeenFought(WORLD_EVENT_TRAINER_START));
    SetTrainerFlag(WORLD_EVENT_TRAINER_START);
    EXPECT(HasTrainerBeenFought(WORLD_EVENT_TRAINER_START));
    ToggleTrainerFlag(WORLD_EVENT_TRAINER_START);
    EXPECT(!HasTrainerBeenFought(WORLD_EVENT_TRAINER_START));
    SetTrainerFlag(WORLD_EVENT_TRAINER_END);
    EXPECT(HasTrainerBeenFought(WORLD_EVENT_TRAINER_END));
    ClearTrainerFlag(WORLD_EVENT_TRAINER_END);
    EXPECT(!HasTrainerBeenFought(WORLD_EVENT_TRAINER_END));
    gSaveblock1.world_event.flag_bits[0] ^= 1;
    SetTrainerFlag(WORLD_EVENT_TRAINER_START);
    EXPECT(!HasTrainerBeenFought(WORLD_EVENT_TRAINER_START));
}

TEST("World coordinate event reads persistent variables and flags")
{
    struct CoordEvent coordEvent = {0};

    WorldEventSave_InitializeCurrent();
    coordEvent.trigger = WORLD_EVENT_VAR_START;
    coordEvent.index = 7;
    EXPECT(VarSet(WORLD_EVENT_VAR_START, 7));
    EXPECT(FieldControlAvatar_TestShouldTriggerScriptRun(&coordEvent));
    coordEvent.index = 8;
    EXPECT(!FieldControlAvatar_TestShouldTriggerScriptRun(&coordEvent));
    coordEvent.trigger = WORLD_EVENT_FLAG_START;
    coordEvent.index = TRUE;
    FlagSet(WORLD_EVENT_FLAG_START);
    EXPECT(FieldControlAvatar_TestShouldTriggerScriptRun(&coordEvent));
    FlagClear(WORLD_EVENT_FLAG_START);
    EXPECT(!FieldControlAvatar_TestShouldTriggerScriptRun(&coordEvent));
}

TEST("Hidden item marker decodes regional and legacy flags without touching item fields")
{
    struct BgEvent event = {0};

    event.kind = BG_EVENT_HIDDEN_ITEM;
    event.bgUnion.hiddenItem.item = 123;
    event.bgUnion.hiddenItem.quantity = 2;
    event.bgUnion.hiddenItem.hiddenItemId = 3;
    EXPECT_EQ(WorldEvent_GetHiddenItemFlag(&event), FLAG_HIDDEN_ITEMS_START + 3);
    event.bgUnion.hiddenItem.hiddenItemId = WORLD_EVENT_HIDDEN_ITEM_JOHTO_MARKER | 767;
    EXPECT_EQ(WorldEvent_GetHiddenItemFlag(&event), JOHTO_FLAG_END);
    event.bgUnion.hiddenItem.hiddenItemId = WORLD_EVENT_HIDDEN_ITEM_JOHTO_MARKER | 768;
    EXPECT_EQ(WorldEvent_GetHiddenItemFlag(&event), 0);
    event.bgUnion.hiddenItem.hiddenItemId = WORLD_EVENT_HIDDEN_ITEM_MARKER | 4095;
    EXPECT_EQ(WorldEvent_GetHiddenItemFlag(&event), WORLD_EVENT_FLAG_END);
    EXPECT(event.bgUnion.hiddenItem.item == 123);
    EXPECT(event.bgUnion.hiddenItem.quantity == 2);
    event.kind = BG_EVENT_SECRET_BASE;
    EXPECT_EQ(WorldEvent_GetHiddenItemFlag(&event), 0);
}
