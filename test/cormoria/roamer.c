#include "global.h"
#include "event_data.h"
#include "load_save.h"
#include "roamer.h"
#include "save.h"
#include "test/test.h"

#if ROM_WORLD == 2

static bool8 IsCormoriaRoamerRoute(u8 mapGroup, u8 mapNum)
{
    return (mapGroup == MAP_GROUP(MAP_CORMORIA_ROUTE4) && mapNum == MAP_NUM(MAP_CORMORIA_ROUTE4))
        || (mapGroup == MAP_GROUP(MAP_CORMORIA_ROUTE5) && mapNum == MAP_NUM(MAP_CORMORIA_ROUTE5))
        || (mapGroup == MAP_GROUP(MAP_CORMORIA_ROUTE6) && mapNum == MAP_NUM(MAP_CORMORIA_ROUTE6));
}

TEST("Cormoria initializes Zeraora at level 40 and keeps it on the route graph")
{
    struct Roamer *roamer;
    u8 mapGroup;
    u8 mapNum;
    u32 i;

    SetSaveBlocksPointers(0);
    memset(&gSaveBlock1Ptr->roamer[0], 0, sizeof(gSaveBlock1Ptr->roamer[0]));
    InitRoamer();
    roamer = &gSaveBlock1Ptr->roamer[0];

    EXPECT_EQ(roamer->species, SPECIES_ZERAORA);
    EXPECT_EQ(roamer->level, 40);
    EXPECT_EQ(roamer->active, TRUE);
    GetRoamerLocation(0, &mapGroup, &mapNum);
    EXPECT(IsCormoriaRoamerRoute(mapGroup, mapNum));

    /* Rehydrate the EWRAM location from the saved record, as a cold reload
     * would, and make sure the map-group half of the identity is retained. */
    roamer->filler[0] = 0xC2;
    roamer->filler[1] = ROM_WORLD_ID & 0xFF;
    roamer->filler[2] = ROM_WORLD_ID >> 8;
    roamer->filler[3] = MAP_GROUP(MAP_CORMORIA_ROUTE5);
    roamer->filler[4] = MAP_NUM(MAP_CORMORIA_ROUTE5);
    GetRoamerLocation(0, &mapGroup, &mapNum);
    EXPECT_EQ(mapGroup, MAP_GROUP(MAP_CORMORIA_ROUTE5));
    EXPECT_EQ(mapNum, MAP_NUM(MAP_CORMORIA_ROUTE5));

    /* A location tagged for another ROM must be replaced with this world's
     * route graph, including when only the high byte of its ID differs. */
    roamer->filler[2] ^= 1;
    GetRoamerLocation(0, &mapGroup, &mapNum);
    EXPECT_EQ(roamer->filler[2], ROM_WORLD_ID >> 8);
    EXPECT(IsCormoriaRoamerRoute(mapGroup, mapNum));

    for (i = 0; i < 128; i++)
    {
        RoamerMove(0);
        GetRoamerLocation(0, &mapGroup, &mapNum);
        EXPECT(IsCormoriaRoamerRoute(mapGroup, mapNum));
    }
}

#else

TEST("Main roamer keeps its Latias and Latios choices")
{
    SetSaveBlocksPointers(0);
    memset(&gSaveBlock1Ptr->roamer[0], 0, sizeof(gSaveBlock1Ptr->roamer[0]));
    gSpecialVar_0x8004 = 0;
    InitRoamer();
    EXPECT_EQ(gSaveBlock1Ptr->roamer[0].species, SPECIES_LATIAS);

    memset(&gSaveBlock1Ptr->roamer[0], 0, sizeof(gSaveBlock1Ptr->roamer[0]));
    gSpecialVar_0x8004 = 1;
    InitRoamer();
    EXPECT_EQ(gSaveBlock1Ptr->roamer[0].species, SPECIES_LATIOS);
}

#endif
