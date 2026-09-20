#include "global.h"
#include "coop/group_travel.h"
#include "coop/net_bridge.h"
#include "event_data.h"
#include "event_object_lock.h"
#include "field_screen_effect.h"
#include "heal_location.h"
#include "johto/kanto_travel.h"
#include "main.h"
#include "overworld.h"
#include "palette.h"
#include "script.h"
#include "constants/johto_content.h"
#include "constants/maps.h"

enum RuntimeState
{
    STATE_IDLE,
    STATE_REQUESTING,
    STATE_OFFER_DEFERRED,
    STATE_OFFER_READY,
    STATE_ACCEPTED,
    STATE_CANCELING,
    STATE_DECLINING,
    STATE_COMMITTING,
    STATE_APPLIED_PENDING,
    STATE_APPLIED,
};

struct CoopGroupTravelRuntime
{
    struct CoopGroupTravelRecord semantic;
    u32 next_request_id;
    u8 state;
    u8 departure;
    bool8 controls_locked;
    bool8 script_lock_handoff;
    bool8 script_objects_frozen;
    bool8 offer_ui_started;
    bool8 semantic_queued;
    bool8 suspended;
    bool8 session_ready;
    /* Only records replayed after an authenticated session-ready boundary
     * may create state from a cold runtime. */
    bool8 recovery_armed;
    bool8 recovered_state;
    bool8 commit_pending;
    bool8 clear_pending;
    bool8 clear_preserve_travel;
#if TESTING
    bool8 test_safe;
    bool8 test_safe_set;
    bool8 test_grouped;
    bool8 test_grouped_set;
#endif
};

static EWRAM_DATA struct CoopGroupTravelRuntime sTravel = {0};
extern const u8 EventScript_CoopGroupTravelOffer[];

static bool8 IsZero(const u8 *bytes, u32 size) { while (size--) if (*bytes++) return FALSE; return TRUE; }
static bool8 RouteFields(u8 route, u8 *era, u8 *destination)
{
    static const u8 sEra[] = {0, 1, 2, 1, 2, 1, 2};
    static const u8 sDestination[] = {0, 3, 4, 1, 2, 5, 6};
    if (route < 1 || route > 6) return FALSE;
    *era = sEra[route]; *destination = sDestination[route]; return TRUE;
}

static bool8 IsGrouped(void)
{
#if TESTING
    if (sTravel.test_grouped_set)
        return sTravel.test_grouped;
#endif
    return CoopNetBridge_IsGrouped();
}

static bool8 RouteMatchesDeparture(u8 route, u8 departure)
{
    switch (departure)
    {
    case COOP_GROUP_TRAVEL_DEPARTURE_TRAIN:
        return route == COOP_GROUP_TRAVEL_ROUTE_TRAIN_ORIGINAL
            || route == COOP_GROUP_TRAVEL_ROUTE_TRAIN_LATER;
    case COOP_GROUP_TRAVEL_DEPARTURE_FERRY:
    case COOP_GROUP_TRAVEL_DEPARTURE_SSAQUA_MAIDEN:
        return route == COOP_GROUP_TRAVEL_ROUTE_FERRY_ORIGINAL
            || route == COOP_GROUP_TRAVEL_ROUTE_FERRY_LATER;
    case COOP_GROUP_TRAVEL_DEPARTURE_GATE:
        return route == COOP_GROUP_TRAVEL_ROUTE_GATE_ORIGINAL
            || route == COOP_GROUP_TRAVEL_ROUTE_GATE_LATER;
    default:
        return FALSE;
    }
}

static bool8 IsMaterializedDeparture(u8 route, u8 departure)
{
    u16 map;

    if (!RouteMatchesDeparture(route, departure) || gSaveBlock1Ptr == NULL)
        return FALSE;
    switch (departure)
    {
    case COOP_GROUP_TRAVEL_DEPARTURE_TRAIN:
        map = MAP_GOLDENROD_CITY_TRAIN_STATION;
        break;
    case COOP_GROUP_TRAVEL_DEPARTURE_FERRY:
    case COOP_GROUP_TRAVEL_DEPARTURE_SSAQUA_MAIDEN:
        map = MAP_OLIVINE_CITY_PORT_INSIDE;
        break;
    case COOP_GROUP_TRAVEL_DEPARTURE_GATE:
        map = MAP_RECEPTION_GATE;
        break;
    default:
        return FALSE;
    }
    return gSaveBlock1Ptr->location.mapGroup == MAP_GROUP(map)
        && gSaveBlock1Ptr->location.mapNum == MAP_NUM(map);
}

