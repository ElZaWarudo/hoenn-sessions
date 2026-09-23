#include "global.h"
#include "data.h"
#include "coop/character.h"
#include "coop/net_bridge.h"
#include "coop/presence_runtime.h"
#include "event_object_movement.h"
#include "field_effect.h"
#include "field_player_avatar.h"
#include "fieldmap.h"
#include "follower_helper.h"
#include "overworld.h"
#include "palette.h"
#include "pokemon.h"
#include "script.h"
#include "sprite.h"
#include "constants/event_object_movement.h"
#include "constants/event_objects.h"
#include "constants/field_effects.h"

extern void MovementType_None(struct Sprite *sprite);

static void RemoveFollowerRenderer(void);

enum CoopPresencePendingType
{
    COOP_PRESENCE_PENDING_NONE = 0,
    COOP_PRESENCE_PENDING_SPAWN,
    COOP_PRESENCE_PENDING_UPDATE,
    COOP_PRESENCE_PENDING_DESPAWN,
    COOP_PRESENCE_PENDING_COMPANION,
    COOP_PRESENCE_PENDING_SIGNAL,
};

struct CoopPresencePendingFrame
{
    enum CoopPresencePendingType type;
    union
    {
        struct CoopPresenceSpawn spawn;
        struct CoopPresenceUpdate update;
        struct CoopPresenceDespawn despawn;
        struct CoopPresenceRemoteCompanion companion;
        struct CoopPresenceRemoteSignal signal;
    } value;
};

struct CoopPresenceRuntime
{
    struct CoopPresenceReducer reducer;
    struct CoopPresencePendingFrame pending[COOP_PRESENCE_RUNTIME_PENDING_CAPACITY];
    u8 pending_read;
    u8 pending_write;
    u8 pending_count;
    u32 session_epoch;
    u32 source_sequence;
    u32 warp_sequence;
    u32 frame_counter;
    u32 last_lifecycle_frame;
    struct CoopPresencePose last_pose;
    bool8 last_pose_valid;
    u64 rendered_handle;
    u8 rendered_object_id;
    u8 rendered_sprite_id;
    u8 rendered_avatar_id;
    u8 rendered_elevation;
    u8 rendered_direction;
    u8 rendered_animation;
    u16 rendered_generation;
    u16 renderer_generation;
    u8 rendered_map_group;
    u8 rendered_map_num;
    s16 rendered_x;
    s16 rendered_y;
    s32 interpolation_start_x;
    s32 interpolation_start_y;
    s32 sprite_target_x;
    s32 sprite_target_y;
    u8 interpolation_remaining;
    u8 interpolation_duration;
    bool8 initialized;
    bool8 transport_ready;
    bool8 renderer_owned;
    u8 despawn_hold_reason;
    u32 despawn_hold_start;
    u32 companion_source_sequence;
    u16 last_companion_species;
    u8 last_companion_flags;
    u32 last_companion_frame;
    u32 signal_source_sequence;
    bool8 signal_used;
    u32 last_signal_frame;
    u8 next_emote;
    bool8 remote_companion_valid;
    u64 remote_companion_handle;
    u32 remote_companion_sequence;
    u16 remote_companion_species;
    u8 remote_companion_flags;
    u32 remote_signal_sequence;
    bool8 remote_signal_seen;
    u8 pending_bubble;
    bool8 pending_bubble_set;
    u32 pending_bubble_frame;
    s16 ping_x;
    s16 ping_y;
    u16 ping_map_group;
    u16 ping_map_num;
    u32 ping_frame;
    bool8 ping_valid;
    bool8 follower_owned;
    u8 follower_object_id;
    u8 follower_sprite_id;
    u16 follower_generation;
    u16 follower_species;
    u8 follower_flags;
    s16 follower_x;
    s16 follower_y;
    u32 follower_step_frame;
};

static EWRAM_DATA struct CoopPresenceRuntime sCoopPresenceRuntime = {0};
static EWRAM_DATA u16 sCoopPresenceRendererGeneration = 0;

static u16 NextRendererGeneration(void)
{
    sCoopPresenceRendererGeneration++;
    if (sCoopPresenceRendererGeneration == 0)
        sCoopPresenceRendererGeneration++;
    return sCoopPresenceRendererGeneration;
}

static bool8 IsNormalOverworld(void)
{
    return gMain.callback1 == CB1_Overworld
        && gMain.callback2 == CB2_Overworld;
}

static bool8 IsPlayerBindingValid(void)
{
    const struct ObjectEvent *player;

    if (gSaveBlock1Ptr == NULL || gPlayerAvatar.objectEventId >= OBJECT_EVENTS_COUNT
     || gPlayerAvatar.spriteId >= MAX_SPRITES)
        return FALSE;
    player = &gObjectEvents[gPlayerAvatar.objectEventId];
    return player->active && player->isPlayer
        && player->spriteId == gPlayerAvatar.spriteId
        && gSprites[player->spriteId].inUse;
}

static bool8 IsWorldLocationCurrent(struct WorldLocation *location)
{
    return location != NULL && CoopWorldLocation_Export(location);
}

static bool8 IsOverworldPoseAllowed(void)
{
    return IsNormalOverworld()
        && !gPaletteFade.active
        && !ArePlayerFieldControlsLocked()
        && IsPlayerBindingValid()
        /* CONTROLLABLE is cleared during ordinary PlayerStep movement; field
         * control locks above are the authority for whether input is safe. */
        && (gPlayerAvatar.flags & PLAYER_AVATAR_FLAG_ON_FOOT) != 0;
}

static bool8 LastPoseMatchesLocation(const struct CoopPresencePose *pose,
                                     const struct WorldLocation *location)
{
    return pose != NULL && location != NULL
        && pose->location.region == location->region
        && pose->location.map_group == location->map_group
        && pose->location.map_number == location->map_number
        && pose->warp_sequence == sCoopPresenceRuntime.warp_sequence;
}

static bool8 BuildHiddenPoseFromLast(struct CoopPresencePose *pose,
                                     const struct WorldLocation *location)
{
    if (pose == NULL || !sCoopPresenceRuntime.last_pose_valid
     || (location != NULL
         && !LastPoseMatchesLocation(&sCoopPresenceRuntime.last_pose, location)))
        return FALSE;

    *pose = sCoopPresenceRuntime.last_pose;
    pose->warp_sequence = sCoopPresenceRuntime.warp_sequence;
    pose->movement_mode = COOP_PRESENCE_MOVEMENT_IDLE;
    pose->animation_id = COOP_PRESENCE_ANIMATION_IDLE;
    pose->player_state = COOP_PRESENCE_PLAYER_HIDDEN;
    return TRUE;
}

static bool8 BuildPose(struct CoopPresencePose *pose, bool8 visible)
{
    struct WorldLocation location;
    struct ObjectEvent *player;
    u8 flags;

    if (pose == NULL)
        return FALSE;
    if (!IsPlayerBindingValid() || !IsWorldLocationCurrent(&location))
        return BuildHiddenPoseFromLast(pose, NULL);

    /* An unsafe field state must never replace the last compatible pose with
     * a transient location.  If no compatible visible pose exists yet, keep
     * publication gated until the player is safe to sample. */
    if (!visible)
        return BuildHiddenPoseFromLast(pose, &location);

    player = &gObjectEvents[gPlayerAvatar.objectEventId];
    flags = gPlayerAvatar.flags;
    pose->location = location;
    pose->elevation = player->previousElevation;
    pose->direction = player->facingDirection;
    pose->client_tick = sCoopPresenceRuntime.frame_counter;
    pose->warp_sequence = sCoopPresenceRuntime.warp_sequence;
    pose->movement_mode = COOP_PRESENCE_MOVEMENT_IDLE;
    pose->animation_id = COOP_PRESENCE_ANIMATION_IDLE;
    pose->avatar_id = CoopCharacter_GetAvatarId();
    pose->player_state = visible ? COOP_PRESENCE_PLAYER_OVERWORLD
                                 : COOP_PRESENCE_PLAYER_HIDDEN;

    if (visible && gPlayerAvatar.runningState == MOVING)
    {
        pose->movement_mode = (flags & PLAYER_AVATAR_FLAG_DASH)
            ? COOP_PRESENCE_MOVEMENT_RUN : COOP_PRESENCE_MOVEMENT_WALK;
        pose->animation_id = COOP_PRESENCE_ANIMATION_LOCOMOTION;
    }
    sCoopPresenceRuntime.last_pose = *pose;
    sCoopPresenceRuntime.last_pose_valid = TRUE;
    return TRUE;
}

bool8 CoopPresenceRuntime_GetLocalState(struct CoopPresenceLocalState *out)
{
    struct CoopPresencePose pose;
    struct CoopPresenceLocalState candidate;

    if (out == NULL || !sCoopPresenceRuntime.initialized
     || !sCoopPresenceRuntime.transport_ready
     || sCoopPresenceRuntime.warp_sequence == 0)
        return FALSE;

    if (BuildPose(&pose, IsOverworldPoseAllowed()))
    {
        candidate.pose = pose;
        candidate.source_sequence = sCoopPresenceRuntime.source_sequence == 0
            ? 1 : sCoopPresenceRuntime.source_sequence;
        *out = candidate;
        return TRUE;
    }

    return FALSE;
}

