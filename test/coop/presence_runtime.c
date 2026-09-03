#include "global.h"
#include "coop/net_bridge.h"
#include "coop/presence_runtime.h"
#include "coop/region.h"
#include "coop/save.h"
#include "constants/region_map_sections.h"
#include "event_data.h"
#include "event_scripts.h"
#include "event_object_movement.h"
#include "field_camera.h"
#include "field_control_avatar.h"
#include "fieldmap.h"
#include "load_save.h"
#include "main.h"
#include "overworld.h"
#include "palette.h"
#include "script.h"
#include "sprite.h"
#include "constants/flags.h"
#include "constants/map_types.h"
#include "constants/metatile_labels.h"
#include "constants/metatile_behaviors.h"
#include "metatile_behavior.h"
#include "test/test.h"

_Static_assert(COOP_PRESENCE_RUNTIME_SAMPLE_INTERVAL == 6,
               "presence samples use the ten-Hz six-frame cadence");
_Static_assert(COOP_PRESENCE_RUNTIME_INTERPOLATION_FRAMES == 6,
               "presence interpolation is six frames");
_Static_assert(COOP_PRESENCE_RUNTIME_STALE_FRAMES == 90,
               "presence visuals expire after ninety frames");
_Static_assert(COOP_PRESENCE_RUNTIME_OBJECT_LOCAL_ID == 0xFC,
               "presence owns the reserved remote object local ID");
_Static_assert(COOP_PRESENCE_WORLD_LOCATION_SIZE == 10,
               "presence V1 locations stay exactly ten bytes");
_Static_assert(COOP_PRESENCE_POSE_SIZE == 24,
               "presence V1 poses stay exactly twenty-four bytes");
_Static_assert(COOP_PRESENCE_LOCAL_STATE_SIZE == 28,
               "presence V1 local state stays exactly twenty-eight bytes");
_Static_assert(COOP_PRESENCE_SPAWN_SIZE == 72,
               "presence V1 spawns stay exactly seventy-two bytes");
_Static_assert(COOP_PRESENCE_UPDATE_SIZE == 40,
               "presence V1 updates stay exactly forty bytes");
_Static_assert(COOP_PRESENCE_DESPAWN_SIZE == 16,
               "presence V1 despawns stay exactly sixteen bytes");
_Static_assert(COOP_PRESENCE_INTERACTION_SIZE == 20,
               "presence V1 interactions stay exactly twenty bytes");

extern void CoopPresenceRuntime_RetireRemoteObjectOnReturnToField(u8 objectEventId);
extern bool8 CoopPresenceRuntime_TestTrySetupDiveDownScript(void);

static struct CoopPresenceSpawn RuntimeSpawn(u64 handle, u32 sequence)
{
    struct CoopPresenceSpawn spawn = {
        .handle = handle,
        .server_sequence = sequence,
        .state = {
            .pose = {
                .location = {
                    .region = COOP_REGION_HOENN,
                    .reserved = 0,
                    .map_group = 1,
                    .map_number = 3,
                    .x = 4,
                    .y = 5,
                },
                .elevation = ELEVATION_DEFAULT,
                .direction = COOP_PRESENCE_DIRECTION_SOUTH,
                .client_tick = 1,
                .warp_sequence = 9,
                .movement_mode = COOP_PRESENCE_MOVEMENT_IDLE,
                .animation_id = COOP_PRESENCE_ANIMATION_IDLE,
                .avatar_id = COOP_PRESENCE_AVATAR_BRENDAN,
                .player_state = COOP_PRESENCE_PLAYER_OVERWORLD,
            },
            .source_sequence = 1,
        },
        .username = {
            .length = 3,
            .bytes = "rom",
        },
    };
    return spawn;
}

static EWRAM_DATA struct MapLayout sRuntimeTestMapLayout;
static EWRAM_DATA u16 sRuntimeTestMapData[32 * 32];
static EWRAM_DATA struct MapEvents sRuntimeTestMapEvents;
static EWRAM_DATA struct ObjectEventTemplate sRuntimeTestObjectTemplates[1];
static EWRAM_DATA struct MapConnection sRuntimeTestDiveConnection;
static EWRAM_DATA struct MapConnections sRuntimeTestMapConnections;

extern const struct Tileset gTileset_General;

struct RuntimeFixtureBackup
{
    struct MapHeader map_header;
    struct BackupMapLayout backup_map_layout;
    struct ObjectEventTemplate object_event_template0;
    struct ObjectEvent object_events[OBJECT_EVENTS_COUNT];
    struct Sprite sprites[3];
    struct LinkPlayerObjectEvent link_player_object_events[4];
    struct PlayerAvatar player_avatar;
    struct SaveBlock1 *save_block1;
    struct CoopSaveV1 coop_save;
    struct Coords16 save_position;
    struct WarpData save_location;
    MainCallback callback1;
    MainCallback callback2;
    bool8 palette_fade_active;
    u16 camera_offset_x;
    u16 camera_offset_y;
    u8 selected_object_event;
    u16 special_var_last_talked;
    u16 special_var_facing;
    bool8 field_controls_locked;
    u8 reserved_sprite_palette_count;
    u16 sprite_palette_tags[16];
    u16 sprite_palette_unfaded[16][16];
    u16 sprite_palette_faded[16][16];
    u8 sprite_tile_alloc_bitmap[128];
};

static EWRAM_DATA struct RuntimeFixtureBackup sRuntimeFixtureBackup;

