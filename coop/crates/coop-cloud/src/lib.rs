//! Secure, region-safe contracts for Phase 2 cloud co-op.
//!
//! This crate intentionally contains DTOs, invariant-bearing values, and
//! compatibility/signature helpers only. Credential verification, persistence,
//! token rotation, leases, and object storage remain server-owned concerns.

#![forbid(unsafe_code)]

pub mod auth;
pub mod ids;
pub mod resume;
pub mod security;
pub mod session;
pub mod snapshot;

pub use auth::{
    AUTH_API_VERSION, ApiVersion, AuthError, LoginRequest, LoginResponse, LogoutRequest,
    LogoutResponse, RefreshRequest, RefreshResponse, RefreshTokenRequest, RefreshTokenResponse,
    RegisterRequest, RegisterResponse, RegistrationRequest, RegistrationResponse, Username,
};
pub use ids::{
    BridgeAbiVersion, CharacterId, ClientInstanceId, CommitId, GameBuildId, IdError,
    IdempotencyKey, MgbaVersion, ProtocolVersion, RefreshFamilyId, Revision, SessionEpoch,
    SessionId, Sha256Digest, SnapshotId, SnapshotRevision, Timestamp, UnixTimestampMillis, UserId,
};
pub use resume::{
    CompatibilityError, CompatibilityTarget, CreatedAt, ManifestBuildInfo, ManifestSignature,
    ManifestSigningKey, PackageArtifact, RESUME_MANIFEST_VERSION, ResumeError,
    ResumePackageArtifact, ResumePackageManifest, ResumeSelection, SIGNING_ALGORITHM_VERSION,
    SavestateFallbackReason, SignedManifestEnvelope, TrustedManifestKey,
};
pub use security::{
    AccessToken, InvitationCode, LoopbackSecret, Password, RefreshToken, SecretError, SigningKey,
    SigningPrivateKey,
};
pub use session::{
    AcquireLeaseRequest, AcquireRequest, CharacterCloudState, HeartbeatLeaseRequest,
    HeartbeatRequest, LeaseContract, LeaseFence, ReconnectLeaseRequest, ReconnectRequest,
    ReleaseLeaseRequest, ReleaseRequest, SessionError,
};
pub use snapshot::{
    ArtifactIdentity, FinalizeSnapshotRequest, ListSnapshotsRequest, ListSnapshotsResponse,
    PrepareSnapshotRequest, PrepareSnapshotResponse, RestoreSnapshotRequest,
    RestoreSnapshotResponse, SnapshotError, SnapshotFence, SnapshotFile, SnapshotFinalizeFence,
    SnapshotFinalizeRequest, SnapshotListRequest, SnapshotListResponse, SnapshotPrepareFence,
    SnapshotPrepareRequest, SnapshotPrepareResponse, SnapshotRecord, SnapshotRestoreRequest,
    SnapshotRestoreResponse, UploadCapabilityUrl, UploadMethod, UploadTarget,
};

#[cfg(test)]
mod tests {
    use coop_protocol::{RegionId, RegionalProgress, TrainerInstanceId, WorldZone};
    use ed25519_dalek::{SigningKey as DalekSigningKey, VerifyingKey};
    use serde_json::json;

    use super::*;

    fn id<T>(constructor: fn(uuid::Uuid) -> Result<T, IdError>) -> T {
        constructor(uuid::Uuid::from_u128(1)).expect("test UUID is non-nil")
    }

    fn state() -> CharacterCloudState {
        let hoenn_trainer = TrainerInstanceId::new(RegionId::Hoenn, "TRAINER_RIVAL").unwrap();
        let hoenn =
            RegionalProgress::new(RegionId::Hoenn, 0b11, 4, vec![hoenn_trainer], vec![]).unwrap();
        let kanto = RegionalProgress::new(RegionId::Kanto, 0xffff, 9, vec![], vec![]).unwrap();
        CharacterCloudState::new(
            id(CharacterId::new),
            WorldZone::new(RegionId::Hoenn, "ROUTE101", 1).unwrap(),
            vec![kanto, hoenn],
        )
        .unwrap()
    }

    fn files(with_resume: bool) -> Vec<SnapshotFile> {
        let mut files = vec![
            SnapshotFile::from_bytes(ArtifactIdentity::CharacterSav, b"sav").unwrap(),
            SnapshotFile::from_bytes(ArtifactIdentity::PendingCommits, b"pending").unwrap(),
        ];
        if with_resume {
            files.push(SnapshotFile::from_bytes(ArtifactIdentity::ResumeSs1, b"state").unwrap());
        }
        files
    }

