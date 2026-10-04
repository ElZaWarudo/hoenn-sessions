//! The authoritative region-qualified map catalog.
//!
//! Map coordinates are generated from `data/maps/map_groups.json`; the
//! checked-in table is deliberately the only runtime source used by the host
//! protocol. Keeping lookup in this module makes both directions exact and
//! gives callers a typed failure for a nonexistent or cross-region pair.

use super::{ProtocolError, RegionId};

/// One map's canonical host identity and engine coordinates.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MapCatalogEntry {
    /// The co-op region after applying the engine section authority.
    pub region: RegionId,
    /// The uppercase map ID with the engine `MAP_` prefix removed.
    pub map: &'static str,
    /// The numeric map-group coordinate used by the ROM bridge.
    pub map_group: u16,
    /// The numeric map-number coordinate used by the ROM bridge.
    pub map_number: u16,
    /// The map layout width in walkable tile coordinates.
    pub width: u16,
    /// The map layout height in walkable tile coordinates.
    pub height: u16,
    /// Whether the engine permits Dig/Escape Rope on this map.
    pub allow_escaping: bool,
    /// Start index of this map's generated vanilla escape endpoints.
    pub escape_targets_start: u16,
    /// Number of generated vanilla escape endpoints for this map.
    pub escape_targets_len: u16,
}

/// One vanilla escape endpoint inherited from an outdoor map warp.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MapEscapeTarget {
    pub map_group: u8,
    pub map_number: u8,
    pub x: u8,
    pub y: u8,
}

impl MapCatalogEntry {
    /// Returns the canonical local map key.
    #[must_use]
    pub const fn map_key(&self) -> &'static str {
        self.map
    }

    /// Returns the canonical map key without the `MAP_` engine prefix.
    #[must_use]
    pub const fn canonical_key(&self) -> &'static str {
        self.map
    }

    /// Returns the numeric map coordinates as `(map_group, map_number)`.
    #[must_use]
    pub const fn coordinates(&self) -> (u16, u16) {
        (self.map_group, self.map_number)
    }

    /// Returns the generated dimensions as `(width, height)`.
    #[must_use]
    pub const fn dimensions(&self) -> (u16, u16) {
        (self.width, self.height)
    }

    /// Returns the generated vanilla escape endpoints for this map.
    #[must_use]
    pub fn escape_targets(&self) -> &'static [MapEscapeTarget] {
        let start = usize::from(self.escape_targets_start);
        let end = start + usize::from(self.escape_targets_len);
        &GENERATED_MAP_ESCAPE_TARGETS[start..end]
    }

    /// Returns the stable region-qualified spelling used by identity APIs.
    #[must_use]
    pub fn qualified_key(&self) -> String {
        format!("{}:{}", self.region, self.map)
    }
}

/// A cardinal map edge from the ROM's authoritative map headers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MapConnectionEntry {
    pub from_group: u16,
    pub from_number: u16,
    pub to_group: u16,
    pub to_number: u16,
}

include!("generated_map_catalog.rs");
include!("generated_map_connections.rs");

/// The complete generated map catalog.
pub const MAP_CATALOG: &[MapCatalogEntry] = GENERATED_MAP_CATALOG;

/// Exact cardinal map edges. Dive, emerge, and warps are excluded.
pub const MAP_CONNECTIONS: &[MapConnectionEntry] = GENERATED_MAP_CONNECTIONS;

/// Whether the current map header names the other map as a cardinal neighbor.
#[must_use]
pub fn maps_share_edge(from_group: u16, from_number: u16, to_group: u16, to_number: u16) -> bool {
    MAP_CONNECTIONS.iter().any(|connection| {
        connection.from_group == from_group
            && connection.from_number == from_number
            && connection.to_group == to_group
            && connection.to_number == to_number
    })
}

/// A zero-sized access façade for callers that prefer an object-like API.
#[derive(Clone, Copy, Debug, Default)]
pub struct MapCatalog;

impl MapCatalog {
    /// Returns every generated map in deterministic map-group order.
    #[must_use]
    pub const fn all() -> &'static [MapCatalogEntry] {
        MAP_CATALOG
    }

    /// Resolves an exact `(region, canonical map key)` pair.
    ///
    /// # Errors
    ///
    /// Returns [`ProtocolError::MapRegionMismatch`] when the key belongs to a
    /// different region, or [`ProtocolError::UnknownMap`] when it is absent.
    pub fn resolve(region: RegionId, map: &str) -> Result<&'static MapCatalogEntry, ProtocolError> {
        resolve_map(region, map)
    }

    /// Resolves an exact `(region, map_group, map_number)` triple.
    ///
    /// # Errors
    ///
    /// Returns [`ProtocolError::MapCoordinateRegionMismatch`] when the
    /// coordinates belong to a different region, or
    /// [`ProtocolError::UnknownMapCoordinates`] when they are absent.
    pub fn resolve_coordinates(
        region: RegionId,
        map_group: u16,
        map_number: u16,
    ) -> Result<&'static MapCatalogEntry, ProtocolError> {
        resolve_map_coordinates(region, map_group, map_number)
    }
}

