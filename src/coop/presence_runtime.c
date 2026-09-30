#include "global.h"
#include "data.h"
#include "bg.h"
#include "coop/character.h"
#include "coop/net_bridge.h"
#include "coop/online.h"
#include "coop/presence_runtime.h"
#include "event_object_movement.h"
#include "event_object_lock.h"
#include "field_effect.h"
#include "field_move.h"
#include "field_message_box.h"
#include "field_player_avatar.h"
#include "fieldmap.h"
#include "follower_helper.h"
#include "menu.h"
#include "overworld.h"
#include "palette.h"
#include "party_menu.h"
#include "pokemon.h"
#include "region_map.h"
#include "script.h"
#include "string_util.h"
#include "sprite.h"
#include "task.h"
#include "text.h"
#include "window.h"
#include "constants/characters.h"
#include "constants/event_object_movement.h"
#include "constants/event_objects.h"
#include "constants/field_effects.h"
#include "constants/moves.h"
#include "constants/region_map_sections.h"

extern void MovementType_None(struct Sprite *sprite);

static void RemoveFollowerRenderer(void);

enum CoopPresencePendingType
{
    COOP_PRESENCE_PENDING_NONE = 0,
    COOP_PRESENCE_PENDING_SPAWN,
    COOP_PRESENCE_PENDING_UPDATE,
    COOP_PRESENCE_PENDING_DESPAWN,
    COOP_PRESENCE_PENDING_INTERACTION,
    COOP_PRESENCE_PENDING_COMPANION,
    COOP_PRESENCE_PENDING_SIGNAL,
};

enum CoopPartnerNotice
{
    COOP_PARTNER_NOTICE_NONE,
    COOP_PARTNER_NOTICE_HIDDEN,
    COOP_PARTNER_NOTICE_DISCONNECTED,
    COOP_PARTNER_NOTICE_LEFT_MAP,
    COOP_PARTNER_NOTICE_WENT_TO_MAP,
    COOP_PARTNER_NOTICE_SESSION_ENDED,
    COOP_PARTNER_NOTICE_RECONNECTED,
};

static const u8 sPartnerHidden[] = _("Partner went out of sight.");
static const u8 sPartnerDisconnected[] = _("Partner disconnected.");
static const u8 sPartnerLeftMap[] = _("Partner left this area.");
static const u8 sPartnerWentToPrefix[] = _("Partner went to ");
static const u8 sPartnerSessionEnded[] = _("Partner's session ended.");
static const u8 sGroupEnded[] = _("Previous group ended.");
static const u8 sPartnerReconnected[] = _("Partner reconnected.");
static const u8 sPartnerBadgeEarned[] = _("Partner earned a Badge.");
static const u8 sPartnerBadgePrefix[] = _("Partner earned ");
static const u8 sPartnerCaughtPrefix[] = _("Partner caught ");
static const u8 sPartnerCaughtGeneric[] = _("Partner caught a new Pokemon.");
static const u8 sPartnerChampion[] = _("Partner became Champion!");
static const u8 sPartnerProgressPeriod[] = _(".");
static const u8 sStoneBadge[] = _("Stone Badge");
static const u8 sKnuckleBadge[] = _("Knuckle Badge");
static const u8 sDynamoBadge[] = _("Dynamo Badge");
static const u8 sHeatBadge[] = _("Heat Badge");
static const u8 sBalanceBadge[] = _("Balance Badge");
static const u8 sFeatherBadge[] = _("Feather Badge");
static const u8 sMindBadge[] = _("Mind Badge");
static const u8 sRainBadge[] = _("Rain Badge");
static const u8 sBoulderBadge[] = _("Boulder Badge");
static const u8 sCascadeBadge[] = _("Cascade Badge");
static const u8 sThunderBadge[] = _("Thunder Badge");
static const u8 sRainbowBadge[] = _("Rainbow Badge");
static const u8 sSoulBadge[] = _("Soul Badge");
static const u8 sMarshBadge[] = _("Marsh Badge");
static const u8 sVolcanoBadge[] = _("Volcano Badge");
static const u8 sEarthBadge[] = _("Earth Badge");
static const u8 sZephyrBadge[] = _("Zephyr Badge");
static const u8 sHiveBadge[] = _("Hive Badge");
static const u8 sPlainBadge[] = _("Plain Badge");
static const u8 sFogBadge[] = _("Fog Badge");
static const u8 sStormBadge[] = _("Storm Badge");
static const u8 sMineralBadge[] = _("Mineral Badge");
static const u8 sGlacierBadge[] = _("Glacier Badge");
static const u8 sRisingBadge[] = _("Rising Badge");
static const u8 *const sBadgeNames[3][8] = {
    {sStoneBadge, sKnuckleBadge, sDynamoBadge, sHeatBadge,
     sBalanceBadge, sFeatherBadge, sMindBadge, sRainBadge},
    {sBoulderBadge, sCascadeBadge, sThunderBadge, sRainbowBadge,
     sSoulBadge, sMarshBadge, sVolcanoBadge, sEarthBadge},
    {sZephyrBadge, sHiveBadge, sPlainBadge, sFogBadge,
     sStormBadge, sMineralBadge, sGlacierBadge, sRisingBadge},
};
static EWRAM_DATA u8 sPartnerProgressMessage[64];
static EWRAM_DATA u8 sPartnerMapMessage[64];

