#include "global.h"
#include "event_data.h"
#include "event_object_movement.h"
#include "item.h"
#include "overworld.h"
#include "script.h"
#include "sprite.h"
#include "string_util.h"
#include "tv.h"
#include "constants/event_objects.h"
#include "constants/event_object_movement.h"
#include "constants/game_stat.h"
#include "constants/vars.h"
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

static bool32 IsJohtoPlot(u8 plot)
{
    return plot >= JOHTO_BERRY_PLOTS_FIRST && plot <= JOHTO_BERRY_PLOTS_LAST;
}

u8 JohtoBerryPlots_TryHarvest(u8 plot)
{
    struct BerryTree *tree;
    enum BerryId berry;
    enum Item item;
    u16 count;
    if (!IsJohtoPlot(plot))
        return JOHTO_BERRY_HARVEST_UNAVAILABLE;
    tree = GetBerryTreeInfo(plot);
    berry = tree->berry;
    count = tree->berryYield;
    item = BerryTypeToItemId(berry);
    if (tree->stage != BERRY_STAGE_BERRIES || item == ITEM_NONE || count == 0)
        return JOHTO_BERRY_HARVEST_UNAVAILABLE;
    if (!AddBagItem(item, count))
        return JOHTO_BERRY_HARVEST_BAG_FULL;

    RemoveBerryTree(plot);
    PlantBerryTree(plot, berry, BERRY_STAGE_SPROUTED, TRUE);
    VarSet(VAR_DAILY_PICKED_BERRIES, VarGet(VAR_DAILY_PICKED_BERRIES) + count);
    IncrementDailyPlantedBerries();
    IncrementGameStat(GAME_STAT_PLANTED_BERRIES);
    return JOHTO_BERRY_HARVEST_SUCCESS;
}

void Script_JohtoHarvestBerryTree(struct ScriptContext *ctx)
{
    struct ObjectEvent *object;
    struct BerryTree *tree;
    u8 plot, berry, count;
    (void)ctx;
    Script_RequestEffects(SCREFF_V1 | SCREFF_SAVE);
    gSpecialVar_Result = JOHTO_BERRY_HARVEST_UNAVAILABLE;
    if (gSelectedObjectEvent >= OBJECT_EVENTS_COUNT)
        return;
    object = &gObjectEvents[gSelectedObjectEvent];
    if (!object->active || object->movementType != MOVEMENT_TYPE_BERRY_TREE_GROWTH
        || object->spriteId >= MAX_SPRITES)
        return;
    plot = GetObjectEventBerryTreeId(gSelectedObjectEvent);
    if (!IsJohtoPlot(plot))
        return;
    tree = GetBerryTreeInfo(plot);
    berry = tree->berry;
    count = tree->berryYield;
    gSpecialVar_Result = JohtoBerryPlots_TryHarvest(plot);
    if (gSpecialVar_Result == JOHTO_BERRY_HARVEST_UNAVAILABLE)
        return;
    gSpecialVar_0x8006 = count;
    CopyItemNameHandlePlural(BerryTypeToItemId(berry), gStringVar1, count);
    ConvertIntToDecimalStringN(gStringVar2, count, STR_CONV_MODE_LEFT_ALIGN, 3);
    if (gSpecialVar_Result == JOHTO_BERRY_HARVEST_SUCCESS)
        SetBerryTreeJustPicked(object->localId, object->mapNum, object->mapGroup);
}
