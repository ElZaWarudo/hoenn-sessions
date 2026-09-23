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
#include "string_util.h"
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
#define CHARACTERS_PER_PAGE 6
#define LEGACY_CHARACTER_COUNT 176

static const struct {
    u16 graphicsId;
    const u8 *name;
} sFeaturedCharacters[] = {
    {OBJ_EVENT_GFX_BRENDAN_NORMAL, COMPOUND_STRING("Brendan")},
    {OBJ_EVENT_GFX_MAY_NORMAL, COMPOUND_STRING("May")},
    {OBJ_EVENT_GFX_RED_NORMAL, COMPOUND_STRING("Red")},
    {OBJ_EVENT_GFX_GREEN_NORMAL, COMPOUND_STRING("Leaf")},
    {OBJ_EVENT_GFX_WALLY, COMPOUND_STRING("Wally")},
    {OBJ_EVENT_GFX_STEVEN, COMPOUND_STRING("Steven")},
    {OBJ_EVENT_GFX_NORMAN, COMPOUND_STRING("Norman")},
    {OBJ_EVENT_GFX_YOUNGSTER, COMPOUND_STRING("Youngster")},
    {OBJ_EVENT_GFX_LASS, COMPOUND_STRING("Lass")},
    {OBJ_EVENT_GFX_PROF_BIRCH, COMPOUND_STRING("Birch")},
    {OBJ_EVENT_GFX_HIKER, COMPOUND_STRING("Hiker")},
    {OBJ_EVENT_GFX_SAILOR, COMPOUND_STRING("Sailor")},
};
_Static_assert(ARRAY_COUNT(sFeaturedCharacters) == COOP_PRESENCE_AVATAR_SAILOR, "featured roster size");

// Append Johto choices so existing saved and network avatar ids keep their meaning.
static const struct {
    u16 graphicsId;
    const u8 *name;
} sJohtoCharacters[] = {
    {OBJ_EVENT_GFX_JOHTO_SILVER, COMPOUND_STRING("Silver")},
    {OBJ_EVENT_GFX_JOHTO_SUPER_NERD, COMPOUND_STRING("Nerd")},
    {OBJ_EVENT_GFX_JOHTO_KIMONO_GIRL, COMPOUND_STRING("Kimono")},
    {OBJ_EVENT_GFX_JOHTO_KURT, COMPOUND_STRING("Kurt")},
    {OBJ_EVENT_GFX_JOHTO_BATTLE_GIRL, COMPOUND_STRING("B.Girl")},
    {OBJ_EVENT_GFX_JOHTO_SAGE, COMPOUND_STRING("Sage")},
    {OBJ_EVENT_GFX_JOHTO_ATTENDANT, COMPOUND_STRING("Attendant")},
    {OBJ_EVENT_GFX_JOHTO_EUSINE, COMPOUND_STRING("Eusine")},
    {OBJ_EVENT_GFX_JOHTO_ENGINEER, COMPOUND_STRING("Engineer")},
    {OBJ_EVENT_GFX_JOHTO_FIREBREATHER, COMPOUND_STRING("Firebr.")},
    {OBJ_EVENT_GFX_JOHTO_JUGGLER, COMPOUND_STRING("Juggler")},
    {OBJ_EVENT_GFX_JOHTO_ARCHER, COMPOUND_STRING("Archer")},
    {OBJ_EVENT_GFX_JOHTO_SCIENTIST_M, COMPOUND_STRING("Sci. M")},
    {OBJ_EVENT_GFX_JOHTO_PROF_ELM, COMPOUND_STRING("Elm")},
    {OBJ_EVENT_GFX_JOHTO_SCIENTIST_F, COMPOUND_STRING("Sci. F")},
    {OBJ_EVENT_GFX_JOHTO_NURSE_CHANSEY, COMPOUND_STRING("Chansey")},
    {OBJ_EVENT_GFX_JOHTO_FALKNER, COMPOUND_STRING("Falkner")},
    {OBJ_EVENT_GFX_JOHTO_BUGSY, COMPOUND_STRING("Bugsy")},
    {OBJ_EVENT_GFX_JOHTO_BURGLAR, COMPOUND_STRING("Burglar")},
    {OBJ_EVENT_GFX_JOHTO_WHITNEY, COMPOUND_STRING("Whitney")},
    {OBJ_EVENT_GFX_JOHTO_ATTENDANT_M, COMPOUND_STRING("Att. M")},
    {OBJ_EVENT_GFX_JOHTO_PROTON, COMPOUND_STRING("Proton")},
    {OBJ_EVENT_GFX_JOHTO_ARIANA, COMPOUND_STRING("Ariana")},
    {OBJ_EVENT_GFX_JOHTO_PETREL, COMPOUND_STRING("Petrel")},
    {OBJ_EVENT_GFX_JOHTO_MORTY, COMPOUND_STRING("Morty")},
    {OBJ_EVENT_GFX_JOHTO_JASMINE, COMPOUND_STRING("Jasmine")},
    {OBJ_EVENT_GFX_JOHTO_CHUCK, COMPOUND_STRING("Chuck")},
    {OBJ_EVENT_GFX_JOHTO_PRYCE, COMPOUND_STRING("Pryce")},
    {OBJ_EVENT_GFX_JOHTO_CLAIR, COMPOUND_STRING("Clair")},
    {OBJ_EVENT_GFX_JOHTO_JANINE, COMPOUND_STRING("Janine")},
};
_Static_assert(LEGACY_CHARACTER_COUNT + ARRAY_COUNT(sJohtoCharacters) == COOP_CHARACTER_COUNT, "character roster size");

