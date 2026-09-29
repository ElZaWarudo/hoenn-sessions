#include "global.h"
#include "coop/battle_runtime.h"
#include "coop/battle_consent.h"
#include "coop/net_bridge.h"
#include "coop/identity.h"
#include "coop/region.h"
#include "battle.h"
#include "random.h"
#include "pokemon.h"
#include "constants/battle.h"
#include "constants/opponents.h"
#include "constants/species.h"
#include "coop/generated_regional_identities.h"

#define COOP_BATTLE_REPLAY_CAPACITY COOP_NET_BRIDGE_QUEUE_CAPACITY

struct CoopBattleReplayRecord
{
    u16 type;
    u16 length;
    u8 payload[COOP_BATTLE_PARTY_SNAPSHOT_SIZE];
};

struct CoopBattleRuntime
{
    u32 epoch;
    u8 manifest[COOP_BATTLE_MANIFEST_SIZE];
    struct CoopBattleTurnBundle bundles[COOP_BATTLE_MAX_TURN];
    u16 latest_turn;
    u8 queued_bundles;
    u8 consumed_bundles;
    u16 pause_turn;
    u8 missing_slot;
    u8 snapshot_id[COOP_BATTLE_ID_SIZE];
    u8 snapshot_count;
    u8 next_snapshot_slot;
    u8 peer_party_count;
    u8 next_peer_party_slot;
    u8 peer_party[6][COOP_BATTLE_PARTY_MON_SIZE];
    u16 last_action_turn;
    u8 last_action[COOP_BATTLE_ACTION_SIZE];
    u16 last_hash_turn;
    u16 last_taken_turn;
    struct CoopBattleReplayRecord replay[COOP_BATTLE_REPLAY_CAPACITY];
    u8 replay_next;
    u8 replay_count;
    bool8 manifest_valid;
    bool8 pause_valid;
    bool8 session_ready;
    bool8 engine_active;
    bool8 engine_faulted;
    bool8 engine_action_submitted;
    bool8 engine_bundle_ready;
    bool8 abort_requested;
    bool8 ready_sent;
    bool8 ready_replay_pending;
    bool8 start_released;
    bool8 terminal_pending;
    bool8 terminal_sent;
    bool8 hash_pending;
    u16 pending_hash_turn;
    u8 pending_hash_outcome;
    u8 pending_hash_digest[COOP_BATTLE_DIGEST_SIZE];
    u16 terminal_turn;
    u8 terminal_result;
    u8 terminal_digest[COOP_BATTLE_DIGEST_SIZE];
    /* The native engine clears the manifest during teardown. Retain the
     * attested trainer battle identity until its server commit is consumed. */
    u8 terminal_battle_id[COOP_BATTLE_ID_SIZE];
    u8 terminal_trainer_region;
    u16 terminal_trainer_ordinal;
    bool8 terminal_identity_valid;
    bool8 terminal_commit_pending;
    bool8 terminal_commit_accepted;
    u8 terminal_commit[COOP_BATTLE_COMMIT_SIZE];
    struct CoopBattleAction engine_peer_action;
};

static EWRAM_DATA struct CoopBattleRuntime sBattleRuntime = {0};

/* The staged counts occupy former padding; the EWRAM plan must not grow. */
_Static_assert(offsetof(struct CoopBattleStartupPlan, original_local) == 32,
               "co-op startup plan counts must fit in existing padding");

static void ClearTerminalCommit(void);

static bool8 IsValidId(const u8 *id)
{
    u8 i;
    for (i = 0; i < COOP_BATTLE_ID_SIZE; i++)
        if (id[i] != 0)
            return TRUE;
    return FALSE;
}

static u16 ReadTurn(const u8 *payload)
{
    return payload[16] | ((u16)payload[17] << 8);
}

static u16 ReadManifestTrainerOrdinal(const u8 *payload)
{
    return payload[COOP_BATTLE_MANIFEST_TRAINER_ORDINAL_OFFSET]
        | ((u16)payload[COOP_BATTLE_MANIFEST_TRAINER_ORDINAL_OFFSET + 1] << 8);
}

/* Any trainer identity in the ROM registry that maps back to a legacy trainer
 * ID is battleable. Unknown ordinals and identities without a legacy ID stay
 * rejected wherever the former Wally/Brock allowlist rejected them. */
static bool8 IsCatalogTrainer(u8 region, u16 ordinal)
{
    return CoopIdentity_ResolveTrainerLegacyId(region, ordinal, NULL);
}

static bool8 IsValidManifestIdentity(const u8 *payload)
{
    u8 kind = payload[COOP_BATTLE_MANIFEST_KIND_OFFSET];
    u8 slot = payload[COOP_BATTLE_MANIFEST_MEMBER_SLOT_OFFSET];
    u8 region = payload[COOP_BATTLE_MANIFEST_REGION_OFFSET];
    u16 ordinal = ReadManifestTrainerOrdinal(payload);

    if (slot > 1)
        return FALSE;
    if (kind == COOP_BATTLE_MANIFEST_KIND_FRIENDLY)
        return region == COOP_REGION_UNSPECIFIED && ordinal == 0;
    if (kind != COOP_BATTLE_MANIFEST_KIND_TRAINER)
        return FALSE;
    return IsCatalogTrainer(region, ordinal);
}

static bool8 IsCurrentBattle(const u8 *payload)
{
    return sBattleRuntime.manifest_valid
        && memcmp(payload, sBattleRuntime.manifest, COOP_BATTLE_ID_SIZE) == 0;
}

static void ClearBattle(void)
{
    bool8 was_active = sBattleRuntime.engine_active;
    bool8 retain_terminal = sBattleRuntime.terminal_identity_valid;
    memset(sBattleRuntime.manifest, 0, sizeof(sBattleRuntime.manifest));
    memset(sBattleRuntime.bundles, 0, sizeof(sBattleRuntime.bundles));
    sBattleRuntime.latest_turn = 0;
    sBattleRuntime.queued_bundles = 0;
    sBattleRuntime.consumed_bundles = 0;
    sBattleRuntime.pause_turn = 0;
    sBattleRuntime.missing_slot = 0;
    sBattleRuntime.manifest_valid = FALSE;
    sBattleRuntime.pause_valid = FALSE;
    memset(sBattleRuntime.snapshot_id, 0, sizeof(sBattleRuntime.snapshot_id));
    sBattleRuntime.snapshot_count = 0;
    sBattleRuntime.next_snapshot_slot = 0;
    sBattleRuntime.peer_party_count = 0;
    sBattleRuntime.next_peer_party_slot = 0;
    memset(sBattleRuntime.peer_party, 0, sizeof(sBattleRuntime.peer_party));
    sBattleRuntime.last_action_turn = 0;
    memset(sBattleRuntime.last_action, 0, sizeof(sBattleRuntime.last_action));
    sBattleRuntime.last_hash_turn = 0;
    sBattleRuntime.last_taken_turn = 0;
    sBattleRuntime.ready_sent = FALSE;
    sBattleRuntime.ready_replay_pending = FALSE;
    sBattleRuntime.start_released = FALSE;
    sBattleRuntime.terminal_pending = FALSE;
    sBattleRuntime.terminal_sent = FALSE;
    sBattleRuntime.hash_pending = FALSE;
    sBattleRuntime.pending_hash_turn = 0;
    sBattleRuntime.pending_hash_outcome = 0;
    memset(sBattleRuntime.pending_hash_digest, 0, sizeof(sBattleRuntime.pending_hash_digest));
    if (!retain_terminal)
    {
        sBattleRuntime.terminal_turn = 0;
        sBattleRuntime.terminal_result = 0;
        memset(sBattleRuntime.terminal_digest, 0, sizeof(sBattleRuntime.terminal_digest));
    }
    /* A transport replay of a terminal frame must survive engine cleanup: the
     * sidecar may have reset the bridge immediately after the ROM queued the
     * result.  Epoch changes clear this replay in OnSessionReady, while a
     * same-epoch retry remains idempotent at the launcher. */
    {
        u8 index;
        u8 retained = 0;
        for (index = sBattleRuntime.replay_next; index < sBattleRuntime.replay_count; index++)
        {
            if (sBattleRuntime.replay[index].type == COOP_BRIDGE_MESSAGE_BATTLE_ABORT_REQUEST
             || sBattleRuntime.replay[index].type == COOP_BRIDGE_MESSAGE_BATTLE_FINISHED
             || sBattleRuntime.replay[index].type == COOP_BRIDGE_MESSAGE_COMMIT_APPLIED)
                sBattleRuntime.replay[retained++] = sBattleRuntime.replay[index];
        }
        sBattleRuntime.replay_next = 0;
        sBattleRuntime.replay_count = retained;
    }
    sBattleRuntime.abort_requested = FALSE;
    /* An abort must never hand B2 back to the in-game partner AI. The future
     * battle cleanup callback must disarm this latch after restoring party. */
    sBattleRuntime.engine_active = was_active;
    sBattleRuntime.engine_faulted = was_active;
    sBattleRuntime.engine_action_submitted = FALSE;
    sBattleRuntime.engine_bundle_ready = FALSE;
    memset(&sBattleRuntime.engine_peer_action, 0, sizeof(sBattleRuntime.engine_peer_action));
}

void CoopBattleRuntime_Init(void)
{
    memset(&sBattleRuntime, 0, sizeof(sBattleRuntime));
    sBattleRuntime.terminal_trainer_region = COOP_REGION_UNSPECIFIED;
}

void CoopBattleRuntime_OnSessionReady(u32 epoch)
{
    bool8 same_epoch_resume = sBattleRuntime.epoch == epoch && epoch != 0;
    u8 read;
    u8 retained;

    if (sBattleRuntime.epoch != epoch)
    {
        ClearBattle();
        ClearTerminalCommit();
        sBattleRuntime.replay_next = 0;
        sBattleRuntime.replay_count = 0;
    }
    if (same_epoch_resume)
    {
        /* Old readiness frames were certified before this replacement
         * generation. Pre-engine readiness must be recomputed against the
         * live party; an active engine replays its anchored digest below. */
        retained = 0;
        for (read = sBattleRuntime.replay_next;
             read < sBattleRuntime.replay_count; read++)
        {
            if (sBattleRuntime.replay[read].type == COOP_BRIDGE_MESSAGE_BATTLE_READY)
                continue;
            sBattleRuntime.replay[retained++] = sBattleRuntime.replay[read];
        }
        sBattleRuntime.replay_next = 0;
        sBattleRuntime.replay_count = retained;
    }
    if (same_epoch_resume && !sBattleRuntime.engine_active)
    {
        /* The control connection may have lost BATTLE_START after the
         * server recorded both ready receipts. Re-emit ROM readiness so the
         * new launcher generation can release start again. */
        sBattleRuntime.ready_sent = FALSE;
        sBattleRuntime.start_released = FALSE;
    }
    else if (same_epoch_resume && sBattleRuntime.ready_sent)
    {
        /* The original pre-battle digest remains in the manifest while the
         * engine owns a staged party. Prove this active ROM still holds that
         * manifest to the replacement launcher before replaying actions. */
        sBattleRuntime.ready_replay_pending = TRUE;
    }
    sBattleRuntime.epoch = epoch;
    sBattleRuntime.session_ready = TRUE;
}

