#ifndef GUARD_COOP_BATTLE_RUNTIME_H
#define GUARD_COOP_BATTLE_RUNTIME_H

#include "gba/types.h"
#include "coop/battle_protocol.h"
#include "pokemon.h"

/* Each member stages one to three usable mons, never more. */
#define COOP_BATTLE_MULTI_PARTY_SIZE 3
/* Slot index recorded for a staged entry beyond the member's staged count. */
#define COOP_BATTLE_UNUSED_SLOT 0xFF
#define COOP_BATTLE_ACTION_SIZE 4
/* A friendly doubles member acts for two battlers per round. */
#define COOP_BATTLE_MAX_MEMBER_ACTIONS 2
#define COOP_BATTLE_MAX_MEMBER_ACTION_BYTES (COOP_BATTLE_ACTION_SIZE * COOP_BATTLE_MAX_MEMBER_ACTIONS)
/* Turn bundles are consumed one round at a time; a small ring suffices. */
#define COOP_BATTLE_BUNDLE_QUEUE 4
#define COOP_BATTLE_DIGEST_VERSION 1
// Requests server cancellation for the currently tracked battle. The ROM waits
// for the terminal ABORT_BATTLE before clearing its battle state.
bool8 CoopBattleRuntime_RequestAbort(u8 reason);

/* Canonical action bytes are independent of the local battler numbering.
 * Targets are BattlerPosition values, with the two player positions ordered
 * by manifest member slot. Run exists only as a friendly forfeit; the bag
 * only as a co-op trainer battle item action. */
enum CoopBattleActionKind
{
    COOP_BATTLE_ACTION_MOVE = 1,
    COOP_BATTLE_ACTION_SWITCH = 2,
    COOP_BATTLE_ACTION_FORCED_SWITCH = 3,
    COOP_BATTLE_ACTION_NO_ACTION = 4,
    COOP_BATTLE_ACTION_AUTO_MOVE = 5,
    /* Friendly battles only: the member gives up (Run). */
    COOP_BATTLE_ACTION_FORFEIT = 6,
    /* Co-op trainer battles only: a bag item on one of the member's own
     * staged Pokemon. Wire: kind, item (u16 LE), then the party slot in the
     * low nibble and, for a one-move PP item, the move slot in bits 4-5. */
    COOP_BATTLE_ACTION_ITEM = 7,
};

/* For COOP_BATTLE_ACTION_ITEM, index is the member's own staged party slot
 * and target the move slot (0 unless a one-move PP item). */
struct CoopBattleAction
{
    u8 kind;
    u8 index;
    u8 target;
    u16 item;
};

/* The battle usages a co-op trainer battle item action may carry. */
bool8 CoopBattleRuntime_IsSharedBattleItem(u16 item);

/* An inert staging result. Battle setup must own any later party swap and
 * restoration; preparing this value never changes the saved player party.
 * Entries at and beyond local_count/peer_count are zeroed mons whose slot
 * index is COOP_BATTLE_UNUSED_SLOT. */
