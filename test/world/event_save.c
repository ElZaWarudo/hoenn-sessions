#include "global.h"
#include "load_save.h"
#include "malloc.h"
#include "test/test.h"
#include "world/event_save.h"

TEST("World event record keeps full regional flag trainer and variable ranges")
{
    WorldEventSave_InitializeCurrent();
    EXPECT(WorldEventSave_SetFlag(0, TRUE));
    EXPECT(WorldEventSave_SetFlag(4095, TRUE));
    EXPECT(WorldEventSave_GetFlag(0));
    EXPECT(WorldEventSave_GetFlag(4095));
    EXPECT(WorldEventSave_SetTrainerDefeated(4095, TRUE));
    EXPECT(WorldEventSave_GetTrainerDefeated(4095));
    EXPECT(WorldEventSave_SetVariable(255, 0xBEEF));
    EXPECT_EQ(WorldEventSave_GetVariable(255), 0xBEEF);
    EXPECT(WorldEventSave_Validate(&gSaveblock1.world_event));
    EXPECT_EQ(WorldEventSave_Load(), WORLD_EVENT_SAVE_LOAD_READY);
    EXPECT(!WorldEventSave_GetFlag(4096));
    EXPECT(!WorldEventSave_SetFlag(4096, TRUE));
    EXPECT(!WorldEventSave_GetTrainerDefeated(4096));
    EXPECT(!WorldEventSave_SetTrainerDefeated(4096, TRUE));
    EXPECT_EQ(WorldEventSave_GetVariable(256), 0);
    EXPECT(!WorldEventSave_SetVariable(256, 1));
}

TEST("World event record preserves corrupt and incompatible bytes")
{
    struct WorldEventSaveV1 *snapshot = Alloc(sizeof(*snapshot));

    ASSUME(snapshot != NULL);

    WorldEventSave_InitializeCurrent();
    gSaveblock1.world_event.flag_bits[0] ^= 1;
    *snapshot = gSaveblock1.world_event;
    EXPECT_EQ(WorldEventSave_Load(), WORLD_EVENT_SAVE_LOAD_CORRUPT);
    EXPECT(!WorldEventSave_PrepareForWrite());
    EXPECT_EQ(memcmp(snapshot, &gSaveblock1.world_event, sizeof(*snapshot)), 0);

    WorldEventSave_InitializeCurrent();
    gSaveblock1.world_event.schema_version++;
    *snapshot = gSaveblock1.world_event;
    EXPECT_EQ(WorldEventSave_Load(), WORLD_EVENT_SAVE_LOAD_INCOMPATIBLE);
    EXPECT(!WorldEventSave_SetFlag(0, TRUE));
    EXPECT_EQ(memcmp(snapshot, &gSaveblock1.world_event, sizeof(*snapshot)), 0);
    Free(snapshot);
}

TEST("World event empty tail initializes without touching legacy or Johto data")
{
    struct JohtoSaveV1 johto;
    u8 prefixByte;

    JohtoSave_InitializeCurrent();
    johto = gSaveblock1.johto;
    prefixByte = ((u8 *)&gSaveblock1.block)[sizeof(struct SaveBlock1) - 1];
    memset(&gSaveblock1.world_event, 0, sizeof(gSaveblock1.world_event));
    EXPECT_EQ(WorldEventSave_Load(), WORLD_EVENT_SAVE_LOAD_INITIALIZED_EMPTY);
    EXPECT(WorldEventSave_Validate(&gSaveblock1.world_event));
    EXPECT_EQ(memcmp(&johto, &gSaveblock1.johto, sizeof(johto)), 0);
    EXPECT_EQ(((u8 *)&gSaveblock1.block)[sizeof(struct SaveBlock1) - 1], prefixByte);
}
