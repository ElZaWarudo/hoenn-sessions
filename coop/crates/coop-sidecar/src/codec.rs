use crc32fast::Hasher;
use thiserror::Error;

pub const BRIDGE_ABI_VERSION: u16 = 1;
pub const GAME_PROTOCOL_VERSION: u16 = 1;
pub const BRIDGE_PAYLOAD_SIZE: usize = 128;
pub const BRIDGE_FRAME_SIZE: usize = 144;
const CHECKSUM_OFFSET: usize = 140;
const HEADER_SIZE: usize = 12;

/// The producer of a bridge message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    RomToSidecar,
    SidecarToRom,
}

/// Message identifiers shared with `enum CoopBridgeMessageType` in the ROM.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum MessageType {
    RomReady = 0x0001,
    PlayerState = 0x0002,
    InteractRemotePlayer = 0x0003,
    GroupInviteRequest = 0x0004,
    TrainerBattleReserve = 0x0005,
    BattleJoinResponse = 0x0006,
    PartySnapshot = 0x0007,
    ActionIntent = 0x0008,
    TurnResultHash = 0x0009,
    BattleFinished = 0x000A,
    CommitApplied = 0x000B,
    CheckpointReady = 0x000C,
    SaveDataUpdated = 0x000D,
    SessionReady = 0x0100,
    RemotePlayerSpawn = 0x0101,
    RemotePlayerUpdate = 0x0102,
    RemotePlayerDespawn = 0x0103,
    GroupInviteReceived = 0x0104,
    GroupStateChanged = 0x0105,
    BattleJoinOffer = 0x0106,
    BattleManifest = 0x0107,
    TurnBundle = 0x0108,
    PauseForReconnect = 0x0109,
    BattleCommit = 0x010A,
    AbortBattle = 0x010B,
    CheckpointGranted = 0x010C,
}

impl MessageType {
    #[must_use]
    pub const fn direction(self) -> Direction {
        match self {
            Self::RomReady
            | Self::PlayerState
            | Self::InteractRemotePlayer
            | Self::GroupInviteRequest
            | Self::TrainerBattleReserve
            | Self::BattleJoinResponse
            | Self::PartySnapshot
            | Self::ActionIntent
            | Self::TurnResultHash
            | Self::BattleFinished
            | Self::CommitApplied
            | Self::CheckpointReady
            | Self::SaveDataUpdated => Direction::RomToSidecar,
            Self::SessionReady
            | Self::RemotePlayerSpawn
            | Self::RemotePlayerUpdate
            | Self::RemotePlayerDespawn
            | Self::GroupInviteReceived
            | Self::GroupStateChanged
            | Self::BattleJoinOffer
            | Self::BattleManifest
            | Self::TurnBundle
            | Self::PauseForReconnect
            | Self::BattleCommit
            | Self::AbortBattle
            | Self::CheckpointGranted => Direction::SidecarToRom,
        }
    }
}

impl TryFrom<u16> for MessageType {
    type Error = FrameCodecError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        let message_type = match value {
            0x0001 => Self::RomReady,
            0x0002 => Self::PlayerState,
            0x0003 => Self::InteractRemotePlayer,
            0x0004 => Self::GroupInviteRequest,
            0x0005 => Self::TrainerBattleReserve,
            0x0006 => Self::BattleJoinResponse,
            0x0007 => Self::PartySnapshot,
            0x0008 => Self::ActionIntent,
            0x0009 => Self::TurnResultHash,
            0x000A => Self::BattleFinished,
            0x000B => Self::CommitApplied,
            0x000C => Self::CheckpointReady,
            0x000D => Self::SaveDataUpdated,
            0x0100 => Self::SessionReady,
            0x0101 => Self::RemotePlayerSpawn,
            0x0102 => Self::RemotePlayerUpdate,
            0x0103 => Self::RemotePlayerDespawn,
            0x0104 => Self::GroupInviteReceived,
            0x0105 => Self::GroupStateChanged,
            0x0106 => Self::BattleJoinOffer,
            0x0107 => Self::BattleManifest,
            0x0108 => Self::TurnBundle,
            0x0109 => Self::PauseForReconnect,
            0x010A => Self::BattleCommit,
            0x010B => Self::AbortBattle,
            0x010C => Self::CheckpointGranted,
            _ => return Err(FrameCodecError::UnknownMessageType(value)),
        };
        Ok(message_type)
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum FrameCodecError {
    #[error("bridge frame must be exactly {BRIDGE_FRAME_SIZE} bytes, received {actual}")]
    IncorrectFrameLength { actual: usize },
    #[error("bridge payload cannot exceed {BRIDGE_PAYLOAD_SIZE} bytes, received {actual}")]
    PayloadTooLarge { actual: usize },
    #[error("bridge sequence zero is reserved and cannot be sent")]
    SequenceZero,
    #[error("unknown bridge message type 0x{0:04X}")]
    UnknownMessageType(u16),
    #[error("message {message_type:?} travels {actual:?}, not expected direction {expected:?}")]
    DirectionMismatch {
        message_type: MessageType,
        expected: Direction,
        actual: Direction,
    },
    #[error("bridge checksum mismatch: expected 0x{expected:08X}, received 0x{actual:08X}")]
    ChecksumMismatch { expected: u32, actual: u32 },
    #[error("bridge payload padding must be zero; byte {payload_offset} was non-zero")]
    NonZeroPadding { payload_offset: usize },
}

/// A validated bridge frame whose unused payload bytes are always zero.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgeFrame {
    message_type: MessageType,
    sequence: u32,
    session_epoch: u32,
    payload: [u8; BRIDGE_PAYLOAD_SIZE],
    payload_len: u16,
}

