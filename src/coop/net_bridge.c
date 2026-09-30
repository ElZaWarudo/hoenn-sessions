#include "global.h"
#include "coop/net_bridge.h"
#include "coop/battle_consent.h"
#include "coop/battle_runtime.h"
#include "coop/friendly_battle.h"
#include "coop/group_travel.h"
#include "coop/online.h"
#include "coop/presence_runtime.h"
#include "coop/progress.h"
#include "coop/save.h"
#include "coop/trade_offer.h"
#include "coop/trade_runtime.h"
#include "johto/bug_contest.h"
#include "constants/map_groups.h"
#include "../data/map_group_count.h"

ALIGNED(4) EWRAM_DATA struct CoopNetBridge gCoopNetBridge = {0};

#define COOP_PROGRESS_NOTICE_CAPACITY 16
/*
 * Progress observations are edge events: a badge transition or a first
 * catch must reach the partner even if the realtime transport is still
 * coming up. This bounded queue can hold more than a full National Pokédex
 * worth of events while the bridge is unavailable. Duplicate observations
 * are coalesced before this bound is reached.
 */
#define COOP_PROGRESS_OBSERVATION_CAPACITY 1088

struct CoopProgressNotice
{
    u8 kind;
    u8 region;
    u16 subject_id;
};

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
    u32 online_request_id;
    bool8 online_status_valid;
    bool8 membership_known;
    bool8 known_grouped;
    bool8 remote_join_possible;
    bool8 local_join_request_pending;
    u32 local_join_online_request_id;
    u32 local_join_pairing_request_id;
    u32 pairing_request_id;
    bool8 pairing_status_valid;
    struct CoopPairingStatus pairing_status;
    bool8 invite_notice_pending;
    struct CoopProgressNotice progress_notices[COOP_PROGRESS_NOTICE_CAPACITY];
    u8 progress_notice_read;
    u8 progress_notice_write;
    u8 progress_notice_count;
    u16 progress_observations[COOP_PROGRESS_OBSERVATION_CAPACITY];
    u16 progress_observation_read;
    u16 progress_observation_write;
    u16 progress_observation_count;
    struct CoopOnlineStatus online_status;
};

static EWRAM_DATA struct CoopNetRuntime sCoopNetRuntime = {0};

/* mGBA pauses the emulated CPU while Lua touches EWRAM. These barriers keep
 * the compiler from moving entry accesses across the volatile queue index
 * publication points used by that turn-taking protocol. */
#define COOP_BRIDGE_MEMORY_BARRIER() __asm__ volatile ("" ::: "memory")

static bool8 IsOutboundMessageType(u16 type)
{
    return type >= COOP_BRIDGE_MESSAGE_ROM_READY
        && type <= COOP_BRIDGE_MESSAGE_TRADE_OFFER_DECISION;
}

