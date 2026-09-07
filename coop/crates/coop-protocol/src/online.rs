//! Fixed, correlated Online menu records. Server identities never enter the ROM.
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const ONLINE_REQUEST_SIZE: usize = 12;
pub const ONLINE_STATUS_SIZE: usize = 112;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum OnlineAction {
    Refresh = 0,
    Invite = 1,
    Accept = 2,
    Decline = 3,
    Leave = 4,
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
    pub nearby_name: String,
    pub incoming_name: String,
    pub group_name: String,
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
            || self.flags & !7 != 0
            || self.nearby_count > 32
            || self.incoming_count > 32
            || self.nearby_page >= self.nearby_count.max(1)
            || self.incoming_page >= self.incoming_count.max(1)
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
        for (offset, name) in [
            (16, &self.nearby_name),
            (48, &self.incoming_name),
            (80, &self.group_name),
        ] {
            if name.len() > 32 || !name.bytes().all(|b| (32..=126).contains(&b)) {
                return Err(OnlineError);
            }
            bytes[offset..offset + name.len()].copy_from_slice(name.as_bytes());
        }
        Ok(bytes)
    }
    /// # Errors
    /// Rejects noncanonical lengths, padding, names, and values.
    pub fn decode(bytes: &[u8]) -> Result<Self, OnlineError> {
        if bytes.len() != ONLINE_STATUS_SIZE || bytes[10..16] != [0; 6] {
            return Err(OnlineError);
        }
        let name = |offset| -> Result<String, OnlineError> {
            let field = &bytes[offset..offset + 32];
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
            nearby_name: name(16)?,
            incoming_name: name(48)?,
            group_name: name(80)?,
        };
        value.encode()?;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
            nearby_name: "A".into(),
            incoming_name: "B".into(),
            group_name: "C".into(),
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
        bytes[10] = 1;
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
            nearby_name: "a".repeat(32),
            incoming_name: "b".repeat(32),
            group_name: "c".repeat(32),
        };
        let bytes = status.encode().unwrap();
        assert_eq!(&bytes[16..48], &[b'a'; 32]);
        assert_eq!(&bytes[48..80], &[b'b'; 32]);
        assert_eq!(&bytes[80..112], &[b'c'; 32]);
        assert_eq!(OnlineStatus::decode(&bytes).unwrap(), status);
        status.nearby_name.push('a');
        assert!(status.encode().is_err());
    }
}
