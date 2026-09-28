#include "global.h"
#include "coop/group_travel.h"
#include "coop/net_bridge.h"
#include "event_data.h"
#include "event_object_movement.h"
#include "event_object_lock.h"
#include "heal_location.h"
#include "item.h"
#include "johto/kanto_travel.h"
#include "johto/save.h"
#include "overworld.h"
#include "region_map.h"
#include "script.h"
#include "start_menu.h"
#include "task.h"
#include "constants/heal_locations.h"
#include "constants/flags.h"
#include "constants/field_specials.h"
#include "constants/johto_content.h"
#include "constants/maps.h"
#include "constants/region_map_sections.h"
#include "test/test.h"

extern void Johto_CommitKantoTravel(void);
extern const u8 EventScript_CoopGroupTravelOffer[];

static struct CoopGroupTravelRecord Record(u8 kind, u8 route, u32 request, u8 proposal)
{
    static const u8 sEra[] = {0, 1, 2, 1, 2, 1, 2, 3};
    static const u8 sDestination[] = {0, 3, 4, 1, 2, 5, 6, 7};
    struct CoopGroupTravelRecord record = {0};
    record.kind = kind; record.route = route;
    if (route >= COOP_GROUP_TRAVEL_ROUTE_FLY_LITTLEROOT
     && route <= COOP_GROUP_TRAVEL_ROUTE_FLY_BATTLE_FRONTIER)
    {
        (void)CoopRegionMap_GroupFlyFields(route, &record.era, &record.destination);
        record.departure = COOP_GROUP_TRAVEL_DEPARTURE_FLY;
    }
    else if (route == COOP_GROUP_TRAVEL_ROUTE_CABLE_CAR_ROUTE112_MT_CHIMNEY
          || route == COOP_GROUP_TRAVEL_ROUTE_CABLE_CAR_MT_CHIMNEY_ROUTE112)
    {
        record.era = COOP_GROUP_TRAVEL_ERA_HOENN;
        record.destination = route == COOP_GROUP_TRAVEL_ROUTE_CABLE_CAR_ROUTE112_MT_CHIMNEY
            ? COOP_GROUP_TRAVEL_DEST_MT_CHIMNEY_CABLE_CAR_STATION
            : COOP_GROUP_TRAVEL_DEST_ROUTE112_CABLE_CAR_STATION;
        record.departure = COOP_GROUP_TRAVEL_DEPARTURE_CABLE_CAR;
    }
    else if (route >= COOP_GROUP_TRAVEL_ROUTE_SS_TIDAL_SLATEPORT_BOARD
          && route <= COOP_GROUP_TRAVEL_ROUTE_SS_TIDAL_LILYCOVE_BOARD)
    {
        record.era = COOP_GROUP_TRAVEL_ERA_HOENN;
        record.destination = COOP_GROUP_TRAVEL_DEST_SS_TIDAL_CORRIDOR;
        record.departure = COOP_GROUP_TRAVEL_DEPARTURE_FERRY;
    }
    else if (route >= COOP_GROUP_TRAVEL_ROUTE_SS_TIDAL_LILYCOVE_EXIT
          && route <= COOP_GROUP_TRAVEL_ROUTE_SS_TIDAL_SLATEPORT_EXIT)
    {
        record.era = COOP_GROUP_TRAVEL_ERA_HOENN;
        record.destination = route == COOP_GROUP_TRAVEL_ROUTE_SS_TIDAL_LILYCOVE_EXIT
            ? COOP_GROUP_TRAVEL_DEST_LILYCOVE_HARBOR
            : COOP_GROUP_TRAVEL_DEST_SLATEPORT_HARBOR;
        record.departure = COOP_GROUP_TRAVEL_DEPARTURE_FERRY;
    }
    else if (route >= COOP_GROUP_TRAVEL_ROUTE_SEAGALLOP_FIRST
          && route <= COOP_GROUP_TRAVEL_ROUTE_SEAGALLOP_BIRTH_VERMILION)
    {
        u8 origin;
        u8 destination;
        u8 slot;
        if (route <= COOP_GROUP_TRAVEL_ROUTE_SEAGALLOP_LAST_REGULAR)
        {
            origin = (route - COOP_GROUP_TRAVEL_ROUTE_SEAGALLOP_FIRST) / 7;
            slot = (route - COOP_GROUP_TRAVEL_ROUTE_SEAGALLOP_FIRST) % 7;
            destination = slot >= origin ? slot + 1 : slot;
        }
        else
            destination = route == COOP_GROUP_TRAVEL_ROUTE_SEAGALLOP_VERMILION_NAVEL ? 8
                : route == COOP_GROUP_TRAVEL_ROUTE_SEAGALLOP_VERMILION_BIRTH ? 9 : 0;
        record.era = destination == 0 ? COOP_GROUP_TRAVEL_ERA_ORIGINAL
            : COOP_GROUP_TRAVEL_ERA_SEVII;
        record.destination = COOP_GROUP_TRAVEL_DEST_SEAGALLOP_VERMILION + destination;
        record.departure = COOP_GROUP_TRAVEL_DEPARTURE_FERRY;
    }
    else if (route == COOP_GROUP_TRAVEL_ROUTE_BILL_CINNABAR_ONE
          || route == COOP_GROUP_TRAVEL_ROUTE_BILL_ONE_CINNABAR)
    {
        record.era = COOP_GROUP_TRAVEL_ERA_ORIGINAL;
        record.destination = route == COOP_GROUP_TRAVEL_ROUTE_BILL_CINNABAR_ONE
            ? COOP_GROUP_TRAVEL_DEST_BILL_ONE_ISLAND_CENTER
            : COOP_GROUP_TRAVEL_DEST_BILL_CINNABAR;
        record.departure = COOP_GROUP_TRAVEL_DEPARTURE_FERRY;
    }
    else if (route >= COOP_GROUP_TRAVEL_ROUTE_BRINEY_HOUSE_DEWFORD)
    {
        record.era = COOP_GROUP_TRAVEL_ERA_HOENN;
        record.destination = route == COOP_GROUP_TRAVEL_ROUTE_DEWFORD_BRINEY_HOUSE
            ? COOP_GROUP_TRAVEL_DEST_BRINEY_HOUSE
            : route == COOP_GROUP_TRAVEL_ROUTE_DEWFORD_ROUTE109
            ? COOP_GROUP_TRAVEL_DEST_ROUTE109
            : COOP_GROUP_TRAVEL_DEST_HOENN_DEWFORD;
        record.departure = COOP_GROUP_TRAVEL_DEPARTURE_FERRY;
    }
    else if (route >= COOP_GROUP_TRAVEL_ROUTE_LILYCOVE_SOUTHERN_ISLAND)
    {
        record.era = COOP_GROUP_TRAVEL_ERA_HOENN;
        record.destination = route == COOP_GROUP_TRAVEL_ROUTE_LILYCOVE_SOUTHERN_ISLAND
            ? COOP_GROUP_TRAVEL_DEST_SOUTHERN_ISLAND
            : route == COOP_GROUP_TRAVEL_ROUTE_LILYCOVE_NAVEL_ROCK
            ? COOP_GROUP_TRAVEL_DEST_NAVEL_ROCK
            : route == COOP_GROUP_TRAVEL_ROUTE_LILYCOVE_BIRTH_ISLAND
            ? COOP_GROUP_TRAVEL_DEST_BIRTH_ISLAND
            : route == COOP_GROUP_TRAVEL_ROUTE_LILYCOVE_FARAWAY_ISLAND
            ? COOP_GROUP_TRAVEL_DEST_FARAWAY_ISLAND
            : route == COOP_GROUP_TRAVEL_ROUTE_NAVEL_ROCK_LILYCOVE
            ? COOP_GROUP_TRAVEL_DEST_LILYCOVE_HARBOR
            : COOP_GROUP_TRAVEL_DEST_BATTLE_FRONTIER;
        record.departure = COOP_GROUP_TRAVEL_DEPARTURE_FERRY;
    }
    else if (route >= COOP_GROUP_TRAVEL_ROUTE_SOUTHERN_ISLAND_LILYCOVE)
    {
        record.era = COOP_GROUP_TRAVEL_ERA_HOENN;
        record.destination = route == COOP_GROUP_TRAVEL_ROUTE_BATTLE_FRONTIER_SLATEPORT
            ? COOP_GROUP_TRAVEL_DEST_SLATEPORT_HARBOR : COOP_GROUP_TRAVEL_DEST_LILYCOVE_HARBOR;
        record.departure = COOP_GROUP_TRAVEL_DEPARTURE_FERRY;
    }
    else if (route >= COOP_GROUP_TRAVEL_ROUTE_OLIVINE_SOUTHERN_ISLAND)
    {
        record.era = COOP_GROUP_TRAVEL_ERA_HOENN;
        record.destination = COOP_GROUP_TRAVEL_DEST_SOUTHERN_ISLAND
            + (route - COOP_GROUP_TRAVEL_ROUTE_OLIVINE_SOUTHERN_ISLAND) % 4;
        record.departure = COOP_GROUP_TRAVEL_DEPARTURE_FERRY;
    }
    else if (route >= COOP_GROUP_TRAVEL_ROUTE_RETURN_FERRY_ORIGINAL)
    {
        record.era = (route & 1) ? COOP_GROUP_TRAVEL_ERA_LATER : COOP_GROUP_TRAVEL_ERA_ORIGINAL;
        record.destination = route <= COOP_GROUP_TRAVEL_ROUTE_RETURN_FERRY_LATER ? 14
            : route <= COOP_GROUP_TRAVEL_ROUTE_RETURN_TRAIN_LATER ? 12
            : COOP_GROUP_TRAVEL_DEST_JOHTO_RECEPTION_GATE;
        record.departure = route <= COOP_GROUP_TRAVEL_ROUTE_RETURN_FERRY_LATER
            ? COOP_GROUP_TRAVEL_DEPARTURE_FERRY
            : route <= COOP_GROUP_TRAVEL_ROUTE_RETURN_TRAIN_LATER
                ? COOP_GROUP_TRAVEL_DEPARTURE_TRAIN : COOP_GROUP_TRAVEL_DEPARTURE_GATE;
    }
    else
    {
        record.era = sEra[route];
        record.destination = sDestination[route];
        record.departure = route <= COOP_GROUP_TRAVEL_ROUTE_TRAIN_LATER
            ? COOP_GROUP_TRAVEL_DEPARTURE_TRAIN
            : route <= COOP_GROUP_TRAVEL_ROUTE_FERRY_LATER
                ? COOP_GROUP_TRAVEL_DEPARTURE_FERRY
                : COOP_GROUP_TRAVEL_DEPARTURE_GATE;
    }
    record.request_id = request;
    memset(record.proposal_id, proposal, sizeof(record.proposal_id));
    return record;
}

static void MaterializeDeparture(u16 map)
{
    gSaveBlock1Ptr->location.mapGroup = MAP_GROUP(map);
    gSaveBlock1Ptr->location.mapNum = MAP_NUM(map);
}

static void EndDepartureScript(void)
{
    ScriptContext_Init();
    UnlockPlayerFieldControls();
}

static void ResetGroupTravelFixture(void)
{
    ResetTasks();
    ScriptContext_Init();
    ScriptUnfreezeObjectEvents();
    UnlockPlayerFieldControls();
    gCoopNetBridge.game_to_network.read_index = 0;
    gCoopNetBridge.game_to_network.write_index = 0;
    gCoopNetBridge.network_to_game.read_index = 0;
    gCoopNetBridge.network_to_game.write_index = 0;
    CoopGroupTravel_Init();
    JohtoSave_InitializeCurrent();
    (void)JohtoTravel_Cancel();
}

static void DiscardSimulatedWarp(void)
{
    /* These tests inspect the commit before the map loader runs. The loader
     * task belongs to that simulated warp and must not leak into the runner. */
    ResetTasks();
}

static void PrepareTravelOrigin(void)
{
    const struct HealLocation *heal;

    JohtoSave_InitializeCurrent();
    MaterializeDeparture(MAP_NEW_BARK_TOWN);
    gMapHeader.regionMapSectionId = MAPSEC_NEW_BARK_TOWN;
    heal = GetHealLocation(HEAL_LOCATION_JOHTO_CHERRYGROVE_CITY);
    gSaveBlock1Ptr->lastHealLocation.mapGroup = heal->mapGroup;
    gSaveBlock1Ptr->lastHealLocation.mapNum = heal->mapNum;
    gSaveBlock1Ptr->lastHealLocation.warpId = WARP_ID_NONE;
    gSaveBlock1Ptr->lastHealLocation.x = heal->x;
    gSaveBlock1Ptr->lastHealLocation.y = heal->y;
}

TEST("Group travel accepts each approved script-locked departure and owns the handoff")
{
    ResetGroupTravelFixture();
    static const struct
    {
        u8 route;
        u8 context;
        u16 map;
    } sDepartures[] = {
        {COOP_GROUP_TRAVEL_ROUTE_TRAIN_ORIGINAL, COOP_GROUP_TRAVEL_DEPARTURE_TRAIN, MAP_GOLDENROD_CITY_TRAIN_STATION},
        {COOP_GROUP_TRAVEL_ROUTE_FERRY_ORIGINAL, COOP_GROUP_TRAVEL_DEPARTURE_FERRY, MAP_OLIVINE_CITY_PORT_INSIDE},
        {COOP_GROUP_TRAVEL_ROUTE_FERRY_LATER, COOP_GROUP_TRAVEL_DEPARTURE_SSAQUA_MAIDEN, MAP_OLIVINE_CITY_PORT_INSIDE},
        {COOP_GROUP_TRAVEL_ROUTE_GATE_LATER, COOP_GROUP_TRAVEL_DEPARTURE_GATE, MAP_RECEPTION_GATE},
        {COOP_GROUP_TRAVEL_ROUTE_RETURN_FERRY_ORIGINAL, COOP_GROUP_TRAVEL_DEPARTURE_FERRY, MAP_KANTO_ORIGINAL_VERMILION_CITY_PORT_INSIDE},
        {COOP_GROUP_TRAVEL_ROUTE_RETURN_FERRY_LATER, COOP_GROUP_TRAVEL_DEPARTURE_FERRY, MAP_KANTO_LATER_VERMILION_CITY_PORT_INSIDE},
        {COOP_GROUP_TRAVEL_ROUTE_RETURN_TRAIN_ORIGINAL, COOP_GROUP_TRAVEL_DEPARTURE_TRAIN, MAP_KANTO_ORIGINAL_SAFFRON_CITY_TRAIN_STATION},
        {COOP_GROUP_TRAVEL_ROUTE_RETURN_TRAIN_LATER, COOP_GROUP_TRAVEL_DEPARTURE_TRAIN, MAP_KANTO_LATER_SAFFRON_CITY_TRAIN_STATION},
        {COOP_GROUP_TRAVEL_ROUTE_RETURN_GATE_ORIGINAL, COOP_GROUP_TRAVEL_DEPARTURE_GATE, MAP_ROUTE22},
        {COOP_GROUP_TRAVEL_ROUTE_RETURN_GATE_LATER, COOP_GROUP_TRAVEL_DEPARTURE_GATE, MAP_KANTO_LATER_ROUTE22},
        {COOP_GROUP_TRAVEL_ROUTE_CABLE_CAR_ROUTE112_MT_CHIMNEY, COOP_GROUP_TRAVEL_DEPARTURE_CABLE_CAR, MAP_ROUTE112_CABLE_CAR_STATION},
        {COOP_GROUP_TRAVEL_ROUTE_CABLE_CAR_MT_CHIMNEY_ROUTE112, COOP_GROUP_TRAVEL_DEPARTURE_CABLE_CAR, MAP_MT_CHIMNEY_CABLE_CAR_STATION},
    };
    u32 i;

    for (i = 0; i < ARRAY_COUNT(sDepartures); i++)
    {
        struct CoopGroupTravelRecord abort;

        CoopGroupTravel_Init();
        CoopGroupTravel_TestSetGrouped(TRUE);
        CoopGroupTravel_TestSetSafe(TRUE);
        gCoopNetBridge.game_to_network.read_index = 0;
        gCoopNetBridge.game_to_network.write_index = 0;
        MaterializeDeparture(sDepartures[i].map);
        gObjectEvents[0].active = TRUE;
        gObjectEvents[0].spriteId = 0;
        gObjectEvents[0].frozen = TRUE;
        ScriptContext_SetupScript(EventScript_CoopGroupTravelOffer);

        EXPECT_EQ(CoopGroupTravel_BeginFromScript(sDepartures[i].route, sDepartures[i].context),
                  COOP_GROUP_TRAVEL_BEGIN_WAITING);
        EXPECT(ArePlayerFieldControlsLocked());
        EndDepartureScript();
        EXPECT(!ArePlayerFieldControlsLocked());
        CoopGroupTravel_Poll();
        EXPECT(ArePlayerFieldControlsLocked());

        abort = *CoopGroupTravel_TestRecord();
        abort.kind = COOP_GROUP_TRAVEL_SERVER_ABORT;
        abort.reason = COOP_GROUP_TRAVEL_REASON_CONFLICT;
        EXPECT(CoopGroupTravel_ReceiveServer(&abort));
        EXPECT(!ArePlayerFieldControlsLocked());
        EXPECT(!gObjectEvents[0].frozen);
        gObjectEvents[0].active = FALSE;
    }
}

