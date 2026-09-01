#include "global.h"
#include "coop/net_bridge.h"
#include "coop/progress.h"

ALIGNED(4) EWRAM_DATA struct CoopNetBridge gCoopNetBridge = {0};

struct CoopNetRuntime
{
    u32 session_epoch;
    u32 tx_sequence;
    u32 rx_sequence;
    u32 frame_counter;
    u32 last_player_state_frame;
    u32 observed_sidecar_heartbeat;
    u32 observed_sidecar_heartbeat_frame;
};

static EWRAM_DATA struct CoopNetRuntime sCoopNetRuntime = {0};

/* mGBA pauses the emulated CPU while Lua touches EWRAM. These barriers keep
 * the compiler from moving entry accesses across the volatile queue index
 * publication points used by that turn-taking protocol. */
#define COOP_BRIDGE_MEMORY_BARRIER() __asm__ volatile ("" ::: "memory")

static bool8 IsOutboundMessageType(u16 type)
{
    return type >= COOP_BRIDGE_MESSAGE_ROM_READY
        && type <= COOP_BRIDGE_MESSAGE_SAVE_DATA_UPDATED;
}

static bool8 IsInboundMessageType(u16 type)
{
    return type >= COOP_BRIDGE_MESSAGE_SESSION_READY
        && type <= COOP_BRIDGE_MESSAGE_CHECKPOINT_GRANTED;
}

static bool8 IsKnownMessageType(u16 type)
{
    return IsOutboundMessageType(type) || IsInboundMessageType(type);
}

static u32 Crc32Update(u32 crc, const u8 *bytes, u32 length)
{
    u32 i;

    for (i = 0; i < length; i++)
    {
        u32 bit;

        crc ^= bytes[i];
        for (bit = 0; bit < 8; bit++)
        {
            if (crc & 1)
                crc = (crc >> 1) ^ 0xEDB88320;
            else
                crc >>= 1;
        }
    }
    return crc;
}

u32 CoopBridge_Crc32(const void *data, u32 length)
{
    if (data == NULL && length != 0)
        return 0;
    return ~Crc32Update(0xFFFFFFFF, data, length);
}

u32 CoopBridgeMessage_ComputeChecksum(const struct CoopBridgeMessage *message)
{
    if (message == NULL)
        return 0;

    return CoopBridge_Crc32(message, offsetof(struct CoopBridgeMessage, checksum));
}

bool8 CoopBridgeMessage_Seal(struct CoopBridgeMessage *message, u16 type,
                             u32 sequence, u32 session_epoch,
                             const void *payload, u16 payload_size)
{
    if (message == NULL
     || !IsKnownMessageType(type)
     || payload_size > COOP_NET_BRIDGE_PAYLOAD_SIZE
     || (payload_size != 0 && payload == NULL)
     || sequence == 0)
        return FALSE;

    memset(message, 0, sizeof(*message));
    message->type = type;
    message->length = payload_size;
    message->sequence = sequence;
    message->session_epoch = session_epoch;
    if (payload_size != 0)
        memcpy(message->payload, payload, payload_size);
    message->checksum = CoopBridgeMessage_ComputeChecksum(message);
    return TRUE;
}

bool8 CoopBridgeMessage_Validate(const struct CoopBridgeMessage *message)
{
    if (message == NULL
     || !IsKnownMessageType(message->type)
     || message->sequence == 0
     || message->length > COOP_NET_BRIDGE_PAYLOAD_SIZE)
        return FALSE;

    return message->checksum == CoopBridgeMessage_ComputeChecksum(message);
}

void CoopBridgeQueue_Init(struct CoopBridgeQueue *queue)
{
    if (queue == NULL)
        return;

    queue->read_index = 0;
    queue->write_index = 0;
    memset(queue->entries, 0, sizeof(queue->entries));
}

static bool8 CoopBridgeQueue_TryGetDepth(const struct CoopBridgeQueue *queue, u16 *depth)
{
    u16 candidate;

    if (queue == NULL || depth == NULL)
        return FALSE;

    candidate = (u16)(queue->write_index - queue->read_index);
    COOP_BRIDGE_MEMORY_BARRIER();
    if (candidate > COOP_NET_BRIDGE_QUEUE_CAPACITY)
        return FALSE;

    *depth = candidate;
    return TRUE;
}

bool8 CoopBridgeQueue_IsEmpty(const struct CoopBridgeQueue *queue)
{
    u16 depth;

    return !CoopBridgeQueue_TryGetDepth(queue, &depth) || depth == 0;
}

bool8 CoopBridgeQueue_IsFull(const struct CoopBridgeQueue *queue)
{
    u16 depth;

    return !CoopBridgeQueue_TryGetDepth(queue, &depth) || depth == COOP_NET_BRIDGE_QUEUE_CAPACITY;
}

