#include "global.h"
#include "coop/progress.h"
#include "test/test.h"

_Static_assert(sizeof(struct RegionalProgress) == 8, "tested regional progress ABI size");
_Static_assert(offsetof(struct RegionalProgress, badge_mask) == 0, "tested badge offset");
_Static_assert(offsetof(struct RegionalProgress, story_checkpoint) == 4, "tested story offset");
_Static_assert(sizeof(((struct RegionalProgress *)0)->story_checkpoint) == 4,
               "story checkpoint stays 32-bit");
_Static_assert(sizeof(struct CoopProgress) == 32, "tested progress ABI size");

static u8 ReferenceBadgeTier(u16 badgeMask)
{
    u8 tier = 0;
    u8 i;

    for (i = 0; i < COOP_PROGRESS_BADGE_COUNT; i++)
    {
        if (badgeMask & (1u << i))
            tier++;
    }
    return tier;
}

TEST("Cloud Coop regional badge tier counts only the regional badge byte")
{
    struct RegionalProgress progress = {0};
    u16 badgeMask;

    EXPECT_EQ(CoopProgress_GetBadgeTier(NULL), 0);
    for (badgeMask = 0; badgeMask <= COOP_PROGRESS_BADGE_MASK; badgeMask++)
    {
        progress.badge_mask = badgeMask | 0xFF00u;
        EXPECT_EQ(CoopProgress_GetBadgeTier(&progress), ReferenceBadgeTier(badgeMask));
    }
}

TEST("Cloud Coop progress exposes four independent regional slots")
{
    struct CoopProgress progress;

    memset(&progress, 0xFF, sizeof(progress));
    CoopProgress_Init(&progress);

    EXPECT_EQ(CoopProgress_GetRegion(&progress, COOP_REGION_HOENN), &progress.regions[0]);
    EXPECT_EQ(CoopProgress_GetRegion(&progress, COOP_REGION_KANTO), &progress.regions[1]);
    EXPECT_EQ(CoopProgress_GetRegion(&progress, COOP_REGION_JOHTO), &progress.regions[2]);
    EXPECT_EQ(CoopProgress_GetRegion(&progress, COOP_REGION_SEVII), &progress.regions[3]);
    EXPECT_EQ(CoopProgress_GetRegion(&progress, COOP_REGION_UNSPECIFIED), NULL);
    EXPECT_EQ(CoopProgress_GetRegion(NULL, COOP_REGION_HOENN), NULL);

    EXPECT_EQ(progress.regions[0].badge_mask, 0);
    EXPECT_EQ(progress.regions[0].reserved, 0);
    EXPECT_EQ(progress.regions[0].story_checkpoint, 0);
    EXPECT_EQ(progress.regions[3].badge_mask, 0);

    progress.regions[1].story_checkpoint = 0x89ABCDEFu;
    EXPECT_EQ(progress.regions[1].story_checkpoint, 0x89ABCDEFu);
    EXPECT_EQ(progress.regions[0].story_checkpoint, 0);
    EXPECT_EQ(progress.regions[2].story_checkpoint, 0);
}

TEST("Cloud Coop group tier is the minimum participant tier in the battle region")
{
    struct CoopProgress participants[COOP_PROGRESS_MAX_PARTICIPANTS];

    CoopProgress_Init(&participants[0]);
    CoopProgress_Init(&participants[1]);

    CoopProgress_GetRegion(&participants[0], COOP_REGION_HOENN)->badge_mask = 0xFF;
    CoopProgress_GetRegion(&participants[0], COOP_REGION_KANTO)->badge_mask = 0x07;
    CoopProgress_GetRegion(&participants[0], COOP_REGION_JOHTO)->badge_mask = 0x3F;
    CoopProgress_GetRegion(&participants[0], COOP_REGION_SEVII)->badge_mask = 0x0F;

    CoopProgress_GetRegion(&participants[1], COOP_REGION_HOENN)->badge_mask = 0x1F;
    CoopProgress_GetRegion(&participants[1], COOP_REGION_KANTO)->badge_mask = 0x01;
    CoopProgress_GetRegion(&participants[1], COOP_REGION_JOHTO)->badge_mask = 0x07;
    CoopProgress_GetRegion(&participants[1], COOP_REGION_SEVII)->badge_mask = 0x03;

    EXPECT_EQ(CoopProgress_GetMinimumTier(COOP_REGION_HOENN, participants, 2), 5);
    EXPECT_EQ(CoopProgress_GetMinimumTier(COOP_REGION_KANTO, participants, 2), 1);
    EXPECT_EQ(CoopProgress_GetMinimumTier(COOP_REGION_JOHTO, participants, 2), 3);
    EXPECT_EQ(CoopProgress_GetMinimumTier(COOP_REGION_SEVII, participants, 2), 2);

    /* Hoenn completion must not raise Kanto's co-op tier. */
    participants[0].regions[0].badge_mask = 0xFF;
    participants[1].regions[0].badge_mask = 0xFF;
    EXPECT_EQ(CoopProgress_GetMinimumTier(COOP_REGION_KANTO, participants, 2), 1);

    EXPECT_EQ(CoopProgress_GetMinimumTier(COOP_REGION_KANTO, participants, 1), 3);
    EXPECT_EQ(CoopProgress_GetMinimumTier(COOP_REGION_UNSPECIFIED, participants, 2), 0);
    EXPECT_EQ(CoopProgress_GetMinimumTier(COOP_REGION_KANTO, NULL, 2), 0);
    EXPECT_EQ(CoopProgress_GetMinimumTier(COOP_REGION_KANTO, participants, 0), 0);
    EXPECT_EQ(CoopProgress_GetMinimumTier(COOP_REGION_KANTO,
                                          participants,
                                          COOP_PROGRESS_MAX_PARTICIPANTS + 1),
              0);
}
