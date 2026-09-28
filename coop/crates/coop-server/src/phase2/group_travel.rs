//! Authenticated, atomic UUID group invitations and regional travel.

use coop_cloud::group::{GroupTravelSceneMarkerRequest, GroupTravelSceneReceiptRequest};
use coop_cloud::{
    AcceptGroupInvitationRequest, AcceptGroupInvitationResponse, ApiVersion, CharacterId,
    CreateGroupInvitationRequest, CreateGroupInvitationResponse, Group, GroupId, GroupInvitationId,
    GroupInvitationView, GroupMemberView, GroupTravelAction, GroupTravelActionRequest,
    GroupTravelCommit, GroupTravelProposalId, GroupTravelProposalRequest,
    GroupTravelProposalStatus, GroupTravelProposalView, GroupView, LeaseFence, MAX_WORLD_REVISION,
    StoryTravelRecoveryAction, StoryTravelRecoveryActionRequest, StoryTravelRecoveryOutcome,
    StoryTravelRecoveryResolutionView, StoryTravelRecoveryView, UnixTimestampMillis,
};
use coop_protocol::{GroupTravelDeparture, RegionId, WorldZone};
use sha2::{Digest, Sha256};

use super::storage::{
    GROUP_IDEMPOTENCY_TTL_MS, GROUP_INVITATION_TTL_MS, GroupIdempotencyRecord,
    GroupIdempotencyResponse, GroupInvitationRecord, GroupRecord, GroupStatus,
    GroupTravelProposalIdempotencyRecord, GroupTravelProposalRecord, MAX_GROUP_IDEMPOTENCY,
    MAX_GROUP_INVITATIONS, Store, StoryRecoveryResolution, StorySceneMarker, StorySceneReceipt,
};
use super::{AuthenticatedActor, Phase2Error};

const OP_CREATE: &str = "group_invitation_create_v1";
const OP_ACCEPT: &str = "group_invitation_accept_v1";
const MAX_INVITATIONS_PER_SENDER_PER_MINUTE: usize = 3;
const OP_PROPOSE_TRAVEL: &str = "group_travel_propose_v1";
const OP_TRAVEL_ACTION: &str = "group_travel_action_v1";
const MAX_ID_CANDIDATES: usize = 8;
const MAX_TRAVEL_PROPOSALS: usize = 4_096;
const MAX_STORY_TRAVEL_ARCHIVES_PER_MEMBER: usize = 8;
const TRAVEL_PROPOSAL_TTL_MS: u64 = 30_000;
const MAX_TRAVEL_CREATE_RECEIPTS: usize = 1_024;
const MAX_TRAVEL_LIFECYCLE_RECEIPTS: usize = MAX_TRAVEL_PROPOSALS - MAX_TRAVEL_CREATE_RECEIPTS;
const TRAVEL_PROPOSAL_REPLAY_TTL_MS: u64 = GROUP_IDEMPOTENCY_TTL_MS;
const STORY_SCENE_RECEIPT_TTL_MS: u64 = 10 * 60_000;
const FIRST_BRINEY_ROUTE_ID: &str = "HOENN:BRINEY_HOUSE_DEWFORD_FERRY";
const BILL_CINNABAR_ONE_ROUTE_ID: &str = "KANTO:CINNABAR_ONE_BILL_FERRY";
const BILL_ONE_CINNABAR_ROUTE_ID: &str = "SEVII:ONE_CINNABAR_BILL_FERRY";

fn is_story_route(id: &str) -> bool {
    matches!(
        id,
        FIRST_BRINEY_ROUTE_ID | BILL_CINNABAR_ONE_ROUTE_ID | BILL_ONE_CINNABAR_ROUTE_ID
    )
}

#[derive(Clone)]
struct RouteDefinition {
    id: &'static str,
    source: WorldZone,
    destination: WorldZone,
    /// Fly can be selected while the two players are on different maps.  The
    /// group and both lease-protected character revisions still fence the
    /// proposal, but the source map is intentionally not a wildcard in the
    /// public view: it is recorded as the group's current zone.
    source_any: bool,
    minimum_badges: u8,
    minimum_story_checkpoint: u32,
}

fn route(
    id: &'static str,
    source: WorldZone,
    destination: WorldZone,
    badges: u8,
    story: u32,
) -> RouteDefinition {
    RouteDefinition {
        id,
        source,
        destination,
        source_any: false,
        minimum_badges: badges,
        minimum_story_checkpoint: story,
    }
}

fn fly_route(id: &'static str, destination: WorldZone) -> RouteDefinition {
    RouteDefinition {
        id,
        // Retain a concrete source for catalog and progress-region purposes;
        // source_any makes the actual proposal source the live group zone.
        source: WorldZone::new(RegionId::Hoenn, "LITTLEROOT_TOWN", 1).expect("map catalog"),
        destination,
        source_any: true,
        minimum_badges: 0,
        minimum_story_checkpoint: 0,
    }
}

fn consent_route_catalog() -> Vec<RouteDefinition> {
    let goldenrod =
        || WorldZone::new(RegionId::Johto, "GOLDENROD_CITY_TRAIN_STATION", 1).expect("map catalog");
    let olivine =
        || WorldZone::new(RegionId::Johto, "OLIVINE_CITY_PORT_INSIDE", 1).expect("map catalog");
    let reception = || WorldZone::new(RegionId::Kanto, "RECEPTION_GATE", 1).expect("map catalog");
    let original_train = || {
        WorldZone::new(
            RegionId::Kanto,
            "KANTO_ORIGINAL_SAFFRON_CITY_TRAIN_STATION",
            1,
        )
        .expect("map catalog")
    };
    let later_train = || {
        WorldZone::new(RegionId::Kanto, "KANTO_LATER_SAFFRON_CITY_TRAIN_STATION", 1)
            .expect("map catalog")
    };
    let original_ferry = || {
        WorldZone::new(
            RegionId::Kanto,
            "KANTO_ORIGINAL_VERMILION_CITY_PORT_INSIDE",
            1,
        )
        .expect("map catalog")
    };
    let later_ferry = || {
        WorldZone::new(RegionId::Kanto, "KANTO_LATER_VERMILION_CITY_PORT_INSIDE", 1)
            .expect("map catalog")
    };
    let original_route22 = || WorldZone::new(RegionId::Kanto, "ROUTE22", 1).expect("map catalog");
    let later_route22 =
        || WorldZone::new(RegionId::Kanto, "KANTO_LATER_ROUTE22", 1).expect("map catalog");
    let southern_island =
        || WorldZone::new(RegionId::Hoenn, "SOUTHERN_ISLAND_EXTERIOR", 1).expect("map catalog");
    let birth_island =
        || WorldZone::new(RegionId::Hoenn, "BIRTH_ISLAND_EXTERIOR", 1).expect("map catalog");
    let faraway_island =
        || WorldZone::new(RegionId::Hoenn, "FARAWAY_ISLAND_ENTRANCE", 1).expect("map catalog");
    let battle_frontier =
        || WorldZone::new(RegionId::Hoenn, "BATTLE_FRONTIER_OUTSIDE_WEST", 1).expect("map catalog");
    let birth_harbor =
        || WorldZone::new(RegionId::Hoenn, "BIRTH_ISLAND_HARBOR", 1).expect("map catalog");
    let lilycove_harbor =
        || WorldZone::new(RegionId::Hoenn, "LILYCOVE_CITY_HARBOR", 1).expect("map catalog");
    let slateport_harbor =
        || WorldZone::new(RegionId::Hoenn, "SLATEPORT_CITY_HARBOR", 1).expect("map catalog");
    let navel_harbor =
        || WorldZone::new(RegionId::Hoenn, "NAVEL_ROCK_HARBOR", 1).expect("map catalog");
    let ss_tidal_corridor =
        || WorldZone::new(RegionId::Hoenn, "SS_TIDAL_CORRIDOR", 1).expect("map catalog");
    let dewford = || WorldZone::new(RegionId::Hoenn, "DEWFORD_TOWN", 1).expect("map catalog");
    let briney_house =
        || WorldZone::new(RegionId::Hoenn, "ROUTE104_MR_BRINEYS_HOUSE", 1).expect("map catalog");
    let route109 = || WorldZone::new(RegionId::Hoenn, "ROUTE109", 1).expect("map catalog");
    let route112_cable_car =
        || WorldZone::new(RegionId::Hoenn, "ROUTE112_CABLE_CAR_STATION", 1).expect("map catalog");
    let mt_chimney_cable_car =
        || WorldZone::new(RegionId::Hoenn, "MT_CHIMNEY_CABLE_CAR_STATION", 1).expect("map catalog");
    let mut catalog = vec![
        route(
            "HOENN:ROUTE112_MT_CHIMNEY_CABLE_CAR",
            route112_cable_car(),
            mt_chimney_cable_car(),
            0,
            0,
        ),
        route(
            "HOENN:MT_CHIMNEY_ROUTE112_CABLE_CAR",
            mt_chimney_cable_car(),
            route112_cable_car(),
            0,
            0,
        ),
        route(
            "JOHTO:GOLDENROD_KANTO_ORIGINAL_TRAIN",
            goldenrod(),
            original_train(),
            0,
            0,
        ),
        route(
            "JOHTO:GOLDENROD_KANTO_LATER_TRAIN",
            goldenrod(),
            later_train(),
            0,
            0,
        ),
        route(
            "JOHTO:OLIVINE_KANTO_ORIGINAL_FERRY",
            olivine(),
            original_ferry(),
            0,
            0,
        ),
        route(
            "JOHTO:OLIVINE_KANTO_LATER_FERRY",
            olivine(),
            later_ferry(),
            0,
            0,
        ),
        route(
            "KANTO:RECEPTION_GATE_KANTO_ORIGINAL_ROUTE22",
            reception(),
            original_route22(),
            0,
            0,
        ),
        route(
            "KANTO:RECEPTION_GATE_KANTO_LATER_ROUTE22",
            reception(),
            later_route22(),
            0,
            0,
        ),
        route(
            "KANTO:ORIGINAL_VERMILION_JOHTO_FERRY",
            original_ferry(),
            olivine(),
            0,
            0,
        ),
        route(
            "KANTO:LATER_VERMILION_JOHTO_FERRY",
            later_ferry(),
            olivine(),
            0,
            0,
        ),
        route(
            "KANTO:ORIGINAL_SAFFRON_JOHTO_TRAIN",
            original_train(),
            goldenrod(),
            0,
            0,
        ),
        route(
            "KANTO:LATER_SAFFRON_JOHTO_TRAIN",
            later_train(),
            goldenrod(),
            0,
            0,
        ),
        route(
            "KANTO:ORIGINAL_ROUTE22_JOHTO_ROUTE22",
            original_route22(),
            reception(),
            0,
            0,
        ),
        route(
            "KANTO:LATER_ROUTE22_JOHTO_ROUTE22",
            later_route22(),
            reception(),
            0,
            0,
        ),
        fly_route(
            "HOENN:FLY_LITTLEROOT",
            WorldZone::new(RegionId::Hoenn, "LITTLEROOT_TOWN", 1).expect("map catalog"),
        ),
        fly_route(
            "JOHTO:FLY_NEW_BARK_TOWN",
            WorldZone::new(RegionId::Johto, "NEW_BARK_TOWN", 1).expect("map catalog"),
        ),
        fly_route(
            "JOHTO:FLY_CHERRYGROVE_CITY",
            WorldZone::new(RegionId::Johto, "CHERRYGROVE_CITY", 1).expect("map catalog"),
        ),
        fly_route(
            "JOHTO:FLY_VIOLET_CITY",
            WorldZone::new(RegionId::Johto, "VIOLET_CITY", 1).expect("map catalog"),
        ),
        fly_route(
            "JOHTO:FLY_AZALEA_TOWN",
            WorldZone::new(RegionId::Johto, "AZALEA_TOWN", 1).expect("map catalog"),
        ),
        fly_route(
            "JOHTO:FLY_GOLDENROD_CITY",
            WorldZone::new(RegionId::Johto, "GOLDENROD_CITY", 1).expect("map catalog"),
        ),
        fly_route(
            "JOHTO:FLY_ECRUTEAK_CITY",
            WorldZone::new(RegionId::Johto, "ECRUTEAK_CITY", 1).expect("map catalog"),
        ),
        fly_route(
            "JOHTO:FLY_OLIVINE_CITY",
            WorldZone::new(RegionId::Johto, "OLIVINE_CITY", 1).expect("map catalog"),
        ),
        fly_route(
            "JOHTO:FLY_CIANWOOD_CITY",
            WorldZone::new(RegionId::Johto, "CIANWOOD_CITY", 1).expect("map catalog"),
        ),
        fly_route(
            "JOHTO:FLY_MAHOGANYTOWN",
            WorldZone::new(RegionId::Johto, "MAHOGANYTOWN", 1).expect("map catalog"),
        ),
        fly_route(
            "JOHTO:FLY_BLACKTHORN_CITY",
            WorldZone::new(RegionId::Johto, "BLACKTHORN_CITY", 1).expect("map catalog"),
        ),
        fly_route(
            "HOENN:FLY_OLDALE_TOWN",
            WorldZone::new(RegionId::Hoenn, "OLDALE_TOWN", 1).expect("map catalog"),
        ),
        fly_route(
            "HOENN:FLY_DEWFORD_TOWN",
            WorldZone::new(RegionId::Hoenn, "DEWFORD_TOWN", 1).expect("map catalog"),
        ),
        fly_route(
            "HOENN:FLY_LAVARIDGE_TOWN",
            WorldZone::new(RegionId::Hoenn, "LAVARIDGE_TOWN", 1).expect("map catalog"),
        ),
        fly_route(
            "HOENN:FLY_FALLARBOR_TOWN",
            WorldZone::new(RegionId::Hoenn, "FALLARBOR_TOWN", 1).expect("map catalog"),
        ),
        fly_route(
            "HOENN:FLY_VERDANTURF_TOWN",
            WorldZone::new(RegionId::Hoenn, "VERDANTURF_TOWN", 1).expect("map catalog"),
        ),
        fly_route(
            "HOENN:FLY_PACIFIDLOG_TOWN",
            WorldZone::new(RegionId::Hoenn, "PACIFIDLOG_TOWN", 1).expect("map catalog"),
        ),
        fly_route(
            "HOENN:FLY_PETALBURG_CITY",
            WorldZone::new(RegionId::Hoenn, "PETALBURG_CITY", 1).expect("map catalog"),
        ),
        fly_route(
            "HOENN:FLY_SLATEPORT_CITY",
            WorldZone::new(RegionId::Hoenn, "SLATEPORT_CITY", 1).expect("map catalog"),
        ),
        fly_route(
            "HOENN:FLY_MAUVILLE_CITY",
            WorldZone::new(RegionId::Hoenn, "MAUVILLE_CITY", 1).expect("map catalog"),
        ),
        fly_route(
            "HOENN:FLY_RUSTBORO_CITY",
            WorldZone::new(RegionId::Hoenn, "RUSTBORO_CITY", 1).expect("map catalog"),
        ),
        fly_route(
            "HOENN:FLY_FORTREE_CITY",
            WorldZone::new(RegionId::Hoenn, "FORTREE_CITY", 1).expect("map catalog"),
        ),
        fly_route(
            "HOENN:FLY_LILYCOVE_CITY",
            WorldZone::new(RegionId::Hoenn, "LILYCOVE_CITY", 1).expect("map catalog"),
        ),
        fly_route(
            "HOENN:FLY_MOSSDEEP_CITY",
            WorldZone::new(RegionId::Hoenn, "MOSSDEEP_CITY", 1).expect("map catalog"),
        ),
        fly_route(
            "HOENN:FLY_SOOTOPOLIS_CITY",
            WorldZone::new(RegionId::Hoenn, "SOOTOPOLIS_CITY", 1).expect("map catalog"),
        ),
        fly_route(
            "KANTO:FLY_PALLET_TOWN",
            WorldZone::new(RegionId::Kanto, "PALLET_TOWN", 1).expect("map catalog"),
        ),
        fly_route(
            "KANTO:FLY_VIRIDIAN_CITY",
            WorldZone::new(RegionId::Kanto, "VIRIDIAN_CITY", 1).expect("map catalog"),
        ),
        fly_route(
            "KANTO:FLY_PEWTER_CITY",
            WorldZone::new(RegionId::Kanto, "PEWTER_CITY", 1).expect("map catalog"),
        ),
        fly_route(
            "KANTO:FLY_CERULEAN_CITY",
            WorldZone::new(RegionId::Kanto, "CERULEAN_CITY", 1).expect("map catalog"),
        ),
        fly_route(
            "KANTO:FLY_LAVENDER_TOWN",
            WorldZone::new(RegionId::Kanto, "LAVENDER_TOWN", 1).expect("map catalog"),
        ),
        fly_route(
            "KANTO:FLY_VERMILION_CITY",
            WorldZone::new(RegionId::Kanto, "VERMILION_CITY", 1).expect("map catalog"),
        ),
        fly_route(
            "KANTO:FLY_CELADON_CITY",
            WorldZone::new(RegionId::Kanto, "CELADON_CITY", 1).expect("map catalog"),
        ),
        fly_route(
            "KANTO:FLY_FUCHSIA_CITY",
            WorldZone::new(RegionId::Kanto, "FUCHSIA_CITY", 1).expect("map catalog"),
        ),
        fly_route(
            "KANTO:FLY_CINNABAR_ISLAND",
            WorldZone::new(RegionId::Kanto, "CINNABAR_ISLAND", 1).expect("map catalog"),
        ),
        fly_route(
            "KANTO:FLY_INDIGO_PLATEAU",
            WorldZone::new(RegionId::Kanto, "INDIGO_PLATEAU_EXTERIOR", 1).expect("map catalog"),
        ),
        fly_route(
            "KANTO:FLY_SAFFRON_CITY",
            WorldZone::new(RegionId::Kanto, "SAFFRON_CITY", 1).expect("map catalog"),
        ),
        fly_route(
            "KANTO_LATER:FLY_PALLET_TOWN",
            WorldZone::new(RegionId::Kanto, "KANTO_LATER_PALLET_TOWN", 1).expect("map catalog"),
        ),
        fly_route(
            "KANTO_LATER:FLY_VIRIDIAN_CITY",
            WorldZone::new(RegionId::Kanto, "KANTO_LATER_VIRIDIAN_CITY", 1).expect("map catalog"),
        ),
        fly_route(
            "KANTO_LATER:FLY_PEWTER_CITY",
            WorldZone::new(RegionId::Kanto, "KANTO_LATER_PEWTER_CITY", 1).expect("map catalog"),
        ),
        fly_route(
            "KANTO_LATER:FLY_CERULEAN_CITY",
            WorldZone::new(RegionId::Kanto, "KANTO_LATER_CERULEAN_CITY", 1).expect("map catalog"),
        ),
        fly_route(
            "KANTO_LATER:FLY_LAVENDER_TOWN",
            WorldZone::new(RegionId::Kanto, "KANTO_LATER_LAVENDER_TOWN", 1).expect("map catalog"),
        ),
        fly_route(
            "KANTO_LATER:FLY_VERMILION_CITY",
            WorldZone::new(RegionId::Kanto, "KANTO_LATER_VERMILION_CITY", 1).expect("map catalog"),
        ),
        fly_route(
            "KANTO_LATER:FLY_CELADON_CITY",
            WorldZone::new(RegionId::Kanto, "KANTO_LATER_CELADON_CITY", 1).expect("map catalog"),
        ),
        fly_route(
            "KANTO_LATER:FLY_FUCHSIA_CITY",
            WorldZone::new(RegionId::Kanto, "KANTO_LATER_FUCHSIA_CITY", 1).expect("map catalog"),
        ),
        fly_route(
            "KANTO_LATER:FLY_SAFFRON_CITY",
            WorldZone::new(RegionId::Kanto, "KANTO_LATER_SAFFRON_CITY", 1).expect("map catalog"),
        ),
        fly_route(
            "KANTO_LATER:FLY_CINNABAR_ISLAND",
            WorldZone::new(RegionId::Kanto, "KANTO_LATER_CINNABAR_ISLAND", 1).expect("map catalog"),
        ),
        fly_route(
            "SEVII:FLY_ONE_ISLAND",
            WorldZone::new(RegionId::Sevii, "ONE_ISLAND", 1).expect("map catalog"),
        ),
        fly_route(
            "SEVII:FLY_TWO_ISLAND",
            WorldZone::new(RegionId::Sevii, "TWO_ISLAND", 1).expect("map catalog"),
        ),
        fly_route(
            "SEVII:FLY_THREE_ISLAND",
            WorldZone::new(RegionId::Sevii, "THREE_ISLAND", 1).expect("map catalog"),
        ),
        fly_route(
            "SEVII:FLY_FOUR_ISLAND",
            WorldZone::new(RegionId::Sevii, "FOUR_ISLAND", 1).expect("map catalog"),
        ),
        fly_route(
            "SEVII:FLY_FIVE_ISLAND",
            WorldZone::new(RegionId::Sevii, "FIVE_ISLAND", 1).expect("map catalog"),
        ),
        fly_route(
            "SEVII:FLY_SEVEN_ISLAND",
            WorldZone::new(RegionId::Sevii, "SEVEN_ISLAND", 1).expect("map catalog"),
        ),
        fly_route(
            "SEVII:FLY_SIX_ISLAND",
            WorldZone::new(RegionId::Sevii, "SIX_ISLAND", 1).expect("map catalog"),
        ),
        fly_route(
            "KANTO:FLY_ROUTE_4_POKECENTER",
            WorldZone::new(RegionId::Kanto, "ROUTE4", 1).expect("map catalog"),
        ),
        fly_route(
            "KANTO:FLY_ROUTE_10_POKECENTER",
            WorldZone::new(RegionId::Kanto, "ROUTE10", 1).expect("map catalog"),
        ),
        fly_route(
            "HOENN:FLY_EVER_GRANDE_CITY_CENTER",
            WorldZone::new(RegionId::Hoenn, "EVER_GRANDE_CITY", 1).expect("map catalog"),
        ),
        fly_route(
            "HOENN:FLY_EVER_GRANDE_CITY_LEAGUE",
            WorldZone::new(RegionId::Hoenn, "EVER_GRANDE_CITY", 1).expect("map catalog"),
        ),
        fly_route(
            "HOENN:FLY_BATTLE_FRONTIER",
            WorldZone::new(RegionId::Hoenn, "BATTLE_FRONTIER_OUTSIDE_EAST", 1)
                .expect("map catalog"),
        ),
        route(
            "JOHTO:OLIVINE_SOUTHERN_ISLAND_FERRY",
            olivine(),
            southern_island(),
            0,
            0,
        ),
        route(
            "JOHTO:OLIVINE_BIRTH_ISLAND_FERRY",
            olivine(),
            birth_island(),
            0,
            0,
        ),
        route(
            "JOHTO:OLIVINE_FARAWAY_ISLAND_FERRY",
            olivine(),
            faraway_island(),
            0,
            0,
        ),
        route(
            "JOHTO:OLIVINE_BATTLE_FRONTIER_FERRY",
            olivine(),
            battle_frontier(),
            0,
            0,
        ),
        route(
            "KANTO_LATER:VERMILION_SOUTHERN_ISLAND_FERRY",
            later_ferry(),
            southern_island(),
            0,
            0,
        ),
        route(
            "KANTO_LATER:VERMILION_BIRTH_ISLAND_FERRY",
            later_ferry(),
            birth_island(),
            0,
            0,
        ),
        route(
            "KANTO_LATER:VERMILION_FARAWAY_ISLAND_FERRY",
            later_ferry(),
            faraway_island(),
            0,
            0,
        ),
        route(
            "KANTO_LATER:VERMILION_BATTLE_FRONTIER_FERRY",
            later_ferry(),
            battle_frontier(),
            0,
            0,
        ),
        route(
            "HOENN:SOUTHERN_ISLAND_LILYCOVE_FERRY",
            southern_island(),
            lilycove_harbor(),
            0,
            0,
        ),
        route(
            "HOENN:BIRTH_ISLAND_LILYCOVE_FERRY",
            birth_harbor(),
            lilycove_harbor(),
            0,
            0,
        ),
        route(
            "HOENN:FARAWAY_ISLAND_LILYCOVE_FERRY",
            faraway_island(),
            lilycove_harbor(),
            0,
            0,
        ),
        route(
            "HOENN:BATTLE_FRONTIER_SLATEPORT_FERRY",
            battle_frontier(),
            slateport_harbor(),
            0,
            0,
        ),
        route(
            "HOENN:BATTLE_FRONTIER_LILYCOVE_FERRY",
            battle_frontier(),
            lilycove_harbor(),
            0,
            0,
        ),
        route(
            "HOENN:LILYCOVE_SOUTHERN_ISLAND_FERRY",
            lilycove_harbor(),
            southern_island(),
            0,
            0,
        ),
        route(
            "HOENN:LILYCOVE_NAVEL_ROCK_FERRY",
            lilycove_harbor(),
            navel_harbor(),
            0,
            0,
        ),
        route(
            "HOENN:LILYCOVE_BIRTH_ISLAND_FERRY",
            lilycove_harbor(),
            birth_harbor(),
            0,
            0,
        ),
        route(
            "HOENN:LILYCOVE_FARAWAY_ISLAND_FERRY",
            lilycove_harbor(),
            faraway_island(),
            0,
            0,
        ),
        route(
            "HOENN:LILYCOVE_BATTLE_FRONTIER_FERRY",
            lilycove_harbor(),
            battle_frontier(),
            0,
            0,
        ),
        route(
            "HOENN:SLATEPORT_BATTLE_FRONTIER_FERRY",
            slateport_harbor(),
            battle_frontier(),
            0,
            0,
        ),
        route(
            "HOENN:NAVEL_ROCK_LILYCOVE_FERRY",
            navel_harbor(),
            lilycove_harbor(),
            0,
            0,
        ),
        route(
            "HOENN:SLATEPORT_SS_TIDAL_BOARD_FERRY",
            slateport_harbor(),
            ss_tidal_corridor(),
            0,
            0,
        ),
        route(
            "HOENN:LILYCOVE_SS_TIDAL_BOARD_FERRY",
            lilycove_harbor(),
            ss_tidal_corridor(),
            0,
            0,
        ),
        route(
            "HOENN:SS_TIDAL_LILYCOVE_EXIT_FERRY",
            ss_tidal_corridor(),
            lilycove_harbor(),
            0,
            0,
        ),
        route(
            "HOENN:SS_TIDAL_SLATEPORT_EXIT_FERRY",
            ss_tidal_corridor(),
            slateport_harbor(),
            0,
            0,
        ),
        route(
            "HOENN:DEWFORD_BRINEY_HOUSE_FERRY",
            dewford(),
            briney_house(),
            0,
            0,
        ),
        route("HOENN:DEWFORD_ROUTE109_FERRY", dewford(), route109(), 0, 0),
        route("HOENN:ROUTE109_DEWFORD_FERRY", route109(), dewford(), 0, 0),
    ];
    const SEAGALLOP_ROUTES: [(&str, usize, usize); 60] = [
        ("SEAGALLOP:VERMILION_ONE_FERRY", 0, 1),
        ("SEAGALLOP:VERMILION_TWO_FERRY", 0, 2),
        ("SEAGALLOP:VERMILION_THREE_FERRY", 0, 3),
        ("SEAGALLOP:VERMILION_FOUR_FERRY", 0, 4),
        ("SEAGALLOP:VERMILION_FIVE_FERRY", 0, 5),
        ("SEAGALLOP:VERMILION_SIX_FERRY", 0, 6),
        ("SEAGALLOP:VERMILION_SEVEN_FERRY", 0, 7),
        ("SEAGALLOP:ONE_VERMILION_FERRY", 1, 0),
        ("SEAGALLOP:ONE_TWO_FERRY", 1, 2),
        ("SEAGALLOP:ONE_THREE_FERRY", 1, 3),
        ("SEAGALLOP:ONE_FOUR_FERRY", 1, 4),
        ("SEAGALLOP:ONE_FIVE_FERRY", 1, 5),
        ("SEAGALLOP:ONE_SIX_FERRY", 1, 6),
        ("SEAGALLOP:ONE_SEVEN_FERRY", 1, 7),
        ("SEAGALLOP:TWO_VERMILION_FERRY", 2, 0),
        ("SEAGALLOP:TWO_ONE_FERRY", 2, 1),
        ("SEAGALLOP:TWO_THREE_FERRY", 2, 3),
        ("SEAGALLOP:TWO_FOUR_FERRY", 2, 4),
        ("SEAGALLOP:TWO_FIVE_FERRY", 2, 5),
        ("SEAGALLOP:TWO_SIX_FERRY", 2, 6),
        ("SEAGALLOP:TWO_SEVEN_FERRY", 2, 7),
        ("SEAGALLOP:THREE_VERMILION_FERRY", 3, 0),
        ("SEAGALLOP:THREE_ONE_FERRY", 3, 1),
        ("SEAGALLOP:THREE_TWO_FERRY", 3, 2),
        ("SEAGALLOP:THREE_FOUR_FERRY", 3, 4),
        ("SEAGALLOP:THREE_FIVE_FERRY", 3, 5),
        ("SEAGALLOP:THREE_SIX_FERRY", 3, 6),
        ("SEAGALLOP:THREE_SEVEN_FERRY", 3, 7),
        ("SEAGALLOP:FOUR_VERMILION_FERRY", 4, 0),
        ("SEAGALLOP:FOUR_ONE_FERRY", 4, 1),
        ("SEAGALLOP:FOUR_TWO_FERRY", 4, 2),
        ("SEAGALLOP:FOUR_THREE_FERRY", 4, 3),
        ("SEAGALLOP:FOUR_FIVE_FERRY", 4, 5),
        ("SEAGALLOP:FOUR_SIX_FERRY", 4, 6),
        ("SEAGALLOP:FOUR_SEVEN_FERRY", 4, 7),
        ("SEAGALLOP:FIVE_VERMILION_FERRY", 5, 0),
        ("SEAGALLOP:FIVE_ONE_FERRY", 5, 1),
        ("SEAGALLOP:FIVE_TWO_FERRY", 5, 2),
        ("SEAGALLOP:FIVE_THREE_FERRY", 5, 3),
        ("SEAGALLOP:FIVE_FOUR_FERRY", 5, 4),
        ("SEAGALLOP:FIVE_SIX_FERRY", 5, 6),
        ("SEAGALLOP:FIVE_SEVEN_FERRY", 5, 7),
        ("SEAGALLOP:SIX_VERMILION_FERRY", 6, 0),
        ("SEAGALLOP:SIX_ONE_FERRY", 6, 1),
        ("SEAGALLOP:SIX_TWO_FERRY", 6, 2),
        ("SEAGALLOP:SIX_THREE_FERRY", 6, 3),
        ("SEAGALLOP:SIX_FOUR_FERRY", 6, 4),
        ("SEAGALLOP:SIX_FIVE_FERRY", 6, 5),
        ("SEAGALLOP:SIX_SEVEN_FERRY", 6, 7),
        ("SEAGALLOP:SEVEN_VERMILION_FERRY", 7, 0),
        ("SEAGALLOP:SEVEN_ONE_FERRY", 7, 1),
        ("SEAGALLOP:SEVEN_TWO_FERRY", 7, 2),
        ("SEAGALLOP:SEVEN_THREE_FERRY", 7, 3),
        ("SEAGALLOP:SEVEN_FOUR_FERRY", 7, 4),
        ("SEAGALLOP:SEVEN_FIVE_FERRY", 7, 5),
        ("SEAGALLOP:SEVEN_SIX_FERRY", 7, 6),
        ("SEAGALLOP:VERMILION_NAVEL_FERRY", 0, 8),
        ("SEAGALLOP:NAVEL_VERMILION_FERRY", 8, 0),
        ("SEAGALLOP:VERMILION_BIRTH_FERRY", 0, 9),
        ("SEAGALLOP:BIRTH_VERMILION_FERRY", 9, 0),
    ];
    const SEAGALLOP_MAPS: [(RegionId, &str); 10] = [
        (RegionId::Kanto, "VERMILION_CITY"),
        (RegionId::Sevii, "ONE_ISLAND_HARBOR"),
        (RegionId::Sevii, "TWO_ISLAND_HARBOR"),
        (RegionId::Sevii, "THREE_ISLAND_HARBOR"),
        (RegionId::Sevii, "FOUR_ISLAND_HARBOR"),
        (RegionId::Sevii, "FIVE_ISLAND_HARBOR"),
        (RegionId::Sevii, "SIX_ISLAND_HARBOR"),
        (RegionId::Sevii, "SEVEN_ISLAND_HARBOR"),
        (RegionId::Sevii, "NAVEL_ROCK_HARBOR_FRLG"),
        (RegionId::Sevii, "BIRTH_ISLAND_HARBOR_FRLG"),
    ];
    for (id, source, destination) in SEAGALLOP_ROUTES {
        let (source_region, source_map) = SEAGALLOP_MAPS[source];
        let (destination_region, destination_map) = SEAGALLOP_MAPS[destination];
        catalog.push(route(
            id,
            WorldZone::new(source_region, source_map, 1).expect("map catalog"),
            WorldZone::new(destination_region, destination_map, 1).expect("map catalog"),
            0,
            0,
        ));
    }
    catalog.push(first_briney_story_route());
    catalog.push(bill_cinnabar_one_story_route());
    catalog.push(bill_one_cinnabar_story_route());
    catalog
}