impl BridgeFrame {
    /// Builds a canonical frame and zero-fills its unused payload capacity.
    ///
    /// # Errors
    ///
    /// Returns an error when `sequence` is zero or `payload` exceeds 128 bytes.
    pub fn new(
        message_type: MessageType,
        sequence: u32,
        session_epoch: u32,
        payload: &[u8],
    ) -> Result<Self, FrameCodecError> {
        if sequence == 0 {
            return Err(FrameCodecError::SequenceZero);
        }
        if payload.len() > BRIDGE_PAYLOAD_SIZE {
            return Err(FrameCodecError::PayloadTooLarge {
                actual: payload.len(),
            });
        }

        let mut padded_payload = [0; BRIDGE_PAYLOAD_SIZE];
        padded_payload[..payload.len()].copy_from_slice(payload);
        let payload_len =
            u16::try_from(payload.len()).map_err(|_| FrameCodecError::PayloadTooLarge {
                actual: payload.len(),
            })?;

        Ok(Self {
            message_type,
            sequence,
            session_epoch,
            payload: padded_payload,
            payload_len,
        })
    }

    #[must_use]
    pub const fn message_type(&self) -> MessageType {
        self.message_type
    }

    #[must_use]
    pub const fn direction(&self) -> Direction {
        self.message_type.direction()
    }

    #[must_use]
    pub const fn sequence(&self) -> u32 {
        self.sequence
    }