static bool8 IsNonzeroBytes(const u8 *bytes, u16 length)
{
    u16 i;

    if (bytes == NULL)
        return FALSE;
    for (i = 0; i < length; i++)
        if (bytes[i] != 0)
            return TRUE;
    return FALSE;
}

static u64 ReadCommitRevision(const u8 *payload)
{
    u64 revision = 0;
    u8 i;

    for (i = 0; i < 8; i++)
        revision |= (u64)payload[COOP_BATTLE_COMMIT_SOURCE_REVISION_OFFSET + i]
            << (i * 8);
    return revision;
}

static void ClearTerminalCommit(void)
{
    memset(sBattleRuntime.terminal_battle_id, 0,
           sizeof(sBattleRuntime.terminal_battle_id));
    sBattleRuntime.terminal_trainer_region = COOP_REGION_UNSPECIFIED;
    sBattleRuntime.terminal_trainer_ordinal = 0;
    sBattleRuntime.terminal_identity_valid = FALSE;
    sBattleRuntime.terminal_commit_pending = FALSE;
    sBattleRuntime.terminal_commit_accepted = FALSE;
    memset(sBattleRuntime.terminal_commit, 0, sizeof(sBattleRuntime.terminal_commit));
}

static void RetainTerminalIdentity(void)
{
    if (!sBattleRuntime.manifest_valid
     || sBattleRuntime.manifest[COOP_BATTLE_MANIFEST_KIND_OFFSET]
        != COOP_BATTLE_MANIFEST_KIND_TRAINER)
        return;
    memcpy(sBattleRuntime.terminal_battle_id, sBattleRuntime.manifest,
           COOP_BATTLE_ID_SIZE);
    sBattleRuntime.terminal_trainer_region =
        sBattleRuntime.manifest[COOP_BATTLE_MANIFEST_REGION_OFFSET];
    sBattleRuntime.terminal_trainer_ordinal = ReadManifestTrainerOrdinal(sBattleRuntime.manifest);
    sBattleRuntime.terminal_identity_valid =
        IsValidId(sBattleRuntime.terminal_battle_id)
        && IsCatalogTrainer(sBattleRuntime.terminal_trainer_region,
                            sBattleRuntime.terminal_trainer_ordinal);
}

void CoopBattleRuntime_OnTransportLost(void)
{
    sBattleRuntime.session_ready = FALSE;
}

static bool8 IsOutboundBattleType(u16 type)
{
    return type == COOP_BRIDGE_MESSAGE_TRAINER_BATTLE_RESERVE
        || type == COOP_BRIDGE_MESSAGE_BATTLE_JOIN_RESPONSE
        || type == COOP_BRIDGE_MESSAGE_PARTY_SNAPSHOT
        || type == COOP_BRIDGE_MESSAGE_ACTION_INTENT
        || type == COOP_BRIDGE_MESSAGE_TURN_RESULT_HASH
        || type == COOP_BRIDGE_MESSAGE_BATTLE_FINISHED
        || type == COOP_BRIDGE_MESSAGE_COMMIT_APPLIED
        || type == COOP_BRIDGE_MESSAGE_BATTLE_READY
        || type == COOP_BRIDGE_MESSAGE_BATTLE_ABORT_REQUEST;
}

void CoopBattleRuntime_PreserveOutbound(void)
{
    const struct CoopBridgeQueue *queue = &gCoopNetBridge.game_to_network;
    u16 depth = (u16)(queue->write_index - queue->read_index);
    u8 queued = 0;
    u8 pending = sBattleRuntime.replay_count - sBattleRuntime.replay_next;
    u16 i;

    if (depth > COOP_NET_BRIDGE_QUEUE_CAPACITY)
        return;
    /* A requested cancellation supersedes every earlier battle write. Keep
     * one exact payload even if the queue plus replay backlog is full. */
    for (i = 0; i < depth; i++)
    {
        const struct CoopBridgeMessage *message =
            &queue->entries[(queue->read_index + i) & (COOP_NET_BRIDGE_QUEUE_CAPACITY - 1)];
        if (message->type == COOP_BRIDGE_MESSAGE_BATTLE_ABORT_REQUEST
         && message->length == COOP_BATTLE_ABORT_REQUEST_SIZE)
        {
            sBattleRuntime.replay[0].type = message->type;
            sBattleRuntime.replay[0].length = message->length;
            memcpy(sBattleRuntime.replay[0].payload, message->payload, message->length);
            sBattleRuntime.replay_next = 0;
            sBattleRuntime.replay_count = 1;
            return;
        }
    }
    for (i = sBattleRuntime.replay_next; i < sBattleRuntime.replay_count; i++)
    {
        if (sBattleRuntime.replay[i].type == COOP_BRIDGE_MESSAGE_BATTLE_ABORT_REQUEST)
        {
            sBattleRuntime.replay[0] = sBattleRuntime.replay[i];
            sBattleRuntime.replay_next = 0;
            sBattleRuntime.replay_count = 1;
            return;
        }
    }
    for (i = 0; i < depth; i++)
    {
        const struct CoopBridgeMessage *message =
            &queue->entries[(queue->read_index + i) & (COOP_NET_BRIDGE_QUEUE_CAPACITY - 1)];
        if (IsOutboundBattleType(message->type)
         && message->length <= COOP_BATTLE_PARTY_SNAPSHOT_SIZE)
            queued++;
    }
    /* Published replay frames plus unpublished records are bounded by the
     * original 32-frame bridge queue while new battle sends are gated. */
    if (queued + pending > COOP_BATTLE_REPLAY_CAPACITY)
        return;
    memmove(&sBattleRuntime.replay[queued],
            &sBattleRuntime.replay[sBattleRuntime.replay_next],
            pending * sizeof(sBattleRuntime.replay[0]));
    queued = 0;
    for (i = 0; i < depth; i++)
    {
        const struct CoopBridgeMessage *message =
            &queue->entries[(queue->read_index + i) & (COOP_NET_BRIDGE_QUEUE_CAPACITY - 1)];
        if (!IsOutboundBattleType(message->type)
         || message->length > COOP_BATTLE_PARTY_SNAPSHOT_SIZE)
            continue;
        sBattleRuntime.replay[queued].type = message->type;
        sBattleRuntime.replay[queued].length = message->length;
        memcpy(sBattleRuntime.replay[queued].payload, message->payload, message->length);
        queued++;
    }
    sBattleRuntime.replay_next = 0;
    sBattleRuntime.replay_count = queued + pending;
}

void CoopBattleRuntime_PollOutboundReplay(void)
{
    u8 ready[COOP_BATTLE_READY_SIZE];

    if (!sBattleRuntime.session_ready || !CoopNetBridge_CanSendBattle())
        return;
    if (sBattleRuntime.terminal_commit_pending)
    {
        if (!CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_COMMIT_APPLIED,
                                                sBattleRuntime.terminal_commit,
                                                COOP_BATTLE_COMMIT_SIZE))
            return;
        /* Keep the accepted record retained for exact replay after a
         * reconnect. A later duplicate BATTLE_COMMIT sets this flag again. */
        sBattleRuntime.terminal_commit_pending = FALSE;
    }
    if (sBattleRuntime.ready_replay_pending)
    {
        u8 slot = sBattleRuntime.manifest[COOP_BATTLE_MANIFEST_MEMBER_SLOT_OFFSET];

        if (!sBattleRuntime.manifest_valid || slot > 1
         || !sBattleRuntime.engine_active || sBattleRuntime.abort_requested)
            sBattleRuntime.ready_replay_pending = FALSE;
        else
        {
            memcpy(ready, sBattleRuntime.manifest, COOP_BATTLE_ID_SIZE);
            memcpy(ready + COOP_BATTLE_ID_SIZE,
                   &sBattleRuntime.manifest[50 + slot * COOP_BATTLE_DIGEST_SIZE],
                   COOP_BATTLE_DIGEST_SIZE);
            if (!CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_BATTLE_READY,
                                                     ready, sizeof(ready)))
                return;
            sBattleRuntime.ready_replay_pending = FALSE;
        }
    }
    while (sBattleRuntime.replay_next < sBattleRuntime.replay_count)
    {
        const struct CoopBattleReplayRecord *record =
            &sBattleRuntime.replay[sBattleRuntime.replay_next];
        if (!CoopNetBridge_EnqueueGameToNetwork(record->type, record->payload,
                                                record->length))
            return;
        sBattleRuntime.replay_next++;
    }
    sBattleRuntime.replay_next = 0;
    sBattleRuntime.replay_count = 0;
}

bool8 CoopBattleRuntime_HasPendingOutboundReplay(void)
{
    return sBattleRuntime.replay_next < sBattleRuntime.replay_count;
}

enum CoopBattleInboundResult CoopBattleRuntime_ReceiveManifest(const u8 *payload, u16 length)
{
    u16 turn;
    if (payload == NULL || length != COOP_BATTLE_MANIFEST_SIZE || !IsValidId(payload)
     || !IsValidManifestIdentity(payload))
        return COOP_BATTLE_INBOUND_MALFORMED;
    turn = ReadTurn(payload);
    if (turn > COOP_BATTLE_MAX_TURN)
        return COOP_BATTLE_INBOUND_MALFORMED;
    if (!sBattleRuntime.session_ready || sBattleRuntime.manifest_valid
     || sBattleRuntime.terminal_identity_valid)
        return COOP_BATTLE_INBOUND_IGNORED;
    memcpy(sBattleRuntime.manifest, payload, length);
    sBattleRuntime.manifest_valid = TRUE;
    sBattleRuntime.latest_turn = turn;
    sBattleRuntime.last_action_turn = turn;
    sBattleRuntime.last_hash_turn = turn;
    sBattleRuntime.last_taken_turn = turn;
    return COOP_BATTLE_INBOUND_ACCEPTED;
}

enum CoopBattleInboundResult CoopBattleRuntime_ReceivePeerPartyChunk(const u8 *payload, u16 length)
{
    struct Pokemon checked;
    u16 species;
    u8 slot;
    u8 count;

    if (payload == NULL || length != COOP_BATTLE_PARTY_SNAPSHOT_SIZE
     || !IsValidId(payload))
        return COOP_BATTLE_INBOUND_MALFORMED;
    slot = payload[16];
    count = payload[18];
    if (count == 0 || count > 6 || slot >= count
     || payload[17] != slot || payload[19] != COOP_BATTLE_PARTY_MON_SIZE
     || sizeof(checked) != COOP_BATTLE_PARTY_MON_SIZE)
        return COOP_BATTLE_INBOUND_MALFORMED;