fn consent_route_definition(id: &str) -> Result<RouteDefinition, Phase2Error> {
    consent_route_catalog()
        .into_iter()
        .find(|route| route.id == id)
        .ok_or(Phase2Error::Forbidden)
}

// This route commits only after both scene markers and finalized save receipts.
fn first_briney_story_route() -> RouteDefinition {
    route(
        FIRST_BRINEY_ROUTE_ID,
        WorldZone::new(RegionId::Hoenn, "ROUTE104_MR_BRINEYS_HOUSE", 1).expect("map catalog"),
        WorldZone::new(RegionId::Hoenn, "DEWFORD_TOWN", 1).expect("map catalog"),
        0,
        0,
    )
}

fn bill_cinnabar_one_story_route() -> RouteDefinition {
    route(
        BILL_CINNABAR_ONE_ROUTE_ID,
        WorldZone::new(RegionId::Kanto, "CINNABAR_ISLAND", 1).expect("map catalog"),
        WorldZone::new(RegionId::Sevii, "ONE_ISLAND_POKEMON_CENTER_1F", 1).expect("map catalog"),
        0,
        0,
    )
}

fn bill_one_cinnabar_story_route() -> RouteDefinition {
    route(
        BILL_ONE_CINNABAR_ROUTE_ID,
        WorldZone::new(RegionId::Sevii, "ONE_ISLAND_POKEMON_CENTER_1F", 1).expect("map catalog"),
        WorldZone::new(RegionId::Kanto, "CINNABAR_ISLAND", 1).expect("map catalog"),
        0,
        0,
    )
}

fn story_route_definition(id: &str) -> Option<RouteDefinition> {
    match id {
        FIRST_BRINEY_ROUTE_ID => Some(first_briney_story_route()),
        BILL_CINNABAR_ONE_ROUTE_ID => Some(bill_cinnabar_one_story_route()),
        BILL_ONE_CINNABAR_ROUTE_ID => Some(bill_one_cinnabar_story_route()),
        _ => None,
    }
}

fn story_save_matches_route(id: &str, save: &coop_save::ValidatedSave) -> bool {
    match id {
        FIRST_BRINEY_ROUTE_ID => save
            .briney_voyage_evidence()
            .is_first_voyage_post_scene_at(0, 11),
        BILL_CINNABAR_ONE_ROUTE_ID => save.bill_voyage_evidence().is_cinnabar_to_one_post_scene(),
        BILL_ONE_CINNABAR_ROUTE_ID => save.bill_voyage_evidence().is_one_to_cinnabar_post_scene(),
        _ => false,
    }
}

fn departure_matches_route(route: &RouteDefinition, departure: GroupTravelDeparture) -> bool {
    if route.id.ends_with("_TRAIN") {
        departure == GroupTravelDeparture::Train
    } else if route.id.ends_with("_FERRY") {
        departure == GroupTravelDeparture::Ferry
            || (matches!(
                route.id,
                "JOHTO:OLIVINE_KANTO_ORIGINAL_FERRY" | "JOHTO:OLIVINE_KANTO_LATER_FERRY"
            ) && departure == GroupTravelDeparture::SsaquaMaiden)
    } else if route.id.ends_with("_ROUTE22") {
        departure == GroupTravelDeparture::Gate
    } else if route.id.ends_with("_CABLE_CAR") {
        departure == GroupTravelDeparture::CableCar
    } else if route.source_any {
        departure == GroupTravelDeparture::Fly
            || (departure == GroupTravelDeparture::Teleport && route.id != "HOENN:FLY_LITTLEROOT")
    } else {
        false
    }
}

pub(super) fn request_fingerprint<T: serde::Serialize>(
    domain: &[u8],
    request: &T,
) -> Result<[u8; 32], Phase2Error> {
    let encoded = serde_json::to_vec(request).map_err(|_| Phase2Error::Internal)?;
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update([0]);
    hasher.update(encoded);
    Ok(hasher.finalize().into())
}

fn path_fingerprint<T: serde::Serialize>(
    domain: &[u8],
    path: &[u8],
    request: &T,
) -> Result<[u8; 32], Phase2Error> {
    let encoded = serde_json::to_vec(request).map_err(|_| Phase2Error::Internal)?;
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update([0]);
    hasher.update(path);
    hasher.update([0]);
    hasher.update(encoded);
    Ok(hasher.finalize().into())
}

pub(super) fn authenticate_caller(
    state: &super::storage::State,
    actor: AuthenticatedActor,
    character_id: CharacterId,
) -> Result<(), Phase2Error> {
    let user = state
        .users_by_id
        .get(&actor.user_id)
        .ok_or(Phase2Error::Authentication)?;
    if user.user_id != actor.user_id
        || user.disabled
        || user.character_id != actor.character_id
        || actor.character_id != character_id
    {
        return Err(Phase2Error::Authentication);
    }
    let character = state
        .characters
        .get(&character_id)
        .ok_or(Phase2Error::Authentication)?;
    if character.owner != actor.user_id || character.state.character_id != character_id {
        return Err(Phase2Error::Authentication);
    }
    Ok(())
}

pub(super) fn validate_member(
    state: &super::storage::State,
    character_id: CharacterId,
) -> Result<(), Phase2Error> {
    let character = state
        .characters
        .get(&character_id)
        .ok_or(Phase2Error::Forbidden)?;
    let user = state
        .users_by_id
        .get(&character.owner)
        .ok_or(Phase2Error::Forbidden)?;
    if user.user_id != character.owner
        || user.disabled
        || user.character_id != character_id
        || character.state.character_id != character_id
    {
        return Err(Phase2Error::Forbidden);
    }
    Ok(())
}

pub(super) fn lease_matches(
    state: &super::storage::State,
    character_id: CharacterId,
    fence: LeaseFence,
    now: u64,
) -> Result<(), Phase2Error> {
    let lease = state
        .leases
        .get(&character_id)
        .ok_or(Phase2Error::Authentication)?;
    if lease.released || lease.contract.expires_at.value() <= now {
        return Err(Phase2Error::Authentication);
    }
    if lease.contract.fence() != fence {
        return Err(Phase2Error::Authentication);
    }
    Ok(())
}

fn active_member_lease(
    state: &super::storage::State,
    character_id: CharacterId,
    now: u64,
) -> Result<(), Phase2Error> {
    validate_member(state, character_id)?;
    let lease = state
        .leases
        .get(&character_id)
        .ok_or(Phase2Error::Forbidden)?;
    if lease.contract.character_id != character_id
        || lease.released
        || lease.contract.expires_at.value() <= now
    {
        return Err(Phase2Error::Forbidden);
    }
    Ok(())
}

pub(super) fn prune_group_state(state: &mut super::storage::State, now: u64) {
    state
        .group_invitations
        .retain(|_, invitation| !invitation.consumed && invitation.expires_at > now);
    state
        .group_idempotency
        .retain(|_, record| record.expires_at > now);
    // Group closure is committed by the online/session transactions before
    // this common group-state sweep runs.  Reconcile travel proposals here so
    // a scene that was accepted before closure cannot keep the old member
    // indexes alive indefinitely.  close_orphaned_proposals retains all
    // marker/receipt evidence needed for the durable recovery path.
    close_orphaned_proposals(state, now);
    super::storage::prune_closed_progress_feeds(state);
}

fn live_invitation_count(state: &super::storage::State, now: u64) -> usize {
    state
        .group_invitations
        .values()
        .filter(|invitation| !invitation.consumed && invitation.expires_at > now)
        .count()
}

fn recent_invitation_count(state: &super::storage::State, sender: CharacterId, now: u64) -> usize {
    // Successful creation receipts outlive invitations, including cancelled ones.
    // Invitation expiry identifies creation time because every invite uses the same TTL.
    state
        .group_idempotency
        .iter()
        .filter(|((character, operation, _), record)| {
            *character == sender
                && matches!(
                    operation.as_str(),
                    OP_CREATE | "online_invite_v1" | "online_invite_last_partner_v1"
                )
                && matches!(
                    &record.response,
                    GroupIdempotencyResponse::Invitation(view) if view.expires_at.value() > now
                )
        })
        .count()
}

pub(super) fn live_idempotency_count(state: &super::storage::State, now: u64) -> usize {
    state
        .group_idempotency
        .values()
        .filter(|record| record.expires_at > now)
        .count()
}

pub(super) fn idempotency_lookup(
    state: &super::storage::State,
    actor: CharacterId,
    operation: &str,
    key: coop_cloud::IdempotencyKey,
    fingerprint: [u8; 32],
    now: u64,
) -> Result<Option<GroupIdempotencyResponse>, Phase2Error> {
    let Some(record) = state
        .group_idempotency
        .get(&(actor, operation.to_owned(), key))
    else {
        return Ok(None);
    };
    if record.expires_at <= now {
        return Ok(None);
    }
    if record.fingerprint != fingerprint {
        return Err(Phase2Error::Conflict);
    }
    Ok(Some(record.response.clone()))
}

pub(super) fn build_invitation_view(
    record: &GroupInvitationRecord,
) -> Result<GroupInvitationView, Phase2Error> {
    Ok(GroupInvitationView {
        api_version: coop_cloud::ApiVersion::V1,
        invitation_id: record.invitation_id,
        inviter_character_id: record.inviter,
        invitee_character_id: record.invitee,
        expires_at: Store::unix_timestamp(record.expires_at)?,
    })
}

fn invitation_candidates(store: &Store) -> Result<Vec<GroupInvitationId>, Phase2Error> {
    (0..MAX_ID_CANDIDATES)
        .map(|_| GroupInvitationId::new(store.random_uuid()?).map_err(|_| Phase2Error::Internal))
        .collect()
}

fn group_candidates(store: &Store) -> Result<Vec<GroupId>, Phase2Error> {
    (0..MAX_ID_CANDIDATES)
        .map(|_| GroupId::new(store.random_uuid()?).map_err(|_| Phase2Error::Internal))
        .collect()
}

pub(super) fn group_view(
    state: &super::storage::State,
    group_id: GroupId,
) -> Result<GroupView, Phase2Error> {
    let record = state.groups.get(&group_id).ok_or(Phase2Error::NotFound)?;
    if record.status != GroupStatus::Active {
        return Err(Phase2Error::NotFound);
    }
    let members = record.group.members();
    let revisions = [
        state
            .characters
            .get(&members[0])
            .ok_or(Phase2Error::Internal)?
            .world_revision,
        state
            .characters
            .get(&members[1])
            .ok_or(Phase2Error::Internal)?
            .world_revision,
    ];
    let member_world_zones = members.map(|member| {
        state
            .characters
            .get(&member)
            .expect("member revisions were checked above")
            .state
            .world_zone
            .clone()
    });
    GroupView::new_with_member_zones(group_id, record.group, member_world_zones, revisions)
        .map_err(|_| Phase2Error::Internal)
}

pub(crate) fn create_invitation(
    store: &Store,
    actor: AuthenticatedActor,
    request: &CreateGroupInvitationRequest,
) -> Result<CreateGroupInvitationResponse, Phase2Error> {
    request
        .validate()
        .map_err(|_| Phase2Error::InvalidRequest)?;
    let fingerprint = request_fingerprint(OP_CREATE.as_bytes(), request)?;
    create_invitation_with_fingerprint(store, actor, request, OP_CREATE, fingerprint)
}

pub(super) fn create_invitation_with_fingerprint(
    store: &Store,
    actor: AuthenticatedActor,
    request: &CreateGroupInvitationRequest,
    operation: &str,
    fingerprint: [u8; 32],
) -> Result<CreateGroupInvitationResponse, Phase2Error> {
    create_invitation_with_options(store, actor, request, operation, fingerprint, false, None)
}

/// Creates a consent invitation for the caller's remembered partner. The
/// target is resolved again inside the write transaction so a stale client
/// cannot turn this action into an arbitrary-character invitation.
pub(super) fn create_last_partner_invitation_with_fingerprint(
    store: &Store,
    actor: AuthenticatedActor,
    request: &CreateGroupInvitationRequest,
    operation: &str,
    fingerprint: [u8; 32],
) -> Result<CreateGroupInvitationResponse, Phase2Error> {
    create_invitation_with_options(
        store,
        actor,
        request,
        operation,
        fingerprint,
        true,
        Some(actor.character_id),
    )
}

fn create_invitation_with_options(
    store: &Store,
    actor: AuthenticatedActor,
    request: &CreateGroupInvitationRequest,
    operation: &str,
    fingerprint: [u8; 32],
    allow_remote_maps: bool,
    last_partner_for: Option<CharacterId>,
) -> Result<CreateGroupInvitationResponse, Phase2Error> {
    let now = store.now();
    let expires_at = now
        .checked_add(GROUP_INVITATION_TTL_MS)
        .ok_or(Phase2Error::Internal)?;
    let receipt_expires = now
        .checked_add(GROUP_IDEMPOTENCY_TTL_MS)
        .ok_or(Phase2Error::Internal)?;
    store.write_transaction(|state| {
        authenticate_caller(state, actor, request.character_id)?;
        lease_matches(state, actor.character_id, request.fence(), now)?;
        if let Some(replay) = idempotency_lookup(
            state,
            actor.character_id,
            operation,
            request.idempotency_key(),
            fingerprint,
            now,
        )? {
            return match replay {
                GroupIdempotencyResponse::Invitation(response) => Ok(response),
                _ => Err(Phase2Error::Conflict),
            };
        }
        if live_idempotency_count(state, now) >= MAX_GROUP_IDEMPOTENCY
            || live_invitation_count(state, now) >= MAX_GROUP_INVITATIONS
        {
            return Err(Phase2Error::Busy);
        }
        if recent_invitation_count(state, actor.character_id, now)
            >= MAX_INVITATIONS_PER_SENDER_PER_MINUTE
        {
            return Err(Phase2Error::Busy);
        }
        let target_id = if let Some(member) = last_partner_for {
            let group_id = state
                .last_group_by_member
                .get(&member)
                .copied()
                .ok_or(Phase2Error::NotFound)?;
            let group = state.groups.get(&group_id).ok_or(Phase2Error::NotFound)?;
            group
                .group
                .members()
                .into_iter()
                .find(|candidate| *candidate != member)
                .ok_or(Phase2Error::NotFound)?
        } else {
            request.invitee_character_id
        };
        if target_id != request.invitee_character_id {
            return Err(Phase2Error::NotFound);
        }
        let target = state
            .characters
            .get(&target_id)
            .ok_or(Phase2Error::Forbidden)?;
        validate_member(state, target_id)?;
        active_member_lease(state, target_id, now)?;
        let inviter = state
            .characters
            .get(&actor.character_id)
            .ok_or(Phase2Error::NotFound)?;
        if !allow_remote_maps && target.state.world_zone != inviter.state.world_zone {
            return Err(Phase2Error::Forbidden);
        }
        if state
            .active_group_by_member
            .contains_key(&actor.character_id)
            || state
                .active_group_by_member
                .contains_key(&request.invitee_character_id)
        {
            return Err(Phase2Error::Conflict);
        }
        let invitation_candidates = invitation_candidates(store)?;
        let invitation_id = invitation_candidates
            .iter()
            .copied()
            .find(|candidate| {
                state
                    .group_invitations
                    .get(candidate)
                    .is_none_or(|record| record.consumed || record.expires_at <= now)
            })
            .ok_or(Phase2Error::Conflict)?;
        let invitation = GroupInvitationRecord {
            invitation_id,
            inviter: actor.character_id,
            invitee: target_id,
            expires_at,
            consumed: false,
            allow_remote_maps,
        };
        let view = build_invitation_view(&invitation)?;
        prune_group_state(state, now);
        state.group_invitations.insert(invitation_id, invitation);
        state.group_idempotency.insert(
            (
                actor.character_id,
                operation.to_owned(),
                request.idempotency_key(),
            ),
            GroupIdempotencyRecord {
                fingerprint,
                response: GroupIdempotencyResponse::Invitation(view),
                expires_at: receipt_expires,
            },
        );
        Ok(view)
    })
}

#[allow(
    clippy::too_many_lines,
    reason = "the acceptance transaction keeps all validation before its mutation suffix"
)]
pub(crate) fn accept_invitation(
    store: &Store,
    actor: AuthenticatedActor,
    invitation_id: GroupInvitationId,
    request: &AcceptGroupInvitationRequest,
) -> Result<AcceptGroupInvitationResponse, Phase2Error> {
    request
        .validate()
        .map_err(|_| Phase2Error::InvalidRequest)?;
    let fingerprint = path_fingerprint(
        OP_ACCEPT.as_bytes(),
        invitation_id.as_uuid().as_bytes(),
        request,
    )?;
    let now = store.now();
    let receipt_expires = now
        .checked_add(GROUP_IDEMPOTENCY_TTL_MS)
        .ok_or(Phase2Error::Internal)?;
    store.write_transaction(|state| {
        authenticate_caller(state, actor, request.character_id)?;
        lease_matches(state, actor.character_id, request.fence(), now)?;
        if let Some(replay) = idempotency_lookup(
            state,
            actor.character_id,
            OP_ACCEPT,
            request.idempotency_key(),
            fingerprint,
            now,
        )? {
            return match replay {
                GroupIdempotencyResponse::Accept(response) => Ok(response),
                _ => Err(Phase2Error::Conflict),
            };
        }
        if live_idempotency_count(state, now) >= MAX_GROUP_IDEMPOTENCY {
            return Err(Phase2Error::Busy);
        }
        let invitation = state
            .group_invitations
            .get(&invitation_id)
            .ok_or(Phase2Error::NotFound)?;
        if invitation.invitee != actor.character_id {
            return Err(Phase2Error::NotFound);
        }
        if invitation.consumed || invitation.expires_at <= now {
            return Err(Phase2Error::Expired);
        }
        let allow_remote_maps = invitation.allow_remote_maps;
        let initiator = invitation.inviter;
        let recipient = invitation.invitee;
        if initiator == recipient {
            return Err(Phase2Error::Internal);
        }
        active_member_lease(state, initiator, now)?;
        active_member_lease(state, recipient, now)?;
        let initiator_record = state
            .characters
            .get(&initiator)
            .ok_or(Phase2Error::Internal)?;
        let recipient_record = state
            .characters
            .get(&recipient)
            .ok_or(Phase2Error::Internal)?;
        if !allow_remote_maps
            && initiator_record.state.world_zone != recipient_record.state.world_zone
        {
            return Err(Phase2Error::Forbidden);
        }
        if state.active_group_by_member.contains_key(&initiator)
            || state.active_group_by_member.contains_key(&recipient)
        {
            return Err(Phase2Error::Conflict);
        }
        let group_candidates = group_candidates(store)?;
        let group_id = group_candidates
            .iter()
            .copied()
            .find(|candidate| !state.groups.contains_key(candidate))
            .ok_or(Phase2Error::Conflict)?;
        let group = Group::new(initiator, recipient).map_err(|_| Phase2Error::Internal)?;
        let group_members = group.members();
        let world_revisions = [
            if group_members[0] == initiator {
                initiator_record.world_revision
            } else {
                recipient_record.world_revision
            },
            if group_members[1] == initiator {
                initiator_record.world_revision
            } else {
                recipient_record.world_revision
            },
        ];
        let member_world_zones = [
            if group_members[0] == initiator {
                initiator_record.state.world_zone.clone()
            } else {
                recipient_record.state.world_zone.clone()
            },
            if group_members[1] == initiator {
                initiator_record.state.world_zone.clone()
            } else {
                recipient_record.state.world_zone.clone()
            },
        ];
        // Keep the legacy shared field aligned with canonical member order;
        // the inviter may be the second member when a remembered partner
        // accepts the invitation.
        let zone = member_world_zones[0].clone();
        let view =
            GroupView::new_with_member_zones(group_id, group, member_world_zones, world_revisions)
                .map_err(|_| Phase2Error::Internal)?;
        let response = AcceptGroupInvitationResponse {
            api_version: coop_cloud::ApiVersion::V1,
            group: view,
        };
        if !state.group_invitations.contains_key(&invitation_id) {
            return Err(Phase2Error::Internal);
        }
        prune_group_state(state, now);
        state
            .group_invitations
            .get_mut(&invitation_id)
            .expect("validated invitation exists")
            .consumed = true;
        state.groups.insert(
            group_id,
            GroupRecord {
                group,
                zone,
                status: GroupStatus::Active,
                zone_revision: 0,
            },
        );
        if allow_remote_maps {
            state.group_member_world_zones.insert(
                group_id,
                [
                    state
                        .characters
                        .get(&group_members[0])
                        .ok_or(Phase2Error::Internal)?
                        .state
                        .world_zone
                        .clone(),
                    state
                        .characters
                        .get(&group_members[1])
                        .ok_or(Phase2Error::Internal)?
                        .state
                        .world_zone
                        .clone(),
                ],
            );
        }
        state.group_end_notices.remove(&initiator);
        state.group_end_notices.remove(&recipient);
        state.active_group_by_member.insert(initiator, group_id);
        state.active_group_by_member.insert(recipient, group_id);
        state.last_group_by_member.insert(initiator, group_id);
        state.last_group_by_member.insert(recipient, group_id);
        state.group_idempotency.insert(
            (
                actor.character_id,
                OP_ACCEPT.to_owned(),
                request.idempotency_key(),
            ),
            GroupIdempotencyRecord {
                fingerprint,
                response: GroupIdempotencyResponse::Accept(response.clone()),
                expires_at: receipt_expires,
            },
        );
        Ok(response)
    })
}

pub(crate) fn inspect_group(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    fence: LeaseFence,
) -> Result<GroupView, Phase2Error> {
    let now = store.now();
    store.read_transaction(|state| {
        authenticate_caller(state, actor, actor.character_id)?;
        let record = state.groups.get(&group_id).ok_or(Phase2Error::NotFound)?;
        if record.status != GroupStatus::Active || !record.group.contains(actor.character_id) {
            return Err(Phase2Error::NotFound);
        }
        let members = record.group.members();
        if state.active_group_by_member.get(&members[0]) != Some(&group_id)
            || state.active_group_by_member.get(&members[1]) != Some(&group_id)
        {
            return Err(Phase2Error::Internal);
        }
        validate_member(state, members[0])?;
        validate_member(state, members[1])?;
        lease_matches(state, actor.character_id, fence, now)?;
        group_view(state, group_id)
    })
}

/// The former direct travel operation is intentionally denied for every caller.
/// Group movement is committed only by accepting a live consent proposal.
pub(crate) fn travel(
    _store: &Store,
    _actor: AuthenticatedActor,
    _group_id: GroupId,
    _request: &coop_cloud::GroupTravelRequest,
) -> Result<coop_cloud::GroupTravelResponse, Phase2Error> {
    Err(Phase2Error::Forbidden)
}

fn proposal_candidates(store: &Store) -> Result<Vec<GroupTravelProposalId>, Phase2Error> {
    (0..MAX_ID_CANDIDATES)
        .map(|_| {
            GroupTravelProposalId::new(store.random_uuid()?).map_err(|_| Phase2Error::Internal)
        })
        .collect()
}

fn release_proposal_indexes(state: &mut super::storage::State, proposal_id: GroupTravelProposalId) {
    state
        .live_group_travel_by_group
        .retain(|_, indexed| *indexed != proposal_id);
    state
        .live_group_travel_by_member
        .retain(|_, indexed| *indexed != proposal_id);
}

fn expire_pending_proposals(state: &mut super::storage::State, now: u64) {
    let expired: Vec<_> = state
        .group_travel_proposals
        .iter()
        .filter_map(|(proposal_id, record)| {
            (matches!(
                record.view.status,
                GroupTravelProposalStatus::Pending
                    | GroupTravelProposalStatus::AwaitingSceneReceipts
            ) && record.view.expires_at.value() <= now)
                .then_some(*proposal_id)
        })
        .collect();
    for proposal_id in expired {
        if let Some(record) = state.group_travel_proposals.get_mut(&proposal_id) {
            if record.view.status == GroupTravelProposalStatus::AwaitingSceneReceipts
                && record.scene_markers.iter().any(Option::is_some)
            {
                // A finalized save may already exist even when its receipt
                // was lost. Keep both locks until an explicit recovery path
                // reconciles those immutable snapshot heads.
                record.view.status = GroupTravelProposalStatus::Suspended;
                continue;
            }
            record.view.status = GroupTravelProposalStatus::Expired;
            record.retain_until = Some(now.saturating_add(TRAVEL_PROPOSAL_REPLAY_TTL_MS));
        }
        release_proposal_indexes(state, proposal_id);
    }
}

fn is_story_travel_archive(record: &GroupTravelProposalRecord) -> bool {
    is_story_route(record.view.route_id.as_str())
        && record.view.status == GroupTravelProposalStatus::Cancelled
        && record.retain_until.is_none()
        && (record.scene_markers.iter().any(Option::is_some)
            || record.scene_receipts.iter().any(Option::is_some))
}

