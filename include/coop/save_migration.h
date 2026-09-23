#ifndef GUARD_COOP_SAVE_MIGRATION_H
#define GUARD_COOP_SAVE_MIGRATION_H

#include "gba/types.h"

struct SaveBlock1;
struct PokemonStorage;

/* Explicit V1-to-V2 step for a launcher-provided save copy. Never call this
 * from normal save loading: legacy marker bits are only interpretable while
 * the source is known to be V1. Validates every persisted owned Pokemon before
 * changing any of them. The caller must seal V2 only after this succeeds. */
bool32 CoopSave_NormalizeLegacyPokemon(struct SaveBlock1 *saveBlock1,
                                      struct PokemonStorage *storage);

#endif /* GUARD_COOP_SAVE_MIGRATION_H */
