#include "global.h"
#include "johto/wild.h"
#include "test/test.h"
#include "wild_encounter.h"

#include "rtc.h"
#include "constants/maps.h"

struct JohtoWildMapLink
{
    u8 group;
    u8 num;
};

/* Fixed source-grounded map linkage captured from data/johto/wild_encounters.json. */
static const struct JohtoWildMapLink sJohtoWildSourceMaps[] =
{
    { 75, 0 }, // MAP_NEW_BARK_TOWN
    { 75, 1 }, // MAP_CHERRYGROVE_CITY
    { 75, 2 }, // MAP_VIOLET_CITY
    { 75, 3 }, // MAP_AZALEA_TOWN
    { 75, 4 }, // MAP_GOLDENROD_CITY
    { 75, 5 }, // MAP_ECRUTEAK_CITY
    { 75, 6 }, // MAP_OLIVINE_CITY
    { 75, 7 }, // MAP_CIANWOOD_CITY
    { 75, 8 }, // MAP_SAFARI_ZONE_GATE
    { 75, 9 }, // MAP_MAHOGANYTOWN
    { 75, 10 }, // MAP_BLACKTHORN_CITY
    { 75, 11 }, // MAP_ROUTE29
    { 75, 12 }, // MAP_ROUTE30
    { 75, 13 }, // MAP_ROUTE31
    { 75, 14 }, // MAP_ROUTE32
    { 75, 15 }, // MAP_ROUTE33
    { 75, 16 }, // MAP_ROUTE34
    { 75, 17 }, // MAP_ROUTE35
    { 75, 18 }, // MAP_ROUTE36
    { 75, 19 }, // MAP_ROUTE37
    { 75, 20 }, // MAP_ROUTE38
    { 75, 21 }, // MAP_ROUTE39
    { 75, 22 }, // MAP_ROUTE40
    { 75, 23 }, // MAP_ROUTE41
    { 75, 24 }, // MAP_ROUTE42
    { 75, 25 }, // MAP_ROUTE43
    { 75, 26 }, // MAP_ROUTE44
    { 75, 27 }, // MAP_ROUTE45
    { 75, 28 }, // MAP_ROUTE46
    { 75, 29 }, // MAP_ROUTE47
    { 75, 30 }, // MAP_ROUTE48
    { 75, 31 }, // MAP_ROUTE26
    { 75, 33 }, // MAP_ROUTE27
    { 75, 34 }, // MAP_ROUTE28
    { 75, 101 }, // MAP_OLIVINE_CITY_PORT_OUTSIDE
    { 76, 21 }, // MAP_DARK_CAVE_SOUTH_SIDE
    { 76, 22 }, // MAP_DARK_CAVE_NORTH_SIDE
    { 76, 24 }, // MAP_SPROUT_TOWER_2F
    { 76, 25 }, // MAP_SPROUT_TOWER_3F
    { 76, 26 }, // MAP_RUINS_OF_ALPH_OUTSIDE
    { 76, 27 }, // MAP_RUINS_OF_ALPH_B1F
    { 76, 29 }, // MAP_UNION_CAVE_1F
    { 76, 30 }, // MAP_UNION_CAVE_B1F
    { 76, 31 }, // MAP_UNION_CAVE_B2F
    { 76, 32 }, // MAP_SLOWPOKE_WELL_B1F
    { 76, 33 }, // MAP_SLOWPOKE_WELL_B2F
    { 76, 34 }, // MAP_ILEX_FOREST
    { 76, 35 }, // MAP_NATIONAL_PARK_NORMAL
    { 76, 36 }, // MAP_NATIONAL_PARK_BUG_CONTEST
    { 76, 37 }, // MAP_BURNED_TOWER_1F
    { 76, 38 }, // MAP_BURNED_TOWER_B1F
    { 76, 39 }, // MAP_CLIFF_EDGE_GATE
    { 76, 40 }, // MAP_MT_MORTAR_1F_SOUTH
    { 76, 41 }, // MAP_MT_MORTAR_1F_NORTH
    { 76, 42 }, // MAP_MT_MORTAR_2F
    { 76, 43 }, // MAP_MT_MORTAR_B1F
    { 76, 44 }, // MAP_LAKE_OF_RAGE
    { 76, 46 }, // MAP_ICE_PATH_1F
    { 76, 47 }, // MAP_ICE_PATH_B1F
    { 76, 48 }, // MAP_ICE_PATH_B2F
    { 76, 49 }, // MAP_ICE_PATH_B3F
    { 76, 50 }, // MAP_ICE_PATH_B4F
    { 76, 52 }, // MAP_DRAGONS_DEN_CAVERN
    { 76, 54 }, // MAP_WHIRL_ISLANDS_1F
    { 76, 55 }, // MAP_WHIRL_ISLANDS_B1F
    { 76, 56 }, // MAP_WHIRL_ISLANDS_B1F_INNER
    { 76, 57 }, // MAP_WHIRL_ISLANDS_B2F
    { 76, 58 }, // MAP_WHIRL_ISLANDS_B3F
    { 76, 59 }, // MAP_WHIRL_ISLANDS_DESCENT
    { 76, 63 }, // MAP_TIN_TOWER_3F
    { 76, 64 }, // MAP_TIN_TOWER_4F
    { 76, 65 }, // MAP_TIN_TOWER_5F
    { 76, 66 }, // MAP_TIN_TOWER_6F
    { 76, 67 }, // MAP_TIN_TOWER_7F
    { 76, 68 }, // MAP_TIN_TOWER_8F
    { 76, 69 }, // MAP_TIN_TOWER_9F
    { 76, 72 }, // MAP_TOHJO_FALLS_CAVERN
    { 76, 74 }, // MAP_MT_SILVER_OUTSIDE
    { 76, 75 }, // MAP_MT_SILVER_1F_ITEM_ROOM
    { 76, 76 }, // MAP_MT_SILVER_1F_WATERFALL_ROOM
    { 76, 77 }, // MAP_MT_SILVER_1F_MOLTRES_ROOM
    { 76, 78 }, // MAP_MT_SILVER_MOUNTAIN_SIDE
    { 76, 79 }, // MAP_MT_SILVER_2F
    { 76, 80 }, // MAP_MT_SILVER_3F
    { 76, 81 }, // MAP_MT_SILVER_SNOW
    { 76, 84 }, // MAP_CLIFF_EDGE_CAVE
    { 76, 86 }, // MAP_ROCKET_HIDEOUT_B1F
    { 76, 104 }, // MAP_SAFARI_ZONE_TOP_LEFT
    { 76, 105 }, // MAP_SAFARI_ZONE_LOW_MID
    { 76, 107 }, // MAP_SAFARI_ZONE_LOW_LEFT
    { 76, 108 }, // MAP_SAFARI_ZONE_LOW_RIGHT
    { 76, 109 }, // MAP_SAFARI_ZONE_TOP_MID
    { 76, 110 }, // MAP_SAFARI_ZONE_TOP_RIGHT
};

