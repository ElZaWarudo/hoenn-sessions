use crc32fast::Hasher;
use thiserror::Error;

pub const BRIDGE_ABI_VERSION: u16 = 1;
pub const GAME_PROTOCOL_VERSION: u16 = 5;
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
    OnlineRequest = 0x000E,
    GroupTravelClient = 0x000F,
    CompanionState = 0x0010,
    SocialSignal = 0x0011,
    PairingRequest = 0x0012,
    ProgressObservation = 0x0013,
    BattleAbortRequest = 0x0014,
    BattleReady = 0x0015,
    /// The ROM offers a party Pokémon to its partner, or withdraws the offer
    /// (`coop_protocol::TradeOfferRequestRecord`, 16 bytes).
    TradeOfferRequest = 0x0016,
    /// The partner's answer to a received offer
    /// (`coop_protocol::TradeOfferDecisionRecord`, 16 bytes).
    TradeOfferDecision = 0x0017,
    PortalTravelRequest = 0x0018,
    ArrivalProof = 0x0019,
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
    OnlineStatus = 0x010D,
    GroupTravelServer = 0x010E,
    RemoteCompanion = 0x010F,
    RemoteSocialSignal = 0x0110,
    RemoteInteraction = 0x0111,
    ProgressEvent = 0x0113,
    PairingStatus = 0x0112,
    PeerPartyChunk = 0x0114,
    BattleConsentOutcome = 0x0115,
    BattleReserveRejected = 0x0116,
    BattleStart = 0x0117,
    GroupEnded = 0x0118,
    /// A server-issued trade outcome. The payload is exactly the 128-byte
    /// `coop_protocol::TradeCommitRecord` layout; the ROM acknowledges it with
    /// a 28-byte `CommitApplied` (`coop_protocol::TradeCommitAppliedRecord`).
    TradeCommit = 0x0119,
    /// The partner offers a Pokémon (`coop_protocol::TradeOfferReceivedRecord`,
    /// 20 bytes).
    TradeOfferReceived = 0x011A,
    /// Where an offer stands (`coop_protocol::TradeOfferStatusRecord`, 12
    /// bytes).
    TradeOfferStatus = 0x011B,
    ArrivalChallenge = 0x011C,
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
            | Self::SaveDataUpdated
            | Self::OnlineRequest
            | Self::GroupTravelClient
            | Self::CompanionState
            | Self::SocialSignal
            | Self::PortalTravelRequest
            | Self::ArrivalProof
            | Self::ProgressObservation
            | Self::BattleAbortRequest
            | Self::BattleReady
            | Self::TradeOfferRequest
            | Self::TradeOfferDecision => Direction::RomToSidecar,
            Self::PairingRequest => Direction::RomToSidecar,
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
            | Self::CheckpointGranted
            | Self::OnlineStatus
            | Self::GroupTravelServer
            | Self::RemoteCompanion
            | Self::RemoteSocialSignal
            | Self::ArrivalChallenge
            | Self::RemoteInteraction => Direction::SidecarToRom,
            Self::ProgressEvent => Direction::SidecarToRom,
            Self::PairingStatus => Direction::SidecarToRom,
            Self::PeerPartyChunk => Direction::SidecarToRom,
            Self::BattleConsentOutcome => Direction::SidecarToRom,
            Self::BattleReserveRejected
            | Self::BattleStart
            | Self::GroupEnded
            | Self::TradeCommit
            | Self::TradeOfferReceived
            | Self::TradeOfferStatus => Direction::SidecarToRom,
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
            0x000E => Self::OnlineRequest,
            0x000F => Self::GroupTravelClient,
            0x0010 => Self::CompanionState,
            0x0011 => Self::SocialSignal,
            0x0012 => Self::PairingRequest,
            0x0013 => Self::ProgressObservation,
            0x0014 => Self::BattleAbortRequest,
            0x0015 => Self::BattleReady,
            0x0016 => Self::TradeOfferRequest,
            0x0017 => Self::TradeOfferDecision,
            0x0018 => Self::PortalTravelRequest,
            0x0019 => Self::ArrivalProof,
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
            0x010D => Self::OnlineStatus,
            0x010E => Self::GroupTravelServer,
            0x010F => Self::RemoteCompanion,
            0x0110 => Self::RemoteSocialSignal,
            0x0111 => Self::RemoteInteraction,
            0x0113 => Self::ProgressEvent,
            0x0112 => Self::PairingStatus,
            0x0114 => Self::PeerPartyChunk,
            0x0115 => Self::BattleConsentOutcome,
            0x0116 => Self::BattleReserveRejected,
            0x0117 => Self::BattleStart,
            0x0118 => Self::GroupEnded,
            0x0119 => Self::TradeCommit,
            0x011A => Self::TradeOfferReceived,
            0x011B => Self::TradeOfferStatus,
            0x011C => Self::ArrivalChallenge,
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

    #[test]
    fn social_message_types_preserve_direction() {
        let companion = BridgeFrame::new(MessageType::CompanionState, 5, 9, &[0_u8; 8]).unwrap();
        let signal = BridgeFrame::new(MessageType::SocialSignal, 6, 9, &[0_u8; 12]).unwrap();
        let progress =
            BridgeFrame::new(MessageType::ProgressObservation, 9, 9, &[1, 1, 2, 0]).unwrap();
        let remote_companion =
            BridgeFrame::new(MessageType::RemoteCompanion, 7, 9, &[0_u8; 16]).unwrap();
        let remote_signal =
            BridgeFrame::new(MessageType::RemoteSocialSignal, 8, 9, &[0_u8; 20]).unwrap();
        assert_eq!(companion.direction(), Direction::RomToSidecar);
        assert_eq!(signal.direction(), Direction::RomToSidecar);
        assert_eq!(progress.direction(), Direction::RomToSidecar);
        assert_eq!(progress.payload(), &[1, 1, 2, 0]);
        assert_eq!(remote_companion.direction(), Direction::SidecarToRom);
        assert_eq!(remote_signal.direction(), Direction::SidecarToRom);
        assert!(companion.ensure_direction(Direction::SidecarToRom).is_err());
        assert!(
            remote_signal
                .ensure_direction(Direction::RomToSidecar)
                .is_err()
        );
    }

    #[test]
    fn portal_request_frame_preserves_id_and_rom_direction() {
        let frame =
            BridgeFrame::new(MessageType::PortalTravelRequest, 6, 9, b"to_cormoria").unwrap();
        assert_eq!(frame.direction(), Direction::RomToSidecar);
        assert_eq!(frame.payload(), b"to_cormoria");
        assert_eq!(
            BridgeFrame::decode_for(&frame.encode(), Direction::RomToSidecar).unwrap(),
            frame
        );
        assert!(BridgeFrame::decode_for(&frame.encode(), Direction::SidecarToRom).is_err());
    }

    #[test]
    fn protocol_five_travel_extensions_do_not_alias_pairing_or_progress() {
        for (wire, kind, direction) in [
            (0x0012, MessageType::PairingRequest, Direction::RomToSidecar),
            (0x0013, MessageType::ProgressObservation, Direction::RomToSidecar),
            (0x0018, MessageType::PortalTravelRequest, Direction::RomToSidecar),
            (0x0019, MessageType::ArrivalProof, Direction::RomToSidecar),
            (0x0111, MessageType::RemoteInteraction, Direction::SidecarToRom),
            (0x011C, MessageType::ArrivalChallenge, Direction::SidecarToRom),
        ] {
            assert_eq!(MessageType::try_from(wire), Ok(kind));
            assert_eq!(kind as u16, wire);
            let frame = BridgeFrame::new(kind, 1, 0, &[]).unwrap();
            assert_eq!(BridgeFrame::decode_for(&frame.encode(), direction).unwrap(), frame);
        }
        assert!(MessageType::try_from(0x001A).is_err());
        assert!(MessageType::try_from(0x011D).is_err());
    }

    #[test]
    fn group_travel_message_types_preserve_direction_and_payload_size() {
        let payload = [0_u8; coop_protocol::GROUP_TRAVEL_RECORD_SIZE];
        let client = BridgeFrame::new(MessageType::GroupTravelClient, 3, 9, &payload).unwrap();
        let server = BridgeFrame::new(MessageType::GroupTravelServer, 4, 9, &payload).unwrap();
        assert_eq!(client.direction(), Direction::RomToSidecar);
        assert_eq!(server.direction(), Direction::SidecarToRom);
        assert_eq!(client.payload().len(), 32);
        assert!(client.ensure_direction(Direction::SidecarToRom).is_err());
        assert!(server.ensure_direction(Direction::RomToSidecar).is_err());
    }

    #[test]
    fn battle_message_types_have_strict_direction_and_fit_frame() {
        use coop_protocol::{
            BattleDigest, BattleId, BattleManifestRecord, BattleReadyRecord, BattleStartRecord,
            TurnBundleRecord,
        };
        let id = BattleId([7; 16]);
        let manifest = BattleManifestRecord {
            friendly_rules: Some(coop_protocol::FriendlyBattleRules {
                format: coop_protocol::FriendlyBattleFormat::Singles,
                level_mode: coop_protocol::FriendlyLevelMode::AsIs,
                team_size: 1,
            }),
            battle_id: id,
            turn: 0,
            seed: BattleDigest([1; 32]),
            snapshot_hashes: [BattleDigest([2; 32]), BattleDigest([3; 32])],
            kind: coop_protocol::BattleKind::Friendly,
            local_member_slot: 0,
            trainer_region: coop_protocol::RegionId::Unspecified,
            trainer_ordinal: 0,
        };
        let bundle = TurnBundleRecord {
            battle_id: id,
            turn: 1,
            actions: [vec![1; 48], vec![2; 48]],
        };
        for (message_type, payload) in [
            (MessageType::BattleManifest, manifest.encode().unwrap()),
            (MessageType::TurnBundle, bundle.encode().unwrap()),
            (
                MessageType::BattleStart,
                BattleStartRecord { battle_id: id }.encode().unwrap(),
            ),
        ] {
            let frame = BridgeFrame::new(message_type, 1, 9, &payload).unwrap();
            assert_eq!(frame.direction(), Direction::SidecarToRom);
            assert!(frame.ensure_direction(Direction::RomToSidecar).is_err());
            assert_eq!(
                BridgeFrame::decode_for(&frame.encode(), Direction::SidecarToRom)
                    .unwrap()
                    .payload(),
                payload
            );
        }
        for message_type in [
            MessageType::PartySnapshot,
            MessageType::ActionIntent,
            MessageType::TurnResultHash,
            MessageType::BattleReady,
        ] {
            assert_eq!(message_type.direction(), Direction::RomToSidecar);
        }
        let ready = BattleReadyRecord {
            battle_id: id,
            party_digest: BattleDigest([4; 32]),
        };
        let payload = ready.encode().unwrap();
        let frame = BridgeFrame::new(MessageType::BattleReady, 2, 9, &payload).unwrap();
        assert_eq!(
            BridgeFrame::decode_for(&frame.encode(), Direction::RomToSidecar)
                .unwrap()
                .payload(),
            payload
        );
    }

    #[test]
    fn peer_party_chunk_has_server_direction_and_exact_wire_length() {
        use coop_protocol::{BattleId, PartySnapshotChunk};
        let chunk = PartySnapshotChunk {
            battle_id: BattleId([7; 16]),
            party_slot: 0,
            chunk_index: 0,
            chunk_count: 1,
            mon: vec![0xAB; 100],
        };
        let payload = chunk.encode().unwrap();
        assert_eq!(payload.len(), 120);
        let frame = BridgeFrame::new(MessageType::PeerPartyChunk, 4, 9, &payload).unwrap();
        assert_eq!(frame.direction(), Direction::SidecarToRom);
        assert!(frame.ensure_direction(Direction::RomToSidecar).is_err());
        assert_eq!(
            BridgeFrame::decode_for(&frame.encode(), Direction::SidecarToRom)
                .unwrap()
                .payload(),
            payload
        );
    }

    fn trade_commit_fixture() -> coop_protocol::TradeCommitRecord {
        let mut incoming_record = [0_u8; 100];
        for (index, byte) in incoming_record.iter_mut().enumerate() {
            *byte = u8::try_from(index).unwrap() ^ 0x5A;
        }
        coop_protocol::TradeCommitRecord {
            commit_id: coop_protocol::BattleId(
                *b"\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\x10",
            ),
            slot: 4,
            outgoing_personality: 0xDEAD_BEEF,
            outgoing_ot_id: 0x0102_0304,
            incoming_record,
        }
    }

    #[test]
    fn trade_commit_fills_the_payload_with_the_documented_layout() {
        let record = trade_commit_fixture();
        let payload = record.encode().unwrap();
        assert_eq!(payload.len(), BRIDGE_PAYLOAD_SIZE);
        assert_eq!(coop_protocol::TradeCommitRecord::WIRE_SIZE, 128);
        assert_eq!(&payload[0..16], &record.commit_id.0);
        assert_eq!(payload[16], 4);
        assert_eq!(&payload[17..20], &[0, 0, 0]);
        assert_eq!(&payload[20..24], &[0xEF, 0xBE, 0xAD, 0xDE]);
        assert_eq!(&payload[24..28], &[0x04, 0x03, 0x02, 0x01]);
        assert_eq!(&payload[28..128], &record.incoming_record);

        let frame = BridgeFrame::new(MessageType::TradeCommit, 11, 9, &payload).unwrap();
        let bytes = frame.encode();
        assert_eq!(&bytes[0..2], &0x0119_u16.to_le_bytes());
        assert_eq!(&bytes[2..4], &128_u16.to_le_bytes());
        assert_eq!(frame.direction(), Direction::SidecarToRom);
        assert!(frame.ensure_direction(Direction::RomToSidecar).is_err());
        let decoded = BridgeFrame::decode_for(&bytes, Direction::SidecarToRom).unwrap();
        assert_eq!(
            coop_protocol::TradeCommitRecord::decode(decoded.payload()),
            Ok(record)
        );
        assert_eq!(MessageType::try_from(0x0119), Ok(MessageType::TradeCommit));
    }

    #[test]
    fn trade_commit_rejects_invalid_records() {
        use coop_protocol::{BattleBridgeError, TradeCommitRecord};
        let record = trade_commit_fixture();
        let payload = record.encode().unwrap();
        for (offset, value) in [(16, 6_u8), (17, 1), (19, 1)] {
            let mut bad = payload.clone();
            bad[offset] = value;
            assert_eq!(
                TradeCommitRecord::decode(&bad),
                Err(BattleBridgeError::Value)
            );
        }
        let mut zero_id = payload.clone();
        zero_id[0..16].fill(0);
        assert_eq!(
            TradeCommitRecord::decode(&zero_id),
            Err(BattleBridgeError::Value)
        );
        let mut empty_mon = payload.clone();
        empty_mon[28..].fill(0);
        assert_eq!(
            TradeCommitRecord::decode(&empty_mon),
            Err(BattleBridgeError::Value)
        );
        assert_eq!(
            TradeCommitRecord::decode(&payload[..127]),
            Err(BattleBridgeError::Length)
        );
        let json = serde_json::to_value(record).unwrap();
        assert_eq!(json["incoming_record"].as_str().unwrap().len(), 200);
        assert_eq!(
            serde_json::from_value::<TradeCommitRecord>(json).unwrap(),
            record
        );
    }

    #[test]
    fn trade_offer_messages_have_fixed_types_directions_and_single_frames() {
        use coop_protocol::{
            TradeOfferAction, TradeOfferDecision, TradeOfferDecisionRecord, TradeOfferOutcome,
            TradeOfferReceivedRecord, TradeOfferRequestRecord, TradeOfferRole,
            TradeOfferStatusRecord,
        };
        let request = TradeOfferRequestRecord {
            action: TradeOfferAction::Offer,
            slot: 1,
            request_id: 7,
            personality: 0x0BAD_F00D,
            ot_id: 0x2222_4444,
        };
        let decision = TradeOfferDecisionRecord {
            decision: TradeOfferDecision::Accept,
            slot: 0,
            offer_token: 9,
            personality: 1,
            ot_id: 2,
        };
        let received = TradeOfferReceivedRecord {
            offer_token: 9,
            species: 263,
            level: 4,
            is_egg: false,
            nickname: [0xFF; 10],
        };
        let status = TradeOfferStatusRecord {
            role: TradeOfferRole::Requester,
            outcome: TradeOfferOutcome::Pending,
            request_id: 7,
            offer_token: 9,
        };
        for (message_type, wire, direction, payload) in [
            (
                MessageType::TradeOfferRequest,
                0x0016_u16,
                Direction::RomToSidecar,
                request.encode().unwrap(),
            ),
            (
                MessageType::TradeOfferDecision,
                0x0017,
                Direction::RomToSidecar,
                decision.encode().unwrap(),
            ),
            (
                MessageType::TradeOfferReceived,
                0x011A,
                Direction::SidecarToRom,
                received.encode().unwrap(),
            ),
            (
                MessageType::TradeOfferStatus,
                0x011B,
                Direction::SidecarToRom,
                status.encode().unwrap(),
            ),
        ] {
            assert_eq!(MessageType::try_from(wire), Ok(message_type));
            assert_eq!(message_type as u16, wire);
            assert_eq!(message_type.direction(), direction);
            let frame = BridgeFrame::new(message_type, 3, 9, &payload).unwrap();
            let bytes = frame.encode();
            assert_eq!(bytes.len(), BRIDGE_FRAME_SIZE);
            assert_eq!(&bytes[0..2], &wire.to_le_bytes());
            assert!(BridgeFrame::decode_for(&bytes, direction).is_ok());
            let other = if direction == Direction::RomToSidecar {
                Direction::SidecarToRom
            } else {
                Direction::RomToSidecar
            };
            assert!(BridgeFrame::decode_for(&bytes, other).is_err());
        }
        assert_eq!(
            TradeOfferRequestRecord::decode(&request.encode().unwrap()),
            Ok(request)
        );
        assert_eq!(
            TradeOfferStatusRecord::decode(&status.encode().unwrap()),
            Ok(status)
        );
        assert!(MessageType::try_from(0x001A).is_err());
        assert!(MessageType::try_from(0x011D).is_err());
    }

    #[test]
    fn trade_commit_ack_is_the_28_byte_header_on_commit_applied() {
        use coop_protocol::{BattleCommitRecord, TradeCommitAppliedRecord};
        let record = trade_commit_fixture();
        let ack = record.applied();
        let payload = ack.encode().unwrap();
        assert_eq!(payload.len(), TradeCommitAppliedRecord::WIRE_SIZE);
        assert_eq!(payload.len(), 28);
        assert_eq!(payload, record.encode().unwrap()[..28]);
        assert_ne!(payload.len(), BattleCommitRecord::WIRE_SIZE);
        let frame = BridgeFrame::new(MessageType::CommitApplied, 5, 9, &payload).unwrap();
        let decoded = BridgeFrame::decode_for(&frame.encode(), Direction::RomToSidecar).unwrap();
        assert_eq!(TradeCommitAppliedRecord::decode(decoded.payload()), Ok(ack));
        assert!(BattleCommitRecord::decode(decoded.payload()).is_err());
    }

    #[test]
    fn battle_consent_message_types_preserve_direction_and_exact_payloads() {
        use coop_protocol::{
            BattleConsentOutcome, BattleConsentOutcomeRecord, BattleDecision, BattleId,
            BattleJoinOfferRecord, BattleJoinResponseRecord, BattleKind,
            BattleReserveRejectedRecord, BattleRole, TrainerBattleReserveRecord,
        };
        let id = BattleId([7; 16]);
        let reserve = TrainerBattleReserveRecord {
            friendly_rules: Some(coop_protocol::FriendlyBattleRules {
                format: coop_protocol::FriendlyBattleFormat::Singles,
                level_mode: coop_protocol::FriendlyLevelMode::AsIs,
                team_size: 1,
            }),
            kind: BattleKind::Friendly,
            request_nonce: 42,
            trainer_region: None,
            trainer_ordinal: None,
        };
        let response = BattleJoinResponseRecord {
            battle_id: id,
            decision: BattleDecision::Accept,
        };
        let offer = BattleJoinOfferRecord {
            friendly_rules: Some(coop_protocol::FriendlyBattleRules {
                format: coop_protocol::FriendlyBattleFormat::Singles,
                level_mode: coop_protocol::FriendlyLevelMode::AsIs,
                team_size: 1,
            }),
            battle_id: id,
            kind: BattleKind::Friendly,
            role: BattleRole::Responder,
            request_nonce: 0,
        };
        for (kind, payload) in [
            (MessageType::TrainerBattleReserve, reserve.encode().unwrap()),
            (MessageType::BattleJoinResponse, response.encode().unwrap()),
        ] {
            let frame = BridgeFrame::new(kind, 2, 9, &payload).unwrap();
            assert_eq!(frame.direction(), Direction::RomToSidecar);
            assert!(frame.ensure_direction(Direction::SidecarToRom).is_err());
        }
        let payload = offer.encode().unwrap();
        let frame = BridgeFrame::new(MessageType::BattleJoinOffer, 3, 9, &payload).unwrap();
        assert_eq!(frame.direction(), Direction::SidecarToRom);
        assert_eq!(frame.payload().len(), BattleJoinOfferRecord::WIRE_SIZE);
        assert_eq!(frame.payload().len(), 25);
        assert!(frame.ensure_direction(Direction::RomToSidecar).is_err());

        let outcome = BattleConsentOutcomeRecord {
            battle_id: id,
            request_nonce: 42,
            outcome: BattleConsentOutcome::Accepted,
        };
        let payload = outcome.encode().unwrap();
        let frame = BridgeFrame::new(MessageType::BattleConsentOutcome, 4, 9, &payload).unwrap();
        assert_eq!(frame.direction(), Direction::SidecarToRom);
        assert_eq!(frame.payload().len(), BattleConsentOutcomeRecord::WIRE_SIZE);
        assert_eq!(
            BridgeFrame::decode_for(&frame.encode(), Direction::SidecarToRom)
                .unwrap()
                .payload(),
            payload
        );

        let rejected = BattleReserveRejectedRecord { request_nonce: 42 };
        let payload = rejected.encode().unwrap();
        let frame = BridgeFrame::new(MessageType::BattleReserveRejected, 5, 9, &payload).unwrap();
        assert_eq!(frame.direction(), Direction::SidecarToRom);
        assert!(frame.ensure_direction(Direction::RomToSidecar).is_err());
        assert_eq!(
            BattleReserveRejectedRecord::decode(frame.payload()),
            Ok(rejected)
        );
    }
}
