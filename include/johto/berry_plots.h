#ifndef GUARD_JOHTO_BERRY_PLOTS_H
#define GUARD_JOHTO_BERRY_PLOTS_H

#include "global.h"

/* Call only after ClearBerryTrees during new-game initialization. */
void JohtoBerryPlots_InitializeNewGame(void);

struct ScriptContext;
u8 JohtoBerryPlots_TryHarvest(u8 plot);
void Script_JohtoHarvestBerryTree(struct ScriptContext *ctx);

#endif