    /* GetMonData may mark a bad checksum on its argument. Keep the wire
     * bytes and the buffered party untouched until validation succeeds. */
    memcpy(&checked, payload + 20, sizeof(checked));
    species = GetMonData(&checked, MON_DATA_SPECIES);
    if (species == SPECIES_NONE || species >= NUM_SPECIES
     || !GetMonData(&checked, MON_DATA_SANITY_HAS_SPECIES)
     || GetMonData(&checked, MON_DATA_SANITY_IS_BAD_EGG))
        return COOP_BATTLE_INBOUND_MALFORMED;
    if (!sBattleRuntime.session_ready || !IsCurrentBattle(payload))
        return COOP_BATTLE_INBOUND_IGNORED;
    if (sBattleRuntime.peer_party_count != 0
     && sBattleRuntime.peer_party_count != count)
        return COOP_BATTLE_INBOUND_MALFORMED;
    if (slot < sBattleRuntime.next_peer_party_slot)
    {
        if (memcmp(sBattleRuntime.peer_party[slot], payload + 20,
                   COOP_BATTLE_PARTY_MON_SIZE) != 0)
            return COOP_BATTLE_INBOUND_MALFORMED;
        return COOP_BATTLE_INBOUND_IGNORED;
    }
    if (slot != sBattleRuntime.next_peer_party_slot)
        return COOP_BATTLE_INBOUND_IGNORED;
    memcpy(sBattleRuntime.peer_party[slot], payload + 20, COOP_BATTLE_PARTY_MON_SIZE);
    sBattleRuntime.peer_party_count = count;
    sBattleRuntime.next_peer_party_slot++;
    return COOP_BATTLE_INBOUND_ACCEPTED;
}

bool8 CoopBattleRuntime_CopyPeerParty(u8 *battle_id, u8 *count, u8 *mons, u16 capacity)
{
    u16 size = sBattleRuntime.peer_party_count * COOP_BATTLE_PARTY_MON_SIZE;
    if (!sBattleRuntime.session_ready || !sBattleRuntime.manifest_valid
     || sBattleRuntime.peer_party_count == 0
     || sBattleRuntime.next_peer_party_slot != sBattleRuntime.peer_party_count
     || battle_id == NULL || count == NULL || mons == NULL || capacity < size)
        return FALSE;
    memcpy(battle_id, sBattleRuntime.manifest, COOP_BATTLE_ID_SIZE);
    *count = sBattleRuntime.peer_party_count;
    memcpy(mons, sBattleRuntime.peer_party, size);
    return TRUE;
}

static bool8 IsUsableBattleMon(const void *mon)
{
    struct Pokemon checked;
    u16 species;

    memcpy(&checked, mon, sizeof(checked));
    species = GetMonData(&checked, MON_DATA_SPECIES);
    return species != SPECIES_NONE && species < NUM_SPECIES
        && GetMonData(&checked, MON_DATA_SANITY_HAS_SPECIES)
        && !GetMonData(&checked, MON_DATA_SANITY_IS_BAD_EGG)
        && !GetMonData(&checked, MON_DATA_IS_EGG)
        && GetMonData(&checked, MON_DATA_HP) != 0;
}

bool8 CoopBattleRuntime_PreparePartnerParty(const u8 *battle_id,
                                           const struct Pokemon *local_party,
                                           u8 local_count,
                                           struct CoopBattlePreparedParty *prepared)
{
    u8 local_slots[COOP_BATTLE_MULTI_PARTY_SIZE];
    u8 peer_slots[COOP_BATTLE_MULTI_PARTY_SIZE];
    u8 selected_local = 0;
    u8 selected_peer = 0;
    u8 i;

    if (battle_id == NULL || local_party == NULL || prepared == NULL
     || local_count == 0 || local_count > PARTY_SIZE
     || sizeof(struct Pokemon) != COOP_BATTLE_PARTY_MON_SIZE
     || !sBattleRuntime.session_ready || !sBattleRuntime.manifest_valid
     || memcmp(battle_id, sBattleRuntime.manifest, COOP_BATTLE_ID_SIZE) != 0
     || sBattleRuntime.peer_party_count == 0
     || sBattleRuntime.peer_party_count > PARTY_SIZE
     || sBattleRuntime.next_peer_party_slot != sBattleRuntime.peer_party_count)
        return FALSE;

    /* Select up to the first three usable mons in their original snapshot
     * order. Both ROMs run this same selection over byte-identical records
     * (the peer snapshot is digest-checked against the manifest), so each
     * member's staged side and its size agree on both ROMs. Every source slot
     * remains available for later battle restoration. */
    for (i = 0; i < sBattleRuntime.peer_party_count
              && selected_peer < COOP_BATTLE_MULTI_PARTY_SIZE; i++)
        if (IsUsableBattleMon(sBattleRuntime.peer_party[i]))
            peer_slots[selected_peer++] = i;
    if (selected_peer == 0)
        return FALSE;

    for (i = 0; i < local_count && selected_local < COOP_BATTLE_MULTI_PARTY_SIZE; i++)
        if (IsUsableBattleMon(&local_party[i]))
            local_slots[selected_local++] = i;
    if (selected_local == 0)
        return FALSE;

    for (i = 0; i < COOP_BATTLE_MULTI_PARTY_SIZE; i++)
    {
        if (i < selected_local)
        {
            prepared->local_slots[i] = local_slots[i];
            memcpy(&prepared->local[i], &local_party[local_slots[i]], sizeof(struct Pokemon));
        }
        else
        {
            prepared->local_slots[i] = COOP_BATTLE_UNUSED_SLOT;
            ZeroMonData(&prepared->local[i]);
        }
        if (i < selected_peer)
        {
            prepared->peer_slots[i] = peer_slots[i];
            memcpy(&prepared->peer[i], sBattleRuntime.peer_party[peer_slots[i]], sizeof(struct Pokemon));
        }
        else
        {
            prepared->peer_slots[i] = COOP_BATTLE_UNUSED_SLOT;
            ZeroMonData(&prepared->peer[i]);
        }
    }
    prepared->local_count = selected_local;
    prepared->peer_count = selected_peer;
    return TRUE;
}

bool8 CoopBattleRuntime_MakeStartupPlan(const u8 *battle_id,
                                        const struct Pokemon *local_party,
                                        u8 local_count,
                                        struct CoopBattleStartupPlan *plan)
{
    struct CoopBattlePreparedParty prepared;
    struct CoopBattleManifestIdentity identity;
    u8 local_digest[COOP_BATTLE_DIGEST_SIZE];
    enum CoopRegion active_region;
    u16 opponent_trainer_id;
    u8 i;

    if (plan == NULL || local_party == NULL || battle_id == NULL
     || local_count > PARTY_SIZE
     || !CoopBattleRuntime_GetManifestIdentity(battle_id, &identity)
     || identity.kind != COOP_BATTLE_MANIFEST_KIND_TRAINER
     || !CoopBattleRuntime_ComputePartyDigest(local_party, local_count,
                                              local_digest, sizeof(local_digest))
     || memcmp(local_digest,
               &sBattleRuntime.manifest[50 + identity.local_member_slot * COOP_BATTLE_DIGEST_SIZE],
               sizeof(local_digest)) != 0)
        return FALSE;
    /* The responder resolves the opponent from the attested identity, and
     * only while standing in that identity's region: an ordinal is never
     * interpreted against another region's trainer table. */
    if (!CoopRegion_TryGetActive(&active_region)
     || active_region != identity.trainer_region
     || !CoopIdentity_ResolveTrainerLegacyId(identity.trainer_region,
                                             identity.trainer_ordinal,
                                             &opponent_trainer_id)
     || opponent_trainer_id == TRAINER_NONE
     || opponent_trainer_id >= TRAINERS_COUNT)
        return FALSE;
    if (!CoopBattleRuntime_PreparePartnerParty(battle_id, local_party,
                                               local_count, &prepared))
        return FALSE;

    memcpy(plan->battle_id, battle_id, COOP_BATTLE_ID_SIZE);
    plan->local_member_slot = identity.local_member_slot;
    plan->member_battler_positions[0] = B_POSITION_PLAYER_LEFT;
    plan->member_battler_positions[1] = B_POSITION_PLAYER_RIGHT;
    plan->member_party_trainers[0] = identity.local_member_slot == 0 ? B_TRAINER_0 : B_TRAINER_2;
    plan->member_party_trainers[1] = identity.local_member_slot == 1 ? B_TRAINER_0 : B_TRAINER_2;
    plan->opponent_trainer_id = opponent_trainer_id;
    memset(plan->original_local, 0, sizeof(plan->original_local));
    memcpy(plan->original_local, local_party, local_count * sizeof(struct Pokemon));
    plan->staged_local_count = prepared.local_count;
    plan->staged_peer_count = prepared.peer_count;
    for (i = 0; i < COOP_BATTLE_MULTI_PARTY_SIZE; i++)
    {
        plan->local_slots[i] = prepared.local_slots[i];
        plan->peer_slots[i] = prepared.peer_slots[i];
        memcpy(&plan->staged_local[i], &prepared.local[i], sizeof(struct Pokemon));
        memcpy(&plan->staged_peer[i], &prepared.peer[i], sizeof(struct Pokemon));
    }
    return TRUE;
}

bool8 CoopBattleRuntime_RestoreLocalParty(const struct CoopBattleStartupPlan *plan,
                                         const u8 *battle_id,
                                         const struct Pokemon *battled_local,
                                         struct Pokemon *restored_local)
{
    struct Pokemon fought[COOP_BATTLE_MULTI_PARTY_SIZE];
    u8 i;

    if (plan == NULL || battle_id == NULL || battled_local == NULL
     || restored_local == NULL
     || !IsValidId(plan->battle_id)
     || memcmp(plan->battle_id, battle_id, COOP_BATTLE_ID_SIZE) != 0
     || plan->staged_local_count == 0
     || plan->staged_local_count > COOP_BATTLE_MULTI_PARTY_SIZE)
        return FALSE;
    for (i = 0; i < plan->staged_local_count; i++)
        if (plan->local_slots[i] >= PARTY_SIZE)
            return FALSE;

    memcpy(fought, battled_local, plan->staged_local_count * sizeof(struct Pokemon));
    memmove(restored_local, plan->original_local, sizeof(plan->original_local));
    for (i = 0; i < plan->staged_local_count; i++)
        memcpy(&restored_local[plan->local_slots[i]], &fought[i],
               sizeof(struct Pokemon));
    return TRUE;
}

