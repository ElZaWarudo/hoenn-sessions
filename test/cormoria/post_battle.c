#include "global.h"
#include "event_data.h"
#include "heal_location.h"
#include "load_save.h"
#include "overworld.h"
#include "save_location.h"
#include "cormoria/heal_locations.h"
#include "constants/maps.h"
#include "test/test.h"

int GameClear(void);

#if ROM_WORLD == 2

TEST("Cormoria GameClear stores the Carabrue Town continue warp")
{
    const struct HealLocation *carabrue = GetHealLocation(HEAL_LOCATION_CORMORIA_CARABRUE_TOWN);

    EXPECT(carabrue != NULL);
    EXPECT_EQ(FlagGet(FLAG_SYS_GAME_CLEAR), FALSE);

    EXPECT_EQ(GameClear(), 0);

    EXPECT_EQ(FlagGet(FLAG_SYS_GAME_CLEAR), TRUE);
    EXPECT_EQ(UseContinueGameWarp(), CONTINUE_GAME_WARP);
    EXPECT_EQ(gSaveBlock1Ptr->continueGameWarp.mapGroup, carabrue->mapGroup);
    EXPECT_EQ(gSaveBlock1Ptr->continueGameWarp.mapNum, carabrue->mapNum);
    EXPECT_EQ(gSaveBlock1Ptr->continueGameWarp.warpId, WARP_ID_NONE);
    EXPECT_EQ(gSaveBlock1Ptr->continueGameWarp.x, carabrue->x);
    EXPECT_EQ(gSaveBlock1Ptr->continueGameWarp.y, carabrue->y);
}

#else

TEST("Main GameClear keeps the Brendan house continue warp")
{
    const struct HealLocation *brendanHouse = GetHealLocation(HEAL_LOCATION_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F);

    EXPECT(brendanHouse != NULL);
    gSaveBlock2Ptr->playerGender = MALE;

    EXPECT_EQ(GameClear(), 0);
    EXPECT_EQ(gSaveBlock1Ptr->continueGameWarp.mapGroup, brendanHouse->mapGroup);
    EXPECT_EQ(gSaveBlock1Ptr->continueGameWarp.mapNum, brendanHouse->mapNum);
    EXPECT_EQ(gSaveBlock1Ptr->continueGameWarp.warpId, WARP_ID_NONE);
    EXPECT_EQ(gSaveBlock1Ptr->continueGameWarp.x, brendanHouse->x);
    EXPECT_EQ(gSaveBlock1Ptr->continueGameWarp.y, brendanHouse->y);
}

#endif
