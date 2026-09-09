#ifndef GUARD_DATA_OBJECT_EVENTS_JOHTO_BERRY_GRAPHICS_H
#define GUARD_DATA_OBJECT_EVENTS_JOHTO_BERRY_GRAPHICS_H

#include "constants/johto_berry_plots.h"

/* Generated from donor revision 751823abaf677020bcd72c45fe3e7cb2b8a576e4. */
const u32 gJohtoBerryPic_CHERI[] = INCBIN_U32("graphics/johto/berry_trees/cheri.4bpp");
const u32 gJohtoBerryPic_CHESTO[] = INCBIN_U32("graphics/johto/berry_trees/chesto.4bpp");
const u32 gJohtoBerryPic_PECHA[] = INCBIN_U32("graphics/johto/berry_trees/pecha.4bpp");
const u32 gJohtoBerryPic_RAWST[] = INCBIN_U32("graphics/johto/berry_trees/rawst.4bpp");
const u32 gJohtoBerryPic_ASPEAR[] = INCBIN_U32("graphics/johto/berry_trees/aspear.4bpp");
const u32 gJohtoBerryPic_LEPPA[] = INCBIN_U32("graphics/johto/berry_trees/leppa.4bpp");
const u32 gJohtoBerryPic_ORAN[] = INCBIN_U32("graphics/johto/berry_trees/oran.4bpp");
const u32 gJohtoBerryPic_PERSIM[] = INCBIN_U32("graphics/johto/berry_trees/persim.4bpp");
const u32 gJohtoBerryPic_LUM[] = INCBIN_U32("graphics/johto/berry_trees/lum.4bpp");
const u32 gJohtoBerryPic_SITRUS[] = INCBIN_U32("graphics/johto/berry_trees/sitrus.4bpp");
const u32 gJohtoBerryPic_DIRT_PILE[] = INCBIN_U32("graphics/johto/berry_trees/dirt_pile.4bpp");
const u32 gJohtoBerryPic_SPROUT[] = INCBIN_U32("graphics/johto/berry_trees/sprout.4bpp");

static const struct SpriteFrameImage sJohtoBerryPicTable_CHERI[] = {
    overworld_frame(gJohtoBerryPic_DIRT_PILE, 2, 2, 0),
    overworld_frame(gJohtoBerryPic_SPROUT, 2, 2, 0),
    overworld_frame(gJohtoBerryPic_SPROUT, 2, 2, 1),
    overworld_frame(gJohtoBerryPic_CHERI, 2, 4, 0),
    overworld_frame(gJohtoBerryPic_CHERI, 2, 4, 1),
    overworld_frame(gJohtoBerryPic_CHERI, 2, 4, 2),
    overworld_frame(gJohtoBerryPic_CHERI, 2, 4, 3),
    overworld_frame(gJohtoBerryPic_CHERI, 2, 4, 4),
    overworld_frame(gJohtoBerryPic_CHERI, 2, 4, 5),
};

static const u16 sJohtoBerryPaletteTags_CHERI[] = {
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_PINK,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_WHITE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_WHITE,
};

static const struct SpriteFrameImage sJohtoBerryPicTable_CHESTO[] = {
    overworld_frame(gJohtoBerryPic_DIRT_PILE, 2, 2, 0),
    overworld_frame(gJohtoBerryPic_SPROUT, 2, 2, 0),
    overworld_frame(gJohtoBerryPic_SPROUT, 2, 2, 1),
    overworld_frame(gJohtoBerryPic_CHESTO, 2, 4, 0),
    overworld_frame(gJohtoBerryPic_CHESTO, 2, 4, 1),
    overworld_frame(gJohtoBerryPic_CHESTO, 2, 4, 2),
    overworld_frame(gJohtoBerryPic_CHESTO, 2, 4, 3),
    overworld_frame(gJohtoBerryPic_CHESTO, 2, 4, 4),
    overworld_frame(gJohtoBerryPic_CHESTO, 2, 4, 5),
};