enum CoopBattleInboundResult CoopBattleRuntime_ReceiveTurnBundle(const u8 *payload, u16 length)
{
    u16 turn;
    u8 firstLength;
    u8 secondLength;
    struct CoopBattleAction checked;
    const u8 *localAction;
    if (payload == NULL || length < 22 || length > 116 || !IsValidId(payload))
        return COOP_BATTLE_INBOUND_MALFORMED;
    turn = ReadTurn(payload);
    firstLength = payload[18];
    secondLength = payload[19];
    if (turn == 0 || turn > COOP_BATTLE_MAX_TURN
     || firstLength == 0 || firstLength > COOP_BATTLE_MAX_ACTION_SIZE
     || secondLength == 0 || secondLength > COOP_BATTLE_MAX_ACTION_SIZE
     || length != 20 + firstLength + secondLength)
        return COOP_BATTLE_INBOUND_MALFORMED;
    /* Identify stale state before interpreting opaque action bytes. A delayed
     * bundle from an earlier battle or an already-consumed turn is a harmless
     * soft drop; only a current, in-order bundle is allowed to reach the
     * engine action decoder and become a protocol error. */
    if (sBattleRuntime.session_ready
     && (!IsCurrentBattle(payload)
      || sBattleRuntime.queued_bundles >= COOP_BATTLE_MAX_TURN
      || turn != sBattleRuntime.latest_turn + 1))
        return COOP_BATTLE_INBOUND_IGNORED;
    if (!CoopBattleRuntime_DecodeAction(payload + 20, firstLength, &checked)
     || !CoopBattleRuntime_DecodeAction(payload + 20 + firstLength, secondLength, &checked))
        return COOP_BATTLE_INBOUND_MALFORMED;
    if (!sBattleRuntime.session_ready)
        return COOP_BATTLE_INBOUND_IGNORED;
    localAction = payload + 20;
    if (sBattleRuntime.manifest[COOP_BATTLE_MANIFEST_MEMBER_SLOT_OFFSET] == 1)
        localAction += firstLength;
    if (turn != sBattleRuntime.last_action_turn
     || memcmp(localAction, sBattleRuntime.last_action, COOP_BATTLE_ACTION_SIZE) != 0)
        return COOP_BATTLE_INBOUND_MALFORMED;
    {
        struct CoopBattleTurnBundle *bundle = &sBattleRuntime.bundles[sBattleRuntime.queued_bundles];
        memset(bundle, 0, sizeof(*bundle));
        memcpy(bundle->battle_id, payload, COOP_BATTLE_ID_SIZE);
        bundle->turn = turn;
        bundle->first_length = firstLength;
        bundle->second_length = secondLength;
        memcpy(bundle->first_action, payload + 20, firstLength);
        memcpy(bundle->second_action, payload + 20 + firstLength, secondLength);
    }
    sBattleRuntime.latest_turn = turn;
    sBattleRuntime.queued_bundles++;
    if (sBattleRuntime.pause_valid && turn >= sBattleRuntime.pause_turn)
        sBattleRuntime.pause_valid = FALSE;
    return COOP_BATTLE_INBOUND_ACCEPTED;
}

enum CoopBattleInboundResult CoopBattleRuntime_ReceivePause(const u8 *payload, u16 length)
{
    u16 turn;
    if (payload == NULL || length != COOP_BATTLE_PAUSE_FOR_RECONNECT_SIZE
     || !IsValidId(payload) || payload[18] > 1)
        return COOP_BATTLE_INBOUND_MALFORMED;
    turn = ReadTurn(payload);
    if (turn == 0 || turn > COOP_BATTLE_MAX_TURN)
        return COOP_BATTLE_INBOUND_MALFORMED;
    if (!sBattleRuntime.session_ready || !IsCurrentBattle(payload)
     || turn <= sBattleRuntime.latest_turn
     || (sBattleRuntime.pause_valid && turn <= sBattleRuntime.pause_turn))
        return COOP_BATTLE_INBOUND_IGNORED;
    sBattleRuntime.pause_turn = turn;
    sBattleRuntime.missing_slot = payload[18];
    sBattleRuntime.pause_valid = TRUE;
    return COOP_BATTLE_INBOUND_ACCEPTED;
}

enum CoopBattleInboundResult CoopBattleRuntime_ReceiveAbort(const u8 *payload, u16 length)
{
    if (payload == NULL || length != COOP_BATTLE_ABORT_SIZE || !IsValidId(payload)
     || payload[16] < COOP_BATTLE_ABORT_CANCELED
     || payload[16] > COOP_BATTLE_ABORT_UNAVAILABLE)
        return COOP_BATTLE_INBOUND_MALFORMED;
    if (!sBattleRuntime.session_ready)
        return COOP_BATTLE_INBOUND_IGNORED;
    if (!IsCurrentBattle(payload))
    {
        if ((sBattleRuntime.snapshot_count == 0
          || memcmp(sBattleRuntime.snapshot_id, payload, COOP_BATTLE_ID_SIZE) != 0)
         && (!sBattleRuntime.terminal_identity_valid
          || memcmp(sBattleRuntime.terminal_battle_id, payload,
                    COOP_BATTLE_ID_SIZE) != 0))
            return COOP_BATTLE_INBOUND_IGNORED;
    }
    ClearTerminalCommit();
    ClearBattle();
    return COOP_BATTLE_INBOUND_ACCEPTED;
}

bool8 CoopBattleRuntime_HasManifest(void)
{
    return sBattleRuntime.manifest_valid;
}

bool8 CoopBattleRuntime_GetManifest(u8 *payload, u16 capacity)
{
    if (!sBattleRuntime.manifest_valid || payload == NULL
     || capacity < COOP_BATTLE_MANIFEST_SIZE)
        return FALSE;
    memcpy(payload, sBattleRuntime.manifest, COOP_BATTLE_MANIFEST_SIZE);
    return TRUE;
}

bool8 CoopBattleRuntime_GetManifestIdentity(const u8 *battle_id,
                                           struct CoopBattleManifestIdentity *identity)
{
    if (!sBattleRuntime.session_ready || battle_id == NULL || identity == NULL
     || !IsCurrentBattle(battle_id))
        return FALSE;
    identity->kind = sBattleRuntime.manifest[COOP_BATTLE_MANIFEST_KIND_OFFSET];
    identity->local_member_slot = sBattleRuntime.manifest[COOP_BATTLE_MANIFEST_MEMBER_SLOT_OFFSET];
    identity->trainer_region = sBattleRuntime.manifest[COOP_BATTLE_MANIFEST_REGION_OFFSET];
    identity->trainer_ordinal = ReadManifestTrainerOrdinal(sBattleRuntime.manifest);
    return TRUE;
}

enum CoopBattleInboundResult CoopBattleRuntime_ReceiveStart(const u8 *payload, u16 length)
{
    if (payload == NULL || length != COOP_BATTLE_START_SIZE || !IsValidId(payload))
        return COOP_BATTLE_INBOUND_MALFORMED;
    if (!sBattleRuntime.session_ready || !IsCurrentBattle(payload)
     || !sBattleRuntime.ready_sent || sBattleRuntime.abort_requested
     || sBattleRuntime.next_peer_party_slot != sBattleRuntime.peer_party_count
     || sBattleRuntime.peer_party_count == 0)
        return COOP_BATTLE_INBOUND_IGNORED;
    if (sBattleRuntime.start_released)
        return COOP_BATTLE_INBOUND_IGNORED;
    sBattleRuntime.start_released = TRUE;
    return COOP_BATTLE_INBOUND_ACCEPTED;
}

enum CoopBattleInboundResult CoopBattleRuntime_ReceiveBattleCommit(const u8 *payload,
                                                                   u16 length)
{
    u16 ordinal;

    if (payload == NULL || length != COOP_BATTLE_COMMIT_SIZE
     || !IsValidId(payload + COOP_BATTLE_COMMIT_BATTLE_ID_OFFSET)
     || !IsNonzeroBytes(payload + COOP_BATTLE_COMMIT_ID_OFFSET, COOP_BATTLE_ID_SIZE)
     || !IsCatalogTrainer(payload[COOP_BATTLE_COMMIT_REGION_OFFSET],
                          payload[COOP_BATTLE_COMMIT_TRAINER_ORDINAL_OFFSET]
                          | ((u16)payload[COOP_BATTLE_COMMIT_TRAINER_ORDINAL_OFFSET + 1] << 8))
     || !IsNonzeroBytes(payload + COOP_BATTLE_COMMIT_SOURCE_REVISION_OFFSET, 8))
        return COOP_BATTLE_INBOUND_MALFORMED;

    ordinal = payload[COOP_BATTLE_COMMIT_TRAINER_ORDINAL_OFFSET]
        | ((u16)payload[COOP_BATTLE_COMMIT_TRAINER_ORDINAL_OFFSET + 1] << 8);
    if (!sBattleRuntime.session_ready || !sBattleRuntime.terminal_identity_valid
     || sBattleRuntime.terminal_result != COOP_BATTLE_FINISHED_WON
     || memcmp(payload + COOP_BATTLE_COMMIT_BATTLE_ID_OFFSET,
               sBattleRuntime.terminal_battle_id, COOP_BATTLE_ID_SIZE) != 0
     || payload[COOP_BATTLE_COMMIT_REGION_OFFSET] != sBattleRuntime.terminal_trainer_region
     || ordinal != sBattleRuntime.terminal_trainer_ordinal)
        return COOP_BATTLE_INBOUND_IGNORED;

    /* A same-epoch reconnect may replay a grant. Only the exact original
     * record is idempotent; a second commit for this battle is stale. */
    if (sBattleRuntime.terminal_commit_pending
     || sBattleRuntime.terminal_commit_accepted)
    {
        if (memcmp(payload, sBattleRuntime.terminal_commit,
                   COOP_BATTLE_COMMIT_SIZE) != 0)
            return COOP_BATTLE_INBOUND_IGNORED;
        sBattleRuntime.terminal_commit_pending = TRUE;
        return COOP_BATTLE_INBOUND_ACCEPTED;
    }

    /* Keep the source revision read explicit in this validation path. The
     * wire rule is nonzero, and the full eight bytes are retained verbatim
     * for the launcher/save layer's stronger CAS check. */
    if (ReadCommitRevision(payload) == 0)
        return COOP_BATTLE_INBOUND_MALFORMED;
    memcpy(sBattleRuntime.terminal_commit, payload, COOP_BATTLE_COMMIT_SIZE);
    sBattleRuntime.terminal_commit_pending = TRUE;
    sBattleRuntime.terminal_commit_accepted = TRUE;
    return COOP_BATTLE_INBOUND_ACCEPTED;
}

bool8 CoopBattleRuntime_HasPendingBattleCommit(void)
{
    return sBattleRuntime.terminal_identity_valid
        && sBattleRuntime.terminal_commit_pending;
}

bool8 CoopBattleRuntime_HasAcceptedBattleCommit(void)
{
    return sBattleRuntime.terminal_identity_valid
        && sBattleRuntime.terminal_commit_accepted;
}

void CoopBattleRuntime_ForgetTerminalCommit(void)
{
    ClearTerminalCommit();
}

bool8 CoopBattleRuntime_IsStartReleased(void)
{
    return sBattleRuntime.session_ready && sBattleRuntime.start_released
        && !sBattleRuntime.abort_requested;
}

#if TESTING
void CoopBattleRuntime_TestSetReadyForStart(void)
{
    sBattleRuntime.ready_sent = TRUE;
    sBattleRuntime.peer_party_count = 1;
    sBattleRuntime.next_peer_party_slot = 1;
}
#endif

