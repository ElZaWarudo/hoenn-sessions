#include <stddef.h>

#include "global.h"
#include "coop/net_bridge.h"
#include "coop/presence_runtime.h"
#include "coop/region.h"
#include "coop/save.h"
#include "gba/flash_internal.h"
#include "fieldmap.h"
#include "load_save.h"
#include "main.h"
#include "overworld.h"
#include "palette.h"
#include "save.h"
#include "test/test.h"

_Static_assert(sizeof(struct CoopBridgeMessage) == 144, "tested message ABI size");
_Static_assert(offsetof(struct CoopBridgeMessage, type) == 0, "tested message type offset");
_Static_assert(offsetof(struct CoopBridgeMessage, length) == 2, "tested message length offset");
_Static_assert(offsetof(struct CoopBridgeMessage, sequence) == 4, "tested message sequence offset");
_Static_assert(offsetof(struct CoopBridgeMessage, session_epoch) == 8, "tested message epoch offset");
_Static_assert(offsetof(struct CoopBridgeMessage, payload) == 12, "tested message payload offset");
_Static_assert(offsetof(struct CoopBridgeMessage, checksum) == 140, "tested message checksum offset");
_Static_assert(sizeof(struct CoopBridgeQueue) == 4612, "tested queue ABI size");
_Static_assert(offsetof(struct CoopBridgeQueue, entries) == 4, "tested queue entries offset");
_Static_assert(offsetof(struct CoopNetBridge, status_flags) == 12, "tested bridge status offset");
_Static_assert(offsetof(struct CoopNetBridge, last_sidecar_heartbeat) == 16, "tested heartbeat offset");
_Static_assert(offsetof(struct CoopNetBridge, game_to_network) == 20, "tested outbound queue offset");
_Static_assert(offsetof(struct CoopNetBridge, network_to_game) == 4632, "tested inbound queue offset");
_Static_assert(sizeof(struct CoopNetBridge) == 9244, "tested bridge ABI size");
_Static_assert(sizeof(struct CoopBridgePlayerState) == COOP_PRESENCE_LOCAL_STATE_SIZE, "tested player-state ABI size");

static struct CoopBridgeQueue *GetTestQueue(void)
{
    return &gCoopNetBridge.game_to_network;
}

static void SealTestMessage(struct CoopBridgeMessage *message, u16 type, u32 sequence, u32 sessionEpoch)
{
    static const u8 sPayload[] = {0x12, 0x34, 0x56, 0x78};

    EXPECT(CoopBridgeMessage_Seal(message,
                                  type,
                                  sequence,
                                  sessionEpoch,
                                  sPayload,
                                  sizeof(sPayload)));
}

static void PopInitialRomReady(void)
{
    struct CoopBridgeMessage message;

    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
}

static void InitTestBridge(void)
{
    CoopSave_InitializeCurrent();
    CoopNetBridge_Init();
}

static void HostWriteInboundUnchecked(const struct CoopBridgeMessage *message)
{
    struct CoopBridgeQueue *queue = &gCoopNetBridge.network_to_game;
    u16 index = queue->write_index & (COOP_NET_BRIDGE_QUEUE_CAPACITY - 1);

    queue->entries[index] = *message;
    queue->write_index++;
}

static u32 sSaveSectorProgramCalls;
static EWRAM_DATA u16 sPresenceBridgeMapData[32 * 32];

static u16 CountSaveSectorProgramCalls(u16 sector, u8 *data)
{
    (void)sector;
    (void)data;
    sSaveSectorProgramCalls++;
    return 1;
}

TEST("Cloud Coop wire ABI matches the documented compact layout")
{
    EXPECT_EQ(sizeof(struct CoopBridgeMessage), 144);
    EXPECT_EQ(offsetof(struct CoopBridgeMessage, type), 0);
    EXPECT_EQ(offsetof(struct CoopBridgeMessage, length), 2);
    EXPECT_EQ(offsetof(struct CoopBridgeMessage, sequence), 4);
    EXPECT_EQ(offsetof(struct CoopBridgeMessage, session_epoch), 8);
    EXPECT_EQ(offsetof(struct CoopBridgeMessage, payload), 12);
    EXPECT_EQ(offsetof(struct CoopBridgeMessage, checksum), 140);
    EXPECT_EQ(sizeof(struct CoopBridgeQueue), 4612);
    EXPECT_EQ(offsetof(struct CoopBridgeQueue, entries), 4);
    EXPECT_EQ(offsetof(struct CoopNetBridge, status_flags), 12);
    EXPECT_EQ(offsetof(struct CoopNetBridge, last_sidecar_heartbeat), 16);
    EXPECT_EQ(offsetof(struct CoopNetBridge, game_to_network), 20);
    EXPECT_EQ(offsetof(struct CoopNetBridge, network_to_game), 4632);
    EXPECT_EQ(sizeof(struct CoopNetBridge), 9244);
    EXPECT_EQ(sizeof(struct CoopBridgePlayerState), COOP_PRESENCE_LOCAL_STATE_SIZE);
}

TEST("Cloud Coop CRC32 matches the canonical check vector")
{
    static const char sCheckVector[] = "123456789";

    EXPECT_EQ(CoopBridge_Crc32(sCheckVector, sizeof(sCheckVector) - 1), 0xCBF43926u);
    EXPECT_EQ(CoopBridge_Crc32(NULL, 0), 0);
    EXPECT_EQ(CoopBridge_Crc32(NULL, 1), 0);
}

