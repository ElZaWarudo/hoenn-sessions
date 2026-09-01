#include "global.h"
#include "coop/progress.h"

EWRAM_DATA struct CoopProgress gCoopProgress = {0};

static s32 GetRegionSlot(enum CoopRegion region)
{
    switch (region)
    {
    case COOP_REGION_HOENN:
        return 0;
    case COOP_REGION_KANTO:
        return 1;
    case COOP_REGION_JOHTO:
        return 2;
    case COOP_REGION_SEVII:
        return 3;
    default:
        return -1;
    }
}

void CoopProgress_Init(struct CoopProgress *progress)
{
    u32 i;

    if (progress == NULL)
        return;

    for (i = 0; i < COOP_PROGRESS_REGION_COUNT; i++)
    {
        progress->regions[i].badge_mask = 0;
        progress->regions[i].reserved = 0;
        progress->regions[i].story_checkpoint = 0;
    }
}

struct RegionalProgress *CoopProgress_GetRegion(struct CoopProgress *progress, enum CoopRegion region)
{
    s32 slot;

    if (progress == NULL)
        return NULL;

    slot = GetRegionSlot(region);
    if (slot < 0)
        return NULL;
    return &progress->regions[slot];
}

const struct RegionalProgress *CoopProgress_GetRegionConst(const struct CoopProgress *progress, enum CoopRegion region)
{
    s32 slot;

    if (progress == NULL)
        return NULL;

    slot = GetRegionSlot(region);
    if (slot < 0)
        return NULL;
    return &progress->regions[slot];
}

u8 CoopProgress_GetBadgeTier(const struct RegionalProgress *progress)
{
    u16 badges;
    u8 tier = 0;

    if (progress == NULL)
        return 0;

    badges = progress->badge_mask & COOP_PROGRESS_BADGE_MASK;
    while (badges != 0)
    {
        tier += (u8)(badges & 1);
        badges >>= 1;
    }
    return tier;
}

u8 CoopProgress_GetMinimumTier(enum CoopRegion battle_region,
                               const struct CoopProgress *participants,
                               u8 participant_count)
{
    u8 i;
    u8 minimum = COOP_PROGRESS_BADGE_COUNT;

    if (!CoopRegion_IsValid(battle_region)
     || participants == NULL
     || participant_count == 0
     || participant_count > COOP_PROGRESS_MAX_PARTICIPANTS)
        return 0;

    for (i = 0; i < participant_count; i++)
    {
        const struct RegionalProgress *progress = CoopProgress_GetRegionConst(&participants[i], battle_region);
        u8 tier;

        if (progress == NULL)
            return 0;

        tier = CoopProgress_GetBadgeTier(progress);
        if (tier < minimum)
            minimum = tier;
    }
    return minimum;
}