TEST("Cable car consent uses exact stations and a stable arrival aisle")
{
    static const struct { u8 route; u16 source; u16 destination; } sCases[] = {
        {COOP_GROUP_TRAVEL_ROUTE_CABLE_CAR_ROUTE112_MT_CHIMNEY,
            MAP_ROUTE112_CABLE_CAR_STATION, MAP_MT_CHIMNEY_CABLE_CAR_STATION},
        {COOP_GROUP_TRAVEL_ROUTE_CABLE_CAR_MT_CHIMNEY_ROUTE112,
            MAP_MT_CHIMNEY_CABLE_CAR_STATION, MAP_ROUTE112_CABLE_CAR_STATION},
    };
    u32 i;

    for (i = 0; i < ARRAY_COUNT(sCases); i++)
    {
        ResetGroupTravelFixture();
        CoopGroupTravel_TestSetGrouped(TRUE);
        MaterializeDeparture(sCases[i].source);
        ScriptContext_SetupScript(EventScript_CoopGroupTravelOffer);
        EXPECT_EQ(CoopGroupTravel_BeginFromScript(sCases[i].route,
            COOP_GROUP_TRAVEL_DEPARTURE_FERRY), COOP_GROUP_TRAVEL_BEGIN_REJECTED);
        EXPECT_EQ(CoopGroupTravel_BeginFromScript(sCases[i].route,
            COOP_GROUP_TRAVEL_DEPARTURE_CABLE_CAR), COOP_GROUP_TRAVEL_BEGIN_WAITING);
        EndDepartureScript();

        MaterializeDeparture(sCases[i].destination);
        gSaveBlock1Ptr->pos.x = 6;
        gSaveBlock1Ptr->pos.y = 8;
        EXPECT(CoopGroupTravel_TestAtExactDestination(sCases[i].route));
        gSaveBlock1Ptr->pos.y = 7;
        EXPECT(!CoopGroupTravel_TestAtExactDestination(sCases[i].route));
    }
    CoopGroupTravel_Init();
}

TEST("Group travel rejects unapproved or unmaterialized script departures")
{
    ResetGroupTravelFixture();
    CoopGroupTravel_Init();
    CoopGroupTravel_TestSetGrouped(TRUE);
    MaterializeDeparture(MAP_OLIVINE_CITY_PORT_INSIDE);
    EXPECT_EQ(CoopGroupTravel_BeginFromScript(COOP_GROUP_TRAVEL_ROUTE_TRAIN_ORIGINAL,
                                              COOP_GROUP_TRAVEL_DEPARTURE_TRAIN),
              COOP_GROUP_TRAVEL_BEGIN_REJECTED);

    ScriptContext_SetupScript(EventScript_CoopGroupTravelOffer);
    EXPECT_EQ(CoopGroupTravel_BeginFromScript(COOP_GROUP_TRAVEL_ROUTE_FERRY_ORIGINAL,
                                              COOP_GROUP_TRAVEL_DEPARTURE_TRAIN),
              COOP_GROUP_TRAVEL_BEGIN_REJECTED);
    EXPECT_EQ(CoopGroupTravel_BeginFromScript(COOP_GROUP_TRAVEL_ROUTE_DIG,
                                              COOP_GROUP_TRAVEL_DEPARTURE_DIG),
              COOP_GROUP_TRAVEL_BEGIN_REJECTED);
    EXPECT_EQ(CoopGroupTravel_BeginFromScript(COOP_GROUP_TRAVEL_ROUTE_ESCAPE_ROPE,
                                              COOP_GROUP_TRAVEL_DEPARTURE_ESCAPE_ROPE),
              COOP_GROUP_TRAVEL_BEGIN_REJECTED);
    EndDepartureScript();
}

TEST("Maiden ferry group commit enters SS Aqua and applies maiden story state")
{
    ResetGroupTravelFixture();
    struct CoopGroupTravelRecord request = Record(COOP_GROUP_TRAVEL_CLIENT_REQUEST,
        COOP_GROUP_TRAVEL_ROUTE_FERRY_LATER, 101, 0);
    struct CoopGroupTravelRecord commit = Record(COOP_GROUP_TRAVEL_SERVER_COMMIT,
        COOP_GROUP_TRAVEL_ROUTE_FERRY_LATER, 101, 8);
    struct CoopGroupTravelRecord complete = commit;

    request.departure = COOP_GROUP_TRAVEL_DEPARTURE_SSAQUA_MAIDEN;
    commit.departure = COOP_GROUP_TRAVEL_DEPARTURE_SSAQUA_MAIDEN;
    complete.departure = COOP_GROUP_TRAVEL_DEPARTURE_SSAQUA_MAIDEN;

    PrepareTravelOrigin();
    VarSet(JOHTO_VAR_SSAQUA_STATE, 0);
    FlagSet(JOHTO_FLAG_HIDE_SSAQUA_1F_GRANDPA);
    FlagClear(JOHTO_FLAG_HIDE_SSAQUA_ROOM_SSE_GRANDDAUGHTER);
    FlagSet(JOHTO_FLAG_HIDE_SSAQUA_SAILOR);
    FlagSet(JOHTO_FLAG_HIDE_SSAQUA_CAPTAINS_ROOM_GRANDDAUGHTER);
    CoopGroupTravel_Init();
    CoopGroupTravel_TestSetSafe(TRUE);
    CoopGroupTravel_TestSetDeparture(COOP_GROUP_TRAVEL_DEPARTURE_SSAQUA_MAIDEN);
    CoopGroupTravel_TestSeedRequest(&request);

    EXPECT(CoopGroupTravel_ReceiveServer(&commit));
    EXPECT_EQ(VarGet(JOHTO_VAR_SSAQUA_STATE), 1);
    EXPECT(!FlagGet(JOHTO_FLAG_HIDE_SSAQUA_1F_GRANDPA));
    EXPECT(FlagGet(JOHTO_FLAG_HIDE_SSAQUA_ROOM_SSE_GRANDDAUGHTER));
    EXPECT(!FlagGet(JOHTO_FLAG_HIDE_SSAQUA_SAILOR));
    EXPECT(!FlagGet(JOHTO_FLAG_HIDE_SSAQUA_CAPTAINS_ROOM_GRANDDAUGHTER));
    ResetTasks();
    UnlockPlayerFieldControls();

    MaterializeDeparture(MAP_SSAQUA_1F);
    gSaveBlock1Ptr->pos.x = 29;
    gSaveBlock1Ptr->pos.y = 3;
    EXPECT(!CoopGroupTravel_TestAtExactDestination(COOP_GROUP_TRAVEL_ROUTE_FERRY_LATER));
    CoopGroupTravel_Poll();
    EXPECT_EQ(JohtoTravel_GetPendingDestination(), JOHTO_TRAVEL_DESTINATION_KANTO_LATER);

    /* Boarding the maiden is an intermediate story warp. The cooperative
     * arrival is acknowledged only at the route's actual Vermilion port. */
    MaterializeDeparture(MAP_KANTO_LATER_VERMILION_CITY_PORT_INSIDE);
    gSaveBlock1Ptr->pos.x = 8;
    gSaveBlock1Ptr->pos.y = 9;
    gMapHeader.regionMapSectionId = MAPSEC_VERMILION_CITY;
    EXPECT(CoopGroupTravel_TestAtExactDestination(COOP_GROUP_TRAVEL_ROUTE_FERRY_LATER));
    CoopGroupTravel_Poll();
    EXPECT_EQ(JohtoTravel_GetPendingDestination(), JOHTO_TRAVEL_DESTINATION_NONE);
    EXPECT_EQ(CoopGroupTravel_TestState(), 9);
    EXPECT(CoopGroupTravel_TestSemanticQueued());
    complete.kind = COOP_GROUP_TRAVEL_SERVER_COMPLETE;
    complete.result = COOP_GROUP_TRAVEL_RESULT_APPLIED;
    EXPECT(CoopGroupTravel_ReceiveServer(&complete));
    EXPECT_EQ(JohtoTravel_GetPendingDestination(), JOHTO_TRAVEL_DESTINATION_NONE);

    (void)JohtoTravel_Cancel();
    PrepareTravelOrigin();
    CoopGroupTravel_Init();
    CoopGroupTravel_TestSetDeparture(COOP_GROUP_TRAVEL_DEPARTURE_SSAQUA_MAIDEN);
    CoopGroupTravel_TestSeedRequest(&request);
    EXPECT(CoopGroupTravel_ReceiveServer(&commit));
    commit.kind = COOP_GROUP_TRAVEL_SERVER_ABORT;
    commit.reason = COOP_GROUP_TRAVEL_REASON_CONFLICT;
    EXPECT(CoopGroupTravel_ReceiveServer(&commit));
    EXPECT_EQ(JohtoTravel_GetPendingDestination(), JOHTO_TRAVEL_DESTINATION_NONE);
    ResetTasks();
}

TEST("Normal ferry context targets Vermilion despite an unadvanced local maiden state")
{
    ResetGroupTravelFixture();
    struct CoopGroupTravelRecord request = Record(COOP_GROUP_TRAVEL_CLIENT_REQUEST,
        COOP_GROUP_TRAVEL_ROUTE_FERRY_ORIGINAL, 102, 0);
    struct CoopGroupTravelRecord commit = Record(COOP_GROUP_TRAVEL_SERVER_COMMIT,
        COOP_GROUP_TRAVEL_ROUTE_FERRY_ORIGINAL, 102, 9);

    PrepareTravelOrigin();
    VarSet(JOHTO_VAR_SSAQUA_STATE, 0);
    FlagSet(JOHTO_FLAG_HIDE_SSAQUA_1F_GRANDPA);
    CoopGroupTravel_Init();
    CoopGroupTravel_TestSetDeparture(COOP_GROUP_TRAVEL_DEPARTURE_FERRY);
    CoopGroupTravel_TestSeedRequest(&request);

    EXPECT(CoopGroupTravel_ReceiveServer(&commit));
    EXPECT_EQ(VarGet(JOHTO_VAR_SSAQUA_STATE), 0);
    EXPECT(FlagGet(JOHTO_FLAG_HIDE_SSAQUA_1F_GRANDPA));
    ResetTasks();
    UnlockPlayerFieldControls();

    MaterializeDeparture(MAP_KANTO_ORIGINAL_VERMILION_CITY_PORT_INSIDE);
    gSaveBlock1Ptr->pos.x = 8;
    gSaveBlock1Ptr->pos.y = 9;
    EXPECT(CoopGroupTravel_TestAtExactDestination(COOP_GROUP_TRAVEL_ROUTE_FERRY_ORIGINAL));
    MaterializeDeparture(MAP_SSAQUA_1F);
    gSaveBlock1Ptr->pos.x = 29;
    gSaveBlock1Ptr->pos.y = 3;
    EXPECT(!CoopGroupTravel_TestAtExactDestination(COOP_GROUP_TRAVEL_ROUTE_FERRY_ORIGINAL));
}

TEST("Cold session replay adopts a pending requester only after session authentication")
{
    ResetGroupTravelFixture();
    struct CoopGroupTravelRecord request = Record(COOP_GROUP_TRAVEL_SERVER_REQUESTING,
        COOP_GROUP_TRAVEL_ROUTE_GATE_LATER, 201, 0);

    CoopGroupTravel_Init();
    CoopGroupTravel_TestSetGrouped(TRUE);
    CoopGroupTravel_TestSetSafe(TRUE);
    MaterializeDeparture(MAP_RECEPTION_GATE);
    EXPECT(!CoopGroupTravel_ReceiveServer(&request));

    CoopGroupTravel_OnSessionReady();
    EXPECT(CoopGroupTravel_ReceiveServer(&request));
    EXPECT_EQ(CoopGroupTravel_TestState(), 1);
    EXPECT_EQ(CoopGroupTravel_TestRecord()->kind, COOP_GROUP_TRAVEL_CLIENT_REQUEST);
    EXPECT_EQ(CoopGroupTravel_TestRecord()->request_id, 201);
}

TEST("Cold session replay defers a responder offer and exposes its route context")
{
    ResetGroupTravelFixture();
    struct CoopGroupTravelRecord offer = Record(COOP_GROUP_TRAVEL_SERVER_OFFER,
        COOP_GROUP_TRAVEL_ROUTE_FERRY_ORIGINAL, 202, 12);

    CoopGroupTravel_Init();
    CoopGroupTravel_TestSetGrouped(TRUE);
    MaterializeDeparture(MAP_OLIVINE_CITY_PORT_INSIDE);
    CoopGroupTravel_OnSessionReady();
    CoopGroupTravel_TestSetSafe(FALSE);
    EXPECT(CoopGroupTravel_ReceiveServer(&offer));
    EXPECT_EQ(CoopGroupTravel_GetOffer(NULL), COOP_GROUP_TRAVEL_OFFER_DEFERRED);

    CoopGroupTravel_TestSetSafe(TRUE);
    CoopGroupTravel_Poll();
    EXPECT_EQ(CoopGroupTravel_GetOffer(NULL), COOP_GROUP_TRAVEL_OFFER_READY);
    Special_CoopGroupTravelGetOffer();
    EXPECT_EQ(gSpecialVar_Result, COOP_GROUP_TRAVEL_OFFER_READY);
    EXPECT_EQ(gSpecialVar_0x8004, COOP_GROUP_TRAVEL_ROUTE_FERRY_ORIGINAL);
    EXPECT_EQ(gSpecialVar_0x8005, COOP_GROUP_TRAVEL_DEPARTURE_FERRY);
    ScriptContext_Stop();
}

TEST("Cold session replay stages a committed trip before the local warp")
{
    ResetGroupTravelFixture();
    struct CoopGroupTravelRecord commit = Record(COOP_GROUP_TRAVEL_SERVER_COMMIT,
        COOP_GROUP_TRAVEL_ROUTE_FERRY_ORIGINAL, 203, 13);

    PrepareTravelOrigin();
    MaterializeDeparture(MAP_OLIVINE_CITY_PORT_INSIDE);
    CoopGroupTravel_Init();
    CoopGroupTravel_TestSetGrouped(TRUE);
    CoopGroupTravel_TestSetSafe(TRUE);
    CoopGroupTravel_OnSessionReady();
    EXPECT(CoopGroupTravel_ReceiveServer(&commit));
    EXPECT_EQ(CoopGroupTravel_TestState(), 7);
    EXPECT_EQ(JohtoTravel_GetPendingDestination(), JOHTO_TRAVEL_DESTINATION_KANTO_ORIGINAL);
    EXPECT(!CoopGroupTravel_TestAtExactDestination(COOP_GROUP_TRAVEL_ROUTE_FERRY_ORIGINAL));
    ResetTasks();
    UnlockPlayerFieldControls();

    MaterializeDeparture(MAP_KANTO_ORIGINAL_VERMILION_CITY_PORT_INSIDE);
    gSaveBlock1Ptr->pos.x = 8;
    gSaveBlock1Ptr->pos.y = 9;
    gMapHeader.regionMapSectionId = MAPSEC_VERMILION_CITY;
    CoopGroupTravel_Poll();
    EXPECT_EQ(JohtoTravel_GetPendingDestination(), JOHTO_TRAVEL_DESTINATION_NONE);
}

TEST("Cold committed replay resumes at the destination without rewarping")
{
    ResetGroupTravelFixture();
    struct CoopGroupTravelRecord commit = Record(COOP_GROUP_TRAVEL_SERVER_COMMIT,
        COOP_GROUP_TRAVEL_ROUTE_FERRY_ORIGINAL, 205, 15);

    PrepareTravelOrigin();
    EXPECT(JohtoTravel_SetPendingDestination(JOHTO_TRAVEL_DESTINATION_KANTO_ORIGINAL));
    MaterializeDeparture(MAP_KANTO_ORIGINAL_VERMILION_CITY_PORT_INSIDE);
    gSaveBlock1Ptr->pos.x = 8;
    gSaveBlock1Ptr->pos.y = 9;
    gMapHeader.regionMapSectionId = MAPSEC_VERMILION_CITY;
    CoopGroupTravel_Init();
    CoopGroupTravel_TestSetGrouped(TRUE);
    CoopGroupTravel_TestSetSafe(TRUE);
    CoopGroupTravel_OnSessionReady();
    EXPECT(CoopGroupTravel_ReceiveServer(&commit));
    EXPECT_EQ(CoopGroupTravel_TestState(), 7);
    CoopGroupTravel_Poll();
    EXPECT_EQ(JohtoTravel_GetPendingDestination(), JOHTO_TRAVEL_DESTINATION_NONE);
    EXPECT_EQ(gSaveBlock1Ptr->location.mapNum, MAP_NUM(MAP_KANTO_ORIGINAL_VERMILION_CITY_PORT_INSIDE));
    EXPECT_EQ(gSaveBlock1Ptr->pos.x, 8);
    EXPECT_EQ(gSaveBlock1Ptr->pos.y, 9);

    /* Once the arrival side has already committed the save, a reconnect
     * replays COMMIT as APPLIED_PENDING and never schedules another warp. */
    CoopGroupTravel_Init();
    CoopGroupTravel_TestSetGrouped(TRUE);
    CoopGroupTravel_TestSetSafe(TRUE);
    CoopGroupTravel_OnSessionReady();
    EXPECT(CoopGroupTravel_ReceiveServer(&commit));
    EXPECT_EQ(CoopGroupTravel_TestState(), 8);
    CoopGroupTravel_Poll();
    EXPECT_EQ(gSaveBlock1Ptr->location.mapNum, MAP_NUM(MAP_KANTO_ORIGINAL_VERMILION_CITY_PORT_INSIDE));
    EXPECT_EQ(gSaveBlock1Ptr->pos.x, 8);
    EXPECT_EQ(gSaveBlock1Ptr->pos.y, 9);
}

TEST("Cold committed maiden replay resumes both Kanto routes aboard SS Aqua")
{
    ResetGroupTravelFixture();
    static const struct
    {
        u8 route;
        enum JohtoTravelDestination destination;
        u16 arrival_map;
    } sCases[] = {
        {COOP_GROUP_TRAVEL_ROUTE_FERRY_ORIGINAL,
         JOHTO_TRAVEL_DESTINATION_KANTO_ORIGINAL,
         MAP_KANTO_ORIGINAL_VERMILION_CITY_PORT_INSIDE},
        {COOP_GROUP_TRAVEL_ROUTE_FERRY_LATER,
         JOHTO_TRAVEL_DESTINATION_KANTO_LATER,
         MAP_KANTO_LATER_VERMILION_CITY_PORT_INSIDE},
    };
    u32 i;

    for (i = 0; i < ARRAY_COUNT(sCases); i++)
    {
        struct CoopGroupTravelRecord commit = Record(COOP_GROUP_TRAVEL_SERVER_COMMIT,
            sCases[i].route, 206 + i, 16);

        commit.departure = COOP_GROUP_TRAVEL_DEPARTURE_SSAQUA_MAIDEN;
        PrepareTravelOrigin();
        EXPECT(JohtoTravel_SetPendingDestination(sCases[i].destination));
        VarSet(JOHTO_VAR_SSAQUA_STATE, 1);
        MaterializeDeparture(MAP_SSAQUA_1F);
        gSaveBlock1Ptr->pos.x = 29;
        gSaveBlock1Ptr->pos.y = 3;
        CoopGroupTravel_Init();
        CoopGroupTravel_TestSetGrouped(TRUE);
        CoopGroupTravel_TestSetSafe(TRUE);
        CoopGroupTravel_OnSessionReady();
        EXPECT(CoopGroupTravel_ReceiveServer(&commit));
        EXPECT_EQ(CoopGroupTravel_TestState(), 7);
        CoopGroupTravel_Poll();
        EXPECT_EQ(JohtoTravel_GetPendingDestination(), sCases[i].destination);
        EXPECT_EQ(gSaveBlock1Ptr->location.mapNum, MAP_NUM(MAP_SSAQUA_1F));

        MaterializeDeparture(sCases[i].arrival_map);
        gSaveBlock1Ptr->pos.x = 8;
        gSaveBlock1Ptr->pos.y = 9;
        gMapHeader.regionMapSectionId = MAPSEC_VERMILION_CITY;
        CoopGroupTravel_Poll();
        EXPECT_EQ(JohtoTravel_GetPendingDestination(), JOHTO_TRAVEL_DESTINATION_NONE);
    }
}