bool8 CoopBridgeQueue_Push(struct CoopBridgeQueue *queue, const struct CoopBridgeMessage *message)
{
    u16 index;

    if (queue == NULL || message == NULL || !CoopBridgeMessage_Validate(message)
     || CoopBridgeQueue_IsFull(queue))
        return FALSE;

    index = queue->write_index & (COOP_NET_BRIDGE_QUEUE_CAPACITY - 1);
    queue->entries[index] = *message;
    COOP_BRIDGE_MEMORY_BARRIER();
    queue->write_index++;
    return TRUE;
}

static bool8 CoopBridgeQueue_PopUnchecked(struct CoopBridgeQueue *queue, struct CoopBridgeMessage *message)
{
    u16 index;
    u16 depth;

    if (queue == NULL || message == NULL
     || !CoopBridgeQueue_TryGetDepth(queue, &depth) || depth == 0)
        return FALSE;

    index = queue->read_index & (COOP_NET_BRIDGE_QUEUE_CAPACITY - 1);
    COOP_BRIDGE_MEMORY_BARRIER();
    *message = queue->entries[index];
    COOP_BRIDGE_MEMORY_BARRIER();
    queue->read_index++;
    return TRUE;
}

bool8 CoopBridgeQueue_Pop(struct CoopBridgeQueue *queue, struct CoopBridgeMessage *message)
{
    if (!CoopBridgeQueue_PopUnchecked(queue, message))
        return FALSE;
    return CoopBridgeMessage_Validate(message);
}

static bool8 CoopBridgeQueue_ReplaceTailType(struct CoopBridgeQueue *queue,
                                             const struct CoopBridgeMessage *message)
{
    u16 depth;
    u16 index;

    if (queue == NULL || message == NULL
     || !CoopBridgeQueue_TryGetDepth(queue, &depth))
        return FALSE;

    if (depth == 0)
        return FALSE;

    /* Only the FIFO tail can be replaced without moving a newer sequence in
     * front of an intervening critical message. */
    index = (queue->write_index - 1) & (COOP_NET_BRIDGE_QUEUE_CAPACITY - 1);
    if (queue->entries[index].type != message->type)
        return FALSE;

    queue->entries[index] = *message;
    COOP_BRIDGE_MEMORY_BARRIER();
    return TRUE;
}

static bool8 IsSequenceNewer(u32 sequence, u32 previous)
{
    if (sequence == 0)
        return FALSE;
    if (previous == 0)
        return TRUE;
    return (s32)(sequence - previous) > 0;
}

static void AdvanceTxSequence(void)
{
    sCoopNetRuntime.tx_sequence++;
    if (sCoopNetRuntime.tx_sequence == 0)
        sCoopNetRuntime.tx_sequence = 1;
}

bool8 CoopNetBridge_EnqueueGameToNetwork(u16 type, const void *payload, u16 payload_size)
{
    struct CoopBridgeMessage message;

    if ((gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_INITIALIZED) == 0
     || !IsOutboundMessageType(type)
     || !CoopBridgeMessage_Seal(&message, type, sCoopNetRuntime.tx_sequence,
                                sCoopNetRuntime.session_epoch, payload, payload_size))
        return FALSE;

    if (type == COOP_BRIDGE_MESSAGE_PLAYER_STATE
     && CoopBridgeQueue_ReplaceTailType(&gCoopNetBridge.game_to_network, &message))
    {
        gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_QUEUE_CONGESTED;
        AdvanceTxSequence();
        return TRUE;
    }

    if (!CoopBridgeQueue_Push(&gCoopNetBridge.game_to_network, &message))
    {
        u16 depth;

        if (CoopBridgeQueue_TryGetDepth(&gCoopNetBridge.game_to_network, &depth)
         && depth == COOP_NET_BRIDGE_QUEUE_CAPACITY)
        {
            gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_QUEUE_CONGESTED;
            /* Position is a latest-value stream. If a critical tail prevents
             * order-safe coalescing, drop this sample and try on schedule. */
            return type == COOP_BRIDGE_MESSAGE_PLAYER_STATE;
        }
        gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_QUEUE_ERROR;
        return FALSE;
    }

    AdvanceTxSequence();
    return TRUE;
}

bool8 CoopNetBridge_DequeueGameToNetwork(struct CoopBridgeMessage *message)
{
    return CoopBridgeQueue_Pop(&gCoopNetBridge.game_to_network, message);
}

