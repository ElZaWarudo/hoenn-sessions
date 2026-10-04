#ifndef GUARD_COOP_PLAYER_TRANSFER_H
#define GUARD_COOP_PLAYER_TRANSFER_H

#include "gba/types.h"

/*
 * This is a description of the projection that a future launcher may use
 * when moving the player between world ROMs.  It deliberately does not copy
 * or modify any save data yet.  The records describe every byte in the four
 * compiler-defined save objects, including bytes that are currently opaque.
 * A receiver may only project SHARED_PLAYER records; the other ownership
 * classes are present so a new region cannot silently lose an undecided byte.
 */
#define COOP_PLAYER_TRANSFER_SCHEMA_MAGIC 0x31545043u /* little-endian "CPT1" */
#define COOP_PLAYER_TRANSFER_SCHEMA_VERSION 3

enum CoopPlayerTransferStorage
{
    COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK1,
    COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK2,
    COOP_PLAYER_TRANSFER_STORAGE_POKEMON_STORAGE,
    COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK3,
    COOP_PLAYER_TRANSFER_STORAGE_COUNT,
};

enum CoopPlayerTransferOwnership
{
    /* Safe to copy after the destination has accepted the shared schema. */
    COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER = 1,
    /* Belongs to the active world's campaign and stays in that world. */
    COOP_PLAYER_TRANSFER_OWNER_WORLD_LOCAL = 2,
    /* A deliberate stop sign until the product decision is made. */
    COOP_PLAYER_TRANSFER_OWNER_LOCAL_PENDING = 3,
};

/* Field IDs are part of the cross-ROM contract.  Never reuse an ID. */
enum CoopPlayerTransferFieldId
{
    COOP_PLAYER_TRANSFER_FIELD_S1_LOCAL_PREFIX = 0x0100,
    COOP_PLAYER_TRANSFER_FIELD_S1_PARTY = 0x0101,
    COOP_PLAYER_TRANSFER_FIELD_S1_MONEY = 0x0102,
    COOP_PLAYER_TRANSFER_FIELD_S1_COINS = 0x0103,
    COOP_PLAYER_TRANSFER_FIELD_S1_REGISTERED_ITEM = 0x0104,
    COOP_PLAYER_TRANSFER_FIELD_S1_PC_ITEMS = 0x0105,
    COOP_PLAYER_TRANSFER_FIELD_S1_BAG = 0x0106,
    COOP_PLAYER_TRANSFER_FIELD_S1_POKEBLOCKS = 0x0107,
    COOP_PLAYER_TRANSFER_FIELD_S1_LOCAL_BEFORE_GAME_STATS = 0x0108,
    COOP_PLAYER_TRANSFER_FIELD_S1_ROOM_DECORATIONS = 0x0109,
    COOP_PLAYER_TRANSFER_FIELD_S1_LOCAL_BEFORE_MAIL = 0x010A,
    COOP_PLAYER_TRANSFER_FIELD_S1_MAIL = 0x010B,
    COOP_PLAYER_TRANSFER_FIELD_S1_LOCAL_BEFORE_DAYCARE = 0x010C,
    COOP_PLAYER_TRANSFER_FIELD_S1_DAYCARE = 0x010D,
    COOP_PLAYER_TRANSFER_FIELD_S1_LOCAL_BEFORE_DEX = 0x010E,
    COOP_PLAYER_TRANSFER_FIELD_S1_DEX_SEEN = 0x010F,
    COOP_PLAYER_TRANSFER_FIELD_S1_DEX_CAUGHT = 0x0110,
    COOP_PLAYER_TRANSFER_FIELD_S1_LOCAL_BEFORE_ROUTE5_DAYCARE = 0x0111,
    COOP_PLAYER_TRANSFER_FIELD_S1_ROUTE5_DAYCARE = 0x0112,
    COOP_PLAYER_TRANSFER_FIELD_S1_GAME_STATS = 0x0113,
    COOP_PLAYER_TRANSFER_FIELD_S1_LOCAL_AFTER_GAME_STATS = 0x0114,
    COOP_PLAYER_TRANSFER_FIELD_S1_DECORATION_INVENTORY = 0x0115,

