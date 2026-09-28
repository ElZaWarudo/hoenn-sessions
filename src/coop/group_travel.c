#include "global.h"
#include "coop/group_travel.h"
#include "coop/net_bridge.h"
#include "event_data.h"
#include "event_object_lock.h"
#include "field_screen_effect.h"
#include "heal_location.h"
#include "item.h"
#include "johto/kanto_travel.h"
#include "main.h"
#include "overworld.h"
#include "palette.h"
#include "region_map.h"
#include "script.h"
#include "string_util.h"
#include "text.h"
#include "window.h"
#include "constants/johto_content.h"
#include "constants/flags.h"
#include "constants/field_specials.h"
#include "constants/game_stat.h"
#include "constants/heal_locations.h"
#include "constants/maps.h"
#include "constants/seagallop.h"

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
    STATE_SCENE_MARKER_WAIT,
    STATE_SCENE_MARKER_ACKED,
    STATE_SCENE_COMPLETE_PENDING,
    STATE_SCENE_COMPLETE_SENT,
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
    u32 vote_deadline_frame;
    u8 vote_window;
    u8 vote_shown_seconds;
    bool8 vote_known;
    bool8 scene_landed;
    bool8 scene_started;
    bool8 last_story_complete_valid;
    struct CoopGroupTravelRecord last_story_complete;
#if TESTING
    bool8 test_safe;
    bool8 test_safe_set;
    bool8 test_grouped;
    bool8 test_grouped_set;
#endif
};

static EWRAM_DATA struct CoopGroupTravelRuntime sTravel = {0};
extern const u8 EventScript_CoopGroupTravelOffer[];
extern const u8 Route104_MrBrineysHouse_EventScript_GroupVoyageStart[];
extern const u8 CinnabarIsland_EventScript_GroupBillAcceptedVoyageStart[];
extern const u8 OneIsland_PokemonCenter_1F_EventScript_GroupBillAcceptedReturnStage[];
extern const u8 OneIsland_PokemonCenter_1F_EventScript_GroupBillReturnStart[];
static const struct WindowTemplate sVoteWindow = {
    .bg = 0, .tilemapLeft = 16, .tilemapTop = 1,
    .width = 13, .height = 2, .paletteNum = 15, .baseBlock = 0x150,
};
static const u8 sVoteTimeText[] = _("Vote: ");
static const u8 sVoteSecondsText[] = _("s left");
static const u8 sNextPortText[] = _("the next port");

static void HideVoteCountdown(void)
{
    if (sTravel.vote_window == WINDOW_NONE)
        return;
    ClearWindowTilemap(sTravel.vote_window);
    CopyWindowToVram(sTravel.vote_window, COPYWIN_MAP);
    RemoveWindow(sTravel.vote_window);
    sTravel.vote_window = WINDOW_NONE;
}

static void ShowVoteCountdown(void)
{
    u8 text[24];
    u8 *end;
    s32 frames;
    u8 remaining;

    if (sTravel.clear_pending || sTravel.suspended
     || (sTravel.state != STATE_OFFER_READY && sTravel.state != STATE_REQUESTING)
     || !sTravel.vote_known)
    {
        HideVoteCountdown();
        return;
    }
    /* Fly requests start while the region map owns the window system. Wait
     * until the field has restored its windows before drawing the vote. */
    if (sTravel.vote_window == WINDOW_NONE
     && (gMain.callback1 != CB1_Overworld || gMain.callback2 != CB2_Overworld))
        return;
    frames = (s32)(sTravel.vote_deadline_frame - gMain.vblankCounter1);
    remaining = frames > 0 ? (frames + 59) / 60 : 0;
    if (remaining == sTravel.vote_shown_seconds && sTravel.vote_window != WINDOW_NONE)
        return;
    if (sTravel.vote_window == WINDOW_NONE)
    {
        sTravel.vote_window = AddWindow(&sVoteWindow);
        if (sTravel.vote_window == WINDOW_NONE)
            return;
        PutWindowTilemap(sTravel.vote_window);
    }
    sTravel.vote_shown_seconds = remaining;
    FillWindowPixelBuffer(sTravel.vote_window, PIXEL_FILL(1));
    end = StringCopy(text, sVoteTimeText);
    end = ConvertIntToDecimalStringN(end, remaining, STR_CONV_MODE_LEFT_ALIGN, 2);
    StringCopy(end, sVoteSecondsText);
    AddTextPrinterParameterized(sTravel.vote_window, FONT_SMALL, text, 4, 0, TEXT_SKIP_DRAW, NULL);
    CopyWindowToVram(sTravel.vote_window, COPYWIN_FULL);
}

static bool8 IsZero(const u8 *bytes, u32 size) { while (size--) if (*bytes++) return FALSE; return TRUE; }
static bool8 IsOutboundIslandRoute(u8 route)
{
    return route >= COOP_GROUP_TRAVEL_ROUTE_OLIVINE_SOUTHERN_ISLAND
        && route <= COOP_GROUP_TRAVEL_ROUTE_VERMILION_BATTLE_FRONTIER;
}

static bool8 IsOlivineIslandRoute(u8 route)
{
    return route >= COOP_GROUP_TRAVEL_ROUTE_OLIVINE_SOUTHERN_ISLAND
        && route <= COOP_GROUP_TRAVEL_ROUTE_OLIVINE_BATTLE_FRONTIER;
}

static bool8 IsIslandReturnRoute(u8 route)
{
    return route >= COOP_GROUP_TRAVEL_ROUTE_SOUTHERN_ISLAND_LILYCOVE
        && route <= COOP_GROUP_TRAVEL_ROUTE_BATTLE_FRONTIER_LILYCOVE;
}

static bool8 IsHoennHarborRoute(u8 route)
{
    return route >= COOP_GROUP_TRAVEL_ROUTE_LILYCOVE_SOUTHERN_ISLAND
        && route <= COOP_GROUP_TRAVEL_ROUTE_NAVEL_ROCK_LILYCOVE;
}

static bool8 IsSSTidalRoute(u8 route)
{
    return route >= COOP_GROUP_TRAVEL_ROUTE_SS_TIDAL_LILYCOVE_EXIT
        && route <= COOP_GROUP_TRAVEL_ROUTE_SS_TIDAL_SLATEPORT_EXIT;
}

static bool8 IsSSTidalBoardingRoute(u8 route)
{
    return route == COOP_GROUP_TRAVEL_ROUTE_SS_TIDAL_SLATEPORT_BOARD
        || route == COOP_GROUP_TRAVEL_ROUTE_SS_TIDAL_LILYCOVE_BOARD;
}

static bool8 SSTidalBoardingReady(void)
{
    /* Scott's first onboard scene runs after the corridor warp. Requiring it
     * here strands grouped players at their first ferry boarding. */
    return FlagGet(FLAG_SYS_GAME_CLEAR);
}

static bool8 IsBrineyRoute(u8 route)
{
    return route >= COOP_GROUP_TRAVEL_ROUTE_BRINEY_HOUSE_DEWFORD
        && route <= COOP_GROUP_TRAVEL_ROUTE_ROUTE109_DEWFORD;
}

static bool8 IsBillStoryRoute(u8 route)
{
    return route == COOP_GROUP_TRAVEL_ROUTE_BILL_CINNABAR_ONE
        || route == COOP_GROUP_TRAVEL_ROUTE_BILL_ONE_CINNABAR;
}

static bool8 IsStoryRoute(u8 route)
{
    return route == COOP_GROUP_TRAVEL_ROUTE_BRINEY_HOUSE_DEWFORD
        || IsBillStoryRoute(route);
}

static bool8 IsSeagallopRoute(u8 route)
{
    return route >= COOP_GROUP_TRAVEL_ROUTE_SEAGALLOP_FIRST
        && route <= COOP_GROUP_TRAVEL_ROUTE_SEAGALLOP_BIRTH_VERMILION;
}

static bool8 SeagallopRouteEndpoints(u8 route, u8 *origin, u8 *destination)
{
    u8 slot;
    if (route >= COOP_GROUP_TRAVEL_ROUTE_SEAGALLOP_FIRST
     && route <= COOP_GROUP_TRAVEL_ROUTE_SEAGALLOP_LAST_REGULAR)
    {
        *origin = (route - COOP_GROUP_TRAVEL_ROUTE_SEAGALLOP_FIRST) / 7;
        slot = (route - COOP_GROUP_TRAVEL_ROUTE_SEAGALLOP_FIRST) % 7;
        *destination = slot >= *origin ? slot + 1 : slot;
        return TRUE;
    }
    switch (route)
    {
    case COOP_GROUP_TRAVEL_ROUTE_SEAGALLOP_VERMILION_NAVEL: *origin = SEAGALLOP_VERMILION_CITY; *destination = SEAGALLOP_NAVEL_ROCK; return TRUE;
    case COOP_GROUP_TRAVEL_ROUTE_SEAGALLOP_NAVEL_VERMILION: *origin = SEAGALLOP_NAVEL_ROCK; *destination = SEAGALLOP_VERMILION_CITY; return TRUE;
    case COOP_GROUP_TRAVEL_ROUTE_SEAGALLOP_VERMILION_BIRTH: *origin = SEAGALLOP_VERMILION_CITY; *destination = SEAGALLOP_BIRTH_ISLAND; return TRUE;
    case COOP_GROUP_TRAVEL_ROUTE_SEAGALLOP_BIRTH_VERMILION: *origin = SEAGALLOP_BIRTH_ISLAND; *destination = SEAGALLOP_VERMILION_CITY; return TRUE;
    default: return FALSE;
    }
}

static u8 SeagallopRouteForEndpoints(u8 origin, u8 destination)
{
    if (origin < 8 && destination < 8 && origin != destination)
        return COOP_GROUP_TRAVEL_ROUTE_SEAGALLOP_FIRST
            + origin * 7 + (destination < origin ? destination : destination - 1);
    if (origin == SEAGALLOP_VERMILION_CITY && destination == SEAGALLOP_NAVEL_ROCK)
        return COOP_GROUP_TRAVEL_ROUTE_SEAGALLOP_VERMILION_NAVEL;
    if (origin == SEAGALLOP_NAVEL_ROCK && destination == SEAGALLOP_VERMILION_CITY)
        return COOP_GROUP_TRAVEL_ROUTE_SEAGALLOP_NAVEL_VERMILION;
    if (origin == SEAGALLOP_VERMILION_CITY && destination == SEAGALLOP_BIRTH_ISLAND)
        return COOP_GROUP_TRAVEL_ROUTE_SEAGALLOP_VERMILION_BIRTH;
    if (origin == SEAGALLOP_BIRTH_ISLAND && destination == SEAGALLOP_VERMILION_CITY)
        return COOP_GROUP_TRAVEL_ROUTE_SEAGALLOP_BIRTH_VERMILION;
    return 0;
}

