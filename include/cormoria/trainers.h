#ifndef GUARD_CORMORIA_TRAINERS_H
#define GUARD_CORMORIA_TRAINERS_H

#include "gba/types.h"
#include "difficulty.h"

struct Trainer;

/* Runtime trainer IDs occupy the added-world trainer window.  A future ROM
 * can reuse the window with a different regional save and registry. */
#define CORMORIA_TRAINER_ID_MIN 0x5000
#define CORMORIA_TRAINER_ID_MAX 0x5FFF
#define CORMORIA_TRAINER_RECORD_COUNT 195

bool8 CormoriaTrainer_IsId(u16 trainerId);
bool8 CormoriaTrainer_IsPopulated(u16 trainerId);
bool8 CormoriaTrainer_GetOrdinal(u16 trainerId, u16 *ordinal);
const struct Trainer *CormoriaTrainer_GetStructAtDifficulty(enum DifficultyLevel difficulty,
                                                            u16 trainerId);

#endif /* GUARD_CORMORIA_TRAINERS_H */
