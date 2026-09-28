#ifndef GUARD_COOP_BATTLE_RUNTIME_H
#define GUARD_COOP_BATTLE_RUNTIME_H

#include "gba/types.h"
#include "coop/battle_protocol.h"
#include "pokemon.h"

#define COOP_BATTLE_MULTI_PARTY_SIZE 3
#define COOP_BATTLE_ACTION_SIZE 4
#define COOP_BATTLE_DIGEST_VERSION 1
// Requests server cancellation for the currently tracked battle. The ROM waits
// for the terminal ABORT_BATTLE before clearing its battle state.
bool8 CoopBattleRuntime_RequestAbort(u8 reason);

/* Canonical action bytes are independent of the local battler numbering.
 * Targets are BattlerPosition values, with the two player positions ordered
 * by manifest member slot. This deliberately excludes bag/run actions. */
enum CoopBattleActionKind
{
    COOP_BATTLE_ACTION_MOVE = 1,
    COOP_BATTLE_ACTION_SWITCH = 2,
    COOP_BATTLE_ACTION_FORCED_SWITCH = 3,
    COOP_BATTLE_ACTION_NO_ACTION = 4,
    COOP_BATTLE_ACTION_AUTO_MOVE = 5,
};

struct CoopBattleAction
{
    u8 kind;
    u8 index;
    u8 target;
};

/* An inert staging result. Battle setup must own any later party swap and
 * restoration; preparing this value never changes the saved player party. */
struct CoopBattlePreparedParty
{
    struct Pokemon local[COOP_BATTLE_MULTI_PARTY_SIZE];
    struct Pokemon peer[COOP_BATTLE_MULTI_PARTY_SIZE];
    u8 local_slots[COOP_BATTLE_MULTI_PARTY_SIZE];
    u8 peer_slots[COOP_BATTLE_MULTI_PARTY_SIZE];
};

/* Prepared, side-effect-free inputs for a future non-link multi battle.
 * Both ROMs keep their own player in B_TRAINER_0 and the peer in B_TRAINER_2;
 * member_battler_positions records the shared, canonical action order. */
struct CoopBattleStartupPlan
{
    u8 battle_id[COOP_BATTLE_ID_SIZE];
    u8 local_member_slot;
    u8 member_battler_positions[2];
    u8 member_party_trainers[2];
    u16 opponent_trainer_id;
    u8 local_slots[COOP_BATTLE_MULTI_PARTY_SIZE];
    u8 peer_slots[COOP_BATTLE_MULTI_PARTY_SIZE];
    struct Pokemon original_local[PARTY_SIZE];
    struct Pokemon staged_local[COOP_BATTLE_MULTI_PARTY_SIZE];
    struct Pokemon staged_peer[COOP_BATTLE_MULTI_PARTY_SIZE];
};

struct CoopBattleManifestIdentity
{
    u8 kind;
    u8 local_member_slot;
    u8 trainer_region;
    u16 trainer_ordinal;
};

enum CoopBattleInboundResult
{
    COOP_BATTLE_INBOUND_MALFORMED,
    COOP_BATTLE_INBOUND_IGNORED,
    COOP_BATTLE_INBOUND_ACCEPTED,
};

struct CoopBattleTurnBundle
{
    u8 battle_id[COOP_BATTLE_ID_SIZE];
    u16 turn;
    u8 first_length;
    u8 second_length;
    u8 first_action[COOP_BATTLE_MAX_ACTION_SIZE];
    u8 second_action[COOP_BATTLE_MAX_ACTION_SIZE];
};

void CoopBattleRuntime_Init(void);
void CoopBattleRuntime_OnSessionReady(u32 epoch);
void CoopBattleRuntime_OnTransportLost(void);
/* Preserve unread battle frames before a same-epoch bridge queue reset. */
void CoopBattleRuntime_PreserveOutbound(void);
void CoopBattleRuntime_PollOutboundReplay(void);
bool8 CoopBattleRuntime_HasPendingOutboundReplay(void);
enum CoopBattleInboundResult CoopBattleRuntime_ReceiveManifest(const u8 *payload, u16 length);
enum CoopBattleInboundResult CoopBattleRuntime_ReceivePeerPartyChunk(const u8 *payload, u16 length);
/* Returns a copy only after every chunk has arrived for the current manifest. */
bool8 CoopBattleRuntime_CopyPeerParty(u8 *battle_id, u8 *count, u8 *mons, u16 capacity);
/* Requires the exact current manifest, a complete 1..6-record peer snapshot,
 * and at least three usable mons on each side. On failure, prepared is left
 * unchanged. */
bool8 CoopBattleRuntime_PreparePartnerParty(const u8 *battle_id,
                                           const struct Pokemon *local_party,
                                           u8 local_count,
                                           struct CoopBattlePreparedParty *prepared);
/* Hash the exact ordered party records used by the launcher snapshot commit. */
bool8 CoopBattleRuntime_ComputePartyDigest(const struct Pokemon *party,
                                           u8 count, u8 *digest, u16 capacity);
/* Only the two catalogued trainer encounters are accepted. All identity
 * fields come from the current validated manifest. No live state changes. */
bool8 CoopBattleRuntime_MakeStartupPlan(const u8 *battle_id,
                                        const struct Pokemon *local_party,
                                        u8 local_count,
                                        struct CoopBattleStartupPlan *plan);
/* Restore the full pre-battle player party, overlaying the three selected
 * local mons' battle results at their original slots. Does not save. */
bool8 CoopBattleRuntime_RestoreLocalParty(const struct CoopBattleStartupPlan *plan,
                                         const u8 *battle_id,
                                         const struct Pokemon *battled_local,
                                         struct Pokemon *restored_local);