TEST("Cold recovery rejects malformed, conflicting, and unsolicited server records")
{
    ResetGroupTravelFixture();
    struct CoopGroupTravelRecord request = Record(COOP_GROUP_TRAVEL_SERVER_REQUESTING,
        COOP_GROUP_TRAVEL_ROUTE_GATE_LATER, 204, 0);
    struct CoopGroupTravelRecord malformed = request;
    struct CoopGroupTravelRecord conflicting = Record(COOP_GROUP_TRAVEL_SERVER_COMMIT,
        COOP_GROUP_TRAVEL_ROUTE_GATE_LATER, 205, 14);

    malformed.era = COOP_GROUP_TRAVEL_ERA_ORIGINAL;
    CoopGroupTravel_Init();
    CoopGroupTravel_TestSetGrouped(TRUE);
    MaterializeDeparture(MAP_RECEPTION_GATE);
    CoopGroupTravel_OnSessionReady();
    EXPECT(!CoopGroupTravel_ReceiveServer(&malformed));
    EXPECT_EQ(CoopGroupTravel_TestState(), 0);

    EXPECT(CoopGroupTravel_ReceiveServer(&request));
    EXPECT(!CoopGroupTravel_ReceiveServer(&conflicting));
    EXPECT_EQ(CoopGroupTravel_TestRecord()->request_id, 204);

    CoopGroupTravel_Init();
    CoopGroupTravel_TestSetGrouped(TRUE);
    MaterializeDeparture(MAP_RECEPTION_GATE);
    EXPECT(!CoopGroupTravel_ReceiveServer(&request));
}

TEST("Group travel protocol has strict golden records for all destinations")
{
    ResetGroupTravelFixture();
    u8 route;
    struct CoopGroupTravelRecord record;
    for (route = 1; route <= COOP_GROUP_TRAVEL_ROUTE_SS_TIDAL_SLATEPORT_EXIT; route++)
    {
        if (route == 32 || route == 92 || route == 93) continue;
        record = Record(COOP_GROUP_TRAVEL_CLIENT_REQUEST, route, route, 0);
        EXPECT(CoopGroupTravelProtocol_ValidateClient(&record));
        /* Requesting and client-request records intentionally share their
         * scalar wire shape; bridge direction supplies the distinction. */
        EXPECT(CoopGroupTravelProtocol_ValidateServer(&record));
        record.reserved1[2] = 1;
        EXPECT(!CoopGroupTravelProtocol_ValidateClient(&record));
    }
    record = Record(COOP_GROUP_TRAVEL_CLIENT_REQUEST,
                    COOP_GROUP_TRAVEL_ROUTE_SS_TIDAL_SLATEPORT_EXIT, 96, 0);
    record.route = COOP_GROUP_TRAVEL_ROUTE_SS_TIDAL_SLATEPORT_EXIT + 1;
    EXPECT(!CoopGroupTravelProtocol_ValidateClient(&record));
    record = Record(COOP_GROUP_TRAVEL_CLIENT_REQUEST,
        COOP_GROUP_TRAVEL_ROUTE_FERRY_LATER, 7, 0);
    record.departure = COOP_GROUP_TRAVEL_DEPARTURE_SSAQUA_MAIDEN;
    EXPECT(CoopGroupTravelProtocol_ValidateClient(&record));
    record.route = COOP_GROUP_TRAVEL_ROUTE_TRAIN_ORIGINAL;
    EXPECT(!CoopGroupTravelProtocol_ValidateClient(&record));
    record = Record(COOP_GROUP_TRAVEL_CLIENT_REQUEST, 32, 32, 0);
    EXPECT(!CoopGroupTravelProtocol_ValidateClient(&record));
}

TEST("Seagallop route mapping rejects self and story edges")
{
    u8 route;
    u8 origin;
    u8 destination;
    for (origin = 0; origin < 8; origin++)
    {
        for (destination = 0; destination < 8; destination++)
        {
            gSpecialVar_0x8004 = origin;
            gSpecialVar_0x8006 = destination;
            Special_CoopSeagallopRoute();
            route = gSpecialVar_Result;
            if (origin == destination)
                EXPECT_EQ(route, 0);
            else
            {
                struct CoopGroupTravelRecord request =
                    Record(COOP_GROUP_TRAVEL_CLIENT_REQUEST, route, route, 0);
                EXPECT(CoopGroupTravelProtocol_ValidateClient(&request));
            }
        }
    }
    gSpecialVar_0x8004 = 8;
    gSpecialVar_0x8006 = 1;
    Special_CoopSeagallopRoute();
    EXPECT_EQ(gSpecialVar_Result, 0);
    gSpecialVar_0x8004 = 1;
    gSpecialVar_0x8006 = 8;
    Special_CoopSeagallopRoute();
    EXPECT_EQ(gSpecialVar_Result, 0);
    gSpecialVar_0x8004 = 0;
    gSpecialVar_0x8006 = 9;
    Special_CoopSeagallopRoute();
    EXPECT_EQ(gSpecialVar_Result, 156);
    gSpecialVar_0x8004 = 9;
    gSpecialVar_0x8006 = 0;
    Special_CoopSeagallopRoute();
    EXPECT_EQ(gSpecialVar_Result, 157);
    gSpecialVar_0x8004 = 0;
    gSpecialVar_0x8006 = 10;
    Special_CoopSeagallopRoute();
    EXPECT_EQ(gSpecialVar_Result, 158);
    gSpecialVar_0x8004 = 10;
    gSpecialVar_0x8006 = 0;
    Special_CoopSeagallopRoute();
    EXPECT_EQ(gSpecialVar_Result, 159);
}

TEST("Island ferry requester and responder both need their own ticket")
{
    static const struct { u8 route; u16 ticket; } sCases[] = {
        {COOP_GROUP_TRAVEL_ROUTE_OLIVINE_SOUTHERN_ISLAND, ITEM_EON_TICKET},
        {COOP_GROUP_TRAVEL_ROUTE_OLIVINE_BIRTH_ISLAND, ITEM_AURORA_TICKET},
        {COOP_GROUP_TRAVEL_ROUTE_OLIVINE_FARAWAY_ISLAND, ITEM_OLD_SEA_MAP},
    };
    u32 i;

    for (i = 0; i < ARRAY_COUNT(sCases); i++)
    {
        u16 held = CountTotalItemQuantityInBag(sCases[i].ticket);
        struct CoopGroupTravelRecord offer = Record(COOP_GROUP_TRAVEL_SERVER_OFFER,
            sCases[i].route, i + 200, 9);
        if (held != 0)
            EXPECT(RemoveBagItem(sCases[i].ticket, held));

        ResetGroupTravelFixture();
        CoopGroupTravel_TestSetGrouped(TRUE);
        CoopGroupTravel_TestSetSafe(TRUE);
        MaterializeDeparture(MAP_OLIVINE_CITY_PORT_INSIDE);
        ScriptContext_SetupScript(EventScript_CoopGroupTravelOffer);
        EXPECT_EQ(CoopGroupTravel_BeginFromScript(sCases[i].route,
            COOP_GROUP_TRAVEL_DEPARTURE_FERRY), COOP_GROUP_TRAVEL_BEGIN_REJECTED);
        EndDepartureScript();

        CoopGroupTravel_OnSessionReady();
        EXPECT(CoopGroupTravel_ReceiveServer(&offer));
        CoopGroupTravel_Poll();
        EXPECT_EQ(CoopGroupTravel_GetOffer(NULL), COOP_GROUP_TRAVEL_OFFER_READY);
        EXPECT(CoopGroupTravel_RespondToOffer(TRUE));
        EXPECT_EQ(CoopGroupTravel_TestRecord()->result, COOP_GROUP_TRAVEL_RESULT_DECLINED);
        if (held != 0)
            EXPECT(AddBagItem(sCases[i].ticket, held));
    }
    CoopGroupTravel_Init();
    UnlockPlayerFieldControls();
}

TEST("Lilycove island ferry requires the destination ticket and unlock on both ROMs")
{
    static const struct { u8 route; u16 ticket; u16 unlock; } sCases[] = {
        {COOP_GROUP_TRAVEL_ROUTE_LILYCOVE_SOUTHERN_ISLAND, ITEM_EON_TICKET, FLAG_ENABLE_SHIP_SOUTHERN_ISLAND},
        {COOP_GROUP_TRAVEL_ROUTE_LILYCOVE_NAVEL_ROCK, ITEM_MYSTIC_TICKET, FLAG_ENABLE_SHIP_NAVEL_ROCK},
        {COOP_GROUP_TRAVEL_ROUTE_LILYCOVE_BIRTH_ISLAND, ITEM_AURORA_TICKET, FLAG_ENABLE_SHIP_BIRTH_ISLAND},
        {COOP_GROUP_TRAVEL_ROUTE_LILYCOVE_FARAWAY_ISLAND, ITEM_OLD_SEA_MAP, FLAG_ENABLE_SHIP_FARAWAY_ISLAND},
    };
    u32 i;
    bool8 hadClear = FlagGet(FLAG_SYS_GAME_CLEAR);

    FlagSet(FLAG_SYS_GAME_CLEAR);
    for (i = 0; i < ARRAY_COUNT(sCases); i++)
    {
        u16 held = CountTotalItemQuantityInBag(sCases[i].ticket);
        bool8 hadUnlock = FlagGet(sCases[i].unlock);
        struct CoopGroupTravelRecord offer = Record(COOP_GROUP_TRAVEL_SERVER_OFFER,
            sCases[i].route, i + 240, 9);
        FlagSet(sCases[i].unlock);
        if (held != 0)
            EXPECT(RemoveBagItem(sCases[i].ticket, held));

        ResetGroupTravelFixture();
        CoopGroupTravel_TestSetGrouped(TRUE);
        CoopGroupTravel_TestSetSafe(TRUE);
        MaterializeDeparture(MAP_LILYCOVE_CITY_HARBOR);
        ScriptContext_SetupScript(EventScript_CoopGroupTravelOffer);
        EXPECT_EQ(CoopGroupTravel_BeginFromScript(sCases[i].route,
            COOP_GROUP_TRAVEL_DEPARTURE_FERRY), COOP_GROUP_TRAVEL_BEGIN_REJECTED);
        EndDepartureScript();

        CoopGroupTravel_OnSessionReady();
        EXPECT(CoopGroupTravel_ReceiveServer(&offer));
        CoopGroupTravel_Poll();
        EXPECT_EQ(CoopGroupTravel_GetOffer(NULL), COOP_GROUP_TRAVEL_OFFER_READY);
        EXPECT(CoopGroupTravel_RespondToOffer(TRUE));
        EXPECT_EQ(CoopGroupTravel_TestRecord()->result, COOP_GROUP_TRAVEL_RESULT_DECLINED);
        if (held != 0)
            EXPECT(AddBagItem(sCases[i].ticket, held));
        if (!hadUnlock)
            FlagClear(sCases[i].unlock);
    }
    if (!hadClear)
        FlagClear(FLAG_SYS_GAME_CLEAR);
    CoopGroupTravel_Init();
    UnlockPlayerFieldControls();
}

TEST("Lilycove first-ticket flags advance only when the group ferry commits")
{
    static const struct { u8 route; u16 shown; u16 destination; s16 x; s16 y; } sCases[] = {
        {COOP_GROUP_TRAVEL_ROUTE_LILYCOVE_SOUTHERN_ISLAND, FLAG_SHOWN_EON_TICKET, MAP_SOUTHERN_ISLAND_EXTERIOR, 13, 22},
        {COOP_GROUP_TRAVEL_ROUTE_LILYCOVE_NAVEL_ROCK, FLAG_SHOWN_MYSTIC_TICKET, MAP_NAVEL_ROCK_HARBOR, 8, 4},
        {COOP_GROUP_TRAVEL_ROUTE_LILYCOVE_BIRTH_ISLAND, FLAG_SHOWN_AURORA_TICKET, MAP_BIRTH_ISLAND_HARBOR, 8, 4},
        {COOP_GROUP_TRAVEL_ROUTE_LILYCOVE_FARAWAY_ISLAND, FLAG_SHOWN_OLD_SEA_MAP, MAP_FARAWAY_ISLAND_ENTRANCE, 13, 38},
    };
    u32 i;

    for (i = 0; i < ARRAY_COUNT(sCases); i++)
    {
        bool8 hadShown = FlagGet(sCases[i].shown);
        struct CoopGroupTravelRecord request = Record(COOP_GROUP_TRAVEL_CLIENT_REQUEST,
            sCases[i].route, i + 250, 0);
        struct CoopGroupTravelRecord abort = Record(COOP_GROUP_TRAVEL_SERVER_ABORT,
            sCases[i].route, i + 250, 9);
        struct CoopGroupTravelRecord commit = Record(COOP_GROUP_TRAVEL_SERVER_COMMIT,
            sCases[i].route, i + 251, 9);
        ResetGroupTravelFixture();
        CoopGroupTravel_TestSetSafe(TRUE);
        FlagClear(sCases[i].shown);
        MaterializeDeparture(MAP_LILYCOVE_CITY_HARBOR);
        CoopGroupTravel_TestSeedRequest(&request);
        abort.reason = COOP_GROUP_TRAVEL_REASON_PARTICIPANT_DECLINED;
        EXPECT(CoopGroupTravel_ReceiveServer(&abort));
        EXPECT(!FlagGet(sCases[i].shown));

        request.request_id = commit.request_id;
        CoopGroupTravel_TestSeedRequest(&request);
        EXPECT(CoopGroupTravel_ReceiveServer(&commit));
        EXPECT(FlagGet(sCases[i].shown));
        MaterializeDeparture(sCases[i].destination);
        gSaveBlock1Ptr->pos.x = sCases[i].x;
        gSaveBlock1Ptr->pos.y = sCases[i].y;
        EXPECT(CoopGroupTravel_TestAtExactDestination(sCases[i].route));
        gSaveBlock1Ptr->pos.x++;
        EXPECT(!CoopGroupTravel_TestAtExactDestination(sCases[i].route));
        if (!hadShown)
            FlagClear(sCases[i].shown);
    }
    CoopGroupTravel_Init();
    DiscardSimulatedWarp();
}

TEST("Island ferry group commit preserves each source port as its respawn")
{
    static const struct { u8 route; u16 source; u8 heal; } sCases[] = {
        {COOP_GROUP_TRAVEL_ROUTE_OLIVINE_BATTLE_FRONTIER, MAP_OLIVINE_CITY_PORT_INSIDE,
            HEAL_LOCATION_JOHTO_OLIVINE_CITY},
        {COOP_GROUP_TRAVEL_ROUTE_VERMILION_BATTLE_FRONTIER,
            MAP_KANTO_LATER_VERMILION_CITY_PORT_INSIDE, HEAL_LOCATION_VERMILION_CITY},
    };
    u32 i;

    for (i = 0; i < ARRAY_COUNT(sCases); i++)
    {
        struct CoopGroupTravelRecord request = Record(COOP_GROUP_TRAVEL_CLIENT_REQUEST,
            sCases[i].route, i + 210, 0);
        struct CoopGroupTravelRecord commit = Record(COOP_GROUP_TRAVEL_SERVER_COMMIT,
            sCases[i].route, i + 210, 9);
        ResetGroupTravelFixture();
        CoopGroupTravel_TestSetSafe(TRUE);
        MaterializeDeparture(sCases[i].source);
        CoopGroupTravel_TestSeedRequest(&request);
        EXPECT(CoopGroupTravel_ReceiveServer(&commit));
        EXPECT_EQ(GetHealLocationIndexByWarpData(&gSaveBlock1Ptr->lastHealLocation),
            sCases[i].heal);
    }
    CoopGroupTravel_Init();
    DiscardSimulatedWarp();
}

