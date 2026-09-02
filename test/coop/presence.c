#include "global.h"
#include "coop/presence.h"
#include "constants/map_groups.h"
#include "test/test.h"

_Static_assert(COOP_PRESENCE_WORLD_LOCATION_REGION_OFFSET == 0, "location region offset");
_Static_assert(COOP_PRESENCE_WORLD_LOCATION_MAP_GROUP_OFFSET == 1, "location map group offset");
_Static_assert(COOP_PRESENCE_WORLD_LOCATION_MAP_NUMBER_OFFSET == 3, "location map number offset");
_Static_assert(COOP_PRESENCE_WORLD_LOCATION_X_OFFSET == 5, "location x offset");
_Static_assert(COOP_PRESENCE_WORLD_LOCATION_Y_OFFSET == 7, "location y offset");
_Static_assert(COOP_PRESENCE_WORLD_LOCATION_RESERVED_OFFSET == 9, "location reserved offset");
_Static_assert(COOP_PRESENCE_POSE_ELEVATION_OFFSET == 10, "pose elevation offset");
_Static_assert(COOP_PRESENCE_POSE_DIRECTION_OFFSET == 11, "pose direction offset");
_Static_assert(COOP_PRESENCE_POSE_CLIENT_TICK_OFFSET == 12, "pose tick offset");
_Static_assert(COOP_PRESENCE_POSE_WARP_SEQUENCE_OFFSET == 16, "pose warp offset");
_Static_assert(COOP_PRESENCE_POSE_MOVEMENT_MODE_OFFSET == 20, "pose movement offset");
_Static_assert(COOP_PRESENCE_POSE_ANIMATION_ID_OFFSET == 21, "pose animation offset");
_Static_assert(COOP_PRESENCE_POSE_AVATAR_ID_OFFSET == 22, "pose avatar offset");
_Static_assert(COOP_PRESENCE_POSE_PLAYER_STATE_OFFSET == 23, "pose state offset");
_Static_assert(COOP_PRESENCE_POSE_LOCATION_OFFSET == 0, "pose location offset");
_Static_assert(COOP_PRESENCE_LOCAL_STATE_SOURCE_SEQUENCE_OFFSET == 24, "state sequence offset");
_Static_assert(COOP_PRESENCE_LOCAL_STATE_POSE_OFFSET == 0, "state pose offset");
_Static_assert(COOP_PRESENCE_SPAWN_USERNAME_OFFSET == 40, "spawn username offset");
_Static_assert(COOP_PRESENCE_SPAWN_HANDLE_OFFSET == 0, "spawn handle offset");
_Static_assert(COOP_PRESENCE_SPAWN_SERVER_SEQUENCE_OFFSET == 8, "spawn server sequence offset");
_Static_assert(COOP_PRESENCE_SPAWN_STATE_OFFSET == 12, "spawn state offset");
_Static_assert(COOP_PRESENCE_SPAWN_USERNAME_SIZE == 32, "spawn username size");
_Static_assert(COOP_PRESENCE_UPDATE_HANDLE_OFFSET == 0, "update handle offset");
_Static_assert(COOP_PRESENCE_UPDATE_SERVER_SEQUENCE_OFFSET == 8, "update server sequence offset");
_Static_assert(COOP_PRESENCE_UPDATE_STATE_OFFSET == 12, "update state offset");
_Static_assert(COOP_PRESENCE_DESPAWN_HANDLE_OFFSET == 0, "despawn handle offset");
_Static_assert(COOP_PRESENCE_DESPAWN_SERVER_SEQUENCE_OFFSET == 8, "despawn server sequence offset");
_Static_assert(COOP_PRESENCE_DESPAWN_REASON_OFFSET == 12, "despawn reason offset");
_Static_assert(COOP_PRESENCE_DESPAWN_RESERVED_OFFSET == 13, "despawn reserved offset");
_Static_assert(COOP_PRESENCE_INTERACTION_X_OFFSET == 16, "interaction x offset");
_Static_assert(COOP_PRESENCE_INTERACTION_Y_OFFSET == 18, "interaction y offset");
_Static_assert(COOP_PRESENCE_INTERACTION_HANDLE_OFFSET == 0, "interaction handle offset");
_Static_assert(COOP_PRESENCE_INTERACTION_SERVER_SEQUENCE_OFFSET == 8, "interaction server sequence offset");
_Static_assert(COOP_PRESENCE_INTERACTION_WARP_SEQUENCE_OFFSET == 12, "interaction warp offset");
_Static_assert(COOP_PRESENCE_WORLD_LOCATION_SIZE == 10, "location size");
_Static_assert(COOP_PRESENCE_POSE_SIZE == 24, "pose size");
_Static_assert(COOP_PRESENCE_LOCAL_STATE_SIZE == 28, "local state size");
_Static_assert(COOP_PRESENCE_SPAWN_SIZE == 72, "spawn size");
_Static_assert(COOP_PRESENCE_UPDATE_SIZE == 40, "update size");
_Static_assert(COOP_PRESENCE_DESPAWN_SIZE == 16, "despawn size");
_Static_assert(COOP_PRESENCE_INTERACTION_SIZE == 20, "interaction size");
_Static_assert(COOP_PRESENCE_USERNAME_MAX == 32, "username max");
_Static_assert(COOP_PRESENCE_WORLD_LOCATION_V1_SIZE == 10, "location V1 size");
_Static_assert(COOP_PRESENCE_POSE_V1_SIZE == 24, "pose V1 size");
_Static_assert(COOP_PRESENCE_LOCAL_STATE_V1_SIZE == 28, "local state V1 size");
_Static_assert(COOP_PRESENCE_SPAWN_V1_SIZE == 72, "spawn V1 size");
_Static_assert(COOP_PRESENCE_UPDATE_V1_SIZE == 40, "update V1 size");
_Static_assert(COOP_PRESENCE_DESPAWN_V1_SIZE == 16, "despawn V1 size");
_Static_assert(COOP_PRESENCE_INTERACTION_V1_SIZE == 20, "interaction V1 size");
_Static_assert(COOP_PRESENCE_DIRECTION_SOUTH == 1, "south ordinal");
_Static_assert(COOP_PRESENCE_DIRECTION_NORTH == 2, "north ordinal");
_Static_assert(COOP_PRESENCE_DIRECTION_WEST == 3, "west ordinal");
_Static_assert(COOP_PRESENCE_DIRECTION_EAST == 4, "east ordinal");
_Static_assert(COOP_PRESENCE_MOVEMENT_IDLE == 0, "idle movement ordinal");
_Static_assert(COOP_PRESENCE_MOVEMENT_WALK == 1, "walk movement ordinal");
_Static_assert(COOP_PRESENCE_MOVEMENT_RUN == 2, "run movement ordinal");
_Static_assert(COOP_PRESENCE_ANIMATION_IDLE == 0, "idle animation ordinal");
_Static_assert(COOP_PRESENCE_ANIMATION_LOCOMOTION == 1, "locomotion animation ordinal");
_Static_assert(COOP_PRESENCE_AVATAR_BRENDAN == 1, "brendan avatar ordinal");
_Static_assert(COOP_PRESENCE_AVATAR_MAY == 2, "may avatar ordinal");
_Static_assert(COOP_PRESENCE_PLAYER_HIDDEN == 0, "hidden player ordinal");
_Static_assert(COOP_PRESENCE_PLAYER_OVERWORLD == 1, "overworld player ordinal");
_Static_assert(COOP_PRESENCE_DESPAWN_HIDDEN == 1, "hidden despawn ordinal");
_Static_assert(COOP_PRESENCE_DESPAWN_STALE == 2, "stale despawn ordinal");
_Static_assert(COOP_PRESENCE_DESPAWN_DISCONNECTED == 3, "disconnected despawn ordinal");
_Static_assert(COOP_PRESENCE_DESPAWN_LEASE_INVALID == 4, "lease invalid despawn ordinal");
_Static_assert(COOP_PRESENCE_DESPAWN_REPLACED == 5, "replaced despawn ordinal");
_Static_assert(COOP_PRESENCE_DESPAWN_PARTITION_LEFT == 6, "partition left despawn ordinal");

static struct WorldLocation Location(s16 x, s16 y)
{
    struct WorldLocation location = {
        .region = COOP_REGION_HOENN,
        .reserved = 0,
        .map_group = 1,
        .map_number = 3,
        .x = x,
        .y = y,
    };
    return location;
}

static struct CoopPresencePose Pose(s16 x, s16 y, u8 direction, u8 playerState)
{
    struct CoopPresencePose pose = {
        .location = Location(x, y),
        .elevation = 7,
        .direction = direction,
        .client_tick = 0x11223344,
        .warp_sequence = 0x55667788,
        .movement_mode = COOP_PRESENCE_MOVEMENT_RUN,
        .animation_id = COOP_PRESENCE_ANIMATION_LOCOMOTION,
        .avatar_id = COOP_PRESENCE_AVATAR_MAY,
        .player_state = playerState,
    };
    return pose;
}

static struct CoopPresenceLocalState State(s16 x, s16 y, u8 direction, u8 playerState)
{
    struct CoopPresenceLocalState state = {
        .pose = Pose(x, y, direction, playerState),
        .source_sequence = 0x99aabbcc,
    };
    return state;
}

static struct CoopPresenceUsername Username(const char *value)
{
    struct CoopPresenceUsername username = {0};
    u8 i;

    for (i = 0; value[i] != '\0' && i < COOP_PRESENCE_USERNAME_MAX; i++)
        username.bytes[i] = value[i];
    username.length = i;
    username.bytes[i] = '\0';
    return username;
}

static struct CoopPresenceSpawn Spawn(u64 handle, u32 serverSequence,
                                      s16 x, s16 y, u8 direction, u8 playerState)
{
    struct CoopPresenceSpawn spawn = {
        .handle = handle,
        .server_sequence = serverSequence,
        .state = State(x, y, direction, playerState),
        .username = Username("ash-kanto"),
    };
    return spawn;
}

static bool8 BytesEqual(const u8 *left, const u8 *right, u32 length)
{
    u32 i;

    for (i = 0; i < length; i++)
    {
        if (left[i] != right[i])
            return FALSE;
    }
    return TRUE;
}

static void CopyTestBytes(u8 *destination, const u8 *source, u32 length)
{
    u32 i;

    for (i = 0; i < length; i++)
        destination[i] = source[i];
}

static void FillTestBytes(u8 *bytes, u8 value, u32 length)
{
    u32 i;

    for (i = 0; i < length; i++)
        bytes[i] = value;
}

static bool8 BytesAre(const u8 *bytes, u8 value, u32 length)
{
    u32 i;

    for (i = 0; i < length; i++)
    {
        if (bytes[i] != value)
            return FALSE;
    }
    return TRUE;
}

static void ExpectCanary(const u8 *bytes, u8 value, u32 length)
{
    EXPECT(BytesAre(bytes, value, length));
}

static void ExpectStructBytes(const void *value, const u8 *snapshot, u32 length)
{
    EXPECT(BytesEqual((const u8 *)value, snapshot, length));
}

static void ExpectLocationEqual(const struct WorldLocation *actual,
                                const struct WorldLocation *expected)
{
    EXPECT_EQ(actual->region, expected->region);
    EXPECT_EQ(actual->reserved, expected->reserved);
    EXPECT_EQ(actual->map_group, expected->map_group);
    EXPECT_EQ(actual->map_number, expected->map_number);
    EXPECT_EQ(actual->x, expected->x);
    EXPECT_EQ(actual->y, expected->y);
}

static void ExpectPoseEqual(const struct CoopPresencePose *actual,
                            const struct CoopPresencePose *expected)
{
    ExpectLocationEqual(&actual->location, &expected->location);
    EXPECT_EQ(actual->elevation, expected->elevation);
    EXPECT_EQ(actual->direction, expected->direction);
    EXPECT_EQ(actual->client_tick, expected->client_tick);
    EXPECT_EQ(actual->warp_sequence, expected->warp_sequence);
    EXPECT_EQ(actual->movement_mode, expected->movement_mode);
    EXPECT_EQ(actual->animation_id, expected->animation_id);
    EXPECT_EQ(actual->avatar_id, expected->avatar_id);
    EXPECT_EQ(actual->player_state, expected->player_state);
}

static void ExpectLocalStateEqual(const struct CoopPresenceLocalState *actual,
                                  const struct CoopPresenceLocalState *expected)
{
    ExpectPoseEqual(&actual->pose, &expected->pose);
    EXPECT_EQ(actual->source_sequence, expected->source_sequence);
}

static void ExpectSpawnEqual(const struct CoopPresenceSpawn *actual,
                             const struct CoopPresenceSpawn *expected)
{
    u8 i;

    EXPECT_EQ(actual->handle, expected->handle);
    EXPECT_EQ(actual->server_sequence, expected->server_sequence);
    ExpectLocalStateEqual(&actual->state, &expected->state);
    EXPECT_EQ(actual->username.length, expected->username.length);
    for (i = 0; i < expected->username.length; i++)
        EXPECT_EQ(actual->username.bytes[i], expected->username.bytes[i]);
    EXPECT_EQ(actual->username.bytes[expected->username.length], '\0');
}

static void ExpectLocalDecodeRejected(const u8 *bytes, u32 length,
                                      struct CoopPresenceLocalState *out,
                                      const u8 *snapshot)
{
    EXPECT(!CoopPresence_DecodeLocalState(bytes, length, out));
    ExpectStructBytes(out, snapshot, sizeof(*out));
}

static void ExpectPoseDecodeRejected(const u8 *bytes, u32 length,
                                     struct CoopPresencePose *out,
                                     const u8 *snapshot)
{
    EXPECT(!CoopPresence_DecodePose(bytes, length, out));
    ExpectStructBytes(out, snapshot, sizeof(*out));
}

static void RejectLocalByte(const u8 *valid, u32 offset, u8 invalid,
                            struct CoopPresenceLocalState *out,
                            const u8 *snapshot)
{
    u8 bytes[COOP_PRESENCE_LOCAL_STATE_SIZE];

    CopyTestBytes(bytes, valid, sizeof(bytes));
    bytes[offset] = invalid;
    ExpectLocalDecodeRejected(bytes, sizeof(bytes), out, snapshot);
}

static void RejectLocalWord(const u8 *valid, u32 offset,
                            struct CoopPresenceLocalState *out,
                            const u8 *snapshot)
{
    u8 bytes[COOP_PRESENCE_LOCAL_STATE_SIZE];
    u8 i;

    CopyTestBytes(bytes, valid, sizeof(bytes));
    for (i = 0; i < sizeof(u32); i++)
        bytes[offset + i] = 0;
    ExpectLocalDecodeRejected(bytes, sizeof(bytes), out, snapshot);
}

static void RejectPoseWord(const u8 *valid, u32 offset,
                           struct CoopPresencePose *out, const u8 *snapshot)
{
    u8 bytes[COOP_PRESENCE_POSE_SIZE];
    u8 i;

    CopyTestBytes(bytes, valid, sizeof(bytes));
    for (i = 0; i < sizeof(u32); i++)
        bytes[offset + i] = 0;
    ExpectPoseDecodeRejected(bytes, sizeof(bytes), out, snapshot);
}

static void RejectSpawnField(const u8 *valid, u32 offset, u8 width,
                             struct CoopPresenceSpawn *out,
                             const u8 *snapshot)
{
    u8 bytes[COOP_PRESENCE_SPAWN_SIZE];
    u8 i;

    CopyTestBytes(bytes, valid, sizeof(bytes));
    for (i = 0; i < width; i++)
        bytes[offset + i] = 0;
    EXPECT(!CoopPresence_DecodeSpawn(bytes, sizeof(bytes), out));
    ExpectStructBytes(out, snapshot, sizeof(*out));
}

static void RejectUpdateField(const u8 *valid, u32 offset, u8 width,
                              struct CoopPresenceUpdate *out,
                              const u8 *snapshot)
{
    u8 bytes[COOP_PRESENCE_UPDATE_SIZE];
    u8 i;

    CopyTestBytes(bytes, valid, sizeof(bytes));
    for (i = 0; i < width; i++)
        bytes[offset + i] = 0;
    EXPECT(!CoopPresence_DecodeUpdate(bytes, sizeof(bytes), out));
    ExpectStructBytes(out, snapshot, sizeof(*out));
}

static void RejectDespawnField(const u8 *valid, u32 offset, u8 width,
                               struct CoopPresenceDespawn *out,
                               const u8 *snapshot)
{
    u8 bytes[COOP_PRESENCE_DESPAWN_SIZE];
    u8 i;

    CopyTestBytes(bytes, valid, sizeof(bytes));
    for (i = 0; i < width; i++)
        bytes[offset + i] = 0;
    EXPECT(!CoopPresence_DecodeDespawn(bytes, sizeof(bytes), out));
    ExpectStructBytes(out, snapshot, sizeof(*out));
}

static void RejectInteractionField(const u8 *valid, u32 offset, u8 width,
                                   struct CoopPresenceInteraction *out,
                                   const u8 *snapshot)
{
    u8 bytes[COOP_PRESENCE_INTERACTION_SIZE];
    u8 i;

    CopyTestBytes(bytes, valid, sizeof(bytes));
    for (i = 0; i < width; i++)
        bytes[offset + i] = 0;
    EXPECT(!CoopPresence_DecodeInteraction(bytes, sizeof(bytes), out));
    ExpectStructBytes(out, snapshot, sizeof(*out));
}

static void RejectPoseByte(const u8 *valid, u32 offset, u8 invalid,
                           struct CoopPresencePose *out, const u8 *snapshot)
{
    u8 bytes[COOP_PRESENCE_POSE_SIZE];

    CopyTestBytes(bytes, valid, sizeof(bytes));
    bytes[offset] = invalid;
    ExpectPoseDecodeRejected(bytes, sizeof(bytes), out, snapshot);
}

static void RejectSpawnStateByte(const u8 *valid, u32 offset, u8 invalid,
                                 struct CoopPresenceSpawn *out,
                                 const u8 *snapshot)
{
    u8 bytes[COOP_PRESENCE_SPAWN_SIZE];

    CopyTestBytes(bytes, valid, sizeof(bytes));
    bytes[COOP_PRESENCE_SPAWN_STATE_OFFSET + offset] = invalid;
    EXPECT(!CoopPresence_DecodeSpawn(bytes, sizeof(bytes), out));
    ExpectStructBytes(out, snapshot, sizeof(*out));
}