fn has_unresolved_story_marker(record: &GroupTravelProposalRecord) -> bool {
    record.view.status != GroupTravelProposalStatus::Committed
        && (0..2).any(|index| {
            record.scene_markers[index].is_some() && record.recovery_resolutions[index].is_none()
        })
}

fn active_travel_proposal_count(state: &super::storage::State) -> usize {
    state
        .group_travel_proposals
        .values()
        .filter(|record| !is_story_travel_archive(record))
        .count()
}

fn prune_story_travel_archives(state: &mut super::storage::State) {
    let mut archives: Vec<_> = state
        .group_travel_proposals
        .iter()
        // A marker may refer to a finalized save whose receipt was lost.
        // Protect unresolved evidence; resolved archives use the bounded quota.
        .filter(|(_, record)| {
            is_story_travel_archive(record) && !has_unresolved_story_marker(record)
        })
        .map(|(id, record)| {
            let marked_at = record
                .scene_markers
                .iter()
                .flatten()
                .map(|marker| marker.marked_at)
                .max()
                .unwrap_or(0);
            (*id, marked_at)
        })
        .collect();
    // Keep the most recent completed evidence within each member's allowance.
    archives.sort_by(|a, b| (b.1, b.0.as_uuid().as_bytes()).cmp(&(a.1, a.0.as_uuid().as_bytes())));
    let mut per_member = std::collections::HashMap::<CharacterId, usize>::new();
    for (id, _) in archives {
        let members = state.group_travel_proposals[&id]
            .view
            .expected_members
            .map(|member| member.character_id);
        if members.iter().any(|member| {
            per_member.get(member).copied().unwrap_or(0) >= MAX_STORY_TRAVEL_ARCHIVES_PER_MEMBER
        }) {
            state.group_travel_proposals.remove(&id);
        } else {
            for member in members {
                *per_member.entry(member).or_default() += 1;
            }
        }
    }
}

fn close_orphaned_proposals(state: &mut super::storage::State, now: u64) {
    let closed: Vec<_> = state
        .group_travel_proposals
        .iter()
        .filter_map(|(proposal_id, record)| {
            (matches!(
                record.view.status,
                GroupTravelProposalStatus::Pending
                    | GroupTravelProposalStatus::AwaitingSceneReceipts
                    | GroupTravelProposalStatus::Suspended
            ) && state
                .groups
                .get(&record.view.group_id)
                .is_some_and(|group| group.status == GroupStatus::Closed))
            .then_some(*proposal_id)
        })
        .collect();
    for proposal_id in closed {
        let record = state
            .group_travel_proposals
            .get_mut(&proposal_id)
            .expect("selected proposal");
        record.view.status = GroupTravelProposalStatus::Cancelled;
        // A scene marker may precede a finalized save whose receipt was lost.
        // Keep that evidence for manual reconciliation, even after releasing
        // the dead group's locks. Per-member archive pruning bounds these rows.
        record.retain_until = if record.scene_markers.iter().any(Option::is_some)
            || record.scene_receipts.iter().any(Option::is_some)
        {
            None
        } else {
            Some(now.saturating_add(TRAVEL_PROPOSAL_REPLAY_TTL_MS))
        };
        release_proposal_indexes(state, proposal_id);
    }
}

fn prune_travel_proposal_state(state: &mut super::storage::State, now: u64) {
    close_orphaned_proposals(state, now);
    expire_pending_proposals(state, now);
    state.group_travel_proposals.retain(|_, record| {
        record
            .retain_until
            .is_none_or(|retain_until| retain_until > now)
    });
    prune_story_travel_archives(state);
    state
        .group_travel_proposal_idempotency
        .retain(|_, record| record.expires_at > now);
}

fn lifecycle_receipt_count(state: &super::storage::State) -> usize {
    state
        .group_travel_proposal_idempotency
        .values()
        .filter(|record| record.lifecycle)
        .count()
}

fn create_receipt_count(state: &super::storage::State) -> usize {
    state
        .group_travel_proposal_idempotency
        .values()
        .filter(|record| !record.lifecycle)
        .count()
}

fn reserved_lifecycle_receipts(state: &super::storage::State) -> usize {
    state
        .group_travel_proposals
        .values()
        .map(|record| match record.view.status {
            GroupTravelProposalStatus::Pending => 3,
            GroupTravelProposalStatus::AwaitingSceneReceipts => 2,
            GroupTravelProposalStatus::Suspended => 2,
            GroupTravelProposalStatus::Committed => 2_usize.saturating_sub(
                record
                    .view
                    .applied_by
                    .iter()
                    .filter(|value| **value)
                    .count(),
            ),
            GroupTravelProposalStatus::Declined
            | GroupTravelProposalStatus::Cancelled
            | GroupTravelProposalStatus::Expired => 0,
        })
        .sum()
}

fn ensure_lifecycle_receipt_slot(state: &super::storage::State) -> Result<(), Phase2Error> {
    if lifecycle_receipt_count(state) < MAX_TRAVEL_LIFECYCLE_RECEIPTS {
        Ok(())
    } else {
        Err(Phase2Error::Busy)
    }
}

pub(super) fn cancel_pending_for_member(
    state: &mut super::storage::State,
    character_id: CharacterId,
) {
    let Some(proposal_id) = state
        .live_group_travel_by_member
        .get(&character_id)
        .copied()
    else {
        return;
    };
    let group_closed = state
        .group_travel_proposals
        .get(&proposal_id)
        .and_then(|record| state.groups.get(&record.view.group_id))
        .is_some_and(|group| group.status == GroupStatus::Closed);
    if let Some(record) = state.group_travel_proposals.get_mut(&proposal_id)
        && (record.view.status == GroupTravelProposalStatus::Pending
            || (group_closed
                && matches!(
                    record.view.status,
                    GroupTravelProposalStatus::AwaitingSceneReceipts
                        | GroupTravelProposalStatus::Suspended
                )))
    {
        record.view.status = GroupTravelProposalStatus::Cancelled;
        // Preserve story markers and receipts indefinitely for the recovery
        // endpoint. A proposal with no scene evidence only needs the normal
        // bounded replay retention window.
        record.retain_until = if record.scene_markers.iter().any(Option::is_some)
            || record.scene_receipts.iter().any(Option::is_some)
        {
            None
        } else {
            Some(
                record
                    .view
                    .expires_at
                    .value()
                    .saturating_add(TRAVEL_PROPOSAL_REPLAY_TTL_MS),
            )
        };
        release_proposal_indexes(state, proposal_id);
    }
}

fn proposal_idempotency_lookup(
    state: &super::storage::State,
    actor: CharacterId,
    operation: &str,
    key: coop_cloud::IdempotencyKey,
    fingerprint: [u8; 32],
) -> Result<Option<GroupTravelProposalView>, Phase2Error> {
    let Some(record) =
        state
            .group_travel_proposal_idempotency
            .get(&(actor, operation.to_owned(), key))
    else {
        return Ok(None);
    };
    if record.fingerprint != fingerprint {
        return Err(Phase2Error::Conflict);
    }
    Ok(Some(record.response.clone()))
}

fn insert_proposal_receipt(
    state: &mut super::storage::State,
    actor: CharacterId,
    operation: &str,
    key: coop_cloud::IdempotencyKey,
    fingerprint: [u8; 32],
    response: GroupTravelProposalView,
    now: u64,
) {
    state.group_travel_proposal_idempotency.insert(
        (actor, operation.to_owned(), key),
        GroupTravelProposalIdempotencyRecord {
            fingerprint,
            response,
            expires_at: now.saturating_add(TRAVEL_PROPOSAL_REPLAY_TTL_MS),
            lifecycle: operation == OP_TRAVEL_ACTION,
        },
    );
}

fn proposal_group(
    state: &super::storage::State,
    group_id: GroupId,
    actor: CharacterId,
) -> Result<(Group, [CharacterId; 2]), Phase2Error> {
    let record = state.groups.get(&group_id).ok_or(Phase2Error::NotFound)?;
    if record.status != GroupStatus::Active || !record.group.contains(actor) {
        return Err(Phase2Error::NotFound);
    }
    let members = record.group.members();
    if members
        .iter()
        .any(|member| state.active_group_by_member.get(member) != Some(&group_id))
    {
        return Err(Phase2Error::Internal);
    }
    Ok((record.group, members))
}

#[allow(
    clippy::too_many_lines,
    reason = "one transaction snapshots every immutable proposal precondition"
)]
pub(crate) fn create_travel_proposal(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    request: &GroupTravelProposalRequest,
) -> Result<GroupTravelProposalView, Phase2Error> {
    request
        .validate()
        .map_err(|_| Phase2Error::InvalidRequest)?;
    let fingerprint = path_fingerprint(
        OP_PROPOSE_TRAVEL.as_bytes(),
        group_id.as_uuid().as_bytes(),
        request,
    )?;
    let candidates = proposal_candidates(store)?;
    let now = store.now();
    let expires_at = now
        .checked_add(TRAVEL_PROPOSAL_TTL_MS)
        .ok_or(Phase2Error::Internal)?;
    store.write_transaction(|state| {
        authenticate_caller(state, actor, request.character_id)?;
        lease_matches(state, actor.character_id, request.fence(), now)?;
        prune_travel_proposal_state(state, now);
        if let Some(replay) = proposal_idempotency_lookup(
            state,
            actor.character_id,
            OP_PROPOSE_TRAVEL,
            request.idempotency_key,
            fingerprint,
        )? {
            return Ok(replay);
        }
        if active_travel_proposal_count(state) >= MAX_TRAVEL_PROPOSALS
            || create_receipt_count(state) >= MAX_TRAVEL_CREATE_RECEIPTS
            || lifecycle_receipt_count(state)
                .saturating_add(reserved_lifecycle_receipts(state))
                .saturating_add(3)
                > MAX_TRAVEL_LIFECYCLE_RECEIPTS
        {
            return Err(Phase2Error::Busy);
        }
        let definition = consent_route_definition(request.route_id.as_str())?;
        if !departure_matches_route(&definition, request.departure) {
            return Err(Phase2Error::Forbidden);
        }
        let (group, members) = proposal_group(state, group_id, actor.character_id)?;
        if state.live_group_travel_by_group.contains_key(&group_id)
            || members
                .iter()
                .any(|member| state.live_group_travel_by_member.contains_key(member))
        {
            return Err(Phase2Error::Conflict);
        }
        let group_record = state.groups.get(&group_id).ok_or(Phase2Error::NotFound)?;
        if !definition.source_any && group_record.zone != definition.source {
            return Err(Phase2Error::Forbidden);
        }
        active_member_lease(state, members[0], now)?;
        active_member_lease(state, members[1], now)?;
        let first = state
            .characters
            .get(&members[0])
            .ok_or(Phase2Error::Internal)?;
        let second = state
            .characters
            .get(&members[1])
            .ok_or(Phase2Error::Internal)?;
        if !definition.source_any
            && (first.state.world_zone != definition.source
                || second.state.world_zone != definition.source)
        {
            return Err(Phase2Error::Forbidden);
        }
        let proposal_id = candidates
            .iter()
            .copied()
            .find(|candidate| !state.group_travel_proposals.contains_key(candidate))
            .ok_or(Phase2Error::Conflict)?;
        let expected_members = [
            GroupMemberView {
                character_id: members[0],
                world_revision: first.world_revision,
            },
            GroupMemberView {
                character_id: members[1],
                world_revision: second.world_revision,
            },
        ];
        let responder = if members[0] == actor.character_id {
            members[1]
        } else {
            members[0]
        };
        let view = GroupTravelProposalView {
            api_version: coop_cloud::ApiVersion::V1,
            proposal_id,
            group_id,
            requester_character_id: actor.character_id,
            responder_character_id: responder,
            route_id: request.route_id.clone(),
            departure: request.departure,
            source: if definition.source_any {
                group_record.zone.clone()
            } else {
                definition.source.clone()
            },
            destination: definition.destination,
            expected_group_zone_revision: group_record.zone_revision,
            expected_members,
            status: GroupTravelProposalStatus::Pending,
            expires_at: Store::unix_timestamp(expires_at)?,
            server_now: None,
            commit: None,
            applied_by: [false; 2],
            scene_marked_by: [false; 2],
            scene_receipted_by: [false; 2],
        };
        insert_proposal_receipt(
            state,
            actor.character_id,
            OP_PROPOSE_TRAVEL,
            request.idempotency_key,
            fingerprint,
            view.clone(),
            now,
        );
        state.group_travel_proposals.insert(
            proposal_id,
            GroupTravelProposalRecord {
                view: view.clone(),
                retain_until: None,
                scene_markers: [None, None],
                recovery_resolutions: [None, None],
                scene_receipts: [None, None],
            },
        );
        state
            .live_group_travel_by_group
            .insert(group_id, proposal_id);
        for member in group.members() {
            state
                .live_group_travel_by_member
                .insert(member, proposal_id);
        }
        Ok(view)
    })
}

pub(crate) fn current_travel_proposal(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    fence: LeaseFence,
) -> Result<GroupTravelProposalView, Phase2Error> {
    let now = store.now();
    store.write_transaction(|state| {
        authenticate_caller(state, actor, actor.character_id)?;
        lease_matches(state, actor.character_id, fence, now)?;
        proposal_group(state, group_id, actor.character_id)?;
        prune_travel_proposal_state(state, now);
        let proposal_id = state
            .live_group_travel_by_group
            .get(&group_id)
            .copied()
            .ok_or(Phase2Error::NotFound)?;
        let view = &state
            .group_travel_proposals
            .get(&proposal_id)
            .ok_or(Phase2Error::Internal)?
            .view;
        if view.requester_character_id != actor.character_id
            && view.responder_character_id != actor.character_id
        {
            return Err(Phase2Error::NotFound);
        }
        Ok(view.clone())
    })
}

pub(crate) fn get_travel_proposal(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    proposal_id: GroupTravelProposalId,
    fence: LeaseFence,
) -> Result<GroupTravelProposalView, Phase2Error> {
    let now = store.now();
    store.write_transaction(|state| {
        authenticate_caller(state, actor, actor.character_id)?;
        lease_matches(state, actor.character_id, fence, now)?;
        prune_travel_proposal_state(state, now);
        let view = &state
            .group_travel_proposals
            .get(&proposal_id)
            .ok_or(Phase2Error::NotFound)?
            .view;
        if view.group_id != group_id
            || (view.requester_character_id != actor.character_id
                && view.responder_character_id != actor.character_id)
        {
            return Err(Phase2Error::NotFound);
        }
        Ok(view.clone())
    })
}

#[allow(
    clippy::too_many_lines,
    reason = "one transaction validates and commits both members"
)]
pub(crate) fn act_on_travel_proposal(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    proposal_id: GroupTravelProposalId,
    request: &GroupTravelActionRequest,
) -> Result<GroupTravelProposalView, Phase2Error> {
    let mut path = Vec::with_capacity(32);
    path.extend_from_slice(group_id.as_uuid().as_bytes());
    path.extend_from_slice(proposal_id.as_uuid().as_bytes());
    let fingerprint = path_fingerprint(OP_TRAVEL_ACTION.as_bytes(), &path, request)?;
    let now = store.now();
    store.write_transaction(|state| {
        authenticate_caller(state, actor, request.character_id)?;
        lease_matches(state, actor.character_id, request.fence(), now)?;
        prune_travel_proposal_state(state, now);
        if let Some(replay) = proposal_idempotency_lookup(
            state,
            actor.character_id,
            OP_TRAVEL_ACTION,
            request.idempotency_key,
            fingerprint,
        )? {
            return Ok(replay);
        }
        let snapshot = state
            .group_travel_proposals
            .get(&proposal_id)
            .ok_or(Phase2Error::NotFound)?
            .view
            .clone();
        if snapshot.group_id != group_id
            || (snapshot.requester_character_id != actor.character_id
                && snapshot.responder_character_id != actor.character_id)
        {
            return Err(Phase2Error::NotFound);
        }
        let mut response = snapshot.clone();
        match request.action {
            GroupTravelAction::Decline => {
                if actor.character_id != snapshot.responder_character_id {
                    return Err(Phase2Error::Forbidden);
                }
                if snapshot.status != GroupTravelProposalStatus::Pending {
                    return Err(Phase2Error::Conflict);
                }
                ensure_lifecycle_receipt_slot(state)?;
                response.status = GroupTravelProposalStatus::Declined;
                let record = state
                    .group_travel_proposals
                    .get_mut(&proposal_id)
                    .expect("proposal exists");
                record.view = response.clone();
                record.retain_until = Some(now.saturating_add(TRAVEL_PROPOSAL_REPLAY_TTL_MS));
                release_proposal_indexes(state, proposal_id);
            }
            GroupTravelAction::Cancel => {
                if actor.character_id != snapshot.requester_character_id {
                    return Err(Phase2Error::Forbidden);
                }
                if snapshot.status != GroupTravelProposalStatus::Pending {
                    return Err(Phase2Error::Conflict);
                }
                ensure_lifecycle_receipt_slot(state)?;
                response.status = GroupTravelProposalStatus::Cancelled;
                let record = state
                    .group_travel_proposals
                    .get_mut(&proposal_id)
                    .expect("proposal exists");
                record.view = response.clone();
                record.retain_until = Some(now.saturating_add(TRAVEL_PROPOSAL_REPLAY_TTL_MS));
                release_proposal_indexes(state, proposal_id);
            }
            GroupTravelAction::Applied => {
                if snapshot.status != GroupTravelProposalStatus::Committed {
                    return Err(Phase2Error::Conflict);
                }
                let members = snapshot.expected_members.map(|member| member.character_id);
                let index = members
                    .iter()
                    .position(|member| *member == actor.character_id)
                    .ok_or(Phase2Error::NotFound)?;
                if snapshot.applied_by[index] {
                    return Err(Phase2Error::Conflict);
                }
                ensure_lifecycle_receipt_slot(state)?;
                response.applied_by[index] = true;
                if response.applied_by == [true, true] {
                    state.group_travel_proposals.remove(&proposal_id);
                    release_proposal_indexes(state, proposal_id);
                } else {
                    state
                        .group_travel_proposals
                        .get_mut(&proposal_id)
                        .expect("proposal exists")
                        .view = response.clone();
                }
            }
            GroupTravelAction::Accept => {
                if actor.character_id != snapshot.responder_character_id {
                    return Err(Phase2Error::Forbidden);
                }
                if snapshot.status != GroupTravelProposalStatus::Pending {
                    return Err(Phase2Error::Conflict);
                }
                let story_route = is_story_route(snapshot.route_id.as_str());
                let definition = if story_route {
                    story_route_definition(snapshot.route_id.as_str())
                        .ok_or(Phase2Error::Conflict)?
                } else {
                    consent_route_definition(snapshot.route_id.as_str())?
                };
                if (!definition.source_any && definition.source != snapshot.source)
                    || definition.destination != snapshot.destination
                    || !departure_matches_route(&definition, snapshot.departure)
                {
                    return Err(Phase2Error::Conflict);
                }
                let (group, members) = proposal_group(state, group_id, actor.character_id)?;
                active_member_lease(state, members[0], now)?;
                active_member_lease(state, members[1], now)?;
                let group_record = state.groups.get(&group_id).ok_or(Phase2Error::NotFound)?;
                if group_record.zone != snapshot.source
                    || group_record.zone_revision != snapshot.expected_group_zone_revision
                {
                    return Err(Phase2Error::Conflict);
                }
                let mut revisions = [0_u64; 2];
                for (index, member) in members.iter().copied().enumerate() {
                    let character = state.characters.get(&member).ok_or(Phase2Error::Internal)?;
                    if (!definition.source_any && character.state.world_zone != snapshot.source)
                        || character.world_revision
                            != snapshot.expected_members[index].world_revision
                        || snapshot.expected_members[index].character_id != member
                    {
                        return Err(Phase2Error::Conflict);
                    }
                    if definition.source_any {
                        // The ROM checks its vanilla Fly visit flag before
                        // proposing or consenting. Cloud Fly-point progress
                        // can lag until a save checkpoint, so require regional
                        // state here without treating it as a live unlock.
                        if character
                            .state
                            .progress_for(definition.destination.region)
                            .is_none()
                        {
                            return Err(Phase2Error::Forbidden);
                        }
                    } else {
                        let progress = character
                            .state
                            .progress_for(definition.source.region)
                            .ok_or(Phase2Error::Forbidden)?;
                        if progress.badge_count() < definition.minimum_badges
                            || progress.story_checkpoint < definition.minimum_story_checkpoint
                            || character
                                .state
                                .progress_for(definition.destination.region)
                                .is_none()
                        {
                            return Err(Phase2Error::Forbidden);
                        }
                    }
                    revisions[index] = character
                        .world_revision
                        .checked_add(1)
                        .filter(|revision| *revision <= MAX_WORLD_REVISION)
                        .ok_or(Phase2Error::Internal)?;
                }
                let zone_revision = group_record
                    .zone_revision
                    .checked_add(1)
                    .filter(|revision| *revision <= MAX_WORLD_REVISION)
                    .ok_or(Phase2Error::Internal)?;
                ensure_lifecycle_receipt_slot(state)?;
                if story_route {
                    // Consent holds both proposal indexes. The scene has not
                    // yet run, so neither world zone may advance here.
                    response.status = GroupTravelProposalStatus::AwaitingSceneReceipts;
                    response.expires_at =
                        Store::unix_timestamp(now.saturating_add(STORY_SCENE_RECEIPT_TTL_MS))?;
                    state
                        .group_travel_proposals
                        .get_mut(&proposal_id)
                        .expect("proposal exists")
                        .view = response.clone();
                    insert_proposal_receipt(
                        state,
                        actor.character_id,
                        OP_TRAVEL_ACTION,
                        request.idempotency_key,
                        fingerprint,
                        response.clone(),
                        now,
                    );
                    return Ok(response);
                }
                response.status = GroupTravelProposalStatus::Committed;
                response.commit = Some(GroupTravelCommit {
                    group_zone_revision: zone_revision,
                    members: [
                        GroupMemberView {
                            character_id: members[0],
                            world_revision: revisions[0],
                        },
                        GroupMemberView {
                            character_id: members[1],
                            world_revision: revisions[1],
                        },
                    ],
                    destination: snapshot.destination.clone(),
                });
                for (index, member) in members.iter().copied().enumerate() {
                    let character = state.characters.get_mut(&member).expect("member exists");
                    character.state.world_zone = snapshot.destination.clone();
                    character.world_revision = revisions[index];
                }
                if let Some(member_world_zones) = state.group_member_world_zones.get_mut(&group_id)
                {
                    for zone in member_world_zones {
                        *zone = snapshot.destination.clone();
                    }
                }
                let record = state.groups.get_mut(&group_id).expect("group exists");
                record.zone = snapshot.destination.clone();
                record.zone_revision = zone_revision;
                state
                    .group_travel_proposals
                    .get_mut(&proposal_id)
                    .expect("proposal exists")
                    .view = response.clone();
                let _ = group;
            }
        }
        insert_proposal_receipt(
            state,
            actor.character_id,
            OP_TRAVEL_ACTION,
            request.idempotency_key,
            fingerprint,
            response.clone(),
            now,
        );
        Ok(response)
    })
}

/// Finds the caller's single unresolved first-Briney marker without relying on
/// an active group or the live proposal indexes. This is a read-only lookup:
/// neither discovery nor a stale fence can reconcile save evidence.
pub(crate) fn discover_story_travel_recovery(
    store: &Store,
    actor: AuthenticatedActor,
    fence: LeaseFence,
) -> Result<StoryTravelRecoveryView, Phase2Error> {
    let now = store.now();
    store.read_transaction(|state| {
        authenticate_caller(state, actor, actor.character_id)?;
        lease_matches(state, actor.character_id, fence, now)?;

        let mut found = None;
        for record in state.group_travel_proposals.values() {
            let view = &record.view;
            if !is_story_route(view.route_id.as_str())
                || !matches!(
                    view.status,
                    GroupTravelProposalStatus::AwaitingSceneReceipts
                        | GroupTravelProposalStatus::Suspended
                        | GroupTravelProposalStatus::Cancelled
                )
                || record.retain_until.is_some_and(|until| until <= now)
            {
                continue;
            }
            let Some(index) = view
                .expected_members
                .iter()
                .position(|member| member.character_id == actor.character_id)
            else {
                continue;
            };
            let Some(marker) = record.scene_markers[index] else {
                continue;
            };
            if record.recovery_resolutions[index].is_some() {
                continue;
            }
            if found.is_some() {
                return Err(Phase2Error::Conflict);
            }
            found = Some(StoryTravelRecoveryView {
                api_version: ApiVersion::V1,
                proposal_id: view.proposal_id,
                group_id: view.group_id,
                status: view.status,
                marker_fence: marker.fence,
                scene_nonce: marker.nonce,
                marked_at: UnixTimestampMillis::new(marker.marked_at),
            });
        }
        found.ok_or(Phase2Error::NotFound)
    })
}