TEST("Island return ferries require source and exact harbor arrival")
{
    static const struct { u8 route; u16 source; u16 destination; } sCases[] = {
        {COOP_GROUP_TRAVEL_ROUTE_SOUTHERN_ISLAND_LILYCOVE,
            MAP_SOUTHERN_ISLAND_EXTERIOR, MAP_LILYCOVE_CITY_HARBOR},
        {COOP_GROUP_TRAVEL_ROUTE_BIRTH_ISLAND_LILYCOVE,
            MAP_BIRTH_ISLAND_HARBOR, MAP_LILYCOVE_CITY_HARBOR},
        {COOP_GROUP_TRAVEL_ROUTE_FARAWAY_ISLAND_LILYCOVE,
            MAP_FARAWAY_ISLAND_ENTRANCE, MAP_LILYCOVE_CITY_HARBOR},
        {COOP_GROUP_TRAVEL_ROUTE_BATTLE_FRONTIER_SLATEPORT,
            MAP_BATTLE_FRONTIER_OUTSIDE_WEST, MAP_SLATEPORT_CITY_HARBOR},
        {COOP_GROUP_TRAVEL_ROUTE_BATTLE_FRONTIER_LILYCOVE,
            MAP_BATTLE_FRONTIER_OUTSIDE_WEST, MAP_LILYCOVE_CITY_HARBOR},
    };
    u32 i;
    bool8 addedTicket = !CheckBagHasItem(ITEM_SS_TICKET, 1);

    if (addedTicket)
        EXPECT(AddBagItem(ITEM_SS_TICKET, 1));

    for (i = 0; i < ARRAY_COUNT(sCases); i++)
    {
        struct CoopGroupTravelRecord request = Record(COOP_GROUP_TRAVEL_CLIENT_REQUEST,
            sCases[i].route, i + 220, 0);
        struct CoopGroupTravelRecord commit = Record(COOP_GROUP_TRAVEL_SERVER_COMMIT,
            sCases[i].route, i + 220, 9);
        ResetGroupTravelFixture();
        CoopGroupTravel_TestSetGrouped(TRUE);
        CoopGroupTravel_TestSetSafe(TRUE);
        MaterializeDeparture(sCases[i].source);
        ScriptContext_SetupScript(EventScript_CoopGroupTravelOffer);
        EXPECT_EQ(CoopGroupTravel_BeginFromScript(sCases[i].route,
            COOP_GROUP_TRAVEL_DEPARTURE_FERRY), COOP_GROUP_TRAVEL_BEGIN_WAITING);
        EndDepartureScript();
        CoopGroupTravel_TestSeedRequest(&request);
        EXPECT(CoopGroupTravel_ReceiveServer(&commit));
        EXPECT_EQ(JohtoTravel_GetPendingDestination(), JOHTO_TRAVEL_DESTINATION_NONE);
        MaterializeDeparture(sCases[i].destination);
        gSaveBlock1Ptr->pos.x = 8;
        gSaveBlock1Ptr->pos.y = 11;
        EXPECT(CoopGroupTravel_TestAtExactDestination(sCases[i].route));
        gSaveBlock1Ptr->pos.x++;
        EXPECT(!CoopGroupTravel_TestAtExactDestination(sCases[i].route));
    }
    CoopGroupTravel_Init();
    UnlockPlayerFieldControls();
    if (addedTicket)
        EXPECT(RemoveBagItem(ITEM_SS_TICKET, 1));
    DiscardSimulatedWarp();
}

TEST("Battle Frontier return ferry requires the SS Ticket on both ROMs")
{
    u16 held = CountTotalItemQuantityInBag(ITEM_SS_TICKET);
    struct CoopGroupTravelRecord offer = Record(COOP_GROUP_TRAVEL_SERVER_OFFER,
        COOP_GROUP_TRAVEL_ROUTE_BATTLE_FRONTIER_LILYCOVE, 230, 9);

    if (held != 0)
        EXPECT(RemoveBagItem(ITEM_SS_TICKET, held));
    ResetGroupTravelFixture();
    CoopGroupTravel_TestSetGrouped(TRUE);
    CoopGroupTravel_TestSetSafe(TRUE);
    MaterializeDeparture(MAP_BATTLE_FRONTIER_OUTSIDE_WEST);
    ScriptContext_SetupScript(EventScript_CoopGroupTravelOffer);
    EXPECT_EQ(CoopGroupTravel_BeginFromScript(
        COOP_GROUP_TRAVEL_ROUTE_BATTLE_FRONTIER_LILYCOVE,
        COOP_GROUP_TRAVEL_DEPARTURE_FERRY), COOP_GROUP_TRAVEL_BEGIN_REJECTED);
    EndDepartureScript();

    CoopGroupTravel_OnSessionReady();
    EXPECT(CoopGroupTravel_ReceiveServer(&offer));
    CoopGroupTravel_Poll();
    EXPECT_EQ(CoopGroupTravel_GetOffer(NULL), COOP_GROUP_TRAVEL_OFFER_READY);
    EXPECT(CoopGroupTravel_RespondToOffer(TRUE));
    EXPECT_EQ(CoopGroupTravel_TestRecord()->result, COOP_GROUP_TRAVEL_RESULT_DECLINED);
    if (held != 0)
        EXPECT(AddBagItem(ITEM_SS_TICKET, held));
    CoopGroupTravel_Init();
    UnlockPlayerFieldControls();
}

TEST("SS Tidal boarding routes permit Scott's first onboard scene")
{
    static const struct { u8 route; u16 source; u16 board; } sCases[] = {
        {COOP_GROUP_TRAVEL_ROUTE_SS_TIDAL_SLATEPORT_BOARD,
            MAP_SLATEPORT_CITY_HARBOR, SS_TIDAL_BOARD_SLATEPORT},
        {COOP_GROUP_TRAVEL_ROUTE_SS_TIDAL_LILYCOVE_BOARD,
            MAP_LILYCOVE_CITY_HARBOR, SS_TIDAL_BOARD_LILYCOVE},
    };
    bool8 hadClear = FlagGet(FLAG_SYS_GAME_CLEAR);
    bool8 hadScott = FlagGet(FLAG_MET_SCOTT_ON_SS_TIDAL);
    u16 oldScottState = VarGet(VAR_SS_TIDAL_SCOTT_STATE);
    u32 i;

    for (i = 0; i < ARRAY_COUNT(sCases); i++)
    {
        struct CoopGroupTravelRecord request = Record(COOP_GROUP_TRAVEL_CLIENT_REQUEST,
            sCases[i].route, 320 + i, 0);
        struct CoopGroupTravelRecord commit = Record(COOP_GROUP_TRAVEL_SERVER_COMMIT,
            sCases[i].route, 320 + i, 9);
        struct CoopGroupTravelRecord offer = Record(COOP_GROUP_TRAVEL_SERVER_OFFER,
            sCases[i].route, 330 + i, 9);
        EXPECT(CoopGroupTravelProtocol_ValidateClient(&request));
        ResetGroupTravelFixture();
        CoopGroupTravel_TestSetGrouped(TRUE);
        MaterializeDeparture(sCases[i].source);
        FlagSet(FLAG_SYS_GAME_CLEAR);
        FlagClear(FLAG_MET_SCOTT_ON_SS_TIDAL);
        VarSet(VAR_SS_TIDAL_SCOTT_STATE, 0);
        ScriptContext_SetupScript(EventScript_CoopGroupTravelOffer);
        EXPECT_EQ(CoopGroupTravel_BeginFromScript(sCases[i].route,
            COOP_GROUP_TRAVEL_DEPARTURE_FERRY), COOP_GROUP_TRAVEL_BEGIN_WAITING);
        EndDepartureScript();
        CoopGroupTravel_Init();
        CoopGroupTravel_TestSetGrouped(TRUE);
        VarSet(VAR_SS_TIDAL_SCOTT_STATE, 1);
        FlagClear(FLAG_SYS_GAME_CLEAR);
        ScriptContext_SetupScript(EventScript_CoopGroupTravelOffer);
        EXPECT_EQ(CoopGroupTravel_BeginFromScript(sCases[i].route,
            COOP_GROUP_TRAVEL_DEPARTURE_FERRY), COOP_GROUP_TRAVEL_BEGIN_REJECTED);
        EndDepartureScript();
        FlagSet(FLAG_SYS_GAME_CLEAR);
        VarSet(VAR_SS_TIDAL_SCOTT_STATE, 0);
        CoopGroupTravel_TestSetSafe(TRUE);
        CoopGroupTravel_OnSessionReady();
        EXPECT(CoopGroupTravel_ReceiveServer(&offer));
        CoopGroupTravel_Poll();
        EXPECT(CoopGroupTravel_RespondToOffer(TRUE));
        EXPECT_EQ(CoopGroupTravel_TestRecord()->result, COOP_GROUP_TRAVEL_RESULT_ACCEPTED);
        CoopGroupTravel_Init();
        CoopGroupTravel_TestSetGrouped(TRUE);
        CoopGroupTravel_TestSetSafe(TRUE);
        MaterializeDeparture(sCases[i].source);
        CoopGroupTravel_TestSeedRequest(&request);
        EXPECT(CoopGroupTravel_ReceiveServer(&commit));
        EXPECT_EQ(VarGet(VAR_SS_TIDAL_STATE), sCases[i].board);
        EXPECT_EQ(VarGet(VAR_SS_TIDAL_SCOTT_STATE), 0);
        EXPECT(!FlagGet(FLAG_MET_SCOTT_ON_SS_TIDAL));
        MaterializeDeparture(MAP_SS_TIDAL_CORRIDOR);
        gSaveBlock1Ptr->pos.x = 1;
        gSaveBlock1Ptr->pos.y = 10;
        EXPECT(CoopGroupTravel_TestAtExactDestination(sCases[i].route));
    }
    if (!hadClear) FlagClear(FLAG_SYS_GAME_CLEAR);
    if (!hadScott) FlagClear(FLAG_MET_SCOTT_ON_SS_TIDAL);
    VarSet(VAR_SS_TIDAL_SCOTT_STATE, oldScottState);
    CoopGroupTravel_Init();
    UnlockPlayerFieldControls();
    DiscardSimulatedWarp();
}

TEST("Briney routes keep the first voyage reserved and exact arrivals")
{
    static const struct
    {
        u8 route;
        u16 map;
        s16 x;
        s16 y;
    } sCases[] = {
        {COOP_GROUP_TRAVEL_ROUTE_DEWFORD_BRINEY_HOUSE,
            MAP_ROUTE104_MR_BRINEYS_HOUSE, 5, 4},
        {COOP_GROUP_TRAVEL_ROUTE_DEWFORD_ROUTE109, MAP_ROUTE109, 21, 26},
        {COOP_GROUP_TRAVEL_ROUTE_ROUTE109_DEWFORD, MAP_DEWFORD_TOWN, 12, 8},
    };
    u32 i;

    for (i = 0; i < ARRAY_COUNT(sCases); i++)
    {
        struct CoopGroupTravelRecord record = Record(
            COOP_GROUP_TRAVEL_CLIENT_REQUEST, sCases[i].route, 340 + i, 0);
        EXPECT(CoopGroupTravelProtocol_ValidateClient(&record));
        EXPECT(CoopGroupTravelProtocol_ValidateServer(&record));
        gSaveBlock1Ptr->location.mapGroup = MAP_GROUP(sCases[i].map);
        gSaveBlock1Ptr->location.mapNum = MAP_NUM(sCases[i].map);
        gSaveBlock1Ptr->pos.x = sCases[i].x;
        gSaveBlock1Ptr->pos.y = sCases[i].y;
        EXPECT(CoopGroupTravel_TestAtExactDestination(sCases[i].route));
        gSaveBlock1Ptr->pos.x++;
        EXPECT(!CoopGroupTravel_TestAtExactDestination(sCases[i].route));
    }

    {
        struct CoopGroupTravelRecord first = Record(COOP_GROUP_TRAVEL_CLIENT_REQUEST,
            COOP_GROUP_TRAVEL_ROUTE_BRINEY_HOUSE_DEWFORD, 344, 0);
        EXPECT(CoopGroupTravelProtocol_ValidateClient(&first));
        EXPECT(CoopGroupTravelProtocol_ValidateServer(&first));
        first.kind = COOP_GROUP_TRAVEL_CLIENT_SCENE_COMPLETE;
        EXPECT(!CoopGroupTravelProtocol_ValidateClient(&first));
        memset(first.proposal_id, 1, sizeof(first.proposal_id));
        EXPECT(CoopGroupTravelProtocol_ValidateClient(&first));
        first.kind = COOP_GROUP_TRAVEL_SERVER_COMMIT;
        EXPECT(!CoopGroupTravelProtocol_ValidateServer(&first));
    }
}

TEST("First Briney scene completion needs marker acknowledgement and vanilla landing evidence")
{
    struct CoopGroupTravelRecord marker = Record(
        COOP_GROUP_TRAVEL_SERVER_SCENE_MARKER_ACCEPTED,
        COOP_GROUP_TRAVEL_ROUTE_BRINEY_HOUSE_DEWFORD, 345, 8);
    bool8 hadCall = FlagGet(FLAG_ENABLE_NORMAN_MATCH_CALL);
    bool8 hadBriney = FlagGet(FLAG_HIDE_MR_BRINEY_DEWFORD_TOWN);
    bool8 hadBoat = FlagGet(FLAG_HIDE_MR_BRINEY_BOAT_DEWFORD_TOWN);
    bool8 hadRouteBoat = FlagGet(FLAG_HIDE_ROUTE_104_MR_BRINEY_BOAT);
    u16 oldBoard = VarGet(VAR_BOARD_BRINEY_BOAT_STATE);

    ResetGroupTravelFixture();
    MaterializeDeparture(MAP_DEWFORD_TOWN);
    VarSet(VAR_BOARD_BRINEY_BOAT_STATE, 0);
    FlagSet(FLAG_ENABLE_NORMAN_MATCH_CALL);
    FlagClear(FLAG_HIDE_MR_BRINEY_DEWFORD_TOWN);
    FlagClear(FLAG_HIDE_MR_BRINEY_BOAT_DEWFORD_TOWN);
    FlagSet(FLAG_HIDE_ROUTE_104_MR_BRINEY_BOAT);
    Special_CoopGroupTravelFirstBrineySceneComplete();
    EXPECT_EQ(gSpecialVar_Result, FALSE);
    CoopGroupTravel_Poll();
    EXPECT_EQ(gCoopNetBridge.game_to_network.write_index, 0);

    CoopGroupTravel_TestSeedSceneMarkerAccepted(&marker);
    CoopGroupTravel_Poll();
    EXPECT_EQ(gCoopNetBridge.game_to_network.write_index, 0);
    Special_CoopGroupTravelFirstBrineySceneComplete();
    EXPECT_EQ(gSpecialVar_Result, FALSE);
    CoopGroupTravel_Poll();
    EXPECT_EQ(gCoopNetBridge.game_to_network.write_index, 0);
    CoopGroupTravel_TestSetSceneStarted(TRUE);
    CoopGroupTravel_TestOwnControlLock();
    Special_CoopGroupTravelFirstBrineySceneComplete();
    EXPECT_EQ(gSpecialVar_Result, TRUE);
    EXPECT(CoopGroupTravel_IsFirstBrineyReceiptPending());
    MaterializeDeparture(MAP_ROUTE104);
    CoopGroupTravel_Poll();
    EXPECT_EQ(gCoopNetBridge.game_to_network.write_index, 0);
    MaterializeDeparture(MAP_DEWFORD_TOWN);
    CoopGroupTravel_Poll();
    EXPECT(ArePlayerFieldControlsLocked());
    CoopGroupTravel_OnTransportLost();
    EXPECT(ArePlayerFieldControlsLocked());
    EXPECT_EQ(gCoopNetBridge.game_to_network.write_index, 1);
    EXPECT_EQ(gCoopNetBridge.game_to_network.entries[0].type,
        COOP_BRIDGE_MESSAGE_GROUP_TRAVEL_CLIENT);
    EXPECT_EQ(gCoopNetBridge.game_to_network.entries[0].payload[0],
        COOP_GROUP_TRAVEL_CLIENT_SCENE_COMPLETE);
    EXPECT_EQ(gCoopNetBridge.game_to_network.entries[0].payload[1],
        COOP_GROUP_TRAVEL_ROUTE_BRINEY_HOUSE_DEWFORD);
    EXPECT_EQ(gCoopNetBridge.game_to_network.entries[0].payload[12], 8);
    /* The scripted save begins only after this scene record is enqueued. */
    Special_CoopGroupTravelFirstBrineyReceiptComplete();
    EXPECT_EQ(gSpecialVar_Result, FALSE);

    if (hadCall) FlagSet(FLAG_ENABLE_NORMAN_MATCH_CALL);
    else FlagClear(FLAG_ENABLE_NORMAN_MATCH_CALL);
    if (hadBriney) FlagSet(FLAG_HIDE_MR_BRINEY_DEWFORD_TOWN);
    else FlagClear(FLAG_HIDE_MR_BRINEY_DEWFORD_TOWN);
    if (hadBoat) FlagSet(FLAG_HIDE_MR_BRINEY_BOAT_DEWFORD_TOWN);
    else FlagClear(FLAG_HIDE_MR_BRINEY_BOAT_DEWFORD_TOWN);
    if (hadRouteBoat) FlagSet(FLAG_HIDE_ROUTE_104_MR_BRINEY_BOAT);
    else FlagClear(FLAG_HIDE_ROUTE_104_MR_BRINEY_BOAT);
    VarSet(VAR_BOARD_BRINEY_BOAT_STATE, oldBoard);
    CoopGroupTravel_Init();
}

