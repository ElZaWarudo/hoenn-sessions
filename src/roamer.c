#include "global.h"
#include "event_data.h"
#include "ow_synchronize.h"
#include "pokemon.h"
#include "random.h"
#include "roamer.h"

enum
{
    MAP_GRP, // map group
    MAP_NUM, // map number
};

struct RoamerLocation
{
    u8 mapGroup;
    u8 mapNum;
};

#define ROAMER(index) (&gSaveBlock1Ptr->roamer[index])
EWRAM_DATA static u8 sLocationHistory[ROAMER_COUNT][3][2] = {0};
EWRAM_DATA static u8 sRoamerLocation[ROAMER_COUNT][2] = {0};
EWRAM_DATA u8 gEncounteredRoamerIndex = 0;

#define ROAMER_LOCATION(map) {MAP_GROUP(map), MAP_NUM(map)}
#define ___ {MAP_GROUP(MAP_UNDEFINED), MAP_NUM(MAP_UNDEFINED)} // For empty spots in the location table

// Note: There are two potential softlocks that can occur with this table if its maps are
//       changed in particular ways. They can be avoided by ensuring the following:
//       - There must be at least 2 location sets that start with a different map,
//         i.e. every location set cannot start with the same map. This is because of
//         the while loop in RoamerMoveToOtherLocationSet.
//       - Each location set must have at least 3 unique maps. This is because of
//         the while loop in RoamerMove. In this loop the first map in the set is
//         ignored, and an additional map is ignored if the roamer was there recently.
//       - Additionally, while not a softlock, it's worth noting that if for any
//         map in the location table there is not a location set that starts with
//         that map then the roamer will be significantly less likely to move away
//         from that map when it lands there.
#define ROAMER_LOCATION_MAGIC 0xC2
#if ROM_WORLD == 2
static const struct RoamerLocation sRoamerLocations[][6] =
{
    { ROAMER_LOCATION(MAP_CORMORIA_ROUTE4), ROAMER_LOCATION(MAP_CORMORIA_ROUTE5), ROAMER_LOCATION(MAP_CORMORIA_ROUTE6), ___, ___, ___ },
    { ROAMER_LOCATION(MAP_CORMORIA_ROUTE5), ROAMER_LOCATION(MAP_CORMORIA_ROUTE4), ROAMER_LOCATION(MAP_CORMORIA_ROUTE6), ___, ___, ___ },
    { ROAMER_LOCATION(MAP_CORMORIA_ROUTE6), ROAMER_LOCATION(MAP_CORMORIA_ROUTE4), ROAMER_LOCATION(MAP_CORMORIA_ROUTE5), ___, ___, ___ },
    { ___, ___, ___, ___, ___, ___ },
};
#else
static const struct RoamerLocation sRoamerLocations[][6] =
{
    { ROAMER_LOCATION(MAP_ROUTE110), ROAMER_LOCATION(MAP_ROUTE111), ROAMER_LOCATION(MAP_ROUTE117), ROAMER_LOCATION(MAP_ROUTE118), ROAMER_LOCATION(MAP_ROUTE134), ___ },
    { ROAMER_LOCATION(MAP_ROUTE111), ROAMER_LOCATION(MAP_ROUTE110), ROAMER_LOCATION(MAP_ROUTE117), ROAMER_LOCATION(MAP_ROUTE118), ___, ___ },
    { ROAMER_LOCATION(MAP_ROUTE117), ROAMER_LOCATION(MAP_ROUTE111), ROAMER_LOCATION(MAP_ROUTE110), ROAMER_LOCATION(MAP_ROUTE118), ___, ___ },
    { ROAMER_LOCATION(MAP_ROUTE118), ROAMER_LOCATION(MAP_ROUTE117), ROAMER_LOCATION(MAP_ROUTE110), ROAMER_LOCATION(MAP_ROUTE111), ROAMER_LOCATION(MAP_ROUTE119), ROAMER_LOCATION(MAP_ROUTE123) },
    { ROAMER_LOCATION(MAP_ROUTE119), ROAMER_LOCATION(MAP_ROUTE118), ROAMER_LOCATION(MAP_ROUTE120), ___, ___, ___ },
    { ROAMER_LOCATION(MAP_ROUTE120), ROAMER_LOCATION(MAP_ROUTE119), ROAMER_LOCATION(MAP_ROUTE121), ___, ___, ___ },
    { ROAMER_LOCATION(MAP_ROUTE121), ROAMER_LOCATION(MAP_ROUTE120), ROAMER_LOCATION(MAP_ROUTE122), ROAMER_LOCATION(MAP_ROUTE123), ___, ___ },
    { ROAMER_LOCATION(MAP_ROUTE122), ROAMER_LOCATION(MAP_ROUTE121), ROAMER_LOCATION(MAP_ROUTE123), ___, ___, ___ },
    { ROAMER_LOCATION(MAP_ROUTE123), ROAMER_LOCATION(MAP_ROUTE122), ROAMER_LOCATION(MAP_ROUTE118), ___, ___, ___ },
    { ROAMER_LOCATION(MAP_ROUTE124), ROAMER_LOCATION(MAP_ROUTE121), ROAMER_LOCATION(MAP_ROUTE125), ROAMER_LOCATION(MAP_ROUTE126), ___, ___ },
    { ROAMER_LOCATION(MAP_ROUTE125), ROAMER_LOCATION(MAP_ROUTE124), ROAMER_LOCATION(MAP_ROUTE127), ___, ___, ___ },
    { ROAMER_LOCATION(MAP_ROUTE126), ROAMER_LOCATION(MAP_ROUTE124), ROAMER_LOCATION(MAP_ROUTE127), ___, ___, ___ },
    { ROAMER_LOCATION(MAP_ROUTE127), ROAMER_LOCATION(MAP_ROUTE125), ROAMER_LOCATION(MAP_ROUTE126), ROAMER_LOCATION(MAP_ROUTE128), ___, ___ },
    { ROAMER_LOCATION(MAP_ROUTE128), ROAMER_LOCATION(MAP_ROUTE127), ROAMER_LOCATION(MAP_ROUTE129), ___, ___, ___ },
    { ROAMER_LOCATION(MAP_ROUTE129), ROAMER_LOCATION(MAP_ROUTE128), ROAMER_LOCATION(MAP_ROUTE130), ___, ___, ___ },
    { ROAMER_LOCATION(MAP_ROUTE130), ROAMER_LOCATION(MAP_ROUTE129), ROAMER_LOCATION(MAP_ROUTE131), ___, ___, ___ },
    { ROAMER_LOCATION(MAP_ROUTE131), ROAMER_LOCATION(MAP_ROUTE130), ROAMER_LOCATION(MAP_ROUTE132), ___, ___, ___ },
    { ROAMER_LOCATION(MAP_ROUTE132), ROAMER_LOCATION(MAP_ROUTE131), ROAMER_LOCATION(MAP_ROUTE133), ___, ___, ___ },
    { ROAMER_LOCATION(MAP_ROUTE133), ROAMER_LOCATION(MAP_ROUTE132), ROAMER_LOCATION(MAP_ROUTE134), ___, ___, ___ },
    { ROAMER_LOCATION(MAP_ROUTE134), ROAMER_LOCATION(MAP_ROUTE133), ROAMER_LOCATION(MAP_ROUTE110), ___, ___, ___ },
    { ___, ___, ___, ___, ___, ___ },
};
#endif

