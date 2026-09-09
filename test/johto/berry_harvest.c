#include "global.h"
#include "berry.h"
#include "event_data.h"
#include "event_object_movement.h"
#include "item.h"
#include "load_save.h"
#include "overworld.h"
#include "script.h"
#include "sprite.h"
#include "string_util.h"
#include "johto/berry_plots.h"
#include "constants/berry.h"
#include "constants/johto_berry_plots.h"
#include "constants/event_object_movement.h"
#include "constants/game_stat.h"
#include "constants/maps.h"
#include "constants/vars.h"
#include "test/test.h"

static EWRAM_DATA struct BerryTree sBefore[BERRY_TREES_COUNT];
static const u8 sThree[] = _("3");

static void ResetHarvest(void)
{
    SetSaveBlocksPointers(0);
    SetBagItemsPointers();
    ClearBag();
    InitEventData();
    ResetGameStats();
    ClearBerryTrees();
    gSaveBlock1Ptr->location.mapGroup = MAP_GROUP(MAP_LITTLEROOT_TOWN);
    gSaveBlock1Ptr->location.mapNum = MAP_NUM(MAP_LITTLEROOT_TOWN);
    JohtoBerryPlots_InitializeNewGame();
    PlantBerryTree(0, BERRY_ID_ORAN, BERRY_STAGE_BERRIES, FALSE);
    GetBerryTreeInfo(90)->berryYield = 3;
    memcpy(sBefore, gSaveBlock1Ptr->berryTrees, sizeof(sBefore));
}

static void ExpectStats(u32 picked, u32 planted)
{
    EXPECT_EQ(VarGet(VAR_DAILY_PICKED_BERRIES), picked);
    EXPECT_EQ(VarGet(VAR_DAILY_PLANTED_BERRIES), planted);
    EXPECT_EQ(GetGameStat(GAME_STAT_PLANTED_BERRIES), planted);
}

TEST("Johto harvesting awards exact berries and regrows independently for another harvest")
{
    u32 i, secondYield;
    ResetHarvest();
    EXPECT(AddBagItem(ITEM_CHERI_BERRY, 2));
    EXPECT_EQ(JohtoBerryPlots_TryHarvest(90), JOHTO_BERRY_HARVEST_SUCCESS);
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_CHERI_BERRY), 5);
    EXPECT_EQ(GetStageByBerryTreeId(90), BERRY_STAGE_SPROUTED);
    EXPECT_EQ(GetBerryTypeByBerryTreeId(90), BERRY_ID_CHERI);
    EXPECT_EQ((u32)GetBerryTreeInfo(90)->stopGrowth, FALSE);
    ExpectStats(3, 1);
    EXPECT_EQ(JohtoBerryPlots_TryHarvest(90), JOHTO_BERRY_HARVEST_UNAVAILABLE);
    ExpectStats(3, 1);
    for (i = 0; i < 3; i++)
        BerryTreeTimeUpdate(GetBerryTreeInfo(90)->minutesUntilNextStage);
    EXPECT_EQ(GetStageByBerryTreeId(90), BERRY_STAGE_BERRIES);
    secondYield = GetBerryTreeInfo(90)->berryYield;
    EXPECT(secondYield > 0);
    EXPECT_EQ(JohtoBerryPlots_TryHarvest(90), JOHTO_BERRY_HARVEST_SUCCESS);
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_CHERI_BERRY), 5 + secondYield);
    ExpectStats(3 + secondYield, 2);
    for (i = 0; i < BERRY_TREES_COUNT; i++)
        if (i != 90)
            EXPECT_EQ(memcmp(&sBefore[i], GetBerryTreeInfo(i), sizeof(sBefore[i])), 0);
}