static void RejectUpdateStateByte(const u8 *valid, u32 offset, u8 invalid,
                                  struct CoopPresenceUpdate *out,
                                  const u8 *snapshot)
{
    u8 bytes[COOP_PRESENCE_UPDATE_SIZE];

    CopyTestBytes(bytes, valid, sizeof(bytes));
    bytes[COOP_PRESENCE_UPDATE_STATE_OFFSET + offset] = invalid;
    EXPECT(!CoopPresence_DecodeUpdate(bytes, sizeof(bytes), out));
    ExpectStructBytes(out, snapshot, sizeof(*out));
}

static void ExpectInteractionRejected(const struct CoopPresenceReducer *reducer,
                                      const struct CoopPresenceLocalContext *local,
                                      u8 *bytes, u8 canary)
{
    FillTestBytes(bytes, canary, COOP_PRESENCE_INTERACTION_SIZE);
    EXPECT(!CoopPresence_EncodeInteraction(reducer, local, bytes,
                                            COOP_PRESENCE_INTERACTION_SIZE));
    ExpectCanary(bytes, canary, COOP_PRESENCE_INTERACTION_SIZE);
}

static void EstablishReducer(struct CoopPresenceReducer *reducer)
{
    struct WorldLocation location = Location(10, 10);

    CoopPresenceReducer_Init(reducer);
    EXPECT(CoopPresenceReducer_Synchronize(reducer, 9, &location, 1));
}

static bool8 EncodeInteractionFixture(const struct CoopPresenceInteraction *value,
                                      u8 *bytes, u32 length)
{
    struct CoopPresenceReducer reducer;
    struct CoopPresenceSpawn spawn = Spawn(value->handle,
                                           value->observed_server_sequence,
                                           value->x, value->y,
                                           COOP_PRESENCE_DIRECTION_NORTH,
                                           COOP_PRESENCE_PLAYER_OVERWORLD);
    struct CoopPresenceLocalContext local = {
        .session_epoch = 9,
        .location = Location(value->x, value->y - 1),
        .elevation = 7,
        .direction = COOP_PRESENCE_DIRECTION_SOUTH,
        .warp_sequence = value->observed_warp_sequence,
    };

    spawn.state.pose.warp_sequence = value->observed_warp_sequence;
    CoopPresenceReducer_Init(&reducer);
    if (!CoopPresenceReducer_Synchronize(&reducer, local.session_epoch,
                                         &local.location, local.warp_sequence))
        return FALSE;
    if (CoopPresenceReducer_ApplySpawn(&reducer, &spawn)
        != COOP_PRESENCE_APPLY_APPLIED)
        return FALSE;
    return CoopPresence_EncodeInteraction(&reducer, &local, bytes, length);
}

static void ExpectReducerClearedAt(const struct CoopPresenceReducer *reducer,
                                   u32 sessionEpoch,
                                   const struct WorldLocation *location,
                                   u32 warpSequence)
{
    u32 i;

    EXPECT(!CoopPresenceReducer_IsActive(reducer));
    EXPECT(CoopPresenceReducer_GetRemote(reducer) == NULL);
    EXPECT(reducer->context_valid);
    EXPECT_EQ(reducer->partition.session_epoch, sessionEpoch);
    EXPECT_EQ(reducer->partition.region, location->region);
    EXPECT_EQ(reducer->partition.map_group, location->map_group);
    EXPECT_EQ(reducer->partition.map_number, location->map_number);
    EXPECT_EQ(reducer->partition.warp_sequence, warpSequence);
    EXPECT_EQ(reducer->remote.handle, 0);
    EXPECT_EQ(reducer->remote.server_sequence, 0);
    EXPECT_EQ(reducer->remote.state.pose.location.region, 0);
    EXPECT_EQ(reducer->remote.state.pose.location.reserved, 0);
    EXPECT_EQ(reducer->remote.state.pose.location.map_group, 0);
    EXPECT_EQ(reducer->remote.state.pose.location.map_number, 0);
    EXPECT_EQ(reducer->remote.state.pose.location.x, 0);
    EXPECT_EQ(reducer->remote.state.pose.location.y, 0);
    EXPECT_EQ(reducer->remote.state.pose.elevation, 0);
    EXPECT_EQ(reducer->remote.state.pose.direction, 0);
    EXPECT_EQ(reducer->remote.state.pose.client_tick, 0);
    EXPECT_EQ(reducer->remote.state.pose.warp_sequence, 0);
    EXPECT_EQ(reducer->remote.state.pose.movement_mode, 0);
    EXPECT_EQ(reducer->remote.state.pose.animation_id, 0);
    EXPECT_EQ(reducer->remote.state.pose.avatar_id, 0);
    EXPECT_EQ(reducer->remote.state.pose.player_state, 0);
    EXPECT_EQ(reducer->remote.state.source_sequence, 0);
    EXPECT_EQ(reducer->remote.username.length, 0);
    for (i = 0; i <= COOP_PRESENCE_USERNAME_MAX; i++)
        EXPECT_EQ(reducer->remote.username.bytes[i], 0);
}

static void ExpectReducerResetState(const struct CoopPresenceReducer *reducer)
{
    u32 i;

    EXPECT(!reducer->context_valid);
    EXPECT(!reducer->remote_active);
    EXPECT(!CoopPresenceReducer_IsActive(reducer));
    EXPECT(CoopPresenceReducer_GetRemote(reducer) == NULL);
    EXPECT_EQ(reducer->partition.session_epoch, 0);
    EXPECT_EQ(reducer->partition.region, 0);
    EXPECT_EQ(reducer->partition.map_group, 0);
    EXPECT_EQ(reducer->partition.map_number, 0);
    EXPECT_EQ(reducer->partition.warp_sequence, 0);
    EXPECT_EQ(reducer->remote.handle, 0);
    EXPECT_EQ(reducer->remote.server_sequence, 0);
    EXPECT_EQ(reducer->remote.state.pose.location.region, 0);
    EXPECT_EQ(reducer->remote.state.pose.location.reserved, 0);
    EXPECT_EQ(reducer->remote.state.pose.location.map_group, 0);
    EXPECT_EQ(reducer->remote.state.pose.location.map_number, 0);
    EXPECT_EQ(reducer->remote.state.pose.location.x, 0);
    EXPECT_EQ(reducer->remote.state.pose.location.y, 0);
    EXPECT_EQ(reducer->remote.state.pose.elevation, 0);
    EXPECT_EQ(reducer->remote.state.pose.direction, 0);
    EXPECT_EQ(reducer->remote.state.pose.client_tick, 0);
    EXPECT_EQ(reducer->remote.state.pose.warp_sequence, 0);
    EXPECT_EQ(reducer->remote.state.pose.movement_mode, 0);
    EXPECT_EQ(reducer->remote.state.pose.animation_id, 0);
    EXPECT_EQ(reducer->remote.state.pose.avatar_id, 0);
    EXPECT_EQ(reducer->remote.state.pose.player_state, 0);
    EXPECT_EQ(reducer->remote.state.source_sequence, 0);
    EXPECT_EQ(reducer->remote.username.length, 0);
    for (i = 0; i <= COOP_PRESENCE_USERNAME_MAX; i++)
        EXPECT_EQ(reducer->remote.username.bytes[i], 0);
}

TEST("Cloud Coop presence wire sizes and golden bytes")
{
    static const u8 localBytes[COOP_PRESENCE_LOCAL_STATE_SIZE] = {
        1, 1, 0, 3, 0, 252, 255, 9, 0, 0, 7, 4, 68, 51, 34, 17,
        136, 119, 102, 85, 2, 1, 2, 1, 204, 187, 170, 153,
    };
    static const u8 spawnPrefix[12] = {
        239, 205, 171, 137, 103, 69, 35, 1, 3, 0, 0, 0,
    };
    static const u8 updateBytes[COOP_PRESENCE_UPDATE_SIZE] = {
        239, 205, 171, 137, 103, 69, 35, 1, 3, 0, 0, 0,
        1, 1, 0, 3, 0, 252, 255, 9, 0, 0, 7, 4, 68, 51, 34, 17,
        136, 119, 102, 85, 2, 1, 2, 1, 204, 187, 170, 153,
    };
    static const u8 interactionBytes[COOP_PRESENCE_INTERACTION_SIZE] = {
        239, 205, 171, 137, 103, 69, 35, 1, 4, 0, 0, 0,
        2, 0, 0, 0, 252, 255, 9, 0,
    };
    struct CoopPresenceLocalState state = State(-4, 9,
                                                 COOP_PRESENCE_DIRECTION_EAST,
                                                 COOP_PRESENCE_PLAYER_OVERWORLD);
    struct CoopPresenceSpawn spawn = Spawn(0x0123456789abcdefULL, 3,
                                           -4, 9,
                                           COOP_PRESENCE_DIRECTION_EAST,
                                           COOP_PRESENCE_PLAYER_OVERWORLD);
    struct CoopPresenceUpdate update = {
        .handle = 0x0123456789abcdefULL,
        .server_sequence = 3,
        .state = state,
    };
    struct CoopPresenceLocalState decodedState = State(1234, -1234,
                                                        COOP_PRESENCE_DIRECTION_NORTH,
                                                        COOP_PRESENCE_PLAYER_HIDDEN);
    struct CoopPresenceSpawn decodedSpawn = Spawn(2, 4, 1234, -1234,
                                                  COOP_PRESENCE_DIRECTION_NORTH,
                                                  COOP_PRESENCE_PLAYER_HIDDEN);
    struct CoopPresenceUpdate decodedUpdate;
    struct CoopPresenceInteraction interaction = {
        .handle = 0x0123456789abcdefULL,
        .observed_server_sequence = 4,
        .observed_warp_sequence = 2,
        .x = -4,
        .y = 9,
    };
    u8 bytes[COOP_PRESENCE_SPAWN_SIZE];

    EXPECT_EQ(COOP_PRESENCE_WORLD_LOCATION_SIZE, 10);
    EXPECT_EQ(COOP_PRESENCE_POSE_SIZE, 24);
    EXPECT_EQ(COOP_PRESENCE_LOCAL_STATE_SIZE, 28);
    EXPECT_EQ(COOP_PRESENCE_SPAWN_SIZE, 72);
    EXPECT_EQ(COOP_PRESENCE_UPDATE_SIZE, 40);
    EXPECT_EQ(COOP_PRESENCE_DESPAWN_SIZE, 16);
    EXPECT_EQ(COOP_PRESENCE_INTERACTION_SIZE, 20);

    EXPECT(CoopPresence_EncodeLocalState(&state, bytes, COOP_PRESENCE_LOCAL_STATE_SIZE));
    EXPECT(BytesEqual(bytes, localBytes, COOP_PRESENCE_LOCAL_STATE_SIZE));
    EXPECT(CoopPresence_DecodeLocalState(bytes, COOP_PRESENCE_LOCAL_STATE_SIZE,
                                         &decodedState));
    ExpectLocalStateEqual(&decodedState, &state);

    EXPECT(CoopPresence_EncodeSpawn(&spawn, bytes, COOP_PRESENCE_SPAWN_SIZE));
    EXPECT(BytesEqual(bytes, spawnPrefix, sizeof(spawnPrefix)));
    EXPECT(BytesEqual(&bytes[COOP_PRESENCE_SPAWN_STATE_OFFSET], localBytes,
                      COOP_PRESENCE_LOCAL_STATE_SIZE));
    EXPECT(BytesEqual(&bytes[COOP_PRESENCE_SPAWN_USERNAME_OFFSET],
                      (const u8 *)"ash-kanto", 9));
    ExpectCanary(&bytes[COOP_PRESENCE_SPAWN_USERNAME_OFFSET + 9], 0,
                 COOP_PRESENCE_USERNAME_MAX - 9);
    EXPECT(CoopPresence_DecodeSpawn(bytes, COOP_PRESENCE_SPAWN_SIZE, &decodedSpawn));
    ExpectSpawnEqual(&decodedSpawn, &spawn);

    EXPECT(CoopPresence_EncodeUpdate(&update, bytes, COOP_PRESENCE_UPDATE_SIZE));
    EXPECT(BytesEqual(bytes, updateBytes, COOP_PRESENCE_UPDATE_SIZE));
    EXPECT(CoopPresence_DecodeUpdate(bytes, COOP_PRESENCE_UPDATE_SIZE, &decodedUpdate));
    EXPECT_EQ(decodedUpdate.handle, update.handle);
    EXPECT_EQ(decodedUpdate.server_sequence, update.server_sequence);
    EXPECT(BytesEqual((const u8 *)&decodedUpdate.state, (const u8 *)&update.state,
                      sizeof(update.state)));

    EXPECT(EncodeInteractionFixture(&interaction, bytes,
                                    COOP_PRESENCE_INTERACTION_SIZE));
    EXPECT(BytesEqual(bytes, interactionBytes, COOP_PRESENCE_INTERACTION_SIZE));
    EXPECT(CoopPresence_DecodeInteraction(bytes, COOP_PRESENCE_INTERACTION_SIZE,
                                           &interaction));
}

TEST("Cloud Coop presence codecs translate little-endian signed and wide fields")
{
    struct CoopPresencePose pose = Pose(-32768, 32767,
                                        COOP_PRESENCE_DIRECTION_NORTH,
                                        COOP_PRESENCE_PLAYER_HIDDEN);
    struct CoopPresencePose decoded = Pose(1234, -1234,
                                           COOP_PRESENCE_DIRECTION_EAST,
                                           COOP_PRESENCE_PLAYER_OVERWORLD);
    struct CoopPresenceDespawn despawn = {
        .handle = 0xfedcba9876543210ULL,
        .server_sequence = 0xa1b2c3d4,
        .reason = COOP_PRESENCE_DESPAWN_PARTITION_LEFT,
    };
    struct CoopPresenceDespawn decodedDespawn;
    u8 poseBytes[COOP_PRESENCE_POSE_SIZE];
    u8 despawnBytes[COOP_PRESENCE_DESPAWN_SIZE];

    EXPECT(CoopPresence_EncodePose(&pose, poseBytes, sizeof(poseBytes)));
    EXPECT_EQ(poseBytes[COOP_PRESENCE_POSE_LOCATION_OFFSET], COOP_REGION_HOENN);
    EXPECT_EQ(poseBytes[COOP_PRESENCE_POSE_LOCATION_OFFSET + 1], 1);
    EXPECT_EQ(poseBytes[COOP_PRESENCE_POSE_LOCATION_OFFSET + 2], 0);
    EXPECT_EQ(poseBytes[COOP_PRESENCE_POSE_LOCATION_OFFSET + 3], 3);
    EXPECT_EQ(poseBytes[COOP_PRESENCE_POSE_LOCATION_OFFSET + 4], 0);
    EXPECT_EQ(poseBytes[COOP_PRESENCE_POSE_LOCATION_OFFSET + 5], 0);
    EXPECT_EQ(poseBytes[COOP_PRESENCE_POSE_LOCATION_OFFSET + 6], 128);
    EXPECT_EQ(poseBytes[COOP_PRESENCE_POSE_LOCATION_OFFSET + 7], 255);
    EXPECT_EQ(poseBytes[COOP_PRESENCE_POSE_LOCATION_OFFSET + 8], 127);
    EXPECT_EQ(poseBytes[COOP_PRESENCE_POSE_LOCATION_OFFSET + 9], 0);
    EXPECT_EQ(poseBytes[COOP_PRESENCE_POSE_CLIENT_TICK_OFFSET], 0x44);
    EXPECT_EQ(poseBytes[COOP_PRESENCE_POSE_CLIENT_TICK_OFFSET + 3], 0x11);
    EXPECT_EQ(poseBytes[COOP_PRESENCE_POSE_WARP_SEQUENCE_OFFSET], 0x88);
    EXPECT_EQ(poseBytes[COOP_PRESENCE_POSE_WARP_SEQUENCE_OFFSET + 3], 0x55);
    EXPECT(CoopPresence_DecodePose(poseBytes, sizeof(poseBytes), &decoded));
    ExpectPoseEqual(&decoded, &pose);

    EXPECT(CoopPresence_EncodeDespawn(&despawn, despawnBytes, sizeof(despawnBytes)));
    EXPECT_EQ(despawnBytes[0], 0x10);
    EXPECT_EQ(despawnBytes[7], 0xfe);
    EXPECT_EQ(despawnBytes[8], 0xd4);
    EXPECT_EQ(despawnBytes[11], 0xa1);
    EXPECT(CoopPresence_DecodeDespawn(despawnBytes, sizeof(despawnBytes), &decodedDespawn));
    EXPECT_EQ(decodedDespawn.handle, 0xfedcba9876543210ULL);
    EXPECT_EQ(decodedDespawn.server_sequence, 0xa1b2c3d4);
    EXPECT_EQ(decodedDespawn.reason, COOP_PRESENCE_DESPAWN_PARTITION_LEFT);
}