bool8 CoopBattleRuntime_RequestAbort(u8 reason)
{
    u8 payload[COOP_BATTLE_ABORT_REQUEST_SIZE];

    if (reason < COOP_BATTLE_ABORT_CANCELED || reason > COOP_BATTLE_ABORT_UNAVAILABLE
     || !sBattleRuntime.manifest_valid || sBattleRuntime.abort_requested)
        return FALSE;
    memcpy(payload, sBattleRuntime.manifest, COOP_BATTLE_ID_SIZE);
    payload[16] = reason;
    if (!sBattleRuntime.session_ready || !CoopNetBridge_CanSendBattle()
     || !CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_BATTLE_ABORT_REQUEST,
                                            payload, sizeof(payload)))
    {
        /* The bridge can be unavailable during battle cleanup. Preserve one
         * cancellation for same-epoch replay after the ROM returns to field. */
        sBattleRuntime.replay[0].type = COOP_BRIDGE_MESSAGE_BATTLE_ABORT_REQUEST;
        sBattleRuntime.replay[0].length = sizeof(payload);
        memcpy(sBattleRuntime.replay[0].payload, payload, sizeof(payload));
        sBattleRuntime.replay_next = 0;
        sBattleRuntime.replay_count = 1;
    }
    sBattleRuntime.abort_requested = TRUE;
    return TRUE;
}

bool8 CoopBattleRuntime_EncodeAction(const struct CoopBattleAction *action,
                                    u8 *bytes, u16 capacity)
{
    u8 encoded[COOP_BATTLE_ACTION_SIZE];
    struct CoopBattleAction checked;

    if (action == NULL || bytes == NULL || capacity < sizeof(encoded))
        return FALSE;
    encoded[0] = action->kind;
    encoded[1] = action->index;
    encoded[2] = action->target;
    encoded[3] = 0;
    if (!CoopBattleRuntime_DecodeAction(encoded, sizeof(encoded), &checked))
        return FALSE;
    memcpy(bytes, encoded, sizeof(encoded));
    return TRUE;
}

bool8 CoopBattleRuntime_DecodeAction(const u8 *bytes, u16 length,
                                    struct CoopBattleAction *action)
{
    if (bytes == NULL || action == NULL || length != COOP_BATTLE_ACTION_SIZE
     || bytes[3] != 0)
        return FALSE;
    switch (bytes[0])
    {
    case COOP_BATTLE_ACTION_MOVE:
        if (bytes[1] >= MAX_MON_MOVES || bytes[2] > B_POSITION_OPPONENT_RIGHT)
            return FALSE;
        break;
    case COOP_BATTLE_ACTION_SWITCH:
    case COOP_BATTLE_ACTION_FORCED_SWITCH:
        if (bytes[1] >= COOP_BATTLE_MULTI_PARTY_SIZE || bytes[2] != 0)
            return FALSE;
        break;
    case COOP_BATTLE_ACTION_NO_ACTION:
    case COOP_BATTLE_ACTION_AUTO_MOVE:
        if (bytes[1] != 0 || bytes[2] != 0)
            return FALSE;
        break;
    default:
        return FALSE;
    }
    action->kind = bytes[0];
    action->index = bytes[1];
    action->target = bytes[2];
    return TRUE;
}

u8 CoopBattleRuntime_TranslateTarget(u8 position, u8 local_member_slot)
{
    if (local_member_slot > 1 || position > B_POSITION_OPPONENT_RIGHT)
        return 0xFF;
    if (local_member_slot == 1 && (position == B_POSITION_PLAYER_LEFT
                                || position == B_POSITION_PLAYER_RIGHT))
        return position ^ B_POSITION_PLAYER_RIGHT;
    return position;
}

bool8 CoopBattleRuntime_ArmEngine(const u8 *battle_id)
{
    if (battle_id == NULL || !sBattleRuntime.session_ready
     || !IsCurrentBattle(battle_id) || sBattleRuntime.engine_active
     || sBattleRuntime.manifest[COOP_BATTLE_MANIFEST_KIND_OFFSET] != COOP_BATTLE_MANIFEST_KIND_TRAINER
     || sBattleRuntime.latest_turn != 0
     || sBattleRuntime.peer_party_count == 0
     || sBattleRuntime.next_peer_party_slot != sBattleRuntime.peer_party_count)
        return FALSE;
    sBattleRuntime.engine_active = TRUE;
    sBattleRuntime.engine_faulted = FALSE;
    RetainTerminalIdentity();
    return TRUE;
}

void CoopBattleRuntime_DisarmEngine(void)
{
    sBattleRuntime.engine_active = FALSE;
    sBattleRuntime.engine_faulted = FALSE;
    sBattleRuntime.engine_action_submitted = FALSE;
    sBattleRuntime.engine_bundle_ready = FALSE;
    memset(&sBattleRuntime.engine_peer_action, 0, sizeof(sBattleRuntime.engine_peer_action));
}

bool8 CoopBattleRuntime_IsEngineActive(void)
{
    return sBattleRuntime.engine_active;
}

bool8 CoopBattleRuntime_IsEngineFaulted(void)
{
    return sBattleRuntime.engine_active && sBattleRuntime.engine_faulted;
}

bool8 CoopBattleRuntime_IsSessionReady(void)
{
    return sBattleRuntime.session_ready;
}

u8 CoopBattleRuntime_EngineLocalMemberSlot(void)
{
    if (!sBattleRuntime.engine_active)
        return 0xFF;
    return sBattleRuntime.manifest[COOP_BATTLE_MANIFEST_MEMBER_SLOT_OFFSET];
}

bool8 CoopBattleRuntime_IsLocalActionSubmitted(void)
{
    return sBattleRuntime.engine_active && sBattleRuntime.engine_action_submitted;
}

bool8 CoopBattleRuntime_SubmitLocalAction(const struct CoopBattleAction *action)
{
    u8 encoded[COOP_BATTLE_ACTION_SIZE];

    if (!sBattleRuntime.engine_active || sBattleRuntime.engine_faulted
     || sBattleRuntime.engine_action_submitted
     || !CoopBattleRuntime_EncodeAction(action, encoded, sizeof(encoded))
     || !CoopBattleRuntime_SendActionIntent(sBattleRuntime.manifest,
                                            sBattleRuntime.latest_turn + 1,
                                            encoded, sizeof(encoded)))
        return FALSE;
    sBattleRuntime.engine_action_submitted = TRUE;
    return TRUE;
}

bool8 CoopBattleRuntime_PollPeerAction(struct CoopBattleAction *action)
{
    struct CoopBattleTurnBundle bundle;
    const u8 *peer;
    u8 peerLength;

    if (!sBattleRuntime.engine_active || sBattleRuntime.engine_faulted
     || !sBattleRuntime.engine_action_submitted
     || !sBattleRuntime.session_ready || !CoopNetBridge_CanSendBattle()
     || sBattleRuntime.replay_next < sBattleRuntime.replay_count
     || sBattleRuntime.pause_valid || action == NULL)
        return FALSE;
    if (!sBattleRuntime.engine_bundle_ready)
    {
        if (!CoopBattleRuntime_TakeTurnBundle(&bundle))
            return FALSE;
        if (sBattleRuntime.manifest[COOP_BATTLE_MANIFEST_MEMBER_SLOT_OFFSET] == 0)
            peer = bundle.second_action, peerLength = bundle.second_length;
        else
            peer = bundle.first_action, peerLength = bundle.first_length;
        if (!CoopBattleRuntime_DecodeAction(peer, peerLength,
                                            &sBattleRuntime.engine_peer_action))
            return FALSE;
        sBattleRuntime.engine_bundle_ready = TRUE;
    }
    *action = sBattleRuntime.engine_peer_action;
    return TRUE;
}

bool8 CoopBattleRuntime_ConfirmPeerAutomaticAction(u8 expected_kind)
{
    struct CoopBattleAction action;

    if (expected_kind != COOP_BATTLE_ACTION_AUTO_MOVE
     && expected_kind != COOP_BATTLE_ACTION_NO_ACTION)
        return FALSE;
    if (!CoopBattleRuntime_PollPeerAction(&action))
        return FALSE;
    if (action.kind != expected_kind)
    {
        sBattleRuntime.engine_faulted = TRUE;
        return FALSE;
    }
    return TRUE;
}

void CoopBattleRuntime_FailEngine(void)
{
    if (sBattleRuntime.engine_active)
        sBattleRuntime.engine_faulted = TRUE;
}

bool8 CoopBattleRuntime_IsEngineTurnReady(void)
{
    return sBattleRuntime.engine_active && !sBattleRuntime.engine_faulted
        && sBattleRuntime.engine_action_submitted
        && sBattleRuntime.engine_bundle_ready && sBattleRuntime.session_ready
        && CoopNetBridge_CanSendBattle()
        && sBattleRuntime.replay_next == sBattleRuntime.replay_count
        && !sBattleRuntime.pause_valid;
}

void CoopBattleRuntime_FinishEngineTurn(void)
{
    if (!CoopBattleRuntime_IsEngineTurnReady())
        return;
    sBattleRuntime.engine_action_submitted = FALSE;
    sBattleRuntime.engine_bundle_ready = FALSE;
    memset(&sBattleRuntime.engine_peer_action, 0, sizeof(sBattleRuntime.engine_peer_action));
}

bool8 CoopBattleRuntime_TakeTurnBundle(struct CoopBattleTurnBundle *bundle)
{
    if (sBattleRuntime.consumed_bundles >= sBattleRuntime.queued_bundles || bundle == NULL)
        return FALSE;
    *bundle = sBattleRuntime.bundles[sBattleRuntime.consumed_bundles++];
    sBattleRuntime.last_taken_turn = bundle->turn;
    return TRUE;
}

bool8 CoopBattleRuntime_GetPause(u16 *turn, u8 *missing_slot)
{
    if (!sBattleRuntime.pause_valid || turn == NULL || missing_slot == NULL)
        return FALSE;
    *turn = sBattleRuntime.pause_turn;
    *missing_slot = sBattleRuntime.missing_slot;
    return TRUE;
}

bool8 CoopBattleRuntime_SendPartySnapshot(const u8 *battle_id, u8 party_slot,
                                           u8 party_count, const u8 *mon, u16 mon_size)
{
    u8 payload[COOP_BATTLE_PARTY_SNAPSHOT_SIZE];
    struct Pokemon checked;
    u16 species;

    if (battle_id == NULL || mon == NULL || mon_size != sizeof(checked)
     || mon_size != COOP_BATTLE_PARTY_MON_SIZE || !sBattleRuntime.session_ready
     || !CoopNetBridge_CanSendBattle()
     || sBattleRuntime.replay_next < sBattleRuntime.replay_count
     || !CoopBattleConsent_IsCurrentBattle(battle_id)
     || sBattleRuntime.manifest_valid
     || party_count == 0 || party_count > 6 || party_slot >= party_count)
        return FALSE;
    if (sBattleRuntime.next_snapshot_slot == 0)
    {
        if (party_slot != 0 || (sBattleRuntime.snapshot_count != 0
         && memcmp(sBattleRuntime.snapshot_id, battle_id, COOP_BATTLE_ID_SIZE) == 0))
            return FALSE;
    }
    else if (sBattleRuntime.snapshot_count != party_count
          || party_slot != sBattleRuntime.next_snapshot_slot
          || memcmp(sBattleRuntime.snapshot_id, battle_id, COOP_BATTLE_ID_SIZE) != 0)
        return FALSE;

    /* Decode a copy: GetMonData may mark a bad checksum on its argument. */
    memcpy(&checked, mon, sizeof(checked));
    species = GetMonData(&checked, MON_DATA_SPECIES);
    if (species == SPECIES_NONE || species >= NUM_SPECIES
     || !GetMonData(&checked, MON_DATA_SANITY_HAS_SPECIES)
     || GetMonData(&checked, MON_DATA_SANITY_IS_BAD_EGG))
        return FALSE;

    memcpy(payload, battle_id, COOP_BATTLE_ID_SIZE);
    payload[16] = party_slot;
    payload[17] = party_slot;
    payload[18] = party_count;
    payload[19] = COOP_BATTLE_PARTY_MON_SIZE;
    memcpy(payload + 20, mon, COOP_BATTLE_PARTY_MON_SIZE);
    if (!CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_PARTY_SNAPSHOT,
                                             payload, sizeof(payload)))
        return FALSE;
    if (party_slot == 0)
    {
        memcpy(sBattleRuntime.snapshot_id, battle_id, COOP_BATTLE_ID_SIZE);
        sBattleRuntime.snapshot_count = party_count;
    }
    sBattleRuntime.next_snapshot_slot++;
    return TRUE;
}

