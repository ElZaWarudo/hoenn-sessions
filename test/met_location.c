#include "global.h"
#include "daycare.h"
#include "egg_hatch.h"
#include "event_data.h"
#include "malloc.h"
#include "overworld.h"
#include "pokemon.h"
#include "region_map.h"
#include "string_util.h"
#include "test/test.h"
#include "constants/characters.h"
#include "constants/region_map_sections.h"

// Met locations share the map-section value space. Cormoria sections occupy
// 250..300 and MAPSEC_NONE is 301, so a legacy u8 write would wrap 256..300
// onto Hoenn sections. These tests pin the V2 producers/readers.

STATIC_ASSERT(MAPSEC_NONE == MET_LOCATION_V2_NONE, TestMetLocationNoneIsMapsecNone);
STATIC_ASSERT(MET_LOCATION_V2_SPECIAL_EGG > MAPSEC_NONE, TestMetLocationSpecialEggIsNotASection);
STATIC_ASSERT(MET_LOCATION_V2_IN_GAME_TRADE > MAPSEC_NONE, TestMetLocationInGameTradeIsNotASection);
STATIC_ASSERT(MET_LOCATION_V2_FATEFUL_ENCOUNTER > MAPSEC_NONE, TestMetLocationFatefulIsNotASection);

#define V2_SAVE_STATUS (COOP_SAVE_STATUS_MET_LOCATION_NORMALIZED)

static void UseSaveStatus(u32 status)
{
    gSaveBlock3Ptr->coop.status_flags = status;
}

static void WarpTo(u8 mapGroup, u8 mapNum)
{
    gSaveBlock1Ptr->location.mapGroup = mapGroup;
    gSaveBlock1Ptr->location.mapNum = mapNum;
}

#define WARP_TO(map) WarpTo(MAP_GROUP(map), MAP_NUM(map))

TEST("Met location: V2 setter round-trips every Cormoria section")
{
    struct Pokemon *mon = AllocZeroed(sizeof(*mon));
    u32 status = gSaveBlock3Ptr->coop.status_flags;
    u16 location;

    UseSaveStatus(V2_SAVE_STATUS);
    EXPECT(IsMetLocationV2FormatActive());
    CreateMon(mon, SPECIES_WOBBUFFET, 5, 0x12345678, OTID_STRUCT_PRESET(0x87654321));
    for (location = 250; location <= MET_LOCATION_V2_MAX; location++)
    {
        SetMonMetLocation(mon, location);
        EXPECT_EQ(GetMonMetLocation(mon), location);
        EXPECT_EQ(GetMonData(mon, MON_DATA_SANITY_IS_BAD_EGG), FALSE);
    }
    SetMonMetLocation(mon, MAPSEC_NONE);
    EXPECT_EQ(GetMonMetLocation(mon), MAPSEC_NONE);
    // MAPSEC_NONE keeps the legacy "none" byte.
    EXPECT_EQ(GetMonData(mon, MON_DATA_MET_LOCATION), 250);
    SetMonMetLocation(mon, MET_LOCATION_V2_SPECIAL_EGG);
    EXPECT_EQ(GetMonMetLocation(mon), MET_LOCATION_V2_SPECIAL_EGG);
    EXPECT_EQ(GetMonData(mon, MON_DATA_MET_LOCATION), METLOC_SPECIAL_EGG);

    UseSaveStatus(status);
    Free(mon);
}