TEST("Cloud Coop presence accepts valid zero-valued pose fields")
{
    struct CoopPresencePose pose = Pose(0, 0, COOP_PRESENCE_DIRECTION_SOUTH,
                                        COOP_PRESENCE_PLAYER_OVERWORLD);
    struct CoopPresencePose decodedPose = Pose(1234, -1234,
                                               COOP_PRESENCE_DIRECTION_NORTH,
                                               COOP_PRESENCE_PLAYER_HIDDEN);
    struct CoopPresenceLocalState state;
    struct CoopPresenceLocalState decodedState = State(1234, -1234,
                                                       COOP_PRESENCE_DIRECTION_NORTH,
                                                       COOP_PRESENCE_PLAYER_HIDDEN);
    struct CoopPresenceSpawn spawn = Spawn(0x0123456789abcdefULL, 3, 0, 0,
                                           COOP_PRESENCE_DIRECTION_SOUTH,
                                           COOP_PRESENCE_PLAYER_OVERWORLD);
    struct CoopPresenceSpawn decodedSpawn = Spawn(2, 4, 1234, -1234,
                                                  COOP_PRESENCE_DIRECTION_NORTH,
                                                  COOP_PRESENCE_PLAYER_HIDDEN);
    struct CoopPresenceUpdate update = {
        .handle = 0x0123456789abcdefULL,
        .server_sequence = 3,
    };
    struct CoopPresenceUpdate decodedUpdate = {
        .handle = 2,
        .server_sequence = 4,
        .state = decodedState,
    };
    u8 bytes[COOP_PRESENCE_SPAWN_SIZE];

    pose.location.map_group = 0;
    pose.location.map_number = 0;
    pose.elevation = 0;
    pose.client_tick = 0;
    pose.warp_sequence = 1;
    pose.movement_mode = COOP_PRESENCE_MOVEMENT_IDLE;
    pose.animation_id = COOP_PRESENCE_ANIMATION_IDLE;
    pose.avatar_id = COOP_PRESENCE_AVATAR_BRENDAN;
    state.pose = pose;
    state.source_sequence = 1;
    spawn.state = state;
    spawn.username = Username("abc");
    update.state = state;

    EXPECT(CoopPresence_EncodePose(&pose, bytes, COOP_PRESENCE_POSE_SIZE));
    EXPECT(CoopPresence_DecodePose(bytes, COOP_PRESENCE_POSE_SIZE, &decodedPose));
    ExpectPoseEqual(&decodedPose, &pose);

    EXPECT(CoopPresence_EncodeLocalState(&state, bytes,
                                         COOP_PRESENCE_LOCAL_STATE_SIZE));
    EXPECT(CoopPresence_DecodeLocalState(bytes, COOP_PRESENCE_LOCAL_STATE_SIZE,
                                          &decodedState));
    ExpectLocalStateEqual(&decodedState, &state);

    EXPECT(CoopPresence_EncodeSpawn(&spawn, bytes, COOP_PRESENCE_SPAWN_SIZE));
    EXPECT(CoopPresence_DecodeSpawn(bytes, COOP_PRESENCE_SPAWN_SIZE,
                                    &decodedSpawn));
    ExpectSpawnEqual(&decodedSpawn, &spawn);
    EXPECT(BytesEqual((const u8 *)decodedSpawn.username.bytes,
                      (const u8 *)spawn.username.bytes,
                      sizeof(spawn.username.bytes)));

    EXPECT(CoopPresence_EncodeUpdate(&update, bytes, COOP_PRESENCE_UPDATE_SIZE));
    EXPECT(CoopPresence_DecodeUpdate(bytes, COOP_PRESENCE_UPDATE_SIZE,
                                     &decodedUpdate));
    EXPECT_EQ(decodedUpdate.handle, update.handle);
    EXPECT_EQ(decodedUpdate.server_sequence, update.server_sequence);
    ExpectLocalStateEqual(&decodedUpdate.state, &update.state);
}

TEST("Cloud Coop presence codecs reject malformed input without mutating outputs")
{
    struct CoopPresenceLocalState state = State(-4, 9,
                                                 COOP_PRESENCE_DIRECTION_EAST,
                                                 COOP_PRESENCE_PLAYER_OVERWORLD);
    struct CoopPresenceLocalState output = State(1234, -1234,
                                                 COOP_PRESENCE_DIRECTION_NORTH,
                                                 COOP_PRESENCE_PLAYER_HIDDEN);
    struct CoopPresenceSpawn spawn = Spawn(1, 3, -4, 9,
                                           COOP_PRESENCE_DIRECTION_EAST,
                                           COOP_PRESENCE_PLAYER_OVERWORLD);
    struct CoopPresenceSpawn spawnOutput = Spawn(2, 4, 2, 3,
                                                 COOP_PRESENCE_DIRECTION_NORTH,
                                                 COOP_PRESENCE_PLAYER_HIDDEN);
    struct CoopPresenceDespawn despawn = {
        .handle = 1,
        .server_sequence = 1,
        .reason = COOP_PRESENCE_DESPAWN_HIDDEN,
    };
    struct CoopPresenceDespawn despawnOutput = {
        .handle = 2,
        .server_sequence = 4,
        .reason = COOP_PRESENCE_DESPAWN_STALE,
    };
    struct CoopPresenceInteraction interaction = {
        .handle = 1,
        .observed_server_sequence = 1,
        .observed_warp_sequence = 1,
        .x = 1,
        .y = 2,
    };
    struct CoopPresenceInteraction interactionOutput = {
        .handle = 2,
        .observed_server_sequence = 4,
        .observed_warp_sequence = 2,
        .x = 3,
        .y = 4,
    };
    u8 bytes[COOP_PRESENCE_SPAWN_SIZE];
    u8 canary[COOP_PRESENCE_INTERACTION_SIZE];
    u8 encodeCanary[COOP_PRESENCE_SPAWN_SIZE];
    u8 outputSnapshot[sizeof(output)];
    u8 spawnSnapshot[sizeof(spawnOutput)];
    u8 despawnSnapshot[sizeof(despawnOutput)];
    u8 interactionSnapshot[sizeof(interactionOutput)];
    u32 i;

    CopyTestBytes(outputSnapshot, (const u8 *)&output, sizeof(output));
    EXPECT(!CoopPresence_DecodeLocalState(NULL, COOP_PRESENCE_LOCAL_STATE_SIZE, &output));
    EXPECT(BytesEqual(outputSnapshot, (const u8 *)&output, sizeof(output)));
    EXPECT(CoopPresence_EncodeLocalState(&state, bytes, COOP_PRESENCE_LOCAL_STATE_SIZE));
    bytes[COOP_PRESENCE_POSE_DIRECTION_OFFSET] = 0;
    EXPECT(!CoopPresence_DecodeLocalState(bytes, COOP_PRESENCE_LOCAL_STATE_SIZE, &output));
    EXPECT(BytesEqual(outputSnapshot, (const u8 *)&output, sizeof(output)));
    bytes[COOP_PRESENCE_POSE_DIRECTION_OFFSET] = COOP_PRESENCE_DIRECTION_EAST;
    for (i = 0; i < sizeof(u32); i++)
        bytes[COOP_PRESENCE_POSE_WARP_SEQUENCE_OFFSET + i] = 0;
    EXPECT(!CoopPresence_DecodeLocalState(bytes, COOP_PRESENCE_LOCAL_STATE_SIZE, &output));
    EXPECT(BytesEqual(outputSnapshot, (const u8 *)&output, sizeof(output)));

    EXPECT(CoopPresence_EncodeLocalState(&state, bytes, COOP_PRESENCE_LOCAL_STATE_SIZE));
    bytes[COOP_PRESENCE_WORLD_LOCATION_REGION_OFFSET] = COOP_REGION_KANTO;
    EXPECT(!CoopPresence_DecodeLocalState(bytes, COOP_PRESENCE_LOCAL_STATE_SIZE, &output));
    EXPECT(BytesEqual(outputSnapshot, (const u8 *)&output, sizeof(output)));
    bytes[COOP_PRESENCE_WORLD_LOCATION_REGION_OFFSET] = COOP_REGION_HOENN;
    bytes[COOP_PRESENCE_WORLD_LOCATION_MAP_GROUP_OFFSET] = MAP_GROUPS_COUNT;
    bytes[COOP_PRESENCE_WORLD_LOCATION_MAP_GROUP_OFFSET + 1] = 0;
    EXPECT(!CoopPresence_DecodeLocalState(bytes, COOP_PRESENCE_LOCAL_STATE_SIZE, &output));
    EXPECT(BytesEqual(outputSnapshot, (const u8 *)&output, sizeof(output)));

    CopyTestBytes(spawnSnapshot, (const u8 *)&spawnOutput, sizeof(spawnOutput));
    EXPECT(CoopPresence_EncodeSpawn(&spawn, bytes, COOP_PRESENCE_SPAWN_SIZE));
    bytes[COOP_PRESENCE_SPAWN_STATE_OFFSET + COOP_PRESENCE_POSE_LOCATION_OFFSET
          + COOP_PRESENCE_WORLD_LOCATION_RESERVED_OFFSET] = 1;
    EXPECT(!CoopPresence_DecodeSpawn(bytes, COOP_PRESENCE_SPAWN_SIZE, &spawnOutput));
    EXPECT(BytesEqual(spawnSnapshot, (const u8 *)&spawnOutput, sizeof(spawnOutput)));
    bytes[COOP_PRESENCE_SPAWN_STATE_OFFSET + COOP_PRESENCE_POSE_LOCATION_OFFSET
          + COOP_PRESENCE_WORLD_LOCATION_RESERVED_OFFSET] = 0;
    bytes[COOP_PRESENCE_SPAWN_USERNAME_OFFSET + 3] = 1;
    EXPECT(!CoopPresence_DecodeSpawn(bytes, COOP_PRESENCE_SPAWN_SIZE, &spawnOutput));
    EXPECT(BytesEqual(spawnSnapshot, (const u8 *)&spawnOutput, sizeof(spawnOutput)));

    CopyTestBytes(despawnSnapshot, (const u8 *)&despawnOutput, sizeof(despawnOutput));
    for (i = COOP_PRESENCE_DESPAWN_RESERVED_OFFSET;
         i < COOP_PRESENCE_DESPAWN_SIZE; i++)
    {
        EXPECT(CoopPresence_EncodeDespawn(&despawn, bytes,
                                          COOP_PRESENCE_DESPAWN_SIZE));
        bytes[i] = 1;
        EXPECT(!CoopPresence_DecodeDespawn(bytes, COOP_PRESENCE_DESPAWN_SIZE,
                                           &despawnOutput));
        EXPECT(BytesEqual(despawnSnapshot, (const u8 *)&despawnOutput,
                          sizeof(despawnOutput)));
    }
    EXPECT(CoopPresence_EncodeDespawn(&despawn, bytes, COOP_PRESENCE_DESPAWN_SIZE));
    bytes[COOP_PRESENCE_DESPAWN_REASON_OFFSET] = 0;
    EXPECT(!CoopPresence_DecodeDespawn(bytes, COOP_PRESENCE_DESPAWN_SIZE, &despawnOutput));
    EXPECT(BytesEqual(despawnSnapshot, (const u8 *)&despawnOutput, sizeof(despawnOutput)));

    interaction.handle = 1;
    CopyTestBytes(interactionSnapshot, (const u8 *)&interactionOutput,
                  sizeof(interactionOutput));
    EXPECT(EncodeInteractionFixture(&interaction, canary, sizeof(canary)));
    for (i = 0; i < sizeof(u64); i++)
        canary[COOP_PRESENCE_INTERACTION_HANDLE_OFFSET + i] = 0;
    EXPECT(!CoopPresence_DecodeInteraction(canary, sizeof(canary), &interactionOutput));
    EXPECT(BytesEqual(interactionSnapshot, (const u8 *)&interactionOutput,
                      sizeof(interactionOutput)));
    interaction.handle = 0;
    FillTestBytes(canary, 0xa5, sizeof(canary));
    EXPECT(!EncodeInteractionFixture(&interaction, canary, sizeof(canary)));
    ExpectCanary(canary, 0xa5, sizeof(canary));

    CopyTestBytes(outputSnapshot, (const u8 *)&output, sizeof(output));
    EXPECT(!CoopPresence_DecodeLocalState(bytes, COOP_PRESENCE_LOCAL_STATE_SIZE - 1, &output));
    EXPECT(BytesEqual(outputSnapshot, (const u8 *)&output, sizeof(output)));
    FillTestBytes(encodeCanary, 0xa5, sizeof(encodeCanary));
    EXPECT(!CoopPresence_EncodePose(NULL, encodeCanary, COOP_PRESENCE_POSE_SIZE));
    ExpectCanary(encodeCanary, 0xa5, sizeof(encodeCanary));
    EXPECT(!EncodeInteractionFixture(&interaction, encodeCanary,
                                     COOP_PRESENCE_INTERACTION_SIZE - 1));
    ExpectCanary(encodeCanary, 0xa5, sizeof(encodeCanary));
}

TEST("Cloud Coop presence usernames and ordinals are closed")
{
    struct CoopPresenceSpawn spawn = Spawn(1, 1, -4, 9,
                                           COOP_PRESENCE_DIRECTION_EAST,
                                           COOP_PRESENCE_PLAYER_OVERWORLD);
    u8 bytes[COOP_PRESENCE_SPAWN_SIZE];
    const char *invalid[] = {"ab", "Abc", "abc_", "a b", "abc/def"};
    u8 i;

    for (i = 0; i < sizeof(invalid) / sizeof(invalid[0]); i++)
    {
        spawn.username = Username(invalid[i]);
        EXPECT(!CoopPresence_EncodeSpawn(&spawn, bytes, sizeof(bytes)));
    }
    spawn.username = Username("a1_");
    EXPECT(!CoopPresence_EncodeSpawn(&spawn, bytes, sizeof(bytes)));

    spawn.username = Username("abc");
    spawn.username.bytes[3] = 'x';
    FillTestBytes(bytes, 0xb6, sizeof(bytes));
    EXPECT(!CoopPresence_EncodeSpawn(&spawn, bytes, sizeof(bytes)));
    ExpectCanary(bytes, 0xb6, sizeof(bytes));
    spawn.username = Username("abc");
    spawn.username.length = COOP_PRESENCE_USERNAME_MAX + 1;
    FillTestBytes(bytes, 0xc7, sizeof(bytes));
    EXPECT(!CoopPresence_EncodeSpawn(&spawn, bytes, sizeof(bytes)));
    ExpectCanary(bytes, 0xc7, sizeof(bytes));
    spawn.username = Username("abc");
    EXPECT(CoopPresence_EncodeSpawn(&spawn, bytes, sizeof(bytes)));
    bytes[COOP_PRESENCE_SPAWN_USERNAME_OFFSET + 4] = 'x';
    EXPECT(!CoopPresence_DecodeSpawn(bytes, sizeof(bytes), &spawn));

    EXPECT(!CoopPresence_EncodeDespawn(&(struct CoopPresenceDespawn){
        .handle = 0, .server_sequence = 1, .reason = COOP_PRESENCE_DESPAWN_HIDDEN,
    }, bytes, sizeof(bytes)));
    EXPECT(!CoopPresence_EncodeDespawn(&(struct CoopPresenceDespawn){
        .handle = 1, .server_sequence = 0, .reason = COOP_PRESENCE_DESPAWN_HIDDEN,
    }, bytes, sizeof(bytes)));
}

TEST("Cloud Coop presence decoders accept every assigned pose ordinal")
{
    struct CoopPresencePose pose = Pose(-4, 9, COOP_PRESENCE_DIRECTION_EAST,
                                        COOP_PRESENCE_PLAYER_OVERWORLD);
    struct CoopPresencePose decoded = Pose(1234, -1234,
                                           COOP_PRESENCE_DIRECTION_NORTH,
                                           COOP_PRESENCE_PLAYER_HIDDEN);
    u8 bytes[COOP_PRESENCE_POSE_SIZE];
    u8 i;

    for (i = COOP_PRESENCE_DIRECTION_SOUTH; i <= COOP_PRESENCE_DIRECTION_EAST; i++)
    {
        pose.direction = i;
        decoded.direction = 0;
        EXPECT(CoopPresence_EncodePose(&pose, bytes, sizeof(bytes)));
        EXPECT(CoopPresence_DecodePose(bytes, sizeof(bytes), &decoded));
        ExpectPoseEqual(&decoded, &pose);
    }
    for (i = COOP_PRESENCE_MOVEMENT_IDLE; i <= COOP_PRESENCE_MOVEMENT_RUN; i++)
    {
        pose.movement_mode = i;
        decoded.movement_mode = 0xff;
        EXPECT(CoopPresence_EncodePose(&pose, bytes, sizeof(bytes)));
        EXPECT(CoopPresence_DecodePose(bytes, sizeof(bytes), &decoded));
        ExpectPoseEqual(&decoded, &pose);
    }
    for (i = COOP_PRESENCE_ANIMATION_IDLE; i <= COOP_PRESENCE_ANIMATION_LOCOMOTION; i++)
    {
        pose.animation_id = i;
        decoded.animation_id = 0xff;
        EXPECT(CoopPresence_EncodePose(&pose, bytes, sizeof(bytes)));
        EXPECT(CoopPresence_DecodePose(bytes, sizeof(bytes), &decoded));
        ExpectPoseEqual(&decoded, &pose);
    }
    for (i = COOP_PRESENCE_AVATAR_BRENDAN; i <= COOP_PRESENCE_AVATAR_MAY; i++)
    {
        pose.avatar_id = i;
        decoded.avatar_id = 0;
        EXPECT(CoopPresence_EncodePose(&pose, bytes, sizeof(bytes)));
        EXPECT(CoopPresence_DecodePose(bytes, sizeof(bytes), &decoded));
        ExpectPoseEqual(&decoded, &pose);
    }
    for (i = COOP_PRESENCE_PLAYER_HIDDEN; i <= COOP_PRESENCE_PLAYER_OVERWORLD; i++)
    {
        pose.player_state = i;
        decoded.player_state = 0xff;
        EXPECT(CoopPresence_EncodePose(&pose, bytes, sizeof(bytes)));
        EXPECT(CoopPresence_DecodePose(bytes, sizeof(bytes), &decoded));
        ExpectPoseEqual(&decoded, &pose);
    }
}

TEST("Cloud Coop presence sequence ordering follows RFC 1982")
{
    EXPECT(CoopPresence_SequenceIsNewer(2, 1));
    EXPECT(!CoopPresence_SequenceIsNewer(1, 1));
    EXPECT(!CoopPresence_SequenceIsNewer(1, 2));
    EXPECT(CoopPresence_SequenceIsNewer(1, 0xffffffff));
    EXPECT(!CoopPresence_SequenceIsNewer(0xffffffff, 1));
    EXPECT(!CoopPresence_SequenceIsNewer(0, 1));
    EXPECT(!CoopPresence_SequenceIsNewer(1, 0));
    EXPECT(CoopPresence_SequenceIsNewer(0x80000000, 1));
    EXPECT(!CoopPresence_SequenceIsNewer(0x80000001, 1));
    EXPECT(!CoopPresence_SequenceIsNewer(0x80000000, 0xffffffff));
    EXPECT(!CoopPresence_SequenceIsNewer(0xffffffff, 0x7fffffff));
    EXPECT_EQ(CoopPresence_NextSequence(0), 1);
    EXPECT_EQ(CoopPresence_NextSequence(0xffffffff), 1);
    EXPECT_EQ(CoopPresence_NextSequence(0xfffffffe), 0xffffffff);
}