    COOP_PLAYER_TRANSFER_FIELD_S2_PLAYER_NAME = 0x0200,
    COOP_PLAYER_TRANSFER_FIELD_S2_RIVAL_NAME = 0x0201,
    COOP_PLAYER_TRANSFER_FIELD_S2_PLAYER_GENDER = 0x0202,
    COOP_PLAYER_TRANSFER_FIELD_S2_REGION = 0x0203,
    COOP_PLAYER_TRANSFER_FIELD_S2_SPECIAL_WARP = 0x0204,
    COOP_PLAYER_TRANSFER_FIELD_S2_TRAINER_ID = 0x0205,
    COOP_PLAYER_TRANSFER_FIELD_S2_PLAY_TIME = 0x0206,
    COOP_PLAYER_TRANSFER_FIELD_S2_OPTIONS_BUTTON = 0x0207,
    COOP_PLAYER_TRANSFER_FIELD_S2_OPTIONS_BITFIELD = 0x0208,
    COOP_PLAYER_TRANSFER_FIELD_S2_POKEDEX = 0x0209,
    COOP_PLAYER_TRANSFER_FIELD_S2_LOCAL_BEFORE_KEY = 0x020A,
    COOP_PLAYER_TRANSFER_FIELD_S2_ENCRYPTION_KEY = 0x020B,
    COOP_PLAYER_TRANSFER_FIELD_S2_LOCAL_BEFORE_BERRY_POWDER = 0x020C,
    COOP_PLAYER_TRANSFER_FIELD_S2_BERRY_POWDER = 0x020D,
    COOP_PLAYER_TRANSFER_FIELD_S2_LOCAL_AFTER_BERRY_POWDER = 0x020E,

    COOP_PLAYER_TRANSFER_FIELD_STORAGE_CURRENT_BOX = 0x0300,
    COOP_PLAYER_TRANSFER_FIELD_STORAGE_BOXES = 0x0301,
    COOP_PLAYER_TRANSFER_FIELD_STORAGE_BOX_NAMES = 0x0302,
    COOP_PLAYER_TRANSFER_FIELD_STORAGE_BOX_WALLPAPERS = 0x0303,
    COOP_PLAYER_TRANSFER_FIELD_STORAGE_FUSIONS = 0x0304,
    COOP_PLAYER_TRANSFER_FIELD_STORAGE_PADDING = 0x0305,

    COOP_PLAYER_TRANSFER_FIELD_S3_LOCAL_PREFIX = 0x0400,
    COOP_PLAYER_TRANSFER_FIELD_S3_COOP_IDENTITY = 0x0401, /* retired after schema v2 */
    COOP_PLAYER_TRANSFER_FIELD_S3_LOCAL_TAIL = 0x0402, /* retired after schema v2 */
    COOP_PLAYER_TRANSFER_FIELD_S3_COOP_RECORD = 0x0403,
};

/* Count of records, independent of the sparse stable field IDs above. */
#define COOP_PLAYER_TRANSFER_FIELD_COUNT 45

/* The fixed-width representation is consumed by the external manifest tool. */
struct CoopPlayerTransferField
{
    u16 id;
    u8 storage;
    u8 ownership;
    u32 offset;
    u32 size;
    u32 reserved;
};

struct CoopPlayerTransferSchemaHeader
{
    u32 magic;
    u16 version;
    u16 field_count;
    u32 descriptor_size;
    u32 header_size;
    u32 save_block1_size;
    u32 save_block2_size;
    u32 pokemon_storage_size;
    u32 save_block3_size;
    u32 fields_offset;
    u32 field_record_size;
    u32 reserved;
};

struct CoopPlayerTransferSchema
{
    struct CoopPlayerTransferSchemaHeader header;
    struct CoopPlayerTransferField fields[COOP_PLAYER_TRANSFER_FIELD_COUNT];
};

extern const struct CoopPlayerTransferSchema gCoopPlayerTransferSchema;

/* Validates the sorted, complete, non-overlapping projection in ROM. */
bool8 CoopPlayerTransfer_Validate(const struct CoopPlayerTransferSchema *schema);

#endif /* GUARD_COOP_PLAYER_TRANSFER_H */