static bool8 ValidateCommon(const struct CoopGroupTravelRecord *record)
{
    u8 era, destination;
    return record != NULL && record->request_id != 0
        && RouteFields(record->route, &era, &destination)
        && record->era == era && record->destination == destination
        && RouteMatchesDeparture(record->route, record->departure)
        && record->result <= COOP_GROUP_TRAVEL_RESULT_APPLIED
        && record->reason <= COOP_GROUP_TRAVEL_REASON_UNSAFE
        && record->reserved0 == 0
        && IsZero(record->reserved1, sizeof(record->reserved1));
}

bool8 CoopGroupTravelProtocol_ValidateClient(const struct CoopGroupTravelRecord *record)
{
    bool8 zero;
    if (!ValidateCommon(record)) return FALSE;
    zero = IsZero(record->proposal_id, sizeof(record->proposal_id));
    switch (record->kind)
    {
    case COOP_GROUP_TRAVEL_CLIENT_REQUEST: return zero && record->result == 0 && record->reason == 0;
    case COOP_GROUP_TRAVEL_CLIENT_DECISION: return !zero && (record->result == 1 || record->result == 2) && record->reason == 0;
    case COOP_GROUP_TRAVEL_CLIENT_CANCEL: return record->result == 0 && record->reason == COOP_GROUP_TRAVEL_REASON_REQUESTER_CANCELED;
    case COOP_GROUP_TRAVEL_CLIENT_APPLIED: return !zero && record->result == COOP_GROUP_TRAVEL_RESULT_APPLIED && record->reason == 0;
    default: return FALSE;
    }
}

bool8 CoopGroupTravelProtocol_ValidateServer(const struct CoopGroupTravelRecord *record)
{
    bool8 zero;
    if (!ValidateCommon(record)) return FALSE;
    zero = IsZero(record->proposal_id, sizeof(record->proposal_id));
    switch (record->kind)
    {
    case COOP_GROUP_TRAVEL_SERVER_REQUESTING: return zero && record->result == 0 && record->reason == 0;
    case COOP_GROUP_TRAVEL_SERVER_OFFER:
    case COOP_GROUP_TRAVEL_SERVER_COMMIT: return !zero && record->result == 0 && record->reason == 0;
    case COOP_GROUP_TRAVEL_SERVER_ABORT:
        return record->result == 0 && record->reason != 0
            && (!zero || record->reason == COOP_GROUP_TRAVEL_REASON_CONFLICT || record->reason == COOP_GROUP_TRAVEL_REASON_UNSAFE);
    case COOP_GROUP_TRAVEL_SERVER_COMPLETE: return !zero && record->result == COOP_GROUP_TRAVEL_RESULT_APPLIED && record->reason == 0;
    default: return FALSE;
    }
}

static bool8 SendSemantic(void)
{
    return CoopGroupTravelProtocol_ValidateClient(&sTravel.semantic)
        && CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_GROUP_TRAVEL_CLIENT,
                                               &sTravel.semantic, sizeof(sTravel.semantic));
}

static void SetRoute(struct CoopGroupTravelRecord *record, u8 route)
{
    memset(record, 0, sizeof(*record));
    record->route = route;
    (void)RouteFields(route, &record->era, &record->destination);
}

static bool8 IsSafeOverworld(void)
{
#if TESTING
    if (sTravel.test_safe_set) return sTravel.test_safe;
#endif
    return gMain.callback1 == CB1_Overworld && gMain.callback2 == CB2_Overworld
        && !gPaletteFade.active && !ArePlayerFieldControlsLocked()
        && !ScriptContext_IsEnabled();
}

static bool8 CanOwnControlLock(void)
{
#if TESTING
    if (sTravel.test_safe_set)
        return sTravel.test_safe;
#endif
    return gMain.callback1 == CB1_Overworld && gMain.callback2 == CB2_Overworld
        && !gPaletteFade.active && !ScriptContext_IsEnabled();
}

static bool8 AcquireControlLock(void)
{
    if (sTravel.controls_locked)
        return ArePlayerFieldControlsLocked();
    if (!CanOwnControlLock() || ArePlayerFieldControlsLocked())
        return FALSE;
    LockPlayerFieldControls();
    sTravel.controls_locked = TRUE;
    return TRUE;
}

static bool8 ReleaseOwnedControlLock(void)
{
    if (!sTravel.controls_locked)
        return TRUE;
    if (!ArePlayerFieldControlsLocked())
    {
        sTravel.controls_locked = FALSE;
        return TRUE;
    }
    if (!CanOwnControlLock())
        return FALSE;
    UnlockPlayerFieldControls();
    sTravel.controls_locked = FALSE;
    return TRUE;
}

