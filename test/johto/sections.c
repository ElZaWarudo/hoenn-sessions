#include "global.h"
#include "coop/region.h"
#include "regions.h"
#include "region_map.h"
#include "string_util.h"
#include "constants/characters.h"
#include "test/test.h"

TEST("Johto section registration keeps current identity boundaries")
{
    EXPECT_EQ(MAPSEC_NEW_BARK_TOWN, 209);
    EXPECT_EQ(MAPSEC_KANTO_VICTORY_ROAD, 132);
    EXPECT_EQ(JOHTO_MAPSEC_START, 209);
    EXPECT_EQ(MAPSEC_JOHTO_CHERRYGROVE_CITY, 210);
    EXPECT_EQ(JOHTO_MAPSEC_END, 249);
    EXPECT_EQ(MAPSEC_NONE, 250);
    EXPECT_EQ(MAPSEC_COUNT, 251);
    EXPECT_EQ(METLOC_SPECIAL_EGG, 253);
    EXPECT_EQ(METLOC_IN_GAME_TRADE, 254);
    EXPECT_EQ(METLOC_FATEFUL_ENCOUNTER, 255);
}

TEST("Johto sections agree across engine and co-op authorities")
{
    u32 section;
    enum CoopRegion normalized;
    for (section = JOHTO_MAPSEC_START; section <= JOHTO_MAPSEC_END; section++)
    {
        EXPECT_EQ(GetRegionForSectionId(section), REGION_JOHTO);
        EXPECT_EQ(CoopRegion_FromSectionId(section), COOP_REGION_JOHTO);
        normalized = COOP_REGION_UNSPECIFIED;
        EXPECT(CoopRegion_Normalize(&normalized, REGION_JOHTO, section));
        EXPECT_EQ(normalized, COOP_REGION_JOHTO);
        EXPECT(!CoopRegion_Normalize(&normalized, REGION_HOENN, section));
        EXPECT(!CoopRegion_Normalize(&normalized, REGION_KANTO, section));
    }
    EXPECT_EQ(GetRegionForSectionId(MAPSEC_LITTLEROOT_TOWN), REGION_HOENN);
    EXPECT_EQ(CoopRegion_FromSectionId(MAPSEC_LITTLEROOT_TOWN), COOP_REGION_HOENN);
    EXPECT_EQ(GetRegionForSectionId(MAPSEC_KANTO_VICTORY_ROAD), REGION_KANTO);
    EXPECT_EQ(CoopRegion_FromSectionId(MAPSEC_KANTO_VICTORY_ROAD), COOP_REGION_KANTO);
    EXPECT(!CoopRegion_Normalize(&normalized, REGION_JOHTO, MAPSEC_KANTO_VICTORY_ROAD));
}

TEST("Johto section registration rejects sentinel and special met locations")
{
    u32 section;
    enum CoopRegion normalized;
    for (section = MAPSEC_NONE; section <= 255; section++)
    {
        normalized = COOP_REGION_KANTO;
        EXPECT_EQ(CoopRegion_FromSectionId(section), COOP_REGION_UNSPECIFIED);
        EXPECT(!CoopRegion_TryFromSectionId(&normalized, section));
        EXPECT_EQ(normalized, COOP_REGION_KANTO);
        EXPECT(!CoopRegion_Normalize(&normalized, REGION_JOHTO, section));
    }
    EXPECT_EQ(CoopRegion_FromSectionId(0xFFFFFFFF), COOP_REGION_UNSPECIFIED);
}

/* Root appends the independent pinned-donor name oracle below. */

