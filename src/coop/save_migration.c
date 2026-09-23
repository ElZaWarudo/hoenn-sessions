#include "global.h"
#include "coop/save.h"
#include "coop/save_migration.h"
#include "pokemon.h"
#include "pokemon_storage_system.h"

static bool32 NormalizeSlot(struct BoxPokemon *boxMon, bool32 apply)
{
    struct BoxPokemon checked;

    if (apply)
        return NormalizeLegacyBoxMonMetLocation(boxMon);

    checked = *boxMon;
    return NormalizeLegacyBoxMonMetLocation(&checked);
}

static bool32 VisitLegacyPokemon(struct SaveBlock1 *saveBlock1,
                                 struct PokemonStorage *storage,
                                 bool32 apply)
{
    u32 box;
    u32 slot;

    for (slot = 0; slot < PARTY_SIZE; slot++)
    {
        if (!NormalizeSlot(&saveBlock1->playerParty[slot].box, apply))
            return FALSE;
    }

    for (box = 0; box < TOTAL_BOXES_COUNT; box++)
    {
        for (slot = 0; slot < IN_BOX_COUNT; slot++)
        {
            if (!NormalizeSlot(&storage->boxes[box][slot], apply))
                return FALSE;
        }
    }

    for (slot = 0; slot < MAX_FUSION_STORAGE; slot++)
    {
        if (!NormalizeSlot(&storage->fusions[slot].box, apply))
            return FALSE;
    }

    for (slot = 0; slot < DAYCARE_MON_COUNT; slot++)
    {
        if (!NormalizeSlot(&saveBlock1->daycare.mons[slot].mon, apply))
            return FALSE;
    }

    if (!NormalizeSlot(&saveBlock1->route5DayCareMon.mon, apply))
        return FALSE;

    return TRUE;
}

bool32 CoopSave_NormalizeLegacyPokemon(struct SaveBlock1 *saveBlock1,
                                      struct PokemonStorage *storage)
{
    if (saveBlock1 == NULL || storage == NULL)
        return FALSE;

    /* The first pass never writes; a corrupt late slot cannot leave earlier
     * party or PC data partially normalized. Both passes run synchronously on
     * the same copied save, with no callbacks or external mutation. */
    return VisitLegacyPokemon(saveBlock1, storage, FALSE)
        && VisitLegacyPokemon(saveBlock1, storage, TRUE);
}

bool32 CoopSaveV2_UpgradeCopiedV1(struct CoopSaveV1 *record,
                                  struct SaveBlock1 *saveBlock1,
                                  struct PokemonStorage *storage)
{
    struct CoopSaveV2 upgraded;

    if (record == NULL || saveBlock1 == NULL || storage == NULL
     || !CoopSave_Validate(record))
        return FALSE;

    /* The V1 and V2 records have identical size and prefix offsets. V1
     * validation guarantees that its 64 reserved bytes are zero. Construct
     * the V2 record before mutating the copied Pokemon. */
    memcpy(&upgraded, record, sizeof(upgraded));
    upgraded.schema_version = COOP_SAVE_V2_SCHEMA_VERSION;
    upgraded.status_flags |= COOP_SAVE_STATUS_MET_LOCATION_NORMALIZED;
    upgraded.cormoria_progress.region = COOP_SAVE_V2_CORMORIA_REGION;
    upgraded.crc32 = CoopSaveV2_CalculateCrc(&upgraded);
    if (!CoopSaveV2_Validate(&upgraded))
        return FALSE;

    if (!CoopSave_NormalizeLegacyPokemon(saveBlock1, storage))
        return FALSE;

    memcpy(record, &upgraded, sizeof(upgraded));
    return TRUE;
}
