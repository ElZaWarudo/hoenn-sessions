#ifndef GUARD_COOP_NET_BRIDGE_H
#define GUARD_COOP_NET_BRIDGE_H

#include <stddef.h>

#include "gba/defines.h"
#include "gba/types.h"
#include "coop/region.h"

#define COOP_NET_BRIDGE_MAGIC 0x504B434Fu
#define COOP_NET_BRIDGE_ABI_VERSION 1
#define COOP_NET_BRIDGE_GAME_PROTOCOL_VERSION 1
#define COOP_NET_BRIDGE_GAME_BUILD_ID 0x00010000u
#define COOP_NET_BRIDGE_PAYLOAD_SIZE 128
#define COOP_NET_BRIDGE_QUEUE_CAPACITY 32

enum CoopBridgeMessageType
{
    COOP_BRIDGE_MESSAGE_NONE = 0,

    COOP_BRIDGE_MESSAGE_ROM_READY = 1,
    COOP_BRIDGE_MESSAGE_PLAYER_STATE = 2,
    COOP_BRIDGE_MESSAGE_INTERACT_REMOTE_PLAYER = 3,
    COOP_BRIDGE_MESSAGE_GROUP_INVITE_REQUEST = 4,
    COOP_BRIDGE_MESSAGE_TRAINER_BATTLE_RESERVE = 5,
    COOP_BRIDGE_MESSAGE_BATTLE_JOIN_RESPONSE = 6,
    COOP_BRIDGE_MESSAGE_PARTY_SNAPSHOT = 7,
    COOP_BRIDGE_MESSAGE_ACTION_INTENT = 8,
    COOP_BRIDGE_MESSAGE_TURN_RESULT_HASH = 9,
    COOP_BRIDGE_MESSAGE_BATTLE_FINISHED = 10,
    COOP_BRIDGE_MESSAGE_COMMIT_APPLIED = 11,
    COOP_BRIDGE_MESSAGE_CHECKPOINT_READY = 12,
    COOP_BRIDGE_MESSAGE_SAVE_DATA_UPDATED = 13,

    COOP_BRIDGE_MESSAGE_SESSION_READY = 0x0100,
    COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_SPAWN = 0x0101,
    COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_UPDATE = 0x0102,
    COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_DESPAWN = 0x0103,
    COOP_BRIDGE_MESSAGE_GROUP_INVITE_RECEIVED = 0x0104,
    COOP_BRIDGE_MESSAGE_GROUP_STATE_CHANGED = 0x0105,
    COOP_BRIDGE_MESSAGE_BATTLE_JOIN_OFFER = 0x0106,
    COOP_BRIDGE_MESSAGE_BATTLE_MANIFEST = 0x0107,
    COOP_BRIDGE_MESSAGE_TURN_BUNDLE = 0x0108,
    COOP_BRIDGE_MESSAGE_PAUSE_FOR_RECONNECT = 0x0109,
    COOP_BRIDGE_MESSAGE_BATTLE_COMMIT = 0x010A,
    COOP_BRIDGE_MESSAGE_ABORT_BATTLE = 0x010B,
    COOP_BRIDGE_MESSAGE_CHECKPOINT_GRANTED = 0x010C,
};

enum CoopBridgeStatus
{
    COOP_BRIDGE_STATUS_INITIALIZED = (1 << 0),
    COOP_BRIDGE_STATUS_ROM_READY_SENT = (1 << 1),
    COOP_BRIDGE_STATUS_SESSION_READY = (1 << 2),
    COOP_BRIDGE_STATUS_PLAYER_STATE_SENT = (1 << 3),
    COOP_BRIDGE_STATUS_QUEUE_CONGESTED = (1 << 4),
    COOP_BRIDGE_STATUS_QUEUE_ERROR = (1 << 5),
    COOP_BRIDGE_STATUS_CHECKSUM_ERROR = (1 << 6),
    COOP_BRIDGE_STATUS_SIDECAR_HEARTBEAT_SEEN = (1 << 7),
    COOP_BRIDGE_STATUS_SIDECAR_HEARTBEAT_STALE = (1 << 8),
    COOP_BRIDGE_STATUS_WORLD_NOT_READY = (1 << 9),
    COOP_BRIDGE_STATUS_PROTOCOL_ERROR = (1 << 10),
};

#define COOP_NET_BRIDGE_PLAYER_STATE_INTERVAL 6
#define COOP_NET_BRIDGE_SIDECAR_STALE_INTERVAL 180
#define COOP_NET_BRIDGE_CHECKPOINT_TIMEOUT_FRAMES 180

/* Checkpoint coordination is deliberately kept outside the wire structure.
 * The structure above is an ABI shared with Lua and changing it would make
 * old sidecars unsafe to use. */
