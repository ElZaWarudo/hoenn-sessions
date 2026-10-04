#include "global.h"
#include "overworld.h"
#include "constants/map_types.h"
#include "test/test.h"

TEST("Cormoria snow and hill retain their donor map type IDs")
{
    EXPECT_EQ(MAP_TYPE_SNOW, 10);
    EXPECT_EQ(MAP_TYPE_HILL, 11);
}

TEST("Cormoria snow and hill behave as outdoor flyable maps")
{
    EXPECT(IsMapTypeOutdoors(MAP_TYPE_SNOW));
    EXPECT(IsMapTypeOutdoors(MAP_TYPE_HILL));
    EXPECT(Overworld_MapTypeAllowsTeleportAndFly(MAP_TYPE_SNOW));
    EXPECT(Overworld_MapTypeAllowsTeleportAndFly(MAP_TYPE_HILL));
    EXPECT(!IsMapTypeIndoors(MAP_TYPE_SNOW));
    EXPECT(!IsMapTypeIndoors(MAP_TYPE_HILL));
    EXPECT(MapHasNaturalLight(MAP_TYPE_SNOW));
    EXPECT(MapHasNaturalLight(MAP_TYPE_HILL));
}

TEST("Existing map types keep their travel classification")
{
    EXPECT(IsMapTypeOutdoors(MAP_TYPE_ROUTE));
    EXPECT(Overworld_MapTypeAllowsTeleportAndFly(MAP_TYPE_ROUTE));
    EXPECT(IsMapTypeIndoors(MAP_TYPE_INDOOR));
    EXPECT(!Overworld_MapTypeAllowsTeleportAndFly(MAP_TYPE_INDOOR));
    EXPECT(IsMapTypeOutdoors(MAP_TYPE_UNDERWATER));
    EXPECT(!Overworld_MapTypeAllowsTeleportAndFly(MAP_TYPE_UNDERWATER));
}