TEST("Cloud Coop presence reducer rejects equal and ambiguous wrapped sequences")
{
    struct CoopPresenceReducer reducer;
    struct CoopPresenceReducer halfRangeMinusOneReducer;
    struct CoopPresenceSpawn spawn = Spawn(1, 0xfffffffe, 11, 10,
                                           COOP_PRESENCE_DIRECTION_WEST,
                                           COOP_PRESENCE_PLAYER_OVERWORLD);
    struct CoopPresenceSpawn halfRangeMinusOneSpawn = Spawn(
        2, 1, 11, 10, COOP_PRESENCE_DIRECTION_WEST,
        COOP_PRESENCE_PLAYER_OVERWORLD);
    struct CoopPresenceUpdate update = {
        .handle = 1,
        .server_sequence = 0xffffffff,
        .state = State(11, 10, COOP_PRESENCE_DIRECTION_WEST,
                       COOP_PRESENCE_PLAYER_OVERWORLD),
    };
    struct CoopPresenceUpdate halfRangeMinusOneUpdate = {
        .handle = 2,
        .server_sequence = 0x80000000,
        .state = State(12, 10, COOP_PRESENCE_DIRECTION_WEST,
                       COOP_PRESENCE_PLAYER_OVERWORLD),
    };

    EstablishReducer(&reducer);
    spawn.state.pose.warp_sequence = 1;
    update.state.pose.warp_sequence = 1;
    EXPECT_EQ(CoopPresenceReducer_ApplySpawn(&reducer, &spawn),
              COOP_PRESENCE_APPLY_APPLIED);
    EXPECT_EQ(CoopPresenceReducer_ApplySpawn(&reducer, &spawn),
              COOP_PRESENCE_APPLY_STALE);
    EXPECT_EQ(CoopPresenceReducer_ApplyUpdate(&reducer, &update),
              COOP_PRESENCE_APPLY_APPLIED);
    update.server_sequence = 1;
    EXPECT_EQ(CoopPresenceReducer_ApplyUpdate(&reducer, &update),
              COOP_PRESENCE_APPLY_APPLIED);
    update.server_sequence = 0x80000001;
    update.state.pose.location.x = 12;
    EXPECT_EQ(CoopPresenceReducer_ApplyUpdate(&reducer, &update),
              COOP_PRESENCE_APPLY_STALE);
    EXPECT_EQ(CoopPresenceReducer_GetRemote(&reducer)->state.pose.location.x, 11);

    EstablishReducer(&halfRangeMinusOneReducer);
    halfRangeMinusOneSpawn.state.pose.warp_sequence = 1;
    halfRangeMinusOneUpdate.state.pose.warp_sequence = 1;
    EXPECT_EQ(CoopPresenceReducer_ApplySpawn(&halfRangeMinusOneReducer,
                                             &halfRangeMinusOneSpawn),
              COOP_PRESENCE_APPLY_APPLIED);
    EXPECT_EQ(CoopPresenceReducer_ApplyUpdate(&halfRangeMinusOneReducer,
                                              &halfRangeMinusOneUpdate),
              COOP_PRESENCE_APPLY_APPLIED);
    EXPECT_EQ(CoopPresenceReducer_GetRemote(&halfRangeMinusOneReducer)
                  ->server_sequence,
              0x80000000);
    EXPECT_EQ(CoopPresenceReducer_GetRemote(&halfRangeMinusOneReducer)
                  ->state.pose.location.x,
              12);
}

TEST("Cloud Coop presence reducer is bounded and lifecycle ordered")
{
    struct CoopPresenceReducer reducer;
    struct CoopPresenceSpawn spawn = Spawn(1, 10, 11, 10,
                                           COOP_PRESENCE_DIRECTION_WEST,
                                           COOP_PRESENCE_PLAYER_OVERWORLD);
    struct CoopPresenceUpdate update = {
        .handle = 1,
        .server_sequence = 11,
        .state = State(11, 10, COOP_PRESENCE_DIRECTION_WEST,
                       COOP_PRESENCE_PLAYER_OVERWORLD),
    };
    struct CoopPresenceDespawn despawn = {
        .handle = 1,
        .server_sequence = 12,
        .reason = COOP_PRESENCE_DESPAWN_STALE,
    };
    struct CoopPresenceSpawn other = Spawn(2, 1, 11, 10,
                                           COOP_PRESENCE_DIRECTION_WEST,
                                           COOP_PRESENCE_PLAYER_OVERWORLD);
    struct WorldLocation location = Location(10, 10);
    struct WorldLocation otherLocation = Location(10, 10);
    u8 reducerSnapshot[sizeof(reducer)];

    EstablishReducer(&reducer);
    spawn.state.pose.warp_sequence = 1;
    update.state.pose.warp_sequence = 1;
    other.state.pose.warp_sequence = 1;
    EXPECT_EQ(CoopPresenceReducer_ApplyUpdate(&reducer, &update),
              COOP_PRESENCE_APPLY_NOT_ACTIVE);
    EXPECT_EQ(CoopPresenceReducer_ApplySpawn(&reducer, &spawn),
              COOP_PRESENCE_APPLY_APPLIED);
    EXPECT(CoopPresenceReducer_IsActive(&reducer));
    EXPECT(CoopPresenceReducer_IsVisible(&reducer));
    EXPECT_EQ(CoopPresenceReducer_ApplySpawn(&reducer, &other),
              COOP_PRESENCE_APPLY_CAPACITY);
    EXPECT_EQ(CoopPresenceReducer_GetRemote(&reducer)->handle, 1);

    update.server_sequence = 10;
    EXPECT_EQ(CoopPresenceReducer_ApplyUpdate(&reducer, &update),
              COOP_PRESENCE_APPLY_STALE);
    update.server_sequence = 11;
    EXPECT_EQ(CoopPresenceReducer_ApplyUpdate(&reducer, &update),
              COOP_PRESENCE_APPLY_APPLIED);
    EXPECT_EQ(CoopPresenceReducer_GetRemote(&reducer)->handle, spawn.handle);
    EXPECT_EQ(CoopPresenceReducer_GetRemote(&reducer)->username.length,
              spawn.username.length);
    EXPECT(BytesEqual((const u8 *)CoopPresenceReducer_GetRemote(&reducer)->username.bytes,
                      (const u8 *)spawn.username.bytes,
                      sizeof(spawn.username.bytes)));
    update.server_sequence = 12;
    update.state.pose.player_state = COOP_PRESENCE_PLAYER_HIDDEN;
    EXPECT_EQ(CoopPresenceReducer_ApplyUpdate(&reducer, &update),
              COOP_PRESENCE_APPLY_APPLIED);
    ExpectReducerClearedAt(&reducer, 9, &location, 1);

    EXPECT_EQ(CoopPresenceReducer_ApplyDespawn(&reducer, &despawn),
              COOP_PRESENCE_APPLY_NOT_ACTIVE);
    update.state = State(11, 10, COOP_PRESENCE_DIRECTION_WEST,
                         COOP_PRESENCE_PLAYER_OVERWORLD);
    update.server_sequence = 13;
    CopyTestBytes(reducerSnapshot, (const u8 *)&reducer, sizeof(reducer));
    EXPECT_EQ(CoopPresenceReducer_ApplyUpdate(&reducer, &update),
              COOP_PRESENCE_APPLY_NOT_ACTIVE);
    ExpectStructBytes(&reducer, reducerSnapshot, sizeof(reducer));
    EXPECT(CoopPresenceReducer_ApplySpawn(&reducer, &spawn) == COOP_PRESENCE_APPLY_APPLIED);
    despawn.server_sequence = 10;
    EXPECT_EQ(CoopPresenceReducer_ApplyDespawn(&reducer, &despawn),
              COOP_PRESENCE_APPLY_STALE);
    spawn.server_sequence = 20;
    spawn.state.pose.location.x = 12;
    spawn.state.pose.location.y = 13;
    spawn.state.pose.direction = COOP_PRESENCE_DIRECTION_NORTH;
    spawn.username = Username("may");
    EXPECT_EQ(CoopPresenceReducer_ApplySpawn(&reducer, &spawn),
              COOP_PRESENCE_APPLY_APPLIED);
    EXPECT_EQ(CoopPresenceReducer_GetRemote(&reducer)->server_sequence, 20);
    EXPECT_EQ(CoopPresenceReducer_GetRemote(&reducer)->state.pose.location.x, 12);
    EXPECT_EQ(CoopPresenceReducer_GetRemote(&reducer)->state.pose.location.y, 13);
    EXPECT_EQ(CoopPresenceReducer_GetRemote(&reducer)->state.pose.direction,
              COOP_PRESENCE_DIRECTION_NORTH);
    EXPECT_EQ(CoopPresenceReducer_GetRemote(&reducer)->username.length, 3);
    EXPECT_EQ(CoopPresenceReducer_GetRemote(&reducer)->username.bytes[0], 'm');
    EXPECT_EQ(CoopPresenceReducer_GetRemote(&reducer)->username.bytes[2], 'y');
    despawn.server_sequence = 21;
    EXPECT_EQ(CoopPresenceReducer_ApplyDespawn(&reducer, &despawn),
              COOP_PRESENCE_APPLY_APPLIED);
    ExpectReducerClearedAt(&reducer, 9, &location, 1);

    EXPECT(CoopPresenceReducer_Synchronize(&reducer, 10, &location, 1));
    EXPECT(CoopPresenceReducer_Synchronize(&reducer, 10, &otherLocation, 2));
    EXPECT(!CoopPresenceReducer_IsActive(&reducer));
    EXPECT(!CoopPresenceReducer_Synchronize(&reducer, 0, &location, 1));
    EXPECT(!CoopPresenceReducer_Synchronize(&reducer, 10, &location, 0));
    CoopPresenceReducer_Reset(&reducer);
    EXPECT(!CoopPresenceReducer_IsActive(&reducer));
}

TEST("Cloud Coop presence reducer rejects partition and handle mismatches")
{
    struct CoopPresenceReducer reducer;
    struct CoopPresenceSpawn spawn = Spawn(1, 1, 11, 10,
                                           COOP_PRESENCE_DIRECTION_WEST,
                                           COOP_PRESENCE_PLAYER_OVERWORLD);
    struct CoopPresenceUpdate update = {
        .handle = 1,
        .server_sequence = 2,
        .state = State(11, 10, COOP_PRESENCE_DIRECTION_WEST,
                       COOP_PRESENCE_PLAYER_OVERWORLD),
    };
    struct WorldLocation location = Location(10, 10);

    EstablishReducer(&reducer);
    spawn.state.pose.warp_sequence = 1;
    update.state.pose.warp_sequence = 1;
    spawn.state.pose.warp_sequence++;
    EXPECT_EQ(CoopPresenceReducer_ApplySpawn(&reducer, &spawn),
              COOP_PRESENCE_APPLY_PARTITION_MISMATCH);
    spawn.state.pose.warp_sequence--;
    EXPECT(CoopPresenceReducer_ApplySpawn(&reducer, &spawn) == COOP_PRESENCE_APPLY_APPLIED);
    update.handle = 2;
    EXPECT_EQ(CoopPresenceReducer_ApplyUpdate(&reducer, &update),
              COOP_PRESENCE_APPLY_HANDLE_MISMATCH);
    update.handle = 1;
    update.state.pose.location.region = COOP_REGION_KANTO;
    EXPECT_EQ(CoopPresenceReducer_ApplyUpdate(&reducer, &update),
              COOP_PRESENCE_APPLY_REJECTED);
    EXPECT(CoopPresenceReducer_IsActive(&reducer));
    EXPECT_EQ(CoopPresenceReducer_GetRemote(&reducer)->handle, 1);

    location.region = COOP_REGION_KANTO;
    location.map_group = 37;
    location.map_number = 0;
    EXPECT(CoopPresenceReducer_Synchronize(&reducer, 9, &location, 1));
    ExpectReducerClearedAt(&reducer, 9, &location, 1);
}

TEST("Cloud Coop presence reducer rejects invalid occupied messages atomically")
{
    struct CoopPresenceReducer reducer;
    struct CoopPresenceSpawn spawn = Spawn(1, 10, 11, 10,
                                           COOP_PRESENCE_DIRECTION_WEST,
                                           COOP_PRESENCE_PLAYER_OVERWORLD);
    struct CoopPresenceUpdate update;
    struct CoopPresenceDespawn despawn = {
        .handle = 1,
        .server_sequence = 12,
        .reason = COOP_PRESENCE_DESPAWN_STALE,
    };
    u8 snapshot[sizeof(reducer)];

    EstablishReducer(&reducer);
    spawn.state.pose.warp_sequence = 1;
    EXPECT(CoopPresenceReducer_ApplySpawn(&reducer, &spawn)
           == COOP_PRESENCE_APPLY_APPLIED);
    update.handle = 1;
    update.server_sequence = 11;
    update.state = spawn.state;
    CopyTestBytes(snapshot, (const u8 *)&reducer, sizeof(reducer));

    spawn.server_sequence = 0;
    EXPECT(CoopPresenceReducer_ApplySpawn(&reducer, &spawn)
           == COOP_PRESENCE_APPLY_REJECTED);
    ExpectStructBytes(&reducer, snapshot, sizeof(reducer));
    spawn.server_sequence = 10;
    spawn.state.pose.warp_sequence = 0;
    EXPECT(CoopPresenceReducer_ApplySpawn(&reducer, &spawn)
           == COOP_PRESENCE_APPLY_REJECTED);
    ExpectStructBytes(&reducer, snapshot, sizeof(reducer));
    spawn.state = update.state;
    spawn.username = Username("ab");
    EXPECT(CoopPresenceReducer_ApplySpawn(&reducer, &spawn)
           == COOP_PRESENCE_APPLY_REJECTED);
    ExpectStructBytes(&reducer, snapshot, sizeof(reducer));
    spawn.username = Username("ash-kanto");

    update.handle = 0;
    EXPECT(CoopPresenceReducer_ApplyUpdate(&reducer, &update)
           == COOP_PRESENCE_APPLY_REJECTED);
    ExpectStructBytes(&reducer, snapshot, sizeof(reducer));
    update.handle = 1;
    update.server_sequence = 0;
    EXPECT(CoopPresenceReducer_ApplyUpdate(&reducer, &update)
           == COOP_PRESENCE_APPLY_REJECTED);
    ExpectStructBytes(&reducer, snapshot, sizeof(reducer));
    update.server_sequence = 11;

    despawn.handle = 0;
    EXPECT(CoopPresenceReducer_ApplyDespawn(&reducer, &despawn)
           == COOP_PRESENCE_APPLY_REJECTED);
    ExpectStructBytes(&reducer, snapshot, sizeof(reducer));
    despawn.handle = 1;
    despawn.server_sequence = 0;
    EXPECT(CoopPresenceReducer_ApplyDespawn(&reducer, &despawn)
           == COOP_PRESENCE_APPLY_REJECTED);
    ExpectStructBytes(&reducer, snapshot, sizeof(reducer));
}

TEST("Cloud Coop presence interaction validates all facings and boundaries")
{
    static const u8 expected[COOP_PRESENCE_INTERACTION_SIZE] = {
        1, 0, 0, 0, 0, 0, 0, 0, 10, 0, 0, 0, 1, 0, 0, 0,
        10, 0, 11, 0,
    };
    struct CoopPresenceReducer reducer;
    struct CoopPresenceSpawn spawn;
    struct CoopPresenceLocalContext local = {
        .session_epoch = 9,
        .location = Location(10, 10),
        .elevation = 7,
        .direction = COOP_PRESENCE_DIRECTION_SOUTH,
        .warp_sequence = 1,
    };
    static const struct {
        s16 local_x;
        s16 local_y;
        s16 remote_x;
        s16 remote_y;
        u8 direction;
    } endpoints[] = {
        {32766, 0, 32767, 0, COOP_PRESENCE_DIRECTION_EAST},
        {-32767, 0, -32768, 0, COOP_PRESENCE_DIRECTION_WEST},
        {0, 32766, 0, 32767, COOP_PRESENCE_DIRECTION_SOUTH},
        {0, -32767, 0, -32768, COOP_PRESENCE_DIRECTION_NORTH},
    };
    struct CoopPresenceInteraction decoded;
    u8 bytes[COOP_PRESENCE_INTERACTION_SIZE];
    u8 lengthCanary[COOP_PRESENCE_INTERACTION_SIZE + 1];
    u8 i;

    EstablishReducer(&reducer);
    spawn = Spawn(1, 10, 10, 11, COOP_PRESENCE_DIRECTION_NORTH,
                  COOP_PRESENCE_PLAYER_OVERWORLD);
    spawn.state.pose.warp_sequence = 1;
    EXPECT(CoopPresenceReducer_ApplySpawn(&reducer, &spawn) == COOP_PRESENCE_APPLY_APPLIED);
    EXPECT(CoopPresence_EncodeInteraction(&reducer, &local, bytes, sizeof(bytes)));
    EXPECT(BytesEqual(bytes, expected, sizeof(expected)));

    /* Contextual encoding rejects both wrong lengths without touching the full
     * caller buffer, and rejects a null destination even for valid context. */
    FillTestBytes(lengthCanary, 0xc7, sizeof(lengthCanary));
    EXPECT(!CoopPresence_EncodeInteraction(&reducer, &local, lengthCanary,
                                           COOP_PRESENCE_INTERACTION_SIZE - 1));
    ExpectCanary(lengthCanary, 0xc7, sizeof(lengthCanary));
    FillTestBytes(lengthCanary, 0xd8, sizeof(lengthCanary));
    EXPECT(!CoopPresence_EncodeInteraction(&reducer, &local, lengthCanary,
                                           COOP_PRESENCE_INTERACTION_SIZE + 1));
    ExpectCanary(lengthCanary, 0xd8, sizeof(lengthCanary));
    EXPECT(!CoopPresence_EncodeInteraction(&reducer, &local, NULL,
                                           COOP_PRESENCE_INTERACTION_SIZE));

    for (i = COOP_PRESENCE_DIRECTION_SOUTH; i <= COOP_PRESENCE_DIRECTION_EAST; i++)
    {
        local.direction = i;
        spawn.state.pose.location.x = 10;
        spawn.state.pose.location.y = 10;
        switch (i)
        {
        case COOP_PRESENCE_DIRECTION_SOUTH:
            spawn.state.pose.location.y = 11;
            break;
        case COOP_PRESENCE_DIRECTION_NORTH:
            spawn.state.pose.location.y = 9;
            break;
        case COOP_PRESENCE_DIRECTION_WEST:
            spawn.state.pose.location.x = 9;
            break;
        case COOP_PRESENCE_DIRECTION_EAST:
            spawn.state.pose.location.x = 11;
            break;
        }
        spawn.server_sequence++;
        EXPECT(CoopPresenceReducer_ApplyUpdate(&reducer,
                                               &(struct CoopPresenceUpdate){
                                                   .handle = 1,
                                                   .server_sequence = spawn.server_sequence,
                                                   .state = spawn.state,
                                               }) == COOP_PRESENCE_APPLY_APPLIED);
        EXPECT(CoopPresence_EncodeInteraction(&reducer, &local, bytes, sizeof(bytes)));
    }

    for (i = 0; i < sizeof(endpoints) / sizeof(endpoints[0]); i++)
    {
        local.location.x = endpoints[i].local_x;
        local.location.y = endpoints[i].local_y;
        local.direction = endpoints[i].direction;
        spawn.state.pose.location.x = endpoints[i].remote_x;
        spawn.state.pose.location.y = endpoints[i].remote_y;
        spawn.server_sequence++;
        EXPECT(CoopPresenceReducer_ApplyUpdate(&reducer,
                                               &(struct CoopPresenceUpdate){
                                                   .handle = 1,
                                                   .server_sequence = spawn.server_sequence,
                                                   .state = spawn.state,
                                               }) == COOP_PRESENCE_APPLY_APPLIED);
        EXPECT(CoopPresence_EncodeInteraction(&reducer, &local, bytes, sizeof(bytes)));
        EXPECT(CoopPresence_DecodeInteraction(bytes, sizeof(bytes), &decoded));
        EXPECT_EQ(decoded.handle, 1);
        EXPECT_EQ(decoded.observed_server_sequence, spawn.server_sequence);
        EXPECT_EQ(decoded.observed_warp_sequence, 1);
        EXPECT_EQ(decoded.x, endpoints[i].remote_x);
        EXPECT_EQ(decoded.y, endpoints[i].remote_y);
    }

    local.direction = COOP_PRESENCE_DIRECTION_EAST;
    local.location.x = 32767;
    spawn.state.pose.location.x = 32767;
    spawn.state.pose.location.y = 10;
    spawn.server_sequence++;
    EXPECT(CoopPresenceReducer_ApplyUpdate(&reducer,
                                           &(struct CoopPresenceUpdate){
                                               .handle = 1,
                                               .server_sequence = spawn.server_sequence,
                                               .state = spawn.state,
                                           }) == COOP_PRESENCE_APPLY_APPLIED);
    for (i = 0; i < sizeof(bytes); i++)
        bytes[i] = 0x5a;
    EXPECT(!CoopPresence_EncodeInteraction(&reducer, &local, bytes, sizeof(bytes)));
    ExpectCanary(bytes, 0x5a, sizeof(bytes));

    local.location.x = 10;
    local.location.y = 10;
    local.direction = COOP_PRESENCE_DIRECTION_SOUTH;
    spawn.state.pose.location.x = 10;
    spawn.state.pose.location.y = 11;
    spawn.server_sequence++;
    spawn.state.pose.elevation++;
    EXPECT(CoopPresenceReducer_ApplyUpdate(&reducer,
                                           &(struct CoopPresenceUpdate){
                                               .handle = 1,
                                               .server_sequence = spawn.server_sequence,
                                               .state = spawn.state,
                                           }) == COOP_PRESENCE_APPLY_APPLIED);
    for (i = 0; i < sizeof(bytes); i++)
        bytes[i] = 0x6b;
    EXPECT(!CoopPresence_EncodeInteraction(&reducer, &local, bytes, sizeof(bytes)));
    ExpectCanary(bytes, 0x6b, sizeof(bytes));
}

