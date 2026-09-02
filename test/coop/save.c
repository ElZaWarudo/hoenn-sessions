#include <limits.h>

#include "global.h"
#include "coop/net_bridge.h"
#include "coop/progress.h"
#include "coop/save.h"
#include "gba/flash_internal.h"
#include "load_save.h"
#include "save.h"
#include "test/test.h"

static EWRAM_DATA struct CoopSaveV1 sSaveSnapshot;

static u16 FailSaveSectorProgram(u16 sector, u8 *data)
{
    (void)sector;
    (void)data;
    return 1;
}

static void ExpectGenerationPayload(const struct CoopBridgeMessage *message, u32 generation)
{
    EXPECT_EQ(message->length, sizeof(generation));
    EXPECT_EQ(message->payload[0], generation & 0xFF);
    EXPECT_EQ(message->payload[1], (generation >> 8) & 0xFF);
    EXPECT_EQ(message->payload[2], (generation >> 16) & 0xFF);
    EXPECT_EQ(message->payload[3], generation >> 24);
}

TEST("Cloud Coop saved progress and descriptor layouts are byte exact")
{
    EXPECT_EQ(sizeof(struct CoopSaveRegionalProgress), 8);
    EXPECT_EQ(sizeof(struct CoopSaveV1), 672);
    EXPECT_EQ(offsetof(struct CoopSaveV1, save_generation), 28);
    EXPECT_EQ(offsetof(struct CoopSaveV1, regional_progress), 36);
    EXPECT_EQ(offsetof(struct CoopSaveV1, trainer_bits), 68);
    EXPECT_EQ(offsetof(struct CoopSaveV1, event_bits), 324);
    EXPECT_EQ(offsetof(struct CoopSaveV1, fly_bits), 580);
    EXPECT_EQ(offsetof(struct CoopSaveV1, gym_bits), 596);
    EXPECT_EQ(offsetof(struct CoopSaveV1, crc32), 668);
    EXPECT_EQ(offsetof(struct SaveBlock3, coop), 4);
    EXPECT_EQ(sizeof(struct SaveBlock3), 676);
    EXPECT_LE(sizeof(struct SaveBlock3), SAVE_BLOCK_3_CHUNK_SIZE * 6);

    EXPECT_EQ(sizeof(gCoopSaveSchemaDescriptor), 64);
    EXPECT_EQ(gCoopSaveSchemaDescriptor.descriptor_magic, COOP_SAVE_DESCRIPTOR_MAGIC);
    EXPECT_EQ(gCoopSaveSchemaDescriptor.descriptor_version, COOP_SAVE_DESCRIPTOR_VERSION);
    EXPECT_EQ(gCoopSaveSchemaDescriptor.save_magic, COOP_SAVE_MAGIC);
    EXPECT_EQ(gCoopSaveSchemaDescriptor.save_schema_version, COOP_SAVE_SCHEMA_VERSION);
    EXPECT_EQ(gCoopSaveSchemaDescriptor.save_struct_size, sizeof(struct CoopSaveV1));
    EXPECT_EQ(gCoopSaveSchemaDescriptor.save_block3_offset, 4);
    EXPECT_EQ(gCoopSaveSchemaDescriptor.generation_offset, 28);
    EXPECT_EQ(gCoopSaveSchemaDescriptor.crc32_offset, 668);
    EXPECT_EQ(gCoopSaveSchemaDescriptor.sector_data_size, SECTOR_DATA_SIZE);
    EXPECT_EQ(gCoopSaveSchemaDescriptor.save_block3_chunk_size, SAVE_BLOCK_3_CHUNK_SIZE);
    EXPECT_EQ(gCoopSaveSchemaDescriptor.sector_size, SECTOR_SIZE);
    EXPECT_EQ(gCoopSaveSchemaDescriptor.sectors_per_slot, NUM_SECTORS_PER_SLOT);
    EXPECT_EQ(gCoopSaveSchemaDescriptor.save_slot_count, NUM_SAVE_SLOTS);
    EXPECT_EQ(gCoopSaveSchemaDescriptor.registry_version, COOP_IDENTITY_REGISTRY_VERSION);
}

