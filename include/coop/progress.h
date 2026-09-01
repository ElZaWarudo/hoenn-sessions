#ifndef GUARD_COOP_PROGRESS_H
#define GUARD_COOP_PROGRESS_H

#include <stddef.h>

#include "gba/defines.h"
#include "gba/types.h"
#include "coop/region.h"

#define COOP_PROGRESS_BADGE_COUNT 8
#define COOP_PROGRESS_BADGE_MASK ((1u << COOP_PROGRESS_BADGE_COUNT) - 1u)
#define COOP_PROGRESS_MAX_PARTICIPANTS 2

/*
 * This is transient ROM state, not the canonical server record or a SaveBlock
 * extension. Region-qualified trainer and fly-point sets remain in the shared
 * protocol until the ROM has generated stable registries for those identities.
 */
struct RegionalProgress
{
    /* 0x00 */ u16 badge_mask;
    /* 0x02 */ u16 reserved;
    /* Zero means no regional story progress; nonzero values require the
     * versioned region registry introduced with save persistence. */
    /* 0x04 */ u32 story_checkpoint;
};

struct CoopProgress
{
    /* Slots are ordered HOENN, KANTO, JOHTO, SEVII. */
    /* 0x00 */ struct RegionalProgress regions[COOP_PROGRESS_REGION_COUNT];
};

typedef struct RegionalProgress RegionalProgress;
typedef struct CoopProgress CoopProgress;

_Static_assert(sizeof(struct RegionalProgress) == 8, "RegionalProgress ABI size");
_Static_assert(offsetof(struct RegionalProgress, badge_mask) == 0, "RegionalProgress badges offset");
_Static_assert(offsetof(struct RegionalProgress, story_checkpoint) == 4, "RegionalProgress story offset");
_Static_assert(sizeof(struct CoopProgress) == 32, "CoopProgress ABI size");

/* Runtime state is EWRAM-only and is initialized separately from SaveBlock. */
extern EWRAM_DATA struct CoopProgress gCoopProgress;

void CoopProgress_Init(struct CoopProgress *progress);
struct RegionalProgress *CoopProgress_GetRegion(struct CoopProgress *progress, enum CoopRegion region);
const struct RegionalProgress *CoopProgress_GetRegionConst(const struct CoopProgress *progress, enum CoopRegion region);
u8 CoopProgress_GetBadgeTier(const struct RegionalProgress *progress);
u8 CoopProgress_GetMinimumTier(enum CoopRegion battle_region,
                               const struct CoopProgress *participants,
                               u8 participant_count);

/* Descriptive aliases used by battle and host-facing callers. */
#define CoopProgress_MinimumBadgeTier CoopProgress_GetMinimumTier
#define CoopProgress_BadgeTier CoopProgress_GetBadgeTier

#endif /* GUARD_COOP_PROGRESS_H */