static EWRAM_DATA u8 sWindow;
static EWRAM_DATA u8 sPreviews[CHARACTERS_PER_PAGE];
static EWRAM_DATA u8 sChoice;
static EWRAM_DATA bool8 sRosterInitialized;
static EWRAM_DATA u16 sCharacterGraphicsIds[COOP_CHARACTER_COUNT];
static const struct WindowTemplate sWindowTemplate = {
    .bg = 0, .tilemapLeft = 2, .tilemapTop = 1,
    .width = 26, .height = 18, .paletteNum = 15, .baseBlock = 8,
};

static bool32 IsFeaturedGraphicsId(u16 graphicsId)
{
    for (u32 i = 0; i < ARRAY_COUNT(sFeaturedCharacters); i++)
        if (sFeaturedCharacters[i].graphicsId == graphicsId)
            return TRUE;
    return FALSE;
}

static bool32 IsSelectableGraphicsId(u16 graphicsId)
{
    const struct ObjectEventGraphicsInfo *info = GetObjectEventGraphicsInfo(graphicsId);
    const union AnimCmd *const *standardAnims = GetObjectEventGraphicsInfo(OBJ_EVENT_GFX_YOUNGSTER)->anims;

    if (info->width != 16 || info->height != 32 || info->inanimate)
        return FALSE;
    if (info->anims == standardAnims)
        return TRUE;
    switch (graphicsId)
    {
    case OBJ_EVENT_GFX_BRENDAN_NORMAL:
    case OBJ_EVENT_GFX_MAY_NORMAL:
    case OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL:
    case OBJ_EVENT_GFX_RIVAL_MAY_NORMAL:
    case OBJ_EVENT_GFX_LINK_BRENDAN:
    case OBJ_EVENT_GFX_LINK_MAY:
    case OBJ_EVENT_GFX_RED_NORMAL:
    case OBJ_EVENT_GFX_GREEN_NORMAL:
        return TRUE;
    default:
        return FALSE;
    }
}

static void InitCharacterRoster(void)
{
    u32 count = 0;

    if (sRosterInitialized)
        return;
    for (u32 i = 0; i < ARRAY_COUNT(sFeaturedCharacters); i++)
        sCharacterGraphicsIds[count++] = sFeaturedCharacters[i].graphicsId;
    for (u32 graphicsId = 0; graphicsId < NUM_OBJ_EVENT_GFX && count < LEGACY_CHARACTER_COUNT; graphicsId++)
    {
        if (!IsFeaturedGraphicsId(graphicsId) && IsSelectableGraphicsId(graphicsId))
            sCharacterGraphicsIds[count++] = graphicsId;
    }
    for (u32 i = 0; i < ARRAY_COUNT(sJohtoCharacters); i++)
        sCharacterGraphicsIds[count++] = sJohtoCharacters[i].graphicsId;
    sRosterInitialized = TRUE;
}

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
    InitCharacterRoster();
    if (avatarId == 0 || avatarId > COOP_CHARACTER_COUNT)
        return OBJ_EVENT_GFX_BRENDAN_NORMAL;
    return sCharacterGraphicsIds[avatarId - 1];
}

u16 CoopCharacter_OverrideNormalGraphics(u16 original)
{
    u8 choice = CoopCharacter_GetSelection();
    return choice == 0 ? original : CoopCharacter_GetGraphicsId(choice);
}

static void RemovePreview(void)
{
    for (u32 i = 0; i < CHARACTERS_PER_PAGE; i++)
    {
        if (sPreviews[i] != MAX_SPRITES)
        {
            FieldEffectFreeGraphicsResources(&gSprites[sPreviews[i]]);
            sPreviews[i] = MAX_SPRITES;
        }
    }
}

static void Print(const u8 *text, u8 x, u8 y)
{
    AddTextPrinterParameterized(sWindow, FONT_SMALL, text, x, y, TEXT_SKIP_DRAW, NULL);
}

