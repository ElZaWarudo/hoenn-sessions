#ifndef GUARD_MASTERY_H
#define GUARD_MASTERY_H

#include "pokemon.h"

// Cosmetic progression after level 100. Ordinary levels and stats stay capped.
#define MASTERY_LEVELS_PER_RANK 100
#define MAX_MASTERY_LEVEL (3 * MASTERY_LEVELS_PER_RANK)
#define MASTERY_EXP_PER_LEVEL 25000

u32 GetMasteryLevel(enum Species species, u32 experience);
u32 GetMonMasteryLevel(struct Pokemon *mon);
u32 GetMaxMonExperience(enum Species species);
bool32 CanMonGainExperience(struct Pokemon *mon);
void AddMonExperience(struct Pokemon *mon, u32 experience);
void GetProgressLevelExpBounds(enum Species species, u32 experience, u32 *start, u32 *next);
u32 GetProgressLevelNextExp(enum Species species, u32 experience);
void FormatMasteryLevel(u8 *dest, u32 masteryLevel, bool32 compact);

#endif
