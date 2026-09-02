#include "global.h"
#include "coop/progress.h"
#include "coop/save.h"
#include "load_save.h"
#include "save.h"

#define COOP_SAVE_CRC_INITIAL 0xFFFFFFFFu
#define COOP_SAVE_CRC_POLYNOMIAL 0xEDB88320u

static const u8 sRegistryDigest[COOP_IDENTITY_REGISTRY_DIGEST_SIZE] =
    COOP_IDENTITY_REGISTRY_DIGEST_BYTES;
static bool8 sSaveLoadResolved;
static bool8 sOnlineEnabled;
static bool8 sSavePreparationSucceeded;

const struct CoopSaveSchemaDescriptor gCoopSaveSchemaDescriptor =
{
    .descriptor_magic = COOP_SAVE_DESCRIPTOR_MAGIC,
    .descriptor_version = COOP_SAVE_DESCRIPTOR_VERSION,
    .descriptor_size = sizeof(struct CoopSaveSchemaDescriptor),
    .save_magic = COOP_SAVE_MAGIC,
    .save_schema_version = COOP_SAVE_SCHEMA_VERSION,
    .save_struct_size = sizeof(struct CoopSaveV1),
    .save_block3_offset = COOP_SAVE_BLOCK3_OFFSET,
    .generation_offset = offsetof(struct CoopSaveV1, save_generation),
    .crc32_offset = offsetof(struct CoopSaveV1, crc32),
    .sector_data_size = SECTOR_DATA_SIZE,
    .save_block3_chunk_size = SAVE_BLOCK_3_CHUNK_SIZE,
    .sector_size = SECTOR_SIZE,
    .sectors_per_slot = NUM_SECTORS_PER_SLOT,
    .save_slot_count = NUM_SAVE_SLOTS,
    .registry_version = COOP_IDENTITY_REGISTRY_VERSION,
    .registry_digest = COOP_IDENTITY_REGISTRY_DIGEST_BYTES,
    .trainer_bits_offset = offsetof(struct CoopSaveV1, trainer_bits),
    .event_bits_offset = offsetof(struct CoopSaveV1, event_bits),
    .fly_bits_offset = offsetof(struct CoopSaveV1, fly_bits),
    .gym_bits_offset = offsetof(struct CoopSaveV1, gym_bits),
    .status_flags_offset = offsetof(struct CoopSaveV1, status_flags),
    .regional_progress_offset = offsetof(struct CoopSaveV1, regional_progress),
};

static enum CoopRegion GetRegionForSlot(u32 slot)
{
    static const enum CoopRegion sRegions[COOP_PROGRESS_REGION_COUNT] =
    {
        COOP_REGION_HOENN,
        COOP_REGION_KANTO,
        COOP_REGION_JOHTO,
        COOP_REGION_SEVII,
    };

    if (slot >= ARRAY_COUNT(sRegions))
        return COOP_REGION_UNSPECIFIED;
    return sRegions[slot];
}

static bool8 BytesHaveValue(const u8 *bytes, u32 length, u8 value)
{
    u32 i;

    for (i = 0; i < length; i++)
    {
        if (bytes[i] != value)
            return FALSE;
    }
    return TRUE;
}

static bool8 IsLegacyOrErased(const struct CoopSaveV1 *save)
{
    const u8 *bytes = (const u8 *)save;

    return BytesHaveValue(bytes, sizeof(*save), 0)
        || BytesHaveValue(bytes, sizeof(*save), 0xFF);
}

static bool8 HasCompatibleHeader(const struct CoopSaveV1 *save)
{
    return save != NULL
        && save->magic == COOP_SAVE_MAGIC
        && save->schema_version == COOP_SAVE_SCHEMA_VERSION
        && save->struct_size == sizeof(*save)
        && save->registry_version == COOP_IDENTITY_REGISTRY_VERSION
        && memcmp(save->registry_digest, sRegistryDigest, sizeof(sRegistryDigest)) == 0;
}