bool8 CoopPresenceRuntime_EncodeLocalState(u8 *bytes, u32 length)
{
    struct CoopPresenceLocalState state;

    if (bytes == NULL || length != COOP_PRESENCE_LOCAL_STATE_SIZE
     || !CoopPresenceRuntime_GetLocalState(&state))
        return FALSE;
    sCoopPresenceRuntime.source_sequence = CoopPresence_NextSequence(
        sCoopPresenceRuntime.source_sequence);
    state.source_sequence = sCoopPresenceRuntime.source_sequence;
    return CoopPresence_EncodeLocalState(&state, bytes, length);
}

static bool8 IsCurrentObjectEvent(const struct ObjectEvent *object_event)
{
    return gSaveBlock1Ptr != NULL && object_event != NULL && object_event->active
        && object_event->localId == COOP_PRESENCE_RUNTIME_OBJECT_LOCAL_ID
        && object_event->mapGroup == gSaveBlock1Ptr->location.mapGroup
        && object_event->mapNum == gSaveBlock1Ptr->location.mapNum;
}

bool8 CoopPresenceRuntime_IsRemoteObject(const struct ObjectEvent *object_event)
{
    return object_event != NULL && object_event->localId == COOP_PRESENCE_RUNTIME_OBJECT_LOCAL_ID;
}

static u8 FindRemoteObjectEvent(void)
{
    u8 i;

    if (gSaveBlock1Ptr == NULL)
        return OBJECT_EVENTS_COUNT;
    for (i = 0; i < OBJECT_EVENTS_COUNT; i++)
    {
        if (IsCurrentObjectEvent(&gObjectEvents[i]))
            return i;
    }
    return OBJECT_EVENTS_COUNT;
}

/* data[0] is the engine's object-event backlink, but it is not an ownership
 * token: a reset/reused sprite can coincidentally retain or receive the same
 * index.  Presence sprites carry a generation marker in data[7], and the
 * movement callback is checked as a type discriminator. */
static bool8 IsRendererSpriteProof(u8 object_id, u8 sprite_id, u16 generation,
                                   u16 graphics_id, bool8 require_backlink)
{
    const struct ObjectEvent *object_event;
    struct Sprite *sprite;

    if (object_id >= OBJECT_EVENTS_COUNT || sprite_id >= MAX_SPRITES
     || generation == 0)
        return FALSE;
    object_event = &gObjectEvents[object_id];
    if ((require_backlink && !object_event->active)
     || object_event->localId != COOP_PRESENCE_RUNTIME_OBJECT_LOCAL_ID
     || object_event->movementType != MOVEMENT_TYPE_NONE
     || (require_backlink && object_event->graphicsId != graphics_id)
     || (require_backlink && object_event->spriteId != sprite_id))
        return FALSE;
    sprite = &gSprites[sprite_id];
    if (!sprite->inUse || sprite->data[0] != (s16)object_id
     || (u16)sprite->data[7] != generation
     || sprite->callback != MovementType_None)
        return FALSE;
    return TRUE;
}

static bool8 FindOwnedSprite(struct ObjectEvent *object_event, u16 generation,
                             struct Sprite **out)
{
    u8 object_id;

    if (!IsCurrentObjectEvent(object_event)
     || object_event->spriteId >= MAX_SPRITES)
        return FALSE;
    object_id = (u8)(object_event - gObjectEvents);
    if (!IsRendererSpriteProof(object_id, object_event->spriteId, generation,
                               object_event->graphicsId, TRUE))
        return FALSE;
    if (out != NULL)
        *out = &gSprites[object_event->spriteId];
    return TRUE;
}

static void ClearRendererIdentity(void)
{
    sCoopPresenceRuntime.renderer_owned = FALSE;
    sCoopPresenceRuntime.rendered_handle = 0;
    sCoopPresenceRuntime.rendered_object_id = OBJECT_EVENTS_COUNT;
    sCoopPresenceRuntime.rendered_sprite_id = MAX_SPRITES;
    sCoopPresenceRuntime.rendered_generation = 0;
    sCoopPresenceRuntime.renderer_generation = 0;
    sCoopPresenceRuntime.rendered_direction = DIR_NONE;
    sCoopPresenceRuntime.rendered_animation = 0xFF;
    sCoopPresenceRuntime.interpolation_remaining = 0;
}

static u16 GetRenderedGraphicsId(void)
{
    return CoopCharacter_GetGraphicsId(sCoopPresenceRuntime.rendered_avatar_id);
}

static void DestroyCachedOwnedSprite(u8 object_id, u8 sprite_id,
                                     u16 generation, u16 graphics_id)
{
    struct Sprite *sprite;

    if (object_id >= OBJECT_EVENTS_COUNT || sprite_id >= MAX_SPRITES)
        return;
    sprite = &gSprites[sprite_id];
    if (!IsRendererSpriteProof(object_id, sprite_id, generation, graphics_id,
                               FALSE))
        return;

    /* A stale ObjectEvent may have lost its sprite backlink during a field
     * resume.  The cached backlink is the only authority for reclaiming the
     * old renderer; never touch a slot now owned by another ObjectEvent. */
    FreeSpriteOamMatrix(sprite);
    DestroySprite(sprite);
}

/* A return-to-field reset destroys sprites before it rebuilds ObjectEvents.
 * If the reserved remote object is left active in that window, the runtime
 * will keep finding an unsprited slot and can no longer recreate the remote.
 * Reclaim only an object whose sprite is absent; an in-use sprite with a
 * different backlink belongs to another subsystem and must not be touched. */
static void RetireUnownedRemoteObject(struct ObjectEvent *object_event)
{
    if (object_event == NULL || !object_event->active
     || object_event->localId != COOP_PRESENCE_RUNTIME_OBJECT_LOCAL_ID)
        return;
    if (!FindOwnedSprite(object_event, sCoopPresenceRuntime.rendered_generation,
                         NULL))
    {
        /* A return-to-field reset can leave the reserved ObjectEvent active
         * while its sprite slot has already been rebound. Retire only the
         * stale ObjectEvent; the foreign sprite remains untouched. */
        object_event->active = FALSE;
    }
}

static void AbandonCreatedRenderer(u8 object_id)
{
    struct ObjectEvent *object_event;

    if (object_id >= OBJECT_EVENTS_COUNT)
        return;
    object_event = &gObjectEvents[object_id];
    if (!IsCurrentObjectEvent(object_event))
        return;

    /* A failed sprite setup can leave the freshly-created ObjectEvent active.
     * Reclaim it only when the sprite is absent or still points back to this
     * newly-created slot; never destroy a sprite that another owner acquired.
     */
    if (object_event->spriteId >= MAX_SPRITES)
    {
        object_event->active = FALSE;
        return;
    }
    if (IsRendererSpriteProof(object_id, object_event->spriteId,
                              sCoopPresenceRuntime.renderer_generation,
                              object_event->graphicsId, TRUE))
        RemoveObjectEvent(object_event);
    else
        object_event->active = FALSE;
}

static void RememberRendererIdentity(const struct CoopPresenceRemote *remote,
                                     u8 object_id,
                                     const struct ObjectEvent *object_event)
{
    sCoopPresenceRuntime.rendered_handle = remote->handle;
    sCoopPresenceRuntime.rendered_avatar_id = remote->state.pose.avatar_id;
    sCoopPresenceRuntime.rendered_elevation = remote->state.pose.elevation;
    sCoopPresenceRuntime.rendered_direction = DIR_NONE;
    sCoopPresenceRuntime.rendered_animation = 0xFF;
    sCoopPresenceRuntime.rendered_map_group = object_event->mapGroup;
    sCoopPresenceRuntime.rendered_map_num = object_event->mapNum;
    sCoopPresenceRuntime.rendered_object_id = object_id;
    sCoopPresenceRuntime.rendered_sprite_id = object_event->spriteId;
    sCoopPresenceRuntime.rendered_generation =
        sCoopPresenceRuntime.renderer_generation;
    sCoopPresenceRuntime.rendered_x = object_event->currentCoords.x;
    sCoopPresenceRuntime.rendered_y = object_event->currentCoords.y;
    sCoopPresenceRuntime.interpolation_remaining = 0;
    sCoopPresenceRuntime.renderer_owned = TRUE;
}

static u8 GetRemoteAnimation(const struct CoopPresencePose *pose)
{
    enum Direction direction;

    if (pose == NULL)
        return ANIM_STD_FACE_SOUTH;
    direction = (enum Direction)pose->direction;
    if (pose->animation_id != COOP_PRESENCE_ANIMATION_LOCOMOTION)
        return GetFaceDirectionAnimNum(direction);
    switch (pose->movement_mode)
    {
    case COOP_PRESENCE_MOVEMENT_RUN:
        switch (direction)
        {
        case DIR_NORTH:
            return ANIM_RUN_NORTH;
        case DIR_WEST:
            return ANIM_RUN_WEST;
        case DIR_EAST:
            return ANIM_RUN_EAST;
        case DIR_SOUTH:
        default:
            return ANIM_RUN_SOUTH;
        }
    case COOP_PRESENCE_MOVEMENT_WALK:
        return GetMoveDirectionAnimNum(direction);
    default:
        return GetFaceDirectionAnimNum(direction);
    }
}

