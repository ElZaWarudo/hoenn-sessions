#include "global.h"
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
