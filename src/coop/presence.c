#include "global.h"
#include "coop/presence.h"
#include "constants/map_groups.h"
#include "../data/map_group_count.h"
#include "overworld.h"

/* mapjson emits one dense count per generated map group plus a zero sentinel.
 * Validate both indexes before calling the engine's unchecked gMapGroups
 * accessor.  The MapHeader then remains authoritative for Kanto/Sevii. */
_Static_assert(sizeof(MAP_GROUP_COUNT) / sizeof(MAP_GROUP_COUNT[0]) == MAP_GROUPS_COUNT + 1,
               "generated presence map group counts");

static u16 ReadLe16(const u8 *bytes)
{
    return (u16)bytes[0] | ((u16)bytes[1] << 8);
}

static s16 ReadLeS16(const u8 *bytes)
{
    return (s16)ReadLe16(bytes);
}

static u32 ReadLe32(const u8 *bytes)
{
    return (u32)bytes[0]
         | ((u32)bytes[1] << 8)
         | ((u32)bytes[2] << 16)
         | ((u32)bytes[3] << 24);
}

static u64 ReadLe64(const u8 *bytes)
{
    return (u64)bytes[0]
         | ((u64)bytes[1] << 8)
         | ((u64)bytes[2] << 16)
         | ((u64)bytes[3] << 24)
         | ((u64)bytes[4] << 32)
         | ((u64)bytes[5] << 40)
         | ((u64)bytes[6] << 48)
         | ((u64)bytes[7] << 56);
}

static void WriteLe16(u8 *bytes, u16 value)
{
    bytes[0] = (u8)value;
    bytes[1] = (u8)(value >> 8);
}

static void WriteLeS16(u8 *bytes, s16 value)
{
    WriteLe16(bytes, (u16)value);
}

static void WriteLe32(u8 *bytes, u32 value)
{
    bytes[0] = (u8)value;
    bytes[1] = (u8)(value >> 8);
    bytes[2] = (u8)(value >> 16);
    bytes[3] = (u8)(value >> 24);
}

static void WriteLe64(u8 *bytes, u64 value)
{
    bytes[0] = (u8)value;
    bytes[1] = (u8)(value >> 8);
    bytes[2] = (u8)(value >> 16);
    bytes[3] = (u8)(value >> 24);
    bytes[4] = (u8)(value >> 32);
    bytes[5] = (u8)(value >> 40);
    bytes[6] = (u8)(value >> 48);
    bytes[7] = (u8)(value >> 56);
}

static void CopyBytes(u8 *destination, const u8 *source, u32 count)
{
    u32 i;

    for (i = 0; i < count; i++)
        destination[i] = source[i];
}

static bool8 IsAlphanumeric(u8 value)
{
    return (value >= 'a' && value <= 'z')
        || (value >= '0' && value <= '9');
}

static bool8 IsUsernameByte(u8 value)
{
    return IsAlphanumeric(value) || value == '_'
        || value == '.' || value == '-';
}

static bool8 ValidateUsername(const struct CoopPresenceUsername *username)
{
    u8 i;

    if (username == NULL || username->length < 3
     || username->length > COOP_PRESENCE_USERNAME_MAX
     || !IsAlphanumeric((u8)username->bytes[0])
     || !IsAlphanumeric((u8)username->bytes[username->length - 1]))
        return FALSE;

    for (i = 0; i < username->length; i++)
    {
        if (!IsUsernameByte((u8)username->bytes[i]))
            return FALSE;
    }

    if (username->bytes[username->length] != '\0')
        return FALSE;
    return TRUE;
}

static bool8 DecodeUsername(const u8 *bytes, struct CoopPresenceUsername *out)
{
    struct CoopPresenceUsername candidate = {0};
    u8 i;
    u8 length = COOP_PRESENCE_USERNAME_MAX;
    bool8 terminated = FALSE;

    if (bytes == NULL || out == NULL)
        return FALSE;

    for (i = 0; i < COOP_PRESENCE_USERNAME_MAX; i++)
    {
        if (bytes[i] == 0)
        {
            if (i < 3)
                return FALSE;
            length = i;
            terminated = TRUE;
            break;
        }
        if (!IsUsernameByte(bytes[i]) || (i == 0 && !IsAlphanumeric(bytes[i])))
            return FALSE;
    }

    if (length < 3 || !IsAlphanumeric(bytes[0])
     || !IsAlphanumeric(bytes[length - 1]))
        return FALSE;

    if (terminated)
    {
        for (i = (u8)(length + 1); i < COOP_PRESENCE_USERNAME_MAX; i++)
        {
            if (bytes[i] != 0)
                return FALSE;
        }
    }

    candidate.length = length;
    for (i = 0; i < length; i++)
        candidate.bytes[i] = (char)bytes[i];
    candidate.bytes[length] = '\0';
    *out = candidate;
    return TRUE;
}