static void RemoveOwnedRenderer(void)
{
    u8 object_id;
    struct ObjectEvent *object_event;
    bool8 object_identity;
    bool8 sprite_identity;

    /* Every real removal drops a pending despawn hold with it. The hold
     * path below never calls this until the grace expires. */
    sCoopPresenceRuntime.despawn_hold_reason = 0;
    RemoveFollowerRenderer();
    if (!sCoopPresenceRuntime.renderer_owned)
    {
        object_id = FindRemoteObjectEvent();
        if (object_id < OBJECT_EVENTS_COUNT)
            RetireUnownedRemoteObject(&gObjectEvents[object_id]);
        ClearRendererIdentity();
        return;
    }

    object_id = sCoopPresenceRuntime.rendered_object_id;
    if (object_id >= OBJECT_EVENTS_COUNT
     || sCoopPresenceRuntime.rendered_sprite_id >= MAX_SPRITES)
    {
        /* The cached identity is no longer resolvable.  Abandon ownership
         * without touching a slot that may now belong to another subsystem. */
        ClearRendererIdentity();
        return;
    }

    object_event = &gObjectEvents[object_id];
    object_identity = object_event->active
        && object_event->localId == COOP_PRESENCE_RUNTIME_OBJECT_LOCAL_ID
        && object_event->movementType == MOVEMENT_TYPE_NONE
        && object_event->mapGroup == sCoopPresenceRuntime.rendered_map_group
        && object_event->mapNum == sCoopPresenceRuntime.rendered_map_num;
    sprite_identity = IsRendererSpriteProof(
        object_id, sCoopPresenceRuntime.rendered_sprite_id,
        sCoopPresenceRuntime.rendered_generation, GetRenderedGraphicsId(),
        FALSE);

    if (sprite_identity)
    {
        if (object_identity
         && object_event->spriteId == sCoopPresenceRuntime.rendered_sprite_id)
            RemoveObjectEvent(object_event);
        else
        {
            /* The ObjectEvent backlink can change during return-to-field.
             * The generation/type marker still proves this cached sprite is
             * ours, so reclaim only the sprite and retire a still-reserved
             * stale object.  A foreign sprite is never touched. */
            DestroyCachedOwnedSprite(object_id,
                                     sCoopPresenceRuntime.rendered_sprite_id,
                                     sCoopPresenceRuntime.rendered_generation,
                                     GetRenderedGraphicsId());
            if (object_identity)
                object_event->active = FALSE;
        }
    }
    else if (object_identity)
    {
        /* The sprite slot was reset or rebound.  Its generation proof is
         * gone; retire only the reserved ObjectEvent and leave that slot. */
        object_event->active = FALSE;
    }
    ClearRendererIdentity();
}

static bool8 AddMapOffset(s16 local, s16 *map)
{
    s32 value = (s32)local + MAP_OFFSET;

    if (map == NULL || value < 0 || value > 32767)
        return FALSE;
    *map = (s16)value;
    return TRUE;
}

static bool8 RemoteCoordinatesValid(const struct CoopPresenceRemote *remote,
                                    s16 *map_x, s16 *map_y)
{
    const struct MapLayout *layout;
    s16 x;
    s16 y;
    s32 left;
    s32 right;
    s32 top;
    s32 bottom;

    if (remote == NULL || map_x == NULL || map_y == NULL || gSaveBlock1Ptr == NULL
     || gMapHeader.mapLayout == NULL
     || remote->state.pose.location.map_group != gSaveBlock1Ptr->location.mapGroup
     || remote->state.pose.location.map_number != gSaveBlock1Ptr->location.mapNum
     || remote->state.pose.elevation > ELEVATION_MULTI_LEVEL)
        return FALSE;
    layout = gMapHeader.mapLayout;
    if (!AddMapOffset(remote->state.pose.location.x, &x)
     || !AddMapOffset(remote->state.pose.location.y, &y)
     || x < MAP_OFFSET || y < MAP_OFFSET
     || x >= layout->width + MAP_OFFSET || y >= layout->height + MAP_OFFSET)
        return FALSE;

    left = (s32)gSaveBlock1Ptr->pos.x - 2;
    right = (s32)gSaveBlock1Ptr->pos.x + MAP_OFFSET_W + 2;
    top = gSaveBlock1Ptr->pos.y;
    bottom = (s32)gSaveBlock1Ptr->pos.y + MAP_OFFSET_H + 2;
    if ((s32)x < left || (s32)x > right || (s32)y < top || (s32)y > bottom)
        return FALSE;
    if (MapGridGetElevationAt(x, y) == ELEVATION_INVALID)
        return FALSE;
    *map_x = x;
    *map_y = y;
    return TRUE;
}

static bool8 GetRemoteSpriteTarget(struct ObjectEvent *object_event, s16 map_x,
                                   s16 map_y, s32 *target_x, s32 *target_y)
{
    const struct ObjectEventGraphicsInfo *graphics;
    s16 x;
    s16 y;

    if (object_event == NULL || target_x == NULL || target_y == NULL)
        return FALSE;
    SetSpritePosToMapCoords(map_x, map_y, &x, &y);
    graphics = GetObjectEventGraphicsInfo(object_event->graphicsId);
    if (graphics == NULL)
        return FALSE;
    *target_x = (s32)x + 8;
    *target_y = (s32)y + 16 - (graphics->height >> 1);
    return TRUE;
}

static u8 GetRemoteInterpolationFrames(u8 movement_mode, s32 offset_x, s32 offset_y)
{
    s32 distance = max(abs(offset_x), abs(offset_y));
    u8 speed = movement_mode == COOP_PRESENCE_MOVEMENT_RUN ? 2 : 1;

    if (movement_mode == COOP_PRESENCE_MOVEMENT_IDLE)
        return COOP_PRESENCE_RUNTIME_INTERPOLATION_FRAMES;
    /* Match on-foot engine speed; cap catch-up time to two tiles of travel. */
    return max(1, (min(distance, 32) + speed - 1) / speed);
}

static s32 GetRemoteInterpolationOffset(s32 start)
{
    u8 elapsed;

    if (sCoopPresenceRuntime.interpolation_remaining == 0)
        return 0;
    elapsed = sCoopPresenceRuntime.interpolation_duration
        - sCoopPresenceRuntime.interpolation_remaining;
    return start - start * elapsed / sCoopPresenceRuntime.interpolation_duration;
}

