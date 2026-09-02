#include "global.h"
#include "coop/net_bridge.h"
#include "coop/progress.h"
#include "coop/save.h"

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
    u32 checkpoint_started_frame;
    u32 save_update_epoch;
    u32 save_update_generation;
    u16 save_update_queue_next_index;
    enum CoopCheckpointState checkpoint_state;
    bool8 cloud_epoch_accepted;
    bool8 save_data_update_pending;
    bool8 save_data_update_queued;
    bool8 flash_save_started;
    bool8 recovery_required;
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

static bool8 IsEmptyPayload(const struct CoopBridgeMessage *message)
{
    u16 i;

    if (message == NULL)
        return FALSE;
    for (i = 0; i < COOP_NET_BRIDGE_PAYLOAD_SIZE; i++)
    {
        if (message->payload[i] != 0)
            return FALSE;
    }
    return TRUE;
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

static bool8 IsCloudSessionActive(void)
{
    return sCoopNetRuntime.cloud_epoch_accepted
        && sCoopNetRuntime.session_epoch != 0
        && CoopSave_IsOnlineEnabled()
        && (gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SESSION_READY) != 0
        && (gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SIDECAR_HEARTBEAT_STALE) == 0;
}

static void TryAnnounceRomReady(void)
{
    if ((gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_INITIALIZED) == 0
     || (gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_ROM_READY_SENT) != 0
     || !CoopSave_IsOnlineEnabled())
        return;

    if (CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_ROM_READY, NULL, 0))
        gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_ROM_READY_SENT;
}

static void CancelCheckpointAuthorization(void)
{
    sCoopNetRuntime.checkpoint_started_frame = 0;
    /* A completed flash save remains SAVING until its critical update is
     * acknowledged, and an epoch mismatch remains explicit recovery. Neither
     * state may be downgraded merely because transport went stale. */
    if (sCoopNetRuntime.checkpoint_state == COOP_CHECKPOINT_STATE_SAVING
     || sCoopNetRuntime.checkpoint_state == COOP_CHECKPOINT_STATE_RECOVERY_REQUIRED)
        return;

    if (IsCloudSessionActive())
        sCoopNetRuntime.checkpoint_state = COOP_CHECKPOINT_STATE_IDLE;
    else
        sCoopNetRuntime.checkpoint_state = COOP_CHECKPOINT_STATE_OFFLINE;
}

static void SetCheckpointStateForAcceptedEpoch(void)
{
    sCoopNetRuntime.checkpoint_started_frame = 0;
    if (sCoopNetRuntime.recovery_required)
    {
        sCoopNetRuntime.checkpoint_state = COOP_CHECKPOINT_STATE_RECOVERY_REQUIRED;
        return;
    }

    if (sCoopNetRuntime.save_data_update_pending
     || sCoopNetRuntime.save_data_update_queued)
    {
        if (sCoopNetRuntime.save_update_epoch == sCoopNetRuntime.session_epoch)
        {
            sCoopNetRuntime.recovery_required = FALSE;
            sCoopNetRuntime.checkpoint_state = COOP_CHECKPOINT_STATE_SAVING;
        }
        else
        {
            sCoopNetRuntime.recovery_required = TRUE;
            sCoopNetRuntime.checkpoint_state = COOP_CHECKPOINT_STATE_RECOVERY_REQUIRED;
        }
    }
    else
    {
        sCoopNetRuntime.recovery_required = FALSE;
        sCoopNetRuntime.save_update_epoch = 0;
        sCoopNetRuntime.save_update_generation = 0;
        sCoopNetRuntime.checkpoint_state = COOP_CHECKPOINT_STATE_IDLE;
    }
}