bool8 CoopNetBridge_EnqueueNetworkToGame(const struct CoopBridgeMessage *message)
{
    if ((gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_INITIALIZED) == 0
     || message == NULL
     || !IsInboundMessageType(message->type))
    {
        gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
        return FALSE;
    }
    if (!CoopBridgeQueue_Push(&gCoopNetBridge.network_to_game, message))
    {
        gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_QUEUE_ERROR;
        return FALSE;
    }
    return TRUE;
}

bool8 CoopNetBridge_DequeueNetworkToGame(struct CoopBridgeMessage *message)
{
    return CoopBridgeQueue_Pop(&gCoopNetBridge.network_to_game, message);
}

static bool8 SendPlayerState(void)
{
    struct CoopBridgePlayerState state;

    sCoopNetRuntime.last_player_state_frame = sCoopNetRuntime.frame_counter;
    memset(&state, 0, sizeof(state));
    if (!CoopWorldLocation_Export(&state.location))
    {
        gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_WORLD_NOT_READY;
        return TRUE;
    }
    gCoopNetBridge.status_flags &= ~COOP_BRIDGE_STATUS_WORLD_NOT_READY;
    state.frame_counter = sCoopNetRuntime.frame_counter;
    if (!CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_PLAYER_STATE,
                                            &state, sizeof(state)))
        return FALSE;
    gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_PLAYER_STATE_SENT;
    return TRUE;
}

/* Returns TRUE when a new epoch resets both queues. */
static bool8 ProcessInboundMessage(const struct CoopBridgeMessage *message)
{
    if (message == NULL
     || message->sequence == 0
     || message->length > COOP_NET_BRIDGE_PAYLOAD_SIZE)
    {
        gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
        return FALSE;
    }

    if (message->checksum != CoopBridgeMessage_ComputeChecksum(message))
    {
        gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_CHECKSUM_ERROR;
        return FALSE;
    }

    if (!IsInboundMessageType(message->type))
    {
        gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
        return FALSE;
    }

    if (message->type == COOP_BRIDGE_MESSAGE_SESSION_READY)
    {
        if (message->length != 0)
        {
            gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
            return FALSE;
        }

        if (message->session_epoch == 0)
            return FALSE;

        if (message->session_epoch == sCoopNetRuntime.session_epoch)
        {
            if ((gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SESSION_READY) != 0)
            {
                if (IsSequenceNewer(message->sequence, sCoopNetRuntime.rx_sequence))
                    sCoopNetRuntime.rx_sequence = message->sequence;
                return FALSE;
            }

            /* Re-arm a disconnected transport without rewinding either
             * sequence domain inside the still-current epoch. */
            CoopBridgeQueue_Init(&gCoopNetBridge.game_to_network);
            CoopBridgeQueue_Init(&gCoopNetBridge.network_to_game);
            if (IsSequenceNewer(message->sequence, sCoopNetRuntime.rx_sequence))
                sCoopNetRuntime.rx_sequence = message->sequence;
            sCoopNetRuntime.last_player_state_frame = 0;
            sCoopNetRuntime.observed_sidecar_heartbeat = gCoopNetBridge.last_sidecar_heartbeat;
            sCoopNetRuntime.observed_sidecar_heartbeat_frame = sCoopNetRuntime.frame_counter;
            gCoopNetBridge.status_flags &= ~(COOP_BRIDGE_STATUS_QUEUE_CONGESTED
                                          | COOP_BRIDGE_STATUS_QUEUE_ERROR
                                          | COOP_BRIDGE_STATUS_CHECKSUM_ERROR
                                          | COOP_BRIDGE_STATUS_SIDECAR_HEARTBEAT_STALE);
            gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_SESSION_READY
                                        | COOP_BRIDGE_STATUS_SIDECAR_HEARTBEAT_SEEN;
            if (CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_ROM_READY, NULL, 0))
                gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_ROM_READY_SENT;
            return TRUE;
        }
        else if (sCoopNetRuntime.session_epoch != 0
              && !IsSequenceNewer(message->session_epoch, sCoopNetRuntime.session_epoch))
            return FALSE;

        /* A restored savestate receives a new epoch. Drop every stale queued
         * message before publishing any state for the replacement session. */
        CoopBridgeQueue_Init(&gCoopNetBridge.game_to_network);
        CoopBridgeQueue_Init(&gCoopNetBridge.network_to_game);
        sCoopNetRuntime.session_epoch = message->session_epoch;
        sCoopNetRuntime.rx_sequence = message->sequence;
        sCoopNetRuntime.tx_sequence = 1;
        sCoopNetRuntime.last_player_state_frame = 0;
        sCoopNetRuntime.observed_sidecar_heartbeat = gCoopNetBridge.last_sidecar_heartbeat;
        sCoopNetRuntime.observed_sidecar_heartbeat_frame = sCoopNetRuntime.frame_counter;
        gCoopNetBridge.status_flags &= ~(COOP_BRIDGE_STATUS_QUEUE_CONGESTED
                                      | COOP_BRIDGE_STATUS_QUEUE_ERROR
                                      | COOP_BRIDGE_STATUS_CHECKSUM_ERROR
                                      | COOP_BRIDGE_STATUS_SIDECAR_HEARTBEAT_STALE);
        gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_SESSION_READY
                                    | COOP_BRIDGE_STATUS_SIDECAR_HEARTBEAT_SEEN;
        if (CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_ROM_READY, NULL, 0))
            gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_ROM_READY_SENT;
        return TRUE;
    }

    /* Later milestones add handlers for the remaining documented inbound
     * messages. Until then, do not advance the receive sequence for one. */
    gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
    return FALSE;
}

