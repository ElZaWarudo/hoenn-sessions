//! Stable identity of a ROM in a release family.
//!
//! This is deliberately separate from [`crate::RegionId`]: one ROM may contain
//! several gameplay regions, and a region ordinal cannot select an artifact or
//! a regional save image. The trusted release catalog resolves this ID.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// Nonzero, release-catalog-owned identity for one ROM world.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RomWorldId(u16);

/// A portal or catalog supplied an invalid ROM world identity.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("ROM world ID must be nonzero")]
pub struct InvalidRomWorldId;

impl RomWorldId {
    /// Creates an ID without inferring a gameplay region or ROM path.
    ///
    /// # Errors
    /// Returns an error for the reserved zero value.
    pub const fn new(value: u16) -> Result<Self, InvalidRomWorldId> {
        if value == 0 {
            Err(InvalidRomWorldId)
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the stable numeric ID used by catalogs and travel messages.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl Serialize for RomWorldId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u16(self.0)
    }
}

impl<'de> Deserialize<'de> for RomWorldId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(u16::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::RomWorldId;

    #[test]
    fn world_ids_are_numeric_and_reject_zero() {
        let third = RomWorldId::new(7).unwrap();
        assert_eq!(third.get(), 7);
        assert_eq!(serde_json::to_string(&third).unwrap(), "7");
        assert_eq!(serde_json::from_str::<RomWorldId>("7").unwrap(), third);
        assert!(RomWorldId::new(0).is_err());
        assert!(serde_json::from_str::<RomWorldId>("0").is_err());
        assert!(serde_json::from_str::<RomWorldId>("\"CORMORIA\"").is_err());
    }
}