static void Draw(void)
{
    u8 page = sChoice / CHARACTERS_PER_PAGE;
    RemovePreview();
    FillWindowPixelBuffer(sWindow, PIXEL_FILL(1));
    Print(COMPOUND_STRING("CHARACTER"), 8, 0);
    ConvertIntToDecimalStringN(gStringVar1, page + 1, STR_CONV_MODE_LEFT_ALIGN, 2);
    ConvertIntToDecimalStringN(gStringVar2, (COOP_CHARACTER_COUNT / CHARACTERS_PER_PAGE) + 1, STR_CONV_MODE_LEFT_ALIGN, 2);
    StringExpandPlaceholders(gStringVar4, COMPOUND_STRING("Page {STR_VAR_1}/{STR_VAR_2}"));
    Print(gStringVar4, 120, 0);
    Print(COMPOUND_STRING("Walking only. Save to keep."), 8, 14);
    for (u32 i = 0; i < CHARACTERS_PER_PAGE; i++)
    {
        // Reserve sprite resources for the highlighted choice first.
        u8 slot = (sChoice % CHARACTERS_PER_PAGE + i) % CHARACTERS_PER_PAGE;
        u16 choice = page * CHARACTERS_PER_PAGE + slot;
        u8 column = slot % 3;
        u8 row = slot / 3;
        u8 x = 4 + column * 68;
        u8 y = 55 + row * 57;
        u8 avatar;

        if (choice > COOP_CHARACTER_COUNT)
            continue;
        if (choice == sChoice)
            Print(COMPOUND_STRING(">"), x, y);
        if (choice == 0)
            Print(COMPOUND_STRING("Original"), x + 8, y);
        else if (choice <= ARRAY_COUNT(sFeaturedCharacters))
            Print(sFeaturedCharacters[choice - 1].name, x + 8, y);
        else if (choice > LEGACY_CHARACTER_COUNT)
            Print(sJohtoCharacters[choice - LEGACY_CHARACTER_COUNT - 1].name, x + 8, y);
        else
        {
            ConvertIntToDecimalStringN(gStringVar1, choice, STR_CONV_MODE_LEFT_ALIGN, 3);
            StringExpandPlaceholders(gStringVar4, COMPOUND_STRING("No. {STR_VAR_1}"));
            Print(gStringVar4, x + 8, y);
        }
        avatar = choice == 0 ? DefaultAvatar() : choice;
        sPreviews[slot] = CreateObjectGraphicsSprite(CoopCharacter_GetGraphicsId(avatar), SpriteCallbackDummy,
                                                     48 + column * 68, 44 + row * 57, 0);
        if (sPreviews[slot] != MAX_SPRITES)
        {
            gSprites[sPreviews[slot]].oam.priority = 0;
            StartSpriteAnim(&gSprites[sPreviews[slot]], ANIM_STD_GO_SOUTH);
        }
        else
        {
            u16 graphicsId = CoopCharacter_GetGraphicsId(avatar);
            const struct ObjectEventGraphicsInfo *info = GetObjectEventGraphicsInfo(graphicsId);
            u16 tileTag = info->tileTag;
            u8 palette = IndexOfSpritePaletteTag(info->paletteTag);

            if (palette != 0xFF)
                FieldEffectFreePaletteIfUnused(palette);
            if (tileTag == TAG_NONE && info->compressed)
                tileTag = COMP_OW_TILE_TAG_BASE + graphicsId;
            if (tileTag != TAG_NONE)
            {
                u16 tileStart = GetSpriteTileStartByTag(tileTag);
                if (tileStart != TAG_NONE)
                    FieldEffectFreeTilesIfUnused(tileStart);
            }
        }
    }
    Print(COMPOUND_STRING("D-PAD: choose  L/R: page"), 8, 124);
    Print(COMPOUND_STRING("A: use  B: cancel"), 8, 136);
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
    if (JOY_NEW(DPAD_LEFT))
    {
        sChoice = sChoice == 0 ? COOP_CHARACTER_COUNT : sChoice - 1;
        Draw();
    }
    else if (JOY_NEW(DPAD_RIGHT))
    {
        sChoice = (sChoice + 1) % (COOP_CHARACTER_COUNT + 1);
        Draw();
    }
    else if (JOY_NEW(DPAD_UP) && sChoice >= 3)
    {
        sChoice -= 3;
        Draw();
    }
    else if (JOY_NEW(DPAD_DOWN) && sChoice + 3 <= COOP_CHARACTER_COUNT)
    {
        sChoice += 3;
        Draw();
    }
    else if (JOY_NEW(L_BUTTON | R_BUTTON))
    {
        u8 pageCount = (COOP_CHARACTER_COUNT / CHARACTERS_PER_PAGE) + 1;
        u8 slot = sChoice % CHARACTERS_PER_PAGE;
        u8 page = sChoice / CHARACTERS_PER_PAGE;

        if (JOY_NEW(L_BUTTON))
            page = page == 0 ? pageCount - 1 : page - 1;
        else
            page = (page + 1) % pageCount;
        sChoice = page * CHARACTERS_PER_PAGE + slot;
        if (sChoice > COOP_CHARACTER_COUNT)
            sChoice = COOP_CHARACTER_COUNT;
        Draw();
    }
    if (JOY_NEW(A_BUTTON))
    {
        struct ObjectEvent *player = &gObjectEvents[gPlayerAvatar.objectEventId];
        // Release preview sheets and palettes before changing the live player sprite.
        RemovePreview();
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
    for (u32 i = 0; i < CHARACTERS_PER_PAGE; i++)
        sPreviews[i] = MAX_SPRITES;
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
