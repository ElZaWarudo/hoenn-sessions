#include "global.h"
#include "coop/trade_runtime.h"
#include "coop/battle_runtime.h"
#include "coop/net_bridge.h"
#include "coop/save.h"
#include "event_object_movement.h"
#include "johto/bug_contest.h"
#include "main.h"
#include "new_game.h"
#include "overworld.h"
#include "palette.h"
#include "pokemon.h"
#include "save.h"
#include "script.h"
#include "constants/items.h"
#include "constants/pokedex.h"

/*
 * Applies a server-issued trade (TradeCommit) to the running ROM.
 *
 * Delivery: the launcher sends a tracked trade once per control generation,
 * so the ROM keeps one validated commit pending until the field is safe
 * instead of dropping it. A newer commit replaces an unapplied one.
 *
 * Idempotency: the save format has no spare field for a commit ID (the CSP1
 * reserved bytes must stay zero), so a repeat is recognized either from the
 * in-memory applied header (same boot) or from party contents (after a
 * reboot): a party slot holds exactly the incoming record and no party
 * Pokemon still carries the outgoing personality and OT ID. Either way the
 * ROM acknowledges again without applying twice.
 *
 * Ordering: the 28-byte CommitApplied is queued first. CHECKPOINT_READY is
 * requested only after the sidecar has consumed it (the bridge refuses a
 * checkpoint while the outbound queue is non-empty) and any battle replay has
 * drained. Field controls stay locked until the checkpoint save completes,
 * and no trade evolution runs: finalize compares the slot byte for byte.
 */

struct CoopTradeRuntime
{
    u8 pending[COOP_TRADE_COMMIT_SIZE];
    u8 applied_header[COOP_TRADE_COMMIT_APPLIED_SIZE];
    u8 state;
    bool8 applied_valid;
    bool8 ack_pending;
    bool8 ack_queued;
    bool8 controls_locked;
    u16 ack_next_index;
    u16 rejected_count;
    u32 frame;
    u32 attempt_started_frame;
    u32 retry_after_frame;
    u32 applied_generation;
};

static EWRAM_DATA struct CoopTradeRuntime sTradeRuntime = {0};

#if TESTING
static bool8 sTradeSaveDryRun;
#endif

static u32 ReadU32(const u8 *bytes)
{
    return (u32)bytes[0] | ((u32)bytes[1] << 8)
         | ((u32)bytes[2] << 16) | ((u32)bytes[3] << 24);
}

static bool8 IsNonzero(const u8 *bytes, u16 length)
{
    u16 i;

    for (i = 0; i < length; i++)
        if (bytes[i] != 0)
            return TRUE;
    return FALSE;
}

/* Structural validation that does not depend on the local party. */
static bool8 IsWellFormedCommit(const u8 *payload, u16 length)
{
    struct Pokemon checked;
    u16 species;

    if (payload == NULL || length != COOP_TRADE_COMMIT_SIZE
     || sizeof(checked) != COOP_TRADE_COMMIT_RECORD_SIZE
     || !IsNonzero(payload, COOP_TRADE_COMMIT_ID_SIZE)
     || payload[COOP_TRADE_COMMIT_SLOT_OFFSET] >= PARTY_SIZE
     || IsNonzero(payload + COOP_TRADE_COMMIT_RESERVED_OFFSET,
                  COOP_TRADE_COMMIT_RESERVED_SIZE)
     || !IsNonzero(payload + COOP_TRADE_COMMIT_RECORD_OFFSET,
                   COOP_TRADE_COMMIT_RECORD_SIZE))
        return FALSE;

    /* Decode a copy: GetMonData marks a bad checksum on its argument. */
    memcpy(&checked, payload + COOP_TRADE_COMMIT_RECORD_OFFSET, sizeof(checked));
    species = GetMonData(&checked, MON_DATA_SPECIES);
    if (species == SPECIES_NONE || species >= NUM_SPECIES
     || !GetMonData(&checked, MON_DATA_SANITY_HAS_SPECIES)
     || GetMonData(&checked, MON_DATA_SANITY_IS_BAD_EGG))
        return FALSE;
    /* A mail index refers to the sender's mail storage and cannot be moved
     * without rewriting the record the server will compare at finalize. */
    if (GetMonData(&checked, MON_DATA_MAIL) != MAIL_NONE)
        return FALSE;
    return TRUE;
}

static void ReleaseControls(void)
{
    if (sTradeRuntime.controls_locked)
    {
        UnlockPlayerFieldControls();
        sTradeRuntime.controls_locked = FALSE;
    }
}

