#include "global.h"
#include "coop/region.h"
#include "constants/region_map_sections.h"
#include "test/test.h"

_Static_assert(sizeof(struct WorldLocation) == 10, "tested world location ABI size");
_Static_assert(sizeof(struct MapHeader) == 0x1C, "tested map header size");
_Static_assert(offsetof(struct MapHeader, engineRegion) == 0x19, "tested engine region offset");
_Static_assert(offsetof(struct MapHeader, battleType) == 0x1B, "tested battle type offset");
_Static_assert(COOP_MAP_ENGINE_REGION_HOENN == 0, "Hoenn map header byte");
_Static_assert(COOP_MAP_ENGINE_REGION_KANTO == 1, "Kanto map header byte");
_Static_assert(offsetof(struct WorldLocation, region) == 0, "tested location region offset");
_Static_assert(offsetof(struct WorldLocation, map_group) == 2, "tested location map group offset");
_Static_assert(offsetof(struct WorldLocation, map_number) == 4, "tested location map number offset");
_Static_assert(offsetof(struct WorldLocation, x) == 6, "tested location x offset");
_Static_assert(offsetof(struct WorldLocation, y) == 8, "tested location y offset");

TEST("Cloud Coop region IDs adapt explicitly from engine regions")
{
    EXPECT_EQ(CoopRegion_FromEngineRegion(REGION_HOENN), COOP_REGION_HOENN);
    EXPECT_EQ(CoopRegion_FromEngineRegion(REGION_KANTO), COOP_REGION_KANTO);
    EXPECT_EQ(CoopRegion_FromEngineRegion(REGION_JOHTO), COOP_REGION_JOHTO);
    EXPECT_EQ(CoopRegion_FromEngineRegion(REGION_NONE), COOP_REGION_UNSPECIFIED);
    EXPECT_EQ(CoopRegion_FromEngineRegion(REGION_UNOVA), COOP_REGION_UNSPECIFIED);

    EXPECT(!CoopRegion_IsValid(COOP_REGION_UNSPECIFIED));
    EXPECT(CoopRegion_IsValid(COOP_REGION_HOENN));
    EXPECT(CoopRegion_IsValid(COOP_REGION_KANTO));
    EXPECT(CoopRegion_IsValid(COOP_REGION_JOHTO));
    EXPECT(CoopRegion_IsValid(COOP_REGION_SEVII));
    EXPECT(!CoopRegion_IsValid(COOP_REGION_COUNT));
}

TEST("Cloud Coop map-section adapter distinguishes Hoenn Kanto and Sevii")
{
    enum CoopRegion region = COOP_REGION_UNSPECIFIED;

    EXPECT_EQ(CoopRegion_FromSectionId(MAPSEC_LITTLEROOT_TOWN), COOP_REGION_HOENN);
    EXPECT_EQ(CoopRegion_FromSectionId(MAPSEC_PALLET_TOWN), COOP_REGION_KANTO);
    EXPECT_EQ(CoopRegion_FromSectionId(MAPSEC_ONE_ISLAND), COOP_REGION_SEVII);
    EXPECT_EQ(CoopRegion_FromSectionId(MAPSEC_SPECIAL_AREA), COOP_REGION_HOENN);
    EXPECT_EQ(CoopRegion_FromSectionId(MAPSEC_NONE), COOP_REGION_UNSPECIFIED);
    EXPECT_EQ(CoopRegion_FromSectionId(MAPSEC_COUNT), COOP_REGION_UNSPECIFIED);
    EXPECT_EQ(CoopRegion_FromSectionId(0xFFFFFFFFu), COOP_REGION_UNSPECIFIED);

    EXPECT(CoopRegion_TryFromSectionId(&region, MAPSEC_ONE_ISLAND));
    EXPECT_EQ(region, COOP_REGION_SEVII);
    EXPECT(!CoopRegion_TryFromSectionId(&region, MAPSEC_NONE));
    EXPECT_EQ(region, COOP_REGION_SEVII);
    EXPECT(!CoopRegion_TryFromSectionId(NULL, MAPSEC_LITTLEROOT_TOWN));
}