static bool8 IdentityOrdinalIsAssigned(const struct CoopIdentityRegistryEntry *entries,
                                       u32 entry_count, u16 ordinal)
{
    u32 lower = 0;
    u32 upper = entry_count;

    while (lower < upper)
    {
        u32 middle = lower + (upper - lower) / 2;

        if (entries[middle].ordinal < ordinal)
            lower = middle + 1;
        else
            upper = middle;
    }
    return lower < entry_count && entries[lower].ordinal == ordinal;
}

static bool8 BitsContainOnlyAssigned(const u8 *bits, u32 byte_count,
                                     const struct CoopIdentityRegistryEntry *entries,
                                     u32 entry_count)
{
    u32 byte;

    for (byte = 0; byte < byte_count; byte++)
    {
        u8 value = bits[byte];
        u8 bit;

        for (bit = 0; bit < 8; bit++)
        {
            if ((value & (1u << bit)) != 0
             && !IdentityOrdinalIsAssigned(entries, entry_count, byte * 8 + bit))
                return FALSE;
        }
    }
    return TRUE;
}

static u16 GetAssignedBadgeMask(enum CoopRegion region)
{
    u16 mask = 0;
    u32 i;

    for (i = 0; i < COOP_BADGE_IDENTITY_COUNT; i++)
    {
        const struct CoopIdentityRegistryEntry *entry = &gCoopBadgeIdentityRegistry[i];

        if (entry->region == region && entry->badge_bit != COOP_IDENTITY_BADGE_BIT_NONE)
            mask |= 1u << entry->badge_bit;
    }
    return mask;
}

static bool8 HasValidBody(const struct CoopSaveV1 *save)
{
    u32 i;

    if (!HasCompatibleHeader(save)
     || (save->status_flags & ~COOP_SAVE_STATUS_KNOWN_MASK) != 0)
        return FALSE;

    for (i = 0; i < ARRAY_COUNT(save->regional_progress); i++)
    {
        if (save->regional_progress[i].region != GetRegionForSlot(i)
         || save->regional_progress[i].reserved != 0
         || (save->regional_progress[i].badge_mask & ~GetAssignedBadgeMask(GetRegionForSlot(i))) != 0)
            return FALSE;
    }

    return BytesHaveValue(save->reserved, sizeof(save->reserved), 0)
        && BitsContainOnlyAssigned(save->trainer_bits, sizeof(save->trainer_bits),
                                   gCoopTrainerIdentityRegistry, COOP_TRAINER_IDENTITY_COUNT)
        && BitsContainOnlyAssigned(save->event_bits, sizeof(save->event_bits),
                                   gCoopEventIdentityRegistry, COOP_EVENT_IDENTITY_COUNT)
        && BitsContainOnlyAssigned(save->fly_bits, sizeof(save->fly_bits),
                                   gCoopFlyPointIdentityRegistry, COOP_FLY_IDENTITY_COUNT)
        && BitsContainOnlyAssigned(save->gym_bits, sizeof(save->gym_bits),
                                   gCoopGymIdentityRegistry, COOP_GYM_IDENTITY_COUNT);
}

u32 CoopSave_Crc32(const void *data, u32 length)
{
    const u8 *bytes = data;
    u32 crc = COOP_SAVE_CRC_INITIAL;
    u32 i;

    if (data == NULL && length != 0)
        return 0;

    for (i = 0; i < length; i++)
    {
        u32 bit;

        crc ^= bytes[i];
        for (bit = 0; bit < 8; bit++)
        {
            if ((crc & 1) != 0)
                crc = (crc >> 1) ^ COOP_SAVE_CRC_POLYNOMIAL;
            else
                crc >>= 1;
        }
    }
    return ~crc;
}

u32 CoopSave_CalculateCrc(const struct CoopSaveV1 *save)
{
    if (save == NULL)
        return 0;
    return CoopSave_Crc32(save, offsetof(struct CoopSaveV1, crc32));
}