TEST("Cloud Coop presence rejects every invalid ordinal independently")
{
    struct CoopPresenceLocalState state = State(-4, 9,
                                                 COOP_PRESENCE_DIRECTION_EAST,
                                                 COOP_PRESENCE_PLAYER_OVERWORLD);
    struct CoopPresenceLocalState output = state;
    struct CoopPresencePose pose = state.pose;
    struct CoopPresencePose poseOutput = pose;
    u8 localBytes[COOP_PRESENCE_LOCAL_STATE_SIZE];
    u8 poseBytes[COOP_PRESENCE_POSE_SIZE];
    u8 localSnapshot[sizeof(output)];
    u8 poseSnapshot[sizeof(poseOutput)];

    output = State(1234, -1234, COOP_PRESENCE_DIRECTION_NORTH,
                   COOP_PRESENCE_PLAYER_HIDDEN);
    poseOutput = Pose(1234, -1234, COOP_PRESENCE_DIRECTION_NORTH,
                      COOP_PRESENCE_PLAYER_HIDDEN);
    CopyTestBytes(localSnapshot, (const u8 *)&output, sizeof(output));
    CopyTestBytes(poseSnapshot, (const u8 *)&poseOutput, sizeof(poseOutput));
    EXPECT(CoopPresence_EncodeLocalState(&state, localBytes, sizeof(localBytes)));
    EXPECT(CoopPresence_EncodePose(&pose, poseBytes, sizeof(poseBytes)));

    RejectLocalByte(localBytes, COOP_PRESENCE_WORLD_LOCATION_REGION_OFFSET, 0,
                    &output, localSnapshot);
    RejectLocalByte(localBytes, COOP_PRESENCE_WORLD_LOCATION_REGION_OFFSET, 5,
                    &output, localSnapshot);
    RejectLocalByte(localBytes, COOP_PRESENCE_POSE_DIRECTION_OFFSET, 0,
                    &output, localSnapshot);
    RejectLocalByte(localBytes, COOP_PRESENCE_POSE_DIRECTION_OFFSET, 5,
                    &output, localSnapshot);
    RejectLocalByte(localBytes, COOP_PRESENCE_POSE_MOVEMENT_MODE_OFFSET, 3,
                    &output, localSnapshot);
    RejectLocalByte(localBytes, COOP_PRESENCE_POSE_ANIMATION_ID_OFFSET, 2,
                    &output, localSnapshot);
    RejectLocalByte(localBytes, COOP_PRESENCE_POSE_AVATAR_ID_OFFSET, 0,
                    &output, localSnapshot);
    RejectLocalByte(localBytes, COOP_PRESENCE_POSE_AVATAR_ID_OFFSET, 3,
                    &output, localSnapshot);
    RejectLocalByte(localBytes, COOP_PRESENCE_POSE_PLAYER_STATE_OFFSET, 2,
                    &output, localSnapshot);

    RejectPoseByte(poseBytes, COOP_PRESENCE_POSE_DIRECTION_OFFSET, 0,
                   &poseOutput, poseSnapshot);
    RejectPoseByte(poseBytes, COOP_PRESENCE_POSE_DIRECTION_OFFSET, 5,
                   &poseOutput, poseSnapshot);
    RejectPoseByte(poseBytes, COOP_PRESENCE_POSE_MOVEMENT_MODE_OFFSET, 3,
                   &poseOutput, poseSnapshot);
    RejectPoseByte(poseBytes, COOP_PRESENCE_POSE_ANIMATION_ID_OFFSET, 2,
                   &poseOutput, poseSnapshot);
    RejectPoseByte(poseBytes, COOP_PRESENCE_POSE_AVATAR_ID_OFFSET, 0,
                   &poseOutput, poseSnapshot);
    RejectPoseByte(poseBytes, COOP_PRESENCE_POSE_AVATAR_ID_OFFSET, 3,
                   &poseOutput, poseSnapshot);
    RejectPoseByte(poseBytes, COOP_PRESENCE_POSE_PLAYER_STATE_OFFSET, 2,
                   &poseOutput, poseSnapshot);

    RejectLocalByte(localBytes, COOP_PRESENCE_WORLD_LOCATION_RESERVED_OFFSET, 1,
                    &output, localSnapshot);
    RejectLocalWord(localBytes, COOP_PRESENCE_POSE_WARP_SEQUENCE_OFFSET,
                    &output, localSnapshot);
    RejectLocalWord(localBytes, COOP_PRESENCE_LOCAL_STATE_SOURCE_SEQUENCE_OFFSET,
                    &output, localSnapshot);
}

TEST("Cloud Coop presence propagates strict ordinal rejection through spawn and update")
{
    struct CoopPresenceSpawn spawn = Spawn(1, 1, -4, 9,
                                           COOP_PRESENCE_DIRECTION_EAST,
                                           COOP_PRESENCE_PLAYER_OVERWORLD);
    struct CoopPresenceSpawn spawnOutput = spawn;
    struct CoopPresenceUpdate update = {
        .handle = 1,
        .server_sequence = 1,
        .state = spawn.state,
    };
    struct CoopPresenceUpdate updateOutput = update;
    u8 spawnBytes[COOP_PRESENCE_SPAWN_SIZE];
    u8 updateBytesLocal[COOP_PRESENCE_UPDATE_SIZE];
    u8 spawnSnapshot[sizeof(spawnOutput)];
    u8 updateSnapshot[sizeof(updateOutput)];
    static const u8 invalidOffsets[] = {
        COOP_PRESENCE_WORLD_LOCATION_REGION_OFFSET,
        COOP_PRESENCE_POSE_DIRECTION_OFFSET,
        COOP_PRESENCE_POSE_MOVEMENT_MODE_OFFSET,
        COOP_PRESENCE_POSE_ANIMATION_ID_OFFSET,
        COOP_PRESENCE_POSE_AVATAR_ID_OFFSET,
        COOP_PRESENCE_POSE_PLAYER_STATE_OFFSET,
    };
    static const u8 invalidValues[] = {0, 0, 3, 2, 0, 2};
    u8 i;

    spawnOutput = Spawn(2, 4, 2, 3, COOP_PRESENCE_DIRECTION_NORTH,
                        COOP_PRESENCE_PLAYER_HIDDEN);
    updateOutput.handle = 2;
    updateOutput.server_sequence = 4;
    updateOutput.state = State(2, 3, COOP_PRESENCE_DIRECTION_NORTH,
                               COOP_PRESENCE_PLAYER_HIDDEN);
    CopyTestBytes(spawnSnapshot, (const u8 *)&spawnOutput, sizeof(spawnOutput));
    CopyTestBytes(updateSnapshot, (const u8 *)&updateOutput, sizeof(updateOutput));
    EXPECT(CoopPresence_EncodeSpawn(&spawn, spawnBytes, sizeof(spawnBytes)));
    EXPECT(CoopPresence_EncodeUpdate(&update, updateBytesLocal,
                                     sizeof(updateBytesLocal)));
    for (i = 0; i < sizeof(invalidOffsets); i++)
    {
        RejectSpawnStateByte(spawnBytes, invalidOffsets[i], invalidValues[i],
                             &spawnOutput, spawnSnapshot);
        RejectUpdateStateByte(updateBytesLocal, invalidOffsets[i], invalidValues[i],
                              &updateOutput, updateSnapshot);
    }

    /* Region has two invalid wire values and avatar/direction have an upper
     * invalid value; those boundaries are covered independently above and
     * here through the outer codecs as well. */
    RejectSpawnStateByte(spawnBytes, COOP_PRESENCE_WORLD_LOCATION_REGION_OFFSET,
                         5, &spawnOutput, spawnSnapshot);
    RejectUpdateStateByte(updateBytesLocal, COOP_PRESENCE_WORLD_LOCATION_REGION_OFFSET,
                          5, &updateOutput, updateSnapshot);
    RejectSpawnStateByte(spawnBytes, COOP_PRESENCE_POSE_DIRECTION_OFFSET,
                         5, &spawnOutput, spawnSnapshot);
    RejectUpdateStateByte(updateBytesLocal, COOP_PRESENCE_POSE_DIRECTION_OFFSET,
                          5, &updateOutput, updateSnapshot);
    RejectSpawnStateByte(spawnBytes, COOP_PRESENCE_POSE_AVATAR_ID_OFFSET,
                         3, &spawnOutput, spawnSnapshot);
    RejectUpdateStateByte(updateBytesLocal, COOP_PRESENCE_POSE_AVATAR_ID_OFFSET,
                          3, &updateOutput, updateSnapshot);
}

TEST("Cloud Coop presence update and length rejection preserve output")
{
    struct CoopPresenceUpdate update = {
        .handle = 0x0123456789abcdefULL,
        .server_sequence = 3,
        .state = State(-4, 9, COOP_PRESENCE_DIRECTION_EAST,
                       COOP_PRESENCE_PLAYER_OVERWORLD),
    };
    struct CoopPresenceUpdate output = update;
    struct CoopPresencePose pose = update.state.pose;
    struct CoopPresencePose poseOutput = pose;
    struct CoopPresenceLocalState state = update.state;
    struct CoopPresenceLocalState stateOutput = state;
    struct CoopPresenceSpawn spawn = Spawn(update.handle, update.server_sequence,
                                           -4, 9, COOP_PRESENCE_DIRECTION_EAST,
                                           COOP_PRESENCE_PLAYER_OVERWORLD);
    struct CoopPresenceSpawn spawnOutput = spawn;
    struct CoopPresenceDespawn despawn = {
        .handle = update.handle,
        .server_sequence = update.server_sequence,
        .reason = COOP_PRESENCE_DESPAWN_HIDDEN,
    };
    struct CoopPresenceDespawn despawnOutput = despawn;
    struct CoopPresenceInteraction interaction = {
        .handle = update.handle,
        .observed_server_sequence = update.server_sequence,
        .observed_warp_sequence = 1,
        .x = -4,
        .y = 9,
    };
    struct CoopPresenceInteraction interactionOutput = interaction;
    u8 bytes[COOP_PRESENCE_SPAWN_SIZE + 1];
    u8 canary[COOP_PRESENCE_SPAWN_SIZE + 1];
    u8 outputSnapshot[sizeof(output)];
    u8 poseSnapshot[sizeof(poseOutput)];
    u8 stateSnapshot[sizeof(stateOutput)];
    u8 spawnSnapshot[sizeof(spawnOutput)];
    u8 despawnSnapshot[sizeof(despawnOutput)];
    u8 interactionSnapshot[sizeof(interactionOutput)];

    output.handle = 2;
    output.server_sequence = 4;
    output.state = State(1234, -1234, COOP_PRESENCE_DIRECTION_NORTH,
                         COOP_PRESENCE_PLAYER_HIDDEN);
    poseOutput = Pose(1234, -1234, COOP_PRESENCE_DIRECTION_NORTH,
                      COOP_PRESENCE_PLAYER_HIDDEN);
    stateOutput = State(1234, -1234, COOP_PRESENCE_DIRECTION_NORTH,
                        COOP_PRESENCE_PLAYER_HIDDEN);
    spawnOutput = Spawn(2, 4, 1234, -1234, COOP_PRESENCE_DIRECTION_NORTH,
                        COOP_PRESENCE_PLAYER_HIDDEN);
    despawnOutput.handle = 2;
    despawnOutput.server_sequence = 4;
    despawnOutput.reason = COOP_PRESENCE_DESPAWN_STALE;
    interactionOutput.handle = 2;
    interactionOutput.observed_server_sequence = 4;
    interactionOutput.observed_warp_sequence = 2;
    interactionOutput.x = 1234;
    interactionOutput.y = -1234;
    CopyTestBytes(outputSnapshot, (const u8 *)&output, sizeof(output));
    CopyTestBytes(poseSnapshot, (const u8 *)&poseOutput, sizeof(poseOutput));
    CopyTestBytes(stateSnapshot, (const u8 *)&stateOutput, sizeof(stateOutput));
    CopyTestBytes(spawnSnapshot, (const u8 *)&spawnOutput, sizeof(spawnOutput));
    CopyTestBytes(despawnSnapshot, (const u8 *)&despawnOutput, sizeof(despawnOutput));
    CopyTestBytes(interactionSnapshot, (const u8 *)&interactionOutput,
                  sizeof(interactionOutput));

    FillTestBytes(canary, 0xa5, sizeof(canary));
    EXPECT(!CoopPresence_EncodeUpdate(&update, canary, COOP_PRESENCE_UPDATE_SIZE - 1));
    ExpectCanary(canary, 0xa5, sizeof(canary));
    EXPECT(!CoopPresence_EncodePose(&pose, canary, COOP_PRESENCE_POSE_SIZE - 1));
    ExpectCanary(canary, 0xa5, sizeof(canary));
    EXPECT(!CoopPresence_EncodeLocalState(&state, canary,
                                          COOP_PRESENCE_LOCAL_STATE_SIZE - 1));
    ExpectCanary(canary, 0xa5, sizeof(canary));
    EXPECT(!CoopPresence_EncodeSpawn(&spawn, canary, COOP_PRESENCE_SPAWN_SIZE - 1));
    ExpectCanary(canary, 0xa5, sizeof(canary));
    EXPECT(!CoopPresence_EncodeDespawn(&despawn, canary,
                                       COOP_PRESENCE_DESPAWN_SIZE - 1));
    ExpectCanary(canary, 0xa5, sizeof(canary));
    EXPECT(!EncodeInteractionFixture(&interaction, canary,
                                     COOP_PRESENCE_INTERACTION_SIZE - 1));
    ExpectCanary(canary, 0xa5, sizeof(canary));
    EXPECT(!CoopPresence_EncodeUpdate(&update, canary, COOP_PRESENCE_UPDATE_SIZE + 1));
    ExpectCanary(canary, 0xa5, sizeof(canary));
    EXPECT(!CoopPresence_EncodePose(&pose, canary, COOP_PRESENCE_POSE_SIZE + 1));
    ExpectCanary(canary, 0xa5, sizeof(canary));
    EXPECT(!CoopPresence_EncodeLocalState(&state, canary,
                                          COOP_PRESENCE_LOCAL_STATE_SIZE + 1));
    ExpectCanary(canary, 0xa5, sizeof(canary));
    EXPECT(!CoopPresence_EncodeSpawn(&spawn, canary, COOP_PRESENCE_SPAWN_SIZE + 1));
    ExpectCanary(canary, 0xa5, sizeof(canary));
    EXPECT(!CoopPresence_EncodeDespawn(&despawn, canary,
                                       COOP_PRESENCE_DESPAWN_SIZE + 1));
    ExpectCanary(canary, 0xa5, sizeof(canary));
    EXPECT(!EncodeInteractionFixture(&interaction, canary,
                                     COOP_PRESENCE_INTERACTION_SIZE + 1));
    ExpectCanary(canary, 0xa5, sizeof(canary));

    EXPECT(CoopPresence_EncodeUpdate(&update, bytes, COOP_PRESENCE_UPDATE_SIZE));
    EXPECT(!CoopPresence_DecodeUpdate(bytes, COOP_PRESENCE_UPDATE_SIZE - 1, &output));
    ExpectStructBytes(&output, outputSnapshot, sizeof(output));
    EXPECT(!CoopPresence_DecodeUpdate(bytes, COOP_PRESENCE_UPDATE_SIZE + 1, &output));
    ExpectStructBytes(&output, outputSnapshot, sizeof(output));

    EXPECT(CoopPresence_EncodePose(&pose, bytes, COOP_PRESENCE_POSE_SIZE));
    EXPECT(!CoopPresence_DecodePose(bytes, COOP_PRESENCE_POSE_SIZE - 1,
                                    &poseOutput));
    ExpectStructBytes(&poseOutput, poseSnapshot, sizeof(poseOutput));
    EXPECT(!CoopPresence_DecodePose(bytes, COOP_PRESENCE_POSE_SIZE + 1,
                                    &poseOutput));
    ExpectStructBytes(&poseOutput, poseSnapshot, sizeof(poseOutput));

    EXPECT(CoopPresence_EncodeLocalState(&state, bytes, COOP_PRESENCE_LOCAL_STATE_SIZE));
    EXPECT(!CoopPresence_DecodeLocalState(bytes, COOP_PRESENCE_LOCAL_STATE_SIZE - 1,
                                          &stateOutput));
    ExpectStructBytes(&stateOutput, stateSnapshot, sizeof(stateOutput));
    EXPECT(!CoopPresence_DecodeLocalState(bytes, COOP_PRESENCE_LOCAL_STATE_SIZE + 1,
                                          &stateOutput));
    ExpectStructBytes(&stateOutput, stateSnapshot, sizeof(stateOutput));

    EXPECT(CoopPresence_EncodeSpawn(&spawn, bytes, COOP_PRESENCE_SPAWN_SIZE));
    EXPECT(!CoopPresence_DecodeSpawn(bytes, COOP_PRESENCE_SPAWN_SIZE - 1,
                                     &spawnOutput));
    ExpectStructBytes(&spawnOutput, spawnSnapshot, sizeof(spawnOutput));
    EXPECT(!CoopPresence_DecodeSpawn(bytes, COOP_PRESENCE_SPAWN_SIZE + 1,
                                     &spawnOutput));
    ExpectStructBytes(&spawnOutput, spawnSnapshot, sizeof(spawnOutput));

    EXPECT(CoopPresence_EncodeDespawn(&despawn, bytes, COOP_PRESENCE_DESPAWN_SIZE));
    EXPECT(!CoopPresence_DecodeDespawn(bytes, COOP_PRESENCE_DESPAWN_SIZE - 1,
                                       &despawnOutput));
    ExpectStructBytes(&despawnOutput, despawnSnapshot, sizeof(despawnOutput));
    EXPECT(!CoopPresence_DecodeDespawn(bytes, COOP_PRESENCE_DESPAWN_SIZE + 1,
                                       &despawnOutput));
    ExpectStructBytes(&despawnOutput, despawnSnapshot, sizeof(despawnOutput));

    EXPECT(EncodeInteractionFixture(&interaction, bytes,
                                    COOP_PRESENCE_INTERACTION_SIZE));
    EXPECT(!CoopPresence_DecodeInteraction(bytes, COOP_PRESENCE_INTERACTION_SIZE - 1,
                                           &interactionOutput));
    ExpectStructBytes(&interactionOutput, interactionSnapshot,
                      sizeof(interactionOutput));
    EXPECT(!CoopPresence_DecodeInteraction(bytes, COOP_PRESENCE_INTERACTION_SIZE + 1,
                                           &interactionOutput));
    ExpectStructBytes(&interactionOutput, interactionSnapshot,
                      sizeof(interactionOutput));
}