/// Returns the complete generated map catalog.
#[must_use]
pub const fn all_maps() -> &'static [MapCatalogEntry] {
    MAP_CATALOG
}

/// Resolves an exact region-qualified canonical map key.
///
/// # Errors
///
/// Returns a typed error when the region is unspecified, the map key is not
/// canonical, the key belongs to another region, or the map is absent.
pub fn resolve_map(region: RegionId, map: &str) -> Result<&'static MapCatalogEntry, ProtocolError> {
    let region = region.ensure_concrete()?;
    super::validate_map_key(map)?;
    if let Some(entry) = MAP_CATALOG
        .iter()
        .find(|entry| entry.region == region && entry.map == map)
    {
        return Ok(entry);
    }

    if let Some(entry) = MAP_CATALOG.iter().find(|entry| entry.map == map) {
        return Err(ProtocolError::MapRegionMismatch {
            map: map.to_owned(),
            expected: region,
            actual: entry.region,
        });
    }
    Err(ProtocolError::UnknownMap {
        region,
        map: map.to_owned(),
    })
}

/// Resolves an exact region-qualified numeric map coordinate.
///
/// # Errors
///
/// Returns a typed error when the region is unspecified, the coordinates
/// belong to another region, or the map is absent.
pub fn resolve_map_coordinates(
    region: RegionId,
    map_group: u16,
    map_number: u16,
) -> Result<&'static MapCatalogEntry, ProtocolError> {
    let region = region.ensure_concrete()?;
    if let Some(entry) = MAP_CATALOG.iter().find(|entry| {
        entry.region == region && entry.map_group == map_group && entry.map_number == map_number
    }) {
        return Ok(entry);
    }

    if let Some(entry) = MAP_CATALOG
        .iter()
        .find(|entry| entry.map_group == map_group && entry.map_number == map_number)
    {
        return Err(ProtocolError::MapCoordinateRegionMismatch {
            map_group,
            map_number,
            expected: region,
            actual: entry.region,
        });
    }
    Err(ProtocolError::UnknownMapCoordinates {
        region,
        map_group,
        map_number,
    })
}

/// Resolve a map's region from its globally unique numeric coordinates.
/// Returns `None` if the map is absent or the generated catalog is ambiguous.
#[must_use]
pub fn resolve_unique_map_coordinates(
    map_group: u16,
    map_number: u16,
) -> Option<&'static MapCatalogEntry> {
    let mut matches = MAP_CATALOG.iter().filter(|entry| {
        entry.map_group == map_group && entry.map_number == map_number
    });
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

/// Alias for reverse lookup by numeric map coordinates.
///
/// # Errors
///
/// Propagates the errors from [`resolve_map_coordinates`].
pub fn resolve_map_by_coordinates(
    region: RegionId,
    map_group: u16,
    map_number: u16,
) -> Result<&'static MapCatalogEntry, ProtocolError> {
    resolve_map_coordinates(region, map_group, map_number)
}

/// Resolves an exact map key and returns its numeric coordinates.
///
/// # Errors
///
/// Propagates the errors from [`resolve_map`].
pub fn coordinates_for_map(region: RegionId, map: &str) -> Result<(u16, u16), ProtocolError> {
    Ok(resolve_map(region, map)?.coordinates())
}

