#include "global.h"
#include "coop/character.h"
#include "coop/presence.h"
#include "event_data.h"
#include "event_object_lock.h"
#include "event_object_movement.h"
#include "field_effect.h"
#include "field_player_avatar.h"
#include "main.h"
#include "menu.h"
#include "script.h"
#include "sound.h"
#include "sprite.h"
#include "task.h"
#include "text.h"
#include "window.h"
#include "constants/event_objects.h"
#include "constants/event_object_movement.h"
#include "constants/regions.h"
#include "constants/songs.h"
#include "constants/vars.h"

// A tagged value in an unused persistent var preserves the existing save layout.
#define CHARACTER_SAVE_TAG 0xCA00

static const struct {
    u16 graphicsId;
    const u8 *name;
} sCharacters[] = {
    {OBJ_EVENT_GFX_BRENDAN_NORMAL, COMPOUND_STRING("Brendan")},
    {OBJ_EVENT_GFX_MAY_NORMAL, COMPOUND_STRING("May")},
    {OBJ_EVENT_GFX_RED_NORMAL, COMPOUND_STRING("Red")},
    {OBJ_EVENT_GFX_GREEN_NORMAL, COMPOUND_STRING("Leaf")},
    {OBJ_EVENT_GFX_WALLY, COMPOUND_STRING("Wally")},
    {OBJ_EVENT_GFX_STEVEN, COMPOUND_STRING("Steven")},
    {OBJ_EVENT_GFX_NORMAN, COMPOUND_STRING("Norman")},
    {OBJ_EVENT_GFX_YOUNGSTER, COMPOUND_STRING("Youngster")},
    {OBJ_EVENT_GFX_LASS, COMPOUND_STRING("Lass")},
    {OBJ_EVENT_GFX_PROF_BIRCH, COMPOUND_STRING("Prof. Birch")},
    {OBJ_EVENT_GFX_HIKER, COMPOUND_STRING("Hiker")},
    {OBJ_EVENT_GFX_SAILOR, COMPOUND_STRING("Sailor")},
};
_Static_assert(ARRAY_COUNT(sCharacters) == COOP_CHARACTER_COUNT, "character roster size");

static EWRAM_DATA u8 sWindow;
static EWRAM_DATA u8 sPreview;
static EWRAM_DATA u8 sChoice;
static const struct WindowTemplate sWindowTemplate = {
    .bg = 0, .tilemapLeft = 2, .tilemapTop = 1,
    .width = 26, .height = 18, .paletteNum = 15, .baseBlock = 8,
};

u8 CoopCharacter_GetSelection(void)
{
    u16 saved = VarGet(VAR_COOP_CHARACTER);
    u8 choice = saved & 0xFF;
    return (saved & 0xFF00) == CHARACTER_SAVE_TAG && choice <= COOP_CHARACTER_COUNT ? choice : 0;
}

bool8 CoopCharacter_SetSelection(u8 selection)
{
    if (selection > COOP_CHARACTER_COUNT) return FALSE;
    return VarSet(VAR_COOP_CHARACTER, selection == 0 ? 0 : CHARACTER_SAVE_TAG | selection);
}

static u8 DefaultAvatar(void)
{
    if (gSaveBlock2Ptr->playerRegion == REGION_KANTO)
        return gSaveBlock2Ptr->playerGender == FEMALE ? COOP_PRESENCE_AVATAR_LEAF : COOP_PRESENCE_AVATAR_RED;
    return gSaveBlock2Ptr->playerGender == FEMALE ? COOP_PRESENCE_AVATAR_MAY : COOP_PRESENCE_AVATAR_BRENDAN;
}

u8 CoopCharacter_GetAvatarId(void)
{
    u8 choice = CoopCharacter_GetSelection();
    return choice == 0 ? DefaultAvatar() : choice;
}

u16 CoopCharacter_GetGraphicsId(u8 avatarId)
{
    if (avatarId == 0 || avatarId > COOP_CHARACTER_COUNT)
        return OBJ_EVENT_GFX_BRENDAN_NORMAL;
    return sCharacters[avatarId - 1].graphicsId;
}

u16 CoopCharacter_OverrideNormalGraphics(u16 original)
{
    u8 choice = CoopCharacter_GetSelection();
    return choice == 0 ? original : CoopCharacter_GetGraphicsId(choice);
}

