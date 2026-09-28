//! Fixed, correlated Online menu records. Server identities never enter the ROM.
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const ONLINE_REQUEST_SIZE: usize = 12;
pub const ONLINE_STATUS_SIZE: usize = 128;
pub const PAIRING_RECORD_SIZE: usize = 12;
const PAIRING_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PairingAction {
    Create,
    Redeem,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PairingRequest {
    pub request_id: u32,
    pub action: PairingAction,
    pub code: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PairingResult {
    Created,
    Joined,
    Unavailable,
    Invalid,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PairingStatus {
    pub request_id: u32,
    pub result: PairingResult,
    pub code: String,
}

fn valid_pairing_code(code: &str) -> bool {
    let bytes = code.as_bytes();
    bytes.len() == 7
        && bytes[3] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 3 || PAIRING_ALPHABET.contains(b))
}

impl PairingRequest {
    pub fn encode(&self) -> Result<[u8; PAIRING_RECORD_SIZE], OnlineError> {
        if self.request_id == 0
            || (self.action == PairingAction::Redeem && !valid_pairing_code(&self.code))
            || (self.action == PairingAction::Create && !self.code.is_empty())
        {
            return Err(OnlineError);
        }
        let mut bytes = [0; PAIRING_RECORD_SIZE];
        bytes[..4].copy_from_slice(&self.request_id.to_le_bytes());
        bytes[4] = match self.action {
            PairingAction::Create => 0,
            PairingAction::Redeem => 1,
        };
        if self.action == PairingAction::Redeem {
            bytes[5..].copy_from_slice(self.code.as_bytes());
        }
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, OnlineError> {
        if bytes.len() != PAIRING_RECORD_SIZE {
            return Err(OnlineError);
        }
        let action = match bytes[4] {
            0 => PairingAction::Create,
            1 => PairingAction::Redeem,
            _ => return Err(OnlineError),
        };
        let code = if action == PairingAction::Create {
            if bytes[5..].iter().any(|b| *b != 0) {
                return Err(OnlineError);
            }
            String::new()
        } else {
            String::from_utf8(bytes[5..].to_vec()).map_err(|_| OnlineError)?
        };
        let value = Self {
            request_id: u32::from_le_bytes(bytes[..4].try_into().map_err(|_| OnlineError)?),
            action,
            code,
        };
        value.encode()?;
        Ok(value)
    }
}

impl PairingStatus {
    pub fn encode(&self) -> Result<[u8; PAIRING_RECORD_SIZE], OnlineError> {
        if self.request_id == 0
            || (self.result == PairingResult::Created && !valid_pairing_code(&self.code))
            || (self.result != PairingResult::Created && !self.code.is_empty())
        {
            return Err(OnlineError);
        }
        let mut bytes = [0; PAIRING_RECORD_SIZE];
        bytes[..4].copy_from_slice(&self.request_id.to_le_bytes());
        bytes[4] = match self.result {
            PairingResult::Created => 0,
            PairingResult::Joined => 1,
            PairingResult::Unavailable => 2,
            PairingResult::Invalid => 3,
        };
        if self.result == PairingResult::Created {
            bytes[5..].copy_from_slice(self.code.as_bytes());
        }
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, OnlineError> {
        if bytes.len() != PAIRING_RECORD_SIZE {
            return Err(OnlineError);
        }
        let result = match bytes[4] {
            0 => PairingResult::Created,
            1 => PairingResult::Joined,
            2 => PairingResult::Unavailable,
            3 => PairingResult::Invalid,
            _ => return Err(OnlineError),
        };
        let code = if result == PairingResult::Created {
            String::from_utf8(bytes[5..].to_vec()).map_err(|_| OnlineError)?
        } else {
            if bytes[5..].iter().any(|b| *b != 0) {
                return Err(OnlineError);
            }
            String::new()
        };
        let value = Self {
            request_id: u32::from_le_bytes(bytes[..4].try_into().map_err(|_| OnlineError)?),
            result,
            code,
        };
        value.encode()?;
        Ok(value)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum OnlineAction {
    Refresh = 0,
    Invite = 1,
    Accept = 2,
    Decline = 3,
    Leave = 4,
    Cancel = 5,
    InviteLastPartner = 6,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum OnlineResult {
    Ready = 0,
    Unavailable = 1,
    Success = 2,
    Stale = 3,
    Failed = 4,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OnlineRequest {
    pub request_id: u32,
    pub view_id: u32,
    pub action: OnlineAction,
    pub page: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OnlineStatus {
    pub request_id: u32,
    pub result: OnlineResult,
    pub flags: u8,
    pub nearby_count: u8,
    pub incoming_count: u8,
    pub nearby_page: u8,
    pub incoming_page: u8,
    pub outgoing_count: u8,
    pub outgoing_page: u8,
    /// Catalogued group zone coordinates, present when flags bit 4 is set.
    pub location_map_group: u16,
    pub location_map_number: u16,
    pub nearby_name: String,
    pub incoming_name: String,
    pub group_name: String,
    pub last_partner_name: String,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("invalid Online record")]
pub struct OnlineError;

impl OnlineRequest {
    /// # Errors
    /// Rejects zero correlation and out-of-range pages.
    pub fn encode(self) -> Result<[u8; ONLINE_REQUEST_SIZE], OnlineError> {
        if self.request_id == 0
            || self.page >= 32
            || ((self.action == OnlineAction::Refresh) != (self.view_id == 0))
        {
            return Err(OnlineError);
        }
        let mut bytes = [0; ONLINE_REQUEST_SIZE];
        bytes[..4].copy_from_slice(&self.request_id.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.view_id.to_le_bytes());
        bytes[8] = self.action as u8;
        bytes[9] = self.page;
        Ok(bytes)
    }
    /// # Errors
    /// Rejects noncanonical lengths, ordinals, padding, and values.
    pub fn decode(bytes: &[u8]) -> Result<Self, OnlineError> {
        if bytes.len() != ONLINE_REQUEST_SIZE || bytes[10..] != [0, 0] {
            return Err(OnlineError);
        }
        let value = Self {
            request_id: u32::from_le_bytes(bytes[..4].try_into().map_err(|_| OnlineError)?),
            view_id: u32::from_le_bytes(bytes[4..8].try_into().map_err(|_| OnlineError)?),
            action: match bytes[8] {
                0 => OnlineAction::Refresh,
                1 => OnlineAction::Invite,
                2 => OnlineAction::Accept,
                3 => OnlineAction::Decline,
                4 => OnlineAction::Leave,
                5 => OnlineAction::Cancel,
                6 => OnlineAction::InviteLastPartner,
                _ => return Err(OnlineError),
            },
            page: bytes[9],
        };
        value.encode()?;
        Ok(value)
    }
}

impl OnlineStatus {
    /// # Errors
    /// Rejects invalid paging, flags, correlation, or non-ASCII names.
    pub fn encode(&self) -> Result<[u8; ONLINE_STATUS_SIZE], OnlineError> {
        if self.request_id == 0
            || self.flags & !63 != 0
            || self.nearby_count > 32
            || self.incoming_count > 32
            || self.nearby_page >= self.nearby_count.max(1)
            || self.incoming_page >= self.incoming_count.max(1)
            || self.outgoing_count > 4
            || self.outgoing_page >= self.outgoing_count.max(1)
            || (self.flags & 1 != 0 && self.outgoing_count != 0)
            || (self.flags & 16 != 0 && self.flags & 1 == 0)
            || (self.flags & 16 == 0
                && (self.location_map_group != 0 || self.location_map_number != 0))
            || ((self.flags & 32 == 0) != self.last_partner_name.is_empty())
        {
            return Err(OnlineError);
        }
        let mut bytes = [0; ONLINE_STATUS_SIZE];
        bytes[..4].copy_from_slice(&self.request_id.to_le_bytes());
        bytes[4..10].copy_from_slice(&[
            self.result as u8,
            self.flags,
            self.nearby_count,
            self.incoming_count,
            self.nearby_page,
            self.incoming_page,
        ]);
        bytes[10] = self.outgoing_count;
        bytes[11] = self.outgoing_page;
        bytes[12..14].copy_from_slice(&self.location_map_group.to_le_bytes());
        bytes[14..16].copy_from_slice(&self.location_map_number.to_le_bytes());
        for (offset, name) in [
            (16, &self.nearby_name),
            (48, &self.incoming_name),
            (80, &self.group_name),
            (112, &self.last_partner_name),
        ] {
            let capacity = if offset == 112 { 16 } else { 32 };
            if name.len() > capacity || !name.bytes().all(|b| (32..=126).contains(&b)) {
                return Err(OnlineError);
            }
            bytes[offset..offset + name.len()].copy_from_slice(name.as_bytes());
        }
        Ok(bytes)
    }
    /// # Errors
    /// Rejects noncanonical lengths, padding, names, and values.
    pub fn decode(bytes: &[u8]) -> Result<Self, OnlineError> {
        if bytes.len() != ONLINE_STATUS_SIZE {
            return Err(OnlineError);
        }
        let name = |offset| -> Result<String, OnlineError> {
            let capacity = if offset == 112 { 16 } else { 32 };
            let field = &bytes[offset..offset + capacity];
            // A maximum-length username fills its slot. Short names retain
            // canonical zero padding; no on-wire terminator is required.
            let end = field.iter().position(|b| *b == 0).unwrap_or(field.len());
            if field[end..].iter().any(|b| *b != 0) {
                return Err(OnlineError);
            }
            String::from_utf8(field[..end].to_vec()).map_err(|_| OnlineError)
        };
        let value = Self {
            request_id: u32::from_le_bytes(bytes[..4].try_into().map_err(|_| OnlineError)?),
            result: match bytes[4] {
                0 => OnlineResult::Ready,
                1 => OnlineResult::Unavailable,
                2 => OnlineResult::Success,
                3 => OnlineResult::Stale,
                4 => OnlineResult::Failed,
                _ => return Err(OnlineError),
            },
            flags: bytes[5],
            nearby_count: bytes[6],
            incoming_count: bytes[7],
            nearby_page: bytes[8],
            incoming_page: bytes[9],
            outgoing_count: bytes[10],
            outgoing_page: bytes[11],
            location_map_group: u16::from_le_bytes(
                bytes[12..14].try_into().map_err(|_| OnlineError)?,
            ),
            location_map_number: u16::from_le_bytes(
                bytes[14..16].try_into().map_err(|_| OnlineError)?,
            ),
            nearby_name: name(16)?,
            incoming_name: name(48)?,
            group_name: name(80)?,
            last_partner_name: name(112)?,
        };
        value.encode()?;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pairing_records_validate_codes_and_padding() {
        let create = PairingRequest {
            request_id: 9,
            action: PairingAction::Create,
            code: String::new(),
        };
        assert_eq!(
            PairingRequest::decode(&create.encode().unwrap()).unwrap(),
            create
        );
        let redeem = PairingRequest {
            request_id: 10,
            action: PairingAction::Redeem,
            code: "HX7-4QK".into(),
        };
        assert_eq!(
            PairingRequest::decode(&redeem.encode().unwrap()).unwrap(),
            redeem
        );
        let created = PairingStatus {
            request_id: 9,
            result: PairingResult::Created,
            code: "HX7-4QK".into(),
        };
        assert_eq!(
            PairingStatus::decode(&created.encode().unwrap()).unwrap(),
            created
        );
        let mut invalid = create.encode().unwrap();
        invalid[11] = b'A';
        assert!(PairingRequest::decode(&invalid).is_err());
        invalid = redeem.encode().unwrap();
        invalid[5] = b'I';
        assert!(PairingRequest::decode(&invalid).is_err());
    }
    #[test]
    fn last_partner_slot_is_bounded_and_flagged() {
        let mut status = OnlineStatus {
            request_id: 1,
            result: OnlineResult::Ready,
            flags: 32,
            nearby_count: 0,
            incoming_count: 0,
            nearby_page: 0,
            incoming_page: 0,
            outgoing_count: 0,
            outgoing_page: 0,
            location_map_group: 0,
            location_map_number: 0,
            nearby_name: String::new(),
            incoming_name: String::new(),
            group_name: String::new(),
            last_partner_name: "SapphirePartner!".into(),
        };
        let bytes = status.encode().unwrap();
        assert_eq!(bytes.len(), 128);
        assert_eq!(OnlineStatus::decode(&bytes).unwrap(), status);
        status.flags = 0;
        assert!(status.encode().is_err());
    }
    #[test]
    fn request_fixture_and_reserved_bytes() {
        let request = OnlineRequest {
            request_id: 0x1234_5678,
            view_id: 1,
            action: OnlineAction::Accept,
            page: 3,
        };
        let mut bytes = [0x78, 0x56, 0x34, 0x12, 1, 0, 0, 0, 2, 3, 0, 0];
        assert_eq!(request.encode().unwrap(), bytes);
        assert_eq!(OnlineRequest::decode(&bytes).unwrap(), request);
        bytes[11] = 1;
        assert!(OnlineRequest::decode(&bytes).is_err());
        assert!(OnlineRequest::decode(&bytes[..7]).is_err());
        let cancel = OnlineRequest {
            action: OnlineAction::Cancel,
            ..request
        };
        assert_eq!(cancel.encode().unwrap()[8], 5);
        assert_eq!(
            OnlineRequest::decode(&cancel.encode().unwrap()).unwrap(),
            cancel
        );
    }
    #[test]
    fn status_fixture_rejects_noncanonical_names_and_padding() {
        let status = OnlineStatus {
            request_id: 1,
            result: OnlineResult::Ready,
            flags: 7,
            nearby_count: 2,
            incoming_count: 1,
            nearby_page: 1,
            incoming_page: 0,
            outgoing_count: 0,
            outgoing_page: 0,
            location_map_group: 0,
            location_map_number: 0,
            nearby_name: "A".into(),
            incoming_name: "B".into(),
            group_name: "C".into(),
            last_partner_name: String::new(),
        };
        let mut bytes = status.encode().unwrap();
        assert_eq!(
            &bytes[..16],
            &[1, 0, 0, 0, 0, 7, 2, 1, 1, 0, 0, 0, 0, 0, 0, 0]
        );
        assert_eq!((bytes[16], bytes[48], bytes[80]), (b'A', b'B', b'C'));
        assert_eq!(OnlineStatus::decode(&bytes).unwrap(), status);
        bytes[18] = b'X';
        assert!(OnlineStatus::decode(&bytes).is_err());
        bytes = status.encode().unwrap();
        bytes[12] = 1;
        assert!(OnlineStatus::decode(&bytes).is_err());
    }

    #[test]
    fn status_preserves_full_32_byte_names_and_rejects_33_bytes() {
        let mut status = OnlineStatus {
            request_id: 1,
            result: OnlineResult::Ready,
            flags: 7,
            nearby_count: 1,
            incoming_count: 1,
            nearby_page: 0,
            incoming_page: 0,
            outgoing_count: 0,
            outgoing_page: 0,
            location_map_group: 0,
            location_map_number: 0,
            nearby_name: "a".repeat(32),
            incoming_name: "b".repeat(32),
            group_name: "c".repeat(32),
            last_partner_name: String::new(),
        };
        let bytes = status.encode().unwrap();
        assert_eq!(&bytes[16..48], &[b'a'; 32]);
        assert_eq!(&bytes[48..80], &[b'b'; 32]);
        assert_eq!(&bytes[80..112], &[b'c'; 32]);
        assert_eq!(OnlineStatus::decode(&bytes).unwrap(), status);
        status.nearby_name.push('a');
        assert!(status.encode().is_err());
    }

    #[test]
    fn status_outgoing_slot_is_bounded_and_cannot_overlap_group() {
        let mut status = OnlineStatus {
            request_id: 1,
            result: OnlineResult::Ready,
            flags: 8,
            nearby_count: 0,
            incoming_count: 0,
            nearby_page: 0,
            incoming_page: 0,
            outgoing_count: 1,
            outgoing_page: 0,
            location_map_group: 0,
            location_map_number: 0,
            nearby_name: String::new(),
            incoming_name: String::new(),
            group_name: "may".into(),
            last_partner_name: String::new(),
        };
        let bytes = status.encode().unwrap();
        assert_eq!(bytes[10..12], [1, 0]);
        assert_eq!(OnlineStatus::decode(&bytes).unwrap(), status);
        status.flags |= 1;
        assert!(status.encode().is_err());
        status.flags = 8;
        status.outgoing_page = 1;
        assert!(status.encode().is_err());
    }

    #[test]
    fn status_location_requires_group_and_explicit_presence() {
        let mut status = OnlineStatus {
            request_id: 1,
            result: OnlineResult::Ready,
            flags: 17,
            nearby_count: 0,
            incoming_count: 0,
            nearby_page: 0,
            incoming_page: 0,
            outgoing_count: 0,
            outgoing_page: 0,
            location_map_group: 37,
            location_map_number: 12,
            nearby_name: String::new(),
            incoming_name: String::new(),
            group_name: "may".into(),
            last_partner_name: String::new(),
        };
        let bytes = status.encode().unwrap();
        assert_eq!(bytes[12..16], [37, 0, 12, 0]);
        assert_eq!(OnlineStatus::decode(&bytes).unwrap(), status);
        status.flags = 1;
        assert!(status.encode().is_err());
        status.flags = 16;
        assert!(status.encode().is_err());
    }
}
