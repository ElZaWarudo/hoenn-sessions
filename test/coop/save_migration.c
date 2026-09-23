#include "global.h"
#include "coop/save.h"
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

TEST("Cloud copied V1 upgrade seals normalized V2 and rejects retry")
{
    union
    {
        struct CoopSaveV1 v1;
        struct CoopSaveV2 v2;
    } record;
    u16 location;

    PlaceLegacyMarkerCollision();
    CoopSave_Initialize(&record.v1);
    record.v1.save_generation = 7;
    record.v1.regional_progress[0].story_checkpoint = 123;
    EXPECT(CoopSave_Seal(&record.v1));

    EXPECT(CoopSaveV2_UpgradeCopiedV1(&record.v1, &sMigrationSaveBlock1,
                                      &sMigrationStorage));
    EXPECT(CoopSaveV2_Validate(&record.v2));
    EXPECT_EQ(record.v2.schema_version, COOP_SAVE_V2_SCHEMA_VERSION);
    EXPECT_EQ(record.v2.status_flags, COOP_SAVE_STATUS_MET_LOCATION_NORMALIZED);
    EXPECT_EQ(record.v2.cormoria_progress.region, COOP_SAVE_V2_CORMORIA_REGION);
    EXPECT_EQ(record.v2.save_generation, 7);
    EXPECT_EQ(record.v2.regional_progress[0].story_checkpoint, 123);
    EXPECT(GetBoxMonMetLocationV2(&sMigrationSaveBlock1.playerParty[0].box, &location));
    EXPECT_EQ(location, MET_LOCATION_V2_LEGACY_253);

    // A second attempt cannot reinterpret V2 marker bytes as legacy bytes.
    EXPECT(SetBoxMonMetLocationV2(&sMigrationSaveBlock1.playerParty[0].box, 250));
    EXPECT(!CoopSaveV2_UpgradeCopiedV1(&record.v1, &sMigrationSaveBlock1,
                                       &sMigrationStorage));
    EXPECT(GetBoxMonMetLocationV2(&sMigrationSaveBlock1.playerParty[0].box, &location));
    EXPECT_EQ(location, 250);
}

TEST("Cloud copied V1 upgrade preserves record and Pokemon on rejection")
{
    struct CoopSaveV1 source;
    struct CoopSaveV1 before;
    struct Pokemon originalParty;

    PlaceLegacyMarkerCollision();
    CoopSave_Initialize(&source);
    before = source;
    originalParty = sMigrationSaveBlock1.playerParty[0];
    sMigrationSaveBlock1.route5DayCareMon.mon.secure.raw[0] ^= 1;

    EXPECT(!CoopSaveV2_UpgradeCopiedV1(&source, &sMigrationSaveBlock1,
                                       &sMigrationStorage));
    EXPECT(memcmp(&source, &before, sizeof(source)) == 0);
    EXPECT(memcmp(&sMigrationSaveBlock1.playerParty[0], &originalParty,
                  sizeof(originalParty)) == 0);
    EXPECT(CoopSave_Validate(&source));
}