/// Resolves one closed first-voyage marker from server-owned finalized evidence.
pub(crate) fn resolve_story_travel_recovery(
    store: &Store,
    actor: AuthenticatedActor,
    proposal_id: GroupTravelProposalId,
    fence: LeaseFence,
    request: &StoryTravelRecoveryActionRequest,
) -> Result<StoryTravelRecoveryResolutionView, Phase2Error> {
    if request.api_version.value() != 1 {
        return Err(Phase2Error::InvalidRequest);
    }
    let now = store.now();
    store.write_transaction(|state| {
        authenticate_caller(state, actor, actor.character_id)?;
        lease_matches(state, actor.character_id, fence, now)?;
        prune_travel_proposal_state(state, now);
        Ok::<_, Phase2Error>(())
    })?;
    let (snapshot, marker, outcome, route_id) = store.read_transaction(|state| {
        authenticate_caller(state, actor, actor.character_id)?;
        lease_matches(state, actor.character_id, fence, now)?;
        let record = state
            .group_travel_proposals
            .get(&proposal_id)
            .ok_or(Phase2Error::NotFound)?;
        let index = record
            .view
            .expected_members
            .iter()
            .position(|member| member.character_id == actor.character_id)
            .ok_or(Phase2Error::NotFound)?;
        if !is_story_route(record.view.route_id.as_str()) {
            return Err(Phase2Error::NotFound);
        }
        let marker = record.scene_markers[index].ok_or(Phase2Error::NotFound)?;
        if let Some(done) = record.recovery_resolutions[index] {
            let outcome = match done {
                StoryRecoveryResolution::Reconciled(_) => StoryTravelRecoveryOutcome::Reconciled,
                StoryRecoveryResolution::Abandoned => StoryTravelRecoveryOutcome::Abandoned,
            };
            let requested = match request.action {
                StoryTravelRecoveryAction::Reconcile => StoryTravelRecoveryOutcome::Reconciled,
                StoryTravelRecoveryAction::Abandon => StoryTravelRecoveryOutcome::Abandoned,
            };
            return if outcome == requested {
                Ok((None, marker, Some(outcome), record.view.route_id.clone()))
            } else {
                Err(Phase2Error::Conflict)
            };
        }
        if record.view.status != GroupTravelProposalStatus::Cancelled
            || state
                .groups
                .get(&record.view.group_id)
                .is_none_or(|group| group.status != GroupStatus::Closed)
            || state
                .active_group_by_member
                .contains_key(&actor.character_id)
            || story_route_definition(record.view.route_id.as_str()).is_none_or(|definition| {
                record.view.source != definition.source
                    || record.view.destination != definition.destination
            })
            || record.view.expected_members[index].world_revision
                != state.characters[&actor.character_id].world_revision
            || state.characters[&actor.character_id].state.world_zone != record.view.source
        {
            return Err(Phase2Error::Conflict);
        }
        if state
            .group_travel_proposals
            .values()
            .filter(|other| {
                other.view.route_id == record.view.route_id
                    && other
                        .view
                        .expected_members
                        .iter()
                        .enumerate()
                        .any(|(i, member)| {
                            member.character_id == actor.character_id
                                && other.scene_markers[i].is_some()
                                && other.recovery_resolutions[i].is_none()
                        })
            })
            .count()
            != 1
        {
            return Err(Phase2Error::Conflict);
        }
        let candidates: Vec<_> = state
            .snapshots
            .values()
            .filter(|snapshot| {
                snapshot.character_id == actor.character_id
                    && snapshot.parent_revision == marker.fence.current_revision
                    && snapshot.created_at.value() >= marker.marked_at
            })
            .cloned()
            .collect();
        if candidates.len() > 1 {
            return Err(Phase2Error::Conflict);
        }
        let snapshot = candidates.into_iter().next();
        let head = &state.characters[&actor.character_id];
        match request.action {
            StoryTravelRecoveryAction::Reconcile => {
                let selected = snapshot.as_ref().ok_or(Phase2Error::Conflict)?;
                if record.scene_receipts[index].is_some_and(|receipt| {
                    receipt.snapshot_id != selected.snapshot_id
                        || receipt.revision != selected.revision
                }) {
                    return Err(Phase2Error::Conflict);
                }
                if head.active_snapshot != Some(selected.snapshot_id)
                    || head.revision != selected.revision
                    || fence.current_revision != selected.revision
                    || selected.session_id != marker.fence.session_id
                    || selected.session_epoch != marker.fence.session_epoch
                    || state
                        .snapshot_by_revision
                        .get(&(actor.character_id, selected.revision))
                        != Some(&selected.snapshot_id)
                {
                    return Err(Phase2Error::Conflict);
                }
            }
            StoryTravelRecoveryAction::Abandon => {
                if record.scene_receipts[index].is_some()
                    || snapshot.is_some()
                    || fence.session_id == marker.fence.session_id
                    || fence.session_epoch.value() <= marker.fence.session_epoch.value()
                    || head.revision != marker.fence.current_revision
                    || fence.current_revision != head.revision
                {
                    return Err(Phase2Error::Conflict);
                }
            }
        }
        Ok((snapshot, marker, None, record.view.route_id.clone()))
    })?;
    if let Some(outcome) = outcome {
        return Ok(StoryTravelRecoveryResolutionView {
            api_version: ApiVersion::V1,
            proposal_id,
            outcome,
        });
    }
    if let Some(snapshot) = &snapshot {
        let sav_file = snapshot
            .files
            .iter()
            .find(|file| file.artifact == coop_cloud::ArtifactIdentity::CharacterSav)
            .ok_or(Phase2Error::Conflict)?;
        let object_key = Store::object_key(
            actor.character_id,
            snapshot.snapshot_id,
            coop_cloud::ArtifactIdentity::CharacterSav,
        );
        let sav_bytes = store
            .objects
            .get(&object_key)?
            .ok_or(Phase2Error::Conflict)?;
        sav_file
            .verify_bytes(&sav_bytes)
            .map_err(|_| Phase2Error::Conflict)?;
        let registry = coop_save::RegistryContract::new(
            coop_protocol::IDENTITY_REGISTRY_VERSION,
            coop_protocol::IDENTITY_REGISTRY_DIGEST,
        );
        let validated =
            coop_save::parse(&sav_bytes, registry).map_err(|_| Phase2Error::Conflict)?;
        if !story_save_matches_route(route_id.as_str(), &validated) {
            return Err(Phase2Error::Conflict);
        }
    }
    store.write_transaction(|state| {
        authenticate_caller(state, actor, actor.character_id)?;
        lease_matches(state, actor.character_id, fence, store.now())?;
        let record = state
            .group_travel_proposals
            .get(&proposal_id)
            .ok_or(Phase2Error::NotFound)?;
        let index = record
            .view
            .expected_members
            .iter()
            .position(|member| member.character_id == actor.character_id)
            .ok_or(Phase2Error::NotFound)?;
        if record.scene_markers[index] != Some(marker)
            || record.recovery_resolutions[index].is_some()
            || record.view.status != GroupTravelProposalStatus::Cancelled
            || state
                .groups
                .get(&record.view.group_id)
                .is_none_or(|group| group.status != GroupStatus::Closed)
            || state
                .active_group_by_member
                .contains_key(&actor.character_id)
        {
            return Err(Phase2Error::Conflict);
        }
        let head = state
            .characters
            .get(&actor.character_id)
            .ok_or(Phase2Error::Conflict)?;
        if head.world_revision != record.view.expected_members[index].world_revision {
            return Err(Phase2Error::Conflict);
        }
        if head.state.world_zone != record.view.source {
            return Err(Phase2Error::Conflict);
        }
        let resolution = if let Some(snapshot) = &snapshot {
            if record.scene_receipts[index].is_some_and(|receipt| {
                receipt.snapshot_id != snapshot.snapshot_id || receipt.revision != snapshot.revision
            }) {
                return Err(Phase2Error::Conflict);
            }
            if state.snapshots.get(&snapshot.snapshot_id) != Some(snapshot)
                || head.active_snapshot != Some(snapshot.snapshot_id)
                || head.revision != snapshot.revision
                || fence.current_revision != snapshot.revision
                || snapshot.session_id != marker.fence.session_id
                || snapshot.session_epoch != marker.fence.session_epoch
                || state
                    .snapshot_by_revision
                    .get(&(actor.character_id, snapshot.revision))
                    != Some(&snapshot.snapshot_id)
                || state
                    .snapshots
                    .values()
                    .filter(|candidate| {
                        candidate.character_id == actor.character_id
                            && candidate.parent_revision == marker.fence.current_revision
                            && candidate.created_at.value() >= marker.marked_at
                    })
                    .count()
                    != 1
            {
                return Err(Phase2Error::Conflict);
            }
            StoryRecoveryResolution::Reconciled(StorySceneReceipt {
                snapshot_id: snapshot.snapshot_id,
                revision: snapshot.revision,
            })
        } else {
            if record.scene_receipts[index].is_some()
                || state.snapshots.values().any(|candidate| {
                    candidate.character_id == actor.character_id
                        && candidate.parent_revision == marker.fence.current_revision
                        && candidate.created_at.value() >= marker.marked_at
                })
                || fence.session_id == marker.fence.session_id
                || fence.session_epoch.value() <= marker.fence.session_epoch.value()
                || head.revision != marker.fence.current_revision
                || fence.current_revision != head.revision
            {
                return Err(Phase2Error::Conflict);
            }
            StoryRecoveryResolution::Abandoned
        };
        let next_revision = if matches!(resolution, StoryRecoveryResolution::Reconciled(_)) {
            Some(
                head.world_revision
                    .checked_add(1)
                    .filter(|revision| *revision <= MAX_WORLD_REVISION)
                    .ok_or(Phase2Error::Conflict)?,
            )
        } else {
            None
        };
        if let Some(next) = next_revision {
            let head = state
                .characters
                .get_mut(&actor.character_id)
                .expect("checked character");
            head.state.world_zone = story_route_definition(route_id.as_str())
                .ok_or(Phase2Error::Conflict)?
                .destination;
            head.world_revision = next;
        }
        state
            .group_travel_proposals
            .get_mut(&proposal_id)
            .expect("checked proposal")
            .recovery_resolutions[index] = Some(resolution);
        Ok(StoryTravelRecoveryResolutionView {
            api_version: ApiVersion::V1,
            proposal_id,
            outcome: if next_revision.is_some() {
                StoryTravelRecoveryOutcome::Reconciled
            } else {
                StoryTravelRecoveryOutcome::Abandoned
            },
        })
    })
}

fn story_member_index(
    view: &GroupTravelProposalView,
    actor: CharacterId,
    group_id: GroupId,
) -> Result<usize, Phase2Error> {
    if view.group_id != group_id || !is_story_route(view.route_id.as_str()) {
        return Err(Phase2Error::NotFound);
    }
    view.expected_members
        .iter()
        .position(|member| member.character_id == actor)
        .ok_or(Phase2Error::NotFound)
}

fn story_baseline_matches(
    state: &super::storage::State,
    view: &GroupTravelProposalView,
) -> Result<(), Phase2Error> {
    let group = state
        .groups
        .get(&view.group_id)
        .ok_or(Phase2Error::Conflict)?;
    if group.status != GroupStatus::Active
        || group.zone != view.source
        || group.zone_revision != view.expected_group_zone_revision
        || group.group.members() != view.expected_members.map(|member| member.character_id)
        || state.live_group_travel_by_group.get(&view.group_id) != Some(&view.proposal_id)
    {
        return Err(Phase2Error::Conflict);
    }
    for member in &view.expected_members {
        let character = state
            .characters
            .get(&member.character_id)
            .ok_or(Phase2Error::Conflict)?;
        if character.world_revision != member.world_revision
            || state.active_group_by_member.get(&member.character_id) != Some(&view.group_id)
            || state.live_group_travel_by_member.get(&member.character_id)
                != Some(&view.proposal_id)
        {
            return Err(Phase2Error::Conflict);
        }
    }
    Ok(())
}

/// Records the precise accepted proposal and ROM scene completion before a
/// post-scene save. A marker alone never moves either member.
pub(crate) fn mark_story_scene(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    proposal_id: GroupTravelProposalId,
    request: &GroupTravelSceneMarkerRequest,
) -> Result<GroupTravelProposalView, Phase2Error> {
    if request.api_version.value() != 1 || request.scene_nonce == 0 {
        return Err(Phase2Error::InvalidRequest);
    }
    let now = store.now();
    store.write_transaction(|state| {
        authenticate_caller(state, actor, request.character_id)?;
        lease_matches(state, actor.character_id, request.fence(), now)?;
        prune_travel_proposal_state(state, now);
        let record = state
            .group_travel_proposals
            .get(&proposal_id)
            .ok_or(Phase2Error::NotFound)?;
        let index = story_member_index(&record.view, actor.character_id, group_id)?;
        if let Some(known) = record.scene_markers[index] {
            if known.fence == request.fence() && known.nonce == request.scene_nonce {
                return Ok(record.view.clone());
            }
            return Err(Phase2Error::Conflict);
        }
        if record.view.status != GroupTravelProposalStatus::AwaitingSceneReceipts {
            return Err(Phase2Error::Conflict);
        }
        if state.group_travel_proposals.iter().any(|(id, existing)| {
            *id != proposal_id
                && is_story_route(existing.view.route_id.as_str())
                && has_unresolved_story_marker(existing)
                && existing.view.expected_members.iter().any(|member| {
                    record
                        .view
                        .expected_members
                        .iter()
                        .any(|current| member.character_id == current.character_id)
                })
        }) {
            return Err(Phase2Error::Conflict);
        }
        story_baseline_matches(state, &record.view)?;
        let record = state
            .group_travel_proposals
            .get_mut(&proposal_id)
            .expect("proposal exists");
        record.scene_markers[index] = Some(StorySceneMarker {
            fence: request.fence(),
            nonce: request.scene_nonce,
            marked_at: now,
        });
        record.view.scene_marked_by[index] = true;
        Ok(record.view.clone())
    })
}