static void BeginRuntimeFixture(struct RuntimeFixtureBackup *backup)
{
    u32 i;

    backup->map_header = gMapHeader;
    backup->backup_map_layout = gBackupMapLayout;
    if (gSaveBlock1Ptr != NULL)
        backup->object_event_template0 = gSaveBlock1Ptr->objectEventTemplates[0];
    for (i = 0; i < OBJECT_EVENTS_COUNT; i++)
        backup->object_events[i] = gObjectEvents[i];
    backup->sprites[0] = gSprites[0];
    backup->sprites[1] = gSprites[1];
    backup->sprites[2] = gSprites[2];
    memcpy(backup->link_player_object_events, gLinkPlayerObjectEvents,
           sizeof(backup->link_player_object_events));
    backup->player_avatar = gPlayerAvatar;
    backup->save_block1 = gSaveBlock1Ptr;
    if (gSaveBlock1Ptr != NULL)
    {
        backup->save_position = gSaveBlock1Ptr->pos;
        backup->save_location = gSaveBlock1Ptr->location;
    }
    if (gSaveBlock3Ptr != NULL)
        backup->coop_save = gSaveBlock3Ptr->coop;
    backup->callback1 = gMain.callback1;
    backup->callback2 = gMain.callback2;
    backup->palette_fade_active = gPaletteFade.active;
    backup->camera_offset_x = gTotalCameraPixelOffsetX;
    backup->camera_offset_y = gTotalCameraPixelOffsetY;
    backup->selected_object_event = gSelectedObjectEvent;
    backup->special_var_last_talked = gSpecialVar_LastTalked;
    backup->special_var_facing = gSpecialVar_Facing;
    backup->field_controls_locked = ArePlayerFieldControlsLocked();
    backup->reserved_sprite_palette_count = gReservedSpritePaletteCount;
    for (i = 0; i < 16; i++)
    {
        u32 j;

        backup->sprite_palette_tags[i] = GetSpritePaletteTagByPaletteNum(i);
        for (j = 0; j < 16; j++)
        {
            backup->sprite_palette_unfaded[i][j] =
                gPlttBufferUnfaded[OBJ_PLTT_ID(i) + j];
            backup->sprite_palette_faded[i][j] =
                gPlttBufferFaded[OBJ_PLTT_ID(i) + j];
        }
    }
    memset(backup->sprite_tile_alloc_bitmap, 0,
           sizeof(backup->sprite_tile_alloc_bitmap));
    for (i = 0; i < sizeof(backup->sprite_tile_alloc_bitmap) * 8; i++)
    {
        if (SpriteTileAllocBitmapOp(i, 2))
            backup->sprite_tile_alloc_bitmap[i / 8] |= (u8)(1 << (i % 8));
    }

    for (i = 0; i < ARRAY_COUNT(sRuntimeTestMapData); i++)
        sRuntimeTestMapData[i] = PACK_ELEVATION(ELEVATION_DEFAULT);
    sRuntimeTestMapLayout = (struct MapLayout){
        .width = 20,
        .height = 20,
        .map = sRuntimeTestMapData,
    };
    gBackupMapLayout.width = 32;
    gBackupMapLayout.height = 32;
    gBackupMapLayout.map = sRuntimeTestMapData;
    gMapHeader.mapLayout = &sRuntimeTestMapLayout;
    gMapHeader.events = NULL;
    gMapHeader.mapScripts = NULL;
    gMapHeader.mapType = MAP_TYPE_TOWN;
    gMapHeader.engineRegion = COOP_MAP_ENGINE_REGION_HOENN;
    gMapHeader.regionMapSectionId = MAPSEC_LITTLEROOT_TOWN;
    gTotalCameraPixelOffsetX = 0;
    gTotalCameraPixelOffsetY = 0;
    gPaletteFade.active = FALSE;
    UnlockPlayerFieldControls();
    FreeAllSpritePalettes();
    for (i = 0; i < sizeof(backup->sprite_tile_alloc_bitmap) * 8; i++)
        SpriteTileAllocBitmapOp(i, 0);
    gMain.callback1 = CB1_Overworld;
    gMain.callback2 = CB2_Overworld;

    for (i = 0; i < OBJECT_EVENTS_COUNT; i++)
        memset(&gObjectEvents[i], 0, sizeof(gObjectEvents[i]));
    memset(&gSprites[0], 0, sizeof(gSprites[0]));
    memset(&gSprites[1], 0, sizeof(gSprites[1]));
    memset(&gSprites[2], 0, sizeof(gSprites[2]));
    gSaveBlock1Ptr = &gSaveblock1.block;
    gSaveBlock1Ptr->location.mapGroup = 1;
    gSaveBlock1Ptr->location.mapNum = 3;
    gSaveBlock1Ptr->pos.x = MAP_OFFSET + 4;
    gSaveBlock1Ptr->pos.y = MAP_OFFSET + 5;

    gObjectEvents[0].active = TRUE;
    gObjectEvents[0].isPlayer = TRUE;
    gObjectEvents[0].localId = LOCALID_PLAYER;
    gObjectEvents[0].mapGroup = 1;
    gObjectEvents[0].mapNum = 3;
    gObjectEvents[0].facingDirection = DIR_SOUTH;
    gObjectEvents[0].currentElevation = ELEVATION_DEFAULT;
    gObjectEvents[0].previousElevation = ELEVATION_DEFAULT;
    gObjectEvents[0].currentCoords.x = MAP_OFFSET + 4;
    gObjectEvents[0].currentCoords.y = MAP_OFFSET + 5;
    gObjectEvents[0].previousCoords = gObjectEvents[0].currentCoords;
    gObjectEvents[0].initialCoords = gObjectEvents[0].currentCoords;
    gObjectEvents[0].spriteId = 0;
    gSprites[0].inUse = TRUE;
    gSprites[0].data[0] = 0;
    gPlayerAvatar.objectEventId = 0;
    gPlayerAvatar.spriteId = 0;
    gPlayerAvatar.flags = PLAYER_AVATAR_FLAG_ON_FOOT | PLAYER_AVATAR_FLAG_CONTROLLABLE;

}

static void EndRuntimeFixture(const struct RuntimeFixtureBackup *backup)
{
    struct SpritePalette palette;
    u32 i;

    CoopPresenceRuntime_TransportLost();
    CoopPresenceRuntime_Init();
    CoopNetBridge_Init();
    gMapHeader = backup->map_header;
    gBackupMapLayout = backup->backup_map_layout;
    for (i = 0; i < OBJECT_EVENTS_COUNT; i++)
        gObjectEvents[i] = backup->object_events[i];
    gSprites[0] = backup->sprites[0];
    gSprites[1] = backup->sprites[1];
    gSprites[2] = backup->sprites[2];
    memcpy(gLinkPlayerObjectEvents, backup->link_player_object_events,
           sizeof(backup->link_player_object_events));
    gPlayerAvatar = backup->player_avatar;
    if (backup->save_block1 != NULL)
    {
        backup->save_block1->pos = backup->save_position;
        backup->save_block1->location = backup->save_location;
        backup->save_block1->objectEventTemplates[0] = backup->object_event_template0;
    }
    if (gSaveBlock3Ptr != NULL)
        gSaveBlock3Ptr->coop = backup->coop_save;
    gSaveBlock1Ptr = backup->save_block1;
    gMain.callback1 = backup->callback1;
    gMain.callback2 = backup->callback2;
    gPaletteFade.active = backup->palette_fade_active;
    gTotalCameraPixelOffsetX = backup->camera_offset_x;
    gTotalCameraPixelOffsetY = backup->camera_offset_y;
    gSelectedObjectEvent = backup->selected_object_event;
    gSpecialVar_LastTalked = backup->special_var_last_talked;
    gSpecialVar_Facing = backup->special_var_facing;
    if (backup->field_controls_locked)
        LockPlayerFieldControls();
    else
        UnlockPlayerFieldControls();

    FreeAllSpritePalettes();
    gReservedSpritePaletteCount = 0;
    for (i = 0; i < 16; i++)
    {
        u32 j;

        if (backup->sprite_palette_tags[i] != TAG_NONE)
        {
            palette.data = backup->sprite_palette_unfaded[i];
            palette.tag = backup->sprite_palette_tags[i];
            LoadSpritePaletteInSlot(&palette, i);
        }
        for (j = 0; j < 16; j++)
        {
            gPlttBufferUnfaded[OBJ_PLTT_ID(i) + j] =
                backup->sprite_palette_unfaded[i][j];
            gPlttBufferFaded[OBJ_PLTT_ID(i) + j] =
                backup->sprite_palette_faded[i][j];
        }
    }
    gReservedSpritePaletteCount = backup->reserved_sprite_palette_count;
    for (i = 0; i < sizeof(backup->sprite_tile_alloc_bitmap) * 8; i++)
    {
        u8 expected = backup->sprite_tile_alloc_bitmap[i / 8]
            & (u8)(1 << (i % 8));
        if ((SpriteTileAllocBitmapOp(i, 2) != 0) != (expected != 0))
            SpriteTileAllocBitmapOp(i, expected != 0);
    }
}