static bool8 IsOverworldWithoutScript(void)
{
    return gMain.callback1 == CB1_Overworld && gMain.callback2 == CB2_Overworld
        && !gPaletteFade.active && !ScriptContext_IsEnabled();
}

static bool8 IsSafeToApply(void)
{
    return IsOverworldWithoutScript()
        && !ArePlayerFieldControlsLocked()
        && CoopNetBridge_CanSendBattle()
        && !CoopBattleRuntime_HasManifest()
        && !CoopBattleRuntime_IsEngineActive()
        && !JohtoBugContest_IsSerializationBlocked()
        && !sTradeRuntime.ack_pending
        && !sTradeRuntime.ack_queued;
}

static bool8 PartyHoldsOutgoing(u32 personality, u32 otId)
{
    u8 i;

    for (i = 0; i < gPlayerPartyCount && i < PARTY_SIZE; i++)
    {
        if (gPlayerParty[i].box.personality == personality
         && gPlayerParty[i].box.otId == otId
         && GetMonData(&gPlayerParty[i], MON_DATA_SANITY_HAS_SPECIES))
            return TRUE;
    }
    return FALSE;
}

static bool8 PartyHoldsRecord(const u8 *record)
{
    u8 i;

    for (i = 0; i < gPlayerPartyCount && i < PARTY_SIZE; i++)
        if (memcmp(&gPlayerParty[i], record, COOP_TRADE_COMMIT_RECORD_SIZE) == 0)
            return TRUE;
    return FALSE;
}

static void RememberApplied(const u8 *payload)
{
    memcpy(sTradeRuntime.applied_header, payload, COOP_TRADE_COMMIT_APPLIED_SIZE);
    sTradeRuntime.applied_valid = TRUE;
    sTradeRuntime.ack_pending = TRUE;
    sTradeRuntime.applied_generation = CoopSave_GetGeneration();
    sTradeRuntime.attempt_started_frame = sTradeRuntime.frame;
    sTradeRuntime.retry_after_frame = sTradeRuntime.frame;
    sTradeRuntime.state = COOP_TRADE_STATE_CHECKPOINT_OWED;
}

static void Reject(void)
{
    if (sTradeRuntime.rejected_count != 0xFFFF)
        sTradeRuntime.rejected_count++;
    sTradeRuntime.state = COOP_TRADE_STATE_IDLE;
    memset(sTradeRuntime.pending, 0, sizeof(sTradeRuntime.pending));
}

static void TryApplyPending(void)
{
    const u8 *payload = sTradeRuntime.pending;
    const u8 *record = payload + COOP_TRADE_COMMIT_RECORD_OFFSET;
    u8 slot = payload[COOP_TRADE_COMMIT_SLOT_OFFSET];
    u32 personality = ReadU32(payload + COOP_TRADE_COMMIT_OUTGOING_PERSONALITY_OFFSET);
    u32 otId = ReadU32(payload + COOP_TRADE_COMMIT_OUTGOING_OT_ID_OFFSET);
    struct Pokemon *mon;

    if (!IsSafeToApply())
        return;

    if (slot >= gPlayerPartyCount
     || gPlayerParty[slot].box.personality != personality
     || gPlayerParty[slot].box.otId != otId
     || !GetMonData(&gPlayerParty[slot], MON_DATA_SANITY_HAS_SPECIES))
    {
        /* A save that already contains this trade (ROM reboot before the
         * launcher saw the acknowledgement) is acknowledged again. */
        if (PartyHoldsRecord(record) && !PartyHoldsOutgoing(personality, otId))
            RememberApplied(payload);
        else
            Reject();
        return;
    }

    mon = &gPlayerParty[slot];
    memcpy(mon, record, COOP_TRADE_COMMIT_RECORD_SIZE);
    /* The dex is outside the finalize comparison; the record is not. */
    if (!GetMonData(mon, MON_DATA_IS_EGG))
    {
        HandleSetPokedexFlagFromMon(mon, FLAG_SET_SEEN);
        HandleSetPokedexFlagFromMon(mon, FLAG_SET_CAUGHT);
    }
#if !TESTING
    if (slot == 0)
        UpdateFollowingPokemon();
#endif
    RememberApplied(payload);
}

static void ReconcileAck(void)
{
    const struct CoopBridgeQueue *queue = &gCoopNetBridge.game_to_network;

    if (!sTradeRuntime.ack_queued)
        return;
    if ((u16)(queue->write_index - queue->read_index) > COOP_NET_BRIDGE_QUEUE_CAPACITY)
        return;
    if ((s16)(queue->read_index - sTradeRuntime.ack_next_index) >= 0)
        sTradeRuntime.ack_queued = FALSE;
}

