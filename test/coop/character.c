#include "global.h"
#include "coop/character.h"
#include "event_data.h"
#include "event_object_movement.h"
#include "field_effect.h"
#include "field_player_avatar.h"
#include "sprite.h"
#include "constants/event_objects.h"
#include "constants/event_object_movement.h"
#include "constants/regions.h"
#include "constants/vars.h"
#include "test/test.h"

_Static_assert(COOP_CHARACTER_COUNT == 176, "all compatible field characters are selectable");

TEST("Cloud Coop Character validates persistent choices without changing identity")
{
    u16 saved = VarGet(VAR_COOP_CHARACTER);
    u8 gender = gSaveBlock2Ptr->playerGender;
    u8 region = gSaveBlock2Ptr->playerRegion;
    u32 i;
    for (i = 1; i <= COOP_CHARACTER_COUNT; i++)
    {
        EXPECT(CoopCharacter_SetSelection(i));
        EXPECT_EQ(CoopCharacter_GetSelection(), i);
        EXPECT_EQ(CoopCharacter_GetAvatarId(), i);
        EXPECT_EQ(gSaveBlock2Ptr->playerGender, gender);
        EXPECT_EQ(gSaveBlock2Ptr->playerRegion, region);
    }
    EXPECT(!CoopCharacter_SetSelection(COOP_CHARACTER_COUNT + 1));
    EXPECT_EQ(CoopCharacter_GetSelection(), COOP_CHARACTER_COUNT);
    EXPECT(CoopCharacter_SetSelection(0));
    EXPECT_EQ(CoopCharacter_GetSelection(), 0);
    VarSet(VAR_COOP_CHARACTER, saved);
}

TEST("Cloud Coop Character ignores untagged and invalid saved values")
{
    u16 saved = VarGet(VAR_COOP_CHARACTER);
    VarSet(VAR_COOP_CHARACTER, 1);
    EXPECT_EQ(CoopCharacter_GetSelection(), 0);
    VarSet(VAR_COOP_CHARACTER, 0xCAFF);
    EXPECT_EQ(CoopCharacter_GetSelection(), 0);
    EXPECT_EQ(CoopCharacter_OverrideNormalGraphics(OBJ_EVENT_GFX_RED_NORMAL), OBJ_EVENT_GFX_RED_NORMAL);
    VarSet(VAR_COOP_CHARACTER, saved);
}

TEST("Cloud Coop Character preserves specialized states and rival graphics")
{
    u16 saved = VarGet(VAR_COOP_CHARACTER);
    u16 bike = GetPlayerAvatarGraphicsIdByStateId(PLAYER_AVATAR_STATE_MACH_BIKE);
    u16 surf = GetPlayerAvatarGraphicsIdByStateId(PLAYER_AVATAR_STATE_SURFING);
    u16 fish = GetPlayerAvatarGraphicsIdByStateId(PLAYER_AVATAR_STATE_FISHING);
    u16 rival = GetRivalAvatarGraphicsIdByStateIdAndGender(PLAYER_AVATAR_STATE_NORMAL, MALE);
    CoopCharacter_SetSelection(5);
    EXPECT_EQ(GetPlayerAvatarGraphicsIdByStateId(PLAYER_AVATAR_STATE_NORMAL), OBJ_EVENT_GFX_WALLY);
    EXPECT_EQ(GetPlayerAvatarGraphicsIdByStateId(PLAYER_AVATAR_STATE_MACH_BIKE), bike);
    EXPECT_EQ(GetPlayerAvatarGraphicsIdByStateId(PLAYER_AVATAR_STATE_SURFING), surf);
    EXPECT_EQ(GetPlayerAvatarGraphicsIdByStateId(PLAYER_AVATAR_STATE_FISHING), fish);
    EXPECT_EQ(GetRivalAvatarGraphicsIdByStateIdAndGender(PLAYER_AVATAR_STATE_NORMAL, MALE), rival);
    VarSet(VAR_COOP_CHARACTER, saved);
}

TEST("Cloud Coop Character roster provides all player direction and run animations")
{
    u32 i, animation;
    for (i = 1; i <= COOP_CHARACTER_COUNT; i++)
    {
        const struct ObjectEventGraphicsInfo *info = GetObjectEventGraphicsInfo(CoopCharacter_GetGraphicsId(i));
        EXPECT_EQ(info->width, 16);
        EXPECT_EQ(info->height, 32);
        EXPECT(!info->inanimate);
        for (animation = ANIM_STD_FACE_SOUTH; animation <= ANIM_RUN_EAST; animation++)
            EXPECT(info->anims[animation] != NULL);
        for (u32 previous = 1; previous < i; previous++)
            EXPECT_NE(CoopCharacter_GetGraphicsId(i), CoopCharacter_GetGraphicsId(previous));
    }
}

TEST("Cloud Coop Character can preview the complete roster without exhausting graphics memory")
{
    u32 i;

    ResetSpriteData();
    FreeAllSpritePalettes();
    for (i = 1; i <= COOP_CHARACTER_COUNT; i++)
    {
        u8 spriteId = CreateObjectGraphicsSprite(
            CoopCharacter_GetGraphicsId(i),
            SpriteCallbackDummy,
            120,
            66,
            0);
        EXPECT_NE(spriteId, MAX_SPRITES);
        if (spriteId == MAX_SPRITES)
            break;
        StartSpriteAnim(&gSprites[spriteId], ANIM_STD_GO_SOUTH);
        FieldEffectFreeGraphicsResources(&gSprites[spriteId]);
    }
    EXPECT_EQ(i, COOP_CHARACTER_COUNT + 1);
}
