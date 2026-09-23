#include "global.h"
#include "coop/save_migration.h"
#include "pokemon.h"
#include "pokemon_storage_system.h"
#include "test/test.h"

// The runner owns these buffers and clears save blocks between tests. Reuse
// them rather than reserving another ~50 KiB of scarce test EWRAM.
#define sMigrationSaveBlock1 (*gSaveBlock1Ptr)
#define sMigrationStorage (*gPokemonStoragePtr)

static void PlaceLegacyMarkerCollision(void)
{
    struct Pokemon mon;
    u32 oldLocation = 253;

    memset(&sMigrationSaveBlock1, 0, sizeof(sMigrationSaveBlock1));
    memset(&sMigrationStorage, 0, sizeof(sMigrationStorage));

    CreateMon(&mon, SPECIES_WOBBUFFET, 50, 0x12345678,
              OTID_STRUCT_PRESET(0x87654321));
    // A checksum-valid legacy Pokemon may carry the future V2 marker.
    SetBoxMonMetLocationV2(&mon.box, 250);
    SetMonData(&mon, MON_DATA_MET_LOCATION, &oldLocation);

    sMigrationSaveBlock1.playerParty[0] = mon;
    sMigrationStorage.boxes[0][0] = mon.box;
    sMigrationStorage.boxes[TOTAL_BOXES_COUNT - 1][IN_BOX_COUNT - 1] = mon.box;
    sMigrationStorage.fusions[MAX_FUSION_STORAGE - 1] = mon;
    sMigrationSaveBlock1.daycare.mons[0].mon = mon.box;
    sMigrationSaveBlock1.daycare.mons[DAYCARE_MON_COUNT - 1].mon = mon.box;
    sMigrationSaveBlock1.route5DayCareMon.mon = mon.box;
}

TEST("Cloud copied-save migration normalizes every persisted Pokemon area")
{
    u16 location;

    PlaceLegacyMarkerCollision();
    EXPECT(CoopSave_NormalizeLegacyPokemon(&sMigrationSaveBlock1, &sMigrationStorage));

    EXPECT(GetBoxMonMetLocationV2(&sMigrationSaveBlock1.playerParty[0].box, &location));
    EXPECT_EQ(location, MET_LOCATION_V2_LEGACY_253);
    EXPECT(GetBoxMonMetLocationV2(&sMigrationStorage.boxes[0][0], &location));
    EXPECT_EQ(location, MET_LOCATION_V2_LEGACY_253);
    EXPECT(GetBoxMonMetLocationV2(&sMigrationStorage.boxes[TOTAL_BOXES_COUNT - 1][IN_BOX_COUNT - 1], &location));
    EXPECT_EQ(location, MET_LOCATION_V2_LEGACY_253);
    EXPECT(GetBoxMonMetLocationV2(&sMigrationStorage.fusions[MAX_FUSION_STORAGE - 1].box, &location));
    EXPECT_EQ(location, MET_LOCATION_V2_LEGACY_253);
    EXPECT(GetBoxMonMetLocationV2(&sMigrationSaveBlock1.daycare.mons[0].mon, &location));
    EXPECT_EQ(location, MET_LOCATION_V2_LEGACY_253);
    EXPECT(GetBoxMonMetLocationV2(&sMigrationSaveBlock1.daycare.mons[DAYCARE_MON_COUNT - 1].mon, &location));
    EXPECT_EQ(location, MET_LOCATION_V2_LEGACY_253);
    EXPECT(GetBoxMonMetLocationV2(&sMigrationSaveBlock1.route5DayCareMon.mon, &location));
    EXPECT_EQ(location, MET_LOCATION_V2_LEGACY_253);
}

TEST("Cloud copied-save migration rejects late corruption before any mutation")
{
    struct Pokemon originalParty;

    PlaceLegacyMarkerCollision();
    originalParty = sMigrationSaveBlock1.playerParty[0];
    sMigrationSaveBlock1.route5DayCareMon.mon.secure.raw[0] ^= 1;

    EXPECT(!CoopSave_NormalizeLegacyPokemon(&sMigrationSaveBlock1, &sMigrationStorage));
    EXPECT(memcmp(&sMigrationSaveBlock1.playerParty[0], &originalParty,
                  sizeof(originalParty)) == 0);
    EXPECT(!CoopSave_NormalizeLegacyPokemon(NULL, &sMigrationStorage));
    EXPECT(!CoopSave_NormalizeLegacyPokemon(&sMigrationSaveBlock1, NULL));
}