static const u16 sJohtoBerryPaletteTags_CHESTO[] = {
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_PINK,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_WHITE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_WHITE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_WHITE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE,
};

static const struct SpriteFrameImage sJohtoBerryPicTable_PECHA[] = {
    overworld_frame(gJohtoBerryPic_DIRT_PILE, 2, 2, 0),
    overworld_frame(gJohtoBerryPic_SPROUT, 2, 2, 0),
    overworld_frame(gJohtoBerryPic_SPROUT, 2, 2, 1),
    overworld_frame(gJohtoBerryPic_PECHA, 2, 4, 0),
    overworld_frame(gJohtoBerryPic_PECHA, 2, 4, 1),
    overworld_frame(gJohtoBerryPic_PECHA, 2, 4, 2),
    overworld_frame(gJohtoBerryPic_PECHA, 2, 4, 3),
    overworld_frame(gJohtoBerryPic_PECHA, 2, 4, 4),
    overworld_frame(gJohtoBerryPic_PECHA, 2, 4, 5),
};

static const u16 sJohtoBerryPaletteTags_PECHA[] = {
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_PINK,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_GREEN,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_GREEN,
};

static const struct SpriteFrameImage sJohtoBerryPicTable_RAWST[] = {
    overworld_frame(gJohtoBerryPic_DIRT_PILE, 2, 2, 0),
    overworld_frame(gJohtoBerryPic_SPROUT, 2, 2, 0),
    overworld_frame(gJohtoBerryPic_SPROUT, 2, 2, 1),
    overworld_frame(gJohtoBerryPic_RAWST, 2, 4, 0),
    overworld_frame(gJohtoBerryPic_RAWST, 2, 4, 1),
    overworld_frame(gJohtoBerryPic_RAWST, 2, 4, 2),
    overworld_frame(gJohtoBerryPic_RAWST, 2, 4, 3),
    overworld_frame(gJohtoBerryPic_RAWST, 2, 4, 4),
    overworld_frame(gJohtoBerryPic_RAWST, 2, 4, 5),
};

static const u16 sJohtoBerryPaletteTags_RAWST[] = {
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_PINK,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_PINK,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_PINK,
};

static const struct SpriteFrameImage sJohtoBerryPicTable_ASPEAR[] = {
    overworld_frame(gJohtoBerryPic_DIRT_PILE, 2, 2, 0),
    overworld_frame(gJohtoBerryPic_SPROUT, 2, 2, 0),
    overworld_frame(gJohtoBerryPic_SPROUT, 2, 2, 1),
    overworld_frame(gJohtoBerryPic_ASPEAR, 2, 4, 0),
    overworld_frame(gJohtoBerryPic_ASPEAR, 2, 4, 1),
    overworld_frame(gJohtoBerryPic_ASPEAR, 2, 4, 2),
    overworld_frame(gJohtoBerryPic_ASPEAR, 2, 4, 3),
    overworld_frame(gJohtoBerryPic_ASPEAR, 2, 4, 4),
    overworld_frame(gJohtoBerryPic_ASPEAR, 2, 4, 5),
};

static const u16 sJohtoBerryPaletteTags_ASPEAR[] = {
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_PINK,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_PINK,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_PINK,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_PINK,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE,
};

static const struct SpriteFrameImage sJohtoBerryPicTable_LEPPA[] = {
    overworld_frame(gJohtoBerryPic_DIRT_PILE, 2, 2, 0),
    overworld_frame(gJohtoBerryPic_SPROUT, 2, 2, 0),
    overworld_frame(gJohtoBerryPic_SPROUT, 2, 2, 1),
    overworld_frame(gJohtoBerryPic_LEPPA, 2, 4, 0),
    overworld_frame(gJohtoBerryPic_LEPPA, 2, 4, 1),
    overworld_frame(gJohtoBerryPic_LEPPA, 2, 4, 2),
    overworld_frame(gJohtoBerryPic_LEPPA, 2, 4, 3),
    overworld_frame(gJohtoBerryPic_LEPPA, 2, 4, 4),
    overworld_frame(gJohtoBerryPic_LEPPA, 2, 4, 5),
};

