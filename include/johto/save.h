#ifndef GUARD_JOHTO_SAVE_H
#define GUARD_JOHTO_SAVE_H

#include <stddef.h>

#include "gba/types.h"

#define JOHTO_SAVE_MAGIC 0x31534F4Au /* little-endian ASCII "JOS1" */
#define JOHTO_SAVE_SCHEMA_VERSION 1
#define JOHTO_SAVE_V1_SIZE 0x16C

#define JOHTO_SAVE_FLAG_BITS_SIZE 96
#define JOHTO_SAVE_TRAINER_BITS_SIZE 64
#define JOHTO_SAVE_VARIABLE_COUNT 96

/* The last SaveBlock1 sector contains 0x108 bytes of legacy data followed by
 * this record.  The old sector checksum therefore remains unchanged when its
 * zero-filled tail is checked with the larger size. */
#define JOHTO_SAVE_LEGACY_TAIL_SIZE 0x108
#define JOHTO_SAVE_SERIALIZED_TAIL_SIZE 0x274

struct JohtoSaveV1
{
    /* 0x000 */ u32 magic;
    /* 0x004 */ u16 schema_version;
    /* 0x006 */ u16 struct_size;
    /* 0x008 */ u8 flag_bits[JOHTO_SAVE_FLAG_BITS_SIZE];
    /* 0x068 */ u8 trainer_bits[JOHTO_SAVE_TRAINER_BITS_SIZE];
    /* 0x0A8 */ u16 variables[JOHTO_SAVE_VARIABLE_COUNT];
    /* 0x168 */ u32 crc32;
};

enum JohtoSaveLoadResult
{
    JOHTO_SAVE_LOAD_READY,
    JOHTO_SAVE_LOAD_INITIALIZED_LEGACY,
    JOHTO_SAVE_LOAD_CORRUPT,
    JOHTO_SAVE_LOAD_INCOMPATIBLE,
};

_Static_assert(sizeof(struct JohtoSaveV1) == JOHTO_SAVE_V1_SIZE,
               "JohtoSaveV1 ABI size");
_Static_assert(offsetof(struct JohtoSaveV1, flag_bits) == 0x08,
               "Johto flag bits offset");
_Static_assert(offsetof(struct JohtoSaveV1, trainer_bits) == 0x68,
               "Johto trainer bits offset");
_Static_assert(offsetof(struct JohtoSaveV1, variables) == 0xA8,
               "Johto variables offset");
_Static_assert(offsetof(struct JohtoSaveV1, crc32) == 0x168,
               "Johto CRC offset");

void JohtoSave_Initialize(struct JohtoSaveV1 *save);
void JohtoSave_InitializeCurrent(void);
enum JohtoSaveLoadResult JohtoSave_Load(void);
bool8 JohtoSave_Validate(const struct JohtoSaveV1 *save);
bool8 JohtoSave_Seal(struct JohtoSaveV1 *save);
bool8 JohtoSave_PrepareForWrite(void);

bool8 JohtoSave_GetFlag(u16 ordinal);
bool8 JohtoSave_SetFlag(u16 ordinal, bool8 value);
bool8 JohtoSave_GetTrainerDefeated(u16 ordinal);
bool8 JohtoSave_SetTrainerDefeated(u16 ordinal, bool8 defeated);
u16 JohtoSave_GetVariable(u16 ordinal);
bool8 JohtoSave_SetVariable(u16 ordinal, u16 value);

u32 JohtoSave_Crc32(const void *data, u32 length);
u32 JohtoSave_CalculateCrc(const struct JohtoSaveV1 *save);

#endif /* GUARD_JOHTO_SAVE_H */
