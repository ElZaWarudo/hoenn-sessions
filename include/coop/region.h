#ifndef GUARD_COOP_REGION_H
#define GUARD_COOP_REGION_H

#include "gba/defines.h"
#include "gba/types.h"
#include "constants/regions.h"

/*
 * These values are part of the co-op ABI.  They intentionally do not share
 * the values of enum Region: the engine's enum is allowed to grow and its
 * ordinal values are not serialized across the bridge.
 */
enum CoopRegion
{
    COOP_REGION_UNSPECIFIED = 0,
    COOP_REGION_HOENN = 1,
    COOP_REGION_KANTO = 2,
    COOP_REGION_JOHTO = 3,
    COOP_REGION_SEVII = 4,
    COOP_REGION_COUNT = 5,
};

/* Stable values emitted in map-header assembly. These are deliberately not
 * the ordinals of enum Region, which is a C implementation detail. */
enum CoopMapEngineRegion
{
    COOP_MAP_ENGINE_REGION_HOENN = 0,
    COOP_MAP_ENGINE_REGION_KANTO = 1,
};

#define COOP_PROGRESS_REGION_COUNT 4

/*
 * The bridge boundary deliberately contains only fixed-width scalar fields.
 * In particular, no MapHeader, WarpData, or ObjectEvent is serialized.
 */
struct WorldLocation
{
    /* 0x00 */ u8 region;
    /* 0x01 */ u8 reserved;
    /* 0x02 */ u16 map_group;
    /* 0x04 */ u16 map_number;
    /* 0x06 */ s16 x;
    /* 0x08 */ s16 y;
} PACKED;

typedef struct WorldLocation WorldLocation;

_Static_assert(sizeof(struct WorldLocation) == 10, "WorldLocation ABI size");
_Static_assert(offsetof(struct WorldLocation, region) == 0, "WorldLocation region offset");
_Static_assert(offsetof(struct WorldLocation, map_group) == 2, "WorldLocation map group offset");
_Static_assert(offsetof(struct WorldLocation, map_number) == 4, "WorldLocation map number offset");
_Static_assert(offsetof(struct WorldLocation, x) == 6, "WorldLocation x offset");
_Static_assert(offsetof(struct WorldLocation, y) == 8, "WorldLocation y offset");

/* Return UNSPECIFIED for an engine region that is not represented by the ABI. */
enum CoopRegion CoopRegion_FromEngineRegion(enum Region region);

/* Return UNSPECIFIED for a map section outside the generated map-section set. */
enum CoopRegion CoopRegion_FromSectionId(u32 section_id);

/*
 * Normalize an engine region plus its active map section.  A Kanto engine
 * region may normalize to Sevii.  Unknown combinations are rejected and do
 * not silently become Hoenn.
 */
bool8 CoopRegion_Normalize(enum CoopRegion *out, enum Region engine_region, u32 section_id);
bool8 CoopRegion_TryFromSectionId(enum CoopRegion *out, u32 section_id);
bool8 CoopRegion_IsValid(enum CoopRegion region);

/* Export the active map identity and player coordinates at the ABI boundary. */
bool8 CoopWorldLocation_Export(struct WorldLocation *out);

/* Compatibility spelling for callers that use the shorter adapter name. */
#define WorldLocation_Export CoopWorldLocation_Export

#endif /* GUARD_COOP_REGION_H */
