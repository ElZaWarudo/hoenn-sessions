#include "global.h"
#include "coop/generated_regional_identities.h"
#include "coop/identity.h"
#include "coop/save.h"

_Static_assert(COOP_TRAINER_IDENTITY_COUNT <= COOP_TRAINER_IDENTITY_CAPACITY,
               "trainer registry exceeds persisted capacity");

/* Hoenn's legacy trainer IDs are one-based and its frozen v1 ordinals are
 * zero-based. Keep the dense fast-path bound tied to the final Hoenn entry,
 * never to the first identity assigned for another region. */
#define COOP_HOENN_TRAINER_LEGACY_MAX \
    (COOP_TRAINER_HOENN_TRAINER_MAY_PLACEHOLDER_ORDINAL + 1u)

bool8 CoopIdentity_ResolveTrainerOrdinal(enum CoopRegion region,
                                         u16 legacy_trainer_id,
                                         u16 *ordinal)
{
    u32 i;

    if (!CoopRegion_IsValid(region)
     || legacy_trainer_id == COOP_IDENTITY_LEGACY_NONE)
        return FALSE;

    /* Hoenn is the hot path while trainer sight is evaluated. Its generated
     * v1 registry is dense, but the entry is still verified before use so a
     * future generator change fails closed instead of changing identity. */
    if (region == COOP_REGION_HOENN
     && legacy_trainer_id > 0
     && legacy_trainer_id <= COOP_HOENN_TRAINER_LEGACY_MAX)
    {
        const struct CoopIdentityRegistryEntry *entry =
            &gCoopTrainerIdentityRegistry[legacy_trainer_id - 1];

        if (entry->region == region
         && entry->legacy_value == legacy_trainer_id
         && entry->ordinal < COOP_TRAINER_IDENTITY_CAPACITY)
        {
            if (ordinal != NULL)
                *ordinal = entry->ordinal;
            return TRUE;
        }
        return FALSE;
    }

    for (i = 0; i < COOP_TRAINER_IDENTITY_COUNT; i++)
    {
        const struct CoopIdentityRegistryEntry *entry =
            &gCoopTrainerIdentityRegistry[i];

        if (entry->region == region
         && entry->legacy_value == legacy_trainer_id
         && entry->ordinal < COOP_TRAINER_IDENTITY_CAPACITY)
        {
            if (ordinal != NULL)
                *ordinal = entry->ordinal;
            return TRUE;
        }
    }
    return FALSE;
}

static enum CoopIdentityAccessResult ResolveActiveTrainer(u16 legacy_trainer_id,
                                                           u16 *ordinal)
{
    enum CoopRegion region;

    if (!CoopSave_IsOnlineEnabled())
        return COOP_IDENTITY_ACCESS_LEGACY;
    if (!CoopRegion_TryGetActive(&region)
     || !CoopIdentity_ResolveTrainerOrdinal(region, legacy_trainer_id, ordinal))
        return COOP_IDENTITY_ACCESS_REJECTED;
    return COOP_IDENTITY_ACCESS_HANDLED;
}

enum CoopIdentityAccessResult CoopIdentity_GetTrainerDefeated(u16 legacy_trainer_id,
                                                               bool8 *defeated)
{
    enum CoopIdentityAccessResult result;
    u16 ordinal;

    if (defeated == NULL)
        return COOP_IDENTITY_ACCESS_REJECTED;

    result = ResolveActiveTrainer(legacy_trainer_id, &ordinal);
    if (result == COOP_IDENTITY_ACCESS_HANDLED)
        *defeated = CoopSave_GetTrainerDefeated(ordinal);
    return result;
}

enum CoopIdentityAccessResult CoopIdentity_SetTrainerDefeated(u16 legacy_trainer_id,
                                                               bool8 defeated)
{
    enum CoopIdentityAccessResult result;
    u16 ordinal;

    result = ResolveActiveTrainer(legacy_trainer_id, &ordinal);
    if (result == COOP_IDENTITY_ACCESS_HANDLED
     && !CoopSave_SetTrainerDefeated(ordinal, defeated))
        return COOP_IDENTITY_ACCESS_REJECTED;
    return result;
}