static bool8 IsInboundMessageType(u16 type)
{
    return type >= COOP_BRIDGE_MESSAGE_SESSION_READY
        && type <= COOP_BRIDGE_MESSAGE_TRADE_OFFER_STATUS;
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

static bool8 IsValidProgressObservation(u8 kind, enum CoopRegion region, u16 subject_id)
{
    return (kind == 1 || kind == 2 || kind == 3)
        && CoopRegion_IsValid(region)
        && (kind != 1 || (region != COOP_REGION_SEVII && subject_id < 8))
        && (kind != 2 || (subject_id != 0 && subject_id <= 1025))
        && (kind != 3 || (region != COOP_REGION_SEVII && subject_id == 1));
}

/* Valid subject ids fit in eleven bits; retain kind and region in the upper
 * five bits while the observation waits for room in the shared bridge. */
static u16 PackProgressObservation(u8 kind, enum CoopRegion region, u16 subject_id)
{
    return ((kind - 1) << 13) | ((region - 1) << 11) | subject_id;
}

static bool8 HasPendingProgressObservation(u8 kind,
                                           enum CoopRegion region,
                                           u16 subject_id)
{
    u16 i;

    for (i = 0; i < sCoopNetRuntime.progress_observation_count; i++)
    {
        u16 index = (sCoopNetRuntime.progress_observation_read + i)
            % COOP_PROGRESS_OBSERVATION_CAPACITY;
        if (sCoopNetRuntime.progress_observations[index]
            == PackProgressObservation(kind, region, subject_id))
            return TRUE;
    }
    return FALSE;
}

/* A progress edge leaves the dedicated queue as soon as it enters the shared
 * FIFO, before the sidecar has necessarily consumed it. Queue rearm must put
 * every unconsumed edge back ahead of newer pending edges. Scan backwards so
 * prepending retains the original FIFO order. A consumed edge is absent from
 * the unread range; replaying an edge whose sidecar read raced a reset is
 * harmless because the server deduplicates progress subjects. */
static void PreserveQueuedProgressObservationsBeforeQueueReset(void)
{
    const struct CoopBridgeQueue *queue = &gCoopNetBridge.game_to_network;
    u16 depth;
    u16 i;

    if (!CoopBridgeQueue_TryGetDepth(queue, &depth))
    {
        gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_QUEUE_ERROR;
        return;
    }

    for (i = 0; i < depth; i++)
    {
        const struct CoopBridgeMessage *message =
            &queue->entries[(queue->write_index - 1 - i)
                            & (COOP_NET_BRIDGE_QUEUE_CAPACITY - 1)];
        u8 kind;
        enum CoopRegion region;
        u16 subject_id;

        if (message->type != COOP_BRIDGE_MESSAGE_PROGRESS_OBSERVATION
         || message->length != 4)
            continue;
        kind = message->payload[0];
        region = message->payload[1];
        subject_id = message->payload[2] | (message->payload[3] << 8);
        if (!IsValidProgressObservation(kind, region, subject_id)
         || HasPendingProgressObservation(kind, region, subject_id))
            continue;

        if (sCoopNetRuntime.progress_observation_count
            == COOP_PROGRESS_OBSERVATION_CAPACITY)
        {
            /* Preserve the older in-flight edge if an expanded subject
             * registry has exhausted the bounded observation backlog. */
            sCoopNetRuntime.progress_observation_write =
                (sCoopNetRuntime.progress_observation_write
                 + COOP_PROGRESS_OBSERVATION_CAPACITY - 1)
                % COOP_PROGRESS_OBSERVATION_CAPACITY;
            sCoopNetRuntime.progress_observation_count--;
            gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_QUEUE_CONGESTED;
        }
        sCoopNetRuntime.progress_observation_read =
            (sCoopNetRuntime.progress_observation_read
             + COOP_PROGRESS_OBSERVATION_CAPACITY - 1)
            % COOP_PROGRESS_OBSERVATION_CAPACITY;
        sCoopNetRuntime.progress_observations[
            sCoopNetRuntime.progress_observation_read] =
            PackProgressObservation(kind, region, subject_id);
        sCoopNetRuntime.progress_observation_count++;
    }
}

/* Keep edge observations out of the realtime transport's latest-value and
 * save/checkpoint lanes. A full bridge queue simply leaves the head in this
 * session queue for the next poll; it is not a transport failure. */
static void TrySendPendingProgressObservations(void)
{
    while (sCoopNetRuntime.progress_observation_count != 0)
    {
        u16 index;
        u8 payload[4];
        u16 observation;

        if (!IsCloudSessionActive()
         || sCoopNetRuntime.checkpoint_state != COOP_CHECKPOINT_STATE_IDLE
         || sCoopNetRuntime.save_data_update_pending
         || sCoopNetRuntime.save_data_update_queued
         || sCoopNetRuntime.flash_save_started)
            return;

        index = sCoopNetRuntime.progress_observation_read;
        observation = sCoopNetRuntime.progress_observations[index];
        payload[0] = (observation >> 13) + 1;
        payload[1] = ((observation >> 11) & 3) + 1;
        payload[2] = observation;
        payload[3] = (observation & 0x7FF) >> 8;
        if (!CoopNetBridge_EnqueueGameToNetwork(
                COOP_BRIDGE_MESSAGE_PROGRESS_OBSERVATION, payload, sizeof(payload)))
            return;

        sCoopNetRuntime.progress_observation_read =
            (index + 1) % COOP_PROGRESS_OBSERVATION_CAPACITY;
        sCoopNetRuntime.progress_observation_count--;
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

bool8 CoopNetBridge_SendOnlineRequest(const struct CoopOnlineRequest *request)
{
    u8 payload[COOP_ONLINE_REQUEST_SIZE] = {0};
    u32 i;

    if (request == NULL || request->request_id == 0
     || request->action > COOP_ONLINE_INVITE_LAST_PARTNER || request->page > 31
     || (request->action != COOP_ONLINE_REFRESH && request->view_id == 0)
     || (request->action == COOP_ONLINE_REFRESH && request->view_id != 0)
     || !IsCloudSessionActive()
     || sCoopNetRuntime.checkpoint_state != COOP_CHECKPOINT_STATE_IDLE
     || !IsSequenceNewer(request->request_id, sCoopNetRuntime.online_request_id))
        return FALSE;
    for (i = 0; i < 4; i++)
    {
        payload[i] = request->request_id >> (i * 8);
        payload[4 + i] = request->view_id >> (i * 8);
    }
    payload[8] = request->action;
    payload[9] = request->page;
    if (!CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_ONLINE_REQUEST,
                                          payload, sizeof(payload)))
        return FALSE;
    sCoopNetRuntime.online_request_id = request->request_id;
    sCoopNetRuntime.online_status_valid = FALSE;
    if (request->action == COOP_ONLINE_INVITE
     || request->action == COOP_ONLINE_INVITE_LAST_PARTNER)
    {
        sCoopNetRuntime.local_join_request_pending = TRUE;
        sCoopNetRuntime.local_join_online_request_id = request->request_id;
        sCoopNetRuntime.remote_join_possible = TRUE;
    }
    return TRUE;
}

bool8 CoopNetBridge_GetOnlineStatus(struct CoopOnlineStatus *status)
{
    if (status == NULL || !IsCloudSessionActive() || !sCoopNetRuntime.online_status_valid)
        return FALSE;
    *status = sCoopNetRuntime.online_status;
    return TRUE;
}

bool8 CoopNetBridge_ObserveProgress(u8 kind, enum CoopRegion region, u16 subject_id)
{
    u16 index;

    if (!CoopSave_IsOnlineEnabled()
     || !IsValidProgressObservation(kind, region, subject_id))
        return FALSE;

    /* The event may arrive before SESSION_READY, while a checkpoint is
     * quiescing the bridge, or while the sidecar has filled the shared FIFO.
     * Keep it in the dedicated edge-event queue and let CoopNetBridge_Poll
     * retry it after the transport becomes writable. */
    if (HasPendingProgressObservation(kind, region, subject_id))
    {
        TrySendPendingProgressObservations();
        return TRUE;
    }

    if (sCoopNetRuntime.progress_observation_count
        == COOP_PROGRESS_OBSERVATION_CAPACITY)
    {
        /* Keep gameplay alive and prefer the newest edge if the bridge stays
         * unavailable long enough to fill the backlog. The normal poll loop
         * drains this queue in order whenever the bridge is writable. */
        sCoopNetRuntime.progress_observation_read =
            (sCoopNetRuntime.progress_observation_read + 1)
            % COOP_PROGRESS_OBSERVATION_CAPACITY;
        sCoopNetRuntime.progress_observation_count--;
        gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_QUEUE_CONGESTED;
    }

    index = sCoopNetRuntime.progress_observation_write;
    sCoopNetRuntime.progress_observations[index] =
        PackProgressObservation(kind, region, subject_id);
    sCoopNetRuntime.progress_observation_write =
        (index + 1) % COOP_PROGRESS_OBSERVATION_CAPACITY;
    sCoopNetRuntime.progress_observation_count++;
    TrySendPendingProgressObservations();
    return TRUE;
}

static bool8 ValidPairingCode(const u8 *code)
{
    static const char alphabet[] = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    u8 i;
    for (i = 0; i < 7; i++)
    {
        const char *p;
        if (i == 3) { if (code[i] != '-') return FALSE; continue; }
        for (p = alphabet; *p != 0 && *p != code[i]; p++) {}
        if (*p == 0) return FALSE;
    }
    return TRUE;
}

bool8 CoopNetBridge_SendPairingRequest(const struct CoopPairingRequest *request)
{
    u8 payload[COOP_PAIRING_RECORD_SIZE] = {0};
    u8 i;
    if (request == NULL || request->request_id == 0 || request->action > COOP_PAIRING_REDEEM
     || (request->action == COOP_PAIRING_REDEEM && !ValidPairingCode(request->code))
     || !IsCloudSessionActive() || sCoopNetRuntime.checkpoint_state != COOP_CHECKPOINT_STATE_IDLE
     || !IsSequenceNewer(request->request_id, sCoopNetRuntime.pairing_request_id))
        return FALSE;
    for (i = 0; i < 4; i++) payload[i] = request->request_id >> (i * 8);
    payload[4] = request->action;
    if (request->action == COOP_PAIRING_REDEEM)
        for (i = 0; i < 7; i++) payload[5 + i] = request->code[i];
    if (!CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_PAIRING_REQUEST, payload, sizeof(payload)))
        return FALSE;
    sCoopNetRuntime.pairing_request_id = request->request_id;
    sCoopNetRuntime.pairing_status_valid = FALSE;
    if (request->action == COOP_PAIRING_CREATE)
    {
        sCoopNetRuntime.local_join_request_pending = TRUE;
        sCoopNetRuntime.local_join_pairing_request_id = request->request_id;
        sCoopNetRuntime.remote_join_possible = TRUE;
    }
    return TRUE;
}

bool8 CoopNetBridge_GetPairingStatus(struct CoopPairingStatus *status)
{
    if (status == NULL || !IsCloudSessionActive() || !sCoopNetRuntime.pairing_status_valid) return FALSE;
    *status = sCoopNetRuntime.pairing_status;
    return TRUE;
}

bool8 CoopNetBridge_TakeInviteNotice(void)
{
    if (!IsCloudSessionActive() || !sCoopNetRuntime.invite_notice_pending)
        return FALSE;
    sCoopNetRuntime.invite_notice_pending = FALSE;
    return TRUE;
}

bool8 CoopNetBridge_TakeProgressNotice(u8 *kind, u8 *region, u16 *subject_id)
{
    const struct CoopProgressNotice *notice;

    if (!IsCloudSessionActive() || sCoopNetRuntime.progress_notice_count == 0
     || kind == NULL || region == NULL || subject_id == NULL)
        return FALSE;
    notice = &sCoopNetRuntime.progress_notices[sCoopNetRuntime.progress_notice_read];
    *kind = notice->kind;
    *region = notice->region;
    *subject_id = notice->subject_id;
    sCoopNetRuntime.progress_notice_read = (sCoopNetRuntime.progress_notice_read + 1)
        % COOP_PROGRESS_NOTICE_CAPACITY;
    sCoopNetRuntime.progress_notice_count--;
    return TRUE;
}

bool8 CoopNetBridge_IsGrouped(void)
{
    return IsCloudSessionActive() && sCoopNetRuntime.membership_known
        && sCoopNetRuntime.known_grouped;
}

bool8 CoopNetBridge_IsOrMayBeGrouped(void)
{
    /* A fresh cloud session may resume an existing group. Keep unilateral
     * travel closed before its first authenticated status, and through later
     * refresh or transport loss until an authenticated update confirms the
     * group ended. */
    return sCoopNetRuntime.known_grouped
        || sCoopNetRuntime.remote_join_possible
        || sCoopNetRuntime.local_join_request_pending
        || (sCoopNetRuntime.session_epoch != 0 && CoopSave_IsOnlineEnabled()
            && !sCoopNetRuntime.membership_known)
        || CoopNetBridge_IsGrouped();
}

bool8 CoopNetBridge_IsSessionActive(void)
{
    return IsCloudSessionActive();
}

bool8 CoopNetBridge_CanSendBattle(void)
{
    return IsCloudSessionActive()
        && sCoopNetRuntime.checkpoint_state == COOP_CHECKPOINT_STATE_IDLE;
}

static bool8 DecodeOnlineName(u8 *out, const u8 *in)
{
    u32 i;
    bool8 terminated = FALSE;

    for (i = 0; i < COOP_ONLINE_NAME_SIZE; i++)
    {
        if (in[i] == 0)
            terminated = TRUE;
        else if (terminated || in[i] < 0x20 || in[i] > 0x7E)
            return FALSE;
        out[i] = in[i];
    }
    out[COOP_ONLINE_NAME_SIZE] = 0;
    return TRUE;
}

static bool8 DecodeLastPartnerName(u8 *out, const u8 *in)
{
    u8 i;
    bool8 terminated = FALSE;
    for (i = 0; i < 16; i++)
    {
        if (in[i] == 0) terminated = TRUE;
        else if (terminated || in[i] < 0x20 || in[i] > 0x7E) return FALSE;
        out[i] = in[i];
    }
    out[16] = 0;
    return TRUE;
}

static bool8 DecodeOnlineStatus(struct CoopOnlineStatus *status, const struct CoopBridgeMessage *message)
{
    const u8 *payload = message->payload;
    u32 i;

    if (message->length != COOP_ONLINE_STATUS_SIZE || payload[4] > COOP_ONLINE_FAILED
     || (payload[5] & ~127) != 0 || payload[6] > 32 || payload[7] > 32
     || payload[8] > 31 || payload[9] > 31
     || payload[10] > 4 || payload[11] >= (payload[10] == 0 ? 1 : payload[10])
     || ((payload[5] & COOP_ONLINE_GROUPED) && payload[10] != 0)
     || ((payload[5] & COOP_ONLINE_HAS_LOCATION) && !(payload[5] & COOP_ONLINE_GROUPED))
     || (payload[6] == 0 ? payload[8] != 0 : payload[8] >= payload[6])
     || (payload[7] == 0 ? payload[9] != 0 : payload[9] >= payload[7]))
        return FALSE;
    if (payload[5] & COOP_ONLINE_HAS_LOCATION)
    {
        u16 group = (u16)payload[12] | ((u16)payload[13] << 8);
        u16 number = (u16)payload[14] | ((u16)payload[15] << 8);
        if (group >= MAP_GROUPS_COUNT || number >= MAP_GROUP_COUNT[group])
            return FALSE;
    }
    else
        for (i = 12; i < 16; i++)
            if (payload[i] != 0)
                return FALSE;
    for (i = COOP_ONLINE_STATUS_SIZE; i < COOP_NET_BRIDGE_PAYLOAD_SIZE; i++)
        if (payload[i] != 0)
            return FALSE;
    if (payload[5] & COOP_ONLINE_HAS_LAST_PARTNER)
    {
        if (payload[112] == 0) return FALSE;
    }
    else
        for (i = 112; i < 128; i++) if (payload[i] != 0) return FALSE;
    status->request_id = (u32)payload[0] | ((u32)payload[1] << 8)
                       | ((u32)payload[2] << 16) | ((u32)payload[3] << 24);
    status->result = payload[4];
    status->flags = payload[5];
    status->nearby_count = payload[6];
    status->incoming_count = payload[7];
    status->nearby_page = payload[8];
    status->incoming_page = payload[9];
    status->outgoing_count = payload[10];
    status->outgoing_page = payload[11];
    status->location_map_group = (u16)payload[12] | ((u16)payload[13] << 8);
    status->location_map_number = (u16)payload[14] | ((u16)payload[15] << 8);
    return status->request_id != 0
        && DecodeOnlineName(status->nearby_name, payload + 16)
        && DecodeOnlineName(status->incoming_name, payload + 48)
        && DecodeOnlineName(status->group_name, payload + 80)
        && DecodeLastPartnerName(status->last_partner_name, payload + 112);
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
    if (JohtoBugContest_IsSerializationBlocked())
        return COOP_CHECKPOINT_REQUEST_REJECTED;

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
    u8 payload[COOP_PRESENCE_LOCAL_STATE_SIZE];

    sCoopNetRuntime.last_player_state_frame = sCoopNetRuntime.frame_counter;
    if (!CoopPresenceRuntime_EncodeLocalState(payload, sizeof(payload)))
    {
        gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_WORLD_NOT_READY;
        return TRUE;
    }
    gCoopNetBridge.status_flags &= ~COOP_BRIDGE_STATUS_WORLD_NOT_READY;
    if (!CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_PLAYER_STATE,
                                            payload, sizeof(payload)))
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
        if (message->length != 0 || !IsEmptyPayload(message))
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
            if (!IsSequenceNewer(message->sequence, sCoopNetRuntime.rx_sequence))
                return FALSE;

            /* SESSION_READY is the transport-generation boundary, even when
             * the previous bridge is still inside its heartbeat window.  Do
             * not let lifecycle traffic from that generation survive the
             * replacement authentication.  Preserve a critical queued save,
             * but reset every presence and interaction generation first. */
            PreserveQueuedSaveDataUpdatedBeforeQueueReset();
            PreserveQueuedProgressObservationsBeforeQueueReset();
            CoopTradeRuntime_PreserveOutbound();
            CoopBattleRuntime_PreserveOutbound();
            CoopBridgeQueue_Init(&gCoopNetBridge.game_to_network);
            CoopBridgeQueue_Init(&gCoopNetBridge.network_to_game);
            sCoopNetRuntime.rx_sequence = message->sequence;
            sCoopNetRuntime.last_player_state_frame = 0;
            sCoopNetRuntime.observed_sidecar_heartbeat = gCoopNetBridge.last_sidecar_heartbeat;
            sCoopNetRuntime.observed_sidecar_heartbeat_frame = sCoopNetRuntime.frame_counter;
            sCoopNetRuntime.cloud_epoch_accepted = TRUE;
            sCoopNetRuntime.online_status_valid = FALSE;
            sCoopNetRuntime.membership_known = FALSE;
            sCoopNetRuntime.local_join_request_pending = FALSE;
            sCoopNetRuntime.local_join_online_request_id = 0;
            sCoopNetRuntime.local_join_pairing_request_id = 0;
            sCoopNetRuntime.online_request_id = 0;
            sCoopNetRuntime.pairing_status_valid = FALSE;
            sCoopNetRuntime.pairing_request_id = 0;
            sCoopNetRuntime.invite_notice_pending = FALSE;
            sCoopNetRuntime.progress_notice_read = 0;
            sCoopNetRuntime.progress_notice_write = 0;
            sCoopNetRuntime.progress_notice_count = 0;
            /* A reconnect starts a fresh presence generation even when the
             * sidecar reuses the current epoch.  Do not let an old reducer,
             * pending lifecycle frame, or sprite survive the queue rearm. */
            CoopPresenceRuntime_Reset();
            CoopPresenceRuntime_SetSessionEpoch(sCoopNetRuntime.session_epoch);
            SetCheckpointStateForAcceptedEpoch();
            gCoopNetBridge.status_flags &= ~(COOP_BRIDGE_STATUS_QUEUE_CONGESTED
                                          | COOP_BRIDGE_STATUS_QUEUE_ERROR
                                          | COOP_BRIDGE_STATUS_CHECKSUM_ERROR
                                          | COOP_BRIDGE_STATUS_SIDECAR_HEARTBEAT_STALE);
            gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_SESSION_READY
                                        | COOP_BRIDGE_STATUS_SIDECAR_HEARTBEAT_SEEN;
            if (CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_ROM_READY, NULL, 0))
                gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_ROM_READY_SENT;
            /* ROM_READY must be the first ROM frame of a replacement bridge.
             * Semantic replay before it is treated as an unauthenticated
             * startup frame by the sidecar and tears the reconnect down. */
            CoopGroupTravel_OnSessionReady();
            CoopBattleConsent_OnSessionReady();
            CoopBattleRuntime_OnSessionReady(sCoopNetRuntime.session_epoch);
            CoopTradeOffer_OnSessionReady();
            return TRUE;
        }
        else if (sCoopNetRuntime.session_epoch != 0
              && !IsSequenceNewer(message->session_epoch, sCoopNetRuntime.session_epoch))
            return FALSE;

        /* A restored savestate receives a new epoch. Drop every stale queued
         * message before publishing any state for the replacement session. */
        PreserveQueuedSaveDataUpdatedBeforeQueueReset();
        PreserveQueuedProgressObservationsBeforeQueueReset();
        CoopTradeRuntime_PreserveOutbound();
        CoopBridgeQueue_Init(&gCoopNetBridge.game_to_network);
        CoopBridgeQueue_Init(&gCoopNetBridge.network_to_game);
        sCoopNetRuntime.session_epoch = message->session_epoch;
        sCoopNetRuntime.rx_sequence = message->sequence;
        sCoopNetRuntime.tx_sequence = 1;
        sCoopNetRuntime.last_player_state_frame = 0;
        sCoopNetRuntime.observed_sidecar_heartbeat = gCoopNetBridge.last_sidecar_heartbeat;
        sCoopNetRuntime.observed_sidecar_heartbeat_frame = sCoopNetRuntime.frame_counter;
        sCoopNetRuntime.cloud_epoch_accepted = TRUE;
        sCoopNetRuntime.online_status_valid = FALSE;
        sCoopNetRuntime.membership_known = FALSE;
        sCoopNetRuntime.local_join_request_pending = FALSE;
        sCoopNetRuntime.local_join_online_request_id = 0;
        sCoopNetRuntime.local_join_pairing_request_id = 0;
        sCoopNetRuntime.online_request_id = 0;
        sCoopNetRuntime.pairing_status_valid = FALSE;
        sCoopNetRuntime.pairing_request_id = 0;
        sCoopNetRuntime.invite_notice_pending = FALSE;
        sCoopNetRuntime.progress_notice_read = 0;
        sCoopNetRuntime.progress_notice_write = 0;
        sCoopNetRuntime.progress_notice_count = 0;
        CoopPresenceRuntime_SetSessionEpoch(sCoopNetRuntime.session_epoch);
        SetCheckpointStateForAcceptedEpoch();
        gCoopNetBridge.status_flags &= ~(COOP_BRIDGE_STATUS_QUEUE_CONGESTED
                                      | COOP_BRIDGE_STATUS_QUEUE_ERROR
                                      | COOP_BRIDGE_STATUS_CHECKSUM_ERROR
                                      | COOP_BRIDGE_STATUS_SIDECAR_HEARTBEAT_STALE);
        gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_SESSION_READY
                                    | COOP_BRIDGE_STATUS_SIDECAR_HEARTBEAT_SEEN;
        if (CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_ROM_READY, NULL, 0))
            gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_ROM_READY_SENT;
        CoopGroupTravel_OnSessionReady();
        CoopBattleConsent_OnSessionReady();
        CoopBattleRuntime_OnSessionReady(sCoopNetRuntime.session_epoch);
        CoopTradeOffer_OnSessionReady();
        return TRUE;
    }

    if (message->type == COOP_BRIDGE_MESSAGE_GROUP_INVITE_RECEIVED)
    {
        u32 i;
        bool8 terminated = FALSE;
        if (!IsCloudSessionActive() || message->session_epoch != sCoopNetRuntime.session_epoch
         || !IsSequenceNewer(message->sequence, sCoopNetRuntime.rx_sequence))
            return FALSE;
        if (message->length != COOP_ONLINE_NAME_SIZE)
            goto invalid_invite_notice;
        for (i = 0; i < COOP_ONLINE_NAME_SIZE; i++)
        {
            u8 c = message->payload[i];
            if (c == 0) terminated = TRUE;
            else if (terminated || c < 0x20 || c > 0x7E)
                goto invalid_invite_notice;
        }
        for (i = COOP_ONLINE_NAME_SIZE; i < COOP_NET_BRIDGE_PAYLOAD_SIZE; i++)
            if (message->payload[i] != 0)
                goto invalid_invite_notice;
        sCoopNetRuntime.rx_sequence = message->sequence;
        sCoopNetRuntime.invite_notice_pending = TRUE;
        return FALSE;
invalid_invite_notice:
        gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
        return FALSE;
    }

    if (message->type == COOP_BRIDGE_MESSAGE_GROUP_STATE_CHANGED)
    {
        u16 i;

        if (!IsCloudSessionActive()
         || message->session_epoch != sCoopNetRuntime.session_epoch
         || !IsSequenceNewer(message->sequence, sCoopNetRuntime.rx_sequence))
            return FALSE;
        if (message->length != 2 || message->payload[0] > 1
         || message->payload[1] > 1)
            goto invalid_group_state;
        for (i = 2; i < COOP_NET_BRIDGE_PAYLOAD_SIZE; i++)
            if (message->payload[i] != 0)
                goto invalid_group_state;
        sCoopNetRuntime.rx_sequence = message->sequence;
        sCoopNetRuntime.membership_known = TRUE;
        sCoopNetRuntime.known_grouped = message->payload[0] != 0;
        sCoopNetRuntime.remote_join_possible = message->payload[1] != 0;
        /* The watcher is paused during artifact mutations. A frame delivered
         * after a mutation reply is the first snapshot that may release its
         * local guard; an older Online Refresh reply never may. */
        sCoopNetRuntime.local_join_request_pending =
            sCoopNetRuntime.local_join_online_request_id != 0
            || sCoopNetRuntime.local_join_pairing_request_id != 0;
        return FALSE;
invalid_group_state:
        gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
        return FALSE;
    }

    if (message->type == COOP_BRIDGE_MESSAGE_PAIRING_STATUS)
    {
        struct CoopPairingStatus status = {0};
        const u8 *payload = message->payload;
        u8 i;
        if (!IsCloudSessionActive() || message->session_epoch != sCoopNetRuntime.session_epoch
         || !IsSequenceNewer(message->sequence, sCoopNetRuntime.rx_sequence)) return FALSE;
        if (message->length != COOP_PAIRING_RECORD_SIZE || payload[4] > COOP_PAIRING_INVALID
         || (payload[4] == COOP_PAIRING_CREATED ? !ValidPairingCode(payload + 5) : FALSE))
            goto invalid_pairing_status;
        if (payload[4] != COOP_PAIRING_CREATED)
            for (i = 5; i < 12; i++) if (payload[i] != 0) goto invalid_pairing_status;
        for (i = 12; i < COOP_NET_BRIDGE_PAYLOAD_SIZE; i++) if (payload[i] != 0) goto invalid_pairing_status;
        status.request_id = (u32)payload[0] | ((u32)payload[1] << 8) | ((u32)payload[2] << 16) | ((u32)payload[3] << 24);
        if (status.request_id == 0) goto invalid_pairing_status;
        status.result = payload[4];
        for (i = 0; i < 7; i++) status.code[i] = payload[5 + i];
        sCoopNetRuntime.rx_sequence = message->sequence;
        if (status.request_id == sCoopNetRuntime.pairing_request_id)
        {
            sCoopNetRuntime.pairing_status = status;
            sCoopNetRuntime.pairing_status_valid = TRUE;
            if (status.request_id == sCoopNetRuntime.local_join_pairing_request_id)
            {
                /* Wait for a watcher snapshot started after the mutation;
                 * a concurrent Online Refresh may still hold older state. */
                sCoopNetRuntime.remote_join_possible = TRUE;
                sCoopNetRuntime.local_join_pairing_request_id = 0;
            }
            if (status.result == COOP_PAIRING_JOINED)
            {
                sCoopNetRuntime.membership_known = TRUE;
                sCoopNetRuntime.known_grouped = TRUE;
            }
        }
        return FALSE;
invalid_pairing_status:
        gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
        return FALSE;
    }

    if (message->type == COOP_BRIDGE_MESSAGE_ONLINE_STATUS)
    {
        struct CoopOnlineStatus status;

        if (!IsCloudSessionActive()
         || message->session_epoch != sCoopNetRuntime.session_epoch
         || !IsSequenceNewer(message->sequence, sCoopNetRuntime.rx_sequence))
            return FALSE;
        if (!DecodeOnlineStatus(&status, message))
        {
            gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
            return FALSE;
        }
        sCoopNetRuntime.rx_sequence = message->sequence;
        if (status.request_id == sCoopNetRuntime.local_join_online_request_id)
        {
            sCoopNetRuntime.local_join_online_request_id = 0;
            sCoopNetRuntime.remote_join_possible = TRUE;
        }
        if (status.request_id == sCoopNetRuntime.online_request_id)
        {
            sCoopNetRuntime.online_status = status;
            sCoopNetRuntime.online_status_valid = TRUE;
            if (status.result == COOP_ONLINE_READY || status.result == COOP_ONLINE_SUCCESS)
            {
                sCoopNetRuntime.membership_known = TRUE;
                sCoopNetRuntime.known_grouped = (status.flags & COOP_ONLINE_GROUPED) != 0;
                sCoopNetRuntime.remote_join_possible =
                    (status.flags & COOP_ONLINE_REMOTE_JOIN_POSSIBLE) != 0;
            }
        }
        return FALSE;
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

    if (message->type == COOP_BRIDGE_MESSAGE_PROGRESS_EVENT)
    {
        u8 kind = message->payload[0];
        u8 region = message->payload[1];
        u16 subjectId = message->payload[2] | ((u16)message->payload[3] << 8);

        if (!IsCloudSessionActive()
         || message->session_epoch != sCoopNetRuntime.session_epoch
         || !IsSequenceNewer(message->sequence, sCoopNetRuntime.rx_sequence))
            return FALSE;
        if (message->length != 4
         || (kind != 1 && kind != 2 && kind != 3)
         || !CoopRegion_IsValid(region)
         || (kind == 1 && (region == COOP_REGION_SEVII || subjectId >= 8))
         || (kind == 2 && (subjectId == 0 || subjectId > 1025))
         || (kind == 3 && (region == COOP_REGION_SEVII || subjectId != 1)))
        {
            gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
            return FALSE;
        }
        if (sCoopNetRuntime.progress_notice_count == COOP_PROGRESS_NOTICE_CAPACITY)
        {
            /* Keep the newest feed entry when a long modal blocked display. */
            sCoopNetRuntime.progress_notice_read = (sCoopNetRuntime.progress_notice_read + 1)
                % COOP_PROGRESS_NOTICE_CAPACITY;
            sCoopNetRuntime.progress_notice_count--;
            gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_QUEUE_CONGESTED;
        }
        sCoopNetRuntime.progress_notices[sCoopNetRuntime.progress_notice_write] =
            (struct CoopProgressNotice){kind, region, subjectId};
        sCoopNetRuntime.progress_notice_write = (sCoopNetRuntime.progress_notice_write + 1)
            % COOP_PROGRESS_NOTICE_CAPACITY;
        sCoopNetRuntime.progress_notice_count++;
        sCoopNetRuntime.rx_sequence = message->sequence;
        return FALSE;
    }

    if (message->type == COOP_BRIDGE_MESSAGE_GROUP_ENDED)
    {
        if (!IsCloudSessionActive()
         || message->session_epoch != sCoopNetRuntime.session_epoch
         || !IsSequenceNewer(message->sequence, sCoopNetRuntime.rx_sequence))
            return FALSE;
        if (!CoopPresenceRuntime_QueueGroupEnded(message->payload, message->length))
        {
            gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
            return FALSE;
        }
        sCoopNetRuntime.rx_sequence = message->sequence;
        return FALSE;
    }

    if (message->type == COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_SPAWN
     || message->type == COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_UPDATE
     || message->type == COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_DESPAWN
     || message->type == COOP_BRIDGE_MESSAGE_REMOTE_COMPANION
     || message->type == COOP_BRIDGE_MESSAGE_REMOTE_SOCIAL_SIGNAL
     || message->type == COOP_BRIDGE_MESSAGE_REMOTE_INTERACTION)
    {
        if (!IsCloudSessionActive()
         || message->session_epoch != sCoopNetRuntime.session_epoch
         || !IsSequenceNewer(message->sequence, sCoopNetRuntime.rx_sequence))
            return FALSE;
        if (!CoopPresenceRuntime_QueueBridgeFrame(message->type,
                                                   message->payload,
                                                   message->length))
        {
            gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
            return FALSE;
        }
        sCoopNetRuntime.rx_sequence = message->sequence;
        return FALSE;
    }

    if (message->type == COOP_BRIDGE_MESSAGE_GROUP_TRAVEL_SERVER)
    {
        u32 i;
        const struct CoopGroupTravelRecord *record;
        if (!IsCloudSessionActive()
         || message->session_epoch != sCoopNetRuntime.session_epoch
         || !IsSequenceNewer(message->sequence, sCoopNetRuntime.rx_sequence)
         || message->length != COOP_GROUP_TRAVEL_RECORD_SIZE)
            return FALSE;
        for (i = message->length; i < COOP_NET_BRIDGE_PAYLOAD_SIZE; i++)
            if (message->payload[i] != 0)
            {
                gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
                return FALSE;
            }
        record = (const struct CoopGroupTravelRecord *)message->payload;
        if (!CoopGroupTravel_ReceiveServer(record))
        {
            gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
            return FALSE;
        }
        sCoopNetRuntime.rx_sequence = message->sequence;
        return FALSE;
    }

    if (message->type == COOP_BRIDGE_MESSAGE_BATTLE_MANIFEST
     || message->type == COOP_BRIDGE_MESSAGE_PEER_PARTY_CHUNK
     || message->type == COOP_BRIDGE_MESSAGE_TURN_BUNDLE
     || message->type == COOP_BRIDGE_MESSAGE_PAUSE_FOR_RECONNECT
     || message->type == COOP_BRIDGE_MESSAGE_BATTLE_START
     || message->type == COOP_BRIDGE_MESSAGE_BATTLE_COMMIT
     || message->type == COOP_BRIDGE_MESSAGE_ABORT_BATTLE)
    {
        u16 i;
        enum CoopBattleInboundResult result;
        if (!IsCloudSessionActive()
         || message->session_epoch != sCoopNetRuntime.session_epoch
         || !IsSequenceNewer(message->sequence, sCoopNetRuntime.rx_sequence))
            return FALSE;
        for (i = message->length; i < COOP_NET_BRIDGE_PAYLOAD_SIZE; i++)
            if (message->payload[i] != 0)
            {
                gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
                return FALSE;
            }
        switch (message->type)
        {
        case COOP_BRIDGE_MESSAGE_BATTLE_MANIFEST:
            result = CoopBattleRuntime_ReceiveManifest(message->payload, message->length);
            break;
        case COOP_BRIDGE_MESSAGE_PEER_PARTY_CHUNK:
            result = CoopBattleRuntime_ReceivePeerPartyChunk(message->payload, message->length);
            break;
        case COOP_BRIDGE_MESSAGE_TURN_BUNDLE:
            result = CoopBattleRuntime_ReceiveTurnBundle(message->payload, message->length);
            break;
        case COOP_BRIDGE_MESSAGE_PAUSE_FOR_RECONNECT:
            result = CoopBattleRuntime_ReceivePause(message->payload, message->length);
            break;
        case COOP_BRIDGE_MESSAGE_BATTLE_START:
            result = CoopBattleRuntime_ReceiveStart(message->payload, message->length);
            break;
        case COOP_BRIDGE_MESSAGE_BATTLE_COMMIT:
            result = CoopBattleRuntime_ReceiveBattleCommit(message->payload, message->length);
            break;
        default:
            result = CoopBattleRuntime_ReceiveAbort(message->payload, message->length);
            if (result != COOP_BATTLE_INBOUND_MALFORMED)
                CoopBattleConsent_ReceiveAbort(message->payload, message->length);
            break;
        }
        if (result == COOP_BATTLE_INBOUND_MALFORMED)
            gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
        else
            sCoopNetRuntime.rx_sequence = message->sequence;
        return FALSE;
    }

    if (message->type == COOP_BRIDGE_MESSAGE_BATTLE_JOIN_OFFER)
    {
        u16 i;
        bool8 accepted;
        u16 expected = COOP_BATTLE_JOIN_OFFER_SIZE;

        if (!IsCloudSessionActive()
         || message->session_epoch != sCoopNetRuntime.session_epoch
         || !IsSequenceNewer(message->sequence, sCoopNetRuntime.rx_sequence))
            return FALSE;
        if (message->length != expected)
        {
            gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
            return FALSE;
        }
        for (i = expected; i < COOP_NET_BRIDGE_PAYLOAD_SIZE; i++)
            if (message->payload[i] != 0)
            {
                gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
                return FALSE;
            }
        accepted = CoopBattleConsent_ReceiveOffer(message->payload, expected);
        if (!accepted)
        {
            gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
            return FALSE;
        }
        sCoopNetRuntime.rx_sequence = message->sequence;
        return FALSE;
    }

    if (message->type == COOP_BRIDGE_MESSAGE_BATTLE_CONSENT_OUTCOME)
    {
        u16 i;
        if (!IsCloudSessionActive()
         || message->session_epoch != sCoopNetRuntime.session_epoch
         || !IsSequenceNewer(message->sequence, sCoopNetRuntime.rx_sequence)
         || message->length != COOP_BATTLE_CONSENT_OUTCOME_SIZE)
            return FALSE;
        for (i = message->length; i < COOP_NET_BRIDGE_PAYLOAD_SIZE; i++)
            if (message->payload[i] != 0)
            {
                gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
                return FALSE;
            }
        if (!CoopBattleConsent_ReceiveOutcome(message->payload, message->length))
        {
            gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
            return FALSE;
        }
        sCoopNetRuntime.rx_sequence = message->sequence;
        return FALSE;
    }

    if (message->type == COOP_BRIDGE_MESSAGE_BATTLE_RESERVE_REJECTED)
    {
        u16 i;
        if (!IsCloudSessionActive()
         || message->session_epoch != sCoopNetRuntime.session_epoch
         || !IsSequenceNewer(message->sequence, sCoopNetRuntime.rx_sequence))
            return FALSE;
        if (message->length != COOP_BATTLE_RESERVE_REJECTED_SIZE
         || (message->payload[0] | message->payload[1]
          | message->payload[2] | message->payload[3]) == 0)
        {
            gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
            return FALSE;
        }
        for (i = message->length; i < COOP_NET_BRIDGE_PAYLOAD_SIZE; i++)
            if (message->payload[i] != 0)
            {
                gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
                return FALSE;
            }
        (void)CoopBattleConsent_ReceiveReserveRejected(message->payload, message->length);
        sCoopNetRuntime.rx_sequence = message->sequence;
        return FALSE;
    }

    if (message->type == COOP_BRIDGE_MESSAGE_TRADE_COMMIT)
    {
        if (!IsCloudSessionActive()
         || message->session_epoch != sCoopNetRuntime.session_epoch
         || !IsSequenceNewer(message->sequence, sCoopNetRuntime.rx_sequence))
            return FALSE;
        /* The record fills the whole payload, so there is no padding. A
         * malformed commit is neither applied nor acknowledged. */
        if (CoopTradeRuntime_ReceiveCommit(message->payload, message->length)
            == COOP_TRADE_INBOUND_MALFORMED)
        {
            gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
            return FALSE;
        }
        sCoopNetRuntime.rx_sequence = message->sequence;
        return FALSE;
    }

    if (message->type == COOP_BRIDGE_MESSAGE_TRADE_OFFER_RECEIVED
     || message->type == COOP_BRIDGE_MESSAGE_TRADE_OFFER_STATUS)
    {
        u16 i;
        bool8 accepted;

        if (!IsCloudSessionActive()
         || message->session_epoch != sCoopNetRuntime.session_epoch
         || !IsSequenceNewer(message->sequence, sCoopNetRuntime.rx_sequence))
            return FALSE;
        accepted = message->length <= COOP_NET_BRIDGE_PAYLOAD_SIZE;
        for (i = message->length; accepted && i < COOP_NET_BRIDGE_PAYLOAD_SIZE; i++)
            if (message->payload[i] != 0)
                accepted = FALSE;
        if (accepted)
            accepted = message->type == COOP_BRIDGE_MESSAGE_TRADE_OFFER_RECEIVED
                ? CoopTradeOffer_ReceiveOffer(message->payload, message->length)
                : CoopTradeOffer_ReceiveStatus(message->payload, message->length);
        if (!accepted)
        {
            gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_PROTOCOL_ERROR;
            return FALSE;
        }
        sCoopNetRuntime.rx_sequence = message->sequence;
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
            PreserveQueuedProgressObservationsBeforeQueueReset();
            CoopTradeRuntime_PreserveOutbound();
            CoopBattleRuntime_PreserveOutbound();
            CoopBridgeQueue_Init(&gCoopNetBridge.game_to_network);
            CoopBridgeQueue_Init(&gCoopNetBridge.network_to_game);
            gCoopNetBridge.status_flags &= ~COOP_BRIDGE_STATUS_SESSION_READY;
            gCoopNetBridge.status_flags |= COOP_BRIDGE_STATUS_SIDECAR_HEARTBEAT_STALE;
            CoopPresenceRuntime_TransportLost();
            CoopGroupTravel_OnTransportLost();
            CoopBattleConsent_OnTransportLost();
            CoopBattleRuntime_OnTransportLost();
            CoopTradeOffer_OnTransportLost();
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
    CoopPresenceRuntime_Init();
    CoopGroupTravel_Init();
    CoopBattleConsent_Init();
    CoopBattleRuntime_Init();
    CoopTradeRuntime_Init();
    CoopTradeOffer_Init();
    CoopFriendly_Init();

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
    CoopPresenceRuntime_AdvanceFrame();
    CoopGroupTravel_Poll();
    CoopBattleConsent_Poll();
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

    CoopOnline_PollInviteNotice();
    /* Runs before the checkpoint early returns below: it owns the trade
     * checkpoint it requests and must observe the grant. */
    CoopTradeRuntime_Poll();
    /* The trade offer UI also owns the checkpoints it requests. */
    CoopTradeOffer_Poll();

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
    CoopBattleRuntime_PollOutboundReplay();
    TrySendPendingProgressObservations();
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

u32 CoopNetBridge_GetSessionEpoch(void)
{
    return sCoopNetRuntime.session_epoch;
}
