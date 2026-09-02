//! Signed resume-package metadata and conservative compatibility selection.

use std::fmt;

use ed25519_dalek::{Signature, VerifyingKey};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, SeqAccess, Visitor},
};
use thiserror::Error;

use crate::{
    BridgeAbiVersion, CharacterId, CommitId, GameBuildId, MgbaVersion, ProtocolVersion, Revision,
    SessionEpoch, Sha256Digest, SigningPrivateKey, SnapshotId, ids::deserialize_bounded_string,
    snapshot::ArtifactIdentity,
};

pub const RESUME_MANIFEST_VERSION: u16 = 1;
pub const SIGNING_ALGORITHM_VERSION: u16 = 1;

/// Errors produced while validating, signing, or selecting resume packages.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ResumeError {
    #[error("created_at must be an RFC3339 UTC timestamp")]
    InvalidCreatedAt,
    #[error("unsupported resume manifest version {0}")]
    UnknownManifestVersion(u16),
    #[error("resume manifest revision must be non-zero and exactly one greater than its parent")]
    InvalidSnapshotRevision,
    #[error("savestate compatibility flag must match savestate_sha256")]
    SavestateFlagMismatch,
    #[error("manifest canonicalization failed: {0}")]
    Canonicalization(String),
    #[error("unsupported signing algorithm version {0}")]
    UnknownSigningAlgorithm(u16),
    #[error("manifest signing key is not trusted")]
    UntrustedSigningKey,
    #[error("manifest signature is invalid")]
    InvalidSignature,
    #[error("manifest signing key is invalid")]
    InvalidKey,
    #[error("manifest signing key is a weak Ed25519 point")]
    WeakSigningKey,
    #[error("manifest signing key ID is invalid")]
    InvalidSigningKeyId,
    #[error("package artifact {path} does not match its digest or size")]
    ArtifactDigestMismatch { path: String },
}

/// A detached Ed25519 signature for the canonical manifest bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ManifestSignature([u8; 64]);

impl ManifestSignature {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 64]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }
}

impl Serialize for ManifestSignature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(&self.0)
    }
}

impl<'de> Deserialize<'de> for ManifestSignature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SignatureVisitor;

        impl<'de> Visitor<'de> for SignatureVisitor {
            type Value = ManifestSignature;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("exactly 64 signature bytes")
            }

            fn visit_bytes<E>(self, bytes: &[u8]) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let bytes: [u8; 64] = bytes
                    .try_into()
                    .map_err(|_| E::custom("manifest signature must contain 64 bytes"))?;
                Ok(ManifestSignature(bytes))
            }

            fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut bytes = [0_u8; 64];
                for (index, byte) in bytes.iter_mut().enumerate() {
                    *byte = sequence.next_element()?.ok_or_else(|| {
                        de::Error::custom("manifest signature must contain 64 bytes")
                    })?;
                    if index == 63 && sequence.next_element::<u8>()?.is_some() {
                        return Err(de::Error::custom(
                            "manifest signature must contain exactly 64 bytes",
                        ));
                    }
                }
                Ok(ManifestSignature(bytes))
            }
        }

        deserializer.deserialize_tuple(64, SignatureVisitor)
    }
}

/// A bounded RFC3339 UTC creation timestamp.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CreatedAt(String);