#undef ___
#undef ROAMER_LOCATION
#define NUM_LOCATION_SETS (ARRAY_COUNT(sRoamerLocations) - 1)
#define NUM_LOCATIONS_PER_SET (ARRAY_COUNT(sRoamerLocations[0]))

static bool8 IsUndefinedRoamerLocation(const struct RoamerLocation *location)
{
    return location->mapGroup == MAP_GROUP(MAP_UNDEFINED)
        && location->mapNum == MAP_NUM(MAP_UNDEFINED);
}

static bool8 IsKnownRoamerLocation(u8 mapGroup, u8 mapNum)
{
    u32 locSet;
    u32 location;

    for (locSet = 0; locSet < NUM_LOCATION_SETS; locSet++)
    {
        for (location = 0; location < NUM_LOCATIONS_PER_SET; location++)
        {
            const struct RoamerLocation *candidate = &sRoamerLocations[locSet][location];

            if (!IsUndefinedRoamerLocation(candidate)
                && candidate->mapGroup == mapGroup
                && candidate->mapNum == mapNum)
                return TRUE;
        }
    }
    return FALSE;
}

/* sRoamerLocation is EWRAM in the original game.  Persisting the active map
 * in the spare bytes of the roamer record keeps a cold save reload from
 * losing the encounter area.  The world-specific marker also prevents a
 * location from one ROM's map graph being interpreted by another ROM. */
