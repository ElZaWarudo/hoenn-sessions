//! Wire contracts for a future two-party, quiesced save trade.
//!
//! These contracts do not authorize snapshot publication. Both launchers must
//! first support an explicit save handoff before the server enables trades.

use serde::{Deserialize, Serialize};

use crate::{
    ApiVersion, CharacterId, GroupId, IdempotencyKey, LeaseFence, Revision, SnapshotId,
    TradeOfferId, UnixTimestampMillis,
};

/// A party position in the six-record saved party, zero based.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct PartyPosition(u8);

impl PartyPosition {
    /// Returns `None` for an index outside the six-record party.
    #[must_use]
    pub const fn new(index: u8) -> Option<Self> {
        if index < 6 { Some(Self(index)) } else { None }
    }

    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl TryFrom<u8> for PartyPosition {
    type Error = &'static str;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value).ok_or("party position must be 0..5")
    }
}

impl From<PartyPosition> for u8 {
    fn from(value: PartyPosition) -> Self {
        value.0
    }
}

/// The initiator consents to their own slot and requests one exact peer slot.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TradeOfferRequest {
    pub api_version: ApiVersion,
    pub fence: LeaseFence,
    pub group_id: GroupId,
    pub own_slot: PartyPosition,
    pub partner_slot: PartyPosition,
    pub partner_expected_revision: Revision,
    pub idempotency_key: IdempotencyKey,
}

/// The partner may consent to the exact anchored slots or decline.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TradeDecision {
    Accept,
    Reject,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TradeDecisionRequest {
    pub api_version: ApiVersion,
    pub fence: LeaseFence,
    pub offer_id: TradeOfferId,
    pub own_slot: PartyPosition,
    pub partner_slot: PartyPosition,
    pub decision: TradeDecision,
    pub idempotency_key: IdempotencyKey,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TradeOfferStatus {
    Pending,
    Accepted,
    Rejected,
    Expired,
}

/// Snapshot heads and revisions anchor exactly what each player consented to.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TradeOfferView {
    pub api_version: ApiVersion,
    pub offer_id: TradeOfferId,
    pub group_id: GroupId,
    pub members: [CharacterId; 2],
    pub initiator: CharacterId,
    pub slots: [PartyPosition; 2],
    pub revisions: [Revision; 2],
    pub snapshots: [SnapshotId; 2],
    pub status: TradeOfferStatus,
    pub expires_at: UnixTimestampMillis,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn party_positions_reject_non_party_values() {
        assert!(PartyPosition::new(5).is_some());
        assert!(PartyPosition::new(6).is_none());
        assert!(serde_json::from_str::<PartyPosition>("6").is_err());
    }
}
