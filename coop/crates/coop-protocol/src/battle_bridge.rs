//! Typed, bounded battle consensus records shared by the ROM bridge and control channel.
//! Party-mon bytes and compact actions are opaque here; decoding never claims they are safe game state.

use crate::RegionId;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

pub const BATTLE_PARTY_MON_SIZE: usize = 100;
pub const BATTLE_MAX_ACTION_SIZE: usize = 48;
pub const BATTLE_MAX_TURN: u16 = 32;
pub const BATTLE_COMMIT_WIRE_SIZE: usize = 43;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum BattleKind {
    CooperativeTrainer = 1,
    Friendly = 2,
}

impl BattleKind {
    fn from_wire(value: u8) -> Result<Self, BattleBridgeError> {
        match value {
            1 => Ok(Self::CooperativeTrainer),
            2 => Ok(Self::Friendly),
            _ => Err(BattleBridgeError::Value),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum BattleDecision {
    Decline = 0,
    Accept = 1,
}

/// Terminal consent outcome delivered to the requester after the partner's
/// response (or the reservation deadline) is known.  This is deliberately
/// separate from `BattleDecision`: the responder sends a decision, while the
/// requester receives an outcome bound to its original reserve nonce.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum BattleConsentOutcome {
    Accepted = 1,
    Declined = 2,
    Expired = 3,
}

impl BattleConsentOutcome {
    fn from_wire(value: u8) -> Result<Self, BattleBridgeError> {
        match value {
            1 => Ok(Self::Accepted),
            2 => Ok(Self::Declined),
            3 => Ok(Self::Expired),
            _ => Err(BattleBridgeError::Value),
        }
    }
}

impl BattleDecision {
    fn from_wire(value: u8) -> Result<Self, BattleBridgeError> {
        match value {
            0 => Ok(Self::Decline),
            1 => Ok(Self::Accept),
            _ => Err(BattleBridgeError::Value),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum BattleRole {
    Requester = 0,
    Responder = 1,
}

impl BattleRole {
    fn from_wire(value: u8) -> Result<Self, BattleBridgeError> {
        match value {
            0 => Ok(Self::Requester),
            1 => Ok(Self::Responder),
            _ => Err(BattleBridgeError::Value),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub enum BattleBridgeError {
    #[error("invalid battle record length")]
    Length,
    #[error("invalid battle record value")]
    Value,
}

/// UUID octets in standard network order, serialized as a canonical lowercase UUID.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct BattleId(pub [u8; 16]);

impl BattleId {
    pub fn parse(value: &str) -> Result<Self, BattleBridgeError> {
        let bytes = value.as_bytes();
        if bytes.len() != 36 {
            return Err(BattleBridgeError::Value);
        }
        let mut result = [0; 16];
        let mut index = 0;
        let mut half = None;
        for (position, byte) in bytes.iter().copied().enumerate() {
            if matches!(position, 8 | 13 | 18 | 23) {
                if byte != b'-' {
                    return Err(BattleBridgeError::Value);
                }
                continue;
            }
            let nibble = hex_digit(byte).ok_or(BattleBridgeError::Value)?;
            if let Some(upper) = half.take() {
                result[index] = (upper << 4) | nibble;
                index += 1;
            } else {
                half = Some(nibble);
            }
        }
        Ok(Self(result))
    }

    pub fn canonical(self) -> String {
        let hex = hex_encode(&self.0);
        format!(
            "{}-{}-{}-{}-{}",
            &hex[0..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..32]
        )
    }
}

impl Serialize for BattleId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.canonical())
    }
}

impl<'de> Deserialize<'de> for BattleId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::parse(&String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// A binary digest represented as exactly 64 lowercase hex characters in JSON.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BattleDigest(pub [u8; 32]);

impl BattleDigest {
    pub fn parse(value: &str) -> Result<Self, BattleBridgeError> {
        if value.len() != 64 {
            return Err(BattleBridgeError::Value);
        }
        let mut result = [0; 32];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            result[index] = (hex_digit(pair[0]).ok_or(BattleBridgeError::Value)? << 4)
                | hex_digit(pair[1]).ok_or(BattleBridgeError::Value)?;
        }
        Ok(Self(result))
    }
}

impl Serialize for BattleDigest {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&hex_encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for BattleDigest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::parse(&String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(char::from(HEX[usize::from(byte >> 4)]));
        result.push(char::from(HEX[usize::from(byte & 15)]));
    }
    result
}

fn id(bytes: &[u8]) -> BattleId {
    BattleId(bytes[0..16].try_into().expect("checked length"))
}
fn turn(bytes: &[u8]) -> u16 {
    u16::from_le_bytes([bytes[16], bytes[17]])
}
fn check_turn(value: u16, manifest: bool) -> Result<(), BattleBridgeError> {
    if value > BATTLE_MAX_TURN || (!manifest && value == 0) {
        Err(BattleBridgeError::Value)
    } else {
        Ok(())
    }
}

fn check_battle_id(value: BattleId) -> Result<(), BattleBridgeError> {
    if value.0 == [0; 16] {
        Err(BattleBridgeError::Value)
    } else {
        Ok(())
    }
}

/// ROM request before the cloud has assigned a battle UUID.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainerBattleReserveRecord {
    pub kind: BattleKind,
    pub request_nonce: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trainer_region: Option<RegionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trainer_ordinal: Option<u16>,
}

impl TrainerBattleReserveRecord {
    pub const FRIENDLY_WIRE_SIZE: usize = 5;
    pub const TRAINER_WIRE_SIZE: usize = 8;

    fn validate(&self) -> Result<(), BattleBridgeError> {
        if self.request_nonce == 0
            || (self.kind == BattleKind::CooperativeTrainer) != self.trainer_ordinal.is_some()
            || (self.kind == BattleKind::CooperativeTrainer) != self.trainer_region.is_some()
            || self.trainer_region == Some(RegionId::Unspecified)
        {
            return Err(BattleBridgeError::Value);
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<Vec<u8>, BattleBridgeError> {
        self.validate()?;
        let mut bytes = Vec::with_capacity(if self.trainer_ordinal.is_some() {
            Self::TRAINER_WIRE_SIZE
        } else {
            Self::FRIENDLY_WIRE_SIZE
        });
        bytes.push(self.kind as u8);
        bytes.extend_from_slice(&self.request_nonce.to_le_bytes());
        if let Some(ordinal) = self.trainer_ordinal {
            bytes.push(
                self.trainer_region
                    .expect("validated trainer region")
                    .wire(),
            );
            bytes.extend_from_slice(&ordinal.to_le_bytes());
        }
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, BattleBridgeError> {
        if bytes.len() != Self::FRIENDLY_WIRE_SIZE && bytes.len() != Self::TRAINER_WIRE_SIZE {
            return Err(BattleBridgeError::Length);
        }
        let result = Self {
            kind: BattleKind::from_wire(bytes[0])?,
            request_nonce: u32::from_le_bytes(bytes[1..5].try_into().expect("checked length")),
            trainer_region: (bytes.len() == Self::TRAINER_WIRE_SIZE)
                .then(|| RegionId::from_wire(bytes[5]).map_err(|_| BattleBridgeError::Value))
                .transpose()?,
            trainer_ordinal: (bytes.len() == Self::TRAINER_WIRE_SIZE)
                .then(|| u16::from_le_bytes(bytes[6..8].try_into().expect("checked length"))),
        };
        result.validate()?;
        Ok(result)
    }
}

/// A pre-offer reserve the launcher could not safely submit or reconcile.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BattleReserveRejectedRecord {
    pub request_nonce: u32,
}

impl BattleReserveRejectedRecord {
    pub const WIRE_SIZE: usize = 4;

    pub fn encode(&self) -> Result<Vec<u8>, BattleBridgeError> {
        if self.request_nonce == 0 {
            return Err(BattleBridgeError::Value);
        }
        Ok(self.request_nonce.to_le_bytes().to_vec())
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, BattleBridgeError> {
        if bytes.len() != Self::WIRE_SIZE {
            return Err(BattleBridgeError::Length);
        }
        let result = Self {
            request_nonce: u32::from_le_bytes(bytes.try_into().expect("checked length")),
        };
        result.encode()?;
        Ok(result)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BattleJoinResponseRecord {
    pub battle_id: BattleId,
    pub decision: BattleDecision,
}

impl BattleJoinResponseRecord {
    pub const WIRE_SIZE: usize = 17;

    pub fn encode(&self) -> Result<Vec<u8>, BattleBridgeError> {
        check_battle_id(self.battle_id)?;
        let mut bytes = Vec::with_capacity(Self::WIRE_SIZE);
        bytes.extend_from_slice(&self.battle_id.0);
        bytes.push(self.decision as u8);
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, BattleBridgeError> {
        if bytes.len() != Self::WIRE_SIZE {
            return Err(BattleBridgeError::Length);
        }
        let result = Self {
            battle_id: id(bytes),
            decision: BattleDecision::from_wire(bytes[16])?,
        };
        check_battle_id(result.battle_id)?;
        Ok(result)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BattleJoinOfferRecord {
    pub battle_id: BattleId,
    pub kind: BattleKind,
    pub role: BattleRole,
    pub request_nonce: u32,
}

/// Result of the partner's consent for the requester.  The reserve nonce is
/// included so a delayed result cannot be applied to a later request after a
/// ROM/control restart.  Replaying the exact record is idempotent on the ROM.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BattleConsentOutcomeRecord {
    pub battle_id: BattleId,
    pub request_nonce: u32,
    pub outcome: BattleConsentOutcome,
}

impl BattleConsentOutcomeRecord {
    pub const WIRE_SIZE: usize = 21;

    fn validate(&self) -> Result<(), BattleBridgeError> {
        check_battle_id(self.battle_id)?;
        if self.request_nonce == 0 {
            return Err(BattleBridgeError::Value);
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<Vec<u8>, BattleBridgeError> {
        self.validate()?;
        let mut bytes = Vec::with_capacity(Self::WIRE_SIZE);
        bytes.extend_from_slice(&self.battle_id.0);
        bytes.extend_from_slice(&self.request_nonce.to_le_bytes());
        bytes.push(self.outcome as u8);
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, BattleBridgeError> {
        if bytes.len() != Self::WIRE_SIZE {
            return Err(BattleBridgeError::Length);
        }
        let result = Self {
            battle_id: id(bytes),
            request_nonce: u32::from_le_bytes(bytes[16..20].try_into().expect("checked length")),
            outcome: BattleConsentOutcome::from_wire(bytes[20])?,
        };
        result.validate()?;
        Ok(result)
    }
}

impl BattleJoinOfferRecord {
    pub const WIRE_SIZE: usize = 22;

    fn validate(&self) -> Result<(), BattleBridgeError> {
        check_battle_id(self.battle_id)?;
        if (self.role == BattleRole::Requester && self.request_nonce == 0)
            || (self.role == BattleRole::Responder && self.request_nonce != 0)
        {
            return Err(BattleBridgeError::Value);
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<Vec<u8>, BattleBridgeError> {
        self.validate()?;
        let mut bytes = Vec::with_capacity(Self::WIRE_SIZE);
        bytes.extend_from_slice(&self.battle_id.0);
        bytes.extend_from_slice(&[self.kind as u8, self.role as u8]);
        bytes.extend_from_slice(&self.request_nonce.to_le_bytes());
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, BattleBridgeError> {
        if bytes.len() != Self::WIRE_SIZE {
            return Err(BattleBridgeError::Length);
        }
        let result = Self {
            battle_id: id(bytes),
            kind: BattleKind::from_wire(bytes[16])?,
            role: BattleRole::from_wire(bytes[17])?,
            request_nonce: u32::from_le_bytes(bytes[18..22].try_into().expect("checked length")),
        };
        result.validate()?;
        Ok(result)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartySnapshotChunk {
    pub battle_id: BattleId,
    pub party_slot: u8,
    pub chunk_index: u8,
    pub chunk_count: u8,
    #[serde(with = "mon_hex")]
    pub mon: Vec<u8>,
}

mod mon_hex {
    use super::{BATTLE_PARTY_MON_SIZE, hex_digit, hex_encode};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&hex_encode(value))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text.len() != BATTLE_PARTY_MON_SIZE * 2 {
            return Err(serde::de::Error::custom("invalid party mon length"));
        }
        text.as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                let upper = hex_digit(pair[0])
                    .ok_or_else(|| serde::de::Error::custom("invalid party mon hex"))?;
                let lower = hex_digit(pair[1])
                    .ok_or_else(|| serde::de::Error::custom("invalid party mon hex"))?;
                Ok((upper << 4) | lower)
            })
            .collect()
    }
}

impl PartySnapshotChunk {
    pub const WIRE_SIZE: usize = 120;
    fn validate(&self) -> Result<(), BattleBridgeError> {
        if self.party_slot >= 6
            || self.chunk_count == 0
            || self.chunk_count > 6
            || self.chunk_index >= self.chunk_count
            || self.party_slot != self.chunk_index
            || self.mon.len() != BATTLE_PARTY_MON_SIZE
        {
            return Err(BattleBridgeError::Value);
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>, BattleBridgeError> {
        self.validate()?;
        let mut bytes = Vec::with_capacity(Self::WIRE_SIZE);
        bytes.extend_from_slice(&self.battle_id.0);
        bytes.extend_from_slice(&[
            self.party_slot,
            self.chunk_index,
            self.chunk_count,
            BATTLE_PARTY_MON_SIZE as u8,
        ]);
        bytes.extend_from_slice(&self.mon);
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, BattleBridgeError> {
        if bytes.len() != Self::WIRE_SIZE {
            return Err(BattleBridgeError::Length);
        }
        if bytes[19] != BATTLE_PARTY_MON_SIZE as u8 {
            return Err(BattleBridgeError::Value);
        }
        let result = Self {
            battle_id: id(bytes),
            party_slot: bytes[16],
            chunk_index: bytes[17],
            chunk_count: bytes[18],
            mon: bytes[20..].to_vec(),
        };
        result.validate()?;
        Ok(result)
    }
}

/// Sent only after the ROM has validated a complete peer party and its own
/// live party against the manifest. The digest is the ROM's current party,
/// allowing the server to compare the readiness claim with its save anchor.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BattleReadyRecord {
    pub battle_id: BattleId,
    pub party_digest: BattleDigest,
}

impl BattleReadyRecord {
    pub const WIRE_SIZE: usize = 48;

    pub fn encode(&self) -> Result<Vec<u8>, BattleBridgeError> {
        check_battle_id(self.battle_id)?;
        let mut bytes = Vec::with_capacity(Self::WIRE_SIZE);
        bytes.extend_from_slice(&self.battle_id.0);
        bytes.extend_from_slice(&self.party_digest.0);
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, BattleBridgeError> {
        if bytes.len() != Self::WIRE_SIZE {
            return Err(BattleBridgeError::Length);
        }
        let result = Self {
            battle_id: id(bytes),
            party_digest: BattleDigest(bytes[16..48].try_into().expect("checked length")),
        };
        check_battle_id(result.battle_id)?;
        Ok(result)
    }
}

/// Server release after both members have acknowledged the same manifest.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BattleStartRecord {
    pub battle_id: BattleId,
}

impl BattleStartRecord {
    pub const WIRE_SIZE: usize = 16;

    pub fn encode(&self) -> Result<Vec<u8>, BattleBridgeError> {
        check_battle_id(self.battle_id)?;
        Ok(self.battle_id.0.to_vec())
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, BattleBridgeError> {
        if bytes.len() != Self::WIRE_SIZE {
            return Err(BattleBridgeError::Length);
        }
        let result = Self {
            battle_id: id(bytes),
        };
        check_battle_id(result.battle_id)?;
        Ok(result)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionIntent {
    pub battle_id: BattleId,
    pub turn: u16,
    pub action: Vec<u8>,
}

impl ActionIntent {
    fn validate(&self) -> Result<(), BattleBridgeError> {
        check_turn(self.turn, false)?;
        if self.action.is_empty() || self.action.len() > BATTLE_MAX_ACTION_SIZE {
            return Err(BattleBridgeError::Value);
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>, BattleBridgeError> {
        self.validate()?;
        let mut bytes = Vec::with_capacity(19 + self.action.len());
        bytes.extend_from_slice(&self.battle_id.0);
        bytes.extend_from_slice(&self.turn.to_le_bytes());
        bytes.push(self.action.len() as u8);
        bytes.extend_from_slice(&self.action);
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, BattleBridgeError> {
        if bytes.len() < 19 || bytes.len() != 19 + usize::from(bytes[18]) {
            return Err(BattleBridgeError::Length);
        }
        let result = Self {
            battle_id: id(bytes),
            turn: turn(bytes),
            action: bytes[19..].to_vec(),
        };
        result.validate()?;
        Ok(result)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnResultHash {
    pub battle_id: BattleId,
    pub turn: u16,
    pub digest: BattleDigest,
}

impl TurnResultHash {
    pub const WIRE_SIZE: usize = 50;
    pub fn encode(&self) -> Result<Vec<u8>, BattleBridgeError> {
        check_turn(self.turn, false)?;
        let mut bytes = Vec::with_capacity(Self::WIRE_SIZE);
        bytes.extend_from_slice(&self.battle_id.0);
        bytes.extend_from_slice(&self.turn.to_le_bytes());
        bytes.extend_from_slice(&self.digest.0);
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, BattleBridgeError> {
        if bytes.len() != Self::WIRE_SIZE {
            return Err(BattleBridgeError::Length);
        }
        let result = Self {
            battle_id: id(bytes),
            turn: turn(bytes),
            digest: BattleDigest(bytes[18..50].try_into().expect("checked length")),
        };
        check_turn(result.turn, false)?;
        Ok(result)
    }
}

/// Canonical terminal outcome reported by the ROM after both members have
/// reached the same final turn. The member outcomes use the manifest's
/// canonical member order; trainer battles use `Won`/`Lost` from the local
/// player's perspective.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum BattleFinishedResult {
    Member0Won = 1,
    Member1Won = 2,
    Draw = 3,
    Won = 4,
    Lost = 5,
}

impl BattleFinishedResult {
    fn from_wire(value: u8) -> Result<Self, BattleBridgeError> {
        match value {
            1 => Ok(Self::Member0Won),
            2 => Ok(Self::Member1Won),
            3 => Ok(Self::Draw),
            4 => Ok(Self::Won),
            5 => Ok(Self::Lost),
            _ => Err(BattleBridgeError::Value),
        }
    }
}

/// A bounded, pre-ledger terminal battle attestation from the ROM.
///
/// This record only reports the terminal consensus input. It does not grant
/// progress or authorize a save mutation; those decisions remain server-side.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BattleFinishedRecord {
    pub battle_id: BattleId,
    pub turn: u16,
    pub result: BattleFinishedResult,
    pub terminal_hash: BattleDigest,
}

/// A server-authorized trainer progress commit delivered to the ROM.
///
/// The commit UUID is deliberately kept as the same fixed-width UUID type as
/// battle IDs.  The two UUIDs are distinct fields on the wire and both must
/// be nonzero, so a commit cannot accidentally be replayed for another battle.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BattleCommitRecord {
    pub battle_id: BattleId,
    pub commit_id: BattleId,
    pub trainer_region: RegionId,
    pub trainer_ordinal: u16,
    pub source_revision: u64,
}

/// ROM acknowledgement for a previously delivered [`BattleCommitRecord`].
/// It has the exact same bytes so the acknowledgement is bound to every
/// server-issued identity and source revision.
pub type CommitAppliedRecord = BattleCommitRecord;

impl BattleCommitRecord {
    pub const WIRE_SIZE: usize = BATTLE_COMMIT_WIRE_SIZE;

    fn validate(&self) -> Result<(), BattleBridgeError> {
        check_battle_id(self.battle_id)?;
        check_battle_id(self.commit_id)?;
        if self.trainer_region == RegionId::Unspecified
            || self.trainer_ordinal == 0
            || self.source_revision == 0
        {
            return Err(BattleBridgeError::Value);
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<Vec<u8>, BattleBridgeError> {
        self.validate()?;
        let mut bytes = Vec::with_capacity(Self::WIRE_SIZE);
        bytes.extend_from_slice(&self.battle_id.0);
        bytes.extend_from_slice(&self.commit_id.0);
        bytes.push(self.trainer_region.wire());
        bytes.extend_from_slice(&self.trainer_ordinal.to_le_bytes());
        bytes.extend_from_slice(&self.source_revision.to_le_bytes());
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, BattleBridgeError> {
        if bytes.len() != Self::WIRE_SIZE {
            return Err(BattleBridgeError::Length);
        }
        let result = Self {
            battle_id: id(bytes),
            commit_id: BattleId(bytes[16..32].try_into().expect("checked length")),
            trainer_region: RegionId::from_wire(bytes[32]).map_err(|_| BattleBridgeError::Value)?,
            trainer_ordinal: u16::from_le_bytes([bytes[33], bytes[34]]),
            source_revision: u64::from_le_bytes(bytes[35..43].try_into().expect("checked length")),
        };
        result.validate()?;
        Ok(result)
    }
}

impl BattleFinishedRecord {
    pub const WIRE_SIZE: usize = 51;

    fn validate(&self) -> Result<(), BattleBridgeError> {
        check_battle_id(self.battle_id)?;
        check_turn(self.turn, false)?;
        if self.terminal_hash.0 == [0; 32] {
            return Err(BattleBridgeError::Value);
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<Vec<u8>, BattleBridgeError> {
        self.validate()?;
        let mut bytes = Vec::with_capacity(Self::WIRE_SIZE);
        bytes.extend_from_slice(&self.battle_id.0);
        bytes.extend_from_slice(&self.turn.to_le_bytes());
        bytes.push(self.result as u8);
        bytes.extend_from_slice(&self.terminal_hash.0);
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, BattleBridgeError> {
        if bytes.len() != Self::WIRE_SIZE {
            return Err(BattleBridgeError::Length);
        }
        let result = Self {
            battle_id: id(bytes),
            turn: turn(bytes),
            result: BattleFinishedResult::from_wire(bytes[18])?,
            terminal_hash: BattleDigest(bytes[19..51].try_into().expect("checked length")),
        };
        result.validate()?;
        Ok(result)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BattleManifestRecord {
    pub battle_id: BattleId,
    /// Zero denotes initial manifest, before turn one.
    pub turn: u16,
    pub seed: BattleDigest,
    pub snapshot_hashes: [BattleDigest; 2],
    pub kind: BattleKind,
    pub local_member_slot: u8,
    pub trainer_region: RegionId,
    pub trainer_ordinal: u16,
}

impl BattleManifestRecord {
    pub const WIRE_SIZE: usize = 119;
    fn validate_identity(&self) -> Result<(), BattleBridgeError> {
        if self.local_member_slot > 1 {
            return Err(BattleBridgeError::Value);
        }
        match self.kind {
            BattleKind::Friendly
                if self.trainer_region == RegionId::Unspecified && self.trainer_ordinal == 0 =>
            {
                Ok(())
            }
            BattleKind::CooperativeTrainer if self.trainer_region != RegionId::Unspecified => {
                Ok(())
            }
            _ => Err(BattleBridgeError::Value),
        }
    }
    pub fn encode(&self) -> Result<Vec<u8>, BattleBridgeError> {
        check_turn(self.turn, true)?;
        self.validate_identity()?;
        let mut bytes = Vec::with_capacity(Self::WIRE_SIZE);
        bytes.extend_from_slice(&self.battle_id.0);
        bytes.extend_from_slice(&self.turn.to_le_bytes());
        bytes.extend_from_slice(&self.seed.0);
        bytes.extend_from_slice(&self.snapshot_hashes[0].0);
        bytes.extend_from_slice(&self.snapshot_hashes[1].0);
        bytes.extend_from_slice(&[
            self.kind as u8,
            self.local_member_slot,
            self.trainer_region.wire(),
        ]);
        bytes.extend_from_slice(&self.trainer_ordinal.to_le_bytes());
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, BattleBridgeError> {
        if bytes.len() != Self::WIRE_SIZE {
            return Err(BattleBridgeError::Length);
        }
        let result = Self {
            battle_id: id(bytes),
            turn: turn(bytes),
            seed: BattleDigest(bytes[18..50].try_into().expect("checked length")),
            snapshot_hashes: [
                BattleDigest(bytes[50..82].try_into().expect("checked length")),
                BattleDigest(bytes[82..114].try_into().expect("checked length")),
            ],
            kind: BattleKind::from_wire(bytes[114])?,
            local_member_slot: bytes[115],
            trainer_region: RegionId::from_wire(bytes[116])
                .map_err(|_| BattleBridgeError::Value)?,
            trainer_ordinal: u16::from_le_bytes([bytes[117], bytes[118]]),
        };
        check_turn(result.turn, true)?;
        result.validate_identity()?;
        Ok(result)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnBundleRecord {
    pub battle_id: BattleId,
    pub turn: u16,
    pub actions: [Vec<u8>; 2],
}

impl TurnBundleRecord {
    fn validate(&self) -> Result<(), BattleBridgeError> {
        check_turn(self.turn, false)?;
        if self
            .actions
            .iter()
            .any(|action| action.is_empty() || action.len() > BATTLE_MAX_ACTION_SIZE)
        {
            return Err(BattleBridgeError::Value);
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>, BattleBridgeError> {
        self.validate()?;
        let mut bytes = Vec::with_capacity(20 + self.actions[0].len() + self.actions[1].len());
        bytes.extend_from_slice(&self.battle_id.0);
        bytes.extend_from_slice(&self.turn.to_le_bytes());
        bytes.extend_from_slice(&[self.actions[0].len() as u8, self.actions[1].len() as u8]);
        bytes.extend_from_slice(&self.actions[0]);
        bytes.extend_from_slice(&self.actions[1]);
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, BattleBridgeError> {
        if bytes.len() < 20 || bytes.len() != 20 + usize::from(bytes[18]) + usize::from(bytes[19]) {
            return Err(BattleBridgeError::Length);
        }
        let split = 20 + usize::from(bytes[18]);
        let result = Self {
            battle_id: id(bytes),
            turn: turn(bytes),
            actions: [bytes[20..split].to_vec(), bytes[split..].to_vec()],
        };
        result.validate()?;
        Ok(result)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PauseForReconnectRecord {
    pub battle_id: BattleId,
    pub turn: u16,
    pub missing_slot: u8,
}

impl PauseForReconnectRecord {
    pub const WIRE_SIZE: usize = 19;
    pub fn encode(&self) -> Result<Vec<u8>, BattleBridgeError> {
        check_turn(self.turn, false)?;
        if self.missing_slot > 1 {
            return Err(BattleBridgeError::Value);
        }
        let mut bytes = Vec::with_capacity(Self::WIRE_SIZE);
        bytes.extend_from_slice(&self.battle_id.0);
        bytes.extend_from_slice(&self.turn.to_le_bytes());
        bytes.push(self.missing_slot);
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, BattleBridgeError> {
        if bytes.len() != Self::WIRE_SIZE {
            return Err(BattleBridgeError::Length);
        }
        let result = Self {
            battle_id: id(bytes),
            turn: turn(bytes),
            missing_slot: bytes[18],
        };
        result.encode()?;
        Ok(result)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbortBattleRecord {
    pub battle_id: BattleId,
    pub reason: u8,
}

impl AbortBattleRecord {
    pub const WIRE_SIZE: usize = 17;
    pub fn encode(&self) -> Result<Vec<u8>, BattleBridgeError> {
        if !(1..=5).contains(&self.reason) {
            return Err(BattleBridgeError::Value);
        }
        let mut bytes = Vec::with_capacity(Self::WIRE_SIZE);
        bytes.extend_from_slice(&self.battle_id.0);
        bytes.push(self.reason);
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, BattleBridgeError> {
        if bytes.len() != Self::WIRE_SIZE {
            return Err(BattleBridgeError::Length);
        }
        let result = Self {
            battle_id: id(bytes),
            reason: bytes[16],
        };
        result.encode()?;
        Ok(result)
    }
}

/// A server-issued trade outcome (an open `TRADE` ledger entry) delivered to
/// the ROM as bridge message `TradeCommit` (0x0119, sidecar to ROM).
///
/// Wire layout, exactly [`TradeCommitRecord::WIRE_SIZE`] = 128 bytes, i.e. the
/// whole bridge payload; every multi-byte integer is little endian:
///
/// | offset | size | field                                                   |
/// |-------:|-----:|---------------------------------------------------------|
/// |      0 |   16 | `commit_id`, ledger commit UUID octets in network order |
/// |     16 |    1 | `slot`, zero-based party slot `0..=5` to overwrite      |
/// |     17 |    3 | reserved, must be zero (keeps the u32 fields aligned)   |
/// |     20 |    4 | `outgoing_personality` of the Pokémon leaving `slot`    |
/// |     24 |    4 | `outgoing_ot_id` of the Pokémon leaving `slot`          |
/// |     28 |  100 | `incoming_record`, the exact 100-byte party `struct Pokemon` |
///
/// The first 28 bytes are the [`TradeCommitAppliedRecord`] header the ROM
/// echoes back once `slot` holds `incoming_record`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TradeCommitRecord {
    pub commit_id: BattleId,
    pub slot: u8,
    pub outgoing_personality: u32,
    pub outgoing_ot_id: u32,
    #[serde(with = "mon_hex_array")]
    pub incoming_record: [u8; BATTLE_PARTY_MON_SIZE],
}

/// ROM acknowledgement for an applied [`TradeCommitRecord`].
///
/// It travels as the existing `CommitApplied` (0x000B) bridge message and is
/// distinguished from the 43-byte battle acknowledgement by its length:
/// exactly [`TradeCommitAppliedRecord::WIRE_SIZE`] = 28 bytes, byte-identical
/// to offsets `0..28` of the delivered `TradeCommit` payload (commit UUID,
/// slot, three zero bytes, outgoing personality, outgoing OT ID).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TradeCommitAppliedRecord {
    pub commit_id: BattleId,
    pub slot: u8,
    pub outgoing_personality: u32,
    pub outgoing_ot_id: u32,
}

mod mon_hex_array {
    use super::{BATTLE_PARTY_MON_SIZE, hex_digit, hex_encode};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(
        value: &[u8; BATTLE_PARTY_MON_SIZE],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&hex_encode(value))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<[u8; BATTLE_PARTY_MON_SIZE], D::Error> {
        let text = String::deserialize(deserializer)?;
        if text.len() != BATTLE_PARTY_MON_SIZE * 2 {
            return Err(serde::de::Error::custom("invalid party mon length"));
        }
        let mut result = [0; BATTLE_PARTY_MON_SIZE];
        for (slot, pair) in result.iter_mut().zip(text.as_bytes().chunks_exact(2)) {
            let upper = hex_digit(pair[0])
                .ok_or_else(|| serde::de::Error::custom("invalid party mon hex"))?;
            let lower = hex_digit(pair[1])
                .ok_or_else(|| serde::de::Error::custom("invalid party mon hex"))?;
            *slot = (upper << 4) | lower;
        }
        Ok(result)
    }
}

const TRADE_COMMIT_HEADER_SIZE: usize = 28;

fn encode_trade_header(
    commit_id: BattleId,
    slot: u8,
    outgoing_personality: u32,
    outgoing_ot_id: u32,
) -> Result<Vec<u8>, BattleBridgeError> {
    check_battle_id(commit_id)?;
    if slot >= 6 {
        return Err(BattleBridgeError::Value);
    }
    let mut bytes = Vec::with_capacity(TradeCommitRecord::WIRE_SIZE);
    bytes.extend_from_slice(&commit_id.0);
    bytes.extend_from_slice(&[slot, 0, 0, 0]);
    bytes.extend_from_slice(&outgoing_personality.to_le_bytes());
    bytes.extend_from_slice(&outgoing_ot_id.to_le_bytes());
    Ok(bytes)
}

fn decode_trade_header(bytes: &[u8]) -> Result<(BattleId, u8, u32, u32), BattleBridgeError> {
    if bytes[17..20] != [0, 0, 0] {
        return Err(BattleBridgeError::Value);
    }
    let commit_id = id(bytes);
    let slot = bytes[16];
    check_battle_id(commit_id)?;
    if slot >= 6 {
        return Err(BattleBridgeError::Value);
    }
    Ok((
        commit_id,
        slot,
        u32::from_le_bytes(bytes[20..24].try_into().expect("checked length")),
        u32::from_le_bytes(bytes[24..28].try_into().expect("checked length")),
    ))
}

impl TradeCommitRecord {
    pub const WIRE_SIZE: usize = TRADE_COMMIT_HEADER_SIZE + BATTLE_PARTY_MON_SIZE;

    pub fn encode(&self) -> Result<Vec<u8>, BattleBridgeError> {
        if self.incoming_record == [0; BATTLE_PARTY_MON_SIZE] {
            return Err(BattleBridgeError::Value);
        }
        let mut bytes = encode_trade_header(
            self.commit_id,
            self.slot,
            self.outgoing_personality,
            self.outgoing_ot_id,
        )?;
        bytes.extend_from_slice(&self.incoming_record);
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, BattleBridgeError> {
        if bytes.len() != Self::WIRE_SIZE {
            return Err(BattleBridgeError::Length);
        }
        let (commit_id, slot, outgoing_personality, outgoing_ot_id) = decode_trade_header(bytes)?;
        let result = Self {
            commit_id,
            slot,
            outgoing_personality,
            outgoing_ot_id,
            incoming_record: bytes[TRADE_COMMIT_HEADER_SIZE..]
                .try_into()
                .expect("checked length"),
        };
        result.encode()?;
        Ok(result)
    }

    /// The acknowledgement the ROM must return for this exact commit.
    #[must_use]
    pub const fn applied(&self) -> TradeCommitAppliedRecord {
        TradeCommitAppliedRecord {
            commit_id: self.commit_id,
            slot: self.slot,
            outgoing_personality: self.outgoing_personality,
            outgoing_ot_id: self.outgoing_ot_id,
        }
    }
}

impl TradeCommitAppliedRecord {
    pub const WIRE_SIZE: usize = TRADE_COMMIT_HEADER_SIZE;

    pub fn encode(&self) -> Result<Vec<u8>, BattleBridgeError> {
        encode_trade_header(
            self.commit_id,
            self.slot,
            self.outgoing_personality,
            self.outgoing_ot_id,
        )
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, BattleBridgeError> {
        if bytes.len() != Self::WIRE_SIZE {
            return Err(BattleBridgeError::Length);
        }
        let (commit_id, slot, outgoing_personality, outgoing_ot_id) = decode_trade_header(bytes)?;
        Ok(Self {
            commit_id,
            slot,
            outgoing_personality,
            outgoing_ot_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> BattleId {
        BattleId::parse("00112233-4455-6677-8899-aabbccddeeff").unwrap()
    }

    #[test]
    fn canonical_ids_and_digests() {
        assert_eq!(id().canonical(), "00112233-4455-6677-8899-aabbccddeeff");
        assert!(BattleId::parse("00112233-4455-6677-8899-AABBCCDDEEFF").is_err());
        assert!(BattleDigest::parse(&"F".repeat(64)).is_err());
        assert_eq!(BattleDigest::parse(&"ab".repeat(32)).unwrap().0, [0xab; 32]);
    }

    #[test]
    fn finished_records_have_fixed_layout_and_strict_terminal_values() {
        let record = BattleFinishedRecord {
            battle_id: id(),
            turn: 32,
            result: BattleFinishedResult::Member1Won,
            terminal_hash: BattleDigest([0xCD; 32]),
        };
        let bytes = record.encode().unwrap();
        assert_eq!(bytes.len(), BattleFinishedRecord::WIRE_SIZE);
        assert_eq!(&bytes[16..18], &[32, 0]);
        assert_eq!(bytes[18], 2);
        assert_eq!(BattleFinishedRecord::decode(&bytes), Ok(record));

        for result in [
            BattleFinishedResult::Member0Won,
            BattleFinishedResult::Member1Won,
            BattleFinishedResult::Draw,
            BattleFinishedResult::Won,
            BattleFinishedResult::Lost,
        ] {
            let record = BattleFinishedRecord { result, ..record };
            assert_eq!(
                BattleFinishedRecord::decode(&record.encode().unwrap()),
                Ok(record)
            );
        }

        let mut invalid = bytes.clone();
        invalid[16..18].fill(0);
        assert!(BattleFinishedRecord::decode(&invalid).is_err());
        invalid[16..18].copy_from_slice(&33_u16.to_le_bytes());
        assert!(BattleFinishedRecord::decode(&invalid).is_err());
        invalid[16..18].copy_from_slice(&1_u16.to_le_bytes());
        invalid[18] = 0;
        assert!(BattleFinishedRecord::decode(&invalid).is_err());
        invalid[18] = 6;
        assert!(BattleFinishedRecord::decode(&invalid).is_err());
        invalid[18] = 2;
        invalid[19..].fill(0);
        assert!(BattleFinishedRecord::decode(&invalid).is_err());
        invalid[19] = 1;
        invalid[0..16].fill(0);
        assert!(BattleFinishedRecord::decode(&invalid).is_err());
        assert!(BattleFinishedRecord::decode(&bytes[..50]).is_err());
        let mut extra = bytes;
        extra.push(0);
        assert!(BattleFinishedRecord::decode(&extra).is_err());
    }

    #[test]
    fn commit_records_have_one_strict_43_byte_layout() {
        let record = BattleCommitRecord {
            battle_id: id(),
            commit_id: BattleId([0xAA; 16]),
            trainer_region: RegionId::Hoenn,
            trainer_ordinal: 518,
            source_revision: 0x0102_0304_0506_0708,
        };
        let bytes = record.encode().unwrap();
        assert_eq!(bytes.len(), BATTLE_COMMIT_WIRE_SIZE);
        assert_eq!(&bytes[32..35], &[1, 0x06, 0x02]);
        assert_eq!(&bytes[35..], &0x0102_0304_0506_0708_u64.to_le_bytes());
        assert_eq!(BattleCommitRecord::decode(&bytes), Ok(record));
        assert_eq!(CommitAppliedRecord::decode(&bytes), Ok(record));

        for invalid in [
            BattleCommitRecord {
                battle_id: BattleId([0; 16]),
                ..record
            },
            BattleCommitRecord {
                commit_id: BattleId([0; 16]),
                ..record
            },
            BattleCommitRecord {
                trainer_region: RegionId::Unspecified,
                ..record
            },
            BattleCommitRecord {
                trainer_ordinal: 0,
                ..record
            },
            BattleCommitRecord {
                source_revision: 0,
                ..record
            },
        ] {
            assert!(invalid.encode().is_err());
        }
        assert!(BattleCommitRecord::decode(&bytes[..42]).is_err());
        let mut extra = bytes;
        extra.push(0);
        assert!(CommitAppliedRecord::decode(&extra).is_err());
        let mut unknown_region = record.encode().unwrap();
        unknown_region[32] = 9;
        assert!(BattleCommitRecord::decode(&unknown_region).is_err());
    }

    #[test]
    fn consent_records_have_exact_layout_and_reject_invalid_values() {
        let reserve = TrainerBattleReserveRecord {
            kind: BattleKind::CooperativeTrainer,
            request_nonce: 0x1234_5678,
            trainer_region: Some(RegionId::Hoenn),
            trainer_ordinal: Some(0x0234),
        };
        assert_eq!(
            reserve.encode().unwrap(),
            [1, 0x78, 0x56, 0x34, 0x12, 1, 0x34, 0x02]
        );
        assert_eq!(
            TrainerBattleReserveRecord::decode(&reserve.encode().unwrap()),
            Ok(reserve)
        );
        assert!(TrainerBattleReserveRecord::decode(&[3, 1, 0, 0, 0]).is_err());
        assert!(TrainerBattleReserveRecord::decode(&[1, 0, 0, 0, 0]).is_err());
        assert!(TrainerBattleReserveRecord::decode(&[1, 1, 0, 0, 0, 0]).is_err());
        assert!(TrainerBattleReserveRecord::decode(&[2, 1, 0, 0, 0, 1, 0, 0]).is_err());
        assert!(TrainerBattleReserveRecord::decode(&[1, 1, 0, 0, 0, 2, 0, 0]).is_ok());
        assert!(TrainerBattleReserveRecord::decode(&[1, 1, 0, 0, 0, 0, 0, 0]).is_err());
        let friendly = TrainerBattleReserveRecord {
            kind: BattleKind::Friendly,
            request_nonce: 42,
            trainer_region: None,
            trainer_ordinal: None,
        };
        assert_eq!(friendly.encode().unwrap(), [2, 42, 0, 0, 0]);
        assert_eq!(
            TrainerBattleReserveRecord::decode(&friendly.encode().unwrap()),
            Ok(friendly)
        );
        let rejected = BattleReserveRejectedRecord {
            request_nonce: 0x1234_5678,
        };
        assert_eq!(rejected.encode().unwrap(), [0x78, 0x56, 0x34, 0x12]);
        assert_eq!(
            BattleReserveRejectedRecord::decode(&[0x78, 0x56, 0x34, 0x12]),
            Ok(rejected)
        );
        assert!(BattleReserveRejectedRecord::decode(&[0; 4]).is_err());
        assert!(BattleReserveRejectedRecord::decode(&[1; 3]).is_err());

        let response = BattleJoinResponseRecord {
            battle_id: id(),
            decision: BattleDecision::Accept,
        };
        let response_bytes = response.encode().unwrap();
        assert_eq!(response_bytes.len(), 17);
        assert_eq!(response_bytes[16], 1);
        assert_eq!(
            BattleJoinResponseRecord::decode(&response_bytes),
            Ok(response)
        );
        let mut invalid = response_bytes.clone();
        invalid[16] = 2;
        assert!(BattleJoinResponseRecord::decode(&invalid).is_err());
        assert!(BattleJoinResponseRecord::decode(&[0; 17]).is_err());
        assert!(BattleJoinResponseRecord::decode(&response_bytes[..16]).is_err());

        let outcome = BattleConsentOutcomeRecord {
            battle_id: id(),
            request_nonce: 0x7856_3412,
            outcome: BattleConsentOutcome::Accepted,
        };
        let outcome_bytes = outcome.encode().unwrap();
        assert_eq!(outcome_bytes.len(), BattleConsentOutcomeRecord::WIRE_SIZE);
        assert_eq!(&outcome_bytes[16..20], &[0x12, 0x34, 0x56, 0x78]);
        assert_eq!(outcome_bytes[20], 1);
        assert_eq!(
            BattleConsentOutcomeRecord::decode(&outcome_bytes),
            Ok(outcome)
        );
        for value in [0, 4, 255] {
            let mut invalid = outcome_bytes.clone();
            invalid[20] = value;
            assert!(BattleConsentOutcomeRecord::decode(&invalid).is_err());
        }
        let mut invalid = outcome_bytes.clone();
        invalid[16..20].fill(0);
        assert!(BattleConsentOutcomeRecord::decode(&invalid).is_err());
        assert!(BattleConsentOutcomeRecord::decode(&outcome_bytes[..20]).is_err());

        let offer = BattleJoinOfferRecord {
            battle_id: id(),
            kind: BattleKind::Friendly,
            role: BattleRole::Responder,
            request_nonce: 0,
        };
        let offer_bytes = offer.encode().unwrap();
        assert_eq!(offer_bytes.len(), 22);
        assert_eq!(&offer_bytes[16..18], &[2, 1]);
        assert_eq!(&offer_bytes[18..22], &[0, 0, 0, 0]);
        assert_eq!(BattleJoinOfferRecord::decode(&offer_bytes), Ok(offer));
        let requester = BattleJoinOfferRecord {
            role: BattleRole::Requester,
            request_nonce: 0x1234_5678,
            ..offer
        };
        let requester_bytes = requester.encode().unwrap();
        assert_eq!(&requester_bytes[16..22], &[2, 0, 0x78, 0x56, 0x34, 0x12]);
        assert_eq!(
            BattleJoinOfferRecord::decode(&requester_bytes),
            Ok(requester)
        );
        assert!(
            BattleJoinOfferRecord {
                request_nonce: 0,
                ..requester
            }
            .encode()
            .is_err()
        );
        assert!(
            BattleJoinOfferRecord {
                request_nonce: 1,
                ..offer
            }
            .encode()
            .is_err()
        );
        let mut invalid = offer_bytes.clone();
        invalid[17] = 2;
        assert!(BattleJoinOfferRecord::decode(&invalid).is_err());
        invalid[17] = 1;
        invalid[16] = 0;
        assert!(BattleJoinOfferRecord::decode(&invalid).is_err());
        invalid = requester_bytes;
        invalid[18..22].fill(0);
        assert!(BattleJoinOfferRecord::decode(&invalid).is_err());
        assert!(BattleJoinOfferRecord::decode(&[0; 18]).is_err());
        assert!(BattleJoinOfferRecord::decode(&[0; 23]).is_err());
    }

    #[test]
    fn all_records_round_trip_and_reject_noncanonical_length() {
        let snapshot = PartySnapshotChunk {
            battle_id: id(),
            party_slot: 2,
            chunk_index: 2,
            chunk_count: 3,
            mon: vec![7; 100],
        };
        let action = ActionIntent {
            battle_id: id(),
            turn: 1,
            action: vec![1, 2],
        };
        let ready = BattleReadyRecord {
            battle_id: id(),
            party_digest: BattleDigest([9; 32]),
        };
        let start = BattleStartRecord { battle_id: id() };
        let hash = TurnResultHash {
            battle_id: id(),
            turn: 32,
            digest: BattleDigest([3; 32]),
        };
        let manifest = BattleManifestRecord {
            battle_id: id(),
            turn: 0,
            seed: BattleDigest([4; 32]),
            snapshot_hashes: [BattleDigest([5; 32]), BattleDigest([6; 32])],
            kind: BattleKind::CooperativeTrainer,
            local_member_slot: 1,
            trainer_region: RegionId::Hoenn,
            trainer_ordinal: 518,
        };
        let bundle = TurnBundleRecord {
            battle_id: id(),
            turn: 2,
            actions: [vec![1; 48], vec![2; 48]],
        };
        let pause = PauseForReconnectRecord {
            battle_id: id(),
            turn: 2,
            missing_slot: 1,
        };
        let abort = AbortBattleRecord {
            battle_id: id(),
            reason: 1,
        };
        macro_rules! round_trip {
            ($record:expr, $type:ty) => {{
                let bytes = $record.encode().unwrap();
                assert!(bytes.len() <= 128);
                assert_eq!(<$type>::decode(&bytes).unwrap(), $record);
                let mut padded = bytes;
                padded.push(0);
                assert!(<$type>::decode(&padded).is_err());
            }};
        }
        round_trip!(snapshot, PartySnapshotChunk);
        round_trip!(ready, BattleReadyRecord);
        round_trip!(start, BattleStartRecord);
        assert!(
            BattleReadyRecord {
                battle_id: BattleId([0; 16]),
                ..ready
            }
            .encode()
            .is_err()
        );
        assert!(BattleStartRecord::decode(&[0; 16]).is_err());
        let mut swapped = snapshot.encode().unwrap();
        swapped[16] = 1;
        assert!(PartySnapshotChunk::decode(&swapped).is_err());
        round_trip!(action, ActionIntent);
        round_trip!(hash, TurnResultHash);
        round_trip!(manifest, BattleManifestRecord);
        round_trip!(bundle, TurnBundleRecord);
        round_trip!(pause, PauseForReconnectRecord);
        round_trip!(abort, AbortBattleRecord);
        assert!(AbortBattleRecord { reason: 5, ..abort }.encode().is_ok());
        assert!(AbortBattleRecord { reason: 6, ..abort }.encode().is_err());
    }

    #[test]
    fn manifest_identity_bytes_are_exact_and_validated() {
        let mut record = BattleManifestRecord {
            battle_id: id(),
            turn: 0,
            seed: BattleDigest([1; 32]),
            snapshot_hashes: [BattleDigest([2; 32]), BattleDigest([3; 32])],
            kind: BattleKind::CooperativeTrainer,
            local_member_slot: 1,
            trainer_region: RegionId::Kanto,
            trainer_ordinal: 0x1234,
        };
        let bytes = record.encode().unwrap();
        assert_eq!(bytes.len(), 119);
        assert_eq!(&bytes[114..], &[1, 1, 2, 0x34, 0x12]);
        assert_eq!(BattleManifestRecord::decode(&bytes), Ok(record));
        assert_eq!(
            BattleManifestRecord::decode(&bytes[..114]),
            Err(BattleBridgeError::Length)
        );
        for (offset, value) in [(114, 3), (115, 2), (116, 9)] {
            let mut invalid = bytes.clone();
            invalid[offset] = value;
            assert_eq!(
                BattleManifestRecord::decode(&invalid),
                Err(BattleBridgeError::Value)
            );
        }
        record.kind = BattleKind::Friendly;
        assert_eq!(record.encode(), Err(BattleBridgeError::Value));
        record.trainer_region = RegionId::Unspecified;
        record.trainer_ordinal = 0;
        for slot in 0..=1 {
            record.local_member_slot = slot;
            assert_eq!(
                BattleManifestRecord::decode(&record.encode().unwrap()),
                Ok(record)
            );
        }
        let mut invalid = record.encode().unwrap();
        invalid[117] = 1;
        assert_eq!(
            BattleManifestRecord::decode(&invalid),
            Err(BattleBridgeError::Value)
        );
    }
}
