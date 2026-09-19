#include "global.h"
#include "coop/group_travel.h"
#include "coop/net_bridge.h"
#include "event_data.h"
#include "event_object_movement.h"
#include "event_object_lock.h"
#include "heal_location.h"
#include "johto/kanto_travel.h"
#include "johto/save.h"
#include "script.h"
#include "task.h"
#include "constants/heal_locations.h"
#include "constants/johto_content.h"
#include "constants/maps.h"
#include "constants/region_map_sections.h"
#include "test/test.h"

extern void Johto_CommitKantoTravel(void);
extern const u8 EventScript_CoopGroupTravelOffer[];

static struct CoopGroupTravelRecord Record(u8 kind, u8 route, u32 request, u8 proposal)
{
    static const u8 sEra[] = {0, 1, 2, 1, 2, 1, 2};
    static const u8 sDestination[] = {0, 3, 4, 1, 2, 5, 6};
    struct CoopGroupTravelRecord record = {0};
    record.kind = kind; record.route = route; record.era = sEra[route];
    record.departure = route <= COOP_GROUP_TRAVEL_ROUTE_TRAIN_LATER
        ? COOP_GROUP_TRAVEL_DEPARTURE_TRAIN
        : route <= COOP_GROUP_TRAVEL_ROUTE_FERRY_LATER
            ? COOP_GROUP_TRAVEL_DEPARTURE_FERRY
            : COOP_GROUP_TRAVEL_DEPARTURE_GATE;
    record.destination = sDestination[route]; record.request_id = request;
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
    for (route = 1; route <= 6; route++)
    {
        record = Record(COOP_GROUP_TRAVEL_CLIENT_REQUEST, route, route, 0);
        EXPECT(CoopGroupTravelProtocol_ValidateClient(&record));
        /* Requesting and client-request records intentionally share their
         * scalar wire shape; bridge direction supplies the distinction. */
        EXPECT(CoopGroupTravelProtocol_ValidateServer(&record));
        record.reserved1[3] = 1;
        EXPECT(!CoopGroupTravelProtocol_ValidateClient(&record));
    }
    record = Record(COOP_GROUP_TRAVEL_CLIENT_REQUEST,
        COOP_GROUP_TRAVEL_ROUTE_FERRY_LATER, 7, 0);
    record.departure = COOP_GROUP_TRAVEL_DEPARTURE_SSAQUA_MAIDEN;
    EXPECT(CoopGroupTravelProtocol_ValidateClient(&record));
    record.route = COOP_GROUP_TRAVEL_ROUTE_TRAIN_ORIGINAL;
    EXPECT(!CoopGroupTravelProtocol_ValidateClient(&record));
}

TEST("Group travel requires exact arrival coordinates for all six routes")
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
    };
    u8 route;

    CoopGroupTravel_Init();
    for (route = COOP_GROUP_TRAVEL_ROUTE_TRAIN_ORIGINAL;
         route <= COOP_GROUP_TRAVEL_ROUTE_GATE_LATER;
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
