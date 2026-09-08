#include "global.h"
#include "johto/save.h"
#include "load_save.h"

#define JOHTO_SAVE_CRC_INITIAL 0xFFFFFFFFu
#define JOHTO_SAVE_CRC_POLYNOMIAL 0xEDB88320u

static const u8 sJohtoDefaultRivalName[] = _("SILVER");

static struct JohtoSaveV1 *GetCurrentSave(void)
{
    return &gSaveblock1.johto;
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

static bool8 IsLegacyOrErased(const struct JohtoSaveV1 *save)
{
    return BytesHaveValue((const u8 *)save, sizeof(*save), 0);
}

static bool8 HasCompatibleHeader(const struct JohtoSaveV1 *save)
{
    return save != NULL
        && save->magic == JOHTO_SAVE_MAGIC
        && save->schema_version == JOHTO_SAVE_SCHEMA_VERSION
        && save->struct_size == sizeof(*save);
}

u32 JohtoSave_Crc32(const void *data, u32 length)
{
    const u8 *bytes = data;
    u32 crc = JOHTO_SAVE_CRC_INITIAL;
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
                crc = (crc >> 1) ^ JOHTO_SAVE_CRC_POLYNOMIAL;
            else
                crc >>= 1;
        }
    }
    return ~crc;
}

u32 JohtoSave_CalculateCrc(const struct JohtoSaveV1 *save)
{
    if (save == NULL)
        return 0;
    return JohtoSave_Crc32(save, offsetof(struct JohtoSaveV1, crc32));
}

bool8 JohtoSave_Seal(struct JohtoSaveV1 *save)
{
    if (!HasCompatibleHeader(save))
        return FALSE;

    save->crc32 = JohtoSave_CalculateCrc(save);
    return TRUE;
}

bool8 JohtoSave_Validate(const struct JohtoSaveV1 *save)
{
    return HasCompatibleHeader(save)
        && save->crc32 == JohtoSave_CalculateCrc(save);
}

void JohtoSave_Initialize(struct JohtoSaveV1 *save)
{
    if (save == NULL)
        return;

    memset(save, 0, sizeof(*save));
    save->magic = JOHTO_SAVE_MAGIC;
    save->schema_version = JOHTO_SAVE_SCHEMA_VERSION;
    save->struct_size = sizeof(*save);
    memcpy(save->rival_name, sJohtoDefaultRivalName, sizeof(sJohtoDefaultRivalName));
    (void)JohtoSave_Seal(save);
}

void JohtoSave_InitializeCurrent(void)
{
    JohtoSave_Initialize(GetCurrentSave());
}

enum JohtoSaveLoadResult JohtoSave_Load(void)
{
    struct JohtoSaveV1 *save = GetCurrentSave();

    if (save == NULL)
        return JOHTO_SAVE_LOAD_CORRUPT;

    if (IsLegacyOrErased(save))
    {
        JohtoSave_Initialize(save);
        return JOHTO_SAVE_LOAD_INITIALIZED_LEGACY;
    }

    if (!HasCompatibleHeader(save))
        return JOHTO_SAVE_LOAD_INCOMPATIBLE;
    if (!JohtoSave_Validate(save))
        return JOHTO_SAVE_LOAD_CORRUPT;
    return JOHTO_SAVE_LOAD_READY;
}

bool8 JohtoSave_PrepareForWrite(void)
{
    struct JohtoSaveV1 *save = GetCurrentSave();

    /* Setters reseal valid records immediately.  Requiring a valid CRC here
     * keeps an unknown header or corrupt body byte-for-byte recoverable while
     * still making every canonical write carry an integrity-protected record. */
    return JohtoSave_Validate(save) && JohtoSave_Seal(save);
}

static bool8 GetBit(const u8 *bits, u16 ordinal)
{
    return (bits[ordinal / 8] & (1u << (ordinal % 8))) != 0;
}

static void SetBit(u8 *bits, u16 ordinal, bool8 value)
{
    u8 mask = 1u << (ordinal % 8);

    if (value)
        bits[ordinal / 8] |= mask;
    else
        bits[ordinal / 8] &= ~mask;
}

bool8 JohtoSave_GetFlag(u16 ordinal)
{
    struct JohtoSaveV1 *save = GetCurrentSave();

    if (!JohtoSave_Validate(save) || ordinal >= JOHTO_SAVE_FLAG_BITS_SIZE * 8)
        return FALSE;
    return GetBit(save->flag_bits, ordinal);
}

bool8 JohtoSave_SetFlag(u16 ordinal, bool8 value)
{
    struct JohtoSaveV1 *save = GetCurrentSave();

    if (!JohtoSave_Validate(save) || ordinal >= JOHTO_SAVE_FLAG_BITS_SIZE * 8)
        return FALSE;
    SetBit(save->flag_bits, ordinal, value);
    return JohtoSave_Seal(save);
}

bool8 JohtoSave_GetTrainerDefeated(u16 ordinal)
{
    struct JohtoSaveV1 *save = GetCurrentSave();

    if (!JohtoSave_Validate(save) || ordinal >= JOHTO_SAVE_TRAINER_BITS_SIZE * 8)
        return FALSE;
    return GetBit(save->trainer_bits, ordinal);
}

bool8 JohtoSave_SetTrainerDefeated(u16 ordinal, bool8 defeated)
{
    struct JohtoSaveV1 *save = GetCurrentSave();

    if (!JohtoSave_Validate(save) || ordinal >= JOHTO_SAVE_TRAINER_BITS_SIZE * 8)
        return FALSE;
    SetBit(save->trainer_bits, ordinal, defeated);
    return JohtoSave_Seal(save);
}

u16 JohtoSave_GetVariable(u16 ordinal)
{
    struct JohtoSaveV1 *save = GetCurrentSave();

    if (!JohtoSave_Validate(save) || ordinal >= JOHTO_SAVE_VARIABLE_COUNT)
        return 0;
    return save->variables[ordinal];
}

bool8 JohtoSave_SetVariable(u16 ordinal, u16 value)
{
    struct JohtoSaveV1 *save = GetCurrentSave();

    if (!JohtoSave_Validate(save) || ordinal >= JOHTO_SAVE_VARIABLE_COUNT)
        return FALSE;
    save->variables[ordinal] = value;
    return JohtoSave_Seal(save);
}