enum CoopCheckpointState
{
    COOP_CHECKPOINT_STATE_OFFLINE = 0,
    COOP_CHECKPOINT_STATE_IDLE,
    COOP_CHECKPOINT_STATE_WAITING_FOR_GRANT,
    COOP_CHECKPOINT_STATE_GRANTED,
    COOP_CHECKPOINT_STATE_SAVING,
    /* A save completed, but its completion event belongs to an older epoch.
     * Normal save/checkpoint work is prohibited until an operator recovers
     * the unreported save. */
    COOP_CHECKPOINT_STATE_RECOVERY_REQUIRED,
};

enum CoopCheckpointRequestResult
{
    /* No cloud epoch has ever been accepted; normal local saves are allowed. */
    COOP_CHECKPOINT_REQUEST_OFFLINE = 0,
    /* A ready message was queued and the caller must wait for a grant. */
    COOP_CHECKPOINT_REQUEST_STARTED,
    /* A cloud epoch exists, but the active session is not safe to checkpoint. */
    COOP_CHECKPOINT_REQUEST_REJECTED,
};

/* Stable, compact, little-endian ROM/Lua message defined by the product ABI.
 * checksum is CRC-32/IEEE over bytes 0..139, including zero-filled payload. */
struct CoopBridgeMessage
{
    /* 0x00 */ u16 type;
    /* 0x02 */ u16 length;
    /* 0x04 */ u32 sequence;
    /* 0x08 */ u32 session_epoch;
    /* 0x0C */ u8 payload[COOP_NET_BRIDGE_PAYLOAD_SIZE];
    /* 0x8C */ u32 checksum;
};

struct CoopBridgeQueue
{
    /* Monotonic wrapping counters distinguish full from empty. The producer
     * publishes write_index only after writing the complete message. */
    /* 0x0000 */ volatile u16 read_index;
    /* 0x0002 */ volatile u16 write_index;
    /* 0x0004 */ struct CoopBridgeMessage entries[COOP_NET_BRIDGE_QUEUE_CAPACITY];
};

struct CoopNetBridge
{
    /* 0x0000 */ u32 magic;
    /* 0x0004 */ u16 abi_version;
    /* 0x0006 */ u16 game_protocol_version;
    /* 0x0008 */ u32 game_build_id;
    /* 0x000C */ u32 status_flags;
    /* Host-owned heartbeat counter, sampled by the ROM once per frame. */
    /* 0x0010 */ volatile u32 last_sidecar_heartbeat;
    /* 0x0014 */ struct CoopBridgeQueue game_to_network;
    /* ...    */ struct CoopBridgeQueue network_to_game;
};

struct CoopBridgePlayerState
{
    struct WorldLocation location;
    u8 reserved[2];
    u32 frame_counter;
};

_Static_assert(sizeof(struct CoopBridgeMessage) == 144, "CoopBridgeMessage ABI size");
_Static_assert(offsetof(struct CoopBridgeMessage, type) == 0, "CoopBridgeMessage type offset");
_Static_assert(offsetof(struct CoopBridgeMessage, length) == 2, "CoopBridgeMessage length offset");
_Static_assert(offsetof(struct CoopBridgeMessage, sequence) == 4, "CoopBridgeMessage sequence offset");
_Static_assert(offsetof(struct CoopBridgeMessage, session_epoch) == 8, "CoopBridgeMessage session offset");
_Static_assert(offsetof(struct CoopBridgeMessage, payload) == 12, "CoopBridgeMessage payload offset");
_Static_assert(offsetof(struct CoopBridgeMessage, checksum) == 140, "CoopBridgeMessage checksum offset");
_Static_assert(sizeof(struct CoopBridgeQueue) == 4 + 144 * 32, "CoopBridgeQueue ABI size");
_Static_assert(offsetof(struct CoopBridgeQueue, read_index) == 0, "CoopBridgeQueue read offset");
_Static_assert(offsetof(struct CoopBridgeQueue, write_index) == 2, "CoopBridgeQueue write offset");
_Static_assert(offsetof(struct CoopBridgeQueue, entries) == 4, "CoopBridgeQueue entries offset");
_Static_assert(sizeof(struct CoopBridgePlayerState) == 16, "CoopBridgePlayerState ABI size");
_Static_assert(offsetof(struct CoopNetBridge, magic) == 0, "CoopNetBridge magic offset");
_Static_assert(offsetof(struct CoopNetBridge, abi_version) == 4, "CoopNetBridge ABI version offset");
_Static_assert(offsetof(struct CoopNetBridge, game_protocol_version) == 6, "CoopNetBridge protocol offset");
_Static_assert(offsetof(struct CoopNetBridge, game_build_id) == 8, "CoopNetBridge build offset");
_Static_assert(offsetof(struct CoopNetBridge, status_flags) == 12, "CoopNetBridge status offset");
_Static_assert(offsetof(struct CoopNetBridge, last_sidecar_heartbeat) == 16, "CoopNetBridge heartbeat offset");
_Static_assert(offsetof(struct CoopNetBridge, game_to_network) == 20, "CoopNetBridge tx queue offset");
_Static_assert(offsetof(struct CoopNetBridge, network_to_game) == 20 + sizeof(struct CoopBridgeQueue), "CoopNetBridge rx queue offset");
_Static_assert(sizeof(struct CoopNetBridge) == 20 + sizeof(struct CoopBridgeQueue) * 2, "CoopNetBridge ABI size");