static bool8 ReleaseTravelBoundary(void)
{
    if (sTravel.script_objects_frozen)
    {
        if (ScriptContext_IsEnabled())
            return FALSE;
        ScriptUnfreezeObjectEvents();
        sTravel.script_objects_frozen = FALSE;
        sTravel.script_lock_handoff = FALSE;
    }
    return ReleaseOwnedControlLock();
}

static void ClearAndUnlock(bool8 preservePendingTravel)
{
    if (!preservePendingTravel)
        (void)JohtoTravel_Cancel();
    if (!ReleaseTravelBoundary())
    {
        sTravel.clear_pending = TRUE;
        sTravel.clear_preserve_travel = preservePendingTravel;
        return;
    }
    memset(&sTravel.semantic, 0, sizeof(sTravel.semantic));
    sTravel.state = STATE_IDLE;
    sTravel.departure = COOP_GROUP_TRAVEL_DEPARTURE_NONE;
    sTravel.offer_ui_started = FALSE;
    sTravel.semantic_queued = FALSE;
    sTravel.suspended = FALSE;
    sTravel.recovery_armed = FALSE;
    sTravel.recovered_state = FALSE;
    sTravel.commit_pending = FALSE;
    sTravel.clear_pending = FALSE;
    sTravel.clear_preserve_travel = FALSE;
    sTravel.script_lock_handoff = FALSE;
    sTravel.script_objects_frozen = FALSE;
}

void CoopGroupTravel_Init(void)
{
    memset(&sTravel, 0, sizeof(sTravel));
    sTravel.next_request_id = 1;
}

enum CoopGroupTravelBeginResult CoopGroupTravel_Begin(u8 route)
{
    if (sTravel.state != STATE_IDLE || route < 1 || route > 6)
        return COOP_GROUP_TRAVEL_BEGIN_REJECTED;
    if (!IsGrouped())
        return COOP_GROUP_TRAVEL_BEGIN_NOT_GROUPED;
    if (!IsSafeOverworld())
        return COOP_GROUP_TRAVEL_BEGIN_REJECTED;
    SetRoute(&sTravel.semantic, route);
    if (route == COOP_GROUP_TRAVEL_ROUTE_FERRY_ORIGINAL
     || route == COOP_GROUP_TRAVEL_ROUTE_FERRY_LATER)
        return COOP_GROUP_TRAVEL_BEGIN_REJECTED;
    sTravel.semantic.departure = route <= COOP_GROUP_TRAVEL_ROUTE_TRAIN_LATER
        ? COOP_GROUP_TRAVEL_DEPARTURE_TRAIN : COOP_GROUP_TRAVEL_DEPARTURE_GATE;
    sTravel.departure = sTravel.semantic.departure;
    sTravel.recovered_state = FALSE;
    sTravel.semantic.kind = COOP_GROUP_TRAVEL_CLIENT_REQUEST;
    sTravel.semantic.request_id = sTravel.next_request_id++;
    if (sTravel.next_request_id == 0) sTravel.next_request_id = 1;
    if (!SendSemantic()) { memset(&sTravel.semantic, 0, sizeof(sTravel.semantic)); return COOP_GROUP_TRAVEL_BEGIN_REJECTED; }
    LockPlayerFieldControls();
    sTravel.controls_locked = TRUE;
    sTravel.state = STATE_REQUESTING;
    sTravel.semantic_queued = TRUE;
    return COOP_GROUP_TRAVEL_BEGIN_WAITING;
}

enum CoopGroupTravelBeginResult CoopGroupTravel_BeginFromScript(u8 route, u8 departure)
{
    if (sTravel.state != STATE_IDLE || !IsMaterializedDeparture(route, departure)
     || !ScriptContext_IsEnabled() || !ArePlayerFieldControlsLocked())
        return COOP_GROUP_TRAVEL_BEGIN_REJECTED;
    if (!IsGrouped())
        return COOP_GROUP_TRAVEL_BEGIN_NOT_GROUPED;
    SetRoute(&sTravel.semantic, route);
    sTravel.semantic.departure = departure;
    sTravel.recovered_state = FALSE;
    sTravel.semantic.kind = COOP_GROUP_TRAVEL_CLIENT_REQUEST;
    sTravel.semantic.request_id = sTravel.next_request_id++;
    if (sTravel.next_request_id == 0)
        sTravel.next_request_id = 1;
    if (!SendSemantic())
    {
        memset(&sTravel.semantic, 0, sizeof(sTravel.semantic));
        return COOP_GROUP_TRAVEL_BEGIN_REJECTED;
    }
    sTravel.state = STATE_REQUESTING;
    sTravel.departure = departure;
    sTravel.semantic_queued = TRUE;
    sTravel.script_lock_handoff = TRUE;
    sTravel.script_objects_frozen = TRUE;
    return COOP_GROUP_TRAVEL_BEGIN_WAITING;
}

