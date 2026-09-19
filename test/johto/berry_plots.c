#include "global.h"
#include "berry.h"
#include "constants/berry.h"
#include "constants/johto_berry_plots.h"
#include "johto/berry_plots.h"
#include "load_save.h"
#include "test/test.h"

static EWRAM_DATA struct BerryTree sBefore[BERRY_TREES_COUNT];
static const enum BerryId sExpectedBerries[] =
{
    BERRY_ID_CHERI,
    BERRY_ID_PECHA,
    BERRY_ID_ORAN,
    BERRY_ID_ORAN,
    BERRY_ID_ORAN,
    BERRY_ID_CHERI,
    BERRY_ID_PECHA,
    BERRY_ID_PERSIM,
    BERRY_ID_PERSIM,
    BERRY_ID_CHESTO,
    BERRY_ID_RAWST,
    BERRY_ID_RAWST,
    BERRY_ID_CHESTO,
    BERRY_ID_LEPPA,
    BERRY_ID_LEPPA,
    BERRY_ID_ASPEAR,
    BERRY_ID_ASPEAR,
    BERRY_ID_LUM,
    BERRY_ID_SITRUS,
    BERRY_ID_SITRUS,
    BERRY_ID_HONDEW,
    BERRY_ID_QUALOT,
    BERRY_ID_SITRUS,
    BERRY_ID_POMEG,
    BERRY_ID_SITRUS,
    BERRY_ID_POMEG,
    BERRY_ID_TAMATO,
    BERRY_ID_GREPA,
    BERRY_ID_TAMATO,
    BERRY_ID_TAMATO,
    BERRY_ID_GREPA,
    BERRY_ID_QUALOT,
    BERRY_ID_HONDEW,
    BERRY_ID_KELPSY,
    BERRY_ID_KELPSY,
};

TEST("Johto new-game crops seed every independent dormant plot only")
{
    u32 i;
    SetSaveBlocksPointers(0);
    ClearBerryTrees();
    for (i = 0; i < BERRY_TREES_COUNT; i++)
        PlantBerryTree(i, BERRY_ID_ORAN, BERRY_STAGE_BERRIES, FALSE);
    memcpy(sBefore, gSaveBlock1Ptr->berryTrees, sizeof(sBefore));
    JohtoBerryPlots_InitializeNewGame();
    for (i = 0; i < BERRY_TREES_COUNT; i++)
    {
        const struct BerryTree *tree = GetBerryTreeInfo(i);
        if (i >= JOHTO_BERRY_PLOTS_FIRST && i <= JOHTO_BERRY_PLOTS_LAST)
        {
            EXPECT_EQ((u32)tree->berry, sExpectedBerries[i - JOHTO_BERRY_PLOTS_FIRST]);
            EXPECT_EQ((u32)tree->stage, BERRY_STAGE_BERRIES);
            EXPECT_EQ((u32)tree->stopGrowth, TRUE);
            EXPECT(tree->berryYield > 0);
        }
        else
            EXPECT_EQ(memcmp(tree, &sBefore[i], sizeof(*tree)), 0);
    }
}

TEST("A removed Johto crop replants and grows without touching other plots")
{
    u32 i;
    u16 duration;
    SetSaveBlocksPointers(0);
    ClearBerryTrees();
    PlantBerryTree(0, BERRY_ID_ORAN, BERRY_STAGE_BERRIES, FALSE);
    JohtoBerryPlots_InitializeNewGame();
    memcpy(sBefore, gSaveBlock1Ptr->berryTrees, sizeof(sBefore));
    RemoveBerryTree(JOHTO_BERRY_TREE_CHERI_2);
    EXPECT_EQ(GetStageByBerryTreeId(JOHTO_BERRY_TREE_CHERI_2), BERRY_STAGE_NO_BERRY);
    PlantBerryTree(JOHTO_BERRY_TREE_CHERI_2, BERRY_ID_ORAN, BERRY_STAGE_PLANTED, TRUE);
    EXPECT_EQ((u32)GetBerryTreeInfo(JOHTO_BERRY_TREE_CHERI_2)->stopGrowth, FALSE);
    duration = GetBerryTreeInfo(JOHTO_BERRY_TREE_CHERI_2)->minutesUntilNextStage;
    EXPECT(duration > 0);
    BerryTreeTimeUpdate(duration);
    EXPECT_EQ(GetStageByBerryTreeId(JOHTO_BERRY_TREE_CHERI_2), BERRY_STAGE_SPROUTED);
    EXPECT_EQ(GetBerryTypeByBerryTreeId(JOHTO_BERRY_TREE_CHERI_2), BERRY_ID_ORAN);
    for (i = 0; i < BERRY_TREES_COUNT; i++)
        if (i != JOHTO_BERRY_TREE_CHERI_2)
            EXPECT_EQ(memcmp(GetBerryTreeInfo(i), &sBefore[i], sizeof(sBefore[i])), 0);
}