static void SetTestMap(u8 mapGroup, u8 mapNum)
{
    gSaveBlock1Ptr->location.mapGroup = mapGroup;
    gSaveBlock1Ptr->location.mapNum = mapNum;
}

TEST("Johto wild selector uses the pinned six and eighteen hour boundaries")
{
    EXPECT_EQ(JohtoWild_TimeForHour(0), TIME_NIGHT);
    EXPECT_EQ(JohtoWild_TimeForHour(5), TIME_NIGHT);
    EXPECT_EQ(JohtoWild_TimeForHour(6), TIME_DAY);
    EXPECT_EQ(JohtoWild_TimeForHour(17), TIME_DAY);
    EXPECT_EQ(JohtoWild_TimeForHour(18), TIME_NIGHT);
    EXPECT_EQ(JohtoWild_TimeForHour(23), TIME_NIGHT);
    EXPECT_EQ(JohtoWild_TimeForHour(24), TIME_DAY);
}

TEST("Johto public lookup resolves every source-linked map")
{
    u8 savedGroup = gSaveBlock1Ptr->location.mapGroup;
    u8 savedNum = gSaveBlock1Ptr->location.mapNum;
    bool8 allFound = TRUE;
    u32 i;

    for (i = 0; i < sizeof(sJohtoWildSourceMaps) / sizeof(sJohtoWildSourceMaps[0]); i++)
    {
        u16 headerId;

        SetTestMap(sJohtoWildSourceMaps[i].group, sJohtoWildSourceMaps[i].num);
        headerId = GetCurrentMapWildMonHeaderId();
        if (headerId == HEADER_NONE
         || gWildMonHeaders[headerId].mapGroup != sJohtoWildSourceMaps[i].group
         || gWildMonHeaders[headerId].mapNum != sJohtoWildSourceMaps[i].num)
            allFound = FALSE;
    }

    gSaveBlock1Ptr->location.mapGroup = savedGroup;
    gSaveBlock1Ptr->location.mapNum = savedNum;
    EXPECT(allFound);
}

