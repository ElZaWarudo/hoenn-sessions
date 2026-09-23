#ifndef GUARD_COOP_SAVE_MIGRATION_H
#define GUARD_COOP_SAVE_MIGRATION_H

#include "gba/types.h"

struct SaveBlock1;
struct PokemonStorage;
struct CoopSaveV1;

/* Explicit V1-to-V2 step for a launcher-provided save copy. Never call this
 * from normal save loading: legacy marker bits are only interpretable while
 * the source is known to be V1. Validates every persisted owned Pokemon before
 * changing any of them. The caller must seal V2 only after this succeeds. */
bool32 CoopSave_NormalizeLegacyPokemon(struct SaveBlock1 *saveBlock1,
                                      struct PokemonStorage *storage);

/* Upgrade the validated V1 record and all owned Pokemon in one isolated save
 * copy. The record is overwritten in place only after normalization succeeds;
 * a second call sees schema V2 and fails before touching Pokemon. The caller
 * must discard the copy on any failure and reseal its Flash1M sectors before
 * using it. */
bool32 CoopSaveV2_UpgradeCopiedV1(struct CoopSaveV1 *record,
                                  struct SaveBlock1 *saveBlock1,
                                  struct PokemonStorage *storage);

#endif /* GUARD_COOP_SAVE_MIGRATION_H */
