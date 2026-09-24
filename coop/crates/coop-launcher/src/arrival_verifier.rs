//! Offline proof that a destination ROM loaded the staged V2 save.
//!
//! This module does not change the travel journal or acquire a cloud lease.
//! The caller supplies the nonce and full-save digest from durable handoff
//! state; a successful result is evidence for a later coordinator decision.

use std::{path::Path, time::Duration};

use coop_cloud::Sha256Digest;
use coop_protocol::RomWorldId;
use coop_save::{RegistryContract, SaveV2Error, parse_v2};
use coop_sidecar::control::{ArrivalProof, ControlCommand, ControlEvent};
use thiserror::Error;
use tokio::time::{Instant, timeout_at};

use crate::{
    process::{CommandSpec, ProcessError, SupervisedChildren, SupervisorEvent},
    session::{SessionError, SessionWorkspace},
};

const ARRIVAL_EVENT_DEADLINE: Duration = Duration::from_secs(45);

/// Inputs already selected and persisted by the multi-ROM coordinator.
/// `persisted_nonce` must be the nonce durably recorded before destination
/// launch; this API cannot establish journal persistence on its own.
pub struct ArrivalVerificationInput<'a> {
    pub expected_full_sav_sha256: Sha256Digest,
    pub staged_sav: &'a [u8],
    pub registry: RegistryContract,
    pub destination_world: RomWorldId,
    pub expected_save_generation: u32,
    pub expected_map_group: u8,
    pub expected_map_num: u8,
    pub persisted_nonce: [u8; 16],
    pub workspace: &'a SessionWorkspace,
    pub sidecar: CommandSpec,
    pub mgba: CommandSpec,
    pub bridge_source: &'a Path,
}

/// Authenticated ROM observation, returned only after both children are reaped.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthenticatedArrivalEvidence {
    pub full_sav_sha256: Sha256Digest,
    pub flash_sha256: Sha256Digest,
    pub destination_world: RomWorldId,
    pub save_generation: u32,
    pub map_group: u8,
    pub map_num: u8,
    pub nonce: [u8; 16],
}