TEST("Cloud Coop bridge messages seal deterministic metadata and reject corruption")
{
    static const u8 sPayload[] = {0xDE, 0xAD, 0xBE, 0xEF};
    struct CoopBridgeMessage message;
    u32 sealedChecksum;

    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_PLAYER_STATE,
                                  42,
                                  9,
                                  sPayload,
                                  sizeof(sPayload)));
    EXPECT(CoopBridgeMessage_Validate(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_PLAYER_STATE);
    EXPECT_EQ(message.length, sizeof(sPayload));
    EXPECT_EQ(message.sequence, 42);
    EXPECT_EQ(message.session_epoch, 9);
    EXPECT_EQ(memcmp(message.payload, sPayload, sizeof(sPayload)), 0);
    EXPECT_EQ(message.payload[sizeof(sPayload)], 0);

    sealedChecksum = message.checksum;
    EXPECT_EQ(sealedChecksum, CoopBridgeMessage_ComputeChecksum(&message));
    message.payload[COOP_NET_BRIDGE_PAYLOAD_SIZE - 1] ^= 1;
    EXPECT(!CoopBridgeMessage_Validate(&message));
    message.payload[COOP_NET_BRIDGE_PAYLOAD_SIZE - 1] ^= 1;
    EXPECT_EQ(message.checksum, sealedChecksum);
    EXPECT(CoopBridgeMessage_Validate(&message));

    message.type = COOP_BRIDGE_MESSAGE_NONE;
    EXPECT(!CoopBridgeMessage_Validate(&message));
    message.type = 0x7777;
    EXPECT(!CoopBridgeMessage_Validate(&message));
    message.type = COOP_BRIDGE_MESSAGE_PLAYER_STATE;
    message.sequence = 0;
    EXPECT(!CoopBridgeMessage_Validate(&message));
    message.sequence = 42;
    message.length = COOP_NET_BRIDGE_PAYLOAD_SIZE + 1;
    EXPECT(!CoopBridgeMessage_Validate(&message));

    EXPECT(!CoopBridgeMessage_Seal(NULL,
                                   COOP_BRIDGE_MESSAGE_PLAYER_STATE,
                                   1,
                                   0,
                                   NULL,
                                   0));
    EXPECT(!CoopBridgeMessage_Seal(&message,
                                   COOP_BRIDGE_MESSAGE_NONE,
                                   1,
                                   0,
                                   NULL,
                                   0));
    EXPECT(!CoopBridgeMessage_Seal(&message,
                                   0x7777,
                                   1,
                                   0,
                                   NULL,
                                   0));
    EXPECT(!CoopBridgeMessage_Seal(&message,
                                   COOP_BRIDGE_MESSAGE_PLAYER_STATE,
                                   0,
                                   0,
                                   NULL,
                                   0));
    EXPECT(!CoopBridgeMessage_Seal(&message,
                                   COOP_BRIDGE_MESSAGE_PLAYER_STATE,
                                   1,
                                   0,
                                   NULL,
                                   1));
    EXPECT(!CoopBridgeMessage_Seal(&message,
                                   COOP_BRIDGE_MESSAGE_PLAYER_STATE,
                                   1,
                                   0,
                                   sPayload,
                                   COOP_NET_BRIDGE_PAYLOAD_SIZE + 1));
}

TEST("Cloud Coop bridge queue preserves FIFO order across u16 counter wrap")
{
    struct CoopBridgeQueue *queue = GetTestQueue();
    struct CoopBridgeMessage message;
    u32 sequence;

    CoopBridgeQueue_Init(queue);
    queue->read_index = 0xFFF0u;
    queue->write_index = 0xFFF0u;
    EXPECT(CoopBridgeQueue_IsEmpty(queue));
    EXPECT(!CoopBridgeQueue_IsFull(queue));

    for (sequence = 1; sequence <= COOP_NET_BRIDGE_QUEUE_CAPACITY; sequence++)
    {
        SealTestMessage(&message, COOP_BRIDGE_MESSAGE_PLAYER_STATE, sequence, 7);
        EXPECT(CoopBridgeQueue_Push(queue, &message));
    }
    EXPECT(!CoopBridgeQueue_IsEmpty(queue));
    EXPECT(CoopBridgeQueue_IsFull(queue));

    SealTestMessage(&message,
                    COOP_BRIDGE_MESSAGE_PLAYER_STATE,
                    COOP_NET_BRIDGE_QUEUE_CAPACITY + 1,
                    7);
    EXPECT(!CoopBridgeQueue_Push(queue, &message));

    EXPECT(CoopBridgeQueue_Pop(queue, &message));
    EXPECT_EQ(message.sequence, 1);
    SealTestMessage(&message,
                    COOP_BRIDGE_MESSAGE_PLAYER_STATE,
                    COOP_NET_BRIDGE_QUEUE_CAPACITY + 1,
                    7);
    EXPECT(CoopBridgeQueue_Push(queue, &message));
    EXPECT(CoopBridgeQueue_IsFull(queue));

    for (sequence = 2; sequence <= COOP_NET_BRIDGE_QUEUE_CAPACITY + 1; sequence++)
    {
        EXPECT(CoopBridgeQueue_Pop(queue, &message));
        EXPECT_EQ(message.sequence, sequence);
    }
    EXPECT(CoopBridgeQueue_IsEmpty(queue));
    EXPECT(!CoopBridgeQueue_IsFull(queue));
    EXPECT(!CoopBridgeQueue_Pop(queue, &message));
    EXPECT_EQ(queue->read_index, queue->write_index);
}