    #[must_use]
    pub const fn session_epoch(&self) -> u32 {
        self.session_epoch
    }

    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload[..usize::from(self.payload_len)]
    }

    #[must_use]
    pub fn encode(&self) -> [u8; BRIDGE_FRAME_SIZE] {
        let mut bytes = [0; BRIDGE_FRAME_SIZE];
        bytes[0..2].copy_from_slice(&(self.message_type as u16).to_le_bytes());
        bytes[2..4].copy_from_slice(&self.payload_len.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.sequence.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.session_epoch.to_le_bytes());
        bytes[HEADER_SIZE..CHECKSUM_OFFSET].copy_from_slice(&self.payload);
        let checksum = crc32(&bytes[..CHECKSUM_OFFSET]);
        bytes[CHECKSUM_OFFSET..BRIDGE_FRAME_SIZE].copy_from_slice(&checksum.to_le_bytes());
        bytes
    }

    /// Decodes and validates one exact bridge frame.
    ///
    /// # Errors
    ///
    /// Returns an error for any size, checksum, type, sequence, payload, or padding violation.
    pub fn decode(bytes: &[u8]) -> Result<Self, FrameCodecError> {
        if bytes.len() != BRIDGE_FRAME_SIZE {
            return Err(FrameCodecError::IncorrectFrameLength {
                actual: bytes.len(),
            });
        }

        let message_type = u16::from_le_bytes([bytes[0], bytes[1]]);
        let payload_len = u16::from_le_bytes([bytes[2], bytes[3]]);
        let sequence = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        let session_epoch = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        let actual_checksum = u32::from_le_bytes([
            bytes[CHECKSUM_OFFSET],
            bytes[CHECKSUM_OFFSET + 1],
            bytes[CHECKSUM_OFFSET + 2],
            bytes[CHECKSUM_OFFSET + 3],
        ]);
        let expected_checksum = crc32(&bytes[..CHECKSUM_OFFSET]);
        if actual_checksum != expected_checksum {
            return Err(FrameCodecError::ChecksumMismatch {
                expected: expected_checksum,
                actual: actual_checksum,
            });
        }
        if sequence == 0 {
            return Err(FrameCodecError::SequenceZero);
        }

        let payload_len = usize::from(payload_len);
        if payload_len > BRIDGE_PAYLOAD_SIZE {
            return Err(FrameCodecError::PayloadTooLarge {
                actual: payload_len,
            });
        }
        let message_type = MessageType::try_from(message_type)?;

        let payload_bytes = &bytes[HEADER_SIZE..CHECKSUM_OFFSET];
        if let Some(relative_offset) = payload_bytes[payload_len..]
            .iter()
            .position(|byte| *byte != 0)
        {
            return Err(FrameCodecError::NonZeroPadding {
                payload_offset: payload_len + relative_offset,
            });
        }

        let mut payload = [0; BRIDGE_PAYLOAD_SIZE];
        payload.copy_from_slice(payload_bytes);
        Ok(Self {
            message_type,
            sequence,
            session_epoch,
            payload,
            payload_len: u16::try_from(payload_len).map_err(|_| {
                FrameCodecError::PayloadTooLarge {
                    actual: payload_len,
                }
            })?,
        })
    }

    /// Decodes a frame and enforces its producer direction.
    ///
    /// # Errors
    ///
    /// Returns any decoding error or a direction mismatch.
    pub fn decode_for(bytes: &[u8], expected: Direction) -> Result<Self, FrameCodecError> {
        let frame = Self::decode(bytes)?;
        frame.ensure_direction(expected)?;
        Ok(frame)
    }

    /// Confirms that this message type is legal in the expected direction.
    ///
    /// # Errors
    ///
    /// Returns [`FrameCodecError::DirectionMismatch`] when the producer is wrong.
    pub fn ensure_direction(&self, expected: Direction) -> Result<(), FrameCodecError> {
        let actual = self.direction();
        if actual != expected {
            return Err(FrameCodecError::DirectionMismatch {
                message_type: self.message_type,
                expected,
                actual,
            });
        }
        Ok(())
    }
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut hasher = Hasher::new();
    hasher.update(bytes);
    hasher.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codec_round_trip_preserves_fields_and_zero_fills_payload() {
        let frame = BridgeFrame::new(MessageType::PlayerState, 42, 7, &[1, 2, 3, 4]).unwrap();
        let bytes = frame.encode();

        assert_eq!(&bytes[16..CHECKSUM_OFFSET], &[0; BRIDGE_PAYLOAD_SIZE - 4]);
        assert_eq!(
            BridgeFrame::decode_for(&bytes, Direction::RomToSidecar).unwrap(),
            frame
        );
    }

    #[test]
    fn codec_rejects_tampering_and_non_zero_padding() {
        let frame = BridgeFrame::new(MessageType::PlayerState, 3, 9, &[0xAA]).unwrap();
        let mut tampered = frame.encode();
        tampered[HEADER_SIZE] ^= 1;
        assert!(matches!(
            BridgeFrame::decode(&tampered),
            Err(FrameCodecError::ChecksumMismatch { .. })
        ));

        let mut invalid_padding = frame.encode();
        invalid_padding[HEADER_SIZE + 1] = 0x55;
        let checksum = crc32(&invalid_padding[..CHECKSUM_OFFSET]);
        invalid_padding[CHECKSUM_OFFSET..].copy_from_slice(&checksum.to_le_bytes());
        assert_eq!(
            BridgeFrame::decode(&invalid_padding),
            Err(FrameCodecError::NonZeroPadding { payload_offset: 1 })
        );
    }

    #[test]
    fn codec_rejects_unknown_types_and_wrong_direction() {
        let mut unknown = BridgeFrame::new(MessageType::RomReady, 1, 0, &[])
            .unwrap()
            .encode();
        unknown[0..2].copy_from_slice(&0x0042_u16.to_le_bytes());
        let checksum = crc32(&unknown[..CHECKSUM_OFFSET]);
        unknown[CHECKSUM_OFFSET..].copy_from_slice(&checksum.to_le_bytes());
        assert_eq!(
            BridgeFrame::decode(&unknown),
            Err(FrameCodecError::UnknownMessageType(0x0042))
        );

        let rom_ready = BridgeFrame::new(MessageType::RomReady, 1, 0, &[])
            .unwrap()
            .encode();
        assert!(matches!(
            BridgeFrame::decode_for(&rom_ready, Direction::SidecarToRom),
            Err(FrameCodecError::DirectionMismatch {
                message_type: MessageType::RomReady,
                expected: Direction::SidecarToRom,
                actual: Direction::RomToSidecar,
            })
        ));
    }

    #[test]
    fn rom_ready_matches_the_c_abi_golden_vector() {
        const ROM_READY_CRC32: u32 = 0x9CEE_373D;

        let frame = BridgeFrame::new(MessageType::RomReady, 1, 0, &[]).unwrap();
        let bytes = frame.encode();
        let mut golden = [0; BRIDGE_FRAME_SIZE];
        golden[0..2].copy_from_slice(&1_u16.to_le_bytes());
        golden[2..4].copy_from_slice(&0_u16.to_le_bytes());
        golden[4..8].copy_from_slice(&1_u32.to_le_bytes());
        golden[8..12].copy_from_slice(&0_u32.to_le_bytes());
        golden[CHECKSUM_OFFSET..].copy_from_slice(&ROM_READY_CRC32.to_le_bytes());

        assert_eq!(bytes, golden);
        assert_eq!(
            u32::from_le_bytes(bytes[CHECKSUM_OFFSET..].try_into().unwrap()),
            ROM_READY_CRC32
        );
        assert_eq!(
            BridgeFrame::decode_for(&golden, Direction::RomToSidecar).unwrap(),
            frame
        );
    }
}