static void TrySendAck(void)
{
    if (!sTradeRuntime.ack_pending || !sTradeRuntime.applied_valid
     || sTradeRuntime.ack_queued || !CoopNetBridge_CanSendBattle())
        return;
    if (!CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_COMMIT_APPLIED,
                                            sTradeRuntime.applied_header,
                                            COOP_TRADE_COMMIT_APPLIED_SIZE))
        return;
    sTradeRuntime.ack_pending = FALSE;
    sTradeRuntime.ack_queued = TRUE;
    sTradeRuntime.ack_next_index = gCoopNetBridge.game_to_network.write_index;
}

static void FinishCheckpoint(void)
{
    ReleaseControls();
    sTradeRuntime.state = COOP_TRADE_STATE_IDLE;
}

static bool8 RunCheckpointSave(void)
{
    u8 status;

#if TESTING
    if (sTradeSaveDryRun)
    {
        CoopNetBridge_NotifySaveResult(TRUE);
        return TRUE;
    }
#endif
    if (gDifferentSaveFile == TRUE)
    {
        status = TrySavingData(SAVE_OVERWRITE_DIFFERENT_FILE);
        gDifferentSaveFile = FALSE;
    }
    else
    {
        status = TrySavingData(SAVE_NORMAL);
    }
    return status == SAVE_STATUS_OK;
}

static void PollCheckpointOwed(void)
{
    enum CoopCheckpointRequestResult result;

    if (!IsOverworldWithoutScript()
     || (s32)(sTradeRuntime.frame - sTradeRuntime.retry_after_frame) < 0)
        return;
    if (!sTradeRuntime.controls_locked)
    {
        if (ArePlayerFieldControlsLocked())
            return;
        LockPlayerFieldControls();
        sTradeRuntime.controls_locked = TRUE;
        sTradeRuntime.attempt_started_frame = sTradeRuntime.frame;
    }
    else if (!ArePlayerFieldControlsLocked())
    {
        /* A script ending may release every lock; keep ours. */
        LockPlayerFieldControls();
    }

    result = COOP_CHECKPOINT_REQUEST_REJECTED;
    if (!sTradeRuntime.ack_pending && !sTradeRuntime.ack_queued
     && !CoopBattleRuntime_HasPendingOutboundReplay())
        result = CoopNetBridge_RequestCheckpoint();
    if (result == COOP_CHECKPOINT_REQUEST_STARTED)
    {
        sTradeRuntime.state = COOP_TRADE_STATE_CHECKPOINT_WAITING;
        return;
    }
    if (sTradeRuntime.frame - sTradeRuntime.attempt_started_frame
        >= COOP_TRADE_CHECKPOINT_ATTEMPT_FRAMES)
    {
        /* Never soft-lock the player behind a refused checkpoint. The trade
         * stays owed and the next attempt locks the field again. */
        ReleaseControls();
        sTradeRuntime.retry_after_frame =
            sTradeRuntime.frame + COOP_TRADE_CHECKPOINT_BACKOFF_FRAMES;
    }
}

static void PollCheckpointWaiting(void)
{
    switch (CoopNetBridge_GetCheckpointState())
    {
    case COOP_CHECKPOINT_STATE_WAITING_FOR_GRANT:
        return;
    case COOP_CHECKPOINT_STATE_GRANTED:
        if (CoopNetBridge_ConsumeCheckpointGrant()
         && CoopNetBridge_IsCheckpointAuthorizedForSave()
         && RunCheckpointSave())
        {
            FinishCheckpoint();
            return;
        }
        /* A failed save reported itself through the bridge; retry later. */
        CoopNetBridge_NotifySaveResult(FALSE);
        break;
    default:
        break;
    }
    sTradeRuntime.state = COOP_TRADE_STATE_CHECKPOINT_OWED;
    sTradeRuntime.attempt_started_frame = sTradeRuntime.frame;
}

void CoopTradeRuntime_Init(void)
{
    if (sTradeRuntime.controls_locked)
        UnlockPlayerFieldControls();
    memset(&sTradeRuntime, 0, sizeof(sTradeRuntime));
}

enum CoopTradeInboundResult CoopTradeRuntime_ReceiveCommit(const u8 *payload, u16 length)
{
    if (!IsWellFormedCommit(payload, length))
    {
        if (sTradeRuntime.rejected_count != 0xFFFF)
            sTradeRuntime.rejected_count++;
        return COOP_TRADE_INBOUND_MALFORMED;
    }