bool8 CoopGroupTravel_Cancel(void)
{
    if (sTravel.state == STATE_IDLE || sTravel.state == STATE_COMMITTING
     || sTravel.state == STATE_APPLIED_PENDING || sTravel.state == STATE_APPLIED)
        return FALSE;
    sTravel.semantic.kind = COOP_GROUP_TRAVEL_CLIENT_CANCEL;
    sTravel.semantic.result = COOP_GROUP_TRAVEL_RESULT_NONE;
    sTravel.semantic.reason = COOP_GROUP_TRAVEL_REASON_REQUESTER_CANCELED;
    sTravel.state = STATE_CANCELING;
    sTravel.semantic_queued = SendSemantic();
    (void)ReleaseTravelBoundary();
    return TRUE;
}

enum CoopGroupTravelOfferState CoopGroupTravel_GetOffer(struct CoopGroupTravelRecord *offer)
{
    if (sTravel.state != STATE_OFFER_DEFERRED && sTravel.state != STATE_OFFER_READY)
        return COOP_GROUP_TRAVEL_OFFER_NONE;
    if (offer != NULL) *offer = sTravel.semantic;
    return sTravel.state == STATE_OFFER_READY ? COOP_GROUP_TRAVEL_OFFER_READY : COOP_GROUP_TRAVEL_OFFER_DEFERRED;
}

bool8 CoopGroupTravel_RespondToOffer(bool8 accept)
{
    if (sTravel.state != STATE_OFFER_READY) return FALSE;
    sTravel.semantic.kind = COOP_GROUP_TRAVEL_CLIENT_DECISION;
    sTravel.semantic.result = accept ? COOP_GROUP_TRAVEL_RESULT_ACCEPTED : COOP_GROUP_TRAVEL_RESULT_DECLINED;
    sTravel.semantic.reason = COOP_GROUP_TRAVEL_REASON_NONE;
    sTravel.semantic_queued = SendSemantic();
    if (accept)
        sTravel.state = STATE_ACCEPTED;
    else
    {
        sTravel.state = STATE_DECLINING;
        (void)ReleaseTravelBoundary();
    }
    return TRUE;
}

struct TravelWarp
{
    u16 map;
    s8 warp;
    s8 warp_x;
    s8 warp_y;
    s16 arrival_x;
    s16 arrival_y;
};
static const struct TravelWarp sTravelWarps[] = {
    {0},
    /* Warp 1 is the adapter's exact (140,16) arrival and avoids narrowing
     * the large imported coordinate through the engine's signed-s8 API. */
    {MAP_KANTO_ORIGINAL_SAFFRON_CITY_TRAIN_STATION, 1, -1, -1, 140, 16},
    {MAP_KANTO_LATER_SAFFRON_CITY_TRAIN_STATION, 1, -1, -1, 140, 16},
    {MAP_KANTO_ORIGINAL_VERMILION_CITY_PORT_INSIDE, WARP_ID_NONE, 8, 9, 8, 9},
    {MAP_KANTO_LATER_VERMILION_CITY_PORT_INSIDE, WARP_ID_NONE, 8, 9, 8, 9},
    {MAP_ROUTE22, WARP_ID_NONE, 9, 12, 9, 12},
    {MAP_KANTO_LATER_ROUTE22, WARP_ID_NONE, 13, 10, 13, 10},
};

static bool8 StageCommit(void)
{
    const struct TravelWarp *warp = &sTravelWarps[sTravel.semantic.route];
    u32 heal = GetHealLocationIndexByWarpData(&gSaveBlock1Ptr->lastHealLocation);
    enum JohtoTravelDestination target = sTravel.semantic.era == COOP_GROUP_TRAVEL_ERA_ORIGINAL
        ? JOHTO_TRAVEL_DESTINATION_KANTO_ORIGINAL : JOHTO_TRAVEL_DESTINATION_KANTO_LATER;
    if (heal == 0 || !JohtoTravel_RecordCurrentHeal(heal)
        || !JohtoTravel_SetPendingDestination(target) || !JohtoTravel_PrepareCrossing())
        return FALSE;
    if (sTravel.departure == COOP_GROUP_TRAVEL_DEPARTURE_SSAQUA_MAIDEN)
    {
        VarSet(JOHTO_VAR_SSAQUA_STATE, 1);
        FlagClear(JOHTO_FLAG_HIDE_SSAQUA_1F_GRANDPA);
        FlagSet(JOHTO_FLAG_HIDE_SSAQUA_ROOM_SSE_GRANDDAUGHTER);
        FlagClear(JOHTO_FLAG_HIDE_SSAQUA_SAILOR);
        FlagClear(JOHTO_FLAG_HIDE_SSAQUA_CAPTAINS_ROOM_GRANDDAUGHTER);
        SetWarpDestination(MAP_GROUP(MAP_SSAQUA_1F), MAP_NUM(MAP_SSAQUA_1F),
                           WARP_ID_NONE, 29, 3);
    }
    else
    {
        SetWarpDestination(MAP_GROUP(warp->map), MAP_NUM(warp->map), warp->warp,
                           warp->warp_x, warp->warp_y);
    }
    DoWarp();
    /* Map loading owns and resets the engine's global field lock.  Forget our
     * pre-warp ownership and reacquire it after the destination is fully safe. */
    sTravel.controls_locked = FALSE;
    return TRUE;
}