TEST("Cloud Coop save CRC32 matches the canonical check vector")
{
    static const char sCheckVector[] = "123456789";

    EXPECT_EQ(CoopSave_Crc32(sCheckVector, sizeof(sCheckVector) - 1), 0xCBF43926u);
    EXPECT_EQ(CoopSave_Crc32(NULL, 0), 0);
    EXPECT_EQ(CoopSave_Crc32(NULL, 1), 0);
}

TEST("Cloud Coop new save initializes a sealed region-qualified V1 record")
{
    u32 i;

    CoopSave_InitializeCurrent();

    EXPECT(CoopSave_Validate(&gSaveBlock3Ptr->coop));
    EXPECT(CoopSave_IsOnlineEnabled());
    EXPECT_EQ(CoopSave_GetGeneration(), 0);
    EXPECT_EQ(gSaveBlock3Ptr->coop.magic, COOP_SAVE_MAGIC);
    EXPECT_EQ(gSaveBlock3Ptr->coop.schema_version, COOP_SAVE_SCHEMA_VERSION);
    EXPECT_EQ(gSaveBlock3Ptr->coop.struct_size, sizeof(struct CoopSaveV1));
    EXPECT_EQ(gSaveBlock3Ptr->coop.registry_version, COOP_IDENTITY_REGISTRY_VERSION);
    for (i = 0; i < COOP_PROGRESS_REGION_COUNT; i++)
    {
        EXPECT_EQ(gSaveBlock3Ptr->coop.regional_progress[i].region, i + 1);
        EXPECT_EQ(gSaveBlock3Ptr->coop.regional_progress[i].reserved, 0);
        EXPECT_EQ(gSaveBlock3Ptr->coop.regional_progress[i].badge_mask, 0);
        EXPECT_EQ(gSaveBlock3Ptr->coop.regional_progress[i].story_checkpoint, 0);
    }
    EXPECT_EQ(gSaveBlock3Ptr->coop.crc32,
              CoopSave_CalculateCrc(&gSaveBlock3Ptr->coop));
}

TEST("Cloud Coop zeroed and erased legacy records initialize fail closed")
{
    gCoopProgress.regions[0].badge_mask = 0xFF;
    memset(&gSaveBlock3Ptr->coop, 0, sizeof(gSaveBlock3Ptr->coop));
    EXPECT_EQ(CoopSave_Load(), COOP_SAVE_LOAD_INITIALIZED_LEGACY);
    EXPECT(CoopSave_Validate(&gSaveBlock3Ptr->coop));
    EXPECT_EQ(gSaveBlock3Ptr->coop.status_flags, COOP_SAVE_STATUS_MIGRATION_AMBIGUOUS);
    EXPECT(!CoopSave_IsOnlineEnabled());
    EXPECT_EQ(gCoopProgress.regions[0].badge_mask, 0);

    gCoopProgress.regions[1].story_checkpoint = 77;
    memset(&gSaveBlock3Ptr->coop, 0xFF, sizeof(gSaveBlock3Ptr->coop));
    EXPECT_EQ(CoopSave_Load(), COOP_SAVE_LOAD_INITIALIZED_LEGACY);
    EXPECT(CoopSave_Validate(&gSaveBlock3Ptr->coop));
    EXPECT_EQ(gSaveBlock3Ptr->coop.status_flags, COOP_SAVE_STATUS_MIGRATION_AMBIGUOUS);
    EXPECT(!CoopSave_IsOnlineEnabled());
    EXPECT_EQ(gCoopProgress.regions[1].story_checkpoint, 0);
}