static bool8 HasBrineyAtSource(u8 route)
{
    if (FlagGet(FLAG_DEFEATED_PETALBURG_GYM))
        return FALSE;
    switch (route)
    {
    case COOP_GROUP_TRAVEL_ROUTE_BRINEY_HOUSE_DEWFORD:
        return VarGet(VAR_BOARD_BRINEY_BOAT_STATE) == 0
            && !FlagGet(FLAG_HIDE_BRINEYS_HOUSE_MR_BRINEY);
    case COOP_GROUP_TRAVEL_ROUTE_DEWFORD_BRINEY_HOUSE:
        return !FlagGet(FLAG_HIDE_MR_BRINEY_DEWFORD_TOWN)
            && !FlagGet(FLAG_HIDE_MR_BRINEY_BOAT_DEWFORD_TOWN);
    case COOP_GROUP_TRAVEL_ROUTE_DEWFORD_ROUTE109:
        return FlagGet(FLAG_DELIVERED_STEVEN_LETTER)
            && !FlagGet(FLAG_HIDE_MR_BRINEY_DEWFORD_TOWN)
            && !FlagGet(FLAG_HIDE_MR_BRINEY_BOAT_DEWFORD_TOWN);
    case COOP_GROUP_TRAVEL_ROUTE_ROUTE109_DEWFORD:
        return !FlagGet(FLAG_HIDE_ROUTE_109_MR_BRINEY)
            && !FlagGet(FLAG_HIDE_ROUTE_109_MR_BRINEY_BOAT);
    default:
        return FALSE;
    }
}

static bool8 SSTidalRouteStateReady(u8 route)
{
    if (route == COOP_GROUP_TRAVEL_ROUTE_SS_TIDAL_LILYCOVE_EXIT)
        return VarGet(VAR_SS_TIDAL_STATE) == SS_TIDAL_LAND_LILYCOVE;
    if (route == COOP_GROUP_TRAVEL_ROUTE_SS_TIDAL_SLATEPORT_EXIT)
        return VarGet(VAR_SS_TIDAL_STATE) == SS_TIDAL_LAND_SLATEPORT;
    return FALSE;
}

static bool8 HasRouteTicket(u8 route)
{
    u16 item;
    u8 origin, destination;

    if (SeagallopRouteEndpoints(route, &origin, &destination))
    {
        /* The sailor's destination menu is local to the requester. Check
         * the same unlock on the responder before accepting an offer. */
        if (destination == SEAGALLOP_NAVEL_ROCK)
            return FlagGet(FLAG_ENABLE_SHIP_NAVEL_ROCK)
                && CheckBagHasItem(ITEM_MYSTIC_TICKET, 1);
        if (destination == SEAGALLOP_BIRTH_ISLAND)
            return FlagGet(FLAG_ENABLE_SHIP_BIRTH_ISLAND)
                && CheckBagHasItem(ITEM_AURORA_TICKET, 1);
        if (origin == SEAGALLOP_NAVEL_ROCK || origin == SEAGALLOP_BIRTH_ISLAND)
            return TRUE;
        if (destination >= SEAGALLOP_FOUR_ISLAND)
            return VarGet(VAR_MAP_SCENE_ONE_ISLAND_POKEMON_CENTER_1F) >= 5
                && CheckBagHasItem(ITEM_RAINBOW_PASS, 1);
        if (destination == SEAGALLOP_VERMILION_CITY)
            return VarGet(VAR_MAP_SCENE_ONE_ISLAND_POKEMON_CENTER_1F) >= 5
                || VarGet(VAR_MAP_SCENE_CINNABAR_ISLAND) >= 4;
        return VarGet(VAR_MAP_SCENE_ONE_ISLAND_POKEMON_CENTER_1F) >= 1
            && (CheckBagHasItem(ITEM_TRI_PASS, 1)
                || CheckBagHasItem(ITEM_RAINBOW_PASS, 1));
    }

    if (IsSSTidalBoardingRoute(route))
        return SSTidalBoardingReady();
    if (route == COOP_GROUP_TRAVEL_ROUTE_BATTLE_FRONTIER_SLATEPORT
     || route == COOP_GROUP_TRAVEL_ROUTE_BATTLE_FRONTIER_LILYCOVE)
        return CheckBagHasItem(ITEM_SS_TICKET, 1);
    if (route == COOP_GROUP_TRAVEL_ROUTE_LILYCOVE_BATTLE_FRONTIER
     || route == COOP_GROUP_TRAVEL_ROUTE_SLATEPORT_BATTLE_FRONTIER)
        return FlagGet(FLAG_SYS_GAME_CLEAR)
            && FlagGet(FLAG_MET_SCOTT_ON_SS_TIDAL);
    if (route >= COOP_GROUP_TRAVEL_ROUTE_LILYCOVE_SOUTHERN_ISLAND
     && route <= COOP_GROUP_TRAVEL_ROUTE_SLATEPORT_BATTLE_FRONTIER)
    {
        if (!FlagGet(FLAG_SYS_GAME_CLEAR)) return FALSE;
        switch (route)
        {
        case COOP_GROUP_TRAVEL_ROUTE_LILYCOVE_SOUTHERN_ISLAND:
            return FlagGet(FLAG_ENABLE_SHIP_SOUTHERN_ISLAND)
                && CheckBagHasItem(ITEM_EON_TICKET, 1);
        case COOP_GROUP_TRAVEL_ROUTE_LILYCOVE_NAVEL_ROCK:
            return FlagGet(FLAG_ENABLE_SHIP_NAVEL_ROCK)
                && CheckBagHasItem(ITEM_MYSTIC_TICKET, 1);
        case COOP_GROUP_TRAVEL_ROUTE_LILYCOVE_BIRTH_ISLAND:
            return FlagGet(FLAG_ENABLE_SHIP_BIRTH_ISLAND)
                && CheckBagHasItem(ITEM_AURORA_TICKET, 1);
        case COOP_GROUP_TRAVEL_ROUTE_LILYCOVE_FARAWAY_ISLAND:
            return FlagGet(FLAG_ENABLE_SHIP_FARAWAY_ISLAND)
                && CheckBagHasItem(ITEM_OLD_SEA_MAP, 1);
        default:
            return TRUE;
        }
    }
    if (IsBrineyRoute(route))
        return HasBrineyAtSource(route);
    if (!IsOutboundIslandRoute(route))
        return TRUE;
    switch ((route - COOP_GROUP_TRAVEL_ROUTE_OLIVINE_SOUTHERN_ISLAND) % 4)
    {
    case 0: item = ITEM_EON_TICKET; break;
    case 1: item = ITEM_AURORA_TICKET; break;
    case 2: item = ITEM_OLD_SEA_MAP; break;
    default: return TRUE; /* Battle Frontier has no special ticket. */
    }
    return CheckBagHasItem(item, 1);
}