static bool8 AtRouteDestination(u8 route)
{
    const struct TravelWarp *warp;

    if (route < COOP_GROUP_TRAVEL_ROUTE_TRAIN_ORIGINAL
     || route > COOP_GROUP_TRAVEL_ROUTE_GATE_LATER)
        return FALSE;
    warp = &sTravelWarps[route];
    return gSaveBlock1Ptr != NULL
        && gSaveBlock1Ptr->location.mapGroup == MAP_GROUP(warp->map)
        && gSaveBlock1Ptr->location.mapNum == MAP_NUM(warp->map)
        && gSaveBlock1Ptr->pos.x == warp->arrival_x
        && gSaveBlock1Ptr->pos.y == warp->arrival_y;
}

static bool8 AtExactDestination(void)
{
    return AtRouteDestination(sTravel.semantic.route);
}

static enum JohtoTravelDestination DestinationForRecord(const struct CoopGroupTravelRecord *record)
{
    return record->era == COOP_GROUP_TRAVEL_ERA_ORIGINAL
        ? JOHTO_TRAVEL_DESTINATION_KANTO_ORIGINAL
        : JOHTO_TRAVEL_DESTINATION_KANTO_LATER;
}

static bool8 IsMaidenInProgress(const struct CoopGroupTravelRecord *record)
{
    return (record->route == COOP_GROUP_TRAVEL_ROUTE_FERRY_ORIGINAL
         || record->route == COOP_GROUP_TRAVEL_ROUTE_FERRY_LATER)
        && record->departure == COOP_GROUP_TRAVEL_DEPARTURE_SSAQUA_MAIDEN
        && gSaveBlock1Ptr != NULL
        && gSaveBlock1Ptr->location.mapGroup == MAP_GROUP(MAP_SSAQUA_1F)
        && gSaveBlock1Ptr->location.mapNum == MAP_NUM(MAP_SSAQUA_1F)
        && gSaveBlock1Ptr->pos.x == 29
        && gSaveBlock1Ptr->pos.y == 3
        && VarGet(JOHTO_VAR_SSAQUA_STATE) == 1
        && JohtoTravel_GetPendingDestination() == DestinationForRecord(record);
}

static bool8 IsRecoverableDestination(const struct CoopGroupTravelRecord *record)
{
    enum JohtoTravelDestination pending;

    if (!IsGrouped())
        return FALSE;
    if (IsMaidenInProgress(record))
        return TRUE;
    if (!AtRouteDestination(record->route))
        return FALSE;
    pending = JohtoTravel_GetPendingDestination();
    return pending == JOHTO_TRAVEL_DESTINATION_NONE
        || pending == DestinationForRecord(record);
}

static bool8 IsCurrentTravelSource(const struct CoopGroupTravelRecord *record)
{
    return IsGrouped() && IsMaterializedDeparture(record->route, record->departure);
}

static bool8 TryStagePendingCommit(void)
{
    bool8 safe = sTravel.controls_locked ? CanOwnControlLock() : IsSafeOverworld();
    if (!sTravel.commit_pending || sTravel.suspended || !safe)
        return TRUE;
    if (!StageCommit())
    {
        ClearAndUnlock(FALSE);
        return FALSE;
    }
    sTravel.commit_pending = FALSE;
    return TRUE;
}

