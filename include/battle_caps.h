#ifndef GUARD_BATTLE_CAPS_H
#define GUARD_BATTLE_CAPS_H

#include "pokemon.h"

u32 GetBadgeBattleLevelCap(void);
enum Species GetSpeciesAtBattleLevelCap(enum Species species, u32 cap);
void BeginBattleLevelCaps(void);
void EndBattleLevelCaps(void);
bool32 BattleCaps_BeginExperience(u32 partyIndex);
bool32 BattleCaps_EndExperience(u32 partyIndex);
/* Sets a battle copy of a Pokemon to exactly this level (up or down) from
 * its own growth table, keeping species, moves and the HP fraction. Used on
 * copies only: friendly battles scale both teams on both ROMs alike. */
void BattleCaps_SetMonLevel(struct Pokemon *mon, u8 level);

#endif