enum CoopBattleInboundResult CoopBattleRuntime_ReceiveTurnBundle(const u8 *payload, u16 length);
enum CoopBattleInboundResult CoopBattleRuntime_ReceivePause(const u8 *payload, u16 length);
enum CoopBattleInboundResult CoopBattleRuntime_ReceiveAbort(const u8 *payload, u16 length);
enum CoopBattleInboundResult CoopBattleRuntime_ReceiveStart(const u8 *payload, u16 length);
/* A terminal cooperative trainer win remains anchored after the battle
 * engine is torn down. The server's commit grant is accepted only when it
 * exactly matches that retained battle and trainer identity. */
enum CoopBattleInboundResult CoopBattleRuntime_ReceiveBattleCommit(const u8 *payload,
                                                                   u16 length);
bool8 CoopBattleRuntime_HasPendingBattleCommit(void);
bool8 CoopBattleRuntime_HasAcceptedBattleCommit(void);
void CoopBattleRuntime_ForgetTerminalCommit(void);
bool8 CoopBattleRuntime_IsStartReleased(void);
#if TESTING
void CoopBattleRuntime_TestSetReadyForStart(void);
#endif
bool8 CoopBattleRuntime_HasManifest(void);
bool8 CoopBattleRuntime_GetManifest(u8 *payload, u16 capacity);
bool8 CoopBattleRuntime_GetManifestIdentity(const u8 *battle_id,
                                           struct CoopBattleManifestIdentity *identity);
bool8 CoopBattleRuntime_EncodeAction(const struct CoopBattleAction *action,
                                    u8 *bytes, u16 capacity);
bool8 CoopBattleRuntime_DecodeAction(const u8 *bytes, u16 length,
                                    struct CoopBattleAction *action);
/* Swap the allied positions for member 1; opponent positions remain fixed. */
u8 CoopBattleRuntime_TranslateTarget(u8 position, u8 local_member_slot);
/* Dormant engine adapter. Battle setup must arm it only after staging and
 * deterministic RNG are installed; no production caller does so yet. */
bool8 CoopBattleRuntime_ArmEngine(const u8 *battle_id);
void CoopBattleRuntime_DisarmEngine(void);
bool8 CoopBattleRuntime_IsEngineActive(void);
bool8 CoopBattleRuntime_IsEngineFaulted(void);
bool8 CoopBattleRuntime_IsSessionReady(void);
u8 CoopBattleRuntime_EngineLocalMemberSlot(void);
bool8 CoopBattleRuntime_IsLocalActionSubmitted(void);
bool8 CoopBattleRuntime_SubmitLocalAction(const struct CoopBattleAction *action);
bool8 CoopBattleRuntime_PollPeerAction(struct CoopBattleAction *action);
/* Used when the native engine bypasses B2's controller for an automatic turn. */
bool8 CoopBattleRuntime_ConfirmPeerAutomaticAction(u8 expected_kind);
/* Latch a local unsupported/desynced decision until battle cleanup disarms. */
void CoopBattleRuntime_FailEngine(void);
bool8 CoopBattleRuntime_IsEngineTurnReady(void);
void CoopBattleRuntime_FinishEngineTurn(void);
bool8 CoopBattleRuntime_TakeTurnBundle(struct CoopBattleTurnBundle *bundle);
bool8 CoopBattleRuntime_GetPause(u16 *turn, u8 *missing_slot);
/* Snapshot chunks are ordered 0..count-1 and carry exactly one raw 100-byte mon. */
bool8 CoopBattleRuntime_SendPartySnapshot(const u8 *battle_id, u8 party_slot,
                                           u8 party_count, const u8 *mon, u16 mon_size);
/* Queue one party record per poll; readiness follows only after a complete
 * peer snapshot and matching manifest digest. */
bool8 CoopBattleRuntime_PollLocalSnapshot(const u8 *battle_id,
                                          const struct Pokemon *party, u8 count);
bool8 CoopBattleRuntime_TrySendReady(const u8 *battle_id,
                                     const struct Pokemon *party, u8 count);
bool8 CoopBattleRuntime_SendActionIntent(const u8 *battle_id, u16 turn,
                                         const u8 *action, u16 action_size);
bool8 CoopBattleRuntime_SendTurnResultHash(const u8 *battle_id, u16 turn,
                                           const u8 *digest, u16 digest_size);
/* Hashes the stable post-turn battle state using the fixed digest version.
 * The output is canonical member order and may be compared across both ROMs. */
bool8 CoopBattleRuntime_ComputeBattleDigest(u8 *digest, u16 capacity);
/* One captured end-turn digest may wait for bridge capacity or reconnection.
 * The battle script must remain on EndTurnEvents while the result is pending. */
enum CoopBattleTurnReportResult
{
    COOP_BATTLE_TURN_REPORT_INVALID,
    COOP_BATTLE_TURN_REPORT_PENDING,
    COOP_BATTLE_TURN_REPORT_SENT,
};
enum CoopBattleTurnReportResult CoopBattleRuntime_ReportTurnState(u16 turn, u8 outcome);
bool8 CoopBattleRuntime_IsTurnHashPending(void);
enum CoopBattleTurnReportResult CoopBattleRuntime_RetryTurnHash(void);
#if TESTING
enum CoopBattleTurnReportResult CoopBattleRuntime_TestReportTurnDigest(u16 turn, u8 outcome,
                                                                      const u8 *digest);
#endif
void CoopBattleRuntime_PollTerminal(void);
bool8 CoopBattleRuntime_IsBattleFinishedSent(void);
void CoopBattleRuntime_CompleteBattle(void);
bool8 CoopBattleRuntime_SendBattleFinished(const u8 *battle_id, u16 turn,
                                           u8 result, const u8 *digest,
                                           u16 digest_size);

#endif
