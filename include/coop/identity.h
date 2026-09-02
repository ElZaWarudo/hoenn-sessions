#ifndef GUARD_COOP_IDENTITY_H
#define GUARD_COOP_IDENTITY_H

#include "gba/types.h"
#include "coop/region.h"

/*
 * LEGACY is an explicit compatibility decision for a save that cannot enter
 * cloud play. HANDLED means the region-qualified save was authoritative.
 * REJECTED means the save was authoritative but the active region or identity
 * was not; callers must not fall back to a colliding raw flag in that case.
 */
enum CoopIdentityAccessResult
{
    COOP_IDENTITY_ACCESS_LEGACY,
    COOP_IDENTITY_ACCESS_HANDLED,
    COOP_IDENTITY_ACCESS_REJECTED,
};

bool8 CoopIdentity_ResolveTrainerOrdinal(enum CoopRegion region,
                                         u16 legacy_trainer_id,
                                         u16 *ordinal);
enum CoopIdentityAccessResult CoopIdentity_GetTrainerDefeated(u16 legacy_trainer_id,
                                                               bool8 *defeated);
enum CoopIdentityAccessResult CoopIdentity_SetTrainerDefeated(u16 legacy_trainer_id,
                                                               bool8 defeated);

#endif /* GUARD_COOP_IDENTITY_H */
