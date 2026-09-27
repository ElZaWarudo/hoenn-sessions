#include "global.h"
#include "event_data.h"
#include "load_save.h"
#include "overworld.h"
#include "constants/maps.h"
#include "test/test.h"

void CableCarWarp(void);

static struct WarpData RunCableCarWarp(u16 direction)
{
    SetSaveBlocksPointers(0);
    gSpecialVar_0x8004 = direction;
    CableCarWarp();
    ApplyCurrentWarp();
    return gSaveBlock1Ptr->location;
}

#if ROM_WORLD == 2

TEST("Cormoria CableCarWarp preserves donor direction and station coordinates")
{
    struct WarpData warp;

    warp = RunCableCarWarp(1);
    EXPECT_EQ(warp.mapGroup, MAP_GROUP(MAP_CORMORIA_PELLUCA_CABLE_CAR_STATION));
    EXPECT_EQ(warp.mapNum, MAP_NUM(MAP_CORMORIA_PELLUCA_CABLE_CAR_STATION));
    EXPECT_EQ(warp.warpId, WARP_ID_NONE);
    EXPECT_EQ(warp.x, 6);
    EXPECT_EQ(warp.y, 4);

    warp = RunCableCarWarp(0);
    EXPECT_EQ(warp.mapGroup, MAP_GROUP(MAP_CORMORIA_MIRROH_BASE_CAMP_CABLE_CAR_STATION));
    EXPECT_EQ(warp.mapNum, MAP_NUM(MAP_CORMORIA_MIRROH_BASE_CAMP_CABLE_CAR_STATION));
    EXPECT_EQ(warp.warpId, WARP_ID_NONE);
    EXPECT_EQ(warp.x, 6);
    EXPECT_EQ(warp.y, 4);
}

#else

TEST("Main CableCarWarp retains Hoenn station destinations")
{
    struct WarpData warp;

    warp = RunCableCarWarp(1);
    EXPECT_EQ(warp.mapGroup, MAP_GROUP(MAP_ROUTE112_CABLE_CAR_STATION));
    EXPECT_EQ(warp.mapNum, MAP_NUM(MAP_ROUTE112_CABLE_CAR_STATION));
    EXPECT_EQ(warp.warpId, WARP_ID_NONE);
    EXPECT_EQ(warp.x, 6);
    EXPECT_EQ(warp.y, 4);

    warp = RunCableCarWarp(0);
    EXPECT_EQ(warp.mapGroup, MAP_GROUP(MAP_MT_CHIMNEY_CABLE_CAR_STATION));
    EXPECT_EQ(warp.mapNum, MAP_NUM(MAP_MT_CHIMNEY_CABLE_CAR_STATION));
    EXPECT_EQ(warp.warpId, WARP_ID_NONE);
    EXPECT_EQ(warp.x, 6);
    EXPECT_EQ(warp.y, 4);
}

#endif