    if (sTradeRuntime.applied_valid
     && memcmp(payload, sTradeRuntime.applied_header, COOP_TRADE_COMMIT_ID_SIZE) == 0)
    {
        /* Redelivery of the commit this boot already applied: acknowledge
         * the exact original header again, never apply twice. */
        if (memcmp(payload, sTradeRuntime.applied_header,
                   COOP_TRADE_COMMIT_APPLIED_SIZE) != 0)
            return COOP_TRADE_INBOUND_IGNORED;
        if (!sTradeRuntime.ack_queued)
            sTradeRuntime.ack_pending = TRUE;
        return COOP_TRADE_INBOUND_ACCEPTED;
    }

    if (sTradeRuntime.state != COOP_TRADE_STATE_IDLE
     && sTradeRuntime.state != COOP_TRADE_STATE_PENDING)
        return COOP_TRADE_INBOUND_IGNORED;

    memcpy(sTradeRuntime.pending, payload, COOP_TRADE_COMMIT_SIZE);
    sTradeRuntime.state = COOP_TRADE_STATE_PENDING;
    return COOP_TRADE_INBOUND_ACCEPTED;
}

void CoopTradeRuntime_PreserveOutbound(void)
{
    struct CoopBridgeQueue *queue = &gCoopNetBridge.game_to_network;
    u16 depth;
    u16 i;

    ReconcileAck();
    if (!sTradeRuntime.ack_queued)
        return;
    sTradeRuntime.ack_queued = FALSE;
    sTradeRuntime.ack_pending = TRUE;

    /* This runtime re-sends the ack itself. Blank the unread copy so the
     * battle replay, which also captures CommitApplied frames before the
     * same reset, cannot deliver a second one later (possibly after the
     * launcher has finalized the trade). The queue is wiped next. */
    depth = (u16)(queue->write_index - queue->read_index);
    if (depth > COOP_NET_BRIDGE_QUEUE_CAPACITY)
        return;
    for (i = 0; i < depth; i++)
    {
        struct CoopBridgeMessage *message =
            &queue->entries[(queue->read_index + i) & (COOP_NET_BRIDGE_QUEUE_CAPACITY - 1)];
        if (message->type == COOP_BRIDGE_MESSAGE_COMMIT_APPLIED
         && message->length == COOP_TRADE_COMMIT_APPLIED_SIZE)
            message->type = COOP_BRIDGE_MESSAGE_NONE;
    }
}

void CoopTradeRuntime_Poll(void)
{
    sTradeRuntime.frame++;
    ReconcileAck();

    if (sTradeRuntime.state == COOP_TRADE_STATE_IDLE && !sTradeRuntime.ack_pending)
        return;

    if (!CoopNetBridge_IsSessionActive())
    {
        /* Wait for the transport with the player free to move; nothing is
         * written to flash without a grant. */
        ReleaseControls();
        if (sTradeRuntime.state == COOP_TRADE_STATE_CHECKPOINT_WAITING)
            sTradeRuntime.state = COOP_TRADE_STATE_CHECKPOINT_OWED;
        return;
    }

    TrySendAck();

    switch (sTradeRuntime.state)
    {
    case COOP_TRADE_STATE_PENDING:
        TryApplyPending();
        if (sTradeRuntime.state == COOP_TRADE_STATE_CHECKPOINT_OWED)
            TrySendAck();
        break;
    case COOP_TRADE_STATE_CHECKPOINT_OWED:
        if (CoopSave_GetGeneration() != sTradeRuntime.applied_generation
         && !sTradeRuntime.ack_pending && !sTradeRuntime.ack_queued)
        {
            /* A later checkpoint save already captured the traded slot. */
            FinishCheckpoint();
            break;
        }
        PollCheckpointOwed();
        break;
    case COOP_TRADE_STATE_CHECKPOINT_WAITING:
        PollCheckpointWaiting();
        break;
    default:
        break;
    }
}

enum CoopTradeRuntimeState CoopTradeRuntime_GetState(void)
{
    return sTradeRuntime.state;
}

#if TESTING
void CoopTradeRuntime_TestSetSaveDryRun(bool8 enabled)
{
    sTradeSaveDryRun = enabled;
}

u16 CoopTradeRuntime_TestGetRejectedCount(void)
{
    return sTradeRuntime.rejected_count;
}

bool8 CoopTradeRuntime_TestHoldsControlLock(void)
{
    return sTradeRuntime.controls_locked;
}
#endif