    fn assert_prepare_response_correlation(
        response: &SnapshotPrepareResponse,
        prepare: &SnapshotPrepareRequest,
    ) {
        assert!(response.matches_request(prepare));
        assert!(response.echoes_declaration(prepare));

        let mut bad_snapshot = response.clone();
        bad_snapshot.snapshot_id = SnapshotId::new(uuid::Uuid::from_u128(2)).unwrap();
        assert!(!bad_snapshot.matches_request(prepare));

        let mut bad_parent = response.clone();
        bad_parent.expected_parent_revision = Revision::new(1);
        bad_parent.next_revision = Revision::new(2);
        assert!(!bad_parent.matches_request(prepare));

        let mut bad_epoch = response.clone();
        bad_epoch.session_epoch = SessionEpoch::new(2).unwrap();
        assert!(!bad_epoch.matches_request(prepare));

        let mut bad_key = response.clone();
        bad_key.idempotency_key = IdempotencyKey::new(uuid::Uuid::from_u128(2)).unwrap();
        assert!(!bad_key.matches_request(prepare));

        let mut bad_files = response.clone();
        bad_files.files[0].size_bytes += 1;
        assert!(!bad_files.matches_request(prepare));

        let mut bad_pending = response.clone();
        bad_pending.files[1].sha256 = Sha256Digest::of_bytes(b"changed-pending");
        bad_pending.pending_commits_sha256 = bad_pending.files[1].sha256;
        assert!(!bad_pending.matches_request(prepare));

        let mut invalid_next = response.clone();
        invalid_next.next_revision = Revision::new(9);
        assert!(!invalid_next.matches_request(prepare));
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum ReconnectModelOutcome {
        Rotated(LeaseContract),
        Replay(LeaseContract),
        Conflict,
        Stale,
    }

    struct ReconnectIdempotencyModel {
        current_epoch: SessionEpoch,
        consumed: Option<(LeaseFence, IdempotencyKey, LeaseContract)>,
    }

    impl ReconnectIdempotencyModel {
        fn new(current_epoch: SessionEpoch) -> Self {
            Self {
                current_epoch,
                consumed: None,
            }
        }

        fn reconnect(
            &mut self,
            fence: LeaseFence,
            idempotency_key: IdempotencyKey,
        ) -> ReconnectModelOutcome {
            if let Some((consumed_fence, consumed_key, lease)) = self.consumed {
                if consumed_key == idempotency_key {
                    return if consumed_fence == fence {
                        ReconnectModelOutcome::Replay(lease)
                    } else {
                        ReconnectModelOutcome::Conflict
                    };
                }
                return ReconnectModelOutcome::Stale;
            }
            if fence.session_epoch != self.current_epoch {
                return ReconnectModelOutcome::Stale;
            }
            let rotated_epoch = SessionEpoch::new(self.current_epoch.value() + 1)
                .expect("test epoch has room for one rotation");
            let rotated_fence = LeaseFence::new(
                fence.session_id,
                fence.character_id,
                fence.current_revision,
                rotated_epoch,
                fence.client_instance_id,
            );
            let lease = LeaseContract::new(rotated_fence, UnixTimestampMillis::new(1), 1)
                .expect("test lease is valid");
            self.current_epoch = rotated_epoch;
            self.consumed = Some((fence, idempotency_key, lease));
            ReconnectModelOutcome::Rotated(lease)
        }
    }

    fn manifest() -> ResumePackageManifest {
        ResumePackageManifest::new(
            id(CharacterId::new),
            Revision::initial(),
            Revision::new(1),
            ManifestBuildInfo {
                game_build_id: GameBuildId::new("emerald-coop-0.1.0+abc1234").unwrap(),
                rom_sha256: Sha256Digest::of_bytes(b"rom"),
                mgba_version: MgbaVersion::new("0.10.5").unwrap(),
                bridge_abi: BridgeAbiVersion::new(1).unwrap(),
                protocol_version: ProtocolVersion::new(1).unwrap(),
                pending_commits_sha256: Sha256Digest::of_bytes(b"pending"),
                snapshot_id: id(SnapshotId::new),
                session_epoch: SessionEpoch::new(7).unwrap(),
                last_commit_id: None,
            },
            Sha256Digest::of_bytes(b"sav-bytes"),
            Some(Sha256Digest::of_bytes(b"state-bytes")),
            true,
            CreatedAt::new("2026-09-01T00:00:00Z").unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn state_is_region_safe_and_round_trips() {
        let original = state();
        let json = serde_json::to_string(&original).unwrap();
        let decoded: CharacterCloudState = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, original);
        assert_eq!(decoded.active_region_badge_tier().unwrap(), 2);
        assert!(
            serde_json::from_str::<CharacterCloudState>(
                &json.replace("ROUTE101", "ROUTE101\",\"extra\":true")
            )
            .is_err()
        );
        let mut reversed = serde_json::to_value(&original).unwrap();
        reversed["regional_progress"]
            .as_array_mut()
            .unwrap()
            .reverse();
        assert!(serde_json::from_value::<CharacterCloudState>(reversed).is_err());
    }

    #[test]
    fn secrets_redact_and_scalars_fail_closed() {
        let password = Password::new("correct horse battery staple").unwrap();
        assert_eq!(password.to_string(), "[REDACTED]");
        assert!(!format!("{password:?}").contains("correct"));
        assert!(Password::new("short").is_err());
        assert_eq!(
            InvitationCode::new("invite").unwrap().to_string(),
            "[REDACTED]"
        );
        assert_eq!(
            AccessToken::new("access").unwrap().to_string(),
            "[REDACTED]"
        );
        assert_eq!(
            RefreshToken::new("refresh").unwrap().to_string(),
            "[REDACTED]"
        );
        assert_eq!(
            LoopbackSecret::new("0123456789abcdef0123456789abcdef")
                .unwrap()
                .to_string(),
            "[REDACTED]"
        );
        assert!(Sha256Digest::parse(&"A".repeat(64)).is_err());
        assert!(SessionEpoch::new(0).is_err());
        assert!(serde_json::from_str::<ApiVersion>("2").is_err());
        assert!(BridgeAbiVersion::new(2).is_err());
        assert!(ProtocolVersion::new(2).is_err());
    }

    #[test]
    fn uuid_username_and_server_owned_acquire_boundaries_are_strict() {
        let uuid = CharacterId::new(uuid::Uuid::from_u128(
            0xabcdef12_3456_4abc_8def_1234567890ab,
        ))
        .unwrap()
        .to_string();
        assert!(CharacterId::parse(&uuid).is_ok());
        assert!(CharacterId::parse(&uuid.to_uppercase()).is_err());
        assert!(CharacterId::parse(&uuid.replace('-', "")).is_err());
        assert!(CharacterId::parse(&format!("{{{uuid}}}")).is_err());
        assert!(CharacterId::parse(&format!("urn:uuid:{uuid}")).is_err());
        assert_eq!(
            Username::new("Ash_Ketchum").unwrap().as_str(),
            "ash_ketchum"
        );
        for invalid in ["ab", "_ash", "ash_", "ash space", "éash"] {
            assert!(Username::new(invalid).is_err(), "accepted {invalid}");
        }
        let request = AcquireLeaseRequest::new(
            id(CharacterId::new),
            id(ClientInstanceId::new),
            id(IdempotencyKey::new),
        );
        let mut wire = serde_json::to_value(request).unwrap();
        wire["session_id"] = json!(id(SessionId::new).to_string());
        wire["session_epoch"] = json!(1);
        assert!(serde_json::from_value::<AcquireLeaseRequest>(wire).is_err());
        let mut legacy_wire = serde_json::to_value(request).unwrap();
        legacy_wire["expected_revision"] = json!(0);
        assert!(serde_json::from_value::<AcquireLeaseRequest>(legacy_wire).is_err());
    }

    #[test]
    fn manifest_signs_and_selects_only_exact_compatible_savestate() {
        let package = manifest();
        let mut inconsistent = package.clone();
        inconsistent.savestate_compatible = false;
        assert!(serde_json::to_value(&inconsistent).is_err());
        let secret = SigningPrivateKey::from_bytes([9; 32]);
        let public = VerifyingKey::from(&DalekSigningKey::from_bytes(&[9; 32]));
        let envelope = SignedManifestEnvelope::sign(package.clone(), &secret, "test-key").unwrap();
        let trusted = TrustedManifestKey::new("test-key", public.to_bytes()).unwrap();
        envelope.verify(&trusted).unwrap();
        let mut missing_parent = serde_json::to_value(&package).unwrap();
        missing_parent
            .as_object_mut()
            .unwrap()
            .remove("parent_revision");
        assert!(serde_json::from_value::<ResumePackageManifest>(missing_parent).is_err());
        for field in ["snapshot_id", "session_epoch"] {
            let mut missing_provenance = serde_json::to_value(&package).unwrap();
            missing_provenance.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<ResumePackageManifest>(missing_provenance).is_err(),
                "accepted manifest without {field}"
            );
        }
        let mut tampered_snapshot = envelope.clone();
        tampered_snapshot.manifest.snapshot_id = SnapshotId::new(uuid::Uuid::from_u128(2)).unwrap();
        assert!(tampered_snapshot.verify(&trusted).is_err());
        let mut tampered_epoch = envelope.clone();
        tampered_epoch.manifest.session_epoch = SessionEpoch::new(8).unwrap();
        assert!(tampered_epoch.verify(&trusted).is_err());
        let target = CompatibilityTarget::new(
            package.game_build_id.clone(),
            package.rom_sha256,
            package.mgba_version.clone(),
            package.bridge_abi,
            package.protocol_version,
            package.revision,
        );
        assert_eq!(
            package
                .select_resume(&target, b"sav-bytes", Some(b"state-bytes"))
                .unwrap(),
            ResumeSelection::UseSavestate
        );
        assert!(matches!(
            package.select_resume(&target, b"wrong", Some(b"state-bytes")),
            Err(CompatibilityError::CorruptSav)
        ));
        assert!(matches!(
            package.select_resume(&target, b"sav-bytes", Some(b"wrong")),
            Ok(ResumeSelection::FallbackToSav(
                SavestateFallbackReason::CorruptSavestate
            ))
        ));
        let mut mismatch = target.clone();
        mismatch.revision = Revision::new(2);
        assert!(matches!(
            package
                .select_resume(&mismatch, b"sav-bytes", Some(b"state-bytes"))
                .unwrap(),
            ResumeSelection::FallbackToSav(SavestateFallbackReason::MetadataMismatch)
        ));
        let mut tampered = envelope;
        tampered.manifest.parent_revision = Revision::new(1);
        tampered.manifest.revision = Revision::new(2);
        assert!(tampered.manifest.validate().is_ok());
        assert!(tampered.verify(&trusted).is_err());
        assert!(TrustedManifestKey::new("test-key", [0; 32]).is_err());
        let mut malformed = package.clone();
        malformed.package_version = 2;
        assert!(matches!(
            malformed.select_resume(&target, b"sav-bytes", Some(b"state-bytes")),
            Err(CompatibilityError::ManifestInvalid)
        ));
        let mut invalid_envelope = serde_json::to_value(&tampered).unwrap();
        invalid_envelope["signing_algorithm_version"] = json!(2);
        assert!(serde_json::from_value::<SignedManifestEnvelope>(invalid_envelope).is_err());
        for invalid in [
            "2026-02-29T00:00:00Z",
            "2026-01-32T00:00:00Z",
            "2026-01-01T24:00:00Z",
            "2026-01-01T00:60:00Z",
            "2026-01-01T00:00:00Zgarbage",
        ] {
            assert!(CreatedAt::new(invalid).is_err(), "accepted {invalid}");
        }
        assert!(CreatedAt::new("2024-02-29T23:59:59Z").is_ok());
    }

    #[test]
    fn leases_and_region_collections_are_bounded() {
        let character_id = id(CharacterId::new);
        let session_id = id(SessionId::new);
        let client_id = id(ClientInstanceId::new);
        let fence = LeaseFence::new(
            session_id,
            character_id,
            Revision::initial(),
            SessionEpoch::new(1).unwrap(),
            client_id,
        );
        assert!(LeaseContract::new(fence, UnixTimestampMillis::new(0), 1).is_err());
        assert!(LeaseContract::new(fence, UnixTimestampMillis::new(1), 0).is_err());
        assert!(LeaseContract::new(fence, UnixTimestampMillis::new(1), 600_001).is_err());
        assert!(
            SnapshotListRequest::new(
                session_id,
                character_id,
                SessionEpoch::new(1).unwrap(),
                client_id,
                0,
            )
            .is_err()
        );
        let progress = [
            RegionId::Hoenn,
            RegionId::Kanto,
            RegionId::Johto,
            RegionId::Sevii,
            RegionId::Hoenn,
        ]
        .into_iter()
        .map(|region| RegionalProgress::new(region, 0, 0, vec![], vec![]).unwrap())
        .collect();
        assert!(
            CharacterCloudState::new(
                character_id,
                WorldZone::new(RegionId::Hoenn, "ROUTE101", 1).unwrap(),
                progress,
            )
            .is_err()
        );
    }

    #[test]
    fn snapshots_require_cas_and_validated_fixed_declarations() {
        let snapshot_id = id(SnapshotId::new);
        let character_id = id(CharacterId::new);
        let session_id = id(SessionId::new);
        let client_id = id(ClientInstanceId::new);
        let idempotency = id(IdempotencyKey::new);
        let fence = SnapshotPrepareFence::new(
            session_id,
            character_id,
            Revision::initial(),
            SessionEpoch::new(1).unwrap(),
            client_id,
            idempotency,
        );
        let declared = files(false);
        let pending_digest = declared
            .iter()
            .find(|file| file.artifact == ArtifactIdentity::PendingCommits)
            .unwrap()
            .sha256;
        let prepare =
            SnapshotPrepareRequest::new(snapshot_id, fence, declared.clone(), pending_digest)
                .unwrap();
        assert_eq!(prepare.expected_parent_revision, Revision::initial());
        let encoded = serde_json::to_string(&prepare).unwrap();
        assert_eq!(
            serde_json::from_str::<SnapshotPrepareRequest>(&encoded).unwrap(),
            prepare
        );
        assert!(
            SnapshotPrepareRequest::new(
                snapshot_id,
                fence,
                files(false),
                Sha256Digest::of_bytes(b"wrong"),
            )
            .is_err()
        );
        let mut missing = serde_json::to_value(&prepare).unwrap();
        missing["files"] = json!([]);
        assert!(serde_json::from_value::<SnapshotPrepareRequest>(missing).is_err());
        for missing_artifact in [
            ArtifactIdentity::CharacterSav,
            ArtifactIdentity::PendingCommits,
        ] {
            let remaining = declared
                .iter()
                .filter(|file| file.artifact != missing_artifact)
                .map(serde_json::to_value)
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            let mut missing = serde_json::to_value(&prepare).unwrap();
            missing["files"] = serde_json::Value::Array(remaining);
            assert!(serde_json::from_value::<SnapshotPrepareRequest>(missing).is_err());
        }
        let mut prepare_drift = serde_json::to_value(&prepare).unwrap();
        prepare_drift["pending_commits_sha256"] = json!(Sha256Digest::of_bytes(b"wrong"));
        assert!(serde_json::from_value::<SnapshotPrepareRequest>(prepare_drift).is_err());
        let mut path_like = serde_json::to_value(&declared[0]).unwrap();
        path_like["artifact"] = json!("../../escape");
        assert!(serde_json::from_value::<SnapshotFile>(path_like).is_err());
        let three = files(true);
        let three_digest = three[1].sha256;
        assert!(SnapshotPrepareRequest::new(snapshot_id, fence, three, three_digest).is_ok());

        let targets = declared
            .iter()
            .map(|file| {
                UploadTarget::new(
                    file.artifact,
                    UploadMethod::Put,
                    "https://example.com/upload?ticket=opaque",
                    UnixTimestampMillis::new(1),
                )
                .unwrap()
            })
            .collect();
        let response = SnapshotPrepareResponse {
            api_version: ApiVersion::V1,
            snapshot_id,
            expected_parent_revision: Revision::initial(),
            next_revision: Revision::new(1),
            session_epoch: fence.session_epoch,
            idempotency_key: fence.idempotency_key,
            files: declared.clone(),
            pending_commits_sha256: pending_digest,
            upload_targets: targets,
        };
        response.validate().unwrap();
        let mut bad_response = response.clone();
        bad_response.pending_commits_sha256 = Sha256Digest::of_bytes(b"wrong");
        assert!(bad_response.validate().is_err());
        assert!(!bad_response.matches_request(&prepare));
        assert_prepare_response_correlation(&response, &prepare);
        assert!(
            serde_json::from_str::<SnapshotPrepareResponse>(
                &serde_json::to_string(&response).unwrap()
            )
            .is_ok()
        );
    }

    #[test]
    fn upload_targets_reject_unsafe_urls_and_redact_secrets() {
        for invalid in [
            "https://example.com/upload",
            "https://example.com/upload?",
            "https://example.com/upload?ticket=",
            "https://example.com/upload?=ticket",
            "https://example.com/upload?ticket",
            "https://example.com/upload?ticket=ok&",
            "https://example.com/upload?ticket=%",
            "https://example.com/upload?ticket=%A",
            "https://example.com/upload?ticket=%ZZ",
            "https://example.com/upload?ticket=%0G",
        ] {
            assert!(
                UploadCapabilityUrl::new(invalid).is_err(),
                "accepted credential-free URL {invalid}"
            );
        }
        let local =
            UploadCapabilityUrl::new("http://127.0.0.1:43127/upload?ticket=opaque").unwrap();
        let https =
            UploadCapabilityUrl::new("https://example.com/upload?X-Goog-Signature=opaque").unwrap();
        let escaped = UploadCapabilityUrl::new("https://example.com/upload?ticket=a%2Fb").unwrap();
        assert_eq!(serde_json::to_value(&local).unwrap(), json!(local.as_str()));
        assert_eq!(serde_json::to_value(&https).unwrap(), json!(https.as_str()));
        assert_eq!(
            serde_json::to_value(&escaped).unwrap(),
            json!(escaped.as_str())
        );
        assert_eq!(local.to_string(), "[REDACTED]");
        assert!(!format!("{local:?}").contains("ticket"));
        assert!(
            UploadTarget::new(
                ArtifactIdentity::CharacterSav,
                UploadMethod::Put,
                "http://example.com/upload",
                UnixTimestampMillis::new(1),
            )
            .is_err()
        );
        assert!(
            UploadTarget::new(
                ArtifactIdentity::CharacterSav,
                UploadMethod::Put,
                "https://user:pass@example.com/upload#fragment",
                UnixTimestampMillis::new(1),
            )
            .is_err()
        );
        let target = UploadTarget::new(
            ArtifactIdentity::CharacterSav,
            UploadMethod::Put,
            "https://example.com/upload?ticket=opaque",
            UnixTimestampMillis::new(1),
        )
        .unwrap();
        assert!(!format!("{target:?}").contains("example.com"));
        assert!(
            UploadTarget::new(
                ArtifactIdentity::CharacterSav,
                UploadMethod::Put,
                "https://example.com/upload?ticket=opaque",
                UnixTimestampMillis::new(0),
            )
            .is_err()
        );
        assert!(
            UploadTarget::new(
                ArtifactIdentity::CharacterSav,
                UploadMethod::Put,
                "http://127.0.0.1:43127/upload?ticket=opaque",
                UnixTimestampMillis::new(1),
            )
            .is_ok()
        );
        assert!(
            UploadTarget::new(
                ArtifactIdentity::CharacterSav,
                UploadMethod::Put,
                "http://127.0.0.1:not-a-port/upload",
                UnixTimestampMillis::new(1),
            )
            .is_err()
        );
    }

    #[test]
    fn finalize_rejects_declaration_drift_and_revision_zero() {
        let snapshot_id = id(SnapshotId::new);
        let character_id = id(CharacterId::new);
        let session_id = id(SessionId::new);
        let client_id = id(ClientInstanceId::new);
        let idempotency = id(IdempotencyKey::new);
        let epoch = SessionEpoch::new(1).unwrap();
        let declared = files(false);
        let pending_digest = declared[1].sha256;
        let finalize = SnapshotFinalizeRequest::new(
            snapshot_id,
            SnapshotFinalizeFence::new(
                session_id,
                character_id,
                Revision::initial(),
                epoch,
                client_id,
                idempotency,
            ),
            declared.clone(),
            pending_digest,
            None,
        )
        .unwrap();
        assert!(finalize.matches_declaration(&declared, pending_digest));
        let mut drift = declared;
        drift[0].size_bytes += 1;
        assert!(!finalize.matches_declaration(&drift, pending_digest));
        let mut finalize_wire = serde_json::to_value(&finalize).unwrap();
        finalize_wire["pending_commits_sha256"] = json!(Sha256Digest::of_bytes(b"wrong"));
        assert!(serde_json::from_value::<SnapshotFinalizeRequest>(finalize_wire).is_err());
        let mut finalize_file_drift = serde_json::to_value(&finalize).unwrap();
        finalize_file_drift["files"] =
            serde_json::Value::Array(vec![serde_json::to_value(&finalize.files[0]).unwrap()]);
        assert!(serde_json::from_value::<SnapshotFinalizeRequest>(finalize_file_drift).is_err());
        assert!(
            SnapshotFinalizeRequest::new(
                snapshot_id,
                SnapshotFinalizeFence::new(
                    session_id,
                    character_id,
                    Revision::initial(),
                    epoch,
                    client_id,
                    idempotency,
                ),
                files(false),
                Sha256Digest::of_bytes(b"wrong"),
                None,
            )
            .is_err()
        );

        let sav = SnapshotFile::from_bytes(ArtifactIdentity::CharacterSav, b"sav").unwrap();
        assert!(
            SnapshotRecord::new(
                snapshot_id,
                SnapshotFence::new(session_id, character_id, epoch),
                Revision::initial(),
                Revision::initial(),
                vec![
                    sav,
                    SnapshotFile::from_bytes(ArtifactIdentity::PendingCommits, b"pending").unwrap()
                ],
                Sha256Digest::of_bytes(b"pending"),
                None,
                UnixTimestampMillis::new(1),
            )
            .is_err()
        );
    }

    #[test]
    fn reconnect_wire_carries_operation_identity_for_epoch_rotation_retries() {
        let fence = LeaseFence::new(
            id(SessionId::new),
            id(CharacterId::new),
            Revision::new(4),
            SessionEpoch::new(7).unwrap(),
            id(ClientInstanceId::new),
        );
        let key = id(IdempotencyKey::new);
        let request = ReconnectLeaseRequest::new(fence, key);
        assert_eq!(request.idempotency_key(), key);
        assert_eq!(request.fence(), fence);
        let wire = serde_json::to_string(&request).unwrap();
        assert_eq!(
            serde_json::from_str::<ReconnectLeaseRequest>(&wire).unwrap(),
            request
        );
        let mut missing_key = serde_json::to_value(request).unwrap();
        missing_key
            .as_object_mut()
            .unwrap()
            .remove("idempotency_key");
        assert!(serde_json::from_value::<ReconnectLeaseRequest>(missing_key).is_err());
        let mut changed_fingerprint = request;
        changed_fingerprint.current_revision = Revision::new(5);
        assert_ne!(changed_fingerprint, request);
        let mut changed_key = request;
        changed_key.idempotency_key = IdempotencyKey::new(uuid::Uuid::from_u128(2)).unwrap();
        assert_ne!(changed_key, request);

        let mut model = ReconnectIdempotencyModel::new(fence.session_epoch);
        let first = model.reconnect(fence, key);
        let rotated = match first {
            ReconnectModelOutcome::Rotated(lease) => lease,
            other => panic!("first reconnect was not a rotation: {other:?}"),
        };
        assert_eq!(
            model.reconnect(fence, key),
            ReconnectModelOutcome::Replay(rotated)
        );
        assert_eq!(
            model.reconnect(fence, key),
            ReconnectModelOutcome::Replay(rotated)
        );

        let mut changed_fingerprint = fence;
        changed_fingerprint.current_revision = Revision::new(5);
        assert_eq!(
            model.reconnect(changed_fingerprint, key),
            ReconnectModelOutcome::Conflict
        );
        assert_eq!(
            model.reconnect(fence, changed_key.idempotency_key),
            ReconnectModelOutcome::Stale
        );
    }

    #[test]
    fn snapshot_and_manifest_lineage_is_mandatory_and_signed() {
        let snapshot_id = id(SnapshotId::new);
        let character_id = id(CharacterId::new);
        let session_id = id(SessionId::new);
        let epoch = SessionEpoch::new(1).unwrap();
        let files = files(false);
        let pending = files[1].sha256;
        let record = SnapshotRecord::new(
            snapshot_id,
            SnapshotFence::new(session_id, character_id, epoch),
            Revision::initial(),
            Revision::new(1),
            files,
            pending,
            None,
            UnixTimestampMillis::new(1),
        )
        .unwrap();
        assert_eq!(record.parent_revision, Revision::initial());
        assert_eq!(
            serde_json::from_value::<SnapshotRecord>(serde_json::to_value(&record).unwrap())
                .unwrap(),
            record
        );

        for invalid_parent in [Revision::new(1), Revision::new(3)] {
            let mut invalid_record = record.clone();
            invalid_record.parent_revision = invalid_parent;
            assert!(serde_json::to_value(&invalid_record).is_err());
            let mut value = serde_json::to_value(&record).unwrap();
            value["parent_revision"] = json!(invalid_parent.value());
            assert!(serde_json::from_value::<SnapshotRecord>(value).is_err());
        }
        let mut missing_parent = serde_json::to_value(&record).unwrap();
        missing_parent
            .as_object_mut()
            .unwrap()
            .remove("parent_revision");
        assert!(serde_json::from_value::<SnapshotRecord>(missing_parent).is_err());

        let listed = SnapshotListResponse {
            api_version: ApiVersion::V1,
            snapshots: vec![record.clone()],
        };
        listed.validate().unwrap();
        let mut invalid_list = listed.clone();
        invalid_list.snapshots[0].parent_revision = Revision::new(2);
        assert!(invalid_list.validate().is_err());
        assert!(serde_json::to_value(&invalid_list).is_err());
    }

    #[test]
    fn upload_targets_are_put_urls_without_a_second_authorization_field() {
        let target = UploadTarget::new(
            ArtifactIdentity::CharacterSav,
            UploadMethod::Put,
            "https://example.com/upload?ticket=opaque",
            UnixTimestampMillis::new(1),
        )
        .unwrap();
        let mut value = serde_json::to_value(&target).unwrap();
        assert_eq!(value["method"], json!("PUT"));
        assert!(value.get("signature").is_none());
        assert_eq!(
            serde_json::from_value::<UploadTarget>(value.clone()).unwrap(),
            target
        );
        value["method"] = json!("put");
        assert!(serde_json::from_value::<UploadTarget>(value).is_err());
        let mut legacy = serde_json::to_value(&target).unwrap();
        legacy["signature"] = json!("invented-header-token");
        assert!(serde_json::from_value::<UploadTarget>(legacy).is_err());
        let mut oversized = serde_json::to_value(&target).unwrap();
        oversized["url"] = json!(format!("https://example.com/{}", "x".repeat(2048)));
        assert!(serde_json::from_value::<UploadTarget>(oversized).is_err());
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "the direct wire matrix keeps repaired boundary cases together"
    )]
    fn direct_wire_matrix_rejects_repaired_boundaries() {
        let mut state_value = serde_json::to_value(state()).unwrap();
        state_value["regional_progress"] = json!(
            [
                {"region":"HOENN","badge_mask":0,"story_checkpoint":0,"defeated_trainers":[],"unlocked_fly_points":[]},
                {"region":"KANTO","badge_mask":0,"story_checkpoint":0,"defeated_trainers":[],"unlocked_fly_points":[]},
                {"region":"JOHTO","badge_mask":0,"story_checkpoint":0,"defeated_trainers":[],"unlocked_fly_points":[]},
                {"region":"SEVII","badge_mask":0,"story_checkpoint":0,"defeated_trainers":[],"unlocked_fly_points":[]},
                {"region":"HOENN","badge_mask":0,"story_checkpoint":0,"defeated_trainers":[],"unlocked_fly_points":[]}
            ]
        );
        assert!(serde_json::from_value::<CharacterCloudState>(state_value).is_err());

        let mut oversized_trainer = serde_json::to_value(state()).unwrap();
        oversized_trainer["regional_progress"][1]["defeated_trainers"] =
            json!([format!("HOENN:TRAINER_{}", "A".repeat(120))]);
        assert!(serde_json::from_value::<CharacterCloudState>(oversized_trainer).is_err());

        let mut too_many_trainers = serde_json::to_value(state()).unwrap();
        too_many_trainers["regional_progress"][0]["defeated_trainers"] = serde_json::Value::Array(
            (0..=4096)
                .map(|index| json!(format!("HOENN:TRAINER_{index:04}")))
                .collect(),
        );
        assert!(serde_json::from_value::<CharacterCloudState>(too_many_trainers).is_err());

        let mut too_many_fly_points = serde_json::to_value(state()).unwrap();
        too_many_fly_points["regional_progress"][0]["unlocked_fly_points"] =
            serde_json::Value::Array(
                (0..=256)
                    .map(|index| json!(format!("HOENN:FLY_{index:04}")))
                    .collect(),
            );
        assert!(serde_json::from_value::<CharacterCloudState>(too_many_fly_points).is_err());

        let mut oversized_map = serde_json::to_value(state()).unwrap();
        oversized_map["world_zone"]["map"] = json!("A".repeat(129));
        assert!(serde_json::from_value::<CharacterCloudState>(oversized_map).is_err());

        for value in [0, 2] {
            assert!(serde_json::from_value::<ApiVersion>(json!(value)).is_err());
            assert!(serde_json::from_value::<BridgeAbiVersion>(json!(value)).is_err());
            assert!(serde_json::from_value::<ProtocolVersion>(json!(value)).is_err());
        }
        assert!(serde_json::from_value::<Username>(json!("A".repeat(33))).is_err());
        assert!(
            serde_json::from_value::<CharacterId>(json!(
                CharacterId::new(uuid::Uuid::from_u128(
                    0xabcdef12_3456_4abc_8def_1234567890ab,
                ))
                .unwrap()
                .to_string()
                .to_uppercase()
            ))
            .is_err()
        );
        assert!(serde_json::from_value::<GameBuildId>(json!("A".repeat(129))).is_err());
        assert!(
            serde_json::from_value::<MgbaVersion>(json!("1.2.".to_owned() + &"0".repeat(126)))
                .is_err()
        );
        assert!(serde_json::from_value::<CreatedAt>(json!("2".repeat(21))).is_err());

        let mut fence = serde_json::to_value(
            LeaseContract::new(
                LeaseFence::new(
                    id(SessionId::new),
                    id(CharacterId::new),
                    Revision::initial(),
                    SessionEpoch::new(1).unwrap(),
                    id(ClientInstanceId::new),
                ),
                UnixTimestampMillis::new(1),
                1,
            )
            .unwrap(),
        )
        .unwrap();
        fence["expires_at"] = json!(0);
        assert!(serde_json::from_value::<LeaseContract>(fence).is_err());

        let list = SnapshotListRequest::new(
            id(SessionId::new),
            id(CharacterId::new),
            SessionEpoch::new(1).unwrap(),
            id(ClientInstanceId::new),
            1,
        )
        .unwrap();
        let mut list_value = serde_json::to_value(list).unwrap();
        list_value["limit"] = json!(101);
        assert!(serde_json::from_value::<SnapshotListRequest>(list_value).is_err());

        for heartbeat_interval_ms in [0, 600_001] {
            let mut lease_value = serde_json::to_value(
                LeaseContract::new(
                    LeaseFence::new(
                        id(SessionId::new),
                        id(CharacterId::new),
                        Revision::initial(),
                        SessionEpoch::new(1).unwrap(),
                        id(ClientInstanceId::new),
                    ),
                    UnixTimestampMillis::new(1),
                    1,
                )
                .unwrap(),
            )
            .unwrap();
            lease_value["heartbeat_interval_ms"] = json!(heartbeat_interval_ms);
            assert!(serde_json::from_value::<LeaseContract>(lease_value).is_err());
        }

        let record = SnapshotRecord::new(
            id(SnapshotId::new),
            SnapshotFence::new(
                id(SessionId::new),
                id(CharacterId::new),
                SessionEpoch::new(1).unwrap(),
            ),
            Revision::initial(),
            Revision::new(1),
            files(false),
            Sha256Digest::of_bytes(b"pending"),
            None,
            UnixTimestampMillis::new(1),
        )
        .unwrap();
        let record_value = serde_json::to_value(&record).unwrap();
        let too_many_snapshots = json!({
            "api_version": 1,
            "snapshots": serde_json::Value::Array(
                (0..101).map(|_| record_value.clone()).collect()
            )
        });
        assert!(serde_json::from_value::<SnapshotListResponse>(too_many_snapshots).is_err());
    }