TEST("Cloud Coop bridge queue rejects malformed producer indexes")
{
    struct CoopBridgeQueue *queue = GetTestQueue();
    struct CoopBridgeMessage message;
    u16 readIndex;
    u16 writeIndex;

    CoopBridgeQueue_Init(queue);
    SealTestMessage(&message, COOP_BRIDGE_MESSAGE_PLAYER_STATE, 1, 7);
    queue->read_index = 100;
    queue->write_index = 100 + COOP_NET_BRIDGE_QUEUE_CAPACITY + 1;
    readIndex = queue->read_index;
    writeIndex = queue->write_index;

    /* Both predicates fail closed: callers can neither consume nor publish. */
    EXPECT(CoopBridgeQueue_IsEmpty(queue));
    EXPECT(CoopBridgeQueue_IsFull(queue));
    EXPECT(!CoopBridgeQueue_Push(queue, &message));
    EXPECT(!CoopBridgeQueue_Pop(queue, &message));
    EXPECT_EQ(queue->read_index, readIndex);
    EXPECT_EQ(queue->write_index, writeIndex);

    queue->read_index = 10;
    queue->write_index = 9;
    EXPECT(CoopBridgeQueue_IsEmpty(queue));
    EXPECT(CoopBridgeQueue_IsFull(queue));
    EXPECT(!CoopBridgeQueue_Push(queue, &message));
    EXPECT(!CoopBridgeQueue_Pop(queue, &message));
}

TEST("Cloud Coop bridge poll discards an impossible inbound queue depth")
{
    InitTestBridge();
    gCoopNetBridge.network_to_game.read_index = 25;
    gCoopNetBridge.network_to_game.write_index = 25 + COOP_NET_BRIDGE_QUEUE_CAPACITY + 1;

    CoopNetBridge_Poll();

    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_QUEUE_ERROR);
    EXPECT_EQ(gCoopNetBridge.network_to_game.read_index,
              gCoopNetBridge.network_to_game.write_index);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.network_to_game));
}

TEST("Cloud Coop session epoch change clears both queues and reissues ROM ready")
{
    struct CoopBridgeMessage message;

    InitTestBridge();
    EXPECT(CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_CHECKPOINT_READY,
                                               NULL,
                                               0));

    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  5,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    SealTestMessage(&message, COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_UPDATE, 6, 17);
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));

    CoopNetBridge_Poll();

    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SESSION_READY);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.network_to_game));
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    EXPECT_EQ(message.sequence, 1);
    EXPECT_EQ(message.session_epoch, 17);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
}

TEST("Cloud Coop same epoch reconnect rejects stale replay and preserves sequences")
{
    struct CoopBridgeMessage message;
    u32 pollCount;
    u16 outboundReadIndex;
    u16 outboundWriteIndex;
    u16 inboundIndex;

    InitTestBridge();
    PopInitialRomReady();

    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  10,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    EXPECT_EQ(message.sequence, 1);
    EXPECT_EQ(message.session_epoch, 17);

    EXPECT(CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_CHECKPOINT_READY,
                                               NULL,
                                               0));
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_CHECKPOINT_READY);
    EXPECT_EQ(message.sequence, 2);

    for (pollCount = 0; pollCount < COOP_NET_BRIDGE_SIDECAR_STALE_INTERVAL; pollCount++)
        CoopNetBridge_Poll();
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SIDECAR_HEARTBEAT_STALE);
    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SESSION_READY));

    EXPECT(CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_CHECKPOINT_READY,
                                               NULL,
                                               0));
    outboundReadIndex = gCoopNetBridge.game_to_network.read_index;
    outboundWriteIndex = gCoopNetBridge.game_to_network.write_index;
    inboundIndex = gCoopNetBridge.network_to_game.read_index;
    EXPECT_EQ(gCoopNetBridge.network_to_game.write_index, inboundIndex);

    /* A replay must be consumed without resetting either queue or re-arming
     * the disconnected session. */
    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  10,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();

    inboundIndex++;
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SIDECAR_HEARTBEAT_STALE);
    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SESSION_READY));
    EXPECT_EQ(gCoopNetBridge.game_to_network.read_index, outboundReadIndex);
    EXPECT_EQ(gCoopNetBridge.game_to_network.write_index, outboundWriteIndex);
    EXPECT_EQ(gCoopNetBridge.network_to_game.read_index, inboundIndex);
    EXPECT_EQ(gCoopNetBridge.network_to_game.write_index, inboundIndex);

    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  9,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();

    inboundIndex++;
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SIDECAR_HEARTBEAT_STALE);
    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SESSION_READY));
    EXPECT_EQ(gCoopNetBridge.game_to_network.read_index, outboundReadIndex);
    EXPECT_EQ(gCoopNetBridge.game_to_network.write_index, outboundWriteIndex);
    EXPECT_EQ(gCoopNetBridge.network_to_game.read_index, inboundIndex);
    EXPECT_EQ(gCoopNetBridge.network_to_game.write_index, inboundIndex);

    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  11,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();

    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SESSION_READY);
    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SIDECAR_HEARTBEAT_STALE));
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    EXPECT_EQ(message.sequence, 4);
    EXPECT_EQ(message.session_epoch, 17);
}

