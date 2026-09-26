//! Release-pinned ROM build selection for resume packages.
//!
//! The digest must come from trusted release configuration. A client-provided
//! world ID or catalog digest cannot authorize a build identity.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use coop_cloud::{RuntimeBuildIdentity, Sha256Digest};
use coop_protocol::RomWorldId;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireCatalog {
    schema_version: u16,
    shared_player_descriptor_sha256: Option<Sha256Digest>,
    shared_player_descriptor_hex: Option<String>,
    worlds: Vec<WireWorld>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireWorld {
    world_id: RomWorldId,
    build: RuntimeBuildIdentity,
    arrivals: Option<WireArrivals>,
    portals: Option<Vec<WirePortal>>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct WirePortal {
    id: String,
    destination_world_id: RomWorldId,
    arrival_portal_id: String,
    return_portal_id: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum WireArrivals {
    Legacy(Vec<String>),
    Pinned(Vec<WireArrival>),
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WireArrival {
    id: String,
    map_group: u8,
    map_number: u8,
    warp_id: u8,
    map_layout_id: u16,
    template_sav_path: String,
    template_sav_sha256: Sha256Digest,
}

/// Exact build identities authenticated by a separately pinned release digest.
pub(crate) struct TrustedBuildCatalog {
    worlds: HashMap<RomWorldId, RuntimeBuildIdentity>,
    descriptor_sha256: Option<Sha256Digest>,
    descriptor: Option<Vec<u8>>,
    portals: HashMap<(RomWorldId, String), WirePortal>,
    arrival_templates: HashMap<(RomWorldId, String), WireArrival>,
    arrival_saves: HashMap<(RomWorldId, String), Vec<u8>>,
}

impl TrustedBuildCatalog {
    /// Validate the complete byte string against a release-owned digest before
    /// interpreting any world entry. At most 32 worlds may be installed.
    pub(crate) fn from_release_bytes(
        bytes: &[u8],
        trusted_digest: Sha256Digest,
    ) -> Result<Self, &'static str> {
        if Sha256Digest::of_bytes(bytes) != trusted_digest {
            return Err("release catalog digest mismatch");
        }
        let wire: WireCatalog =
            serde_json::from_slice(bytes).map_err(|_| "invalid build catalog")?;
        if !matches!(wire.schema_version, 1 | 2 | 3)
            || wire.worlds.is_empty()
            || wire.worlds.len() > 32
            || (wire.schema_version == 1) != wire.shared_player_descriptor_sha256.is_none()
        {
            return Err("invalid build catalog schema or size");
        }
        let descriptor = match (wire.schema_version, wire.shared_player_descriptor_hex) {
            (3, Some(encoded)) => {
                if encoded.len() > 4096 || encoded.len() % 2 != 0 {
                    return Err("invalid player descriptor encoding");
                }
                let bytes = encoded
                    .as_bytes()
                    .chunks_exact(2)
                    .map(|pair| {
                        std::str::from_utf8(pair)
                            .ok()
                            .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                            .ok_or("invalid player descriptor encoding")
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if wire.shared_player_descriptor_sha256 != Some(Sha256Digest::of_bytes(&bytes)) {
                    return Err("player descriptor digest mismatch");
                }
                Some(bytes)
            }
            (1 | 2, None) => None,
            _ => return Err("player descriptor does not match catalog schema"),
        };
        let mut worlds = HashMap::with_capacity(wire.worlds.len());
        let mut build_ids = HashSet::with_capacity(wire.worlds.len());
        let mut rom_digests = HashSet::with_capacity(wire.worlds.len());
        let mut arrivals = HashMap::with_capacity(wire.worlds.len());
        let mut arrival_templates = HashMap::new();
        let mut portals = HashMap::new();
        for entry in wire.worlds {
            if wire.schema_version == 1 {
                if entry.arrivals.is_some() || entry.portals.is_some() {
                    return Err("legacy build catalog cannot include travel routes");
                }
            } else {
                let world_arrivals = entry.arrivals.ok_or("missing world arrivals")?;
                let world_portals = entry.portals.ok_or("missing world portals")?;
                let mut unique_arrivals = HashSet::new();
                match (wire.schema_version, world_arrivals) {
                    (2, WireArrivals::Legacy(names)) => {
                        for arrival in names {
                            if !valid_portal_id(&arrival) || !unique_arrivals.insert(arrival) {
                                return Err("invalid or duplicate arrival portal");
                            }
                        }
                    }
                    (3, WireArrivals::Pinned(templates)) => {
                        for arrival in templates {
                            if !valid_portal_id(&arrival.id)
                                || arrival.map_layout_id == 0
                                || !valid_release_path(&arrival.template_sav_path)
                                || !unique_arrivals.insert(arrival.id.clone())
                            {
                                return Err("invalid or duplicate arrival template");
                            }
                            arrival_templates.insert((entry.world_id, arrival.id.clone()), arrival);
                        }
                    }
                    _ => return Err("arrival format does not match catalog schema"),
                }
                arrivals.insert(entry.world_id, unique_arrivals);
                for portal in world_portals {
                    if !valid_portal_id(&portal.id)
                        || !valid_portal_id(&portal.arrival_portal_id)
                        || !valid_portal_id(&portal.return_portal_id)
                        || portal.destination_world_id == entry.world_id
                        || portals
                            .insert((entry.world_id, portal.id.clone()), portal)
                            .is_some()
                    {
                        return Err("invalid or duplicate world portal");
                    }
                }
            }
            if !build_ids.insert(entry.build.game_build_id.clone())
                || !rom_digests.insert(entry.build.rom_sha256)
                || worlds.insert(entry.world_id, entry.build).is_some()
            {
                return Err("duplicate release world or build identity");
            }
        }
        if portals.len() > 256 {
            return Err("too many world portals");
        }
        for ((source, _), portal) in &portals {
            let destination = portal.destination_world_id;
            if !worlds.contains_key(&destination)
                || !arrivals
                    .get(&destination)
                    .is_some_and(|names| names.contains(&portal.arrival_portal_id))
                || !portals
                    .get(&(destination, portal.return_portal_id.clone()))
                    .is_some_and(|reverse| {
                        reverse.destination_world_id == *source
                            && reverse.return_portal_id == portal.id
                    })
            {
                return Err("world portal has no reciprocal arrival and return");
            }
        }
        Ok(Self {
            worlds,
            descriptor_sha256: wire.shared_player_descriptor_sha256,
            descriptor,
            portals,
            arrival_templates,
            arrival_saves: HashMap::new(),
        })
    }

    /// Select only a world durably bound to this snapshot. The optional
    /// requested ID is a consistency check, never the source of authority.
    pub(crate) fn for_snapshot(
        &self,
        snapshot_world: Option<RomWorldId>,
        requested_world: Option<RomWorldId>,
    ) -> Result<&RuntimeBuildIdentity, &'static str> {
        let world_id = snapshot_world.ok_or("snapshot has no authoritative world")?;
        if requested_world.is_some_and(|requested| requested != world_id) {
            return Err("requested world differs from snapshot world");
        }
        self.worlds.get(&world_id).ok_or("unknown snapshot world")
    }

    /// Resolve an exact catalog build asserted by a runtime to one world.
    /// Duplicate build IDs and ROM digests are rejected when loading.
    pub(crate) fn world_for_build(&self, build: &RuntimeBuildIdentity) -> Option<RomWorldId> {
        self.worlds
            .iter()
            .find_map(|(world, installed)| (installed == build).then_some(*world))
    }

    /// Resolve the destination arrival from a separately digest-pinned travel
    /// catalog. A caller-provided arrival must never choose the spawn point.
    /// Legacy build-only catalogs cannot authorize travel.
    #[allow(dead_code)] // Used by the forthcoming atomic handoff.
    pub(crate) fn resolve_portal(
        &self,
        source: RomWorldId,
        portal_id: &str,
        descriptor_sha256: Sha256Digest,
    ) -> Option<(RomWorldId, &str)> {
        if self.descriptor_sha256 != Some(descriptor_sha256) {
            return None;
        }
        self.portals
            .get(&(source, portal_id.to_owned()))
            .map(|portal| {
                (
                    portal.destination_world_id,
                    portal.arrival_portal_id.as_str(),
                )
            })
    }

    /// A release-pinned cold-start save for a first visit. Returning to a
    /// visited world instead uses that world's dormant head.
    pub(crate) fn arrival_template(
        &self,
        world_id: RomWorldId,
        arrival_id: &str,
    ) -> Option<(&str, Sha256Digest, (u8, u8, u8))> {
        self.arrival_templates
            .get(&(world_id, arrival_id.to_owned()))
            .map(|arrival| {
                (
                    arrival.template_sav_path.as_str(),
                    arrival.template_sav_sha256,
                    (arrival.map_group, arrival.map_number, arrival.warp_id),
                )
            })
    }

    pub(crate) fn transfer_descriptor(&self) -> Option<&[u8]> {
        self.descriptor.as_deref()
    }

    /// Load every first-arrival image at startup from the release directory.
    /// Each image is checked against the pinned digest and parsed as a V2 save;
    /// later filesystem changes cannot alter the cached travel input.
    pub(crate) fn load_arrival_saves(&mut self, release_root: &Path) -> Result<(), &'static str> {
        let root = release_root
            .canonicalize()
            .map_err(|_| "release root is missing")?;
        let mut loaded = HashMap::with_capacity(self.arrival_templates.len());
        for (key, arrival) in &self.arrival_templates {
            let path = root
                .join(&arrival.template_sav_path)
                .canonicalize()
                .map_err(|_| "arrival save is missing")?;
            if !path.starts_with(&root) || !path.is_file() {
                return Err("arrival save escapes release root");
            }
            let size = path
                .metadata()
                .map_err(|_| "arrival save metadata unavailable")?
                .len();
            if size != coop_save::FLASH_IMAGE_SIZE as u64
                && size != (coop_save::FLASH_IMAGE_SIZE + coop_save::RTC_TRAILER_SIZE) as u64
            {
                return Err("invalid arrival save size");
            }
            let bytes = std::fs::read(&path).map_err(|_| "arrival save unreadable")?;
            if Sha256Digest::of_bytes(&bytes) != arrival.template_sav_sha256 {
                return Err("arrival save digest mismatch");
            }
            let save = coop_save::parse_v2(&bytes, super::identity_registry_contract())
                .map_err(|_| "arrival save is not valid V2")?;
            if !save.coop().online_eligible() {
                return Err("arrival save is not online eligible");
            }
            let local = save
                .logical_sector_payload(1)
                .ok_or("arrival save has no map state")?;
            if local.get(4..7) != Some(&[arrival.map_group, arrival.map_number, arrival.warp_id]) {
                return Err("arrival save map does not match catalog");
            }
            if local.get(0x32..0x34) != Some(arrival.map_layout_id.to_le_bytes().as_slice()) {
                return Err("arrival save layout does not match catalog");
            }
            loaded.insert(key.clone(), bytes);
        }
        self.arrival_saves = loaded;
        Ok(())
    }

    pub(crate) fn arrival_save(&self, world_id: RomWorldId, arrival_id: &str) -> Option<&[u8]> {
        self.arrival_saves
            .get(&(world_id, arrival_id.to_owned()))
            .map(Vec::as_slice)
    }
}

fn valid_release_path(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('\\')
        && !value.contains(':')
        && !value.starts_with('/')
        && value
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn valid_portal_id(value: &str) -> bool {
    value.len() <= 96
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

#[cfg(test)]
mod tests {
    use coop_cloud::Sha256Digest;
    use coop_protocol::RomWorldId;
    use serde_json::{Value, json};

    use super::TrustedBuildCatalog;

    fn fixture() -> Vec<u8> {
        let worlds = [(1, "main"), (2, "cormoria"), (7, "third")]
            .into_iter()
            .map(|(world_id, name)| {
                json!({
                    "world_id": world_id,
                    "build": {
                        "game_build_id": format!("game-{name}"),
                        "rom_sha256": Sha256Digest::of_bytes(name.as_bytes()).as_hex(),
                        "mgba_version": "0.10.5",
                        "bridge_abi": 1,
                        "protocol_version": 1
                    }
                })
            })
            .collect::<Vec<_>>();
        serde_json::to_vec(&json!({"schema_version": 1, "worlds": worlds})).unwrap()
    }

    fn travel_fixture() -> Vec<u8> {
        let mut catalog: Value = serde_json::from_slice(&fixture()).unwrap();
        catalog["schema_version"] = json!(2);
        catalog["shared_player_descriptor_sha256"] =
            json!(Sha256Digest::of_bytes(b"shared-schema").as_hex());
        let routes = [
            (
                vec!["from_cormoria", "from_third"],
                vec![
                    json!({"id":"to_cormoria","destination_world_id":2,"arrival_portal_id":"from_main","return_portal_id":"to_main"}),
                    json!({"id":"to_third","destination_world_id":7,"arrival_portal_id":"from_main","return_portal_id":"to_main"}),
                ],
            ),
            (
                vec!["from_main", "from_third"],
                vec![
                    json!({"id":"to_main","destination_world_id":1,"arrival_portal_id":"from_cormoria","return_portal_id":"to_cormoria"}),
                    json!({"id":"to_third","destination_world_id":7,"arrival_portal_id":"from_cormoria","return_portal_id":"to_cormoria"}),
                ],
            ),
            (
                vec!["from_main", "from_cormoria"],
                vec![
                    json!({"id":"to_main","destination_world_id":1,"arrival_portal_id":"from_third","return_portal_id":"to_third"}),
                    json!({"id":"to_cormoria","destination_world_id":2,"arrival_portal_id":"from_third","return_portal_id":"to_third"}),
                ],
            ),
        ];
        for (world, (arrivals, portals)) in catalog["worlds"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .zip(routes)
        {
            world["arrivals"] = json!(arrivals);
            world["portals"] = json!(portals);
        }
        serde_json::to_vec(&catalog).unwrap()
    }

    #[test]
    fn pinned_three_world_portals_and_shared_descriptor_authorize_only_exact_edges() {
        let bytes = travel_fixture();
        let catalog =
            TrustedBuildCatalog::from_release_bytes(&bytes, Sha256Digest::of_bytes(&bytes))
                .expect("pinned travel catalog");
        let main = RomWorldId::new(1).unwrap();
        let cormoria = RomWorldId::new(2).unwrap();
        let third = RomWorldId::new(7).unwrap();
        let schema = Sha256Digest::of_bytes(b"shared-schema");
        assert_eq!(
            catalog.resolve_portal(main, "to_cormoria", schema),
            Some((cormoria, "from_main"))
        );
        assert_eq!(
            catalog.resolve_portal(cormoria, "to_third", schema),
            Some((third, "from_cormoria"))
        );
        assert_eq!(catalog.resolve_portal(main, "not_a_route", schema), None);
        assert_eq!(
            catalog.resolve_portal(main, "to_cormoria", Sha256Digest::of_bytes(b"other")),
            None
        );
        let legacy = fixture();
        let legacy =
            TrustedBuildCatalog::from_release_bytes(&legacy, Sha256Digest::of_bytes(&legacy))
                .unwrap();
        assert_eq!(legacy.resolve_portal(main, "to_cormoria", schema), None);
    }

    #[test]
    fn travel_catalog_rejects_broken_reciprocal_portals() {
        let mut tampered: Value = serde_json::from_slice(&travel_fixture()).unwrap();
        tampered["worlds"][1]["portals"][0]["return_portal_id"] = json!("wrong");
        let bytes = serde_json::to_vec(&tampered).unwrap();
        assert!(
            TrustedBuildCatalog::from_release_bytes(&bytes, Sha256Digest::of_bytes(&bytes))
                .is_err()
        );
    }

    #[test]
    fn generator_produced_three_world_catalog_loads_with_exact_portal_contract() {
        let bytes = include_bytes!("fixtures/travel-catalog-v3.json");
        let catalog = TrustedBuildCatalog::from_release_bytes(bytes, Sha256Digest::of_bytes(bytes))
            .expect("Python-generated pinned catalog");
        let json: Value = serde_json::from_slice(bytes).unwrap();
        let schema: Sha256Digest =
            serde_json::from_value(json["shared_player_descriptor_sha256"].clone()).unwrap();
        assert_eq!(
            Sha256Digest::of_bytes(catalog.transfer_descriptor().unwrap()),
            schema
        );
        assert_eq!(
            catalog.resolve_portal(RomWorldId::new(1).unwrap(), "to_next", schema),
            Some((RomWorldId::new(2).unwrap(), "from_previous"))
        );
        assert_eq!(
            catalog.resolve_portal(RomWorldId::new(1).unwrap(), "unknown", schema),
            None
        );
        let (path, digest, map) = catalog
            .arrival_template(RomWorldId::new(2).unwrap(), "from_previous")
            .expect("pinned first-arrival save");
        assert_eq!(path, "cormoria-from_previous.sav");
        assert_ne!(digest, Sha256Digest::of_bytes(b"wrong"));
        assert_eq!(map, (79, 1, 255));
    }

    #[test]
    fn arrival_loader_rejects_pinned_save_with_wrong_layout() {
        let mut catalog: Value =
            serde_json::from_slice(include_bytes!("fixtures/travel-catalog-v3.json")).unwrap();
        catalog["worlds"][1]["arrivals"][0]["map_layout_id"] = json!(1314);
        let bytes = serde_json::to_vec(&catalog).unwrap();
        let mut trusted =
            TrustedBuildCatalog::from_release_bytes(&bytes, Sha256Digest::of_bytes(&bytes))
                .expect("digest-pinned catalog");
        let root =
            std::env::temp_dir().join(format!("coop-arrival-layout-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let template = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../tools/tests/fixtures/arrival-v3-cormoria-carabrue.sav"),
        )
        .unwrap();
        for world in catalog["worlds"].as_array().unwrap() {
            for arrival in world["arrivals"].as_array().unwrap() {
                let name = arrival["template_sav_path"].as_str().unwrap();
                std::fs::write(root.join(name), &template).unwrap();
            }
        }
        let result = trusted.load_arrival_saves(&root);
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(
            result.unwrap_err(),
            "arrival save layout does not match catalog"
        );
    }

    #[test]
    fn release_catalog_rejects_unpinned_or_escaping_arrival_save() {
        let base: Value =
            serde_json::from_slice(include_bytes!("fixtures/travel-catalog-v3.json")).unwrap();
        for invalid in [
            json!("../other.sav"),
            json!("C:\\other.sav"),
            json!("/tmp/other.sav"),
        ] {
            let mut tampered = base.clone();
            tampered["worlds"][1]["arrivals"][0]["template_sav_path"] = invalid;
            let bytes = serde_json::to_vec(&tampered).unwrap();
            assert!(
                TrustedBuildCatalog::from_release_bytes(&bytes, Sha256Digest::of_bytes(&bytes))
                    .is_err()
            );
        }
        let mut tampered = base;
        tampered["worlds"][1]["arrivals"][0]
            .as_object_mut()
            .unwrap()
            .remove("template_sav_sha256");
        let bytes = serde_json::to_vec(&tampered).unwrap();
        assert!(
            TrustedBuildCatalog::from_release_bytes(&bytes, Sha256Digest::of_bytes(&bytes))
                .is_err()
        );
    }

    #[test]
    fn selects_three_worlds_by_authoritative_snapshot_identity() {
        let bytes = fixture();
        let catalog =
            TrustedBuildCatalog::from_release_bytes(&bytes, Sha256Digest::of_bytes(&bytes))
                .expect("pinned catalog");
        for (number, build) in [(1, "game-main"), (2, "game-cormoria"), (7, "game-third")] {
            let world = RomWorldId::new(number).unwrap();
            assert_eq!(
                catalog
                    .for_snapshot(Some(world), Some(world))
                    .unwrap()
                    .game_build_id
                    .value(),
                build
            );
        }
    }

    #[test]
    fn unknown_unbound_and_mismatched_worlds_fail_closed() {
        let bytes = fixture();
        let catalog =
            TrustedBuildCatalog::from_release_bytes(&bytes, Sha256Digest::of_bytes(&bytes))
                .expect("pinned catalog");
        let main = RomWorldId::new(1).unwrap();
        let cormoria = RomWorldId::new(2).unwrap();
        let unknown = RomWorldId::new(9).unwrap();
        assert!(catalog.for_snapshot(None, Some(main)).is_err());
        assert!(catalog.for_snapshot(Some(unknown), None).is_err());
        assert!(catalog.for_snapshot(Some(main), Some(cormoria)).is_err());
    }

    #[test]
    fn substituted_or_duplicate_catalog_is_rejected() {
        let bytes = fixture();
        let pinned = Sha256Digest::of_bytes(&bytes);
        let mut tampered: Value = serde_json::from_slice(&bytes).unwrap();
        tampered["worlds"][1]["build"]["rom_sha256"] =
            tampered["worlds"][0]["build"]["rom_sha256"].clone();
        let tampered = serde_json::to_vec(&tampered).unwrap();
        assert!(TrustedBuildCatalog::from_release_bytes(&tampered, pinned).is_err());
        assert!(
            TrustedBuildCatalog::from_release_bytes(&tampered, Sha256Digest::of_bytes(&tampered))
                .is_err()
        );
    }
}
