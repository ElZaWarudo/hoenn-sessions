#include "global.h"
#include "pokemon_storage_system.h"

#include "coop/player_transfer.h"
#include "coop/save.h"

#define MEMBER_SIZE(type, member) (sizeof(((type *)0)->member))
#define MEMBER_END(type, member) \
    (offsetof(type, member) + MEMBER_SIZE(type, member))

/*
 * These assertions make the compiler the authority for every offset and
 * length in the descriptor.  In particular, options_bitfield_storage is the
 * complete C bitfield allocation unit; it is not a collection of guessed
 * bit offsets.
 */
_Static_assert(offsetof(struct SaveBlock2, pokedex)
                   > offsetof(struct SaveBlock2, optionsButtonMode) + sizeof(u8),
               "SaveBlock2 options storage must precede Pokedex");
_Static_assert(offsetof(struct SaveBlock2, pokedex)
                   - (offsetof(struct SaveBlock2, optionsButtonMode) + sizeof(u8))
                   == sizeof(u16),
               "SaveBlock2 option bitfields must occupy one u16 storage unit");
_Static_assert(sizeof(struct CoopSaveV1) == COOP_SAVE_V1_SIZE,
               "SaveBlock3 co-op record ABI drift");
_Static_assert(MEMBER_SIZE(struct SaveBlock3, coop) == COOP_SAVE_V2_SIZE,
               "SaveBlock3 co-op projection size drift");
_Static_assert(offsetof(struct CoopSaveV1, registry_digest)
                   == offsetof(struct CoopSaveV1, registry_version) + sizeof(u32),
               "co-op registry identity must stay contiguous");
_Static_assert(sizeof(((struct CoopSaveV1 *)0)->registry_digest)
                   == COOP_IDENTITY_REGISTRY_DIGEST_SIZE,
               "co-op registry identity digest size drift");

