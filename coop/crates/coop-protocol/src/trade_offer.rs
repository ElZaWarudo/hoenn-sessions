//! In-game trade offer bridge records (game protocol 4).
//!
//! Four single-frame messages carry the consent part of a trade between the
//! ROM and the launcher; the trade itself still arrives as the ledger's
//! `TradeCommit` (0x0119). Every multi-byte integer is little endian and every
//! reserved byte must be zero.
//!
//! | type   | direction     | record                         | bytes |
//! |-------:|---------------|--------------------------------|------:|
//! | 0x0016 | ROM → sidecar | [`TradeOfferRequestRecord`]    |    16 |
//! | 0x0017 | ROM → sidecar | [`TradeOfferDecisionRecord`]   |    16 |
//! | 0x011A | sidecar → ROM | [`TradeOfferReceivedRecord`]   |    20 |
//! | 0x011B | sidecar → ROM | [`TradeOfferStatusRecord`]     |    12 |

use serde::{Deserialize, Serialize};

use crate::BattleBridgeError;

const PARTY_SIZE: u8 = 6;

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("checked length"),
    )
}

/// What the requester's ROM asks for.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TradeOfferAction {
    /// Offer the Pokémon in `slot` to the partner.
    Offer = 1,
    /// Withdraw the offer made under `request_id`.
    Cancel = 2,
}

/// `TradeOfferRequest` (0x0016, ROM to sidecar), 16 bytes:
///
/// | offset | size | field                                              |
/// |-------:|-----:|----------------------------------------------------|
/// |      0 |    1 | `action`: 1 offer, 2 cancel                         |
/// |      1 |    1 | `slot`, party slot 0..=5 (zero for cancel)          |
/// |      2 |    2 | reserved, zero                                      |
/// |      4 |    4 | `request_id`, nonzero, chosen by the ROM            |
/// |      8 |    4 | `personality` of the offered Pokémon (zero: cancel) |
/// |     12 |    4 | `ot_id` of the offered Pokémon (zero: cancel)       |
///
/// The ROM sends an offer only after its own checkpoint, so the server's
/// snapshot head holds the live party the player picked from.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TradeOfferRequestRecord {
    pub action: TradeOfferAction,
    pub slot: u8,
    pub request_id: u32,
    pub personality: u32,
    pub ot_id: u32,
}

impl TradeOfferRequestRecord {
    pub const WIRE_SIZE: usize = 16;

    fn validate(&self) -> Result<(), BattleBridgeError> {
        let valid = self.request_id != 0
            && match self.action {
                TradeOfferAction::Offer => self.slot < PARTY_SIZE,
                TradeOfferAction::Cancel => {
                    self.slot == 0 && self.personality == 0 && self.ot_id == 0
                }
            };
        if valid {
            Ok(())
        } else {
            Err(BattleBridgeError::Value)
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>, BattleBridgeError> {
        self.validate()?;
        let mut bytes = Vec::with_capacity(Self::WIRE_SIZE);
        bytes.extend_from_slice(&[self.action as u8, self.slot, 0, 0]);
        bytes.extend_from_slice(&self.request_id.to_le_bytes());
        bytes.extend_from_slice(&self.personality.to_le_bytes());
        bytes.extend_from_slice(&self.ot_id.to_le_bytes());
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, BattleBridgeError> {
        if bytes.len() != Self::WIRE_SIZE {
            return Err(BattleBridgeError::Length);
        }
        let action = match bytes[0] {
            1 => TradeOfferAction::Offer,
            2 => TradeOfferAction::Cancel,
            _ => return Err(BattleBridgeError::Value),
        };
        if bytes[2..4] != [0, 0] {
            return Err(BattleBridgeError::Value);
        }
        let result = Self {
            action,
            slot: bytes[1],
            request_id: u32_at(bytes, 4),
            personality: u32_at(bytes, 8),
            ot_id: u32_at(bytes, 12),
        };
        result.validate()?;
        Ok(result)
    }
}

/// The partner's answer to a received offer.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TradeOfferDecision {
    Accept = 1,
    Decline = 2,
}

/// `TradeOfferDecision` (0x0017, ROM to sidecar), 16 bytes:
///
/// | offset | size | field                                                  |
/// |-------:|-----:|--------------------------------------------------------|
/// |      0 |    1 | `decision`: 1 accept, 2 decline                         |
/// |      1 |    1 | `slot`, the partner's party slot 0..=5 (zero: decline)  |
/// |      2 |    2 | reserved, zero                                          |
/// |      4 |    4 | `offer_token` from `TradeOfferReceived`, nonzero        |
/// |      8 |    4 | `personality` of the Pokémon given (zero: decline)      |
/// |     12 |    4 | `ot_id` of the Pokémon given (zero: decline)            |
///
/// An accept follows the partner's own checkpoint, for the same reason as
/// the offer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TradeOfferDecisionRecord {
    pub decision: TradeOfferDecision,
    pub slot: u8,
    pub offer_token: u32,
    pub personality: u32,
    pub ot_id: u32,
}

impl TradeOfferDecisionRecord {
    pub const WIRE_SIZE: usize = 16;