TEST("Cloud Coop rejects unsupported inbound types before later valid session traffic")
{
    struct CoopBridgeMessage message;

    InitTestBridge();
    PopInitialRomReady();

    SealTestMessage(&message, COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_UPDATE, 100, 21);
    message.type = 0x7777;
    message.checksum = CoopBridgeMessage_ComputeChecksum(&message);
    EXPECT(!CoopNetBridge_EnqueueNetworkToGame(&message));
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.network_to_game));

    SealTestMessage(&message, COOP_BRIDGE_MESSAGE_ROM_READY, 101, 21);
    EXPECT(!CoopNetBridge_EnqueueNetworkToGame(&message));
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.network_to_game));

    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  1,
                                  21,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();

    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SESSION_READY);
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    EXPECT_EQ(message.sequence, 1);
    EXPECT_EQ(message.session_epoch, 21);
}

TEST("Cloud Coop raw unsupported inbound traffic cannot suppress a valid new epoch")
{
    struct CoopBridgeMessage message;

    InitTestBridge();
    PopInitialRomReady();

    /* Model mGBA Lua publishing directly into EWRAM, bypassing the C helper. */
    SealTestMessage(&message, COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_UPDATE, 100, 21);
    message.type = 0x7777;
    message.checksum = CoopBridgeMessage_ComputeChecksum(&message);
    HostWriteInboundUnchecked(&message);
    SealTestMessage(&message, COOP_BRIDGE_MESSAGE_ROM_READY, 101, 21);
    HostWriteInboundUnchecked(&message);
    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  1,
                                  21,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));

    CoopNetBridge_Poll();

    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR);
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SESSION_READY);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.network_to_game));
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    EXPECT_EQ(message.sequence, 1);
    EXPECT_EQ(message.session_epoch, 21);
}

TEST("Cloud Coop malformed remote lifecycle payload does not consume its outer sequence")
{
    struct CoopBridgeMessage message;
    u8 malformed_spawn[COOP_PRESENCE_SPAWN_SIZE] = {0};

    InitTestBridge();
    PopInitialRomReady();
    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  1,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);

    /* The outer frame is valid, but a spawn must be exactly 72 bytes.  The
     * bridge must reject it without advancing rx_sequence or mutating the
     * runtime's pending lifecycle queue. */
    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_SPAWN,
                                  2,
                                  17,
                                  malformed_spawn,
                                  COOP_PRESENCE_SPAWN_SIZE - 1));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR);

    /* Reusing the rejected outer sequence as a fresh SESSION_READY proves
     * that malformed remote data did not partially consume the sequence
     * domain.  The accepted reconnect also clears pending bridge queues. */
    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  2,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SESSION_READY);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    EXPECT_EQ(message.session_epoch, 17);
}