const struct CoopPlayerTransferSchema gCoopPlayerTransferSchema
    __attribute__((used, section(".rodata.coop_player_transfer"))) =
{
    .header =
    {
        .magic = COOP_PLAYER_TRANSFER_SCHEMA_MAGIC,
        .version = COOP_PLAYER_TRANSFER_SCHEMA_VERSION,
        .field_count = COOP_PLAYER_TRANSFER_FIELD_COUNT,
        .descriptor_size = sizeof(struct CoopPlayerTransferSchema),
        .header_size = offsetof(struct CoopPlayerTransferSchema, fields),
        .save_block1_size = sizeof(struct SaveBlock1),
        .save_block2_size = sizeof(struct SaveBlock2),
        .pokemon_storage_size = sizeof(struct PokemonStorage),
        .save_block3_size = sizeof(struct SaveBlock3),
        .fields_offset = offsetof(struct CoopPlayerTransferSchema, fields),
        .field_record_size = sizeof(struct CoopPlayerTransferField),
        .reserved = 0,
    },
    .fields =
    {
        /* SaveBlock1: all party and inventory objects cross with the player. */
        {
            COOP_PLAYER_TRANSFER_FIELD_S1_LOCAL_PREFIX,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK1,
            COOP_PLAYER_TRANSFER_OWNER_WORLD_LOCAL,
            0,
            offsetof(struct SaveBlock1, playerPartyCount),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S1_PARTY,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK1,
            COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER,
            offsetof(struct SaveBlock1, playerPartyCount),
            MEMBER_END(struct SaveBlock1, playerParty)
                - offsetof(struct SaveBlock1, playerPartyCount),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S1_MONEY,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK1,
            COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER,
            offsetof(struct SaveBlock1, money),
            MEMBER_SIZE(struct SaveBlock1, money),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S1_COINS,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK1,
            COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER,
            offsetof(struct SaveBlock1, coins),
            MEMBER_SIZE(struct SaveBlock1, coins),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S1_REGISTERED_ITEM,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK1,
            COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER,
            offsetof(struct SaveBlock1, registeredItem),
            MEMBER_SIZE(struct SaveBlock1, registeredItem),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S1_PC_ITEMS,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK1,
            COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER,
            offsetof(struct SaveBlock1, pcItems),
            MEMBER_SIZE(struct SaveBlock1, pcItems),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S1_BAG,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK1,
            COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER,
            offsetof(struct SaveBlock1, bag),
            MEMBER_SIZE(struct SaveBlock1, bag),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S1_POKEBLOCKS,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK1,
            COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER,
            offsetof(struct SaveBlock1, pokeblocks),
            MEMBER_SIZE(struct SaveBlock1, pokeblocks),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S1_LOCAL_BEFORE_GAME_STATS,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK1,
            COOP_PLAYER_TRANSFER_OWNER_WORLD_LOCAL,
            MEMBER_END(struct SaveBlock1, pokeblocks),
            offsetof(struct SaveBlock1, gameStats)
                - MEMBER_END(struct SaveBlock1, pokeblocks),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S1_GAME_STATS,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK1,
            /* Values are encrypted with SaveBlock2.encryptionKey. */
            COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER,
            offsetof(struct SaveBlock1, gameStats),
            MEMBER_SIZE(struct SaveBlock1, gameStats),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S1_LOCAL_AFTER_GAME_STATS,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK1,
            /* Berry-tree plots and secret-base placements stay in the world. */
            COOP_PLAYER_TRANSFER_OWNER_WORLD_LOCAL,
            MEMBER_END(struct SaveBlock1, gameStats),
            offsetof(struct SaveBlock1, playerRoomDecorations)
                - MEMBER_END(struct SaveBlock1, gameStats),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S1_ROOM_DECORATIONS,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK1,
            /* The player's room is a physical location in this world. */
            COOP_PLAYER_TRANSFER_OWNER_WORLD_LOCAL,
            offsetof(struct SaveBlock1, playerRoomDecorations),
            offsetof(struct SaveBlock1, decorationDesks)
                - offsetof(struct SaveBlock1, playerRoomDecorations),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S1_DECORATION_INVENTORY,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK1,
            /* Furnishings belong to the world where they may be placed. */
            COOP_PLAYER_TRANSFER_OWNER_WORLD_LOCAL,
            offsetof(struct SaveBlock1, decorationDesks),
            MEMBER_END(struct SaveBlock1, decorationCushions)
                - offsetof(struct SaveBlock1, decorationDesks),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S1_LOCAL_BEFORE_MAIL,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK1,
            COOP_PLAYER_TRANSFER_OWNER_WORLD_LOCAL,
            MEMBER_END(struct SaveBlock1, decorationCushions),
            offsetof(struct SaveBlock1, mail)
                - MEMBER_END(struct SaveBlock1, decorationCushions),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S1_MAIL,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK1,
            COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER,
            offsetof(struct SaveBlock1, mail),
            MEMBER_SIZE(struct SaveBlock1, mail),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S1_LOCAL_BEFORE_DAYCARE,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK1,
            COOP_PLAYER_TRANSFER_OWNER_WORLD_LOCAL,
            MEMBER_END(struct SaveBlock1, mail),
            offsetof(struct SaveBlock1, daycare)
                - MEMBER_END(struct SaveBlock1, mail),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S1_DAYCARE,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK1,
            /* Deposited Pokémon travel with the one active player authority. */
            COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER,
            offsetof(struct SaveBlock1, daycare),
            MEMBER_SIZE(struct SaveBlock1, daycare),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S1_LOCAL_BEFORE_DEX,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK1,
            COOP_PLAYER_TRANSFER_OWNER_WORLD_LOCAL,
            MEMBER_END(struct SaveBlock1, daycare),
            offsetof(struct SaveBlock1, dexSeen)
                - MEMBER_END(struct SaveBlock1, daycare),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S1_DEX_SEEN,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK1,
            COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER,
            offsetof(struct SaveBlock1, dexSeen),
            MEMBER_SIZE(struct SaveBlock1, dexSeen),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S1_DEX_CAUGHT,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK1,
            COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER,
            offsetof(struct SaveBlock1, dexCaught),
            MEMBER_SIZE(struct SaveBlock1, dexCaught),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S1_LOCAL_BEFORE_ROUTE5_DAYCARE,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK1,
            COOP_PLAYER_TRANSFER_OWNER_WORLD_LOCAL,
            MEMBER_END(struct SaveBlock1, dexCaught),
            offsetof(struct SaveBlock1, route5DayCareMon)
                - MEMBER_END(struct SaveBlock1, dexCaught),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S1_ROUTE5_DAYCARE,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK1,
            /* The Route 5 deposit follows the same authority. */
            COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER,
            offsetof(struct SaveBlock1, route5DayCareMon),
            sizeof(struct SaveBlock1) - offsetof(struct SaveBlock1, route5DayCareMon),
            0,
        },

        /* SaveBlock2: identity, settings and Pokédex are common player data. */
        {
            COOP_PLAYER_TRANSFER_FIELD_S2_PLAYER_NAME,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK2,
            COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER,
            offsetof(struct SaveBlock2, playerName),
            MEMBER_SIZE(struct SaveBlock2, playerName),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S2_RIVAL_NAME,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK2,
            /* Each campaign can name its own rival; the world image retains it. */
            COOP_PLAYER_TRANSFER_OWNER_WORLD_LOCAL,
            MEMBER_END(struct SaveBlock2, playerName),
            offsetof(struct SaveBlock2, playerGender)
                - MEMBER_END(struct SaveBlock2, playerName),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S2_PLAYER_GENDER,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK2,
            COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER,
            offsetof(struct SaveBlock2, playerGender),
            MEMBER_SIZE(struct SaveBlock2, playerGender),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S2_REGION,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK2,
            /* Starting region selects the player's avatar, not the active ROM. */
            COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER,
            offsetof(struct SaveBlock2, playerRegion),
            MEMBER_SIZE(struct SaveBlock2, playerRegion),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S2_SPECIAL_WARP,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK2,
            COOP_PLAYER_TRANSFER_OWNER_WORLD_LOCAL,
            offsetof(struct SaveBlock2, specialSaveWarpFlags),
            MEMBER_SIZE(struct SaveBlock2, specialSaveWarpFlags),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S2_TRAINER_ID,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK2,
            COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER,
            offsetof(struct SaveBlock2, playerTrainerId),
            MEMBER_SIZE(struct SaveBlock2, playerTrainerId),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S2_PLAY_TIME,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK2,
            COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER,
            MEMBER_END(struct SaveBlock2, playerTrainerId),
            offsetof(struct SaveBlock2, optionsButtonMode)
                - MEMBER_END(struct SaveBlock2, playerTrainerId),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S2_OPTIONS_BUTTON,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK2,
            COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER,
            offsetof(struct SaveBlock2, optionsButtonMode),
            MEMBER_SIZE(struct SaveBlock2, optionsButtonMode),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S2_OPTIONS_BITFIELD,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK2,
            COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER,
            offsetof(struct SaveBlock2, optionsButtonMode)
                + MEMBER_SIZE(struct SaveBlock2, optionsButtonMode),
            offsetof(struct SaveBlock2, pokedex)
                - (offsetof(struct SaveBlock2, optionsButtonMode)
                   + MEMBER_SIZE(struct SaveBlock2, optionsButtonMode)),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S2_POKEDEX,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK2,
            COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER,
            offsetof(struct SaveBlock2, pokedex),
            MEMBER_SIZE(struct SaveBlock2, pokedex),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S2_LOCAL_BEFORE_KEY,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK2,
            COOP_PLAYER_TRANSFER_OWNER_WORLD_LOCAL,
            MEMBER_END(struct SaveBlock2, pokedex),
            offsetof(struct SaveBlock2, encryptionKey)
                - MEMBER_END(struct SaveBlock2, pokedex),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S2_ENCRYPTION_KEY,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK2,
            /* Retain the destination key; transferred encrypted values must
             * be rekeyed before the destination save becomes active. */
            COOP_PLAYER_TRANSFER_OWNER_WORLD_LOCAL,
            offsetof(struct SaveBlock2, encryptionKey),
            MEMBER_SIZE(struct SaveBlock2, encryptionKey),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S2_LOCAL_BEFORE_BERRY_POWDER,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK2,
            COOP_PLAYER_TRANSFER_OWNER_WORLD_LOCAL,
            MEMBER_END(struct SaveBlock2, encryptionKey),
            offsetof(struct SaveBlock2, berryCrush.berryPowderAmount)
                - MEMBER_END(struct SaveBlock2, encryptionKey),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S2_BERRY_POWDER,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK2,
            /* Encrypted with SaveBlock2.encryptionKey; transfer needs rekey. */
            COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER,
            offsetof(struct SaveBlock2, berryCrush.berryPowderAmount),
            MEMBER_SIZE(struct BerryCrush, berryPowderAmount),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S2_LOCAL_AFTER_BERRY_POWDER,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK2,
            COOP_PLAYER_TRANSFER_OWNER_WORLD_LOCAL,
            offsetof(struct SaveBlock2, berryCrush.berryPowderAmount)
                + MEMBER_SIZE(struct BerryCrush, berryPowderAmount),
            sizeof(struct SaveBlock2)
                - (offsetof(struct SaveBlock2, berryCrush.berryPowderAmount)
                   + MEMBER_SIZE(struct BerryCrush, berryPowderAmount)),
            0,
        },

        /* Pokémon storage is a shared player inventory, including UI state. */
        {
            COOP_PLAYER_TRANSFER_FIELD_STORAGE_CURRENT_BOX,
            COOP_PLAYER_TRANSFER_STORAGE_POKEMON_STORAGE,
            COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER,
            offsetof(struct PokemonStorage, currentBox),
            MEMBER_SIZE(struct PokemonStorage, currentBox),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_STORAGE_PADDING,
            COOP_PLAYER_TRANSFER_STORAGE_POKEMON_STORAGE,
            COOP_PLAYER_TRANSFER_OWNER_WORLD_LOCAL,
            MEMBER_END(struct PokemonStorage, currentBox),
            offsetof(struct PokemonStorage, boxes)
                - MEMBER_END(struct PokemonStorage, currentBox),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_STORAGE_BOXES,
            COOP_PLAYER_TRANSFER_STORAGE_POKEMON_STORAGE,
            COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER,
            offsetof(struct PokemonStorage, boxes),
            MEMBER_SIZE(struct PokemonStorage, boxes),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_STORAGE_BOX_NAMES,
            COOP_PLAYER_TRANSFER_STORAGE_POKEMON_STORAGE,
            COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER,
            offsetof(struct PokemonStorage, boxNames),
            MEMBER_SIZE(struct PokemonStorage, boxNames),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_STORAGE_BOX_WALLPAPERS,
            COOP_PLAYER_TRANSFER_STORAGE_POKEMON_STORAGE,
            COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER,
            offsetof(struct PokemonStorage, boxWallpapers),
            MEMBER_SIZE(struct PokemonStorage, boxWallpapers),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_STORAGE_FUSIONS,
            COOP_PLAYER_TRANSFER_STORAGE_POKEMON_STORAGE,
            COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER,
            MEMBER_END(struct PokemonStorage, boxWallpapers),
            sizeof(struct PokemonStorage) - MEMBER_END(struct PokemonStorage, boxWallpapers),
            0,
        },

        /* SaveBlock3 carries the co-op release identity; progress is local. */
        {
            COOP_PLAYER_TRANSFER_FIELD_S3_LOCAL_PREFIX,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK3,
            COOP_PLAYER_TRANSFER_OWNER_WORLD_LOCAL,
            0,
            offsetof(struct SaveBlock3, coop),
            0,
        },
        {
            COOP_PLAYER_TRANSFER_FIELD_S3_COOP_RECORD,
            COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK3,
            COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER,
            offsetof(struct SaveBlock3, coop),
            MEMBER_SIZE(struct SaveBlock3, coop),
            0,
        },
    },
};