extern EWRAM_DATA struct CoopNetBridge gCoopNetBridge;

u32 CoopBridge_Crc32(const void *data, u32 length);
u32 CoopBridgeMessage_ComputeChecksum(const struct CoopBridgeMessage *message);
bool8 CoopBridgeMessage_Seal(struct CoopBridgeMessage *message, u16 type,
                             u32 sequence, u32 session_epoch,
                             const void *payload, u16 payload_size);
bool8 CoopBridgeMessage_Validate(const struct CoopBridgeMessage *message);

void CoopBridgeQueue_Init(struct CoopBridgeQueue *queue);
bool8 CoopBridgeQueue_IsEmpty(const struct CoopBridgeQueue *queue);
bool8 CoopBridgeQueue_IsFull(const struct CoopBridgeQueue *queue);
bool8 CoopBridgeQueue_Push(struct CoopBridgeQueue *queue, const struct CoopBridgeMessage *message);
bool8 CoopBridgeQueue_Pop(struct CoopBridgeQueue *queue, struct CoopBridgeMessage *message);

void CoopNetBridge_Init(void);
void CoopNetBridge_Poll(void);
enum CoopCheckpointState CoopNetBridge_GetCheckpointState(void);
bool8 CoopNetBridge_IsCloudMode(void);
bool8 CoopNetBridge_IsRecoveryRequired(void);
enum CoopCheckpointRequestResult CoopNetBridge_RequestCheckpoint(void);
bool8 CoopNetBridge_ConsumeCheckpointGrant(void);
bool8 CoopNetBridge_IsCheckpointAuthorizedForSave(void);
/* Called by the normal save path after TrySavingData has completed. A failed
 * save never emits a wire message; a successful save queues one critical
 * SAVE_DATA_UPDATED with the sealed generation as an exact four-byte
 * little-endian payload, and retries it from the poll loop if the outbound
 * ring is full. */
void CoopNetBridge_NotifySaveResult(bool8 save_succeeded);
bool8 CoopNetBridge_EnqueueGameToNetwork(u16 type, const void *payload, u16 payload_size);
bool8 CoopNetBridge_DequeueGameToNetwork(struct CoopBridgeMessage *message);
bool8 CoopNetBridge_EnqueueNetworkToGame(const struct CoopBridgeMessage *message);
bool8 CoopNetBridge_DequeueNetworkToGame(struct CoopBridgeMessage *message);

#if TESTING
enum CoopStartMenuTestSaveResult
{
    COOP_START_MENU_TEST_SAVE_IN_PROGRESS = 0,
    COOP_START_MENU_TEST_SAVE_SUCCESS,
    COOP_START_MENU_TEST_SAVE_CANCELED,
    COOP_START_MENU_TEST_SAVE_ERROR,
};

/* The ROM test image has no existing start-menu callback harness. These
 * test-only entry points exercise the production post-confirmation callbacks
 * without opening UI windows or touching flash. */
void CoopStartMenu_TestSetSaveDryRun(bool8 enabled);
void CoopStartMenu_TestSetCheckpointRequired(bool8 required);
u8 CoopStartMenu_TestRunSaveSavingMessageCallback(void);
u8 CoopStartMenu_TestRunSaveDoSaveCallback(void);
u8 CoopStartMenu_TestRunCheckpointWaitCallback(void);
u8 CoopStartMenu_TestRunCheckpointAbortCallback(void);
u8 CoopStartMenu_TestRunAuthorizedSaveCallback(void);
#endif

#define CoopNetBridge_PushTx CoopNetBridge_EnqueueGameToNetwork
#define CoopNetBridge_PopTx CoopNetBridge_DequeueGameToNetwork
#define CoopNetBridge_PushRx CoopNetBridge_EnqueueNetworkToGame
#define CoopNetBridge_PopRx CoopNetBridge_DequeueNetworkToGame

#endif /* GUARD_COOP_NET_BRIDGE_H */
