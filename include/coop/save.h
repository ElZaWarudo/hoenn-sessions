#ifndef GUARD_COOP_SAVE_H
#define GUARD_COOP_SAVE_H

#include <stddef.h>

#include "gba/defines.h"
#include "gba/types.h"
#include "coop/generated_regional_identities.h"

#define COOP_SAVE_MAGIC 0x31505343u /* little-endian ASCII "CSP1" */
#define COOP_SAVE_SCHEMA_VERSION 1
#define COOP_SAVE_V1_SIZE 672
#define COOP_SAVE_BLOCK3_OFFSET 4

#define COOP_SAVE_TRAINER_BITS_SIZE 256
#define COOP_SAVE_EVENT_BITS_SIZE 256
#define COOP_SAVE_FLY_BITS_SIZE 16
#define COOP_SAVE_GYM_BITS_SIZE 8
#define COOP_SAVE_RESERVED_SIZE 64

#define COOP_SAVE_STATUS_MIGRATION_AMBIGUOUS (1u << 0)
#define COOP_SAVE_STATUS_KNOWN_MASK COOP_SAVE_STATUS_MIGRATION_AMBIGUOUS

#define COOP_SAVE_DESCRIPTOR_MAGIC 0x31445343u /* little-endian ASCII "CSD1" */
#define COOP_SAVE_DESCRIPTOR_VERSION 1

struct CoopSaveRegionalProgress
{
    /* 0x00 */ u8 region;
    /* 0x01 */ u8 reserved;
    /* 0x02 */ u16 badge_mask;
    /* 0x04 */ u32 story_checkpoint;
};

struct CoopSaveV1
{
    /* 0x000 */ u32 magic;
    /* 0x004 */ u16 schema_version;
    /* 0x006 */ u16 struct_size;
    /* 0x008 */ u32 registry_version;
    /* 0x00C */ u8 registry_digest[COOP_IDENTITY_REGISTRY_DIGEST_SIZE];
    /* 0x01C */ u32 save_generation;
    /* 0x020 */ u32 status_flags;
    /* 0x024 */ struct CoopSaveRegionalProgress regional_progress[4];
    /* 0x044 */ u8 trainer_bits[COOP_SAVE_TRAINER_BITS_SIZE];
    /* 0x144 */ u8 event_bits[COOP_SAVE_EVENT_BITS_SIZE];
    /* 0x244 */ u8 fly_bits[COOP_SAVE_FLY_BITS_SIZE];
    /* 0x254 */ u8 gym_bits[COOP_SAVE_GYM_BITS_SIZE];
    /* 0x25C */ u8 reserved[COOP_SAVE_RESERVED_SIZE];
    /* 0x29C */ u32 crc32;
};

/* ROM-resident contract read by the bridge-manifest generator. */
struct CoopSaveSchemaDescriptor
{
    /* 0x00 */ u32 descriptor_magic;
    /* 0x04 */ u16 descriptor_version;
    /* 0x06 */ u16 descriptor_size;
    /* 0x08 */ u32 save_magic;
    /* 0x0C */ u16 save_schema_version;
    /* 0x0E */ u16 save_struct_size;
    /* 0x10 */ u16 save_block3_offset;
    /* 0x12 */ u16 generation_offset;
    /* 0x14 */ u16 crc32_offset;
    /* 0x16 */ u16 sector_data_size;
    /* 0x18 */ u16 save_block3_chunk_size;
    /* 0x1A */ u16 sector_size;
    /* 0x1C */ u16 sectors_per_slot;
    /* 0x1E */ u16 save_slot_count;
    /* 0x20 */ u32 registry_version;
    /* 0x24 */ u8 registry_digest[COOP_IDENTITY_REGISTRY_DIGEST_SIZE];
    /* 0x34 */ u16 trainer_bits_offset;
    /* 0x36 */ u16 event_bits_offset;
    /* 0x38 */ u16 fly_bits_offset;
    /* 0x3A */ u16 gym_bits_offset;
    /* 0x3C */ u16 status_flags_offset;
    /* 0x3E */ u16 regional_progress_offset;
};

enum CoopSaveLoadResult
{
    COOP_SAVE_LOAD_READY,
    COOP_SAVE_LOAD_INITIALIZED_LEGACY,
    COOP_SAVE_LOAD_CORRUPT,
    COOP_SAVE_LOAD_INCOMPATIBLE,
};