static void SaveRoamerLocation(u32 roamerIndex)
{
    ROAMER(roamerIndex)->filler[0] = ROAMER_LOCATION_MAGIC;
    ROAMER(roamerIndex)->filler[1] = ROM_WORLD_ID & 0xFF;
    ROAMER(roamerIndex)->filler[2] = ROM_WORLD_ID >> 8;
    ROAMER(roamerIndex)->filler[3] = sRoamerLocation[roamerIndex][MAP_GRP];
    ROAMER(roamerIndex)->filler[4] = sRoamerLocation[roamerIndex][MAP_NUM];
}

static void SetRoamerLocation(u32 roamerIndex, const struct RoamerLocation *location)
{
    sRoamerLocation[roamerIndex][MAP_GRP] = location->mapGroup;
    sRoamerLocation[roamerIndex][MAP_NUM] = location->mapNum;
    SaveRoamerLocation(roamerIndex);
}

static void EnsureRoamerLocation(u32 roamerIndex)
{
    struct Roamer *roamer = ROAMER(roamerIndex);

    if (!roamer->active)
        return;

    if (roamer->filler[0] == ROAMER_LOCATION_MAGIC
        && roamer->filler[1] == (ROM_WORLD_ID & 0xFF)
        && roamer->filler[2] == (ROM_WORLD_ID >> 8)
        && IsKnownRoamerLocation(roamer->filler[3], roamer->filler[4]))
    {
        sRoamerLocation[roamerIndex][MAP_GRP] = roamer->filler[3];
        sRoamerLocation[roamerIndex][MAP_NUM] = roamer->filler[4];
    }
    else
    {
        SetRoamerLocation(roamerIndex, &sRoamerLocations[Random() % NUM_LOCATION_SETS][0]);
    }
}

void DeactivateAllRoamers(void)
{
    u32 i;

    for (i = 0; i < ROAMER_COUNT; i++)
        SetRoamerInactive(i);
}

static void ClearRoamerLocationHistory(u32 roamerIndex)
{
    u32 i;

    for (i = 0; i < ARRAY_COUNT(sLocationHistory[roamerIndex]); i++)
    {
        sLocationHistory[roamerIndex][i][MAP_GRP] = 0;
        sLocationHistory[roamerIndex][i][MAP_NUM] = 0;
    }
}

void MoveAllRoamersToOtherLocationSets(void)
{
    u32 i;

    for (i = 0; i < ROAMER_COUNT; i++)
        RoamerMoveToOtherLocationSet(i);
}

void MoveAllRoamers(void)
{
    u32 i;

    for (i = 0; i < ROAMER_COUNT; i++)
        RoamerMove(i);
}

static void CreateInitialRoamerMon(u8 index, enum Species species, u8 level)
{
    ClearRoamerLocationHistory(index);
    u32 personality = GetMonPersonality(species,
        GetSynchronizedGender(ROAMER_ORIGIN, species),
        GetSynchronizedNature(ROAMER_ORIGIN, species),
        RANDOM_UNOWN_LETTER);
    CreateMonWithIVs(&gParties[B_TRAINER_1][0], species, level, personality, OTID_STRUCT_PLAYER_ID, USE_RANDOM_IVS);
    GiveMonInitialMoveset(&gParties[B_TRAINER_1][0]);
    ROAMER(index)->ivs = GetMonData(&gParties[B_TRAINER_1][0], MON_DATA_IVS);
    ROAMER(index)->personality = GetMonData(&gParties[B_TRAINER_1][0], MON_DATA_PERSONALITY);
    ROAMER(index)->species = species;
    ROAMER(index)->level = level;
    ROAMER(index)->statusA = 0;
    ROAMER(index)->statusB = 0;
    ROAMER(index)->hp = GetMonData(&gParties[B_TRAINER_1][0], MON_DATA_MAX_HP);
    ROAMER(index)->cool = GetMonData(&gParties[B_TRAINER_1][0], MON_DATA_COOL);
    ROAMER(index)->beauty = GetMonData(&gParties[B_TRAINER_1][0], MON_DATA_BEAUTY);
    ROAMER(index)->cute = GetMonData(&gParties[B_TRAINER_1][0], MON_DATA_CUTE);
    ROAMER(index)->smart = GetMonData(&gParties[B_TRAINER_1][0], MON_DATA_SMART);
    ROAMER(index)->tough = GetMonData(&gParties[B_TRAINER_1][0], MON_DATA_TOUGH);
    ROAMER(index)->shiny = GetMonData(&gParties[B_TRAINER_1][0], MON_DATA_IS_SHINY);
    ROAMER(index)->active = TRUE;
    SetRoamerLocation(index, &sRoamerLocations[Random() % NUM_LOCATION_SETS][0]);
}