TEST("Met location: sections below 250 keep the exact legacy encoding")
{
    struct Pokemon *legacy = AllocZeroed(sizeof(*legacy) * 2);
    struct Pokemon *v2 = &legacy[1];
    u32 status = gSaveBlock3Ptr->coop.status_flags;
    u16 location = 0;
    u8 legacyByte;

    PARAMETRIZE { location = 0; }
    PARAMETRIZE { location = MAPSEC_LITTLEROOT_TOWN; }
    PARAMETRIZE { location = MAPSEC_PALLET_TOWN; }
    PARAMETRIZE { location = 249; }
    PARAMETRIZE { location = MAPSEC_NONE; }
    PARAMETRIZE { location = MET_LOCATION_V2_SPECIAL_EGG; }
    PARAMETRIZE { location = MET_LOCATION_V2_IN_GAME_TRADE; }
    PARAMETRIZE { location = MET_LOCATION_V2_FATEFUL_ENCOUNTER; }

    UseSaveStatus(V2_SAVE_STATUS);
    CreateMon(legacy, SPECIES_WOBBUFFET, 5, 0x12345678, OTID_STRUCT_PRESET(0x87654321));
    *v2 = *legacy;
    legacyByte = MetLocationToLegacyByte(location);
    SetMonData(legacy, MON_DATA_MET_LOCATION, &legacyByte);
    SetMonMetLocation(v2, location);
    EXPECT(memcmp(legacy, v2, sizeof(*legacy)) == 0);
    EXPECT_EQ(GetMonMetLocation(v2), location);

    // The same bytes are produced when the save is not V2.
    UseSaveStatus(COOP_SAVE_STATUS_MIGRATION_AMBIGUOUS);
    *v2 = *legacy;
    SetMonMetLocation(v2, location);
    EXPECT(memcmp(legacy, v2, sizeof(*legacy)) == 0);
    EXPECT_EQ(GetMonMetLocation(v2), location);

    UseSaveStatus(status);
    Free(legacy);
}

TEST("Met location: CreateMon below 250 is byte-identical with and without V2")
{
    struct Pokemon *mons = AllocZeroed(sizeof(*mons) * 2);
    u32 status = gSaveBlock3Ptr->coop.status_flags;

    WARP_TO(MAP_LITTLEROOT_TOWN);
    ASSUME(GetCurrentRegionMapSectionId() == MAPSEC_LITTLEROOT_TOWN);
    UseSaveStatus(0);
    CreateMon(&mons[0], SPECIES_WOBBUFFET, 5, 0x12345678, OTID_STRUCT_PRESET(0x87654321));
    UseSaveStatus(V2_SAVE_STATUS);
    CreateMon(&mons[1], SPECIES_WOBBUFFET, 5, 0x12345678, OTID_STRUCT_PRESET(0x87654321));
    EXPECT(memcmp(&mons[0].box, &mons[1].box, sizeof(mons[0].box)) == 0);
    EXPECT_EQ(GetMonData(&mons[1], MON_DATA_MET_LOCATION), MAPSEC_LITTLEROOT_TOWN);
    EXPECT_EQ(GetMonMetLocation(&mons[1]), MAPSEC_LITTLEROOT_TOWN);

    UseSaveStatus(status);
    Free(mons);
}

TEST("Met location: a non-V2 save stores unencodable sections as none, never a Hoenn section")
{
    struct Pokemon *mon = AllocZeroed(sizeof(*mon));
    u32 status = gSaveBlock3Ptr->coop.status_flags;
    u16 location = 0;

    PARAMETRIZE { location = 250; }
    PARAMETRIZE { location = 256; }
    PARAMETRIZE { location = 258; }
    PARAMETRIZE { location = MET_LOCATION_V2_MAX; }

    UseSaveStatus(COOP_SAVE_STATUS_MIGRATION_AMBIGUOUS);
    EXPECT(!IsMetLocationV2FormatActive());
    CreateMon(mon, SPECIES_WOBBUFFET, 5, 0x12345678, OTID_STRUCT_PRESET(0x87654321));
    SetMonMetLocation(mon, location);
    EXPECT_EQ(GetMonData(mon, MON_DATA_MET_LOCATION), 250);
    EXPECT_EQ(GetMonMetLocation(mon), MAPSEC_NONE);

    UseSaveStatus(status);
    Free(mon);
}