static bool8 RouteFields(u8 route, u8 *era, u8 *destination)
{
    static const u8 sEra[] = {0, 1, 2, 1, 2, 1, 2, 3};
    static const u8 sDestination[] = {0, 3, 4, 1, 2, 5, 6, 7};
    u8 origin, seagallopDestination;
    if (route == COOP_GROUP_TRAVEL_ROUTE_CABLE_CAR_ROUTE112_MT_CHIMNEY
     || route == COOP_GROUP_TRAVEL_ROUTE_CABLE_CAR_MT_CHIMNEY_ROUTE112)
    {
        *era = COOP_GROUP_TRAVEL_ERA_HOENN;
        *destination = route == COOP_GROUP_TRAVEL_ROUTE_CABLE_CAR_ROUTE112_MT_CHIMNEY
            ? COOP_GROUP_TRAVEL_DEST_MT_CHIMNEY_CABLE_CAR_STATION
            : COOP_GROUP_TRAVEL_DEST_ROUTE112_CABLE_CAR_STATION;
        return TRUE;
    }
    if (IsBillStoryRoute(route))
    {
        *era = COOP_GROUP_TRAVEL_ERA_ORIGINAL;
        *destination = route == COOP_GROUP_TRAVEL_ROUTE_BILL_CINNABAR_ONE
            ? COOP_GROUP_TRAVEL_DEST_BILL_ONE_ISLAND_CENTER
            : COOP_GROUP_TRAVEL_DEST_BILL_CINNABAR;
        return TRUE;
    }
    if (SeagallopRouteEndpoints(route, &origin, &seagallopDestination))
    {
        *era = seagallopDestination == SEAGALLOP_VERMILION_CITY
            ? COOP_GROUP_TRAVEL_ERA_ORIGINAL : COOP_GROUP_TRAVEL_ERA_SEVII;
        *destination = COOP_GROUP_TRAVEL_DEST_SEAGALLOP_VERMILION
            + seagallopDestination - (seagallopDestination > SEAGALLOP_CINNABAR_ISLAND);
        return TRUE;
    }
    if (route >= COOP_GROUP_TRAVEL_ROUTE_FLY_LITTLEROOT
     && route <= COOP_GROUP_TRAVEL_ROUTE_FLY_BATTLE_FRONTIER)
        return CoopRegionMap_GroupFlyFields(route, era, destination);
    if (route >= COOP_GROUP_TRAVEL_ROUTE_RETURN_FERRY_ORIGINAL
     && route <= COOP_GROUP_TRAVEL_ROUTE_RETURN_GATE_LATER)
    {
        *era = (route & 1) ? COOP_GROUP_TRAVEL_ERA_LATER : COOP_GROUP_TRAVEL_ERA_ORIGINAL;
        *destination = route <= COOP_GROUP_TRAVEL_ROUTE_RETURN_FERRY_LATER ? 14
            : route <= COOP_GROUP_TRAVEL_ROUTE_RETURN_TRAIN_LATER ? 12
            : COOP_GROUP_TRAVEL_DEST_JOHTO_RECEPTION_GATE;
        return TRUE;
    }
    if (IsOutboundIslandRoute(route))
    {
        *era = COOP_GROUP_TRAVEL_ERA_HOENN;
        *destination = COOP_GROUP_TRAVEL_DEST_SOUTHERN_ISLAND
            + (route - COOP_GROUP_TRAVEL_ROUTE_OLIVINE_SOUTHERN_ISLAND) % 4;
        return TRUE;
    }
    if (IsIslandReturnRoute(route))
    {
        *era = COOP_GROUP_TRAVEL_ERA_HOENN;
        *destination = route == COOP_GROUP_TRAVEL_ROUTE_BATTLE_FRONTIER_SLATEPORT
            ? COOP_GROUP_TRAVEL_DEST_SLATEPORT_HARBOR
            : COOP_GROUP_TRAVEL_DEST_LILYCOVE_HARBOR;
        return TRUE;
    }
    if (IsHoennHarborRoute(route))
    {
        *era = COOP_GROUP_TRAVEL_ERA_HOENN;
        *destination = route == COOP_GROUP_TRAVEL_ROUTE_LILYCOVE_SOUTHERN_ISLAND
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
        return TRUE;
    }
    if (IsSSTidalBoardingRoute(route))
    {
        *era = COOP_GROUP_TRAVEL_ERA_HOENN;
        *destination = COOP_GROUP_TRAVEL_DEST_SS_TIDAL_CORRIDOR;
        return TRUE;
    }
    if (IsSSTidalRoute(route))
    {
        *era = COOP_GROUP_TRAVEL_ERA_HOENN;
        *destination = route == COOP_GROUP_TRAVEL_ROUTE_SS_TIDAL_LILYCOVE_EXIT
            ? COOP_GROUP_TRAVEL_DEST_LILYCOVE_HARBOR
            : COOP_GROUP_TRAVEL_DEST_SLATEPORT_HARBOR;
        return TRUE;
    }
    if (IsBrineyRoute(route))
    {
        *era = COOP_GROUP_TRAVEL_ERA_HOENN;
        switch (route)
        {
        case COOP_GROUP_TRAVEL_ROUTE_BRINEY_HOUSE_DEWFORD:
            *destination = COOP_GROUP_TRAVEL_DEST_HOENN_DEWFORD;
            break;
        case COOP_GROUP_TRAVEL_ROUTE_ROUTE109_DEWFORD:
            *destination = COOP_GROUP_TRAVEL_DEST_HOENN_DEWFORD;
            break;
        case COOP_GROUP_TRAVEL_ROUTE_DEWFORD_BRINEY_HOUSE:
            *destination = COOP_GROUP_TRAVEL_DEST_BRINEY_HOUSE;
            break;
        case COOP_GROUP_TRAVEL_ROUTE_DEWFORD_ROUTE109:
            *destination = COOP_GROUP_TRAVEL_DEST_ROUTE109;
            break;
        default:
            return FALSE;
        }
        return TRUE;
    }
    if (route < 1 || route > COOP_GROUP_TRAVEL_ROUTE_GATE_LATER) return FALSE;
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
    case COOP_GROUP_TRAVEL_DEPARTURE_CABLE_CAR:
        return route == COOP_GROUP_TRAVEL_ROUTE_CABLE_CAR_ROUTE112_MT_CHIMNEY
            || route == COOP_GROUP_TRAVEL_ROUTE_CABLE_CAR_MT_CHIMNEY_ROUTE112;
    case COOP_GROUP_TRAVEL_DEPARTURE_TRAIN:
        return route == COOP_GROUP_TRAVEL_ROUTE_TRAIN_ORIGINAL
            || route == COOP_GROUP_TRAVEL_ROUTE_TRAIN_LATER
            || route == COOP_GROUP_TRAVEL_ROUTE_RETURN_TRAIN_ORIGINAL
            || route == COOP_GROUP_TRAVEL_ROUTE_RETURN_TRAIN_LATER;
    case COOP_GROUP_TRAVEL_DEPARTURE_FERRY:
        return route == COOP_GROUP_TRAVEL_ROUTE_FERRY_ORIGINAL
            || route == COOP_GROUP_TRAVEL_ROUTE_FERRY_LATER
            || route == COOP_GROUP_TRAVEL_ROUTE_RETURN_FERRY_ORIGINAL
            || route == COOP_GROUP_TRAVEL_ROUTE_RETURN_FERRY_LATER
            || IsOutboundIslandRoute(route)
            || IsIslandReturnRoute(route)
            || IsHoennHarborRoute(route)
            || IsSSTidalRoute(route)
            || IsSSTidalBoardingRoute(route)
            || IsBrineyRoute(route)
            || IsSeagallopRoute(route)
            || IsBillStoryRoute(route);
    case COOP_GROUP_TRAVEL_DEPARTURE_SSAQUA_MAIDEN:
        return route == COOP_GROUP_TRAVEL_ROUTE_FERRY_ORIGINAL
            || route == COOP_GROUP_TRAVEL_ROUTE_FERRY_LATER;
    case COOP_GROUP_TRAVEL_DEPARTURE_GATE:
        return route == COOP_GROUP_TRAVEL_ROUTE_GATE_ORIGINAL
            || route == COOP_GROUP_TRAVEL_ROUTE_GATE_LATER
            || route == COOP_GROUP_TRAVEL_ROUTE_RETURN_GATE_ORIGINAL
            || route == COOP_GROUP_TRAVEL_ROUTE_RETURN_GATE_LATER;
    case COOP_GROUP_TRAVEL_DEPARTURE_FLY:
    {
        u8 era, destination;
        return route >= COOP_GROUP_TRAVEL_ROUTE_FLY_LITTLEROOT
            && route <= COOP_GROUP_TRAVEL_ROUTE_FLY_BATTLE_FRONTIER
            && CoopRegionMap_GroupFlyFields(route, &era, &destination);
    }
    default:
        return FALSE;
    }
}

static bool8 IsMaterializedDeparture(u8 route, u8 departure)
{
    u16 map;

    if (!RouteMatchesDeparture(route, departure) || gSaveBlock1Ptr == NULL)
        return FALSE;
    if (departure == COOP_GROUP_TRAVEL_DEPARTURE_FLY)
        return TRUE;
    if (IsBillStoryRoute(route))
    {
        map = route == COOP_GROUP_TRAVEL_ROUTE_BILL_CINNABAR_ONE
            ? MAP_CINNABAR_ISLAND : MAP_ONE_ISLAND_POKEMON_CENTER_1F;
        if (route == COOP_GROUP_TRAVEL_ROUTE_BILL_CINNABAR_ONE
         && (VarGet(VAR_MAP_SCENE_CINNABAR_ISLAND) != 2
          || VarGet(VAR_MAP_SCENE_CINNABAR_ISLAND_2) != 3
          || FlagGet(FLAG_HIDE_CINNABAR_BILL)
          || FlagGet(FLAG_TEMP_2)
          || gSaveBlock1Ptr->pos.x != 20
          || (gSaveBlock1Ptr->pos.y != 5 && gSaveBlock1Ptr->pos.y != 6)))
            return FALSE;
        if (route == COOP_GROUP_TRAVEL_ROUTE_BILL_ONE_CINNABAR
         && VarGet(VAR_MAP_SCENE_ONE_ISLAND_POKEMON_CENTER_1F) != 2)
            return FALSE;
        if (gSaveBlock1Ptr->location.mapGroup != MAP_GROUP(map)
         || gSaveBlock1Ptr->location.mapNum != MAP_NUM(map))
            return FALSE;
        /* A responder can wait one tile west of the active coord triggers.
         * Both positions have a known lane for Bill's farewell movements. */
        return route != COOP_GROUP_TRAVEL_ROUTE_BILL_ONE_CINNABAR
            || ((gSaveBlock1Ptr->pos.x == 11 || gSaveBlock1Ptr->pos.x == 12)
             && gSaveBlock1Ptr->pos.y >= 6
             && gSaveBlock1Ptr->pos.y <= 9);
    }
    switch (departure)
    {
    case COOP_GROUP_TRAVEL_DEPARTURE_CABLE_CAR:
        map = route == COOP_GROUP_TRAVEL_ROUTE_CABLE_CAR_ROUTE112_MT_CHIMNEY
            ? MAP_ROUTE112_CABLE_CAR_STATION : MAP_MT_CHIMNEY_CABLE_CAR_STATION;
        break;
    case COOP_GROUP_TRAVEL_DEPARTURE_TRAIN:
        map = route >= COOP_GROUP_TRAVEL_ROUTE_RETURN_FERRY_ORIGINAL
            ? (route == COOP_GROUP_TRAVEL_ROUTE_RETURN_TRAIN_ORIGINAL
                ? MAP_KANTO_ORIGINAL_SAFFRON_CITY_TRAIN_STATION
                : MAP_KANTO_LATER_SAFFRON_CITY_TRAIN_STATION)
            : MAP_GOLDENROD_CITY_TRAIN_STATION;
        break;
    case COOP_GROUP_TRAVEL_DEPARTURE_FERRY:
    case COOP_GROUP_TRAVEL_DEPARTURE_SSAQUA_MAIDEN:
        if (IsSSTidalBoardingRoute(route))
            map = route == COOP_GROUP_TRAVEL_ROUTE_SS_TIDAL_SLATEPORT_BOARD
                ? MAP_SLATEPORT_CITY_HARBOR : MAP_LILYCOVE_CITY_HARBOR;
        else if (IsSSTidalRoute(route))
            map = MAP_SS_TIDAL_CORRIDOR;
        else if (IsSeagallopRoute(route))
        {
            static const u16 sSeagallopSourceMaps[] = {
                MAP_VERMILION_CITY, MAP_ONE_ISLAND_HARBOR,
                MAP_TWO_ISLAND_HARBOR, MAP_THREE_ISLAND_HARBOR,
                MAP_FOUR_ISLAND_HARBOR, MAP_FIVE_ISLAND_HARBOR,
                MAP_SIX_ISLAND_HARBOR, MAP_SEVEN_ISLAND_HARBOR,
                MAP_CINNABAR_ISLAND, MAP_NAVEL_ROCK_HARBOR_FRLG,
                MAP_BIRTH_ISLAND_HARBOR_FRLG,
            };
            u8 origin, destination;
            if (!SeagallopRouteEndpoints(route, &origin, &destination))
                return FALSE;
            map = sSeagallopSourceMaps[origin];
        }
        else if (IsBrineyRoute(route))
        map = route == COOP_GROUP_TRAVEL_ROUTE_BRINEY_HOUSE_DEWFORD
                ? MAP_ROUTE104_MR_BRINEYS_HOUSE
                : route == COOP_GROUP_TRAVEL_ROUTE_DEWFORD_BRINEY_HOUSE
                || route == COOP_GROUP_TRAVEL_ROUTE_DEWFORD_ROUTE109
                ? MAP_DEWFORD_TOWN : MAP_ROUTE109;
        else if (IsHoennHarborRoute(route))
            map = route == COOP_GROUP_TRAVEL_ROUTE_SLATEPORT_BATTLE_FRONTIER
                ? MAP_SLATEPORT_CITY_HARBOR
                : route == COOP_GROUP_TRAVEL_ROUTE_NAVEL_ROCK_LILYCOVE
                ? MAP_NAVEL_ROCK_HARBOR : MAP_LILYCOVE_CITY_HARBOR;
        else if (IsIslandReturnRoute(route))
            map = route == COOP_GROUP_TRAVEL_ROUTE_SOUTHERN_ISLAND_LILYCOVE
                ? MAP_SOUTHERN_ISLAND_EXTERIOR
                : route == COOP_GROUP_TRAVEL_ROUTE_BIRTH_ISLAND_LILYCOVE
                ? MAP_BIRTH_ISLAND_HARBOR
                : route == COOP_GROUP_TRAVEL_ROUTE_FARAWAY_ISLAND_LILYCOVE
                ? MAP_FARAWAY_ISLAND_ENTRANCE
                : MAP_BATTLE_FRONTIER_OUTSIDE_WEST;
        else if (IsOutboundIslandRoute(route))
            map = IsOlivineIslandRoute(route)
                ? MAP_OLIVINE_CITY_PORT_INSIDE
                : MAP_KANTO_LATER_VERMILION_CITY_PORT_INSIDE;
        else
        map = route >= COOP_GROUP_TRAVEL_ROUTE_RETURN_FERRY_ORIGINAL
            ? (route == COOP_GROUP_TRAVEL_ROUTE_RETURN_FERRY_ORIGINAL
                ? MAP_KANTO_ORIGINAL_VERMILION_CITY_PORT_INSIDE
                : MAP_KANTO_LATER_VERMILION_CITY_PORT_INSIDE)
            : MAP_OLIVINE_CITY_PORT_INSIDE;
        break;
    case COOP_GROUP_TRAVEL_DEPARTURE_GATE:
        map = route >= COOP_GROUP_TRAVEL_ROUTE_RETURN_FERRY_ORIGINAL
            ? (route == COOP_GROUP_TRAVEL_ROUTE_RETURN_GATE_ORIGINAL
                ? MAP_ROUTE22 : MAP_KANTO_LATER_ROUTE22)
            : MAP_RECEPTION_GATE;
        break;
    default:
        return FALSE;
    }
    return gSaveBlock1Ptr->location.mapGroup == MAP_GROUP(map)
        && gSaveBlock1Ptr->location.mapNum == MAP_NUM(map)
        && (!IsSSTidalRoute(route) || SSTidalRouteStateReady(route))
        && (!IsSSTidalBoardingRoute(route) || SSTidalBoardingReady());
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
    if (IsStoryRoute(record->route)
     && record->kind == COOP_GROUP_TRAVEL_CLIENT_APPLIED) return FALSE;
    if (record->remaining_seconds != 0) return FALSE;
    zero = IsZero(record->proposal_id, sizeof(record->proposal_id));
    switch (record->kind)
    {
    case COOP_GROUP_TRAVEL_CLIENT_REQUEST: return zero && record->result == 0 && record->reason == 0;
    case COOP_GROUP_TRAVEL_CLIENT_DECISION: return !zero && (record->result == 1 || record->result == 2) && record->reason == 0;
    case COOP_GROUP_TRAVEL_CLIENT_CANCEL: return record->result == 0 && record->reason == COOP_GROUP_TRAVEL_REASON_REQUESTER_CANCELED;
    case COOP_GROUP_TRAVEL_CLIENT_APPLIED: return !zero && record->result == COOP_GROUP_TRAVEL_RESULT_APPLIED && record->reason == 0;
    case COOP_GROUP_TRAVEL_CLIENT_SCENE_MARKER_REQUEST:
    case COOP_GROUP_TRAVEL_CLIENT_SCENE_COMPLETE:
        return IsStoryRoute(record->route)
            && !zero && record->result == 0 && record->reason == 0;
    default: return FALSE;
    }
}

