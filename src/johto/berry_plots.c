#include "global.h"
#include "berry.h"
#include "constants/berry.h"
#include "constants/johto_berry_plots.h"
#include "johto/berry_plots.h"

static const struct
{
    u8 id;
    enum BerryId berry;
} sInitialPlots[] =
{
    {JOHTO_BERRY_TREE_CHERI_2, BERRY_ID_CHERI},
    {JOHTO_BERRY_TREE_PECHA_2, BERRY_ID_PECHA},
    {JOHTO_BERRY_TREE_ORAN_1, BERRY_ID_ORAN},
    {JOHTO_BERRY_TREE_ROUTE_102_ORAN, BERRY_ID_ORAN},
    {JOHTO_BERRY_TREE_ORAN_2, BERRY_ID_ORAN},
    {JOHTO_BERRY_TREE_CHERI_1, BERRY_ID_CHERI},
    {JOHTO_BERRY_TREE_PECHA_1, BERRY_ID_PECHA},
    {JOHTO_BERRY_TREE_PERSIM_1, BERRY_ID_PERSIM},
    {JOHTO_BERRY_TREE_PERSIM_2, BERRY_ID_PERSIM},
    {JOHTO_BERRY_TREE_CHESTO_1, BERRY_ID_CHESTO},
    {JOHTO_BERRY_TREE_RAWST_1, BERRY_ID_RAWST},
    {JOHTO_BERRY_TREE_RAWST_2, BERRY_ID_RAWST},
    {JOHTO_BERRY_TREE_CHESTO_2, BERRY_ID_CHESTO},
    {JOHTO_BERRY_TREE_LEPPA_1, BERRY_ID_LEPPA},
    {JOHTO_BERRY_TREE_LEPPA_2, BERRY_ID_LEPPA},
    {JOHTO_BERRY_TREE_ASPEAR_1, BERRY_ID_ASPEAR},
    {JOHTO_BERRY_TREE_ASPEAR_2, BERRY_ID_ASPEAR},
    {JOHTO_BERRY_TREE_LUM_1, BERRY_ID_LUM},
    {JOHTO_BERRY_TREE_SITRUS_1, BERRY_ID_SITRUS},
    {JOHTO_BERRY_TREE_ROUTE_118_SITRUS_1, BERRY_ID_SITRUS},
};

_Static_assert(ARRAY_COUNT(sInitialPlots) == JOHTO_BERRY_PLOTS_COUNT, "Johto plot count");
_Static_assert(JOHTO_BERRY_PLOTS_LAST < BERRY_TREES_COUNT, "Johto plots fit saved berry array");

void JohtoBerryPlots_InitializeNewGame(void)
{
    u32 i;
    for (i = 0; i < ARRAY_COUNT(sInitialPlots); i++)
        PlantBerryTree(sInitialPlots[i].id, sInitialPlots[i].berry, BERRY_STAGE_BERRIES, FALSE);
}
