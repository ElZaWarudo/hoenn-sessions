#include "global.h"
#include "fieldmap.h"
#include "field_camera.h"
#include "fldeff.h"
#include "overworld.h"
#include "tileset_anims.h"
#include "constants/metatile_behaviors.h"
#include "constants/metatile_labels.h"
#include "test/test.h"

#define LAYOUT_KANTO_LATER_SAFARI_BEACH 1136
#define LAYOUT_KANTO_LATER_SAFARI_BRUSH 1137
#define LAYOUT_KANTO_LATER_SAFARI_MOUNTAIN 1138

extern void Test_RunCutGrassMetatilePass(s16 x, s16 y, u8 side);

static const u16 sGeneralFlowerFrame1[] = INCBIN_U16("data/tilesets/primary/general/anim/flower/1.4bpp");
static const u16 sGeneralWaterFrame0[] = INCBIN_U16("data/tilesets/primary/general/anim/water/0.4bpp");
static const u16 sGeneralSandWaterEdgeFrame0[] = INCBIN_U16("data/tilesets/primary/general/anim/sand_water_edge/0.4bpp");
static const u16 sGeneralWaterfallFrame0[] = INCBIN_U16("data/tilesets/primary/general/anim/waterfall/0.4bpp");
static const u16 sGeneralLandWaterEdgeFrame0[] = INCBIN_U16("data/tilesets/primary/general/anim/land_water_edge/0.4bpp");

static void RunAnimationFrames(u32 count)
{
    while (count-- != 0)
    {
        UpdateTilesetAnimations();
        TransferTilesetAnimsBuffer();
    }
}

static void ExpectAnimationCopy(u16 destinationTile, const void *source, u16 size)
{
    EXPECT_EQ(memcmp((const void *)(BG_VRAM + TILE_OFFSET_4BPP(destinationTile)), source, size), 0);
}

TEST("Kanto Later General Safari layouts reject the FRLG primary gap")
{
    static const u16 layoutIds[] = {
        LAYOUT_KANTO_LATER_SAFARI_BEACH,
        LAYOUT_KANTO_LATER_SAFARI_BRUSH,
        LAYOUT_KANTO_LATER_SAFARI_MOUNTAIN,
    };
    u32 i;

    for (i = 0; i < ARRAY_COUNT(layoutIds); i++)
    {
        const struct MapLayout *layout = GetMapLayout(layoutIds[i]);
        EXPECT(layout != NULL);
        EXPECT(IsMetatileIdValidForMapLayout(layout, 0x1FF));
        EXPECT(!IsMetatileIdValidForMapLayout(layout, 0x200));
        EXPECT(!IsMetatileIdValidForMapLayout(layout, 0x208));
        EXPECT(!IsMetatileIdValidForMapLayout(layout, 0x27F));
        EXPECT(IsMetatileIdValidForMapLayout(layout, 0x280));
        EXPECT(IsMetatileIdValidForMapLayout(layout, 0x3FF));
    }

    // Native layouts keep their established 512/640 split behavior.
    EXPECT(IsMetatileIdValidForMapLayout(GetMapLayout(1), 0x208));
}

TEST("Kanto Later General Safari map grid rejects invalid dynamic writes")
{
    const struct MapLayout *savedLayout = gMapHeader.mapLayout;
    struct BackupMapLayout savedBackup = gBackupMapLayout;
    u16 mapData[] = {0x15};

    gMapHeader.mapLayout = GetMapLayout(LAYOUT_KANTO_LATER_SAFARI_BRUSH);
    gBackupMapLayout.width = 1;
    gBackupMapLayout.height = 1;
    gBackupMapLayout.map = mapData;

    MapGridSetMetatileIdAt(0, 0, 0x208);
    EXPECT_EQ(mapData[0], 0x15);
    MapGridSetMetatileEntryAt(0, 0, 0x27F);
    EXPECT_EQ(mapData[0], 0x15);

    mapData[0] = 0x208;
    EXPECT_EQ(MapGridGetMetatileIdAt(0, 0), 0);
    EXPECT_EQ(GetAttributeByMetatileIdAndMapLayout(0x208, METATILE_ATTRIBUTE_BEHAVIOR, TRUE), MB_INVALID);

    MapGridSetMetatileIdAt(0, 0, 0x280);
    EXPECT_EQ(mapData[0], 0x280);

    gBackupMapLayout = savedBackup;
    gMapHeader.mapLayout = savedLayout;
}