static bool8 IsDirection(u8 value)
{
    return value >= COOP_PRESENCE_DIRECTION_SOUTH
        && value <= COOP_PRESENCE_DIRECTION_EAST;
}

static bool8 IsMovementMode(u8 value)
{
    return value <= COOP_PRESENCE_MOVEMENT_RUN;
}

static bool8 IsAnimationId(u8 value)
{
    return value <= COOP_PRESENCE_ANIMATION_LOCOMOTION;
}

static bool8 IsAvatarId(u8 value)
{
    return value == COOP_PRESENCE_AVATAR_BRENDAN
        || value == COOP_PRESENCE_AVATAR_MAY;
}

static bool8 IsPlayerState(u8 value)
{
    return value <= COOP_PRESENCE_PLAYER_OVERWORLD;
}

static bool8 IsDespawnReason(u8 value)
{
    return value >= COOP_PRESENCE_DESPAWN_HIDDEN
        && value <= COOP_PRESENCE_DESPAWN_PARTITION_LEFT;
}

static bool8 TryResolveLocationRegion(enum CoopRegion *out, u16 mapGroup, u16 mapNumber)
{
    const struct MapHeader *mapHeader;
    enum Region engineRegion;

    if (out == NULL || mapGroup >= MAP_GROUPS_COUNT
     || mapNumber >= MAP_GROUP_COUNT[mapGroup])
        return FALSE;

    mapHeader = Overworld_GetMapHeaderByGroupAndId(mapGroup, mapNumber);
    if (mapHeader == NULL)
        return FALSE;

    switch (mapHeader->engineRegion)
    {
    case COOP_MAP_ENGINE_REGION_HOENN:
        engineRegion = REGION_HOENN;
        break;
    case COOP_MAP_ENGINE_REGION_KANTO:
        engineRegion = REGION_KANTO;
        break;
    default:
        return FALSE;
    }

    return CoopRegion_Normalize(out, engineRegion, mapHeader->regionMapSectionId);
}

static bool8 IsLocationValid(const struct WorldLocation *location)
{
    enum CoopRegion resolvedRegion;

    return location != NULL && location->reserved == 0
        && CoopRegion_IsValid((enum CoopRegion)location->region)
        && TryResolveLocationRegion(&resolvedRegion, location->map_group,
                                    location->map_number)
        && resolvedRegion == (enum CoopRegion)location->region;
}

static bool8 IsPoseValid(const struct CoopPresencePose *pose)
{
    return pose != NULL
        && IsLocationValid(&pose->location)
        && IsDirection(pose->direction)
        && pose->warp_sequence != 0
        && IsMovementMode(pose->movement_mode)
        && IsAnimationId(pose->animation_id)
        && IsAvatarId(pose->avatar_id)
        && IsPlayerState(pose->player_state);
}

static bool8 IsLocalStateValid(const struct CoopPresenceLocalState *state)
{
    return state != NULL && IsPoseValid(&state->pose)
        && state->source_sequence != 0;
}

static bool8 IsHandleValid(u64 handle)
{
    return handle != 0;
}

static bool8 IsSequenceValid(u32 sequence)
{
    return sequence != 0;
}

static bool8 IsExactLength(const u8 *bytes, u32 length, u32 expected)
{
    return bytes != NULL && length == expected;
}

static bool8 DecodeLocation(const u8 *bytes, struct WorldLocation *out)
{
    struct WorldLocation candidate;

    if (bytes == NULL || out == NULL || bytes[COOP_PRESENCE_WORLD_LOCATION_RESERVED_OFFSET] != 0)
        return FALSE;

    candidate.region = bytes[COOP_PRESENCE_WORLD_LOCATION_REGION_OFFSET];
    candidate.reserved = 0;
    candidate.map_group = ReadLe16(&bytes[COOP_PRESENCE_WORLD_LOCATION_MAP_GROUP_OFFSET]);
    candidate.map_number = ReadLe16(&bytes[COOP_PRESENCE_WORLD_LOCATION_MAP_NUMBER_OFFSET]);
    candidate.x = ReadLeS16(&bytes[COOP_PRESENCE_WORLD_LOCATION_X_OFFSET]);
    candidate.y = ReadLeS16(&bytes[COOP_PRESENCE_WORLD_LOCATION_Y_OFFSET]);
    if (!IsLocationValid(&candidate))
        return FALSE;
    *out = candidate;
    return TRUE;
}