bool8 CoopBattleRuntime_SendActionIntent(const u8 *battle_id, u16 turn,
                                         const u8 *action, u16 action_size)
{
    u8 payload[19 + COOP_BATTLE_MAX_ACTION_SIZE];
    struct CoopBattleAction checked;

    if (battle_id == NULL || action == NULL || action_size == 0
     || action_size > COOP_BATTLE_MAX_ACTION_SIZE || turn == 0
     || turn > COOP_BATTLE_MAX_TURN || !sBattleRuntime.session_ready
     || !CoopNetBridge_CanSendBattle() || !IsCurrentBattle(battle_id)
     || sBattleRuntime.replay_next < sBattleRuntime.replay_count
     || turn != sBattleRuntime.latest_turn + 1
     || turn != sBattleRuntime.last_action_turn + 1
     || !CoopBattleRuntime_DecodeAction(action, action_size, &checked))
        return FALSE;
    memcpy(payload, battle_id, COOP_BATTLE_ID_SIZE);
    payload[16] = turn;
    payload[17] = turn >> 8;
    payload[18] = action_size;
    memcpy(payload + 19, action, action_size);
    if (!CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_ACTION_INTENT,
                                             payload, 19 + action_size))
        return FALSE;
    sBattleRuntime.last_action_turn = turn;
    memcpy(sBattleRuntime.last_action, action, COOP_BATTLE_ACTION_SIZE);
    return TRUE;
}

bool8 CoopBattleRuntime_SendTurnResultHash(const u8 *battle_id, u16 turn,
                                           const u8 *digest, u16 digest_size)
{
    u8 payload[COOP_BATTLE_TURN_RESULT_HASH_SIZE];

    if (battle_id == NULL || digest == NULL || digest_size != COOP_BATTLE_DIGEST_SIZE
     || turn == 0 || turn > COOP_BATTLE_MAX_TURN || !sBattleRuntime.session_ready
     || !CoopNetBridge_CanSendBattle() || !IsCurrentBattle(battle_id)
     || sBattleRuntime.replay_next < sBattleRuntime.replay_count
     || turn != sBattleRuntime.last_hash_turn + 1
     || turn != sBattleRuntime.last_taken_turn
     || turn != sBattleRuntime.last_action_turn)
        return FALSE;
    memcpy(payload, battle_id, COOP_BATTLE_ID_SIZE);
    payload[16] = turn;
    payload[17] = turn >> 8;
    memcpy(payload + 18, digest, COOP_BATTLE_DIGEST_SIZE);
    if (!CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_TURN_RESULT_HASH,
                                             payload, sizeof(payload)))
        return FALSE;
    sBattleRuntime.last_hash_turn = turn;
    return TRUE;
}

bool8 CoopBattleRuntime_PollLocalSnapshot(const u8 *battle_id,
                                          const struct Pokemon *party, u8 count)
{
    u8 next = sBattleRuntime.next_snapshot_slot;

    if (battle_id == NULL || party == NULL || count == 0 || count > PARTY_SIZE
     || sBattleRuntime.manifest_valid || sBattleRuntime.abort_requested
     || (sBattleRuntime.snapshot_count != 0
      && (sBattleRuntime.snapshot_count != count
       || memcmp(sBattleRuntime.snapshot_id, battle_id, COOP_BATTLE_ID_SIZE) != 0)))
        return FALSE;
    if (next == count)
        return TRUE;
    if (next > count)
        return FALSE;
    return CoopBattleRuntime_SendPartySnapshot(battle_id, next, count,
                                               (const u8 *)&party[next],
                                               sizeof(party[next]));
}

bool8 CoopBattleRuntime_TrySendReady(const u8 *battle_id,
                                     const struct Pokemon *party, u8 count)
{
    u8 payload[COOP_BATTLE_READY_SIZE];
    u8 local_slot;

    if (battle_id == NULL || party == NULL || count == 0 || count > PARTY_SIZE
     || !sBattleRuntime.session_ready || !IsCurrentBattle(battle_id)
     || sBattleRuntime.abort_requested || sBattleRuntime.engine_active
     || sBattleRuntime.snapshot_count != count
     || sBattleRuntime.next_snapshot_slot != count
     || memcmp(sBattleRuntime.snapshot_id, battle_id, COOP_BATTLE_ID_SIZE) != 0
     || sBattleRuntime.peer_party_count == 0
     || sBattleRuntime.next_peer_party_slot != sBattleRuntime.peer_party_count
     || !CoopBattleConsent_IsCurrentBattle(battle_id))
        return FALSE;
    if (sBattleRuntime.ready_sent)
        return TRUE;
    if (!CoopNetBridge_CanSendBattle()
     || sBattleRuntime.replay_next < sBattleRuntime.replay_count)
        return FALSE;
    local_slot = sBattleRuntime.manifest[COOP_BATTLE_MANIFEST_MEMBER_SLOT_OFFSET];
    memcpy(payload, battle_id, COOP_BATTLE_ID_SIZE);
    if (!CoopBattleRuntime_ComputePartyDigest(party, count,
                                               payload + COOP_BATTLE_ID_SIZE,
                                               COOP_BATTLE_DIGEST_SIZE)
     || memcmp(payload + COOP_BATTLE_ID_SIZE,
               &sBattleRuntime.manifest[50 + local_slot * COOP_BATTLE_DIGEST_SIZE],
               COOP_BATTLE_DIGEST_SIZE) != 0
     || !CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_BATTLE_READY,
                                             payload, sizeof(payload)))
        return FALSE;
    sBattleRuntime.ready_sent = TRUE;
    return TRUE;
}

/* The battle digest is deliberately implemented here instead of depending on
 * a host crypto library.  The context lives on the caller's stack, so a
 * terminal report costs no persistent EWRAM.  All integer fields below are
 * written little-endian before the SHA-256 compression, while the SHA output
 * remains the standard big-endian digest required by battle_bridge.rs. */
struct CoopBattleSha256
{
    u32 state[8];
    u32 length_lo;
    u32 length_hi;
    u8 block[64];
    u8 used;
};

static const u32 sCoopBattleSha256K[64] =
{
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5,
    0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
    0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
    0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
    0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
    0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
    0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
    0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
};

static u32 CoopBattleSha256_RotateRight(u32 value, u8 count)
{
    return (value >> count) | (value << (32 - count));
}

static u32 CoopBattleSha256_ReadBigEndian(const u8 *bytes)
{
    return ((u32)bytes[0] << 24) | ((u32)bytes[1] << 16)
        | ((u32)bytes[2] << 8) | bytes[3];
}

static void CoopBattleSha256_Transform(struct CoopBattleSha256 *context)
{
    u32 words[64];
    u32 a, b, c, d, e, f, g, h;
    u32 i;

    for (i = 0; i < 16; i++)
        words[i] = CoopBattleSha256_ReadBigEndian(&context->block[i * 4]);
    for (i = 16; i < ARRAY_COUNT(words); i++)
    {
        u32 s0 = CoopBattleSha256_RotateRight(words[i - 15], 7)
            ^ CoopBattleSha256_RotateRight(words[i - 15], 18)
            ^ (words[i - 15] >> 3);
        u32 s1 = CoopBattleSha256_RotateRight(words[i - 2], 17)
            ^ CoopBattleSha256_RotateRight(words[i - 2], 19)
            ^ (words[i - 2] >> 10);
        words[i] = words[i - 16] + s0 + words[i - 7] + s1;
    }

    a = context->state[0];
    b = context->state[1];
    c = context->state[2];
    d = context->state[3];
    e = context->state[4];
    f = context->state[5];
    g = context->state[6];
    h = context->state[7];
    for (i = 0; i < ARRAY_COUNT(words); i++)
    {
        u32 s1 = CoopBattleSha256_RotateRight(e, 6)
            ^ CoopBattleSha256_RotateRight(e, 11)
            ^ CoopBattleSha256_RotateRight(e, 25);
        u32 choose = (e & f) ^ ((~e) & g);
        u32 temp1 = h + s1 + choose + sCoopBattleSha256K[i] + words[i];
        u32 s0 = CoopBattleSha256_RotateRight(a, 2)
            ^ CoopBattleSha256_RotateRight(a, 13)
            ^ CoopBattleSha256_RotateRight(a, 22);
        u32 majority = (a & b) ^ (a & c) ^ (b & c);
        u32 temp2 = s0 + majority;
        h = g;
        g = f;
        f = e;
        e = d + temp1;
        d = c;
        c = b;
        b = a;
        a = temp1 + temp2;
    }
    context->state[0] += a;
    context->state[1] += b;
    context->state[2] += c;
    context->state[3] += d;
    context->state[4] += e;
    context->state[5] += f;
    context->state[6] += g;
    context->state[7] += h;
}

static void CoopBattleSha256_Init(struct CoopBattleSha256 *context)
{
    static const u32 initial[8] =
    {
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    };
    memcpy(context->state, initial, sizeof(initial));
    context->length_lo = 0;
    context->length_hi = 0;
    context->used = 0;
}

static void CoopBattleSha256_Update(struct CoopBattleSha256 *context,
                                    const u8 *bytes, u32 length)
{
    u32 old_length = context->length_lo;
    u32 copied;

    context->length_lo += length << 3;
    if (context->length_lo < old_length)
        context->length_hi++;
    context->length_hi += length >> 29;
    while (length != 0)
    {
        copied = 64 - context->used;
        if (copied > length)
            copied = length;
        memcpy(&context->block[context->used], bytes, copied);
        context->used += copied;
        bytes += copied;
        length -= copied;
        if (context->used == 64)
        {
            CoopBattleSha256_Transform(context);
            context->used = 0;
        }
    }
}

