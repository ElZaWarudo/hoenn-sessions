#include "global.h"
#include "battle.h"
#include "battle_setup.h"
#include "event_data.h"
#include "test/test.h"
#include "trainer_slide.h"
#include "constants/difficulty.h"
#include "constants/flags.h"
#include "constants/johto_content.h"
#include "constants/trainers.h"
#include "constants/vars.h"

// TRAINER_LEAF_TEST is private to trainer_slide.c and is intentionally kept at 2
// in the shared testing table. This fixture uses its public SetTrainerSlideMessage
// entry without exposing a production trainer-slide identifier.
#define TRAINER_SLIDE_TEST_TRAINER 2

static EWRAM_DATA struct BattleStruct sTrainerSlideBattleStruct;
static const u8 sStaleTrainerSlide[] = {0xFF};

struct TrainerSlideFixtureState
{
    struct BattleStruct *battleStruct;
    u32 battleTypeFlags;
    TrainerBattleParameter trainerBattleParameter;
    bool8 trainerSlidesFlag;
    u16 trainerSlidesVar;
    enum BattlerId scriptingBattler;
};

static void BeginTrainerSlideFixture(struct TrainerSlideFixtureState *state)
{
    state->battleStruct = gBattleStruct;
    state->battleTypeFlags = gBattleTypeFlags;
    state->trainerBattleParameter = gTrainerBattleParameter;
    state->trainerSlidesFlag = FlagGet(TESTING_FLAG_TRAINER_SLIDES);
    state->trainerSlidesVar = VarGet(TESTING_VAR_TRAINER_SLIDES);
    state->scriptingBattler = gBattleScripting.battler;

    memset(&sTrainerSlideBattleStruct, 0, sizeof(sTrainerSlideBattleStruct));
    gBattleStruct = &sTrainerSlideBattleStruct;
    gBattleTypeFlags = BATTLE_TYPE_TRAINER;
    TRAINER_BATTLE_PARAM.opponentA = JOHTO_TRAINER_ID_MIN;
    TRAINER_BATTLE_PARAM.opponentB = TRAINER_NONE;
    gBattleScripting.battler = B_BATTLER_0;
    FlagSet(TESTING_FLAG_TRAINER_SLIDES);
    VarSet(TESTING_VAR_TRAINER_SLIDES, TRAINER_SLIDE_BEFORE_FIRST_TURN);
}

static void EndTrainerSlideFixture(const struct TrainerSlideFixtureState *state)
{
    gBattleStruct = state->battleStruct;
    gBattleTypeFlags = state->battleTypeFlags;
    gTrainerBattleParameter = state->trainerBattleParameter;
    gBattleScripting.battler = state->scriptingBattler;

    if (state->trainerSlidesFlag)
        FlagSet(TESTING_FLAG_TRAINER_SLIDES);
    else
        FlagClear(TESTING_FLAG_TRAINER_SLIDES);
    VarSet(TESTING_VAR_TRAINER_SLIDES, state->trainerSlidesVar);
}