TEST("Met location: overwriting a wide location with a legacy special clears the marker")
{
    struct Pokemon *mon = AllocZeroed(sizeof(*mon));
    u32 status = gSaveBlock3Ptr->coop.status_flags;
    u16 v2Location;

    UseSaveStatus(V2_SAVE_STATUS);
    CreateMon(mon, SPECIES_WOBBUFFET, 5, 0x12345678, OTID_STRUCT_PRESET(0x87654321));
    SetMonMetLocation(mon, 260);
    SetMonMetLocation(mon, MET_LOCATION_V2_IN_GAME_TRADE);
    EXPECT(GetBoxMonMetLocationV2(&mon->box, &v2Location));
    EXPECT_EQ(v2Location, MET_LOCATION_V2_IN_GAME_TRADE);
    EXPECT_EQ(GetMonData(mon, MON_DATA_MET_LOCATION), METLOC_IN_GAME_TRADE);

    UseSaveStatus(status);
    Free(mon);
}

TEST("Met location: legacy script/link bytes map to logical locations")
{
    EXPECT_EQ(MetLocationFromLegacyByte(0), 0);
    EXPECT_EQ(MetLocationFromLegacyByte(249), 249);
    EXPECT_EQ(MetLocationFromLegacyByte(250), MAPSEC_NONE);
    EXPECT_EQ(MetLocationFromLegacyByte(METLOC_SPECIAL_EGG), MET_LOCATION_V2_SPECIAL_EGG);
    EXPECT_EQ(MetLocationFromLegacyByte(METLOC_IN_GAME_TRADE), MET_LOCATION_V2_IN_GAME_TRADE);
    EXPECT_EQ(MetLocationFromLegacyByte(METLOC_FATEFUL_ENCOUNTER), MET_LOCATION_V2_FATEFUL_ENCOUNTER);
    EXPECT_EQ(MetLocationToLegacyByte(MET_LOCATION_V2_FATEFUL_ENCOUNTER), METLOC_FATEFUL_ENCOUNTER);
    EXPECT_EQ(MetLocationToLegacyByte(MAPSEC_NONE), 250);
    EXPECT_EQ(MetLocationToLegacyByte(256), 250);
    EXPECT_EQ(MetLocationToLegacyByte(MET_LOCATION_V2_MAX), 250);
}

TEST("Met location: summary map-name lookup stays in range for sections 250..301")
{
    u8 *name = AllocZeroed(32);
    u16 location;

    for (location = 250; location <= MAPSEC_NONE; location++)
    {
        memset(name, 0xAA, 32);
        GetMapNameHandleAquaHideout(name, location);
        EXPECT_LT(StringLength(name), 32);
        if (location < MAPSEC_NONE)
            EXPECT_NE(name[0], EOS);
    }
    Free(name);
}

#if ROM_WORLD == 2

TEST("Met location: Cormoria CreateMon records wide sections exactly")
{
    struct Pokemon *mon = AllocZeroed(sizeof(*mon));
    u32 status = gSaveBlock3Ptr->coop.status_flags;
    u16 expected = 0;

    PARAMETRIZE { WARP_TO(MAP_CORMORIA_CARABRUE_TOWN); expected = MAPSEC_CORMORIA_CARABRUE_TOWN; }
    PARAMETRIZE { WARP_TO(MAP_CORMORIA_ROUTE2); expected = MAPSEC_CORMORIA_ROUTE2; }
    PARAMETRIZE { WARP_TO(MAP_CORMORIA_GASTREE_CITY); expected = MAPSEC_CORMORIA_GASTREE_CITY; }
    PARAMETRIZE { WARP_TO(MAP_CORMORIA_CHAMPIONSHIP_CORRIDOR); expected = MAPSEC_CORMORIA_CHAMPIONSHIP_CORRIDOR; }

    ASSUME(GetCurrentRegionMapSectionId() == expected);
    UseSaveStatus(V2_SAVE_STATUS);
    CreateMon(mon, SPECIES_WOBBUFFET, 5, 0x12345678, OTID_STRUCT_PRESET(0x87654321));
    EXPECT_EQ(GetMonMetLocation(mon), expected);
    EXPECT_EQ(GetMonData(mon, MON_DATA_SANITY_IS_BAD_EGG), FALSE);

    UseSaveStatus(status);
    Free(mon);
}