impl CreatedAt {
    /// Validates an RFC3339 UTC timestamp without accepting local offsets.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed, non-UTC, or oversized text.
    pub fn new(value: impl Into<String>) -> Result<Self, ResumeError> {
        let value = value.into();
        if value.len() != 20 {
            return Err(ResumeError::InvalidCreatedAt);
        }
        let bytes = value.as_bytes();
        if bytes[4] != b'-'
            || bytes[7] != b'-'
            || bytes[10] != b'T'
            || bytes[13] != b':'
            || bytes[16] != b':'
            || bytes[19] != b'Z'
            || !bytes[..4].iter().all(u8::is_ascii_digit)
            || !bytes[5..7].iter().all(u8::is_ascii_digit)
            || !bytes[8..10].iter().all(u8::is_ascii_digit)
            || !bytes[11..13].iter().all(u8::is_ascii_digit)
            || !bytes[14..16].iter().all(u8::is_ascii_digit)
            || !bytes[17..19].iter().all(u8::is_ascii_digit)
        {
            return Err(ResumeError::InvalidCreatedAt);
        }
        let year = decimal(&bytes[0..4]);
        let month = decimal(&bytes[5..7]);
        let day = decimal(&bytes[8..10]);
        let hour = decimal(&bytes[11..13]);
        let minute = decimal(&bytes[14..16]);
        let second = decimal(&bytes[17..19]);
        let month_days = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if year.is_multiple_of(4)
                && (!year.is_multiple_of(100) || year.is_multiple_of(400)) =>
            {
                29
            }
            2 => 28,
            _ => 0,
        };
        if !(1..=month_days).contains(&day) || hour > 23 || minute > 59 || second > 59 {
            return Err(ResumeError::InvalidCreatedAt);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn decimal(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .fold(0, |value, byte| value * 10 + u32::from(byte - b'0'))
}

impl fmt::Display for CreatedAt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Serialize for CreatedAt {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for CreatedAt {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Self::new(deserialize_bounded_string(deserializer, 20, "created_at")?)
            .map_err(serde::de::Error::custom)
    }
}

/// Compatibility and optional metadata used to construct a manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManifestBuildInfo {
    pub game_build_id: GameBuildId,
    pub rom_sha256: Sha256Digest,
    pub mgba_version: MgbaVersion,
    pub bridge_abi: BridgeAbiVersion,
    pub protocol_version: ProtocolVersion,
    pub pending_commits_sha256: Sha256Digest,
    /// The finalized snapshot this package came from.
    pub snapshot_id: SnapshotId,
    /// The lease epoch that authorized the finalized snapshot.
    pub session_epoch: SessionEpoch,
    pub last_commit_id: Option<CommitId>,
}

/// Metadata in `manifest.json`; signatures live in [`SignedManifestEnvelope`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResumePackageManifest {
    pub package_version: u16,
    pub character_id: CharacterId,
    pub parent_revision: Revision,
    pub revision: Revision,
    pub game_build_id: GameBuildId,
    pub rom_sha256: Sha256Digest,
    pub mgba_version: MgbaVersion,
    pub bridge_abi: BridgeAbiVersion,
    pub protocol_version: ProtocolVersion,
    pub save_sha256: Sha256Digest,
    pub savestate_sha256: Option<Sha256Digest>,
    pub savestate_compatible: bool,
    pub created_at: CreatedAt,
    pub last_commit_id: Option<CommitId>,
    pub snapshot_id: SnapshotId,
    pub session_epoch: SessionEpoch,
    pub pending_commits_sha256: Sha256Digest,
}

#[derive(Serialize)]
struct SerializableResumePackageManifest<'a> {
    package_version: u16,
    character_id: CharacterId,
    parent_revision: Revision,
    revision: Revision,
    game_build_id: &'a GameBuildId,
    rom_sha256: Sha256Digest,
    mgba_version: &'a MgbaVersion,
    bridge_abi: BridgeAbiVersion,
    protocol_version: ProtocolVersion,
    save_sha256: Sha256Digest,
    savestate_sha256: Option<Sha256Digest>,
    savestate_compatible: bool,
    created_at: &'a CreatedAt,
    last_commit_id: Option<CommitId>,
    snapshot_id: SnapshotId,
    session_epoch: SessionEpoch,
    pending_commits_sha256: Sha256Digest,
}