TEST("Bill story routes require complete destination evidence and exact server completion")
{
    struct CoopGroupTravelRecord marker = Record(
        COOP_GROUP_TRAVEL_SERVER_SCENE_MARKER_ACCEPTED,
        COOP_GROUP_TRAVEL_ROUTE_BILL_CINNABAR_ONE, 400, 12);
    struct CoopGroupTravelRecord complete = marker;
    u16 oldCinnabar = VarGet(VAR_MAP_SCENE_CINNABAR_ISLAND);
    u16 oldHarbor = VarGet(VAR_MAP_SCENE_ONE_ISLAND_HARBOR);
    u16 oldCenter = VarGet(VAR_MAP_SCENE_ONE_ISLAND_POKEMON_CENTER_1F);
    bool8 oldMap = FlagGet(FLAG_SYS_SEVII_MAP_123);
    bool8 oldPc = FlagGet(FLAG_SYS_PC_STORAGE_DISABLED);
    bool8 oldLostelleGame = FlagGet(FLAG_HIDE_TWO_ISLAND_GAME_CORNER_LOSTELLE);
    bool8 oldLostelleHome = FlagGet(FLAG_HIDE_LOSTELLE_IN_HER_HOME);

    ResetGroupTravelFixture();
    CoopGroupTravel_TestSetSafe(TRUE);
    MaterializeDeparture(MAP_ONE_ISLAND_POKEMON_CENTER_1F);
    VarSet(VAR_MAP_SCENE_CINNABAR_ISLAND, 2);
    VarSet(VAR_MAP_SCENE_ONE_ISLAND_HARBOR, 3);
    VarSet(VAR_MAP_SCENE_ONE_ISLAND_POKEMON_CENTER_1F, 1);
    FlagSet(FLAG_SYS_SEVII_MAP_123);
    FlagSet(FLAG_SYS_PC_STORAGE_DISABLED);
    Special_CoopGroupTravelBillSceneComplete();
    EXPECT_EQ(gSpecialVar_Result, FALSE);
    CoopGroupTravel_TestSeedSceneMarkerAccepted(&marker);
    CoopGroupTravel_TestSetSceneStarted(TRUE);
    CoopGroupTravel_TestOwnControlLock();
    Special_CoopGroupTravelBillSceneComplete();
    EXPECT_EQ(gSpecialVar_Result, TRUE);
    EXPECT(CoopGroupTravel_IsFirstBrineyReceiptPending());
    complete.kind = COOP_GROUP_TRAVEL_SERVER_COMPLETE;
    complete.result = COOP_GROUP_TRAVEL_RESULT_APPLIED;
    EXPECT(!CoopGroupTravel_ReceiveServer(&complete));
    MaterializeDeparture(MAP_ONE_ISLAND_HARBOR);
    CoopGroupTravel_Poll();
    EXPECT_EQ(gCoopNetBridge.game_to_network.write_index, 0);
    MaterializeDeparture(MAP_ONE_ISLAND_POKEMON_CENTER_1F);
    CoopGroupTravel_Poll();
    EXPECT_EQ(gCoopNetBridge.game_to_network.write_index, 1);
    EXPECT(ArePlayerFieldControlsLocked());
    complete.proposal_id[0]++;
    EXPECT(!CoopGroupTravel_ReceiveServer(&complete));
    complete.proposal_id[0]--;
    CoopGroupTravel_OnSessionReady();
    CoopGroupTravel_OnTransportLost();
    EXPECT(!CoopGroupTravel_ReceiveServer(&complete));
    CoopGroupTravel_OnSessionReady();
    EXPECT(CoopGroupTravel_ReceiveServer(&complete));
    EXPECT(CoopGroupTravel_ReceiveServer(&complete));
    EXPECT(!CoopGroupTravel_IsFirstBrineyReceiptPending());

    ResetGroupTravelFixture();
    marker = Record(COOP_GROUP_TRAVEL_SERVER_SCENE_MARKER_ACCEPTED,
        COOP_GROUP_TRAVEL_ROUTE_BILL_ONE_CINNABAR, 401, 13);
    complete = marker;
    complete.kind = COOP_GROUP_TRAVEL_SERVER_COMPLETE;
    complete.result = COOP_GROUP_TRAVEL_RESULT_APPLIED;
    CoopGroupTravel_TestSeedSceneMarkerAccepted(&marker);
    CoopGroupTravel_TestSetSceneStarted(TRUE);
    MaterializeDeparture(MAP_CINNABAR_ISLAND);
    VarSet(VAR_MAP_SCENE_ONE_ISLAND_POKEMON_CENTER_1F, 3);
    VarSet(VAR_MAP_SCENE_CINNABAR_ISLAND, 3);
    FlagSet(FLAG_HIDE_TWO_ISLAND_GAME_CORNER_LOSTELLE);
    FlagClear(FLAG_HIDE_LOSTELLE_IN_HER_HOME);
    Special_CoopGroupTravelBillSceneComplete();
    EXPECT_EQ(gSpecialVar_Result, FALSE);
    VarSet(VAR_MAP_SCENE_CINNABAR_ISLAND, 4);
    Special_CoopGroupTravelBillSceneComplete();
    EXPECT_EQ(gSpecialVar_Result, TRUE);
    CoopGroupTravel_Poll();
    EXPECT(!CoopGroupTravel_ReceiveServer(&complete));
    CoopGroupTravel_OnSessionReady();
    EXPECT(CoopGroupTravel_ReceiveServer(&complete));

    VarSet(VAR_MAP_SCENE_CINNABAR_ISLAND, oldCinnabar);
    VarSet(VAR_MAP_SCENE_ONE_ISLAND_HARBOR, oldHarbor);
    VarSet(VAR_MAP_SCENE_ONE_ISLAND_POKEMON_CENTER_1F, oldCenter);
    if (oldMap) FlagSet(FLAG_SYS_SEVII_MAP_123); else FlagClear(FLAG_SYS_SEVII_MAP_123);
    if (oldPc) FlagSet(FLAG_SYS_PC_STORAGE_DISABLED); else FlagClear(FLAG_SYS_PC_STORAGE_DISABLED);
    if (oldLostelleGame) FlagSet(FLAG_HIDE_TWO_ISLAND_GAME_CORNER_LOSTELLE);
    else FlagClear(FLAG_HIDE_TWO_ISLAND_GAME_CORNER_LOSTELLE);
    if (oldLostelleHome) FlagSet(FLAG_HIDE_LOSTELLE_IN_HER_HOME);
    else FlagClear(FLAG_HIDE_LOSTELLE_IN_HER_HOME);
    ResetGroupTravelFixture();
}

TEST("Bill story departures reject wrong map and unfinished scene")
{
    u16 oldCinnabar = VarGet(VAR_MAP_SCENE_CINNABAR_ISLAND);
    u16 oldCinnabarSecond = VarGet(VAR_MAP_SCENE_CINNABAR_ISLAND_2);
    u16 oldCenter = VarGet(VAR_MAP_SCENE_ONE_ISLAND_POKEMON_CENTER_1F);
    s16 oldX = gSaveBlock1Ptr->pos.x;
    s16 oldY = gSaveBlock1Ptr->pos.y;
    bool8 oldBillHidden = FlagGet(FLAG_HIDE_CINNABAR_BILL);
    bool8 oldAlternateScene = FlagGet(FLAG_TEMP_2);
    ResetGroupTravelFixture();
    CoopGroupTravel_TestSetGrouped(TRUE);
    CoopGroupTravel_TestSetSafe(TRUE);
    MaterializeDeparture(MAP_CINNABAR_ISLAND_POKEMON_CENTER_1F);
    VarSet(VAR_MAP_SCENE_CINNABAR_ISLAND, 1);
    VarSet(VAR_MAP_SCENE_CINNABAR_ISLAND_2, 3);
    ScriptContext_SetupScript(EventScript_CoopGroupTravelOffer);
    EXPECT_EQ(CoopGroupTravel_BeginFromScript(COOP_GROUP_TRAVEL_ROUTE_BILL_CINNABAR_ONE,
        COOP_GROUP_TRAVEL_DEPARTURE_FERRY), COOP_GROUP_TRAVEL_BEGIN_REJECTED);
    ScriptContext_Stop();
    MaterializeDeparture(MAP_CINNABAR_ISLAND);
    gSaveBlock1Ptr->pos.x = 20;
    gSaveBlock1Ptr->pos.y = 5;
    FlagClear(FLAG_HIDE_CINNABAR_BILL);
    FlagClear(FLAG_TEMP_2);
    VarSet(VAR_MAP_SCENE_CINNABAR_ISLAND, 2);
    VarSet(VAR_MAP_SCENE_CINNABAR_ISLAND_2, 2);
    ScriptContext_SetupScript(EventScript_CoopGroupTravelOffer);
    EXPECT_EQ(CoopGroupTravel_BeginFromScript(COOP_GROUP_TRAVEL_ROUTE_BILL_CINNABAR_ONE,
        COOP_GROUP_TRAVEL_DEPARTURE_FERRY), COOP_GROUP_TRAVEL_BEGIN_REJECTED);
    ScriptContext_Stop();
    VarSet(VAR_MAP_SCENE_CINNABAR_ISLAND_2, 3);
    gSaveBlock1Ptr->pos.x = 19;
    ScriptContext_SetupScript(EventScript_CoopGroupTravelOffer);
    EXPECT_EQ(CoopGroupTravel_BeginFromScript(COOP_GROUP_TRAVEL_ROUTE_BILL_CINNABAR_ONE,
        COOP_GROUP_TRAVEL_DEPARTURE_FERRY), COOP_GROUP_TRAVEL_BEGIN_REJECTED);
    ScriptContext_Stop();
    gSaveBlock1Ptr->pos.x = 20;
    FlagSet(FLAG_HIDE_CINNABAR_BILL);
    ScriptContext_SetupScript(EventScript_CoopGroupTravelOffer);
    EXPECT_EQ(CoopGroupTravel_BeginFromScript(COOP_GROUP_TRAVEL_ROUTE_BILL_CINNABAR_ONE,
        COOP_GROUP_TRAVEL_DEPARTURE_FERRY), COOP_GROUP_TRAVEL_BEGIN_REJECTED);
    ScriptContext_Stop();
    FlagClear(FLAG_HIDE_CINNABAR_BILL);
    ScriptContext_SetupScript(EventScript_CoopGroupTravelOffer);
    EXPECT_EQ(CoopGroupTravel_BeginFromScript(COOP_GROUP_TRAVEL_ROUTE_BILL_CINNABAR_ONE,
        COOP_GROUP_TRAVEL_DEPARTURE_FERRY), COOP_GROUP_TRAVEL_BEGIN_WAITING);
    ScriptContext_Stop();
    EXPECT(CoopGroupTravel_Cancel());
    EXPECT_EQ(VarGet(VAR_MAP_SCENE_CINNABAR_ISLAND), 2);
    EXPECT_EQ(VarGet(VAR_MAP_SCENE_CINNABAR_ISLAND_2), 3);
    ResetGroupTravelFixture();
    CoopGroupTravel_TestSetGrouped(TRUE);
    CoopGroupTravel_TestSetSafe(TRUE);
    MaterializeDeparture(MAP_CINNABAR_ISLAND);
    gSaveBlock1Ptr->pos.x = 20;
    gSaveBlock1Ptr->pos.y = 6;
    FlagClear(FLAG_HIDE_CINNABAR_BILL);
    FlagClear(FLAG_TEMP_2);
    VarSet(VAR_MAP_SCENE_CINNABAR_ISLAND, 2);
    VarSet(VAR_MAP_SCENE_CINNABAR_ISLAND_2, 3);
    ScriptContext_SetupScript(EventScript_CoopGroupTravelOffer);
    EXPECT_EQ(CoopGroupTravel_BeginFromScript(COOP_GROUP_TRAVEL_ROUTE_BILL_CINNABAR_ONE,
        COOP_GROUP_TRAVEL_DEPARTURE_FERRY), COOP_GROUP_TRAVEL_BEGIN_WAITING);
    ScriptContext_Stop();
    EXPECT(CoopGroupTravel_Cancel());
    ResetGroupTravelFixture();
    CoopGroupTravel_TestSetGrouped(TRUE);
    CoopGroupTravel_TestSetSafe(TRUE);
    VarSet(VAR_MAP_SCENE_CINNABAR_ISLAND, 1);
    MaterializeDeparture(MAP_ONE_ISLAND_HARBOR);
    ScriptContext_SetupScript(EventScript_CoopGroupTravelOffer);
    EXPECT_EQ(CoopGroupTravel_BeginFromScript(COOP_GROUP_TRAVEL_ROUTE_BILL_ONE_CINNABAR,
        COOP_GROUP_TRAVEL_DEPARTURE_FERRY), COOP_GROUP_TRAVEL_BEGIN_REJECTED);
    ScriptContext_Stop();
    MaterializeDeparture(MAP_ONE_ISLAND_POKEMON_CENTER_1F);
    VarSet(VAR_MAP_SCENE_ONE_ISLAND_POKEMON_CENTER_1F, 1);
    ScriptContext_SetupScript(EventScript_CoopGroupTravelOffer);
    EXPECT_EQ(CoopGroupTravel_BeginFromScript(COOP_GROUP_TRAVEL_ROUTE_BILL_ONE_CINNABAR,
        COOP_GROUP_TRAVEL_DEPARTURE_FERRY), COOP_GROUP_TRAVEL_BEGIN_REJECTED);
    ScriptContext_Stop();
    VarSet(VAR_MAP_SCENE_CINNABAR_ISLAND, oldCinnabar);
    VarSet(VAR_MAP_SCENE_CINNABAR_ISLAND_2, oldCinnabarSecond);
    VarSet(VAR_MAP_SCENE_ONE_ISLAND_POKEMON_CENTER_1F, oldCenter);
    gSaveBlock1Ptr->pos.x = oldX;
    gSaveBlock1Ptr->pos.y = oldY;
    if (oldBillHidden) FlagSet(FLAG_HIDE_CINNABAR_BILL); else FlagClear(FLAG_HIDE_CINNABAR_BILL);
    if (oldAlternateScene) FlagSet(FLAG_TEMP_2); else FlagClear(FLAG_TEMP_2);
    ResetGroupTravelFixture();
}

TEST("Bill return stages beside a trigger and reconstructs every farewell lane")
{
    struct CoopGroupTravelRecord marker = Record(
        COOP_GROUP_TRAVEL_SERVER_SCENE_MARKER_ACCEPTED,
        COOP_GROUP_TRAVEL_ROUTE_BILL_ONE_CINNABAR, 402, 14);
    u8 oldGroup = gSaveBlock1Ptr->location.mapGroup;
    u8 oldMap = gSaveBlock1Ptr->location.mapNum;
    s16 oldX = gSaveBlock1Ptr->pos.x;
    s16 oldY = gSaveBlock1Ptr->pos.y;
    u16 oldScene = VarGet(VAR_MAP_SCENE_ONE_ISLAND_POKEMON_CENTER_1F);
    u16 oldLane = VarGet(VAR_TEMP_1);
    u8 lane;

    ResetGroupTravelFixture();
    CoopGroupTravel_TestSetGrouped(TRUE);
    CoopGroupTravel_TestSetSafe(TRUE);
    MaterializeDeparture(MAP_ONE_ISLAND_POKEMON_CENTER_1F);
    VarSet(VAR_MAP_SCENE_ONE_ISLAND_POKEMON_CENTER_1F, 2);
    gSaveBlock1Ptr->pos.x = 10;
    gSaveBlock1Ptr->pos.y = 6;
    ScriptContext_SetupScript(EventScript_CoopGroupTravelOffer);
    EXPECT_EQ(CoopGroupTravel_BeginFromScript(COOP_GROUP_TRAVEL_ROUTE_BILL_ONE_CINNABAR,
        COOP_GROUP_TRAVEL_DEPARTURE_FERRY), COOP_GROUP_TRAVEL_BEGIN_REJECTED);
    ScriptContext_Stop();
    gSaveBlock1Ptr->pos.x = 12;
    gSaveBlock1Ptr->pos.y = 5;
    ScriptContext_SetupScript(EventScript_CoopGroupTravelOffer);
    EXPECT_EQ(CoopGroupTravel_BeginFromScript(COOP_GROUP_TRAVEL_ROUTE_BILL_ONE_CINNABAR,
        COOP_GROUP_TRAVEL_DEPARTURE_FERRY), COOP_GROUP_TRAVEL_BEGIN_REJECTED);
    ScriptContext_Stop();
    gSaveBlock1Ptr->pos.y = 10;
    ScriptContext_SetupScript(EventScript_CoopGroupTravelOffer);
    EXPECT_EQ(CoopGroupTravel_BeginFromScript(COOP_GROUP_TRAVEL_ROUTE_BILL_ONE_CINNABAR,
        COOP_GROUP_TRAVEL_DEPARTURE_FERRY), COOP_GROUP_TRAVEL_BEGIN_REJECTED);
    ScriptContext_Stop();

    for (lane = 1; lane <= 4; lane++)
    {
        ResetGroupTravelFixture();
        CoopGroupTravel_TestSetGrouped(TRUE);
        CoopGroupTravel_TestSetSafe(TRUE);
        MaterializeDeparture(MAP_ONE_ISLAND_POKEMON_CENTER_1F);
        VarSet(VAR_MAP_SCENE_ONE_ISLAND_POKEMON_CENTER_1F, 2);
        gSaveBlock1Ptr->pos.x = 11;
        gSaveBlock1Ptr->pos.y = lane + 5;
        VarSet(VAR_TEMP_1, lane == 4 ? 1 : 4);
        CoopGroupTravel_TestSeedSceneMarkerAccepted(&marker);
        CoopGroupTravel_Poll();
        EXPECT_EQ(VarGet(VAR_TEMP_1), lane);
        ScriptContext_Stop();
    }

    ResetGroupTravelFixture();
    CoopGroupTravel_TestSetGrouped(TRUE);
    CoopGroupTravel_TestSetSafe(TRUE);
    MaterializeDeparture(MAP_ONE_ISLAND_POKEMON_CENTER_1F);
    VarSet(VAR_MAP_SCENE_ONE_ISLAND_POKEMON_CENTER_1F, 2);
    gSaveBlock1Ptr->pos.x = 12;
    gSaveBlock1Ptr->pos.y = 6;
    VarSet(VAR_TEMP_1, 4);
    CoopGroupTravel_TestSeedSceneMarkerAccepted(&marker);
    CoopGroupTravel_Poll();
    EXPECT_EQ(VarGet(VAR_TEMP_1), 1);
    ScriptContext_Stop();

    gSaveBlock1Ptr->location.mapGroup = oldGroup;
    gSaveBlock1Ptr->location.mapNum = oldMap;
    gSaveBlock1Ptr->pos.x = oldX;
    gSaveBlock1Ptr->pos.y = oldY;
    VarSet(VAR_MAP_SCENE_ONE_ISLAND_POKEMON_CENTER_1F, oldScene);
    VarSet(VAR_TEMP_1, oldLane);
    ResetGroupTravelFixture();
}