static bool8 EnsureRemoteRenderer(const struct CoopPresenceRemote *remote)
{
    struct ObjectEvent *object_event;
    struct Sprite *sprite;
    const struct ObjectEventGraphicsInfo *graphics;
    u16 graphics_id;
    s16 map_x;
    s16 map_y;
    u8 object_id;
    u8 created_id;
    bool8 created = FALSE;

    if (!RemoteCoordinatesValid(remote, &map_x, &map_y))
        return FALSE;
    graphics_id = CoopCharacter_GetGraphicsId(remote->state.pose.avatar_id);
    graphics = GetObjectEventGraphicsInfo(graphics_id);
    if (graphics == NULL)
        return FALSE;
    if (graphics->paletteTag != TAG_NONE
     && LoadObjectEventPalette(graphics->paletteTag) == 0xFF)
        return FALSE;

    object_id = FindRemoteObjectEvent();
    if (object_id >= OBJECT_EVENTS_COUNT)
    {
        if (GetFirstInactiveObjectEventId() >= OBJECT_EVENTS_COUNT)
            return FALSE;
        created_id = SpawnSpecialObjectEventParameterized(
            graphics_id, MOVEMENT_TYPE_NONE, COOP_PRESENCE_RUNTIME_OBJECT_LOCAL_ID,
            map_x, map_y, remote->state.pose.elevation);
        if (created_id >= OBJECT_EVENTS_COUNT)
            return FALSE;
        if (!IsCurrentObjectEvent(&gObjectEvents[created_id]))
        {
            AbandonCreatedRenderer(created_id);
            return FALSE;
        }
        object_id = created_id;
        created = TRUE;
        sCoopPresenceRuntime.renderer_generation = NextRendererGeneration();
        gSprites[gObjectEvents[object_id].spriteId].data[7] =
            (s16)sCoopPresenceRuntime.renderer_generation;
    }

    object_event = &gObjectEvents[object_id];
    if (!FindOwnedSprite(object_event,
                         created ? sCoopPresenceRuntime.renderer_generation
                                 : sCoopPresenceRuntime.rendered_generation,
                         &sprite))
    {
        if (created)
            AbandonCreatedRenderer(object_id);
        return FALSE;
    }
    if (created)
    {
        RememberRendererIdentity(remote, object_id, object_event);
        object_event->initialCoords = object_event->currentCoords;
    }
    else if (!sCoopPresenceRuntime.renderer_owned
          || object_id != sCoopPresenceRuntime.rendered_object_id)
    {
        /* A reserved slot can survive a runtime reset.  Adopt it only after
         * resolving the current ObjectEvent and sprite identities and
         * confirming that it already has the expected remote graphics. */
        if (object_event->isPlayer || object_event->graphicsId != graphics_id)
            return FALSE;
        RememberRendererIdentity(remote, object_id, object_event);
        object_event->initialCoords = object_event->currentCoords;
    }
    else if (object_event->spriteId != sCoopPresenceRuntime.rendered_sprite_id
          || object_event->mapGroup != sCoopPresenceRuntime.rendered_map_group
          || object_event->mapNum != sCoopPresenceRuntime.rendered_map_num)
    {
        /* The reserved ObjectEvent may have been rebound after the last
         * frame.  Do not mutate a newly attached sprite through stale cache
         * identity; relinquish ownership and let the next update re-resolve
         * the slot from scratch. */
        RemoveOwnedRenderer();
        return FALSE;
    }
    else if (sCoopPresenceRuntime.rendered_handle != remote->handle
          || sCoopPresenceRuntime.rendered_avatar_id != remote->state.pose.avatar_id
          || sCoopPresenceRuntime.rendered_elevation != remote->state.pose.elevation)
    {
        RemoveOwnedRenderer();
        return EnsureRemoteRenderer(remote);
    }
    if (object_event->graphicsId != graphics_id)
    {
        ObjectEventSetGraphicsId(object_event, graphics_id);
        if (!FindOwnedSprite(object_event,
                             sCoopPresenceRuntime.rendered_generation,
                             &sprite))
        {
            RemoveOwnedRenderer();
            return FALSE;
        }
        sCoopPresenceRuntime.interpolation_remaining = 0;
    }

    if (!GetRemoteSpriteTarget(object_event, map_x, map_y,
                               &sCoopPresenceRuntime.sprite_target_x,
                               &sCoopPresenceRuntime.sprite_target_y))
        return FALSE;

    if (object_event->currentCoords.x != map_x || object_event->currentCoords.y != map_y
     || object_event->currentElevation != remote->state.pose.elevation
     || object_event->previousElevation != remote->state.pose.elevation)
    {
        bool8 coords_differ = object_event->currentCoords.x != map_x
            || object_event->currentCoords.y != map_y;
        s32 dx = (s32)map_x - object_event->currentCoords.x;
        s32 dy = (s32)map_y - object_event->currentCoords.y;
        s32 start_x = GetRemoteInterpolationOffset(sCoopPresenceRuntime.interpolation_start_x) - dx * 16;
        s32 start_y = GetRemoteInterpolationOffset(sCoopPresenceRuntime.interpolation_start_y) - dy * 16;
        bool8 snap = !coords_differ || dx > 2 || dx < -2 || dy > 2 || dy < -2
            || object_event->currentElevation != remote->state.pose.elevation
            || object_event->previousElevation != remote->state.pose.elevation;
        /* MoveObjectEventToMapCoords updates the complete engine position
         * state and ground-effect bookkeeping.  Retain the unfinished step
         * as a pixel offset from the new tile, independent of the camera. */
        MoveObjectEventToMapCoords(object_event, map_x, map_y);
        object_event->initialCoords = object_event->currentCoords;
        object_event->currentElevation = remote->state.pose.elevation;
        object_event->previousElevation = remote->state.pose.elevation;
        sCoopPresenceRuntime.sprite_target_x = sprite->x;
        sCoopPresenceRuntime.sprite_target_y = sprite->y;
        sCoopPresenceRuntime.rendered_x = map_x;
        sCoopPresenceRuntime.rendered_y = map_y;
        if (!snap)
        {
            sCoopPresenceRuntime.interpolation_start_x = start_x;
            sCoopPresenceRuntime.interpolation_start_y = start_y;
            sCoopPresenceRuntime.interpolation_duration =
                GetRemoteInterpolationFrames(remote->state.pose.movement_mode, start_x, start_y);
            sCoopPresenceRuntime.interpolation_remaining =
                sCoopPresenceRuntime.interpolation_duration;
        }
        else
        {
            sCoopPresenceRuntime.interpolation_remaining = 0;
        }
    }

    {
        enum Direction direction = (enum Direction)remote->state.pose.direction;
        u8 animation = GetRemoteAnimation(&remote->state.pose);

        /* StartSpriteAnimInDirection seeks command zero.  Calling it from
         * every frame update therefore prevents walking/running animations
         * from ever reaching their later commands.  Direction is still
         * refreshed each frame, but the seek is limited to a real change. */
        if (sCoopPresenceRuntime.rendered_direction != direction
         || sCoopPresenceRuntime.rendered_animation != animation)
            StartSpriteAnimInDirection(object_event, sprite, direction, animation);
        else
            SetObjectEventDirection(object_event, direction);
        sCoopPresenceRuntime.rendered_direction = direction;
        sCoopPresenceRuntime.rendered_animation = animation;
    }
    sprite->x2 = 0;
    sprite->y2 = 0;
    if (sCoopPresenceRuntime.interpolation_remaining != 0)
        sCoopPresenceRuntime.interpolation_remaining--;
    sprite->x = (s16)(sCoopPresenceRuntime.sprite_target_x
        + GetRemoteInterpolationOffset(sCoopPresenceRuntime.interpolation_start_x));
    sprite->y = (s16)(sCoopPresenceRuntime.sprite_target_y
        + GetRemoteInterpolationOffset(sCoopPresenceRuntime.interpolation_start_y));
    SetObjectSubpriorityByElevation(remote->state.pose.elevation, sprite, 1);
    return TRUE;
}

static bool8 IsOwnedRendererEffectivelyVisible(const struct CoopPresenceRemote *remote)
{
    struct ObjectEvent *object_event;
    struct Sprite *sprite;
    s16 map_x;
    s16 map_y;
    u8 object_id;
    u16 graphics_id;

    if (remote == NULL || !sCoopPresenceRuntime.renderer_owned
     || sCoopPresenceRuntime.rendered_handle != remote->handle)
        return FALSE;
    object_id = FindRemoteObjectEvent();
    if (object_id >= OBJECT_EVENTS_COUNT
     || object_id != sCoopPresenceRuntime.rendered_object_id)
        return FALSE;
    object_event = &gObjectEvents[object_id];
    if (!IsCurrentObjectEvent(object_event)
     || object_event->spriteId != sCoopPresenceRuntime.rendered_sprite_id
     || object_event->isPlayer || object_event->invisible || object_event->offScreen
     || !FindOwnedSprite(object_event,
                         sCoopPresenceRuntime.rendered_generation,
                         &sprite) || sprite->invisible)
        return FALSE;
    graphics_id = CoopCharacter_GetGraphicsId(remote->state.pose.avatar_id);
    if (object_event->graphicsId != graphics_id
     || !RemoteCoordinatesValid(remote, &map_x, &map_y)
     || object_event->currentCoords.x != map_x
     || object_event->currentCoords.y != map_y
     || object_event->currentElevation != remote->state.pose.elevation
     || object_event->previousElevation != remote->state.pose.elevation)
        return FALSE;
    return TRUE;
}

static bool8 IsTransientDespawnReason(u8 reason)
{
    return reason == COOP_PRESENCE_DESPAWN_HIDDEN
        || reason == COOP_PRESENCE_DESPAWN_DISCONNECTED;
}

static bool8 DespawnHoldActive(void)
{
    return sCoopPresenceRuntime.despawn_hold_reason != 0
        && sCoopPresenceRuntime.frame_counter - sCoopPresenceRuntime.despawn_hold_start
            < COOP_PRESENCE_RUNTIME_DESPAWN_HOLD_FRAMES;
}

static EWRAM_DATA u16 sCoopFollowerGeneration = 0;

static u16 NextFollowerGeneration(void)
{
    sCoopFollowerGeneration++;
    if (sCoopFollowerGeneration == 0)
        sCoopFollowerGeneration++;
    return sCoopFollowerGeneration;
}

bool8 CoopPresenceRuntime_IsFollowerObject(const struct ObjectEvent *object_event)
{
    return object_event != NULL
        && object_event->localId == COOP_PRESENCE_RUNTIME_FOLLOWER_LOCAL_ID;
}

static bool8 IsCurrentFollowerObject(const struct ObjectEvent *object_event)
{
    return gSaveBlock1Ptr != NULL && object_event != NULL && object_event->active
        && object_event->localId == COOP_PRESENCE_RUNTIME_FOLLOWER_LOCAL_ID
        && object_event->mapGroup == gSaveBlock1Ptr->location.mapGroup
        && object_event->mapNum == gSaveBlock1Ptr->location.mapNum;
}

static u8 FindFollowerObjectEvent(void)
{
    u8 i;

    if (gSaveBlock1Ptr == NULL)
        return OBJECT_EVENTS_COUNT;
    for (i = 0; i < OBJECT_EVENTS_COUNT; i++)
    {
        if (IsCurrentFollowerObject(&gObjectEvents[i]))
            return i;
    }
    return OBJECT_EVENTS_COUNT;
}

static bool8 IsFollowerSpriteProof(u8 object_id, u8 sprite_id, u16 generation,
                                   u16 graphics_id, bool8 require_backlink)
{
    const struct ObjectEvent *object_event;
    struct Sprite *sprite;

    if (object_id >= OBJECT_EVENTS_COUNT || sprite_id >= MAX_SPRITES
     || generation == 0)
        return FALSE;
    object_event = &gObjectEvents[object_id];
    if ((require_backlink && !object_event->active)
     || object_event->localId != COOP_PRESENCE_RUNTIME_FOLLOWER_LOCAL_ID
     || object_event->movementType != MOVEMENT_TYPE_NONE
     || (require_backlink && object_event->graphicsId != graphics_id)
     || (require_backlink && object_event->spriteId != sprite_id))
        return FALSE;
    sprite = &gSprites[sprite_id];
    if (!sprite->inUse || sprite->data[0] != (s16)object_id
     || (u16)sprite->data[7] != generation
     || sprite->callback != MovementType_None)
        return FALSE;
    return TRUE;
}

static bool8 FindOwnedFollowerSprite(struct ObjectEvent *object_event, u16 generation,
                                     struct Sprite **out)
{
    u8 object_id;