static void EncodeLocation(const struct WorldLocation *location, u8 *bytes)
{
    bytes[COOP_PRESENCE_WORLD_LOCATION_REGION_OFFSET] = location->region;
    WriteLe16(&bytes[COOP_PRESENCE_WORLD_LOCATION_MAP_GROUP_OFFSET], location->map_group);
    WriteLe16(&bytes[COOP_PRESENCE_WORLD_LOCATION_MAP_NUMBER_OFFSET], location->map_number);
    WriteLeS16(&bytes[COOP_PRESENCE_WORLD_LOCATION_X_OFFSET], location->x);
    WriteLeS16(&bytes[COOP_PRESENCE_WORLD_LOCATION_Y_OFFSET], location->y);
    bytes[COOP_PRESENCE_WORLD_LOCATION_RESERVED_OFFSET] = 0;
}

static bool8 DecodePoseUnchecked(const u8 *bytes, struct CoopPresencePose *out)
{
    struct CoopPresencePose candidate = {0};

    if (!DecodeLocation(&bytes[COOP_PRESENCE_POSE_LOCATION_OFFSET], &candidate.location))
        return FALSE;
    candidate.elevation = bytes[COOP_PRESENCE_POSE_ELEVATION_OFFSET];
    candidate.direction = bytes[COOP_PRESENCE_POSE_DIRECTION_OFFSET];
    candidate.client_tick = ReadLe32(&bytes[COOP_PRESENCE_POSE_CLIENT_TICK_OFFSET]);
    candidate.warp_sequence = ReadLe32(&bytes[COOP_PRESENCE_POSE_WARP_SEQUENCE_OFFSET]);
    candidate.movement_mode = bytes[COOP_PRESENCE_POSE_MOVEMENT_MODE_OFFSET];
    candidate.animation_id = bytes[COOP_PRESENCE_POSE_ANIMATION_ID_OFFSET];
    candidate.avatar_id = bytes[COOP_PRESENCE_POSE_AVATAR_ID_OFFSET];
    candidate.player_state = bytes[COOP_PRESENCE_POSE_PLAYER_STATE_OFFSET];
    if (!IsPoseValid(&candidate))
        return FALSE;
    *out = candidate;
    return TRUE;
}

static void EncodePoseUnchecked(const struct CoopPresencePose *value, u8 *bytes)
{
    EncodeLocation(&value->location, &bytes[COOP_PRESENCE_POSE_LOCATION_OFFSET]);
    bytes[COOP_PRESENCE_POSE_ELEVATION_OFFSET] = value->elevation;
    bytes[COOP_PRESENCE_POSE_DIRECTION_OFFSET] = value->direction;
    WriteLe32(&bytes[COOP_PRESENCE_POSE_CLIENT_TICK_OFFSET], value->client_tick);
    WriteLe32(&bytes[COOP_PRESENCE_POSE_WARP_SEQUENCE_OFFSET], value->warp_sequence);
    bytes[COOP_PRESENCE_POSE_MOVEMENT_MODE_OFFSET] = value->movement_mode;
    bytes[COOP_PRESENCE_POSE_ANIMATION_ID_OFFSET] = value->animation_id;
    bytes[COOP_PRESENCE_POSE_AVATAR_ID_OFFSET] = value->avatar_id;
    bytes[COOP_PRESENCE_POSE_PLAYER_STATE_OFFSET] = value->player_state;
}

static bool8 DecodeLocalStateUnchecked(const u8 *bytes, struct CoopPresenceLocalState *out)
{
    struct CoopPresenceLocalState candidate = {0};

    if (!DecodePoseUnchecked(&bytes[COOP_PRESENCE_LOCAL_STATE_POSE_OFFSET], &candidate.pose))
        return FALSE;
    candidate.source_sequence = ReadLe32(&bytes[COOP_PRESENCE_LOCAL_STATE_SOURCE_SEQUENCE_OFFSET]);
    if (!IsLocalStateValid(&candidate))
        return FALSE;
    *out = candidate;
    return TRUE;
}

