#ifndef GUARD_WORLD_EVENT_SAVE_H
#define GUARD_WORLD_EVENT_SAVE_H

#include <stddef.h>

#include "gba/types.h"

/* One record belongs to the currently mounted ROM's regional save image.
 * The launcher indexes those images by stable world ID; no other world's
 * event state is copied into this record during travel. */
#define WORLD_EVENT_SAVE_MAGIC 0x31534557u /* little-endian "WES1" */
#define WORLD_EVENT_SAVE_SCHEMA_VERSION 1
#define WORLD_EVENT_SAVE_FLAG_BITS_SIZE 512
#define WORLD_EVENT_SAVE_TRAINER_BITS_SIZE 512
#define WORLD_EVENT_SAVE_VARIABLE_COUNT 256
#define WORLD_EVENT_SAVE_V1_SIZE 0x60C

struct WorldEventSaveV1
{
    u32 magic;
    u16 schema_version;
    u16 struct_size;
    u8 flag_bits[WORLD_EVENT_SAVE_FLAG_BITS_SIZE];
    u8 trainer_bits[WORLD_EVENT_SAVE_TRAINER_BITS_SIZE];
    u16 variables[WORLD_EVENT_SAVE_VARIABLE_COUNT];
    u32 crc32;
};

enum WorldEventSaveLoadResult
{
    WORLD_EVENT_SAVE_LOAD_READY,
    WORLD_EVENT_SAVE_LOAD_INITIALIZED_EMPTY,
    WORLD_EVENT_SAVE_LOAD_CORRUPT,
    WORLD_EVENT_SAVE_LOAD_INCOMPATIBLE,
};

_Static_assert(sizeof(struct WorldEventSaveV1) == WORLD_EVENT_SAVE_V1_SIZE,
               "world event save ABI size");
_Static_assert(offsetof(struct WorldEventSaveV1, flag_bits) == 0x008,
               "world flag bits offset");
_Static_assert(offsetof(struct WorldEventSaveV1, trainer_bits) == 0x208,
               "world trainer bits offset");
_Static_assert(offsetof(struct WorldEventSaveV1, variables) == 0x408,
               "world variables offset");
_Static_assert(offsetof(struct WorldEventSaveV1, crc32) == 0x608,
               "world event CRC offset");

void WorldEventSave_Initialize(struct WorldEventSaveV1 *save);
void WorldEventSave_InitializeCurrent(void);
enum WorldEventSaveLoadResult WorldEventSave_Load(void);
bool8 WorldEventSave_Validate(const struct WorldEventSaveV1 *save);
bool8 WorldEventSave_IsEmpty(const struct WorldEventSaveV1 *save);
bool8 WorldEventSave_Seal(struct WorldEventSaveV1 *save);
bool8 WorldEventSave_PrepareForWrite(void);
bool8 WorldEventSave_GetFlag(u16 ordinal);
bool8 WorldEventSave_SetFlag(u16 ordinal, bool8 value);
bool8 WorldEventSave_GetTrainerDefeated(u16 ordinal);
bool8 WorldEventSave_SetTrainerDefeated(u16 ordinal, bool8 value);
u16 WorldEventSave_GetVariable(u16 ordinal);
bool8 WorldEventSave_SetVariable(u16 ordinal, u16 value);

#endif // GUARD_WORLD_EVENT_SAVE_H