TEST("Cloud Coop valid save reloads independent regional progress")
{
    CoopSave_InitializeCurrent();
    gSaveBlock3Ptr->coop.regional_progress[0].badge_mask = 0x81;
    gSaveBlock3Ptr->coop.regional_progress[0].story_checkpoint = 11;
    gSaveBlock3Ptr->coop.regional_progress[1].badge_mask = 0x03;
    gSaveBlock3Ptr->coop.regional_progress[1].story_checkpoint = 22;
    gSaveBlock3Ptr->coop.regional_progress[2].badge_mask = 0x04;
    gSaveBlock3Ptr->coop.regional_progress[2].story_checkpoint = 33;
    gSaveBlock3Ptr->coop.regional_progress[3].badge_mask = 0;
    gSaveBlock3Ptr->coop.regional_progress[3].story_checkpoint = 44;
    EXPECT(CoopSave_Seal(&gSaveBlock3Ptr->coop));

    CoopProgress_Init(&gCoopProgress);
    EXPECT_EQ(CoopSave_Load(), COOP_SAVE_LOAD_READY);
    EXPECT_EQ(gCoopProgress.regions[0].badge_mask, 0x81);
    EXPECT_EQ(gCoopProgress.regions[0].story_checkpoint, 11);
    EXPECT_EQ(gCoopProgress.regions[1].badge_mask, 0x03);
    EXPECT_EQ(gCoopProgress.regions[1].story_checkpoint, 22);
    EXPECT_EQ(gCoopProgress.regions[2].badge_mask, 0x04);
    EXPECT_EQ(gCoopProgress.regions[2].story_checkpoint, 33);
    EXPECT_EQ(gCoopProgress.regions[3].badge_mask, 0);
    EXPECT_EQ(gCoopProgress.regions[3].story_checkpoint, 44);
}

TEST("Cloud Coop prepare synchronizes progress and advances sealed generation")
{
    CoopSave_InitializeCurrent();
    gCoopProgress.regions[0].badge_mask = 0x05;
    gCoopProgress.regions[0].story_checkpoint = 1234;
    gCoopProgress.regions[3].badge_mask = 0;
    gCoopProgress.regions[3].story_checkpoint = 5678;

    EXPECT(CoopSave_PrepareForWrite());
    EXPECT(CoopSave_WasPreparedForWrite());
    EXPECT_EQ(CoopSave_GetGeneration(), 1);
    EXPECT_EQ(gSaveBlock3Ptr->coop.regional_progress[0].badge_mask, 0x05);
    EXPECT_EQ(gSaveBlock3Ptr->coop.regional_progress[0].story_checkpoint, 1234);
    EXPECT_EQ(gSaveBlock3Ptr->coop.regional_progress[3].badge_mask, 0);
    EXPECT_EQ(gSaveBlock3Ptr->coop.regional_progress[3].story_checkpoint, 5678);
    EXPECT(CoopSave_Validate(&gSaveBlock3Ptr->coop));

    gSaveBlock3Ptr->coop.save_generation = UINT_MAX;
    EXPECT(CoopSave_Seal(&gSaveBlock3Ptr->coop));
    EXPECT(CoopSave_PrepareForWrite());
    EXPECT(CoopSave_WasPreparedForWrite());
    EXPECT_EQ(CoopSave_GetGeneration(), 0);
    EXPECT(CoopSave_Validate(&gSaveBlock3Ptr->coop));
}

TEST("Cloud Coop corrupt and future records stay untouched and offline")
{
    CoopSave_InitializeCurrent();
    CoopProgress_Init(&gCoopProgress);
    gCoopProgress.regions[0].badge_mask = 0x03;
    gSaveBlock3Ptr->coop.trainer_bits[7] ^= 0x40;
    sSaveSnapshot = gSaveBlock3Ptr->coop;
    EXPECT_EQ(CoopSave_Load(), COOP_SAVE_LOAD_CORRUPT);
    EXPECT(!CoopSave_IsOnlineEnabled());
    EXPECT(!CoopSave_PrepareForWrite());
    EXPECT(!CoopSave_WasPreparedForWrite());
    EXPECT_EQ(gCoopProgress.regions[0].badge_mask, 0);
    EXPECT_EQ(memcmp(&sSaveSnapshot, &gSaveBlock3Ptr->coop, sizeof(sSaveSnapshot)), 0);

    CoopSave_InitializeCurrent();
    CoopProgress_Init(&gCoopProgress);
    gCoopProgress.regions[1].story_checkpoint = 77;
    gSaveBlock3Ptr->coop.schema_version = COOP_SAVE_SCHEMA_VERSION + 1;
    sSaveSnapshot = gSaveBlock3Ptr->coop;
    EXPECT_EQ(CoopSave_Load(), COOP_SAVE_LOAD_INCOMPATIBLE);
    EXPECT(!CoopSave_IsOnlineEnabled());
    EXPECT(!CoopSave_PrepareForWrite());
    EXPECT_EQ(gCoopProgress.regions[1].story_checkpoint, 0);
    EXPECT_EQ(memcmp(&sSaveSnapshot, &gSaveBlock3Ptr->coop, sizeof(sSaveSnapshot)), 0);
}