static void EncodeLocalStateUnchecked(const struct CoopPresenceLocalState *value, u8 *bytes)
{
    EncodePoseUnchecked(&value->pose, &bytes[COOP_PRESENCE_LOCAL_STATE_POSE_OFFSET]);
    WriteLe32(&bytes[COOP_PRESENCE_LOCAL_STATE_SOURCE_SEQUENCE_OFFSET], value->source_sequence);
}

bool8 CoopPresence_DecodePose(const u8 *bytes, u32 length, struct CoopPresencePose *out)
{
    struct CoopPresencePose candidate;

    if (out == NULL || !IsExactLength(bytes, length, COOP_PRESENCE_POSE_SIZE))
        return FALSE;
    if (!DecodePoseUnchecked(bytes, &candidate))
        return FALSE;
    *out = candidate;
    return TRUE;
}

bool8 CoopPresence_EncodePose(const struct CoopPresencePose *value, u8 *bytes, u32 length)
{
    u8 candidate[COOP_PRESENCE_POSE_SIZE] = {0};

    if (value == NULL || bytes == NULL || length != COOP_PRESENCE_POSE_SIZE
     || !IsPoseValid(value))
        return FALSE;
    EncodePoseUnchecked(value, candidate);
    CopyBytes(bytes, candidate, COOP_PRESENCE_POSE_SIZE);
    return TRUE;
}

bool8 CoopPresence_DecodeLocalState(const u8 *bytes, u32 length,
                                    struct CoopPresenceLocalState *out)
{
    struct CoopPresenceLocalState candidate;

    if (out == NULL || !IsExactLength(bytes, length, COOP_PRESENCE_LOCAL_STATE_SIZE))
        return FALSE;
    if (!DecodeLocalStateUnchecked(bytes, &candidate))
        return FALSE;
    *out = candidate;
    return TRUE;
}

bool8 CoopPresence_EncodeLocalState(const struct CoopPresenceLocalState *value,
                                    u8 *bytes, u32 length)
{
    u8 candidate[COOP_PRESENCE_LOCAL_STATE_SIZE] = {0};

    if (value == NULL || bytes == NULL || length != COOP_PRESENCE_LOCAL_STATE_SIZE
     || !IsLocalStateValid(value))
        return FALSE;
    EncodeLocalStateUnchecked(value, candidate);
    CopyBytes(bytes, candidate, COOP_PRESENCE_LOCAL_STATE_SIZE);
    return TRUE;
}

bool8 CoopPresence_DecodeSpawn(const u8 *bytes, u32 length,
                               struct CoopPresenceSpawn *out)
{
    struct CoopPresenceSpawn candidate = {0};

    if (out == NULL || !IsExactLength(bytes, length, COOP_PRESENCE_SPAWN_SIZE))
        return FALSE;
    candidate.handle = ReadLe64(&bytes[COOP_PRESENCE_SPAWN_HANDLE_OFFSET]);
    candidate.server_sequence = ReadLe32(&bytes[COOP_PRESENCE_SPAWN_SERVER_SEQUENCE_OFFSET]);
    if (!IsHandleValid(candidate.handle)
     || !IsSequenceValid(candidate.server_sequence)
     || !DecodeLocalStateUnchecked(&bytes[COOP_PRESENCE_SPAWN_STATE_OFFSET], &candidate.state)
     || !DecodeUsername(&bytes[COOP_PRESENCE_SPAWN_USERNAME_OFFSET], &candidate.username))
        return FALSE;
    *out = candidate;
    return TRUE;
}

bool8 CoopPresence_EncodeSpawn(const struct CoopPresenceSpawn *value,
                               u8 *bytes, u32 length)
{
    u8 candidate[COOP_PRESENCE_SPAWN_SIZE] = {0};
    u8 i;

    if (value == NULL || bytes == NULL || length != COOP_PRESENCE_SPAWN_SIZE
     || !IsHandleValid(value->handle) || !IsSequenceValid(value->server_sequence)
     || !IsLocalStateValid(&value->state) || !ValidateUsername(&value->username))
        return FALSE;
    WriteLe64(&candidate[COOP_PRESENCE_SPAWN_HANDLE_OFFSET], value->handle);
    WriteLe32(&candidate[COOP_PRESENCE_SPAWN_SERVER_SEQUENCE_OFFSET], value->server_sequence);
    EncodeLocalStateUnchecked(&value->state, &candidate[COOP_PRESENCE_SPAWN_STATE_OFFSET]);
    for (i = 0; i < value->username.length; i++)
        candidate[COOP_PRESENCE_SPAWN_USERNAME_OFFSET + i] = (u8)value->username.bytes[i];
    CopyBytes(bytes, candidate, COOP_PRESENCE_SPAWN_SIZE);
    return TRUE;
}

