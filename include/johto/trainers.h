#ifndef GUARD_JOHTO_TRAINERS_H
#define GUARD_JOHTO_TRAINERS_H

#include "gba/types.h"
#include "constants/johto_trainers.h"
#include "difficulty.h"

struct Trainer;

/* Namespace membership is intentionally separate from a populated lookup. */
bool8 JohtoTrainer_IsId(u16 trainerId);
bool8 JohtoTrainer_IsPopulated(u16 trainerId);
bool8 JohtoTrainer_GetOrdinal(u16 trainerId, u16 *ordinal);

const struct Trainer *JohtoTrainer_GetStruct(u16 trainerId);
const struct Trainer *JohtoTrainer_GetStructAtDifficulty(enum DifficultyLevel difficulty,
                                                         u16 trainerId);

#endif /* GUARD_JOHTO_TRAINERS_H */