static void RemovePreview(void)
{
    if (sPreview != MAX_SPRITES)
    {
        u8 palette = gSprites[sPreview].oam.paletteNum;
        DestroySprite(&gSprites[sPreview]);
        FieldEffectFreePaletteIfUnused(palette);
        sPreview = MAX_SPRITES;
    }
}

static void Print(const u8 *text, u8 x, u8 y)
{
    AddTextPrinterParameterized(sWindow, FONT_SMALL, text, x, y, TEXT_SKIP_DRAW, NULL);
}

static void Draw(void)
{
    u8 avatar = sChoice == 0 ? DefaultAvatar() : sChoice;
    RemovePreview();
    FillWindowPixelBuffer(sWindow, PIXEL_FILL(1));
    Print(COMPOUND_STRING("CHARACTER"), 8, 0);
    Print(sChoice == 0 ? COMPOUND_STRING("Original appearance") : sCharacters[sChoice - 1].name, 8, 20);
    Print(COMPOUND_STRING("LEFT/RIGHT: choose"), 8, 80);
    Print(COMPOUND_STRING("A: use     B: cancel"), 8, 96);
    Print(COMPOUND_STRING("Walking appearance only."), 8, 116);
    Print(COMPOUND_STRING("Save your game to keep it."), 8, 130);
    sPreview = CreateObjectGraphicsSprite(CoopCharacter_GetGraphicsId(avatar), SpriteCallbackDummy, 120, 66, 0);
    if (sPreview != MAX_SPRITES)
    {
        gSprites[sPreview].oam.priority = 0;
        StartSpriteAnim(&gSprites[sPreview], ANIM_STD_GO_SOUTH);
    }
    else
    {
        u8 palette = IndexOfSpritePaletteTag(GetObjectEventGraphicsInfo(CoopCharacter_GetGraphicsId(avatar))->paletteTag);
        if (palette != 0xFF) FieldEffectFreePaletteIfUnused(palette);
        Print(COMPOUND_STRING("Preview unavailable"), 8, 48);
    }
    CopyWindowToVram(sWindow, COPYWIN_GFX);
}

static void Close(u8 taskId)
{
    RemovePreview();
    ClearStdWindowAndFrame(sWindow, TRUE);
    RemoveWindow(sWindow);
    ScriptUnfreezeObjectEvents();
    UnlockPlayerFieldControls();
    DestroyTask(taskId);
}

static void Task_Character(u8 taskId)
{
    if (JOY_NEW(B_BUTTON)) { Close(taskId); return; }
    if (JOY_NEW(DPAD_LEFT | DPAD_UP))
    {
        sChoice = sChoice == 0 ? COOP_CHARACTER_COUNT : sChoice - 1;
        Draw();
    }
    else if (JOY_NEW(DPAD_RIGHT | DPAD_DOWN))
    {
        sChoice = (sChoice + 1) % (COOP_CHARACTER_COUNT + 1);
        Draw();
    }
    if (JOY_NEW(A_BUTTON))
    {
        struct ObjectEvent *player = &gObjectEvents[gPlayerAvatar.objectEventId];
        CoopCharacter_SetSelection(sChoice);
        // Specialized states retain their native graphics until returning on foot.
        if (gPlayerAvatar.flags & PLAYER_AVATAR_FLAG_ON_FOOT)
        {
            ObjectEventSetGraphicsId(player, GetPlayerAvatarGraphicsIdByStateId(PLAYER_AVATAR_STATE_NORMAL));
            ObjectEventTurn(player, player->facingDirection);
        }
        PlaySE(SE_SELECT);
        Close(taskId);
    }
}

void CoopCharacter_Open(void)
{
    if (GetTaskCount() == NUM_TASKS) goto fail;
    sWindow = AddWindow(&sWindowTemplate);
    if (sWindow == WINDOW_NONE) goto fail;
    CreateTask(Task_Character, 0x50);
    sPreview = MAX_SPRITES;
    sChoice = CoopCharacter_GetSelection();
    PutWindowTilemap(sWindow);
    DrawStdWindowFrame(sWindow, FALSE);
    Draw();
    CopyWindowToVram(sWindow, COPYWIN_FULL);
    return;
fail:
    ScriptUnfreezeObjectEvents();
    UnlockPlayerFieldControls();
}