static u8 GetFirstInactiveRoamerIndex(void)
{
    u32 i;

    for (i = 0; i < ROAMER_COUNT; i++)
    {
        if (!ROAMER(i)->active)
            return i;
    }
    return ROAMER_COUNT;
}

bool8 TryAddRoamer(enum Species species, u8 level)
{
    u8 index = GetFirstInactiveRoamerIndex();

    if (index < ROAMER_COUNT)
    {
        // Create the roamer and stop searching
        CreateInitialRoamerMon(index, species, level);
        return TRUE;
    }

    // Maximum active roamers found: do nothing and let the calling function know
    return FALSE;
}

// gSpecialVar_0x8004 here corresponds to the options in the multichoice MULTI_TV_LATI (0 for 'Red', 1 for 'Blue')
void InitRoamer(void)
{
#if ROM_WORLD == 2
    TryAddRoamer(SPECIES_ZERAORA, 40);
#else
    if (gSpecialVar_0x8004 == 0) // Red
        TryAddRoamer(SPECIES_LATIAS, 40);
    else
        TryAddRoamer(SPECIES_LATIOS, 40);
#endif
}

void UpdateLocationHistoryForRoamer(void)
{
    u32 i;

    for (i = 0; i < ROAMER_COUNT; i++)
    {
        EnsureRoamerLocation(i);
        sLocationHistory[i][2][MAP_GRP] = sLocationHistory[i][1][MAP_GRP];
        sLocationHistory[i][2][MAP_NUM] = sLocationHistory[i][1][MAP_NUM];

        sLocationHistory[i][1][MAP_GRP] = sLocationHistory[i][0][MAP_GRP];
        sLocationHistory[i][1][MAP_NUM] = sLocationHistory[i][0][MAP_NUM];

        sLocationHistory[i][0][MAP_GRP] = gSaveBlock1Ptr->location.mapGroup;
        sLocationHistory[i][0][MAP_NUM] = gSaveBlock1Ptr->location.mapNum;
    }
}

void RoamerMoveToOtherLocationSet(u32 roamerIndex)
{
    struct RoamerLocation location;

    if (!ROAMER(roamerIndex)->active)
        return;

    EnsureRoamerLocation(roamerIndex);

    // Choose a location set that starts with a map
    // different from the roamer's current map
    do
    {
        location = sRoamerLocations[Random() % NUM_LOCATION_SETS][0];
        if (sRoamerLocation[roamerIndex][MAP_GRP] != location.mapGroup
            || sRoamerLocation[roamerIndex][MAP_NUM] != location.mapNum)
        {
            SetRoamerLocation(roamerIndex, &location);
            return;
        }
    } while (sRoamerLocation[roamerIndex][MAP_GRP] == location.mapGroup
             && sRoamerLocation[roamerIndex][MAP_NUM] == location.mapNum);
    SetRoamerLocation(roamerIndex, &location);
}

