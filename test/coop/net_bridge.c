#include <stddef.h>

#include "global.h"
#include "coop/net_bridge.h"
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
_Static_assert(sizeof(struct CoopBridgePlayerState) == 16, "tested player-state ABI size");

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

static void HostWriteInboundUnchecked(const struct CoopBridgeMessage *message)
{
    struct CoopBridgeQueue *queue = &gCoopNetBridge.network_to_game;
    u16 index = queue->write_index & (COOP_NET_BRIDGE_QUEUE_CAPACITY - 1);

    queue->entries[index] = *message;
    queue->write_index++;
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
    EXPECT_EQ(sizeof(struct CoopBridgePlayerState), 16);
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
    CoopNetBridge_Init();
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

    CoopNetBridge_Init();
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

    CoopNetBridge_Init();
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

    CoopNetBridge_Init();
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

    CoopNetBridge_Init();
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