bool8 CoopPresence_DecodeUpdate(const u8 *bytes, u32 length,
                                struct CoopPresenceUpdate *out)
{
    struct CoopPresenceUpdate candidate = {0};

    if (out == NULL || !IsExactLength(bytes, length, COOP_PRESENCE_UPDATE_SIZE))
        return FALSE;
    candidate.handle = ReadLe64(&bytes[COOP_PRESENCE_UPDATE_HANDLE_OFFSET]);
    candidate.server_sequence = ReadLe32(&bytes[COOP_PRESENCE_UPDATE_SERVER_SEQUENCE_OFFSET]);
    if (!IsHandleValid(candidate.handle)
     || !IsSequenceValid(candidate.server_sequence)
     || !DecodeLocalStateUnchecked(&bytes[COOP_PRESENCE_UPDATE_STATE_OFFSET], &candidate.state))
        return FALSE;
    *out = candidate;
    return TRUE;
}

bool8 CoopPresence_EncodeUpdate(const struct CoopPresenceUpdate *value,
                                u8 *bytes, u32 length)
{
    u8 candidate[COOP_PRESENCE_UPDATE_SIZE] = {0};

    if (value == NULL || bytes == NULL || length != COOP_PRESENCE_UPDATE_SIZE
     || !IsHandleValid(value->handle) || !IsSequenceValid(value->server_sequence)
     || !IsLocalStateValid(&value->state))
        return FALSE;
    WriteLe64(&candidate[COOP_PRESENCE_UPDATE_HANDLE_OFFSET], value->handle);
    WriteLe32(&candidate[COOP_PRESENCE_UPDATE_SERVER_SEQUENCE_OFFSET], value->server_sequence);
    EncodeLocalStateUnchecked(&value->state, &candidate[COOP_PRESENCE_UPDATE_STATE_OFFSET]);
    CopyBytes(bytes, candidate, COOP_PRESENCE_UPDATE_SIZE);
    return TRUE;
}

bool8 CoopPresence_DecodeDespawn(const u8 *bytes, u32 length,
                                 struct CoopPresenceDespawn *out)
{
    struct CoopPresenceDespawn candidate = {0};
    u8 i;

    if (out == NULL || !IsExactLength(bytes, length, COOP_PRESENCE_DESPAWN_SIZE))
        return FALSE;
    for (i = COOP_PRESENCE_DESPAWN_RESERVED_OFFSET; i < COOP_PRESENCE_DESPAWN_SIZE; i++)
    {
        if (bytes[i] != 0)
            return FALSE;
    }
    candidate.handle = ReadLe64(&bytes[COOP_PRESENCE_DESPAWN_HANDLE_OFFSET]);
    candidate.server_sequence = ReadLe32(&bytes[COOP_PRESENCE_DESPAWN_SERVER_SEQUENCE_OFFSET]);
    candidate.reason = bytes[COOP_PRESENCE_DESPAWN_REASON_OFFSET];
    if (!IsHandleValid(candidate.handle) || !IsSequenceValid(candidate.server_sequence)
     || !IsDespawnReason(candidate.reason))
        return FALSE;
    *out = candidate;
    return TRUE;
}

bool8 CoopPresence_EncodeDespawn(const struct CoopPresenceDespawn *value,
                                 u8 *bytes, u32 length)
{
    u8 candidate[COOP_PRESENCE_DESPAWN_SIZE] = {0};

    if (value == NULL || bytes == NULL || length != COOP_PRESENCE_DESPAWN_SIZE
     || !IsHandleValid(value->handle) || !IsSequenceValid(value->server_sequence)
     || !IsDespawnReason(value->reason))
        return FALSE;
    WriteLe64(&candidate[COOP_PRESENCE_DESPAWN_HANDLE_OFFSET], value->handle);
    WriteLe32(&candidate[COOP_PRESENCE_DESPAWN_SERVER_SEQUENCE_OFFSET], value->server_sequence);
    candidate[COOP_PRESENCE_DESPAWN_REASON_OFFSET] = value->reason;
    CopyBytes(bytes, candidate, COOP_PRESENCE_DESPAWN_SIZE);
    return TRUE;
}