TEST("Full berry pocket and unavailable Johto plots leave tree bag and statistics unchanged")
{
    u32 i;
    struct BagPocket *pocket;
    ResetHarvest();
    pocket = &gBagPockets[POCKET_BERRIES];
    for (i = 0; i < pocket->capacity; i++)
        BagPocket_SetSlotItemIdAndCount(pocket, i, ITEM_ORAN_BERRY, MAX_BAG_ITEM_CAPACITY);
    EXPECT_EQ(JohtoBerryPlots_TryHarvest(90), JOHTO_BERRY_HARVEST_BAG_FULL);
    EXPECT_EQ(memcmp(sBefore, gSaveBlock1Ptr->berryTrees, sizeof(sBefore)), 0);
    for (i = 0; i < pocket->capacity; i++)
    {
        struct ItemSlot slot = BagPocket_GetSlotData(pocket, i);
        EXPECT_EQ(slot.itemId, ITEM_ORAN_BERRY);
        EXPECT_EQ(slot.quantity, MAX_BAG_ITEM_CAPACITY);
    }
    EXPECT_EQ(JohtoBerryPlots_TryHarvest(89), JOHTO_BERRY_HARVEST_UNAVAILABLE);
    EXPECT_EQ(JohtoBerryPlots_TryHarvest(110), JOHTO_BERRY_HARVEST_UNAVAILABLE);
    EXPECT_EQ(JohtoBerryPlots_TryHarvest(255), JOHTO_BERRY_HARVEST_UNAVAILABLE);
    GetBerryTreeInfo(90)->berryYield = 0;
    EXPECT_EQ(JohtoBerryPlots_TryHarvest(90), JOHTO_BERRY_HARVEST_UNAVAILABLE);
    GetBerryTreeInfo(90)->berry = BERRY_ID_NONE;
    EXPECT_EQ(JohtoBerryPlots_TryHarvest(90), JOHTO_BERRY_HARVEST_UNAVAILABLE);
    ExpectStats(0, 0);
}

TEST("Johto harvest native validates selected identity and captures original dialogue values")
{
    struct ObjectEvent objectBefore;
    struct Sprite spriteBefore;
    u8 selectedBefore = gSelectedObjectEvent;
    ResetHarvest();
    objectBefore = gObjectEvents[0];
    spriteBefore = gSprites[0];
    gSelectedObjectEvent = OBJECT_EVENTS_COUNT;
    Script_JohtoHarvestBerryTree(NULL);
    EXPECT_EQ(gSpecialVar_Result, JOHTO_BERRY_HARVEST_UNAVAILABLE);
    gSelectedObjectEvent = 0;
    memset(&gObjectEvents[0], 0, sizeof(gObjectEvents[0]));
    Script_JohtoHarvestBerryTree(NULL);
    EXPECT_EQ(gSpecialVar_Result, JOHTO_BERRY_HARVEST_UNAVAILABLE);
    gObjectEvents[0].active = TRUE;
    gObjectEvents[0].trainerRange_berryTreeId = 90;
    Script_JohtoHarvestBerryTree(NULL);
    EXPECT_EQ(gSpecialVar_Result, JOHTO_BERRY_HARVEST_UNAVAILABLE);
    gObjectEvents[0].movementType = MOVEMENT_TYPE_BERRY_TREE_GROWTH;
    gObjectEvents[0].localId = 77;
    gObjectEvents[0].mapNum = 99;
    gObjectEvents[0].mapGroup = 99;
    gSprites[0].data[7] = 0;
    Script_JohtoHarvestBerryTree(NULL);
    EXPECT_EQ(gSpecialVar_Result, JOHTO_BERRY_HARVEST_SUCCESS);
    EXPECT_EQ(gSpecialVar_0x8006, 3);
    EXPECT_EQ(StringCompare(gStringVar2, sThree), 0);
    EXPECT(gSprites[0].data[7] & (1 << 2)); // Production just-picked flag.
    EXPECT_EQ(GetStageByBerryTreeId(90), BERRY_STAGE_SPROUTED);
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_CHERI_BERRY), 3);
    gObjectEvents[0] = objectBefore;
    gSprites[0] = spriteBefore;
    gSelectedObjectEvent = selectedBefore;
}

TEST("Johto harvest native declares save effects before any transaction")
{
    u8 program[6];
    u32 i, pointer = (uintptr_t)Script_JohtoHarvestBerryTree | 0x0A000000;
    ResetHarvest();
    program[0] = 0x23;
    for (i = 0; i < 4; i++)
        program[i + 1] = pointer >> (i * 8);
    program[5] = 0x02;
    EXPECT(RunScriptImmediatelyUntilEffect(SCREFF_V1 | SCREFF_SAVE, program, NULL));
    EXPECT_EQ(memcmp(sBefore, gSaveBlock1Ptr->berryTrees, sizeof(sBefore)), 0);
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_CHERI_BERRY), 0);
    ExpectStats(0, 0);
}