    fn validate(&self) -> Result<(), BattleBridgeError> {
        let valid = self.offer_token != 0
            && match self.decision {
                TradeOfferDecision::Accept => self.slot < PARTY_SIZE,
                TradeOfferDecision::Decline => {
                    self.slot == 0 && self.personality == 0 && self.ot_id == 0
                }
            };
        if valid {
            Ok(())
        } else {
            Err(BattleBridgeError::Value)
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>, BattleBridgeError> {
        self.validate()?;
        let mut bytes = Vec::with_capacity(Self::WIRE_SIZE);
        bytes.extend_from_slice(&[self.decision as u8, self.slot, 0, 0]);
        bytes.extend_from_slice(&self.offer_token.to_le_bytes());
        bytes.extend_from_slice(&self.personality.to_le_bytes());
        bytes.extend_from_slice(&self.ot_id.to_le_bytes());
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, BattleBridgeError> {
        if bytes.len() != Self::WIRE_SIZE {
            return Err(BattleBridgeError::Length);
        }
        let decision = match bytes[0] {
            1 => TradeOfferDecision::Accept,
            2 => TradeOfferDecision::Decline,
            _ => return Err(BattleBridgeError::Value),
        };
        if bytes[2..4] != [0, 0] {
            return Err(BattleBridgeError::Value);
        }
        let result = Self {
            decision,
            slot: bytes[1],
            offer_token: u32_at(bytes, 4),
            personality: u32_at(bytes, 8),
            ot_id: u32_at(bytes, 12),
        };
        result.validate()?;
        Ok(result)
    }
}

/// `TradeOfferReceived` (0x011A, sidecar to ROM), 20 bytes:
///
/// | offset | size | field                                                  |
/// |-------:|-----:|--------------------------------------------------------|
/// |      0 |    4 | `offer_token`, nonzero, names the offer in the decision |
/// |      4 |    2 | `species` of the offered Pokémon, nonzero               |
/// |      6 |    1 | `level`, 0..=100                                        |
/// |      7 |    1 | flags: bit 0 egg; other bits zero                       |
/// |      8 |   10 | `nickname`, the game's encoding, `0xFF` padded          |
/// |     18 |    2 | reserved, zero                                          |
///
/// The summary is read by the server from the initiator's checkpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TradeOfferReceivedRecord {
    pub offer_token: u32,
    pub species: u16,
    pub level: u8,
    pub is_egg: bool,
    pub nickname: [u8; 10],
}

impl TradeOfferReceivedRecord {
    pub const WIRE_SIZE: usize = 20;
    const EGG_FLAG: u8 = 1;

    fn validate(&self) -> Result<(), BattleBridgeError> {
        if self.offer_token == 0 || self.species == 0 || self.level > 100 {
            return Err(BattleBridgeError::Value);
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<Vec<u8>, BattleBridgeError> {
        self.validate()?;
        let mut bytes = Vec::with_capacity(Self::WIRE_SIZE);
        bytes.extend_from_slice(&self.offer_token.to_le_bytes());
        bytes.extend_from_slice(&self.species.to_le_bytes());
        bytes.push(self.level);
        bytes.push(if self.is_egg { Self::EGG_FLAG } else { 0 });
        bytes.extend_from_slice(&self.nickname);
        bytes.extend_from_slice(&[0, 0]);
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, BattleBridgeError> {
        if bytes.len() != Self::WIRE_SIZE {
            return Err(BattleBridgeError::Length);
        }
        if bytes[7] & !Self::EGG_FLAG != 0 || bytes[18..20] != [0, 0] {
            return Err(BattleBridgeError::Value);
        }
        let result = Self {
            offer_token: u32_at(bytes, 0),
            species: u16::from_le_bytes([bytes[4], bytes[5]]),
            level: bytes[6],
            is_egg: bytes[7] & Self::EGG_FLAG != 0,
            nickname: bytes[8..18].try_into().expect("checked length"),
        };
        result.validate()?;
        Ok(result)
    }
}

/// Which side of the offer a status describes.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TradeOfferRole {
    /// The ROM that sent the offer; the status names its `request_id`.
    Requester = 1,
    /// The ROM that received the offer; the status names its `offer_token`.
    Responder = 2,
}

/// Where an offer stands, or why it could not proceed.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TradeOfferOutcome {
    /// Requester only: the server holds the offer; waiting for the partner.
    Pending = 1,
    /// Both members consented; the ledger will deliver `TradeCommit`.
    Accepted = 2,
    /// The partner declined.
    Declined = 3,
    /// The offer's window lapsed.
    Expired = 4,
    /// The requester withdrew the offer.
    Cancelled = 5,
    /// The cloud could not be reached; nothing changed.
    Unavailable = 6,
    /// No group, or the partner is not online.
    PartnerUnavailable = 7,
    /// An offered Pokémon holds mail.
    Mail = 8,
    /// Another offer or an unapplied trade is in the way.
    Busy = 9,
    /// The checkpoint did not hold the picked Pokémon; nothing changed.
    Stale = 10,
}

impl TradeOfferOutcome {
    fn from_wire(value: u8) -> Result<Self, BattleBridgeError> {
        Ok(match value {
            1 => Self::Pending,
            2 => Self::Accepted,
            3 => Self::Declined,
            4 => Self::Expired,
            5 => Self::Cancelled,
            6 => Self::Unavailable,
            7 => Self::PartnerUnavailable,
            8 => Self::Mail,
            9 => Self::Busy,
            10 => Self::Stale,
            _ => return Err(BattleBridgeError::Value),
        })
    }
}

/// `TradeOfferStatus` (0x011B, sidecar to ROM), 12 bytes:
///
/// | offset | size | field                                                  |
/// |-------:|-----:|--------------------------------------------------------|
/// |      0 |    1 | `role`: 1 requester, 2 responder                        |
/// |      1 |    1 | `outcome` ([`TradeOfferOutcome`])                       |
/// |      2 |    2 | reserved, zero                                          |
/// |      4 |    4 | `request_id` (requester, nonzero; responder, zero)      |
/// |      8 |    4 | `offer_token` (zero until the server holds an offer)    |
///
/// A responder status always names its nonzero offer token and is never
/// `Pending`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TradeOfferStatusRecord {
    pub role: TradeOfferRole,
    pub outcome: TradeOfferOutcome,
    pub request_id: u32,
    pub offer_token: u32,
}

impl TradeOfferStatusRecord {
    pub const WIRE_SIZE: usize = 12;

    fn validate(&self) -> Result<(), BattleBridgeError> {
        let valid = match self.role {
            TradeOfferRole::Requester => {
                self.request_id != 0
                    && (self.outcome != TradeOfferOutcome::Pending || self.offer_token != 0)
            }
            TradeOfferRole::Responder => {
                self.request_id == 0
                    && self.offer_token != 0
                    && self.outcome != TradeOfferOutcome::Pending
            }
        };
        if valid {
            Ok(())
        } else {
            Err(BattleBridgeError::Value)
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>, BattleBridgeError> {
        self.validate()?;
        let mut bytes = Vec::with_capacity(Self::WIRE_SIZE);
        bytes.extend_from_slice(&[self.role as u8, self.outcome as u8, 0, 0]);
        bytes.extend_from_slice(&self.request_id.to_le_bytes());
        bytes.extend_from_slice(&self.offer_token.to_le_bytes());
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, BattleBridgeError> {
        if bytes.len() != Self::WIRE_SIZE {
            return Err(BattleBridgeError::Length);
        }
        let role = match bytes[0] {
            1 => TradeOfferRole::Requester,
            2 => TradeOfferRole::Responder,
            _ => return Err(BattleBridgeError::Value),
        };
        if bytes[2..4] != [0, 0] {
            return Err(BattleBridgeError::Value);
        }
        let result = Self {
            role,
            outcome: TradeOfferOutcome::from_wire(bytes[1])?,
            request_id: u32_at(bytes, 4),
            offer_token: u32_at(bytes, 8),
        };
        result.validate()?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_layout_and_cancel_rules() {
        let offer = TradeOfferRequestRecord {
            action: TradeOfferAction::Offer,
            slot: 2,
            request_id: 0x0102_0304,
            personality: 0xAABB_CCDD,
            ot_id: 0x1122_3344,
        };
        let bytes = offer.encode().unwrap();
        assert_eq!(
            bytes,
            [
                1, 2, 0, 0, 4, 3, 2, 1, 0xDD, 0xCC, 0xBB, 0xAA, 0x44, 0x33, 0x22, 0x11
            ]
        );
        assert_eq!(TradeOfferRequestRecord::decode(&bytes), Ok(offer));
        let cancel = TradeOfferRequestRecord {
            action: TradeOfferAction::Cancel,
            slot: 0,
            request_id: 9,
            personality: 0,
            ot_id: 0,
        };
        assert_eq!(
            TradeOfferRequestRecord::decode(&cancel.encode().unwrap()),
            Ok(cancel)
        );
        for bad in [
            TradeOfferRequestRecord { slot: 6, ..offer },
            TradeOfferRequestRecord {
                request_id: 0,
                ..offer
            },
            TradeOfferRequestRecord { slot: 1, ..cancel },
            TradeOfferRequestRecord {
                personality: 1,
                ..cancel
            },
        ] {
            assert_eq!(bad.encode(), Err(BattleBridgeError::Value));
        }
        let mut reserved = bytes.clone();
        reserved[3] = 1;
        assert_eq!(
            TradeOfferRequestRecord::decode(&reserved),
            Err(BattleBridgeError::Value)
        );
        let mut action = bytes.clone();
        action[0] = 3;
        assert_eq!(
            TradeOfferRequestRecord::decode(&action),
            Err(BattleBridgeError::Value)
        );
        assert_eq!(
            TradeOfferRequestRecord::decode(&bytes[..15]),
            Err(BattleBridgeError::Length)
        );
    }

    #[test]
    fn decision_layout_and_decline_rules() {
        let accept = TradeOfferDecisionRecord {
            decision: TradeOfferDecision::Accept,
            slot: 5,
            offer_token: 77,
            personality: 1,
            ot_id: 2,
        };
        let bytes = accept.encode().unwrap();
        assert_eq!(bytes, [1, 5, 0, 0, 77, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0]);
        assert_eq!(TradeOfferDecisionRecord::decode(&bytes), Ok(accept));
        let decline = TradeOfferDecisionRecord {
            decision: TradeOfferDecision::Decline,
            slot: 0,
            offer_token: 77,
            personality: 0,
            ot_id: 0,
        };
        assert_eq!(
            TradeOfferDecisionRecord::decode(&decline.encode().unwrap()),
            Ok(decline)
        );
        assert!(
            TradeOfferDecisionRecord {
                offer_token: 0,
                ..accept
            }
            .encode()
            .is_err()
        );
        assert!(
            TradeOfferDecisionRecord { slot: 3, ..decline }
                .encode()
                .is_err()
        );
    }

    #[test]
    fn received_layout_carries_the_raw_nickname_and_egg_flag() {
        let received = TradeOfferReceivedRecord {
            offer_token: 0xDEAD_BEEF,
            species: 0x0123,
            level: 42,
            is_egg: true,
            nickname: [0xBB, 0xCC, 0xFF, 0, 0, 0, 0, 0, 0, 0],
        };
        let bytes = received.encode().unwrap();
        assert_eq!(bytes.len(), TradeOfferReceivedRecord::WIRE_SIZE);
        assert_eq!(&bytes[..8], &[0xEF, 0xBE, 0xAD, 0xDE, 0x23, 0x01, 42, 1]);
        assert_eq!(&bytes[8..18], &received.nickname);
        assert_eq!(&bytes[18..], &[0, 0]);
        assert_eq!(TradeOfferReceivedRecord::decode(&bytes), Ok(received));
        let mut flags = bytes.clone();
        flags[7] = 2;
        assert!(TradeOfferReceivedRecord::decode(&flags).is_err());
        assert!(
            TradeOfferReceivedRecord {
                level: 101,
                ..received
            }
            .encode()
            .is_err()
        );
        assert!(
            TradeOfferReceivedRecord {
                species: 0,
                ..received
            }
            .encode()
            .is_err()
        );
    }

    #[test]
    fn status_layout_binds_role_to_its_identifier() {
        let pending = TradeOfferStatusRecord {
            role: TradeOfferRole::Requester,
            outcome: TradeOfferOutcome::Pending,
            request_id: 5,
            offer_token: 6,
        };
        let bytes = pending.encode().unwrap();
        assert_eq!(bytes, [1, 1, 0, 0, 5, 0, 0, 0, 6, 0, 0, 0]);
        assert_eq!(TradeOfferStatusRecord::decode(&bytes), Ok(pending));
        // A failure before the server held an offer has no token.
        let failed = TradeOfferStatusRecord {
            outcome: TradeOfferOutcome::Mail,
            offer_token: 0,
            ..pending
        };
        assert_eq!(
            TradeOfferStatusRecord::decode(&failed.encode().unwrap()),
            Ok(failed)
        );
        assert!(
            TradeOfferStatusRecord {
                offer_token: 0,
                ..pending
            }
            .encode()
            .is_err()
        );
        let responder = TradeOfferStatusRecord {
            role: TradeOfferRole::Responder,
            outcome: TradeOfferOutcome::Expired,
            request_id: 0,
            offer_token: 6,
        };
        assert_eq!(
            TradeOfferStatusRecord::decode(&responder.encode().unwrap()),
            Ok(responder)
        );
        assert!(
            TradeOfferStatusRecord {
                outcome: TradeOfferOutcome::Pending,
                ..responder
            }
            .encode()
            .is_err()
        );
        assert!(
            TradeOfferStatusRecord {
                request_id: 1,
                ..responder
            }
            .encode()
            .is_err()
        );
        let mut outcome = bytes.clone();
        outcome[1] = 11;
        assert!(TradeOfferStatusRecord::decode(&outcome).is_err());
    }
}