TEST("Met location: Cormoria section >= 256 does not alias a Hoenn section")
{
    struct Pokemon *mon = AllocZeroed(sizeof(*mon));
    u32 status = gSaveBlock3Ptr->coop.status_flags;
    u16 v2Location;

    WARP_TO(MAP_CORMORIA_GASTREE_CITY);
    ASSUME(GetCurrentRegionMapSectionId() >= 256);
    UseSaveStatus(V2_SAVE_STATUS);
    CreateMon(mon, SPECIES_WOBBUFFET, 5, 0x12345678, OTID_STRUCT_PRESET(0x87654321));
    EXPECT(GetBoxMonMetLocationV2(&mon->box, &v2Location));
    EXPECT_EQ(v2Location, MAPSEC_CORMORIA_GASTREE_CITY);
    EXPECT_NE(GetMonMetLocation(mon), MAPSEC_CORMORIA_GASTREE_CITY - 256);

    UseSaveStatus(status);
    Free(mon);
}

TEST("Met location: Hot Springs egg created in Cormoria Route 2 stays a special egg")
{
    struct Pokemon *mon = AllocZeroed(sizeof(*mon));
    u32 status = gSaveBlock3Ptr->coop.status_flags;

    // MAPSEC_CORMORIA_ROUTE2 (253) shares its byte with METLOC_SPECIAL_EGG.
    WARP_TO(MAP_CORMORIA_ROUTE2);
    ASSUME(GetCurrentRegionMapSectionId() == METLOC_SPECIAL_EGG);
    UseSaveStatus(V2_SAVE_STATUS);
    CreateEgg(mon, SPECIES_WYNAUT, TRUE);
    EXPECT_EQ(GetMonMetLocation(mon), MET_LOCATION_V2_SPECIAL_EGG);
    CreateEgg(mon, SPECIES_WYNAUT, FALSE);
    EXPECT_EQ(GetMonMetLocation(mon), MAPSEC_CORMORIA_ROUTE2);

    UseSaveStatus(status);
    Free(mon);
}

TEST("Met location: egg hatched in a Cormoria section records that section")
{
    u32 status = gSaveBlock3Ptr->coop.status_flags;
    u16 expected = 0;

    PARAMETRIZE { WARP_TO(MAP_CORMORIA_ROUTE2); expected = MAPSEC_CORMORIA_ROUTE2; }
    PARAMETRIZE { WARP_TO(MAP_CORMORIA_GASTREE_CITY); expected = MAPSEC_CORMORIA_GASTREE_CITY; }
    PARAMETRIZE { WARP_TO(MAP_CORMORIA_CHAMPIONSHIP_CORRIDOR); expected = MAPSEC_CORMORIA_CHAMPIONSHIP_CORRIDOR; }

    ASSUME(GetCurrentRegionMapSectionId() == expected);
    UseSaveStatus(V2_SAVE_STATUS);
    ZeroPlayerPartyMons();
    CreateEgg(&gParties[B_TRAINER_0][0], SPECIES_WYNAUT, TRUE);
    gSpecialVar_0x8004 = 0;
    ScriptHatchMon();
    EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_IS_EGG), FALSE);
    EXPECT_EQ(GetMonData(&gParties[B_TRAINER_0][0], MON_DATA_MET_LEVEL), 0);
    EXPECT_EQ(GetMonMetLocation(&gParties[B_TRAINER_0][0]), expected);

    UseSaveStatus(status);
}

#endif // ROM_WORLD == 2