TEST("First Briney group voyage starts only after accepted marker and complete clears landing")
{
    struct CoopGroupTravelRecord ready = Record(
        COOP_GROUP_TRAVEL_SERVER_SCENE_READY,
        COOP_GROUP_TRAVEL_ROUTE_BRINEY_HOUSE_DEWFORD, 346, 9);
    struct CoopGroupTravelRecord accepted = ready;
    struct CoopGroupTravelRecord complete = ready;
    struct CoopGroupTravelRecord abort = ready;
    bool8 hadGym = FlagGet(FLAG_DEFEATED_PETALBURG_GYM);
    bool8 hadHouse = FlagGet(FLAG_HIDE_BRINEYS_HOUSE_MR_BRINEY);
    bool8 hadIntro = FlagGet(FLAG_MR_BRINEY_SAILING_INTRO);
    bool8 hadCall = FlagGet(FLAG_ENABLE_NORMAN_MATCH_CALL);
    bool8 hadBriney = FlagGet(FLAG_HIDE_MR_BRINEY_DEWFORD_TOWN);
    bool8 hadBoat = FlagGet(FLAG_HIDE_MR_BRINEY_BOAT_DEWFORD_TOWN);
    bool8 hadRouteBoat = FlagGet(FLAG_HIDE_ROUTE_104_MR_BRINEY_BOAT);
    u16 oldBoard = VarGet(VAR_BOARD_BRINEY_BOAT_STATE);

    ResetGroupTravelFixture();
    CoopGroupTravel_TestSetGrouped(TRUE);
    CoopGroupTravel_TestSetSafe(TRUE);
    MaterializeDeparture(MAP_ROUTE104_MR_BRINEYS_HOUSE);
    FlagClear(FLAG_DEFEATED_PETALBURG_GYM);
    FlagClear(FLAG_HIDE_BRINEYS_HOUSE_MR_BRINEY);
    FlagClear(FLAG_MR_BRINEY_SAILING_INTRO);
    VarSet(VAR_BOARD_BRINEY_BOAT_STATE, 0);
    ScriptContext_SetupScript(EventScript_CoopGroupTravelOffer);
    EXPECT_EQ(CoopGroupTravel_BeginFromScript(COOP_GROUP_TRAVEL_ROUTE_BRINEY_HOUSE_DEWFORD,
        COOP_GROUP_TRAVEL_DEPARTURE_FERRY), COOP_GROUP_TRAVEL_BEGIN_WAITING);
    ready.request_id = CoopGroupTravel_TestRecord()->request_id;
    accepted.request_id = ready.request_id;
    complete.request_id = ready.request_id;
    abort.request_id = ready.request_id;
    EXPECT(!FlagGet(FLAG_MR_BRINEY_SAILING_INTRO));
    EXPECT_EQ(VarGet(VAR_BOARD_BRINEY_BOAT_STATE), 0);
    EndDepartureScript();
    EXPECT(CoopGroupTravel_ReceiveServer(&ready));
    CoopGroupTravel_Poll();
    EXPECT(!ScriptContext_IsEnabled());
    accepted.kind = COOP_GROUP_TRAVEL_SERVER_SCENE_MARKER_ACCEPTED;
    EXPECT(CoopGroupTravel_ReceiveServer(&accepted));
    CoopGroupTravel_Poll();
    EXPECT(ScriptContext_IsEnabled());
    EXPECT(!FlagGet(FLAG_MR_BRINEY_SAILING_INTRO));
    abort.kind = COOP_GROUP_TRAVEL_SERVER_ABORT;
    abort.reason = COOP_GROUP_TRAVEL_REASON_CONFLICT;
    EXPECT(!CoopGroupTravel_ReceiveServer(&abort));
    ScriptContext_Stop();
    CoopGroupTravel_Init();

    ResetGroupTravelFixture();
    CoopGroupTravel_TestSetGrouped(TRUE);
    CoopGroupTravel_TestSetSafe(TRUE);
    MaterializeDeparture(MAP_ROUTE104_MR_BRINEYS_HOUSE);
    CoopGroupTravel_OnSessionReady();
    EXPECT(CoopGroupTravel_ReceiveServer(&accepted));
    CoopGroupTravel_Poll();
    EXPECT(ScriptContext_IsEnabled());
    ScriptContext_Stop();
    CoopGroupTravel_Init();

    ResetGroupTravelFixture();
    CoopGroupTravel_OnSessionReady();
    CoopGroupTravel_TestSeedSceneMarkerAccepted(&accepted);
    CoopGroupTravel_TestSetSceneStarted(TRUE);
    MaterializeDeparture(MAP_DEWFORD_TOWN);
    VarSet(VAR_BOARD_BRINEY_BOAT_STATE, 0);
    FlagSet(FLAG_ENABLE_NORMAN_MATCH_CALL);
    FlagClear(FLAG_HIDE_MR_BRINEY_DEWFORD_TOWN);
    FlagClear(FLAG_HIDE_MR_BRINEY_BOAT_DEWFORD_TOWN);
    FlagSet(FLAG_HIDE_ROUTE_104_MR_BRINEY_BOAT);
    complete.kind = COOP_GROUP_TRAVEL_SERVER_COMPLETE;
    complete.result = COOP_GROUP_TRAVEL_RESULT_APPLIED;
    EXPECT(!CoopGroupTravel_ReceiveServer(&complete));
    Special_CoopGroupTravelFirstBrineySceneComplete();
    CoopGroupTravel_Poll();
    EXPECT(CoopGroupTravel_IsFirstBrineyReceiptPending());
    complete.proposal_id[0]++;
    EXPECT(!CoopGroupTravel_ReceiveServer(&complete));
    complete.proposal_id[0]--;
    MaterializeDeparture(MAP_ROUTE104);
    EXPECT(!CoopGroupTravel_ReceiveServer(&complete));
    MaterializeDeparture(MAP_DEWFORD_TOWN);
    EXPECT(CoopGroupTravel_ReceiveServer(&complete));
    EXPECT(!CoopGroupTravel_IsFirstBrineyReceiptPending());
    Special_CoopGroupTravelFirstBrineyReceiptComplete();
    EXPECT_EQ(gSpecialVar_Result, TRUE);
    EXPECT_EQ(CoopGroupTravel_TestState(), 0);
    EXPECT(CoopGroupTravel_ReceiveServer(&complete));
    complete.proposal_id[0]++;
    EXPECT(!CoopGroupTravel_ReceiveServer(&complete));

    if (hadGym) FlagSet(FLAG_DEFEATED_PETALBURG_GYM);
    else FlagClear(FLAG_DEFEATED_PETALBURG_GYM);
    if (hadHouse) FlagSet(FLAG_HIDE_BRINEYS_HOUSE_MR_BRINEY);
    else FlagClear(FLAG_HIDE_BRINEYS_HOUSE_MR_BRINEY);
    if (hadIntro) FlagSet(FLAG_MR_BRINEY_SAILING_INTRO);
    else FlagClear(FLAG_MR_BRINEY_SAILING_INTRO);
    if (hadCall) FlagSet(FLAG_ENABLE_NORMAN_MATCH_CALL);
    else FlagClear(FLAG_ENABLE_NORMAN_MATCH_CALL);
    if (hadBriney) FlagSet(FLAG_HIDE_MR_BRINEY_DEWFORD_TOWN);
    else FlagClear(FLAG_HIDE_MR_BRINEY_DEWFORD_TOWN);
    if (hadBoat) FlagSet(FLAG_HIDE_MR_BRINEY_BOAT_DEWFORD_TOWN);
    else FlagClear(FLAG_HIDE_MR_BRINEY_BOAT_DEWFORD_TOWN);
    if (hadRouteBoat) FlagSet(FLAG_HIDE_ROUTE_104_MR_BRINEY_BOAT);
    else FlagClear(FLAG_HIDE_ROUTE_104_MR_BRINEY_BOAT);
    VarSet(VAR_BOARD_BRINEY_BOAT_STATE, oldBoard);
    ResetGroupTravelFixture();
}

TEST("First Briney scripted save requires online checkpoint before local flash")
{
    struct CoopGroupTravelRecord marker = Record(
        COOP_GROUP_TRAVEL_SERVER_SCENE_MARKER_ACCEPTED,
        COOP_GROUP_TRAVEL_ROUTE_BRINEY_HOUSE_DEWFORD, 347, 10);
    struct CoopBridgeMessage message;

    ResetGroupTravelFixture();
    CoopNetBridge_Init();
    while (CoopNetBridge_DequeueGameToNetwork(&message));
    CoopGroupTravel_TestSeedSceneMarkerAccepted(&marker);
    CoopGroupTravel_TestSetSceneStarted(TRUE);
    MaterializeDeparture(MAP_DEWFORD_TOWN);
    Special_CoopGroupTravelFirstBrineySceneComplete();
    EXPECT(CoopGroupTravel_IsFirstBrineyReceiptPending());

    CoopStartMenu_TestSetSaveDryRun(TRUE);
    SaveGame();
    EXPECT_EQ(CoopStartMenu_TestRunSaveSavingMessageCallback(), COOP_START_MENU_TEST_SAVE_IN_PROGRESS);
    EXPECT_EQ(CoopStartMenu_TestRunSaveDoSaveCallback(), COOP_START_MENU_TEST_SAVE_IN_PROGRESS);
    EXPECT_EQ(CoopStartMenu_TestRunCheckpointAbortCallback(), COOP_START_MENU_TEST_SAVE_CANCELED);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    CoopStartMenu_TestSetSaveDryRun(FALSE);
    ResetGroupTravelFixture();
}

TEST("Dewford to Route 109 follows the Steven letter gate before Devon Goods delivery")
{
    bool8 hadLetter = FlagGet(FLAG_DELIVERED_STEVEN_LETTER);
    bool8 hadGoods = FlagGet(FLAG_DELIVERED_DEVON_GOODS);
    bool8 hadGym = FlagGet(FLAG_DEFEATED_PETALBURG_GYM);
    bool8 hadBriney = FlagGet(FLAG_HIDE_MR_BRINEY_DEWFORD_TOWN);
    bool8 hadBoat = FlagGet(FLAG_HIDE_MR_BRINEY_BOAT_DEWFORD_TOWN);
    struct CoopGroupTravelRecord abort;

    FlagClear(FLAG_DELIVERED_STEVEN_LETTER);
    FlagClear(FLAG_DELIVERED_DEVON_GOODS);
    FlagClear(FLAG_DEFEATED_PETALBURG_GYM);
    FlagClear(FLAG_HIDE_MR_BRINEY_DEWFORD_TOWN);
    FlagClear(FLAG_HIDE_MR_BRINEY_BOAT_DEWFORD_TOWN);
    ResetGroupTravelFixture();
    CoopGroupTravel_TestSetGrouped(TRUE);
    CoopGroupTravel_TestSetSafe(TRUE);
    MaterializeDeparture(MAP_DEWFORD_TOWN);
    ScriptContext_SetupScript(EventScript_CoopGroupTravelOffer);
    EXPECT_EQ(CoopGroupTravel_BeginFromScript(COOP_GROUP_TRAVEL_ROUTE_DEWFORD_ROUTE109,
        COOP_GROUP_TRAVEL_DEPARTURE_FERRY), COOP_GROUP_TRAVEL_BEGIN_REJECTED);
    FlagSet(FLAG_DELIVERED_STEVEN_LETTER);
    EXPECT_EQ(CoopGroupTravel_BeginFromScript(COOP_GROUP_TRAVEL_ROUTE_DEWFORD_ROUTE109,
        COOP_GROUP_TRAVEL_DEPARTURE_FERRY), COOP_GROUP_TRAVEL_BEGIN_WAITING);
    EXPECT(!FlagGet(FLAG_DELIVERED_DEVON_GOODS));
    abort = *CoopGroupTravel_TestRecord();
    abort.kind = COOP_GROUP_TRAVEL_SERVER_ABORT;
    abort.reason = COOP_GROUP_TRAVEL_REASON_CONFLICT;
    EXPECT(CoopGroupTravel_ReceiveServer(&abort));
    EndDepartureScript();
    CoopGroupTravel_Init();
    if (!hadLetter) FlagClear(FLAG_DELIVERED_STEVEN_LETTER);
    if (hadGoods) FlagSet(FLAG_DELIVERED_DEVON_GOODS);
    if (hadGym) FlagSet(FLAG_DEFEATED_PETALBURG_GYM);
    if (hadBriney) FlagSet(FLAG_HIDE_MR_BRINEY_DEWFORD_TOWN);
    if (hadBoat) FlagSet(FLAG_HIDE_MR_BRINEY_BOAT_DEWFORD_TOWN);
}

TEST("SS Tidal exit needs each ROM's matching LAND state")
{
    static const struct { u8 route; u16 land; u16 destination; u8 heal; } sCases[] = {
        {COOP_GROUP_TRAVEL_ROUTE_SS_TIDAL_LILYCOVE_EXIT,
            SS_TIDAL_LAND_LILYCOVE, MAP_LILYCOVE_CITY_HARBOR, HEAL_LOCATION_LILYCOVE_CITY},
        {COOP_GROUP_TRAVEL_ROUTE_SS_TIDAL_SLATEPORT_EXIT,
            SS_TIDAL_LAND_SLATEPORT, MAP_SLATEPORT_CITY_HARBOR, HEAL_LOCATION_SLATEPORT_CITY},
    };
    u32 i;

    for (i = 0; i < ARRAY_COUNT(sCases); i++)
    {
        struct CoopGroupTravelRecord offer = Record(COOP_GROUP_TRAVEL_SERVER_OFFER,
            sCases[i].route, 300 + i, 9);
        struct CoopGroupTravelRecord request = Record(COOP_GROUP_TRAVEL_CLIENT_REQUEST,
            sCases[i].route, 310 + i, 0);
        struct CoopGroupTravelRecord commit = Record(COOP_GROUP_TRAVEL_SERVER_COMMIT,
            sCases[i].route, 310 + i, 9);

        ResetGroupTravelFixture();
        CoopGroupTravel_TestSetGrouped(TRUE);
        CoopGroupTravel_TestSetSafe(TRUE);
        MaterializeDeparture(MAP_SS_TIDAL_CORRIDOR);
        VarSet(VAR_SS_TIDAL_STATE, SS_TIDAL_DEPART_LILYCOVE);
        ScriptContext_SetupScript(EventScript_CoopGroupTravelOffer);
        EXPECT_EQ(CoopGroupTravel_BeginFromScript(sCases[i].route,
            COOP_GROUP_TRAVEL_DEPARTURE_FERRY), COOP_GROUP_TRAVEL_BEGIN_REJECTED);
        EndDepartureScript();
        CoopGroupTravel_OnSessionReady();
        EXPECT(!CoopGroupTravel_ReceiveServer(&offer));

        VarSet(VAR_SS_TIDAL_STATE, sCases[i].land);
        EXPECT(CoopGroupTravel_ReceiveServer(&offer));
        CoopGroupTravel_Poll();
        EXPECT_EQ(CoopGroupTravel_GetOffer(NULL), COOP_GROUP_TRAVEL_OFFER_READY);
        EXPECT(CoopGroupTravel_RespondToOffer(TRUE));
        EXPECT_EQ(CoopGroupTravel_TestRecord()->result, COOP_GROUP_TRAVEL_RESULT_ACCEPTED);

        CoopGroupTravel_Init();
        CoopGroupTravel_TestSetGrouped(TRUE);
        CoopGroupTravel_TestSetSafe(TRUE);
        MaterializeDeparture(MAP_SS_TIDAL_CORRIDOR);
        VarSet(VAR_SS_TIDAL_STATE, sCases[i].land);
        CoopGroupTravel_TestSeedRequest(&request);
        EXPECT(CoopGroupTravel_ReceiveServer(&commit));
        EXPECT_EQ(GetHealLocationIndexByWarpData(&gSaveBlock1Ptr->lastHealLocation),
            sCases[i].heal);
        EXPECT_EQ(JohtoTravel_GetPendingDestination(), JOHTO_TRAVEL_DESTINATION_NONE);
        MaterializeDeparture(sCases[i].destination);
        gSaveBlock1Ptr->pos.x = 8;
        gSaveBlock1Ptr->pos.y = 11;
        EXPECT(CoopGroupTravel_TestAtExactDestination(sCases[i].route));
        gSaveBlock1Ptr->pos.x++;
        EXPECT(!CoopGroupTravel_TestAtExactDestination(sCases[i].route));
    }
    CoopGroupTravel_Init();
    UnlockPlayerFieldControls();
    DiscardSimulatedWarp();
}

TEST("Reverse ferry requires the exact Kanto terminal and excludes maiden voyage")
{
    ResetGroupTravelFixture();
    CoopGroupTravel_TestSetGrouped(TRUE);
    CoopGroupTravel_TestSetSafe(TRUE);
    MaterializeDeparture(MAP_KANTO_LATER_VERMILION_CITY_PORT_INSIDE);
    ScriptContext_SetupScript(EventScript_CoopGroupTravelOffer);
    EXPECT_EQ(CoopGroupTravel_BeginFromScript(COOP_GROUP_TRAVEL_ROUTE_RETURN_FERRY_ORIGINAL,
        COOP_GROUP_TRAVEL_DEPARTURE_FERRY), COOP_GROUP_TRAVEL_BEGIN_REJECTED);
    EXPECT_EQ(CoopGroupTravel_BeginFromScript(COOP_GROUP_TRAVEL_ROUTE_RETURN_FERRY_LATER,
        COOP_GROUP_TRAVEL_DEPARTURE_SSAQUA_MAIDEN), COOP_GROUP_TRAVEL_BEGIN_REJECTED);
    EndDepartureScript();
}

TEST("Reverse gate commit completes at the Kanto-context reception gate")
{
    u8 route;

    for (route = COOP_GROUP_TRAVEL_ROUTE_RETURN_GATE_ORIGINAL;
         route <= COOP_GROUP_TRAVEL_ROUTE_RETURN_GATE_LATER; route++)
    {
        struct CoopGroupTravelRecord commit = Record(COOP_GROUP_TRAVEL_SERVER_COMMIT,
            route, route, 17);

        ResetGroupTravelFixture();
        CoopGroupTravel_TestSetGrouped(TRUE);
        CoopGroupTravel_TestSetSafe(TRUE);
        MaterializeDeparture(MAP_ROUTE22);
        EXPECT(JohtoTravel_SetPendingDestination(JOHTO_TRAVEL_DESTINATION_JOHTO));
        EXPECT(!JohtoTravel_CommitAtReceptionGate());
        MaterializeDeparture(MAP_RECEPTION_GATE);
        gMapHeader.regionMapSectionId = MAPSEC_KANTO_VICTORY_ROAD;
        gSaveBlock1Ptr->pos.x = 18;
        gSaveBlock1Ptr->pos.y = 9;
        EXPECT_EQ(JohtoTravel_GetCurrentContext(), JOHTO_TRAVEL_CONTEXT_KANTO_ORIGINAL);
        CoopGroupTravel_TestSeedCommitting(&commit);
        CoopGroupTravel_Poll();
        EXPECT_EQ(JohtoTravel_GetPendingDestination(), JOHTO_TRAVEL_DESTINATION_NONE);
        EXPECT_EQ(GetHealLocationIndexByWarpData(&gSaveBlock1Ptr->lastHealLocation),
                  HEAL_LOCATION_JOHTO_NEW_BARK_TOWN);
        EXPECT(CoopGroupTravel_TestState() == 8 || CoopGroupTravel_TestState() == 9);
    }
}