impl Serialize for ResumePackageManifest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.validate().map_err(serde::ser::Error::custom)?;
        SerializableResumePackageManifest {
            package_version: self.package_version,
            character_id: self.character_id,
            parent_revision: self.parent_revision,
            revision: self.revision,
            game_build_id: &self.game_build_id,
            rom_sha256: self.rom_sha256,
            mgba_version: &self.mgba_version,
            bridge_abi: self.bridge_abi,
            protocol_version: self.protocol_version,
            save_sha256: self.save_sha256,
            savestate_sha256: self.savestate_sha256,
            savestate_compatible: self.savestate_compatible,
            created_at: &self.created_at,
            last_commit_id: self.last_commit_id,
            snapshot_id: self.snapshot_id,
            session_epoch: self.session_epoch,
            pending_commits_sha256: self.pending_commits_sha256,
        }
        .serialize(serializer)
    }
}

impl ResumePackageManifest {
    /// Constructs and validates a version-one manifest.
    ///
    /// # Errors
    ///
    /// Returns an error for unsupported revisions, versions, or metadata.
    #[expect(
        clippy::too_many_arguments,
        reason = "the signed manifest constructor mirrors its fixed wire fields"
    )]
    pub fn new(
        character_id: CharacterId,
        parent_revision: Revision,
        revision: Revision,
        build: ManifestBuildInfo,
        save_sha256: Sha256Digest,
        savestate_sha256: Option<Sha256Digest>,
        savestate_compatible: bool,
        created_at: CreatedAt,
    ) -> Result<Self, ResumeError> {
        let manifest = Self {
            package_version: RESUME_MANIFEST_VERSION,
            character_id,
            parent_revision,
            revision,
            game_build_id: build.game_build_id,
            rom_sha256: build.rom_sha256,
            mgba_version: build.mgba_version,
            bridge_abi: build.bridge_abi,
            protocol_version: build.protocol_version,
            save_sha256,
            savestate_sha256,
            savestate_compatible,
            created_at,
            last_commit_id: build.last_commit_id,
            snapshot_id: build.snapshot_id,
            session_epoch: build.session_epoch,
            pending_commits_sha256: build.pending_commits_sha256,
        };
        manifest.validate()?;
        Ok(manifest)
    }

    /// Validates all required manifest fields and optional savestate rules.
    ///
    /// # Errors
    ///
    /// Returns an error for unsupported values or inconsistent savestate metadata.
    pub fn validate(&self) -> Result<(), ResumeError> {
        if self.package_version != RESUME_MANIFEST_VERSION {
            return Err(ResumeError::UnknownManifestVersion(self.package_version));
        }
        if self.revision.is_initial() || self.parent_revision.next() != Ok(self.revision) {
            return Err(ResumeError::InvalidSnapshotRevision);
        }
        if self.session_epoch.value() == 0 {
            return Err(ResumeError::InvalidSnapshotRevision);
        }
        if self.savestate_compatible != self.savestate_sha256.is_some() {
            return Err(ResumeError::SavestateFlagMismatch);
        }
        Ok(())
    }

    #[must_use]
    pub const fn snapshot_revision(&self) -> Revision {
        self.revision
    }

    /// Returns the finalized snapshot provenance carried by this package.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the lease epoch that authorized this package.
    #[must_use]
    pub const fn session_epoch(&self) -> SessionEpoch {
        self.session_epoch
    }

    /// Returns canonical RFC8785 JSON bytes for detached signing.
    ///
    /// # Errors
    ///
    /// Returns an error when validation or canonicalization fails.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ResumeError> {
        self.validate()?;
        serde_jcs::to_vec(self).map_err(|error| ResumeError::Canonicalization(error.to_string()))
    }

    /// Selects an optional savestate after validating the manifest first.
    ///
    /// # Errors
    ///
    /// Returns only hard errors for malformed metadata or corrupt canonical
    /// SAV; optional savestate problems are explicit fallback decisions.
    pub fn select_resume(
        &self,
        target: &CompatibilityTarget,
        character_sav: &[u8],
        resume_ss1: Option<&[u8]>,
    ) -> Result<ResumeSelection, CompatibilityError> {
        self.validate()
            .map_err(|_| CompatibilityError::ManifestInvalid)?;
        if Sha256Digest::of_bytes(character_sav) != self.save_sha256 {
            return Err(CompatibilityError::CorruptSav);
        }
        if !self.savestate_compatible {
            return Ok(ResumeSelection::FallbackToSav(
                SavestateFallbackReason::NotMarkedCompatible,
            ));
        }
        if !target.matches(self) {
            return Ok(ResumeSelection::FallbackToSav(
                SavestateFallbackReason::MetadataMismatch,
            ));
        }
        let Some(bytes) = resume_ss1 else {
            return Ok(ResumeSelection::FallbackToSav(
                SavestateFallbackReason::MissingSavestate,
            ));
        };
        if Sha256Digest::of_bytes(bytes)
            != self
                .savestate_sha256
                .ok_or(CompatibilityError::ManifestInvalid)?
        {
            return Ok(ResumeSelection::FallbackToSav(
                SavestateFallbackReason::CorruptSavestate,
            ));
        }
        Ok(ResumeSelection::UseSavestate)
    }

    /// Alias for compatibility evaluation.
    ///
    /// # Errors
    ///
    /// Propagates the compatibility result from [`Self::select_resume`].
    pub fn evaluate_compatibility(
        &self,
        target: &CompatibilityTarget,
        character_sav: &[u8],
        resume_ss1: Option<&[u8]>,
    ) -> Result<ResumeSelection, CompatibilityError> {
        self.select_resume(target, character_sav, resume_ss1)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireManifest {
    package_version: u16,
    character_id: CharacterId,
    parent_revision: Revision,
    revision: Revision,
    game_build_id: GameBuildId,
    rom_sha256: Sha256Digest,
    mgba_version: MgbaVersion,
    bridge_abi: BridgeAbiVersion,
    protocol_version: ProtocolVersion,
    save_sha256: Sha256Digest,
    savestate_sha256: Option<Sha256Digest>,
    savestate_compatible: bool,
    created_at: CreatedAt,
    last_commit_id: Option<CommitId>,
    snapshot_id: SnapshotId,
    session_epoch: SessionEpoch,
    pending_commits_sha256: Sha256Digest,
}

impl<'de> Deserialize<'de> for ResumePackageManifest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = WireManifest::deserialize(deserializer)?;
        let manifest = Self {
            package_version: wire.package_version,
            character_id: wire.character_id,
            parent_revision: wire.parent_revision,
            revision: wire.revision,
            game_build_id: wire.game_build_id,
            rom_sha256: wire.rom_sha256,
            mgba_version: wire.mgba_version,
            bridge_abi: wire.bridge_abi,
            protocol_version: wire.protocol_version,
            save_sha256: wire.save_sha256,
            savestate_sha256: wire.savestate_sha256,
            savestate_compatible: wire.savestate_compatible,
            created_at: wire.created_at,
            last_commit_id: wire.last_commit_id,
            snapshot_id: wire.snapshot_id,
            session_epoch: wire.session_epoch,
            pending_commits_sha256: wire.pending_commits_sha256,
        };
        manifest.validate().map_err(serde::de::Error::custom)?;
        Ok(manifest)
    }
}

