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

static const struct TrainerMon sJoeyParty[] =
{
    {
        .iv = TRAINER_PARTY_IVS(0, 0, 0, 0, 0, 0),
        .moves = { MOVE_NONE, MOVE_NONE, MOVE_NONE, MOVE_NONE },
        .species = SPECIES_RATTATA,
        .heldItem = ITEM_NONE,
        .ability = ABILITY_NONE,
        .lvl = 4,
        .ball = BALL_POKE,
    },
};

/* Keep this table dense over imported records.  The reserved ID range is
 * represented by bounds checks in the accessor rather than 0x4000 sparse
 * legacy entries. */
const struct Trainer gJohtoTrainers[DIFFICULTY_COUNT][JOHTO_TRAINER_RECORD_COUNT] =
{
    [DIFFICULTY_NORMAL] =
    {
        [JOHTO_TRAINER_ORDINAL_JOEY] =
        {
            .aiFlags = AI_FLAG_CHECK_BAD_MOVE,
            .party = sJoeyParty,
            .trainerClass = TRAINER_CLASS_YOUNGSTER,
            .encounterMusic = TRAINER_ENCOUNTER_MUSIC_MALE,
            .multiTeamSize = MULTI_TEAM_SIZE_FULL,
            .gender = TRAINER_GENDER_MALE,
            .battleType = TRAINER_BATTLE_TYPE_SINGLES,
            .partySize = ARRAY_COUNT(sJoeyParty),
            .trainerPic = TRAINER_PIC_YOUNGSTER,
            .trainerName = _("JOEY"),
        },
    },
};

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