/// Resolves an exact numeric coordinate and returns its canonical map key.
///
/// # Errors
///
/// Propagates the errors from [`resolve_map_coordinates`].
pub fn map_key_for_coordinates(
    region: RegionId,
    map_group: u16,
    map_number: u16,
) -> Result<&'static str, ProtocolError> {
    Ok(resolve_map_coordinates(region, map_group, map_number)?.map)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn cardinal_connections_resolve_to_same_region_maps() {
        assert!(maps_share_edge(0, 19, 0, 20)); // Route 104 to Route 105.
        assert!(!maps_share_edge(0, 19, 0, 9)); // Littleroot is not adjacent.
        for connection in MAP_CONNECTIONS {
            let source = MAP_CATALOG
                .iter()
                .find(|entry| {
                    entry.coordinates() == (connection.from_group, connection.from_number)
                })
                .unwrap();
            let target = MAP_CATALOG
                .iter()
                .find(|entry| entry.coordinates() == (connection.to_group, connection.to_number))
                .unwrap();
            assert_eq!(source.region, target.region);
        }
    }

    #[test]
    fn generated_catalog_has_complete_unique_coverage() {
        assert_eq!(MAP_CATALOG.len(), 1509);

        let keys: HashSet<_> = MAP_CATALOG
            .iter()
            .map(|entry| (entry.region, entry.map))
            .collect();
        let coordinates: HashSet<_> = MAP_CATALOG
            .iter()
            .map(|entry| (entry.map_group, entry.map_number))
            .collect();
        assert_eq!(keys.len(), MAP_CATALOG.len());
        assert_eq!(coordinates.len(), MAP_CATALOG.len());
        assert!(
            MAP_CATALOG
                .iter()
                .any(|entry| entry.region == RegionId::Hoenn)
        );
        assert!(
            MAP_CATALOG
                .iter()
                .any(|entry| entry.region == RegionId::Kanto)
        );
        assert!(
            MAP_CATALOG
                .iter()
                .any(|entry| entry.region == RegionId::Sevii)
        );
        let later_kanto: Vec<_> = MAP_CATALOG
            .iter()
            .filter(|entry| entry.map.starts_with("KANTO_LATER_"))
            .collect();
        assert_eq!(later_kanto.len(), 168);
        assert!(
            later_kanto
                .iter()
                .all(|entry| entry.region == RegionId::Kanto)
        );
        assert!(
            MAP_CATALOG
                .iter()
                .any(|entry| { entry.region == RegionId::Johto && entry.map == "NEW_BARK_TOWN" })
        );
        assert!(
            MAP_CATALOG.iter().any(|entry| {
                entry.region == RegionId::Cormoria
                    && entry.map == "CORMORIA_RIVETSHORE_CITY_HARBOR"
            })
        );
    }

    #[test]
    fn forward_and_reverse_resolution_are_exact() {
        let johto = resolve_map(RegionId::Johto, "NEW_BARK_TOWN").unwrap();
        assert_eq!(
            map_key_for_coordinates(RegionId::Johto, johto.map_group, johto.map_number).unwrap(),
            "NEW_BARK_TOWN"
        );
        let hoenn = resolve_map(RegionId::Hoenn, "LITTLEROOT_TOWN").unwrap();
        assert_eq!(hoenn.coordinates(), (0, 9));
        assert_eq!(
            resolve_map_coordinates(RegionId::Hoenn, 0, 9).unwrap(),
            hoenn
        );

        let kanto = resolve_map(RegionId::Kanto, "PALLET_TOWN").unwrap();
        assert_eq!(kanto.coordinates(), (37, 0));
        assert_eq!(
            map_key_for_coordinates(RegionId::Kanto, 37, 0).unwrap(),
            "PALLET_TOWN"
        );

        let sevii = resolve_map(RegionId::Sevii, "ONE_ISLAND").unwrap();
        assert_eq!(sevii.coordinates(), (37, 12));
        let cormoria = resolve_map(RegionId::Cormoria, "CORMORIA_RIVETSHORE_CITY_HARBOR")
            .unwrap();
        assert_eq!(cormoria.coordinates(), (82, 21));
        assert_eq!(
            resolve_map_coordinates(RegionId::Cormoria, 82, 21).unwrap(),
            cormoria
        );
        assert!(matches!(
            resolve_map(RegionId::Kanto, "LITTLEROOT_TOWN"),
            Err(ProtocolError::MapRegionMismatch { .. })
        ));
        assert!(matches!(
            resolve_map(RegionId::Hoenn, "NOT_A_MAP"),
            Err(ProtocolError::UnknownMap { .. })
        ));
        assert!(matches!(
            resolve_map_coordinates(RegionId::Kanto, 0, 9),
            Err(ProtocolError::MapCoordinateRegionMismatch { .. })
        ));
        assert!(matches!(
            resolve_map_coordinates(RegionId::Hoenn, u16::MAX, u16::MAX),
            Err(ProtocolError::UnknownMapCoordinates { .. })
        ));
    }

    #[test]
    fn geographic_kanto_and_later_kanto_keep_wire_compatible_region_ids() {
        let route_26 = resolve_map(RegionId::Kanto, "ROUTE26").unwrap();
        assert_eq!(route_26.region, RegionId::Kanto);
        assert!(matches!(
            resolve_map(RegionId::Johto, "ROUTE26"),
            Err(ProtocolError::MapRegionMismatch { .. })
        ));

        let later_vermilion = resolve_map(RegionId::Kanto, "KANTO_LATER_VERMILION_CITY").unwrap();
        assert_eq!(later_vermilion.region, RegionId::Kanto);
        assert_eq!(
            resolve_map_coordinates(
                RegionId::Kanto,
                later_vermilion.map_group,
                later_vermilion.map_number
            )
            .unwrap(),
            later_vermilion
        );
    }
}