static void CoopBattleSha256_Final(struct CoopBattleSha256 *context, u8 *digest)
{
    u8 i;
    u8 length_bytes[8];

    context->block[context->used++] = 0x80;
    while (context->used != 56)
    {
        if (context->used == 64)
        {
            CoopBattleSha256_Transform(context);
            context->used = 0;
        }
        context->block[context->used++] = 0;
    }
    length_bytes[0] = context->length_hi >> 24;
    length_bytes[1] = context->length_hi >> 16;
    length_bytes[2] = context->length_hi >> 8;
    length_bytes[3] = context->length_hi;
    length_bytes[4] = context->length_lo >> 24;
    length_bytes[5] = context->length_lo >> 16;
    length_bytes[6] = context->length_lo >> 8;
    length_bytes[7] = context->length_lo;
    memcpy(&context->block[56], length_bytes, sizeof(length_bytes));
    CoopBattleSha256_Transform(context);
    for (i = 0; i < ARRAY_COUNT(context->state); i++)
    {
        digest[i * 4] = context->state[i] >> 24;
        digest[i * 4 + 1] = context->state[i] >> 16;
        digest[i * 4 + 2] = context->state[i] >> 8;
        digest[i * 4 + 3] = context->state[i];
    }
}

bool8 CoopBattleRuntime_ComputePartyDigest(const struct Pokemon *party,
                                           u8 count, u8 *digest, u16 capacity)
{
    static const u8 prefix[] = "coop-battle-party-v1";
    struct CoopBattleSha256 context;

    if (party == NULL || digest == NULL || count == 0 || count > PARTY_SIZE
     || capacity < COOP_BATTLE_DIGEST_SIZE
     || sizeof(struct Pokemon) != COOP_BATTLE_PARTY_MON_SIZE)
        return FALSE;
    CoopBattleSha256_Init(&context);
    CoopBattleSha256_Update(&context, prefix, sizeof(prefix));
    CoopBattleSha256_Update(&context, &count, sizeof(count));
    CoopBattleSha256_Update(&context, (const u8 *)party,
                           count * sizeof(struct Pokemon));
    CoopBattleSha256_Final(&context, digest);
    return TRUE;
}

static void CoopBattleDigest_Bytes(struct CoopBattleSha256 *context,
                                   const void *data, u32 length)
{
    CoopBattleSha256_Update(context, data, length);
}

static void CoopBattleDigest_U8(struct CoopBattleSha256 *context, u8 value)
{
    CoopBattleDigest_Bytes(context, &value, sizeof(value));
}

static void CoopBattleDigest_U16(struct CoopBattleSha256 *context, u16 value)
{
    u8 bytes[2] = {value, value >> 8};
    CoopBattleDigest_Bytes(context, bytes, sizeof(bytes));
}

static void CoopBattleDigest_U32(struct CoopBattleSha256 *context, u32 value)
{
    u8 bytes[4] = {value, value >> 8, value >> 16, value >> 24};
    CoopBattleDigest_Bytes(context, bytes, sizeof(bytes));
}

static void CoopBattleDigest_BattlePokemon(struct CoopBattleSha256 *context,
                                           const struct BattlePokemon *mon)
{
    u8 i;

    CoopBattleDigest_U16(context, mon->species);
    CoopBattleDigest_U16(context, mon->attack);
    CoopBattleDigest_U16(context, mon->defense);
    CoopBattleDigest_U16(context, mon->speed);
    CoopBattleDigest_U16(context, mon->spAttack);
    CoopBattleDigest_U16(context, mon->spDefense);
    for (i = 0; i < MAX_MON_MOVES; i++)
        CoopBattleDigest_U16(context, mon->moves[i]);
    CoopBattleDigest_U8(context, mon->hpIV);
    CoopBattleDigest_U8(context, mon->attackIV);
    CoopBattleDigest_U8(context, mon->defenseIV);
    CoopBattleDigest_U8(context, mon->speedIV);
    CoopBattleDigest_U8(context, mon->spAttackIV);
    CoopBattleDigest_U8(context, mon->spDefenseIV);
    CoopBattleDigest_U8(context, mon->abilityNum);
    CoopBattleDigest_Bytes(context, mon->statStages, NUM_BATTLE_STATS);
    CoopBattleDigest_U16(context, mon->ability);
    for (i = 0; i < ARRAY_COUNT(mon->types); i++)
        CoopBattleDigest_U8(context, mon->types[i]);
    CoopBattleDigest_Bytes(context, mon->pp, sizeof(mon->pp));
    CoopBattleDigest_U16(context, mon->hp);
    CoopBattleDigest_U8(context, mon->level);
    CoopBattleDigest_U8(context, mon->friendship);
    CoopBattleDigest_U16(context, mon->maxHP);
    CoopBattleDigest_U16(context, mon->item);
    CoopBattleDigest_Bytes(context, mon->nickname, sizeof(mon->nickname));
    CoopBattleDigest_U8(context, mon->ppBonuses);
    CoopBattleDigest_Bytes(context, mon->otName, sizeof(mon->otName));
    CoopBattleDigest_U32(context, mon->experience);
    CoopBattleDigest_U32(context, mon->personality);
    CoopBattleDigest_U32(context, mon->status1);
    /* Volatile bitfields are the protocol's versioned engine representation;
     * this fixed-size byte serialization includes every volatile timer and
     * target without copying compiler padding from the surrounding mon. */
    CoopBattleDigest_Bytes(context, &mon->volatiles, sizeof(mon->volatiles));
    CoopBattleDigest_U32(context, mon->otId);
    CoopBattleDigest_U8(context, mon->metLevel);
    CoopBattleDigest_U8(context, mon->isShiny);
    CoopBattleDigest_U8(context, mon->affectionHearts);
}

static enum BattleTrainer CoopBattleDigest_MemberTrainer(u8 member)
{
    u8 local_slot = sBattleRuntime.manifest[COOP_BATTLE_MANIFEST_MEMBER_SLOT_OFFSET];
    return member == local_slot ? B_TRAINER_0 : B_TRAINER_2;
}

static enum BattlerPosition CoopBattleDigest_MemberPosition(u8 member)
{
    u8 local_slot = sBattleRuntime.manifest[COOP_BATTLE_MANIFEST_MEMBER_SLOT_OFFSET];
    return member == local_slot ? B_POSITION_PLAYER_LEFT : B_POSITION_PLAYER_RIGHT;
}

static enum BattlerId CoopBattleDigest_BattlerAt(enum BattlerPosition position)
{
    enum BattlerId battler = GetBattlerAtPosition(position);
    return battler < MAX_BATTLERS_COUNT ? battler : MAX_BATTLERS_COUNT;
}

static void CoopBattleDigest_Party(struct CoopBattleSha256 *context,
                                   u8 canonical_slot,
                                   enum BattleTrainer trainer)
{
    u8 i;

    /* Never hash the local BattleTrainer ordinal here.  B_TRAINER_0 and
     * B_TRAINER_2 swap meaning when the manifest's member slot changes;
     * the canonical member slot is the stable wire identity. */
    CoopBattleDigest_U8(context, canonical_slot);
    CoopBattleDigest_U8(context, gPartiesCount[trainer]);
    for (i = 0; i < PARTY_SIZE; i++)
        CoopBattleDigest_Bytes(context, &gParties[trainer][i], sizeof(struct Pokemon));
}

static void CoopBattleDigest_Battler(struct CoopBattleSha256 *context,
                                     enum BattlerPosition position)
{
    enum BattlerId battler = CoopBattleDigest_BattlerAt(position);

    CoopBattleDigest_U8(context, position);
    if (battler == MAX_BATTLERS_COUNT)
    {
        CoopBattleDigest_U8(context, 0xFF);
        return;
    }
    CoopBattleDigest_U8(context, gBattlerPartyIndexes[battler]);
    CoopBattleDigest_BattlePokemon(context, &gBattleMons[battler]);
    CoopBattleDigest_Bytes(context, &gProtectStructs[battler], sizeof(gProtectStructs[battler]));
    CoopBattleDigest_Bytes(context, &gSpecialStatuses[battler], sizeof(gSpecialStatuses[battler]));
    CoopBattleDigest_Bytes(context, &gBattleStruct->battlerState[battler], sizeof(gBattleStruct->battlerState[battler]));
    CoopBattleDigest_Bytes(context, &gBattleStruct->futureSight[battler], sizeof(gBattleStruct->futureSight[battler]));
    CoopBattleDigest_Bytes(context, &gBattleStruct->wish[battler], sizeof(gBattleStruct->wish[battler]));
}

