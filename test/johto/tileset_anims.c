#include "global.h"
#include "fieldmap.h"
#include "test/test.h"
#include "tileset_anims.h"

static const u16 sGeneralFlowerFrame0[] = INCBIN_U16("graphics/johto/tileset_anims/general_flower/0.4bpp");
static const u16 sGeneralFlowerFrame4[] = INCBIN_U16("graphics/johto/tileset_anims/general_flower/4.4bpp");
static const u16 sGeneralSandFrame1[] = INCBIN_U16("graphics/johto/tileset_anims/general_sand/1.4bpp");
static const u16 sGeneralWaterfallFrame0[] = INCBIN_U16("graphics/johto/tileset_anims/general_waterfall/0.4bpp");
static const u16 sParkLargeFrame1[] = INCBIN_U16("graphics/johto/tileset_anims/park_large/1.4bpp");
static const u16 sParkSmallFrame0[] = INCBIN_U16("graphics/johto/tileset_anims/park_small/0.4bpp");
static const u16 sParkRedFrame1[] = INCBIN_U16("graphics/johto/tileset_anims/park_red/1.4bpp");
static const u16 sParkYellowFrame1[] = INCBIN_U16("graphics/johto/tileset_anims/park_yellow/1.4bpp");
static const u16 sTheaterFrame1[] = INCBIN_U16("graphics/johto/tileset_anims/theater/1.4bpp");
static const u16 sAzaleaFrame1[] = INCBIN_U16("graphics/johto/tileset_anims/azalea/1.4bpp");
static const u16 sBlackthornFrame0[] = INCBIN_U16("graphics/johto/tileset_anims/blackthorn/0.4bpp");
static const u16 sBlackthornFrame7[] = INCBIN_U16("graphics/johto/tileset_anims/blackthorn/7.4bpp");
static const u16 sCeladonFrame0[] = INCBIN_U16("data/tilesets/secondary/celadon_city_frlg/anim/fountain/0.4bpp");
static const u16 sCeladonFrame1[] = INCBIN_U16("data/tilesets/secondary/celadon_city_frlg/anim/fountain/1.4bpp");
static const u16 sCeladonFrame4[] = INCBIN_U16("data/tilesets/secondary/celadon_city_frlg/anim/fountain/4.4bpp");
static const u16 sSilphCoFrame0[] = INCBIN_U16("data/tilesets/secondary/silph_co_frlg/anim/fountain/0.4bpp");
static const u16 sSilphCoFrame1[] = INCBIN_U16("data/tilesets/secondary/silph_co_frlg/anim/fountain/1.4bpp");
static const u16 sSilphCoFrame3[] = INCBIN_U16("data/tilesets/secondary/silph_co_frlg/anim/fountain/3.4bpp");

static const u16 sGeneralLandFrame0[] = INCBIN_U16("graphics/johto/tileset_anims/general_land/0.4bpp");
static const u16 sGeneralLandFrame3[] = INCBIN_U16("graphics/johto/tileset_anims/general_land/3.4bpp");

static void RestoreAnimationFixture(const struct MapLayout *layout, u16 dispcnt)
{
    const struct MapLayout emptyLayout = {0};
    gMapHeader.mapLayout = layout != NULL ? layout : &emptyLayout;
    InitTilesetAnimations();
    gMapHeader.mapLayout = layout;
    REG_DISPCNT = dispcnt;
}

static void RunAnimationFrames(u32 count)
{
    while (count-- != 0)
    {
        UpdateTilesetAnimations();
        TransferTilesetAnimsBuffer();
    }
}

static void PrepareAnimationFixture(struct MapLayout *layout,
                                    struct Tileset *primary,
                                    struct Tileset *secondary,
                                    TilesetCB primaryCallback,
                                    TilesetCB secondaryCallback)
{
    memset(layout, 0, sizeof(*layout));
    memset(primary, 0, sizeof(*primary));
    memset(secondary, 0, sizeof(*secondary));
    primary->callback = primaryCallback;
    secondary->callback = secondaryCallback;
    layout->primaryTileset = primary;
    layout->secondaryTileset = secondary;
    gMapHeader.mapLayout = layout;
    REG_DISPCNT |= DISPCNT_FORCED_BLANK;
    CpuFill16(0xA55A, (u16 *)BG_VRAM, VRAM_SIZE);
    InitTilesetAnimations();
}

static void ExpectCopy(u16 destinationTile, const void *source, u16 size)
{
    EXPECT_EQ(memcmp((const void *)(BG_VRAM + TILE_OFFSET_4BPP(destinationTile)), source, size), 0);
}

static void ExpectVramSentinels(void)
{
    EXPECT_EQ(*(const u16 *)(BG_VRAM + TILE_OFFSET_4BPP(0)), 0xA55A);
    EXPECT_EQ(*(const u16 *)(BG_VRAM + TILE_OFFSET_4BPP(1023)), 0xA55A);
    EXPECT_EQ(*(const u16 *)(BG_VRAM + BG_VRAM_SIZE), 0xA55A);
}