TEST("Cloud Coop presence validates nested update fields and output canaries")
{
    struct CoopPresenceUpdate update = {
        .handle = 0x0123456789abcdefULL,
        .server_sequence = 3,
        .state = State(-4, 9, COOP_PRESENCE_DIRECTION_EAST,
                       COOP_PRESENCE_PLAYER_OVERWORLD),
    };
    struct CoopPresenceUpdate output = update;
    u8 bytes[COOP_PRESENCE_UPDATE_SIZE];
    u8 snapshot[sizeof(output)];
    u8 canary[COOP_PRESENCE_UPDATE_SIZE];
    u32 i;

    output.handle = 2;
    output.server_sequence = 4;
    output.state = State(1234, -1234, COOP_PRESENCE_DIRECTION_NORTH,
                         COOP_PRESENCE_PLAYER_HIDDEN);
    CopyTestBytes(snapshot, (const u8 *)&output, sizeof(output));
    EXPECT(CoopPresence_EncodeUpdate(&update, bytes, sizeof(bytes)));

    bytes[COOP_PRESENCE_UPDATE_STATE_OFFSET
          + COOP_PRESENCE_POSE_LOCATION_OFFSET
          + COOP_PRESENCE_WORLD_LOCATION_RESERVED_OFFSET] = 1;
    EXPECT(!CoopPresence_DecodeUpdate(bytes, sizeof(bytes), &output));
    ExpectStructBytes(&output, snapshot, sizeof(output));
    bytes[COOP_PRESENCE_UPDATE_STATE_OFFSET
          + COOP_PRESENCE_POSE_LOCATION_OFFSET
          + COOP_PRESENCE_WORLD_LOCATION_RESERVED_OFFSET] = 0;

    for (i = 0; i < sizeof(u32); i++)
        bytes[COOP_PRESENCE_UPDATE_STATE_OFFSET
              + COOP_PRESENCE_POSE_WARP_SEQUENCE_OFFSET + i] = 0;
    EXPECT(!CoopPresence_DecodeUpdate(bytes, sizeof(bytes), &output));
    ExpectStructBytes(&output, snapshot, sizeof(output));
    EXPECT(CoopPresence_EncodeUpdate(&update, bytes, sizeof(bytes)));

    for (i = 0; i < sizeof(u32); i++)
        bytes[COOP_PRESENCE_UPDATE_STATE_OFFSET
              + COOP_PRESENCE_LOCAL_STATE_SOURCE_SEQUENCE_OFFSET + i] = 0;
    EXPECT(!CoopPresence_DecodeUpdate(bytes, sizeof(bytes), &output));
    ExpectStructBytes(&output, snapshot, sizeof(output));
    EXPECT(CoopPresence_EncodeUpdate(&update, bytes, sizeof(bytes)));

    for (i = 0; i < sizeof(u64); i++)
        bytes[COOP_PRESENCE_UPDATE_HANDLE_OFFSET + i] = 0;
    EXPECT(!CoopPresence_DecodeUpdate(bytes, sizeof(bytes), &output));
    ExpectStructBytes(&output, snapshot, sizeof(output));
    EXPECT(CoopPresence_EncodeUpdate(&update, bytes, sizeof(bytes)));

    for (i = 0; i < sizeof(u32); i++)
        bytes[COOP_PRESENCE_UPDATE_SERVER_SEQUENCE_OFFSET + i] = 0;
    EXPECT(!CoopPresence_DecodeUpdate(bytes, sizeof(bytes), &output));
    ExpectStructBytes(&output, snapshot, sizeof(output));

    FillTestBytes(canary, 0xa5, sizeof(canary));
    update.handle = 0;
    EXPECT(!CoopPresence_EncodeUpdate(&update, canary, sizeof(canary)));
    ExpectCanary(canary, 0xa5, sizeof(canary));
    update.handle = 0x0123456789abcdefULL;
    update.server_sequence = 0;
    EXPECT(!CoopPresence_EncodeUpdate(&update, canary, sizeof(canary)));
    ExpectCanary(canary, 0xa5, sizeof(canary));
}

TEST("Cloud Coop presence accepts canonical usernames at both length edges")
{
    struct CoopPresenceSpawn spawn = Spawn(1, 1, -4, 9,
                                           COOP_PRESENCE_DIRECTION_EAST,
                                           COOP_PRESENCE_PLAYER_OVERWORLD);
    struct CoopPresenceSpawn output = spawn;
    u8 bytes[COOP_PRESENCE_SPAWN_SIZE];
    u8 snapshot[sizeof(output)];
    u8 canary[COOP_PRESENCE_SPAWN_SIZE];
    const char *full = "abcdefghijklmnopqrstuvwxyz012345";
    const char *interior = "a_b.c-9";

    spawn.username = Username("abc");
    EXPECT(CoopPresence_EncodeSpawn(&spawn, bytes, sizeof(bytes)));
    EXPECT(CoopPresence_DecodeSpawn(bytes, sizeof(bytes), &output));
    EXPECT_EQ(output.username.length, 3);
    EXPECT_EQ(output.username.bytes[3], '\0');
    ExpectCanary(&bytes[COOP_PRESENCE_SPAWN_USERNAME_OFFSET + 3], 0,
                 COOP_PRESENCE_USERNAME_MAX - 3);

    spawn.username = Username(interior);
    EXPECT(CoopPresence_EncodeSpawn(&spawn, bytes, sizeof(bytes)));
    EXPECT(CoopPresence_DecodeSpawn(bytes, sizeof(bytes), &output));
    EXPECT_EQ(output.username.length, 7);
    EXPECT(BytesEqual((const u8 *)output.username.bytes,
                      (const u8 *)interior, output.username.length));
    EXPECT_EQ(output.username.bytes[output.username.length], '\0');
    ExpectCanary(&bytes[COOP_PRESENCE_SPAWN_USERNAME_OFFSET + 7], 0,
                 COOP_PRESENCE_USERNAME_MAX - 7);

    spawn.username = Username("abc");
    spawn.username.bytes[4] = 'x';
    EXPECT(CoopPresence_EncodeSpawn(&spawn, bytes, sizeof(bytes)));
    ExpectCanary(&bytes[COOP_PRESENCE_SPAWN_USERNAME_OFFSET + 3], 0,
                 COOP_PRESENCE_USERNAME_MAX - 3);

    spawn.username = Username(full);
    EXPECT_EQ(spawn.username.length, COOP_PRESENCE_USERNAME_MAX);
    EXPECT(CoopPresence_EncodeSpawn(&spawn, bytes, sizeof(bytes)));
    output.username.bytes[COOP_PRESENCE_USERNAME_MAX] = 'x';
    EXPECT(CoopPresence_DecodeSpawn(bytes, sizeof(bytes), &output));
    EXPECT_EQ(output.username.length, COOP_PRESENCE_USERNAME_MAX);
    EXPECT(BytesEqual((const u8 *)output.username.bytes,
                      (const u8 *)full, COOP_PRESENCE_USERNAME_MAX));
    EXPECT_EQ(output.username.bytes[COOP_PRESENCE_USERNAME_MAX], '\0');

    spawn.username = Username("abc");
    EXPECT(CoopPresence_EncodeSpawn(&spawn, bytes, sizeof(bytes)));
    bytes[COOP_PRESENCE_SPAWN_USERNAME_OFFSET + 4] = 'x';
    CopyTestBytes(snapshot, (const u8 *)&output, sizeof(output));
    EXPECT(!CoopPresence_DecodeSpawn(bytes, sizeof(bytes), &output));
    ExpectStructBytes(&output, snapshot, sizeof(output));

    spawn.username = Username("ab-");
    FillTestBytes(canary, 0xa5, sizeof(canary));
    EXPECT(!CoopPresence_EncodeSpawn(&spawn, canary, sizeof(canary)));
    ExpectCanary(canary, 0xa5, sizeof(canary));
    spawn.username = Username("-ab");
    EXPECT(!CoopPresence_EncodeSpawn(&spawn, canary, sizeof(canary)));
    ExpectCanary(canary, 0xa5, sizeof(canary));
    spawn.username = Username("a1_");
    EXPECT(!CoopPresence_EncodeSpawn(&spawn, canary, sizeof(canary)));
    ExpectCanary(canary, 0xa5, sizeof(canary));
}

TEST("Cloud Coop presence decoder rejects each malformed username wire independently")
{
    struct CoopPresenceSpawn spawn = Spawn(1, 1, -4, 9,
                                           COOP_PRESENCE_DIRECTION_EAST,
                                           COOP_PRESENCE_PLAYER_OVERWORLD);
    struct CoopPresenceSpawn output = Spawn(2, 4, 1234, -1234,
                                            COOP_PRESENCE_DIRECTION_NORTH,
                                            COOP_PRESENCE_PLAYER_HIDDEN);
    u8 bytes[COOP_PRESENCE_SPAWN_SIZE];
    u8 snapshot[sizeof(output)];

    spawn.username = Username("abc");
    CopyTestBytes(snapshot, (const u8 *)&output, sizeof(output));

    EXPECT(CoopPresence_EncodeSpawn(&spawn, bytes, sizeof(bytes)));
    bytes[COOP_PRESENCE_SPAWN_USERNAME_OFFSET + 2] = 0;
    EXPECT(!CoopPresence_DecodeSpawn(bytes, sizeof(bytes), &output));
    ExpectStructBytes(&output, snapshot, sizeof(output));

    EXPECT(CoopPresence_EncodeSpawn(&spawn, bytes, sizeof(bytes)));
    bytes[COOP_PRESENCE_SPAWN_USERNAME_OFFSET] = 'A';
    EXPECT(!CoopPresence_DecodeSpawn(bytes, sizeof(bytes), &output));
    ExpectStructBytes(&output, snapshot, sizeof(output));

    EXPECT(CoopPresence_EncodeSpawn(&spawn, bytes, sizeof(bytes)));
    bytes[COOP_PRESENCE_SPAWN_USERNAME_OFFSET] = '-';
    EXPECT(!CoopPresence_DecodeSpawn(bytes, sizeof(bytes), &output));
    ExpectStructBytes(&output, snapshot, sizeof(output));

    EXPECT(CoopPresence_EncodeSpawn(&spawn, bytes, sizeof(bytes)));
    bytes[COOP_PRESENCE_SPAWN_USERNAME_OFFSET + 2] = '_';
    EXPECT(!CoopPresence_DecodeSpawn(bytes, sizeof(bytes), &output));
    ExpectStructBytes(&output, snapshot, sizeof(output));

    EXPECT(CoopPresence_EncodeSpawn(&spawn, bytes, sizeof(bytes)));
    bytes[COOP_PRESENCE_SPAWN_USERNAME_OFFSET + 1] = '/';
    EXPECT(!CoopPresence_DecodeSpawn(bytes, sizeof(bytes), &output));
    ExpectStructBytes(&output, snapshot, sizeof(output));

    EXPECT(CoopPresence_EncodeSpawn(&spawn, bytes, sizeof(bytes)));
    bytes[COOP_PRESENCE_SPAWN_USERNAME_OFFSET + 3] = 0;
    bytes[COOP_PRESENCE_SPAWN_USERNAME_OFFSET + 4] = 'x';
    EXPECT(!CoopPresence_DecodeSpawn(bytes, sizeof(bytes), &output));
    ExpectStructBytes(&output, snapshot, sizeof(output));
}

TEST("Cloud Coop presence keeps regional map identity authoritative")
{
    struct CoopPresenceLocalState state = State(-4, 9,
                                                 COOP_PRESENCE_DIRECTION_EAST,
                                                 COOP_PRESENCE_PLAYER_OVERWORLD);
    struct CoopPresenceLocalState output = state;
    struct CoopPresenceLocalState kanto = state;
    struct CoopPresenceLocalState sevii = state;
    u8 bytes[COOP_PRESENCE_LOCAL_STATE_SIZE];
    u8 snapshot[sizeof(output)];
    u8 canary[COOP_PRESENCE_LOCAL_STATE_SIZE];

    kanto.pose.location.region = COOP_REGION_KANTO;
    kanto.pose.location.map_group = 37;
    kanto.pose.location.map_number = 0;
    EXPECT(CoopPresence_EncodeLocalState(&kanto, bytes, sizeof(bytes)));
    EXPECT(CoopPresence_DecodeLocalState(bytes, sizeof(bytes), &output));
    EXPECT_EQ(output.pose.location.region, COOP_REGION_KANTO);
    EXPECT_EQ(output.pose.location.map_group, 37);
    EXPECT_EQ(output.pose.location.map_number, 0);

    sevii.pose.location.region = COOP_REGION_SEVII;
    sevii.pose.location.map_group = 35;
    sevii.pose.location.map_number = 96;
    EXPECT(CoopPresence_EncodeLocalState(&sevii, bytes, sizeof(bytes)));
    EXPECT(CoopPresence_DecodeLocalState(bytes, sizeof(bytes), &output));
    EXPECT_EQ(output.pose.location.region, COOP_REGION_SEVII);
    EXPECT_EQ(output.pose.location.map_group, 35);
    EXPECT_EQ(output.pose.location.map_number, 96);

    state.pose.location.map_number = 0xffff;
    output = State(1234, -1234, COOP_PRESENCE_DIRECTION_NORTH,
                   COOP_PRESENCE_PLAYER_HIDDEN);
    CopyTestBytes(snapshot, (const u8 *)&output, sizeof(output));
    FillTestBytes(canary, 0xa5, sizeof(canary));
    EXPECT(!CoopPresence_EncodeLocalState(&state, canary, sizeof(canary)));
    ExpectCanary(canary, 0xa5, sizeof(canary));
    EXPECT(CoopPresence_EncodeLocalState(&kanto, bytes, sizeof(bytes)));
    bytes[COOP_PRESENCE_WORLD_LOCATION_MAP_NUMBER_OFFSET] = 0xff;
    bytes[COOP_PRESENCE_WORLD_LOCATION_MAP_NUMBER_OFFSET + 1] = 0xff;
    EXPECT(!CoopPresence_DecodeLocalState(bytes, sizeof(bytes), &output));
    ExpectStructBytes(&output, snapshot, sizeof(output));

    state = State(-4, 9, COOP_PRESENCE_DIRECTION_EAST,
                  COOP_PRESENCE_PLAYER_OVERWORLD);
    state.pose.location.map_number = 5;
    FillTestBytes(canary, 0xa5, sizeof(canary));
    EXPECT(!CoopPresence_EncodeLocalState(&state, canary, sizeof(canary)));
    ExpectCanary(canary, 0xa5, sizeof(canary));
    state = State(-4, 9, COOP_PRESENCE_DIRECTION_EAST,
                  COOP_PRESENCE_PLAYER_OVERWORLD);
    EXPECT(CoopPresence_EncodeLocalState(&state, bytes, sizeof(bytes)));
    bytes[COOP_PRESENCE_WORLD_LOCATION_MAP_NUMBER_OFFSET] = 5;
    bytes[COOP_PRESENCE_WORLD_LOCATION_MAP_NUMBER_OFFSET + 1] = 0;
    EXPECT(!CoopPresence_DecodeLocalState(bytes, sizeof(bytes), &output));
    ExpectStructBytes(&output, snapshot, sizeof(output));
}