bool8 CoopSave_Seal(struct CoopSaveV1 *save)
{
    if (!HasValidBody(save))
    {
        if (gSaveBlock3Ptr != NULL && save == &gSaveBlock3Ptr->coop)
            sOnlineEnabled = FALSE;
        return FALSE;
    }

    save->crc32 = CoopSave_CalculateCrc(save);
    if (gSaveBlock3Ptr != NULL && save == &gSaveBlock3Ptr->coop)
        sOnlineEnabled = (save->status_flags & COOP_SAVE_STATUS_MIGRATION_AMBIGUOUS) == 0;
    return TRUE;
}

bool8 CoopSave_Validate(const struct CoopSaveV1 *save)
{
    return HasValidBody(save)
        && save->crc32 == CoopSave_CalculateCrc(save);
}

void CoopSave_Initialize(struct CoopSaveV1 *save)
{
    u32 i;

    if (save == NULL)
        return;

    memset(save, 0, sizeof(*save));
    save->magic = COOP_SAVE_MAGIC;
    save->schema_version = COOP_SAVE_SCHEMA_VERSION;
    save->struct_size = sizeof(*save);
    save->registry_version = COOP_IDENTITY_REGISTRY_VERSION;
    memcpy(save->registry_digest, sRegistryDigest, sizeof(sRegistryDigest));
    for (i = 0; i < ARRAY_COUNT(save->regional_progress); i++)
        save->regional_progress[i].region = GetRegionForSlot(i);
    (void)CoopSave_Seal(save);
}

static void CopyProgressFromSave(const struct CoopSaveV1 *save)
{
    u32 i;

    CoopProgress_Init(&gCoopProgress);
    for (i = 0; i < ARRAY_COUNT(save->regional_progress); i++)
    {
        struct RegionalProgress *progress =
            CoopProgress_GetRegion(&gCoopProgress, GetRegionForSlot(i));

        if (progress != NULL)
        {
            progress->badge_mask = save->regional_progress[i].badge_mask;
            progress->story_checkpoint = save->regional_progress[i].story_checkpoint;
        }
    }
}

static bool8 RuntimeProgressCanBeSaved(void)
{
    u32 i;

    for (i = 0; i < COOP_PROGRESS_REGION_COUNT; i++)
    {
        const struct RegionalProgress *progress =
            CoopProgress_GetRegionConst(&gCoopProgress, GetRegionForSlot(i));

        if (progress == NULL
         || progress->reserved != 0
         || (progress->badge_mask & ~GetAssignedBadgeMask(GetRegionForSlot(i))) != 0)
            return FALSE;
    }
    return TRUE;
}

static void InitializeCurrentWithStatus(u32 status_flags)
{
    sOnlineEnabled = FALSE;
    sSavePreparationSucceeded = FALSE;
    if (gSaveBlock3Ptr == NULL)
        return;

    CoopProgress_Init(&gCoopProgress);
    CoopSave_Initialize(&gSaveBlock3Ptr->coop);
    gSaveBlock3Ptr->coop.status_flags = status_flags;
    (void)CoopSave_Seal(&gSaveBlock3Ptr->coop);
    sSaveLoadResolved = TRUE;
}

void CoopSave_InitializeCurrent(void)
{
    InitializeCurrentWithStatus(0);
}

enum CoopSaveLoadResult CoopSave_Load(void)
{
    struct CoopSaveV1 *save;

    sOnlineEnabled = FALSE;
    sSavePreparationSucceeded = FALSE;
    if (gSaveBlock3Ptr == NULL)
    {
        CoopProgress_Init(&gCoopProgress);
        return COOP_SAVE_LOAD_CORRUPT;
    }

    save = &gSaveBlock3Ptr->coop;
    if (IsLegacyOrErased(save))
    {
        /* A pre-schema save may already contain trainer, event, and badge
         * progress whose legacy identifiers collide across regions. Keep
         * vanilla play available, but never claim a lossless migration. */
        InitializeCurrentWithStatus(COOP_SAVE_STATUS_MIGRATION_AMBIGUOUS);
        return COOP_SAVE_LOAD_INITIALIZED_LEGACY;
    }