TEST("Cloud Coop presence sequence domains remain wrap-safe")
{
    EXPECT_EQ(CoopPresence_NextSequence(0), 1);
    EXPECT_EQ(CoopPresence_NextSequence(0xFFFFFFFFu), 1);
    EXPECT(CoopPresence_SequenceIsNewer(1, 0xFFFFFFFFu));
    EXPECT(!CoopPresence_SequenceIsNewer(0xFFFFFFFFu, 1));
    EXPECT(CoopPresence_SequenceIsNewer(9, 8));
    EXPECT(!CoopPresence_SequenceIsNewer(8, 9));
}

TEST("Cloud Coop presence runtime keeps the bounded V1 contract")
{
    struct ObjectEvent remote = {0};
    u8 malformed[COOP_PRESENCE_SPAWN_SIZE] = {0};
    u8 spawn_bytes[COOP_PRESENCE_SPAWN_SIZE];
    struct CoopPresenceSpawn spawn;
    u8 i;

    CoopPresenceRuntime_Init();
    EXPECT(!CoopPresenceRuntime_GetLocalState(NULL));
    EXPECT(!CoopPresenceRuntime_QueueBridgeFrame(
        COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_SPAWN,
        malformed, sizeof(malformed)));

    spawn = RuntimeSpawn(1, 1);
    EXPECT(CoopPresence_EncodeSpawn(&spawn, spawn_bytes, sizeof(spawn_bytes)));
    for (i = 0; i < COOP_PRESENCE_RUNTIME_PENDING_CAPACITY; i++)
        EXPECT(CoopPresenceRuntime_QueueBridgeFrame(
            COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_SPAWN,
            spawn_bytes, sizeof(spawn_bytes)));
    EXPECT(!CoopPresenceRuntime_QueueBridgeFrame(
        COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_SPAWN,
        spawn_bytes, sizeof(spawn_bytes)));
    EXPECT(!CoopPresenceReducer_IsActive(CoopPresenceRuntime_GetReducer()));

    remote.localId = COOP_PRESENCE_RUNTIME_OBJECT_LOCAL_ID;
    EXPECT(CoopPresenceRuntime_IsRemoteObject(&remote));
    remote.localId = LOCALID_NONE;
    EXPECT(!CoopPresenceRuntime_IsRemoteObject(&remote));
    CoopPresenceRuntime_Reset();
}

TEST("Cloud Coop presence runtime queues exact lifecycle frames atomically")
{
    struct CoopPresenceSpawn spawn = RuntimeSpawn(2, 7);
    struct CoopPresenceUpdate update = {
        .handle = 2,
        .server_sequence = 8,
        .state = spawn.state,
    };
    struct CoopPresenceDespawn despawn = {
        .handle = 2,
        .server_sequence = 9,
        .reason = COOP_PRESENCE_DESPAWN_DISCONNECTED,
    };
    u8 spawn_bytes[COOP_PRESENCE_SPAWN_SIZE];
    u8 update_bytes[COOP_PRESENCE_UPDATE_SIZE];
    u8 despawn_bytes[COOP_PRESENCE_DESPAWN_SIZE];
    u8 malformed_spawn[COOP_PRESENCE_SPAWN_SIZE];
    u8 malformed_update[COOP_PRESENCE_UPDATE_SIZE];
    u8 malformed_despawn[COOP_PRESENCE_DESPAWN_SIZE];
    u8 i;

    EXPECT(CoopPresence_EncodeSpawn(&spawn, spawn_bytes, sizeof(spawn_bytes)));
    EXPECT(CoopPresence_EncodeUpdate(&update, update_bytes, sizeof(update_bytes)));
    EXPECT(CoopPresence_EncodeDespawn(&despawn, despawn_bytes, sizeof(despawn_bytes)));
    memset(malformed_spawn, 0, sizeof(malformed_spawn));
    memset(malformed_update, 0, sizeof(malformed_update));
    memset(malformed_despawn, 0, sizeof(malformed_despawn));

    CoopPresenceRuntime_Init();
    EXPECT(CoopPresenceRuntime_QueueBridgeFrame(
        COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_SPAWN,
        spawn_bytes, sizeof(spawn_bytes)));
    EXPECT(!CoopPresenceRuntime_QueueBridgeFrame(
        COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_SPAWN,
        malformed_spawn, sizeof(malformed_spawn) - 1));
    EXPECT(CoopPresenceRuntime_QueueBridgeFrame(
        COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_UPDATE,
        update_bytes, sizeof(update_bytes)));
    EXPECT(!CoopPresenceRuntime_QueueBridgeFrame(
        COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_UPDATE,
        malformed_update, sizeof(malformed_update)));
    EXPECT(CoopPresenceRuntime_QueueBridgeFrame(
        COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_DESPAWN,
        despawn_bytes, sizeof(despawn_bytes)));
    malformed_despawn[COOP_PRESENCE_DESPAWN_RESERVED_OFFSET] = 1;
    EXPECT(!CoopPresenceRuntime_QueueBridgeFrame(
        COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_DESPAWN,
        malformed_despawn, sizeof(malformed_despawn)));

    /* The rejected frames above consume no queue entries: exactly 29 more
     * accepted frames fill the remaining bounded capacity. */
    for (i = 0; i < COOP_PRESENCE_RUNTIME_PENDING_CAPACITY - 3; i++)
        EXPECT(CoopPresenceRuntime_QueueBridgeFrame(
            COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_SPAWN,
            spawn_bytes, sizeof(spawn_bytes)));
    EXPECT(!CoopPresenceRuntime_QueueBridgeFrame(
        COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_SPAWN,
        spawn_bytes, sizeof(spawn_bytes)));
    EXPECT(!CoopPresenceReducer_IsActive(CoopPresenceRuntime_GetReducer()));
    CoopPresenceRuntime_TransportLost();
    EXPECT(!CoopPresenceReducer_IsActive(CoopPresenceRuntime_GetReducer()));
}