bool8 CoopPresence_DecodeInteraction(const u8 *bytes, u32 length,
                                     struct CoopPresenceInteraction *out)
{
    struct CoopPresenceInteraction candidate = {0};

    if (out == NULL || !IsExactLength(bytes, length, COOP_PRESENCE_INTERACTION_SIZE))
        return FALSE;
    candidate.handle = ReadLe64(&bytes[COOP_PRESENCE_INTERACTION_HANDLE_OFFSET]);
    candidate.observed_server_sequence = ReadLe32(
        &bytes[COOP_PRESENCE_INTERACTION_SERVER_SEQUENCE_OFFSET]);
    candidate.observed_warp_sequence = ReadLe32(
        &bytes[COOP_PRESENCE_INTERACTION_WARP_SEQUENCE_OFFSET]);
    candidate.x = ReadLeS16(&bytes[COOP_PRESENCE_INTERACTION_X_OFFSET]);
    candidate.y = ReadLeS16(&bytes[COOP_PRESENCE_INTERACTION_Y_OFFSET]);
    if (!IsHandleValid(candidate.handle)
     || !IsSequenceValid(candidate.observed_server_sequence)
     || !IsSequenceValid(candidate.observed_warp_sequence))
        return FALSE;
    *out = candidate;
    return TRUE;
}

static bool8 EncodeInteractionValue(const struct CoopPresenceInteraction *value,
                                    u8 *bytes, u32 length)
{
    u8 candidate[COOP_PRESENCE_INTERACTION_SIZE] = {0};

    if (value == NULL || bytes == NULL || length != COOP_PRESENCE_INTERACTION_SIZE
     || !IsHandleValid(value->handle)
     || !IsSequenceValid(value->observed_server_sequence)
     || !IsSequenceValid(value->observed_warp_sequence))
        return FALSE;
    WriteLe64(&candidate[COOP_PRESENCE_INTERACTION_HANDLE_OFFSET], value->handle);
    WriteLe32(&candidate[COOP_PRESENCE_INTERACTION_SERVER_SEQUENCE_OFFSET],
              value->observed_server_sequence);
    WriteLe32(&candidate[COOP_PRESENCE_INTERACTION_WARP_SEQUENCE_OFFSET],
              value->observed_warp_sequence);
    WriteLeS16(&candidate[COOP_PRESENCE_INTERACTION_X_OFFSET], value->x);
    WriteLeS16(&candidate[COOP_PRESENCE_INTERACTION_Y_OFFSET], value->y);
    CopyBytes(bytes, candidate, COOP_PRESENCE_INTERACTION_SIZE);
    return TRUE;
}

bool8 CoopPresence_SequenceIsNewer(u32 candidate, u32 reference)
{
    u32 delta;

    if (candidate == 0 || reference == 0 || candidate == reference)
        return FALSE;
    delta = candidate - reference;
    return delta < 0x80000000u;
}

u32 CoopPresence_NextSequence(u32 current)
{
    current++;
    return current == 0 ? 1 : current;
}

static void ClearRemote(struct CoopPresenceReducer *reducer)
{
    reducer->remote_active = FALSE;
    reducer->remote.handle = 0;
    reducer->remote.server_sequence = 0;
    reducer->remote.state = (struct CoopPresenceLocalState){0};
    reducer->remote.username = (struct CoopPresenceUsername){0};
}

static bool8 SamePartition(const struct CoopPresencePartition *partition,
                           const struct WorldLocation *location,
                           u32 warp_sequence)
{
    return partition->region == location->region
        && partition->map_group == location->map_group
        && partition->map_number == location->map_number
        && partition->warp_sequence == warp_sequence;
}

static bool8 RemoteStateMatchesPartition(const struct CoopPresenceReducer *reducer,
                                         const struct CoopPresenceLocalState *state)
{
    return reducer->context_valid
        && SamePartition(&reducer->partition, &state->pose.location,
                         state->pose.warp_sequence);
}

