#ifndef GUARD_COOP_TRADE_RUNTIME_H
#define GUARD_COOP_TRADE_RUNTIME_H

#include "gba/types.h"

/* TradeCommit (0x0119, sidecar to ROM). The payload is exactly 128 bytes,
 * little endian, and mirrors coop_protocol::TradeCommitRecord:
 *   0  16  commit_id (ledger UUID octets, nonzero)
 *  16   1  slot (party slot 0..5 the trade was offered from; a hint: the
 *          ROM replaces whichever slot holds the outgoing Pokemon)
 *  17   3  reserved, zero
 *  20   4  outgoing_personality
 *  24   4  outgoing_ot_id
 *  28 100  incoming_record (exact party struct Pokemon)
 * The ROM acknowledges with a 28-byte CommitApplied (0x000B) that is
 * byte-identical to payload bytes 0..28. */
#define COOP_TRADE_COMMIT_SIZE 128
#define COOP_TRADE_COMMIT_ID_SIZE 16
#define COOP_TRADE_COMMIT_SLOT_OFFSET 16
#define COOP_TRADE_COMMIT_RESERVED_OFFSET 17
#define COOP_TRADE_COMMIT_RESERVED_SIZE 3
#define COOP_TRADE_COMMIT_OUTGOING_PERSONALITY_OFFSET 20
#define COOP_TRADE_COMMIT_OUTGOING_OT_ID_OFFSET 24
#define COOP_TRADE_COMMIT_RECORD_OFFSET 28
#define COOP_TRADE_COMMIT_RECORD_SIZE 100
#define COOP_TRADE_COMMIT_APPLIED_SIZE 28

/* Frames the ROM keeps field controls locked while a post-trade checkpoint
 * is refused before it releases the player and backs off. */
#define COOP_TRADE_CHECKPOINT_ATTEMPT_FRAMES 600
#define COOP_TRADE_CHECKPOINT_BACKOFF_FRAMES 600

enum CoopTradeInboundResult
{
    COOP_TRADE_INBOUND_ACCEPTED,
    COOP_TRADE_INBOUND_IGNORED,
    COOP_TRADE_INBOUND_MALFORMED,
};

enum CoopTradeRuntimeState
{
    COOP_TRADE_STATE_IDLE,
    /* One validated commit waits for a safe field state. */
    COOP_TRADE_STATE_PENDING,
    /* The slot holds the incoming record; a checkpoint must follow. */
    COOP_TRADE_STATE_CHECKPOINT_OWED,
    COOP_TRADE_STATE_CHECKPOINT_WAITING,
};

void CoopTradeRuntime_Init(void);
enum CoopTradeInboundResult CoopTradeRuntime_ReceiveCommit(const u8 *payload, u16 length);
/* Called before the bridge rearms its queues: an acknowledgement the sidecar
 * has not consumed is re-sent after ROM_READY. */
void CoopTradeRuntime_PreserveOutbound(void);
void CoopTradeRuntime_Poll(void);
enum CoopTradeRuntimeState CoopTradeRuntime_GetState(void);
/* Writes the checkpoint save after a consumed, authorized grant. Shared with
 * the trade offer UI, which checkpoints before an offer or an accept. */
bool8 CoopTradeRuntime_RunCheckpointSave(void);

#if TESTING
void CoopTradeRuntime_TestSetSaveDryRun(bool8 enabled);
u16 CoopTradeRuntime_TestGetRejectedCount(void);
bool8 CoopTradeRuntime_TestHoldsControlLock(void);
#endif

#endif /* GUARD_COOP_TRADE_RUNTIME_H */