TEST("Cloud Coop presence reducer preserves snapshots across rejection classes")
{
    struct CoopPresenceReducer reducer;
    struct CoopPresenceSpawn spawn = Spawn(1, 10, 11, 10,
                                           COOP_PRESENCE_DIRECTION_WEST,
                                           COOP_PRESENCE_PLAYER_OVERWORLD);
    struct CoopPresenceSpawn other = Spawn(2, 10, 11, 10,
                                           COOP_PRESENCE_DIRECTION_WEST,
                                           COOP_PRESENCE_PLAYER_OVERWORLD);
    struct CoopPresenceUpdate update = {
        .handle = 1,
        .server_sequence = 11,
        .state = State(11, 10, COOP_PRESENCE_DIRECTION_WEST,
                       COOP_PRESENCE_PLAYER_OVERWORLD),
    };
    struct CoopPresenceDespawn despawn = {
        .handle = 1,
        .server_sequence = 11,
        .reason = COOP_PRESENCE_DESPAWN_STALE,
    };
    struct WorldLocation location = Location(10, 10);
    struct WorldLocation invalid = location;
    struct WorldLocation moved = location;
    u8 snapshot[sizeof(reducer)];

    spawn.state.pose.warp_sequence = 1;
    other.state.pose.warp_sequence = 1;
    update.state.pose.warp_sequence = 1;
    EstablishReducer(&reducer);

    CopyTestBytes(snapshot, (const u8 *)&reducer, sizeof(reducer));
    EXPECT(!CoopPresenceReducer_Synchronize(&reducer, 0, &location, 1));
    ExpectStructBytes(&reducer, snapshot, sizeof(reducer));
    EXPECT(!CoopPresenceReducer_Synchronize(&reducer, 9, NULL, 1));
    ExpectStructBytes(&reducer, snapshot, sizeof(reducer));
    EXPECT(!CoopPresenceReducer_Synchronize(&reducer, 9, &location, 0));
    ExpectStructBytes(&reducer, snapshot, sizeof(reducer));
    invalid.reserved = 1;
    EXPECT(!CoopPresenceReducer_Synchronize(&reducer, 9, &invalid, 1));
    ExpectStructBytes(&reducer, snapshot, sizeof(reducer));
    invalid = location;
    invalid.map_number = 0xffff;
    EXPECT(!CoopPresenceReducer_Synchronize(&reducer, 9, &invalid, 1));
    ExpectStructBytes(&reducer, snapshot, sizeof(reducer));

    EXPECT_EQ(CoopPresenceReducer_ApplySpawn(&reducer, &spawn),
              COOP_PRESENCE_APPLY_APPLIED);
    CopyTestBytes(snapshot, (const u8 *)&reducer, sizeof(reducer));
    spawn.handle = 0;
    EXPECT_EQ(CoopPresenceReducer_ApplySpawn(&reducer, &spawn),
              COOP_PRESENCE_APPLY_REJECTED);
    ExpectStructBytes(&reducer, snapshot, sizeof(reducer));
    spawn.handle = 1;
    update.state.source_sequence = 0;
    EXPECT_EQ(CoopPresenceReducer_ApplyUpdate(&reducer, &update),
              COOP_PRESENCE_APPLY_REJECTED);
    ExpectStructBytes(&reducer, snapshot, sizeof(reducer));
    update.state.source_sequence = 0x99aabbcc;
    despawn.reason = 0;
    EXPECT_EQ(CoopPresenceReducer_ApplyDespawn(&reducer, &despawn),
              COOP_PRESENCE_APPLY_REJECTED);
    ExpectStructBytes(&reducer, snapshot, sizeof(reducer));
    despawn.reason = COOP_PRESENCE_DESPAWN_STALE;
    EXPECT_EQ(CoopPresenceReducer_ApplySpawn(&reducer, &spawn),
              COOP_PRESENCE_APPLY_STALE);
    ExpectStructBytes(&reducer, snapshot, sizeof(reducer));
    EXPECT_EQ(CoopPresenceReducer_ApplySpawn(&reducer, &other),
              COOP_PRESENCE_APPLY_CAPACITY);
    ExpectStructBytes(&reducer, snapshot, sizeof(reducer));
    EXPECT_EQ(CoopPresenceReducer_ApplyUpdate(&reducer, &update),
              COOP_PRESENCE_APPLY_APPLIED);
    CopyTestBytes(snapshot, (const u8 *)&reducer, sizeof(reducer));
    update.server_sequence = 11;
    EXPECT_EQ(CoopPresenceReducer_ApplyUpdate(&reducer, &update),
              COOP_PRESENCE_APPLY_STALE);
    ExpectStructBytes(&reducer, snapshot, sizeof(reducer));
    update.server_sequence = 10;
    EXPECT_EQ(CoopPresenceReducer_ApplyUpdate(&reducer, &update),
              COOP_PRESENCE_APPLY_STALE);
    ExpectStructBytes(&reducer, snapshot, sizeof(reducer));
    update.server_sequence = 12;
    update.handle = 2;
    EXPECT_EQ(CoopPresenceReducer_ApplyUpdate(&reducer, &update),
              COOP_PRESENCE_APPLY_HANDLE_MISMATCH);
    ExpectStructBytes(&reducer, snapshot, sizeof(reducer));
    update.handle = 1;
    update.state.pose.location.map_group++;
    EXPECT_EQ(CoopPresenceReducer_ApplyUpdate(&reducer, &update),
              COOP_PRESENCE_APPLY_PARTITION_MISMATCH);
    ExpectStructBytes(&reducer, snapshot, sizeof(reducer));
    update.state = State(11, 10, COOP_PRESENCE_DIRECTION_WEST,
                         COOP_PRESENCE_PLAYER_OVERWORLD);
    update.state.pose.warp_sequence = 1;
    despawn.server_sequence = 12;
    despawn.handle = 2;
    EXPECT_EQ(CoopPresenceReducer_ApplyDespawn(&reducer, &despawn),
              COOP_PRESENCE_APPLY_HANDLE_MISMATCH);
    ExpectStructBytes(&reducer, snapshot, sizeof(reducer));
    despawn.handle = 1;
    despawn.server_sequence = 11;
    EXPECT_EQ(CoopPresenceReducer_ApplyDespawn(&reducer, &despawn),
              COOP_PRESENCE_APPLY_STALE);
    ExpectStructBytes(&reducer, snapshot, sizeof(reducer));

    /* An unchanged context must retain the active slot byte-for-byte. */
    EXPECT(CoopPresenceReducer_Synchronize(&reducer, 9, &location, 1));
    ExpectStructBytes(&reducer, snapshot, sizeof(reducer));
    moved.x++;
    moved.y--;
    EXPECT(CoopPresenceReducer_Synchronize(&reducer, 9, &moved, 1));
    ExpectStructBytes(&reducer, snapshot, sizeof(reducer));

    /* Each partition component is changed while a remote is active. */
    EXPECT(CoopPresenceReducer_Synchronize(&reducer, 10, &location, 1));
    ExpectReducerClearedAt(&reducer, 10, &location, 1);
    EstablishReducer(&reducer);
    EXPECT(CoopPresenceReducer_ApplySpawn(&reducer, &spawn) == COOP_PRESENCE_APPLY_APPLIED);
    location.region = COOP_REGION_KANTO;
    location.map_group = 37;
    location.map_number = 0;
    EXPECT(CoopPresenceReducer_Synchronize(&reducer, 9, &location, 1));
    ExpectReducerClearedAt(&reducer, 9, &location, 1);
    EstablishReducer(&reducer);
    EXPECT(CoopPresenceReducer_ApplySpawn(&reducer, &spawn) == COOP_PRESENCE_APPLY_APPLIED);
    location = Location(10, 10);
    location.map_group = 2;
    location.map_number = 0;
    EXPECT(CoopPresenceReducer_Synchronize(&reducer, 9, &location, 1));
    ExpectReducerClearedAt(&reducer, 9, &location, 1);
    EstablishReducer(&reducer);
    EXPECT(CoopPresenceReducer_ApplySpawn(&reducer, &spawn) == COOP_PRESENCE_APPLY_APPLIED);
    location = Location(10, 10);
    location.map_number = 0;
    EXPECT(CoopPresenceReducer_Synchronize(&reducer, 9, &location, 1));
    ExpectReducerClearedAt(&reducer, 9, &location, 1);
    EstablishReducer(&reducer);
    EXPECT(CoopPresenceReducer_ApplySpawn(&reducer, &spawn) == COOP_PRESENCE_APPLY_APPLIED);
    location = Location(10, 10);
    EXPECT(CoopPresenceReducer_Synchronize(&reducer, 9, &location, 2));
    ExpectReducerClearedAt(&reducer, 9, &location, 2);

    EstablishReducer(&reducer);
    EXPECT(CoopPresenceReducer_ApplySpawn(&reducer, &spawn) == COOP_PRESENCE_APPLY_APPLIED);
    CoopPresenceReducer_Reset(&reducer);
    ExpectReducerResetState(&reducer);
}

TEST("Cloud Coop presence interaction decodes all facings and rejects context traps")
{
    struct CoopPresenceReducer reducer;
    struct CoopPresenceSpawn spawn = Spawn(1, 10, 10, 11,
                                           COOP_PRESENCE_DIRECTION_NORTH,
                                           COOP_PRESENCE_PLAYER_OVERWORLD);
    struct CoopPresenceLocalContext local = {
        .session_epoch = 9,
        .location = Location(10, 10),
        .elevation = 7,
        .direction = COOP_PRESENCE_DIRECTION_SOUTH,
        .warp_sequence = 1,
    };
    struct CoopPresenceInteraction decoded;
    u8 bytes[COOP_PRESENCE_INTERACTION_SIZE];
    u32 nextSequence = 11;
    u8 direction;

    EstablishReducer(&reducer);
    spawn.state.pose.warp_sequence = 1;
    EXPECT(CoopPresenceReducer_ApplySpawn(&reducer, &spawn)
           == COOP_PRESENCE_APPLY_APPLIED);

    for (direction = COOP_PRESENCE_DIRECTION_SOUTH;
         direction <= COOP_PRESENCE_DIRECTION_EAST; direction++)
    {
        struct CoopPresenceUpdate update = {
            .handle = 1,
            .server_sequence = nextSequence,
            .state = spawn.state,
        };

        local.direction = direction;
        update.state.pose.location.x = local.location.x;
        update.state.pose.location.y = local.location.y;
        switch (direction)
        {
        case COOP_PRESENCE_DIRECTION_SOUTH:
            update.state.pose.location.y++;
            break;
        case COOP_PRESENCE_DIRECTION_NORTH:
            update.state.pose.location.y--;
            break;
        case COOP_PRESENCE_DIRECTION_WEST:
            update.state.pose.location.x--;
            break;
        case COOP_PRESENCE_DIRECTION_EAST:
            update.state.pose.location.x++;
            break;
        }
        EXPECT(CoopPresenceReducer_ApplyUpdate(&reducer, &update)
               == COOP_PRESENCE_APPLY_APPLIED);
        EXPECT(CoopPresence_EncodeInteraction(&reducer, &local, bytes,
                                               sizeof(bytes)));
        EXPECT(CoopPresence_DecodeInteraction(bytes, sizeof(bytes), &decoded));
        EXPECT_EQ(decoded.handle, 1);
        EXPECT_EQ(decoded.observed_server_sequence, nextSequence);
        EXPECT_EQ(decoded.observed_warp_sequence, 1);
        EXPECT_EQ(decoded.x, update.state.pose.location.x);
        EXPECT_EQ(decoded.y, update.state.pose.location.y);
        nextSequence++;
    }

    /* Every context and visibility mismatch preserves the destination. */
    local.direction = COOP_PRESENCE_DIRECTION_SOUTH;
    spawn.state.pose.location.x = 10;
    spawn.state.pose.location.y = 11;
    nextSequence++;
    EXPECT(CoopPresenceReducer_ApplyUpdate(&reducer,
                                           &(struct CoopPresenceUpdate){
                                               .handle = 1,
                                               .server_sequence = nextSequence,
                                               .state = spawn.state,
                                           }) == COOP_PRESENCE_APPLY_APPLIED);
    local.direction = 0;
    ExpectInteractionRejected(&reducer, &local, bytes, 0x30);
    local.direction = 5;
    ExpectInteractionRejected(&reducer, &local, bytes, 0x30);
    local.direction = COOP_PRESENCE_DIRECTION_SOUTH;
    local.direction = COOP_PRESENCE_DIRECTION_NORTH;
    ExpectInteractionRejected(&reducer, &local, bytes, 0x31);
    local.direction = COOP_PRESENCE_DIRECTION_SOUTH;

    local.session_epoch = 10;
    ExpectInteractionRejected(&reducer, &local, bytes, 0x32);
    local.session_epoch = 9;
    local.location.region = COOP_REGION_KANTO;
    local.location.map_group = 37;
    local.location.map_number = 0;
    ExpectInteractionRejected(&reducer, &local, bytes, 0x33);
    local.location.region = COOP_REGION_HOENN;
    local.location.map_group = 2;
    local.location.map_number = 0;
    ExpectInteractionRejected(&reducer, &local, bytes, 0x34);
    local.location.map_group = 1;
    local.location.map_number = 0;
    ExpectInteractionRejected(&reducer, &local, bytes, 0x35);
    local.location.map_number = 3;
    local.warp_sequence = 2;
    ExpectInteractionRejected(&reducer, &local, bytes, 0x36);
    local.warp_sequence = 1;
    local.elevation = 8;
    ExpectInteractionRejected(&reducer, &local, bytes, 0x37);
    local.elevation = 7;

    CoopPresenceReducer_Reset(&reducer);
    ExpectInteractionRejected(&reducer, &local, bytes, 0x38);
    EstablishReducer(&reducer);
    spawn.state.pose.location.x = 10;
    spawn.state.pose.location.y = 11;
    spawn.state.pose.player_state = COOP_PRESENCE_PLAYER_HIDDEN;
    spawn.server_sequence = 1;
    EXPECT(CoopPresenceReducer_ApplySpawn(&reducer, &spawn)
           == COOP_PRESENCE_APPLY_APPLIED);
    EXPECT(CoopPresenceReducer_IsActive(&reducer));
    EXPECT(!CoopPresenceReducer_IsVisible(&reducer));
    ExpectInteractionRejected(&reducer, &local, bytes, 0x39);
}

TEST("Cloud Coop presence interaction rejects signed-coordinate overflow at every edge")
{
    struct CoopPresenceReducer reducer;
    struct CoopPresenceSpawn spawn = Spawn(1, 10, 10, 11,
                                           COOP_PRESENCE_DIRECTION_NORTH,
                                           COOP_PRESENCE_PLAYER_OVERWORLD);
    struct CoopPresenceLocalContext local = {
        .session_epoch = 9,
        .location = Location(10, 10),
        .elevation = 7,
        .direction = COOP_PRESENCE_DIRECTION_EAST,
        .warp_sequence = 1,
    };
    struct CoopPresenceUpdate update;
    u8 bytes[COOP_PRESENCE_INTERACTION_SIZE];
    u32 sequence = 11;

    EstablishReducer(&reducer);
    spawn.state.pose.warp_sequence = 1;
    EXPECT(CoopPresenceReducer_ApplySpawn(&reducer, &spawn)
           == COOP_PRESENCE_APPLY_APPLIED);
    update.handle = 1;
    update.state = spawn.state;

    update.server_sequence = sequence++;
    update.state.pose.location.x = -32768;
    update.state.pose.location.y = 10;
    EXPECT(CoopPresenceReducer_ApplyUpdate(&reducer, &update)
           == COOP_PRESENCE_APPLY_APPLIED);
    local.location.x = 32767;
    local.location.y = 10;
    local.direction = COOP_PRESENCE_DIRECTION_EAST;
    ExpectInteractionRejected(&reducer, &local, bytes, 0x41);

    update.server_sequence = sequence++;
    update.state.pose.location.x = 32767;
    update.state.pose.location.y = 10;
    EXPECT(CoopPresenceReducer_ApplyUpdate(&reducer, &update)
           == COOP_PRESENCE_APPLY_APPLIED);
    local.location.x = -32768;
    local.direction = COOP_PRESENCE_DIRECTION_WEST;
    ExpectInteractionRejected(&reducer, &local, bytes, 0x42);

    update.server_sequence = sequence++;
    update.state.pose.location.x = 10;
    update.state.pose.location.y = -32768;
    EXPECT(CoopPresenceReducer_ApplyUpdate(&reducer, &update)
           == COOP_PRESENCE_APPLY_APPLIED);
    local.location.x = 10;
    local.location.y = 32767;
    local.direction = COOP_PRESENCE_DIRECTION_SOUTH;
    ExpectInteractionRejected(&reducer, &local, bytes, 0x43);

    update.server_sequence = sequence++;
    update.state.pose.location.x = 10;
    update.state.pose.location.y = 32767;
    EXPECT(CoopPresenceReducer_ApplyUpdate(&reducer, &update)
           == COOP_PRESENCE_APPLY_APPLIED);
    local.location.y = -32768;
    local.direction = COOP_PRESENCE_DIRECTION_NORTH;
    ExpectInteractionRejected(&reducer, &local, bytes, 0x44);
}