void CoopPresenceReducer_Init(struct CoopPresenceReducer *reducer)
{
    if (reducer == NULL)
        return;
    *reducer = (struct CoopPresenceReducer){0};
}

void CoopPresenceReducer_Reset(struct CoopPresenceReducer *reducer)
{
    CoopPresenceReducer_Init(reducer);
}

bool8 CoopPresenceReducer_Synchronize(struct CoopPresenceReducer *reducer,
                                      u32 session_epoch,
                                      const struct WorldLocation *location,
                                      u32 warp_sequence)
{
    struct CoopPresencePartition candidate;

    if (reducer == NULL || session_epoch == 0 || warp_sequence == 0
     || !IsLocationValid(location))
        return FALSE;
    candidate.session_epoch = session_epoch;
    candidate.region = location->region;
    candidate.map_group = location->map_group;
    candidate.map_number = location->map_number;
    candidate.warp_sequence = warp_sequence;
    if (!reducer->context_valid
     || reducer->partition.session_epoch != candidate.session_epoch
     || reducer->partition.region != candidate.region
     || reducer->partition.map_group != candidate.map_group
     || reducer->partition.map_number != candidate.map_number
     || reducer->partition.warp_sequence != candidate.warp_sequence)
    {
        ClearRemote(reducer);
        reducer->partition = candidate;
        reducer->context_valid = TRUE;
    }
    return TRUE;
}

bool8 CoopPresenceReducer_IsActive(const struct CoopPresenceReducer *reducer)
{
    return reducer != NULL && reducer->context_valid && reducer->remote_active;
}

bool8 CoopPresenceReducer_IsVisible(const struct CoopPresenceReducer *reducer)
{
    return CoopPresenceReducer_IsActive(reducer)
        && reducer->remote.state.pose.player_state == COOP_PRESENCE_PLAYER_OVERWORLD;
}

const struct CoopPresenceRemote *CoopPresenceReducer_GetRemote(
    const struct CoopPresenceReducer *reducer)
{
    if (!CoopPresenceReducer_IsActive(reducer))
        return NULL;
    return &reducer->remote;
}

enum CoopPresenceApplyResult CoopPresenceReducer_ApplySpawn(
    struct CoopPresenceReducer *reducer,
    const struct CoopPresenceSpawn *spawn)
{
    if (reducer == NULL || spawn == NULL || !reducer->context_valid
     || !IsHandleValid(spawn->handle) || !IsSequenceValid(spawn->server_sequence)
     || !IsLocalStateValid(&spawn->state) || !ValidateUsername(&spawn->username))
        return COOP_PRESENCE_APPLY_REJECTED;
    if (!RemoteStateMatchesPartition(reducer, &spawn->state))
        return COOP_PRESENCE_APPLY_PARTITION_MISMATCH;
    if (reducer->remote_active)
    {
        if (reducer->remote.handle != spawn->handle)
            return COOP_PRESENCE_APPLY_CAPACITY;
        if (!CoopPresence_SequenceIsNewer(spawn->server_sequence,
                                          reducer->remote.server_sequence))
            return COOP_PRESENCE_APPLY_STALE;
    }
    reducer->remote_active = TRUE;
    reducer->remote.handle = spawn->handle;
    reducer->remote.server_sequence = spawn->server_sequence;
    reducer->remote.state = spawn->state;
    reducer->remote.username = spawn->username;
    return COOP_PRESENCE_APPLY_APPLIED;
}

enum CoopPresenceApplyResult CoopPresenceReducer_ApplyUpdate(
    struct CoopPresenceReducer *reducer,
    const struct CoopPresenceUpdate *update)
{
    if (reducer == NULL || update == NULL || !reducer->context_valid
     || !IsHandleValid(update->handle) || !IsSequenceValid(update->server_sequence)
     || !IsLocalStateValid(&update->state))
        return COOP_PRESENCE_APPLY_REJECTED;
    if (!reducer->remote_active)
        return COOP_PRESENCE_APPLY_NOT_ACTIVE;
    if (reducer->remote.handle != update->handle)
        return COOP_PRESENCE_APPLY_HANDLE_MISMATCH;
    if (!RemoteStateMatchesPartition(reducer, &update->state))
        return COOP_PRESENCE_APPLY_PARTITION_MISMATCH;
    if (!CoopPresence_SequenceIsNewer(update->server_sequence,
                                      reducer->remote.server_sequence))
        return COOP_PRESENCE_APPLY_STALE;
    if (update->state.pose.player_state == COOP_PRESENCE_PLAYER_HIDDEN)
    {
        ClearRemote(reducer);
        return COOP_PRESENCE_APPLY_APPLIED;
    }
    reducer->remote.server_sequence = update->server_sequence;
    reducer->remote.state = update->state;
    return COOP_PRESENCE_APPLY_APPLIED;
}

