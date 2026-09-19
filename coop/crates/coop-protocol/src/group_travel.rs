//! Strict fixed-size records for consented Johto group travel.

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const GROUP_TRAVEL_RECORD_SIZE: usize = 32;

#[repr(u8)]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupTravelRoute {
    TrainOriginal = 1,
    TrainLater = 2,
    FerryOriginal = 3,
    FerryLater = 4,
    GateOriginal = 5,
    GateLater = 6,
}

impl GroupTravelRoute {
    #[must_use]
    pub const fn era(self) -> GroupTravelEra {
        match self {
            Self::TrainOriginal | Self::FerryOriginal | Self::GateOriginal => {
                GroupTravelEra::Original
            }
            Self::TrainLater | Self::FerryLater | Self::GateLater => GroupTravelEra::Later,
        }
    }

    #[must_use]
    pub const fn destination(self) -> GroupTravelDestination {
        match self {
            Self::TrainOriginal => GroupTravelDestination::OriginalSaffron,
            Self::TrainLater => GroupTravelDestination::LaterSaffron,
            Self::FerryOriginal => GroupTravelDestination::OriginalVermilion,
            Self::FerryLater => GroupTravelDestination::LaterVermilion,
            Self::GateOriginal => GroupTravelDestination::OriginalRoute22,
            Self::GateLater => GroupTravelDestination::LaterRoute22,
        }
    }

    fn from_wire(value: u8) -> Result<Self, GroupTravelCodecError> {
        match value {
            1 => Ok(Self::TrainOriginal),
            2 => Ok(Self::TrainLater),
            3 => Ok(Self::FerryOriginal),
            4 => Ok(Self::FerryLater),
            5 => Ok(Self::GateOriginal),
            6 => Ok(Self::GateLater),
            value => Err(GroupTravelCodecError::InvalidRoute(value)),
        }
    }
}

/// The script-owned departure path that authorized a group-travel request.
///
/// Ferry routes intentionally have two distinct contexts: the normal Olivine
/// ferry and the S.S. Aqua maiden voyage. This value occupies byte six of the
/// fixed-size record so a responder never infers the journey from local flags.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupTravelDeparture {
    Train = 1,
    Ferry = 2,
    SsaquaMaiden = 3,
    Gate = 4,
}

impl GroupTravelDeparture {
    fn from_wire(value: u8) -> Result<Self, GroupTravelCodecError> {
        match value {
            1 => Ok(Self::Train),
            2 => Ok(Self::Ferry),
            3 => Ok(Self::SsaquaMaiden),
            4 => Ok(Self::Gate),
            value => Err(GroupTravelCodecError::InvalidDeparture(value)),
        }
    }