TEST("Cloud Coop presence runtime keeps publication gated by a ready epoch")
{
    struct CoopPresenceLocalState state;

    CoopPresenceRuntime_Init();
    EXPECT(!CoopPresenceRuntime_GetLocalState(&state));
    CoopPresenceRuntime_SetSessionEpoch(23);
    /* The test runner has no bound on-foot avatar, so even a nonzero epoch
     * cannot manufacture an OVERWORLD state. */
    EXPECT(!CoopPresenceRuntime_GetLocalState(&state));
    CoopPresenceRuntime_TransportLost();
    EXPECT(!CoopPresenceRuntime_GetLocalState(&state));
}

TEST("Cloud Coop presence runtime executes hidden stale and warp lifecycle paths")
{
    struct CoopPresenceSpawn spawn = RuntimeSpawn(3, 1);
    struct CoopPresenceUpdate update;
    struct CoopPresenceLocalState state;
    u8 spawn_bytes[COOP_PRESENCE_SPAWN_SIZE];
    u8 update_bytes[COOP_PRESENCE_UPDATE_SIZE];
    u8 local_bytes[COOP_PRESENCE_LOCAL_STATE_SIZE];
    u32 frame;

    BeginRuntimeFixture(&sRuntimeFixtureBackup);
    CoopSave_InitializeCurrent();
    CoopNetBridge_Init();
    CoopPresenceRuntime_SetSessionEpoch(23);

    spawn.state.pose.location.y = 6;
    spawn.state.pose.warp_sequence = 1;
    EXPECT(CoopPresenceRuntime_GetLocalState(&state));
    EXPECT_EQ(state.pose.player_state, COOP_PRESENCE_PLAYER_OVERWORLD);
    EXPECT(CoopPresence_EncodeSpawn(&spawn, spawn_bytes, sizeof(spawn_bytes)));
    EXPECT(CoopPresenceRuntime_QueueBridgeFrame(
        COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_SPAWN,
        spawn_bytes, sizeof(spawn_bytes)));
    CoopPresenceRuntime_Update();
    EXPECT(CoopPresenceReducer_IsVisible(CoopPresenceRuntime_GetReducer()));
    EXPECT(gObjectEvents[1].active);
    EXPECT_EQ(CoopPresenceRuntime_TryInteract(),
              COOP_PRESENCE_INTERACTION_CONSUMED_NO_LOCK);

    /* Publication changes to an explicit HIDDEN state when the avatar loses
     * controllability; the renderer and interaction gate follow that state. */
    gPlayerAvatar.flags = PLAYER_AVATAR_FLAG_ON_FOOT;
    EXPECT(CoopPresenceRuntime_GetLocalState(&state));
    EXPECT_EQ(state.pose.player_state, COOP_PRESENCE_PLAYER_HIDDEN);
    EXPECT(CoopPresenceRuntime_EncodeLocalState(
        local_bytes, sizeof(local_bytes)));
    EXPECT(CoopPresence_DecodeLocalState(
        local_bytes, sizeof(local_bytes), &state));
    EXPECT_EQ(state.pose.player_state, COOP_PRESENCE_PLAYER_HIDDEN);
    CoopPresenceRuntime_Update();
    EXPECT(!gObjectEvents[1].active);
    EXPECT_EQ(CoopPresenceRuntime_TryInteract(), COOP_PRESENCE_INTERACTION_NONE);

    /* A fresh lifecycle update recreates the renderer and resets the stale
     * clock before the complete ninety-frame expiry window. */
    gPlayerAvatar.flags = PLAYER_AVATAR_FLAG_ON_FOOT
        | PLAYER_AVATAR_FLAG_CONTROLLABLE;
    update = (struct CoopPresenceUpdate){
        .handle = spawn.handle,
        .server_sequence = 2,
        .state = spawn.state,
    };
    EXPECT(CoopPresence_EncodeUpdate(&update, update_bytes, sizeof(update_bytes)));
    EXPECT(CoopPresenceRuntime_QueueBridgeFrame(
        COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_UPDATE,
        update_bytes, sizeof(update_bytes)));
    CoopPresenceRuntime_Update();
    EXPECT(gObjectEvents[1].active);
    EXPECT(gSprites[gObjectEvents[1].spriteId].inUse);
    for (frame = 0; frame < COOP_PRESENCE_RUNTIME_STALE_FRAMES; frame++)
    {
        CoopPresenceRuntime_AdvanceFrame();
        CoopPresenceRuntime_Update();
    }
    EXPECT(!gObjectEvents[1].active);
    EXPECT_EQ(CoopPresenceRuntime_TryInteract(), COOP_PRESENCE_INTERACTION_NONE);

    /* Stale teardown is recoverable on the next accepted lifecycle update. */
    update.server_sequence = 3;
    EXPECT(CoopPresence_EncodeUpdate(&update, update_bytes, sizeof(update_bytes)));
    EXPECT(CoopPresenceRuntime_QueueBridgeFrame(
        COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_UPDATE,
        update_bytes, sizeof(update_bytes)));
    CoopPresenceRuntime_Update();
    EXPECT(gObjectEvents[1].active);
    EXPECT(gSprites[gObjectEvents[1].spriteId].inUse);

    /* A committed warp tears down the renderer and reducer atomically, then
     * a spawn in the new warp domain recreates it from scratch. */
    CoopPresenceRuntime_OnWarpCommit();
    EXPECT(!gObjectEvents[1].active);
    EXPECT(!CoopPresenceReducer_IsActive(CoopPresenceRuntime_GetReducer()));
    EXPECT_EQ(CoopPresenceRuntime_TryInteract(), COOP_PRESENCE_INTERACTION_NONE);
    spawn.server_sequence = 4;
    spawn.state.pose.warp_sequence = 2;
    EXPECT(CoopPresence_EncodeSpawn(&spawn, spawn_bytes, sizeof(spawn_bytes)));
    EXPECT(CoopPresenceRuntime_QueueBridgeFrame(
        COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_SPAWN,
        spawn_bytes, sizeof(spawn_bytes)));
    CoopPresenceRuntime_Update();
    EXPECT(CoopPresenceReducer_IsVisible(CoopPresenceRuntime_GetReducer()));
    EXPECT(gObjectEvents[1].active);
    EXPECT(gSprites[gObjectEvents[1].spriteId].inUse);

    EndRuntimeFixture(&sRuntimeFixtureBackup);
}