static const u16 sJohtoBerryPaletteTags_LEPPA[] = {
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_PINK,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_PINK,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_PINK,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_PINK,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_WHITE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_WHITE,
};

static const struct SpriteFrameImage sJohtoBerryPicTable_ORAN[] = {
    overworld_frame(gJohtoBerryPic_DIRT_PILE, 2, 2, 0),
    overworld_frame(gJohtoBerryPic_SPROUT, 2, 2, 0),
    overworld_frame(gJohtoBerryPic_SPROUT, 2, 2, 1),
    overworld_frame(gJohtoBerryPic_ORAN, 2, 4, 0),
    overworld_frame(gJohtoBerryPic_ORAN, 2, 4, 1),
    overworld_frame(gJohtoBerryPic_ORAN, 2, 4, 2),
    overworld_frame(gJohtoBerryPic_ORAN, 2, 4, 3),
    overworld_frame(gJohtoBerryPic_ORAN, 2, 4, 4),
    overworld_frame(gJohtoBerryPic_ORAN, 2, 4, 5),
};

static const u16 sJohtoBerryPaletteTags_ORAN[] = {
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_PINK,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_WHITE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_WHITE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_WHITE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_PINK,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_PINK,
};

static const struct SpriteFrameImage sJohtoBerryPicTable_PERSIM[] = {
    overworld_frame(gJohtoBerryPic_DIRT_PILE, 2, 2, 0),
    overworld_frame(gJohtoBerryPic_SPROUT, 2, 2, 0),
    overworld_frame(gJohtoBerryPic_SPROUT, 2, 2, 1),
    overworld_frame(gJohtoBerryPic_PERSIM, 2, 4, 0),
    overworld_frame(gJohtoBerryPic_PERSIM, 2, 4, 1),
    overworld_frame(gJohtoBerryPic_PERSIM, 2, 4, 2),
    overworld_frame(gJohtoBerryPic_PERSIM, 2, 4, 3),
    overworld_frame(gJohtoBerryPic_PERSIM, 2, 4, 4),
    overworld_frame(gJohtoBerryPic_PERSIM, 2, 4, 5),
};

static const u16 sJohtoBerryPaletteTags_PERSIM[] = {
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_PINK,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_WHITE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_WHITE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_WHITE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_PINK,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_PINK,
};

static const struct SpriteFrameImage sJohtoBerryPicTable_LUM[] = {
    overworld_frame(gJohtoBerryPic_DIRT_PILE, 2, 2, 0),
    overworld_frame(gJohtoBerryPic_SPROUT, 2, 2, 0),
    overworld_frame(gJohtoBerryPic_SPROUT, 2, 2, 1),
    overworld_frame(gJohtoBerryPic_LUM, 2, 4, 0),
    overworld_frame(gJohtoBerryPic_LUM, 2, 4, 1),
    overworld_frame(gJohtoBerryPic_LUM, 2, 4, 2),
    overworld_frame(gJohtoBerryPic_LUM, 2, 4, 3),
    overworld_frame(gJohtoBerryPic_LUM, 2, 4, 4),
    overworld_frame(gJohtoBerryPic_LUM, 2, 4, 5),
};

static const u16 sJohtoBerryPaletteTags_LUM[] = {
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_PINK,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_GREEN,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_GREEN,
};

static const struct SpriteFrameImage sJohtoBerryPicTable_SITRUS[] = {
    overworld_frame(gJohtoBerryPic_DIRT_PILE, 2, 2, 0),
    overworld_frame(gJohtoBerryPic_SPROUT, 2, 2, 0),
    overworld_frame(gJohtoBerryPic_SPROUT, 2, 2, 1),
    overworld_frame(gJohtoBerryPic_SITRUS, 2, 4, 0),
    overworld_frame(gJohtoBerryPic_SITRUS, 2, 4, 1),
    overworld_frame(gJohtoBerryPic_SITRUS, 2, 4, 2),
    overworld_frame(gJohtoBerryPic_SITRUS, 2, 4, 3),
    overworld_frame(gJohtoBerryPic_SITRUS, 2, 4, 4),
    overworld_frame(gJohtoBerryPic_SITRUS, 2, 4, 5),
};

