#include "global.h"
#include "event_data.h"
#include "heal_location.h"
#include "load_save.h"
#include "overworld.h"
#include "cormoria/heal_locations.h"
#include "constants/cormoria_event_ids.h"
#include "test/overworld_script.h"
#include "test/test.h"

static void ExpectSavedHealLocation(u32 index)
{
    const struct HealLocation *expected = GetHealLocation(index);
    struct WarpData *actual = &gSaveBlock1Ptr->lastHealLocation;
    EXPECT(expected != NULL);
    EXPECT_EQ(actual->mapGroup, expected->mapGroup);
    EXPECT_EQ(actual->mapNum, expected->mapNum);
    EXPECT_EQ(actual->warpId, WARP_ID_NONE);
    EXPECT_EQ(actual->x, expected->x);
    EXPECT_EQ(actual->y, expected->y);
    EXPECT_EQ(GetHealLocationIndexByWarpData(actual), index);
}

TEST("Cormoria setrespawn preserves every native host heal ID")
{
    u32 i, index = 1;
    for (i = 1; i < NUM_HEAL_LOCATIONS; i++)
        PARAMETRIZE { index = i; }
    SetSaveBlocksPointers(0);
    gSpecialVar_0x8004 = index;
    RUN_OVERWORLD_SCRIPT(setrespawn VAR_0x8004;);
    ExpectSavedHealLocation(index);
}

TEST("Cormoria setrespawn rejects invalid full-width IDs without truncation")
{
    static const u16 invalidIds[] = {
        HEAL_LOCATION_NONE, 0x02FF, 0x0311, 0xF003, 0x4003, 0xFFFF,
#if ROM_WORLD != 2
        Cormoria_HEAL_LOCATION_CARABRUE_TOWN,
#endif
    };
    u32 i, fixture = 0;
    struct WarpData before;
    for (i = 0; i < ARRAY_COUNT(invalidIds); i++)
        PARAMETRIZE { fixture = i; }
    SetSaveBlocksPointers(0);
    SetLastHealLocationWarp(1);
    before = gSaveBlock1Ptr->lastHealLocation;
    // Pass the value through the script variable. Direct operands above
    // VARS_START would exercise VarGet's address lookup instead of this ID.
    gSpecialVar_0x8004 = invalidIds[fixture];
    RUN_OVERWORLD_SCRIPT(setrespawn VAR_0x8004;);
    EXPECT_EQ(memcmp(&before, &gSaveBlock1Ptr->lastHealLocation, sizeof(before)), 0);
}

#if ROM_WORLD == 2
TEST("Cormoria setrespawn maps all qualified ledger IDs to native heal locations")
{
    // Ledger order is alphabetical; native table order follows the campaign.
    // Keep each named pairing explicit rather than calculating an offset.
    static const struct { u16 scriptId; u8 nativeId; } cases[] = {
        {Cormoria_HEAL_LOCATION_ANCIENT_CERAM, HEAL_LOCATION_CORMORIA_ANCIENT_CERAM},
        {Cormoria_HEAL_LOCATION_ANCIENT_CORMORIA_FINAL_ISLAND, HEAL_LOCATION_CORMORIA_ANCIENT_CORMORIA_FINAL_ISLAND},
        {Cormoria_HEAL_LOCATION_ANCIENT_MIRROH, HEAL_LOCATION_CORMORIA_ANCIENT_MIRROH},
        {Cormoria_HEAL_LOCATION_CARABRUE_TOWN, HEAL_LOCATION_CORMORIA_CARABRUE_TOWN},
        {Cormoria_HEAL_LOCATION_CERAM_BASE_CAMP, HEAL_LOCATION_CORMORIA_CERAM_BASE_CAMP},
        {Cormoria_HEAL_LOCATION_CHAMPIONSHIP_CORRIDOR, HEAL_LOCATION_CORMORIA_CHAMPIONSHIP_CORRIDOR},
        {Cormoria_HEAL_LOCATION_FENNILAHL_TOWN, HEAL_LOCATION_CORMORIA_FENNILAHL_TOWN},
        {Cormoria_HEAL_LOCATION_GALECREST_CITY, HEAL_LOCATION_CORMORIA_GALECREST_CITY},
        {Cormoria_HEAL_LOCATION_GASTREE_CITY, HEAL_LOCATION_CORMORIA_GASTREE_CITY},
        {Cormoria_HEAL_LOCATION_MIRROH_BASE_CAMP, HEAL_LOCATION_CORMORIA_MIRROH_BASE_CAMP},
        {Cormoria_HEAL_LOCATION_PELLUCA_CITY, HEAL_LOCATION_CORMORIA_PELLUCA_CITY},
        {Cormoria_HEAL_LOCATION_RIVETSHORE_CITY, HEAL_LOCATION_CORMORIA_RIVETSHORE_CITY},
        {Cormoria_HEAL_LOCATION_SILVERSUN_CITY, HEAL_LOCATION_CORMORIA_SILVERSUN_CITY},
        {Cormoria_HEAL_LOCATION_SSELEGANT, HEAL_LOCATION_CORMORIA_SSELEGANT},
        {Cormoria_HEAL_LOCATION_UNCHARTED_ISLAND, HEAL_LOCATION_CORMORIA_UNCHARTED_ISLAND},
        {Cormoria_HEAL_LOCATION_VICTORY_CAPE, HEAL_LOCATION_CORMORIA_VICTORY_CAPE},
        {Cormoria_HEAL_LOCATION_WINTERLILY_HOLLOW, HEAL_LOCATION_CORMORIA_WINTERLILY_HOLLOW},
    };
    u32 i, fixture = 0;
    for (i = 0; i < ARRAY_COUNT(cases); i++)
        PARAMETRIZE { fixture = i; }
    SetSaveBlocksPointers(0);
    SetLastHealLocationWarp(1);
    gSpecialVar_0x8004 = cases[fixture].scriptId;
    RUN_OVERWORLD_SCRIPT(setrespawn VAR_0x8004;);
    ExpectSavedHealLocation(cases[fixture].nativeId);
}
#endif