TEST("Cloud Coop newer live same epoch SESSION_READY cuts over pending presence")
{
    struct CoopBridgeMessage message;
    struct CoopPresenceSpawn spawn = {
        .handle = 9,
        .server_sequence = 1,
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
                .warp_sequence = 1,
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
    u8 spawn_bytes[COOP_PRESENCE_SPAWN_SIZE];
    struct MapLayout map_layout = {
        .width = 20,
        .height = 20,
        .map = sPresenceBridgeMapData,
    };
    struct MapHeader saved_map_header = gMapHeader;
    struct BackupMapLayout saved_backup_map_layout = gBackupMapLayout;
    struct CoopSaveV1 saved_coop_save;
    struct PlayerAvatar saved_player_avatar = gPlayerAvatar;
    struct SaveBlock1 *saved_save_block1 = gSaveBlock1Ptr;
    struct ObjectEvent saved_object_event0 = gObjectEvents[0];
    struct ObjectEvent saved_object_event1 = gObjectEvents[1];
    struct Sprite saved_sprite0 = gSprites[0];
    struct Sprite saved_sprite1 = gSprites[1];
    struct Coords16 saved_save_position;
    struct WarpData saved_save_location;
    MainCallback saved_callback1 = gMain.callback1;
    MainCallback saved_callback2 = gMain.callback2;
    bool8 saved_palette_fade_active = gPaletteFade.active;
    u32 i;

    if (saved_save_block1 != NULL)
    {
        saved_save_position = saved_save_block1->pos;
        saved_save_location = saved_save_block1->location;
    }
    if (gSaveBlock3Ptr != NULL)
        saved_coop_save = gSaveBlock3Ptr->coop;
    InitTestBridge();
    PopInitialRomReady();
    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  1,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);

    for (i = 0; i < ARRAY_COUNT(sPresenceBridgeMapData); i++)
        sPresenceBridgeMapData[i] = PACK_ELEVATION(ELEVATION_DEFAULT);
    gMapHeader.mapLayout = &map_layout;
    gMapHeader.events = NULL;
    gMapHeader.engineRegion = COOP_MAP_ENGINE_REGION_HOENN;
    gMapHeader.regionMapSectionId = MAPSEC_LITTLEROOT_TOWN;
    gBackupMapLayout.width = 32;
    gBackupMapLayout.height = 32;
    gBackupMapLayout.map = sPresenceBridgeMapData;
    gSaveBlock1Ptr = &gSaveblock1.block;
    gSaveBlock1Ptr->location.mapGroup = 1;
    gSaveBlock1Ptr->location.mapNum = 3;
    gSaveBlock1Ptr->pos.x = MAP_OFFSET + 4;
    gSaveBlock1Ptr->pos.y = MAP_OFFSET + 5;
    memset(&gObjectEvents[0], 0, sizeof(gObjectEvents[0]));
    memset(&gObjectEvents[1], 0, sizeof(gObjectEvents[1]));
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
    memset(&gSprites[0], 0, sizeof(gSprites[0]));
    memset(&gSprites[1], 0, sizeof(gSprites[1]));
    gSprites[0].inUse = TRUE;
    gSprites[0].data[0] = 0;
    gPlayerAvatar.objectEventId = 0;
    gPlayerAvatar.spriteId = 0;
    gPlayerAvatar.flags = PLAYER_AVATAR_FLAG_ON_FOOT | PLAYER_AVATAR_FLAG_CONTROLLABLE;
    gMain.callback1 = CB1_Overworld;
    gMain.callback2 = CB2_Overworld;
    gPaletteFade.active = FALSE;

    EXPECT(CoopPresence_EncodeSpawn(&spawn, spawn_bytes, sizeof(spawn_bytes)));
    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_SPAWN,
                                  2,
                                  17,
                                  spawn_bytes,
                                  sizeof(spawn_bytes)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    CoopPresenceRuntime_Update();
    EXPECT(CoopPresenceReducer_IsActive(CoopPresenceRuntime_GetReducer()));

    /* This malformed frame is rejected and must not consume sequence 3. */
    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_SPAWN,
                                  3,
                                  17,
                                  spawn_bytes,
                                  sizeof(spawn_bytes) - 1));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  3,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));

    CoopNetBridge_Poll();
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SESSION_READY);
    EXPECT(!CoopPresenceReducer_IsActive(CoopPresenceRuntime_GetReducer()));
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    EXPECT_EQ(message.session_epoch, 17);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.network_to_game));

    CoopPresenceRuntime_TransportLost();
    CoopNetBridge_Init();
    gMapHeader = saved_map_header;
    gBackupMapLayout = saved_backup_map_layout;
    gObjectEvents[0] = saved_object_event0;
    gObjectEvents[1] = saved_object_event1;
    gSprites[0] = saved_sprite0;
    gSprites[1] = saved_sprite1;
    gPlayerAvatar = saved_player_avatar;
    if (saved_save_block1 != NULL)
    {
        saved_save_block1->pos = saved_save_position;
        saved_save_block1->location = saved_save_location;
    }
    if (gSaveBlock3Ptr != NULL)
        gSaveBlock3Ptr->coop = saved_coop_save;
    gSaveBlock1Ptr = saved_save_block1;
    gMain.callback1 = saved_callback1;
    gMain.callback2 = saved_callback2;
    gPaletteFade.active = saved_palette_fade_active;
}

static void EstablishTestCloudSession(void)
{
    struct CoopBridgeMessage message;

    InitTestBridge();
    PopInitialRomReady();
    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  1,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
}

static void StartTestCheckpoint(void)
{
    struct CoopBridgeMessage message;

    EXPECT_EQ(CoopNetBridge_RequestCheckpoint(), COOP_CHECKPOINT_REQUEST_STARTED);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_WAITING_FOR_GRANT);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_CHECKPOINT_READY);
    EXPECT_EQ(message.length, 0);
    EXPECT_EQ(message.session_epoch, 17);
}

static void DeliverTestGrant(u32 sequence, u32 epoch, u16 payloadSize)
{
    struct CoopBridgeMessage message;
    u8 payload = 0xA5;

    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_CHECKPOINT_GRANTED,
                                  sequence,
                                  epoch,
                                  &payload,
                                  payloadSize));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
}

TEST("Cloud Coop checkpoint request is online-only and requires drained queues")
{
    struct CoopBridgeMessage message;

    InitTestBridge();
    EXPECT_EQ(CoopNetBridge_RequestCheckpoint(), COOP_CHECKPOINT_REQUEST_OFFLINE);

    EstablishTestCloudSession();
    EXPECT(CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_PLAYER_STATE, NULL, 0));
    EXPECT_EQ(CoopNetBridge_RequestCheckpoint(), COOP_CHECKPOINT_REQUEST_REJECTED);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(CoopNetBridge_RequestCheckpoint(), COOP_CHECKPOINT_REQUEST_STARTED);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_WAITING_FOR_GRANT);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
}

TEST("Cloud Coop checkpoint grant accepts only a fresh empty current epoch")
{
    struct CoopBridgeMessage message;

    EstablishTestCloudSession();
    StartTestCheckpoint();

    /* The session-ready sequence is stale, and a different epoch is never a
     * grant for the pending request. Neither may authorize a flash write. */
    DeliverTestGrant(1, 17, 0);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_WAITING_FOR_GRANT);
    DeliverTestGrant(2, 99, 0);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_WAITING_FOR_GRANT);

    /* A nonempty current-epoch grant is malformed and is not freshened. */
    DeliverTestGrant(2, 17, 1);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_WAITING_FOR_GRANT);
    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR);

    DeliverTestGrant(2, 17, 0);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_GRANTED);
    EXPECT(!CoopNetBridge_IsCheckpointAuthorizedForSave());
    EXPECT(CoopNetBridge_ConsumeCheckpointGrant());
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_SAVING);
    EXPECT(CoopNetBridge_IsCheckpointAuthorizedForSave());

    /* A second consume cannot cause the normal save callback to run twice. */
    EXPECT(!CoopNetBridge_ConsumeCheckpointGrant());
    (void)message;
}