static void CoopBattleDigest_BattleState(struct CoopBattleSha256 *context)
{
    static const u8 version_tag[] = {'C', 'O', 'O', 'P', '-', 'B', 'A', 'T', 'T', 'L', 'E'};
    enum BattleTrainer member0_trainer;
    enum BattleTrainer member1_trainer;
    enum BattlerId battler;
    u8 i;

    CoopBattleDigest_Bytes(context, version_tag, sizeof(version_tag));
    CoopBattleDigest_U8(context, COOP_BATTLE_DIGEST_VERSION);
    /* Canonical member-to-position mapping. The local slot is used only to
     * choose the party/battler source; it is not itself part of the digest. */
    CoopBattleDigest_U8(context, 2);
    CoopBattleDigest_U8(context, 0);
    CoopBattleDigest_U8(context, B_POSITION_PLAYER_LEFT);
    CoopBattleDigest_U8(context, 1);
    CoopBattleDigest_U8(context, B_POSITION_PLAYER_RIGHT);

    member0_trainer = CoopBattleDigest_MemberTrainer(0);
    member1_trainer = CoopBattleDigest_MemberTrainer(1);
    CoopBattleDigest_Party(context, 0, member0_trainer);
    CoopBattleDigest_Party(context, 1, member1_trainer);
    CoopBattleDigest_Party(context, 2, B_TRAINER_1);
    CoopBattleDigest_Party(context, 3, B_TRAINER_3);
    /* PartyState carries delayed held-item, form, and sent-out effects that
     * are not represented by the six serialized party records. */
    CoopBattleDigest_Bytes(context, gBattleStruct->partyState,
                           sizeof(gBattleStruct->partyState));

    CoopBattleDigest_Battler(context, CoopBattleDigest_MemberPosition(0));
    CoopBattleDigest_Battler(context, CoopBattleDigest_MemberPosition(1));
    CoopBattleDigest_Battler(context, B_POSITION_OPPONENT_LEFT);
    CoopBattleDigest_Battler(context, B_POSITION_OPPONENT_RIGHT);

    CoopBattleDigest_Bytes(context, &gRngValue, sizeof(gRngValue));
    CoopBattleDigest_Bytes(context, &gRng2Value, sizeof(gRng2Value));
    CoopBattleDigest_U32(context, gBattleTypeFlags);
    CoopBattleDigest_U8(context, gBattleOutcome);
    CoopBattleDigest_U16(context, gBattleTurnCounter);
    CoopBattleDigest_U8(context, gBattlersCount);
    CoopBattleDigest_U8(context, gAbsentBattlerFlags);
    CoopBattleDigest_U32(context, gHitMarker);
    CoopBattleDigest_U16(context, gBattleWeather);
    CoopBattleDigest_U8(context, gBattleEnvironment);
    CoopBattleDigest_U32(context, gFieldStatuses);
    CoopBattleDigest_Bytes(context, &gFieldTimers, sizeof(gFieldTimers));
    CoopBattleDigest_Bytes(context, gSideStatuses, sizeof(gSideStatuses));
    CoopBattleDigest_Bytes(context, gSideTimers, sizeof(gSideTimers));
    CoopBattleDigest_Bytes(context, &gBattleStruct->eventState, sizeof(gBattleStruct->eventState));
    CoopBattleDigest_Bytes(context, gBattleStruct->moveTarget, sizeof(gBattleStruct->moveTarget));
    CoopBattleDigest_Bytes(context, gBattleStruct->chosenMovePositions, sizeof(gBattleStruct->chosenMovePositions));
    CoopBattleDigest_Bytes(context, gBattleStruct->monToSwitchIntoId, sizeof(gBattleStruct->monToSwitchIntoId));
    CoopBattleDigest_Bytes(context, gBattleStruct->battlerPartyIndexes, sizeof(gBattleStruct->battlerPartyIndexes));
    CoopBattleDigest_Bytes(context, gBattleStruct->battlerPartyOrders, sizeof(gBattleStruct->battlerPartyOrders));
    CoopBattleDigest_Bytes(context, gBattleStruct->lastTakenMove, sizeof(gBattleStruct->lastTakenMove));
    CoopBattleDigest_Bytes(context, gBattleStruct->lastTakenMoveFrom, sizeof(gBattleStruct->lastTakenMoveFrom));
    CoopBattleDigest_Bytes(context, gBattleStruct->passiveHpUpdate, sizeof(gBattleStruct->passiveHpUpdate));
    CoopBattleDigest_Bytes(context, gBattleStruct->moveDamage, sizeof(gBattleStruct->moveDamage));
    CoopBattleDigest_Bytes(context, gBattleStruct->moveResultFlags, sizeof(gBattleStruct->moveResultFlags));
    CoopBattleDigest_Bytes(context, gBattleStruct->hazardsQueue, sizeof(gBattleStruct->hazardsQueue));
    CoopBattleDigest_Bytes(context, gBattleStruct->numHazards, sizeof(gBattleStruct->numHazards));
    CoopBattleDigest_Bytes(context, &gBattleStruct->zmove, sizeof(gBattleStruct->zmove));
    CoopBattleDigest_Bytes(context, &gBattleStruct->dynamax, sizeof(gBattleStruct->dynamax));
    CoopBattleDigest_U8(context, gBattleStruct->hazardsCounter);
    CoopBattleDigest_U8(context, gBattleStruct->submoveAnnouncement);
    CoopBattleDigest_U8(context, gBattleStruct->effectsBeforeUsingMoveDone);
    CoopBattleDigest_Bytes(context, gBattleStruct->prevTurnSpecies, sizeof(gBattleStruct->prevTurnSpecies));
    for (i = 0; i < MAX_BATTLERS_COUNT; i++)
    {
        battler = CoopBattleDigest_BattlerAt((enum BattlerPosition)i);
        CoopBattleDigest_U8(context, battler == MAX_BATTLERS_COUNT ? 0xFF : gBattlerPositions[battler]);
    }
}

bool8 CoopBattleRuntime_ComputeBattleDigest(u8 *digest, u16 capacity)
{
    struct CoopBattleSha256 context;

    if (digest == NULL || capacity < COOP_BATTLE_DIGEST_SIZE
     || !sBattleRuntime.engine_active || !sBattleRuntime.manifest_valid
     || gBattleStruct == NULL)
        return FALSE;
    CoopBattleSha256_Init(&context);
    CoopBattleDigest_BattleState(&context);
    CoopBattleSha256_Final(&context, digest);
    return TRUE;
}

static u8 CoopBattleRuntime_FinishedResult(u8 outcome)
{
    if (sBattleRuntime.manifest[COOP_BATTLE_MANIFEST_KIND_OFFSET]
        == COOP_BATTLE_MANIFEST_KIND_FRIENDLY)
    {
        if (outcome == B_OUTCOME_WON)
            return sBattleRuntime.manifest[COOP_BATTLE_MANIFEST_MEMBER_SLOT_OFFSET] == 0
                ? COOP_BATTLE_FINISHED_MEMBER0_WON : COOP_BATTLE_FINISHED_MEMBER1_WON;
        if (outcome == B_OUTCOME_DREW)
            return COOP_BATTLE_FINISHED_DRAW;
        return sBattleRuntime.manifest[COOP_BATTLE_MANIFEST_MEMBER_SLOT_OFFSET] == 0
            ? COOP_BATTLE_FINISHED_MEMBER1_WON : COOP_BATTLE_FINISHED_MEMBER0_WON;
    }
    if (outcome == B_OUTCOME_WON)
        return COOP_BATTLE_FINISHED_WON;
    if (outcome == B_OUTCOME_DREW)
        return COOP_BATTLE_FINISHED_DRAW;
    return COOP_BATTLE_FINISHED_LOST;
}

bool8 CoopBattleRuntime_SendBattleFinished(const u8 *battle_id, u16 turn,
                                           u8 result, const u8 *digest,
                                           u16 digest_size)
{
    u8 payload[COOP_BATTLE_FINISHED_SIZE];

    if (battle_id == NULL || digest == NULL || digest_size != COOP_BATTLE_DIGEST_SIZE
     || result < COOP_BATTLE_FINISHED_MEMBER0_WON
     || result > COOP_BATTLE_FINISHED_LOST || turn == 0
     || turn > COOP_BATTLE_MAX_TURN || !sBattleRuntime.session_ready
     || !CoopNetBridge_CanSendBattle() || !IsCurrentBattle(battle_id)
     || sBattleRuntime.replay_next < sBattleRuntime.replay_count
     || turn != sBattleRuntime.last_hash_turn)
        return FALSE;
    memcpy(payload, battle_id, COOP_BATTLE_ID_SIZE);
    payload[16] = turn;
    payload[17] = turn >> 8;
    payload[18] = result;
    memcpy(payload + 19, digest, COOP_BATTLE_DIGEST_SIZE);
    if (!CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_BATTLE_FINISHED,
                                             payload, sizeof(payload)))
        return FALSE;
    sBattleRuntime.terminal_sent = TRUE;
    sBattleRuntime.terminal_pending = FALSE;
    return TRUE;
}

bool8 CoopBattleRuntime_IsTurnHashPending(void)
{
    return sBattleRuntime.hash_pending;
}

enum CoopBattleTurnReportResult CoopBattleRuntime_RetryTurnHash(void)
{
    u16 turn = sBattleRuntime.pending_hash_turn;

    if (!sBattleRuntime.hash_pending || !sBattleRuntime.engine_active
     || !sBattleRuntime.manifest_valid || sBattleRuntime.engine_faulted
     || turn == 0 || turn > COOP_BATTLE_MAX_TURN
     || turn != sBattleRuntime.last_hash_turn + 1
     || turn != sBattleRuntime.last_taken_turn
     || turn != sBattleRuntime.last_action_turn
     || (gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_QUEUE_ERROR))
        return COOP_BATTLE_TURN_REPORT_INVALID;
    if (!sBattleRuntime.session_ready || !CoopNetBridge_CanSendBattle()
     || sBattleRuntime.replay_next < sBattleRuntime.replay_count)
        return COOP_BATTLE_TURN_REPORT_PENDING;
    if (!CoopBattleRuntime_SendTurnResultHash(sBattleRuntime.manifest, turn,
                                               sBattleRuntime.pending_hash_digest,
                                               sizeof(sBattleRuntime.pending_hash_digest)))
        return (gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_QUEUE_ERROR)
            ? COOP_BATTLE_TURN_REPORT_INVALID : COOP_BATTLE_TURN_REPORT_PENDING;

    sBattleRuntime.hash_pending = FALSE;
    if (sBattleRuntime.pending_hash_outcome != 0)
    {
        memcpy(sBattleRuntime.terminal_digest, sBattleRuntime.pending_hash_digest,
               sizeof(sBattleRuntime.terminal_digest));
        sBattleRuntime.terminal_turn = turn;
        sBattleRuntime.terminal_result =
            CoopBattleRuntime_FinishedResult(sBattleRuntime.pending_hash_outcome);
        sBattleRuntime.terminal_pending = TRUE;
        /* A full bridge may delay the finish frame; PollTerminal retries it. */
        (void)CoopBattleRuntime_SendBattleFinished(sBattleRuntime.manifest, turn,
                                                    sBattleRuntime.terminal_result,
                                                    sBattleRuntime.terminal_digest,
                                                    sizeof(sBattleRuntime.terminal_digest));
    }
    return COOP_BATTLE_TURN_REPORT_SENT;
}

static enum CoopBattleTurnReportResult ReportTurnDigest(u16 turn, u8 outcome,
                                                        const u8 *digest)
{
    if (digest == NULL || !sBattleRuntime.engine_active
     || !sBattleRuntime.manifest_valid || sBattleRuntime.engine_faulted
     || turn == 0 || turn > COOP_BATTLE_MAX_TURN
     || turn != sBattleRuntime.last_hash_turn + 1
     || turn != sBattleRuntime.last_taken_turn
     || turn != sBattleRuntime.last_action_turn
     || sBattleRuntime.hash_pending)
        return COOP_BATTLE_TURN_REPORT_INVALID;
    sBattleRuntime.hash_pending = TRUE;
    sBattleRuntime.pending_hash_turn = turn;
    sBattleRuntime.pending_hash_outcome = outcome;
    memcpy(sBattleRuntime.pending_hash_digest, digest,
           sizeof(sBattleRuntime.pending_hash_digest));
    return CoopBattleRuntime_RetryTurnHash();
}

#if TESTING
enum CoopBattleTurnReportResult CoopBattleRuntime_TestReportTurnDigest(u16 turn, u8 outcome,
                                                                      const u8 *digest)
{
    return ReportTurnDigest(turn, outcome, digest);
}
#endif

enum CoopBattleTurnReportResult CoopBattleRuntime_ReportTurnState(u16 turn, u8 outcome)
{
    u8 digest[COOP_BATTLE_DIGEST_SIZE];

    if (!CoopBattleRuntime_ComputeBattleDigest(digest, sizeof(digest)))
        return COOP_BATTLE_TURN_REPORT_INVALID;
    return ReportTurnDigest(turn, outcome, digest);
}

void CoopBattleRuntime_PollTerminal(void)
{
    if (sBattleRuntime.terminal_pending)
        (void)CoopBattleRuntime_SendBattleFinished(sBattleRuntime.manifest,
                                                    sBattleRuntime.terminal_turn,
                                                    sBattleRuntime.terminal_result,
                                                    sBattleRuntime.terminal_digest,
                                                    sizeof(sBattleRuntime.terminal_digest));
}

bool8 CoopBattleRuntime_IsBattleFinishedSent(void)
{
    return sBattleRuntime.terminal_sent;
}

void CoopBattleRuntime_CompleteBattle(void)
{
    if (!sBattleRuntime.terminal_sent)
        return;
    ClearBattle();
    CoopBattleRuntime_DisarmEngine();
}