TEST("Trainer slides reject Johto and invalid table indices")
{
    struct TrainerSlideFixtureState state;
    u32 ordinal;

    BeginTrainerSlideFixture(&state);

    for (ordinal = 0; ordinal < JOHTO_TRAINER_RECORD_COUNT; ordinal++)
    {
        gBattleStruct->trainerSlideMsg = sStaleTrainerSlide;
        SetTrainerSlideMessage(DIFFICULTY_NORMAL,
                               JOHTO_TRAINER_ID_MIN + ordinal,
                               TRAINER_SLIDE_BEFORE_FIRST_TURN);
        EXPECT_EQ(gBattleStruct->trainerSlideMsg, NULL);
    }

    gBattleStruct->trainerSlideMsg = sStaleTrainerSlide;
    SetTrainerSlideMessage(DIFFICULTY_NORMAL, 0xFFFF, TRAINER_SLIDE_BEFORE_FIRST_TURN);
    EXPECT_EQ(gBattleStruct->trainerSlideMsg, NULL);
    gBattleStruct->trainerSlideMsg = sStaleTrainerSlide;
    SetTrainerSlideMessage(DIFFICULTY_NORMAL, 0xFFFFFFFFu, TRAINER_SLIDE_BEFORE_FIRST_TURN);
    EXPECT_EQ(gBattleStruct->trainerSlideMsg, NULL);

    gBattleStruct->trainerSlideMsg = sStaleTrainerSlide;
    SetTrainerSlideMessage((enum DifficultyLevel)DIFFICULTY_COUNT,
                           TRAINER_SLIDE_TEST_TRAINER,
                           TRAINER_SLIDE_BEFORE_FIRST_TURN);
    EXPECT_EQ(gBattleStruct->trainerSlideMsg, NULL);
    gBattleStruct->trainerSlideMsg = sStaleTrainerSlide;
    SetTrainerSlideMessage((enum DifficultyLevel)-1,
                           TRAINER_SLIDE_TEST_TRAINER,
                           TRAINER_SLIDE_BEFORE_FIRST_TURN);
    EXPECT_EQ(gBattleStruct->trainerSlideMsg, NULL);

    gBattleStruct->trainerSlideMsg = sStaleTrainerSlide;
    SetTrainerSlideMessage(DIFFICULTY_NORMAL,
                           TRAINER_SLIDE_TEST_TRAINER,
                           TRAINER_SLIDE_COUNT);
    EXPECT_EQ(gBattleStruct->trainerSlideMsg, NULL);
    gBattleStruct->trainerSlideMsg = sStaleTrainerSlide;
    SetTrainerSlideMessage(DIFFICULTY_NORMAL,
                           TRAINER_SLIDE_TEST_TRAINER,
                           0xFFFFFFFFu);
    EXPECT_EQ(gBattleStruct->trainerSlideMsg, NULL);

    EndTrainerSlideFixture(&state);
}

TEST("Trainer slides preserve valid test messages and normal fallback")
{
    struct TrainerSlideFixtureState state;
    const u8 *normalMessage;

    BeginTrainerSlideFixture(&state);

    SetTrainerSlideMessage(DIFFICULTY_NORMAL,
                           TRAINER_SLIDE_TEST_TRAINER,
                           TRAINER_SLIDE_BEFORE_FIRST_TURN);
    normalMessage = gBattleStruct->trainerSlideMsg;
    EXPECT(normalMessage != NULL);

    SetTrainerSlideMessage(DIFFICULTY_HARD,
                           TRAINER_SLIDE_TEST_TRAINER,
                           TRAINER_SLIDE_BEFORE_FIRST_TURN);
    EXPECT_EQ(gBattleStruct->trainerSlideMsg, normalMessage);

    FlagClear(TESTING_FLAG_TRAINER_SLIDES);
    gBattleStruct->trainerSlideMsg = sStaleTrainerSlide;
    SetTrainerSlideMessage(DIFFICULTY_NORMAL,
                           TRAINER_SLIDE_TEST_TRAINER,
                           TRAINER_SLIDE_BEFORE_FIRST_TURN);
    EXPECT_EQ(gBattleStruct->trainerSlideMsg, NULL);

    EndTrainerSlideFixture(&state);
}

TEST("Johto trainer slides fail closed for every public slide kind")
{
    struct TrainerSlideFixtureState state;
    enum TrainerSlideType slideId;

    BeginTrainerSlideFixture(&state);
    gBattleStruct->trainerSlideMsg = sStaleTrainerSlide;

    for (slideId = TRAINER_SLIDE_NONE; slideId < TRAINER_SLIDE_COUNT; slideId++)
    {
        EXPECT_EQ(ShouldDoTrainerSlide(B_BATTLER_1, slideId), TRAINER_SLIDE_TARGET_NONE);
        EXPECT_EQ(gBattleStruct->trainerSlideMsg, sStaleTrainerSlide);
    }

    EXPECT_EQ(ShouldDoTrainerSlide(B_BATTLER_1, TRAINER_SLIDE_COUNT), TRAINER_SLIDE_TARGET_NONE);
    EXPECT_EQ(ShouldDoTrainerSlide(B_BATTLER_1, (enum TrainerSlideType)-1), TRAINER_SLIDE_TARGET_NONE);
    EXPECT_EQ(gBattleStruct->trainerSlideMsg, sStaleTrainerSlide);

    EndTrainerSlideFixture(&state);
}