TEST("Cloud Coop checkpoint timeout never enters the save state")
{
    struct CoopBridgeMessage message;
    u32 frame;

    EstablishTestCloudSession();
    StartTestCheckpoint();
    for (frame = 0; frame < COOP_NET_BRIDGE_CHECKPOINT_TIMEOUT_FRAMES; frame++)
    {
        gCoopNetBridge.last_sidecar_heartbeat++;
        CoopNetBridge_Poll();
    }

    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    EXPECT(CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_PLAYER_STATE, NULL, 0));
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_PLAYER_STATE);
}

TEST("Cloud Coop epoch and heartbeat changes cancel a pending checkpoint")
{
    struct CoopBridgeMessage message;
    u32 frame;

    EstablishTestCloudSession();
    StartTestCheckpoint();
    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  2,
                                  18,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);

    EstablishTestCloudSession();
    StartTestCheckpoint();
    for (frame = 0; frame < COOP_NET_BRIDGE_SIDECAR_STALE_INTERVAL; frame++)
        CoopNetBridge_Poll();
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_OFFLINE);
    EXPECT(!CoopNetBridge_IsCheckpointAuthorizedForSave());
    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SESSION_READY));
    EXPECT(CoopNetBridge_RequestCheckpoint() == COOP_CHECKPOINT_REQUEST_REJECTED);
}

TEST("Cloud Coop successful save emits one generation update and failure emits none")
{
    struct CoopBridgeMessage message;

    EstablishTestCloudSession();
    StartTestCheckpoint();
    DeliverTestGrant(2, 17, 0);
    EXPECT(CoopNetBridge_ConsumeCheckpointGrant());
    CoopNetBridge_NotifySaveResult(FALSE);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));

    StartTestCheckpoint();
    DeliverTestGrant(3, 17, 0);
    EXPECT(CoopNetBridge_ConsumeCheckpointGrant());
    CoopNetBridge_NotifySaveResult(TRUE);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_SAVING);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_SAVE_DATA_UPDATED);
    EXPECT_EQ(message.length, sizeof(u32));
    EXPECT_EQ(message.payload[0], 0);
    EXPECT_EQ(message.payload[1], 0);
    EXPECT_EQ(message.payload[2], 0);
    EXPECT_EQ(message.payload[3], 0);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    CoopNetBridge_NotifySaveResult(TRUE);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
}

TEST("Cloud Coop retries a full critical save update without consuming sequence")
{
    struct CoopBridgeMessage message;
    u16 sequenceBefore;
    u32 i;

    EstablishTestCloudSession();
    StartTestCheckpoint();
    DeliverTestGrant(2, 17, 0);
    EXPECT(CoopNetBridge_ConsumeCheckpointGrant());
    for (i = 0; i < COOP_NET_BRIDGE_QUEUE_CAPACITY; i++)
        EXPECT(CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_ROM_READY, NULL, 0));

    sequenceBefore = gCoopNetBridge.game_to_network.entries[
        (gCoopNetBridge.game_to_network.write_index - 1)
        & (COOP_NET_BRIDGE_QUEUE_CAPACITY - 1)].sequence;
    CoopNetBridge_NotifySaveResult(TRUE);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_SAVING);
    EXPECT(CoopBridgeQueue_IsFull(&gCoopNetBridge.game_to_network));
    EXPECT_EQ(gCoopNetBridge.game_to_network.entries[
        (gCoopNetBridge.game_to_network.write_index - 1)
        & (COOP_NET_BRIDGE_QUEUE_CAPACITY - 1)].sequence, sequenceBefore);

    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    CoopNetBridge_Poll();
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_SAVING);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    while (!CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network))
    {
        EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
        if (message.type == COOP_BRIDGE_MESSAGE_SAVE_DATA_UPDATED)
            break;
    }
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_SAVE_DATA_UPDATED);
    EXPECT_EQ(message.length, sizeof(u32));
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
}

TEST("Cloud Coop retains a pending update across same-epoch heartbeat recovery")
{
    struct CoopBridgeMessage message;
    u32 frame;
    u32 updates = 0;

    EstablishTestCloudSession();
    StartTestCheckpoint();
    DeliverTestGrant(2, 17, 0);
    EXPECT(CoopNetBridge_ConsumeCheckpointGrant());
    for (frame = 0; frame < COOP_NET_BRIDGE_QUEUE_CAPACITY; frame++)
        EXPECT(CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_ROM_READY, NULL, 0));
    CoopNetBridge_NotifySaveResult(TRUE);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_SAVING);

    for (frame = 0; frame < COOP_NET_BRIDGE_SIDECAR_STALE_INTERVAL; frame++)
        CoopNetBridge_Poll();
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_SAVING);
    EXPECT(!CoopNetBridge_IsRecoveryRequired());
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));

    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  3,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_SAVING);

    while (!CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network))
    {
        EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
        if (message.type == COOP_BRIDGE_MESSAGE_SAVE_DATA_UPDATED)
        {
            updates++;
            EXPECT_EQ(message.session_epoch, 17);
            EXPECT_EQ(message.length, sizeof(u32));
        }
    }
    EXPECT_EQ(updates, 1);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
}