static void ObserveSidecarHeartbeat(void)
{
    u32 heartbeat = gCoopNetBridge.last_sidecar_heartbeat;

    if (heartbeat != sCoopNetRuntime.observed_sidecar_heartbeat)
    {
        sCoopNetRuntime.observed_sidecar_heartbeat = heartbeat;
        sCoopNetRuntime.observed_sidecar_heartbeat_frame = sCoopNetRuntime.frame_counter;
        gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_SIDECAR_HEARTBEAT_SEEN;
        gCoopNetBridge.status_flags &= ~COOP_BRIDGE_STATUS_SIDECAR_HEARTBEAT_STALE;
    }
    else if ((gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SIDECAR_HEARTBEAT_SEEN) != 0
          && sCoopNetRuntime.frame_counter - sCoopNetRuntime.observed_sidecar_heartbeat_frame
             >= COOP_NET_BRIDGE_SIDECAR_STALE_INTERVAL)
    {
        if ((gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SIDECAR_HEARTBEAT_STALE) == 0)
        {
            CoopBridgeQueue_Init(&gCoopNetBridge.game_to_network);
            CoopBridgeQueue_Init(&gCoopNetBridge.network_to_game);
            gCoopNetBridge.status_flags &= ~COOP_BRIDGE_STATUS_SESSION_READY;
            gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_SIDECAR_HEARTBEAT_STALE;
        }
    }
}

void CoopNetBridge_Init(void)
{
    memset(&gCoopNetBridge, 0, sizeof(gCoopNetBridge));
    memset(&sCoopNetRuntime, 0, sizeof(sCoopNetRuntime));
    CoopProgress_Init(&gCoopProgress);
    CoopBridgeQueue_Init(&gCoopNetBridge.game_to_network);
    CoopBridgeQueue_Init(&gCoopNetBridge.network_to_game);
    gCoopNetBridge.magic = COOP_NET_BRIDGE_MAGIC;
    gCoopNetBridge.abi_version = COOP_NET_BRIDGE_ABI_VERSION;
    gCoopNetBridge.game_protocol_version = COOP_NET_BRIDGE_GAME_PROTOCOL_VERSION;
    gCoopNetBridge.game_build_id = COOP_NET_BRIDGE_GAME_BUILD_ID;
    sCoopNetRuntime.tx_sequence = 1;
    gCoopNetBridge.status_flags = COOP_BRIDGE_STATUS_INITIALIZED;

    if (CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_ROM_READY, NULL, 0))
        gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_ROM_READY_SENT;
}

void CoopNetBridge_Poll(void)
{
    struct CoopBridgeMessage message;
    u16 inbound_count;
    u16 inbound_depth;

    if ((gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_INITIALIZED) == 0)
        return;

    sCoopNetRuntime.frame_counter++;
    ObserveSidecarHeartbeat();

    if (!CoopBridgeQueue_TryGetDepth(&gCoopNetBridge.network_to_game, &inbound_depth))
    {
        /* The host owns write_index. Fail closed and discard an impossible
         * queue state instead of replaying overwritten entries or stalling. */
        gCoopNetBridge.network_to_game.read_index = gCoopNetBridge.network_to_game.write_index;
        gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_QUEUE_ERROR;
    }
    else
    {
        for (inbound_count = 0; inbound_count < inbound_depth; inbound_count++)
        {
            if (!CoopBridgeQueue_PopUnchecked(&gCoopNetBridge.network_to_game, &message))
            {
                gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_QUEUE_ERROR;
                break;
            }
            if (ProcessInboundMessage(&message))
                break;
        }
    }

    if ((gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SESSION_READY) == 0)
        return;

    if (sCoopNetRuntime.last_player_state_frame == 0
     || sCoopNetRuntime.frame_counter - sCoopNetRuntime.last_player_state_frame
        >= COOP_NET_BRIDGE_PLAYER_STATE_INTERVAL)
    {
        if (!SendPlayerState())
            gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_QUEUE_ERROR;
    }
}