    #[must_use]
    pub const fn matches_route(self, route: GroupTravelRoute) -> bool {
        match self {
            Self::Train => matches!(
                route,
                GroupTravelRoute::TrainOriginal | GroupTravelRoute::TrainLater
            ),
            Self::Ferry | Self::SsaquaMaiden => matches!(
                route,
                GroupTravelRoute::FerryOriginal | GroupTravelRoute::FerryLater
            ),
            Self::Gate => matches!(
                route,
                GroupTravelRoute::GateOriginal | GroupTravelRoute::GateLater
            ),
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupTravelEra {
    Original = 1,
    Later = 2,
}

impl GroupTravelEra {
    fn from_wire(value: u8) -> Result<Self, GroupTravelCodecError> {
        match value {
            1 => Ok(Self::Original),
            2 => Ok(Self::Later),
            value => Err(GroupTravelCodecError::InvalidEra(value)),
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupTravelDestination {
    OriginalVermilion = 1,
    LaterVermilion = 2,
    OriginalSaffron = 3,
    LaterSaffron = 4,
    OriginalRoute22 = 5,
    LaterRoute22 = 6,
}

impl GroupTravelDestination {
    fn from_wire(value: u8) -> Result<Self, GroupTravelCodecError> {
        match value {
            1 => Ok(Self::OriginalVermilion),
            2 => Ok(Self::LaterVermilion),
            3 => Ok(Self::OriginalSaffron),
            4 => Ok(Self::LaterSaffron),
            5 => Ok(Self::OriginalRoute22),
            6 => Ok(Self::LaterRoute22),
            value => Err(GroupTravelCodecError::InvalidDestination(value)),
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupTravelResult {
    None = 0,
    Accepted = 1,
    Declined = 2,
    Applied = 3,
}

impl GroupTravelResult {
    fn from_wire(value: u8) -> Result<Self, GroupTravelCodecError> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::Accepted),
            2 => Ok(Self::Declined),
            3 => Ok(Self::Applied),
            value => Err(GroupTravelCodecError::InvalidResult(value)),
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupTravelReason {
    None = 0,
    ParticipantDeclined = 1,
    RequesterCanceled = 2,
    Conflict = 3,
    Unsafe = 4,
}

impl GroupTravelReason {
    fn from_wire(value: u8) -> Result<Self, GroupTravelCodecError> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::ParticipantDeclined),
            2 => Ok(Self::RequesterCanceled),
            3 => Ok(Self::Conflict),
            4 => Ok(Self::Unsafe),
            value => Err(GroupTravelCodecError::InvalidReason(value)),
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupTravelClientKind {
    Request = 1,
    Decision = 2,
    Cancel = 3,
    Applied = 4,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupTravelServerKind {
    Requesting = 1,
    Offer = 2,
    Commit = 3,
    Abort = 4,
    Complete = 5,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GroupTravelClientRecord {
    pub kind: GroupTravelClientKind,
    pub route: GroupTravelRoute,
    pub departure: GroupTravelDeparture,
    pub request_id: u32,
    pub proposal_id: [u8; 16],
    pub result: GroupTravelResult,
    pub reason: GroupTravelReason,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GroupTravelServerRecord {
    pub kind: GroupTravelServerKind,
    pub route: GroupTravelRoute,
    pub departure: GroupTravelDeparture,
    pub request_id: u32,
    pub proposal_id: [u8; 16],
    pub result: GroupTravelResult,
    pub reason: GroupTravelReason,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum GroupTravelCodecError {
    #[error("group-travel record must be exactly 32 bytes, received {0}")]
    InvalidLength(usize),
    #[error("invalid group-travel client kind {0}")]
    InvalidClientKind(u8),
    #[error("invalid group-travel server kind {0}")]
    InvalidServerKind(u8),
    #[error("invalid group-travel route {0}")]
    InvalidRoute(u8),
    #[error("invalid group-travel departure {0}")]
    InvalidDeparture(u8),
    #[error("invalid group-travel era {0}")]
    InvalidEra(u8),
    #[error("invalid group-travel destination {0}")]
    InvalidDestination(u8),
    #[error("invalid group-travel result {0}")]
    InvalidResult(u8),
    #[error("invalid group-travel reason {0}")]
    InvalidReason(u8),
    #[error("request id zero is reserved")]
    RequestIdZero,
    #[error("proposal id is invalid for this phase")]
    InvalidProposalId,
    #[error("route, era, and destination disagree")]
    RouteMismatch,
    #[error("result or reason is invalid for this phase")]
    InvalidOutcome,
    #[error("reserved group-travel byte {0} must be zero")]
    NonZeroPadding(usize),
}

type DecodedCommon = (
    GroupTravelRoute,
    GroupTravelDeparture,
    u32,
    [u8; 16],
    GroupTravelResult,
    GroupTravelReason,
);

fn decode_common(bytes: &[u8]) -> Result<DecodedCommon, GroupTravelCodecError> {
    if bytes.len() != GROUP_TRAVEL_RECORD_SIZE {
        return Err(GroupTravelCodecError::InvalidLength(bytes.len()));
    }
    for index in [7_usize, 28, 29, 30, 31] {
        if bytes[index] != 0 {
            return Err(GroupTravelCodecError::NonZeroPadding(index));
        }
    }
    let route = GroupTravelRoute::from_wire(bytes[1])?;
    let departure = GroupTravelDeparture::from_wire(bytes[6])?;
    if !departure.matches_route(route) {
        return Err(GroupTravelCodecError::RouteMismatch);
    }
    let era = GroupTravelEra::from_wire(bytes[2])?;
    let destination = GroupTravelDestination::from_wire(bytes[3])?;
    if era != route.era() || destination != route.destination() {
        return Err(GroupTravelCodecError::RouteMismatch);
    }
    let request_id = u32::from_le_bytes(bytes[8..12].try_into().expect("fixed range"));
    if request_id == 0 {
        return Err(GroupTravelCodecError::RequestIdZero);
    }
    let proposal_id = bytes[12..28].try_into().expect("fixed range");
    Ok((
        route,
        departure,
        request_id,
        proposal_id,
        GroupTravelResult::from_wire(bytes[4])?,
        GroupTravelReason::from_wire(bytes[5])?,
    ))
}

fn encode_common(
    kind: u8,
    route: GroupTravelRoute,
    departure: GroupTravelDeparture,
    request_id: u32,
    proposal_id: [u8; 16],
    result: GroupTravelResult,
    reason: GroupTravelReason,
) -> [u8; GROUP_TRAVEL_RECORD_SIZE] {
    let mut bytes = [0; GROUP_TRAVEL_RECORD_SIZE];
    bytes[0] = kind;
    bytes[1] = route as u8;
    bytes[6] = departure as u8;
    bytes[2] = route.era() as u8;
    bytes[3] = route.destination() as u8;
    bytes[4] = result as u8;
    bytes[5] = reason as u8;
    bytes[8..12].copy_from_slice(&request_id.to_le_bytes());
    bytes[12..28].copy_from_slice(&proposal_id);
    bytes
}

fn proposal_is_zero(value: &[u8; 16]) -> bool {
    value.iter().all(|byte| *byte == 0)
}

impl GroupTravelClientRecord {
    /// Encodes the canonical fixed-size client record.
    ///
    /// # Errors
    /// Returns an error when phase fields or identifiers are inconsistent.
    pub fn encode(self) -> Result<[u8; GROUP_TRAVEL_RECORD_SIZE], GroupTravelCodecError> {
        self.validate()?;
        Ok(encode_common(
            self.kind as u8,
            self.route,
            self.departure,
            self.request_id,
            self.proposal_id,
            self.result,
            self.reason,
        ))
    }
    /// Decodes and strictly validates a client record.
    ///
    /// # Errors
    /// Returns an error for size, enum, correlation, outcome, or padding violations.
    pub fn decode(bytes: &[u8]) -> Result<Self, GroupTravelCodecError> {
        let (route, departure, request_id, proposal_id, result, reason) = decode_common(bytes)?;
        let kind = match bytes[0] {
            1 => GroupTravelClientKind::Request,
            2 => GroupTravelClientKind::Decision,
            3 => GroupTravelClientKind::Cancel,
            4 => GroupTravelClientKind::Applied,
            value => return Err(GroupTravelCodecError::InvalidClientKind(value)),
        };
        let value = Self {
            kind,
            route,
            departure,
            request_id,
            proposal_id,
            result,
            reason,
        };
        value.validate()?;
        Ok(value)
    }
    fn validate(&self) -> Result<(), GroupTravelCodecError> {
        if self.request_id == 0 {
            return Err(GroupTravelCodecError::RequestIdZero);
        }
        let zero = proposal_is_zero(&self.proposal_id);
        let valid = match self.kind {
            GroupTravelClientKind::Request => {
                zero && self.result == GroupTravelResult::None
                    && self.reason == GroupTravelReason::None
            }
            GroupTravelClientKind::Decision => {
                !zero
                    && matches!(
                        self.result,
                        GroupTravelResult::Accepted | GroupTravelResult::Declined
                    )
                    && self.reason == GroupTravelReason::None
            }
            GroupTravelClientKind::Cancel => {
                self.result == GroupTravelResult::None
                    && self.reason == GroupTravelReason::RequesterCanceled
            }
            GroupTravelClientKind::Applied => {
                !zero
                    && self.result == GroupTravelResult::Applied
                    && self.reason == GroupTravelReason::None
            }
        };
        if valid {
            Ok(())
        } else if matches!(
            self.kind,
            GroupTravelClientKind::Decision | GroupTravelClientKind::Applied
        ) && zero
        {
            Err(GroupTravelCodecError::InvalidProposalId)
        } else {
            Err(GroupTravelCodecError::InvalidOutcome)
        }
    }
}

impl GroupTravelServerRecord {
    /// Encodes the canonical fixed-size server record.
    ///
    /// # Errors
    /// Returns an error when phase fields or identifiers are inconsistent.
    pub fn encode(self) -> Result<[u8; GROUP_TRAVEL_RECORD_SIZE], GroupTravelCodecError> {
        self.validate()?;
        Ok(encode_common(
            self.kind as u8,
            self.route,
            self.departure,
            self.request_id,
            self.proposal_id,
            self.result,
            self.reason,
        ))
    }
    /// Decodes and strictly validates a server record.
    ///
    /// # Errors
    /// Returns an error for size, enum, correlation, outcome, or padding violations.
    pub fn decode(bytes: &[u8]) -> Result<Self, GroupTravelCodecError> {
        let (route, departure, request_id, proposal_id, result, reason) = decode_common(bytes)?;
        let kind = match bytes[0] {
            1 => GroupTravelServerKind::Requesting,
            2 => GroupTravelServerKind::Offer,
            3 => GroupTravelServerKind::Commit,
            4 => GroupTravelServerKind::Abort,
            5 => GroupTravelServerKind::Complete,
            value => return Err(GroupTravelCodecError::InvalidServerKind(value)),
        };
        let value = Self {
            kind,
            route,
            departure,
            request_id,
            proposal_id,
            result,
            reason,
        };
        value.validate()?;
        Ok(value)
    }
    fn validate(&self) -> Result<(), GroupTravelCodecError> {
        if self.request_id == 0 {
            return Err(GroupTravelCodecError::RequestIdZero);
        }
        let zero = proposal_is_zero(&self.proposal_id);
        let valid = match self.kind {
            GroupTravelServerKind::Requesting => {
                zero && self.result == GroupTravelResult::None
                    && self.reason == GroupTravelReason::None
            }
            GroupTravelServerKind::Offer | GroupTravelServerKind::Commit => {
                !zero
                    && self.result == GroupTravelResult::None
                    && self.reason == GroupTravelReason::None
            }
            GroupTravelServerKind::Abort => {
                self.result == GroupTravelResult::None
                    && self.reason != GroupTravelReason::None
                    && (!zero
                        || matches!(
                            self.reason,
                            GroupTravelReason::Conflict | GroupTravelReason::Unsafe
                        ))
            }
            GroupTravelServerKind::Complete => {
                !zero
                    && self.result == GroupTravelResult::Applied
                    && self.reason == GroupTravelReason::None
            }
        };
        if valid {
            Ok(())
        } else if !matches!(
            self.kind,
            GroupTravelServerKind::Requesting | GroupTravelServerKind::Abort
        ) && zero
        {
            Err(GroupTravelCodecError::InvalidProposalId)
        } else {
            Err(GroupTravelCodecError::InvalidOutcome)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn golden_vectors_cover_all_six_destinations() {
        for (index, route) in [
            GroupTravelRoute::TrainOriginal,
            GroupTravelRoute::TrainLater,
            GroupTravelRoute::FerryOriginal,
            GroupTravelRoute::FerryLater,
            GroupTravelRoute::GateOriginal,
            GroupTravelRoute::GateLater,
        ]
        .into_iter()
        .enumerate()
        {
            let departure = match route {
                GroupTravelRoute::TrainOriginal | GroupTravelRoute::TrainLater => {
                    GroupTravelDeparture::Train
                }
                GroupTravelRoute::FerryOriginal | GroupTravelRoute::FerryLater => {
                    GroupTravelDeparture::Ferry
                }
                GroupTravelRoute::GateOriginal | GroupTravelRoute::GateLater => {
                    GroupTravelDeparture::Gate
                }
            };
            let record = GroupTravelClientRecord {
                kind: GroupTravelClientKind::Request,
                route,
                departure,
                request_id: u32::try_from(index).unwrap() + 1,
                proposal_id: [0; 16],
                result: GroupTravelResult::None,
                reason: GroupTravelReason::None,
            };
            let encoded = record.encode().unwrap();
            assert_eq!(encoded[1], u8::try_from(index).unwrap() + 1);
            assert_eq!(GroupTravelClientRecord::decode(&encoded).unwrap(), record);
        }
    }
    #[test]
    fn rejects_direction_kind_padding_and_mismatch() {
        let record = GroupTravelServerRecord {
            kind: GroupTravelServerKind::Commit,
            route: GroupTravelRoute::GateLater,
            departure: GroupTravelDeparture::Gate,
            request_id: 7,
            proposal_id: [9; 16],
            result: GroupTravelResult::None,
            reason: GroupTravelReason::None,
        };
        let mut encoded = record.encode().unwrap();
        assert!(GroupTravelClientRecord::decode(&encoded).is_err());
        encoded[28] = 1;
        assert_eq!(
            GroupTravelServerRecord::decode(&encoded),
            Err(GroupTravelCodecError::NonZeroPadding(28))
        );
        encoded[28] = 0;
        encoded[6] = GroupTravelDeparture::Train as u8;
        assert_eq!(
            GroupTravelServerRecord::decode(&encoded),
            Err(GroupTravelCodecError::RouteMismatch)
        );
        encoded[6] = GroupTravelDeparture::Gate as u8;
        encoded[2] = 1;
        assert_eq!(
            GroupTravelServerRecord::decode(&encoded),
            Err(GroupTravelCodecError::RouteMismatch)
        );
    }
}
