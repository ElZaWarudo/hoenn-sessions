#ifndef GUARD_BATTLE_CAPS_H
#define GUARD_BATTLE_CAPS_H

#include "pokemon.h"

u32 GetBadgeBattleLevelCap(void);
enum Species GetSpeciesAtBattleLevelCap(enum Species species, u32 cap);
void BeginBattleLevelCaps(void);
void EndBattleLevelCaps(void);
bool32 BattleCaps_BeginExperience(u32 partyIndex);
bool32 BattleCaps_EndExperience(u32 partyIndex);

#endif