bool8 CoopGroupTravel_ReceiveServer(const struct CoopGroupTravelRecord *record)
{
    bool8 at_destination;

    if (!CoopGroupTravelProtocol_ValidateServer(record)) return FALSE;
    if (sTravel.state == STATE_IDLE)
    {
        /* The bridge calls OnSessionReady before replaying server state. A
         * valid wire record outside that authenticated window is unsolicited
         * and must not create a travel session from cold state. */
        if (!sTravel.session_ready ||
            (!IsCurrentTravelSource(record) &&
             (record->kind != COOP_GROUP_TRAVEL_SERVER_COMMIT
              || !IsRecoverableDestination(record))))
            return FALSE;
        if (record->kind == COOP_GROUP_TRAVEL_SERVER_ABORT
         || record->kind == COOP_GROUP_TRAVEL_SERVER_COMPLETE)
            return FALSE;
        sTravel.recovery_armed = FALSE;
        if (record->kind == COOP_GROUP_TRAVEL_SERVER_OFFER)
        {
            sTravel.semantic = *record;
            sTravel.departure = record->departure;
            sTravel.recovered_state = TRUE;
            sTravel.state = STATE_OFFER_DEFERRED;
            return TRUE;
        }
        if (record->kind == COOP_GROUP_TRAVEL_SERVER_REQUESTING)
        {
            sTravel.semantic = *record;
            sTravel.semantic.kind = COOP_GROUP_TRAVEL_CLIENT_REQUEST;
            sTravel.departure = record->departure;
            sTravel.recovered_state = TRUE;
            sTravel.state = STATE_REQUESTING;
            sTravel.semantic_queued = FALSE;
            sTravel.suspended = FALSE;
            return TRUE;
        }
        /* A replayed commit is accepted only while the local participant is
         * still at the exact departure terminal. DoWarp is deferred until
         * Poll reaches a safe field callback, which covers map/script reloads
         * during launcher recovery. */
        if (record->kind == COOP_GROUP_TRAVEL_SERVER_COMMIT)
        {
            at_destination = AtRouteDestination(record->route);
            sTravel.semantic = *record;
            sTravel.departure = record->departure;
            sTravel.recovered_state = TRUE;
            sTravel.state = STATE_COMMITTING;
            sTravel.commit_pending = !at_destination && !IsMaidenInProgress(record);
            sTravel.semantic_queued = FALSE;
            sTravel.suspended = FALSE;
            if (at_destination && JohtoTravel_GetPendingDestination()
                                  == JOHTO_TRAVEL_DESTINATION_NONE)
            {
                sTravel.semantic.kind = COOP_GROUP_TRAVEL_CLIENT_APPLIED;
                sTravel.semantic.result = COOP_GROUP_TRAVEL_RESULT_APPLIED;
                sTravel.semantic.reason = COOP_GROUP_TRAVEL_REASON_NONE;
                sTravel.state = STATE_APPLIED_PENDING;
            }
            return TryStagePendingCommit();
        }
        return FALSE;
    }
    if (record->kind == COOP_GROUP_TRAVEL_SERVER_OFFER)
    {
        if (sTravel.state == STATE_OFFER_DEFERRED || sTravel.state == STATE_OFFER_READY
         || sTravel.state == STATE_ACCEPTED || sTravel.state == STATE_DECLINING
         || sTravel.state == STATE_CANCELING)
            return record->request_id == sTravel.semantic.request_id
                && record->route == sTravel.semantic.route
                && record->departure == sTravel.semantic.departure
                && memcmp(record->proposal_id, sTravel.semantic.proposal_id, 16) == 0;
        return FALSE;
    }
    if (record->kind == COOP_GROUP_TRAVEL_SERVER_ABORT)
    {
        if (sTravel.state != STATE_IDLE
         && record->request_id == sTravel.semantic.request_id
         && record->route == sTravel.semantic.route
         && record->departure == sTravel.semantic.departure)
            ClearAndUnlock(FALSE);
        return TRUE;
    }
    if (record->request_id != sTravel.semantic.request_id
        || record->route != sTravel.semantic.route
        || record->departure != sTravel.semantic.departure)
        return FALSE;
    if (record->kind == COOP_GROUP_TRAVEL_SERVER_REQUESTING)
    {
        if (sTravel.state != STATE_REQUESTING) return FALSE;
        sTravel.semantic = *record;
        sTravel.semantic.kind = COOP_GROUP_TRAVEL_CLIENT_REQUEST;
        return TRUE;
    }
    if (record->kind == COOP_GROUP_TRAVEL_SERVER_COMMIT)
    {
        if (sTravel.state == STATE_APPLIED)
        {
            sTravel.semantic_queued = FALSE;
            return TRUE;
        }
        if (sTravel.state == STATE_COMMITTING || sTravel.state == STATE_APPLIED_PENDING) return TRUE;
        if (sTravel.state != STATE_REQUESTING && sTravel.state != STATE_ACCEPTED) return FALSE;
        if (sTravel.state == STATE_ACCEPTED
            && memcmp(record->proposal_id, sTravel.semantic.proposal_id, 16) != 0)
            return FALSE;
        sTravel.semantic = *record;
        sTravel.departure = record->departure;
        sTravel.state = STATE_COMMITTING;
        sTravel.commit_pending = TRUE;
        sTravel.semantic_queued = FALSE;
        sTravel.suspended = FALSE;
        if (!sTravel.recovered_state)
        {
            if (!StageCommit())
            {
                ClearAndUnlock(FALSE);
                return FALSE;
            }
            sTravel.commit_pending = FALSE;
            return TRUE;
        }
        return TryStagePendingCommit();
    }
    if (record->kind == COOP_GROUP_TRAVEL_SERVER_COMPLETE
     && sTravel.state == STATE_APPLIED
     && record->request_id == sTravel.semantic.request_id
     && record->route == sTravel.semantic.route
     && record->departure == sTravel.semantic.departure
     && memcmp(record->proposal_id, sTravel.semantic.proposal_id, 16) == 0)
    {
        ClearAndUnlock(sTravel.departure == COOP_GROUP_TRAVEL_DEPARTURE_SSAQUA_MAIDEN);
        return TRUE;
    }
    return FALSE;
}