struct CoopPresencePendingFrame
{
    enum CoopPresencePendingType type;
    union
    {
        struct CoopPresenceSpawn spawn;
        struct CoopPresenceUpdate update;
        struct CoopPresenceDespawn despawn;
        struct CoopPresenceRemoteInteraction interaction;
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
    u8 pending_partner_notice;
    bool8 pending_group_ended_notice;
    bool8 last_group_ended_valid;
    u8 last_group_ended_id[16];
    u16 departed_map_group;
    u16 departed_map_number;
    u8 pending_progress_kind;
    u8 pending_progress_region;
    u16 pending_progress_subject_id;
    u32 despawn_hold_start;
    u32 companion_source_sequence;
    u16 last_companion_species;
    u8 last_companion_form;
    u8 last_companion_flags;
    u32 last_companion_frame;
    u32 signal_source_sequence;
    bool8 signal_used;
    u32 last_signal_frame;
    bool8 remote_companion_valid;
    u64 remote_companion_handle;
    u32 remote_companion_sequence;
    u16 remote_companion_species;
    u8 remote_companion_form;
    u8 remote_companion_flags;
    u32 remote_signal_sequence;
    bool8 remote_signal_seen;
    bool8 remote_interaction_seen;
    u32 remote_interaction_sequence;
    bool8 pending_partner_interaction;
    u8 interaction_menu_task;
    u8 interaction_menu_window;
    bool8 interaction_menu_controls_locked;
    u8 pending_bubble;
    bool8 pending_bubble_set;
    u32 pending_bubble_frame;
    s16 ping_x;
    s16 ping_y;
    u16 ping_map_group;
    u16 ping_map_num;
    u32 ping_frame;
    bool8 ping_valid;
    u32 ping_effect_frame;
    bool8 ping_effect_seen;
    u8 name_window;
    u32 name_label_until;
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
static bool8 IsOwnedRendererEffectivelyVisible(const struct CoopPresenceRemote *remote);
static void ClosePartnerInteractionMenu(void);

#define COOP_NAME_LABEL_FRAMES 90

static u8 UsernameCharToGameText(u8 ch)
{
    if (ch >= 'A' && ch <= 'Z')
        return CHAR_A + ch - 'A';
    if (ch >= 'a' && ch <= 'z')
        return CHAR_a + ch - 'a';
    if (ch >= '0' && ch <= '9')
        return CHAR_0 + ch - '0';
    if (ch == '-')
        return CHAR_HYPHEN;
    return CHAR_PERIOD;
}

static void HidePartnerName(void)
{
    if (sCoopPresenceRuntime.name_window == WINDOW_NONE)
        return;
    ClearWindowTilemap(sCoopPresenceRuntime.name_window);
    CopyWindowToVram(sCoopPresenceRuntime.name_window, COPYWIN_MAP);
    RemoveWindow(sCoopPresenceRuntime.name_window);
    sCoopPresenceRuntime.name_window = WINDOW_NONE;
}

void CoopPresenceRuntime_HidePartnerName(void)
{
    if (sCoopPresenceRuntime.initialized)
        HidePartnerName();
}

#if TESTING
u8 CoopPresenceRuntime_TestNameWindow(void)
{
    return sCoopPresenceRuntime.name_window;
}
#endif

static void UpdatePartnerName(const struct CoopPresenceRemote *remote)
{
    struct WindowTemplate window = {
        .bg = 0, .height = 2, .paletteNum = 15, .baseBlock = 8,
    };
    u8 text[COOP_PRESENCE_USERNAME_MAX * 2 + 1];
    s16 x;
    s16 y;
    u8 i;
    u8 length = 0;

    if (sCoopPresenceRuntime.interaction_menu_task != TASK_NONE
     || sCoopPresenceRuntime.interaction_menu_window != WINDOW_NONE
     || CoopOnline_IsOpen())
    {
        HidePartnerName();
        return;
    }
    if (sCoopPresenceRuntime.name_window != WINDOW_NONE)
    {
        if (sCoopPresenceRuntime.frame_counter < sCoopPresenceRuntime.name_label_until
         && remote != NULL && IsOwnedRendererEffectivelyVisible(remote)
         && IsFieldMessageBoxHidden())
        {
            u8 nameWindow = sCoopPresenceRuntime.name_window;
            u8 width = GetWindowAttribute(nameWindow, WINDOW_WIDTH);
            s16 left = (gSprites[sCoopPresenceRuntime.rendered_sprite_id].x / 8) - width / 2;
            s16 top = (gSprites[sCoopPresenceRuntime.rendered_sprite_id].y / 8) - 4;
            u8 newLeft = left < 1 ? 1 : left > 29 - width ? 29 - width : left;
            u8 newTop = top < 1 ? 1 : top > 17 ? 17 : top;

            if (GetWindowAttribute(nameWindow, WINDOW_TILEMAP_LEFT) != newLeft
             || GetWindowAttribute(nameWindow, WINDOW_TILEMAP_TOP) != newTop)
            {
                ClearWindowTilemap(nameWindow);
                SetWindowAttribute(nameWindow, WINDOW_TILEMAP_LEFT, newLeft);
                SetWindowAttribute(nameWindow, WINDOW_TILEMAP_TOP, newTop);
                PutWindowTilemap(nameWindow);
                CopyBgTilemapBufferToVram(0);
            }
            return;
        }
        HidePartnerName();
    }
    if (remote == NULL || !IsOwnedRendererEffectivelyVisible(remote)
     || !IsFieldMessageBoxHidden()
     || (gMain.newKeysRaw & L_BUTTON) == 0
     || (gMain.newKeysRaw & (R_BUTTON | SELECT_BUTTON | START_BUTTON | A_BUTTON | B_BUTTON
                           | DPAD_UP | DPAD_DOWN | DPAD_LEFT | DPAD_RIGHT)) != 0)
        return;

    for (i = 0; i < remote->username.length; i++)
    {
        if (remote->username.bytes[i] == '_')
        {
            text[length++] = CHAR_EXTRA_SYMBOL;
            text[length++] = CHAR_UNDERSCORE;
        }
        else
            text[length++] = UsernameCharToGameText((u8)remote->username.bytes[i]);
    }
    text[length] = EOS;
    window.width = (GetStringWidth(FONT_SMALL, text, 0) + 15) / 8;
    if (window.width < 3)
        window.width = 3;
    if (window.width > 28)
        window.width = 28;
    x = (gSprites[sCoopPresenceRuntime.rendered_sprite_id].x / 8) - window.width / 2;
    y = (gSprites[sCoopPresenceRuntime.rendered_sprite_id].y / 8) - 4;
    window.tilemapLeft = x < 1 ? 1 : x > 29 - window.width ? 29 - window.width : x;
    window.tilemapTop = y < 1 ? 1 : y > 17 ? 17 : y;
    sCoopPresenceRuntime.name_window = AddWindow(&window);
    if (sCoopPresenceRuntime.name_window == WINDOW_NONE)
        return;
    PutWindowTilemap(sCoopPresenceRuntime.name_window);
    FillWindowPixelBuffer(sCoopPresenceRuntime.name_window, PIXEL_FILL(1));
    AddTextPrinterParameterized(sCoopPresenceRuntime.name_window, FONT_SMALL,
                                text, 4, 0, TEXT_SKIP_DRAW, NULL);
    CopyWindowToVram(sCoopPresenceRuntime.name_window, COPYWIN_FULL);
    sCoopPresenceRuntime.name_label_until = sCoopPresenceRuntime.frame_counter + COOP_NAME_LABEL_FRAMES;
}

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

static bool8 RemoteMapIsLocalOrConnected(const struct WorldLocation *remote)
{
    struct WorldLocation local;
    const struct MapConnections *connections;
    s32 i;

    if (remote == NULL || !CoopWorldLocation_Export(&local)
     || remote->region != local.region)
        return FALSE;
    if (remote->map_group == local.map_group
     && remote->map_number == local.map_number)
        return TRUE;
    connections = gMapHeader.connections;
    if (connections == NULL)
        return FALSE;
    for (i = 0; i < connections->count; i++)
    {
        const struct MapConnection *connection = &connections->connections[i];
        if (connection->mapGroup == remote->map_group
         && connection->mapNum == remote->map_number
         && (connection->direction == CONNECTION_NORTH
          || connection->direction == CONNECTION_SOUTH
          || connection->direction == CONNECTION_WEST
          || connection->direction == CONNECTION_EAST))
            return TRUE;
    }
    return FALSE;
}

static bool8 RemoteCoordinatesValid(const struct CoopPresenceRemote *remote,
                                    s16 *map_x, s16 *map_y)
{
    const struct MapLayout *layout;
    const struct MapConnections *connections;
    struct WorldLocation local;
    s32 projected_x;
    s32 projected_y;
    s32 left;
    s32 right;
    s32 top;
    s32 bottom;
    s32 i;

    if (remote == NULL || map_x == NULL || map_y == NULL || gSaveBlock1Ptr == NULL
     || gMapHeader.mapLayout == NULL
     || !CoopWorldLocation_Export(&local)
     || remote->state.pose.location.region != local.region
     || remote->state.pose.elevation > ELEVATION_MULTI_LEVEL)
        return FALSE;
    layout = gMapHeader.mapLayout;
    projected_x = remote->state.pose.location.x;
    projected_y = remote->state.pose.location.y;
    if (remote->state.pose.location.map_group == local.map_group
     && remote->state.pose.location.map_number == local.map_number)
    {
        if (projected_x < 0 || projected_y < 0
         || projected_x >= layout->width || projected_y >= layout->height)
            return FALSE;
    }
    else
    {
        bool8 found = FALSE;
        connections = gMapHeader.connections;
        if (connections == NULL)
            return FALSE;
        for (i = 0; i < connections->count; i++)
        {
            const struct MapConnection *connection = &connections->connections[i];
            const struct MapHeader *neighbor;

            if (connection->mapGroup != remote->state.pose.location.map_group
             || connection->mapNum != remote->state.pose.location.map_number)
                continue;
            neighbor = GetMapHeaderFromConnection(connection);
            if (neighbor == NULL || neighbor->mapLayout == NULL
             || projected_x < 0 || projected_y < 0
             || projected_x >= neighbor->mapLayout->width
             || projected_y >= neighbor->mapLayout->height)
                return FALSE;
            switch (connection->direction)
            {
            case CONNECTION_EAST:
                projected_x += layout->width;
                projected_y += connection->offset;
                break;
            case CONNECTION_WEST:
                projected_x -= neighbor->mapLayout->width;
                projected_y += connection->offset;
                break;
            case CONNECTION_SOUTH:
                projected_x += connection->offset;
                projected_y += layout->height;
                break;
            case CONNECTION_NORTH:
                projected_x += connection->offset;
                projected_y -= neighbor->mapLayout->height;
                break;
            default:
                return FALSE;
            }
            found = TRUE;
            break;
        }
        if (!found)
            return FALSE;
    }
    projected_x += MAP_OFFSET;
    projected_y += MAP_OFFSET;
    if (projected_x < 0 || projected_y < 0
     || projected_x >= layout->width + MAP_OFFSET * 2
     || projected_y >= layout->height + MAP_OFFSET * 2)
        return FALSE;

    left = (s32)gSaveBlock1Ptr->pos.x - 2;
    right = (s32)gSaveBlock1Ptr->pos.x + MAP_OFFSET_W + 2;
    top = gSaveBlock1Ptr->pos.y;
    bottom = (s32)gSaveBlock1Ptr->pos.y + MAP_OFFSET_H + 2;
    if (projected_x < left || projected_x > right
     || projected_y < top || projected_y > bottom)
        return FALSE;
    if (MapGridGetElevationAt(projected_x, projected_y) == ELEVATION_INVALID)
        return FALSE;
    *map_x = projected_x;
    *map_y = projected_y;
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
    s32 horizontal = offset_x < 0 ? -offset_x : offset_x;
    s32 vertical = offset_y < 0 ? -offset_y : offset_y;
    s32 distance = max(horizontal, vertical);
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

/* The partner's form id selects the form species to draw.  GetFormSpeciesId
 * does not bound its table index, so an unknown form keeps the sent species. */
static u16 RemoteFollowerSpecies(u16 species, u8 form)
{
    const u16 *forms = GetSpeciesFormTable(species);
    u8 i;

    if (forms == NULL)
        return species;
    for (i = 0; i <= form; i++)
    {
        if (forms[i] == FORM_SPECIES_END)
            return species;
    }
    return forms[form];
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
    ClosePartnerInteractionMenu();
    sCoopPresenceRuntime.remote_companion_valid = FALSE;
    sCoopPresenceRuntime.remote_companion_handle = 0;
    sCoopPresenceRuntime.remote_companion_sequence = 0;
    sCoopPresenceRuntime.remote_companion_species = 0;
    sCoopPresenceRuntime.remote_companion_form = 0;
    sCoopPresenceRuntime.remote_companion_flags = 0;
    sCoopPresenceRuntime.remote_signal_sequence = 0;
    sCoopPresenceRuntime.remote_signal_seen = FALSE;
    sCoopPresenceRuntime.remote_interaction_seen = FALSE;
    sCoopPresenceRuntime.remote_interaction_sequence = 0;
    sCoopPresenceRuntime.pending_partner_interaction = FALSE;
    sCoopPresenceRuntime.pending_bubble_set = FALSE;
    sCoopPresenceRuntime.pending_bubble = 0;
    sCoopPresenceRuntime.ping_valid = FALSE;
    sCoopPresenceRuntime.ping_effect_seen = FALSE;
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
    u16 species;

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
    species = RemoteFollowerSpecies(sCoopPresenceRuntime.remote_companion_species,
                                    sCoopPresenceRuntime.remote_companion_form);
    graphics_id = FollowerGraphicsId(species, sCoopPresenceRuntime.remote_companion_flags);
    graphics = GetObjectEventGraphicsInfo(graphics_id);
    if (graphics == NULL)
        return FALSE;
    /* Species sprites use a dynamic palette that the spawn path loads from
     * the graphics id; it is not in the static object-event palette table. */
    if (graphics->paletteTag != TAG_NONE
     && graphics->paletteTag != OBJ_EVENT_PAL_TAG_DYNAMIC
     && LoadObjectEventPalette(graphics->paletteTag) == 0xFF)
        return FALSE;

    if (sCoopPresenceRuntime.follower_owned
     && (sCoopPresenceRuntime.follower_species != species
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
            sCoopPresenceRuntime.follower_species = species;
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
            sCoopPresenceRuntime.follower_species = species;
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

static bool8 ReadLeadCompanion(u16 *species, u8 *form, u8 *flags)
{
    u8 i;

    if (species == NULL || form == NULL || flags == NULL || gSaveBlock1Ptr == NULL)
        return FALSE;
    for (i = 0; i < PARTY_SIZE; i++)
    {
        u16 candidate = (u16)GetMonData(&gPlayerParty[i], MON_DATA_SPECIES);

        if (candidate == SPECIES_NONE || candidate == SPECIES_EGG
         || candidate >= NUM_SPECIES)
            continue;
        if (GetMonData(&gPlayerParty[i], MON_DATA_IS_EGG))
            continue;
        /* Unown letters live in the personality, not MON_DATA_SPECIES; the
         * local follower draws the letter form, so publish that form too. */
        if (candidate == SPECIES_UNOWN)
            candidate = (u16)GetUnownSpeciesId(GetMonData(&gPlayerParty[i], MON_DATA_PERSONALITY));
        *species = candidate;
        *form = GetFormIdFromFormSpeciesId(candidate);
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
    u8 form;
    u8 flags;

    if (!sCoopPresenceRuntime.initialized || !sCoopPresenceRuntime.transport_ready
     || sCoopPresenceRuntime.warp_sequence == 0 || !IsOverworldPoseAllowed())
        return;
    if (!ReadLeadCompanion(&species, &form, &flags))
        return;
    if (species == sCoopPresenceRuntime.last_companion_species
     && form == sCoopPresenceRuntime.last_companion_form
     && flags == sCoopPresenceRuntime.last_companion_flags
     && sCoopPresenceRuntime.last_companion_frame != 0
     && sCoopPresenceRuntime.frame_counter - sCoopPresenceRuntime.last_companion_frame
        < COOP_PRESENCE_RUNTIME_COMPANION_INTERVAL)
        return;
    sCoopPresenceRuntime.companion_source_sequence = CoopPresence_NextSequence(
        sCoopPresenceRuntime.companion_source_sequence);
    companion.species = species;
    companion.form = form;
    companion.flags = flags;
    companion.source_sequence = sCoopPresenceRuntime.companion_source_sequence;
    if (!CoopPresence_EncodeLocalCompanion(&companion, payload, sizeof(payload)))
        return;
    if (!CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_COMPANION_STATE,
                                            payload, sizeof(payload)))
        return;
    sCoopPresenceRuntime.last_companion_species = species;
    sCoopPresenceRuntime.last_companion_form = form;
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

bool8 CoopPresenceRuntime_TryEmote(u8 emote)
{
    const struct ObjectEvent *player;

    if (!CanUseSignal() || emote < COOP_PRESENCE_EMOTE_EXCLAIM
     || emote > COOP_PRESENCE_EMOTE_MAX)
        return FALSE;
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

static void UpdatePingMarker(void)
{
    s16 x;
    s16 y;

    if (!CoopPresenceRuntime_GetPingMarker(&x, &y)
     || gMapHeader.mapLayout == NULL
     || x < 0 || y < 0
     || x >= gMapHeader.mapLayout->width
     || y >= gMapHeader.mapLayout->height
     || (sCoopPresenceRuntime.frame_counter - sCoopPresenceRuntime.ping_frame) % 60 != 0
     || (sCoopPresenceRuntime.ping_effect_seen
      && sCoopPresenceRuntime.ping_effect_frame == sCoopPresenceRuntime.frame_counter))
        return;
    gFieldEffectArguments[0] = x;
    gFieldEffectArguments[1] = y;
    gFieldEffectArguments[2] = 2;
    FieldEffectStart(FLDEFF_SPARKLE);
    sCoopPresenceRuntime.ping_effect_frame = sCoopPresenceRuntime.frame_counter;
    sCoopPresenceRuntime.ping_effect_seen = TRUE;
}

static const struct WindowTemplate sPartnerInteractionWindow = {
    .bg = 0, .tilemapLeft = 9, .tilemapTop = 2,
    .width = 20, .height = 11, .paletteNum = 15, .baseBlock = 8,
};

static const u8 sPartnerInteractionTitle[] = _("Nearby player");
static const u8 sPartnerInteractionWave[] = _("Wave");
static const u8 sPartnerInteractionTrade[] = _("Trade");
static const u8 sPartnerInteractionBattle[] = _("Battle");
static const u8 sPartnerInteractionTravel[] = _("Travel together");
static const u8 sPartnerInteractionCheck[] = _("Check Pokemon");
static const u8 sPartnerInteractionUnavailable[] = _("That option is not available yet.");
static const u8 sPartnerInteractionTravelHint[] = _("Use FLY or a travel gate together.");
static const u8 sPartnerInteractionWaved[] = _("You waved at them.");
static const u8 sPartnerInteractionSent[] = _("Calling nearby player.");
static const u8 sPartnerInteractionLeadPrefix[] = _("Partner's lead: ");
static const u8 sPartnerInteractionNoLead[] = _("Partner's lead is unknown.");

enum
{
    PARTNER_INTERACTION_WAVE,
    PARTNER_INTERACTION_TRADE,
    PARTNER_INTERACTION_BATTLE,
    PARTNER_INTERACTION_TRAVEL,
    PARTNER_INTERACTION_CHECK,
    PARTNER_INTERACTION_COUNT,
};

static void DrawPartnerInteractionMenu(void)
{
    static const u8 *const options[] = {
        sPartnerInteractionWave,
        sPartnerInteractionTrade,
        sPartnerInteractionBattle,
        sPartnerInteractionTravel,
        sPartnerInteractionCheck,
    };
    u8 i;

    if (sCoopPresenceRuntime.interaction_menu_window == WINDOW_NONE)
        return;
    FillWindowPixelBuffer(sCoopPresenceRuntime.interaction_menu_window, PIXEL_FILL(1));
    AddTextPrinterParameterized(sCoopPresenceRuntime.interaction_menu_window, FONT_SMALL,
                                sPartnerInteractionTitle, 8, 1, TEXT_SKIP_DRAW, NULL);
    for (i = 0; i < ARRAY_COUNT(options); i++)
        AddTextPrinterParameterized(sCoopPresenceRuntime.interaction_menu_window, FONT_SMALL,
                                    options[i], 16, (u8)(18 + i * 14), TEXT_SKIP_DRAW, NULL);
    CopyWindowToVram(sCoopPresenceRuntime.interaction_menu_window, COPYWIN_GFX);
}

static bool8 CanChoosePartnerFlyDestination(void)
{
    u8 mon;
    u8 move;

    if (!CoopNetBridge_IsGrouped() || !IsFieldMoveUnlocked(FIELD_MOVE_FLY)
     || !SetUpFieldMove_Fly())
        return FALSE;
    for (mon = 0; mon < PARTY_SIZE; mon++)
        for (move = 0; move < MAX_MON_MOVES; move++)
            if (GetMonData(&gParties[B_TRAINER_0][mon], MON_DATA_MOVE1 + move) == MOVE_FLY)
                return TRUE;
    return FALSE;
}

static void RunPartnerInteraction(s8 selection)
{
    if (selection == PARTNER_INTERACTION_WAVE)
    {
        if (CoopPresenceRuntime_TryEmote(COOP_PRESENCE_EMOTE_HEART))
            (void)ShowFieldAutoScrollMessage(sPartnerInteractionWaved);
        else
            (void)ShowFieldAutoScrollMessage(sPartnerInteractionUnavailable);
    }
    else if (selection == PARTNER_INTERACTION_CHECK)
    {
        u16 species = sCoopPresenceRuntime.remote_companion_species;

        if (sCoopPresenceRuntime.remote_companion_valid
         && species != SPECIES_NONE && species < NUM_SPECIES)
        {
            StringCopy(sPartnerProgressMessage, sPartnerInteractionLeadPrefix);
            StringAppend(sPartnerProgressMessage, GetSpeciesName(species));
            StringAppend(sPartnerProgressMessage, sPartnerProgressPeriod);
            (void)ShowFieldAutoScrollMessage(sPartnerProgressMessage);
        }
        else
            (void)ShowFieldAutoScrollMessage(sPartnerInteractionNoLead);
    }
    else if (selection == PARTNER_INTERACTION_TRAVEL)
    {
        if (CanChoosePartnerFlyDestination())
            CoopRegionMap_OpenPartnerFlyMap();
        else
            (void)ShowFieldAutoScrollMessage(sPartnerInteractionTravelHint);
    }
    else
    {
        /* Trade needs the gameplay ledger, and a partner-versus-partner
         * battle needs an opponent-side lockstep engine. Keep the menu
         * honest until those are ready. */
        (void)ShowFieldAutoScrollMessage(sPartnerInteractionUnavailable);
    }
}

static void Task_PartnerInteractionMenu(u8 taskId)
{
    s8 selection = Menu_ProcessInputNoWrap();

    if (selection == MENU_NOTHING_CHOSEN)
        return;
    ClosePartnerInteractionMenu();
    if (selection == MENU_B_PRESSED || selection < 0
     || selection >= PARTNER_INTERACTION_COUNT)
        return;
    RunPartnerInteraction(selection);
}

#if TESTING
u8 CoopPresenceRuntime_TestInteractionMenuTask(void)
{
    return sCoopPresenceRuntime.interaction_menu_task;
}

/* Choose a menu entry exactly as the menu task does after input. */
bool8 CoopPresenceRuntime_TestChooseInteraction(s8 selection)
{
    if (sCoopPresenceRuntime.interaction_menu_task == TASK_NONE)
        return FALSE;
    ClosePartnerInteractionMenu();
    if (selection == MENU_B_PRESSED || selection < 0
     || selection >= PARTNER_INTERACTION_COUNT)
        return TRUE;
    RunPartnerInteraction(selection);
    return TRUE;
}
#endif

static void OpenPartnerInteractionMenu(void)
{
    u8 task_id;

    if (sCoopPresenceRuntime.interaction_menu_task != TASK_NONE
     || sCoopPresenceRuntime.interaction_menu_window != WINDOW_NONE
     || !IsFieldMessageBoxHidden() || ArePlayerFieldControlsLocked()
     || gPaletteFade.active || GetTaskCount() == NUM_TASKS)
        return;
    task_id = CreateTask(Task_PartnerInteractionMenu, 0x50);
    if (task_id == TASK_NONE)
        return;
    sCoopPresenceRuntime.interaction_menu_window = AddWindow(&sPartnerInteractionWindow);
    if (sCoopPresenceRuntime.interaction_menu_window == WINDOW_NONE)
    {
        DestroyTask(task_id);
        return;
    }
    sCoopPresenceRuntime.interaction_menu_task = task_id;
    HidePartnerName();
    FreezeObjectEvents();
    PlayerFreeze();
    StopPlayerAvatar();
    LockPlayerFieldControls();
    sCoopPresenceRuntime.interaction_menu_controls_locked = TRUE;
    DrawStdWindowFrame(sCoopPresenceRuntime.interaction_menu_window, FALSE);
    DrawPartnerInteractionMenu();
    InitMenuInUpperLeftCornerNormal(sCoopPresenceRuntime.interaction_menu_window,
                                    PARTNER_INTERACTION_COUNT, 0);
}

static void ClosePartnerInteractionMenu(void)
{
    if (sCoopPresenceRuntime.interaction_menu_window != WINDOW_NONE)
    {
        ClearStdWindowAndFrame(sCoopPresenceRuntime.interaction_menu_window, TRUE);
        RemoveWindow(sCoopPresenceRuntime.interaction_menu_window);
        sCoopPresenceRuntime.interaction_menu_window = WINDOW_NONE;
    }
    if (sCoopPresenceRuntime.interaction_menu_task != TASK_NONE)
    {
        DestroyTask(sCoopPresenceRuntime.interaction_menu_task);
        sCoopPresenceRuntime.interaction_menu_task = TASK_NONE;
    }
    if (sCoopPresenceRuntime.interaction_menu_controls_locked)
    {
        if (ArePlayerFieldControlsLocked())
        {
            ScriptUnfreezeObjectEvents();
            UnlockPlayerFieldControls();
        }
        sCoopPresenceRuntime.interaction_menu_controls_locked = FALSE;
    }
}

static void MaybeOpenPartnerInteractionMenu(void)
{
    if (!sCoopPresenceRuntime.pending_partner_interaction)
        return;
    if (!IsFieldMessageBoxHidden() || ArePlayerFieldControlsLocked()
     || gPaletteFade.active || ScriptContext_IsEnabled())
        return;
    sCoopPresenceRuntime.pending_partner_interaction = FALSE;
    OpenPartnerInteractionMenu();
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
    sCoopPresenceRuntime.remote_companion_form = companion->form;
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
        sCoopPresenceRuntime.ping_map_group = active->state.pose.location.map_group;
        sCoopPresenceRuntime.ping_map_num = active->state.pose.location.map_number;
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

static enum CoopPresenceApplyResult ApplyRemoteInteraction(
    const struct CoopPresenceRemoteInteraction *interaction)
{
    const struct CoopPresenceRemote *active;

    if (interaction == NULL || gSaveBlock1Ptr == NULL)
        return COOP_PRESENCE_APPLY_REJECTED;
    active = CoopPresenceReducer_GetRemote(&sCoopPresenceRuntime.reducer);
    if (active == NULL || active->handle != interaction->handle)
        return COOP_PRESENCE_APPLY_STALE;
    if (sCoopPresenceRuntime.remote_interaction_seen
     && !CoopPresence_SequenceIsNewer(interaction->server_sequence,
                                      sCoopPresenceRuntime.remote_interaction_sequence))
        return COOP_PRESENCE_APPLY_STALE;
    sCoopPresenceRuntime.remote_interaction_seen = TRUE;
    sCoopPresenceRuntime.remote_interaction_sequence = interaction->server_sequence;
    sCoopPresenceRuntime.pending_partner_interaction = TRUE;
    return COOP_PRESENCE_APPLY_APPLIED;
}

static void QueueDestinationNotice(const struct WorldLocation *destination)
{
    const struct MapHeader *mapHeader;
    u8 mapName[32];

    if (sCoopPresenceRuntime.pending_partner_notice != COOP_PARTNER_NOTICE_LEFT_MAP
     && sCoopPresenceRuntime.pending_partner_notice != COOP_PARTNER_NOTICE_WENT_TO_MAP)
        return;
    if (destination->map_group == sCoopPresenceRuntime.departed_map_group
     && destination->map_number == sCoopPresenceRuntime.departed_map_number)
    {
        /* The partner came back before the queued departure was displayed. */
        sCoopPresenceRuntime.pending_partner_notice = COOP_PARTNER_NOTICE_NONE;
        return;
    }
    mapHeader = Overworld_GetMapHeaderByGroupAndId(destination->map_group,
                                                   destination->map_number);
    if (mapHeader == NULL || mapHeader->regionMapSectionId == MAPSEC_NONE)
    {
        sCoopPresenceRuntime.pending_partner_notice = COOP_PARTNER_NOTICE_LEFT_MAP;
        return;
    }
    GetMapNameGeneric(mapName, mapHeader->regionMapSectionId);
    StringCopy(sPartnerMapMessage, sPartnerWentToPrefix);
    StringAppend(sPartnerMapMessage, mapName);
    StringAppend(sPartnerMapMessage, sPartnerProgressPeriod);
    sCoopPresenceRuntime.pending_partner_notice = COOP_PARTNER_NOTICE_WENT_TO_MAP;
}

static void ApplyPendingFrames(void)
{
    struct CoopPresenceLocalState local;
    struct CoopPresencePendingFrame *pending;
    enum CoopPresenceApplyResult result;
    struct WorldLocation location;
    bool8 renderer_owned;
    const struct CoopPresenceRemote *departing;
    u16 departedMapGroup;
    u16 departedMapNumber;

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
            result = RemoteMapIsLocalOrConnected(&pending->value.spawn.state.pose.location)
                ? CoopPresenceReducer_ApplySpawnConnected(&sCoopPresenceRuntime.reducer,
                                                           &pending->value.spawn)
                : COOP_PRESENCE_APPLY_PARTITION_MISMATCH;
            if (result == COOP_PRESENCE_APPLY_CAPACITY
             && (pending->value.spawn.state.pose.location.map_group != location.map_group
              || pending->value.spawn.state.pose.location.map_number != location.map_number))
            {
                /* The server sends cross-map spawns only to an active group
                 * partner. Give that partner the ROM's sole remote slot. */
                CoopPresenceReducer_Reset(&sCoopPresenceRuntime.reducer);
                (void)CoopPresenceReducer_Synchronize(&sCoopPresenceRuntime.reducer,
                                                      sCoopPresenceRuntime.session_epoch,
                                                      &location,
                                                      sCoopPresenceRuntime.warp_sequence);
                result = CoopPresenceReducer_ApplySpawnConnected(
                    &sCoopPresenceRuntime.reducer, &pending->value.spawn);
            }
            if (result == COOP_PRESENCE_APPLY_APPLIED)
            {
                if (sCoopPresenceRuntime.despawn_hold_reason == COOP_PRESENCE_DESPAWN_DISCONNECTED)
                {
                    if (sCoopPresenceRuntime.pending_partner_notice == COOP_PARTNER_NOTICE_DISCONNECTED)
                        sCoopPresenceRuntime.pending_partner_notice = COOP_PARTNER_NOTICE_NONE;
                    else
                        sCoopPresenceRuntime.pending_partner_notice = COOP_PARTNER_NOTICE_RECONNECTED;
                }
                else
                    QueueDestinationNotice(&pending->value.spawn.state.pose.location);
                sCoopPresenceRuntime.despawn_hold_reason = 0;
            }
            break;
        case COOP_PRESENCE_PENDING_UPDATE:
            departing = CoopPresenceReducer_GetRemote(&sCoopPresenceRuntime.reducer);
            departedMapGroup = departing != NULL ? departing->state.pose.location.map_group : 0;
            departedMapNumber = departing != NULL ? departing->state.pose.location.map_number : 0;
            result = RemoteMapIsLocalOrConnected(&pending->value.update.state.pose.location)
                ? CoopPresenceReducer_ApplyUpdateConnected(&sCoopPresenceRuntime.reducer,
                                                            &pending->value.update)
                : COOP_PRESENCE_APPLY_PARTITION_MISMATCH;
            if (result == COOP_PRESENCE_APPLY_APPLIED)
            {
                if (departing != NULL
                 && sCoopPresenceRuntime.pending_partner_notice == COOP_PARTNER_NOTICE_NONE
                 && (departedMapGroup != pending->value.update.state.pose.location.map_group
                  || departedMapNumber != pending->value.update.state.pose.location.map_number))
                {
                    sCoopPresenceRuntime.pending_partner_notice = COOP_PARTNER_NOTICE_LEFT_MAP;
                    sCoopPresenceRuntime.departed_map_group = departedMapGroup;
                    sCoopPresenceRuntime.departed_map_number = departedMapNumber;
                }
                QueueDestinationNotice(&pending->value.update.state.pose.location);
            }
            break;
        case COOP_PRESENCE_PENDING_DESPAWN:
            renderer_owned = sCoopPresenceRuntime.renderer_owned;
            departing = CoopPresenceReducer_GetRemote(&sCoopPresenceRuntime.reducer);
            departedMapGroup = departing != NULL ? departing->state.pose.location.map_group : 0;
            departedMapNumber = departing != NULL ? departing->state.pose.location.map_number : 0;
            result = CoopPresenceReducer_ApplyDespawn(&sCoopPresenceRuntime.reducer,
                                                      &pending->value.despawn);
            if (result == COOP_PRESENCE_APPLY_APPLIED)
            {
                sCoopPresenceRuntime.pending_partner_interaction = FALSE;
                switch (pending->value.despawn.reason)
                {
                case COOP_PRESENCE_DESPAWN_HIDDEN:
                    sCoopPresenceRuntime.pending_partner_notice = COOP_PARTNER_NOTICE_HIDDEN;
                    break;
                case COOP_PRESENCE_DESPAWN_DISCONNECTED:
                    sCoopPresenceRuntime.pending_partner_notice = COOP_PARTNER_NOTICE_DISCONNECTED;
                    break;
                case COOP_PRESENCE_DESPAWN_PARTITION_LEFT:
                    if (renderer_owned
                     || (departedMapGroup == location.map_group
                      && departedMapNumber == location.map_number))
                    {
                        sCoopPresenceRuntime.pending_partner_notice = COOP_PARTNER_NOTICE_LEFT_MAP;
                        sCoopPresenceRuntime.departed_map_group = departedMapGroup;
                        sCoopPresenceRuntime.departed_map_number = departedMapNumber;
                    }
                    break;
                case COOP_PRESENCE_DESPAWN_LEASE_INVALID:
                    sCoopPresenceRuntime.pending_partner_notice = COOP_PARTNER_NOTICE_SESSION_ENDED;
                    break;
                }
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
        case COOP_PRESENCE_PENDING_INTERACTION:
            result = ApplyRemoteInteraction(&pending->value.interaction);
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
         && pending->type != COOP_PRESENCE_PENDING_INTERACTION
         && pending->type != COOP_PRESENCE_PENDING_COMPANION
         && pending->type != COOP_PRESENCE_PENDING_SIGNAL)
            sCoopPresenceRuntime.last_lifecycle_frame = sCoopPresenceRuntime.frame_counter;
        pending->type = COOP_PRESENCE_PENDING_NONE;
        sCoopPresenceRuntime.pending_read = (u8)((sCoopPresenceRuntime.pending_read + 1)
            % COOP_PRESENCE_RUNTIME_PENDING_CAPACITY);
        sCoopPresenceRuntime.pending_count--;
    }
}

static void ShowPendingPartnerNotice(void)
{
    const u8 *message = NULL;
    u8 kind;
    u8 region;
    u16 subjectId;
    enum Species species;

    if (!IsFieldMessageBoxHidden() || ArePlayerFieldControlsLocked()
     || ScriptContext_IsEnabled() || gPaletteFade.active || CoopOnline_IsOpen())
        return;
    if (sCoopPresenceRuntime.pending_group_ended_notice)
    {
        if (ShowFieldAutoScrollMessage(sGroupEnded))
            sCoopPresenceRuntime.pending_group_ended_notice = FALSE;
        return;
    }
    switch (sCoopPresenceRuntime.pending_partner_notice)
    {
    case COOP_PARTNER_NOTICE_HIDDEN: message = sPartnerHidden; break;
    case COOP_PARTNER_NOTICE_DISCONNECTED: message = sPartnerDisconnected; break;
    case COOP_PARTNER_NOTICE_LEFT_MAP: message = sPartnerLeftMap; break;
    case COOP_PARTNER_NOTICE_WENT_TO_MAP: message = sPartnerMapMessage; break;
    case COOP_PARTNER_NOTICE_SESSION_ENDED: message = sPartnerSessionEnded; break;
    case COOP_PARTNER_NOTICE_RECONNECTED: message = sPartnerReconnected; break;
    }
    if (message != NULL && ShowFieldAutoScrollMessage(message))
    {
        sCoopPresenceRuntime.pending_partner_notice = COOP_PARTNER_NOTICE_NONE;
        return;
    }
    if (message != NULL)
        return;

    if (sCoopPresenceRuntime.pending_progress_kind == 0
     && CoopNetBridge_TakeProgressNotice(&kind, &region, &subjectId))
    {
        sCoopPresenceRuntime.pending_progress_kind = kind;
        sCoopPresenceRuntime.pending_progress_region = region;
        sCoopPresenceRuntime.pending_progress_subject_id = subjectId;
    }
    if (sCoopPresenceRuntime.pending_progress_kind == 1)
    {
        region = sCoopPresenceRuntime.pending_progress_region;
        subjectId = sCoopPresenceRuntime.pending_progress_subject_id;
        if (region >= COOP_REGION_HOENN && region <= COOP_REGION_JOHTO && subjectId < 8)
        {
            StringCopy(sPartnerProgressMessage, sPartnerBadgePrefix);
            StringAppend(sPartnerProgressMessage,
                         sBadgeNames[region - COOP_REGION_HOENN][subjectId]);
            StringAppend(sPartnerProgressMessage, sPartnerProgressPeriod);
            message = sPartnerProgressMessage;
        }
        else
            message = sPartnerBadgeEarned;
    }
    else if (sCoopPresenceRuntime.pending_progress_kind == 2)
    {
        species = NationalPokedexNumToSpecies(sCoopPresenceRuntime.pending_progress_subject_id);
        if (species == SPECIES_NONE)
            message = sPartnerCaughtGeneric;
        else
        {
            StringCopy(sPartnerProgressMessage, sPartnerCaughtPrefix);
            StringAppend(sPartnerProgressMessage, GetSpeciesName(species));
            StringAppend(sPartnerProgressMessage, sPartnerProgressPeriod);
            message = sPartnerProgressMessage;
        }
    }
    else if (sCoopPresenceRuntime.pending_progress_kind == 3)
        message = sPartnerChampion;
    if (message != NULL && ShowFieldAutoScrollMessage(message))
        sCoopPresenceRuntime.pending_progress_kind = 0;
}

void CoopPresenceRuntime_Update(void)
{
    const struct CoopPresenceRemote *remote;
    struct WorldLocation location;

    if (!sCoopPresenceRuntime.initialized)
        return;
    if (!IsNormalOverworld())
    {
        HidePartnerName();
        return;
    }
    ApplyPendingFrames();
    ShowPendingPartnerNotice();
    if (!IsOverworldPoseAllowed())
    {
        HidePartnerName();
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
        HidePartnerName();
        RemoveOwnedRenderer();
        return;
    }
    UpdatePingMarker();
    remote = CoopPresenceReducer_GetRemote(&sCoopPresenceRuntime.reducer);
    if (remote == NULL || !CoopPresenceReducer_IsVisible(&sCoopPresenceRuntime.reducer)
     || sCoopPresenceRuntime.frame_counter - sCoopPresenceRuntime.last_lifecycle_frame
        >= COOP_PRESENCE_RUNTIME_STALE_FRAMES)
    {
        HidePartnerName();
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
        HidePartnerName();
        RemoveOwnedRenderer();
        return;
    }
    MaybeOpenPartnerInteractionMenu();
    UpdatePartnerName(remote);
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
    case COOP_BRIDGE_MESSAGE_REMOTE_INTERACTION:
        if (!CoopPresence_DecodeRemoteInteraction(payload, length, &candidate.value.interaction))
            return FALSE;
        candidate.type = COOP_PRESENCE_PENDING_INTERACTION;
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
    sCoopPresenceRuntime.name_window = WINDOW_NONE;
    sCoopPresenceRuntime.interaction_menu_task = TASK_NONE;
    sCoopPresenceRuntime.interaction_menu_window = WINDOW_NONE;
    sCoopPresenceRuntime.interaction_menu_controls_locked = FALSE;
    CoopPresenceReducer_Init(&sCoopPresenceRuntime.reducer);
    sCoopPresenceRuntime.warp_sequence = 1;
    sCoopPresenceRuntime.rendered_object_id = OBJECT_EVENTS_COUNT;
    sCoopPresenceRuntime.follower_object_id = OBJECT_EVENTS_COUNT;
    sCoopPresenceRuntime.follower_sprite_id = MAX_SPRITES;
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
    // An unread notice can be replayed by the same lease after transport
    // recovery. A notice already shown keeps its UUID tombstone.
    if (sCoopPresenceRuntime.pending_group_ended_notice)
        sCoopPresenceRuntime.last_group_ended_valid = FALSE;
    sCoopPresenceRuntime.pending_group_ended_notice = FALSE;
    HidePartnerName();
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
        sCoopPresenceRuntime.last_group_ended_valid = FALSE;
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

bool8 CoopPresenceRuntime_QueueGroupEnded(const u8 *groupId, u16 length)
{
    if (!sCoopPresenceRuntime.initialized || groupId == NULL || length != 16)
        return FALSE;
    if (sCoopPresenceRuntime.last_group_ended_valid
     && memcmp(sCoopPresenceRuntime.last_group_ended_id, groupId, 16) == 0)
        return TRUE;
    memcpy(sCoopPresenceRuntime.last_group_ended_id, groupId, 16);
    sCoopPresenceRuntime.last_group_ended_valid = TRUE;
    sCoopPresenceRuntime.pending_partner_notice = COOP_PARTNER_NOTICE_NONE;
    sCoopPresenceRuntime.pending_group_ended_notice = TRUE;
    return TRUE;
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
    if (CoopNetBridge_EnqueueGameToNetwork(
            COOP_BRIDGE_MESSAGE_INTERACT_REMOTE_PLAYER, payload, sizeof(payload)))
        (void)ShowFieldAutoScrollMessage(sPartnerInteractionSent);
    return COOP_PRESENCE_INTERACTION_CONSUMED_NO_LOCK;
}

/* Reads only the reducer, never the renderer: a trainer script locks field
 * controls and retires the partner sprite, but the partner is still there.
 * The same freshness and projection rules as TryInteract apply, so a stale,
 * hidden, unconnected or off-view partner is never "nearby". */
bool8 CoopPresenceRuntime_IsPartnerNearby(u8 maxTiles)
{
    const struct CoopPresenceRemote *remote;
    s16 map_x;
    s16 map_y;
    s32 dx;
    s32 dy;

    if (!sCoopPresenceRuntime.initialized || !sCoopPresenceRuntime.transport_ready
     || gSaveBlock1Ptr == NULL
     || !CoopPresenceReducer_IsVisible(&sCoopPresenceRuntime.reducer)
     || sCoopPresenceRuntime.frame_counter - sCoopPresenceRuntime.last_lifecycle_frame
        >= COOP_PRESENCE_RUNTIME_STALE_FRAMES)
        return FALSE;
    remote = CoopPresenceReducer_GetRemote(&sCoopPresenceRuntime.reducer);
    if (remote == NULL
     || !RemoteMapIsLocalOrConnected(&remote->state.pose.location)
     || !RemoteCoordinatesValid(remote, &map_x, &map_y))
        return FALSE;
    dx = (s32)map_x - ((s32)gSaveBlock1Ptr->pos.x + MAP_OFFSET);
    dy = (s32)map_y - ((s32)gSaveBlock1Ptr->pos.y + MAP_OFFSET);
    if (dx < 0)
        dx = -dx;
    if (dy < 0)
        dy = -dy;
    return dx <= maxTiles && dy <= maxTiles;
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