TEST("Cloud Coop preserves an enqueued undrained update across same-epoch heartbeat recovery")
{
    struct CoopBridgeMessage message;
    u32 frame;
    u32 updates = 0;

    EstablishTestCloudSession();
    StartTestCheckpoint();
    DeliverTestGrant(2, 17, 0);
    EXPECT(CoopNetBridge_ConsumeCheckpointGrant());
    CoopNetBridge_NotifySaveResult(TRUE);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_SAVING);

    /* The update was accepted into the FIFO, but the sidecar has not
     * advanced read_index yet. A stale reset must make it retryable. */
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.network_to_game));
    for (frame = 0; frame < COOP_NET_BRIDGE_SIDECAR_STALE_INTERVAL; frame++)
        CoopNetBridge_Poll();
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_SAVING);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));

    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  3,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_SAVING);

    while (!CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network))
    {
        EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
        if (message.type == COOP_BRIDGE_MESSAGE_SAVE_DATA_UPDATED)
        {
            updates++;
            EXPECT_EQ(message.session_epoch, 17);
            EXPECT_EQ(message.length, sizeof(u32));
        }
    }
    EXPECT_EQ(updates, 1);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
}

TEST("Cloud Coop does not duplicate a drained update after heartbeat recovery")
{
    struct CoopBridgeMessage message;
    u32 frame;
    u32 updates = 0;

    EstablishTestCloudSession();
    StartTestCheckpoint();
    DeliverTestGrant(2, 17, 0);
    EXPECT(CoopNetBridge_ConsumeCheckpointGrant());
    CoopNetBridge_NotifySaveResult(TRUE);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_SAVE_DATA_UPDATED);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);

    for (frame = 0; frame < COOP_NET_BRIDGE_SIDECAR_STALE_INTERVAL; frame++)
        CoopNetBridge_Poll();
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_OFFLINE);

    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  3,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    while (!CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network))
    {
        EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
        if (message.type == COOP_BRIDGE_MESSAGE_SAVE_DATA_UPDATED)
            updates++;
    }
    EXPECT_EQ(updates, 0);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
}

TEST("Cloud Coop malformed consumer index cannot acknowledge a critical update")
{
    EstablishTestCloudSession();
    StartTestCheckpoint();
    DeliverTestGrant(2, 17, 0);
    EXPECT(CoopNetBridge_ConsumeCheckpointGrant());
    CoopNetBridge_NotifySaveResult(TRUE);

    /* The sidecar owns read_index. An impossible depth must not be treated as
     * proof that the queued SAVE_DATA_UPDATED was consumed. */
    gCoopNetBridge.game_to_network.read_index =
        gCoopNetBridge.game_to_network.write_index
        + COOP_NET_BRIDGE_QUEUE_CAPACITY + 1;
    CoopNetBridge_Poll();

    EXPECT(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_QUEUE_ERROR);
    EXPECT(CoopNetBridge_IsRecoveryRequired());
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_RECOVERY_REQUIRED);
    EXPECT_EQ(CoopNetBridge_RequestCheckpoint(), COOP_CHECKPOINT_REQUEST_REJECTED);
}

TEST("Cloud Coop malformed consumer index without a critical update stays idle")
{
    EstablishTestCloudSession();
    gCoopNetBridge.game_to_network.read_index =
        gCoopNetBridge.game_to_network.write_index
        + COOP_NET_BRIDGE_QUEUE_CAPACITY + 1;
    CoopNetBridge_Poll();

    EXPECT(!(gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_QUEUE_ERROR));
    EXPECT(!CoopNetBridge_IsRecoveryRequired());
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
}

TEST("Cloud Coop enters explicit recovery when a pending update crosses epochs")
{
    struct CoopBridgeMessage message;
    u32 frame;

    EstablishTestCloudSession();
    StartTestCheckpoint();
    DeliverTestGrant(2, 17, 0);
    EXPECT(CoopNetBridge_ConsumeCheckpointGrant());
    for (frame = 0; frame < COOP_NET_BRIDGE_QUEUE_CAPACITY; frame++)
        EXPECT(CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_ROM_READY, NULL, 0));
    CoopNetBridge_NotifySaveResult(TRUE);

    for (frame = 0; frame < COOP_NET_BRIDGE_SIDECAR_STALE_INTERVAL; frame++)
        CoopNetBridge_Poll();
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_SAVING);

    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  3,
                                  18,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_RECOVERY_REQUIRED);
    EXPECT(CoopNetBridge_IsRecoveryRequired());
    EXPECT_EQ(CoopNetBridge_RequestCheckpoint(), COOP_CHECKPOINT_REQUEST_REJECTED);
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
}