    if (!IsCurrentFollowerObject(object_event)
     || object_event->spriteId >= MAX_SPRITES)
        return FALSE;
    object_id = (u8)(object_event - gObjectEvents);
    if (!IsFollowerSpriteProof(object_id, object_event->spriteId, generation,
                               object_event->graphicsId, TRUE))
        return FALSE;
    if (out != NULL)
        *out = &gSprites[object_event->spriteId];
    return TRUE;
}

static u16 FollowerGraphicsId(u16 species, u8 flags)
{
    u16 graphics_id = (u16)(OBJ_EVENT_MON | species);

    if ((flags & COOP_PRESENCE_COMPANION_FLAG_SHINY) != 0)
        graphics_id = (u16)(graphics_id | OBJ_EVENT_MON_SHINY);
    return graphics_id;
}

static void ClearFollowerIdentity(void)
{
    sCoopPresenceRuntime.follower_owned = FALSE;
    sCoopPresenceRuntime.follower_object_id = OBJECT_EVENTS_COUNT;
    sCoopPresenceRuntime.follower_sprite_id = MAX_SPRITES;
    sCoopPresenceRuntime.follower_generation = 0;
    sCoopPresenceRuntime.follower_species = 0;
    sCoopPresenceRuntime.follower_flags = 0;
    sCoopPresenceRuntime.follower_x = 0;
    sCoopPresenceRuntime.follower_y = 0;
}

static void RemoveFollowerRenderer(void)
{
    u8 object_id;
    struct ObjectEvent *object_event;

    if (!sCoopPresenceRuntime.follower_owned)
    {
        object_id = FindFollowerObjectEvent();
        if (object_id < OBJECT_EVENTS_COUNT)
        {
            object_event = &gObjectEvents[object_id];
            if (!FindOwnedFollowerSprite(object_event,
                                         sCoopPresenceRuntime.follower_generation,
                                         NULL))
                object_event->active = FALSE;
        }
        ClearFollowerIdentity();
        return;
    }

    object_id = sCoopPresenceRuntime.follower_object_id;
    if (object_id < OBJECT_EVENTS_COUNT
     && sCoopPresenceRuntime.follower_sprite_id < MAX_SPRITES
     && IsFollowerSpriteProof(object_id, sCoopPresenceRuntime.follower_sprite_id,
                              sCoopPresenceRuntime.follower_generation,
                              FollowerGraphicsId(sCoopPresenceRuntime.follower_species,
                                                 sCoopPresenceRuntime.follower_flags),
                              FALSE))
    {
        object_event = &gObjectEvents[object_id];
        if (object_event->active
         && object_event->localId == COOP_PRESENCE_RUNTIME_FOLLOWER_LOCAL_ID
         && object_event->spriteId == sCoopPresenceRuntime.follower_sprite_id)
            RemoveObjectEvent(object_event);
        else if (object_event->active
              && object_event->localId == COOP_PRESENCE_RUNTIME_FOLLOWER_LOCAL_ID)
            object_event->active = FALSE;
    }
    ClearFollowerIdentity();
}

static void ClearSocialState(void)
{
    sCoopPresenceRuntime.remote_companion_valid = FALSE;
    sCoopPresenceRuntime.remote_companion_handle = 0;
    sCoopPresenceRuntime.remote_companion_sequence = 0;
    sCoopPresenceRuntime.remote_companion_species = 0;
    sCoopPresenceRuntime.remote_companion_flags = 0;
    sCoopPresenceRuntime.remote_signal_sequence = 0;
    sCoopPresenceRuntime.remote_signal_seen = FALSE;
    sCoopPresenceRuntime.pending_bubble_set = FALSE;
    sCoopPresenceRuntime.pending_bubble = 0;
    sCoopPresenceRuntime.ping_valid = FALSE;
    RemoveFollowerRenderer();
}

/* Tile behind the remote avatar: the follower trails instead of stacking. */
static bool8 FollowerTargetTile(const struct CoopPresenceRemote *remote, s16 map_x, s16 map_y,
                                s16 *out_x, s16 *out_y)
{
    s16 x = map_x;
    s16 y = map_y;

    if (remote == NULL || out_x == NULL || out_y == NULL)
        return FALSE;
    switch (remote->state.pose.direction)
    {
    case COOP_PRESENCE_DIRECTION_SOUTH:
        y--;
        break;
    case COOP_PRESENCE_DIRECTION_NORTH:
        y++;
        break;
    case COOP_PRESENCE_DIRECTION_WEST:
        x++;
        break;
    case COOP_PRESENCE_DIRECTION_EAST:
        x--;
        break;
    default:
        return FALSE;
    }
    if (gMapHeader.mapLayout == NULL
     || x < MAP_OFFSET || y < MAP_OFFSET
     || x >= gMapHeader.mapLayout->width + MAP_OFFSET
     || y >= gMapHeader.mapLayout->height + MAP_OFFSET)
        return FALSE;
    *out_x = x;
    *out_y = y;
    return TRUE;
}

static bool8 EnsureFollowerRenderer(const struct CoopPresenceRemote *remote,
                                    s16 map_x, s16 map_y)
{
    struct ObjectEvent *object_event;
    struct Sprite *sprite;
    const struct ObjectEventGraphicsInfo *graphics;
    u16 graphics_id;
    s16 target_x;
    s16 target_y;
    u8 object_id;
    u8 created_id;
    u16 generation;

    if (remote == NULL || !sCoopPresenceRuntime.remote_companion_valid
     || remote->handle != sCoopPresenceRuntime.remote_companion_handle
     || sCoopPresenceRuntime.remote_companion_species == 0
     || sCoopPresenceRuntime.remote_companion_species >= NUM_SPECIES)
        return FALSE;
    if (!FollowerTargetTile(remote, map_x, map_y, &target_x, &target_y))
    {
        target_x = map_x;
        target_y = (s16)(map_y + 1);
    }
    graphics_id = FollowerGraphicsId(sCoopPresenceRuntime.remote_companion_species,
                                     sCoopPresenceRuntime.remote_companion_flags);
    graphics = GetObjectEventGraphicsInfo(graphics_id);
    if (graphics == NULL)
        return FALSE;
    if (graphics->paletteTag != TAG_NONE
     && LoadObjectEventPalette(graphics->paletteTag) == 0xFF)
        return FALSE;

    if (sCoopPresenceRuntime.follower_owned
     && (sCoopPresenceRuntime.follower_species != sCoopPresenceRuntime.remote_companion_species
      || sCoopPresenceRuntime.follower_flags != sCoopPresenceRuntime.remote_companion_flags))
        RemoveFollowerRenderer();

    if (!sCoopPresenceRuntime.follower_owned)
    {
        object_id = FindFollowerObjectEvent();
        if (object_id >= OBJECT_EVENTS_COUNT)
        {
            if (GetFirstInactiveObjectEventId() >= OBJECT_EVENTS_COUNT)
                return FALSE;
            created_id = SpawnSpecialObjectEventParameterized(
                graphics_id, MOVEMENT_TYPE_NONE, COOP_PRESENCE_RUNTIME_FOLLOWER_LOCAL_ID,
                target_x, target_y, remote->state.pose.elevation);
            if (created_id >= OBJECT_EVENTS_COUNT)
                return FALSE;
            if (!IsCurrentFollowerObject(&gObjectEvents[created_id]))
            {
                gObjectEvents[created_id].active = FALSE;
                return FALSE;
            }
            object_id = created_id;
            generation = NextFollowerGeneration();
            gSprites[gObjectEvents[object_id].spriteId].data[7] = (s16)generation;
            sCoopPresenceRuntime.follower_owned = TRUE;
            sCoopPresenceRuntime.follower_object_id = object_id;
            sCoopPresenceRuntime.follower_sprite_id = gObjectEvents[object_id].spriteId;
            sCoopPresenceRuntime.follower_generation = generation;
            sCoopPresenceRuntime.follower_species = sCoopPresenceRuntime.remote_companion_species;
            sCoopPresenceRuntime.follower_flags = sCoopPresenceRuntime.remote_companion_flags;
            sCoopPresenceRuntime.follower_x = target_x;
            sCoopPresenceRuntime.follower_y = target_y;
            sCoopPresenceRuntime.follower_step_frame = sCoopPresenceRuntime.frame_counter;
        }
        else
        {
            object_event = &gObjectEvents[object_id];
            if (object_event->isPlayer || object_event->graphicsId != graphics_id)
                return FALSE;
            if (!FindOwnedFollowerSprite(object_event,
                                         sCoopPresenceRuntime.follower_generation,
                                         NULL))
                return FALSE;
            sCoopPresenceRuntime.follower_owned = TRUE;
            sCoopPresenceRuntime.follower_object_id = object_id;
            sCoopPresenceRuntime.follower_sprite_id = object_event->spriteId;
            sCoopPresenceRuntime.follower_species = sCoopPresenceRuntime.remote_companion_species;
            sCoopPresenceRuntime.follower_flags = sCoopPresenceRuntime.remote_companion_flags;
            sCoopPresenceRuntime.follower_x = object_event->currentCoords.x;
            sCoopPresenceRuntime.follower_y = object_event->currentCoords.y;
            sCoopPresenceRuntime.follower_step_frame = sCoopPresenceRuntime.frame_counter;
        }
    }

    object_id = sCoopPresenceRuntime.follower_object_id;
    if (object_id >= OBJECT_EVENTS_COUNT)
    {
        RemoveFollowerRenderer();
        return FALSE;
    }
    object_event = &gObjectEvents[object_id];
    if (!FindOwnedFollowerSprite(object_event,
                                 sCoopPresenceRuntime.follower_generation,
                                 &sprite))
    {
        RemoveFollowerRenderer();
        return FALSE;
    }
    if (object_event->graphicsId != graphics_id)
    {
        RemoveFollowerRenderer();
        return FALSE;
    }

    if ((sCoopPresenceRuntime.follower_x != target_x
      || sCoopPresenceRuntime.follower_y != target_y)
     && sCoopPresenceRuntime.frame_counter - sCoopPresenceRuntime.follower_step_frame
        >= COOP_PRESENCE_RUNTIME_FOLLOWER_STEP_INTERVAL)
    {
        s16 step_x = sCoopPresenceRuntime.follower_x;
        s16 step_y = sCoopPresenceRuntime.follower_y;

        if (step_x != target_x)
            step_x += (s16)((target_x > step_x) ? 1 : -1);
        else if (step_y != target_y)
            step_y += (s16)((target_y > step_y) ? 1 : -1);
        MoveObjectEventToMapCoords(object_event, step_x, step_y);
        SetObjectEventDirection(object_event,
            (step_x != sCoopPresenceRuntime.follower_x)
                ? (step_x > sCoopPresenceRuntime.follower_x ? DIR_EAST : DIR_WEST)
                : (step_y > sCoopPresenceRuntime.follower_y ? DIR_SOUTH : DIR_NORTH));
        sCoopPresenceRuntime.follower_x = step_x;
        sCoopPresenceRuntime.follower_y = step_y;
        sCoopPresenceRuntime.follower_step_frame = sCoopPresenceRuntime.frame_counter;
    }
    (void)sprite;
    return TRUE;
}

