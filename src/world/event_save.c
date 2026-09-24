#include "global.h"
#include "coop/save.h"
#include "load_save.h"
#include "world/event_save.h"

static struct WorldEventSaveV1 *GetCurrentSave(void)
{
    return &gSaveblock1.world_event;
}

static bool8 HasCompatibleHeader(const struct WorldEventSaveV1 *save)
{
    return save != NULL
        && save->magic == WORLD_EVENT_SAVE_MAGIC
        && save->schema_version == WORLD_EVENT_SAVE_SCHEMA_VERSION
        && save->struct_size == sizeof(*save);
}

bool8 WorldEventSave_IsEmpty(const struct WorldEventSaveV1 *save)
{
    const u8 *bytes = (const u8 *)save;
    u32 i;

    if (save == NULL)
        return FALSE;
    for (i = 0; i < sizeof(*save); i++)
    {
        if (bytes[i] != 0)
            return FALSE;
    }
    return TRUE;
}

static u32 CalculateCrc(const struct WorldEventSaveV1 *save)
{
    return CoopSave_Crc32(save, offsetof(struct WorldEventSaveV1, crc32));
}

bool8 WorldEventSave_Seal(struct WorldEventSaveV1 *save)
{
    if (!HasCompatibleHeader(save))
        return FALSE;
    save->crc32 = CalculateCrc(save);
    return TRUE;
}

bool8 WorldEventSave_Validate(const struct WorldEventSaveV1 *save)
{
    return HasCompatibleHeader(save) && save->crc32 == CalculateCrc(save);
}

void WorldEventSave_Initialize(struct WorldEventSaveV1 *save)
{
    if (save == NULL)
        return;
    memset(save, 0, sizeof(*save));
    save->magic = WORLD_EVENT_SAVE_MAGIC;
    save->schema_version = WORLD_EVENT_SAVE_SCHEMA_VERSION;
    save->struct_size = sizeof(*save);
    (void)WorldEventSave_Seal(save);
}

void WorldEventSave_InitializeCurrent(void)
{
    WorldEventSave_Initialize(GetCurrentSave());
}

enum WorldEventSaveLoadResult WorldEventSave_Load(void)
{
    struct WorldEventSaveV1 *save = GetCurrentSave();

    if (WorldEventSave_IsEmpty(save))
    {
        WorldEventSave_Initialize(save);
        return WORLD_EVENT_SAVE_LOAD_INITIALIZED_EMPTY;
    }
    if (!HasCompatibleHeader(save))
        return WORLD_EVENT_SAVE_LOAD_INCOMPATIBLE;
    if (!WorldEventSave_Validate(save))
        return WORLD_EVENT_SAVE_LOAD_CORRUPT;
    return WORLD_EVENT_SAVE_LOAD_READY;
}

bool8 WorldEventSave_PrepareForWrite(void)
{
    struct WorldEventSaveV1 *save = GetCurrentSave();

    if (WorldEventSave_IsEmpty(save))
        WorldEventSave_Initialize(save);
    return WorldEventSave_Validate(save) && WorldEventSave_Seal(save);
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

bool8 WorldEventSave_GetFlag(u16 ordinal)
{
    struct WorldEventSaveV1 *save = GetCurrentSave();

    if (!WorldEventSave_Validate(save) || ordinal >= WORLD_EVENT_SAVE_FLAG_BITS_SIZE * 8)
        return FALSE;
    return GetBit(save->flag_bits, ordinal);
}

bool8 WorldEventSave_SetFlag(u16 ordinal, bool8 value)
{
    struct WorldEventSaveV1 *save = GetCurrentSave();

    if (!WorldEventSave_Validate(save) || ordinal >= WORLD_EVENT_SAVE_FLAG_BITS_SIZE * 8)
        return FALSE;
    SetBit(save->flag_bits, ordinal, value);
    return WorldEventSave_Seal(save);
}

bool8 WorldEventSave_GetTrainerDefeated(u16 ordinal)
{
    struct WorldEventSaveV1 *save = GetCurrentSave();

    if (!WorldEventSave_Validate(save) || ordinal >= WORLD_EVENT_SAVE_TRAINER_BITS_SIZE * 8)
        return FALSE;
    return GetBit(save->trainer_bits, ordinal);
}

bool8 WorldEventSave_SetTrainerDefeated(u16 ordinal, bool8 value)
{
    struct WorldEventSaveV1 *save = GetCurrentSave();

    if (!WorldEventSave_Validate(save) || ordinal >= WORLD_EVENT_SAVE_TRAINER_BITS_SIZE * 8)
        return FALSE;
    SetBit(save->trainer_bits, ordinal, value);
    return WorldEventSave_Seal(save);
}

u16 WorldEventSave_GetVariable(u16 ordinal)
{
    struct WorldEventSaveV1 *save = GetCurrentSave();

    if (!WorldEventSave_Validate(save) || ordinal >= WORLD_EVENT_SAVE_VARIABLE_COUNT)
        return 0;
    return save->variables[ordinal];
}

bool8 WorldEventSave_SetVariable(u16 ordinal, u16 value)
{
    struct WorldEventSaveV1 *save = GetCurrentSave();

    if (!WorldEventSave_Validate(save) || ordinal >= WORLD_EVENT_SAVE_VARIABLE_COUNT)
        return FALSE;
    save->variables[ordinal] = value;
    return WorldEventSave_Seal(save);
}
