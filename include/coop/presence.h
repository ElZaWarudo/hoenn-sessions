#ifndef GUARD_COOP_PRESENCE_H
#define GUARD_COOP_PRESENCE_H

#include "gba/types.h"
#include "coop/region.h"

/*
 * Presence V1 is a payload contract, not a C ABI.  The value structures below
 * are deliberately not packed and must never be read from, or written to, a
 * wire buffer by casting.  The codec functions translate each scalar at its
 * documented little-endian offset.
 */
#define COOP_PRESENCE_WORLD_LOCATION_SIZE 10
#define COOP_PRESENCE_POSE_SIZE 24
#define COOP_PRESENCE_LOCAL_STATE_SIZE 28
#define COOP_PRESENCE_SPAWN_SIZE 72
#define COOP_PRESENCE_UPDATE_SIZE 40
#define COOP_PRESENCE_DESPAWN_SIZE 16
#define COOP_PRESENCE_INTERACTION_SIZE 20

/* Versioned spellings keep the wire revision explicit at call sites. */
#define COOP_PRESENCE_WORLD_LOCATION_V1_SIZE COOP_PRESENCE_WORLD_LOCATION_SIZE
#define COOP_PRESENCE_POSE_V1_SIZE COOP_PRESENCE_POSE_SIZE
#define COOP_PRESENCE_LOCAL_STATE_V1_SIZE COOP_PRESENCE_LOCAL_STATE_SIZE
#define COOP_PRESENCE_SPAWN_V1_SIZE COOP_PRESENCE_SPAWN_SIZE
#define COOP_PRESENCE_UPDATE_V1_SIZE COOP_PRESENCE_UPDATE_SIZE
#define COOP_PRESENCE_DESPAWN_V1_SIZE COOP_PRESENCE_DESPAWN_SIZE
#define COOP_PRESENCE_INTERACTION_V1_SIZE COOP_PRESENCE_INTERACTION_SIZE

#define COOP_PRESENCE_WORLD_LOCATION_REGION_OFFSET 0
#define COOP_PRESENCE_WORLD_LOCATION_MAP_GROUP_OFFSET 1
#define COOP_PRESENCE_WORLD_LOCATION_MAP_NUMBER_OFFSET 3
#define COOP_PRESENCE_WORLD_LOCATION_X_OFFSET 5
#define COOP_PRESENCE_WORLD_LOCATION_Y_OFFSET 7
#define COOP_PRESENCE_WORLD_LOCATION_RESERVED_OFFSET 9

#define COOP_PRESENCE_POSE_LOCATION_OFFSET 0
#define COOP_PRESENCE_POSE_ELEVATION_OFFSET 10
#define COOP_PRESENCE_POSE_DIRECTION_OFFSET 11
#define COOP_PRESENCE_POSE_CLIENT_TICK_OFFSET 12
#define COOP_PRESENCE_POSE_WARP_SEQUENCE_OFFSET 16
#define COOP_PRESENCE_POSE_MOVEMENT_MODE_OFFSET 20
#define COOP_PRESENCE_POSE_ANIMATION_ID_OFFSET 21
#define COOP_PRESENCE_POSE_AVATAR_ID_OFFSET 22
#define COOP_PRESENCE_POSE_PLAYER_STATE_OFFSET 23

#define COOP_PRESENCE_LOCAL_STATE_POSE_OFFSET 0
#define COOP_PRESENCE_LOCAL_STATE_SOURCE_SEQUENCE_OFFSET 24

#define COOP_PRESENCE_SPAWN_HANDLE_OFFSET 0
#define COOP_PRESENCE_SPAWN_SERVER_SEQUENCE_OFFSET 8
#define COOP_PRESENCE_SPAWN_STATE_OFFSET 12
#define COOP_PRESENCE_SPAWN_USERNAME_OFFSET 40
#define COOP_PRESENCE_SPAWN_USERNAME_SIZE 32

#define COOP_PRESENCE_UPDATE_HANDLE_OFFSET 0
#define COOP_PRESENCE_UPDATE_SERVER_SEQUENCE_OFFSET 8
#define COOP_PRESENCE_UPDATE_STATE_OFFSET 12

