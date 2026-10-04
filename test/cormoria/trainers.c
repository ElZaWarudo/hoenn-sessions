#include "global.h"
#include "data.h"
#include "difficulty.h"
#include "coop/generated_regional_identities.h"
#include "coop/identity.h"
#include "cormoria/trainers.h"
#include "test/test.h"
#include "constants/battle_ai.h"
#include "constants/cormoria_event_ids.h"
#include "constants/species.h"
#include "constants/trainers.h"

#if ROM_WORLD == 2
TEST("Cormoria Route 1 trainer uses its exact easy and normal donor records")
{
    enum DifficultyLevel previous = GetCurrentDifficultyLevel();
    const struct Trainer *trainer;
    u16 ordinal = 0;

    EXPECT(CormoriaTrainer_GetOrdinal(Cormoria_TRAINER_ROUTE1_A, &ordinal));
    EXPECT_EQ(ordinal, 0x5E);

    SetCurrentDifficultyLevel(DIFFICULTY_NORMAL);
    trainer = GetTrainerStructFromId(Cormoria_TRAINER_ROUTE1_A);
    EXPECT_EQ(trainer->trainerClass, TRAINER_CLASS_YOUNGSTER);
    EXPECT_EQ(trainer->trainerPic, TRAINER_PIC_YOUNGSTER);
    EXPECT_EQ(trainer->aiFlags, AI_FLAG_SMART_TRAINER);
    EXPECT_EQ((u16)trainer->partySize, 1);
    EXPECT_EQ(trainer->party[0].species, SPECIES_RATTATA_ALOLA);
    EXPECT_EQ(trainer->party[0].lvl, 3);
    EXPECT_EQ(trainer->party[0].iv, TRAINER_PARTY_IVS(31, 31, 31, 31, 31, 31));

    SetCurrentDifficultyLevel(DIFFICULTY_EASY);
    trainer = GetTrainerStructFromId(Cormoria_TRAINER_ROUTE1_A);
    EXPECT_EQ(trainer->aiFlags, AI_FLAG_BASIC_TRAINER);
    EXPECT_EQ(trainer->party[0].iv, TRAINER_PARTY_IVS(0, 0, 0, 0, 0, 0));
    SetCurrentDifficultyLevel(previous);
}

TEST("Cormoria unpopulated IDs fail closed before host trainer array")
{
    const u16 absent = CORMORIA_TRAINER_ID_MIN + CORMORIA_TRAINER_RECORD_COUNT;
    u16 ordinal = 0xFFFF;

    EXPECT(!CormoriaTrainer_IsPopulated(Cormoria_TRAINER_NONE));
    EXPECT(!CormoriaTrainer_GetOrdinal(Cormoria_TRAINER_NONE, &ordinal));
    EXPECT_EQ(ordinal, 0xFFFF);
    EXPECT(CormoriaTrainer_GetStructAtDifficulty(DIFFICULTY_NORMAL,
                                                Cormoria_TRAINER_NONE) == NULL);
    EXPECT_EQ(GetTrainerStructFromId(Cormoria_TRAINER_NONE),
              &gTrainers[DIFFICULTY_NORMAL][TRAINER_NONE]);
    EXPECT(!CormoriaTrainer_IsPopulated(absent));
    EXPECT_EQ(SanitizeTrainerId(absent), TRAINER_NONE);
    EXPECT_EQ(GetTrainerStructFromId(absent), &gTrainers[DIFFICULTY_NORMAL][TRAINER_NONE]);
}

TEST("Cormoria easy-only trainer remains addressable at normal difficulty")
{
    enum DifficultyLevel previous = GetCurrentDifficultyLevel();

    SetCurrentDifficultyLevel(DIFFICULTY_NORMAL);
    EXPECT_EQ(GetTrainerDifficultyLevel(Cormoria_TRAINER_ROUTE8_E), DIFFICULTY_EASY);
    EXPECT_EQ(GetTrainerStructFromId(Cormoria_TRAINER_ROUTE8_E),
              CormoriaTrainer_GetStructAtDifficulty(DIFFICULTY_EASY,
                                                    Cormoria_TRAINER_ROUTE8_E));
    SetCurrentDifficultyLevel(previous);
}

TEST("Cormoria trainer has qualified online identity")
{
    u16 ordinal = 0;

    EXPECT(CoopIdentity_ResolveTrainerOrdinal(COOP_REGION_CORMORIA,
                                              Cormoria_TRAINER_ROUTE1_A,
                                              &ordinal));
    EXPECT_EQ(ordinal, COOP_TRAINER_CORMORIA_TRAINER_ROUTE1_A_ORDINAL);
    EXPECT_EQ(ordinal, 856 + 0x5E - 1);
}
#endif