TEST("Group Fly route IDs follow the vanilla Fly table across regions")
{
    EXPECT_EQ(CoopRegionMap_GroupFlyMapSection(7), MAPSEC_LITTLEROOT_TOWN);
    EXPECT_EQ(CoopRegionMap_GroupFlyMapSection(8), MAPSEC_NEW_BARK_TOWN);
    EXPECT_EQ(CoopRegionMap_GroupFlyMapSection(17), MAPSEC_JOHTO_BLACKTHORN_CITY);
    EXPECT_EQ(CoopRegionMap_GroupFlyMapSection(18), MAPSEC_OLDALE_TOWN);
    EXPECT_EQ(CoopRegionMap_GroupFlyMapSection(31), MAPSEC_SOOTOPOLIS_CITY);
    EXPECT_EQ(CoopRegionMap_GroupFlyMapSection(32), MAPSEC_NONE);
    EXPECT_EQ(CoopRegionMap_GroupFlyMapSection(33), MAPSEC_PALLET_TOWN);
    EXPECT_EQ(CoopRegionMap_GroupFlyMapSection(54), MAPSEC_ONE_ISLAND);
    EXPECT_EQ(CoopRegionMap_GroupFlyMapSection(61), MAPSEC_ROUTE_4_POKECENTER);
    EXPECT_EQ(CoopRegionMap_GroupFlyMapSection(62), MAPSEC_ROUTE_10_POKECENTER);
    EXPECT_EQ(CoopRegionMap_GroupFlyMapSection(63), MAPSEC_EVER_GRANDE_CITY);
    EXPECT_EQ(CoopRegionMap_GroupFlyMapSection(64), MAPSEC_EVER_GRANDE_CITY);
    EXPECT_EQ(CoopRegionMap_GroupFlyMapSection(65), MAPSEC_BATTLE_FRONTIER);
}

TEST("Group travel offer carries a bounded server vote countdown")
{
    struct CoopGroupTravelRecord offer = Record(COOP_GROUP_TRAVEL_SERVER_OFFER,
        COOP_GROUP_TRAVEL_ROUTE_TRAIN_ORIGINAL, 7, 1);
    offer.remaining_seconds = 29;
    EXPECT(CoopGroupTravelProtocol_ValidateServer(&offer));
    EXPECT(!CoopGroupTravelProtocol_ValidateClient(&offer));
    offer.remaining_seconds = 31;
    EXPECT(!CoopGroupTravelProtocol_ValidateServer(&offer));
    offer.remaining_seconds = 0;
    EXPECT(CoopGroupTravelProtocol_ValidateServer(&offer));
}

TEST("Group Fly requires the vanilla Littleroot visit on both ROMs")
{
    struct CoopGroupTravelRecord offer = Record(COOP_GROUP_TRAVEL_SERVER_OFFER,
        COOP_GROUP_TRAVEL_ROUTE_FLY_LITTLEROOT, 35, 7);
    bool8 wasVisited = FlagGet(FLAG_VISITED_LITTLEROOT_TOWN);

    ResetGroupTravelFixture();
    CoopGroupTravel_TestSetGrouped(TRUE);
    CoopGroupTravel_TestSetSafe(TRUE);
    MaterializeDeparture(MAP_ROUTE104);
    FlagClear(FLAG_VISITED_LITTLEROOT_TOWN);
    EXPECT_EQ(CoopGroupTravel_BeginFly(COOP_GROUP_TRAVEL_ROUTE_FLY_LITTLEROOT),
              COOP_GROUP_TRAVEL_BEGIN_REJECTED);
    CoopGroupTravel_OnSessionReady();
    EXPECT(CoopGroupTravel_ReceiveServer(&offer));
    CoopGroupTravel_Poll();
    EXPECT_EQ(CoopGroupTravel_GetOffer(NULL), COOP_GROUP_TRAVEL_OFFER_READY);
    EXPECT(CoopGroupTravel_RespondToOffer(TRUE));
    EXPECT_EQ(CoopGroupTravel_TestRecord()->result, COOP_GROUP_TRAVEL_RESULT_DECLINED);

    CoopGroupTravel_Init();
    if (wasVisited)
        FlagSet(FLAG_VISITED_LITTLEROOT_TOWN);
    UnlockPlayerFieldControls();
}

TEST("Group Fly vote restores its field lock after the map returns")
{
    bool8 wasVisited = FlagGet(FLAG_VISITED_LITTLEROOT_TOWN);

    ResetGroupTravelFixture();
    CoopGroupTravel_TestSetGrouped(TRUE);
    CoopGroupTravel_TestSetSafe(TRUE);
    CoopGroupTravel_OnSessionReady();
    MaterializeDeparture(MAP_ROUTE104);
    FlagSet(FLAG_VISITED_LITTLEROOT_TOWN);
    EXPECT_EQ(CoopGroupTravel_BeginFly(COOP_GROUP_TRAVEL_ROUTE_FLY_LITTLEROOT),
              COOP_GROUP_TRAVEL_BEGIN_WAITING);
    EXPECT(ArePlayerFieldControlsLocked());

    /* Returning from the destination map unlocks the field after BeginFly.
     * An already-queued request must still reacquire the vote lock. */
    UnlockPlayerFieldControls();
    EXPECT(!ArePlayerFieldControlsLocked());
    CoopGroupTravel_Poll();
    EXPECT(ArePlayerFieldControlsLocked());

    CoopGroupTravel_Init();
    UnlockPlayerFieldControls();
    if (!wasVisited)
        FlagClear(FLAG_VISITED_LITTLEROOT_TOWN);
}

TEST("Group Teleport uses the exact last heal point and waits for consent")
{
    const struct HealLocation *heal = GetHealLocation(HEAL_LOCATION_JOHTO_NEW_BARK_TOWN);
    struct CoopGroupTravelRecord offer = Record(COOP_GROUP_TRAVEL_SERVER_OFFER, 8, 41, 6);

    ResetGroupTravelFixture();
    CoopGroupTravel_TestSetGrouped(TRUE);
    CoopGroupTravel_TestSetSafe(TRUE);
    CoopGroupTravel_OnSessionReady();
    MaterializeDeparture(MAP_ROUTE104);
    gSaveBlock1Ptr->lastHealLocation.mapGroup = heal->mapGroup;
    gSaveBlock1Ptr->lastHealLocation.mapNum = heal->mapNum;
    gSaveBlock1Ptr->lastHealLocation.warpId = WARP_ID_NONE;
    gSaveBlock1Ptr->lastHealLocation.x = heal->x;
    gSaveBlock1Ptr->lastHealLocation.y = heal->y;
    EXPECT(CoopGroupTravel_CanTeleport());
    EXPECT_EQ(CoopGroupTravel_BeginTeleport(), COOP_GROUP_TRAVEL_BEGIN_WAITING);
    EXPECT_EQ(CoopGroupTravel_TestRecord()->route, 8);
    EXPECT_EQ(CoopGroupTravel_TestRecord()->departure, COOP_GROUP_TRAVEL_DEPARTURE_TELEPORT);
    EXPECT(CoopGroupTravelProtocol_ValidateClient(CoopGroupTravel_TestRecord()));
    EXPECT(ArePlayerFieldControlsLocked());

    CoopGroupTravel_Init();
    UnlockPlayerFieldControls();
    CoopGroupTravel_TestSetGrouped(TRUE);
    offer.departure = COOP_GROUP_TRAVEL_DEPARTURE_TELEPORT;
    EXPECT(CoopGroupTravelProtocol_ValidateServer(&offer));
    gSaveBlock1Ptr->lastHealLocation.x++;
    EXPECT(!CoopGroupTravel_CanTeleport());
    EXPECT_EQ(CoopGroupTravel_BeginTeleport(), COOP_GROUP_TRAVEL_BEGIN_REJECTED);
    CoopGroupTravel_Init();
}

static struct CoopGroupTravelRecord EscapeRecord(u8 kind, u8 route, u32 request)
{
    struct CoopGroupTravelRecord record = {0};
    record.kind = kind;
    record.route = route;
    record.departure = route == COOP_GROUP_TRAVEL_ROUTE_DIG
        ? COOP_GROUP_TRAVEL_DEPARTURE_DIG
        : COOP_GROUP_TRAVEL_DEPARTURE_ESCAPE_ROPE;
    record.era = MAP_GROUP(MAP_GRANITE_CAVE_1F);
    record.destination = MAP_NUM(MAP_GRANITE_CAVE_1F);
    record.reserved0 = MAP_GROUP(MAP_ROUTE106);
    record.reserved1[0] = MAP_NUM(MAP_ROUTE106);
    record.reserved1[1] = 48;
    record.reserved1[2] = 17;
    record.request_id = request;
    memset(record.proposal_id, 7, sizeof(record.proposal_id));
    return record;
}

TEST("Group Dig encodes a fixed escape endpoint and rejects changes")
{
    struct CoopGroupTravelRecord reply;
    bool8 oldAllowEscaping = gMapHeader.allowEscaping;

    ResetGroupTravelFixture();
    CoopGroupTravel_TestSetGrouped(TRUE);
    CoopGroupTravel_TestSetSafe(TRUE);
    CoopGroupTravel_OnSessionReady();
    MaterializeDeparture(MAP_GRANITE_CAVE_1F);
    gMapHeader.allowEscaping = TRUE;
    SetEscapeWarp(MAP_GROUP(MAP_ROUTE106), MAP_NUM(MAP_ROUTE106),
                  WARP_ID_NONE, 48, 17);
    EXPECT(CoopGroupTravel_CanEscape());
    EXPECT_EQ(CoopGroupTravel_BeginDig(), COOP_GROUP_TRAVEL_BEGIN_WAITING);
    EXPECT(CoopGroupTravelProtocol_ValidateClient(CoopGroupTravel_TestRecord()));
    EXPECT_EQ(CoopGroupTravel_TestRecord()->era, MAP_GROUP(MAP_GRANITE_CAVE_1F));
    EXPECT_EQ(CoopGroupTravel_TestRecord()->reserved1[2], 17);
    reply = *CoopGroupTravel_TestRecord();
    reply.kind = COOP_GROUP_TRAVEL_SERVER_REQUESTING;
    reply.reserved1[2] = 18;
    EXPECT(!CoopGroupTravel_ReceiveServer(&reply));
    reply.reserved1[2] = 17;
    EXPECT(CoopGroupTravel_ReceiveServer(&reply));
    gMapHeader.allowEscaping = oldAllowEscaping;
    CoopGroupTravel_Init();
    UnlockPlayerFieldControls();
}

TEST("Group Dig offer refuses an escape endpoint changed before consent")
{
    struct CoopGroupTravelRecord offer = EscapeRecord(
        COOP_GROUP_TRAVEL_SERVER_OFFER, COOP_GROUP_TRAVEL_ROUTE_DIG, 42);
    bool8 oldAllowEscaping = gMapHeader.allowEscaping;

    ResetGroupTravelFixture();
    CoopGroupTravel_TestSetGrouped(TRUE);
    CoopGroupTravel_TestSetSafe(TRUE);
    CoopGroupTravel_OnSessionReady();
    MaterializeDeparture(MAP_GRANITE_CAVE_1F);
    gMapHeader.allowEscaping = TRUE;
    SetEscapeWarp(MAP_GROUP(MAP_ROUTE106), MAP_NUM(MAP_ROUTE106),
                  WARP_ID_NONE, 48, 17);
    EXPECT(CoopGroupTravelProtocol_ValidateServer(&offer));
    EXPECT(CoopGroupTravel_ReceiveServer(&offer));
    CoopGroupTravel_Poll();
    SetEscapeWarp(MAP_GROUP(MAP_ROUTE106), MAP_NUM(MAP_ROUTE106),
                  WARP_ID_NONE, 48, 18);
    EXPECT(CoopGroupTravel_RespondToOffer(TRUE));
    EXPECT_EQ(CoopGroupTravel_TestRecord()->result,
              COOP_GROUP_TRAVEL_RESULT_DECLINED);
    gMapHeader.allowEscaping = oldAllowEscaping;
    CoopGroupTravel_Init();
    UnlockPlayerFieldControls();
}

TEST("Group Dig commit stages the agreed warp and replays only at its arrival")
{
    struct CoopGroupTravelRecord commit;
    bool8 oldAllowEscaping = gMapHeader.allowEscaping;

    ResetGroupTravelFixture();
    CoopGroupTravel_TestSetGrouped(TRUE);
    CoopGroupTravel_TestSetSafe(TRUE);
    CoopGroupTravel_OnSessionReady();
    MaterializeDeparture(MAP_GRANITE_CAVE_1F);
    gMapHeader.allowEscaping = TRUE;
    SetEscapeWarp(MAP_GROUP(MAP_ROUTE106), MAP_NUM(MAP_ROUTE106),
                  WARP_ID_NONE, 48, 17);
    EXPECT_EQ(CoopGroupTravel_BeginDig(), COOP_GROUP_TRAVEL_BEGIN_WAITING);
    commit = *CoopGroupTravel_TestRecord();
    commit.kind = COOP_GROUP_TRAVEL_SERVER_COMMIT;
    memset(commit.proposal_id, 9, sizeof(commit.proposal_id));
    EXPECT(CoopGroupTravel_ReceiveServer(&commit));
    DiscardSimulatedWarp();

    CoopGroupTravel_Init();
    CoopGroupTravel_TestSetGrouped(TRUE);
    CoopGroupTravel_TestSetSafe(TRUE);
    CoopGroupTravel_OnSessionReady();
    MaterializeDeparture(MAP_ROUTE106);
    gSaveBlock1Ptr->pos.x = 48;
    gSaveBlock1Ptr->pos.y = 18;
    EXPECT(!CoopGroupTravel_ReceiveServer(&commit));
    gSaveBlock1Ptr->pos.y = 17;
    EXPECT(CoopGroupTravel_ReceiveServer(&commit));
    EXPECT_EQ(CoopGroupTravel_TestRecord()->kind, COOP_GROUP_TRAVEL_CLIENT_APPLIED);
    gMapHeader.allowEscaping = oldAllowEscaping;
    CoopGroupTravel_Init();
    UnlockPlayerFieldControls();
}

TEST("Group Escape Rope validates its source and exact arrival")
{
    struct CoopGroupTravelRecord record = EscapeRecord(
        COOP_GROUP_TRAVEL_CLIENT_REQUEST, COOP_GROUP_TRAVEL_ROUTE_ESCAPE_ROPE, 43);

    memset(record.proposal_id, 0, sizeof(record.proposal_id));
    EXPECT(CoopGroupTravelProtocol_ValidateClient(&record));
    record.kind = COOP_GROUP_TRAVEL_SERVER_OFFER;
    memset(record.proposal_id, 7, sizeof(record.proposal_id));
    EXPECT(CoopGroupTravelProtocol_ValidateServer(&record));
    record.reserved1[1] = 128;
    EXPECT(!CoopGroupTravelProtocol_ValidateServer(&record));
    record.reserved1[1] = 48;
    record.reserved0 = 255;
    EXPECT(!CoopGroupTravelProtocol_ValidateServer(&record));
    record.reserved0 = MAP_GROUP(MAP_ROUTE106);
    record.era = 255;
    EXPECT(!CoopGroupTravelProtocol_ValidateServer(&record));
}

TEST("Group travel refreshes offer countdown from server updates")
{
    struct CoopGroupTravelRecord offer = Record(COOP_GROUP_TRAVEL_SERVER_OFFER,
        COOP_GROUP_TRAVEL_ROUTE_TRAIN_ORIGINAL, 34, 5);
    struct CoopGroupTravelRecord visible;
    ResetGroupTravelFixture();
    CoopGroupTravel_TestSetGrouped(TRUE);
    MaterializeDeparture(MAP_GOLDENROD_CITY_TRAIN_STATION);
    CoopGroupTravel_OnSessionReady();
    offer.remaining_seconds = 26;
    EXPECT(CoopGroupTravel_ReceiveServer(&offer));
    EXPECT_EQ(CoopGroupTravel_GetOffer(&visible), COOP_GROUP_TRAVEL_OFFER_DEFERRED);
    EXPECT_EQ(visible.remaining_seconds, 26);
    offer.remaining_seconds = 13;
    EXPECT(CoopGroupTravel_ReceiveServer(&offer));
    EXPECT_EQ(CoopGroupTravel_GetOffer(&visible), COOP_GROUP_TRAVEL_OFFER_DEFERRED);
    EXPECT_EQ(visible.remaining_seconds, 13);
}

TEST("Group travel requires exact arrival coordinates for all consent routes")
{
    ResetGroupTravelFixture();
    static const struct
    {
        u16 map;
        s16 x;
        s16 y;
    } sExpected[] = {
        {0},
        {MAP_KANTO_ORIGINAL_SAFFRON_CITY_TRAIN_STATION, 140, 16},
        {MAP_KANTO_LATER_SAFFRON_CITY_TRAIN_STATION, 140, 16},
        {MAP_KANTO_ORIGINAL_VERMILION_CITY_PORT_INSIDE, 8, 9},
        {MAP_KANTO_LATER_VERMILION_CITY_PORT_INSIDE, 8, 9},
        {MAP_ROUTE22, 9, 12},
        {MAP_KANTO_LATER_ROUTE22, 13, 10},
        {MAP_LITTLEROOT_TOWN, 5, 6},
    };
    u8 route;

    CoopGroupTravel_Init();
    for (route = COOP_GROUP_TRAVEL_ROUTE_TRAIN_ORIGINAL;
         route <= COOP_GROUP_TRAVEL_ROUTE_FLY_LITTLEROOT;
         route++)
    {
        gSaveBlock1Ptr->location.mapGroup = MAP_GROUP(sExpected[route].map);
        gSaveBlock1Ptr->location.mapNum = MAP_NUM(sExpected[route].map);
        gSaveBlock1Ptr->pos.x = sExpected[route].x;
        gSaveBlock1Ptr->pos.y = sExpected[route].y;
        EXPECT(CoopGroupTravel_TestAtExactDestination(route));
        gSaveBlock1Ptr->pos.x++;
        EXPECT(!CoopGroupTravel_TestAtExactDestination(route));
    }
}