TEST("Kanto Later General Safari sanitizes invalid border and drawing reads")
{
    const struct MapLayout *savedLayout = gMapHeader.mapLayout;
    struct BackupMapLayout savedBackup = gBackupMapLayout;
    struct MapLayout layout = *GetMapLayout(LAYOUT_KANTO_LATER_SAFARI_BEACH);
    u16 border[] = {0x208, 0x208, 0x208, 0x208};
    u16 mapData[] = {0x208};
    u16 bg1[0x400] = {0};
    u16 bg2[0x400] = {0};
    u16 bg3[0x400] = {0};
    u16 *savedBg1 = gOverworldTilemapBuffer_Bg1;
    u16 *savedBg2 = gOverworldTilemapBuffer_Bg2;
    u16 *savedBg3 = gOverworldTilemapBuffer_Bg3;

    layout.border = border;
    gMapHeader.mapLayout = &layout;
    gBackupMapLayout.width = 1;
    gBackupMapLayout.height = 1;
    gBackupMapLayout.map = mapData;
    EXPECT_EQ(MapGridGetMetatileIdAt(-1, -1), 0);

    gOverworldTilemapBuffer_Bg1 = bg1;
    gOverworldTilemapBuffer_Bg2 = bg2;
    gOverworldTilemapBuffer_Bg3 = bg3;
    CurrentMapDrawMetatileAt(gSaveBlock1Ptr->pos.x, gSaveBlock1Ptr->pos.y);
    EXPECT_EQ(MapGridGetMetatileIdAt(0, 0), 0);

    gOverworldTilemapBuffer_Bg1 = savedBg1;
    gOverworldTilemapBuffer_Bg2 = savedBg2;
    gOverworldTilemapBuffer_Bg3 = savedBg3;
    gBackupMapLayout = savedBackup;
    gMapHeader.mapLayout = savedLayout;
}

TEST("Kanto Later General Cut passes avoid Fortree root writes")
{
    const struct MapLayout *savedLayout = gMapHeader.mapLayout;
    struct BackupMapLayout savedBackup = gBackupMapLayout;
    u16 mapData[25];
    u32 i;

    gMapHeader.mapLayout = GetMapLayout(LAYOUT_KANTO_LATER_SAFARI_BRUSH);
    gBackupMapLayout.width = 5;
    gBackupMapLayout.height = 5;
    gBackupMapLayout.map = mapData;

    for (i = 0; i < ARRAY_COUNT(mapData); i++)
        mapData[i] = METATILE_General_LongGrass;
    Test_RunCutGrassMetatilePass(1, 1, 3);
    EXPECT_EQ(mapData[1 + 1 * 5], METATILE_General_Grass);
    EXPECT_NE(mapData[1 + 4 * 5], 0x208);

    for (i = 0; i < ARRAY_COUNT(mapData); i++)
        mapData[i] = METATILE_General_LongGrass;
    Test_RunCutGrassMetatilePass(0, 0, 5);
    for (i = 0; i < ARRAY_COUNT(mapData); i++)
        EXPECT_EQ(mapData[i], METATILE_General_Grass);

    mapData[0] = 0x208;
    FixLongGrassMetatilesWindowTop(0, -1);
    FixLongGrassMetatilesWindowBottom(0, -1);
    EXPECT_EQ(mapData[0], 0x208);

    gBackupMapLayout = savedBackup;
    gMapHeader.mapLayout = savedLayout;
}

TEST("Kanto Later General uses the closed native animation callback")
{
    EXPECT(GetMapLayout(LAYOUT_KANTO_LATER_SAFARI_BEACH)->primaryTileset->callback == InitTilesetAnim_General);
    EXPECT(GetMapLayout(LAYOUT_KANTO_LATER_SAFARI_BRUSH)->primaryTileset->callback == InitTilesetAnim_General);
    EXPECT(GetMapLayout(LAYOUT_KANTO_LATER_SAFARI_MOUNTAIN)->primaryTileset->callback == InitTilesetAnim_General);
}

TEST("Kanto Later General preserves native animation frames and cadence")
{
    const struct MapLayout emptyLayout = {0};
    const struct MapLayout *savedLayout = gMapHeader.mapLayout;
    u16 savedDispcnt = REG_DISPCNT;

    gMapHeader.mapLayout = GetMapLayout(LAYOUT_KANTO_LATER_SAFARI_BEACH);
    REG_DISPCNT |= DISPCNT_FORCED_BLANK;
    CpuFill16(0xA55A, (u16 *)BG_VRAM, VRAM_SIZE);
    InitTilesetAnimations();

    RunAnimationFrames(1);
    ExpectAnimationCopy(432, sGeneralWaterFrame0, 30 * TILE_SIZE_4BPP);
    RunAnimationFrames(1);
    ExpectAnimationCopy(464, sGeneralSandWaterEdgeFrame0, 10 * TILE_SIZE_4BPP);
    RunAnimationFrames(1);
    ExpectAnimationCopy(496, sGeneralWaterfallFrame0, 6 * TILE_SIZE_4BPP);
    RunAnimationFrames(1);
    ExpectAnimationCopy(480, sGeneralLandWaterEdgeFrame0, 10 * TILE_SIZE_4BPP);
    RunAnimationFrames(12);
    ExpectAnimationCopy(508, sGeneralFlowerFrame1, 4 * TILE_SIZE_4BPP);
    EXPECT_EQ(*(const u16 *)(BG_VRAM + TILE_OFFSET_4BPP(0)), 0xA55A);
    EXPECT_EQ(*(const u16 *)(BG_VRAM + TILE_OFFSET_4BPP(1023)), 0xA55A);

    gMapHeader.mapLayout = savedLayout != NULL ? savedLayout : &emptyLayout;
    InitTilesetAnimations();
    gMapHeader.mapLayout = savedLayout;
    REG_DISPCNT = savedDispcnt;
}