TEST("Cloud Coop rejects noncanonical saved bodies even with a matching CRC")
{
    CoopSave_InitializeCurrent();
    gSaveBlock3Ptr->coop.status_flags = COOP_SAVE_STATUS_KNOWN_MASK << 1;
    gSaveBlock3Ptr->coop.crc32 = CoopSave_CalculateCrc(&gSaveBlock3Ptr->coop);
    EXPECT(!CoopSave_Validate(&gSaveBlock3Ptr->coop));
    EXPECT_EQ(CoopSave_Load(), COOP_SAVE_LOAD_CORRUPT);

    CoopSave_InitializeCurrent();
    gSaveBlock3Ptr->coop.regional_progress[0].region = COOP_REGION_KANTO;
    gSaveBlock3Ptr->coop.crc32 = CoopSave_CalculateCrc(&gSaveBlock3Ptr->coop);
    EXPECT(!CoopSave_Validate(&gSaveBlock3Ptr->coop));

    CoopSave_InitializeCurrent();
    gSaveBlock3Ptr->coop.regional_progress[0].reserved = 1;
    gSaveBlock3Ptr->coop.crc32 = CoopSave_CalculateCrc(&gSaveBlock3Ptr->coop);
    EXPECT(!CoopSave_Validate(&gSaveBlock3Ptr->coop));

    CoopSave_InitializeCurrent();
    gSaveBlock3Ptr->coop.regional_progress[0].badge_mask = 0x100;
    gSaveBlock3Ptr->coop.crc32 = CoopSave_CalculateCrc(&gSaveBlock3Ptr->coop);
    EXPECT(!CoopSave_Validate(&gSaveBlock3Ptr->coop));

    CoopSave_InitializeCurrent();
    gSaveBlock3Ptr->coop.reserved[0] = 1;
    gSaveBlock3Ptr->coop.crc32 = CoopSave_CalculateCrc(&gSaveBlock3Ptr->coop);
    EXPECT(!CoopSave_Validate(&gSaveBlock3Ptr->coop));
}

TEST("Cloud Coop rejects identity tails and unassigned regional badges")
{
    CoopSave_InitializeCurrent();

    /* These bits fit the fixed-capacity fields but are not assigned by the
     * generated registry. A forged matching CRC must not make them valid. */
    gSaveBlock3Ptr->coop.trainer_bits[COOP_SAVE_TRAINER_BITS_SIZE - 1] = 0x80;
    gSaveBlock3Ptr->coop.event_bits[COOP_SAVE_EVENT_BITS_SIZE - 1] = 0x80;
    gSaveBlock3Ptr->coop.fly_bits[COOP_SAVE_FLY_BITS_SIZE - 1] = 0x80;
    gSaveBlock3Ptr->coop.gym_bits[COOP_SAVE_GYM_BITS_SIZE - 1] = 0x80;
    gSaveBlock3Ptr->coop.crc32 = CoopSave_CalculateCrc(&gSaveBlock3Ptr->coop);
    EXPECT(!CoopSave_Validate(&gSaveBlock3Ptr->coop));

    CoopSave_InitializeCurrent();
    gSaveBlock3Ptr->coop.regional_progress[3].badge_mask = 1;
    gSaveBlock3Ptr->coop.crc32 = CoopSave_CalculateCrc(&gSaveBlock3Ptr->coop);
    EXPECT(!CoopSave_Validate(&gSaveBlock3Ptr->coop));

    CoopSave_InitializeCurrent();
    gSaveBlock3Ptr->coop.regional_progress[0].badge_mask = 0xFF;
    gSaveBlock3Ptr->coop.regional_progress[1].badge_mask = 0xFF;
    gSaveBlock3Ptr->coop.regional_progress[2].badge_mask = 0xFF;
    gSaveBlock3Ptr->coop.crc32 = CoopSave_CalculateCrc(&gSaveBlock3Ptr->coop);
    EXPECT(CoopSave_Validate(&gSaveBlock3Ptr->coop));
}

