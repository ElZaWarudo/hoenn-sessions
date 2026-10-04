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

/// Identifies one Pokémon by the two words the game never rewrites.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TradePokemonKey {
    pub personality: u32,
    pub ot_id: u32,
}

/// The initiator consents to their own slot.
///
/// A *strict* offer also names one exact peer slot and the peer's expected
/// revision; the partner can only accept those anchors. An *open* offer
/// (`partner_slot` and `partner_expected_revision` both absent) lets the
/// partner choose its own slot when it accepts: the partner's side is then
/// anchored to the accepting member's current head at decision time. Mixing
/// the two shapes is invalid.
///
/// `own_pokemon`, when present, must name the Pokémon in `own_slot` of the
/// caller's anchored head. The in-game trade UI always sends it, so a stale
/// head can never trade a different Pokémon than the one the player picked.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TradeOfferRequest {
    pub api_version: ApiVersion,
    pub fence: LeaseFence,
    pub group_id: GroupId,
    pub own_slot: PartyPosition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partner_slot: Option<PartyPosition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partner_expected_revision: Option<Revision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub own_pokemon: Option<TradePokemonKey>,
    pub idempotency_key: IdempotencyKey,
}

impl TradeOfferRequest {
    /// Whether the partner chooses its own slot at acceptance.
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.partner_slot.is_none() && self.partner_expected_revision.is_none()
    }

    /// Whether exactly one of the two strict partner anchors is present.
    #[must_use]
    pub const fn is_mixed(&self) -> bool {
        self.partner_slot.is_some() != self.partner_expected_revision.is_some()
    }
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
    /// For an accept, the Pokémon the accepting player picked; it must be in
    /// `own_slot` of the head the acceptance anchors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub own_pokemon: Option<TradePokemonKey>,
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
    /// An open offer: until acceptance the partner's slot is a placeholder
    /// and its revision and snapshot are the head read at creation.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub partner_chooses: bool,
}

/// What the partner sees of the initiator's offered Pokémon, read by the
/// server from the initiator's anchored head. `nickname` keeps the game's
/// own character encoding and `0xFF` padding.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TradeOfferedPokemon {
    pub species: u16,
    pub level: u8,
    pub is_egg: bool,
    pub nickname: [u8; 10],
}

/// `GET /v1/groups/{group_id}/trade-offers/current`: the group's pending
/// offer and the Pokémon it offers.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TradeOfferCurrentView {
    pub offer: TradeOfferView,
    pub offered: TradeOfferedPokemon,
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

    #[test]
    fn open_offers_omit_partner_anchors_and_mixed_shapes_are_detected() {
        let json = serde_json::json!({
            "api_version": 1,
            "fence": {
                "session_id": "00000000-0000-0000-0000-00000000000a",
                "character_id": "00000000-0000-0000-0000-00000000000b",
                "current_revision": 2,
                "session_epoch": 1,
                "client_instance_id": "00000000-0000-0000-0000-00000000000c"
            },
            "group_id": "00000000-0000-0000-0000-00000000000d",
            "own_slot": 1,
            "own_pokemon": {"personality": 7, "ot_id": 9},
            "idempotency_key": "00000000-0000-0000-0000-00000000000e"
        });
        let request: TradeOfferRequest = serde_json::from_value(json.clone()).unwrap();
        assert!(request.is_open());
        assert!(!request.is_mixed());
        assert_eq!(
            request.own_pokemon,
            Some(TradePokemonKey {
                personality: 7,
                ot_id: 9
            })
        );
        assert_eq!(serde_json::to_value(request).unwrap(), json);
        let mut mixed = request;
        mixed.partner_slot = PartyPosition::new(0);
        assert!(mixed.is_mixed());
        assert!(!mixed.is_open());
    }
}
