#include "global.h"
#include "fieldmap.h"
#include "overworld.h"
#include "tileset_anims.h"
#include "test/test.h"

// Dimensions and first block values checked against the pinned donor files.
TEST("Johto scenery registers outdoor indoor dungeon and final layouts")
{
    static const struct { u16 id, width, height, firstBlock; } cases[] = {
        {786, 30, 39, 0x414},
        {839, 25, 10, 0x4D8},
        {935, 78, 50, 0x691},
        {1024, 30, 32, 0x414},
    };
    u32 i;
    for (i = 0; i < ARRAY_COUNT(cases); i++)
    {
        const struct MapLayout *layout = GetMapLayout(cases[i].id);
        EXPECT(layout != NULL);
        EXPECT_EQ((u32)layout->isFrlg, TRUE);
        EXPECT_EQ(layout->width, cases[i].width);
        EXPECT_EQ(layout->height, cases[i].height);
        EXPECT_EQ(layout->borderWidth, 2);
        EXPECT_EQ(layout->borderHeight, 2);
        EXPECT_EQ(layout->map[0], cases[i].firstBlock);
        EXPECT(layout->border != NULL);
        EXPECT(layout->primaryTileset != layout->secondaryTileset);
        EXPECT(layout->primaryTileset->tiles != NULL && layout->secondaryTileset->tiles != NULL);
        EXPECT_EQ((u32)layout->primaryTileset->isSecondary, FALSE);
        EXPECT_EQ((u32)layout->secondaryTileset->isSecondary, TRUE);
    }
    EXPECT(GetMapLayout(786)->primaryTileset->callback == InitTilesetAnim_JohtoGeneral);
    EXPECT(GetMapLayout(786)->secondaryTileset->callback == NULL);
    EXPECT(GetMapLayout(839)->primaryTileset->callback == NULL);
    EXPECT(GetMapLayout(935)->primaryTileset->callback == InitTilesetAnim_JohtoGeneral);
    EXPECT(GetMapLayout(1031)->secondaryTileset->callback == InitTilesetAnim_CeladonCity);
    EXPECT(GetMapLayout(1034)->secondaryTileset->callback == InitTilesetAnim_JohtoBlackthornGym);
    EXPECT(GetMapLayout(1124)->secondaryTileset->callback == InitTilesetAnim_SilphCo);
}

TEST("Johto scenery converts source attributes for the public FRLG accessor")
{
    const struct MapLayout *saved = gMapHeader.mapLayout;
    gMapHeader.mapLayout = GetMapLayout(786);
    // Donor General tile 5 is Headbutt; 11 is tall grass; 59 is pond water.
    EXPECT_EQ(GetAttributeByMetatileIdAndMapLayout(5, METATILE_ATTRIBUTE_BEHAVIOR, TRUE), 0xF0);
    EXPECT_EQ(GetAttributeByMetatileIdAndMapLayout(11, METATILE_ATTRIBUTE_BEHAVIOR, TRUE), 2);
    EXPECT_EQ(GetAttributeByMetatileIdAndMapLayout(59, METATILE_ATTRIBUTE_BEHAVIOR, TRUE), 0x10);
    gMapHeader.mapLayout = saved;
}