static u8 CoopEmoteToFollowerEmotion(u8 emote)
{
    switch (emote)
    {
    case COOP_PRESENCE_EMOTE_EXCLAIM:
        return FOLLOWER_EMOTION_SURPRISE;
    case COOP_PRESENCE_EMOTE_QUESTION:
        return FOLLOWER_EMOTION_CURIOUS;
    case COOP_PRESENCE_EMOTE_HEART:
        return FOLLOWER_EMOTION_LOVE;
    case COOP_PRESENCE_EMOTE_MUSIC:
        return FOLLOWER_EMOTION_MUSIC;
    case COOP_PRESENCE_EMOTE_SWEAT:
        return FOLLOWER_EMOTION_UPSET;
    case COOP_PRESENCE_EMOTE_ANGER:
        return FOLLOWER_EMOTION_ANGRY;
    case COOP_PRESENCE_EMOTE_SLEEP:
        return FOLLOWER_EMOTION_NEUTRAL;
    case COOP_PRESENCE_EMOTE_STAR:
    default:
        return FOLLOWER_EMOTION_HAPPY;
    }
}

static void ShowBubbleOnObject(struct ObjectEvent *object_event, u8 emotion)
{
    if (object_event == NULL || !object_event->active
     || object_event->spriteId >= MAX_SPRITES
     || !gSprites[object_event->spriteId].inUse)
        return;
    ObjectEventGetLocalIdAndMap(object_event,
        &gFieldEffectArguments[0], &gFieldEffectArguments[1], &gFieldEffectArguments[2]);
    gFieldEffectArguments[7] = (s32)(emotion % FOLLOWER_EMOTION_LENGTH);
    FieldEffectStart(FLDEFF_EMOTE);
}

static void ConsumePendingBubble(void)
{
    u8 object_id;
    struct ObjectEvent *object_event;

    if (!sCoopPresenceRuntime.pending_bubble_set)
        return;
    /* A stale bubble is dropped rather than shown late. */
    if (sCoopPresenceRuntime.frame_counter - sCoopPresenceRuntime.pending_bubble_frame > 90)
    {
        sCoopPresenceRuntime.pending_bubble_set = FALSE;
        return;
    }
    if (sCoopPresenceRuntime.renderer_owned)
    {
        object_id = sCoopPresenceRuntime.rendered_object_id;
        if (object_id < OBJECT_EVENTS_COUNT)
        {
            object_event = &gObjectEvents[object_id];
            if (IsCurrentObjectEvent(object_event)
             && FindOwnedSprite(object_event,
                                sCoopPresenceRuntime.rendered_generation,
                                NULL))
                ShowBubbleOnObject(object_event, sCoopPresenceRuntime.pending_bubble);
        }
    }
    sCoopPresenceRuntime.pending_bubble_set = FALSE;
}

static bool8 ReadLeadCompanion(u16 *species, u8 *flags)
{
    u8 i;

    if (species == NULL || flags == NULL || gSaveBlock1Ptr == NULL)
        return FALSE;
    for (i = 0; i < PARTY_SIZE; i++)
    {
        u16 candidate = (u16)GetMonData(&gPlayerParty[i], MON_DATA_SPECIES);

        if (candidate == SPECIES_NONE || candidate == SPECIES_EGG
         || candidate >= NUM_SPECIES)
            continue;
        if (GetMonData(&gPlayerParty[i], MON_DATA_IS_EGG))
            continue;
        *species = candidate;
        *flags = IsMonShiny(&gPlayerParty[i]) ? COOP_PRESENCE_COMPANION_FLAG_SHINY : 0;
        return TRUE;
    }
    return FALSE;
}

static void MaybePublishCompanion(void)
{
    struct CoopPresenceLocalCompanion companion;
    u8 payload[COOP_PRESENCE_LOCAL_COMPANION_SIZE];
    u16 species;
    u8 flags;

    if (!sCoopPresenceRuntime.initialized || !sCoopPresenceRuntime.transport_ready
     || sCoopPresenceRuntime.warp_sequence == 0 || !IsOverworldPoseAllowed())
        return;
    if (!ReadLeadCompanion(&species, &flags))
        return;
    if (species == sCoopPresenceRuntime.last_companion_species
     && flags == sCoopPresenceRuntime.last_companion_flags
     && sCoopPresenceRuntime.last_companion_frame != 0
     && sCoopPresenceRuntime.frame_counter - sCoopPresenceRuntime.last_companion_frame
        < COOP_PRESENCE_RUNTIME_COMPANION_INTERVAL)
        return;
    sCoopPresenceRuntime.companion_source_sequence = CoopPresence_NextSequence(
        sCoopPresenceRuntime.companion_source_sequence);
    companion.species = species;
    companion.form = 0;
    companion.flags = flags;
    companion.source_sequence = sCoopPresenceRuntime.companion_source_sequence;
    if (!CoopPresence_EncodeLocalCompanion(&companion, payload, sizeof(payload)))
        return;
    if (!CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_COMPANION_STATE,
                                            payload, sizeof(payload)))
        return;
    sCoopPresenceRuntime.last_companion_species = species;
    sCoopPresenceRuntime.last_companion_flags = flags;
    sCoopPresenceRuntime.last_companion_frame = sCoopPresenceRuntime.frame_counter;
}

static bool8 CanUseSignal(void)
{
    if (!sCoopPresenceRuntime.initialized || !sCoopPresenceRuntime.transport_ready
     || !IsOverworldPoseAllowed())
        return FALSE;
    if (sCoopPresenceRuntime.signal_used
     && sCoopPresenceRuntime.frame_counter - sCoopPresenceRuntime.last_signal_frame
        < COOP_PRESENCE_RUNTIME_SIGNAL_COOLDOWN)
        return FALSE;
    return TRUE;
}

static bool8 EnqueueSignal(u8 kind, u8 emote, s16 x, s16 y)
{
    struct CoopPresenceLocalSignal signal;
    u8 payload[COOP_PRESENCE_LOCAL_SIGNAL_SIZE];

    if (!CanUseSignal())
        return FALSE;
    sCoopPresenceRuntime.signal_source_sequence = CoopPresence_NextSequence(
        sCoopPresenceRuntime.signal_source_sequence);
    signal.kind = kind;
    signal.emote = emote;
    signal.x = x;
    signal.y = y;
    signal.source_sequence = sCoopPresenceRuntime.signal_source_sequence;
    if (!CoopPresence_EncodeLocalSignal(&signal, payload, sizeof(payload)))
        return FALSE;
    if (!CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_SOCIAL_SIGNAL,
                                            payload, sizeof(payload)))
        return FALSE;
    sCoopPresenceRuntime.signal_used = TRUE;
    sCoopPresenceRuntime.last_signal_frame = sCoopPresenceRuntime.frame_counter;
    return TRUE;
}

bool8 CoopPresenceRuntime_TryPing(void)
{
    const struct ObjectEvent *player;
    s16 x;
    s16 y;

    if (!CanUseSignal() || !IsPlayerBindingValid())
        return FALSE;
    player = &gObjectEvents[gPlayerAvatar.objectEventId];
    x = (s16)(player->currentCoords.x - MAP_OFFSET);
    y = (s16)(player->currentCoords.y - MAP_OFFSET);
    return EnqueueSignal(COOP_PRESENCE_SIGNAL_PING, COOP_PRESENCE_EMOTE_NONE, x, y);
}

