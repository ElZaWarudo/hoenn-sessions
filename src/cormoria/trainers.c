#include "global.h"
#include "data.h"
#include "cormoria/trainers.h"
#include "constants/abilities.h"
#include "constants/battle_ai.h"
#include "constants/moves.h"
#include "constants/pokeball.h"
#include "constants/species.h"
#include "constants/trainers.h"
#include "battle_transition.h"

_Static_assert(CORMORIA_TRAINER_ID_MIN + CORMORIA_TRAINER_RECORD_COUNT - 1 <= CORMORIA_TRAINER_ID_MAX,
               "Cormoria trainer records exceed their runtime ID window");

#if ROM_WORLD == 2
#include "../data/cormoria/trainers.h"
#endif

bool8 CormoriaTrainer_IsId(u16 trainerId)
{
    return trainerId >= CORMORIA_TRAINER_ID_MIN && trainerId <= CORMORIA_TRAINER_ID_MAX;
}

bool8 CormoriaTrainer_GetOrdinal(u16 trainerId, u16 *ordinal)
{
#if ROM_WORLD == 2
    u16 candidate;

    if (ordinal == NULL || !CormoriaTrainer_IsId(trainerId))
        return FALSE;
    candidate = trainerId - CORMORIA_TRAINER_ID_MIN;
    if (candidate >= CORMORIA_TRAINER_RECORD_COUNT
     || ((gCormoriaTrainers[DIFFICULTY_NORMAL][candidate].party == NULL
       || gCormoriaTrainers[DIFFICULTY_NORMAL][candidate].partySize == 0)
      && (gCormoriaTrainers[DIFFICULTY_EASY][candidate].party == NULL
       || gCormoriaTrainers[DIFFICULTY_EASY][candidate].partySize == 0)))
        return FALSE;
    *ordinal = candidate;
    return TRUE;
#else
    (void)trainerId;
    (void)ordinal;
    return FALSE;
#endif
}

bool8 CormoriaTrainer_IsPopulated(u16 trainerId)
{
    u16 ordinal;

    return CormoriaTrainer_GetOrdinal(trainerId, &ordinal);
}

const struct Trainer *CormoriaTrainer_GetStructAtDifficulty(enum DifficultyLevel difficulty,
                                                            u16 trainerId)
{
#if ROM_WORLD == 2
    u16 ordinal;

    if (difficulty < DIFFICULTY_MIN || difficulty > DIFFICULTY_MAX
     || !CormoriaTrainer_GetOrdinal(trainerId, &ordinal))
        return NULL;
    if (gCormoriaTrainers[difficulty][ordinal].party == NULL
     || gCormoriaTrainers[difficulty][ordinal].partySize == 0)
        return NULL;
    return &gCormoriaTrainers[difficulty][ordinal];
#else
    (void)difficulty;
    (void)trainerId;
    return NULL;
#endif
}
