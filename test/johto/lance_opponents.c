#include "global.h"
#include "data.h"
#include "test/test.h"
#include "constants/johto_content.h"
#include "constants/trainers.h"

// GetTrainerStructFromId reads the real Johto roster in TESTING. This fixture
// checks metadata, not the substituted AreMultiPartiesFullTeams battle policy.
TEST("Johto Lance opponents alone use half teams in the public roster")
{
    u32 ordinal;
    u32 halfCount = 0;

    EXPECT_EQ(JOHTO_TRAINER_RECORD_COUNT, 412);
    for (ordinal = 0; ordinal < JOHTO_TRAINER_RECORD_COUNT; ordinal++)
    {
        u16 id = JOHTO_TRAINER_ID_MIN + ordinal;
        const struct Trainer *trainer = GetTrainerStructFromId(id);
        u32 expected = (id == JOHTO_TRAINER_ARIANA_1 || id == JOHTO_TRAINER_GRUNT_23)
            ? MULTI_TEAM_SIZE_HALF : MULTI_TEAM_SIZE_FULL;

        EXPECT(trainer != NULL);
        EXPECT(trainer->party != NULL);
        EXPECT_EQ((u32)trainer->multiTeamSize, expected);
        if (trainer->multiTeamSize == MULTI_TEAM_SIZE_HALF)
            halfCount++;
    }
    EXPECT_EQ(halfCount, 2);
}