#define COOP_PRESENCE_DESPAWN_HANDLE_OFFSET 0
#define COOP_PRESENCE_DESPAWN_SERVER_SEQUENCE_OFFSET 8
#define COOP_PRESENCE_DESPAWN_REASON_OFFSET 12
#define COOP_PRESENCE_DESPAWN_RESERVED_OFFSET 13

#define COOP_PRESENCE_INTERACTION_HANDLE_OFFSET 0
#define COOP_PRESENCE_INTERACTION_SERVER_SEQUENCE_OFFSET 8
#define COOP_PRESENCE_INTERACTION_WARP_SEQUENCE_OFFSET 12
#define COOP_PRESENCE_INTERACTION_X_OFFSET 16
#define COOP_PRESENCE_INTERACTION_Y_OFFSET 18

#define COOP_PRESENCE_USERNAME_MAX 32

enum CoopPresenceDirection
{
    COOP_PRESENCE_DIRECTION_SOUTH = 1,
    COOP_PRESENCE_DIRECTION_NORTH = 2,
    COOP_PRESENCE_DIRECTION_WEST = 3,
    COOP_PRESENCE_DIRECTION_EAST = 4,
};

enum CoopPresenceMovementMode
{
    COOP_PRESENCE_MOVEMENT_IDLE = 0,
    COOP_PRESENCE_MOVEMENT_WALK = 1,
    COOP_PRESENCE_MOVEMENT_RUN = 2,
};

enum CoopPresenceAnimationId
{
    COOP_PRESENCE_ANIMATION_IDLE = 0,
    COOP_PRESENCE_ANIMATION_LOCOMOTION = 1,
};

enum CoopPresenceAvatarId
{
    COOP_PRESENCE_AVATAR_BRENDAN = 1,
    COOP_PRESENCE_AVATAR_MAY = 2,
};

enum CoopPresencePlayerState
{
    COOP_PRESENCE_PLAYER_HIDDEN = 0,
    COOP_PRESENCE_PLAYER_OVERWORLD = 1,
};

enum CoopPresenceDespawnReason
{
    COOP_PRESENCE_DESPAWN_HIDDEN = 1,
    COOP_PRESENCE_DESPAWN_STALE = 2,
    COOP_PRESENCE_DESPAWN_DISCONNECTED = 3,
    COOP_PRESENCE_DESPAWN_LEASE_INVALID = 4,
    COOP_PRESENCE_DESPAWN_REPLACED = 5,
    COOP_PRESENCE_DESPAWN_PARTITION_LEFT = 6,
};

/* bytes[0..length) are content and bytes[length] is the required terminator.
 * Later bytes are nonsemantic; encoders canonicalize unused wire padding. */
struct CoopPresenceUsername
{
    u8 length;
    char bytes[COOP_PRESENCE_USERNAME_MAX + 1];
};

struct CoopPresencePose
{
    struct WorldLocation location;
    u8 elevation;
    u8 direction;
    u32 client_tick;
    u32 warp_sequence;
    u8 movement_mode;
    u8 animation_id;
    u8 avatar_id;
    u8 player_state;
};

struct CoopPresenceLocalState
{
    struct CoopPresencePose pose;
    u32 source_sequence;
};

struct CoopPresenceSpawn
{
    u64 handle;
    u32 server_sequence;
    struct CoopPresenceLocalState state;
    struct CoopPresenceUsername username;
};

struct CoopPresenceUpdate
{
    u64 handle;
    u32 server_sequence;
    struct CoopPresenceLocalState state;
};

struct CoopPresenceDespawn
{
    u64 handle;
    u32 server_sequence;
    u8 reason;
};

struct CoopPresenceInteraction
{
    u64 handle;
    u32 observed_server_sequence;
    u32 observed_warp_sequence;
    s16 x;
    s16 y;
};

/* Only partition identity is retained; coordinates are not part of the key. */
struct CoopPresencePartition
{
    u32 session_epoch;
    u8 region;
    u16 map_group;
    u16 map_number;
    u32 warp_sequence;
};

struct CoopPresenceLocalContext
{
    u32 session_epoch;
    struct WorldLocation location;
    u8 elevation;
    u8 direction;
    u32 warp_sequence;
};