TEST("Cloud Coop presence runtime directly covers renderer interpolation animation and return")
{
    struct CoopPresenceSpawn spawn = RuntimeSpawn(4, 1);
    struct CoopPresenceUpdate update;
    const struct ObjectEventGraphicsInfo *graphics;
    struct Sprite *remote_sprite;
    u8 spawn_bytes[COOP_PRESENCE_SPAWN_SIZE];
    u8 update_bytes[COOP_PRESENCE_UPDATE_SIZE];
    u8 interaction_bytes[COOP_PRESENCE_INTERACTION_SIZE];
    struct CoopPresenceLocalState local_state;
    struct CoopPresenceLocalContext local_context;
    s16 expected_x;
    s16 expected_y;
    s16 arrived_x;
    s16 arrived_y;
    s16 interpolation_start_x;
    s16 interpolation_target_x;
    u8 recreated_sprite_id = 1;
    u32 i;

    BeginRuntimeFixture(&sRuntimeFixtureBackup);
    CoopSave_InitializeCurrent();
    CoopNetBridge_Init();
    CoopPresenceRuntime_SetSessionEpoch(23);

    spawn.state.pose.location.y = 6;
    spawn.state.pose.warp_sequence = 1;
    EXPECT(CoopPresence_EncodeSpawn(&spawn, spawn_bytes, sizeof(spawn_bytes)));
    EXPECT(CoopPresenceRuntime_QueueBridgeFrame(
        COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_SPAWN,
        spawn_bytes, sizeof(spawn_bytes)));
    CoopPresenceRuntime_Update();
    EXPECT(gObjectEvents[1].active);
    EXPECT(CoopPresenceRuntime_IsRemoteObject(&gObjectEvents[1]));
    EXPECT(CoopPresenceReducer_IsActive(CoopPresenceRuntime_GetReducer()));
    EXPECT(CoopPresenceReducer_IsVisible(CoopPresenceRuntime_GetReducer()));
    EXPECT_EQ(gObjectEvents[1].spriteId, 1);
    EXPECT_EQ(gObjectEvents[1].mapGroup, 1);
    EXPECT_EQ(gObjectEvents[1].mapNum, 3);
    EXPECT_EQ(gObjectEvents[1].currentCoords.x, MAP_OFFSET + 4);
    EXPECT_EQ(gObjectEvents[1].currentCoords.y, MAP_OFFSET + 6);
    EXPECT(!gObjectEvents[1].isPlayer);
    EXPECT(!gObjectEvents[1].invisible);
    EXPECT(!gObjectEvents[1].offScreen);
    EXPECT(gSprites[1].inUse);
    EXPECT_EQ(gSprites[1].data[0], 1);
    EXPECT(CoopPresenceRuntime_GetLocalState(&local_state));
    local_context = (struct CoopPresenceLocalContext){
        .session_epoch = 23,
        .location = local_state.pose.location,
        .elevation = local_state.pose.elevation,
        .direction = local_state.pose.direction,
        .warp_sequence = local_state.pose.warp_sequence,
    };
    EXPECT(CoopPresence_EncodeInteraction(
        CoopPresenceRuntime_GetReducer(), &local_context,
        interaction_bytes, sizeof(interaction_bytes)));

    /* The renderer is now a proven visible owner, so interaction admission is
     * executable rather than merely a reducer visibility assertion. */
    EXPECT_EQ(CoopPresenceRuntime_TryInteract(),
              COOP_PRESENCE_INTERACTION_CONSUMED_NO_LOCK);
    remote_sprite = &gSprites[gObjectEvents[1].spriteId];
    remote_sprite->animCmdIndex = 1;
    CoopPresenceRuntime_Update();
    EXPECT_EQ(remote_sprite->animCmdIndex, 1);
    EXPECT_EQ(remote_sprite->x2, 0);
    EXPECT_EQ(remote_sprite->y2, 0);
    interpolation_start_x = remote_sprite->x;

    /* A one-tile retarget is interpolated over exactly six runtime updates. */
    update = (struct CoopPresenceUpdate){
        .handle = spawn.handle,
        .server_sequence = 2,
        .state = spawn.state,
    };
    update.state.pose.location.x = 5;
    SetSpritePosToMapCoords(MAP_OFFSET + 5, MAP_OFFSET + 6,
                            &interpolation_target_x, &expected_y);
    interpolation_target_x += 8;
    EXPECT(CoopPresence_EncodeUpdate(&update, update_bytes, sizeof(update_bytes)));
    EXPECT(CoopPresenceRuntime_QueueBridgeFrame(
        COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_UPDATE,
        update_bytes, sizeof(update_bytes)));
    CoopPresenceRuntime_Update();
    remote_sprite = &gSprites[gObjectEvents[1].spriteId];
    EXPECT(remote_sprite->x > interpolation_start_x);
    EXPECT(remote_sprite->x < interpolation_target_x);
    EXPECT_EQ(remote_sprite->x2, 0);
    EXPECT_EQ(remote_sprite->y2, 0);
    for (i = 1; i < COOP_PRESENCE_RUNTIME_INTERPOLATION_FRAMES; i++)
        CoopPresenceRuntime_Update();
    remote_sprite = &gSprites[gObjectEvents[1].spriteId];
    arrived_x = remote_sprite->x;
    arrived_y = remote_sprite->y;
    SetSpritePosToMapCoords(MAP_OFFSET + 5, MAP_OFFSET + 6,
                            &expected_x, &expected_y);
    graphics = GetObjectEventGraphicsInfo(OBJ_EVENT_GFX_BRENDAN_NORMAL);
    expected_x += 8;
    expected_y += 16 - (graphics->height >> 1);
    EXPECT_EQ(arrived_x, expected_x);
    EXPECT_EQ(arrived_y, expected_y);

    /* A discontinuity snaps immediately and does not consume interpolation
     * frames, while preserving the zero sprite offsets invariant. */
    update.server_sequence = 3;
    update.state.pose.location.x = 10;
    EXPECT(CoopPresence_EncodeUpdate(&update, update_bytes, sizeof(update_bytes)));
    EXPECT(CoopPresenceRuntime_QueueBridgeFrame(
        COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_UPDATE,
        update_bytes, sizeof(update_bytes)));
    CoopPresenceRuntime_Update();
    remote_sprite = &gSprites[gObjectEvents[1].spriteId];
    SetSpritePosToMapCoords(MAP_OFFSET + 10, MAP_OFFSET + 6,
                            &expected_x, &expected_y);
    expected_x += 8;
    expected_y += 16 - (graphics->height >> 1);
    EXPECT_EQ(remote_sprite->x, expected_x);
    EXPECT_EQ(remote_sprite->y, expected_y);
    EXPECT_EQ(remote_sprite->x2, 0);
    EXPECT_EQ(remote_sprite->y2, 0);

    /* Simulate the sprite-table reset that precedes a normal return to field.
     * Keep an independent local object/sprite in the neighboring slot: the
     * reserved remote is retired and recreated without evicting that foreign
     * owner.  The narrow runtime seam avoids perturbing the global sprite
     * allocator while still exercising the stale unsprited ownership path. */
    gObjectEvents[2].active = TRUE;
    gObjectEvents[2].localId = 1;
    gObjectEvents[2].mapGroup = 1;
    gObjectEvents[2].mapNum = 3;
    gObjectEvents[2].movementType = MOVEMENT_TYPE_NONE;
    gObjectEvents[2].currentCoords.x = MAP_OFFSET + 2;
    gObjectEvents[2].currentCoords.y = MAP_OFFSET + 5;
    gObjectEvents[2].spriteId = 2;
    gSprites[2].inUse = TRUE;
    /* Deliberately collide with the remote ObjectEvent index.  The backlink
     * alone must not authorize presence teardown of this foreign sprite. */
    gSprites[2].data[0] = 1;
    gSprites[1].inUse = FALSE;
    /* Leave the remote ObjectEvent active with an absent sprite, exactly as
     * ResumeMap does before the return-to-field object pass. */
    CoopPresenceRuntime_RetireRemoteObjectOnReturnToField(1);
    EXPECT(!gObjectEvents[1].active);
    CoopPresenceRuntime_Update();
    EXPECT(gObjectEvents[1].active);
    EXPECT(CoopPresenceRuntime_IsRemoteObject(&gObjectEvents[1]));
    recreated_sprite_id = gObjectEvents[1].spriteId;
    gPlayerAvatar.objectEventId = 0;
    gPlayerAvatar.spriteId = gObjectEvents[0].spriteId;
    gPlayerAvatar.flags = PLAYER_AVATAR_FLAG_ON_FOOT | PLAYER_AVATAR_FLAG_CONTROLLABLE;
    gSprites[gPlayerAvatar.spriteId].inUse = TRUE;
    gSprites[gPlayerAvatar.spriteId].data[0] = 0;
    EXPECT(gObjectEvents[0].active);
    EXPECT(gObjectEvents[2].active);
    EXPECT(gSprites[2].inUse);
    EXPECT_EQ(gSprites[2].data[0], 1);

    /* Keep the local player adjacent to the remote after the far snap so the
     * queue-full assertion reaches the transport result rather than the
     * spatial admission check. */
    gObjectEvents[0].currentCoords.x = MAP_OFFSET + 10;
    gObjectEvents[0].previousCoords.x = MAP_OFFSET + 10;
    gObjectEvents[0].initialCoords.x = MAP_OFFSET + 10;
    gSaveBlock1Ptr->pos.x = MAP_OFFSET + 10;

    /* Fill the bounded outbound queue: a visible remote is still consumed
     * when its interaction cannot be published this frame. */
    while (!CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network))
    {
        struct CoopBridgeMessage discarded;
        EXPECT(CoopNetBridge_DequeueGameToNetwork(&discarded));
    }
    for (i = 0; i < COOP_NET_BRIDGE_QUEUE_CAPACITY; i++)
        EXPECT(CoopNetBridge_EnqueueGameToNetwork(
            COOP_BRIDGE_MESSAGE_CHECKPOINT_READY, NULL, 0));
    EXPECT(CoopBridgeQueue_IsFull(&gCoopNetBridge.game_to_network));
    EXPECT_EQ(CoopPresenceRuntime_TryInteract(),
              COOP_PRESENCE_INTERACTION_CONSUMED_NO_LOCK);

    /* A stale cached sprite identity must not destroy a foreign sprite or
     * mutate the local ObjectEvent that now owns it. */
    gObjectEvents[1].spriteId = 2;
    CoopPresenceRuntime_RetireRemoteObjectOnReturnToField(1);
    EXPECT(!gObjectEvents[1].active);
    CoopPresenceRuntime_TransportLost();
    EXPECT(!gSprites[recreated_sprite_id].inUse);
    EXPECT(gObjectEvents[2].active);
    EXPECT(gSprites[2].inUse);
    EXPECT_EQ(gSprites[2].data[0], 1);

    EndRuntimeFixture(&sRuntimeFixtureBackup);
}