void CoopGroupTravel_Poll(void)
{
    if (sTravel.clear_pending)
    {
        (void)ReleaseTravelBoundary();
        if (!sTravel.controls_locked && !sTravel.script_objects_frozen)
            ClearAndUnlock(sTravel.clear_preserve_travel);
        return;
    }
    if (sTravel.state == STATE_COMMITTING && !TryStagePendingCommit())
        return;
    if (sTravel.script_lock_handoff)
    {
        if (ScriptContext_IsEnabled() || !AcquireControlLock())
            return;
        sTravel.script_lock_handoff = FALSE;
    }
    if (sTravel.state == STATE_OFFER_DEFERRED && IsSafeOverworld())
    {
        if (!AcquireControlLock())
            return;
        sTravel.state = STATE_OFFER_READY;
        sTravel.offer_ui_started = TRUE;
        ScriptContext_SetupScript(EventScript_CoopGroupTravelOffer);
    }
    if ((sTravel.state == STATE_REQUESTING || sTravel.state == STATE_ACCEPTED)
     && !sTravel.semantic_queued && !sTravel.suspended && AcquireControlLock())
        sTravel.semantic_queued = SendSemantic();
    if ((sTravel.state == STATE_CANCELING || sTravel.state == STATE_DECLINING)
     && !sTravel.semantic_queued && !sTravel.suspended)
        sTravel.semantic_queued = SendSemantic();
    if ((sTravel.state == STATE_CANCELING || sTravel.state == STATE_DECLINING
      || sTravel.suspended) && (sTravel.controls_locked || sTravel.script_objects_frozen))
        (void)ReleaseTravelBoundary();
    if (sTravel.state == STATE_COMMITTING && AtExactDestination()
     && JohtoTravel_TryCommitArrival())
    {
        sTravel.semantic.kind = COOP_GROUP_TRAVEL_CLIENT_APPLIED;
        sTravel.semantic.result = COOP_GROUP_TRAVEL_RESULT_APPLIED;
        sTravel.semantic.reason = COOP_GROUP_TRAVEL_REASON_NONE;
        sTravel.state = STATE_APPLIED_PENDING;
        sTravel.semantic_queued = FALSE;
    }
    if ((sTravel.state == STATE_APPLIED_PENDING || sTravel.state == STATE_APPLIED)
     && AtExactDestination())
    {
        /* Hold the player at the exact committed arrival even while the
         * transport is down.  Reconnect may replay only from this location. */
        if (AcquireControlLock() && !sTravel.suspended && !sTravel.semantic_queued)
        {
            sTravel.semantic_queued = SendSemantic();
            if (sTravel.semantic_queued)
                sTravel.state = STATE_APPLIED;
        }
    }
}

void CoopGroupTravel_OnSessionReady(void)
{
    sTravel.session_ready = TRUE;
    sTravel.suspended = FALSE;
    sTravel.semantic_queued = FALSE;
    sTravel.recovery_armed = sTravel.state == STATE_IDLE;
    /* Arrival replay is intentionally driven by Poll, which owns the final
     * destination check and control-lock acquisition. */
    if (sTravel.state == STATE_APPLIED_PENDING || sTravel.state == STATE_APPLIED)
        return;
    if ((sTravel.state == STATE_REQUESTING || sTravel.state == STATE_ACCEPTED)
     && !AcquireControlLock())
        return;
    if (sTravel.state != STATE_IDLE && CoopGroupTravelProtocol_ValidateClient(&sTravel.semantic))
    {
        sTravel.semantic_queued = SendSemantic();
        if (sTravel.state == STATE_APPLIED_PENDING && sTravel.semantic_queued)
            sTravel.state = STATE_APPLIED;
    }
}

