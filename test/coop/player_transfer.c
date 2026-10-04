#include "global.h"
#include "coop/player_transfer.h"
#include "pokemon_storage_system.h"
#include "malloc.h"
#include "test/test.h"

static const struct CoopPlayerTransferField *FindField(u16 id)
{
    u16 i;

    for (i = 0; i < gCoopPlayerTransferSchema.header.field_count; i++)
    {
        if (gCoopPlayerTransferSchema.fields[i].id == id)
            return &gCoopPlayerTransferSchema.fields[i];
    }
    return NULL;
}

TEST("Shared-player descriptor covers compiler-defined save spans")
{
    const struct CoopPlayerTransferField *field;

    EXPECT(CoopPlayerTransfer_Validate(&gCoopPlayerTransferSchema));
    EXPECT_EQ(gCoopPlayerTransferSchema.header.magic,
              COOP_PLAYER_TRANSFER_SCHEMA_MAGIC);
    EXPECT_EQ(gCoopPlayerTransferSchema.header.version,
              COOP_PLAYER_TRANSFER_SCHEMA_VERSION);
    EXPECT_EQ(gCoopPlayerTransferSchema.header.field_count,
              COOP_PLAYER_TRANSFER_FIELD_COUNT);
    EXPECT_EQ(gCoopPlayerTransferSchema.header.save_block1_size,
              sizeof(struct SaveBlock1));
    EXPECT_EQ(gCoopPlayerTransferSchema.header.save_block2_size,
              sizeof(struct SaveBlock2));
    EXPECT_EQ(gCoopPlayerTransferSchema.header.pokemon_storage_size,
              sizeof(struct PokemonStorage));
    EXPECT_EQ(gCoopPlayerTransferSchema.header.save_block3_size,
              sizeof(struct SaveBlock3));

    field = FindField(COOP_PLAYER_TRANSFER_FIELD_S1_PARTY);
    EXPECT(field != NULL);
    EXPECT_EQ(field->offset, offsetof(struct SaveBlock1, playerPartyCount));
    EXPECT_EQ(field->size,
              offsetof(struct SaveBlock1, money)
                  - offsetof(struct SaveBlock1, playerPartyCount));

    field = FindField(COOP_PLAYER_TRANSFER_FIELD_S1_DEX_SEEN);
    EXPECT(field != NULL);
    EXPECT_EQ(field->ownership, COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER);

    field = FindField(COOP_PLAYER_TRANSFER_FIELD_S2_ENCRYPTION_KEY);
    EXPECT(field != NULL);
    EXPECT_EQ(field->offset, offsetof(struct SaveBlock2, encryptionKey));
    EXPECT_EQ(field->ownership, COOP_PLAYER_TRANSFER_OWNER_WORLD_LOCAL);
    field = FindField(COOP_PLAYER_TRANSFER_FIELD_S1_DEX_CAUGHT);
    EXPECT(field != NULL);
    EXPECT_EQ(field->ownership, COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER);
    field = FindField(COOP_PLAYER_TRANSFER_FIELD_S1_DAYCARE);
    EXPECT(field != NULL);
    EXPECT_EQ(field->ownership, COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER);

    field = FindField(COOP_PLAYER_TRANSFER_FIELD_S1_ROUTE5_DAYCARE);
    EXPECT(field != NULL);
    EXPECT_EQ(field->ownership, COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER);

    field = FindField(COOP_PLAYER_TRANSFER_FIELD_S1_GAME_STATS);
    EXPECT(field != NULL);
    EXPECT_EQ(field->offset, offsetof(struct SaveBlock1, gameStats));
    EXPECT_EQ(field->size, sizeof(((struct SaveBlock1 *)0)->gameStats));
    EXPECT_EQ(field->ownership, COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER);

    field = FindField(COOP_PLAYER_TRANSFER_FIELD_S1_ROOM_DECORATIONS);
    EXPECT(field != NULL);
    EXPECT_EQ(field->ownership, COOP_PLAYER_TRANSFER_OWNER_WORLD_LOCAL);
    field = FindField(COOP_PLAYER_TRANSFER_FIELD_S1_DECORATION_INVENTORY);
    EXPECT(field != NULL);
    EXPECT_EQ(field->offset, offsetof(struct SaveBlock1, decorationDesks));
    EXPECT_EQ(field->ownership, COOP_PLAYER_TRANSFER_OWNER_WORLD_LOCAL);

    field = FindField(COOP_PLAYER_TRANSFER_FIELD_S1_REGISTERED_ITEM);
    EXPECT(field != NULL);
    EXPECT_EQ(field->ownership, COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER);
    field = FindField(COOP_PLAYER_TRANSFER_FIELD_S2_PLAY_TIME);
    EXPECT(field != NULL);
    EXPECT_EQ(field->ownership, COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER);
    field = FindField(COOP_PLAYER_TRANSFER_FIELD_S2_BERRY_POWDER);
    EXPECT(field != NULL);
    EXPECT_EQ(field->offset, offsetof(struct SaveBlock2, berryCrush.berryPowderAmount));
    EXPECT_EQ(field->ownership, COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER);
    field = FindField(COOP_PLAYER_TRANSFER_FIELD_S2_RIVAL_NAME);
    EXPECT(field != NULL);
    EXPECT_EQ(field->ownership, COOP_PLAYER_TRANSFER_OWNER_WORLD_LOCAL);
    field = FindField(COOP_PLAYER_TRANSFER_FIELD_S2_REGION);
    EXPECT(field != NULL);
    EXPECT_EQ(field->ownership, COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER);

    field = FindField(COOP_PLAYER_TRANSFER_FIELD_S2_OPTIONS_BITFIELD);
    EXPECT(field != NULL);
    EXPECT_EQ(field->size, sizeof(u16));
    EXPECT_EQ(field->ownership, COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER);

    field = FindField(COOP_PLAYER_TRANSFER_FIELD_S3_COOP_RECORD);
    EXPECT(field != NULL);
    EXPECT_EQ(field->size, COOP_SAVE_V2_SIZE);
    EXPECT_EQ(field->ownership, COOP_PLAYER_TRANSFER_OWNER_SHARED_PLAYER);
}

TEST("Shared-player descriptor rejects an overlap or missing byte")
{
    struct CoopPlayerTransferSchema *copy = Alloc(sizeof(*copy));

    EXPECT(copy != NULL);
    if (copy == NULL)
        return;
    *copy = gCoopPlayerTransferSchema;
    EXPECT(CoopPlayerTransfer_Validate(copy));
    copy->fields[1].offset++;
    EXPECT(!CoopPlayerTransfer_Validate(copy));

    *copy = gCoopPlayerTransferSchema;
    copy->fields[1].offset = 0;
    EXPECT(!CoopPlayerTransfer_Validate(copy));
    Free(copy);
}