TEST("Johto section names resolve through the public runtime API")
{
    static const struct { u32 section; const u8 *name; } expected[] = {
        { MAPSEC_NEW_BARK_TOWN, COMPOUND_STRING("NEW BARK TOWN") },
        { MAPSEC_JOHTO_CHERRYGROVE_CITY, COMPOUND_STRING("CHERRYGROVE CITY") },
        { MAPSEC_JOHTO_VIOLET_CITY, COMPOUND_STRING("VIOLET CITY") },
        { MAPSEC_JOHTO_AZALEA_TOWN, COMPOUND_STRING("AZALEA TOWN") },
        { MAPSEC_JOHTO_GOLDENROD_CITY, COMPOUND_STRING("GOLDENROD CITY") },
        { MAPSEC_JOHTO_ECRUTEAK_CITY, COMPOUND_STRING("ECRUTEAK CITY") },
        { MAPSEC_JOHTO_OLIVINE_CITY, COMPOUND_STRING("OLIVINE CITY") },
        { MAPSEC_JOHTO_CIANWOOD_CITY, COMPOUND_STRING("CIANWOOD CITY") },
        { MAPSEC_JOHTO_SAFARI_ZONE_GATE, COMPOUND_STRING("SAFARI ZONE GATE") },
        { MAPSEC_JOHTO_MAHOGANY_TOWN, COMPOUND_STRING("MAHOGANY TOWN") },
        { MAPSEC_JOHTO_BLACKTHORN_CITY, COMPOUND_STRING("BLACKTHORN CITY") },
        { MAPSEC_JOHTO_ROUTE_29, COMPOUND_STRING("ROUTE 29") },
        { MAPSEC_JOHTO_ROUTE_30, COMPOUND_STRING("ROUTE 30") },
        { MAPSEC_JOHTO_ROUTE_31, COMPOUND_STRING("ROUTE 31") },
        { MAPSEC_JOHTO_ROUTE_32, COMPOUND_STRING("ROUTE 32") },
        { MAPSEC_JOHTO_ROUTE_33, COMPOUND_STRING("ROUTE 33") },
        { MAPSEC_JOHTO_ROUTE_34, COMPOUND_STRING("ROUTE 34") },
        { MAPSEC_JOHTO_ROUTE_35, COMPOUND_STRING("ROUTE 35") },
        { MAPSEC_JOHTO_ROUTE_36, COMPOUND_STRING("ROUTE 36") },
        { MAPSEC_JOHTO_ROUTE_37, COMPOUND_STRING("ROUTE 37") },
        { MAPSEC_JOHTO_ROUTE_38, COMPOUND_STRING("ROUTE 38") },
        { MAPSEC_JOHTO_ROUTE_39, COMPOUND_STRING("ROUTE 39") },
        { MAPSEC_JOHTO_ROUTE_40, COMPOUND_STRING("ROUTE 40") },
        { MAPSEC_JOHTO_ROUTE_41, COMPOUND_STRING("ROUTE 41") },
        { MAPSEC_JOHTO_ROUTE_42, COMPOUND_STRING("ROUTE 42") },
        { MAPSEC_JOHTO_ROUTE_43, COMPOUND_STRING("ROUTE 43") },
        { MAPSEC_JOHTO_ROUTE_44, COMPOUND_STRING("ROUTE 44") },
        { MAPSEC_JOHTO_ROUTE_45, COMPOUND_STRING("ROUTE 45") },
        { MAPSEC_JOHTO_ROUTE_46, COMPOUND_STRING("ROUTE 46") },
        { MAPSEC_JOHTO_ROUTE_47, COMPOUND_STRING("ROUTE 47") },
        { MAPSEC_JOHTO_ROUTE_48, COMPOUND_STRING("ROUTE 48") },
        { MAPSEC_JOHTO_ROUTE_26, COMPOUND_STRING("ROUTE 26") },
        { MAPSEC_JOHTO_ROUTE_27, COMPOUND_STRING("ROUTE 27") },
        { MAPSEC_JOHTO_ROUTE_28, COMPOUND_STRING("ROUTE 28") },
        { MAPSEC_JOHTO_LAKE_OF_RAGE, COMPOUND_STRING("LAKE OF RAGE") },
        { MAPSEC_JOHTO_NATIONAL_PARK, COMPOUND_STRING("NATIONAL PARK") },
        { MAPSEC_JOHTO_RUINS_OF_ALPH, COMPOUND_STRING("RUINS OF ALPH") },
        { MAPSEC_JOHTO_MT_SILVER, COMPOUND_STRING("MT. SILVER") },
        { MAPSEC_JOHTO_ILEX_FOREST, COMPOUND_STRING("ILEX FOREST") },
        { MAPSEC_JOHTO_WHIRL_ISLANDS, COMPOUND_STRING("WHIRL ISLANDS") },
        { MAPSEC_JOHTO_SS_AQUA, COMPOUND_STRING("S.S. AQUA") },
    };
    u32 i;
    u8 actual[64];
    EXPECT_EQ(ARRAY_COUNT(expected), 41);
    for (i = 0; i < ARRAY_COUNT(expected); i++)
    {
        actual[0] = EOS;
        GetMapName(actual, expected[i].section, 0);
        EXPECT(actual[0] != EOS);
        EXPECT_EQ(StringCompare(actual, expected[i].name), 0);
    }
}
