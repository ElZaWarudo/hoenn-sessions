#include "global.h"
#include "constants/metatile_behaviors.h"
#include "metatile_behavior.h"
#include "test/test.h"

TEST("Johto metatile reservations retain the host tail and exact values")
{
    EXPECT_EQ(MB_ROCK_CLIMB, 0xEF);
    EXPECT_EQ(MB_JOHTO_HEADBUTT_TREE, 0xF0);
    EXPECT_EQ(MB_JOHTO_WATER_NORTH_ARROW_WARP, 0xF1);
    EXPECT_EQ(MB_JOHTO_INERT, 0xF2);
    EXPECT_EQ(MB_JOHTO_DEOXYS_ATTACK, 0xF3);
    EXPECT_EQ(NUM_METATILE_BEHAVIORS, 0xF4);
}

TEST("Johto reserved metatiles are bounded and have isolated behavior")
{
    u16 value;

    EXPECT(MetatileBehavior_IsEncounterTile(MB_TALL_GRASS));
    EXPECT(MetatileBehavior_IsSurfableWaterOrUnderwater(MB_OCEAN_WATER));
    EXPECT(MetatileBehavior_IsNorthArrowWarp(MB_NORTH_ARROW_WARP));
    EXPECT(MetatileBehavior_IsNorthArrowWarp(MB_STAIRS_OUTSIDE_ABANDONED_SHIP));
    EXPECT(MetatileBehavior_IsSurfableWaterOrUnderwater(MB_JOHTO_WATER_NORTH_ARROW_WARP));
    EXPECT(!MetatileBehavior_IsEncounterTile(MB_JOHTO_HEADBUTT_TREE));
    EXPECT(!MetatileBehavior_IsEncounterTile(MB_JOHTO_WATER_NORTH_ARROW_WARP));
    EXPECT(!MetatileBehavior_IsEncounterTile(MB_JOHTO_INERT));
    EXPECT(!MetatileBehavior_IsEncounterTile(MB_JOHTO_DEOXYS_ATTACK));
    EXPECT(!MetatileBehavior_IsSurfableWaterOrUnderwater(MB_JOHTO_HEADBUTT_TREE));
    EXPECT(!MetatileBehavior_IsSurfableWaterOrUnderwater(MB_JOHTO_INERT));
    EXPECT(!MetatileBehavior_IsSurfableWaterOrUnderwater(MB_JOHTO_DEOXYS_ATTACK));
    EXPECT(MetatileBehavior_IsNorthArrowWarp(MB_JOHTO_WATER_NORTH_ARROW_WARP));
    EXPECT(!MetatileBehavior_IsNorthArrowWarp(MB_JOHTO_HEADBUTT_TREE));
    EXPECT(!MetatileBehavior_IsNorthArrowWarp(MB_JOHTO_INERT));
    EXPECT(!MetatileBehavior_IsNorthArrowWarp(MB_JOHTO_DEOXYS_ATTACK));

    // Every byte outside the expanded table is safe input to the public
    // indexed predicates; none may read past sTileBitAttributes.
    for (value = NUM_METATILE_BEHAVIORS; value <= 0xFF; value++)
    {
        EXPECT(!MetatileBehavior_IsEncounterTile((u8)value));
        EXPECT(!MetatileBehavior_IsSurfableWaterOrUnderwater((u8)value));
    }
}

TEST("Existing north arrow and water semantics remain unchanged")
{
    EXPECT(MetatileBehavior_IsNorthArrowWarp(MB_NORTH_ARROW_WARP));
    EXPECT(MetatileBehavior_IsNorthArrowWarp(MB_STAIRS_OUTSIDE_ABANDONED_SHIP));
    EXPECT(!MetatileBehavior_IsNorthArrowWarp(MB_SOUTH_ARROW_WARP));
    EXPECT(MetatileBehavior_IsSurfableWaterOrUnderwater(MB_WATER_SOUTH_ARROW_WARP));
    EXPECT(!MetatileBehavior_IsSurfableWaterOrUnderwater(MB_SOUTH_ARROW_WARP));
    EXPECT(MetatileBehavior_IsEncounterTile(MB_TALL_GRASS));
    EXPECT(!MetatileBehavior_IsEncounterTile(MB_SHALLOW_WATER));
}
