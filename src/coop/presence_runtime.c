#include "global.h"
#include "coop/net_bridge.h"
#include "coop/presence_runtime.h"
#include "event_object_movement.h"
#include "field_player_avatar.h"
#include "fieldmap.h"
#include "overworld.h"
#include "palette.h"
#include "script.h"
#include "sprite.h"
#include "constants/event_object_movement.h"
#include "constants/event_objects.h"

extern void MovementType_None(struct Sprite *sprite);

enum CoopPresencePendingType
{
    COOP_PRESENCE_PENDING_NONE = 0,
    COOP_PRESENCE_PENDING_SPAWN,
    COOP_PRESENCE_PENDING_UPDATE,
    COOP_PRESENCE_PENDING_DESPAWN,
};

struct CoopPresencePendingFrame
{
    enum CoopPresencePendingType type;
    union
    {
        struct CoopPresenceSpawn spawn;
        struct CoopPresenceUpdate update;
        struct CoopPresenceDespawn despawn;
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
    s32 sprite_start_x;
    s32 sprite_start_y;
    s32 sprite_target_x;
    s32 sprite_target_y;
    u8 interpolation_remaining;
    bool8 initialized;
    bool8 transport_ready;
    bool8 renderer_owned;
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
        && (gPlayerAvatar.flags & (PLAYER_AVATAR_FLAG_ON_FOOT | PLAYER_AVATAR_FLAG_CONTROLLABLE))
           == (PLAYER_AVATAR_FLAG_ON_FOOT | PLAYER_AVATAR_FLAG_CONTROLLABLE);
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
    pose->avatar_id = gPlayerAvatar.gender ? COOP_PRESENCE_AVATAR_MAY
                                           : COOP_PRESENCE_AVATAR_BRENDAN;
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
    return sCoopPresenceRuntime.rendered_avatar_id == COOP_PRESENCE_AVATAR_MAY
        ? OBJ_EVENT_GFX_MAY_NORMAL : OBJ_EVENT_GFX_BRENDAN_NORMAL;
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
    graphics_id = remote->state.pose.avatar_id == COOP_PRESENCE_AVATAR_MAY
        ? OBJ_EVENT_GFX_MAY_NORMAL : OBJ_EVENT_GFX_BRENDAN_NORMAL;
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
        s32 start_x = sprite->x;
        s32 start_y = sprite->y;
        s32 dx = (s32)map_x - object_event->currentCoords.x;
        s32 dy = (s32)map_y - object_event->currentCoords.y;
        bool8 snap = !coords_differ || dx > 2 || dx < -2 || dy > 2 || dy < -2
            || object_event->currentElevation != remote->state.pose.elevation
            || object_event->previousElevation != remote->state.pose.elevation;
        /* MoveObjectEventToMapCoords updates the complete engine position
         * state and ground-effect bookkeeping.  Capture the old rendered
         * base first, then restore it only for a six-frame interpolation. */
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
            sCoopPresenceRuntime.sprite_start_x = start_x;
            sCoopPresenceRuntime.sprite_start_y = start_y;
            sprite->x = (s16)start_x;
            sprite->y = (s16)start_y;
            sCoopPresenceRuntime.interpolation_remaining =
                COOP_PRESENCE_RUNTIME_INTERPOLATION_FRAMES;
        }
        else
        {
            sprite->x = (s16)sCoopPresenceRuntime.sprite_target_x;
            sprite->y = (s16)sCoopPresenceRuntime.sprite_target_y;
            sCoopPresenceRuntime.interpolation_remaining = 0;
        }
    }
    else if (sCoopPresenceRuntime.interpolation_remaining == 0)
    {
        /* Spawn and recreation establish the logical tile before this first
         * update.  Explicitly seed the base sprite position for that case. */
        sprite->x = (s16)sCoopPresenceRuntime.sprite_target_x;
        sprite->y = (s16)sCoopPresenceRuntime.sprite_target_y;
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
    {
        u8 elapsed = (u8)(COOP_PRESENCE_RUNTIME_INTERPOLATION_FRAMES
                          - sCoopPresenceRuntime.interpolation_remaining + 1);
        sprite->x = (s16)(sCoopPresenceRuntime.sprite_start_x
            + ((sCoopPresenceRuntime.sprite_target_x - sCoopPresenceRuntime.sprite_start_x)
               * elapsed) / COOP_PRESENCE_RUNTIME_INTERPOLATION_FRAMES);
        sprite->y = (s16)(sCoopPresenceRuntime.sprite_start_y
            + ((sCoopPresenceRuntime.sprite_target_y - sCoopPresenceRuntime.sprite_start_y)
               * elapsed) / COOP_PRESENCE_RUNTIME_INTERPOLATION_FRAMES);
        sCoopPresenceRuntime.interpolation_remaining--;
    }
    else
    {
        sprite->x = (s16)sCoopPresenceRuntime.sprite_target_x;
        sprite->y = (s16)sCoopPresenceRuntime.sprite_target_y;
    }
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
    graphics_id = remote->state.pose.avatar_id == COOP_PRESENCE_AVATAR_MAY
        ? OBJ_EVENT_GFX_MAY_NORMAL : OBJ_EVENT_GFX_BRENDAN_NORMAL;
    if (object_event->graphicsId != graphics_id
     || !RemoteCoordinatesValid(remote, &map_x, &map_y)
     || object_event->currentCoords.x != map_x
     || object_event->currentCoords.y != map_y
     || object_event->currentElevation != remote->state.pose.elevation
     || object_event->previousElevation != remote->state.pose.elevation)
        return FALSE;
    return TRUE;
}

static void ApplyPendingFrames(void)
{
    struct CoopPresenceLocalState local;
    struct CoopPresencePendingFrame *pending;
    enum CoopPresenceApplyResult result;
    struct WorldLocation location;

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
            break;
        case COOP_PRESENCE_PENDING_UPDATE:
            result = CoopPresenceReducer_ApplyUpdate(&sCoopPresenceRuntime.reducer,
                                                     &pending->value.update);
            break;
        case COOP_PRESENCE_PENDING_DESPAWN:
            result = CoopPresenceReducer_ApplyDespawn(&sCoopPresenceRuntime.reducer,
                                                      &pending->value.despawn);
            break;
        default:
            result = COOP_PRESENCE_APPLY_REJECTED;
            break;
        }
        if (result == COOP_PRESENCE_APPLY_APPLIED)
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
        RemoveOwnedRenderer();
        return;
    }
    if (!EnsureRemoteRenderer(remote))
        RemoveOwnedRenderer();
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
