#include "global.h"
#include "berry.h"
#include "constants/berry.h"
#include "constants/johto_berry_plots.h"
#include "cormoria/berry_plots.h"
#include "load_save.h"
#include "malloc.h"
#include "test/test.h"

TEST("Cormoria new-game berries preserve all other regional plot slots")
{
    struct BerryTree *sBefore = Alloc(BERRY_TREES_COUNT * sizeof(*sBefore));
    u32 i;
    EXPECT(sBefore != NULL);

    SetSaveBlocksPointers(0);
    ClearBerryTrees();
    PlantBerryTree(JOHTO_BERRY_TREE_CHERI_2, BERRY_ID_PECHA, BERRY_STAGE_BERRIES, FALSE);
    PlantBerryTree(BERRY_TREE_ROUTE_102_ORAN, BERRY_ID_CHERI, BERRY_STAGE_BERRIES, FALSE);
    memcpy(sBefore, gSaveBlock1Ptr->berryTrees, BERRY_TREES_COUNT * sizeof(*sBefore));
    CormoriaBerryPlots_SeedNewGame();

#if ROM_WORLD == 2
    for (i = 0; i < BERRY_TREES_COUNT; i++)
    {
        const struct BerryTree *tree = GetBerryTreeInfo(i);
        if (i < CORMORIA_BERRY_PLOTS_FIRST || i > CORMORIA_BERRY_PLOTS_LAST
            || i == Cormoria_BERRY_TREE_VILETHORN_ORAN
            || (i >= Cormoria_BERRY_TREE_PELLUCA_C && i <= Cormoria_BERRY_TREE_RIVETSHORE_C))
            EXPECT_EQ(memcmp(tree, &sBefore[i], sizeof(*tree)), 0);
        else
        {
            EXPECT_EQ((u8)tree->stage, BERRY_STAGE_BERRIES);
            EXPECT_EQ((u8)tree->stopGrowth, TRUE);
            EXPECT(tree->berryYield > 0);
        }
    }
    EXPECT_EQ(GetBerryTypeByBerryTreeId(Cormoria_BERRY_TREE_FENNILAHL_ORAN), BERRY_ID_ORAN);
    EXPECT_EQ(GetBerryTypeByBerryTreeId(Cormoria_BERRY_TREE_GALECREST_C), BERRY_ID_OCCA);
    EXPECT_EQ(GetBerryTypeByBerryTreeId(Cormoria_BERRY_TREE_VILETHORN_MICLE), BERRY_ID_MICLE);
    EXPECT_EQ(GetBerryTypeByBerryTreeId(Cormoria_BERRY_TREE_HOYA_C), BERRY_ID_COLBUR);
#else
    for (i = 0; i < BERRY_TREES_COUNT; i++)
        EXPECT_EQ(memcmp(GetBerryTreeInfo(i), &sBefore[i], sizeof(sBefore[i])), 0);
#endif
    Free(sBefore);
}
