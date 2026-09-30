#include "global.h"
#include "field_effect.h"
#include "field_player_avatar.h"
#include "fldeff.h"
#include "follower_npc.h"
#include "party_menu.h"
#include "overworld.h"
#include "task.h"
#include "constants/field_effects.h"
#include "coop/net_bridge.h"
#include "coop/group_travel.h"

static void FieldCallback_Teleport(void);
static void FieldCallback_GroupTeleport(void);
static void StartTeleportFieldEffect(void);

bool32 SetUpFieldMove_Teleport(void)
{
    if (CoopNetBridge_IsOrMayBeGrouped() && !CoopNetBridge_IsGrouped())
        return FALSE;
    if (CoopNetBridge_IsGrouped() && !CoopGroupTravel_CanTeleport())
        return FALSE;

    if (!CheckFollowerNPCFlag(FOLLOWER_NPC_FLAG_CAN_LEAVE_ROUTE))
        return FALSE;

    if (Overworld_MapTypeAllowsTeleportAndFly(gMapHeader.mapType) == TRUE)
    {
        if (CoopNetBridge_IsGrouped())
        {
            gFieldCallback2 = FieldCallback_PrepareFadeInForGroupTravel;
            gPostMenuFieldCallback = FieldCallback_GroupTeleport;
        }
        else
        {
            gFieldCallback2 = FieldCallback_PrepareFadeInForTeleport;
            gPostMenuFieldCallback = FieldCallback_Teleport;
        }
        return TRUE;
    }
    return FALSE;
}

static void FieldCallback_Teleport(void)
{
    Overworld_ResetStateAfterTeleport();
    FieldEffectStart(FLDEFF_USE_TELEPORT);
    gFieldEffectArguments[0] = (u32)GetCursorSelectionMonId();
}

static void FieldCallback_GroupTeleport(void)
{
    (void)CoopGroupTravel_BeginTeleport();
}

bool8 FldEff_UseTeleport(void)
{
    u8 taskId = CreateFieldMoveTask();
    gTasks[taskId].data[8] = (u32)StartTeleportFieldEffect >> 16;
    gTasks[taskId].data[9] = (u32)StartTeleportFieldEffect;
    SetPlayerAvatarTransitionFlags(PLAYER_AVATAR_FLAG_ON_FOOT);
    return FALSE;
}

static void StartTeleportFieldEffect(void)
{
    FieldEffectActiveListRemove(FLDEFF_USE_TELEPORT);
    FldEff_TeleportWarpOut();
}