/// The detached signature and trust metadata for a manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignedManifestEnvelope {
    pub package_version: u16,
    pub signing_algorithm_version: u16,
    pub signing_key_id: String,
    pub manifest: ResumePackageManifest,
    pub signature: ManifestSignature,
}

#[derive(Serialize)]
struct SerializableSignedManifestEnvelope<'a> {
    package_version: u16,
    signing_algorithm_version: u16,
    signing_key_id: &'a str,
    manifest: &'a ResumePackageManifest,
    signature: ManifestSignature,
}

impl Serialize for SignedManifestEnvelope {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.validate().map_err(serde::ser::Error::custom)?;
        SerializableSignedManifestEnvelope {
            package_version: self.package_version,
            signing_algorithm_version: self.signing_algorithm_version,
            signing_key_id: &self.signing_key_id,
            manifest: &self.manifest,
            signature: self.signature,
        }
        .serialize(serializer)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireSignedManifestEnvelope {
    package_version: u16,
    signing_algorithm_version: u16,
    #[serde(deserialize_with = "deserialize_signing_key_id")]
    signing_key_id: String,
    manifest: ResumePackageManifest,
    signature: ManifestSignature,
}

fn deserialize_signing_key_id<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_string(deserializer, 128, "signing key ID")
}

impl<'de> Deserialize<'de> for SignedManifestEnvelope {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = WireSignedManifestEnvelope::deserialize(deserializer)?;
        let envelope = Self {
            package_version: wire.package_version,
            signing_algorithm_version: wire.signing_algorithm_version,
            signing_key_id: wire.signing_key_id,
            manifest: wire.manifest,
            signature: wire.signature,
        };
        envelope.validate().map_err(serde::de::Error::custom)?;
        Ok(envelope)
    }
}