TEST("Cloud Coop presence encoder validation is atomic for every payload")
{
    struct CoopPresencePose pose = Pose(-4, 9, COOP_PRESENCE_DIRECTION_EAST,
                                        COOP_PRESENCE_PLAYER_OVERWORLD);
    struct CoopPresenceLocalState state = State(-4, 9,
                                                 COOP_PRESENCE_DIRECTION_EAST,
                                                 COOP_PRESENCE_PLAYER_OVERWORLD);
    struct CoopPresenceSpawn spawn = Spawn(1, 1, -4, 9,
                                           COOP_PRESENCE_DIRECTION_EAST,
                                           COOP_PRESENCE_PLAYER_OVERWORLD);
    struct CoopPresenceUpdate update = {
        .handle = 1,
        .server_sequence = 1,
        .state = state,
    };
    struct CoopPresenceDespawn despawn = {
        .handle = 1,
        .server_sequence = 1,
        .reason = COOP_PRESENCE_DESPAWN_HIDDEN,
    };
    struct CoopPresenceInteraction interaction = {
        .handle = 1,
        .observed_server_sequence = 1,
        .observed_warp_sequence = 1,
        .x = -4,
        .y = 9,
    };
    u8 bytes[COOP_PRESENCE_SPAWN_SIZE];

    FillTestBytes(bytes, 0xa5, sizeof(bytes));
    pose.direction = 0;
    EXPECT(!CoopPresence_EncodePose(&pose, bytes, COOP_PRESENCE_POSE_SIZE));
    ExpectCanary(bytes, 0xa5, sizeof(bytes));
    pose.direction = COOP_PRESENCE_DIRECTION_EAST;
    state.pose.warp_sequence = 0;
    EXPECT(!CoopPresence_EncodeLocalState(&state, bytes,
                                          COOP_PRESENCE_LOCAL_STATE_SIZE));
    ExpectCanary(bytes, 0xa5, sizeof(bytes));
    state.pose.warp_sequence = 0x55667788;
    spawn.handle = 0;
    EXPECT(!CoopPresence_EncodeSpawn(&spawn, bytes, COOP_PRESENCE_SPAWN_SIZE));
    ExpectCanary(bytes, 0xa5, sizeof(bytes));
    spawn.handle = 1;
    update.state.pose.player_state = 2;
    EXPECT(!CoopPresence_EncodeUpdate(&update, bytes, COOP_PRESENCE_UPDATE_SIZE));
    ExpectCanary(bytes, 0xa5, sizeof(bytes));
    update.state.pose.player_state = COOP_PRESENCE_PLAYER_OVERWORLD;
    despawn.reason = 0;
    EXPECT(!CoopPresence_EncodeDespawn(&despawn, bytes, COOP_PRESENCE_DESPAWN_SIZE));
    ExpectCanary(bytes, 0xa5, sizeof(bytes));
    despawn.reason = COOP_PRESENCE_DESPAWN_HIDDEN;
    despawn.handle = 0;
    EXPECT(!CoopPresence_EncodeDespawn(&despawn, bytes, COOP_PRESENCE_DESPAWN_SIZE));
    ExpectCanary(bytes, 0xa5, sizeof(bytes));
    despawn.handle = 1;
    despawn.server_sequence = 0;
    EXPECT(!CoopPresence_EncodeDespawn(&despawn, bytes, COOP_PRESENCE_DESPAWN_SIZE));
    ExpectCanary(bytes, 0xa5, sizeof(bytes));
    despawn.server_sequence = 1;
    interaction.observed_warp_sequence = 0;
    EXPECT(!EncodeInteractionFixture(&interaction, bytes,
                                     COOP_PRESENCE_INTERACTION_SIZE));
    ExpectCanary(bytes, 0xa5, sizeof(bytes));
}

TEST("Cloud Coop presence decoders reject zero handles sequences and reasons")
{
    struct CoopPresenceSpawn spawn = Spawn(1, 1, -4, 9,
                                           COOP_PRESENCE_DIRECTION_EAST,
                                           COOP_PRESENCE_PLAYER_OVERWORLD);
    struct CoopPresenceSpawn spawnOutput = spawn;
    struct CoopPresenceUpdate update = {
        .handle = 1,
        .server_sequence = 1,
        .state = State(-4, 9, COOP_PRESENCE_DIRECTION_EAST,
                       COOP_PRESENCE_PLAYER_OVERWORLD),
    };
    struct CoopPresenceUpdate updateOutput = update;
    struct CoopPresenceDespawn despawn = {
        .handle = 1,
        .server_sequence = 1,
        .reason = COOP_PRESENCE_DESPAWN_HIDDEN,
    };
    struct CoopPresenceDespawn despawnOutput = despawn;
    struct CoopPresenceInteraction interaction = {
        .handle = 1,
        .observed_server_sequence = 1,
        .observed_warp_sequence = 1,
        .x = -4,
        .y = 9,
    };
    struct CoopPresenceInteraction interactionOutput = interaction;
    u8 spawnSnapshot[sizeof(spawnOutput)];
    u8 updateSnapshot[sizeof(updateOutput)];
    u8 despawnSnapshot[sizeof(despawnOutput)];
    u8 interactionSnapshot[sizeof(interactionOutput)];
    u8 bytes[COOP_PRESENCE_SPAWN_SIZE];
    u32 i;

    CopyTestBytes(spawnSnapshot, (const u8 *)&spawnOutput, sizeof(spawnOutput));
    CopyTestBytes(updateSnapshot, (const u8 *)&updateOutput, sizeof(updateOutput));
    CopyTestBytes(despawnSnapshot, (const u8 *)&despawnOutput, sizeof(despawnOutput));
    CopyTestBytes(interactionSnapshot, (const u8 *)&interactionOutput,
                  sizeof(interactionOutput));

    for (i = COOP_PRESENCE_DESPAWN_HIDDEN;
         i <= COOP_PRESENCE_DESPAWN_PARTITION_LEFT; i++)
    {
        despawn.reason = i;
        EXPECT(CoopPresence_EncodeDespawn(&despawn, bytes,
                                          COOP_PRESENCE_DESPAWN_SIZE));
        EXPECT(CoopPresence_DecodeDespawn(bytes, COOP_PRESENCE_DESPAWN_SIZE,
                                          &despawnOutput));
        EXPECT_EQ(despawnOutput.reason, i);
    }
    despawn.reason = COOP_PRESENCE_DESPAWN_HIDDEN;
    despawnOutput = (struct CoopPresenceDespawn) {
        .handle = 0xfedcba9876543210ULL,
        .server_sequence = 0x12345678,
        .reason = COOP_PRESENCE_DESPAWN_STALE,
    };
    CopyTestBytes(despawnSnapshot, (const u8 *)&despawnOutput,
                  sizeof(despawnOutput));

    EXPECT(CoopPresence_EncodeSpawn(&spawn, bytes, COOP_PRESENCE_SPAWN_SIZE));
    for (i = 0; i < sizeof(u64); i++)
        bytes[COOP_PRESENCE_SPAWN_HANDLE_OFFSET + i] = 0;
    EXPECT(!CoopPresence_DecodeSpawn(bytes, COOP_PRESENCE_SPAWN_SIZE,
                                     &spawnOutput));
    ExpectStructBytes(&spawnOutput, spawnSnapshot, sizeof(spawnOutput));
    EXPECT(CoopPresence_EncodeSpawn(&spawn, bytes, COOP_PRESENCE_SPAWN_SIZE));
    for (i = 0; i < sizeof(u32); i++)
        bytes[COOP_PRESENCE_SPAWN_SERVER_SEQUENCE_OFFSET + i] = 0;
    EXPECT(!CoopPresence_DecodeSpawn(bytes, COOP_PRESENCE_SPAWN_SIZE,
                                     &spawnOutput));
    ExpectStructBytes(&spawnOutput, spawnSnapshot, sizeof(spawnOutput));

    EXPECT(CoopPresence_EncodeUpdate(&update, bytes, COOP_PRESENCE_UPDATE_SIZE));
    for (i = 0; i < sizeof(u64); i++)
        bytes[COOP_PRESENCE_UPDATE_HANDLE_OFFSET + i] = 0;
    EXPECT(!CoopPresence_DecodeUpdate(bytes, COOP_PRESENCE_UPDATE_SIZE,
                                      &updateOutput));
    ExpectStructBytes(&updateOutput, updateSnapshot, sizeof(updateOutput));
    EXPECT(CoopPresence_EncodeUpdate(&update, bytes, COOP_PRESENCE_UPDATE_SIZE));
    for (i = 0; i < sizeof(u32); i++)
        bytes[COOP_PRESENCE_UPDATE_SERVER_SEQUENCE_OFFSET + i] = 0;
    EXPECT(!CoopPresence_DecodeUpdate(bytes, COOP_PRESENCE_UPDATE_SIZE,
                                      &updateOutput));
    ExpectStructBytes(&updateOutput, updateSnapshot, sizeof(updateOutput));

    EXPECT(CoopPresence_EncodeDespawn(&despawn, bytes, COOP_PRESENCE_DESPAWN_SIZE));
    bytes[COOP_PRESENCE_DESPAWN_REASON_OFFSET] = 0;
    EXPECT(!CoopPresence_DecodeDespawn(bytes, COOP_PRESENCE_DESPAWN_SIZE,
                                       &despawnOutput));
    ExpectStructBytes(&despawnOutput, despawnSnapshot, sizeof(despawnOutput));
    EXPECT(CoopPresence_EncodeDespawn(&despawn, bytes, COOP_PRESENCE_DESPAWN_SIZE));
    bytes[COOP_PRESENCE_DESPAWN_REASON_OFFSET] = 7;
    EXPECT(!CoopPresence_DecodeDespawn(bytes, COOP_PRESENCE_DESPAWN_SIZE,
                                       &despawnOutput));
    ExpectStructBytes(&despawnOutput, despawnSnapshot, sizeof(despawnOutput));

    EXPECT(EncodeInteractionFixture(&interaction, bytes,
                                    COOP_PRESENCE_INTERACTION_SIZE));
    for (i = 0; i < sizeof(u32); i++)
        bytes[COOP_PRESENCE_INTERACTION_SERVER_SEQUENCE_OFFSET + i] = 0;
    EXPECT(!CoopPresence_DecodeInteraction(bytes, COOP_PRESENCE_INTERACTION_SIZE,
                                           &interactionOutput));
    ExpectStructBytes(&interactionOutput, interactionSnapshot,
                      sizeof(interactionOutput));
    EXPECT(EncodeInteractionFixture(&interaction, bytes,
                                    COOP_PRESENCE_INTERACTION_SIZE));
    for (i = 0; i < sizeof(u32); i++)
        bytes[COOP_PRESENCE_INTERACTION_WARP_SEQUENCE_OFFSET + i] = 0;
    EXPECT(!CoopPresence_DecodeInteraction(bytes, COOP_PRESENCE_INTERACTION_SIZE,
                                           &interactionOutput));
    ExpectStructBytes(&interactionOutput, interactionSnapshot,
                      sizeof(interactionOutput));
}

TEST("Cloud Coop presence rejects each zero identity field independently")
{
    struct CoopPresencePose pose = Pose(-4, 9, COOP_PRESENCE_DIRECTION_EAST,
                                        COOP_PRESENCE_PLAYER_OVERWORLD);
    struct CoopPresencePose poseOutput = Pose(1234, -1234,
                                              COOP_PRESENCE_DIRECTION_NORTH,
                                              COOP_PRESENCE_PLAYER_HIDDEN);
    struct CoopPresenceLocalState state = State(-4, 9,
                                                 COOP_PRESENCE_DIRECTION_EAST,
                                                 COOP_PRESENCE_PLAYER_OVERWORLD);
    struct CoopPresenceLocalState stateOutput = State(1234, -1234,
                                                       COOP_PRESENCE_DIRECTION_NORTH,
                                                       COOP_PRESENCE_PLAYER_HIDDEN);
    struct CoopPresenceSpawn spawn = Spawn(0x0123456789abcdefULL, 3, -4, 9,
                                           COOP_PRESENCE_DIRECTION_EAST,
                                           COOP_PRESENCE_PLAYER_OVERWORLD);
    struct CoopPresenceSpawn spawnOutput = Spawn(2, 4, 1234, -1234,
                                                 COOP_PRESENCE_DIRECTION_NORTH,
                                                 COOP_PRESENCE_PLAYER_HIDDEN);
    struct CoopPresenceUpdate update = {
        .handle = 0x0123456789abcdefULL,
        .server_sequence = 3,
        .state = state,
    };
    struct CoopPresenceUpdate updateOutput = {
        .handle = 2,
        .server_sequence = 4,
        .state = stateOutput,
    };
    struct CoopPresenceDespawn despawn = {
        .handle = 0x0123456789abcdefULL,
        .server_sequence = 3,
        .reason = COOP_PRESENCE_DESPAWN_HIDDEN,
    };
    struct CoopPresenceDespawn despawnOutput = {
        .handle = 2,
        .server_sequence = 4,
        .reason = COOP_PRESENCE_DESPAWN_STALE,
    };
    struct CoopPresenceInteraction interaction = {
        .handle = 0x0123456789abcdefULL,
        .observed_server_sequence = 3,
        .observed_warp_sequence = 2,
        .x = -4,
        .y = 9,
    };
    struct CoopPresenceInteraction interactionOutput = {
        .handle = 2,
        .observed_server_sequence = 4,
        .observed_warp_sequence = 3,
        .x = 1234,
        .y = -1234,
    };
    u8 poseBytes[COOP_PRESENCE_POSE_SIZE];
    u8 localBytes[COOP_PRESENCE_LOCAL_STATE_SIZE];
    u8 spawnBytes[COOP_PRESENCE_SPAWN_SIZE];
    u8 updateBytesLocal[COOP_PRESENCE_UPDATE_SIZE];
    u8 despawnBytes[COOP_PRESENCE_DESPAWN_SIZE];
    u8 interactionBytesLocal[COOP_PRESENCE_INTERACTION_SIZE];
    u8 poseSnapshot[sizeof(poseOutput)];
    u8 localSnapshot[sizeof(stateOutput)];
    u8 spawnSnapshot[sizeof(spawnOutput)];
    u8 updateSnapshot[sizeof(updateOutput)];
    u8 despawnSnapshot[sizeof(despawnOutput)];
    u8 interactionSnapshot[sizeof(interactionOutput)];

    CopyTestBytes(poseSnapshot, (const u8 *)&poseOutput, sizeof(poseOutput));
    CopyTestBytes(localSnapshot, (const u8 *)&stateOutput, sizeof(stateOutput));
    CopyTestBytes(spawnSnapshot, (const u8 *)&spawnOutput, sizeof(spawnOutput));
    CopyTestBytes(updateSnapshot, (const u8 *)&updateOutput, sizeof(updateOutput));
    CopyTestBytes(despawnSnapshot, (const u8 *)&despawnOutput, sizeof(despawnOutput));
    CopyTestBytes(interactionSnapshot, (const u8 *)&interactionOutput,
                  sizeof(interactionOutput));

    EXPECT(CoopPresence_EncodePose(&pose, poseBytes, sizeof(poseBytes)));
    RejectPoseWord(poseBytes, COOP_PRESENCE_POSE_WARP_SEQUENCE_OFFSET,
                   &poseOutput, poseSnapshot);
    EXPECT(CoopPresence_EncodeLocalState(&state, localBytes, sizeof(localBytes)));
    RejectLocalWord(localBytes, COOP_PRESENCE_POSE_WARP_SEQUENCE_OFFSET,
                    &stateOutput, localSnapshot);
    RejectLocalWord(localBytes, COOP_PRESENCE_LOCAL_STATE_SOURCE_SEQUENCE_OFFSET,
                    &stateOutput, localSnapshot);

    EXPECT(CoopPresence_EncodeSpawn(&spawn, spawnBytes, sizeof(spawnBytes)));
    RejectSpawnField(spawnBytes, COOP_PRESENCE_SPAWN_HANDLE_OFFSET, sizeof(u64),
                     &spawnOutput, spawnSnapshot);
    RejectSpawnField(spawnBytes, COOP_PRESENCE_SPAWN_SERVER_SEQUENCE_OFFSET,
                     sizeof(u32), &spawnOutput, spawnSnapshot);
    RejectSpawnField(spawnBytes, COOP_PRESENCE_SPAWN_STATE_OFFSET
                     + COOP_PRESENCE_POSE_WARP_SEQUENCE_OFFSET, sizeof(u32),
                     &spawnOutput, spawnSnapshot);
    RejectSpawnField(spawnBytes, COOP_PRESENCE_SPAWN_STATE_OFFSET
                     + COOP_PRESENCE_LOCAL_STATE_SOURCE_SEQUENCE_OFFSET,
                     sizeof(u32), &spawnOutput, spawnSnapshot);

    EXPECT(CoopPresence_EncodeUpdate(&update, updateBytesLocal,
                                     sizeof(updateBytesLocal)));
    RejectUpdateField(updateBytesLocal, COOP_PRESENCE_UPDATE_HANDLE_OFFSET,
                      sizeof(u64), &updateOutput, updateSnapshot);
    RejectUpdateField(updateBytesLocal, COOP_PRESENCE_UPDATE_SERVER_SEQUENCE_OFFSET,
                      sizeof(u32), &updateOutput, updateSnapshot);
    RejectUpdateField(updateBytesLocal, COOP_PRESENCE_UPDATE_STATE_OFFSET
                      + COOP_PRESENCE_POSE_WARP_SEQUENCE_OFFSET, sizeof(u32),
                      &updateOutput, updateSnapshot);

    EXPECT(CoopPresence_EncodeDespawn(&despawn, despawnBytes,
                                      sizeof(despawnBytes)));
    RejectDespawnField(despawnBytes, COOP_PRESENCE_DESPAWN_HANDLE_OFFSET,
                       sizeof(u64), &despawnOutput, despawnSnapshot);
    RejectDespawnField(despawnBytes, COOP_PRESENCE_DESPAWN_SERVER_SEQUENCE_OFFSET,
                       sizeof(u32), &despawnOutput, despawnSnapshot);

    EXPECT(EncodeInteractionFixture(&interaction, interactionBytesLocal,
                                    sizeof(interactionBytesLocal)));
    RejectInteractionField(interactionBytesLocal,
                           COOP_PRESENCE_INTERACTION_HANDLE_OFFSET, sizeof(u64),
                           &interactionOutput, interactionSnapshot);
    RejectInteractionField(interactionBytesLocal,
                           COOP_PRESENCE_INTERACTION_SERVER_SEQUENCE_OFFSET,
                           sizeof(u32), &interactionOutput, interactionSnapshot);
    RejectInteractionField(interactionBytesLocal,
                           COOP_PRESENCE_INTERACTION_WARP_SEQUENCE_OFFSET,
                           sizeof(u32), &interactionOutput, interactionSnapshot);
}
