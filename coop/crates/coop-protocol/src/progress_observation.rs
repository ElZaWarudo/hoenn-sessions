//! Bounded, observational progress events. These never authorize a save change.

use serde::{Deserialize, Serialize};

use crate::RegionId;

pub const PROGRESS_OBSERVATION_PAYLOAD_SIZE: usize = 4;
pub const MAX_NATIONAL_DEX_NUMBER: u16 = 1025;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressKindV1 {
    BadgeEarned,
    FirstCaught,
    StoryMilestone,
}

impl ProgressKindV1 {
    const fn wire(self) -> u8 {
        match self {
            Self::BadgeEarned => 1,
            Self::FirstCaught => 2,
            Self::StoryMilestone => 3,
        }
    }

    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::BadgeEarned),
            2 => Some(Self::FirstCaught),
            3 => Some(Self::StoryMilestone),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgressObservationV1 {
    pub kind: ProgressKindV1,
    pub region_id: RegionId,
    pub subject_id: u16,
    pub session_epoch: u32,
    pub source_sequence: u32,
}

impl ProgressObservationV1 {
    pub fn decode_bridge(payload: &[u8], session_epoch: u32, source_sequence: u32) -> Option<Self> {
        if payload.len() != PROGRESS_OBSERVATION_PAYLOAD_SIZE {
            return None;
        }
        let observation = Self {
            kind: ProgressKindV1::from_wire(payload[0])?,
            region_id: RegionId::from_wire(payload[1]).ok()?,
            subject_id: u16::from_le_bytes([payload[2], payload[3]]),
            session_epoch,
            source_sequence,
        };
        observation.is_valid().then_some(observation)
    }

    #[must_use]
    pub fn encode_bridge(self) -> [u8; PROGRESS_OBSERVATION_PAYLOAD_SIZE] {
        let id = self.subject_id.to_le_bytes();
        [self.kind.wire(), self.region_id.wire(), id[0], id[1]]
    }

    #[must_use]
    pub fn is_valid(self) -> bool {
        self.session_epoch != 0
            && self.source_sequence != 0
            && match self.kind {
                ProgressKindV1::BadgeEarned => {
                    matches!(
                        self.region_id,
                        RegionId::Hoenn | RegionId::Kanto | RegionId::Johto
                    ) && self.subject_id < 8
                }
                ProgressKindV1::FirstCaught => {
                    self.region_id != RegionId::Unspecified
                        && (1..=MAX_NATIONAL_DEX_NUMBER).contains(&self.subject_id)
                }
                ProgressKindV1::StoryMilestone => {
                    matches!(
                        self.region_id,
                        RegionId::Hoenn | RegionId::Kanto | RegionId::Johto
                    ) && self.subject_id == 1
                }
            }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_round_trip_and_bounds() {
        let event = ProgressObservationV1 {
            kind: ProgressKindV1::BadgeEarned,
            region_id: RegionId::Johto,
            subject_id: 7,
            session_epoch: 4,
            source_sequence: 12,
        };
        assert_eq!(
            ProgressObservationV1::decode_bridge(&event.encode_bridge(), 4, 12),
            Some(event)
        );
        assert!(ProgressObservationV1::decode_bridge(&[1, 4, 0, 0], 4, 12).is_none());
        assert!(ProgressObservationV1::decode_bridge(&[2, 1, 0, 0], 4, 12).is_none());
        assert!(ProgressObservationV1::decode_bridge(&[1, 1, 8, 0], 4, 12).is_none());
        assert!(ProgressObservationV1::decode_bridge(&[2, 1, 1, 0], 0, 12).is_none());
        let champion = ProgressObservationV1 {
            kind: ProgressKindV1::StoryMilestone,
            region_id: RegionId::Hoenn,
            subject_id: 1,
            session_epoch: 4,
            source_sequence: 13,
        };
        assert_eq!(
            ProgressObservationV1::decode_bridge(&champion.encode_bridge(), 4, 13),
            Some(champion)
        );
        assert!(ProgressObservationV1::decode_bridge(&[3, 1, 2, 0], 4, 13).is_none());
    }
}