TEST("Cloud Coop presence retries renderer creation after object capacity")
{
    struct CoopPresenceSpawn spawn = RuntimeSpawn(6, 1);
    u8 spawn_bytes[COOP_PRESENCE_SPAWN_SIZE];
    u32 i;

    BeginRuntimeFixture(&sRuntimeFixtureBackup);
    CoopSave_InitializeCurrent();
    CoopNetBridge_Init();
    CoopPresenceRuntime_SetSessionEpoch(23);
    spawn.state.pose.location.y = 6;
    spawn.state.pose.warp_sequence = 1;
    EXPECT(CoopPresence_EncodeSpawn(&spawn, spawn_bytes, sizeof(spawn_bytes)));
    EXPECT(CoopPresenceRuntime_QueueBridgeFrame(
        COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_SPAWN,
        spawn_bytes, sizeof(spawn_bytes)));

    /* Occupy every ObjectEvent slot with independent map objects.  The
     * reducer still accepts the spawn, but the renderer must fail closed. */
    for (i = 1; i < OBJECT_EVENTS_COUNT; i++)
    {
        gObjectEvents[i].active = TRUE;
        gObjectEvents[i].localId = (u8)i;
        gObjectEvents[i].mapGroup = 1;
        gObjectEvents[i].mapNum = 3;
        gObjectEvents[i].movementType = MOVEMENT_TYPE_NONE;
        gObjectEvents[i].currentElevation = ELEVATION_DEFAULT;
        gObjectEvents[i].previousElevation = ELEVATION_DEFAULT;
    }
    CoopPresenceRuntime_Update();
    EXPECT(CoopPresenceReducer_IsVisible(CoopPresenceRuntime_GetReducer()));
    EXPECT(!CoopPresenceRuntime_IsRemoteObject(&gObjectEvents[1]));

    /* Releasing exactly one genuine ObjectEvent slot lets the next update
     * retry the real production spawn path. */
    gObjectEvents[1].active = FALSE;
    CoopPresenceRuntime_Update();
    EXPECT(gObjectEvents[1].active);
    EXPECT(CoopPresenceRuntime_IsRemoteObject(&gObjectEvents[1]));
    EXPECT(gSprites[gObjectEvents[1].spriteId].inUse);

    EndRuntimeFixture(&sRuntimeFixtureBackup);
}