struct CoopPresenceRemote
{
    u64 handle;
    u32 server_sequence;
    struct CoopPresenceLocalState state;
    struct CoopPresenceUsername username;
};

struct CoopPresenceReducer
{
    bool8 context_valid;
    struct CoopPresencePartition partition;
    bool8 remote_active;
    struct CoopPresenceRemote remote;
};

enum CoopPresenceApplyResult
{
    COOP_PRESENCE_APPLY_REJECTED = 0,
    COOP_PRESENCE_APPLY_APPLIED = 1,
    COOP_PRESENCE_APPLY_STALE = 2,
    COOP_PRESENCE_APPLY_PARTITION_MISMATCH = 3,
    COOP_PRESENCE_APPLY_CAPACITY = 4,
    COOP_PRESENCE_APPLY_NOT_ACTIVE = 5,
    COOP_PRESENCE_APPLY_HANDLE_MISMATCH = 6,
};

bool8 CoopPresence_DecodePose(const u8 *bytes, u32 length,
                              struct CoopPresencePose *out);
bool8 CoopPresence_EncodePose(const struct CoopPresencePose *value,
                              u8 *bytes, u32 length);
bool8 CoopPresence_DecodeLocalState(const u8 *bytes, u32 length,
                                     struct CoopPresenceLocalState *out);
bool8 CoopPresence_EncodeLocalState(const struct CoopPresenceLocalState *value,
                                    u8 *bytes, u32 length);
bool8 CoopPresence_DecodeSpawn(const u8 *bytes, u32 length,
                               struct CoopPresenceSpawn *out);
bool8 CoopPresence_EncodeSpawn(const struct CoopPresenceSpawn *value,
                               u8 *bytes, u32 length);
bool8 CoopPresence_DecodeUpdate(const u8 *bytes, u32 length,
                                struct CoopPresenceUpdate *out);
bool8 CoopPresence_EncodeUpdate(const struct CoopPresenceUpdate *value,
                                u8 *bytes, u32 length);
bool8 CoopPresence_DecodeDespawn(const u8 *bytes, u32 length,
                                 struct CoopPresenceDespawn *out);
bool8 CoopPresence_EncodeDespawn(const struct CoopPresenceDespawn *value,
                                 u8 *bytes, u32 length);
bool8 CoopPresence_DecodeInteraction(const u8 *bytes, u32 length,
                                     struct CoopPresenceInteraction *out);

bool8 CoopPresence_SequenceIsNewer(u32 candidate, u32 reference);
u32 CoopPresence_NextSequence(u32 current);

void CoopPresenceReducer_Init(struct CoopPresenceReducer *reducer);
void CoopPresenceReducer_Reset(struct CoopPresenceReducer *reducer);
bool8 CoopPresenceReducer_Synchronize(struct CoopPresenceReducer *reducer,
                                      u32 session_epoch,
                                      const struct WorldLocation *location,
                                      u32 warp_sequence);
bool8 CoopPresenceReducer_IsActive(const struct CoopPresenceReducer *reducer);
bool8 CoopPresenceReducer_IsVisible(const struct CoopPresenceReducer *reducer);
const struct CoopPresenceRemote *CoopPresenceReducer_GetRemote(
    const struct CoopPresenceReducer *reducer);
enum CoopPresenceApplyResult CoopPresenceReducer_ApplySpawn(
    struct CoopPresenceReducer *reducer,
    const struct CoopPresenceSpawn *spawn);
enum CoopPresenceApplyResult CoopPresenceReducer_ApplyUpdate(
    struct CoopPresenceReducer *reducer,
    const struct CoopPresenceUpdate *update);
enum CoopPresenceApplyResult CoopPresenceReducer_ApplyDespawn(
    struct CoopPresenceReducer *reducer,
    const struct CoopPresenceDespawn *despawn);

bool8 CoopPresence_EncodeInteraction(
    const struct CoopPresenceReducer *reducer,
    const struct CoopPresenceLocalContext *local,
    u8 *bytes, u32 length);

#endif /* GUARD_COOP_PRESENCE_H */
