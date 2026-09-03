#include "global.h"
#include "coop/region.h"
#include "fieldmap.h"
#include "constants/region_map_sections.h"
#include "field_player_avatar.h"
#include "regions.h"

static bool8 IsKnownMapSection(u32 section_id)
{
    /* MAPSEC_NONE is the generated sentinel, not a valid location. The
     * explicit count also rejects unused values between the table and the
     * sentinel. */
    return section_id < MAPSEC_COUNT && section_id != MAPSEC_NONE;
}

static enum Region EngineRegion_FromMapHeaderValue(u8 value)
{
    switch (value)
    {
    case COOP_MAP_ENGINE_REGION_HOENN:
        return REGION_HOENN;
    case COOP_MAP_ENGINE_REGION_KANTO:
        return REGION_KANTO;
    default:
        /* Also accept an already-expanded C enum value for callers that
         * construct a MapHeader in tests or tooling. Generated assembly uses
         * only the stable values above. */
        if (value == REGION_HOENN)
            return REGION_HOENN;
        if (value == REGION_KANTO)
            return REGION_KANTO;
        return REGION_NONE;
    }
}

enum CoopRegion CoopRegion_FromEngineRegion(enum Region region)
{
    switch (region)
    {
    case REGION_HOENN:
        return COOP_REGION_HOENN;
    case REGION_KANTO:
        return COOP_REGION_KANTO;
    case REGION_JOHTO:
        return COOP_REGION_JOHTO;
    default:
        return COOP_REGION_UNSPECIFIED;
    }
}

enum CoopRegion CoopRegion_FromSectionId(u32 section_id)
{
    if (!IsKnownMapSection(section_id))
        return COOP_REGION_UNSPECIFIED;

    if (section_id >= KANTO_MAPSEC_START && section_id < MAPSEC_SPECIAL_AREA)
    {
        if (GetKantoSubregion(section_id) != KANTO_SUBREGION_KANTO)
            return COOP_REGION_SEVII;
        return COOP_REGION_KANTO;
    }

    /* All remaining generated sections are Hoenn, including special areas. */
    return COOP_REGION_HOENN;
}

bool8 CoopRegion_Normalize(enum CoopRegion *out, enum Region engine_region, u32 section_id)
{
    enum CoopRegion section_region;

    if (out == NULL)
        return FALSE;

    switch (engine_region)
    {
    case REGION_JOHTO:
        /* Johto has an identity ordinal but no playable map registry in the
         * pinned engine. Do not manufacture a location for an unknown map. */
        return FALSE;
    case REGION_HOENN:
        section_region = CoopRegion_FromSectionId(section_id);
        if (section_region != COOP_REGION_HOENN)
            return FALSE;
        *out = section_region;
        return TRUE;
    case REGION_KANTO:
        section_region = CoopRegion_FromSectionId(section_id);
        if (section_region != COOP_REGION_KANTO && section_region != COOP_REGION_SEVII)
        {
            /* FRLG's special-area maps retain MAPSEC_SPECIAL_AREA even
             * though that section is outside the ordinary Kanto range. The
             * generated engine region is the authority for this case. */
            if (section_id != MAPSEC_SPECIAL_AREA)
                return FALSE;
            section_region = COOP_REGION_KANTO;
        }
        *out = section_region;
        return TRUE;
    default:
        return FALSE;
    }
}

bool8 CoopRegion_TryFromSectionId(enum CoopRegion *out, u32 section_id)
{
    enum CoopRegion region;

    if (out == NULL)
        return FALSE;

    region = CoopRegion_FromSectionId(section_id);
    if (region == COOP_REGION_UNSPECIFIED)
        return FALSE;

    *out = region;
    return TRUE;
}

bool8 CoopRegion_TryGetActive(enum CoopRegion *out)
{
    if (out == NULL)
        return FALSE;

    return CoopRegion_Normalize(out,
                                EngineRegion_FromMapHeaderValue(gMapHeader.engineRegion),
                                gMapHeader.regionMapSectionId);
}

bool8 CoopRegion_IsValid(enum CoopRegion region)
{
    return region >= COOP_REGION_HOENN && region < COOP_REGION_COUNT;
}

bool8 CoopWorldLocation_Export(struct WorldLocation *out)
{
    enum CoopRegion region;
    s32 map_group;
    s32 map_number;
    s16 x;
    s16 y;

    if (out == NULL || gSaveBlock1Ptr == NULL)
        return FALSE;

    if (gPlayerAvatar.objectEventId >= OBJECT_EVENTS_COUNT
     || !gObjectEvents[gPlayerAvatar.objectEventId].active
     || !gObjectEvents[gPlayerAvatar.objectEventId].isPlayer)
        return FALSE;

    if (!CoopRegion_TryGetActive(&region))
        return FALSE;

    /* SaveBlock's map components are signed engine fields; reject sentinels
     * before widening them into the unsigned bridge representation. */
    map_group = gSaveBlock1Ptr->location.mapGroup;
    map_number = gSaveBlock1Ptr->location.mapNum;
    if (map_group < 0 || map_group > 0xFFFF || map_number < 0 || map_number > 0xFFFF)
        return FALSE;
    PlayerGetDestCoords(&x, &y);

    /* PlayerGetDestCoords and ObjectEvent coordinates include the map's
     * seven-tile collision border.  Presence coordinates are deliberately
     * map-local so a seamless connection cannot leak the engine offset onto
     * the wire.  Widen before subtracting to keep the signed boundary safe. */
    if ((s32)x - MAP_OFFSET < -32768 || (s32)x - MAP_OFFSET > 32767
     || (s32)y - MAP_OFFSET < -32768 || (s32)y - MAP_OFFSET > 32767)
        return FALSE;

    out->region = (u8)region;
    out->reserved = 0;
    out->map_group = (u16)map_group;
    out->map_number = (u16)map_number;
    out->x = (s16)((s32)x - MAP_OFFSET);
    out->y = (s16)((s32)y - MAP_OFFSET);
    return TRUE;
}