#[derive(Debug, Error)]
pub enum ArrivalVerificationError {
    #[error("arrival nonce or expected save generation is zero")]
    InvalidExpectation,
    #[error("staged save differs from trusted full-save digest")]
    StagedDigestMismatch,
    #[error("staged save is not a valid V2 image for the registry")]
    InvalidStagedSave(#[source] SaveV2Error),
    #[error("staged V2 generation differs from expected generation")]
    StagedGenerationMismatch,
    #[error("arrival verifier requires an explicit sidecar and owned staged ROM")]
    InvalidProcessSpec,
    #[error("ROM proof nonce differs from persisted nonce")]
    NonceMismatch,
    #[error("ROM proof flash hash differs from staged Flash1M image")]
    FlashDigestMismatch,
    #[error("ROM proof world differs from destination world")]
    WorldMismatch,
    #[error("ROM proof generation differs from staged save")]
    GenerationMismatch,
    #[error("ROM proof map differs from expected arrival map")]
    MapMismatch,
    #[error("verifier returned an unrelated control or child event")]
    UnexpectedEvent,
    #[error("offline arrival verification timed out")]
    Timeout,
    #[error("private workspace operation failed")]
    Workspace(#[from] SessionError),
    #[error("verifier process or control operation failed")]
    Process(#[from] ProcessError),
    #[error("verification failed ({verification}) and process cleanup failed ({cleanup})")]
    Cleanup {
        verification: Box<Self>,
        cleanup: ProcessError,
    },
}

#[derive(Clone, Copy)]
struct ExpectedProof {
    full_sav_sha256: Sha256Digest,
    flash_sha256: Sha256Digest,
    destination_world: RomWorldId,
    save_generation: u32,
    map_group: u8,
    map_num: u8,
    nonce: [u8; 16],
}

impl ExpectedProof {
    fn check(
        self,
        proof: ArrivalProof,
    ) -> Result<AuthenticatedArrivalEvidence, ArrivalVerificationError> {
        if proof.nonce != self.nonce {
            return Err(ArrivalVerificationError::NonceMismatch);
        }
        if Sha256Digest::from_bytes(proof.flash_sha256) != self.flash_sha256 {
            return Err(ArrivalVerificationError::FlashDigestMismatch);
        }
        if proof.world_id != u32::from(self.destination_world.get()) {
            return Err(ArrivalVerificationError::WorldMismatch);
        }
        if proof.save_generation != self.save_generation {
            return Err(ArrivalVerificationError::GenerationMismatch);
        }
        if (proof.map_group, proof.map_num) != (self.map_group, self.map_num) {
            return Err(ArrivalVerificationError::MapMismatch);
        }
        Ok(AuthenticatedArrivalEvidence {
            full_sav_sha256: self.full_sav_sha256,
            flash_sha256: self.flash_sha256,
            destination_world: self.destination_world,
            save_generation: self.save_generation,
            map_group: self.map_group,
            map_num: self.map_num,
            nonce: self.nonce,
        })
    }
}

fn preflight(
    input: &ArrivalVerificationInput<'_>,
) -> Result<ExpectedProof, ArrivalVerificationError> {
    if input.persisted_nonce == [0; 16] || input.expected_save_generation == 0 {
        return Err(ArrivalVerificationError::InvalidExpectation);
    }
    if Sha256Digest::of_bytes(input.staged_sav) != input.expected_full_sav_sha256 {
        return Err(ArrivalVerificationError::StagedDigestMismatch);
    }
    let parsed = parse_v2(input.staged_sav, input.registry)
        .map_err(ArrivalVerificationError::InvalidStagedSave)?;
    if parsed.coop().save_generation != input.expected_save_generation {
        return Err(ArrivalVerificationError::StagedGenerationMismatch);
    }
    Ok(ExpectedProof {
        full_sav_sha256: input.expected_full_sav_sha256,
        flash_sha256: Sha256Digest::of_bytes(parsed.flash_bytes()),
        destination_world: input.destination_world,
        save_generation: input.expected_save_generation,
        map_group: input.expected_map_group,
        map_num: input.expected_map_num,
        nonce: input.persisted_nonce,
    })
}

async fn next_event_before(
    children: &mut SupervisedChildren,
    deadline: Instant,
) -> Result<ControlEvent, ArrivalVerificationError> {
    match timeout_at(deadline, children.next_event())
        .await
        .map_err(|_| ArrivalVerificationError::Timeout)??
    {
        SupervisorEvent::Control(event) => Ok(event),
        SupervisorEvent::ChildExited => Err(ArrivalVerificationError::UnexpectedEvent),
    }
}

/// Verifies one offline destination load and always stops/reaps its processes.
///
/// # Errors
///
/// Returns an error for invalid staged data, an unrelated or mismatched ROM
/// observation, a timeout, or uncertain child cleanup. No authenticated
/// evidence is returned if cleanup is uncertain.
pub async fn verify_arrival(
    input: ArrivalVerificationInput<'_>,
) -> Result<AuthenticatedArrivalEvidence, ArrivalVerificationError> {
    let expected = preflight(&input)?;
    if !input.sidecar.is_arrival_verifier() || !input.mgba.owns_staged_rom() {
        return Err(ArrivalVerificationError::InvalidProcessSpec);
    }
    input
        .workspace
        .write_atomic("character.sav", input.staged_sav)?;
    let mut children = SupervisedChildren::start_arrival_verifier_with_bridge(
        input.sidecar,
        input.mgba,
        input.workspace,
        input.bridge_source,
    )
    .await?;

    let deadline = Instant::now() + ARRIVAL_EVENT_DEADLINE;
    let verification = async {
        if !matches!(
            next_event_before(&mut children, deadline).await?,
            ControlEvent::ArrivalVerifierReady {}
        ) {
            return Err(ArrivalVerificationError::UnexpectedEvent);
        }
        timeout_at(
            deadline,
            children.control.send(&ControlCommand::ArrivalChallenge {
                nonce: expected.nonce,
            }),
        )
        .await
        .map_err(|_| ArrivalVerificationError::Timeout)??;
        let ControlEvent::ArrivalProof(proof) = next_event_before(&mut children, deadline).await?
        else {
            return Err(ArrivalVerificationError::UnexpectedEvent);
        };
        expected.check(proof)
    }
    .await;

    let cleanup = children.stop_in_place().await;
    match (verification, cleanup) {
        (Ok(evidence), Ok(())) => Ok(evidence),
        (Err(verification), Ok(())) => Err(verification),
        (Ok(_), Err(cleanup)) => Err(ArrivalVerificationError::Process(cleanup)),
        (Err(verification), Err(cleanup)) => Err(ArrivalVerificationError::Cleanup {
            verification: Box::new(verification),
            cleanup,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coop_protocol::{IDENTITY_REGISTRY_DIGEST, IDENTITY_REGISTRY_VERSION, RegionId};
    use coop_save::{
        COOP_SAVE_OFFSET, COOP_SAVE_V1_MAGIC, COOP_SAVE_V1_SIZE, LOGICAL_SECTOR_DATA_SIZES,
        SAVE_BLOCK3_CAPACITY, SAVE_BLOCK3_CHUNK_OFFSET, SAVE_BLOCK3_CHUNK_SIZE, SECTOR_SIZE,
        SECTORS_PER_SLOT, sector_checksum,
    };

    fn registry() -> RegistryContract {
        RegistryContract::new(IDENTITY_REGISTRY_VERSION, IDENTITY_REGISTRY_DIGEST)
    }

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = 0xffff_ffff_u32;
        for byte in bytes {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb8_8320 & 0_u32.wrapping_sub(crc & 1));
            }
        }
        !crc
    }

    fn fixture_v2(generation: u32) -> Vec<u8> {
        let mut payload = [0_u8; COOP_SAVE_V1_SIZE];
        payload[0..4].copy_from_slice(&COOP_SAVE_V1_MAGIC.to_le_bytes());
        payload[4..6].copy_from_slice(&coop_save::v2::COOP_SAVE_V2_SCHEMA_VERSION.to_le_bytes());
        payload[6..8].copy_from_slice(&(COOP_SAVE_V1_SIZE as u16).to_le_bytes());
        payload[8..12].copy_from_slice(&IDENTITY_REGISTRY_VERSION.to_le_bytes());
        payload[12..28].copy_from_slice(&IDENTITY_REGISTRY_DIGEST);
        payload[28..32].copy_from_slice(&generation.to_le_bytes());
        payload[32..36].copy_from_slice(
            &coop_save::v2::COOP_SAVE_STATUS_MET_LOCATION_NORMALIZED.to_le_bytes(),
        );
        for (index, region) in [1_u8, 2, 3, 4].into_iter().enumerate() {
            payload[36 + index * 8] = region;
        }
        payload[coop_save::v2::COOP_SAVE_V2_CORMORIA_PROGRESS_OFFSET] = RegionId::Cormoria.wire();
        let payload_crc = crc32(&payload[..668]);
        payload[668..672].copy_from_slice(&payload_crc.to_le_bytes());
        let mut block3 = [0xff_u8; SAVE_BLOCK3_CAPACITY];
        block3[COOP_SAVE_OFFSET..COOP_SAVE_OFFSET + COOP_SAVE_V1_SIZE].copy_from_slice(&payload);
        let mut bytes = vec![0xff_u8; 128 * 1024];
        for (slot, counter) in [(0_usize, 0_u32), (1, 1)] {
            let base = slot * SECTORS_PER_SLOT * SECTOR_SIZE;
            for (logical, payload_size) in LOGICAL_SECTOR_DATA_SIZES.iter().enumerate() {
                let sector =
                    &mut bytes[base + logical * SECTOR_SIZE..base + (logical + 1) * SECTOR_SIZE];
                sector.fill(0);
                let chunk = logical * SAVE_BLOCK3_CHUNK_SIZE;
                sector[SAVE_BLOCK3_CHUNK_OFFSET..SAVE_BLOCK3_CHUNK_OFFSET + SAVE_BLOCK3_CHUNK_SIZE]
                    .copy_from_slice(&block3[chunk..chunk + SAVE_BLOCK3_CHUNK_SIZE]);
                sector[4084..4086].copy_from_slice(&(logical as u16).to_le_bytes());
                let checksum = sector_checksum(&sector[..*payload_size]);
                sector[4086..4088].copy_from_slice(&checksum.to_le_bytes());
                sector[4088..4092].copy_from_slice(&0x0801_2025_u32.to_le_bytes());
                sector[4092..4096].copy_from_slice(&counter.to_le_bytes());
            }
        }
        bytes
    }

    fn expected(staged_sav: &[u8]) -> ExpectedProof {
        ExpectedProof {
            full_sav_sha256: Sha256Digest::of_bytes(staged_sav),
            flash_sha256: Sha256Digest::of_bytes(&staged_sav[..128 * 1024]),
            destination_world: RomWorldId::new(2).unwrap(),
            save_generation: 7,
            map_group: 3,
            map_num: 4,
            nonce: [5; 16],
        }
    }

    fn proof(expected: ExpectedProof) -> ArrivalProof {
        ArrivalProof {
            nonce: expected.nonce,
            flash_sha256: *expected.flash_sha256.as_bytes(),
            world_id: u32::from(expected.destination_world.get()),
            save_generation: expected.save_generation,
            map_group: expected.map_group,
            map_num: expected.map_num,
        }
    }

    #[test]
    fn exact_proof_rejects_wrong_nonce_flash_world_generation_and_map() {
        let mut staged = fixture_v2(7);
        staged.extend_from_slice(&[0; 16]);
        let expected = expected(&staged);
        assert_ne!(expected.full_sav_sha256, expected.flash_sha256);
        assert_eq!(expected.check(proof(expected)).unwrap().nonce, [5; 16]);
        let mut cases = Vec::new();
        let mut wrong = proof(expected);
        wrong.nonce[0] ^= 1;
        cases.push((wrong, "nonce"));
        let mut wrong = proof(expected);
        wrong.flash_sha256[0] ^= 1;
        cases.push((wrong, "flash"));
        let mut wrong = proof(expected);
        wrong.world_id += 1;
        cases.push((wrong, "world"));
        let mut wrong = proof(expected);
        wrong.save_generation += 1;
        cases.push((wrong, "generation"));
        let mut wrong = proof(expected);
        wrong.map_num += 1;
        cases.push((wrong, "map"));
        for (wrong, field) in cases {
            let error = expected.check(wrong).unwrap_err();
            assert!(
                matches!(
                    (&error, field),
                    (ArrivalVerificationError::NonceMismatch, "nonce")
                        | (ArrivalVerificationError::FlashDigestMismatch, "flash")
                        | (ArrivalVerificationError::WorldMismatch, "world")
                        | (ArrivalVerificationError::GenerationMismatch, "generation")
                        | (ArrivalVerificationError::MapMismatch, "map")
                ),
                "unexpected mismatch for {field}: {error}"
            );
        }
    }

    #[tokio::test]
    async fn preflight_checks_full_digest_v2_registry_generation_and_nonce_before_write() {
        let root = tempfile::tempdir().unwrap();
        let workspace = SessionWorkspace::create(root.path()).unwrap();
        let staged = fixture_v2(7);
        let sidecar = CommandSpec::sidecar_template("missing-sidecar.exe").unwrap();
        let mgba = CommandSpec::sidecar_template("missing-mgba.exe").unwrap();
        let mut input = ArrivalVerificationInput {
            expected_full_sav_sha256: Sha256Digest::of_bytes(&staged),
            staged_sav: &staged,
            registry: registry(),
            destination_world: RomWorldId::new(2).unwrap(),
            expected_save_generation: 7,
            expected_map_group: 3,
            expected_map_num: 4,
            persisted_nonce: [5; 16],
            workspace: &workspace,
            sidecar,
            mgba,
            bridge_source: root.path(),
        };
        assert_eq!(
            preflight(&input).unwrap().flash_sha256,
            Sha256Digest::of_bytes(&staged)
        );
        input.expected_full_sav_sha256 = Sha256Digest::of_bytes(b"wrong");
        assert!(matches!(
            preflight(&input),
            Err(ArrivalVerificationError::StagedDigestMismatch)
        ));
        input.expected_full_sav_sha256 = Sha256Digest::of_bytes(&staged);
        input.registry = RegistryContract::new(registry().version + 1, registry().digest);
        assert!(matches!(
            preflight(&input),
            Err(ArrivalVerificationError::InvalidStagedSave(_))
        ));
        input.registry = registry();
        input.expected_save_generation = 8;
        assert!(matches!(
            preflight(&input),
            Err(ArrivalVerificationError::StagedGenerationMismatch)
        ));
        input.expected_save_generation = 7;
        input.persisted_nonce = [0; 16];
        assert!(matches!(
            preflight(&input),
            Err(ArrivalVerificationError::InvalidExpectation)
        ));
        input.persisted_nonce = [5; 16];
        input.sidecar = input.sidecar.clone().with_arrival_verifier().unwrap();
        assert!(matches!(
            verify_arrival(input).await,
            Err(ArrivalVerificationError::InvalidProcessSpec)
        ));
        assert!(!workspace.path().join("character.sav").exists());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn rejected_verifier_start_cleans_owned_staging_before_return() {
        use crate::process::{staged_rom_marker_contents, staged_rom_marker_path};

        let root = tempfile::tempdir().unwrap();
        let workspace = SessionWorkspace::create(root.path()).unwrap();
        let sidecar_path = root.path().join("sidecar.exe");
        let mgba_path = root.path().join("mgba.exe");
        let rom = root.path().join("destination.gba");
        std::fs::write(&sidecar_path, b"sidecar identity fixture").unwrap();
        std::fs::write(&mgba_path, b"not the pinned official emulator").unwrap();
        std::fs::write(&rom, b"staged destination ROM").unwrap();
        let marker = staged_rom_marker_path(&rom);
        std::fs::write(&marker, staged_rom_marker_contents(&rom).unwrap()).unwrap();
        let staged = fixture_v2(7);
        let input = ArrivalVerificationInput {
            expected_full_sav_sha256: Sha256Digest::of_bytes(&staged),
            staged_sav: &staged,
            registry: registry(),
            destination_world: RomWorldId::new(2).unwrap(),
            expected_save_generation: 7,
            expected_map_group: 3,
            expected_map_num: 4,
            persisted_nonce: [5; 16],
            workspace: &workspace,
            sidecar: CommandSpec::sidecar_template(&sidecar_path)
                .unwrap()
                .with_arrival_verifier()
                .unwrap(),
            mgba: CommandSpec::mgba_owned_staged(&mgba_path, &rom, &marker).unwrap(),
            bridge_source: root.path(),
        };
        assert!(matches!(
            verify_arrival(input).await,
            Err(ArrivalVerificationError::Process(
                ProcessError::MgbaIdentity
            ))
        ));
        assert!(!rom.exists());
        assert!(!marker.exists());
        assert_eq!(
            std::fs::read(workspace.path().join("character.sav")).unwrap(),
            staged
        );
    }
}