impl SignedManifestEnvelope {
    /// Signs all manifest fields with Ed25519 over JCS bytes.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid manifests or key IDs.
    pub fn sign(
        manifest: ResumePackageManifest,
        key: &SigningPrivateKey,
        signing_key_id: impl Into<String>,
    ) -> Result<Self, ResumeError> {
        let signing_key_id = validate_key_id(signing_key_id.into())?;
        let signature = ed25519_dalek::Signer::sign(&key.to_dalek(), &manifest.canonical_bytes()?);
        Ok(Self {
            package_version: RESUME_MANIFEST_VERSION,
            signing_algorithm_version: SIGNING_ALGORITHM_VERSION,
            signing_key_id,
            manifest,
            signature: ManifestSignature(signature.to_bytes()),
        })
    }

    /// Verifies using a separately supplied pinned key.
    ///
    /// # Errors
    ///
    /// Returns an error for unknown versions, key mismatch, or tampering.
    pub fn verify(&self, trusted: &TrustedManifestKey) -> Result<(), ResumeError> {
        self.validate()?;
        if self.signing_key_id != trusted.key_id {
            return Err(ResumeError::UntrustedSigningKey);
        }
        trusted
            .verifying_key
            .verify_strict(
                &self.manifest.canonical_bytes()?,
                &Signature::from_bytes(self.signature.as_bytes()),
            )
            .map_err(|_| ResumeError::InvalidSignature)
    }

    /// Validates detached envelope metadata before verification.
    ///
    /// # Errors
    ///
    /// Returns an error for unsupported versions, invalid key IDs, or an
    /// invalid nested manifest.
    pub fn validate(&self) -> Result<(), ResumeError> {
        if self.package_version != RESUME_MANIFEST_VERSION {
            return Err(ResumeError::UnknownManifestVersion(self.package_version));
        }
        if self.signing_algorithm_version != SIGNING_ALGORITHM_VERSION {
            return Err(ResumeError::UnknownSigningAlgorithm(
                self.signing_algorithm_version,
            ));
        }
        validate_key_id(self.signing_key_id.clone())?;
        self.manifest.validate()
    }
}

/// Compatibility identity expected by the current launcher.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompatibilityTarget {
    pub game_build_id: GameBuildId,
    pub rom_sha256: Sha256Digest,
    pub mgba_version: MgbaVersion,
    pub bridge_abi: BridgeAbiVersion,
    pub protocol_version: ProtocolVersion,
    pub revision: Revision,
}

impl CompatibilityTarget {
    #[must_use]
    pub fn new(
        game_build_id: GameBuildId,
        rom_sha256: Sha256Digest,
        mgba_version: MgbaVersion,
        bridge_abi: BridgeAbiVersion,
        protocol_version: ProtocolVersion,
        revision: Revision,
    ) -> Self {
        Self {
            game_build_id,
            rom_sha256,
            mgba_version,
            bridge_abi,
            protocol_version,
            revision,
        }
    }

    #[must_use]
    pub fn matches(&self, manifest: &ResumePackageManifest) -> bool {
        self.game_build_id == manifest.game_build_id
            && self.rom_sha256 == manifest.rom_sha256
            && self.mgba_version == manifest.mgba_version
            && self.bridge_abi == manifest.bridge_abi
            && self.protocol_version == manifest.protocol_version
            && self.revision == manifest.revision
    }