    #[test]
    fn auth_expiries_are_required_and_nonzero_on_the_wire() {
        let login = LoginResponse::new(
            id(UserId::new),
            id(CharacterId::new),
            AccessToken::new("access").unwrap(),
            RefreshToken::new("refresh").unwrap(),
            id(RefreshFamilyId::new),
            UnixTimestampMillis::new(10),
            UnixTimestampMillis::new(20),
        )
        .unwrap();
        assert_eq!(
            serde_json::from_value::<LoginResponse>(serde_json::to_value(&login).unwrap()).unwrap(),
            login
        );
        for field in ["access_expires_at", "refresh_expires_at"] {
            let mut value = serde_json::to_value(&login).unwrap();
            value[field] = json!(0);
            assert!(serde_json::from_value::<LoginResponse>(value).is_err());
        }

        let refresh = RefreshResponse::new(
            AccessToken::new("access").unwrap(),
            RefreshToken::new("refresh").unwrap(),
            id(RefreshFamilyId::new),
            UnixTimestampMillis::new(10),
            UnixTimestampMillis::new(20),
        )
        .unwrap();
        assert_eq!(
            serde_json::from_value::<RefreshResponse>(serde_json::to_value(&refresh).unwrap())
                .unwrap(),
            refresh
        );
        let mut value = serde_json::to_value(&refresh).unwrap();
        value["access_expires_at"] = json!(0);
        assert!(serde_json::from_value::<RefreshResponse>(value).is_err());
    }
}