TEST("Cloud Coop production save callback waits for grant after confirmation")
{
    EstablishTestCloudSession();
    CoopStartMenu_TestSetSaveDryRun(TRUE);
    CoopStartMenu_TestSetCheckpointRequired(TRUE);

    /* SaveDoSaveCallback is the first production callback after the existing
     * Yes/No and overwrite prompts. It must wait instead of reaching flash. */
    EXPECT_EQ(CoopStartMenu_TestRunSaveSavingMessageCallback(), COOP_START_MENU_TEST_SAVE_IN_PROGRESS);
    EXPECT_EQ(CoopStartMenu_TestRunSaveDoSaveCallback(), COOP_START_MENU_TEST_SAVE_IN_PROGRESS);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_WAITING_FOR_GRANT);

    {
        struct CoopBridgeMessage message;
        EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
        EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_CHECKPOINT_READY);
    }

    DeliverTestGrant(2, 17, 0);
    EXPECT_EQ(CoopStartMenu_TestRunCheckpointWaitCallback(), COOP_START_MENU_TEST_SAVE_SUCCESS);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_SAVING);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    CoopStartMenu_TestSetSaveDryRun(FALSE);
}

TEST("Cloud Coop rejected production save callback returns through interactive recovery")
{
    u32 i;

    EstablishTestCloudSession();
    for (i = 0; i < COOP_NET_BRIDGE_QUEUE_CAPACITY; i++)
        EXPECT(CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_ROM_READY, NULL, 0));

    CoopStartMenu_TestSetSaveDryRun(TRUE);
    CoopStartMenu_TestSetCheckpointRequired(TRUE);
    EXPECT_EQ(CoopStartMenu_TestRunSaveSavingMessageCallback(), COOP_START_MENU_TEST_SAVE_IN_PROGRESS);
    EXPECT_EQ(CoopStartMenu_TestRunSaveDoSaveCallback(), COOP_START_MENU_TEST_SAVE_IN_PROGRESS);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
    EXPECT_EQ(CoopStartMenu_TestRunCheckpointAbortCallback(), COOP_START_MENU_TEST_SAVE_CANCELED);
    CoopStartMenu_TestSetSaveDryRun(FALSE);
}

TEST("Cloud Coop production save callback fails closed before TrySavingData on auth loss")
{
    struct CoopBridgeMessage message;
    u32 frame;

    EstablishTestCloudSession();
    CoopStartMenu_TestSetSaveDryRun(TRUE);
    CoopStartMenu_TestSetCheckpointRequired(TRUE);
    EXPECT_EQ(CoopStartMenu_TestRunSaveSavingMessageCallback(), COOP_START_MENU_TEST_SAVE_IN_PROGRESS);
    EXPECT_EQ(CoopStartMenu_TestRunSaveDoSaveCallback(), COOP_START_MENU_TEST_SAVE_IN_PROGRESS);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_CHECKPOINT_READY);
    DeliverTestGrant(2, 17, 0);
    EXPECT(CoopNetBridge_ConsumeCheckpointGrant());

    for (frame = 0; frame < COOP_NET_BRIDGE_SIDECAR_STALE_INTERVAL; frame++)
        CoopNetBridge_Poll();
    EXPECT(!CoopNetBridge_IsCheckpointAuthorizedForSave());
    EXPECT_EQ(CoopStartMenu_TestRunAuthorizedSaveCallback(), COOP_START_MENU_TEST_SAVE_IN_PROGRESS);
    EXPECT_EQ(CoopStartMenu_TestRunCheckpointAbortCallback(), COOP_START_MENU_TEST_SAVE_CANCELED);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    CoopNetBridge_NotifySaveResult(FALSE);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
    CoopStartMenu_TestSetSaveDryRun(FALSE);
    (void)message;
}

TEST("Cloud Coop SaveFailedScreen retries cannot bypass a revoked checkpoint")
{
    u16 (*programFlashSector)(u16, u8 *) = ProgramFlashSector;
    bool32 flashMemoryPresent = gFlashMemoryPresent;

    EstablishTestCloudSession();
    StartTestCheckpoint();
    DeliverTestGrant(2, 17, 0);
    EXPECT(CoopNetBridge_ConsumeCheckpointGrant());

    /* Simulate a record becoming invalid after negotiation and after the
     * grant was consumed. The cloud epoch remains sticky, but authorization
     * is revoked and the normal callback reports the failed save. */
    gSaveBlock3Ptr->coop.trainer_bits[COOP_SAVE_TRAINER_BITS_SIZE - 1] = 0x80;
    EXPECT(!CoopSave_Seal(&gSaveBlock3Ptr->coop));
    EXPECT(CoopNetBridge_IsCloudMode());
    CoopNetBridge_NotifySaveResult(FALSE);

    sSaveSectorProgramCalls = 0;
    ProgramFlashSector = CountSaveSectorProgramCalls;
    gFlashMemoryPresent = TRUE;
    HandleSavingData(SAVE_NORMAL);
    EXPECT_EQ(sSaveSectorProgramCalls, 0);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);

    ProgramFlashSector = programFlashSector;
    gFlashMemoryPresent = flashMemoryPresent;
}

TEST("Cloud Coop forced save callback bypasses checkpoint negotiation")
{
    EstablishTestCloudSession();
    CoopStartMenu_TestSetSaveDryRun(TRUE);
    CoopStartMenu_TestSetCheckpointRequired(FALSE);

    EXPECT_EQ(CoopStartMenu_TestRunSaveSavingMessageCallback(), COOP_START_MENU_TEST_SAVE_IN_PROGRESS);
    EXPECT_EQ(CoopStartMenu_TestRunAuthorizedSaveCallback(), COOP_START_MENU_TEST_SAVE_SUCCESS);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    CoopStartMenu_TestSetSaveDryRun(FALSE);
}