static const u16 sJohtoBerryPaletteTags_SITRUS[] = {
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_PINK,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_GREEN,
    OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_GREEN,
};

static const struct SpriteFrameImage *const sJohtoBerryPicTables[] = {
    [BERRY_ID_CHERI] = sJohtoBerryPicTable_CHERI,
    [BERRY_ID_CHESTO] = sJohtoBerryPicTable_CHESTO,
    [BERRY_ID_PECHA] = sJohtoBerryPicTable_PECHA,
    [BERRY_ID_RAWST] = sJohtoBerryPicTable_RAWST,
    [BERRY_ID_ASPEAR] = sJohtoBerryPicTable_ASPEAR,
    [BERRY_ID_LEPPA] = sJohtoBerryPicTable_LEPPA,
    [BERRY_ID_ORAN] = sJohtoBerryPicTable_ORAN,
    [BERRY_ID_PERSIM] = sJohtoBerryPicTable_PERSIM,
    [BERRY_ID_LUM] = sJohtoBerryPicTable_LUM,
    [BERRY_ID_SITRUS] = sJohtoBerryPicTable_SITRUS,
};

static const u16 *const sJohtoBerryPaletteTags[] = {
    [BERRY_ID_CHERI] = sJohtoBerryPaletteTags_CHERI,
    [BERRY_ID_CHESTO] = sJohtoBerryPaletteTags_CHESTO,
    [BERRY_ID_PECHA] = sJohtoBerryPaletteTags_PECHA,
    [BERRY_ID_RAWST] = sJohtoBerryPaletteTags_RAWST,
    [BERRY_ID_ASPEAR] = sJohtoBerryPaletteTags_ASPEAR,
    [BERRY_ID_LEPPA] = sJohtoBerryPaletteTags_LEPPA,
    [BERRY_ID_ORAN] = sJohtoBerryPaletteTags_ORAN,
    [BERRY_ID_PERSIM] = sJohtoBerryPaletteTags_PERSIM,
    [BERRY_ID_LUM] = sJohtoBerryPaletteTags_LUM,
    [BERRY_ID_SITRUS] = sJohtoBerryPaletteTags_SITRUS,
};

static bool8 JohtoBerryGraphics_IsSupported(u8 plotId, u8 berryId, u8 stage)
{
    return plotId >= JOHTO_BERRY_PLOTS_FIRST
        && plotId <= JOHTO_BERRY_PLOTS_LAST
        && berryId >= BERRY_ID_CHERI
        && berryId <= BERRY_ID_SITRUS
        && stage < ARRAY_COUNT(sJohtoBerryPaletteTags_CHERI);
}

static bool8 JohtoBerryGraphics_Apply(struct ObjectEvent *objectEvent, struct Sprite *sprite, u8 berryId, u8 stage)
{
    u8 paletteIndex;
    const u16 *paletteTags;
    if (!JohtoBerryGraphics_IsSupported(objectEvent->trainerRange_berryTreeId, berryId, stage))
        return FALSE;
    paletteTags = sJohtoBerryPaletteTags[berryId];
    if (paletteTags == NULL)
        return FALSE;
    paletteIndex = FindObjectEventPaletteIndexByTag(paletteTags[stage]);
    if (paletteIndex == 0xFF)
        return FALSE;
    UpdateSpritePalette(&sObjectEventSpritePalettes[paletteIndex], sprite);
    sprite->images = sJohtoBerryPicTables[berryId];
    return TRUE;
}

#endif // GUARD_DATA_OBJECT_EVENTS_JOHTO_BERRY_GRAPHICS_H