    /// Returns the revision-independent identity of the runtime described by
    /// this compatibility target.
    #[must_use]
    pub fn runtime_build_identity(&self) -> crate::RuntimeBuildIdentity {
        crate::RuntimeBuildIdentity::from_compatibility_target(self)
    }
}

/// A launcher trust root kept outside the package being verified.
#[derive(Clone)]
pub struct TrustedManifestKey {
    key_id: String,
    verifying_key: VerifyingKey,
}

impl TrustedManifestKey {
    /// Creates a strong trusted Ed25519 verification root.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed IDs, invalid keys, or weak points.
    pub fn new(key_id: impl Into<String>, bytes: [u8; 32]) -> Result<Self, ResumeError> {
        let key_id = validate_key_id(key_id.into())?;
        let verifying_key =
            VerifyingKey::from_bytes(&bytes).map_err(|_| ResumeError::InvalidKey)?;
        if verifying_key.is_weak() {
            return Err(ResumeError::WeakSigningKey);
        }
        Ok(Self {
            key_id,
            verifying_key,
        })
    }

    #[must_use]
    pub fn key_id(&self) -> &str {
        &self.key_id
    }
}

fn validate_key_id(value: String) -> Result<String, ResumeError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return Err(ResumeError::InvalidSigningKeyId);
    }
    Ok(value)
}

/// Explicit launcher action after compatibility checks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResumeSelection {
    UseSavestate,
    FallbackToSav(SavestateFallbackReason),
}

/// Why an optional savestate was not selected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SavestateFallbackReason {
    NotMarkedCompatible,
    MetadataMismatch,
    MissingSavestate,
    CorruptSavestate,
}

/// Hard failures must not be silently downgraded.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum CompatibilityError {
    #[error("canonical character.sav is corrupt")]
    CorruptSav,
    #[error("resume manifest is invalid")]
    ManifestInvalid,
}

/// Backwards-compatible package artifact wrapper using fixed identities.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResumePackageArtifact {
    pub artifact: ArtifactIdentity,
    pub sha256: Sha256Digest,
    pub size_bytes: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireResumePackageArtifact {
    artifact: ArtifactIdentity,
    sha256: Sha256Digest,
    size_bytes: u64,
}

impl<'de> Deserialize<'de> for ResumePackageArtifact {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = WireResumePackageArtifact::deserialize(deserializer)?;
        Self::new(wire.artifact, wire.sha256, wire.size_bytes).map_err(serde::de::Error::custom)
    }
}

impl ResumePackageArtifact {
    /// Creates fixed-identity package metadata.
    ///
    /// # Errors
    ///
    /// This constructor currently cannot fail; the result form preserves the
    /// validated-constructor convention used by snapshot consumers.
    pub fn new(
        artifact: ArtifactIdentity,
        sha256: Sha256Digest,
        size_bytes: u64,
    ) -> Result<Self, ResumeError> {
        Ok(Self {
            artifact,
            sha256,
            size_bytes,
        })
    }

    /// Computes a fixed-identity artifact digest.
    ///
    /// # Errors
    ///
    /// Returns an error only if future identity validation rejects the value.
    pub fn from_bytes(artifact: ArtifactIdentity, bytes: &[u8]) -> Result<Self, ResumeError> {
        Self::new(artifact, Sha256Digest::of_bytes(bytes), bytes.len() as u64)
    }

    /// Verifies bytes against this artifact record.
    ///
    /// # Errors
    ///
    /// Returns an error when size or digest differs.
    pub fn verify_bytes(&self, bytes: &[u8]) -> Result<(), ResumeError> {
        if self.size_bytes != bytes.len() as u64 || Sha256Digest::of_bytes(bytes) != self.sha256 {
            return Err(ResumeError::ArtifactDigestMismatch {
                path: self.artifact.as_str().to_owned(),
            });
        }
        Ok(())
    }
}

pub type PackageArtifact = ResumePackageArtifact;
pub type ManifestSigningKey = SigningPrivateKey;