TEST("Cloud Coop invalid runtime progress cannot corrupt a sealed record")
{
    CoopSave_InitializeCurrent();
    sSaveSnapshot = gSaveBlock3Ptr->coop;
    gCoopProgress.regions[0].badge_mask = 0x100;

    EXPECT(!CoopSave_PrepareForWrite());
    EXPECT_EQ(memcmp(&sSaveSnapshot, &gSaveBlock3Ptr->coop, sizeof(sSaveSnapshot)), 0);
    EXPECT(CoopSave_Validate(&gSaveBlock3Ptr->coop));
}

TEST("Cloud Coop failed full saves restore the prepared generation for retry")
{
    u16 (*programFlashSector)(u16, u8 *) = ProgramFlashSector;
    bool32 flashMemoryPresent = gFlashMemoryPresent;

    SetSaveBlocksPointers(0);
    CoopSave_InitializeCurrent();
    gSaveBlock3Ptr->coop.save_generation = 41;
    EXPECT(CoopSave_Seal(&gSaveBlock3Ptr->coop));
    Save_ResetSaveCounters();
    ProgramFlashSector = FailSaveSectorProgram;
    gFlashMemoryPresent = TRUE;

    HandleSavingData(SAVE_NORMAL);
    EXPECT_EQ(gSaveBlock3Ptr->coop.save_generation, 41);
    EXPECT(CoopSave_Validate(&gSaveBlock3Ptr->coop));

    /* A second failed attempt must prepare from the same cached record, not
     * from the generation that was only ever attempted in RAM. */
    HandleSavingData(SAVE_NORMAL);
    EXPECT_EQ(gSaveBlock3Ptr->coop.save_generation, 41);
    EXPECT(CoopSave_Validate(&gSaveBlock3Ptr->coop));

    ProgramFlashSector = programFlashSector;
    gFlashMemoryPresent = flashMemoryPresent;
    Save_ResetSaveCounters();
}

TEST("Cloud Coop migration ambiguity preserves play but disables cloud authority")
{
    CoopSave_InitializeCurrent();
    gSaveBlock3Ptr->coop.status_flags = COOP_SAVE_STATUS_MIGRATION_AMBIGUOUS;
    EXPECT(CoopSave_Seal(&gSaveBlock3Ptr->coop));

    EXPECT(CoopSave_Validate(&gSaveBlock3Ptr->coop));
    EXPECT(!CoopSave_IsOnlineEnabled());
    EXPECT_EQ(CoopSave_Load(), COOP_SAVE_LOAD_READY);
    EXPECT(CoopSave_PrepareForWrite());
    EXPECT_EQ(CoopSave_GetGeneration(), 1);
    EXPECT(!CoopSave_IsOnlineEnabled());
}