TEST("Johto source tables preserve representative slots and effective host widths")
{
    u8 savedGroup = gSaveBlock1Ptr->location.mapGroup;
    u8 savedNum = gSaveBlock1Ptr->location.mapNum;
    bool8 sourceMatches = FALSE;
    u16 headerId;

    SetTestMap(75, 0); // MAP_NEW_BARK_TOWN, fixed source linkage.
    headerId = GetCurrentMapWildMonHeaderId();
    if (headerId != HEADER_NONE)
    {
        const struct WildPokemonHeader *header = &gWildMonHeaders[headerId];
        const struct WildPokemonInfo *land = header->encounterTypes[TIME_DAY].landMonsInfo;
        const struct WildPokemonInfo *water = header->encounterTypes[TIME_DAY].waterMonsInfo;
        const struct WildPokemonInfo *rock = header->encounterTypes[TIME_DAY].rockSmashMonsInfo;
        const struct WildPokemonInfo *fishing = header->encounterTypes[TIME_DAY].fishingMonsInfo;

        sourceMatches = land != NULL && water != NULL && rock != NULL && fishing != NULL
            && land->encounterRate == 0
            && land->wildPokemon[0].species == SPECIES_NONE
            && land->wildPokemon[11].species == SPECIES_NONE
            && water->encounterRate == 7
            && water->wildPokemon[0].minLevel == 25
            && water->wildPokemon[0].maxLevel == 29
            && water->wildPokemon[0].species == SPECIES_TENTACOOL
            && water->wildPokemon[4].species == SPECIES_TENTACOOL
            && water->wildPokemon[11].species == SPECIES_TENTACOOL
            && rock->encounterRate == 60
            && rock->wildPokemon[0].species == SPECIES_PINECO
            && rock->wildPokemon[4].species == SPECIES_LEDYBA
            && fishing->encounterRate == 30
            && fishing->wildPokemon[0].species == SPECIES_MAGIKARP
            && fishing->wildPokemon[9].species == SPECIES_LANTURN;
    }

    gSaveBlock1Ptr->location.mapGroup = savedGroup;
    gSaveBlock1Ptr->location.mapNum = savedNum;
    EXPECT(sourceMatches);
    EXPECT_EQ(LAND_WILD_COUNT, 12);
    EXPECT_EQ(WATER_WILD_COUNT, 5);
    EXPECT_EQ(ROCK_WILD_COUNT, 5);
    EXPECT_EQ(FISH_WILD_COUNT, 10);
}