bool8 CoopPresenceRuntime_TryEmote(void)
{
    u8 emote;
    const struct ObjectEvent *player;

    if (!CanUseSignal())
        return FALSE;
    if (sCoopPresenceRuntime.next_emote < COOP_PRESENCE_EMOTE_EXCLAIM
     || sCoopPresenceRuntime.next_emote > COOP_PRESENCE_EMOTE_MAX)
        sCoopPresenceRuntime.next_emote = COOP_PRESENCE_EMOTE_EXCLAIM;
    emote = sCoopPresenceRuntime.next_emote;
    sCoopPresenceRuntime.next_emote = (u8)(emote >= COOP_PRESENCE_EMOTE_MAX
        ? COOP_PRESENCE_EMOTE_EXCLAIM : emote + 1);
    if (!EnqueueSignal(COOP_PRESENCE_SIGNAL_EMOTE, emote, 0, 0))
        return FALSE;
    if (IsPlayerBindingValid())
    {
        player = &gObjectEvents[gPlayerAvatar.objectEventId];
        ShowBubbleOnObject((struct ObjectEvent *)player,
                           CoopEmoteToFollowerEmotion(emote));
    }
    return TRUE;
}

bool8 CoopPresenceRuntime_GetPingMarker(s16 *x, s16 *y)
{
    if (x == NULL || y == NULL || !sCoopPresenceRuntime.ping_valid
     || gSaveBlock1Ptr == NULL
     || sCoopPresenceRuntime.ping_map_group != gSaveBlock1Ptr->location.mapGroup
     || sCoopPresenceRuntime.ping_map_num != gSaveBlock1Ptr->location.mapNum
     || sCoopPresenceRuntime.frame_counter - sCoopPresenceRuntime.ping_frame
        >= COOP_PRESENCE_RUNTIME_PING_TTL_FRAMES)
        return FALSE;
    *x = sCoopPresenceRuntime.ping_x;
    *y = sCoopPresenceRuntime.ping_y;
    return TRUE;
}

static enum CoopPresenceApplyResult ApplyRemoteCompanion(
    const struct CoopPresenceRemoteCompanion *companion)
{
    const struct CoopPresenceRemote *active;

    if (companion == NULL || gSaveBlock1Ptr == NULL)
        return COOP_PRESENCE_APPLY_REJECTED;
    active = CoopPresenceReducer_GetRemote(&sCoopPresenceRuntime.reducer);
    if (active == NULL || active->handle != companion->handle)
        return COOP_PRESENCE_APPLY_STALE;
    if (sCoopPresenceRuntime.remote_companion_valid
     && sCoopPresenceRuntime.remote_companion_handle == companion->handle
     && !CoopPresence_SequenceIsNewer(companion->server_sequence,
                                      sCoopPresenceRuntime.remote_companion_sequence))
        return COOP_PRESENCE_APPLY_STALE;
    sCoopPresenceRuntime.remote_companion_valid = TRUE;
    sCoopPresenceRuntime.remote_companion_handle = companion->handle;
    sCoopPresenceRuntime.remote_companion_sequence = companion->server_sequence;
    sCoopPresenceRuntime.remote_companion_species = companion->species;
    sCoopPresenceRuntime.remote_companion_flags = companion->flags;
    return COOP_PRESENCE_APPLY_APPLIED;
}

static enum CoopPresenceApplyResult ApplyRemoteSignal(
    const struct CoopPresenceRemoteSignal *signal)
{
    const struct CoopPresenceRemote *active;

    if (signal == NULL || gSaveBlock1Ptr == NULL)
        return COOP_PRESENCE_APPLY_REJECTED;
    active = CoopPresenceReducer_GetRemote(&sCoopPresenceRuntime.reducer);
    if (active == NULL || active->handle != signal->handle)
        return COOP_PRESENCE_APPLY_STALE;
    if (sCoopPresenceRuntime.remote_signal_seen
     && !CoopPresence_SequenceIsNewer(signal->server_sequence,
                                      sCoopPresenceRuntime.remote_signal_sequence))
        return COOP_PRESENCE_APPLY_STALE;
    sCoopPresenceRuntime.remote_signal_seen = TRUE;
    sCoopPresenceRuntime.remote_signal_sequence = signal->server_sequence;
    if (signal->kind == COOP_PRESENCE_SIGNAL_PING)
    {
        sCoopPresenceRuntime.ping_x = signal->x;
        sCoopPresenceRuntime.ping_y = signal->y;
        sCoopPresenceRuntime.ping_map_group = gSaveBlock1Ptr->location.mapGroup;
        sCoopPresenceRuntime.ping_map_num = gSaveBlock1Ptr->location.mapNum;
        sCoopPresenceRuntime.ping_frame = sCoopPresenceRuntime.frame_counter;
        sCoopPresenceRuntime.ping_valid = TRUE;
        sCoopPresenceRuntime.pending_bubble =
            CoopEmoteToFollowerEmotion(COOP_PRESENCE_EMOTE_EXCLAIM);
    }
    else
    {
        sCoopPresenceRuntime.pending_bubble = CoopEmoteToFollowerEmotion(signal->emote);
    }
    sCoopPresenceRuntime.pending_bubble_set = TRUE;
    sCoopPresenceRuntime.pending_bubble_frame = sCoopPresenceRuntime.frame_counter;
    return COOP_PRESENCE_APPLY_APPLIED;
}

static void ApplyPendingFrames(void)
{
    struct CoopPresenceLocalState local;
    struct CoopPresencePendingFrame *pending;
    enum CoopPresenceApplyResult result;
    struct WorldLocation location;
    bool8 renderer_owned;

    if (!CoopPresenceRuntime_GetLocalState(&local)
     || !IsWorldLocationCurrent(&location)
     || !CoopPresenceReducer_Synchronize(&sCoopPresenceRuntime.reducer,
                                          sCoopPresenceRuntime.session_epoch,
                                          &location,
                                          sCoopPresenceRuntime.warp_sequence))
        return;

    while (sCoopPresenceRuntime.pending_count != 0)
    {
        pending = &sCoopPresenceRuntime.pending[sCoopPresenceRuntime.pending_read];
        switch (pending->type)
        {
        case COOP_PRESENCE_PENDING_SPAWN:
            result = CoopPresenceReducer_ApplySpawn(&sCoopPresenceRuntime.reducer,
                                                    &pending->value.spawn);
            if (result == COOP_PRESENCE_APPLY_APPLIED)
                sCoopPresenceRuntime.despawn_hold_reason = 0;
            break;
        case COOP_PRESENCE_PENDING_UPDATE:
            result = CoopPresenceReducer_ApplyUpdate(&sCoopPresenceRuntime.reducer,
                                                     &pending->value.update);
            break;
        case COOP_PRESENCE_PENDING_DESPAWN:
            renderer_owned = sCoopPresenceRuntime.renderer_owned;
            result = CoopPresenceReducer_ApplyDespawn(&sCoopPresenceRuntime.reducer,
                                                      &pending->value.despawn);
            if (result == COOP_PRESENCE_APPLY_APPLIED)
            {
                if (IsTransientDespawnReason(pending->value.despawn.reason) && renderer_owned)
                {
                    sCoopPresenceRuntime.despawn_hold_reason = pending->value.despawn.reason;
                    sCoopPresenceRuntime.despawn_hold_start = sCoopPresenceRuntime.frame_counter;
                }
                else
                {
                    sCoopPresenceRuntime.despawn_hold_reason = 0;
                    ClearSocialState();
                }
            }
            break;
        case COOP_PRESENCE_PENDING_COMPANION:
            result = ApplyRemoteCompanion(&pending->value.companion);
            break;
        case COOP_PRESENCE_PENDING_SIGNAL:
            result = ApplyRemoteSignal(&pending->value.signal);
            break;
        default:
            result = COOP_PRESENCE_APPLY_REJECTED;
            break;
        }
        /* Companion and signal records never refresh avatar liveness; only
         * the pose lifecycle owns the stale-removal deadline. */
        if (result == COOP_PRESENCE_APPLY_APPLIED
         && pending->type != COOP_PRESENCE_PENDING_COMPANION
         && pending->type != COOP_PRESENCE_PENDING_SIGNAL)
            sCoopPresenceRuntime.last_lifecycle_frame = sCoopPresenceRuntime.frame_counter;
        pending->type = COOP_PRESENCE_PENDING_NONE;
        sCoopPresenceRuntime.pending_read = (u8)((sCoopPresenceRuntime.pending_read + 1)
            % COOP_PRESENCE_RUNTIME_PENDING_CAPACITY);
        sCoopPresenceRuntime.pending_count--;
    }
}

void CoopPresenceRuntime_Update(void)
{
    const struct CoopPresenceRemote *remote;
    struct WorldLocation location;

    if (!sCoopPresenceRuntime.initialized || !IsNormalOverworld())
        return;
    ApplyPendingFrames();
    if (!IsOverworldPoseAllowed())
    {
        RemoveOwnedRenderer();
        return;
    }
    if (!IsWorldLocationCurrent(&location)
     || !CoopPresenceReducer_Synchronize(&sCoopPresenceRuntime.reducer,
                                         sCoopPresenceRuntime.session_epoch,
                                         &location,
                                         sCoopPresenceRuntime.warp_sequence)
     || !sCoopPresenceRuntime.transport_ready)
    {
        RemoveOwnedRenderer();
        return;
    }
    remote = CoopPresenceReducer_GetRemote(&sCoopPresenceRuntime.reducer);
    if (remote == NULL || !CoopPresenceReducer_IsVisible(&sCoopPresenceRuntime.reducer)
     || sCoopPresenceRuntime.frame_counter - sCoopPresenceRuntime.last_lifecycle_frame
        >= COOP_PRESENCE_RUNTIME_STALE_FRAMES)
    {
        /* A transient despawn freezes the placed sprite for a short grace
         * instead of popping it. The reducer already dropped the remote,
         * so TryInteract stays disabled for the whole hold, and only a
         * new spawn, warp, epoch, or transport change ends it. */
        if (!DespawnHoldActive())
            RemoveOwnedRenderer();
        return;
    }
    if (!EnsureRemoteRenderer(remote))
    {
        RemoveOwnedRenderer();
        return;
    }
    ConsumePendingBubble();
    {
        s16 map_x;
        s16 map_y;

        if (RemoteCoordinatesValid(remote, &map_x, &map_y))
            EnsureFollowerRenderer(remote, map_x, map_y);
    }
    MaybePublishCompanion();
}

