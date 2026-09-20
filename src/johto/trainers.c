#include "global.h"
#include "data.h"
#include "johto/trainers.h"
#include "constants/abilities.h"
#include "constants/battle_ai.h"
#include "constants/moves.h"
#include "constants/pokeball.h"
#include "constants/species.h"
#include "constants/trainers.h"

_Static_assert(JOHTO_TRAINER_NAMESPACE_SIZE == 512,
               "Johto trainer namespace must fit JohtoSave trainer bits");

#include "../data/johto/trainers.h"

bool8 JohtoTrainer_IsId(u16 trainerId)
{
    return trainerId >= JOHTO_TRAINER_ID_MIN
        && trainerId <= JOHTO_TRAINER_ID_MAX;
}

bool8 JohtoTrainer_GetOrdinal(u16 trainerId, u16 *ordinal)
{
    u16 candidate;

    if (ordinal == NULL || !JohtoTrainer_IsId(trainerId))
        return FALSE;

    candidate = trainerId - JOHTO_TRAINER_ID_MIN;
    if (candidate >= JOHTO_TRAINER_RECORD_COUNT
     || gJohtoTrainers[DIFFICULTY_NORMAL][candidate].party == NULL)
        return FALSE;

    *ordinal = candidate;
    return TRUE;
}

bool8 JohtoTrainer_IsPopulated(u16 trainerId)
{
    u16 ordinal;

    return JohtoTrainer_GetOrdinal(trainerId, &ordinal);
}

const struct Trainer *JohtoTrainer_GetStructAtDifficulty(enum DifficultyLevel difficulty,
                                                         u16 trainerId)
{
    u16 ordinal;

    if (difficulty < DIFFICULTY_MIN || difficulty > DIFFICULTY_MAX
     || !JohtoTrainer_IsId(trainerId))
        return NULL;

    ordinal = trainerId - JOHTO_TRAINER_ID_MIN;
    if (ordinal >= JOHTO_TRAINER_RECORD_COUNT)
        return NULL;
    if (gJohtoTrainers[difficulty][ordinal].party == NULL)
        return NULL;
    return &gJohtoTrainers[difficulty][ordinal];
}

const struct Trainer *JohtoTrainer_GetStruct(u16 trainerId)
{
    return JohtoTrainer_GetStructAtDifficulty(DIFFICULTY_NORMAL, trainerId);
}
