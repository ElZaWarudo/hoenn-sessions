#include "global.h"
#include "berry.h"
#include "constants/berry.h"
#include "constants/johto_berry_plots.h"
#include "cormoria/berry_plots.h"

struct CormoriaInitialBerryPlot
{
    u8 id;
    enum BerryId berry;
};

#if ROM_WORLD == 2
// Donor new_game.inc seeds exactly these 24 plots. Vilethorn Oran and the
// four unused declared IDs remain empty, matching the pinned campaign.
static const struct CormoriaInitialBerryPlot sInitialPlots[] =
{
    {Cormoria_BERRY_TREE_FENNILAHL_ORAN, BERRY_ID_ORAN},
    {Cormoria_BERRY_TREE_ROUTE2_ORAN, BERRY_ID_ORAN},
    {Cormoria_BERRY_TREE_ROUTE2_PECHA, BERRY_ID_PECHA},
    {Cormoria_BERRY_TREE_ROUTE4_RAWST, BERRY_ID_RAWST},
    {Cormoria_BERRY_TREE_ROUTE4_PECHA, BERRY_ID_PECHA},
    {Cormoria_BERRY_TREE_GALECREST_ORAN, BERRY_ID_ORAN},
    {Cormoria_BERRY_TREE_GALECREST_B, BERRY_ID_CHESTO},
    {Cormoria_BERRY_TREE_GALECREST_C, BERRY_ID_OCCA},
    {Cormoria_BERRY_TREE_ROUTE5_KEBIA, BERRY_ID_KEBIA},
    {Cormoria_BERRY_TREE_ROUTE5_PECHA, BERRY_ID_PECHA},
    {Cormoria_BERRY_TREE_ROUTE5_SITRUS_1, BERRY_ID_SITRUS},
    {Cormoria_BERRY_TREE_ROUTE5_SITRUS_2, BERRY_ID_SITRUS},
    {Cormoria_BERRY_TREE_ROUTE5_COLBUR, BERRY_ID_COLBUR},
    {Cormoria_BERRY_TREE_ROUTE5_LIECHI, BERRY_ID_LIECHI},
    {Cormoria_BERRY_TREE_VILETHORN_PECHA, BERRY_ID_PECHA},
    {Cormoria_BERRY_TREE_VILETHORN_APICOT, BERRY_ID_APICOT},
    {Cormoria_BERRY_TREE_VILETHORN_MICLE, BERRY_ID_MICLE},
    {Cormoria_BERRY_TREE_VILETHORN_LEPPA, BERRY_ID_LEPPA},
    {Cormoria_BERRY_TREE_VILETHORN_CHESTO, BERRY_ID_CHESTO},
    {Cormoria_BERRY_TREE_PELLUCA_A, BERRY_ID_LUM},
    {Cormoria_BERRY_TREE_PELLUCA_B, BERRY_ID_CHOPLE},
    {Cormoria_BERRY_TREE_HOYA_A, BERRY_ID_LUM},
    {Cormoria_BERRY_TREE_HOYA_B, BERRY_ID_SITRUS},
    {Cormoria_BERRY_TREE_HOYA_C, BERRY_ID_COLBUR},
};

_Static_assert(ARRAY_COUNT(sInitialPlots) == 24, "Cormoria donor seed count");
#endif
_Static_assert(CORMORIA_BERRY_PLOTS_FIRST > JOHTO_BERRY_PLOTS_LAST,
               "Cormoria plots must not share Johto slots");
_Static_assert(CORMORIA_BERRY_PLOTS_LAST < BERRY_TREES_COUNT,
               "Cormoria plots must fit the regional save array");

void CormoriaBerryPlots_SeedNewGame(void)
{
#if ROM_WORLD == 2
    u32 i;

    for (i = 0; i < ARRAY_COUNT(sInitialPlots); i++)
        PlantBerryTree(sInitialPlots[i].id, sInitialPlots[i].berry, BERRY_STAGE_BERRIES, FALSE);
#endif
}