bool8 CoopPresenceRuntime_QueueBridgeFrame(u16 type, const u8 *payload, u16 length)
{
    struct CoopPresencePendingFrame candidate = {0};

    if (!sCoopPresenceRuntime.initialized || payload == NULL
     || sCoopPresenceRuntime.pending_count >= COOP_PRESENCE_RUNTIME_PENDING_CAPACITY)
        return FALSE;
    switch (type)
    {
    case COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_SPAWN:
        if (!CoopPresence_DecodeSpawn(payload, length, &candidate.value.spawn))
            return FALSE;
        candidate.type = COOP_PRESENCE_PENDING_SPAWN;
        break;
    case COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_UPDATE:
        if (!CoopPresence_DecodeUpdate(payload, length, &candidate.value.update))
            return FALSE;
        candidate.type = COOP_PRESENCE_PENDING_UPDATE;
        break;
    case COOP_BRIDGE_MESSAGE_REMOTE_PLAYER_DESPAWN:
        if (!CoopPresence_DecodeDespawn(payload, length, &candidate.value.despawn))
            return FALSE;
        candidate.type = COOP_PRESENCE_PENDING_DESPAWN;
        break;
    case COOP_BRIDGE_MESSAGE_REMOTE_COMPANION:
        if (!CoopPresence_DecodeRemoteCompanion(payload, length, &candidate.value.companion))
            return FALSE;
        candidate.type = COOP_PRESENCE_PENDING_COMPANION;
        break;
    case COOP_BRIDGE_MESSAGE_REMOTE_SOCIAL_SIGNAL:
        if (!CoopPresence_DecodeRemoteSignal(payload, length, &candidate.value.signal))
            return FALSE;
        candidate.type = COOP_PRESENCE_PENDING_SIGNAL;
        break;
    default:
        return FALSE;
    }
    sCoopPresenceRuntime.pending[sCoopPresenceRuntime.pending_write] = candidate;
    sCoopPresenceRuntime.pending_write = (u8)((sCoopPresenceRuntime.pending_write + 1)
        % COOP_PRESENCE_RUNTIME_PENDING_CAPACITY);
    sCoopPresenceRuntime.pending_count++;
    return TRUE;
}

void CoopPresenceRuntime_Init(void)
{
    memset(&sCoopPresenceRuntime, 0, sizeof(sCoopPresenceRuntime));
    CoopPresenceReducer_Init(&sCoopPresenceRuntime.reducer);
    sCoopPresenceRuntime.warp_sequence = 1;
    sCoopPresenceRuntime.rendered_object_id = OBJECT_EVENTS_COUNT;
    sCoopPresenceRuntime.follower_object_id = OBJECT_EVENTS_COUNT;
    sCoopPresenceRuntime.follower_sprite_id = MAX_SPRITES;
    sCoopPresenceRuntime.next_emote = COOP_PRESENCE_EMOTE_EXCLAIM;
    sCoopPresenceRuntime.initialized = TRUE;
    /* Presence is not publishable until the transport accepts a nonzero
     * SESSION_READY epoch. */
    sCoopPresenceRuntime.transport_ready = FALSE;
}

bool8 CoopPresenceRuntime_IsOwnedRendererSprite(u8 objectEventId, u8 spriteId)
{
    if (!sCoopPresenceRuntime.initialized || !sCoopPresenceRuntime.renderer_owned
     || objectEventId != sCoopPresenceRuntime.rendered_object_id
     || spriteId != sCoopPresenceRuntime.rendered_sprite_id)
        return FALSE;
    return IsRendererSpriteProof(objectEventId, spriteId,
                                  sCoopPresenceRuntime.rendered_generation,
                                  GetRenderedGraphicsId(), TRUE);
}

void CoopPresenceRuntime_Reset(void)
{
    if (!sCoopPresenceRuntime.initialized)
        return;
    CoopPresenceReducer_Reset(&sCoopPresenceRuntime.reducer);
    sCoopPresenceRuntime.pending_read = 0;
    sCoopPresenceRuntime.pending_write = 0;
    sCoopPresenceRuntime.pending_count = 0;
    sCoopPresenceRuntime.rendered_handle = 0;
    sCoopPresenceRuntime.last_pose_valid = FALSE;
    sCoopPresenceRuntime.last_lifecycle_frame = sCoopPresenceRuntime.frame_counter;
    ClearSocialState();
    RemoveOwnedRenderer();
}

void CoopPresenceRuntime_SetSessionEpoch(u32 session_epoch)
{
    if (!sCoopPresenceRuntime.initialized)
        CoopPresenceRuntime_Init();
    if (sCoopPresenceRuntime.session_epoch != session_epoch)
    {
        CoopPresenceRuntime_Reset();
        sCoopPresenceRuntime.session_epoch = session_epoch;
    }
    sCoopPresenceRuntime.transport_ready = session_epoch != 0;
}

void CoopPresenceRuntime_TransportLost(void)
{
    if (sCoopPresenceRuntime.initialized)
    {
        CoopPresenceRuntime_Reset();
        sCoopPresenceRuntime.transport_ready = FALSE;
    }
}

void CoopPresenceRuntime_AdvanceFrame(void)
{
    if (sCoopPresenceRuntime.initialized)
        sCoopPresenceRuntime.frame_counter++;
}

void CoopPresenceRuntime_OnWarpCommit(void)
{
    if (!sCoopPresenceRuntime.initialized)
        return;
    sCoopPresenceRuntime.warp_sequence = CoopPresence_NextSequence(
        sCoopPresenceRuntime.warp_sequence);
    CoopPresenceRuntime_Reset();
}

enum CoopPresenceInteractionResult CoopPresenceRuntime_TryInteract(void)
{
    const struct CoopPresenceRemote *remote;
    struct CoopPresenceLocalState local;
    u8 payload[COOP_PRESENCE_INTERACTION_SIZE];

    if (!sCoopPresenceRuntime.initialized || !IsOverworldPoseAllowed()
     || !sCoopPresenceRuntime.transport_ready
     || !CoopPresenceRuntime_GetLocalState(&local)
     || !CoopPresenceReducer_IsVisible(&sCoopPresenceRuntime.reducer)
     || sCoopPresenceRuntime.frame_counter - sCoopPresenceRuntime.last_lifecycle_frame
        >= COOP_PRESENCE_RUNTIME_STALE_FRAMES)
        return COOP_PRESENCE_INTERACTION_NONE;

    remote = CoopPresenceReducer_GetRemote(&sCoopPresenceRuntime.reducer);
    if (!IsOwnedRendererEffectivelyVisible(remote)
     || !CoopPresence_EncodeInteraction(&sCoopPresenceRuntime.reducer,
                                        &(struct CoopPresenceLocalContext){
                                            .session_epoch = sCoopPresenceRuntime.session_epoch,
                                            .location = local.pose.location,
                                            .elevation = local.pose.elevation,
                                            .direction = local.pose.direction,
                                            .warp_sequence = local.pose.warp_sequence,
                                        }, payload, sizeof(payload)))
        return COOP_PRESENCE_INTERACTION_NONE;

    /* A queue-full send is still a consumed remote interaction.  The local
     * field path must not lock controls or fall through to a vanilla script. */
    (void)CoopNetBridge_EnqueueGameToNetwork(
        COOP_BRIDGE_MESSAGE_INTERACT_REMOTE_PLAYER, payload, sizeof(payload));
    return COOP_PRESENCE_INTERACTION_CONSUMED_NO_LOCK;
}

const struct CoopPresenceReducer *CoopPresenceRuntime_GetReducer(void)
{
    return &sCoopPresenceRuntime.reducer;
}

void CoopPresenceRuntime_RetireFollowerObjectOnReturnToField(u8 objectEventId)
{
    if (objectEventId >= OBJECT_EVENTS_COUNT
     || !gObjectEvents[objectEventId].active
     || !CoopPresenceRuntime_IsFollowerObject(&gObjectEvents[objectEventId]))
        return;

    /* ResumeMap has already reset the sprite table. Retire the reserved
     * follower object without destroying a slot rebound to another owner. */
    if (gObjectEvents[objectEventId].spriteId >= MAX_SPRITES
     || !IsFollowerSpriteProof(objectEventId, gObjectEvents[objectEventId].spriteId,
                               sCoopPresenceRuntime.follower_generation,
                               gObjectEvents[objectEventId].graphicsId, FALSE))
        gObjectEvents[objectEventId].active = FALSE;
    else
        RemoveObjectEvent(&gObjectEvents[objectEventId]);
    if (sCoopPresenceRuntime.follower_owned
     && sCoopPresenceRuntime.follower_object_id == objectEventId)
        ClearFollowerIdentity();
}