static void ReconcileQueuedSaveDataUpdated(void)
{
    u16 depth;

    if (!sCoopNetRuntime.save_data_update_queued)
        return;

    if (!CoopBridgeQueue_TryGetDepth(&gCoopNetBridge.game_to_network, &depth))
    {
        /* A malformed consumer index makes it impossible to prove whether a
         * queued critical event was consumed. Preserve every bit of evidence
         * and require explicit recovery instead of acknowledging by guess. */
        gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_QUEUE_ERROR;
        sCoopNetRuntime.save_data_update_pending |=
            sCoopNetRuntime.save_data_update_queued;
        sCoopNetRuntime.recovery_required = TRUE;
        sCoopNetRuntime.checkpoint_state = COOP_CHECKPOINT_STATE_RECOVERY_REQUIRED;
        return;
    }

    /* The sidecar is the consumer and advances read_index directly in EWRAM.
     * Once it reaches the counter immediately after our entry, FIFO ordering
     * proves that the critical update was consumed. */
    if ((s16)(gCoopNetBridge.game_to_network.read_index
              - sCoopNetRuntime.save_update_queue_next_index) < 0)
        return;

    sCoopNetRuntime.save_data_update_queued = FALSE;
    sCoopNetRuntime.save_data_update_pending = FALSE;
    sCoopNetRuntime.flash_save_started = FALSE;
    sCoopNetRuntime.save_update_epoch = 0;
    sCoopNetRuntime.save_update_generation = 0;
    if (!sCoopNetRuntime.save_data_update_pending
     && sCoopNetRuntime.checkpoint_state == COOP_CHECKPOINT_STATE_SAVING)
        sCoopNetRuntime.checkpoint_state = COOP_CHECKPOINT_STATE_IDLE;
}

static void PreserveQueuedSaveDataUpdatedBeforeQueueReset(void)
{
    ReconcileQueuedSaveDataUpdated();
    if (sCoopNetRuntime.recovery_required)
        return;

    if (sCoopNetRuntime.save_data_update_queued)
    {
        /* Queue reset would otherwise erase an update that was accepted but
         * never consumed. Keep its epoch and make it retryable. */
        sCoopNetRuntime.save_data_update_queued = FALSE;
        sCoopNetRuntime.save_data_update_pending = TRUE;
        sCoopNetRuntime.checkpoint_state = COOP_CHECKPOINT_STATE_SAVING;
    }
}

static void TrySendPendingSaveDataUpdated(void)
{
    u8 payload[sizeof(u32)];

    if (!sCoopNetRuntime.save_data_update_pending
     || sCoopNetRuntime.save_data_update_queued)
        return;

    if (!IsCloudSessionActive())
    {
        /* A heartbeat gap is recoverable when the sidecar returns with the
         * same epoch. Keep the event and its origin until then. */
        return;
    }

    if (sCoopNetRuntime.save_update_epoch != sCoopNetRuntime.session_epoch)
    {
        /* Never send an old save completion in a replacement epoch. Retain
         * the evidence and stop all normal work for explicit recovery. */
        sCoopNetRuntime.recovery_required = TRUE;
        sCoopNetRuntime.checkpoint_state = COOP_CHECKPOINT_STATE_RECOVERY_REQUIRED;
        return;
    }

    payload[0] = sCoopNetRuntime.save_update_generation;
    payload[1] = sCoopNetRuntime.save_update_generation >> 8;
    payload[2] = sCoopNetRuntime.save_update_generation >> 16;
    payload[3] = sCoopNetRuntime.save_update_generation >> 24;
    if (CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_SAVE_DATA_UPDATED,
                                            payload,
                                            sizeof(payload)))
    {
        sCoopNetRuntime.save_data_update_queued = TRUE;
        sCoopNetRuntime.save_update_queue_next_index =
            gCoopNetBridge.game_to_network.write_index;
    }
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
    bool8 result = CoopBridgeQueue_Pop(&gCoopNetBridge.game_to_network, message);

    if (result)
        ReconcileQueuedSaveDataUpdated();
    return result;
}

enum CoopCheckpointState CoopNetBridge_GetCheckpointState(void)
{
    return sCoopNetRuntime.checkpoint_state;
}