TEST("Group travel defers unsafe offers and abort clears them")
{
    ResetGroupTravelFixture();
    struct CoopGroupTravelRecord offer = Record(COOP_GROUP_TRAVEL_SERVER_OFFER,
        COOP_GROUP_TRAVEL_ROUTE_FERRY_LATER, 77, 9);
    struct CoopGroupTravelRecord abort = offer;
    abort.kind = COOP_GROUP_TRAVEL_SERVER_ABORT;
    abort.reason = COOP_GROUP_TRAVEL_REASON_PARTICIPANT_DECLINED;
    CoopGroupTravel_Init();
    CoopGroupTravel_TestSetGrouped(TRUE);
    MaterializeDeparture(MAP_OLIVINE_CITY_PORT_INSIDE);
    CoopGroupTravel_OnSessionReady();
    CoopGroupTravel_TestSetSafe(FALSE);
    EXPECT(CoopGroupTravel_ReceiveServer(&offer));
    EXPECT(CoopGroupTravel_ReceiveServer(&offer));
    CoopGroupTravel_Poll();
    EXPECT_EQ(CoopGroupTravel_GetOffer(NULL), COOP_GROUP_TRAVEL_OFFER_DEFERRED);
    EXPECT(CoopGroupTravel_ReceiveServer(&abort));
    EXPECT_EQ(CoopGroupTravel_GetOffer(NULL), COOP_GROUP_TRAVEL_OFFER_NONE);
}

TEST("Group travel accepts a second live offer without a new session-ready frame")
{
    ResetGroupTravelFixture();
    struct CoopGroupTravelRecord first = Record(COOP_GROUP_TRAVEL_SERVER_OFFER,
        COOP_GROUP_TRAVEL_ROUTE_FERRY_LATER, 78, 9);
    struct CoopGroupTravelRecord abort = first;
    struct CoopGroupTravelRecord second = Record(COOP_GROUP_TRAVEL_SERVER_OFFER,
        COOP_GROUP_TRAVEL_ROUTE_FERRY_ORIGINAL, 79, 10);

    abort.kind = COOP_GROUP_TRAVEL_SERVER_ABORT;
    abort.reason = COOP_GROUP_TRAVEL_REASON_PARTICIPANT_DECLINED;
    CoopGroupTravel_TestSetGrouped(TRUE);
    CoopGroupTravel_TestSetSafe(TRUE);
    MaterializeDeparture(MAP_OLIVINE_CITY_PORT_INSIDE);
    CoopGroupTravel_OnSessionReady();

    EXPECT(CoopGroupTravel_ReceiveServer(&first));
    EXPECT(CoopGroupTravel_ReceiveServer(&abort));
    EXPECT_EQ(CoopGroupTravel_TestState(), 0);
    EXPECT(CoopGroupTravel_ReceiveServer(&second));
    EXPECT_EQ(CoopGroupTravel_GetOffer(NULL), COOP_GROUP_TRAVEL_OFFER_DEFERRED);
}

TEST("Group travel pre-create abort permits only fixed failure reasons")
{
    ResetGroupTravelFixture();
    struct CoopGroupTravelRecord abort = Record(COOP_GROUP_TRAVEL_SERVER_ABORT,
        COOP_GROUP_TRAVEL_ROUTE_GATE_ORIGINAL, 5, 0);
    abort.reason = COOP_GROUP_TRAVEL_REASON_CONFLICT;
    EXPECT(CoopGroupTravelProtocol_ValidateServer(&abort));
    abort.reason = COOP_GROUP_TRAVEL_REASON_PARTICIPANT_DECLINED;
    EXPECT(!CoopGroupTravelProtocol_ValidateServer(&abort));
}

TEST("Group travel retries decisions and applied events after queue saturation")
{
    ResetGroupTravelFixture();
    struct CoopGroupTravelRecord offer = Record(COOP_GROUP_TRAVEL_SERVER_OFFER,
        COOP_GROUP_TRAVEL_ROUTE_FERRY_LATER, 88, 4);
    struct CoopGroupTravelRecord abort = offer;
    struct CoopGroupTravelRecord commit = offer;

    abort.kind = COOP_GROUP_TRAVEL_SERVER_ABORT;
    abort.reason = COOP_GROUP_TRAVEL_REASON_PARTICIPANT_DECLINED;
    commit.kind = COOP_GROUP_TRAVEL_SERVER_COMMIT;
    CoopGroupTravel_Init();
    CoopGroupTravel_TestSetGrouped(TRUE);
    MaterializeDeparture(MAP_OLIVINE_CITY_PORT_INSIDE);
    CoopGroupTravel_OnSessionReady();
    CoopGroupTravel_TestSetSafe(TRUE);
    gCoopNetBridge.game_to_network.read_index = 0;
    gCoopNetBridge.game_to_network.write_index = COOP_NET_BRIDGE_QUEUE_CAPACITY;

    EXPECT(CoopGroupTravel_ReceiveServer(&offer));
    CoopGroupTravel_Poll();
    EXPECT_EQ(CoopGroupTravel_GetOffer(NULL), COOP_GROUP_TRAVEL_OFFER_READY);
    EXPECT(CoopGroupTravel_RespondToOffer(FALSE));
    EXPECT(!CoopGroupTravel_TestSemanticQueued());
    ScriptContext_Stop();
    gCoopNetBridge.game_to_network.read_index++;
    CoopGroupTravel_Poll();
    EXPECT(CoopGroupTravel_TestSemanticQueued());
    EXPECT(CoopGroupTravel_ReceiveServer(&abort));
    CoopGroupTravel_Poll();

    gCoopNetBridge.game_to_network.read_index = 0;
    gCoopNetBridge.game_to_network.write_index = COOP_NET_BRIDGE_QUEUE_CAPACITY;
    CoopGroupTravel_TestSeedAppliedPending(&commit);
    gSaveBlock1Ptr->location.mapGroup = MAP_GROUP(MAP_KANTO_LATER_VERMILION_CITY_PORT_INSIDE);
    gSaveBlock1Ptr->location.mapNum = MAP_NUM(MAP_KANTO_LATER_VERMILION_CITY_PORT_INSIDE);
    gSaveBlock1Ptr->pos.x = 8;
    gSaveBlock1Ptr->pos.y = 9;
    CoopGroupTravel_Poll();
    EXPECT(!CoopGroupTravel_TestSemanticQueued());
    gCoopNetBridge.game_to_network.read_index++;
    CoopGroupTravel_Poll();
    EXPECT(CoopGroupTravel_TestSemanticQueued());
    CoopGroupTravel_Init();
    UnlockPlayerFieldControls();
}

TEST("Group travel reconnect unlocks precommit waits and replays after ready")
{
    ResetGroupTravelFixture();
    struct CoopGroupTravelRecord request = Record(COOP_GROUP_TRAVEL_CLIENT_REQUEST,
        COOP_GROUP_TRAVEL_ROUTE_GATE_LATER, 91, 0);
    struct CoopGroupTravelRecord abort = request;

    abort.kind = COOP_GROUP_TRAVEL_SERVER_ABORT;
    abort.reason = COOP_GROUP_TRAVEL_REASON_CONFLICT;
    CoopGroupTravel_Init();
    CoopGroupTravel_TestSetSafe(TRUE);
    gCoopNetBridge.game_to_network.read_index = 0;
    gCoopNetBridge.game_to_network.write_index = 0;
    CoopGroupTravel_TestSeedRequest(&request);

    CoopGroupTravel_OnSessionReady();
    EXPECT(ArePlayerFieldControlsLocked());
    EXPECT(CoopGroupTravel_TestSemanticQueued());
    CoopGroupTravel_OnTransportLost();
    EXPECT(!ArePlayerFieldControlsLocked());
    EXPECT(!CoopGroupTravel_TestSemanticQueued());
    CoopGroupTravel_OnSessionReady();
    EXPECT(ArePlayerFieldControlsLocked());
    EXPECT(CoopGroupTravel_TestSemanticQueued());
    EXPECT(CoopGroupTravel_ReceiveServer(&abort));
    CoopGroupTravel_Poll();
    EXPECT(!ArePlayerFieldControlsLocked());
}

TEST("Group travel complete cannot release a script-owned interaction boundary")
{
    ResetGroupTravelFixture();
    struct CoopGroupTravelRecord commit = Record(COOP_GROUP_TRAVEL_SERVER_COMMIT,
        COOP_GROUP_TRAVEL_ROUTE_TRAIN_LATER, 92, 6);
    struct CoopGroupTravelRecord complete = commit;

    complete.kind = COOP_GROUP_TRAVEL_SERVER_COMPLETE;
    complete.result = COOP_GROUP_TRAVEL_RESULT_APPLIED;
    CoopGroupTravel_Init();
    CoopGroupTravel_TestSetSafe(TRUE);
    gCoopNetBridge.game_to_network.read_index = 0;
    gCoopNetBridge.game_to_network.write_index = 0;
    CoopGroupTravel_TestSeedAppliedPending(&commit);
    gSaveBlock1Ptr->location.mapGroup = MAP_GROUP(MAP_KANTO_LATER_SAFFRON_CITY_TRAIN_STATION);
    gSaveBlock1Ptr->location.mapNum = MAP_NUM(MAP_KANTO_LATER_SAFFRON_CITY_TRAIN_STATION);
    gSaveBlock1Ptr->pos.x = 140;
    gSaveBlock1Ptr->pos.y = 16;
    CoopGroupTravel_Poll();
    EXPECT(ArePlayerFieldControlsLocked());
    ScriptContext_SetupScript(EventScript_CoopGroupTravelOffer);

    EXPECT(CoopGroupTravel_ReceiveServer(&complete));
    EXPECT(ArePlayerFieldControlsLocked());
    CoopGroupTravel_Poll();
    EXPECT(ArePlayerFieldControlsLocked());
    ScriptContext_Stop();
    CoopGroupTravel_Poll();
    EXPECT(!ArePlayerFieldControlsLocked());
}

TEST("Group travel disconnected arrival locks before replay and rejects drift")
{
    ResetGroupTravelFixture();
    const struct HealLocation *heal;
    struct CoopGroupTravelRecord commit = Record(COOP_GROUP_TRAVEL_SERVER_COMMIT,
        COOP_GROUP_TRAVEL_ROUTE_GATE_LATER, 93, 7);

    JohtoSave_InitializeCurrent();
    gSaveBlock1Ptr->location.mapGroup = MAP_GROUP(MAP_NEW_BARK_TOWN);
    gSaveBlock1Ptr->location.mapNum = MAP_NUM(MAP_NEW_BARK_TOWN);
    gMapHeader.regionMapSectionId = MAPSEC_NEW_BARK_TOWN;
    heal = GetHealLocation(HEAL_LOCATION_JOHTO_CHERRYGROVE_CITY);
    gSaveBlock1Ptr->lastHealLocation.mapGroup = heal->mapGroup;
    gSaveBlock1Ptr->lastHealLocation.mapNum = heal->mapNum;
    gSaveBlock1Ptr->lastHealLocation.warpId = WARP_ID_NONE;
    gSaveBlock1Ptr->lastHealLocation.x = heal->x;
    gSaveBlock1Ptr->lastHealLocation.y = heal->y;
    EXPECT(JohtoTravel_RecordCurrentHeal(HEAL_LOCATION_JOHTO_CHERRYGROVE_CITY));
    EXPECT(JohtoTravel_SetPendingDestination(JOHTO_TRAVEL_DESTINATION_KANTO_LATER));
    EXPECT(JohtoTravel_PrepareCrossing());

    CoopGroupTravel_Init();
    CoopGroupTravel_TestSetSafe(TRUE);
    CoopGroupTravel_TestSeedCommitting(&commit);
    CoopGroupTravel_OnTransportLost();
    gSaveBlock1Ptr->location.mapGroup = MAP_GROUP(MAP_KANTO_LATER_ROUTE22);
    gSaveBlock1Ptr->location.mapNum = MAP_NUM(MAP_KANTO_LATER_ROUTE22);
    gSaveBlock1Ptr->pos.x = 13;
    gSaveBlock1Ptr->pos.y = 10;
    gMapHeader.regionMapSectionId = MAPSEC_ROUTE_22;
    gCoopNetBridge.game_to_network.read_index = 0;
    gCoopNetBridge.game_to_network.write_index = 0;

    CoopGroupTravel_Poll();
    EXPECT(ArePlayerFieldControlsLocked());
    EXPECT(!CoopGroupTravel_TestSemanticQueued());

    /* A forced location mutation models movement or a script warp attempting
     * to race reconnect after the local arrival commit. */
    gSaveBlock1Ptr->pos.x = 14;
    CoopGroupTravel_OnSessionReady();
    CoopGroupTravel_Poll();
    EXPECT(ArePlayerFieldControlsLocked());
    EXPECT(!CoopGroupTravel_TestSemanticQueued());

    gSaveBlock1Ptr->pos.x = 13;
    CoopGroupTravel_Poll();
    EXPECT(ArePlayerFieldControlsLocked());
    EXPECT(CoopGroupTravel_TestSemanticQueued());
    UnlockPlayerFieldControls();
    CoopGroupTravel_Init();
}

TEST("Group Fly arrival acknowledges without a Johto crossing")
{
    struct CoopGroupTravelRecord commit = Record(COOP_GROUP_TRAVEL_SERVER_COMMIT,
        COOP_GROUP_TRAVEL_ROUTE_FLY_LITTLEROOT, 94, 8);

    ResetGroupTravelFixture();
    CoopGroupTravel_TestSetSafe(TRUE);
    CoopGroupTravel_TestSeedCommitting(&commit);
    MaterializeDeparture(MAP_LITTLEROOT_TOWN);
    gSaveBlock1Ptr->pos.x = 5;
    gSaveBlock1Ptr->pos.y = 6;
    EXPECT_EQ(JohtoTravel_GetPendingDestination(), JOHTO_TRAVEL_DESTINATION_NONE);
    CoopGroupTravel_Poll();
    EXPECT(CoopGroupTravel_TestState() == 8 || CoopGroupTravel_TestState() == 9);
    EXPECT_EQ(CoopGroupTravel_TestRecord()->kind, COOP_GROUP_TRAVEL_CLIENT_APPLIED);
    UnlockPlayerFieldControls();
    CoopGroupTravel_Init();
}

TEST("Seagallop arrival acknowledges without a Johto crossing")
{
    struct CoopGroupTravelRecord commit = Record(COOP_GROUP_TRAVEL_SERVER_COMMIT,
        COOP_GROUP_TRAVEL_ROUTE_SEAGALLOP_FIRST, 350, 8);

    ResetGroupTravelFixture();
    CoopGroupTravel_TestSetSafe(TRUE);
    CoopGroupTravel_TestSeedCommitting(&commit);
    MaterializeDeparture(MAP_ONE_ISLAND_HARBOR);
    gSaveBlock1Ptr->pos.x = 8;
    gSaveBlock1Ptr->pos.y = 5;
    EXPECT_EQ(JohtoTravel_GetPendingDestination(), JOHTO_TRAVEL_DESTINATION_NONE);
    CoopGroupTravel_Poll();
    EXPECT(CoopGroupTravel_TestState() == 8 || CoopGroupTravel_TestState() == 9);
    EXPECT_EQ(CoopGroupTravel_TestRecord()->kind, COOP_GROUP_TRAVEL_CLIENT_APPLIED);
    CoopGroupTravel_OnTransportLost();
    EXPECT(!CoopGroupTravel_TestSemanticQueued());
    CoopGroupTravel_OnSessionReady();
    CoopGroupTravel_Poll();
    EXPECT(CoopGroupTravel_TestSemanticQueued());
    UnlockPlayerFieldControls();
    CoopGroupTravel_Init();
}

TEST("Map script commit cannot consume a group-managed arrival")
{
    ResetGroupTravelFixture();
    const struct HealLocation *heal;
    JohtoSave_InitializeCurrent();
    gSaveBlock1Ptr->location.mapGroup = MAP_GROUP(MAP_NEW_BARK_TOWN);
    gSaveBlock1Ptr->location.mapNum = MAP_NUM(MAP_NEW_BARK_TOWN);
    gMapHeader.regionMapSectionId = MAPSEC_NEW_BARK_TOWN;
    heal = GetHealLocation(HEAL_LOCATION_JOHTO_CHERRYGROVE_CITY);
    gSaveBlock1Ptr->lastHealLocation.mapGroup = heal->mapGroup;
    gSaveBlock1Ptr->lastHealLocation.mapNum = heal->mapNum;
    gSaveBlock1Ptr->lastHealLocation.warpId = WARP_ID_NONE;
    gSaveBlock1Ptr->lastHealLocation.x = heal->x;
    gSaveBlock1Ptr->lastHealLocation.y = heal->y;
    EXPECT(JohtoTravel_RecordCurrentHeal(HEAL_LOCATION_JOHTO_CHERRYGROVE_CITY));
    EXPECT(JohtoTravel_SetPendingDestination(JOHTO_TRAVEL_DESTINATION_KANTO_LATER));
    EXPECT(JohtoTravel_PrepareCrossing());
    gSaveBlock1Ptr->location.mapGroup = MAP_GROUP(MAP_KANTO_LATER_PALLET_TOWN);
    gSaveBlock1Ptr->location.mapNum = MAP_NUM(MAP_KANTO_LATER_PALLET_TOWN);
    gMapHeader.regionMapSectionId = MAPSEC_PALLET_TOWN;

    CoopGroupTravel_TestSetManagingArrival(TRUE);
    Johto_CommitKantoTravel();
    EXPECT(gSpecialVar_Result);
    EXPECT_EQ(JohtoTravel_GetPendingDestination(), JOHTO_TRAVEL_DESTINATION_KANTO_LATER);

    CoopGroupTravel_TestSetManagingArrival(FALSE);
    Johto_CommitKantoTravel();
    EXPECT(gSpecialVar_Result);
    EXPECT_EQ(JohtoTravel_GetPendingDestination(), JOHTO_TRAVEL_DESTINATION_NONE);
}
