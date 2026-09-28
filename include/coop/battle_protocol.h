#ifndef GUARD_COOP_BATTLE_PROTOCOL_H
#define GUARD_COOP_BATTLE_PROTOCOL_H

#include "gba/types.h"

// These are bridge payloads, not native C structs. Read/write u16 fields in
// little-endian order; UUID octets use the standard UUID byte order.
#define COOP_BATTLE_ID_SIZE 16
#define COOP_BATTLE_DIGEST_SIZE 32
#define COOP_BATTLE_PARTY_MON_SIZE 100
#define COOP_BATTLE_MAX_ACTION_SIZE 48
#define COOP_BATTLE_MAX_TURN 32
#define COOP_BATTLE_RESERVE_SIZE 5
#define COOP_BATTLE_TRAINER_RESERVE_SIZE 8
#define COOP_BATTLE_JOIN_RESPONSE_SIZE 17
#define COOP_BATTLE_JOIN_OFFER_SIZE 22
#define COOP_BATTLE_CONSENT_OUTCOME_SIZE 21
#define COOP_BATTLE_PARTY_SNAPSHOT_SIZE 120
#define COOP_BATTLE_TURN_RESULT_HASH_SIZE 50
#define COOP_BATTLE_FINISHED_SIZE 51
#define COOP_BATTLE_MANIFEST_SIZE 119
#define COOP_BATTLE_MANIFEST_KIND_OFFSET 114
#define COOP_BATTLE_MANIFEST_MEMBER_SLOT_OFFSET 115
#define COOP_BATTLE_MANIFEST_REGION_OFFSET 116
#define COOP_BATTLE_MANIFEST_TRAINER_ORDINAL_OFFSET 117
#define COOP_BATTLE_MANIFEST_KIND_TRAINER 1
#define COOP_BATTLE_MANIFEST_KIND_FRIENDLY 2
#define COOP_BATTLE_PAUSE_FOR_RECONNECT_SIZE 19
#define COOP_BATTLE_ABORT_SIZE 17
#define COOP_BATTLE_ABORT_REQUEST_SIZE 17
#define COOP_BATTLE_COMMIT_SIZE 43
#define COOP_BATTLE_COMMIT_BATTLE_ID_OFFSET 0
#define COOP_BATTLE_COMMIT_ID_OFFSET 16
#define COOP_BATTLE_COMMIT_REGION_OFFSET 32
#define COOP_BATTLE_COMMIT_TRAINER_ORDINAL_OFFSET 33
#define COOP_BATTLE_COMMIT_SOURCE_REVISION_OFFSET 35
#define COOP_BATTLE_RESERVE_REJECTED_SIZE 4
#define COOP_BATTLE_READY_SIZE 48
#define COOP_BATTLE_START_SIZE 16
// BATTLE_RESERVE_REJECTED (0x0116): nonzero request nonce, little endian.
// BATTLE_ABORT_REQUEST (0x0014): UUID [0..16), reason [16] (1..5).
// BATTLE_READY (0x0015): UUID [0..16), canonical live-party SHA-256 [16..48).
// BATTLE_START (0x0117): UUID [0..16), released only after both ready writes.

// TRAINER_BATTLE_RESERVE (0x0005): [0] kind (1 cooperative trainer,
// 2 friendly), [1..5) nonzero request nonce (u32 little-endian).
// Cooperative trainer additionally carries [5] concrete region (CoopRegion),
// [6..8) stable trainer ordinal (u16 little-endian). Friendly has exactly
// five bytes.
// BATTLE_JOIN_RESPONSE (0x0006): [0..16) battle UUID, [16] decision
// (0 decline, 1 accept).
// BATTLE_JOIN_OFFER (0x0106): [0..16) battle UUID, [16] kind as above,
// [17] role (0 requester, 1 responder), [18..22) request nonce (u32 LE).
// The requester's nonce echoes the nonzero reserve nonce; the responder's
// nonce is zero.
// BATTLE_CONSENT_OUTCOME (0x0115): [0..16) battle UUID,
// [16..20) request nonce (u32 LE), [20] outcome (1 accepted, 2 declined,
// 3 expired). This is delivered only to the requester and may be replayed
// after a reconnect.
// PARTY_SNAPSHOT (0x0007): [0..16) battle UUID, [16] party slot 0..5,
// [17] chunk index (=party slot), [18] party/chunk count 1..6,
// [19] mon length (=100),
// [20..120) opaque 100-byte party-mon snapshot. One mon per frame.
// PEER_PARTY_CHUNK (0x0114): same 120-byte layout, sidecar to ROM.
// ACTION_INTENT (0x0008): UUID [0..16), LE turn [16..18), action length
// [18], compact opaque action [19..19+length), length 1..48.
// TURN_RESULT_HASH (0x0009): UUID [0..16), LE turn [16..18),
// binary 32-byte digest [18..50).
// BATTLE_MANIFEST (0x0107): UUID [0..16), LE turn [16..18) (zero is
// initial manifest), 32-byte seed [18..50), first member's snapshot hash
// [50..82), second member's snapshot hash [82..114), kind [114]
// (1 trainer, 2 friendly), local member slot [115] (0 or 1), trainer
// CoopRegion [116], LE trainer ordinal [117..119). Friendly requires
// region and ordinal zero; trainer requires a concrete region.
// TURN_BUNDLE (0x0108): UUID [0..16), LE turn [16..18), first/second
// action lengths [18]/[19], then both compact actions in member order.
// Each length is 1..48; total payload is 22..116 bytes.
// PAUSE_FOR_RECONNECT (0x0109): UUID [0..16), LE turn [16..18),
// missing member slot [18] (0 or 1).
// BATTLE_COMMIT (0x010A) and COMMIT_APPLIED (0x000B): battle UUID
// [0..16), commit UUID [16..32), CoopRegion [32], LE trainer ordinal
// [33..35), LE source snapshot revision [35..43). Both UUIDs, region,
// ordinal, and revision are nonzero. The ROM must echo the complete record
// in COMMIT_APPLIED after the corresponding save has been written.
// ABORT_BATTLE (0x010B): UUID [0..16), reason [16] (1..5).
// No payload flags or trailing bytes are accepted for any record.

enum CoopBattleAbortReason
{
    COOP_BATTLE_ABORT_CANCELED = 1,
    COOP_BATTLE_ABORT_DISCONNECTED = 2,
    COOP_BATTLE_ABORT_DESYNC = 3,
    COOP_BATTLE_ABORT_EXPIRED = 4,
    COOP_BATTLE_ABORT_UNAVAILABLE = 5,
};

enum CoopBattleFinishedResult
{
    COOP_BATTLE_FINISHED_MEMBER0_WON = 1,
    COOP_BATTLE_FINISHED_MEMBER1_WON = 2,
    COOP_BATTLE_FINISHED_DRAW = 3,
    COOP_BATTLE_FINISHED_WON = 4,
    COOP_BATTLE_FINISHED_LOST = 5,
};

enum CoopBattleConsentOutcome
{
    COOP_BATTLE_CONSENT_ACCEPTED = 1,
    COOP_BATTLE_CONSENT_DECLINED = 2,
    COOP_BATTLE_CONSENT_EXPIRED = 3,
};

#endif