bool8 CoopNetBridge_IsCloudMode(void)
{
    /* Once the sidecar accepts the cloud epoch, the session owns save
     * authorization.  Keep this boundary sticky even if a later save
     * validation fails: otherwise a corrupt record could silently fall back
     * to local writes after cloud negotiation. */
    return sCoopNetRuntime.cloud_epoch_accepted;
}

bool8 CoopNetBridge_IsRecoveryRequired(void)
{
    return sCoopNetRuntime.recovery_required;
}

enum CoopCheckpointRequestResult CoopNetBridge_RequestCheckpoint(void)
{
    if (!sCoopNetRuntime.cloud_epoch_accepted)
        return COOP_CHECKPOINT_REQUEST_OFFLINE;

    if (!IsCloudSessionActive()
     || sCoopNetRuntime.checkpoint_state != COOP_CHECKPOINT_STATE_IDLE
     || sCoopNetRuntime.save_data_update_pending
     || sCoopNetRuntime.save_data_update_queued
     || sCoopNetRuntime.flash_save_started
     || (gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_WORLD_NOT_READY) != 0
     || !CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network)
     || CoopBridgeQueue_IsFull(&gCoopNetBridge.game_to_network)
     || !CoopBridgeQueue_IsEmpty(&gCoopNetBridge.network_to_game)
     || CoopBridgeQueue_IsFull(&gCoopNetBridge.network_to_game))
        return COOP_CHECKPOINT_REQUEST_REJECTED;

    /* Do not transition to WaitingForGrant until the critical ready event is
     * actually published. A full queue therefore leaves tx_sequence intact
     * and permits a later, deterministic retry. */
    if (!CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_CHECKPOINT_READY,
                                            NULL,
                                            0))
        return COOP_CHECKPOINT_REQUEST_REJECTED;

    sCoopNetRuntime.checkpoint_state = COOP_CHECKPOINT_STATE_WAITING_FOR_GRANT;
    sCoopNetRuntime.checkpoint_started_frame = sCoopNetRuntime.frame_counter;
    return COOP_CHECKPOINT_REQUEST_STARTED;
}

bool8 CoopNetBridge_ConsumeCheckpointGrant(void)
{
    if (sCoopNetRuntime.checkpoint_state != COOP_CHECKPOINT_STATE_GRANTED)
        return FALSE;

    sCoopNetRuntime.checkpoint_state = COOP_CHECKPOINT_STATE_SAVING;
    return TRUE;
}

bool8 CoopNetBridge_IsCheckpointAuthorizedForSave(void)
{
    return IsCloudSessionActive()
        && !sCoopNetRuntime.recovery_required
        && !sCoopNetRuntime.save_data_update_pending
        && !sCoopNetRuntime.save_data_update_queued
        && !sCoopNetRuntime.flash_save_started
        && sCoopNetRuntime.checkpoint_state == COOP_CHECKPOINT_STATE_SAVING
        && (sCoopNetRuntime.save_update_epoch == 0
            || sCoopNetRuntime.save_update_epoch == sCoopNetRuntime.session_epoch);
}