void RoamerMove(u32 roamerIndex)
{
    u8 locSet = 0;

    if (!ROAMER(roamerIndex)->active)
        return;

    EnsureRoamerLocation(roamerIndex);

    if ((Random() % 16) == 0)
    {
        RoamerMoveToOtherLocationSet(roamerIndex);
    }
    else
    {
        while (locSet < NUM_LOCATION_SETS)
        {
            // Find the location set that starts with the roamer's current map
            if (sRoamerLocation[roamerIndex][MAP_GRP] == sRoamerLocations[locSet][0].mapGroup
                && sRoamerLocation[roamerIndex][MAP_NUM] == sRoamerLocations[locSet][0].mapNum)
            {
                struct RoamerLocation location;
                // Choose a new map (excluding the first) within this set
                // Also exclude a map if the roamer was there 2 moves ago
                do
                {
                    location = sRoamerLocations[locSet][(Random() % (NUM_LOCATIONS_PER_SET - 1)) + 1];
                } while ((sLocationHistory[roamerIndex][2][MAP_GRP] == location.mapGroup
                        && sLocationHistory[roamerIndex][2][MAP_NUM] == location.mapNum)
                        || IsUndefinedRoamerLocation(&location));
                SetRoamerLocation(roamerIndex, &location);
                return;
            }
            locSet++;
        }
    }
}

bool8 IsRoamerAt(u32 roamerIndex, u8 mapGroup, u8 mapNum)
{
    EnsureRoamerLocation(roamerIndex);
    if (ROAMER(roamerIndex)->active && mapGroup == sRoamerLocation[roamerIndex][MAP_GRP] && mapNum == sRoamerLocation[roamerIndex][MAP_NUM])
        return TRUE;
    else
        return FALSE;
}

void CreateRoamerMonInstance(u32 roamerIndex)
{
    u32 status = ROAMER(roamerIndex)->statusA + (ROAMER(roamerIndex)->statusB << 8);
    struct Pokemon *mon = &gParties[B_TRAINER_1][0];
    ZeroEnemyPartyMons();
    CreateMonWithIVsPersonality(mon, ROAMER(roamerIndex)->species, ROAMER(roamerIndex)->level, ROAMER(roamerIndex)->ivs, ROAMER(roamerIndex)->personality);
    SetMonData(mon, MON_DATA_STATUS, &status);
    SetMonData(mon, MON_DATA_HP, &ROAMER(roamerIndex)->hp);
    SetMonData(mon, MON_DATA_COOL, &ROAMER(roamerIndex)->cool);
    SetMonData(mon, MON_DATA_BEAUTY, &ROAMER(roamerIndex)->beauty);
    SetMonData(mon, MON_DATA_CUTE, &ROAMER(roamerIndex)->cute);
    SetMonData(mon, MON_DATA_SMART, &ROAMER(roamerIndex)->smart);
    SetMonData(mon, MON_DATA_TOUGH, &ROAMER(roamerIndex)->tough);
    SetMonData(mon, MON_DATA_IS_SHINY, &ROAMER(roamerIndex)->shiny);
}

bool8 TryStartRoamerEncounter(void)
{
    u32 i;

    for (i = 0; i < ROAMER_COUNT; i++)
    {
        if (IsRoamerAt(i, gSaveBlock1Ptr->location.mapGroup, gSaveBlock1Ptr->location.mapNum) == TRUE && (Random() % 4) == 0)
        {
            CreateRoamerMonInstance(i);
            gEncounteredRoamerIndex = i;
            return TRUE;
        }
    }
    return FALSE;
}

void UpdateRoamerHPStatus(struct Pokemon *mon)
{
    u32 status = GetMonData(mon, MON_DATA_STATUS);

    ROAMER(gEncounteredRoamerIndex)->hp = GetMonData(mon, MON_DATA_HP);
    ROAMER(gEncounteredRoamerIndex)->statusA = status;
    ROAMER(gEncounteredRoamerIndex)->statusB = status >> 8;

    RoamerMoveToOtherLocationSet(gEncounteredRoamerIndex);
}

void SetRoamerInactive(u32 roamerIndex)
{
    ROAMER(roamerIndex)->active = FALSE;
}

void GetRoamerLocation(u32 roamerIndex, u8 *mapGroup, u8 *mapNum)
{
    EnsureRoamerLocation(roamerIndex);
    *mapGroup = sRoamerLocation[roamerIndex][MAP_GRP];
    *mapNum = sRoamerLocation[roamerIndex][MAP_NUM];
}