bool8 CoopGroupTravelProtocol_ValidateServer(const struct CoopGroupTravelRecord *record)
{
    bool8 zero;
    if (!ValidateCommon(record)) return FALSE;
    if (IsStoryRoute(record->route)
     && record->kind == COOP_GROUP_TRAVEL_SERVER_COMMIT) return FALSE;
    if (record->remaining_seconds > 30
     || (record->kind != COOP_GROUP_TRAVEL_SERVER_REQUESTING
      && record->kind != COOP_GROUP_TRAVEL_SERVER_OFFER
      && record->remaining_seconds != 0)) return FALSE;
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
    case COOP_GROUP_TRAVEL_SERVER_SCENE_MARKER_ACCEPTED:
    case COOP_GROUP_TRAVEL_SERVER_SCENE_READY:
        return IsStoryRoute(record->route)
            && !zero && record->result == 0 && record->reason == 0;
    default: return FALSE;
    }
}

static bool8 SendSemantic(void)
{
    struct CoopGroupTravelRecord client = sTravel.semantic;
    client.remaining_seconds = 0;
    return CoopGroupTravelProtocol_ValidateClient(&client)
        && CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_GROUP_TRAVEL_CLIENT,
                                               &client, sizeof(client));
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
        return sTravel.test_safe && !ScriptContext_IsEnabled();
#endif
    return gMain.callback1 == CB1_Overworld && gMain.callback2 == CB2_Overworld
        && !gPaletteFade.active && !ScriptContext_IsEnabled();
}

static bool8 AcquireControlLock(void)
{
    if (sTravel.controls_locked)
    {
        if (ArePlayerFieldControlsLocked())
            return TRUE;
        /* Returning from the Fly map runs a field callback that unlocks
         * controls.  Restore our vote lock once the field is safe again. */
        if (!CanOwnControlLock())
            return FALSE;
        LockPlayerFieldControls();
        return TRUE;
    }
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
    HideVoteCountdown();
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
    sTravel.vote_known = FALSE;
    sTravel.scene_landed = FALSE;
    sTravel.scene_started = FALSE;
    sTravel.script_lock_handoff = FALSE;
    sTravel.script_objects_frozen = FALSE;
}

void CoopGroupTravel_Init(void)
{
    memset(&sTravel, 0, sizeof(sTravel));
    sTravel.vote_window = WINDOW_NONE;
    sTravel.next_request_id = 1;
}

enum CoopGroupTravelBeginResult CoopGroupTravel_Begin(u8 route)
{
    if (sTravel.state != STATE_IDLE || route < 1 || route >= COOP_GROUP_TRAVEL_ROUTE_FLY_LITTLEROOT)
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

enum CoopGroupTravelBeginResult CoopGroupTravel_BeginFly(u8 route)
{
    u8 era, destination;