static u32 StorageSize(u8 storage, const struct CoopPlayerTransferSchema *schema)
{
    switch (storage)
    {
    case COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK1:
        return schema->header.save_block1_size;
    case COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK2:
        return schema->header.save_block2_size;
    case COOP_PLAYER_TRANSFER_STORAGE_POKEMON_STORAGE:
        return schema->header.pokemon_storage_size;
    case COOP_PLAYER_TRANSFER_STORAGE_SAVE_BLOCK3:
        return schema->header.save_block3_size;
    default:
        return 0;
    }
}

bool8 CoopPlayerTransfer_Validate(const struct CoopPlayerTransferSchema *schema)
{
    u8 storage;
    u32 cursor = 0;
    u16 i;

    if (schema == NULL
        || schema->header.magic != COOP_PLAYER_TRANSFER_SCHEMA_MAGIC
        || schema->header.version != COOP_PLAYER_TRANSFER_SCHEMA_VERSION
        || schema->header.field_count != COOP_PLAYER_TRANSFER_FIELD_COUNT
        || schema->header.descriptor_size != sizeof(*schema)
        || schema->header.header_size != offsetof(struct CoopPlayerTransferSchema, fields)
        || schema->header.save_block1_size != sizeof(struct SaveBlock1)
        || schema->header.save_block2_size != sizeof(struct SaveBlock2)
        || schema->header.pokemon_storage_size != sizeof(struct PokemonStorage)
        || schema->header.save_block3_size != sizeof(struct SaveBlock3)
        || schema->header.fields_offset != offsetof(struct CoopPlayerTransferSchema, fields)
        || schema->header.field_record_size != sizeof(struct CoopPlayerTransferField)
        || schema->header.reserved != 0)
        return FALSE;

    for (storage = 0; storage < COOP_PLAYER_TRANSFER_STORAGE_COUNT; storage++)
    {
        u32 span = StorageSize(storage, schema);

        if (span == 0)
            return FALSE;
        if (storage != 0 && cursor != StorageSize(storage - 1, schema))
            return FALSE;
        cursor = 0;
        for (i = 0; i < schema->header.field_count; i++)
        {
            const struct CoopPlayerTransferField *field = &schema->fields[i];
            u16 j;

            if (field->storage != storage)
                continue;
            if (field->id == 0 || field->size == 0
                || field->offset != cursor
                || field->offset > span
                || field->size > span - field->offset
                || field->reserved != 0
                || (field->ownership != COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER
                    && field->ownership != COOP_PLAYER_TRANSFER_OWNER_WORLD_LOCAL
                    && field->ownership != COOP_PLAYER_TRANSFER_OWNER_LOCAL_PENDING))
                return FALSE;
            for (j = 0; j < i; j++)
            {
                if (schema->fields[j].id == field->id)
                    return FALSE;
            }
            cursor += field->size;
        }
        if (cursor != span)
            return FALSE;
    }

    return TRUE;
}