void CoopGroupTravel_OnTransportLost(void)
{
    sTravel.session_ready = FALSE;
    if (sTravel.state == STATE_IDLE)
        return;
    sTravel.suspended = TRUE;
    sTravel.semantic_queued = FALSE;
    /* Precommit waits must never strand the player offline.  Arrival states
     * retain their world-transition safety and reacquire ownership in Poll. */
    if (sTravel.state != STATE_COMMITTING && sTravel.state != STATE_APPLIED_PENDING
     && sTravel.state != STATE_APPLIED)
        (void)ReleaseTravelBoundary();
}
bool8 CoopGroupTravel_IsManagingArrival(void) { return sTravel.state == STATE_COMMITTING; }

void Special_CoopGroupTravelBegin(void)
{
    u8 departure = gSpecialVar_0x8005;
    if (gSpecialVar_0x8004 == COOP_GROUP_TRAVEL_ROUTE_GATE_ORIGINAL
     || gSpecialVar_0x8004 == COOP_GROUP_TRAVEL_ROUTE_GATE_LATER)
        departure = COOP_GROUP_TRAVEL_DEPARTURE_GATE;
    gSpecialVar_Result = CoopGroupTravel_BeginFromScript(gSpecialVar_0x8004, departure);
}
void Special_CoopGroupTravelGetOffer(void)
{
    struct CoopGroupTravelRecord offer;

    gSpecialVar_0x8004 = 0;
    gSpecialVar_0x8005 = 0;
    gSpecialVar_Result = CoopGroupTravel_GetOffer(&offer);
    if (gSpecialVar_Result == COOP_GROUP_TRAVEL_OFFER_READY)
    {
        gSpecialVar_0x8004 = offer.route;
        gSpecialVar_0x8005 = offer.departure;
    }
}
void Special_CoopGroupTravelRespond(void) { gSpecialVar_Result = CoopGroupTravel_RespondToOffer(gSpecialVar_0x8004 != 0); }
void Special_CoopGroupTravelCancel(void) { gSpecialVar_Result = CoopGroupTravel_Cancel(); }

#if TESTING
void CoopGroupTravel_TestSetSafe(bool8 safe) { sTravel.test_safe_set = TRUE; sTravel.test_safe = safe; }
void CoopGroupTravel_TestSetManagingArrival(bool8 managing) { sTravel.state = managing ? STATE_COMMITTING : STATE_IDLE; }
u8 CoopGroupTravel_TestState(void) { return sTravel.state; }
void CoopGroupTravel_TestSeedRequest(const struct CoopGroupTravelRecord *record)
{
    sTravel.semantic = *record;
    sTravel.departure = record->departure;
    sTravel.state = STATE_REQUESTING;
    sTravel.semantic_queued = FALSE;
    sTravel.suspended = FALSE;
    sTravel.recovery_armed = FALSE;
    sTravel.recovered_state = FALSE;
    sTravel.commit_pending = FALSE;
}
void CoopGroupTravel_TestSeedAppliedPending(const struct CoopGroupTravelRecord *record)
{
    sTravel.semantic = *record;
    sTravel.semantic.kind = COOP_GROUP_TRAVEL_CLIENT_APPLIED;
    sTravel.semantic.result = COOP_GROUP_TRAVEL_RESULT_APPLIED;
    sTravel.semantic.reason = COOP_GROUP_TRAVEL_REASON_NONE;
    sTravel.state = STATE_APPLIED_PENDING;
    sTravel.semantic_queued = FALSE;
    sTravel.suspended = FALSE;
    sTravel.controls_locked = FALSE;
    sTravel.recovery_armed = FALSE;
    sTravel.recovered_state = FALSE;
    sTravel.commit_pending = FALSE;
}
void CoopGroupTravel_TestSeedCommitting(const struct CoopGroupTravelRecord *record)
{
    sTravel.semantic = *record;
    sTravel.state = STATE_COMMITTING;
    sTravel.semantic_queued = FALSE;
    sTravel.suspended = FALSE;
    sTravel.controls_locked = FALSE;
    sTravel.recovery_armed = FALSE;
    sTravel.recovered_state = FALSE;
    sTravel.commit_pending = FALSE;
}
bool8 CoopGroupTravel_TestSemanticQueued(void) { return sTravel.semantic_queued; }
bool8 CoopGroupTravel_TestAtExactDestination(u8 route)
{
    if (route < COOP_GROUP_TRAVEL_ROUTE_TRAIN_ORIGINAL
     || route > COOP_GROUP_TRAVEL_ROUTE_GATE_LATER)
        return FALSE;
    sTravel.semantic.route = route;
    return AtExactDestination();
}
void CoopGroupTravel_TestSetGrouped(bool8 grouped) { sTravel.test_grouped_set = TRUE; sTravel.test_grouped = grouped; }
void CoopGroupTravel_TestSetDeparture(u8 departure) { sTravel.departure = departure; }
const struct CoopGroupTravelRecord *CoopGroupTravel_TestRecord(void) { return &sTravel.semantic; }
#endif