void CoopNetBridge_NotifySaveResult(bool8 save_succeeded)
{
    if (sCoopNetRuntime.checkpoint_state != COOP_CHECKPOINT_STATE_SAVING)
        return;

    if (sCoopNetRuntime.flash_save_started
     || sCoopNetRuntime.save_data_update_pending
     || sCoopNetRuntime.save_data_update_queued)
        return;

    sCoopNetRuntime.checkpoint_started_frame = 0;
    sCoopNetRuntime.flash_save_started = TRUE;
    if (!save_succeeded)
    {
        sCoopNetRuntime.flash_save_started = FALSE;
        sCoopNetRuntime.save_data_update_pending = FALSE;
        sCoopNetRuntime.save_update_epoch = 0;
        sCoopNetRuntime.save_update_generation = 0;
        sCoopNetRuntime.checkpoint_state = COOP_CHECKPOINT_STATE_IDLE;
        return;
    }

    sCoopNetRuntime.save_data_update_pending = TRUE;
    sCoopNetRuntime.save_update_epoch = sCoopNetRuntime.session_epoch;
    sCoopNetRuntime.save_update_generation = CoopSave_GetGeneration();
    TrySendPendingSaveDataUpdated();
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
        if (!CoopSave_IsOnlineEnabled())
            return FALSE;

        if (message->session_epoch == sCoopNetRuntime.session_epoch)
        {
            if ((gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SESSION_READY) != 0)
            {
                if (IsSequenceNewer(message->sequence, sCoopNetRuntime.rx_sequence))
                    sCoopNetRuntime.rx_sequence = message->sequence;
                return FALSE;
            }
            if (!IsSequenceNewer(message->sequence, sCoopNetRuntime.rx_sequence))
                return FALSE;

            /* Re-arm a disconnected transport without rewinding either
             * sequence domain inside the still-current epoch. */
            PreserveQueuedSaveDataUpdatedBeforeQueueReset();
            CoopBridgeQueue_Init(&gCoopNetBridge.game_to_network);
            CoopBridgeQueue_Init(&gCoopNetBridge.network_to_game);
            sCoopNetRuntime.rx_sequence = message->sequence;
            sCoopNetRuntime.last_player_state_frame = 0;
            sCoopNetRuntime.observed_sidecar_heartbeat = gCoopNetBridge.last_sidecar_heartbeat;
            sCoopNetRuntime.observed_sidecar_heartbeat_frame = sCoopNetRuntime.frame_counter;
            sCoopNetRuntime.cloud_epoch_accepted = TRUE;
            SetCheckpointStateForAcceptedEpoch();
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
        PreserveQueuedSaveDataUpdatedBeforeQueueReset();
        CoopBridgeQueue_Init(&gCoopNetBridge.game_to_network);
        CoopBridgeQueue_Init(&gCoopNetBridge.network_to_game);
        sCoopNetRuntime.session_epoch = message->session_epoch;
        sCoopNetRuntime.rx_sequence = message->sequence;
        sCoopNetRuntime.tx_sequence = 1;
        sCoopNetRuntime.last_player_state_frame = 0;
        sCoopNetRuntime.observed_sidecar_heartbeat = gCoopNetBridge.last_sidecar_heartbeat;
        sCoopNetRuntime.observed_sidecar_heartbeat_frame = sCoopNetRuntime.frame_counter;
        sCoopNetRuntime.cloud_epoch_accepted = TRUE;
        SetCheckpointStateForAcceptedEpoch();
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

    if (message->type == COOP_BRIDGE_MESSAGE_CHECKPOINT_GRANTED)
    {
        /* Grants carry no data. A grant from another epoch or an already
         * consumed sequence is harmless transport noise, not permission to
         * touch flash. */
        if (message->length != 0 || !IsEmptyPayload(message))
        {
            gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
            return FALSE;
        }
        if (!IsCloudSessionActive()
         || message->session_epoch != sCoopNetRuntime.session_epoch
         || !IsSequenceNewer(message->sequence, sCoopNetRuntime.rx_sequence))
            return FALSE;

        sCoopNetRuntime.rx_sequence = message->sequence;
        if (sCoopNetRuntime.checkpoint_state == COOP_CHECKPOINT_STATE_WAITING_FOR_GRANT)
        {
            sCoopNetRuntime.checkpoint_state = COOP_CHECKPOINT_STATE_GRANTED;
            sCoopNetRuntime.checkpoint_started_frame = 0;
        }
        return FALSE;
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
            PreserveQueuedSaveDataUpdatedBeforeQueueReset();
            CoopBridgeQueue_Init(&gCoopNetBridge.game_to_network);
            CoopBridgeQueue_Init(&gCoopNetBridge.network_to_game);
            gCoopNetBridge.status_flags &= ~COOP_BRIDGE_STATUS_SESSION_READY;
            gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_SIDECAR_HEARTBEAT_STALE;
            CancelCheckpointAuthorization();
        }
    }
}