_Static_assert(COOP_IDENTITY_REGISTRY_DIGEST_SIZE == 16, "registry digest ABI size");
_Static_assert(sizeof(struct CoopSaveRegionalProgress) == 8, "saved regional progress ABI size");
_Static_assert(offsetof(struct CoopSaveRegionalProgress, region) == 0, "saved region offset");
_Static_assert(offsetof(struct CoopSaveRegionalProgress, badge_mask) == 2, "saved badge offset");
_Static_assert(offsetof(struct CoopSaveRegionalProgress, story_checkpoint) == 4, "saved story offset");

_Static_assert(sizeof(struct CoopSaveV1) == COOP_SAVE_V1_SIZE, "CoopSaveV1 ABI size");
_Static_assert(sizeof(((struct CoopSaveV1 *)0)->trainer_bits) * 8 == COOP_TRAINER_IDENTITY_CAPACITY, "trainer bit capacity");
_Static_assert(sizeof(((struct CoopSaveV1 *)0)->event_bits) * 8 == COOP_EVENT_IDENTITY_CAPACITY, "event bit capacity");
_Static_assert(sizeof(((struct CoopSaveV1 *)0)->fly_bits) * 8 == COOP_FLY_POINT_IDENTITY_CAPACITY, "fly bit capacity");
_Static_assert(sizeof(((struct CoopSaveV1 *)0)->gym_bits) * 8 == COOP_GYM_IDENTITY_CAPACITY, "gym bit capacity");
_Static_assert(offsetof(struct CoopSaveV1, registry_digest) == 12, "registry digest offset");
_Static_assert(offsetof(struct CoopSaveV1, save_generation) == 28, "save generation offset");
_Static_assert(offsetof(struct CoopSaveV1, status_flags) == 32, "save status offset");
_Static_assert(offsetof(struct CoopSaveV1, regional_progress) == 36, "regional progress offset");
_Static_assert(offsetof(struct CoopSaveV1, trainer_bits) == 68, "trainer bits offset");
_Static_assert(offsetof(struct CoopSaveV1, event_bits) == 324, "event bits offset");
_Static_assert(offsetof(struct CoopSaveV1, fly_bits) == 580, "fly bits offset");
_Static_assert(offsetof(struct CoopSaveV1, gym_bits) == 596, "gym bits offset");
_Static_assert(offsetof(struct CoopSaveV1, reserved) == 604, "reserved bytes offset");
_Static_assert(offsetof(struct CoopSaveV1, crc32) == 668, "save CRC offset");

_Static_assert(sizeof(struct CoopSaveSchemaDescriptor) == 64, "save descriptor ABI size");
_Static_assert(offsetof(struct CoopSaveSchemaDescriptor, registry_version) == 32, "descriptor registry offset");
_Static_assert(offsetof(struct CoopSaveSchemaDescriptor, registry_digest) == 36, "descriptor digest offset");
_Static_assert(offsetof(struct CoopSaveSchemaDescriptor, regional_progress_offset) == 62, "descriptor tail offset");

extern const struct CoopSaveSchemaDescriptor gCoopSaveSchemaDescriptor;

void CoopSave_Initialize(struct CoopSaveV1 *save);
void CoopSave_InitializeCurrent(void);
enum CoopSaveLoadResult CoopSave_Load(void);
bool8 CoopSave_Validate(const struct CoopSaveV1 *save);
bool8 CoopSave_Seal(struct CoopSaveV1 *save);
bool8 CoopSave_PrepareForWrite(void);
/* True only when the most recent canonical-save preparation succeeded.  The
 * flash writer uses this to abort cloud saves before touching any sector. */
bool8 CoopSave_WasPreparedForWrite(void);
bool8 CoopSave_LoadRuntimeProgress(void);
bool8 CoopSave_IsOnlineEnabled(void);
u32 CoopSave_GetGeneration(void);
bool8 CoopSave_GetTrainerDefeated(u16 ordinal);
bool8 CoopSave_SetTrainerDefeated(u16 ordinal, bool8 defeated);
bool8 CoopSave_GetEventCompleted(u16 ordinal);
bool8 CoopSave_SetEventCompleted(u16 ordinal, bool8 completed);
bool8 CoopSave_GetFlyPointUnlocked(u16 ordinal);
bool8 CoopSave_SetFlyPointUnlocked(u16 ordinal, bool8 unlocked);
bool8 CoopSave_GetGymDefeated(u16 ordinal);
bool8 CoopSave_SetGymDefeated(u16 ordinal, bool8 defeated);
u32 CoopSave_Crc32(const void *data, u32 length);
u32 CoopSave_CalculateCrc(const struct CoopSaveV1 *save);

#endif /* GUARD_COOP_SAVE_H */