    if (CoopSave_Validate(save))
    {
        CopyProgressFromSave(save);
        sSaveLoadResolved = TRUE;
        sOnlineEnabled = (save->status_flags & COOP_SAVE_STATUS_MIGRATION_AMBIGUOUS) == 0;
        return COOP_SAVE_LOAD_READY;
    }

    /* Do not let a previous slot's runtime progress survive a failed load.
     * The incompatible bytes remain in SaveBlock3 for offline recovery, but
     * no stale progress may be presented to the bridge or gameplay. */
    CoopProgress_Init(&gCoopProgress);
    sSaveLoadResolved = TRUE;
    if (!HasCompatibleHeader(save))
        return COOP_SAVE_LOAD_INCOMPATIBLE;
    return COOP_SAVE_LOAD_CORRUPT;
}

bool8 CoopSave_LoadRuntimeProgress(void)
{
    if (!sSaveLoadResolved
     || gSaveBlock3Ptr == NULL
     || !CoopSave_Validate(&gSaveBlock3Ptr->coop))
    {
        sOnlineEnabled = FALSE;
        CoopProgress_Init(&gCoopProgress);
        return FALSE;
    }

    CopyProgressFromSave(&gSaveBlock3Ptr->coop);
    sOnlineEnabled = (gSaveBlock3Ptr->coop.status_flags & COOP_SAVE_STATUS_MIGRATION_AMBIGUOUS) == 0;
    return TRUE;
}

bool8 CoopSave_PrepareForWrite(void)
{
    struct CoopSaveV1 *save;
    u32 i;

    sSavePreparationSucceeded = FALSE;
    if (gSaveBlock3Ptr == NULL)
    {
        sOnlineEnabled = FALSE;
        return FALSE;
    }
    if (!RuntimeProgressCanBeSaved())
        return FALSE;

    save = &gSaveBlock3Ptr->coop;
    if (!CoopSave_Validate(save))
    {
        sOnlineEnabled = FALSE;
        return FALSE;
    }

    for (i = 0; i < ARRAY_COUNT(save->regional_progress); i++)
    {
        const struct RegionalProgress *progress =
            CoopProgress_GetRegionConst(&gCoopProgress, GetRegionForSlot(i));

        save->regional_progress[i].badge_mask = progress->badge_mask;
        save->regional_progress[i].story_checkpoint = progress->story_checkpoint;
    }
    save->save_generation++;
    sSavePreparationSucceeded = CoopSave_Seal(save);
    return sSavePreparationSucceeded;
}

bool8 CoopSave_WasPreparedForWrite(void)
{
    return sSavePreparationSucceeded;
}

bool8 CoopSave_IsOnlineEnabled(void)
{
    return sSaveLoadResolved && gSaveBlock3Ptr != NULL && sOnlineEnabled;
}

u32 CoopSave_GetGeneration(void)
{
    if (gSaveBlock3Ptr == NULL || !CoopSave_Validate(&gSaveBlock3Ptr->coop))
        return 0;
    return gSaveBlock3Ptr->coop.save_generation;
}

static bool8 GetPersistedBit(const u8 *bits, u32 byte_count,
                             const struct CoopIdentityRegistryEntry *entries,
                             u32 entry_count, u16 ordinal)
{
    if (gSaveBlock3Ptr == NULL
     || bits == NULL
     || entries == NULL
     || !CoopSave_IsOnlineEnabled()
     || ordinal >= byte_count * 8
     || !IdentityOrdinalIsAssigned(entries, entry_count, ordinal))
        return FALSE;

    return (bits[ordinal / 8] & (1u << (ordinal % 8))) != 0;
}