TEST("Johto live RTC selector drives Johto and preserves the ordinary host path")
{
    u8 savedGroup = gSaveBlock1Ptr->location.mapGroup;
    u8 savedNum = gSaveBlock1Ptr->location.mapNum;
    struct Time savedLocalTime = gLocalTime;
    struct Time savedLocalTimeOffset = gSaveBlock2Ptr->localTimeOffset;
#if OW_USE_FAKE_RTC
    struct SiiRtcInfo savedFakeRtc = gSaveBlock3Ptr->fakeRTC;
#endif
    const u8 hours[] = { 5, 6, 17, 18 };
    const enum TimeOfDay expected[] = { TIME_NIGHT, TIME_DAY, TIME_DAY, TIME_NIGHT };
    enum TimeOfDay actual[sizeof(hours) / sizeof(hours[0])];
    bool8 johtoHeaderFound;
    u16 johtoHeaderId;
    u16 ordinaryHeaderId;
    bool8 ordinaryPath;
    u32 i;

    SetTestMap(75, 11); // MAP_ROUTE29.
    johtoHeaderId = GetCurrentMapWildMonHeaderId();
    johtoHeaderFound = johtoHeaderId != HEADER_NONE;
    RtcInitLocalTimeOffset(0, 0);
    for (i = 0; i < sizeof(hours) / sizeof(hours[0]); i++)
    {
        RtcCalcLocalTimeOffset(0, hours[i], 30, 0);
        actual[i] = johtoHeaderFound
            ? GetTimeOfDayForEncounters(johtoHeaderId, WILD_AREA_LAND)
            : TIME_OF_DAY_DEFAULT;
    }

    SetTestMap(MAP_GROUP(MAP_ROUTE101), MAP_NUM(MAP_ROUTE101));
    ordinaryHeaderId = GetCurrentMapWildMonHeaderId();
    ordinaryPath = ordinaryHeaderId != HEADER_NONE
        && gWildMonHeaders[ordinaryHeaderId].mapGroup == MAP_GROUP(MAP_ROUTE101)
        && gWildMonHeaders[ordinaryHeaderId].mapNum == MAP_NUM(MAP_ROUTE101)
        && GetTimeOfDayForEncounters(ordinaryHeaderId, WILD_AREA_LAND) == TIME_OF_DAY_DEFAULT;

    gSaveBlock1Ptr->location.mapGroup = savedGroup;
    gSaveBlock1Ptr->location.mapNum = savedNum;
    gSaveBlock2Ptr->localTimeOffset = savedLocalTimeOffset;
    gLocalTime = savedLocalTime;
#if OW_USE_FAKE_RTC
    gSaveBlock3Ptr->fakeRTC = savedFakeRtc;
#endif

    EXPECT(johtoHeaderFound);
    for (i = 0; i < sizeof(hours) / sizeof(hours[0]); i++)
        EXPECT_EQ(actual[i], expected[i]);
    EXPECT(ordinaryPath);
}

TEST("Johto wild headers preserve day night source data and fallback slots")
{
    u8 savedGroup = gSaveBlock1Ptr->location.mapGroup;
    u8 savedNum = gSaveBlock1Ptr->location.mapNum;
    u16 headerId;
    const struct WildPokemonHeader *header;

    SetTestMap(75, 11); // Route 29
    headerId = GetCurrentMapWildMonHeaderId();
    EXPECT_NE(headerId, HEADER_NONE);
    header = &gWildMonHeaders[headerId];
    EXPECT_EQ(header->encounterTypes[TIME_DAY].landMonsInfo->encounterRate, 20);
    EXPECT_EQ(header->encounterTypes[TIME_DAY].landMonsInfo->wildPokemon[0].species, SPECIES_PIDGEY);
    EXPECT_EQ(header->encounterTypes[TIME_NIGHT].landMonsInfo->wildPokemon[0].species, SPECIES_HOOTHOOT);
    EXPECT_EQ(header->encounterTypes[TIME_MORNING].landMonsInfo->wildPokemon[0].species, SPECIES_PIDGEY);
    EXPECT_EQ(header->encounterTypes[TIME_EVENING].landMonsInfo->wildPokemon[0].species, SPECIES_HOOTHOOT);

    SetTestMap(75, 3); // Azalea Town has an explicit day table only.
    headerId = GetCurrentMapWildMonHeaderId();
    EXPECT_NE(headerId, HEADER_NONE);
    header = &gWildMonHeaders[headerId];
    EXPECT_EQ(header->encounterTypes[TIME_EVENING].waterMonsInfo, header->encounterTypes[TIME_DAY].waterMonsInfo);

    SetTestMap(75, 127); // Assigned Johto range, no encounter table.
    EXPECT_EQ(GetCurrentMapWildMonHeaderId(), HEADER_NONE);
    EXPECT_EQ(GetTimeOfDayForEncounters(HEADER_NONE, WILD_AREA_LAND), TIME_OF_DAY_DEFAULT);

    gSaveBlock1Ptr->location.mapGroup = savedGroup;
    gSaveBlock1Ptr->location.mapNum = savedNum;
}