TEST("Cloud Coop local-link lookup isolates remote presence collisions")
{
    struct CoopPresenceSpawn spawn = RuntimeSpawn(7, 1);
    struct MapPosition position;
    u8 spawn_bytes[COOP_PRESENCE_SPAWN_SIZE];

    BeginRuntimeFixture(&sRuntimeFixtureBackup);
    CoopSave_InitializeCurrent();
    CoopNetBridge_Init();
    CoopPresenceRuntime_SetSessionEpoch(23);
    spawn.state.pose.location.y = 6;
    spawn.state.pose.warp_sequence = 1;
    EXPECT(CoopPresence_EncodeSpawn(&spawn, spawn_bytes, sizeof(spawn_bytes)));
    EXPECT(CoopPresenceRuntime_QueueBridgeFrame(
        COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_SPAWN,
        spawn_bytes, sizeof(spawn_bytes)));
    CoopPresenceRuntime_Update();

    gObjectEvents[2].active = TRUE;
    gObjectEvents[2].localId = 1;
    gObjectEvents[2].mapGroup = 1;
    gObjectEvents[2].mapNum = 3;
    gObjectEvents[2].movementType = MOVEMENT_TYPE_NONE;
    gObjectEvents[2].currentElevation = ELEVATION_DEFAULT;
    gObjectEvents[2].previousElevation = ELEVATION_DEFAULT;
    gObjectEvents[2].currentCoords.x = MAP_OFFSET + 4;
    gObjectEvents[2].currentCoords.y = MAP_OFFSET + 6;
    gObjectEvents[2].previousCoords = gObjectEvents[2].currentCoords;
    gObjectEvents[2].initialCoords = gObjectEvents[2].currentCoords;
    sRuntimeTestObjectTemplates[0] = (struct ObjectEventTemplate){
        .localId = 1,
        .graphicsId = OBJ_EVENT_GFX_BRENDAN_NORMAL,
        .kind = OBJ_KIND_NORMAL,
        .x = 4,
        .y = 6,
        .elevation = ELEVATION_DEFAULT,
        .movementType = MOVEMENT_TYPE_NONE,
        .script = EventScript_TestSignpostMsg,
    };
    sRuntimeTestMapEvents = (struct MapEvents){
        .objectEventCount = 1,
        .objectEvents = sRuntimeTestObjectTemplates,
    };
    gMapHeader.events = &sRuntimeTestMapEvents;
    gSaveBlock1Ptr->objectEventTemplates[0] = sRuntimeTestObjectTemplates[0];
    gLinkPlayerObjectEvents[0] = (struct LinkPlayerObjectEvent){
        .active = TRUE,
        .linkPlayerId = 0,
        .objEventId = 2,
        .movementMode = 0,
    };
    position = (struct MapPosition){
        .x = MAP_OFFSET + 4,
        .y = MAP_OFFSET + 6,
        .elevation = ELEVATION_DEFAULT,
    };

    /* A valid link-player ObjectEvent has precedence over its ordinary local
     * script, while the reserved remote remains excluded from object lookup. */
    EXPECT_EQ(GetObjectEventIdByPosition(position.x, position.y,
                                         position.elevation), 2);
    EXPECT(GetInteractedLinkPlayerScript(&position, 0, DIR_SOUTH) == NULL);

    /* Collision uses the same reserved-remote exclusion in both directions:
     * the player may pass through the visible remote, while the ordinary
     * link-player ObjectEvent at the same tile remains a real blocker. */
    EXPECT_EQ(GetObjectObjectCollidesWith(&gObjectEvents[0],
                                          position.x, position.y, FALSE), 2);
    EXPECT_EQ(GetObjectObjectCollidesWith(&gObjectEvents[1],
                                          position.x, position.y, FALSE),
              OBJECT_EVENTS_COUNT);

    /* With the ordinary object removed, the remote alone is nonblocking. */
    gObjectEvents[2].active = FALSE;
    EXPECT_EQ(GetObjectObjectCollidesWith(&gObjectEvents[0],
                                          position.x, position.y, FALSE),
              OBJECT_EVENTS_COUNT);
    gObjectEvents[2].active = TRUE;

    /* A link record referring only to the remote does not hide the genuine
     * local object; the local script remains independently addressable. */
    gLinkPlayerObjectEvents[0].objEventId = 1;
    EXPECT_EQ(GetObjectEventIdByPosition(position.x, position.y,
                                         position.elevation), 2);
    EXPECT(GetInteractedLinkPlayerScript(&position, 0, DIR_SOUTH)
           == EventScript_TestSignpostMsg);
    gObjectEvents[2].active = FALSE;
    EXPECT_EQ(GetObjectEventIdByPosition(position.x, position.y,
                                         position.elevation),
              OBJECT_EVENTS_COUNT);

    EndRuntimeFixture(&sRuntimeFixtureBackup);
}