static bool8 SetPersistedBit(u8 *bits, u32 byte_count,
                             const struct CoopIdentityRegistryEntry *entries,
                             u32 entry_count, u16 ordinal, bool8 value)
{
    u8 mask;

    if (gSaveBlock3Ptr == NULL
     || bits == NULL
     || entries == NULL
     || !CoopSave_Validate(&gSaveBlock3Ptr->coop)
     || ordinal >= byte_count * 8
     || !IdentityOrdinalIsAssigned(entries, entry_count, ordinal))
        return FALSE;

    mask = 1u << (ordinal % 8);
    if (value)
        bits[ordinal / 8] |= mask;
    else
        bits[ordinal / 8] &= ~mask;
    return CoopSave_Seal(&gSaveBlock3Ptr->coop);
}

bool8 CoopSave_GetTrainerDefeated(u16 ordinal)
{
    return GetPersistedBit(gSaveBlock3Ptr == NULL ? NULL : gSaveBlock3Ptr->coop.trainer_bits,
                           COOP_SAVE_TRAINER_BITS_SIZE, gCoopTrainerIdentityRegistry,
                           COOP_TRAINER_IDENTITY_COUNT, ordinal);
}

bool8 CoopSave_SetTrainerDefeated(u16 ordinal, bool8 defeated)
{
    return SetPersistedBit(gSaveBlock3Ptr == NULL ? NULL : gSaveBlock3Ptr->coop.trainer_bits,
                           COOP_SAVE_TRAINER_BITS_SIZE, gCoopTrainerIdentityRegistry,
                           COOP_TRAINER_IDENTITY_COUNT, ordinal, defeated);
}

bool8 CoopSave_GetEventCompleted(u16 ordinal)
{
    return GetPersistedBit(gSaveBlock3Ptr == NULL ? NULL : gSaveBlock3Ptr->coop.event_bits,
                           COOP_SAVE_EVENT_BITS_SIZE, gCoopEventIdentityRegistry,
                           COOP_EVENT_IDENTITY_COUNT, ordinal);
}

bool8 CoopSave_SetEventCompleted(u16 ordinal, bool8 completed)
{
    return SetPersistedBit(gSaveBlock3Ptr == NULL ? NULL : gSaveBlock3Ptr->coop.event_bits,
                           COOP_SAVE_EVENT_BITS_SIZE, gCoopEventIdentityRegistry,
                           COOP_EVENT_IDENTITY_COUNT, ordinal, completed);
}

bool8 CoopSave_GetFlyPointUnlocked(u16 ordinal)
{
    return GetPersistedBit(gSaveBlock3Ptr == NULL ? NULL : gSaveBlock3Ptr->coop.fly_bits,
                           COOP_SAVE_FLY_BITS_SIZE, gCoopFlyPointIdentityRegistry,
                           COOP_FLY_IDENTITY_COUNT, ordinal);
}

bool8 CoopSave_SetFlyPointUnlocked(u16 ordinal, bool8 unlocked)
{
    return SetPersistedBit(gSaveBlock3Ptr == NULL ? NULL : gSaveBlock3Ptr->coop.fly_bits,
                           COOP_SAVE_FLY_BITS_SIZE, gCoopFlyPointIdentityRegistry,
                           COOP_FLY_IDENTITY_COUNT, ordinal, unlocked);
}

bool8 CoopSave_GetGymDefeated(u16 ordinal)
{
    return GetPersistedBit(gSaveBlock3Ptr == NULL ? NULL : gSaveBlock3Ptr->coop.gym_bits,
                           COOP_SAVE_GYM_BITS_SIZE, gCoopGymIdentityRegistry,
                           COOP_GYM_IDENTITY_COUNT, ordinal);
}

bool8 CoopSave_SetGymDefeated(u16 ordinal, bool8 defeated)
{
    return SetPersistedBit(gSaveBlock3Ptr == NULL ? NULL : gSaveBlock3Ptr->coop.gym_bits,
                           COOP_SAVE_GYM_BITS_SIZE, gCoopGymIdentityRegistry,
                           COOP_GYM_IDENTITY_COUNT, ordinal, defeated);
}