TEST("Cloud Coop typed identity bits reject unassigned tails and stay sealed")
{
    CoopSave_InitializeCurrent();

    EXPECT(CoopSave_SetTrainerDefeated(COOP_TRAINER_IDENTITY_COUNT - 1, TRUE));
    EXPECT(CoopSave_GetTrainerDefeated(COOP_TRAINER_IDENTITY_COUNT - 1));
    EXPECT(!CoopSave_SetTrainerDefeated(COOP_TRAINER_IDENTITY_COUNT, TRUE));
    EXPECT(!CoopSave_GetTrainerDefeated(COOP_TRAINER_IDENTITY_CAPACITY - 1));
    EXPECT(CoopSave_SetTrainerDefeated(COOP_TRAINER_IDENTITY_COUNT - 1, FALSE));
    EXPECT(!CoopSave_GetTrainerDefeated(COOP_TRAINER_IDENTITY_COUNT - 1));

    EXPECT(CoopSave_SetEventCompleted(COOP_EVENT_IDENTITY_COUNT - 1, TRUE));
    EXPECT(CoopSave_GetEventCompleted(COOP_EVENT_IDENTITY_COUNT - 1));
    EXPECT(!CoopSave_SetEventCompleted(COOP_EVENT_IDENTITY_COUNT, TRUE));
    EXPECT(!CoopSave_GetEventCompleted(COOP_EVENT_IDENTITY_CAPACITY - 1));
    EXPECT(CoopSave_SetFlyPointUnlocked(COOP_FLY_IDENTITY_COUNT - 1, TRUE));
    EXPECT(CoopSave_GetFlyPointUnlocked(COOP_FLY_IDENTITY_COUNT - 1));
    EXPECT(!CoopSave_SetFlyPointUnlocked(COOP_FLY_IDENTITY_COUNT, TRUE));
    EXPECT(!CoopSave_GetFlyPointUnlocked(COOP_FLY_POINT_IDENTITY_CAPACITY - 1));
    EXPECT(CoopSave_SetGymDefeated(COOP_GYM_IDENTITY_COUNT - 1, TRUE));
    EXPECT(CoopSave_GetGymDefeated(COOP_GYM_IDENTITY_COUNT - 1));
    EXPECT(!CoopSave_SetGymDefeated(COOP_GYM_IDENTITY_COUNT, TRUE));
    EXPECT(!CoopSave_GetGymDefeated(COOP_GYM_IDENTITY_CAPACITY - 1));
    EXPECT(CoopSave_Validate(&gSaveBlock3Ptr->coop));
}

TEST("Cloud Coop bridge preserves validated progress and rejects ambiguous authority")
{
    struct CoopBridgeMessage message;

    CoopSave_InitializeCurrent();
    gSaveBlock3Ptr->coop.regional_progress[1].badge_mask = 0x07;
    gSaveBlock3Ptr->coop.regional_progress[1].story_checkpoint = 99;
    EXPECT(CoopSave_Seal(&gSaveBlock3Ptr->coop));
    CoopProgress_Init(&gCoopProgress);
    CoopNetBridge_Init();
    EXPECT_EQ(gCoopProgress.regions[1].badge_mask, 0x07);
    EXPECT_EQ(gCoopProgress.regions[1].story_checkpoint, 99);

    gSaveBlock3Ptr->coop.status_flags = COOP_SAVE_STATUS_MIGRATION_AMBIGUOUS;
    EXPECT(CoopSave_Seal(&gSaveBlock3Ptr->coop));
    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  1,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(!CoopNetBridge_IsCloudMode());
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_OFFLINE);
}

TEST("Cloud Coop legacy load stays offline while a new game can announce readiness")
{
    struct CoopBridgeMessage message;

    memset(&gSaveBlock3Ptr->coop, 0, sizeof(gSaveBlock3Ptr->coop));
    EXPECT_EQ(CoopSave_Load(), COOP_SAVE_LOAD_INITIALIZED_LEGACY);
    CoopNetBridge_Init();
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  1,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    EXPECT(!CoopNetBridge_IsCloudMode());
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_OFFLINE);

    CoopSave_InitializeCurrent();
    EXPECT(CoopSave_IsOnlineEnabled());
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
}

TEST("Cloud Coop save update carries the sealed generation in little endian")
{
    struct CoopBridgeMessage message;

    CoopSave_InitializeCurrent();
    gSaveBlock3Ptr->coop.save_generation = 0x78563411;
    EXPECT(CoopSave_Seal(&gSaveBlock3Ptr->coop));
    EXPECT(CoopSave_PrepareForWrite());
    EXPECT_EQ(CoopSave_GetGeneration(), 0x78563412);
    CoopNetBridge_Init();
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  1,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(CoopNetBridge_RequestCheckpoint(), COOP_CHECKPOINT_REQUEST_STARTED);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_CHECKPOINT_GRANTED,
                                  2,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_ConsumeCheckpointGrant());
    CoopNetBridge_NotifySaveResult(TRUE);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_SAVE_DATA_UPDATED);
    ExpectGenerationPayload(&message, 0x78563412);
}