/// Records immutable selected-slot evidence from one finalized save. The
/// second distinct receipt commits only the shared world zone, atomically in
/// repository state; physical save artifacts are already finalized.
pub(crate) fn receipt_story_scene(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    proposal_id: GroupTravelProposalId,
    request: &GroupTravelSceneReceiptRequest,
) -> Result<GroupTravelProposalView, Phase2Error> {
    if request.api_version.value() != 1 {
        return Err(Phase2Error::InvalidRequest);
    }
    let now = store.now();
    let (snapshot, marker, route_id) = store.read_transaction(|state| {
        authenticate_caller(state, actor, request.character_id)?;
        lease_matches(state, actor.character_id, request.fence(), now)?;
        let record = state
            .group_travel_proposals
            .get(&proposal_id)
            .ok_or(Phase2Error::NotFound)?;
        let index = story_member_index(&record.view, actor.character_id, group_id)?;
        let marker = record.scene_markers[index].ok_or(Phase2Error::Conflict)?;
        let snapshot = state
            .snapshots
            .get(&request.snapshot_id)
            .ok_or(Phase2Error::NotFound)?;
        Ok::<_, Phase2Error>((snapshot.clone(), marker, record.view.route_id.clone()))
    })?;
    if snapshot.character_id != actor.character_id
        || snapshot.snapshot_id != request.snapshot_id
        || snapshot.revision != request.current_revision
        || snapshot.parent_revision != marker.fence.current_revision
        || snapshot.session_id != marker.fence.session_id
        || snapshot.session_epoch != marker.fence.session_epoch
        || snapshot.created_at.value() < marker.marked_at
    {
        return Err(Phase2Error::Conflict);
    }
    let sav_file = snapshot
        .files
        .iter()
        .find(|file| file.artifact == coop_cloud::ArtifactIdentity::CharacterSav)
        .ok_or(Phase2Error::Conflict)?;
    let object_key = Store::object_key(
        actor.character_id,
        request.snapshot_id,
        coop_cloud::ArtifactIdentity::CharacterSav,
    );
    let sav_bytes = store
        .objects
        .get(&object_key)?
        .ok_or(Phase2Error::Conflict)?;
    sav_file
        .verify_bytes(&sav_bytes)
        .map_err(|_| Phase2Error::Conflict)?;
    let registry = coop_save::RegistryContract::new(
        coop_protocol::IDENTITY_REGISTRY_VERSION,
        coop_protocol::IDENTITY_REGISTRY_DIGEST,
    );
    let validated = coop_save::parse(&sav_bytes, registry).map_err(|_| Phase2Error::Conflict)?;
    if !story_save_matches_route(route_id.as_str(), &validated) {
        return Err(Phase2Error::Conflict);
    }
    store.write_transaction(|state| {
        authenticate_caller(state, actor, request.character_id)?;
        lease_matches(state, actor.character_id, request.fence(), store.now())?;
        prune_travel_proposal_state(state, store.now());
        let record = state
            .group_travel_proposals
            .get(&proposal_id)
            .ok_or(Phase2Error::NotFound)?;
        let index = story_member_index(&record.view, actor.character_id, group_id)?;
        let exact = StorySceneReceipt {
            snapshot_id: request.snapshot_id,
            revision: request.current_revision,
        };
        if state.snapshots.get(&request.snapshot_id) != Some(&snapshot) {
            return Err(Phase2Error::Conflict);
        }
        if let Some(known) = record.scene_receipts[index] {
            return if known == exact {
                Ok(record.view.clone())
            } else {
                Err(Phase2Error::Conflict)
            };
        }
        if !matches!(
            record.view.status,
            GroupTravelProposalStatus::AwaitingSceneReceipts | GroupTravelProposalStatus::Suspended
        ) || record.scene_markers[index] != Some(marker)
            || state
                .characters
                .get(&actor.character_id)
                .is_none_or(|character| {
                    character.active_snapshot != Some(request.snapshot_id)
                        || character.revision != request.current_revision
                })
        {
            return Err(Phase2Error::Conflict);
        }
        story_baseline_matches(state, &record.view)?;
        // In-memory repositories do not roll back a callback that returns an
        // error, so finish every fallible check before the first mutation.
        let snapshot_view = record.view.clone();
        let mut receipts = record.scene_receipts;
        receipts[index] = Some(exact);
        let final_receipt = receipts.iter().all(Option::is_some);
        let mut zone_revision = 0;
        let mut member_views = snapshot_view.expected_members;
        if final_receipt {
            for (index, member) in snapshot_view.expected_members.iter().enumerate() {
                let head = state
                    .characters
                    .get(&member.character_id)
                    .ok_or(Phase2Error::Conflict)?;
                let receipt = receipts[index].ok_or(Phase2Error::Conflict)?;
                if head.active_snapshot != Some(receipt.snapshot_id)
                    || head.revision != receipt.revision
                {
                    return Err(Phase2Error::Conflict);
                }
                active_member_lease(state, member.character_id, store.now())?;
            }
            let group = state.groups.get(&group_id).ok_or(Phase2Error::Conflict)?;
            zone_revision = group
                .zone_revision
                .checked_add(1)
                .filter(|revision| *revision <= MAX_WORLD_REVISION)
                .ok_or(Phase2Error::Internal)?;
            for member in &mut member_views {
                member.world_revision = member
                    .world_revision
                    .checked_add(1)
                    .filter(|revision| *revision <= MAX_WORLD_REVISION)
                    .ok_or(Phase2Error::Internal)?;
            }
        }
        let record = state
            .group_travel_proposals
            .get_mut(&proposal_id)
            .expect("proposal exists");
        record.scene_receipts[index] = Some(exact);
        record.view.scene_receipted_by[index] = true;
        if !final_receipt {
            return Ok(record.view.clone());
        }
        for member in &member_views {
            let head = state
                .characters
                .get_mut(&member.character_id)
                .expect("member exists");
            head.state.world_zone = snapshot_view.destination.clone();
            head.world_revision = member.world_revision;
        }
        if let Some(zones) = state.group_member_world_zones.get_mut(&group_id) {
            zones.fill(snapshot_view.destination.clone());
        }
        let group = state.groups.get_mut(&group_id).expect("group exists");
        group.zone = snapshot_view.destination.clone();
        group.zone_revision = zone_revision;
        let record = state
            .group_travel_proposals
            .get_mut(&proposal_id)
            .expect("proposal exists");
        record.view.status = GroupTravelProposalStatus::Committed;
        record.view.commit = Some(GroupTravelCommit {
            group_zone_revision: zone_revision,
            members: member_views,
            destination: snapshot_view.destination,
        });
        record.retain_until = Some(store.now().saturating_add(TRAVEL_PROPOSAL_REPLAY_TTL_MS));
        let committed = record.view.clone();
        release_proposal_indexes(state, proposal_id);
        Ok(committed)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cable_car_consent_routes_are_directional_and_station_bound() {
        for (id, source, destination) in [
            (
                "HOENN:ROUTE112_MT_CHIMNEY_CABLE_CAR",
                "ROUTE112_CABLE_CAR_STATION",
                "MT_CHIMNEY_CABLE_CAR_STATION",
            ),
            (
                "HOENN:MT_CHIMNEY_ROUTE112_CABLE_CAR",
                "MT_CHIMNEY_CABLE_CAR_STATION",
                "ROUTE112_CABLE_CAR_STATION",
            ),
        ] {
            let definition = consent_route_definition(id).unwrap();
            assert_eq!(definition.source.map.as_str(), source);
            assert_eq!(definition.destination.map.as_str(), destination);
            assert!(!definition.source_any);
            assert!(departure_matches_route(
                &definition,
                GroupTravelDeparture::CableCar
            ));
            assert!(!departure_matches_route(
                &definition,
                GroupTravelDeparture::Ferry
            ));
        }
    }

    #[test]
    fn teleport_uses_consent_fly_routes_except_littleroot() {
        let destination = consent_route_definition("JOHTO:FLY_NEW_BARK_TOWN").unwrap();
        assert!(destination.source_any);
        assert!(departure_matches_route(
            &destination,
            GroupTravelDeparture::Teleport
        ));
        let littleroot = consent_route_definition("HOENN:FLY_LITTLEROOT").unwrap();
        assert!(!departure_matches_route(
            &littleroot,
            GroupTravelDeparture::Teleport
        ));
    }

    #[test]
    fn bill_story_sources_are_exact_and_publicly_consent_based() {
        let outbound = bill_cinnabar_one_story_route();
        let inbound = bill_one_cinnabar_story_route();
        assert_eq!(outbound.source.map.as_str(), "CINNABAR_ISLAND");
        assert_eq!(
            outbound.destination.map.as_str(),
            "ONE_ISLAND_POKEMON_CENTER_1F"
        );
        assert_eq!(inbound.source.map.as_str(), "ONE_ISLAND_POKEMON_CENTER_1F");
        assert_eq!(inbound.destination.map.as_str(), "CINNABAR_ISLAND");
        assert!(!outbound.source_any && !inbound.source_any);
        assert!(consent_route_definition(BILL_CINNABAR_ONE_ROUTE_ID).is_ok());
        assert!(consent_route_definition(BILL_ONE_CINNABAR_ROUTE_ID).is_ok());
    }

    #[test]
    fn bill_story_proposals_wait_for_both_scene_receipts_before_moving_either_member() {
        for definition in [
            bill_cinnabar_one_story_route(),
            bill_one_cinnabar_story_route(),
        ] {
            let app = super::super::Phase2App::test();
            let (first, first_lease, second, second_lease, group_id) = two_member_group(&app);
            app.store
                .write_transaction(|state| {
                    for member in [first, second] {
                        let progress = [
                            RegionId::Hoenn,
                            RegionId::Kanto,
                            RegionId::Johto,
                            RegionId::Sevii,
                        ]
                        .into_iter()
                        .map(|region| {
                            RegionalProgress::new(region, 0, 0, vec![], vec![])
                                .map_err(|_| Phase2Error::Internal)
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                        state
                            .characters
                            .get_mut(&member.character_id)
                            .ok_or(Phase2Error::Internal)?
                            .state = CharacterCloudState::new(
                            member.character_id,
                            definition.source.clone(),
                            progress,
                        )
                        .map_err(|_| Phase2Error::Internal)?;
                    }
                    let group = state
                        .groups
                        .get_mut(&group_id)
                        .ok_or(Phase2Error::Internal)?;
                    group.zone = definition.source.clone();
                    group.zone_revision = 0;
                    Ok::<(), Phase2Error>(())
                })
                .expect("both members have Bill source and destination progress");
            let proposal = create_travel_proposal(
                &app.store,
                first,
                group_id,
                &GroupTravelProposalRequest::new_with_departure(
                    first_lease.fence(),
                    definition.id,
                    GroupTravelDeparture::Ferry,
                    IdempotencyKey::new(Uuid::new_v4()).expect("key"),
                )
                .expect("request"),
            )
            .expect("Bill proposal");
            let accepted = act_on_travel_proposal(
                &app.store,
                second,
                group_id,
                proposal.proposal_id,
                &GroupTravelActionRequest::new(
                    second_lease.fence(),
                    GroupTravelAction::Accept,
                    IdempotencyKey::new(Uuid::new_v4()).expect("key"),
                ),
            )
            .expect("Bill consent");
            assert_eq!(
                accepted.status,
                GroupTravelProposalStatus::AwaitingSceneReceipts
            );
            assert!(accepted.commit.is_none());
            app.store
                .read_transaction(|state| {
                    assert_eq!(state.groups[&group_id].zone, definition.source);
                    for member in [first, second] {
                        assert_eq!(
                            state.characters[&member.character_id].state.world_zone,
                            definition.source
                        );
                    }
                    Ok::<_, Phase2Error>(())
                })
                .expect("no world mutation before scene receipts");
        }
    }
    use coop_cloud::CharacterCloudState;
    use coop_cloud::GroupTravelRequest;
    use coop_cloud::{
        AcquireLeaseRequest, ClientInstanceId, CreateGroupInvitationRequest, IdempotencyKey,
        InvitationCode, OnlineAction, OnlineActionRequest, OnlineActionResponse,
        OnlineSnapshotRequest, Password, RegisterRequest,
    };
    use coop_protocol::{RegionalProgress, WorldZone};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use uuid::Uuid;

    #[derive(Clone)]
    struct ToggleRepository {
        inner: super::super::super::phase2::InMemoryRepository,
        fail_writes: Arc<AtomicBool>,
    }

    impl super::super::super::phase2::Repository for ToggleRepository {
        fn read_transaction(
            &self,
            operation: &mut dyn FnMut(
                &super::super::super::phase2::storage::State,
            ) -> Result<
                (),
                super::super::super::phase2::storage::StorageError,
            >,
        ) -> Result<(), super::super::super::phase2::storage::StorageError> {
            self.inner.read_transaction(operation)
        }

        fn write_transaction(
            &self,
            operation: &mut dyn FnMut(
                &mut super::super::super::phase2::storage::State,
            ) -> Result<
                (),
                super::super::super::phase2::storage::StorageError,
            >,
        ) -> Result<(), super::super::super::phase2::storage::StorageError> {
            if self.fail_writes.load(Ordering::Acquire) {
                return Err(super::super::super::phase2::storage::StorageError::Transaction);
            }
            self.inner.write_transaction(operation)
        }
    }

    fn app_with_toggle_repository() -> (super::super::Phase2App, Arc<AtomicBool>) {
        let fail_writes = Arc::new(AtomicBool::new(false));
        let repository = ToggleRepository {
            inner: super::super::super::phase2::InMemoryRepository::new(),
            fail_writes: fail_writes.clone(),
        };
        let config = super::super::Phase2Config::local(
            vec![0x55; 32],
            coop_cloud::SigningPrivateKey::from_bytes([7; 32]),
            "local-test-key",
        )
        .expect("test config")
        .with_test_adapters(
            Arc::new(super::super::FixedClock::new(1_700_000_000_000)),
            Arc::new(super::super::FixedEntropy::new((0_u8..=255).collect())),
        )
        .with_password_engine(Arc::new(
            super::super::ArgonPasswordEngine::new(8_192, 1, 1).expect("test Argon2 policy"),
        ))
        .with_adapters(
            Arc::new(repository),
            Arc::new(super::super::InMemoryObjectStore::new()),
        );
        (
            super::super::Phase2App::new(config).expect("test config is local"),
            fail_writes,
        )
    }

    fn app_with_clock() -> (super::super::Phase2App, Arc<super::super::FixedClock>) {
        let clock = Arc::new(super::super::FixedClock::new(1_700_000_000_000));
        let config = super::super::Phase2Config::local(
            vec![0x55; 32],
            coop_cloud::SigningPrivateKey::from_bytes([7; 32]),
            "local-test-key",
        )
        .expect("test config")
        .with_test_adapters(
            clock.clone(),
            Arc::new(super::super::FixedEntropy::new((0_u8..=255).collect())),
        )
        .with_password_engine(Arc::new(
            super::super::ArgonPasswordEngine::new(8_192, 1, 1).expect("test Argon2 policy"),
        ));
        (
            super::super::Phase2App::new(config).expect("test config is local"),
            clock,
        )
    }

    fn account(
        app: &super::super::Phase2App,
        name: &str,
        invitation: &str,
    ) -> (AuthenticatedActor, coop_cloud::LeaseContract) {
        app.add_invitation(invitation).expect("invite");
        let registration = app
            .register(
                RegisterRequest::new(
                    name,
                    Password::new("correct horse battery staple").expect("password"),
                    InvitationCode::new(invitation).expect("invitation"),
                )
                .expect("register"),
            )
            .expect("registered");
        let actor = AuthenticatedActor {
            user_id: registration.user_id,
            character_id: registration.character_id,
        };
        let client = ClientInstanceId::new(Uuid::new_v4()).expect("client");
        let lease = app
            .acquire(
                actor,
                AcquireLeaseRequest::new(
                    registration.character_id,
                    client,
                    IdempotencyKey::new(Uuid::new_v4()).expect("key"),
                ),
            )
            .expect("lease");
        (actor, lease)
    }

    fn two_member_group(
        app: &super::super::Phase2App,
    ) -> (
        AuthenticatedActor,
        coop_cloud::LeaseContract,
        AuthenticatedActor,
        coop_cloud::LeaseContract,
        coop_cloud::GroupId,
    ) {
        let (first_actor, first_lease) = account(app, "first", "first-invite");
        let (second_actor, second_lease) = account(app, "second", "second-invite");
        let invitation = create_invitation(
            &app.store,
            first_actor,
            &CreateGroupInvitationRequest::new(
                first_lease.fence(),
                second_actor.character_id,
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            ),
        )
        .expect("group invitation");
        let accepted = accept_invitation(
            &app.store,
            second_actor,
            invitation.invitation_id,
            &AcceptGroupInvitationRequest::new(
                second_lease.fence(),
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            ),
        )
        .expect("accepted");
        (
            first_actor,
            first_lease,
            second_actor,
            second_lease,
            accepted.group.group_id,
        )
    }

    #[test]
    fn remembered_partner_invitation_allows_remote_maps_and_is_idempotent() {
        let app = super::super::Phase2App::test();
        let (first, first_lease, second, second_lease, group_id) = two_member_group(&app);
        let first_zone = WorldZone::new(RegionId::Hoenn, "ROUTE104", 1).expect("first zone");
        let second_zone = WorldZone::new(RegionId::Hoenn, "ROUTE105", 1).expect("second zone");
        app.store
            .write_transaction(|state| {
                state.groups.get_mut(&group_id).expect("group").status = GroupStatus::Closed;
                state.active_group_by_member.remove(&first.character_id);
                state.active_group_by_member.remove(&second.character_id);
                state
                    .characters
                    .get_mut(&first.character_id)
                    .expect("first")
                    .state
                    .world_zone = first_zone.clone();
                state
                    .characters
                    .get_mut(&second.character_id)
                    .expect("second")
                    .state
                    .world_zone = second_zone.clone();
                Ok::<_, Phase2Error>(())
            })
            .expect("close prior group");

        let snapshot = super::super::online::snapshot(
            &app,
            second,
            &OnlineSnapshotRequest {
                api_version: coop_cloud::ApiVersion::V1,
                fence: second_lease.fence(),
                incoming_after: None,
            },
        )
        .expect("snapshot");
        let remembered = snapshot.last_partner.expect("remembered partner");
        assert_eq!(remembered.character_id, first.character_id);
        assert_eq!(remembered.username.as_str(), "first");

        let action = OnlineActionRequest {
            api_version: coop_cloud::ApiVersion::V1,
            fence: second_lease.fence(),
            idempotency_key: IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            action: OnlineAction::InviteLastPartner,
        };
        let invited = super::super::online::action(&app, second, &action).expect("invite");
        let invitation = match invited {
            OnlineActionResponse::Invited { invitation } => invitation,
            other => panic!("unexpected response: {other:?}"),
        };
        assert_eq!(invitation.invitee_character_id, first.character_id);
        assert_eq!(
            super::super::online::action(&app, second, &action),
            Ok(OnlineActionResponse::Invited { invitation })
        );
        let accepted = accept_invitation(
            &app.store,
            first,
            invitation.invitation_id,
            &AcceptGroupInvitationRequest::new(
                first_lease.fence(),
                IdempotencyKey::new(Uuid::new_v4()).expect("accept key"),
            ),
        )
        .expect("cross-map acceptance");
        assert_eq!(
            accepted.group.member_world_zones,
            [first_zone.clone(), second_zone]
        );
        app.store
            .read_transaction(|state| {
                assert_eq!(state.groups[&accepted.group.group_id].zone, first_zone);
                Ok::<_, Phase2Error>(())
            })
            .expect("canonical shared group zone");
    }

    #[test]
    fn remembered_partner_invitations_share_sender_rate_limit() {
        let app = super::super::Phase2App::test();
        let (first, _first_lease, second, second_lease, group_id) = two_member_group(&app);
        app.store
            .write_transaction(|state| {
                state.groups.get_mut(&group_id).expect("group").status = GroupStatus::Closed;
                state.active_group_by_member.remove(&first.character_id);
                state.active_group_by_member.remove(&second.character_id);
                Ok::<_, Phase2Error>(())
            })
            .expect("close prior group");

        let invite = || OnlineActionRequest {
            api_version: coop_cloud::ApiVersion::V1,
            fence: second_lease.fence(),
            idempotency_key: IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            action: OnlineAction::InviteLastPartner,
        };
        for _ in 0..MAX_INVITATIONS_PER_SENDER_PER_MINUTE {
            assert!(matches!(
                super::super::online::action(&app, second, &invite()),
                Ok(OnlineActionResponse::Invited { .. })
            ));
        }
        assert_eq!(
            super::super::online::action(&app, second, &invite()),
            Err(Phase2Error::Busy)
        );
    }

    #[test]
    fn remembered_partner_invitation_denies_stale_partner() {
        let (app, _clock) = app_with_clock();
        let (first, first_lease, second, _second_lease, group_id) = two_member_group(&app);
        app.store
            .write_transaction(|state| {
                state.groups.get_mut(&group_id).expect("group").status = GroupStatus::Closed;
                state.active_group_by_member.remove(&first.character_id);
                state.active_group_by_member.remove(&second.character_id);
                state
                    .leases
                    .get_mut(&second.character_id)
                    .expect("second lease")
                    .contract
                    .expires_at = Store::unix_timestamp(app.store.now() - 1).expect("timestamp");
                Ok::<_, Phase2Error>(())
            })
            .expect("close prior group");
        let result = super::super::online::action(
            &app,
            first,
            &OnlineActionRequest {
                api_version: coop_cloud::ApiVersion::V1,
                fence: first_lease.fence(),
                idempotency_key: IdempotencyKey::new(Uuid::new_v4()).expect("key"),
                action: OnlineAction::InviteLastPartner,
            },
        );
        assert_eq!(result, Err(Phase2Error::Forbidden));
    }

    #[test]
    fn expired_member_closes_group_and_releases_both_indexes_once() {
        let (app, clock) = app_with_clock();
        let (first, first_lease, second, _second_lease, group_id) = two_member_group(&app);
        assert!(
            super::super::sessions::expire_groups(&app.store)
                .expect("still within reconnect window")
                .is_empty()
        );
        let source =
            WorldZone::new(RegionId::Johto, "GOLDENROD_CITY_TRAIN_STATION", 1).expect("source");
        set_consent_progress(&app, group_id, [first, second], source.clone());
        let proposal = create_travel_proposal(
            &app.store,
            first,
            group_id,
            &GroupTravelProposalRequest::new(
                first_lease.fence(),
                "JOHTO:GOLDENROD_KANTO_LATER_TRAIN",
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            )
            .expect("request"),
        )
        .expect("proposal");
        app.store
            .write_transaction(|state| {
                state
                    .group_member_world_zones
                    .insert(group_id, [source.clone(), source.clone()]);
                Ok::<_, Phase2Error>(())
            })
            .expect("persist member zones");
        let initial_grace = app
            .store
            .read_transaction(|state| {
                Ok::<_, Phase2Error>(state.leases[&first.character_id].grace_until)
            })
            .expect("lease");
        clock.advance(
            super::super::storage::LEASE_TTL_MS + super::super::storage::RECONNECT_GRACE_MS + 1,
        );
        let now = app.store.now();
        assert!(now > initial_grace);
        app.store
            .write_transaction(|state| {
                state
                    .leases
                    .get_mut(&second.character_id)
                    .expect("partner lease")
                    .grace_until = now + 1;
                Ok::<_, Phase2Error>(())
            })
            .expect("partner remains connected");

        let events = super::super::sessions::expire_groups(&app.store).expect("expiry sweep");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].group_id, group_id);
        assert_eq!(events[0].expired_member, first.character_id);
        assert_eq!(events[0].partner, second.character_id);
        app.store
            .read_transaction(|state| {
                assert_eq!(
                    state.groups[&group_id].status,
                    super::super::storage::GroupStatus::Closed
                );
                assert!(
                    !state
                        .active_group_by_member
                        .contains_key(&first.character_id)
                );
                assert!(
                    !state
                        .active_group_by_member
                        .contains_key(&second.character_id)
                );
                assert_eq!(
                    state.group_travel_proposals[&proposal.proposal_id]
                        .view
                        .status,
                    GroupTravelProposalStatus::Cancelled
                );
                assert!(!state.live_group_travel_by_group.contains_key(&group_id));
                assert!(
                    !state
                        .live_group_travel_by_member
                        .contains_key(&first.character_id)
                );
                assert!(
                    !state
                        .live_group_travel_by_member
                        .contains_key(&second.character_id)
                );
                assert!(!state.group_member_world_zones.contains_key(&group_id));
                Ok::<_, Phase2Error>(())
            })
            .expect("closed group");
        assert!(
            super::super::sessions::expire_groups(&app.store)
                .expect("repeat sweep")
                .is_empty()
        );
    }

    fn set_progress(
        app: &super::super::Phase2App,
        actor: AuthenticatedActor,
        badges: u16,
        include_destination: bool,
    ) {
        let source = WorldZone::new(RegionId::Hoenn, "SLATEPORT_CITY_HARBOR", 1).expect("source");
        let mut records =
            vec![RegionalProgress::new(RegionId::Hoenn, badges, 0, vec![], vec![]).expect("hoenn")];
        if include_destination {
            records
                .push(RegionalProgress::new(RegionId::Sevii, 0, 0, vec![], vec![]).expect("sevii"));
            records.push(
                RegionalProgress::new(RegionId::Kanto, 0xff, 99, vec![], vec![]).expect("kanto"),
            );
        }
        app.store
            .write_transaction(|state| {
                let character = state
                    .characters
                    .get_mut(&actor.character_id)
                    .ok_or(super::super::storage::StorageError::Transaction)?;
                character.state =
                    CharacterCloudState::new(actor.character_id, source.clone(), records.clone())
                        .map_err(|_| super::super::storage::StorageError::Transaction)?;
                if let Some(group_id) = state
                    .active_group_by_member
                    .get(&actor.character_id)
                    .copied()
                {
                    state.groups.get_mut(&group_id).expect("active group").zone = source;
                }
                Ok::<(), Phase2Error>(())
            })
            .expect("state");
    }

    fn set_consent_progress(
        app: &super::super::Phase2App,
        group_id: GroupId,
        members: [AuthenticatedActor; 2],
        source: WorldZone,
    ) {
        app.store
            .write_transaction(|state| {
                for actor in members {
                    let progress = vec![
                        RegionalProgress::new(RegionId::Johto, 0, 0, vec![], vec![])
                            .map_err(|_| Phase2Error::Internal)?,
                        RegionalProgress::new(RegionId::Kanto, 0, 0, vec![], vec![])
                            .map_err(|_| Phase2Error::Internal)?,
                        RegionalProgress::new(RegionId::Hoenn, 0, 0, vec![], vec![])
                            .map_err(|_| Phase2Error::Internal)?,
                    ];
                    let character = state
                        .characters
                        .get_mut(&actor.character_id)
                        .ok_or(Phase2Error::Internal)?;
                    character.state =
                        CharacterCloudState::new(actor.character_id, source.clone(), progress)
                            .map_err(|_| Phase2Error::Internal)?;
                }
                let group = state
                    .groups
                    .get_mut(&group_id)
                    .ok_or(Phase2Error::Internal)?;
                group.zone = source;
                group.zone_revision = 0;
                Ok::<(), Phase2Error>(())
            })
            .expect("consent state");
    }

    #[test]
    fn all_consent_routes_have_pinned_maps() {
        for route in consent_route_catalog().into_iter() {
            assert!(route.source.map_entry().is_ok());
            assert!(route.destination.map_entry().is_ok());
            assert!(consent_route_definition(route.id).is_ok());
        }
        assert!(consent_route_definition("JOHTO:TO_KANTO").is_err());
    }

    #[test]
    fn briney_routes_are_exact_and_first_voyage_is_available() {
        for (id, source, destination) in [
            (
                FIRST_BRINEY_ROUTE_ID,
                "ROUTE104_MR_BRINEYS_HOUSE",
                "DEWFORD_TOWN",
            ),
            (
                "HOENN:DEWFORD_BRINEY_HOUSE_FERRY",
                "DEWFORD_TOWN",
                "ROUTE104_MR_BRINEYS_HOUSE",
            ),
            ("HOENN:DEWFORD_ROUTE109_FERRY", "DEWFORD_TOWN", "ROUTE109"),
            ("HOENN:ROUTE109_DEWFORD_FERRY", "ROUTE109", "DEWFORD_TOWN"),
        ] {
            let definition = consent_route_definition(id).expect("Briney route");
            assert_eq!(definition.source.map.as_str(), source);
            assert_eq!(definition.destination.map.as_str(), destination);
            assert!(departure_matches_route(
                &definition,
                GroupTravelDeparture::Ferry
            ));
            assert!(!departure_matches_route(
                &definition,
                GroupTravelDeparture::SsaquaMaiden
            ));
        }
    }

    #[test]
    fn reverse_routes_require_the_pinned_kanto_era_and_normal_departure() {
        for (id, source) in [
            (
                "KANTO:ORIGINAL_VERMILION_JOHTO_FERRY",
                "KANTO_ORIGINAL_VERMILION_CITY_PORT_INSIDE",
            ),
            (
                "KANTO:LATER_VERMILION_JOHTO_FERRY",
                "KANTO_LATER_VERMILION_CITY_PORT_INSIDE",
            ),
            (
                "KANTO:ORIGINAL_SAFFRON_JOHTO_TRAIN",
                "KANTO_ORIGINAL_SAFFRON_CITY_TRAIN_STATION",
            ),
            (
                "KANTO:LATER_SAFFRON_JOHTO_TRAIN",
                "KANTO_LATER_SAFFRON_CITY_TRAIN_STATION",
            ),
            ("KANTO:ORIGINAL_ROUTE22_JOHTO_ROUTE22", "ROUTE22"),
            ("KANTO:LATER_ROUTE22_JOHTO_ROUTE22", "KANTO_LATER_ROUTE22"),
        ] {
            let definition = consent_route_definition(id).expect("reverse route");
            assert_eq!(definition.source.map.as_str(), source);
            assert!(!definition.source_any);
            assert!(!departure_matches_route(
                &definition,
                GroupTravelDeparture::SsaquaMaiden
            ));
        }
    }

    #[test]
    fn outbound_island_routes_require_the_exact_ferry_terminal() {
        for (id, source) in [
            (
                "JOHTO:OLIVINE_SOUTHERN_ISLAND_FERRY",
                "OLIVINE_CITY_PORT_INSIDE",
            ),
            (
                "JOHTO:OLIVINE_BIRTH_ISLAND_FERRY",
                "OLIVINE_CITY_PORT_INSIDE",
            ),
            (
                "JOHTO:OLIVINE_FARAWAY_ISLAND_FERRY",
                "OLIVINE_CITY_PORT_INSIDE",
            ),
            (
                "JOHTO:OLIVINE_BATTLE_FRONTIER_FERRY",
                "OLIVINE_CITY_PORT_INSIDE",
            ),
            (
                "KANTO_LATER:VERMILION_SOUTHERN_ISLAND_FERRY",
                "KANTO_LATER_VERMILION_CITY_PORT_INSIDE",
            ),
            (
                "KANTO_LATER:VERMILION_BIRTH_ISLAND_FERRY",
                "KANTO_LATER_VERMILION_CITY_PORT_INSIDE",
            ),
            (
                "KANTO_LATER:VERMILION_FARAWAY_ISLAND_FERRY",
                "KANTO_LATER_VERMILION_CITY_PORT_INSIDE",
            ),
            (
                "KANTO_LATER:VERMILION_BATTLE_FRONTIER_FERRY",
                "KANTO_LATER_VERMILION_CITY_PORT_INSIDE",
            ),
        ] {
            let definition = consent_route_definition(id).expect("outbound island route");
            assert_eq!(definition.source.map.as_str(), source);
            assert!(!definition.source_any);
            assert!(departure_matches_route(
                &definition,
                GroupTravelDeparture::Ferry
            ));
            assert!(!departure_matches_route(
                &definition,
                GroupTravelDeparture::SsaquaMaiden
            ));
        }
    }

    #[test]
    fn island_returns_reject_maiden_departure() {
        for id in [
            "HOENN:SOUTHERN_ISLAND_LILYCOVE_FERRY",
            "HOENN:BIRTH_ISLAND_LILYCOVE_FERRY",
            "HOENN:FARAWAY_ISLAND_LILYCOVE_FERRY",
            "HOENN:BATTLE_FRONTIER_SLATEPORT_FERRY",
            "HOENN:BATTLE_FRONTIER_LILYCOVE_FERRY",
        ] {
            let definition = consent_route_definition(id).expect("island return route");
            assert!(departure_matches_route(
                &definition,
                GroupTravelDeparture::Ferry
            ));
            assert!(!departure_matches_route(
                &definition,
                GroupTravelDeparture::SsaquaMaiden
            ));
        }
    }

    #[test]
    fn fly_catalog_preserves_wire_order_and_ever_grande_hole() {
        let expected = [
            "JOHTO:FLY_NEW_BARK_TOWN",
            "JOHTO:FLY_CHERRYGROVE_CITY",
            "JOHTO:FLY_VIOLET_CITY",
            "JOHTO:FLY_AZALEA_TOWN",
            "JOHTO:FLY_GOLDENROD_CITY",
            "JOHTO:FLY_ECRUTEAK_CITY",
            "JOHTO:FLY_OLIVINE_CITY",
            "JOHTO:FLY_CIANWOOD_CITY",
            "JOHTO:FLY_MAHOGANYTOWN",
            "JOHTO:FLY_BLACKTHORN_CITY",
            "HOENN:FLY_OLDALE_TOWN",
            "HOENN:FLY_DEWFORD_TOWN",
            "HOENN:FLY_LAVARIDGE_TOWN",
            "HOENN:FLY_FALLARBOR_TOWN",
            "HOENN:FLY_VERDANTURF_TOWN",
            "HOENN:FLY_PACIFIDLOG_TOWN",
            "HOENN:FLY_PETALBURG_CITY",
            "HOENN:FLY_SLATEPORT_CITY",
            "HOENN:FLY_MAUVILLE_CITY",
            "HOENN:FLY_RUSTBORO_CITY",
            "HOENN:FLY_FORTREE_CITY",
            "HOENN:FLY_LILYCOVE_CITY",
            "HOENN:FLY_MOSSDEEP_CITY",
            "HOENN:FLY_SOOTOPOLIS_CITY",
            "KANTO:FLY_PALLET_TOWN",
            "KANTO:FLY_VIRIDIAN_CITY",
            "KANTO:FLY_PEWTER_CITY",
            "KANTO:FLY_CERULEAN_CITY",
            "KANTO:FLY_LAVENDER_TOWN",
            "KANTO:FLY_VERMILION_CITY",
            "KANTO:FLY_CELADON_CITY",
            "KANTO:FLY_FUCHSIA_CITY",
            "KANTO:FLY_CINNABAR_ISLAND",
            "KANTO:FLY_INDIGO_PLATEAU",
            "KANTO:FLY_SAFFRON_CITY",
            "KANTO_LATER:FLY_PALLET_TOWN",
            "KANTO_LATER:FLY_VIRIDIAN_CITY",
            "KANTO_LATER:FLY_PEWTER_CITY",
            "KANTO_LATER:FLY_CERULEAN_CITY",
            "KANTO_LATER:FLY_LAVENDER_TOWN",
            "KANTO_LATER:FLY_VERMILION_CITY",
            "KANTO_LATER:FLY_CELADON_CITY",
            "KANTO_LATER:FLY_FUCHSIA_CITY",
            "KANTO_LATER:FLY_SAFFRON_CITY",
            "KANTO_LATER:FLY_CINNABAR_ISLAND",
            "SEVII:FLY_ONE_ISLAND",
            "SEVII:FLY_TWO_ISLAND",
            "SEVII:FLY_THREE_ISLAND",
            "SEVII:FLY_FOUR_ISLAND",
            "SEVII:FLY_FIVE_ISLAND",
            "SEVII:FLY_SEVEN_ISLAND",
            "SEVII:FLY_SIX_ISLAND",
            "KANTO:FLY_ROUTE_4_POKECENTER",
            "KANTO:FLY_ROUTE_10_POKECENTER",
            "HOENN:FLY_EVER_GRANDE_CITY_CENTER",
            "HOENN:FLY_EVER_GRANDE_CITY_LEAGUE",
            "HOENN:FLY_BATTLE_FRONTIER",
            "JOHTO:OLIVINE_SOUTHERN_ISLAND_FERRY",
            "JOHTO:OLIVINE_BIRTH_ISLAND_FERRY",
            "JOHTO:OLIVINE_FARAWAY_ISLAND_FERRY",
            "JOHTO:OLIVINE_BATTLE_FRONTIER_FERRY",
            "KANTO_LATER:VERMILION_SOUTHERN_ISLAND_FERRY",
            "KANTO_LATER:VERMILION_BIRTH_ISLAND_FERRY",
            "KANTO_LATER:VERMILION_FARAWAY_ISLAND_FERRY",
            "KANTO_LATER:VERMILION_BATTLE_FRONTIER_FERRY",
            "HOENN:SOUTHERN_ISLAND_LILYCOVE_FERRY",
            "HOENN:BIRTH_ISLAND_LILYCOVE_FERRY",
            "HOENN:FARAWAY_ISLAND_LILYCOVE_FERRY",
            "HOENN:BATTLE_FRONTIER_SLATEPORT_FERRY",
            "HOENN:BATTLE_FRONTIER_LILYCOVE_FERRY",
            "HOENN:LILYCOVE_SOUTHERN_ISLAND_FERRY",
            "HOENN:LILYCOVE_NAVEL_ROCK_FERRY",
            "HOENN:LILYCOVE_BIRTH_ISLAND_FERRY",
            "HOENN:LILYCOVE_FARAWAY_ISLAND_FERRY",
            "HOENN:LILYCOVE_BATTLE_FRONTIER_FERRY",
            "HOENN:SLATEPORT_BATTLE_FRONTIER_FERRY",
            "HOENN:NAVEL_ROCK_LILYCOVE_FERRY",
            "HOENN:SLATEPORT_SS_TIDAL_BOARD_FERRY",
            "HOENN:LILYCOVE_SS_TIDAL_BOARD_FERRY",
            "HOENN:SS_TIDAL_LILYCOVE_EXIT_FERRY",
            "HOENN:SS_TIDAL_SLATEPORT_EXIT_FERRY",
            "HOENN:DEWFORD_BRINEY_HOUSE_FERRY",
            "HOENN:DEWFORD_ROUTE109_FERRY",
            "HOENN:ROUTE109_DEWFORD_FERRY",
            "SEAGALLOP:VERMILION_ONE_FERRY",
            "SEAGALLOP:VERMILION_TWO_FERRY",
            "SEAGALLOP:VERMILION_THREE_FERRY",
            "SEAGALLOP:VERMILION_FOUR_FERRY",
            "SEAGALLOP:VERMILION_FIVE_FERRY",
            "SEAGALLOP:VERMILION_SIX_FERRY",
            "SEAGALLOP:VERMILION_SEVEN_FERRY",
            "SEAGALLOP:ONE_VERMILION_FERRY",
            "SEAGALLOP:ONE_TWO_FERRY",
            "SEAGALLOP:ONE_THREE_FERRY",
            "SEAGALLOP:ONE_FOUR_FERRY",
            "SEAGALLOP:ONE_FIVE_FERRY",
            "SEAGALLOP:ONE_SIX_FERRY",
            "SEAGALLOP:ONE_SEVEN_FERRY",
            "SEAGALLOP:TWO_VERMILION_FERRY",
            "SEAGALLOP:TWO_ONE_FERRY",
            "SEAGALLOP:TWO_THREE_FERRY",
            "SEAGALLOP:TWO_FOUR_FERRY",
            "SEAGALLOP:TWO_FIVE_FERRY",
            "SEAGALLOP:TWO_SIX_FERRY",
            "SEAGALLOP:TWO_SEVEN_FERRY",
            "SEAGALLOP:THREE_VERMILION_FERRY",
            "SEAGALLOP:THREE_ONE_FERRY",
            "SEAGALLOP:THREE_TWO_FERRY",
            "SEAGALLOP:THREE_FOUR_FERRY",
            "SEAGALLOP:THREE_FIVE_FERRY",
            "SEAGALLOP:THREE_SIX_FERRY",
            "SEAGALLOP:THREE_SEVEN_FERRY",
            "SEAGALLOP:FOUR_VERMILION_FERRY",
            "SEAGALLOP:FOUR_ONE_FERRY",
            "SEAGALLOP:FOUR_TWO_FERRY",
            "SEAGALLOP:FOUR_THREE_FERRY",
            "SEAGALLOP:FOUR_FIVE_FERRY",
            "SEAGALLOP:FOUR_SIX_FERRY",
            "SEAGALLOP:FOUR_SEVEN_FERRY",
            "SEAGALLOP:FIVE_VERMILION_FERRY",
            "SEAGALLOP:FIVE_ONE_FERRY",
            "SEAGALLOP:FIVE_TWO_FERRY",
            "SEAGALLOP:FIVE_THREE_FERRY",
            "SEAGALLOP:FIVE_FOUR_FERRY",
            "SEAGALLOP:FIVE_SIX_FERRY",
            "SEAGALLOP:FIVE_SEVEN_FERRY",
            "SEAGALLOP:SIX_VERMILION_FERRY",
            "SEAGALLOP:SIX_ONE_FERRY",
            "SEAGALLOP:SIX_TWO_FERRY",
            "SEAGALLOP:SIX_THREE_FERRY",
            "SEAGALLOP:SIX_FOUR_FERRY",
            "SEAGALLOP:SIX_FIVE_FERRY",
            "SEAGALLOP:SIX_SEVEN_FERRY",
            "SEAGALLOP:SEVEN_VERMILION_FERRY",
            "SEAGALLOP:SEVEN_ONE_FERRY",
            "SEAGALLOP:SEVEN_TWO_FERRY",
            "SEAGALLOP:SEVEN_THREE_FERRY",
            "SEAGALLOP:SEVEN_FOUR_FERRY",
            "SEAGALLOP:SEVEN_FIVE_FERRY",
            "SEAGALLOP:SEVEN_SIX_FERRY",
            "SEAGALLOP:VERMILION_NAVEL_FERRY",
            "SEAGALLOP:NAVEL_VERMILION_FERRY",
            "SEAGALLOP:VERMILION_BIRTH_FERRY",
            "SEAGALLOP:BIRTH_VERMILION_FERRY",
        ];
        let catalog = consent_route_catalog();
        assert_eq!(catalog.len(), 162);
        assert_eq!(
            catalog.last().expect("story route").id,
            BILL_ONE_CINNABAR_ROUTE_ID
        );
        assert_eq!(catalog[14].id, "HOENN:FLY_LITTLEROOT");
        assert_eq!(
            catalog
                .iter()
                .skip(15)
                .take(expected.len())
                .map(|route| route.id)
                .collect::<Vec<_>>(),
            expected
        );
        assert!(consent_route_definition("HOENN:FLY_EVER_GRANDE_CITY").is_err());
        assert!(consent_route_definition("SEAGALLOP:CINNABAR_ONE_FERRY").is_err());
        assert!(consent_route_definition("SEAGALLOP:ONE_CINNABAR_FERRY").is_err());
        let navel_return = consent_route_definition("SEAGALLOP:NAVEL_VERMILION_FERRY")
            .expect("event-island return route");
        assert_eq!(
            navel_return.source,
            WorldZone::new(RegionId::Sevii, "NAVEL_ROCK_HARBOR_FRLG", 1).unwrap()
        );
        assert_eq!(
            navel_return.destination,
            WorldZone::new(RegionId::Kanto, "VERMILION_CITY", 1).unwrap()
        );
        assert_eq!(
            consent_route_definition("HOENN:FLY_EVER_GRANDE_CITY_CENTER")
                .expect("Ever Grande center")
                .destination,
            WorldZone::new(RegionId::Hoenn, "EVER_GRANDE_CITY", 1).unwrap()
        );
        assert_eq!(
            consent_route_definition("HOENN:FLY_EVER_GRANDE_CITY_LEAGUE")
                .expect("Ever Grande League")
                .destination,
            WorldZone::new(RegionId::Hoenn, "EVER_GRANDE_CITY", 1).unwrap()
        );
        assert_eq!(
            consent_route_definition("HOENN:FLY_BATTLE_FRONTIER")
                .expect("Battle Frontier")
                .destination,
            WorldZone::new(RegionId::Hoenn, "BATTLE_FRONTIER_OUTSIDE_EAST", 1).unwrap()
        );
    }

    #[test]
    fn consent_catalog_creates_proposals_with_exact_route_contracts() {
        let cases = [
            (
                "JOHTO:GOLDENROD_KANTO_ORIGINAL_TRAIN",
                GroupTravelDeparture::Train,
                RegionId::Johto,
                "GOLDENROD_CITY_TRAIN_STATION",
                RegionId::Kanto,
                "KANTO_ORIGINAL_SAFFRON_CITY_TRAIN_STATION",
            ),
            (
                "JOHTO:GOLDENROD_KANTO_LATER_TRAIN",
                GroupTravelDeparture::Train,
                RegionId::Johto,
                "GOLDENROD_CITY_TRAIN_STATION",
                RegionId::Kanto,
                "KANTO_LATER_SAFFRON_CITY_TRAIN_STATION",
            ),
            (
                "JOHTO:OLIVINE_KANTO_ORIGINAL_FERRY",
                GroupTravelDeparture::Ferry,
                RegionId::Johto,
                "OLIVINE_CITY_PORT_INSIDE",
                RegionId::Kanto,
                "KANTO_ORIGINAL_VERMILION_CITY_PORT_INSIDE",
            ),
            (
                "JOHTO:OLIVINE_KANTO_LATER_FERRY",
                GroupTravelDeparture::Ferry,
                RegionId::Johto,
                "OLIVINE_CITY_PORT_INSIDE",
                RegionId::Kanto,
                "KANTO_LATER_VERMILION_CITY_PORT_INSIDE",
            ),
            (
                "KANTO:RECEPTION_GATE_KANTO_ORIGINAL_ROUTE22",
                GroupTravelDeparture::Gate,
                RegionId::Kanto,
                "RECEPTION_GATE",
                RegionId::Kanto,
                "ROUTE22",
            ),
            (
                "KANTO:RECEPTION_GATE_KANTO_LATER_ROUTE22",
                GroupTravelDeparture::Gate,
                RegionId::Kanto,
                "RECEPTION_GATE",
                RegionId::Kanto,
                "KANTO_LATER_ROUTE22",
            ),
            (
                "KANTO:ORIGINAL_VERMILION_JOHTO_FERRY",
                GroupTravelDeparture::Ferry,
                RegionId::Kanto,
                "KANTO_ORIGINAL_VERMILION_CITY_PORT_INSIDE",
                RegionId::Johto,
                "OLIVINE_CITY_PORT_INSIDE",
            ),
            (
                "KANTO:LATER_VERMILION_JOHTO_FERRY",
                GroupTravelDeparture::Ferry,
                RegionId::Kanto,
                "KANTO_LATER_VERMILION_CITY_PORT_INSIDE",
                RegionId::Johto,
                "OLIVINE_CITY_PORT_INSIDE",
            ),
            (
                "KANTO:ORIGINAL_SAFFRON_JOHTO_TRAIN",
                GroupTravelDeparture::Train,
                RegionId::Kanto,
                "KANTO_ORIGINAL_SAFFRON_CITY_TRAIN_STATION",
                RegionId::Johto,
                "GOLDENROD_CITY_TRAIN_STATION",
            ),
            (
                "KANTO:LATER_SAFFRON_JOHTO_TRAIN",
                GroupTravelDeparture::Train,
                RegionId::Kanto,
                "KANTO_LATER_SAFFRON_CITY_TRAIN_STATION",
                RegionId::Johto,
                "GOLDENROD_CITY_TRAIN_STATION",
            ),
            (
                "KANTO:ORIGINAL_ROUTE22_JOHTO_ROUTE22",
                GroupTravelDeparture::Gate,
                RegionId::Kanto,
                "ROUTE22",
                RegionId::Kanto,
                "RECEPTION_GATE",
            ),
            (
                "KANTO:LATER_ROUTE22_JOHTO_ROUTE22",
                GroupTravelDeparture::Gate,
                RegionId::Kanto,
                "KANTO_LATER_ROUTE22",
                RegionId::Kanto,
                "RECEPTION_GATE",
            ),
            (
                "HOENN:SOUTHERN_ISLAND_LILYCOVE_FERRY",
                GroupTravelDeparture::Ferry,
                RegionId::Hoenn,
                "SOUTHERN_ISLAND_EXTERIOR",
                RegionId::Hoenn,
                "LILYCOVE_CITY_HARBOR",
            ),
            (
                "HOENN:BIRTH_ISLAND_LILYCOVE_FERRY",
                GroupTravelDeparture::Ferry,
                RegionId::Hoenn,
                "BIRTH_ISLAND_HARBOR",
                RegionId::Hoenn,
                "LILYCOVE_CITY_HARBOR",
            ),
            (
                "HOENN:FARAWAY_ISLAND_LILYCOVE_FERRY",
                GroupTravelDeparture::Ferry,
                RegionId::Hoenn,
                "FARAWAY_ISLAND_ENTRANCE",
                RegionId::Hoenn,
                "LILYCOVE_CITY_HARBOR",
            ),
            (
                "HOENN:BATTLE_FRONTIER_SLATEPORT_FERRY",
                GroupTravelDeparture::Ferry,
                RegionId::Hoenn,
                "BATTLE_FRONTIER_OUTSIDE_WEST",
                RegionId::Hoenn,
                "SLATEPORT_CITY_HARBOR",
            ),
            (
                "HOENN:BATTLE_FRONTIER_LILYCOVE_FERRY",
                GroupTravelDeparture::Ferry,
                RegionId::Hoenn,
                "BATTLE_FRONTIER_OUTSIDE_WEST",
                RegionId::Hoenn,
                "LILYCOVE_CITY_HARBOR",
            ),
            (
                "HOENN:LILYCOVE_SOUTHERN_ISLAND_FERRY",
                GroupTravelDeparture::Ferry,
                RegionId::Hoenn,
                "LILYCOVE_CITY_HARBOR",
                RegionId::Hoenn,
                "SOUTHERN_ISLAND_EXTERIOR",
            ),
            (
                "HOENN:LILYCOVE_NAVEL_ROCK_FERRY",
                GroupTravelDeparture::Ferry,
                RegionId::Hoenn,
                "LILYCOVE_CITY_HARBOR",
                RegionId::Hoenn,
                "NAVEL_ROCK_HARBOR",
            ),
            (
                "HOENN:LILYCOVE_BIRTH_ISLAND_FERRY",
                GroupTravelDeparture::Ferry,
                RegionId::Hoenn,
                "LILYCOVE_CITY_HARBOR",
                RegionId::Hoenn,
                "BIRTH_ISLAND_HARBOR",
            ),
            (
                "HOENN:LILYCOVE_FARAWAY_ISLAND_FERRY",
                GroupTravelDeparture::Ferry,
                RegionId::Hoenn,
                "LILYCOVE_CITY_HARBOR",
                RegionId::Hoenn,
                "FARAWAY_ISLAND_ENTRANCE",
            ),
            (
                "HOENN:LILYCOVE_BATTLE_FRONTIER_FERRY",
                GroupTravelDeparture::Ferry,
                RegionId::Hoenn,
                "LILYCOVE_CITY_HARBOR",
                RegionId::Hoenn,
                "BATTLE_FRONTIER_OUTSIDE_WEST",
            ),
            (
                "HOENN:SLATEPORT_BATTLE_FRONTIER_FERRY",
                GroupTravelDeparture::Ferry,
                RegionId::Hoenn,
                "SLATEPORT_CITY_HARBOR",
                RegionId::Hoenn,
                "BATTLE_FRONTIER_OUTSIDE_WEST",
            ),
            (
                "HOENN:NAVEL_ROCK_LILYCOVE_FERRY",
                GroupTravelDeparture::Ferry,
                RegionId::Hoenn,
                "NAVEL_ROCK_HARBOR",
                RegionId::Hoenn,
                "LILYCOVE_CITY_HARBOR",
            ),
            (
                "HOENN:SLATEPORT_SS_TIDAL_BOARD_FERRY",
                GroupTravelDeparture::Ferry,
                RegionId::Hoenn,
                "SLATEPORT_CITY_HARBOR",
                RegionId::Hoenn,
                "SS_TIDAL_CORRIDOR",
            ),
            (
                "HOENN:LILYCOVE_SS_TIDAL_BOARD_FERRY",
                GroupTravelDeparture::Ferry,
                RegionId::Hoenn,
                "LILYCOVE_CITY_HARBOR",
                RegionId::Hoenn,
                "SS_TIDAL_CORRIDOR",
            ),
            (
                "HOENN:SS_TIDAL_LILYCOVE_EXIT_FERRY",
                GroupTravelDeparture::Ferry,
                RegionId::Hoenn,
                "SS_TIDAL_CORRIDOR",
                RegionId::Hoenn,
                "LILYCOVE_CITY_HARBOR",
            ),
            (
                "HOENN:SS_TIDAL_SLATEPORT_EXIT_FERRY",
                GroupTravelDeparture::Ferry,
                RegionId::Hoenn,
                "SS_TIDAL_CORRIDOR",
                RegionId::Hoenn,
                "SLATEPORT_CITY_HARBOR",
            ),
        ];

        for (route_id, departure, source_region, source_map, destination_region, destination_map) in
            cases
        {
            let app = super::super::Phase2App::test();
            let (first, first_lease, second, _second_lease, group_id) = two_member_group(&app);
            let source = WorldZone::new(source_region, source_map, 1).expect("source");
            let destination =
                WorldZone::new(destination_region, destination_map, 1).expect("destination");
            set_consent_progress(&app, group_id, [first, second], source.clone());
            let request = GroupTravelProposalRequest::new_with_departure(
                first_lease.fence(),
                route_id,
                departure,
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            )
            .expect("request");

            let proposal = create_travel_proposal(&app.store, first, group_id, &request)
                .expect("catalog route creates a proposal");
            assert_eq!(proposal.route_id.as_str(), route_id);
            assert_eq!(proposal.departure, departure);
            assert_eq!(proposal.source, source);
            assert_eq!(proposal.destination, destination);
            assert_eq!(proposal.status, GroupTravelProposalStatus::Pending);
            assert_eq!(proposal.requester_character_id, first.character_id);
            assert_eq!(proposal.responder_character_id, second.character_id);
        }
    }

    #[test]
    fn fly_littleroot_allows_cross_map_group_before_fly_point_checkpoint() {
        let app = super::super::Phase2App::test();
        let (first, first_lease, second, second_lease, group_id) = two_member_group(&app);
        let first_zone = WorldZone::new(RegionId::Hoenn, "ROUTE104", 1).expect("first zone");
        let second_zone = WorldZone::new(RegionId::Hoenn, "ROUTE105", 1).expect("second zone");
        let destination =
            WorldZone::new(RegionId::Hoenn, "LITTLEROOT_TOWN", 1).expect("destination");
        app.store
            .write_transaction(|state| {
                for (actor, zone) in [(first, first_zone.clone()), (second, second_zone.clone())] {
                    let progress = vec![
                        RegionalProgress::new(RegionId::Hoenn, 0, 0, vec![], vec![])
                            .map_err(|_| Phase2Error::Internal)?,
                    ];
                    let character = state
                        .characters
                        .get_mut(&actor.character_id)
                        .ok_or(Phase2Error::Internal)?;
                    character.state = CharacterCloudState::new(actor.character_id, zone, progress)
                        .map_err(|_| Phase2Error::Internal)?;
                }
                state.groups.get_mut(&group_id).expect("group").zone = first_zone.clone();
                let members = state.groups[&group_id].group.members();
                let member_zones =
                    members.map(|member| state.characters[&member].state.world_zone.clone());
                state
                    .group_member_world_zones
                    .insert(group_id, member_zones);
                Ok::<(), Phase2Error>(())
            })
            .expect("cross-map fly state");
        let request = GroupTravelProposalRequest::new_with_departure(
            first_lease.fence(),
            "HOENN:FLY_LITTLEROOT",
            GroupTravelDeparture::Fly,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        )
        .expect("fly request");
        let proposal =
            create_travel_proposal(&app.store, first, group_id, &request).expect("fly proposal");
        assert_eq!(proposal.source, first_zone);
        assert_eq!(proposal.destination, destination);
        let accept = GroupTravelActionRequest::new(
            second_lease.fence(),
            GroupTravelAction::Accept,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        );
        let committed =
            act_on_travel_proposal(&app.store, second, group_id, proposal.proposal_id, &accept)
                .expect("fly accept");
        assert_eq!(committed.status, GroupTravelProposalStatus::Committed);
        app.store
            .read_transaction(|state| {
                assert_eq!(
                    state.characters[&first.character_id].state.world_zone,
                    destination
                );
                assert_eq!(
                    state.characters[&second.character_id].state.world_zone,
                    destination
                );
                assert_eq!(
                    state.group_member_world_zones[&group_id],
                    [destination.clone(), destination.clone()]
                );
                Ok::<(), Phase2Error>(())
            })
            .expect("committed fly zones");
    }

    #[test]
    fn proposal_rejects_route_departure_context_mismatch() {
        let app = super::super::Phase2App::test();
        let (first, first_lease, second, _second_lease, group_id) = two_member_group(&app);
        let source =
            WorldZone::new(RegionId::Johto, "GOLDENROD_CITY_TRAIN_STATION", 1).expect("source");
        set_consent_progress(&app, group_id, [first, second], source);
        let request = GroupTravelProposalRequest::new_with_departure(
            first_lease.fence(),
            "JOHTO:GOLDENROD_KANTO_ORIGINAL_TRAIN",
            GroupTravelDeparture::Gate,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        )
        .expect("request shape");
        assert_eq!(
            create_travel_proposal(&app.store, first, group_id, &request),
            Err(Phase2Error::Forbidden)
        );
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "the lifecycle assertion keeps one proposal identity from create through cleanup"
    )]
    fn consent_accept_is_atomic_idempotent_and_waits_for_both_applied() {
        let app = super::super::Phase2App::test();
        let (first, first_lease, second, second_lease, group_id) = two_member_group(&app);
        let source =
            WorldZone::new(RegionId::Johto, "GOLDENROD_CITY_TRAIN_STATION", 1).expect("source");
        set_consent_progress(&app, group_id, [first, second], source);

        let create = GroupTravelProposalRequest::new(
            first_lease.fence(),
            "JOHTO:GOLDENROD_KANTO_ORIGINAL_TRAIN",
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        )
        .expect("request");
        let proposal =
            create_travel_proposal(&app.store, first, group_id, &create).expect("proposal");
        assert_eq!(proposal.status, GroupTravelProposalStatus::Pending);
        assert_eq!(
            create_travel_proposal(&app.store, first, group_id, &create).expect("create replay"),
            proposal
        );
        let conflicting = GroupTravelProposalRequest::new(
            first_lease.fence(),
            "JOHTO:GOLDENROD_KANTO_LATER_TRAIN",
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        )
        .expect("request");
        assert_eq!(
            create_travel_proposal(&app.store, first, group_id, &conflicting),
            Err(Phase2Error::Conflict)
        );

        let requester_accept = GroupTravelActionRequest::new(
            first_lease.fence(),
            GroupTravelAction::Accept,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        );
        assert_eq!(
            act_on_travel_proposal(
                &app.store,
                first,
                group_id,
                proposal.proposal_id,
                &requester_accept,
            ),
            Err(Phase2Error::Forbidden)
        );

        let accept = GroupTravelActionRequest::new(
            second_lease.fence(),
            GroupTravelAction::Accept,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        );
        let committed =
            act_on_travel_proposal(&app.store, second, group_id, proposal.proposal_id, &accept)
                .expect("accept");
        assert_eq!(committed.status, GroupTravelProposalStatus::Committed);
        let commit = committed.commit.as_ref().expect("commit payload");
        assert_eq!(commit.group_zone_revision, 1);
        assert_eq!(commit.members[0].world_revision, 1);
        assert_eq!(commit.members[1].world_revision, 1);
        assert_eq!(
            act_on_travel_proposal(&app.store, second, group_id, proposal.proposal_id, &accept,)
                .expect("accept replay"),
            committed
        );

        app.store
            .write_transaction(|state| {
                let missing_create =
                    MAX_TRAVEL_CREATE_RECEIPTS.saturating_sub(create_receipt_count(state));
                for index in 0..missing_create {
                    let key = IdempotencyKey::new(Uuid::from_u128(
                        10_000_u128.saturating_add(index as u128),
                    ))
                    .map_err(|_| Phase2Error::Internal)?;
                    state.group_travel_proposal_idempotency.insert(
                        (first.character_id, format!("capacity-{index:04}"), key),
                        GroupTravelProposalIdempotencyRecord {
                            fingerprint: [u8::try_from(index % 256).expect("bounded"); 32],
                            response: committed.clone(),
                            expires_at: 1_700_000_000_000_u64
                                .saturating_add(TRAVEL_PROPOSAL_REPLAY_TTL_MS),
                            lifecycle: false,
                        },
                    );
                }
                let protected = reserved_lifecycle_receipts(state);
                let missing_lifecycle = MAX_TRAVEL_LIFECYCLE_RECEIPTS
                    .saturating_sub(protected)
                    .saturating_sub(lifecycle_receipt_count(state));
                for index in 0..missing_lifecycle {
                    let key = IdempotencyKey::new(Uuid::from_u128(
                        20_000_u128.saturating_add(index as u128),
                    ))
                    .map_err(|_| Phase2Error::Internal)?;
                    state.group_travel_proposal_idempotency.insert(
                        (
                            first.character_id,
                            format!("lifecycle-capacity-{index:04}"),
                            key,
                        ),
                        GroupTravelProposalIdempotencyRecord {
                            fingerprint: [u8::try_from(index % 256).expect("bounded"); 32],
                            response: committed.clone(),
                            expires_at: 1_700_000_000_000_u64
                                .saturating_add(TRAVEL_PROPOSAL_REPLAY_TTL_MS),
                            lifecycle: true,
                        },
                    );
                }
                assert_eq!(
                    state
                        .group_travel_proposal_idempotency
                        .len()
                        .saturating_add(protected),
                    MAX_TRAVEL_PROPOSALS,
                );
                Ok::<(), Phase2Error>(())
            })
            .expect("fill replay capacity");
        assert_eq!(
            create_travel_proposal(&app.store, first, group_id, &create)
                .expect("create replay at capacity"),
            proposal
        );
        assert_eq!(
            act_on_travel_proposal(&app.store, second, group_id, proposal.proposal_id, &accept)
                .expect("accept replay at capacity"),
            committed
        );

        let first_applied = GroupTravelActionRequest::new(
            first_lease.fence(),
            GroupTravelAction::Applied,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        );
        let delivered = act_on_travel_proposal(
            &app.store,
            first,
            group_id,
            proposal.proposal_id,
            &first_applied,
        )
        .expect("first applied");
        let conflicting_applied = GroupTravelActionRequest::new(
            first_lease.fence(),
            GroupTravelAction::Cancel,
            first_applied.idempotency_key,
        );
        assert_eq!(
            act_on_travel_proposal(
                &app.store,
                first,
                group_id,
                proposal.proposal_id,
                &conflicting_applied,
            ),
            Err(Phase2Error::Conflict)
        );
        assert_eq!(
            act_on_travel_proposal(
                &app.store,
                first,
                group_id,
                proposal.proposal_id,
                &first_applied,
            )
            .expect("applied exact replay"),
            delivered
        );
        let before_fresh_replay = app
            .store
            .inspect_state(|state| {
                (
                    lifecycle_receipt_count(state),
                    reserved_lifecycle_receipts(state),
                )
            })
            .expect("receipt accounting");
        let fresh_applied = GroupTravelActionRequest::new(
            first_lease.fence(),
            GroupTravelAction::Applied,
            IdempotencyKey::new(Uuid::new_v4()).expect("fresh key"),
        );
        assert_eq!(
            act_on_travel_proposal(
                &app.store,
                first,
                group_id,
                proposal.proposal_id,
                &fresh_applied,
            ),
            Err(Phase2Error::Conflict)
        );
        let after_fresh_replay = app
            .store
            .inspect_state(|state| {
                (
                    lifecycle_receipt_count(state),
                    reserved_lifecycle_receipts(state),
                )
            })
            .expect("receipt accounting");
        assert_eq!(after_fresh_replay, before_fresh_replay);
        assert!(delivered.applied_by.iter().any(|applied| *applied));
        assert!(
            get_travel_proposal(
                &app.store,
                second,
                group_id,
                proposal.proposal_id,
                second_lease.fence(),
            )
            .is_ok()
        );

        let second_applied = GroupTravelActionRequest::new(
            second_lease.fence(),
            GroupTravelAction::Applied,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        );
        let fully_delivered = act_on_travel_proposal(
            &app.store,
            second,
            group_id,
            proposal.proposal_id,
            &second_applied,
        )
        .expect("second applied");
        assert_eq!(fully_delivered.applied_by, [true, true]);
        assert_eq!(
            get_travel_proposal(
                &app.store,
                second,
                group_id,
                proposal.proposal_id,
                second_lease.fence(),
            ),
            Err(Phase2Error::NotFound)
        );
        assert_eq!(
            act_on_travel_proposal(
                &app.store,
                second,
                group_id,
                proposal.proposal_id,
                &second_applied,
            )
            .expect("applied replay"),
            fully_delivered
        );
    }

    #[test]
    fn decline_and_explicit_release_cancel_without_world_mutation() {
        let app = super::super::Phase2App::test();
        let (first, first_lease, second, second_lease, group_id) = two_member_group(&app);
        let source =
            WorldZone::new(RegionId::Johto, "OLIVINE_CITY_PORT_INSIDE", 1).expect("source");
        set_consent_progress(&app, group_id, [first, second], source.clone());
        let request = GroupTravelProposalRequest::new(
            first_lease.fence(),
            "JOHTO:OLIVINE_KANTO_LATER_FERRY",
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        )
        .expect("request");
        let proposal =
            create_travel_proposal(&app.store, first, group_id, &request).expect("proposal");
        let decline = GroupTravelActionRequest::new(
            second_lease.fence(),
            GroupTravelAction::Decline,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        );
        let declined =
            act_on_travel_proposal(&app.store, second, group_id, proposal.proposal_id, &decline)
                .expect("decline");
        assert_eq!(declined.status, GroupTravelProposalStatus::Declined);
        let state = app
            .store
            .inspect_state(|state| {
                let group = state.groups.get(&group_id).expect("group");
                (
                    group.zone.clone(),
                    group.zone_revision,
                    state.characters[&first.character_id].world_revision,
                    state.characters[&second.character_id].world_revision,
                )
            })
            .expect("state");
        assert_eq!(state, (source, 0, 0, 0));

        let second_request = GroupTravelProposalRequest::new(
            first_lease.fence(),
            "JOHTO:OLIVINE_KANTO_LATER_FERRY",
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        )
        .expect("request");
        let second_proposal = create_travel_proposal(&app.store, first, group_id, &second_request)
            .expect("second proposal");
        app.release(
            first,
            coop_cloud::ReleaseLeaseRequest::new(
                first_lease.fence(),
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            ),
        )
        .expect("release");
        let cancelled = app
            .store
            .inspect_state(|state| {
                state.group_travel_proposals[&second_proposal.proposal_id]
                    .view
                    .status
            })
            .expect("state");
        assert_eq!(cancelled, GroupTravelProposalStatus::Cancelled);
    }

    #[test]
    fn direct_travel_is_denied_even_with_live_proposal() {
        let app = super::super::Phase2App::test();
        let (first, first_lease, second, second_lease, group_id) = two_member_group(&app);
        let source =
            WorldZone::new(RegionId::Johto, "GOLDENROD_CITY_TRAIN_STATION", 1).expect("source");
        set_consent_progress(&app, group_id, [first, second], source);
        let proposal_request = GroupTravelProposalRequest::new_with_departure(
            first_lease.fence(),
            "JOHTO:GOLDENROD_KANTO_ORIGINAL_TRAIN",
            GroupTravelDeparture::Train,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        )
        .expect("request");
        let proposal = create_travel_proposal(&app.store, first, group_id, &proposal_request)
            .expect("proposal");
        assert_eq!(proposal.status, GroupTravelProposalStatus::Pending);

        let legacy = GroupTravelRequest::new(
            first_lease.fence(),
            "HOENN:SLATEPORT_SEVII_FERRY",
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        )
        .expect("travel request");
        assert_eq!(
            travel(&app.store, first, group_id, &legacy),
            Err(Phase2Error::Forbidden)
        );

        let decline = GroupTravelActionRequest::new(
            second_lease.fence(),
            GroupTravelAction::Decline,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        );
        let declined =
            act_on_travel_proposal(&app.store, second, group_id, proposal.proposal_id, &decline)
                .expect("decline");
        assert_eq!(declined.status, GroupTravelProposalStatus::Declined);
        assert_eq!(
            travel(&app.store, first, group_id, &legacy),
            Err(Phase2Error::Forbidden)
        );
    }

    #[test]
    fn terminal_state_and_receipts_replay_then_prune_before_new_admission() {
        let app = super::super::Phase2App::test();
        let (first, first_lease, second, second_lease, group_id) = two_member_group(&app);
        let source =
            WorldZone::new(RegionId::Johto, "OLIVINE_CITY_PORT_INSIDE", 1).expect("source");
        set_consent_progress(&app, group_id, [first, second], source);
        let proposal = create_travel_proposal(
            &app.store,
            first,
            group_id,
            &GroupTravelProposalRequest::new(
                first_lease.fence(),
                "JOHTO:OLIVINE_KANTO_ORIGINAL_FERRY",
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            )
            .expect("request"),
        )
        .expect("proposal");
        let decline = GroupTravelActionRequest::new(
            second_lease.fence(),
            GroupTravelAction::Decline,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        );
        let declined =
            act_on_travel_proposal(&app.store, second, group_id, proposal.proposal_id, &decline)
                .expect("decline");
        assert_eq!(
            act_on_travel_proposal(&app.store, second, group_id, proposal.proposal_id, &decline,)
                .expect("exact replay within retention"),
            declined
        );

        let now = app.store.now();
        app.store
            .write_transaction(|state| {
                state
                    .group_travel_proposals
                    .get_mut(&proposal.proposal_id)
                    .expect("terminal proposal")
                    .retain_until = Some(now.saturating_sub(1));
                for receipt in state.group_travel_proposal_idempotency.values_mut() {
                    receipt.expires_at = now.saturating_sub(1);
                }
                Ok::<(), Phase2Error>(())
            })
            .expect("age terminal state");
        let replacement = create_travel_proposal(
            &app.store,
            first,
            group_id,
            &GroupTravelProposalRequest::new(
                first_lease.fence(),
                "JOHTO:OLIVINE_KANTO_LATER_FERRY",
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            )
            .expect("request"),
        )
        .expect("new proposal after pruning");
        assert_eq!(replacement.status, GroupTravelProposalStatus::Pending);
        let retained = app
            .store
            .inspect_state(|state| {
                (
                    state
                        .group_travel_proposals
                        .contains_key(&proposal.proposal_id),
                    state.group_travel_proposal_idempotency.len(),
                )
            })
            .expect("state");
        assert_eq!(retained, (false, 1));
    }

    #[test]
    fn expiry_and_accept_revalidation_never_partially_mutate_world_state() {
        let (app, clock) = app_with_clock();
        let (first, first_lease, second, second_lease, group_id) = two_member_group(&app);
        let source =
            WorldZone::new(RegionId::Johto, "GOLDENROD_CITY_TRAIN_STATION", 1).expect("source");
        set_consent_progress(&app, group_id, [first, second], source.clone());
        let proposal = create_travel_proposal(
            &app.store,
            first,
            group_id,
            &GroupTravelProposalRequest::new(
                first_lease.fence(),
                "JOHTO:GOLDENROD_KANTO_LATER_TRAIN",
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            )
            .expect("request"),
        )
        .expect("proposal");

        app.store
            .write_transaction(|state| {
                state
                    .characters
                    .get_mut(&first.character_id)
                    .ok_or(Phase2Error::Internal)?
                    .world_revision = 1;
                Ok::<(), Phase2Error>(())
            })
            .expect("stale expected revision");
        let accept = GroupTravelActionRequest::new(
            second_lease.fence(),
            GroupTravelAction::Accept,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        );
        assert_eq!(
            act_on_travel_proposal(&app.store, second, group_id, proposal.proposal_id, &accept,),
            Err(Phase2Error::Conflict)
        );
        let unchanged = app
            .store
            .inspect_state(|state| {
                (
                    state.groups[&group_id].zone.clone(),
                    state.groups[&group_id].zone_revision,
                    state.characters[&first.character_id].world_revision,
                    state.characters[&second.character_id].world_revision,
                )
            })
            .expect("state");
        assert_eq!(unchanged, (source, 0, 1, 0));

        clock.advance(20_000);
        let first_heartbeat = app
            .heartbeat(
                first,
                coop_cloud::HeartbeatLeaseRequest::new(first_lease.fence()),
            )
            .expect("first heartbeat");
        let _second_heartbeat = app
            .heartbeat(
                second,
                coop_cloud::HeartbeatLeaseRequest::new(second_lease.fence()),
            )
            .expect("second heartbeat");
        clock.advance(10_001);
        assert_eq!(
            current_travel_proposal(&app.store, first, group_id, first_heartbeat.fence()),
            Err(Phase2Error::NotFound)
        );
        let expired = get_travel_proposal(
            &app.store,
            first,
            group_id,
            proposal.proposal_id,
            first_heartbeat.fence(),
        )
        .expect("expired proposal remains inspectable");
        assert_eq!(expired.status, GroupTravelProposalStatus::Expired);
    }

    #[test]
    fn online_leave_cancels_pending_proposal() {
        let app = super::super::Phase2App::test();
        let (first, first_lease, second, _second_lease, group_id) = two_member_group(&app);
        let source = WorldZone::new(RegionId::Kanto, "RECEPTION_GATE", 1).expect("source");
        set_consent_progress(&app, group_id, [first, second], source);
        let proposal = create_travel_proposal(
            &app.store,
            first,
            group_id,
            &GroupTravelProposalRequest::new(
                first_lease.fence(),
                "KANTO:RECEPTION_GATE_KANTO_ORIGINAL_ROUTE22",
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            )
            .expect("request"),
        )
        .expect("proposal");
        let response = super::super::online::action(
            &app,
            first,
            &coop_cloud::OnlineActionRequest {
                api_version: coop_cloud::ApiVersion::V1,
                fence: first_lease.fence(),
                idempotency_key: IdempotencyKey::new(Uuid::new_v4()).expect("key"),
                action: coop_cloud::OnlineAction::Leave { group_id },
            },
        )
        .expect("leave");
        assert_eq!(response, coop_cloud::OnlineActionResponse::Left);
        let status = app
            .store
            .inspect_state(|state| {
                state.group_travel_proposals[&proposal.proposal_id]
                    .view
                    .status
            })
            .expect("state");
        assert_eq!(status, GroupTravelProposalStatus::Cancelled);
    }

    #[test]
    fn old_group_record_defaults_zone_revision() {
        let first = CharacterId::new(Uuid::from_u128(1)).expect("id");
        let second = CharacterId::new(Uuid::from_u128(2)).expect("id");
        let record = GroupRecord {
            group: Group::new(first, second).expect("group"),
            zone: WorldZone::new(RegionId::Kanto, "RECEPTION_GATE", 1).expect("zone"),
            status: GroupStatus::Active,
            zone_revision: 9,
        };
        let mut value = serde_json::to_value(record).expect("serialize");
        value
            .as_object_mut()
            .expect("object")
            .remove("zone_revision");
        let decoded: GroupRecord = serde_json::from_value(value).expect("old record");
        assert_eq!(decoded.zone_revision, 0);
    }

    #[test]
    fn create_prunes_expired_state_before_admission() {
        let app = super::super::Phase2App::test();
        let (first_actor, first_lease) = account(&app, "first", "first-invite");
        let (second_actor, _second_lease) = account(&app, "second", "second-invite");
        let now = app.store.now();
        app.store
            .write_transaction(|state| {
                for index in 0..16_u128 {
                    let invitation_id = GroupInvitationId::new(Uuid::from_u128(u128::MAX - index))
                        .map_err(|_| Phase2Error::Internal)?;
                    state.group_invitations.insert(
                        invitation_id,
                        GroupInvitationRecord {
                            invitation_id,
                            inviter: first_actor.character_id,
                            invitee: second_actor.character_id,
                            expires_at: now - 1,
                            consumed: false,
                            allow_remote_maps: false,
                        },
                    );
                    let key = coop_cloud::IdempotencyKey::new(Uuid::from_u128(index + 1))
                        .map_err(|_| Phase2Error::Internal)?;
                    state.group_idempotency.insert(
                        (first_actor.character_id, "expired".to_owned(), key),
                        GroupIdempotencyRecord {
                            fingerprint: [0; 32],
                            response: GroupIdempotencyResponse::Invitation(GroupInvitationView {
                                api_version: coop_cloud::ApiVersion::V1,
                                invitation_id,
                                inviter_character_id: first_actor.character_id,
                                invitee_character_id: second_actor.character_id,
                                expires_at: coop_cloud::UnixTimestampMillis::new(now - 1),
                            }),
                            expires_at: now - 1,
                        },
                    );
                }
                Ok::<(), Phase2Error>(())
            })
            .expect("expired state");
        let request = CreateGroupInvitationRequest::new(
            first_lease.fence(),
            second_actor.character_id,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        );
        create_invitation(&app.store, first_actor, &request).expect("create after prune");
        let lengths = app
            .store
            .inspect_state(|state| (state.group_invitations.len(), state.group_idempotency.len()))
            .expect("state");
        assert_eq!(lengths, (1, 1));
    }

    #[test]
    fn concurrent_acceptance_has_one_committed_group() {
        let app = super::super::Phase2App::test();
        let (first_actor, first_lease) = account(&app, "first", "first-invite");
        let (second_actor, second_lease) = account(&app, "second", "second-invite");
        let invitation = create_invitation(
            &app.store,
            first_actor,
            &CreateGroupInvitationRequest::new(
                first_lease.fence(),
                second_actor.character_id,
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            ),
        )
        .expect("group invitation");
        let request_one = AcceptGroupInvitationRequest::new(
            second_lease.fence(),
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        );
        let request_two = AcceptGroupInvitationRequest::new(
            second_lease.fence(),
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        );
        let app_one = app.clone();
        let app_two = app.clone();
        let (result_one, result_two) = std::thread::scope(|scope| {
            let first = scope.spawn(move || {
                app_one.accept_group_invitation(second_actor, invitation.invitation_id, request_one)
            });
            let second = scope.spawn(move || {
                app_two.accept_group_invitation(second_actor, invitation.invitation_id, request_two)
            });
            (
                first.join().expect("first acceptance"),
                second.join().expect("second acceptance"),
            )
        });
        assert_eq!(
            i32::from(result_one.is_ok()) + i32::from(result_two.is_ok()),
            1
        );
        assert_eq!(
            app.store
                .inspect_state(|state| (state.groups.len(), state.active_group_by_member.len()))
                .expect("state"),
            (1, 2)
        );
    }

    #[test]
    fn caller_identity_is_revalidated_for_every_group_operation() {
        let app = super::super::Phase2App::test();
        let (first_actor, first_lease, _second_actor, _second_lease, group_id) =
            two_member_group(&app);
        let (third_actor, _third_lease) = account(&app, "third", "third-invite");
        app.store
            .write_transaction(|state| {
                state
                    .users_by_id
                    .get_mut(&first_actor.user_id)
                    .expect("caller")
                    .disabled = true;
                Ok::<(), Phase2Error>(())
            })
            .expect("disable caller");

        let create = CreateGroupInvitationRequest::new(
            first_lease.fence(),
            third_actor.character_id,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        );
        assert_eq!(
            create_invitation(&app.store, first_actor, &create),
            Err(Phase2Error::Authentication)
        );
        assert_eq!(
            inspect_group(&app.store, first_actor, group_id, first_lease.fence()),
            Err(Phase2Error::Authentication)
        );
        let travel_request = GroupTravelRequest::new(
            first_lease.fence(),
            "HOENN:SLATEPORT_SEVII_FERRY",
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        )
        .expect("travel request");
        assert_eq!(
            travel(&app.store, first_actor, group_id, &travel_request),
            Err(Phase2Error::Forbidden)
        );

        app.store
            .write_transaction(|state| {
                state
                    .users_by_id
                    .get_mut(&first_actor.user_id)
                    .expect("caller")
                    .disabled = false;
                state
                    .characters
                    .get_mut(&first_actor.character_id)
                    .expect("caller character")
                    .state
                    .character_id = third_actor.character_id;
                Ok::<(), Phase2Error>(())
            })
            .expect("inconsistent caller");
        assert_eq!(
            inspect_group(&app.store, first_actor, group_id, first_lease.fence()),
            Err(Phase2Error::Authentication)
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn companion_identity_and_lease_failures_are_policy_denials() {
        let app = super::super::Phase2App::test();
        let (first_actor, first_lease) = account(&app, "first", "first-invite");
        let (second_actor, _second_lease) = account(&app, "second", "second-invite");
        let (third_actor, _third_lease) = account(&app, "third", "third-invite");

        app.store
            .write_transaction(|state| {
                state
                    .users_by_id
                    .get_mut(&second_actor.user_id)
                    .expect("target")
                    .disabled = true;
                Ok::<(), Phase2Error>(())
            })
            .expect("disable target");
        let create = CreateGroupInvitationRequest::new(
            first_lease.fence(),
            second_actor.character_id,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        );
        assert_eq!(
            create_invitation(&app.store, first_actor, &create),
            Err(Phase2Error::Forbidden)
        );

        app.store
            .write_transaction(|state| {
                state
                    .users_by_id
                    .get_mut(&second_actor.user_id)
                    .expect("target")
                    .disabled = false;
                state
                    .characters
                    .get_mut(&second_actor.character_id)
                    .expect("target character")
                    .state
                    .character_id = third_actor.character_id;
                Ok::<(), Phase2Error>(())
            })
            .expect("inconsistent target");
        assert_eq!(
            create_invitation(&app.store, first_actor, &create),
            Err(Phase2Error::Forbidden)
        );

        app.store
            .write_transaction(|state| {
                state
                    .characters
                    .get_mut(&second_actor.character_id)
                    .expect("target character")
                    .state
                    .character_id = second_actor.character_id;
                state
                    .leases
                    .get_mut(&second_actor.character_id)
                    .expect("target lease")
                    .released = true;
                Ok::<(), Phase2Error>(())
            })
            .expect("inactive target lease");
        assert_eq!(
            create_invitation(&app.store, first_actor, &create),
            Err(Phase2Error::Forbidden)
        );

        // Rebuild the target lease through a fresh app so acceptance reaches
        // the companion checks without depending on lease internals.
        let app = super::super::Phase2App::test();
        let (first_actor, first_lease) = account(&app, "first", "first-invite");
        let (second_actor, second_lease) = account(&app, "second", "second-invite");
        let invitation = create_invitation(
            &app.store,
            first_actor,
            &CreateGroupInvitationRequest::new(
                first_lease.fence(),
                second_actor.character_id,
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            ),
        )
        .expect("invitation");
        app.store
            .write_transaction(|state| {
                state
                    .users_by_id
                    .get_mut(&first_actor.user_id)
                    .expect("companion")
                    .disabled = true;
                Ok::<(), Phase2Error>(())
            })
            .expect("disable companion");
        assert_eq!(
            accept_invitation(
                &app.store,
                second_actor,
                invitation.invitation_id,
                &AcceptGroupInvitationRequest::new(
                    second_lease.fence(),
                    IdempotencyKey::new(Uuid::new_v4()).expect("key"),
                ),
            ),
            Err(Phase2Error::Forbidden)
        );
        app.store
            .write_transaction(|state| {
                state
                    .users_by_id
                    .get_mut(&first_actor.user_id)
                    .expect("companion")
                    .disabled = false;
                state
                    .leases
                    .get_mut(&first_actor.character_id)
                    .expect("companion lease")
                    .released = true;
                Ok::<(), Phase2Error>(())
            })
            .expect("release companion");
        assert_eq!(
            accept_invitation(
                &app.store,
                second_actor,
                invitation.invitation_id,
                &AcceptGroupInvitationRequest::new(
                    second_lease.fence(),
                    IdempotencyKey::new(Uuid::new_v4()).expect("key"),
                ),
            ),
            Err(Phase2Error::Forbidden)
        );
    }

    #[test]
    fn foreign_invitation_acceptance_is_hidden_across_terminal_states() {
        let app = super::super::Phase2App::test();
        let (sender, sender_lease) = account(&app, "inviter", "inviter-invite");
        let (recipient, _recipient_lease) = account(&app, "invitee", "invitee-invite");
        let (foreign, foreign_lease) = account(&app, "foreign", "foreign-invite");
        let invitation = create_invitation(
            &app.store,
            sender,
            &CreateGroupInvitationRequest::new(
                sender_lease.fence(),
                recipient.character_id,
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            ),
        )
        .expect("invitation");
        let accept = || {
            accept_invitation(
                &app.store,
                foreign,
                invitation.invitation_id,
                &AcceptGroupInvitationRequest::new(
                    foreign_lease.fence(),
                    IdempotencyKey::new(Uuid::new_v4()).expect("key"),
                ),
            )
        };
        assert_eq!(accept(), Err(Phase2Error::NotFound));
        let now = app.store.now();
        app.store
            .write_transaction(|state| {
                state
                    .group_invitations
                    .get_mut(&invitation.invitation_id)
                    .expect("invitation")
                    .expires_at = now - 1;
                Ok::<(), Phase2Error>(())
            })
            .expect("expire invitation");
        assert_eq!(accept(), Err(Phase2Error::NotFound));
        app.store
            .write_transaction(|state| {
                state
                    .group_invitations
                    .get_mut(&invitation.invitation_id)
                    .expect("invitation")
                    .consumed = true;
                Ok::<(), Phase2Error>(())
            })
            .expect("consume invitation");
        assert_eq!(accept(), Err(Phase2Error::NotFound));
        let missing = GroupInvitationId::new(Uuid::new_v4()).expect("missing");
        assert_eq!(
            accept_invitation(
                &app.store,
                foreign,
                missing,
                &AcceptGroupInvitationRequest::new(
                    foreign_lease.fence(),
                    IdempotencyKey::new(Uuid::new_v4()).expect("key"),
                ),
            ),
            Err(Phase2Error::NotFound)
        );
    }

    #[test]
    fn sender_can_cancel_only_their_invitation_without_affecting_other_invitees() {
        let app = super::super::Phase2App::test();
        let (sender, sender_lease) = account(&app, "sender", "sender-invite");
        let (first, first_lease) = account(&app, "first", "first-invite");
        let (second, second_lease) = account(&app, "second", "second-invite");
        let create = |invitee| {
            create_invitation(
                &app.store,
                sender,
                &CreateGroupInvitationRequest::new(
                    sender_lease.fence(),
                    invitee,
                    IdempotencyKey::new(Uuid::new_v4()).expect("key"),
                ),
            )
            .expect("invitation")
        };
        let cancelled = create(first.character_id);
        let unaffected = create(second.character_id);
        let key = IdempotencyKey::new(Uuid::new_v4()).expect("key");
        let cancel = coop_cloud::OnlineActionRequest {
            api_version: coop_cloud::ApiVersion::V1,
            fence: sender_lease.fence(),
            idempotency_key: key,
            action: coop_cloud::OnlineAction::Cancel {
                invitation_id: cancelled.invitation_id,
            },
        };
        assert_eq!(
            super::super::online::action(
                &app,
                first,
                &coop_cloud::OnlineActionRequest {
                    fence: first_lease.fence(),
                    ..cancel.clone()
                }
            ),
            Err(Phase2Error::NotFound)
        );
        assert_eq!(
            super::super::online::action(&app, sender, &cancel),
            Ok(coop_cloud::OnlineActionResponse::Cancelled)
        );
        assert_eq!(
            super::super::online::action(&app, sender, &cancel),
            Ok(coop_cloud::OnlineActionResponse::Cancelled)
        );
        assert_eq!(
            accept_invitation(
                &app.store,
                first,
                cancelled.invitation_id,
                &AcceptGroupInvitationRequest::new(
                    first_lease.fence(),
                    IdempotencyKey::new(Uuid::new_v4()).expect("key"),
                ),
            ),
            Err(Phase2Error::NotFound)
        );
        assert!(
            accept_invitation(
                &app.store,
                second,
                unaffected.invitation_id,
                &AcceptGroupInvitationRequest::new(
                    second_lease.fence(),
                    IdempotencyKey::new(Uuid::new_v4()).expect("key"),
                ),
            )
            .is_ok()
        );
    }

    #[test]
    fn invitation_limit_counts_cancelled_invites_and_expires_after_a_minute() {
        let (app, clock) = app_with_clock();
        let (sender, lease) = account(&app, "sender", "sender-invite");
        let (recipient, _recipient_lease) = account(&app, "recipient", "recipient-invite");
        let mut first = None;
        for _ in 0..MAX_INVITATIONS_PER_SENDER_PER_MINUTE {
            let invitation = create_invitation(
                &app.store,
                sender,
                &CreateGroupInvitationRequest::new(
                    lease.fence(),
                    recipient.character_id,
                    IdempotencyKey::new(Uuid::new_v4()).expect("key"),
                ),
            )
            .expect("within sender limit");
            assert_eq!(
                invitation.expires_at.value(),
                app.store.now() + GROUP_INVITATION_TTL_MS
            );
            first.get_or_insert(invitation);
        }
        let first = first.expect("first invitation");
        let cancel = coop_cloud::OnlineActionRequest {
            api_version: coop_cloud::ApiVersion::V1,
            fence: lease.fence(),
            idempotency_key: IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            action: coop_cloud::OnlineAction::Cancel {
                invitation_id: first.invitation_id,
            },
        };
        assert_eq!(
            super::super::online::action(&app, sender, &cancel),
            Ok(coop_cloud::OnlineActionResponse::Cancelled)
        );
        assert_eq!(
            create_invitation(
                &app.store,
                sender,
                &CreateGroupInvitationRequest::new(
                    lease.fence(),
                    recipient.character_id,
                    IdempotencyKey::new(Uuid::new_v4()).expect("key"),
                ),
            ),
            Err(Phase2Error::Busy)
        );
        clock.advance(GROUP_INVITATION_TTL_MS);
        app.store
            .read_transaction(|state| {
                assert_eq!(
                    recent_invitation_count(state, sender.character_id, app.store.now()),
                    0
                );
                Ok::<(), Phase2Error>(())
            })
            .expect("window elapsed");
    }

    #[test]
    fn inspection_fails_closed_when_reverse_membership_index_is_inconsistent() {
        let app = super::super::Phase2App::test();
        let (first_actor, first_lease, second_actor, _second_lease, group_id) =
            two_member_group(&app);
        app.store
            .write_transaction(|state| {
                state
                    .active_group_by_member
                    .remove(&second_actor.character_id);
                Ok::<(), Phase2Error>(())
            })
            .expect("remove reverse index");
        assert_eq!(
            inspect_group(&app.store, first_actor, group_id, first_lease.fence()),
            Err(Phase2Error::Internal)
        );
    }

    #[test]
    fn injected_repository_failure_preserves_group_state() {
        let (app, fail_writes) = app_with_toggle_repository();
        let (first_actor, first_lease, second_actor, _second_lease, group_id) =
            two_member_group(&app);
        set_progress(&app, first_actor, 0xff, true);
        set_progress(&app, second_actor, 0xff, true);
        let before = app
            .store
            .inspect_state(|state| {
                (
                    state.characters[&first_actor.character_id]
                        .state
                        .world_zone
                        .clone(),
                    state.characters[&second_actor.character_id]
                        .state
                        .world_zone
                        .clone(),
                    state.characters[&first_actor.character_id].world_revision,
                    state.characters[&second_actor.character_id].world_revision,
                    state.groups[&group_id].zone.clone(),
                    state.active_group_by_member.clone(),
                    state.group_invitations.len(),
                    state.group_idempotency.len(),
                )
            })
            .expect("state");
        fail_writes.store(true, Ordering::Release);
        let request = GroupTravelRequest::new(
            first_lease.fence(),
            "HOENN:SLATEPORT_SEVII_FERRY",
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        )
        .expect("travel");
        assert_eq!(
            app.travel_group(first_actor, group_id, request),
            Err(Phase2Error::Forbidden)
        );
        let after = app
            .store
            .inspect_state(|state| {
                (
                    state.characters[&first_actor.character_id]
                        .state
                        .world_zone
                        .clone(),
                    state.characters[&second_actor.character_id]
                        .state
                        .world_zone
                        .clone(),
                    state.characters[&first_actor.character_id].world_revision,
                    state.characters[&second_actor.character_id].world_revision,
                    state.groups[&group_id].zone.clone(),
                    state.active_group_by_member.clone(),
                    state.group_invitations.len(),
                    state.group_idempotency.len(),
                )
            })
            .expect("state");
        assert_eq!(after, before);
    }

    #[test]
    fn story_consent_and_marker_keep_zones_and_suspend_on_lost_save_receipt() {
        let (app, clock) = app_with_clock();
        let (first, first_lease, second, second_lease, group_id) = two_member_group(&app);
        let source = first_briney_story_route().source;
        set_consent_progress(&app, group_id, [first, second], source.clone());
        let request = GroupTravelProposalRequest::new_with_departure(
            first_lease.fence(),
            FIRST_BRINEY_ROUTE_ID,
            GroupTravelDeparture::Ferry,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        )
        .expect("well formed");
        let proposal = create_travel_proposal(&app.store, first, group_id, &request)
            .expect("public story proposal");
        let accepted = act_on_travel_proposal(
            &app.store,
            second,
            group_id,
            proposal.proposal_id,
            &GroupTravelActionRequest::new(
                second_lease.fence(),
                GroupTravelAction::Accept,
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            ),
        )
        .expect("consent");
        assert_eq!(
            accepted.status,
            GroupTravelProposalStatus::AwaitingSceneReceipts
        );
        assert!(accepted.commit.is_none());
        app.store
            .read_transaction(|state| {
                assert_eq!(state.groups[&group_id].zone, source);
                assert_eq!(
                    state.characters[&first.character_id].state.world_zone,
                    source
                );
                assert_eq!(
                    state.characters[&second.character_id].state.world_zone,
                    source
                );
                Ok::<_, Phase2Error>(())
            })
            .expect("no premature move");

        let marker = GroupTravelSceneMarkerRequest {
            api_version: coop_cloud::ApiVersion::V1,
            session_id: first_lease.fence().session_id,
            character_id: first.character_id,
            current_revision: first_lease.fence().current_revision,
            session_epoch: first_lease.fence().session_epoch,
            client_instance_id: first_lease.fence().client_instance_id,
            scene_nonce: 7,
        };
        let marked = mark_story_scene(&app.store, first, group_id, proposal.proposal_id, &marker)
            .expect("marker");
        assert_eq!(
            marked
                .scene_marked_by
                .iter()
                .filter(|marked| **marked)
                .count(),
            1
        );
        assert_eq!(
            mark_story_scene(&app.store, first, group_id, proposal.proposal_id, &marker),
            Ok(marked)
        );
        let absent_receipt = GroupTravelSceneReceiptRequest {
            api_version: coop_cloud::ApiVersion::V1,
            session_id: first_lease.fence().session_id,
            character_id: first.character_id,
            current_revision: first_lease.fence().current_revision,
            session_epoch: first_lease.fence().session_epoch,
            client_instance_id: first_lease.fence().client_instance_id,
            snapshot_id: coop_cloud::SnapshotId::new(Uuid::new_v4()).expect("snapshot id"),
        };
        assert_eq!(
            receipt_story_scene(
                &app.store,
                first,
                group_id,
                proposal.proposal_id,
                &absent_receipt
            ),
            Err(Phase2Error::NotFound)
        );
        clock.advance(STORY_SCENE_RECEIPT_TTL_MS + 1);
        let suspended = app
            .store
            .write_transaction(|state| {
                prune_travel_proposal_state(state, app.store.now());
                Ok::<_, Phase2Error>(
                    state.group_travel_proposals[&proposal.proposal_id]
                        .view
                        .clone(),
                )
            })
            .expect("suspend expired marker");
        assert_eq!(suspended.status, GroupTravelProposalStatus::Suspended);
        app.store
            .read_transaction(|state| {
                assert_eq!(state.groups[&group_id].zone, source);
                assert_eq!(
                    state.live_group_travel_by_group[&group_id],
                    proposal.proposal_id
                );
                assert_eq!(
                    state.live_group_travel_by_member[&first.character_id],
                    proposal.proposal_id
                );
                assert_eq!(
                    state.live_group_travel_by_member[&second.character_id],
                    proposal.proposal_id
                );
                Ok::<_, Phase2Error>(())
            })
            .expect("durable suspended lock");
        app.store
            .write_transaction(|state| {
                // A lease/session operation must not release the scene lock while
                // the group is still active; the closed-group transition below is
                // the recovery-safe point.
                cancel_pending_for_member(state, first.character_id);
                assert_eq!(
                    state.group_travel_proposals[&proposal.proposal_id]
                        .view
                        .status,
                    GroupTravelProposalStatus::Suspended
                );
                assert_eq!(
                    state.live_group_travel_by_group[&group_id],
                    proposal.proposal_id
                );
                Ok::<_, Phase2Error>(())
            })
            .expect("active suspended proposal remains fenced");
        app.store
            .write_transaction(|state| {
                state
                    .leases
                    .get_mut(&first.character_id)
                    .expect("first lease")
                    .grace_until = app.store.now().saturating_sub(1);
                Ok::<_, Phase2Error>(())
            })
            .expect("expire first lease");
        let expired =
            super::super::sessions::expire_groups(&app.store).expect("closed expired group");
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].group_id, group_id);
        app.store
            .read_transaction(|state| {
                let abandoned = &state.group_travel_proposals[&proposal.proposal_id];
                assert_eq!(abandoned.view.status, GroupTravelProposalStatus::Cancelled);
                assert!(abandoned.scene_markers[0].is_some());
                assert!(abandoned.retain_until.is_none());
                assert!(!state.live_group_travel_by_group.contains_key(&group_id));
                assert!(
                    !state
                        .live_group_travel_by_member
                        .contains_key(&first.character_id)
                );
                assert!(
                    !state
                        .live_group_travel_by_member
                        .contains_key(&second.character_id)
                );
                Ok::<_, Phase2Error>(())
            })
            .expect("closed suspended proposal");
    }

    #[test]
    fn closed_story_group_releases_locks_for_new_group_and_keeps_scene_evidence() {
        let app = super::super::Phase2App::test();
        let (first, first_lease, second, second_lease, group_id) = two_member_group(&app);
        let source = first_briney_story_route().source;
        set_consent_progress(&app, group_id, [first, second], source.clone());
        let fly = GroupTravelProposalRequest::new_with_departure(
            first_lease.fence(),
            "HOENN:FLY_LITTLEROOT",
            GroupTravelDeparture::Fly,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        )
        .expect("fly request");
        let proposal = create_travel_proposal(&app.store, first, group_id, &fly).expect("proposal");
        app.store
            .write_transaction(|state| {
                let view = &mut state
                    .group_travel_proposals
                    .get_mut(&proposal.proposal_id)
                    .expect("proposal")
                    .view;
                view.route_id = coop_cloud::RouteId::new(FIRST_BRINEY_ROUTE_ID)
                    .map_err(|_| Phase2Error::Internal)?;
                view.departure = GroupTravelDeparture::Ferry;
                view.destination = first_briney_story_route().destination;
                Ok::<_, Phase2Error>(())
            })
            .expect("test-only route");
        let accepted = act_on_travel_proposal(
            &app.store,
            second,
            group_id,
            proposal.proposal_id,
            &GroupTravelActionRequest::new(
                second_lease.fence(),
                GroupTravelAction::Accept,
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            ),
        )
        .expect("consent");
        assert_eq!(
            accepted.status,
            GroupTravelProposalStatus::AwaitingSceneReceipts
        );
        mark_story_scene(
            &app.store,
            first,
            group_id,
            proposal.proposal_id,
            &GroupTravelSceneMarkerRequest {
                api_version: coop_cloud::ApiVersion::V1,
                session_id: first_lease.fence().session_id,
                character_id: first.character_id,
                current_revision: first_lease.fence().current_revision,
                session_epoch: first_lease.fence().session_epoch,
                client_instance_id: first_lease.fence().client_instance_id,
                scene_nonce: 7,
            },
        )
        .expect("scene marker");
        super::super::online::action(
            &app,
            first,
            &OnlineActionRequest {
                api_version: coop_cloud::ApiVersion::V1,
                fence: first_lease.fence(),
                idempotency_key: IdempotencyKey::new(Uuid::new_v4()).expect("key"),
                action: OnlineAction::Leave { group_id },
            },
        )
        .expect("leave");
        let invitation = create_invitation(
            &app.store,
            first,
            &CreateGroupInvitationRequest::new(
                first_lease.fence(),
                second.character_id,
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            ),
        )
        .expect("new invitation");
        let new_group = accept_invitation(
            &app.store,
            second,
            invitation.invitation_id,
            &AcceptGroupInvitationRequest::new(
                second_lease.fence(),
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            ),
        )
        .expect("new group")
        .group
        .group_id;
        let next_request = GroupTravelProposalRequest::new_with_departure(
            first_lease.fence(),
            "HOENN:FLY_LITTLEROOT",
            GroupTravelDeparture::Fly,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        )
        .expect("next request");
        let next = create_travel_proposal(&app.store, first, new_group, &next_request)
            .expect("old locks cannot block new group");
        assert_eq!(next.status, GroupTravelProposalStatus::Pending);
        app.store
            .read_transaction(|state| {
                let abandoned = &state.group_travel_proposals[&proposal.proposal_id];
                assert_eq!(abandoned.view.status, GroupTravelProposalStatus::Cancelled);
                assert!(abandoned.retain_until.is_none());
                assert!(abandoned.scene_markers[0].is_some());
                assert!(abandoned.view.scene_marked_by[0]);
                assert!(abandoned.view.commit.is_none());
                assert_eq!(state.groups[&group_id].zone, source);
                assert_eq!(
                    state.live_group_travel_by_member[&first.character_id],
                    next.proposal_id
                );
                assert_eq!(
                    state.live_group_travel_by_member[&second.character_id],
                    next.proposal_id
                );
                assert!(!state.live_group_travel_by_group.contains_key(&group_id));
                Ok::<_, Phase2Error>(())
            })
            .expect("closed story evidence");
        let recovery = discover_story_travel_recovery(&app.store, first, first_lease.fence())
            .expect("own marker remains discoverable after closure and new proposal");
        assert_eq!(recovery.proposal_id, proposal.proposal_id);
        assert_eq!(recovery.group_id, group_id);
        assert_eq!(recovery.status, GroupTravelProposalStatus::Cancelled);
        assert_eq!(recovery.marker_fence, first_lease.fence());
        assert_eq!(recovery.scene_nonce, 7);
        assert!(recovery.marked_at.value() > 0);
        assert_eq!(
            discover_story_travel_recovery(&app.store, second, second_lease.fence()),
            Err(Phase2Error::NotFound)
        );
        assert_eq!(
            discover_story_travel_recovery(&app.store, first, second_lease.fence()),
            Err(Phase2Error::Authentication)
        );
        let abandon = StoryTravelRecoveryActionRequest {
            api_version: ApiVersion::V1,
            action: StoryTravelRecoveryAction::Abandon,
        };
        assert_eq!(
            resolve_story_travel_recovery(
                &app.store,
                first,
                proposal.proposal_id,
                first_lease.fence(),
                &abandon
            ),
            Err(Phase2Error::Conflict)
        );
        assert_eq!(
            resolve_story_travel_recovery(
                &app.store,
                second,
                proposal.proposal_id,
                second_lease.fence(),
                &abandon
            ),
            Err(Phase2Error::NotFound)
        );

        // A second marked attempt cannot multiply one member's unresolved
        // post-scene save evidence, even after the old group has closed.
        app.store
            .write_transaction(|state| {
                let view = &mut state
                    .group_travel_proposals
                    .get_mut(&next.proposal_id)
                    .expect("next proposal")
                    .view;
                view.route_id = coop_cloud::RouteId::new(FIRST_BRINEY_ROUTE_ID)
                    .map_err(|_| Phase2Error::Internal)?;
                view.departure = GroupTravelDeparture::Ferry;
                view.destination = first_briney_story_route().destination;
                Ok::<_, Phase2Error>(())
            })
            .expect("test-only route");
        let second_attempt = act_on_travel_proposal(
            &app.store,
            second,
            new_group,
            next.proposal_id,
            &GroupTravelActionRequest::new(
                second_lease.fence(),
                GroupTravelAction::Accept,
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            ),
        )
        .expect("second consent");
        assert_eq!(
            second_attempt.status,
            GroupTravelProposalStatus::AwaitingSceneReceipts
        );
        assert_eq!(
            mark_story_scene(
                &app.store,
                first,
                new_group,
                next.proposal_id,
                &GroupTravelSceneMarkerRequest {
                    api_version: coop_cloud::ApiVersion::V1,
                    session_id: first_lease.fence().session_id,
                    character_id: first.character_id,
                    current_revision: first_lease.fence().current_revision,
                    session_epoch: first_lease.fence().session_epoch,
                    client_instance_id: first_lease.fence().client_instance_id,
                    scene_nonce: 8,
                }
            ),
            Err(Phase2Error::Conflict)
        );
        assert_eq!(
            mark_story_scene(
                &app.store,
                second,
                new_group,
                next.proposal_id,
                &GroupTravelSceneMarkerRequest {
                    api_version: coop_cloud::ApiVersion::V1,
                    session_id: second_lease.fence().session_id,
                    character_id: second.character_id,
                    current_revision: second_lease.fence().current_revision,
                    session_epoch: second_lease.fence().session_epoch,
                    client_instance_id: second_lease.fence().client_instance_id,
                    scene_nonce: 9,
                }
            ),
            Err(Phase2Error::Conflict)
        );

        app.store
            .write_transaction(|state| {
                let mut duplicate = state.group_travel_proposals[&proposal.proposal_id].clone();
                let duplicate_id = GroupTravelProposalId::new(Uuid::new_v4())
                    .map_err(|_| Phase2Error::Internal)?;
                duplicate.view.proposal_id = duplicate_id;
                state.group_travel_proposals.insert(duplicate_id, duplicate);
                Ok::<_, Phase2Error>(())
            })
            .expect("second unresolved marker");
        assert_eq!(
            discover_story_travel_recovery(&app.store, first, first_lease.fence()),
            Err(Phase2Error::Conflict)
        );

        // Completed historical closures cannot consume global proposal slots
        // or grow one account's evictable archive without limit.
        app.store
            .write_transaction(|state| {
                let source = state.group_travel_proposals[&proposal.proposal_id].clone();
                for _ in 0..MAX_TRAVEL_PROPOSALS {
                    let mut archived = source.clone();
                    let id = GroupTravelProposalId::new(Uuid::new_v4())
                        .map_err(|_| Phase2Error::Internal)?;
                    archived.view.proposal_id = id;
                    archived.scene_markers[0]
                        .as_mut()
                        .expect("marker")
                        .marked_at -= 1;
                    let receipt = StorySceneReceipt {
                        snapshot_id: coop_cloud::SnapshotId::new(Uuid::new_v4())
                            .map_err(|_| Phase2Error::Internal)?,
                        revision: first_lease.fence().current_revision,
                    };
                    archived.scene_receipts[0] = Some(receipt);
                    archived.recovery_resolutions[0] =
                        Some(StoryRecoveryResolution::Reconciled(receipt));
                    state.group_travel_proposals.insert(id, archived);
                }
                prune_travel_proposal_state(state, app.store.now());
                assert_eq!(
                    state
                        .group_travel_proposals
                        .values()
                        .filter(|record| is_story_travel_archive(record))
                        .count(),
                    MAX_STORY_TRAVEL_ARCHIVES_PER_MEMBER + 2
                );
                assert_eq!(active_travel_proposal_count(state), 1);
                assert!(
                    state
                        .group_travel_proposals
                        .contains_key(&proposal.proposal_id)
                );
                Ok::<_, Phase2Error>(())
            })
            .expect("bounded story archive");
        // Resolved recovery records are replayable while retained, but cannot
        // grow without bound. The original unresolved marker stays protected.
        app.store
            .write_transaction(|state| {
                let source = state.group_travel_proposals[&proposal.proposal_id].clone();
                for _ in 0..=MAX_STORY_TRAVEL_ARCHIVES_PER_MEMBER {
                    let mut archived = source.clone();
                    let id = GroupTravelProposalId::new(Uuid::new_v4())
                        .map_err(|_| Phase2Error::Internal)?;
                    archived.view.proposal_id = id;
                    archived.recovery_resolutions[0] = Some(StoryRecoveryResolution::Abandoned);
                    state.group_travel_proposals.insert(id, archived);
                }
                prune_travel_proposal_state(state, app.store.now());
                assert_eq!(
                    state
                        .group_travel_proposals
                        .values()
                        .filter(|record| is_story_travel_archive(record)
                            && record.recovery_resolutions[0]
                                == Some(StoryRecoveryResolution::Abandoned))
                        .count(),
                    MAX_STORY_TRAVEL_ARCHIVES_PER_MEMBER
                );
                assert!(
                    state
                        .group_travel_proposals
                        .contains_key(&proposal.proposal_id)
                );
                assert_eq!(active_travel_proposal_count(state), 1);
                Ok::<_, Phase2Error>(())
            })
            .expect("resolved recovery archives are bounded");
        let (other_first, other_lease) = account(&app, "other-first", "other-first-invite");
        let (other_second, other_second_lease) =
            account(&app, "other-second", "other-second-invite");
        let invitation = create_invitation(
            &app.store,
            other_first,
            &CreateGroupInvitationRequest::new(
                other_lease.fence(),
                other_second.character_id,
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            ),
        )
        .expect("unrelated invitation");
        let other_group = accept_invitation(
            &app.store,
            other_second,
            invitation.invitation_id,
            &AcceptGroupInvitationRequest::new(
                other_second_lease.fence(),
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            ),
        )
        .expect("unrelated group")
        .group
        .group_id;
        set_consent_progress(
            &app,
            other_group,
            [other_first, other_second],
            first_briney_story_route().source,
        );
        let ordinary = create_travel_proposal(
            &app.store,
            other_first,
            other_group,
            &GroupTravelProposalRequest::new_with_departure(
                other_lease.fence(),
                "HOENN:FLY_LITTLEROOT",
                GroupTravelDeparture::Fly,
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            )
            .expect("ordinary request"),
        )
        .expect("unrelated travel remains available");
        assert_eq!(ordinary.status, GroupTravelProposalStatus::Pending);

        // A is a participant in nine old proposals where only partner B
        // marked the scene. Even though A exceeds the archive allowance,
        // pruning must retain B's finalized-save recovery evidence in all nine.
        app.store
            .write_transaction(|state| {
                let source = state.group_travel_proposals[&proposal.proposal_id].clone();
                let mut ninth = None;
                for _ in 0..=MAX_STORY_TRAVEL_ARCHIVES_PER_MEMBER {
                    let mut archived = source.clone();
                    let id = GroupTravelProposalId::new(Uuid::new_v4())
                        .map_err(|_| Phase2Error::Internal)?;
                    archived.view.proposal_id = id;
                    archived.scene_markers = [
                        None,
                        Some(StorySceneMarker {
                            fence: second_lease.fence(),
                            nonce: 10,
                            marked_at: app.store.now(),
                        }),
                    ];
                    archived.scene_receipts = [None, None];
                    archived.view.scene_marked_by = [false, true];
                    state.group_travel_proposals.insert(id, archived);
                    ninth = Some(id);
                }
                prune_travel_proposal_state(state, app.store.now());
                assert!(
                    state
                        .group_travel_proposals
                        .contains_key(&ninth.expect("ninth"))
                );
                assert_eq!(
                    state
                        .group_travel_proposals
                        .values()
                        .filter(|record| {
                            is_story_travel_archive(record)
                                && record.scene_markers[1].is_some()
                                && has_unresolved_story_marker(record)
                        })
                        .count(),
                    MAX_STORY_TRAVEL_ARCHIVES_PER_MEMBER + 1
                );
                assert_eq!(active_travel_proposal_count(state), 2);
                Ok::<_, Phase2Error>(())
            })
            .expect("partner recovery evidence survives ninth archive");
    }

    #[test]
    fn closed_story_marker_can_be_abandoned_only_on_new_lease_without_save() {
        let app = super::super::Phase2App::test();
        let (first, first_lease, second, second_lease, group_id) = two_member_group(&app);
        set_consent_progress(
            &app,
            group_id,
            [first, second],
            first_briney_story_route().source,
        );
        let request = GroupTravelProposalRequest::new_with_departure(
            first_lease.fence(),
            "HOENN:FLY_LITTLEROOT",
            GroupTravelDeparture::Fly,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        )
        .expect("request");
        let proposal =
            create_travel_proposal(&app.store, first, group_id, &request).expect("proposal");
        app.store
            .write_transaction(|state| {
                let view = &mut state
                    .group_travel_proposals
                    .get_mut(&proposal.proposal_id)
                    .expect("proposal")
                    .view;
                view.route_id = coop_cloud::RouteId::new(FIRST_BRINEY_ROUTE_ID)
                    .map_err(|_| Phase2Error::Internal)?;
                view.departure = GroupTravelDeparture::Ferry;
                view.destination = first_briney_story_route().destination;
                Ok::<_, Phase2Error>(())
            })
            .expect("test-only route");
        act_on_travel_proposal(
            &app.store,
            second,
            group_id,
            proposal.proposal_id,
            &GroupTravelActionRequest::new(
                second_lease.fence(),
                GroupTravelAction::Accept,
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            ),
        )
        .expect("consent");
        mark_story_scene(
            &app.store,
            first,
            group_id,
            proposal.proposal_id,
            &GroupTravelSceneMarkerRequest {
                api_version: ApiVersion::V1,
                session_id: first_lease.fence().session_id,
                character_id: first.character_id,
                current_revision: first_lease.fence().current_revision,
                session_epoch: first_lease.fence().session_epoch,
                client_instance_id: first_lease.fence().client_instance_id,
                scene_nonce: 7,
            },
        )
        .expect("marker");
        super::super::online::action(
            &app,
            first,
            &OnlineActionRequest {
                api_version: ApiVersion::V1,
                fence: first_lease.fence(),
                idempotency_key: IdempotencyKey::new(Uuid::new_v4()).expect("key"),
                action: OnlineAction::Leave { group_id },
            },
        )
        .expect("leave");
        let abandon = StoryTravelRecoveryActionRequest {
            api_version: ApiVersion::V1,
            action: StoryTravelRecoveryAction::Abandon,
        };
        assert_eq!(
            resolve_story_travel_recovery(
                &app.store,
                first,
                proposal.proposal_id,
                first_lease.fence(),
                &abandon
            ),
            Err(Phase2Error::Conflict)
        );
        app.release(
            first,
            coop_cloud::ReleaseLeaseRequest::new(
                first_lease.fence(),
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            ),
        )
        .expect("release");
        let new_lease = app
            .acquire(
                first,
                AcquireLeaseRequest::new(
                    first.character_id,
                    ClientInstanceId::new(Uuid::new_v4()).expect("client"),
                    IdempotencyKey::new(Uuid::new_v4()).expect("key"),
                ),
            )
            .expect("new lease");
        let resolved = resolve_story_travel_recovery(
            &app.store,
            first,
            proposal.proposal_id,
            new_lease.fence(),
            &abandon,
        )
        .expect("abandon");
        assert_eq!(resolved.outcome, StoryTravelRecoveryOutcome::Abandoned);
        assert_eq!(
            resolve_story_travel_recovery(
                &app.store,
                first,
                proposal.proposal_id,
                new_lease.fence(),
                &abandon
            ),
            Ok(resolved)
        );
        assert_eq!(
            resolve_story_travel_recovery(
                &app.store,
                first,
                proposal.proposal_id,
                new_lease.fence(),
                &StoryTravelRecoveryActionRequest {
                    api_version: ApiVersion::V1,
                    action: StoryTravelRecoveryAction::Reconcile,
                }
            ),
            Err(Phase2Error::Conflict)
        );
        assert_eq!(
            discover_story_travel_recovery(&app.store, first, new_lease.fence()),
            Err(Phase2Error::NotFound)
        );
        app.store
            .read_transaction(|state| {
                let record = &state.group_travel_proposals[&proposal.proposal_id];
                assert_eq!(
                    record.recovery_resolutions[0],
                    Some(StoryRecoveryResolution::Abandoned)
                );
                assert!(record.scene_markers[0].is_some());
                assert!(record.scene_receipts[0].is_none());
                assert_eq!(state.characters[&first.character_id].world_revision, 0);
                Ok::<_, Phase2Error>(())
            })
            .expect("audit marker remains");
    }

    fn dewford_first_voyage_sav() -> Vec<u8> {
        let mut bytes =
            include_bytes!("../../../coop-save/tests/fixtures/stock-mgba-first-save.sav").to_vec();
        // The selected stock slot carries the real ROM extension. Only the
        // SaveBlock1 scene facts change; seal each affected flash sector.
        for (offset, data) in [
            (0x04_usize, &[0_u8, 11][..]),
            (0x14b8, &[0, 0][..]),
            (0x1270 + 0x26, &[0x04][..]),
            (0x1270 + 0x5c, &[0x40][..]),
        ] {
            for (index, byte) in data.iter().enumerate() {
                let position = offset + index;
                let logical = 1 + position / coop_save::SAVE_BLOCK3_CHUNK_OFFSET;
                for slot in 0..2 {
                    let sector = (0..coop_save::SECTORS_PER_SLOT).find_map(|physical| {
                        let start = (slot * coop_save::SECTORS_PER_SLOT + physical)
                            * coop_save::SECTOR_SIZE;
                        (u16::from_le_bytes(bytes[start + 4084..start + 4086].try_into().unwrap())
                            == logical as u16)
                            .then_some(start)
                    });
                    let Some(start) = sector else {
                        continue;
                    };
                    bytes[start + position % coop_save::SAVE_BLOCK3_CHUNK_OFFSET] = *byte;
                    let size = coop_save::LOGICAL_SECTOR_DATA_SIZES[logical];
                    let checksum = coop_save::sector_checksum(&bytes[start..start + size]);
                    bytes[start + 4086..start + 4088].copy_from_slice(&checksum.to_le_bytes());
                }
            }
        }
        let validated = coop_save::parse(
            &bytes,
            coop_save::RegistryContract::new(
                coop_protocol::IDENTITY_REGISTRY_VERSION,
                coop_protocol::IDENTITY_REGISTRY_DIGEST,
            ),
        )
        .expect("valid stock SAV");
        assert!(
            validated
                .briney_voyage_evidence()
                .is_first_voyage_post_scene_at(0, 11)
        );
        bytes
    }

    fn finalize_first_voyage_snapshot(
        app: &super::super::Phase2App,
        actor: AuthenticatedActor,
        fence: LeaseFence,
        sav: &[u8],
    ) -> (coop_cloud::SnapshotId, LeaseFence) {
        let snapshot_id = coop_cloud::SnapshotId::new(Uuid::new_v4()).expect("snapshot id");
        let sav_file =
            coop_cloud::SnapshotFile::from_bytes(coop_cloud::ArtifactIdentity::CharacterSav, sav)
                .expect("sav file");
        let pending_file = coop_cloud::SnapshotFile::from_bytes(
            coop_cloud::ArtifactIdentity::PendingCommits,
            b"[]",
        )
        .expect("pending file");
        let next_revision = fence.current_revision.next().expect("next revision");
        let snapshot = coop_cloud::SnapshotRecord::new(
            snapshot_id,
            coop_cloud::SnapshotFence::new(
                fence.session_id,
                actor.character_id,
                fence.session_epoch,
            ),
            fence.current_revision,
            next_revision,
            vec![sav_file, pending_file.clone()],
            pending_file.sha256,
            None,
            UnixTimestampMillis::new(app.store.now()),
        )
        .expect("finalized record");
        app.store
            .objects
            .put(
                Store::object_key(
                    actor.character_id,
                    snapshot_id,
                    coop_cloud::ArtifactIdentity::CharacterSav,
                ),
                sav.to_vec(),
            )
            .expect("stored SAV");
        app.store
            .write_transaction(|state| {
                state.snapshots.insert(snapshot_id, snapshot.clone());
                state
                    .snapshot_by_revision
                    .insert((actor.character_id, next_revision), snapshot_id);
                let head = state
                    .characters
                    .get_mut(&actor.character_id)
                    .expect("character");
                head.revision = next_revision;
                head.active_snapshot = Some(snapshot_id);
                let lease = state.leases.get_mut(&actor.character_id).expect("lease");
                lease.contract = coop_cloud::LeaseContract::new(
                    coop_cloud::LeaseFence::new(
                        fence.session_id,
                        actor.character_id,
                        next_revision,
                        fence.session_epoch,
                        fence.client_instance_id,
                    ),
                    lease.contract.expires_at,
                    lease.contract.heartbeat_interval_ms,
                )
                .map_err(|_| Phase2Error::Internal)?;
                Ok::<_, Phase2Error>(())
            })
            .expect("server finalized head");
        let updated_fence = app
            .store
            .inspect_state(|state| state.leases[&actor.character_id].contract.fence())
            .expect("fence");
        (snapshot_id, updated_fence)
    }

    #[test]
    fn both_first_voyage_receipts_commit_zones_and_release_live_proposal_indexes() {
        let app = super::super::Phase2App::test();
        let (first, first_lease, second, second_lease, group_id) = two_member_group(&app);
        let source = first_briney_story_route().source;
        let destination = first_briney_story_route().destination;
        set_consent_progress(&app, group_id, [first, second], source.clone());
        let proposal = create_travel_proposal(
            &app.store,
            first,
            group_id,
            &GroupTravelProposalRequest::new_with_departure(
                first_lease.fence(),
                FIRST_BRINEY_ROUTE_ID,
                GroupTravelDeparture::Ferry,
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            )
            .expect("request"),
        )
        .expect("proposal");
        act_on_travel_proposal(
            &app.store,
            second,
            group_id,
            proposal.proposal_id,
            &GroupTravelActionRequest::new(
                second_lease.fence(),
                GroupTravelAction::Accept,
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            ),
        )
        .expect("consent");
        for (actor, fence, nonce) in [
            (first, first_lease.fence(), 7),
            (second, second_lease.fence(), 8),
        ] {
            mark_story_scene(
                &app.store,
                actor,
                group_id,
                proposal.proposal_id,
                &GroupTravelSceneMarkerRequest {
                    api_version: ApiVersion::V1,
                    session_id: fence.session_id,
                    character_id: actor.character_id,
                    current_revision: fence.current_revision,
                    session_epoch: fence.session_epoch,
                    client_instance_id: fence.client_instance_id,
                    scene_nonce: nonce,
                },
            )
            .expect("marker");
        }
        let sav = dewford_first_voyage_sav();
        let (first_snapshot, first_fence) =
            finalize_first_voyage_snapshot(&app, first, first_lease.fence(), &sav);
        let first_receipt = GroupTravelSceneReceiptRequest {
            api_version: ApiVersion::V1,
            session_id: first_fence.session_id,
            character_id: first.character_id,
            current_revision: first_fence.current_revision,
            session_epoch: first_fence.session_epoch,
            client_instance_id: first_fence.client_instance_id,
            snapshot_id: first_snapshot,
        };
        let partial = receipt_story_scene(
            &app.store,
            first,
            group_id,
            proposal.proposal_id,
            &first_receipt,
        )
        .expect("first receipt");
        assert_eq!(
            partial.status,
            GroupTravelProposalStatus::AwaitingSceneReceipts
        );
        app.store
            .read_transaction(|state| {
                assert_eq!(state.groups[&group_id].zone, source);
                assert_eq!(
                    state.characters[&first.character_id].state.world_zone,
                    source
                );
                assert_eq!(
                    state.characters[&second.character_id].state.world_zone,
                    source
                );
                assert_eq!(
                    state.live_group_travel_by_group[&group_id],
                    proposal.proposal_id
                );
                Ok::<_, Phase2Error>(())
            })
            .expect("first receipt does not move either member");
        let (second_snapshot, second_fence) =
            finalize_first_voyage_snapshot(&app, second, second_lease.fence(), &sav);
        let second_receipt = GroupTravelSceneReceiptRequest {
            api_version: ApiVersion::V1,
            session_id: second_fence.session_id,
            character_id: second.character_id,
            current_revision: second_fence.current_revision,
            session_epoch: second_fence.session_epoch,
            client_instance_id: second_fence.client_instance_id,
            snapshot_id: second_snapshot,
        };
        let committed = receipt_story_scene(
            &app.store,
            second,
            group_id,
            proposal.proposal_id,
            &second_receipt,
        )
        .expect("second receipt");
        assert_eq!(committed.status, GroupTravelProposalStatus::Committed);
        assert_eq!(committed.scene_receipted_by, [true, true]);
        assert_eq!(
            receipt_story_scene(
                &app.store,
                first,
                group_id,
                proposal.proposal_id,
                &first_receipt
            ),
            Ok(committed.clone())
        );
        app.store
            .read_transaction(|state| {
                assert_eq!(state.groups[&group_id].zone, destination);
                assert_eq!(state.groups[&group_id].zone_revision, 1);
                for actor in [first, second] {
                    assert_eq!(
                        state.characters[&actor.character_id].state.world_zone,
                        destination
                    );
                    assert_eq!(state.characters[&actor.character_id].world_revision, 1);
                    assert!(
                        !state
                            .live_group_travel_by_member
                            .contains_key(&actor.character_id)
                    );
                }
                assert!(!state.live_group_travel_by_group.contains_key(&group_id));
                Ok::<_, Phase2Error>(())
            })
            .expect("atomic shared zone commit and unlocked proposal");
        create_travel_proposal(
            &app.store,
            first,
            group_id,
            &GroupTravelProposalRequest::new_with_departure(
                first_fence,
                "HOENN:DEWFORD_BRINEY_HOUSE_FERRY",
                GroupTravelDeparture::Ferry,
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            )
            .expect("next request"),
        )
        .expect("new route after story commit");
    }

    #[test]
    fn receipted_story_marker_blocks_resume_and_reconciles_only_verified_active_head() {
        let app = super::super::Phase2App::test();
        let (first, first_lease, second, second_lease, group_id) = two_member_group(&app);
        set_consent_progress(
            &app,
            group_id,
            [first, second],
            first_briney_story_route().source,
        );
        let request = GroupTravelProposalRequest::new_with_departure(
            first_lease.fence(),
            "HOENN:FLY_LITTLEROOT",
            GroupTravelDeparture::Fly,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        )
        .expect("request");
        let proposal =
            create_travel_proposal(&app.store, first, group_id, &request).expect("proposal");
        app.store
            .write_transaction(|state| {
                let view = &mut state
                    .group_travel_proposals
                    .get_mut(&proposal.proposal_id)
                    .expect("proposal")
                    .view;
                view.route_id = coop_cloud::RouteId::new(FIRST_BRINEY_ROUTE_ID)
                    .map_err(|_| Phase2Error::Internal)?;
                view.departure = GroupTravelDeparture::Ferry;
                view.destination = first_briney_story_route().destination;
                Ok::<_, Phase2Error>(())
            })
            .expect("test-only route");
        act_on_travel_proposal(
            &app.store,
            second,
            group_id,
            proposal.proposal_id,
            &GroupTravelActionRequest::new(
                second_lease.fence(),
                GroupTravelAction::Accept,
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            ),
        )
        .expect("consent");
        mark_story_scene(
            &app.store,
            first,
            group_id,
            proposal.proposal_id,
            &GroupTravelSceneMarkerRequest {
                api_version: ApiVersion::V1,
                session_id: first_lease.fence().session_id,
                character_id: first.character_id,
                current_revision: first_lease.fence().current_revision,
                session_epoch: first_lease.fence().session_epoch,
                client_instance_id: first_lease.fence().client_instance_id,
                scene_nonce: 7,
            },
        )
        .expect("marker");
        let sav = dewford_first_voyage_sav();
        let snapshot_id = coop_cloud::SnapshotId::new(Uuid::new_v4()).expect("snapshot id");
        let sav_file =
            coop_cloud::SnapshotFile::from_bytes(coop_cloud::ArtifactIdentity::CharacterSav, &sav)
                .expect("sav file");
        let pending_file = coop_cloud::SnapshotFile::from_bytes(
            coop_cloud::ArtifactIdentity::PendingCommits,
            b"[]",
        )
        .expect("pending file");
        let next_revision = first_lease
            .fence()
            .current_revision
            .next()
            .expect("next revision");
        let snapshot = coop_cloud::SnapshotRecord::new(
            snapshot_id,
            coop_cloud::SnapshotFence::new(
                first_lease.fence().session_id,
                first.character_id,
                first_lease.fence().session_epoch,
            ),
            first_lease.fence().current_revision,
            next_revision,
            vec![sav_file, pending_file.clone()],
            pending_file.sha256,
            None,
            UnixTimestampMillis::new(app.store.now()),
        )
        .expect("finalized record");
        let object_key = Store::object_key(
            first.character_id,
            snapshot_id,
            coop_cloud::ArtifactIdentity::CharacterSav,
        );
        app.store
            .objects
            .put(object_key.clone(), sav.clone())
            .expect("stored SAV");
        app.store
            .write_transaction(|state| {
                state.snapshots.insert(snapshot_id, snapshot.clone());
                state
                    .snapshot_by_revision
                    .insert((first.character_id, next_revision), snapshot_id);
                let head = state
                    .characters
                    .get_mut(&first.character_id)
                    .expect("character");
                head.revision = next_revision;
                head.active_snapshot = Some(snapshot_id);
                let lease = state.leases.get_mut(&first.character_id).expect("lease");
                lease.contract = coop_cloud::LeaseContract::new(
                    coop_cloud::LeaseFence::new(
                        first_lease.fence().session_id,
                        first.character_id,
                        next_revision,
                        first_lease.fence().session_epoch,
                        first_lease.fence().client_instance_id,
                    ),
                    lease.contract.expires_at,
                    lease.contract.heartbeat_interval_ms,
                )
                .map_err(|_| Phase2Error::Internal)?;
                state
                    .group_travel_proposals
                    .get_mut(&proposal.proposal_id)
                    .expect("proposal")
                    .scene_receipts[0] = Some(StorySceneReceipt {
                    snapshot_id,
                    revision: next_revision,
                });
                Ok::<_, Phase2Error>(())
            })
            .expect("server finalized head");
        let finalized_fence = app
            .store
            .inspect_state(|state| state.leases[&first.character_id].contract.fence())
            .expect("fence");
        assert_eq!(
            discover_story_travel_recovery(&app.store, first, finalized_fence)
                .expect("pending receipted scene")
                .proposal_id,
            proposal.proposal_id
        );
        let reconcile = StoryTravelRecoveryActionRequest {
            api_version: ApiVersion::V1,
            action: StoryTravelRecoveryAction::Reconcile,
        };
        assert_eq!(
            resolve_story_travel_recovery(
                &app.store,
                first,
                proposal.proposal_id,
                finalized_fence,
                &reconcile
            ),
            Err(Phase2Error::Conflict)
        );
        super::super::online::action(
            &app,
            first,
            &OnlineActionRequest {
                api_version: ApiVersion::V1,
                fence: finalized_fence,
                idempotency_key: IdempotencyKey::new(Uuid::new_v4()).expect("key"),
                action: OnlineAction::Leave { group_id },
            },
        )
        .expect("leave");
        assert_eq!(
            discover_story_travel_recovery(&app.store, first, finalized_fence)
                .expect("closed receipted scene")
                .proposal_id,
            proposal.proposal_id
        );
        assert_eq!(
            resolve_story_travel_recovery(
                &app.store,
                first,
                proposal.proposal_id,
                finalized_fence,
                &StoryTravelRecoveryActionRequest {
                    api_version: ApiVersion::V1,
                    action: StoryTravelRecoveryAction::Abandon,
                }
            ),
            Err(Phase2Error::Conflict)
        );
        app.store
            .objects
            .put(object_key.clone(), b"corrupt".to_vec())
            .expect("corrupt object");
        assert_eq!(
            resolve_story_travel_recovery(
                &app.store,
                first,
                proposal.proposal_id,
                finalized_fence,
                &reconcile
            ),
            Err(Phase2Error::Conflict)
        );
        app.store
            .objects
            .put(object_key, sav)
            .expect("restore object");
        let conflicting_id = coop_cloud::SnapshotId::new(Uuid::new_v4()).expect("snapshot id");
        app.store
            .write_transaction(|state| {
                let mut duplicate = snapshot.clone();
                duplicate.snapshot_id = conflicting_id;
                state.snapshots.insert(conflicting_id, duplicate);
                Ok::<_, Phase2Error>(())
            })
            .expect("ambiguous finalized lineage");
        assert_eq!(
            resolve_story_travel_recovery(
                &app.store,
                first,
                proposal.proposal_id,
                finalized_fence,
                &reconcile
            ),
            Err(Phase2Error::Conflict)
        );
        app.store
            .write_transaction(|state| {
                state.snapshots.remove(&conflicting_id);
                Ok::<_, Phase2Error>(())
            })
            .expect("restore unique lineage");
        let resolved = resolve_story_travel_recovery(
            &app.store,
            first,
            proposal.proposal_id,
            finalized_fence,
            &reconcile,
        )
        .expect("reconcile");
        assert_eq!(resolved.outcome, StoryTravelRecoveryOutcome::Reconciled);
        assert_eq!(
            resolve_story_travel_recovery(
                &app.store,
                first,
                proposal.proposal_id,
                finalized_fence,
                &reconcile
            ),
            Ok(resolved)
        );
        app.store
            .read_transaction(|state| {
                let head = &state.characters[&first.character_id];
                assert_eq!(
                    head.state.world_zone,
                    first_briney_story_route().destination
                );
                assert_eq!(head.world_revision, 1);
                let record = &state.group_travel_proposals[&proposal.proposal_id];
                assert_eq!(
                    record.scene_receipts[0],
                    Some(StorySceneReceipt {
                        snapshot_id,
                        revision: next_revision,
                    })
                );
                assert!(record.scene_markers[0].is_some());
                assert_eq!(
                    record.recovery_resolutions[0],
                    Some(StoryRecoveryResolution::Reconciled(StorySceneReceipt {
                        snapshot_id,
                        revision: next_revision,
                    }))
                );
                Ok::<_, Phase2Error>(())
            })
            .expect("reconciled state");
    }
}
