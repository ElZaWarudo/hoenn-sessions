#include "global.h"
#include "berry.h"
#include "constants/berry.h"
#include "constants/johto_berry_plots.h"
#include "load_save.h"
#include "malloc.h"
#include "test/test.h"

static void ResetCrops(void)
{
    SetSaveBlocksPointers(0);
    ClearBerryTrees();
}

TEST("Johto ripe lifetime includes 90 through 124 but preserves host policy at 89 and 125")
{
    struct BerryTree *sBefore = Alloc(BERRY_TREES_COUNT * sizeof(*sBefore));
    u32 i;
    EXPECT(sBefore != NULL);
    ResetCrops();
    for (i = 89; i <= 125; i++)
        PlantBerryTree(i, BERRY_ID_ORAN, BERRY_STAGE_BERRIES, TRUE);
    memcpy(sBefore, gSaveBlock1Ptr->berryTrees, BERRY_TREES_COUNT * sizeof(*sBefore));
    BerryTreeTimeUpdate(0x7FFFFFFF);
    for (i = 0; i < BERRY_TREES_COUNT; i++)
    {
        if ((i == 89 || i == 125) && !OW_BERRY_IMMORTAL)
            EXPECT_EQ(GetStageByBerryTreeId(i), BERRY_STAGE_NO_BERRY);
        else
            EXPECT_EQ(memcmp(GetBerryTreeInfo(i), &sBefore[i], sizeof(sBefore[i])), 0);
    }
    Free(sBefore);
}

TEST("Johto sprouted crops mature after a huge absence without cycling or gardening")
{
    struct BerryTree *sBefore = Alloc(BERRY_TREES_COUNT * sizeof(*sBefore));
    u32 i;
    EXPECT(sBefore != NULL);
    ResetCrops();
    for (i = JOHTO_BERRY_PLOTS_FIRST; i <= JOHTO_BERRY_PLOTS_LAST; i++)
        PlantBerryTree(i, BERRY_ID_ORAN, BERRY_STAGE_SPROUTED, TRUE);
    PlantBerryTree(0, BERRY_ID_CHERI, BERRY_STAGE_SPROUTED, FALSE);
    memcpy(sBefore, gSaveBlock1Ptr->berryTrees, BERRY_TREES_COUNT * sizeof(*sBefore));
    BerryTreeTimeUpdate(0x7FFFFFFF);
    for (i = 0; i < BERRY_TREES_COUNT; i++)
    {
        const struct BerryTree *tree = GetBerryTreeInfo(i);
        if (i >= JOHTO_BERRY_PLOTS_FIRST && i <= JOHTO_BERRY_PLOTS_LAST)
        {
            EXPECT_EQ((u32)tree->stage, BERRY_STAGE_BERRIES);
            EXPECT_EQ((u32)tree->berry, BERRY_ID_ORAN);
            EXPECT(tree->berryYield >= GetBerryInfo(BERRY_ID_ORAN)->minYield);
            EXPECT(tree->berryYield <= GetBerryInfo(BERRY_ID_ORAN)->maxYield);
            EXPECT_EQ((u32)tree->regrowthCount, 0);
            EXPECT_EQ((u32)tree->moistureClock, (u32)sBefore[i].moistureClock);
            EXPECT_EQ((u32)tree->moistureLevel, (u32)sBefore[i].moistureLevel);
        }
        else
            EXPECT_EQ(memcmp(tree, &sBefore[i], sizeof(*tree)), 0);
    }
    memcpy(sBefore, gSaveBlock1Ptr->berryTrees, BERRY_TREES_COUNT * sizeof(*sBefore));
    BerryTreeTimeUpdate(0x7FFFFFFF);
    BerryTreeTimeUpdate(1);
    EXPECT_EQ(memcmp(gSaveBlock1Ptr->berryTrees, sBefore, BERRY_TREES_COUNT * sizeof(*sBefore)), 0);
    Free(sBefore);
}

TEST("Johto elapsed clock honors the exact next stage and ignores nonpositive time")
{
    struct BerryTree *sBefore = Alloc(BERRY_TREES_COUNT * sizeof(*sBefore));
    u16 duration;
    EXPECT(sBefore != NULL);
    ResetCrops();
    PlantBerryTree(90, BERRY_ID_CHERI, BERRY_STAGE_SPROUTED, TRUE);
    PlantBerryTree(124, BERRY_ID_ORAN, BERRY_STAGE_SPROUTED, FALSE);
    memcpy(sBefore, gSaveBlock1Ptr->berryTrees, BERRY_TREES_COUNT * sizeof(*sBefore));
    BerryTreeTimeUpdate(0);
    BerryTreeTimeUpdate(-1);
    BerryTreeTimeUpdate(-0x7FFFFFFF);
    EXPECT_EQ(memcmp(gSaveBlock1Ptr->berryTrees, sBefore, BERRY_TREES_COUNT * sizeof(*sBefore)), 0);
    duration = GetBerryTreeInfo(90)->minutesUntilNextStage;
    EXPECT(duration > 1);
    BerryTreeTimeUpdate(duration - 1);
    EXPECT_EQ(GetStageByBerryTreeId(90), BERRY_STAGE_SPROUTED);
    EXPECT_EQ((u32)GetBerryTreeInfo(90)->minutesUntilNextStage, 1);
    BerryTreeTimeUpdate(1);
    EXPECT_EQ(GetStageByBerryTreeId(90), BERRY_STAGE_TALLER);
    EXPECT_EQ(memcmp(GetBerryTreeInfo(124), &sBefore[124], sizeof(sBefore[124])), 0);
    Free(sBefore);
}

TEST("Host ripe trees still regrow at their timer while Johto stays ripe")
{
    struct BerryTree *sBefore = Alloc(BERRY_TREES_COUNT * sizeof(*sBefore));
    u16 duration;
    EXPECT(sBefore != NULL);
    ResetCrops();
    PlantBerryTree(89, BERRY_ID_ORAN, BERRY_STAGE_BERRIES, TRUE);
    PlantBerryTree(90, BERRY_ID_ORAN, BERRY_STAGE_BERRIES, TRUE);
    duration = GetBerryTreeInfo(89)->minutesUntilNextStage;
    memcpy(sBefore, gSaveBlock1Ptr->berryTrees, BERRY_TREES_COUNT * sizeof(*sBefore));
    BerryTreeTimeUpdate(duration);
    EXPECT_EQ(GetStageByBerryTreeId(89), OW_BERRY_IMMORTAL ? BERRY_STAGE_BERRIES : BERRY_STAGE_SPROUTED);
    EXPECT_EQ((u32)GetBerryTreeInfo(89)->regrowthCount, OW_BERRY_IMMORTAL ? 0 : 1);
    EXPECT_EQ(memcmp(GetBerryTreeInfo(90), &sBefore[90], sizeof(sBefore[90])), 0);
    Free(sBefore);
}