TEST("Cloud Coop field input preserves vanilla interaction precedence")
{
    struct CoopPresenceSpawn spawn = RuntimeSpawn(5, 1);
    struct FieldInput input;
    struct CoopBridgeMessage message;
    u8 spawn_bytes[COOP_PRESENCE_SPAWN_SIZE];

    BeginRuntimeFixture(&sRuntimeFixtureBackup);
    CoopSave_InitializeCurrent();
    CoopNetBridge_Init();
    CoopPresenceRuntime_SetSessionEpoch(23);
    spawn.state.pose.warp_sequence = 1;
    spawn.state.pose.location.y = 6;
    EXPECT(CoopPresence_EncodeSpawn(&spawn, spawn_bytes, sizeof(spawn_bytes)));
    EXPECT(CoopPresenceRuntime_QueueBridgeFrame(
        COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_SPAWN,
        spawn_bytes, sizeof(spawn_bytes)));
    CoopPresenceRuntime_Update();
    EXPECT(CoopPresenceReducer_IsVisible(CoopPresenceRuntime_GetReducer()));

    /* Put a genuine local ObjectEvent on the same tile as the remote.  The
     * field lookup deliberately skips the reserved remote slot and resolves
     * this real object first. */
    gObjectEvents[2].active = TRUE;
    gObjectEvents[2].localId = 1;
    gObjectEvents[2].mapGroup = 1;
    gObjectEvents[2].mapNum = 3;
    gObjectEvents[2].movementType = MOVEMENT_TYPE_NONE;
    gObjectEvents[2].currentElevation = ELEVATION_DEFAULT;
    gObjectEvents[2].previousElevation = ELEVATION_DEFAULT;
    gObjectEvents[2].currentCoords.x = MAP_OFFSET + 4;
    gObjectEvents[2].currentCoords.y = MAP_OFFSET + 6;
    gObjectEvents[2].previousCoords = gObjectEvents[2].currentCoords;
    gObjectEvents[2].initialCoords = gObjectEvents[2].currentCoords;
    gObjectEvents[2].spriteId = 2;
    gSprites[2].inUse = TRUE;
    gSprites[2].data[0] = 2;
    sRuntimeTestObjectTemplates[0] = (struct ObjectEventTemplate){
        .localId = 1,
        .graphicsId = OBJ_EVENT_GFX_BRENDAN_NORMAL,
        .kind = OBJ_KIND_NORMAL,
        .x = 4,
        .y = 6,
        .elevation = ELEVATION_DEFAULT,
        .movementType = MOVEMENT_TYPE_NONE,
        .script = EventScript_TestSignpostMsg,
    };
    sRuntimeTestMapEvents = (struct MapEvents){
        .objectEventCount = 1,
        .objectEvents = sRuntimeTestObjectTemplates,
    };
    gMapHeader.events = &sRuntimeTestMapEvents;
    gSaveBlock1Ptr->objectEventTemplates[0] = sRuntimeTestObjectTemplates[0];
    while (!CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network))
        EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));

    /* A real local object script owns the same tile as the remote. */
    ScriptContext_Init();
    FieldClearPlayerInput(&input);
    input.pressedAButton = TRUE;
    EXPECT_EQ(ProcessPlayerFieldInput(&input), FIELD_INPUT_RESULT_SCRIPT_STARTED);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    ScriptContext_Stop();

    /* Reset the complete runtime fixture before proving the remote-only path.
     * This keeps the script context and local object teardown independent of
     * the fallback assertion while retaining a real, empty map-event table. */
    ScriptContext_Init();
    EndRuntimeFixture(&sRuntimeFixtureBackup);

    BeginRuntimeFixture(&sRuntimeFixtureBackup);
    CoopSave_InitializeCurrent();
    CoopNetBridge_Init();
    CoopPresenceRuntime_SetSessionEpoch(23);
    spawn = RuntimeSpawn(5, 1);
    spawn.state.pose.warp_sequence = 1;
    spawn.state.pose.location.y = 6;
    EXPECT(CoopPresence_EncodeSpawn(&spawn, spawn_bytes, sizeof(spawn_bytes)));
    EXPECT(CoopPresenceRuntime_QueueBridgeFrame(
        COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_SPAWN,
        spawn_bytes, sizeof(spawn_bytes)));
    CoopPresenceRuntime_Update();
    EXPECT(CoopPresenceReducer_IsVisible(CoopPresenceRuntime_GetReducer()));

    sRuntimeTestMapEvents = (struct MapEvents){0};
    gMapHeader.events = &sRuntimeTestMapEvents;
    while (!CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network))
        EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    FieldClearPlayerInput(&input);
    input.pressedAButton = TRUE;
    EXPECT_EQ(ProcessPlayerFieldInput(&input), FIELD_INPUT_RESULT_CONSUMED_NO_LOCK);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_INTERACT_REMOTE_PLAYER);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    ScriptContext_Init();

    EndRuntimeFixture(&sRuntimeFixtureBackup);
}

TEST("Cloud Coop dive action takes precedence over remote overlap")
{
    struct CoopPresenceSpawn spawn = RuntimeSpawn(8, 1);
    struct FieldInput input;
    struct CoopBridgeMessage message;
    u8 spawn_bytes[COOP_PRESENCE_SPAWN_SIZE];
    bool8 had_dive_badge;

    BeginRuntimeFixture(&sRuntimeFixtureBackup);
    CoopSave_InitializeCurrent();
    CoopNetBridge_Init();
    CoopPresenceRuntime_SetSessionEpoch(23);
    spawn.state.pose.location.y = 6;
    spawn.state.pose.warp_sequence = 1;
    EXPECT(CoopPresence_EncodeSpawn(&spawn, spawn_bytes, sizeof(spawn_bytes)));
    EXPECT(CoopPresenceRuntime_QueueBridgeFrame(
        COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_SPAWN,
        spawn_bytes, sizeof(spawn_bytes)));
    CoopPresenceRuntime_Update();
    EXPECT(CoopPresenceReducer_IsVisible(CoopPresenceRuntime_GetReducer()));

    /* Use a real diveable tile and map connection so the production
     * TrySetupDiveDownScript branch returns true at the same tile as remote. */
    sRuntimeTestMapLayout.primaryTileset = &gTileset_General;
    sRuntimeTestMapData[(MAP_OFFSET + 4)
        + (MAP_OFFSET + 5) * 32] = METATILE_General_RoughDeepWater;
    sRuntimeTestDiveConnection = (struct MapConnection){
        .direction = CONNECTION_DIVE,
        .offset = 0,
        .mapGroup = 1,
        .mapNum = 3,
    };
    sRuntimeTestMapConnections = (struct MapConnections){
        .count = 1,
        .connections = &sRuntimeTestDiveConnection,
    };
    gMapHeader.connections = &sRuntimeTestMapConnections;
    had_dive_badge = FlagGet(FLAG_BADGE07_GET);
    FlagSet(FLAG_BADGE07_GET);
    EXPECT_EQ(gMapHeader.mapType, MAP_TYPE_TOWN);
    EXPECT_EQ(gPlayerAvatar.objectEventId, 0);
    EXPECT_EQ(gObjectEvents[0].currentCoords.y, MAP_OFFSET + 5);
    EXPECT_EQ(gMapHeader.connections->connections->direction, CONNECTION_DIVE);
    EXPECT_EQ(MapGridGetMetatileBehaviorAt(MAP_OFFSET + 4, MAP_OFFSET + 5),
              MB_DEEP_WATER);
    EXPECT_EQ(TrySetDiveWarp(), 2);
    ScriptContext_Init();
    EXPECT(CoopPresenceRuntime_TestTrySetupDiveDownScript());
    ScriptContext_Init();
    UnlockPlayerFieldControls();
    while (!CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network))
        EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    FieldClearPlayerInput(&input);
    input.pressedAButton = TRUE;
    EXPECT_EQ(ProcessPlayerFieldInput(&input), FIELD_INPUT_RESULT_SCRIPT_STARTED);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    ScriptContext_Stop();
    if (!had_dive_badge)
        FlagClear(FLAG_BADGE07_GET);

    EndRuntimeFixture(&sRuntimeFixtureBackup);
}