void CoopNetBridge_Init(void)
{
    memset(&gCoopNetBridge, 0, sizeof(gCoopNetBridge));
    memset(&sCoopNetRuntime, 0, sizeof(sCoopNetRuntime));
    if (!CoopSave_LoadRuntimeProgress())
        CoopProgress_Init(&gCoopProgress);
    CoopBridgeQueue_Init(&gCoopNetBridge.game_to_network);
    CoopBridgeQueue_Init(&gCoopNetBridge.network_to_game);
    gCoopNetBridge.magic = COOP_NET_BRIDGE_MAGIC;
    gCoopNetBridge.abi_version = COOP_NET_BRIDGE_ABI_VERSION;
    gCoopNetBridge.game_protocol_version = COOP_NET_BRIDGE_GAME_PROTOCOL_VERSION;
    gCoopNetBridge.game_build_id = COOP_NET_BRIDGE_GAME_BUILD_ID;
    sCoopNetRuntime.tx_sequence = 1;
    sCoopNetRuntime.checkpoint_state = COOP_CHECKPOINT_STATE_OFFLINE;
    gCoopNetBridge.status_flags = COOP_BRIDGE_STATUS_INITIALIZED;

    TryAnnounceRomReady();
}

void CoopNetBridge_Poll(void)
{
    struct CoopBridgeMessage message;
    u16 inbound_count;
    u16 inbound_depth;

    if ((gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_INITIALIZED) == 0)
        return;

    sCoopNetRuntime.frame_counter++;
    /* AgbMain initializes the bridge before flash is loaded. Do not invite a
     * cloud session until the save layer has classified and validated V1. */
    TryAnnounceRomReady();
    /* The sidecar owns read_index. Reconcile a critical event before any
     * heartbeat-driven reset so an already-consumed update is not retried,
     * while an accepted-but-undrained one can be preserved for rearm. */
    ReconcileQueuedSaveDataUpdated();
    ObserveSidecarHeartbeat();

    if (sCoopNetRuntime.checkpoint_state == COOP_CHECKPOINT_STATE_WAITING_FOR_GRANT
     && sCoopNetRuntime.frame_counter - sCoopNetRuntime.checkpoint_started_frame
        >= COOP_NET_BRIDGE_CHECKPOINT_TIMEOUT_FRAMES)
        CancelCheckpointAuthorization();

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

    if (sCoopNetRuntime.checkpoint_state == COOP_CHECKPOINT_STATE_WAITING_FOR_GRANT
     || sCoopNetRuntime.checkpoint_state == COOP_CHECKPOINT_STATE_GRANTED)
        return;

    if (sCoopNetRuntime.checkpoint_state == COOP_CHECKPOINT_STATE_RECOVERY_REQUIRED)
        return;

    ReconcileQueuedSaveDataUpdated();
    if (sCoopNetRuntime.checkpoint_state == COOP_CHECKPOINT_STATE_SAVING)
    {
        TrySendPendingSaveDataUpdated();
        return;
    }

    /* SaveDataUpdated is critical and must be delivered before the latest
     * value PlayerState stream is allowed to add more queue pressure. */
    TrySendPendingSaveDataUpdated();
    if (sCoopNetRuntime.save_data_update_pending
     || !CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network))
        return;

    if (sCoopNetRuntime.last_player_state_frame == 0
     || sCoopNetRuntime.frame_counter - sCoopNetRuntime.last_player_state_frame
        >= COOP_NET_BRIDGE_PLAYER_STATE_INTERVAL)
    {
        if (!SendPlayerState())
            gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_QUEUE_ERROR;
    }
}