TEST("Cloud Coop normalization rejects contradictory engine and map regions")
{
    enum CoopRegion region;

    region = COOP_REGION_UNSPECIFIED;
    EXPECT(CoopRegion_Normalize(&region, REGION_HOENN, MAPSEC_LITTLEROOT_TOWN));
    EXPECT_EQ(region, COOP_REGION_HOENN);

    region = COOP_REGION_UNSPECIFIED;
    EXPECT(CoopRegion_Normalize(&region, REGION_KANTO, MAPSEC_PALLET_TOWN));
    EXPECT_EQ(region, COOP_REGION_KANTO);

    region = COOP_REGION_UNSPECIFIED;
    EXPECT(CoopRegion_Normalize(&region, REGION_KANTO, MAPSEC_ONE_ISLAND));
    EXPECT_EQ(region, COOP_REGION_SEVII);

    region = COOP_REGION_UNSPECIFIED;
    EXPECT(CoopRegion_Normalize(&region, REGION_KANTO, MAPSEC_SPECIAL_AREA));
    EXPECT_EQ(region, COOP_REGION_KANTO);

    region = COOP_REGION_UNSPECIFIED;
    EXPECT(CoopRegion_Normalize(&region, REGION_HOENN, MAPSEC_SPECIAL_AREA));
    EXPECT_EQ(region, COOP_REGION_HOENN);

    region = COOP_REGION_UNSPECIFIED;
    EXPECT(!CoopRegion_Normalize(&region, REGION_JOHTO, MAPSEC_NONE));
    EXPECT_EQ(region, COOP_REGION_UNSPECIFIED);

    region = COOP_REGION_SEVII;
    EXPECT(!CoopRegion_Normalize(&region, REGION_HOENN, MAPSEC_PALLET_TOWN));
    EXPECT_EQ(region, COOP_REGION_SEVII);
    EXPECT(!CoopRegion_Normalize(&region, REGION_KANTO, MAPSEC_LITTLEROOT_TOWN));
    EXPECT_EQ(region, COOP_REGION_SEVII);
    EXPECT(!CoopRegion_Normalize(&region, REGION_KANTO, MAPSEC_AQUA_HIDEOUT));
    EXPECT_EQ(region, COOP_REGION_SEVII);
    EXPECT(!CoopRegion_Normalize(&region, REGION_NONE, MAPSEC_LITTLEROOT_TOWN));
    EXPECT_EQ(region, COOP_REGION_SEVII);
    EXPECT(!CoopRegion_Normalize(NULL, REGION_HOENN, MAPSEC_LITTLEROOT_TOWN));
}

TEST("Cloud Coop active region derives Kanto and Sevii from the current map")
{
    enum CoopRegion region = COOP_REGION_UNSPECIFIED;

    gMapHeader.engineRegion = COOP_MAP_ENGINE_REGION_HOENN;
    gMapHeader.regionMapSectionId = MAPSEC_LITTLEROOT_TOWN;
    EXPECT(CoopRegion_TryGetActive(&region));
    EXPECT_EQ(region, COOP_REGION_HOENN);

    gMapHeader.engineRegion = COOP_MAP_ENGINE_REGION_KANTO;
    gMapHeader.regionMapSectionId = MAPSEC_PALLET_TOWN;
    EXPECT(CoopRegion_TryGetActive(&region));
    EXPECT_EQ(region, COOP_REGION_KANTO);

    gMapHeader.regionMapSectionId = MAPSEC_ONE_ISLAND;
    EXPECT(CoopRegion_TryGetActive(&region));
    EXPECT_EQ(region, COOP_REGION_SEVII);

    gMapHeader.engineRegion = REGION_JOHTO;
    gMapHeader.regionMapSectionId = MAPSEC_NONE;
    EXPECT(!CoopRegion_TryGetActive(&region));
    EXPECT_EQ(region, COOP_REGION_SEVII);

    gMapHeader.engineRegion = COOP_MAP_ENGINE_REGION_HOENN;
    gMapHeader.regionMapSectionId = MAPSEC_PALLET_TOWN;
    EXPECT(!CoopRegion_TryGetActive(&region));
    EXPECT_EQ(region, COOP_REGION_SEVII);
    EXPECT(!CoopRegion_TryGetActive(NULL));
}
