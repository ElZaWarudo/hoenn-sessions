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

    /// Returns the stable region-qualified spelling used by identity APIs.
    #[must_use]
    pub fn qualified_key(&self) -> String {
        format!("{}:{}", self.region, self.map)
    }
}

include!("generated_map_catalog.rs");

/// The complete generated map catalog.
pub const MAP_CATALOG: &[MapCatalogEntry] = GENERATED_MAP_CATALOG;

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
    fn generated_catalog_has_complete_unique_coverage() {
        assert_eq!(MAP_CATALOG.len(), 935);

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
    }

    #[test]
    fn forward_and_reverse_resolution_are_exact() {
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
}