    if (sTravel.state != STATE_IDLE)
        return COOP_GROUP_TRAVEL_BEGIN_REJECTED;
    if (!IsGrouped())
        return COOP_GROUP_TRAVEL_BEGIN_NOT_GROUPED;
    if (!CoopRegionMap_GroupFlyFields(route, &era, &destination)
     || !CoopRegionMap_GroupFlyUnlocked(route))
        return COOP_GROUP_TRAVEL_BEGIN_REJECTED;
    /* This first Fly route uses the vanilla visit flag on each ROM.  The
     * server still fences both members and requires the partner's consent. */
    SetRoute(&sTravel.semantic, route);
    sTravel.semantic.departure = COOP_GROUP_TRAVEL_DEPARTURE_FLY;
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
    LockPlayerFieldControls();
    sTravel.controls_locked = TRUE;
    sTravel.state = STATE_REQUESTING;
    sTravel.semantic_queued = TRUE;
    sTravel.departure = COOP_GROUP_TRAVEL_DEPARTURE_FLY;
    return COOP_GROUP_TRAVEL_BEGIN_WAITING;
}

enum CoopGroupTravelBeginResult CoopGroupTravel_BeginFromScript(u8 route, u8 departure)
{
    if (sTravel.state != STATE_IDLE || !IsMaterializedDeparture(route, departure)
     || !HasRouteTicket(route)
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
    HideVoteCountdown();
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
    if (accept && sTravel.semantic.departure == COOP_GROUP_TRAVEL_DEPARTURE_FLY
     && !CoopRegionMap_GroupFlyUnlocked(sTravel.semantic.route))
        accept = FALSE;
    if (accept && !HasRouteTicket(sTravel.semantic.route))
        accept = FALSE;
    if (accept && IsStoryRoute(sTravel.semantic.route)
     && !IsMaterializedDeparture(sTravel.semantic.route, sTravel.semantic.departure))
        accept = FALSE;
    if (accept && (IsSSTidalRoute(sTravel.semantic.route)
                || IsSSTidalBoardingRoute(sTravel.semantic.route)
                || sTravel.semantic.departure == COOP_GROUP_TRAVEL_DEPARTURE_CABLE_CAR)
     && !IsMaterializedDeparture(sTravel.semantic.route, sTravel.semantic.departure))
        accept = FALSE;
    HideVoteCountdown();
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
    {MAP_LITTLEROOT_TOWN, WARP_ID_NONE, 5, 6, 5, 6},
    [COOP_GROUP_TRAVEL_ROUTE_RETURN_FERRY_ORIGINAL] = {MAP_OLIVINE_CITY_PORT_INSIDE, WARP_ID_NONE, 8, 16, 8, 16},
    [COOP_GROUP_TRAVEL_ROUTE_RETURN_FERRY_LATER] = {MAP_OLIVINE_CITY_PORT_INSIDE, WARP_ID_NONE, 8, 16, 8, 16},
    [COOP_GROUP_TRAVEL_ROUTE_RETURN_TRAIN_ORIGINAL] = {MAP_GOLDENROD_CITY_TRAIN_STATION, WARP_ID_NONE, 19, 16, 19, 16},
    [COOP_GROUP_TRAVEL_ROUTE_RETURN_TRAIN_LATER] = {MAP_GOLDENROD_CITY_TRAIN_STATION, WARP_ID_NONE, 19, 16, 19, 16},
    [COOP_GROUP_TRAVEL_ROUTE_RETURN_GATE_ORIGINAL] = {MAP_RECEPTION_GATE, WARP_ID_NONE, 18, 9, 18, 9},
    [COOP_GROUP_TRAVEL_ROUTE_RETURN_GATE_LATER] = {MAP_RECEPTION_GATE, WARP_ID_NONE, 18, 9, 18, 9},
    [COOP_GROUP_TRAVEL_ROUTE_OLIVINE_SOUTHERN_ISLAND] = {MAP_SOUTHERN_ISLAND_EXTERIOR, WARP_ID_NONE, 13, 22, 13, 22},
    [COOP_GROUP_TRAVEL_ROUTE_OLIVINE_BIRTH_ISLAND] = {MAP_BIRTH_ISLAND_EXTERIOR, WARP_ID_NONE, 13, 23, 13, 23},
    [COOP_GROUP_TRAVEL_ROUTE_OLIVINE_FARAWAY_ISLAND] = {MAP_FARAWAY_ISLAND_ENTRANCE, WARP_ID_NONE, 13, 38, 13, 38},
    [COOP_GROUP_TRAVEL_ROUTE_OLIVINE_BATTLE_FRONTIER] = {MAP_BATTLE_FRONTIER_OUTSIDE_WEST, WARP_ID_NONE, 20, 67, 20, 67},
    [COOP_GROUP_TRAVEL_ROUTE_VERMILION_SOUTHERN_ISLAND] = {MAP_SOUTHERN_ISLAND_EXTERIOR, WARP_ID_NONE, 13, 22, 13, 22},
    [COOP_GROUP_TRAVEL_ROUTE_VERMILION_BIRTH_ISLAND] = {MAP_BIRTH_ISLAND_EXTERIOR, WARP_ID_NONE, 13, 23, 13, 23},
    [COOP_GROUP_TRAVEL_ROUTE_VERMILION_FARAWAY_ISLAND] = {MAP_FARAWAY_ISLAND_ENTRANCE, WARP_ID_NONE, 13, 38, 13, 38},
    [COOP_GROUP_TRAVEL_ROUTE_VERMILION_BATTLE_FRONTIER] = {MAP_BATTLE_FRONTIER_OUTSIDE_WEST, WARP_ID_NONE, 20, 67, 20, 67},
    [COOP_GROUP_TRAVEL_ROUTE_SOUTHERN_ISLAND_LILYCOVE] = {MAP_LILYCOVE_CITY_HARBOR, WARP_ID_NONE, 8, 11, 8, 11},
    [COOP_GROUP_TRAVEL_ROUTE_BIRTH_ISLAND_LILYCOVE] = {MAP_LILYCOVE_CITY_HARBOR, WARP_ID_NONE, 8, 11, 8, 11},
    [COOP_GROUP_TRAVEL_ROUTE_FARAWAY_ISLAND_LILYCOVE] = {MAP_LILYCOVE_CITY_HARBOR, WARP_ID_NONE, 8, 11, 8, 11},
    [COOP_GROUP_TRAVEL_ROUTE_BATTLE_FRONTIER_SLATEPORT] = {MAP_SLATEPORT_CITY_HARBOR, WARP_ID_NONE, 8, 11, 8, 11},
    [COOP_GROUP_TRAVEL_ROUTE_BATTLE_FRONTIER_LILYCOVE] = {MAP_LILYCOVE_CITY_HARBOR, WARP_ID_NONE, 8, 11, 8, 11},
    [COOP_GROUP_TRAVEL_ROUTE_LILYCOVE_SOUTHERN_ISLAND] = {MAP_SOUTHERN_ISLAND_EXTERIOR, WARP_ID_NONE, 13, 22, 13, 22},
    [COOP_GROUP_TRAVEL_ROUTE_LILYCOVE_NAVEL_ROCK] = {MAP_NAVEL_ROCK_HARBOR, WARP_ID_NONE, 8, 4, 8, 4},
    [COOP_GROUP_TRAVEL_ROUTE_LILYCOVE_BIRTH_ISLAND] = {MAP_BIRTH_ISLAND_HARBOR, WARP_ID_NONE, 8, 4, 8, 4},
    [COOP_GROUP_TRAVEL_ROUTE_LILYCOVE_FARAWAY_ISLAND] = {MAP_FARAWAY_ISLAND_ENTRANCE, WARP_ID_NONE, 13, 38, 13, 38},
    [COOP_GROUP_TRAVEL_ROUTE_LILYCOVE_BATTLE_FRONTIER] = {MAP_BATTLE_FRONTIER_OUTSIDE_WEST, WARP_ID_NONE, 19, 67, 19, 67},
    [COOP_GROUP_TRAVEL_ROUTE_SLATEPORT_BATTLE_FRONTIER] = {MAP_BATTLE_FRONTIER_OUTSIDE_WEST, WARP_ID_NONE, 19, 67, 19, 67},
    [COOP_GROUP_TRAVEL_ROUTE_NAVEL_ROCK_LILYCOVE] = {MAP_LILYCOVE_CITY_HARBOR, WARP_ID_NONE, 8, 11, 8, 11},
    [COOP_GROUP_TRAVEL_ROUTE_SS_TIDAL_SLATEPORT_BOARD] = {MAP_SS_TIDAL_CORRIDOR, WARP_ID_NONE, 1, 10, 1, 10},
    [COOP_GROUP_TRAVEL_ROUTE_SS_TIDAL_LILYCOVE_BOARD] = {MAP_SS_TIDAL_CORRIDOR, WARP_ID_NONE, 1, 10, 1, 10},
    [COOP_GROUP_TRAVEL_ROUTE_SS_TIDAL_LILYCOVE_EXIT] = {MAP_LILYCOVE_CITY_HARBOR, WARP_ID_NONE, 8, 11, 8, 11},
    [COOP_GROUP_TRAVEL_ROUTE_SS_TIDAL_SLATEPORT_EXIT] = {MAP_SLATEPORT_CITY_HARBOR, WARP_ID_NONE, 8, 11, 8, 11},
    [COOP_GROUP_TRAVEL_ROUTE_DEWFORD_BRINEY_HOUSE] = {MAP_ROUTE104_MR_BRINEYS_HOUSE, WARP_ID_NONE, 5, 4, 5, 4},
    [COOP_GROUP_TRAVEL_ROUTE_DEWFORD_ROUTE109] = {MAP_ROUTE109, WARP_ID_NONE, 21, 26, 21, 26},
    [COOP_GROUP_TRAVEL_ROUTE_ROUTE109_DEWFORD] = {MAP_DEWFORD_TOWN, WARP_ID_NONE, 12, 8, 12, 8},
    [100] = {MAP_ONE_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [101] = {MAP_TWO_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [102] = {MAP_THREE_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [103] = {MAP_FOUR_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [104] = {MAP_FIVE_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [105] = {MAP_SIX_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [106] = {MAP_SEVEN_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [107] = {MAP_VERMILION_CITY, WARP_ID_NONE, 23, 32, 23, 32},
    [108] = {MAP_TWO_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [109] = {MAP_THREE_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [110] = {MAP_FOUR_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [111] = {MAP_FIVE_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [112] = {MAP_SIX_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [113] = {MAP_SEVEN_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [114] = {MAP_VERMILION_CITY, WARP_ID_NONE, 23, 32, 23, 32},
    [115] = {MAP_ONE_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [116] = {MAP_THREE_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [117] = {MAP_FOUR_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [118] = {MAP_FIVE_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [119] = {MAP_SIX_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [120] = {MAP_SEVEN_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [121] = {MAP_VERMILION_CITY, WARP_ID_NONE, 23, 32, 23, 32},
    [122] = {MAP_ONE_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [123] = {MAP_TWO_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [124] = {MAP_FOUR_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [125] = {MAP_FIVE_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [126] = {MAP_SIX_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [127] = {MAP_SEVEN_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [128] = {MAP_VERMILION_CITY, WARP_ID_NONE, 23, 32, 23, 32},
    [129] = {MAP_ONE_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [130] = {MAP_TWO_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [131] = {MAP_THREE_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [132] = {MAP_FIVE_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [133] = {MAP_SIX_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [134] = {MAP_SEVEN_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [135] = {MAP_VERMILION_CITY, WARP_ID_NONE, 23, 32, 23, 32},
    [136] = {MAP_ONE_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [137] = {MAP_TWO_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [138] = {MAP_THREE_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [139] = {MAP_FOUR_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [140] = {MAP_SIX_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [141] = {MAP_SEVEN_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [142] = {MAP_VERMILION_CITY, WARP_ID_NONE, 23, 32, 23, 32},
    [143] = {MAP_ONE_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [144] = {MAP_TWO_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [145] = {MAP_THREE_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [146] = {MAP_FOUR_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [147] = {MAP_FIVE_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [148] = {MAP_SEVEN_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [149] = {MAP_VERMILION_CITY, WARP_ID_NONE, 23, 32, 23, 32},
    [150] = {MAP_ONE_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [151] = {MAP_TWO_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [152] = {MAP_THREE_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [153] = {MAP_FOUR_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [154] = {MAP_FIVE_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [155] = {MAP_SIX_ISLAND_HARBOR, WARP_ID_NONE, 8, 5, 8, 5},
    [156] = {MAP_NAVEL_ROCK_HARBOR_FRLG, WARP_ID_NONE, 8, 5, 8, 5},
    [157] = {MAP_VERMILION_CITY, WARP_ID_NONE, 23, 32, 23, 32},
    [158] = {MAP_BIRTH_ISLAND_HARBOR_FRLG, WARP_ID_NONE, 8, 5, 8, 5},
    [159] = {MAP_VERMILION_CITY, WARP_ID_NONE, 23, 32, 23, 32},
    [160] = {MAP_ONE_ISLAND_POKEMON_CENTER_1F, WARP_ID_NONE, 9, 9, 9, 9},
    [161] = {MAP_CINNABAR_ISLAND, WARP_ID_NONE, 21, 7, 21, 7},
    [COOP_GROUP_TRAVEL_ROUTE_CABLE_CAR_ROUTE112_MT_CHIMNEY] = {MAP_MT_CHIMNEY_CABLE_CAR_STATION, WARP_ID_NONE, 6, 8, 6, 8},
    [COOP_GROUP_TRAVEL_ROUTE_CABLE_CAR_MT_CHIMNEY_ROUTE112] = {MAP_ROUTE112_CABLE_CAR_STATION, WARP_ID_NONE, 6, 8, 6, 8},
};

static bool8 StageCommit(void)
{
    const struct TravelWarp *warp;
    u32 flyHeal;
    u32 heal;
    enum JohtoTravelDestination target;

    if (sTravel.semantic.departure == COOP_GROUP_TRAVEL_DEPARTURE_FLY)
    {
        if (sTravel.semantic.route == COOP_GROUP_TRAVEL_ROUTE_FLY_LITTLEROOT)
        {
            warp = &sTravelWarps[sTravel.semantic.route];
            SetWarpDestination(MAP_GROUP(warp->map), MAP_NUM(warp->map), warp->warp,
                               warp->warp_x, warp->warp_y);
        }
        else
        {
            flyHeal = CoopRegionMap_GroupFlyHealLocation(sTravel.semantic.route);
            if (flyHeal == HEAL_LOCATION_NONE || flyHeal > 255
             || GetHealLocation(flyHeal) == NULL)
                return FALSE;
            SetWarpDestinationToHealLocation(flyHeal);
        }
        DoWarp();
        sTravel.controls_locked = FALSE;
        return TRUE;
    }
    warp = &sTravelWarps[sTravel.semantic.route];
    if (sTravel.semantic.departure == COOP_GROUP_TRAVEL_DEPARTURE_CABLE_CAR)
    {
        if (!IsMaterializedDeparture(sTravel.semantic.route, sTravel.semantic.departure))
            return FALSE;
        /* Land on the aisle beyond the attendant's reset tile at (6, 7).
         * This stable tile is also the reconnect receipt position. */
        VarSet(VAR_CABLE_CAR_STATION_STATE, 0);
        IncrementGameStat(GAME_STAT_RODE_CABLE_CAR);
        SetWarpDestination(MAP_GROUP(warp->map), MAP_NUM(warp->map), warp->warp,
                           warp->warp_x, warp->warp_y);
        DoWarp();
        sTravel.controls_locked = FALSE;
        return TRUE;
    }
    if (IsSSTidalBoardingRoute(sTravel.semantic.route))
    {
        if (!IsMaterializedDeparture(sTravel.semantic.route, sTravel.semantic.departure)
         || !SSTidalBoardingReady())
            return FALSE;
        VarSet(VAR_SS_TIDAL_STATE,
            sTravel.semantic.route == COOP_GROUP_TRAVEL_ROUTE_SS_TIDAL_SLATEPORT_BOARD
                ? SS_TIDAL_BOARD_SLATEPORT : SS_TIDAL_BOARD_LILYCOVE);
        SetWarpDestination(MAP_GROUP(warp->map), MAP_NUM(warp->map), warp->warp,
                           warp->warp_x, warp->warp_y);
        DoWarp();
        sTravel.controls_locked = FALSE;
        return TRUE;
    }
    if (IsBrineyRoute(sTravel.semantic.route))
    {
        switch (sTravel.semantic.route)
        {
        case COOP_GROUP_TRAVEL_ROUTE_DEWFORD_BRINEY_HOUSE:
            FlagSet(FLAG_HIDE_MR_BRINEY_DEWFORD_TOWN);
            FlagSet(FLAG_HIDE_MR_BRINEY_BOAT_DEWFORD_TOWN);
            FlagSet(FLAG_HIDE_ROUTE_109_MR_BRINEY);
            FlagSet(FLAG_HIDE_ROUTE_109_MR_BRINEY_BOAT);
            FlagSet(FLAG_HIDE_ROUTE_104_MR_BRINEY);
            FlagClear(FLAG_HIDE_BRINEYS_HOUSE_MR_BRINEY);
            FlagClear(FLAG_HIDE_BRINEYS_HOUSE_PEEKO);
            FlagClear(FLAG_HIDE_ROUTE_104_MR_BRINEY_BOAT);
            VarSet(VAR_BOARD_BRINEY_BOAT_STATE, 2);
            VarSet(VAR_BRINEY_LOCATION, 1);
            break;
        case COOP_GROUP_TRAVEL_ROUTE_DEWFORD_ROUTE109:
            FlagSet(FLAG_HIDE_ROUTE_104_MR_BRINEY);
            FlagSet(FLAG_HIDE_ROUTE_104_MR_BRINEY_BOAT);
            FlagSet(FLAG_HIDE_BRINEYS_HOUSE_MR_BRINEY);
            FlagSet(FLAG_HIDE_BRINEYS_HOUSE_PEEKO);
            FlagSet(FLAG_HIDE_MR_BRINEY_DEWFORD_TOWN);
            FlagSet(FLAG_HIDE_MR_BRINEY_BOAT_DEWFORD_TOWN);
            FlagClear(FLAG_HIDE_ROUTE_109_MR_BRINEY);
            FlagClear(FLAG_HIDE_ROUTE_109_MR_BRINEY_BOAT);
            VarSet(VAR_BRINEY_LOCATION, 3);
            break;
        case COOP_GROUP_TRAVEL_ROUTE_ROUTE109_DEWFORD:
            FlagSet(FLAG_HIDE_ROUTE_104_MR_BRINEY);
            FlagSet(FLAG_HIDE_ROUTE_104_MR_BRINEY_BOAT);
            FlagSet(FLAG_HIDE_BRINEYS_HOUSE_MR_BRINEY);
            FlagSet(FLAG_HIDE_BRINEYS_HOUSE_PEEKO);
            FlagSet(FLAG_HIDE_ROUTE_109_MR_BRINEY);
            FlagSet(FLAG_HIDE_ROUTE_109_MR_BRINEY_BOAT);
            FlagClear(FLAG_HIDE_MR_BRINEY_DEWFORD_TOWN);
            FlagClear(FLAG_HIDE_MR_BRINEY_BOAT_DEWFORD_TOWN);
            VarSet(VAR_BRINEY_LOCATION, 2);
            break;
        default:
            return FALSE;
        }
        SetWarpDestination(MAP_GROUP(warp->map), MAP_NUM(warp->map), warp->warp,
                           warp->warp_x, warp->warp_y);
        DoWarp();
        sTravel.controls_locked = FALSE;
        return TRUE;
    }
    if (IsOutboundIslandRoute(sTravel.semantic.route)
     || IsIslandReturnRoute(sTravel.semantic.route)
     || IsHoennHarborRoute(sTravel.semantic.route)
     || IsSSTidalBoardingRoute(sTravel.semantic.route)
     || IsSSTidalRoute(sTravel.semantic.route)
     || IsSeagallopRoute(sTravel.semantic.route))
    {
        if (IsSSTidalRoute(sTravel.semantic.route))
        {
            if (!SSTidalRouteStateReady(sTravel.semantic.route))
                return FALSE;
            switch (sTravel.semantic.route)
            {
            case COOP_GROUP_TRAVEL_ROUTE_SS_TIDAL_LILYCOVE_EXIT:
                SetLastHealLocationWarp(HEAL_LOCATION_LILYCOVE_CITY); break;
            case COOP_GROUP_TRAVEL_ROUTE_SS_TIDAL_SLATEPORT_EXIT:
                SetLastHealLocationWarp(HEAL_LOCATION_SLATEPORT_CITY); break;
            }
            if (FlagGet(FLAG_RECEIVED_TM_SNATCH))
                FlagSet(FLAG_HIDE_SS_TIDAL_ROOMS_SNATCH_GIVER);
        }
        if (IsOutboundIslandRoute(sTravel.semantic.route))
            SetLastHealLocationWarp(IsOlivineIslandRoute(sTravel.semantic.route)
                ? HEAL_LOCATION_JOHTO_OLIVINE_CITY : HEAL_LOCATION_VERMILION_CITY);
        /* A first ticket showing is skipped by the grouped script path. Set
         * its persistent flag only after both players consented to travel. */
        switch (sTravel.semantic.route)
        {
        case COOP_GROUP_TRAVEL_ROUTE_LILYCOVE_SOUTHERN_ISLAND:
            FlagSet(FLAG_SHOWN_EON_TICKET); break;
        case COOP_GROUP_TRAVEL_ROUTE_LILYCOVE_NAVEL_ROCK:
            FlagSet(FLAG_SHOWN_MYSTIC_TICKET); break;
        case COOP_GROUP_TRAVEL_ROUTE_LILYCOVE_BIRTH_ISLAND:
            FlagSet(FLAG_SHOWN_AURORA_TICKET); break;
        case COOP_GROUP_TRAVEL_ROUTE_LILYCOVE_FARAWAY_ISLAND:
            FlagSet(FLAG_SHOWN_OLD_SEA_MAP); break;
        case COOP_GROUP_TRAVEL_ROUTE_SEAGALLOP_VERMILION_NAVEL:
            FlagSet(FLAG_SHOWN_MYSTIC_TICKET); break;
        case COOP_GROUP_TRAVEL_ROUTE_SEAGALLOP_VERMILION_BIRTH:
            FlagSet(FLAG_SHOWN_AURORA_TICKET); break;
        }
        SetWarpDestination(MAP_GROUP(warp->map), MAP_NUM(warp->map), warp->warp,
                           warp->warp_x, warp->warp_y);
        DoWarp();
        sTravel.controls_locked = FALSE;
        return TRUE;
    }
    heal = GetHealLocationIndexByWarpData(&gSaveBlock1Ptr->lastHealLocation);
    target = sTravel.semantic.route >= COOP_GROUP_TRAVEL_ROUTE_RETURN_FERRY_ORIGINAL
        ? JOHTO_TRAVEL_DESTINATION_JOHTO
        : sTravel.semantic.era == COOP_GROUP_TRAVEL_ERA_ORIGINAL
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
    const struct HealLocation *healLocation;
    u32 heal;

    if (route < COOP_GROUP_TRAVEL_ROUTE_TRAIN_ORIGINAL)
        return FALSE;
    if (route > COOP_GROUP_TRAVEL_ROUTE_FLY_LITTLEROOT
     && route <= COOP_GROUP_TRAVEL_ROUTE_FLY_BATTLE_FRONTIER)
    {
        heal = CoopRegionMap_GroupFlyHealLocation(route);
        healLocation = GetHealLocation(heal);
        return gSaveBlock1Ptr != NULL && healLocation != NULL
            && gSaveBlock1Ptr->location.mapGroup == healLocation->mapGroup
            && gSaveBlock1Ptr->location.mapNum == healLocation->mapNum
            && gSaveBlock1Ptr->pos.x == healLocation->x
            && gSaveBlock1Ptr->pos.y == healLocation->y;
    }
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
    if (record->departure == COOP_GROUP_TRAVEL_DEPARTURE_FLY
        || IsOutboundIslandRoute(record->route)
        || IsIslandReturnRoute(record->route)
        || IsHoennHarborRoute(record->route)
        || IsSSTidalBoardingRoute(record->route)
        || IsSSTidalRoute(record->route)
        || IsBrineyRoute(record->route)
        || IsSeagallopRoute(record->route)
        || record->departure == COOP_GROUP_TRAVEL_DEPARTURE_CABLE_CAR)
        return JOHTO_TRAVEL_DESTINATION_NONE;
    return record->route >= COOP_GROUP_TRAVEL_ROUTE_RETURN_FERRY_ORIGINAL
        ? JOHTO_TRAVEL_DESTINATION_JOHTO
        : record->era == COOP_GROUP_TRAVEL_ERA_ORIGINAL
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
    return IsGrouped() && IsMaterializedDeparture(record->route, record->departure)
        && (record->route != COOP_GROUP_TRAVEL_ROUTE_BRINEY_HOUSE_DEWFORD
            || HasBrineyAtSource(record->route));
}

static bool8 FirstBrineySceneReachedDewford(void)
{
    return gSaveBlock1Ptr != NULL
        && gSaveBlock1Ptr->location.mapGroup == MAP_GROUP(MAP_DEWFORD_TOWN)
        && gSaveBlock1Ptr->location.mapNum == MAP_NUM(MAP_DEWFORD_TOWN)
        && VarGet(VAR_BOARD_BRINEY_BOAT_STATE) == 0
        && FlagGet(FLAG_ENABLE_NORMAN_MATCH_CALL)
        && !FlagGet(FLAG_HIDE_MR_BRINEY_DEWFORD_TOWN)
        && !FlagGet(FLAG_HIDE_MR_BRINEY_BOAT_DEWFORD_TOWN)
        && FlagGet(FLAG_HIDE_ROUTE_104_MR_BRINEY_BOAT);
}

static bool8 StorySceneAtDestination(void)
{
    if (gSaveBlock1Ptr == NULL)
        return FALSE;
    if (sTravel.semantic.route == COOP_GROUP_TRAVEL_ROUTE_BRINEY_HOUSE_DEWFORD)
        return FirstBrineySceneReachedDewford();
    if (sTravel.semantic.route == COOP_GROUP_TRAVEL_ROUTE_BILL_CINNABAR_ONE)
        return gSaveBlock1Ptr->location.mapGroup == MAP_GROUP(MAP_ONE_ISLAND_POKEMON_CENTER_1F)
            && gSaveBlock1Ptr->location.mapNum == MAP_NUM(MAP_ONE_ISLAND_POKEMON_CENTER_1F)
            && VarGet(VAR_MAP_SCENE_CINNABAR_ISLAND) >= 2
            && VarGet(VAR_MAP_SCENE_ONE_ISLAND_HARBOR) >= 3
            && VarGet(VAR_MAP_SCENE_ONE_ISLAND_POKEMON_CENTER_1F) >= 1
            && FlagGet(FLAG_SYS_SEVII_MAP_123)
            && FlagGet(FLAG_SYS_PC_STORAGE_DISABLED);
    if (sTravel.semantic.route == COOP_GROUP_TRAVEL_ROUTE_BILL_ONE_CINNABAR)
        return gSaveBlock1Ptr->location.mapGroup == MAP_GROUP(MAP_CINNABAR_ISLAND)
            && gSaveBlock1Ptr->location.mapNum == MAP_NUM(MAP_CINNABAR_ISLAND)
            && VarGet(VAR_MAP_SCENE_ONE_ISLAND_POKEMON_CENTER_1F) >= 3
            && VarGet(VAR_MAP_SCENE_CINNABAR_ISLAND) >= 4
            && FlagGet(FLAG_HIDE_TWO_ISLAND_GAME_CORNER_LOSTELLE)
            && !FlagGet(FLAG_HIDE_LOSTELLE_IN_HER_HOME);
    return FALSE;
}

static bool8 TryStagePendingCommit(void)
{
    if (!sTravel.commit_pending || sTravel.suspended)
        return TRUE;
    if (sTravel.controls_locked
        ? !(CanOwnControlLock() && AcquireControlLock())
        : !IsSafeOverworld())
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
        if (record->kind == COOP_GROUP_TRAVEL_SERVER_COMPLETE
         && IsStoryRoute(record->route)
         && sTravel.session_ready
         && sTravel.last_story_complete_valid
         && memcmp(record, &sTravel.last_story_complete, sizeof(*record)) == 0)
            return TRUE;
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
            sTravel.vote_known = record->remaining_seconds != 0;
            sTravel.vote_deadline_frame = gMain.vblankCounter1 + record->remaining_seconds * 60;
            sTravel.departure = record->departure;
            sTravel.recovered_state = TRUE;
            sTravel.state = STATE_OFFER_DEFERRED;
            return TRUE;
        }
        if (record->kind == COOP_GROUP_TRAVEL_SERVER_REQUESTING)
        {
            sTravel.semantic = *record;
            sTravel.vote_known = record->remaining_seconds != 0;
            sTravel.vote_deadline_frame = gMain.vblankCounter1 + record->remaining_seconds * 60;
            sTravel.semantic.kind = COOP_GROUP_TRAVEL_CLIENT_REQUEST;
            sTravel.departure = record->departure;
            sTravel.recovered_state = TRUE;
            sTravel.state = STATE_REQUESTING;
            sTravel.semantic_queued = FALSE;
            sTravel.suspended = FALSE;
            return TRUE;
        }
        if (record->kind == COOP_GROUP_TRAVEL_SERVER_SCENE_READY
         || record->kind == COOP_GROUP_TRAVEL_SERVER_SCENE_MARKER_ACCEPTED)
        {
            /* Both replay forms require the untouched house. The accepted
             * marker may be replayed before SceneReady on a new ROM runtime. */
            sTravel.semantic = *record;
            sTravel.semantic.kind = COOP_GROUP_TRAVEL_CLIENT_SCENE_MARKER_REQUEST;
            sTravel.departure = record->departure;
            sTravel.recovered_state = TRUE;
            sTravel.state = record->kind == COOP_GROUP_TRAVEL_SERVER_SCENE_READY
                ? STATE_SCENE_MARKER_WAIT : STATE_SCENE_MARKER_ACKED;
            sTravel.scene_landed = FALSE;
            sTravel.scene_started = FALSE;
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
            HideVoteCountdown();
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
        {
            bool8 matches = record->request_id == sTravel.semantic.request_id
                && record->route == sTravel.semantic.route
                && record->departure == sTravel.semantic.departure
                && memcmp(record->proposal_id, sTravel.semantic.proposal_id, 16) == 0;
            if (matches && (sTravel.state == STATE_OFFER_DEFERRED
                         || sTravel.state == STATE_OFFER_READY))
            {
                sTravel.semantic.remaining_seconds = record->remaining_seconds;
                if (record->remaining_seconds != 0)
                    sTravel.vote_known = TRUE;
                sTravel.vote_deadline_frame = gMain.vblankCounter1 + record->remaining_seconds * 60;
            }
            return matches;
        }
        return FALSE;
    }
    if (record->kind == COOP_GROUP_TRAVEL_SERVER_ABORT)
    {
        if (IsStoryRoute(sTravel.semantic.route)
         && sTravel.scene_started)
            return FALSE;
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
        if (record->remaining_seconds != 0)
            sTravel.vote_known = TRUE;
        sTravel.vote_deadline_frame = gMain.vblankCounter1 + record->remaining_seconds * 60;
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
        HideVoteCountdown();
        sTravel.semantic = *record;
        sTravel.departure = record->departure;
        sTravel.state = STATE_COMMITTING;
        sTravel.commit_pending = TRUE;
        sTravel.semantic_queued = FALSE;
        sTravel.suspended = FALSE;
        return TryStagePendingCommit();
    }
    if (record->kind == COOP_GROUP_TRAVEL_SERVER_SCENE_READY)
    {
        if (sTravel.state == STATE_SCENE_MARKER_ACKED
         || sTravel.state == STATE_SCENE_COMPLETE_PENDING
         || sTravel.state == STATE_SCENE_COMPLETE_SENT)
            return memcmp(record->proposal_id, sTravel.semantic.proposal_id, 16) == 0;
        if (sTravel.state != STATE_REQUESTING && sTravel.state != STATE_ACCEPTED
         && sTravel.state != STATE_SCENE_MARKER_WAIT)
            return FALSE;
        if (sTravel.state == STATE_ACCEPTED
         && memcmp(record->proposal_id, sTravel.semantic.proposal_id, 16) != 0)
            return FALSE;
        HideVoteCountdown();
        sTravel.semantic = *record;
        sTravel.semantic.kind = COOP_GROUP_TRAVEL_CLIENT_SCENE_MARKER_REQUEST;
        sTravel.departure = record->departure;
        sTravel.state = STATE_SCENE_MARKER_WAIT;
        sTravel.scene_landed = FALSE;
        if (!sTravel.semantic_queued)
            sTravel.semantic_queued = SendSemantic();
        return TRUE;
    }
    if (record->kind == COOP_GROUP_TRAVEL_SERVER_COMPLETE
     && IsStoryRoute(record->route))
    {
        if (!sTravel.session_ready
         || (sTravel.state != STATE_SCENE_COMPLETE_PENDING
          && sTravel.state != STATE_SCENE_COMPLETE_SENT)
         || record->request_id != sTravel.semantic.request_id
         || record->departure != sTravel.semantic.departure
         || memcmp(record->proposal_id, sTravel.semantic.proposal_id, 16) != 0
         || !sTravel.scene_started || !sTravel.scene_landed
         || !StorySceneAtDestination())
            return FALSE;
        sTravel.last_story_complete = *record;
        sTravel.last_story_complete_valid = TRUE;
        ClearAndUnlock(FALSE);
        return TRUE;
    }
    if (record->kind == COOP_GROUP_TRAVEL_SERVER_SCENE_MARKER_ACCEPTED)
    {
        if (sTravel.state != STATE_SCENE_MARKER_WAIT
         && sTravel.state != STATE_SCENE_MARKER_ACKED
         && sTravel.state != STATE_SCENE_COMPLETE_PENDING
         && sTravel.state != STATE_SCENE_COMPLETE_SENT)
            return FALSE;
        if (memcmp(record->proposal_id, sTravel.semantic.proposal_id, 16) != 0)
            return FALSE;
        if (sTravel.state == STATE_SCENE_COMPLETE_PENDING
         || sTravel.state == STATE_SCENE_COMPLETE_SENT)
            return TRUE;
        sTravel.semantic_queued = FALSE;
        sTravel.state = STATE_SCENE_MARKER_ACKED;
        return TRUE;
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
    ShowVoteCountdown();
    if (sTravel.clear_pending)
    {
        (void)ReleaseTravelBoundary();
        if (!sTravel.controls_locked && !sTravel.script_objects_frozen)
            ClearAndUnlock(sTravel.clear_preserve_travel);
        return;
    }
    if (sTravel.state == STATE_COMMITTING && !TryStagePendingCommit())
        return;
    if (sTravel.state == STATE_SCENE_MARKER_ACKED && sTravel.scene_landed
     && StorySceneAtDestination())
    {
        sTravel.semantic.kind = COOP_GROUP_TRAVEL_CLIENT_SCENE_COMPLETE;
        sTravel.semantic_queued = FALSE;
        sTravel.state = STATE_SCENE_COMPLETE_PENDING;
    }
    if ((sTravel.state == STATE_SCENE_COMPLETE_PENDING
      || sTravel.state == STATE_SCENE_COMPLETE_SENT)
     && !sTravel.suspended && !sTravel.semantic_queued
     && sTravel.scene_landed && StorySceneAtDestination())
    {
        sTravel.semantic_queued = SendSemantic();
        if (sTravel.semantic_queued)
            sTravel.state = STATE_SCENE_COMPLETE_SENT;
    }
    if (sTravel.script_lock_handoff)
    {
        if (ScriptContext_IsEnabled() || !AcquireControlLock())
            return;
        sTravel.script_lock_handoff = FALSE;
    }
    if (sTravel.state == STATE_SCENE_MARKER_ACKED && !sTravel.scene_started
     && !sTravel.suspended && IsCurrentTravelSource(&sTravel.semantic)
     && AcquireControlLock())
    {
        sTravel.scene_started = TRUE;
        if (sTravel.semantic.route == COOP_GROUP_TRAVEL_ROUTE_BRINEY_HOUSE_DEWFORD)
            ScriptContext_SetupScript(Route104_MrBrineysHouse_EventScript_GroupVoyageStart);
        else if (sTravel.semantic.route == COOP_GROUP_TRAVEL_ROUTE_BILL_CINNABAR_ONE)
            ScriptContext_SetupScript(CinnabarIsland_EventScript_GroupBillAcceptedVoyageStart);
        else if (sTravel.semantic.route == COOP_GROUP_TRAVEL_ROUTE_BILL_ONE_CINNABAR)
        {
            VarSet(VAR_TEMP_1, gSaveBlock1Ptr->pos.y - 5);
            ScriptContext_SetupScript(gSaveBlock1Ptr->pos.x == 11
                ? OneIsland_PokemonCenter_1F_EventScript_GroupBillAcceptedReturnStage
                : OneIsland_PokemonCenter_1F_EventScript_GroupBillReturnStart);
        }
        return;
    }
    /* The landing script owns the boundary through checkpoint and receipt.
     * Releasing it here permits an off-map save that the server must reject. */
    if (sTravel.state == STATE_OFFER_DEFERRED && IsSafeOverworld())
    {
        if (!AcquireControlLock())
            return;
        sTravel.state = STATE_OFFER_READY;
        sTravel.offer_ui_started = TRUE;
        ScriptContext_SetupScript(EventScript_CoopGroupTravelOffer);
    }
    if ((sTravel.state == STATE_REQUESTING || sTravel.state == STATE_ACCEPTED
      || sTravel.state == STATE_SCENE_MARKER_WAIT)
     && !sTravel.suspended && AcquireControlLock() && !sTravel.semantic_queued)
        sTravel.semantic_queued = SendSemantic();
    if ((sTravel.state == STATE_CANCELING || sTravel.state == STATE_DECLINING)
     && !sTravel.semantic_queued && !sTravel.suspended)
        sTravel.semantic_queued = SendSemantic();
    if ((sTravel.state == STATE_CANCELING || sTravel.state == STATE_DECLINING
      || sTravel.suspended) && (sTravel.controls_locked || sTravel.script_objects_frozen))
        (void)ReleaseTravelBoundary();
    if (sTravel.state == STATE_COMMITTING && AtExactDestination()
     && (sTravel.semantic.departure == COOP_GROUP_TRAVEL_DEPARTURE_FLY
      || IsOutboundIslandRoute(sTravel.semantic.route)
      || IsIslandReturnRoute(sTravel.semantic.route)
      || IsHoennHarborRoute(sTravel.semantic.route)
      || IsSSTidalBoardingRoute(sTravel.semantic.route)
      || IsSSTidalRoute(sTravel.semantic.route)
      || IsBrineyRoute(sTravel.semantic.route)
      || IsSeagallopRoute(sTravel.semantic.route)
      || sTravel.semantic.departure == COOP_GROUP_TRAVEL_DEPARTURE_CABLE_CAR
      || (sTravel.semantic.route >= COOP_GROUP_TRAVEL_ROUTE_RETURN_GATE_ORIGINAL
       && sTravel.semantic.route <= COOP_GROUP_TRAVEL_ROUTE_RETURN_GATE_LATER
       && JohtoTravel_CommitAtReceptionGate())
      || JohtoTravel_TryCommitArrival()))
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
    if ((sTravel.state == STATE_SCENE_COMPLETE_PENDING
      || sTravel.state == STATE_SCENE_COMPLETE_SENT)
     && (!sTravel.scene_landed || !StorySceneAtDestination()))
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
    HideVoteCountdown();
    sTravel.semantic.remaining_seconds = 0;
    sTravel.vote_known = FALSE;
    sTravel.session_ready = FALSE;
    if (sTravel.state == STATE_IDLE)
        return;
    sTravel.suspended = TRUE;
    sTravel.semantic_queued = FALSE;
    /* Precommit waits must never strand the player offline.  Arrival states
     * retain their world-transition safety and reacquire ownership in Poll. */
    if (sTravel.state != STATE_COMMITTING && sTravel.state != STATE_APPLIED_PENDING
     && sTravel.state != STATE_APPLIED
     && sTravel.state != STATE_SCENE_COMPLETE_PENDING
     && sTravel.state != STATE_SCENE_COMPLETE_SENT
     && !(sTravel.state == STATE_SCENE_MARKER_ACKED && sTravel.scene_started))
        (void)ReleaseTravelBoundary();
}
bool8 CoopGroupTravel_IsManagingArrival(void) { return sTravel.state == STATE_COMMITTING; }
bool8 CoopGroupTravel_IsFirstBrineyReceiptPending(void)
{
    return IsStoryRoute(sTravel.semantic.route)
        && sTravel.scene_started && sTravel.scene_landed
        && (sTravel.state == STATE_SCENE_MARKER_ACKED
         || sTravel.state == STATE_SCENE_COMPLETE_PENDING
         || sTravel.state == STATE_SCENE_COMPLETE_SENT);
}

void Special_CoopGroupTravelFirstBrineySceneComplete(void)
{
    /* The script calls this only after the vanilla landing dialogue. It
     * records scene completion, not cloud save finalization. The launcher
     * must wait for a checkpoint-authorized finalized save before receipt. */
    gSpecialVar_Result = FALSE;
    if (sTravel.state == STATE_SCENE_MARKER_ACKED && sTravel.scene_started
     && sTravel.semantic.route == COOP_GROUP_TRAVEL_ROUTE_BRINEY_HOUSE_DEWFORD
     && !IsZero(sTravel.semantic.proposal_id, sizeof(sTravel.semantic.proposal_id)))
    {
        sTravel.scene_landed = TRUE;
        gSpecialVar_Result = TRUE;
    }
}

void Special_CoopGroupTravelBillSceneComplete(void)
{
    gSpecialVar_Result = FALSE;
    if (sTravel.state == STATE_SCENE_MARKER_ACKED && sTravel.scene_started
     && IsBillStoryRoute(sTravel.semantic.route)
     && !IsZero(sTravel.semantic.proposal_id, sizeof(sTravel.semantic.proposal_id))
     && StorySceneAtDestination())
    {
        sTravel.scene_landed = TRUE;
        gSpecialVar_Result = TRUE;
    }
}

void Special_CoopGroupTravelBillSceneRunning(void)
{
    gSpecialVar_Result = IsBillStoryRoute(sTravel.semantic.route)
        && sTravel.scene_started && sTravel.state == STATE_SCENE_MARKER_ACKED;
}

void Special_CoopGroupTravelFirstBrineyReceiptComplete(void)
{
    gSpecialVar_Result = sTravel.last_story_complete_valid
        && (sTravel.clear_pending || sTravel.state == STATE_IDLE);
}

void Special_CoopGroupTravelBegin(void)
{
    u8 departure = gSpecialVar_0x8005;
    if (gSpecialVar_0x8004 == COOP_GROUP_TRAVEL_ROUTE_GATE_ORIGINAL
     || gSpecialVar_0x8004 == COOP_GROUP_TRAVEL_ROUTE_GATE_LATER
     || gSpecialVar_0x8004 == COOP_GROUP_TRAVEL_ROUTE_RETURN_GATE_ORIGINAL
     || gSpecialVar_0x8004 == COOP_GROUP_TRAVEL_ROUTE_RETURN_GATE_LATER)
        departure = COOP_GROUP_TRAVEL_DEPARTURE_GATE;
    else if (gSpecialVar_0x8004 == COOP_GROUP_TRAVEL_ROUTE_RETURN_FERRY_ORIGINAL
          || gSpecialVar_0x8004 == COOP_GROUP_TRAVEL_ROUTE_RETURN_FERRY_LATER)
        departure = COOP_GROUP_TRAVEL_DEPARTURE_FERRY;
    else if (gSpecialVar_0x8004 == COOP_GROUP_TRAVEL_ROUTE_RETURN_TRAIN_ORIGINAL
          || gSpecialVar_0x8004 == COOP_GROUP_TRAVEL_ROUTE_RETURN_TRAIN_LATER)
        departure = COOP_GROUP_TRAVEL_DEPARTURE_TRAIN;
    gSpecialVar_Result = CoopGroupTravel_BeginFromScript(gSpecialVar_0x8004, departure);
}
void Special_CoopGroupTravelIsGrouped(void) { gSpecialVar_Result = IsGrouped(); }
void Special_CoopSeagallopRoute(void)
{
    gSpecialVar_Result = SeagallopRouteForEndpoints(gSpecialVar_0x8004, gSpecialVar_0x8006);
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
        if (offer.departure == COOP_GROUP_TRAVEL_DEPARTURE_FLY)
            GetMapNameGeneric(gStringVar1, CoopRegionMap_GroupFlyMapSection(offer.route));
        else if (offer.route >= COOP_GROUP_TRAVEL_ROUTE_OLIVINE_SOUTHERN_ISLAND)
        {
            const struct TravelWarp *warp = &sTravelWarps[offer.route];
            const struct MapHeader *mapHeader = Overworld_GetMapHeaderByGroupAndId(
                MAP_GROUP(warp->map), MAP_NUM(warp->map));
            if (mapHeader != NULL && mapHeader->regionMapSectionId != MAPSEC_NONE)
                GetMapNameGeneric(gStringVar1, mapHeader->regionMapSectionId);
            else
                StringCopy(gStringVar1, sNextPortText);
        }
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
    u8 era, destination;

    if (!RouteFields(route, &era, &destination))
        return FALSE;
    sTravel.semantic.route = route;
    return AtExactDestination();
}
void CoopGroupTravel_TestSetGrouped(bool8 grouped) { sTravel.test_grouped_set = TRUE; sTravel.test_grouped = grouped; }
void CoopGroupTravel_TestSetDeparture(u8 departure) { sTravel.departure = departure; }
const struct CoopGroupTravelRecord *CoopGroupTravel_TestRecord(void) { return &sTravel.semantic; }
void CoopGroupTravel_TestSeedSceneMarkerAccepted(const struct CoopGroupTravelRecord *record)
{
    sTravel.semantic = *record;
    sTravel.semantic.kind = COOP_GROUP_TRAVEL_CLIENT_SCENE_MARKER_REQUEST;
    sTravel.state = STATE_SCENE_MARKER_ACKED;
    sTravel.scene_landed = FALSE;
    sTravel.scene_started = FALSE;
    sTravel.semantic_queued = FALSE;
    sTravel.suspended = FALSE;
}
void CoopGroupTravel_TestSetSceneStarted(bool8 started) { sTravel.scene_started = started; }
void CoopGroupTravel_TestOwnControlLock(void) { LockPlayerFieldControls(); sTravel.controls_locked = TRUE; }
#endif