struct CoopBattlePreparedParty
{
    struct Pokemon local[COOP_BATTLE_MULTI_PARTY_SIZE];
    struct Pokemon peer[COOP_BATTLE_MULTI_PARTY_SIZE];
    u8 local_slots[COOP_BATTLE_MULTI_PARTY_SIZE];
    u8 peer_slots[COOP_BATTLE_MULTI_PARTY_SIZE];
    u8 local_count;
    u8 peer_count;
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
    /* 1..COOP_BATTLE_MULTI_PARTY_SIZE; these fill existing alignment padding
     * before original_local, so the EWRAM plan does not grow. */
    u8 staged_local_count;
    u8 staged_peer_count;
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

/* The challenge a friendly battle was accepted with. */
struct CoopBattleFriendlyRules
{
    u8 format;     // COOP_BATTLE_FRIENDLY_SINGLES / _DOUBLES
    u8 level_mode; // COOP_BATTLE_FRIENDLY_LEVELS_AS_IS / _50
    u8 count;      // 1..6 Pokemon per side (2..6 for doubles)
};

bool8 CoopBattleRuntime_IsValidFriendlyRules(const struct CoopBattleFriendlyRules *rules);
/* Reads/writes the three rule bytes shared by the reserve, offer and manifest. */
bool8 CoopBattleRuntime_DecodeFriendlyRules(const u8 *bytes, struct CoopBattleFriendlyRules *rules);
void CoopBattleRuntime_EncodeFriendlyRules(const struct CoopBattleFriendlyRules *rules, u8 *bytes);
bool8 CoopBattleRuntime_GetFriendlyRules(const u8 *battle_id, struct CoopBattleFriendlyRules *rules);

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
 * and at least one usable (non-egg, HP > 0) mon on each side. Stages the
 * first one to three usable mons per side in party order. On failure,
 * prepared is left unchanged. */
bool8 CoopBattleRuntime_PreparePartnerParty(const u8 *battle_id,
                                           const struct Pokemon *local_party,
                                           u8 local_count,
                                           struct CoopBattlePreparedParty *prepared);
/* Hash the exact ordered party records used by the launcher snapshot commit. */
bool8 CoopBattleRuntime_ComputePartyDigest(const struct Pokemon *party,
                                           u8 count, u8 *digest, u16 capacity);
/* Any trainer identity that resolves through the ROM identity registry in the
 * active region is accepted; the opponent is its legacy trainer ID. All
 * identity fields come from the current validated manifest. No live state
 * changes. */
bool8 CoopBattleRuntime_MakeStartupPlan(const u8 *battle_id,
                                        const struct Pokemon *local_party,
                                        u8 local_count,
                                        struct CoopBattleStartupPlan *plan);
/* Restore the full pre-battle player party, overlaying the staged local
 * mons' battle results (staged_local_count of them) at their original
 * slots. Does not save. */
bool8 CoopBattleRuntime_RestoreLocalParty(const struct CoopBattleStartupPlan *plan,
                                         const u8 *battle_id,
                                         const struct Pokemon *battled_local,
                                         struct Pokemon *restored_local);
/* Friendly battles. local_party holds PARTY_SIZE records (the live party);
 * team is the ordered selection (rules.count records) this ROM sent as its
 * snapshot; it must match the manifest's digest for the local member, and
 * the peer's complete snapshot must hold rules.count usable records. On
 * success the plan holds the battle ID, the member slot and all six
 * pre-battle records (original_local) for a byte-exact restore; staged
 * counts are rules.count. Nothing live changes. */
bool8 CoopBattleRuntime_MakeFriendlyPlan(const u8 *battle_id,
                                         const struct Pokemon *local_party,
                                         u8 local_count,
                                         const struct Pokemon *team,
                                         u8 team_count,
                                         struct CoopBattleStartupPlan *plan);
/* Copies the peer's staged records (the opponent side of a friendly battle). */
bool8 CoopBattleRuntime_CopyPeerTeam(struct Pokemon *team, u8 capacity, u8 *count);
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
void CoopBattleRuntime_TestSetBattleFinishedSent(void);
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
/* Latched at arm time, so they stay stable through a fault/abort cleanup. */
bool8 CoopBattleRuntime_IsFriendlyEngine(void);
bool8 CoopBattleRuntime_IsFriendlyDoubles(void);
/* Friendly battles number battlers canonically: member 0 owns battlers 0/2
 * and member 1 owns battlers 1/3 on both ROMs; member 1's ROM draws them
 * mirrored (its own battlers at the bottom). Trainer battles swap the two
 * player battlers on member 1's ROM. Returns the battler both ROMs agree on. */
u8 CoopBattleRuntime_CanonicalBattler(u8 battler);
bool8 CoopBattleRuntime_IsEngineFaulted(void);
bool8 CoopBattleRuntime_IsSessionReady(void);
u8 CoopBattleRuntime_EngineLocalMemberSlot(void);
bool8 CoopBattleRuntime_IsLocalActionSubmitted(void);
bool8 CoopBattleRuntime_SubmitLocalAction(const struct CoopBattleAction *action);
bool8 CoopBattleRuntime_PollPeerAction(struct CoopBattleAction *action);
/* Friendly: one action per member battler (1 singles, 2 doubles). */
bool8 CoopBattleRuntime_SubmitLocalActions(const struct CoopBattleAction *actions, u8 count);
bool8 CoopBattleRuntime_PollPeerActions(struct CoopBattleAction *actions, u8 *count);
/* The local actions of the round in progress (after SubmitLocalActions). */
bool8 CoopBattleRuntime_GetLocalActions(struct CoopBattleAction *actions, u8 *count);
/* Friendly: a decision round ends once every controller took its actions. */
void CoopBattleRuntime_EndDecisionRound(void);
/* The round whose bundle was taken last (0 before the first turn). */
u16 CoopBattleRuntime_CurrentRound(void);
/* TRUE once the current round's state hash has been sent. */
bool8 CoopBattleRuntime_IsRoundHashed(void);
/* Ends the battle on the last hashed round (a decision round already hashed
 * the final state) with the given local-perspective battle outcome. */
bool8 CoopBattleRuntime_FinishOnLastHash(u8 outcome);
/* Used when the native engine bypasses B2's controller for an automatic turn. */
bool8 CoopBattleRuntime_ConfirmPeerAutomaticAction(u8 expected_kind);
/* Latch a local unsupported/desynced decision until battle cleanup disarms. */
void CoopBattleRuntime_FailEngine(void);
/* Seed for the random choices in a co-op opponent party that the trainer
 * data leaves open (the second mon of a single-mon trainer). It hashes only
 * the battle ID [0, 16), the battle seed [18, 50) and the trainer ID: bytes
 * that are identical on both ROMs, unlike the turn or member-slot bytes. */
u32 CoopBattleRuntime_DeriveOpponentSeed(const u8 *manifest, u16 trainer_id);
bool8 CoopBattleRuntime_GetOpponentSeed(u16 trainer_id, u32 *seed);
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
/* The terminal attestation was captured (sent, or waiting for the bridge). */
bool8 CoopBattleRuntime_IsTerminalReported(void);
enum CoopBattleTurnReportResult CoopBattleRuntime_RetryTurnHash(void);
/* Friendly: a decision inside a turn (a replacement after a faint, U-turn)
 * is a round of its own. The turn's state is hashed first, at the same
 * point on both ROMs, so the next round's action can be accepted. */
enum CoopBattleTurnReportResult CoopBattleRuntime_FlushRoundHash(void);
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