enum CoopPresenceApplyResult CoopPresenceReducer_ApplyDespawn(
    struct CoopPresenceReducer *reducer,
    const struct CoopPresenceDespawn *despawn)
{
    if (reducer == NULL || despawn == NULL || !reducer->context_valid
     || !IsHandleValid(despawn->handle) || !IsSequenceValid(despawn->server_sequence)
     || !IsDespawnReason(despawn->reason))
        return COOP_PRESENCE_APPLY_REJECTED;
    if (!reducer->remote_active)
        return COOP_PRESENCE_APPLY_NOT_ACTIVE;
    if (reducer->remote.handle != despawn->handle)
        return COOP_PRESENCE_APPLY_HANDLE_MISMATCH;
    if (!CoopPresence_SequenceIsNewer(despawn->server_sequence,
                                      reducer->remote.server_sequence))
        return COOP_PRESENCE_APPLY_STALE;
    ClearRemote(reducer);
    return COOP_PRESENCE_APPLY_APPLIED;
}

static bool8 LocalContextValid(const struct CoopPresenceLocalContext *local)
{
    return local != NULL && local->session_epoch != 0
        && local->warp_sequence != 0 && IsLocationValid(&local->location)
        && IsDirection(local->direction);
}

static bool8 LocalMatchesReducer(const struct CoopPresenceReducer *reducer,
                                 const struct CoopPresenceLocalContext *local)
{
    return reducer != NULL && reducer->context_valid
        && reducer->partition.session_epoch == local->session_epoch
        && SamePartition(&reducer->partition, &local->location, local->warp_sequence);
}

bool8 CoopPresence_EncodeInteraction(
    const struct CoopPresenceReducer *reducer,
    const struct CoopPresenceLocalContext *local,
    u8 *bytes, u32 length)
{
    struct CoopPresenceInteraction value;
    u8 candidate[COOP_PRESENCE_INTERACTION_SIZE] = {0};
    s32 targetX;
    s32 targetY;

    if (bytes == NULL || length != COOP_PRESENCE_INTERACTION_SIZE
     || !CoopPresenceReducer_IsVisible(reducer)
     || !LocalContextValid(local) || !LocalMatchesReducer(reducer, local))
        return FALSE;
    if (reducer->remote.state.pose.elevation != local->elevation
     || reducer->remote.state.pose.location.region != local->location.region
     || reducer->remote.state.pose.location.map_group != local->location.map_group
     || reducer->remote.state.pose.location.map_number != local->location.map_number
     || reducer->remote.state.pose.warp_sequence != local->warp_sequence)
        return FALSE;

    targetX = local->location.x;
    targetY = local->location.y;
    switch (local->direction)
    {
    case COOP_PRESENCE_DIRECTION_SOUTH:
        targetY++;
        break;
    case COOP_PRESENCE_DIRECTION_NORTH:
        targetY--;
        break;
    case COOP_PRESENCE_DIRECTION_WEST:
        targetX--;
        break;
    case COOP_PRESENCE_DIRECTION_EAST:
        targetX++;
        break;
    default:
        return FALSE;
    }
    if (targetX < -32768 || targetX > 32767 || targetY < -32768 || targetY > 32767
     || reducer->remote.state.pose.location.x != (s16)targetX
     || reducer->remote.state.pose.location.y != (s16)targetY)
        return FALSE;

    value.handle = reducer->remote.handle;
    value.observed_server_sequence = reducer->remote.server_sequence;
    value.observed_warp_sequence = reducer->remote.state.pose.warp_sequence;
    value.x = reducer->remote.state.pose.location.x;
    value.y = reducer->remote.state.pose.location.y;
    if (!EncodeInteractionValue(&value, candidate,
                                COOP_PRESENCE_INTERACTION_SIZE))
        return FALSE;
    CopyBytes(bytes, candidate, COOP_PRESENCE_INTERACTION_SIZE);
    return TRUE;
}