TEST("Johto general animation copies exact frames, cadence, offset, and wraps")
{
    struct MapLayout layout;
    struct Tileset primary;
    struct Tileset secondary;
    const struct MapLayout *oldLayout = gMapHeader.mapLayout;
    u16 oldDispcnt = REG_DISPCNT;

    PrepareAnimationFixture(&layout, &primary, &secondary, InitTilesetAnim_JohtoGeneral, NULL);
    RunAnimationFrames(1);
    ExpectCopy(480, sGeneralLandFrame0, 320);
    RunAnimationFrames(1);
    ExpectCopy(508, sGeneralFlowerFrame0, 128);
    ExpectVramSentinels();

    RunAnimationFrames(6);
    ExpectCopy(416, sGeneralSandFrame1, 576);
    RunAnimationFrames(41);
    ExpectCopy(480, sGeneralLandFrame3, 320);
    RunAnimationFrames(16);
    ExpectCopy(480, sGeneralLandFrame0, 320);
    RunAnimationFrames(1);
    ExpectCopy(508, sGeneralFlowerFrame4, 128);
    RunAnimationFrames(16);
    ExpectCopy(508, sGeneralFlowerFrame0, 128);

    CpuFill16(0xA55A, (u16 *)BG_VRAM, VRAM_SIZE);
    InitTilesetAnimations();
    RunAnimationFrames(3);
    ExpectCopy(450, (const u8 *)sGeneralWaterfallFrame0 + 1088, 384);

    RestoreAnimationFixture(oldLayout, oldDispcnt);
}

TEST("Johto National Park animation keeps four streams on the secondary sheet")
{
    struct MapLayout layout;
    struct Tileset primary;
    struct Tileset secondary;
    const struct MapLayout *oldLayout = gMapHeader.mapLayout;
    u16 oldDispcnt = REG_DISPCNT;

    PrepareAnimationFixture(&layout, &primary, &secondary, NULL, InitTilesetAnim_JohtoNationalPark);
    RunAnimationFrames(1);
    ExpectCopy(744, sParkSmallFrame0, 256);
    RunAnimationFrames(9);
    ExpectCopy(728, sParkLargeFrame1, 256);
    RunAnimationFrames(8);
    ExpectCopy(736, sParkRedFrame1, 128);
    RunAnimationFrames(10);
    ExpectCopy(740, sParkYellowFrame1, 128);
    ExpectVramSentinels();

    RestoreAnimationFixture(oldLayout, oldDispcnt);
}

TEST("Johto gym and theater callbacks preserve stream timing")
{
    struct MapLayout layout;
    struct Tileset primary;
    struct Tileset secondary;
    const struct MapLayout *oldLayout = gMapHeader.mapLayout;
    u16 oldDispcnt = REG_DISPCNT;

    PrepareAnimationFixture(&layout, &primary, &secondary, NULL, InitTilesetAnim_JohtoEcruteakTheater);
    RunAnimationFrames(10);
    ExpectCopy(744, sTheaterFrame1, 128);

    PrepareAnimationFixture(&layout, &primary, &secondary, NULL, InitTilesetAnim_JohtoAzaleaGym);
    RunAnimationFrames(10);
    ExpectCopy(739, sAzaleaFrame1, 128);

    PrepareAnimationFixture(&layout, &primary, &secondary, NULL, InitTilesetAnim_JohtoBlackthornGym);
    RunAnimationFrames(1);
    ExpectCopy(961, sBlackthornFrame0, 128);
    RunAnimationFrames(112);
    ExpectCopy(961, sBlackthornFrame7, 128);
    RunAnimationFrames(16);
    ExpectCopy(961, sBlackthornFrame0, 128);

    RestoreAnimationFixture(oldLayout, oldDispcnt);
}

TEST("Imported Kanto callbacks copy exact secondary frames on cadence and wrap")
{
    struct MapLayout layout;
    struct Tileset primary;
    struct Tileset secondary;
    const struct MapLayout *oldLayout = gMapHeader.mapLayout;
    u16 oldDispcnt = REG_DISPCNT;

    PrepareAnimationFixture(&layout, &primary, &secondary, NULL, InitTilesetAnim_CeladonCity);
    RunAnimationFrames(12);
    ExpectCopy(744, sCeladonFrame1, 256);
    RunAnimationFrames(36);
    ExpectCopy(744, sCeladonFrame4, 256);
    RunAnimationFrames(12);
    ExpectCopy(744, sCeladonFrame0, 256);
    ExpectVramSentinels();

    PrepareAnimationFixture(&layout, &primary, &secondary, NULL, InitTilesetAnim_SilphCo);
    RunAnimationFrames(10);
    ExpectCopy(976, sSilphCoFrame1, 256);
    RunAnimationFrames(20);
    ExpectCopy(976, sSilphCoFrame3, 256);
    RunAnimationFrames(10);
    ExpectCopy(976, sSilphCoFrame0, 256);
    ExpectVramSentinels();

    RestoreAnimationFixture(oldLayout, oldDispcnt);
}
