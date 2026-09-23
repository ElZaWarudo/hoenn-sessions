//! Fail-closed verification and atomic publication of the Windows runtime set.
//!
//! This module deliberately has no network or process-launching code.  The
//! caller supplies the bounded signed envelope and the bytes obtained from its
//! own transport.  Trust, compatibility, byte identity, and publication are
//! all checked before a generation can become visible.

#![allow(clippy::module_name_repetitions)]

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

/// The only release descriptor schema accepted by this launcher.
pub const RELEASE_SCHEMA: u16 = 1;
/// The only first-release target accepted by this launcher.
pub const WINDOWS_PLATFORM: &str = "windows-x86_64";
/// Maximum encoded envelope size accepted before JSON parsing.
pub const MAX_ENVELOPE_BYTES: usize = 1024 * 1024;
/// Maximum signed payload size accepted before descriptor parsing.
pub const MAX_PAYLOAD_BYTES: usize = 512 * 1024;
/// Maximum release identifier length in bytes.
pub const MAX_RELEASE_ID_BYTES: usize = 128;
/// Maximum size of one runtime artifact.
pub const MAX_ARTIFACT_BYTES: u64 = 512 * 1024 * 1024;
/// Maximum signed release lifetime accepted by the launcher.
pub const MAX_RELEASE_LIFETIME_SECONDS: i64 = 90 * 24 * 60 * 60;
/// Clock skew tolerated for a future-issued descriptor.
pub const MAX_CLOCK_SKEW_SECONDS: i64 = 5 * 60;
/// Name of the private generation directory below a caller-owned root.
pub const GENERATIONS_DIRECTORY: &str = "generations";
/// Completion evidence written last in every generation.
pub const COMPLETE_MARKER: &str = ".complete";
/// Exact signed envelope persisted before completion evidence.
pub const SIGNED_RELEASE_ENVELOPE: &str = ".signed-release";

const MAX_KEY_ID_BYTES: usize = 64;
const MAX_MARKER_BYTES: usize = 512;
const MAX_HEAD_BYTES: usize = 1024;
const STAGING_PREFIX: &str = ".staging-";
const QUARANTINE_PREFIX: &str = ".quarantine-";
const CURRENT_MARKER_PREFIX: &str = ".accepted-generation-";
const CURRENT_MARKER_TEMP_PREFIX: &str = ".accepted-generation-staging-";
const ACCEPTED_HEAD_PREFIX: &str = ".accepted-head-";
const ACCEPTED_HEAD_TEMP_PREFIX: &str = ".accepted-head-staging-";

/// Fixed identities in every Windows runtime release set.
///
/// The destination for each identity is fixed by [`ArtifactIdentity::destination`];
/// the signed descriptor cannot provide a path.  The bootstrapper is MSI-owned
/// and deliberately absent; the bridge is represented by
/// its three shipped Lua files plus the generated address table, while the
/// compatibility, trust, and notices assets remain separate files so each is
/// hash checked independently.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub enum ArtifactIdentity {
    DesktopApp,
    Bootstrapper,
    ManagedMgba,
    Rom,
    Sidecar,
    BridgeMain,
    BridgeMemory,
    BridgeProtocol,
    BridgeAddresses,
    CompatibilityManifest,
    TrustBundle,
    Notices,
}

/// The complete fixed runtime artifact set, in canonical order.
///
/// The bootstrapper is intentionally absent: it is MSI-owned and must never
/// be self-replaced by a running launcher.  The stable bootstrap selects one
/// of these immutable runtime generations, while MSI updates the bootstrapper
/// independently.
pub const FIXED_ARTIFACT_IDENTITIES: [ArtifactIdentity; 11] = [
    ArtifactIdentity::DesktopApp,
    ArtifactIdentity::ManagedMgba,
    ArtifactIdentity::Rom,
    ArtifactIdentity::Sidecar,
    ArtifactIdentity::BridgeMain,
    ArtifactIdentity::BridgeMemory,
    ArtifactIdentity::BridgeProtocol,
    ArtifactIdentity::BridgeAddresses,
    ArtifactIdentity::CompatibilityManifest,
    ArtifactIdentity::TrustBundle,
    ArtifactIdentity::Notices,
];

impl ArtifactIdentity {
    /// Returns all fixed identities in descriptor order.
    pub const fn all() -> &'static [Self] {
        &FIXED_ARTIFACT_IDENTITIES
    }

    /// Returns the canonical wire identity.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DesktopApp => "desktop-app",
            Self::Bootstrapper => "bootstrapper",
            Self::ManagedMgba => "managed-mgba",
            Self::Rom => "rom",
            Self::Sidecar => "sidecar",
            Self::BridgeMain => "bridge-main",
            Self::BridgeMemory => "bridge-memory",
            Self::BridgeProtocol => "bridge-protocol",
            Self::BridgeAddresses => "bridge-addresses",
            Self::CompatibilityManifest => "compatibility-manifest",
            Self::TrustBundle => "trust-bundle",
            Self::Notices => "notices",
        }
    }

    /// Returns the only relative destination accepted for this identity.
    pub const fn destination(self) -> &'static str {
        match self {
            Self::DesktopApp => "app/coop-launcher.exe",
            Self::Bootstrapper => "app/coop-bootstrapper.exe",
            Self::ManagedMgba => "runtime/mgba.exe",
            Self::Rom => "runtime/game.gba",
            Self::Sidecar => "runtime/coop-sidecar.exe",
            Self::BridgeMain => "bridge/main.lua",
            Self::BridgeMemory => "bridge/memory.lua",
            Self::BridgeProtocol => "bridge/protocol.lua",
            Self::BridgeAddresses => "bridge/generated_addresses.lua",
            Self::CompatibilityManifest => "bridge_manifest.json",
            Self::TrustBundle => "trust/release-trust.json",
            Self::Notices => "THIRD_PARTY_NOTICES.txt",
        }
    }

    fn from_wire(value: &str) -> Result<Self, UpdateError> {
        match value {
            "desktop-app" => Ok(Self::DesktopApp),
            "bootstrapper" => Ok(Self::Bootstrapper),
            "managed-mgba" | "mgba" => Ok(Self::ManagedMgba),
            "rom" => Ok(Self::Rom),
            "sidecar" => Ok(Self::Sidecar),
            "bridge-main" => Ok(Self::BridgeMain),
            "bridge-memory" => Ok(Self::BridgeMemory),
            "bridge-protocol" => Ok(Self::BridgeProtocol),
            "bridge-addresses" => Ok(Self::BridgeAddresses),
            "compatibility-manifest" | "manifest" => Ok(Self::CompatibilityManifest),
            "trust-bundle" | "trust" => Ok(Self::TrustBundle),
            "notices" => Ok(Self::Notices),
            _ => Err(UpdateError::UnknownArtifact(value.to_owned())),
        }
    }
}

impl fmt::Display for ArtifactIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for ArtifactIdentity {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ArtifactIdentity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::from_wire(&value).map_err(serde::de::Error::custom)
    }
}

/// A fixed artifact's signed size and SHA-256 digest.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactDescriptor {
    /// Fixed artifact identity.
    pub id: ArtifactIdentity,
    /// Exact byte length expected from the transport.
    pub size: u64,
    /// Lower-case hexadecimal SHA-256 digest.
    pub sha256: String,
}

impl ArtifactDescriptor {
    /// Creates a descriptor from trusted bytes, primarily for release tooling
    /// and deterministic tests.
    pub fn from_bytes(id: ArtifactIdentity, bytes: &[u8]) -> Self {
        Self {
            id,
            size: bytes.len() as u64,
            sha256: hex_digest(bytes),
        }
    }

    fn digest_bytes(&self) -> Result<[u8; 32], UpdateError> {
        parse_digest(&self.sha256)
    }
}

/// Signed metadata describing one complete compatible release set.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseDescriptor {
    /// Release schema version.
    pub schema: u16,
    /// Immutable release identifier used as the generation directory name.
    pub release_id: String,
    /// Monotonic release sequence used to reject replay and rollback.
    pub sequence: u64,
    /// Unix timestamp at which this descriptor became valid.
    pub issued_at: i64,
    /// Unix timestamp after which this descriptor is no longer valid.
    pub expires_at: i64,
    /// Target platform and architecture.
    pub platform: String,
    /// Exactly one descriptor for every [`FIXED_ARTIFACT_IDENTITIES`] entry.
    pub artifacts: Vec<ArtifactDescriptor>,
}

impl ReleaseDescriptor {
    /// Validates all structural, compatibility, identity, and bound checks.
    pub fn validate(&self) -> Result<(), UpdateError> {
        if self.schema != RELEASE_SCHEMA {
            return Err(UpdateError::UnsupportedSchema(self.schema));
        }
        if self.platform != WINDOWS_PLATFORM {
            return Err(UpdateError::UnsupportedPlatform(self.platform.clone()));
        }
        validate_release_id(&self.release_id)?;
        if self.artifacts.len() > FIXED_ARTIFACT_IDENTITIES.len() {
            return Err(UpdateError::TooManyArtifacts(self.artifacts.len()));
        }

        let mut seen = BTreeSet::new();
        for artifact in &self.artifacts {
            if !seen.insert(artifact.id) {
                return Err(UpdateError::DuplicateArtifact(artifact.id));
            }
            if artifact.size == 0 || artifact.size > MAX_ARTIFACT_BYTES {
                return Err(UpdateError::InvalidArtifactSize {
                    artifact: artifact.id,
                    size: artifact.size,
                });
            }
            artifact.digest_bytes()?;
        }
        for expected in FIXED_ARTIFACT_IDENTITIES {
            if !seen.contains(&expected) {
                return Err(UpdateError::MissingArtifact(expected));
            }
        }
        Ok(())
    }

    /// Validates timestamp bounds against a caller-supplied Unix clock.
    pub fn validate_at(&self, now: i64) -> Result<(), UpdateError> {
        self.validate()?;
        if self.issued_at > now.saturating_add(MAX_CLOCK_SKEW_SECONDS) {
            return Err(UpdateError::ReleaseIssuedInFuture(self.issued_at));
        }
        if self.expires_at <= self.issued_at
            || self.expires_at.saturating_sub(self.issued_at) > MAX_RELEASE_LIFETIME_SECONDS
        {
            return Err(UpdateError::InvalidReleaseLifetime);
        }
        if self.expires_at <= now {
            return Err(UpdateError::ReleaseExpired(self.expires_at));
        }
        Ok(())
    }

    /// Returns the descriptor for one fixed identity.
    pub fn artifact(&self, identity: ArtifactIdentity) -> Option<&ArtifactDescriptor> {
        self.artifacts
            .iter()
            .find(|artifact| artifact.id == identity)
    }
}

/// A release descriptor accepted only after its signature and key id verify.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedRelease {
    descriptor: ReleaseDescriptor,
    payload: Vec<u8>,
    payload_sha256: [u8; 32],
    signed_envelope: Vec<u8>,
}

impl VerifiedRelease {
    /// Returns the verified descriptor.
    pub const fn descriptor(&self) -> &ReleaseDescriptor {
        &self.descriptor
    }

    /// Returns the immutable release identifier.
    pub fn release_id(&self) -> &str {
        &self.descriptor.release_id
    }

    /// Returns the exact signed descriptor bytes.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Returns the exact bounded signed envelope bytes that were verified.
    pub fn signed_envelope(&self) -> &[u8] {
        &self.signed_envelope
    }

    /// Returns the payload SHA-256 used by completion evidence.
    pub fn payload_sha256(&self) -> [u8; 32] {
        self.payload_sha256
    }
}

/// The exact, bounded completion evidence persisted beside a generation.
///
/// The accepted-generation records repeat the signed release sequence and
/// validity window as well as the signed payload digest.  On restart those
/// values are checked against the complete generation before they can become
/// the rollback floor, so a partially-written or hand-edited record cannot
/// silently advance the accepted state.
#[derive(Clone, Debug, Eq, PartialEq)]
struct CompletionMarker {
    schema: u16,
    platform: String,
    release_id: String,
    sequence: u64,
    issued_at: i64,
    expires_at: i64,
    payload_sha256: [u8; 32],
}

impl CompletionMarker {
    fn from_release(release: &VerifiedRelease) -> Self {
        Self {
            schema: RELEASE_SCHEMA,
            platform: WINDOWS_PLATFORM.to_owned(),
            release_id: release.release_id().to_owned(),
            sequence: release.descriptor.sequence,
            issued_at: release.descriptor.issued_at,
            expires_at: release.descriptor.expires_at,
            payload_sha256: release.payload_sha256,
        }
    }

    fn encode(&self) -> Vec<u8> {
        format!(
            "schema={}\nplatform={}\nrelease_id={}\nsequence={}\nissued_at={}\nexpires_at={}\npayload_sha256={}\n",
            self.schema,
            self.platform,
            self.release_id,
            self.sequence,
            self.issued_at,
            self.expires_at,
            hex_bytes(&self.payload_sha256),
        )
        .into_bytes()
    }

    fn matches_release(&self, release: &VerifiedRelease) -> bool {
        self == &Self::from_release(release)
    }

    fn validate_structure(&self) -> bool {
        self.schema == RELEASE_SCHEMA
            && self.platform == WINDOWS_PLATFORM
            && validate_release_id(&self.release_id).is_ok()
            && self.expires_at > self.issued_at
            && self
                .expires_at
                .checked_sub(self.issued_at)
                .is_some_and(|lifetime| lifetime <= MAX_RELEASE_LIFETIME_SECONDS)
    }
}

/// An append-only head record that makes marker-history truncation fail
/// closed.  The marker file remains the complete-generation evidence; this
/// record is the durable continuity invariant that tells cold-start recovery
/// which marker must still exist.
#[derive(Clone, Debug, Eq, PartialEq)]
struct AcceptedHead {
    marker_file: String,
    marker_sha256: [u8; 32],
    marker: CompletionMarker,
}

impl AcceptedHead {
    fn encode(&self) -> Vec<u8> {
        let marker = String::from_utf8(self.marker.encode())
            .expect("completion marker is ASCII by construction");
        format!(
            "marker_file={}\nmarker_sha256={}\n{}",
            self.marker_file,
            hex_bytes(&self.marker_sha256),
            marker,
        )
        .into_bytes()
    }
}

/// A pinned Ed25519 release-verification key.
#[derive(Clone, Debug)]
pub struct TrustedReleaseKey {
    key_id: String,
    verifying_key: VerifyingKey,
}

impl TrustedReleaseKey {
    /// Creates a key after enforcing a bounded, unambiguous key id and a
    /// valid strong Ed25519 public key.
    pub fn new(key_id: impl Into<String>, public_key: [u8; 32]) -> Result<Self, UpdateError> {
        let key_id = key_id.into();
        validate_key_id(&key_id)?;
        let verifying_key =
            VerifyingKey::from_bytes(&public_key).map_err(|_| UpdateError::InvalidTrustedKey)?;
        Ok(Self {
            key_id,
            verifying_key,
        })
    }

    /// Returns the pinned key id.
    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    /// Returns the raw public-key bytes for diagnostics or configuration.
    pub fn public_key(&self) -> [u8; 32] {
        self.verifying_key.to_bytes()
    }
}

/// Wire envelope for an exact signed payload.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SignedReleaseEnvelope {
    /// Envelope schema version.
    pub schema: u16,
    /// Key id selected by the signer.
    pub key_id: String,
    /// Standard-base64 exact descriptor bytes.
    pub payload: String,
    /// Standard-base64 Ed25519 signature over decoded `payload` bytes.
    pub signature: String,
}

impl SignedReleaseEnvelope {
    /// Verifies a bounded JSON envelope and only then parses its descriptor.
    pub fn verify_json(
        envelope_bytes: &[u8],
        trusted_key: &TrustedReleaseKey,
    ) -> Result<VerifiedRelease, UpdateError> {
        if envelope_bytes.len() > MAX_ENVELOPE_BYTES {
            return Err(UpdateError::EnvelopeTooLarge(envelope_bytes.len()));
        }
        let envelope: Self =
            serde_json::from_slice(envelope_bytes).map_err(|_| UpdateError::MalformedEnvelope)?;
        let mut verified = envelope.verify(trusted_key)?;
        verified.signed_envelope = envelope_bytes.to_vec();
        Ok(verified)
    }

    /// Verifies this already-decoded envelope and then parses its exact bytes.
    pub fn verify(&self, trusted_key: &TrustedReleaseKey) -> Result<VerifiedRelease, UpdateError> {
        if self.schema != RELEASE_SCHEMA {
            return Err(UpdateError::UnsupportedSchema(self.schema));
        }
        if self.key_id != trusted_key.key_id {
            return Err(UpdateError::KeyIdMismatch);
        }

        let payload = decode_canonical_base64(&self.payload, MAX_PAYLOAD_BYTES)
            .map_err(|_| UpdateError::MalformedEnvelope)?;
        let signature_bytes = decode_canonical_base64(&self.signature, 64)
            .map_err(|_| UpdateError::MalformedEnvelope)?;
        let signature =
            Signature::from_slice(&signature_bytes).map_err(|_| UpdateError::MalformedEnvelope)?;
        trusted_key
            .verifying_key
            .verify(&payload, &signature)
            .map_err(|_| UpdateError::SignatureInvalid)?;

        // This is intentionally the first descriptor parse.  No metadata is
        // trusted before the exact signed payload has been authenticated.
        let descriptor: ReleaseDescriptor =
            serde_json::from_slice(&payload).map_err(|_| UpdateError::MalformedDescriptor)?;
        descriptor.validate()?;
        let payload_sha256 = Sha256::digest(&payload);
        let mut payload_digest = [0_u8; 32];
        payload_digest.copy_from_slice(&payload_sha256);
        Ok(VerifiedRelease {
            descriptor,
            payload,
            payload_sha256: payload_digest,
            signed_envelope: serde_json::to_vec(self)
                .map_err(|_| UpdateError::MalformedEnvelope)?,
        })
    }

    /// Encodes an exact payload and signature as the canonical wire envelope.
    pub fn sign_payload(
        payload: &[u8],
        key_id: impl Into<String>,
        signing_key: &ed25519_dalek::SigningKey,
    ) -> Result<Vec<u8>, UpdateError> {
        if payload.len() > MAX_PAYLOAD_BYTES {
            return Err(UpdateError::PayloadTooLarge(payload.len()));
        }
        let key_id = key_id.into();
        validate_key_id(&key_id)?;
        let signature = ed25519_dalek::Signer::sign(signing_key, payload);
        let envelope = Self {
            schema: RELEASE_SCHEMA,
            key_id,
            payload: BASE64.encode(payload),
            signature: BASE64.encode(signature.to_bytes()),
        };
        serde_json::to_vec(&envelope).map_err(|_| UpdateError::MalformedEnvelope)
    }
}

/// Verifies a signed release envelope using a pinned public key.
pub fn verify_signed_release(
    envelope_bytes: &[u8],
    trusted_key: &TrustedReleaseKey,
) -> Result<VerifiedRelease, UpdateError> {
    SignedReleaseEnvelope::verify_json(envelope_bytes, trusted_key)
}

/// One transport-supplied artifact body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactPayload {
    /// Fixed artifact identity.
    pub identity: ArtifactIdentity,
    /// Bytes whose size and digest must match the signed descriptor.
    pub bytes: Vec<u8>,
}

impl ArtifactPayload {
    /// Creates one payload entry.
    pub fn new(identity: ArtifactIdentity, bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            identity,
            bytes: bytes.into(),
        }
    }
}

/// A validated-friendly collection of transport payloads.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ArtifactSet {
    entries: Vec<ArtifactPayload>,
}

impl ArtifactSet {
    /// Creates a collection. Duplicate identities are rejected during install
    /// so callers can construct this value without silently dropping bytes.
    pub fn new(entries: impl IntoIterator<Item = ArtifactPayload>) -> Self {
        Self {
            entries: entries.into_iter().collect(),
        }
    }

    /// Returns the entries in caller order.
    pub fn entries(&self) -> &[ArtifactPayload] {
        &self.entries
    }
}

/// Sources accepted by [`GenerationStore::install`].
pub trait ArtifactSource {
    /// Copies source entries into a bounded, duplicate-preserving list.
    fn collect_entries(self) -> Result<Vec<ArtifactPayload>, UpdateError>;
}

impl ArtifactSource for ArtifactSet {
    fn collect_entries(self) -> Result<Vec<ArtifactPayload>, UpdateError> {
        Ok(self.entries)
    }
}

impl ArtifactSource for &ArtifactSet {
    fn collect_entries(self) -> Result<Vec<ArtifactPayload>, UpdateError> {
        Ok(self.entries.clone())
    }
}

impl ArtifactSource for Vec<ArtifactPayload> {
    fn collect_entries(self) -> Result<Vec<ArtifactPayload>, UpdateError> {
        Ok(self)
    }
}

impl ArtifactSource for &[ArtifactPayload] {
    fn collect_entries(self) -> Result<Vec<ArtifactPayload>, UpdateError> {
        Ok(self.to_vec())
    }
}

impl ArtifactSource for Vec<(ArtifactIdentity, Vec<u8>)> {
    fn collect_entries(self) -> Result<Vec<ArtifactPayload>, UpdateError> {
        Ok(self
            .into_iter()
            .map(|(identity, bytes)| ArtifactPayload::new(identity, bytes))
            .collect())
    }
}

impl ArtifactSource for BTreeMap<ArtifactIdentity, Vec<u8>> {
    fn collect_entries(self) -> Result<Vec<ArtifactPayload>, UpdateError> {
        Ok(self
            .into_iter()
            .map(|(identity, bytes)| ArtifactPayload::new(identity, bytes))
            .collect())
    }
}

impl ArtifactSource for &BTreeMap<ArtifactIdentity, Vec<u8>> {
    fn collect_entries(self) -> Result<Vec<ArtifactPayload>, UpdateError> {
        Ok(self
            .iter()
            .map(|(identity, bytes)| ArtifactPayload::new(*identity, bytes.clone()))
            .collect())
    }
}

/// A private store of immutable complete generations.
#[derive(Clone, Debug)]
pub struct GenerationStore {
    root: PathBuf,
    generations: PathBuf,
}

/// Compatibility alias for callers that prefer the release-oriented name.
pub type ReleaseStore = GenerationStore;

/// Result of activating or reusing one generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstalledGeneration {
    path: PathBuf,
    release_id: String,
    reused: bool,
}

impl InstalledGeneration {
    /// Returns the complete immutable generation path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the release identifier.
    pub fn release_id(&self) -> &str {
        &self.release_id
    }

    /// Returns true when an already complete generation was revalidated and
    /// reused without replacing any files.
    pub const fn reused(&self) -> bool {
        self.reused
    }
}

#[derive(Clone, Debug)]
struct ArtifactRecord {
    path: PathBuf,
    size: u64,
    digest: [u8; 32],
}

/// A fixed artifact path and its signed digest borrowed from an accepted
/// generation.  The borrow keeps the opaque generation guard alive for the
/// lifetime of the typed path/digest view.
pub struct GenerationArtifact<'a> {
    identity: ArtifactIdentity,
    record: &'a ArtifactRecord,
    _guard: &'a GenerationGuard,
}

impl<'a> GenerationArtifact<'a> {
    /// Returns the fixed identity.
    pub const fn identity(&self) -> ArtifactIdentity {
        self.identity
    }

    /// Returns the fixed absolute path inside the accepted generation.
    pub fn path(&self) -> &Path {
        &self.record.path
    }

    /// Returns the exact signed byte length.
    pub const fn size(&self) -> u64 {
        self.record.size
    }

    /// Returns the exact signed SHA-256 digest.
    pub const fn digest(&self) -> [u8; 32] {
        self.record.digest
    }
}

/// Opaque no-follow publication/spawn guard.  On Windows it owns deny-write /
/// deny-delete handles for the executable artifacts and relevant ancestors;
/// on other platforms it preserves the same API and lifetime boundary.
pub struct GenerationGuard {
    #[cfg(windows)]
    windows: WindowsAcceptedGuard,
}

/// A cold-opened accepted generation.  Keeping this value (or its handoff)
/// alive retains the generation guard until the caller has spawned its fixed
/// desktop/sidecar paths.
pub struct AcceptedGeneration {
    path: PathBuf,
    release_id: String,
    sequence: u64,
    artifacts: BTreeMap<ArtifactIdentity, ArtifactRecord>,
    guard: GenerationGuard,
}

impl AcceptedGeneration {
    /// Returns the accepted generation root.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the accepted signed release id.
    pub fn release_id(&self) -> &str {
        &self.release_id
    }

    /// Returns the accepted signed release sequence.
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Cold-opened generations are already complete and therefore reused.
    pub const fn reused(&self) -> bool {
        true
    }

    /// Returns a typed fixed artifact view tied to this generation guard.
    pub fn artifact(&self, identity: ArtifactIdentity) -> Option<GenerationArtifact<'_>> {
        self.artifacts
            .get(&identity)
            .map(|record| GenerationArtifact {
                identity,
                record,
                _guard: &self.guard,
            })
    }

    /// Transfers guard ownership to the spawn/backend handoff object.
    pub fn handoff(self) -> GenerationHandoff {
        GenerationHandoff { generation: self }
    }

    /// Alias for callers that prefer an explicit ownership-transfer name.
    pub fn into_handoff(self) -> GenerationHandoff {
        self.handoff()
    }
}

/// Explicit handoff object retained by bootstrap/backend code until process
/// creation has completed.  It owns the same opaque generation guard as the
/// cold-open result and exposes only fixed typed artifact views.
pub struct GenerationHandoff {
    generation: AcceptedGeneration,
}

impl GenerationHandoff {
    /// Returns the accepted generation root.
    pub fn path(&self) -> &Path {
        self.generation.path()
    }

    /// Returns the accepted signed release id.
    pub fn release_id(&self) -> &str {
        self.generation.release_id()
    }

    /// Returns a typed fixed artifact view tied to the retained guard.
    pub fn artifact(&self, identity: ArtifactIdentity) -> Option<GenerationArtifact<'_>> {
        self.generation.artifact(identity)
    }

    /// Returns the accepted generation after the caller has finished its
    /// spawn handoff.
    pub fn into_generation(self) -> AcceptedGeneration {
        self.generation
    }
}

impl GenerationStore {
    /// Creates a store under an owner-controlled root, rejecting existing
    /// symlink/reparse components and ensuring the generations directory is a
    /// real directory.
    pub fn new(root: impl AsRef<Path>) -> Result<Self, UpdateError> {
        let root = root.as_ref().to_path_buf();
        validate_store_path(&root)?;
        fs::create_dir_all(&root).map_err(UpdateError::Io)?;
        ensure_real_directory(&root)?;

        let generations = root.join(GENERATIONS_DIRECTORY);
        reject_unsafe_components(&generations)?;
        if generations.exists() {
            ensure_real_directory(&generations)?;
        } else {
            fs::create_dir(&generations).map_err(UpdateError::Io)?;
        }
        Ok(Self { root, generations })
    }

    /// Returns the caller-owned root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the private generations directory.
    pub fn generations_path(&self) -> &Path {
        &self.generations
    }

    /// Verifies every supplied byte and atomically publishes one complete
    /// generation.  No destination is taken from untrusted metadata.
    pub fn install<S: ArtifactSource>(
        &self,
        release: &VerifiedRelease,
        source: S,
    ) -> Result<InstalledGeneration, UpdateError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| UpdateError::ClockUnavailable)?
            .as_secs()
            .try_into()
            .map_err(|_| UpdateError::ClockUnavailable)?;
        self.install_at(release, source, now)
    }

    /// Installs using an explicit Unix timestamp.  The explicit-clock form
    /// keeps expiry and rollback decisions deterministic for callers and
    /// tests while [`Self::install`] uses the host clock in production.
    pub fn install_at<S: ArtifactSource>(
        &self,
        release: &VerifiedRelease,
        source: S,
        now: i64,
    ) -> Result<InstalledGeneration, UpdateError> {
        release.descriptor.validate_at(now)?;
        let entries = source.collect_entries()?;
        let artifacts = validate_payloads(release, entries)?;
        self.validate_store()?;

        let accepted = self.load_current_marker()?;
        if let Some(marker) = accepted.as_ref() {
            if release.descriptor.sequence < marker.sequence {
                return Err(UpdateError::ReleaseRollback {
                    sequence: release.descriptor.sequence,
                    floor: marker.sequence,
                });
            }
            if release.descriptor.sequence == marker.sequence && !marker.matches_release(release) {
                return Err(UpdateError::SequenceConflict);
            }
        }

        let target = self.generations.join(release.release_id());
        reject_unsafe_components(&target)?;

        if self.reconcile_staging(&target, release)? {
            self.persist_current_marker(release)?;
            return Ok(InstalledGeneration {
                path: target,
                release_id: release.release_id().to_owned(),
                reused: true,
            });
        }

        if target.exists() {
            validate_complete_generation(&target, release)?;
            if accepted
                .as_ref()
                .map_or(true, |marker| marker.sequence < release.descriptor.sequence)
            {
                self.persist_current_marker(release)?;
            }
            return Ok(InstalledGeneration {
                path: target,
                release_id: release.release_id().to_owned(),
                reused: true,
            });
        }

        let staging = self.create_staging_directory()?;
        // Once created, this directory is intentionally left in place on every
        // error so recovery tooling can distinguish an interrupted update.
        for identity in FIXED_ARTIFACT_IDENTITIES {
            let bytes = artifacts
                .get(&identity)
                .expect("validate_payloads checked every fixed identity");
            write_fixed_artifact(&staging, identity, bytes)?;
        }
        sync_directory_tree(&staging)?;
        write_signed_envelope(&staging, release)?;
        sync_directory_tree(&staging)?;
        write_completion_marker(&staging, release)?;
        sync_directory_tree(&staging)?;
        self.publish_staging(&staging, &target, release)?;
        self.persist_current_marker(release)?;

        Ok(InstalledGeneration {
            path: target,
            release_id: release.release_id().to_owned(),
            reused: false,
        })
    }

    /// Restores the exact signed bytes of an already accepted generation.
    ///
    /// A damaged artifact may make the bootstrapper select MSI-owned
    /// onboarding. The accepted marker remains the rollback floor, and each
    /// replacement file is synced and renamed independently so an interrupted
    /// repair can be retried without accepting a different release.
    ///
    /// # Errors
    ///
    /// Rejects mismatched releases, unsafe paths, invalid downloads, or I/O
    /// failures. The caller must have verified the signed release envelope.
    pub fn repair_accepted_at<S: ArtifactSource>(
        &self,
        release: &VerifiedRelease,
        source: S,
        now: i64,
    ) -> Result<InstalledGeneration, UpdateError> {
        release.descriptor.validate_at(now)?;
        let artifacts = validate_payloads(release, source.collect_entries()?)?;
        self.validate_store()?;
        let marker = self
            .load_current_marker()?
            .ok_or(UpdateError::NoAcceptedGeneration)?;
        if !marker.matches_release(release) {
            return Err(UpdateError::SequenceConflict);
        }
        let target = self.generations.join(release.release_id());
        reject_unsafe_components(&target)?;
        ensure_real_directory(&target)?;
        for identity in FIXED_ARTIFACT_IDENTITIES {
            let bytes = artifacts
                .get(&identity)
                .expect("validate_payloads checked every fixed identity");
            repair_existing_file(&target.join(identity.destination()), bytes)?;
        }
        repair_existing_file(&target.join(SIGNED_RELEASE_ENVELOPE), release.signed_envelope())?;
        validate_complete_generation(&target, release)?;
        Ok(InstalledGeneration {
            path: target,
            release_id: release.release_id().to_owned(),
            reused: true,
        })
    }

    fn persist_current_marker(&self, release: &VerifiedRelease) -> Result<(), UpdateError> {
        let marker = CompletionMarker::from_release(release);
        let bytes = marker.encode();
        if bytes.len() > MAX_MARKER_BYTES {
            return Err(UpdateError::MarkerTooLarge);
        }

        // Accepted records are append-only.  A fresh final pathname means the
        // publication itself is one atomic create/rename even on Windows,
        // where replacing an open marker in place is not atomic.  Restart
        // selects the highest validated sequence as the rollback floor.
        let temporary = self.generations.join(format!(
            "{CURRENT_MARKER_TEMP_PREFIX}{}",
            Uuid::new_v4().simple()
        ));
        let final_path = self.generations.join(format!(
            "{CURRENT_MARKER_PREFIX}{}",
            Uuid::new_v4().simple()
        ));
        let final_name = final_path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| UpdateError::InvalidCurrentMarker(final_path.clone()))?
            .to_owned();
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .map_err(UpdateError::CurrentMarkerIo)?;
        file.write_all(&bytes)
            .map_err(UpdateError::CurrentMarkerIo)?;
        file.sync_all().map_err(UpdateError::CurrentMarkerIo)?;
        fs::rename(&temporary, &final_path).map_err(UpdateError::CurrentMarkerIo)?;
        sync_directory(&self.generations).map_err(|error| match error {
            UpdateError::StagingIo(error) => UpdateError::CurrentMarkerIo(error),
            other => other,
        })?;

        self.persist_accepted_head(&final_name, &marker, &bytes)
    }

    fn persist_accepted_head(
        &self,
        marker_file: &str,
        marker: &CompletionMarker,
        marker_bytes: &[u8],
    ) -> Result<(), UpdateError> {
        let head = AcceptedHead {
            marker_file: marker_file.to_owned(),
            marker_sha256: Sha256::digest(marker_bytes).into(),
            marker: marker.clone(),
        };
        let bytes = head.encode();
        if bytes.len() > MAX_HEAD_BYTES {
            return Err(UpdateError::MarkerTooLarge);
        }
        let temporary = self.generations.join(format!(
            "{ACCEPTED_HEAD_TEMP_PREFIX}{}",
            Uuid::new_v4().simple()
        ));
        let final_path = self
            .generations
            .join(format!("{ACCEPTED_HEAD_PREFIX}{}", Uuid::new_v4().simple()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .map_err(UpdateError::CurrentMarkerIo)?;
        file.write_all(&bytes)
            .map_err(UpdateError::CurrentMarkerIo)?;
        file.sync_all().map_err(UpdateError::CurrentMarkerIo)?;
        fs::rename(&temporary, &final_path).map_err(UpdateError::CurrentMarkerIo)?;
        sync_directory(&self.generations).map_err(|error| match error {
            UpdateError::StagingIo(error) => UpdateError::CurrentMarkerIo(error),
            other => other,
        })
    }

    fn load_current_marker(&self) -> Result<Option<CompletionMarker>, UpdateError> {
        let mut markers = BTreeMap::<String, CompletionMarker>::new();
        let mut heads = Vec::<(PathBuf, AcceptedHead)>::new();
        for entry in fs::read_dir(&self.generations).map_err(UpdateError::CurrentMarkerIo)? {
            let entry = entry.map_err(UpdateError::CurrentMarkerIo)?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(CURRENT_MARKER_TEMP_PREFIX) {
                if !is_owned_marker_temp_name(&name) {
                    return Err(UpdateError::InvalidCurrentMarker(path));
                }
                // A temp marker is never an accepted floor.  Its generated
                // name proves ownership, so an interrupted write can be
                // removed deterministically on the next launch.
                fs::remove_file(&path)
                    .map_err(|_| UpdateError::InvalidCurrentMarker(path.clone()))?;
                continue;
            }
            if name.starts_with(ACCEPTED_HEAD_TEMP_PREFIX) {
                if !is_owned_head_temp_name(&name) {
                    return Err(UpdateError::InvalidCurrentMarker(path));
                }
                fs::remove_file(&path)
                    .map_err(|_| UpdateError::InvalidCurrentMarker(path.clone()))?;
                continue;
            }
            if name.starts_with(ACCEPTED_HEAD_PREFIX) {
                if !is_owned_head_name(&name) {
                    return Err(UpdateError::InvalidCurrentMarker(path));
                }
                let bytes = read_marker_record(&path)?;
                let head = parse_accepted_head(&bytes)
                    .ok_or_else(|| UpdateError::InvalidCurrentMarker(path.clone()))?;
                heads.push((path, head));
                continue;
            }
            if !name.starts_with(CURRENT_MARKER_PREFIX) {
                continue;
            }
            if !is_owned_marker_name(&name) {
                return Err(UpdateError::InvalidCurrentMarker(path));
            }
            let metadata = fs::symlink_metadata(&path)
                .map_err(|_| UpdateError::InvalidCurrentMarker(path.clone()))?;
            if link_or_reparse(&metadata)
                || !metadata.is_file()
                || metadata.len() > MAX_MARKER_BYTES as u64
            {
                return Err(UpdateError::InvalidCurrentMarker(path));
            }
            let bytes =
                fs::read(&path).map_err(|_| UpdateError::InvalidCurrentMarker(path.clone()))?;
            let marker = parse_completion_marker(&bytes)
                .ok_or_else(|| UpdateError::InvalidCurrentMarker(path.clone()))?;
            if !marker.validate_structure() {
                return Err(UpdateError::InvalidCurrentMarker(path));
            }

            let generation = self.generations.join(&marker.release_id);
            reject_unsafe_components(&generation)
                .map_err(|_| UpdateError::InvalidCurrentMarker(generation.clone()))?;
            let generation_metadata = fs::symlink_metadata(&generation)
                .map_err(|_| UpdateError::InvalidCurrentMarker(generation.clone()))?;
            if link_or_reparse(&generation_metadata) || !generation_metadata.is_dir() {
                return Err(UpdateError::InvalidCurrentMarker(generation));
            }
            let complete = generation.join(COMPLETE_MARKER);
            let complete_metadata = fs::symlink_metadata(&complete)
                .map_err(|_| UpdateError::InvalidCurrentMarker(complete.clone()))?;
            if link_or_reparse(&complete_metadata)
                || !complete_metadata.is_file()
                || complete_metadata.len() != bytes.len() as u64
            {
                return Err(UpdateError::InvalidCurrentMarker(complete));
            }
            let complete_bytes = fs::read(&complete)
                .map_err(|_| UpdateError::InvalidCurrentMarker(complete.clone()))?;
            if complete_bytes != bytes || complete_bytes != marker.encode() {
                return Err(UpdateError::InvalidCurrentMarker(complete));
            }
            markers.insert(name, marker);
        }

        if markers.is_empty() && heads.is_empty() {
            return Ok(None);
        }
        if markers.is_empty() || heads.is_empty() {
            return Err(UpdateError::MarkerHistoryRegression(
                self.generations.clone(),
            ));
        }

        let mut referenced = BTreeSet::new();
        let mut current: Option<CompletionMarker> = None;
        for (path, head) in heads {
            let marker = markers
                .get(&head.marker_file)
                .ok_or_else(|| UpdateError::MarkerHistoryRegression(path.clone()))?;
            let marker_bytes = marker.encode();
            let digest: [u8; 32] = Sha256::digest(&marker_bytes).into();
            if digest != head.marker_sha256 || marker != &head.marker {
                return Err(UpdateError::MarkerHistoryRegression(path));
            }
            referenced.insert(head.marker_file);
            match current.as_ref() {
                Some(previous) if marker.sequence < previous.sequence => {}
                Some(previous) if marker.sequence == previous.sequence && marker != previous => {
                    return Err(UpdateError::InvalidCurrentMarker(path));
                }
                _ => current = Some(marker.clone()),
            }
        }
        if referenced.len() != markers.len() {
            let orphan = markers
                .keys()
                .find(|name| !referenced.contains(*name))
                .map(|name| self.generations.join(name))
                .unwrap_or_else(|| self.generations.clone());
            return Err(UpdateError::MarkerHistoryRegression(orphan));
        }
        Ok(current)
    }

    /// Revalidates a named complete generation without activating anything.
    pub fn validate_generation(&self, release: &VerifiedRelease) -> Result<(), UpdateError> {
        let target = self.generations.join(release.release_id());
        reject_unsafe_components(&target)?;
        validate_complete_generation(&target, release)
    }

    /// Opens the highest accepted complete generation after a cold start.
    ///
    /// The persisted signed envelope is verified with the caller's pinned key
    /// before the marker and every fixed artifact are revalidated.  No path is
    /// returned until the accepted sequence, validity window, signed marker,
    /// envelope bytes, and complete generation all agree.
    pub fn open_accepted_current(
        &self,
        trusted_key: &TrustedReleaseKey,
        now: i64,
    ) -> Result<AcceptedGeneration, UpdateError> {
        self.validate_store()?;
        let marker = self
            .load_current_marker()?
            .ok_or(UpdateError::NoAcceptedGeneration)?;
        let generation = self.generations.join(&marker.release_id);
        let envelope_path = generation.join(SIGNED_RELEASE_ENVELOPE);
        reject_unsafe_components(&envelope_path)?;
        let metadata = fs::symlink_metadata(&envelope_path)
            .map_err(|_| UpdateError::InvalidCompleteGeneration(generation.clone()))?;
        if link_or_reparse(&metadata)
            || !metadata.is_file()
            || metadata.len() > MAX_ENVELOPE_BYTES as u64
        {
            return Err(UpdateError::InvalidCompleteGeneration(generation));
        }
        let envelope = fs::read(&envelope_path)
            .map_err(|_| UpdateError::InvalidCompleteGeneration(generation.clone()))?;
        let release = SignedReleaseEnvelope::verify_json(&envelope, trusted_key)?;
        release.descriptor.validate_at(now)?;
        if !marker.matches_release(&release) {
            return Err(UpdateError::InvalidCurrentMarker(generation));
        }
        validate_complete_generation(&generation, &release)?;
        let artifacts = generation_artifact_records(&generation, &release)?;
        let guard = GenerationGuard::open(&generation, &release)?;
        Ok(AcceptedGeneration {
            path: generation,
            release_id: release.release_id().to_owned(),
            sequence: release.descriptor.sequence,
            artifacts,
            guard,
        })
    }

    fn validate_store(&self) -> Result<(), UpdateError> {
        reject_unsafe_components(&self.root)?;
        ensure_real_directory(&self.root)?;
        reject_unsafe_components(&self.generations)?;
        ensure_real_directory(&self.generations)
    }

    fn create_staging_directory(&self) -> Result<PathBuf, UpdateError> {
        for _ in 0..8 {
            let staging = self
                .generations
                .join(format!("{STAGING_PREFIX}{}", Uuid::new_v4().simple()));
            match fs::create_dir(&staging) {
                Ok(()) => {
                    reject_unsafe_components(&staging)?;
                    return Ok(staging);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(UpdateError::StagingIo(error)),
            }
        }
        Err(UpdateError::StagingNameExhausted)
    }

    /// Reconciles launcher-owned staging directories before creating another
    /// generation.  A complete compatible staging directory is resumed; an
    /// incomplete directory with an exact generated name is moved aside for
    /// diagnostics.  Ambiguous names or reparse points remain blocked rather
    /// than being deleted by a recovery path.
    fn reconcile_staging(
        &self,
        target: &Path,
        release: &VerifiedRelease,
    ) -> Result<bool, UpdateError> {
        let mut staging_paths = Vec::new();
        for entry in fs::read_dir(&self.generations).map_err(UpdateError::Io)? {
            let entry = entry.map_err(UpdateError::Io)?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(STAGING_PREFIX) {
                staging_paths.push(path);
            }
        }
        staging_paths.sort();

        for staging in staging_paths {
            let name = staging
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            if !is_owned_staging_name(name) {
                return Err(UpdateError::InterruptedStaging(staging));
            }
            let metadata = fs::symlink_metadata(&staging).map_err(UpdateError::Io)?;
            if link_or_reparse(&metadata) || !metadata.is_dir() {
                return Err(UpdateError::InterruptedStaging(staging));
            }

            let marker = staging.join(COMPLETE_MARKER);
            let marker_state = read_staging_marker(&marker)?;
            match marker_state {
                Some(marker_state) if marker_state.matches_release(release) => {
                    // The marker is only completion evidence; all fixed files
                    // are still re-read and rehashed before this directory is
                    // renamed into the generations store.
                    if validate_complete_generation(&staging, release).is_err() {
                        // A launcher-owned marker with missing, tampered, or
                        // legacy contents is proven incomplete.  Quarantine
                        // it so a failed install cannot poison every retry.
                        self.quarantine_staging(&staging)?;
                        continue;
                    }
                    if target.exists() {
                        // A previous launch may have published this release
                        // before persisting its accepted marker.  The target
                        // wins; the duplicate staging directory is merely a
                        // proven-owned interrupted remainder.
                        self.quarantine_staging(&staging)?;
                        continue;
                    }
                    self.publish_staging(&staging, target, release)?;
                    return Ok(true);
                }
                Some(_) | None => {
                    // An exact generated name is the only proof that this
                    // directory belongs to this updater.  Preserve it under a
                    // quarantine name rather than deleting user data.
                    self.quarantine_staging(&staging)?;
                }
            }
        }
        Ok(false)
    }

    fn quarantine_staging(&self, staging: &Path) -> Result<PathBuf, UpdateError> {
        for _ in 0..8 {
            let quarantine = self
                .generations
                .join(format!("{QUARANTINE_PREFIX}{}", Uuid::new_v4().simple()));
            match fs::rename(staging, &quarantine) {
                Ok(()) => {
                    sync_directory(&self.generations)?;
                    return Ok(quarantine);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(UpdateError::QuarantineIo(error)),
            }
        }
        Err(UpdateError::StagingNameExhausted)
    }

    fn publish_staging(
        &self,
        staging: &Path,
        target: &Path,
        release: &VerifiedRelease,
    ) -> Result<(), UpdateError> {
        // This is deliberately immediately before the publication boundary:
        // every expected staged file is re-read and rehashed after the marker
        // was synced.  The Windows guard phase repeats that check through
        // no-follow handles bound to the opened file identities.
        validate_complete_generation(staging, release)?;
        #[cfg(windows)]
        let guards = WindowsPublicationGuards::acquire(staging, release)?;
        #[cfg(windows)]
        guards.verify(release)?;
        #[cfg(windows)]
        // Windows does not permit MoveFileEx to rename a directory tree while
        // no-follow handles to descendants are live, even when all three
        // share bits are enabled.  The bound handles therefore cover the
        // final rehash boundary and are released only for the single atomic
        // rename syscall itself.
        drop(guards);
        // Re-read once more after releasing Windows' rename-incompatible
        // descendant handles.  This keeps the publication decision tied to
        // the bytes observed in the last possible path-based read before the
        // atomic directory operation.
        validate_complete_generation(staging, release)?;
        fs::rename(staging, target).map_err(UpdateError::ActivationIo)?;
        sync_directory(&self.generations)?;
        // A replacement race can only happen in the tiny Windows handle
        // release/rename interval.  Revalidate the published directory and
        // quarantine it if that race won, so an invalid generation is never
        // left as the launch candidate and the prior generations remain.
        if validate_complete_generation(target, release).is_err() {
            self.quarantine_staging(target)?;
            return Err(UpdateError::PublicationIntegrity(target.to_path_buf()));
        }
        Ok(())
    }
}

fn generation_artifact_records(
    generation: &Path,
    release: &VerifiedRelease,
) -> Result<BTreeMap<ArtifactIdentity, ArtifactRecord>, UpdateError> {
    let mut artifacts = BTreeMap::new();
    for identity in FIXED_ARTIFACT_IDENTITIES {
        let descriptor = release
            .descriptor
            .artifact(identity)
            .expect("verified descriptor contains every fixed identity");
        let digest = descriptor.digest_bytes()?;
        artifacts.insert(
            identity,
            ArtifactRecord {
                path: generation.join(identity.destination()),
                size: descriptor.size,
                digest,
            },
        );
    }
    Ok(artifacts)
}

fn validate_payloads(
    release: &VerifiedRelease,
    entries: Vec<ArtifactPayload>,
) -> Result<BTreeMap<ArtifactIdentity, Vec<u8>>, UpdateError> {
    if entries.len() > FIXED_ARTIFACT_IDENTITIES.len() {
        return Err(UpdateError::TooManyArtifacts(entries.len()));
    }
    let mut by_identity = BTreeMap::new();
    for entry in entries {
        if by_identity.insert(entry.identity, entry.bytes).is_some() {
            return Err(UpdateError::DuplicateArtifact(entry.identity));
        }
    }
    if by_identity.len() != FIXED_ARTIFACT_IDENTITIES.len() {
        for identity in FIXED_ARTIFACT_IDENTITIES {
            if !by_identity.contains_key(&identity) {
                return Err(UpdateError::MissingArtifact(identity));
            }
        }
    }
    for identity in FIXED_ARTIFACT_IDENTITIES {
        let descriptor = release
            .descriptor
            .artifact(identity)
            .expect("verified descriptor contains every fixed identity");
        let bytes = by_identity
            .get(&identity)
            .expect("source contains every fixed identity");
        if bytes.len() as u64 != descriptor.size {
            return Err(UpdateError::ArtifactSizeMismatch {
                artifact: identity,
                expected: descriptor.size,
                actual: bytes.len() as u64,
            });
        }
        let digest = Sha256::digest(bytes);
        if digest.as_slice() != descriptor.digest_bytes()?.as_slice() {
            return Err(UpdateError::ArtifactDigestMismatch(identity));
        }
    }
    Ok(by_identity)
}

fn write_fixed_artifact(
    staging: &Path,
    identity: ArtifactIdentity,
    bytes: &[u8],
) -> Result<(), UpdateError> {
    let relative = Path::new(identity.destination());
    ensure_safe_relative_path(relative)?;
    let destination = staging.join(relative);
    let parent = destination
        .parent()
        .ok_or(UpdateError::UnsafeDestination(identity))?;
    reject_unsafe_components(parent)?;
    fs::create_dir_all(parent).map_err(UpdateError::StagingIo)?;
    ensure_real_directory(parent)?;

    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&destination).map_err(UpdateError::StagingIo)?;
    file.write_all(bytes).map_err(UpdateError::StagingIo)?;
    file.sync_all().map_err(UpdateError::StagingIo)?;
    Ok(())
}

fn repair_existing_file(path: &Path, bytes: &[u8]) -> Result<(), UpdateError> {
    let parent = path
        .parent()
        .ok_or_else(|| UpdateError::UnsafeStorePath(path.to_path_buf()))?;
    reject_unsafe_components(parent)?;
    ensure_real_directory(parent)?;
    for entry in fs::read_dir(parent).map_err(UpdateError::StagingIo)? {
        let entry = entry.map_err(UpdateError::StagingIo)?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let suffix = name.strip_prefix(".repair-");
        if suffix.is_some_and(|suffix| {
            suffix.len() == 32
                && suffix
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        }) {
            let metadata = fs::symlink_metadata(entry.path()).map_err(UpdateError::StagingIo)?;
            if link_or_reparse(&metadata) || !metadata.is_file() {
                return Err(UpdateError::SymlinkOrReparse(entry.path()));
            }
            fs::remove_file(entry.path()).map_err(UpdateError::StagingIo)?;
        }
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if link_or_reparse(&metadata) || !metadata.is_file() => {
            return Err(UpdateError::SymlinkOrReparse(path.to_path_buf()));
        }
        Ok(metadata) if metadata.len() == bytes.len() as u64 => {
            if fs::read(path).map_err(UpdateError::StagingIo)? == bytes {
                return Ok(());
            }
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(UpdateError::StagingIo(error)),
    }
    let temporary = parent.join(format!(".repair-{}", Uuid::new_v4().simple()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(UpdateError::StagingIo)?;
        file.write_all(bytes).map_err(UpdateError::StagingIo)?;
        file.sync_all().map_err(UpdateError::StagingIo)?;
        drop(file);
        fs::rename(&temporary, path).map_err(UpdateError::ActivationIo)?;
        sync_directory(parent)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn write_completion_marker(staging: &Path, release: &VerifiedRelease) -> Result<(), UpdateError> {
    let marker = staging.join(COMPLETE_MARKER);
    let content = CompletionMarker::from_release(release).encode();
    if content.len() > MAX_MARKER_BYTES {
        return Err(UpdateError::MarkerTooLarge);
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(marker).map_err(UpdateError::StagingIo)?;
    file.write_all(&content).map_err(UpdateError::StagingIo)?;
    file.sync_all().map_err(UpdateError::StagingIo)
}

fn write_signed_envelope(staging: &Path, release: &VerifiedRelease) -> Result<(), UpdateError> {
    if release.signed_envelope.len() > MAX_ENVELOPE_BYTES {
        return Err(UpdateError::EnvelopeTooLarge(release.signed_envelope.len()));
    }
    let path = staging.join(SIGNED_RELEASE_ENVELOPE);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(UpdateError::StagingIo)?;
    file.write_all(&release.signed_envelope)
        .map_err(UpdateError::StagingIo)?;
    file.sync_all().map_err(UpdateError::StagingIo)
}

fn validate_complete_generation(
    generation: &Path,
    release: &VerifiedRelease,
) -> Result<(), UpdateError> {
    reject_unsafe_components(generation)?;
    ensure_real_directory(generation)
        .map_err(|_| UpdateError::InvalidCompleteGeneration(generation.to_path_buf()))?;

    let marker_path = generation.join(COMPLETE_MARKER);
    reject_unsafe_components(&marker_path)?;
    let marker_metadata = fs::symlink_metadata(&marker_path)
        .map_err(|_| UpdateError::InvalidCompleteGeneration(generation.to_path_buf()))?;
    if link_or_reparse(&marker_metadata) || !marker_metadata.is_file() {
        return Err(UpdateError::InvalidCompleteGeneration(
            generation.to_path_buf(),
        ));
    }
    if marker_metadata.len() > MAX_MARKER_BYTES as u64 {
        return Err(UpdateError::InvalidCompleteGeneration(
            generation.to_path_buf(),
        ));
    }
    let marker = fs::read(&marker_path)
        .map_err(|_| UpdateError::InvalidCompleteGeneration(generation.to_path_buf()))?;
    let expected_marker = CompletionMarker::from_release(release).encode();
    if marker != expected_marker {
        return Err(UpdateError::InvalidCompleteGeneration(
            generation.to_path_buf(),
        ));
    }

    let signed_envelope_path = generation.join(SIGNED_RELEASE_ENVELOPE);
    reject_unsafe_components(&signed_envelope_path)?;
    let signed_envelope_metadata = fs::symlink_metadata(&signed_envelope_path)
        .map_err(|_| UpdateError::InvalidCompleteGeneration(generation.to_path_buf()))?;
    if link_or_reparse(&signed_envelope_metadata)
        || !signed_envelope_metadata.is_file()
        || signed_envelope_metadata.len() > MAX_ENVELOPE_BYTES as u64
    {
        return Err(UpdateError::InvalidCompleteGeneration(
            generation.to_path_buf(),
        ));
    }
    let signed_envelope = fs::read(&signed_envelope_path)
        .map_err(|_| UpdateError::InvalidCompleteGeneration(generation.to_path_buf()))?;
    if signed_envelope != release.signed_envelope {
        return Err(UpdateError::InvalidCompleteGeneration(
            generation.to_path_buf(),
        ));
    }

    let mut expected_files: BTreeSet<&str> = FIXED_ARTIFACT_IDENTITIES
        .iter()
        .map(|identity| identity.destination())
        .collect();
    expected_files.insert(SIGNED_RELEASE_ENVELOPE);
    let mut actual_files = BTreeSet::new();
    collect_generation_files(generation, generation, &mut actual_files)?;
    if actual_files
        .iter()
        .any(|path| *path != COMPLETE_MARKER && !expected_files.contains(path.as_str()))
        || actual_files.len() != expected_files.len() + 1
    {
        return Err(UpdateError::InvalidCompleteGeneration(
            generation.to_path_buf(),
        ));
    }

    for identity in FIXED_ARTIFACT_IDENTITIES {
        let descriptor = release
            .descriptor
            .artifact(identity)
            .expect("verified descriptor contains every fixed identity");
        let path = generation.join(identity.destination());
        reject_unsafe_components(&path)?;
        let metadata = fs::symlink_metadata(&path)
            .map_err(|_| UpdateError::InvalidCompleteGeneration(generation.to_path_buf()))?;
        if link_or_reparse(&metadata) || !metadata.is_file() || metadata.len() != descriptor.size {
            return Err(UpdateError::InvalidCompleteGeneration(
                generation.to_path_buf(),
            ));
        }
        let digest = digest_file(&path, descriptor.size)
            .map_err(|_| UpdateError::InvalidCompleteGeneration(generation.to_path_buf()))?;
        if digest
            != descriptor
                .digest_bytes()
                .map_err(|_| UpdateError::InvalidCompleteGeneration(generation.to_path_buf()))?
        {
            return Err(UpdateError::InvalidCompleteGeneration(
                generation.to_path_buf(),
            ));
        }
    }
    Ok(())
}

fn collect_generation_files(
    root: &Path,
    current: &Path,
    files: &mut BTreeSet<String>,
) -> Result<(), UpdateError> {
    let entries = fs::read_dir(current)
        .map_err(|_| UpdateError::InvalidCompleteGeneration(root.to_path_buf()))?;
    for entry in entries {
        let entry =
            entry.map_err(|_| UpdateError::InvalidCompleteGeneration(root.to_path_buf()))?;
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|_| UpdateError::InvalidCompleteGeneration(root.to_path_buf()))?;
        let relative_string = relative.to_string_lossy().replace('\\', "/");
        let metadata = fs::symlink_metadata(&path)
            .map_err(|_| UpdateError::InvalidCompleteGeneration(root.to_path_buf()))?;
        if link_or_reparse(&metadata) {
            return Err(UpdateError::InvalidCompleteGeneration(root.to_path_buf()));
        }
        if metadata.is_dir() {
            collect_generation_files(root, &path, files)?;
        } else if metadata.is_file() {
            files.insert(relative_string);
        } else {
            return Err(UpdateError::InvalidCompleteGeneration(root.to_path_buf()));
        }
    }
    Ok(())
}

fn digest_file(path: &Path, expected_size: u64) -> io::Result<[u8; 32]> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 32 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > expected_size || total > MAX_ARTIFACT_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "artifact too large",
            ));
        }
        digest.update(&buffer[..read]);
    }
    if total != expected_size {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "artifact size mismatch",
        ));
    }
    let output = digest.finalize();
    let mut bytes = [0_u8; 32];
    bytes.copy_from_slice(&output);
    Ok(bytes)
}

impl GenerationGuard {
    fn open(generation: &Path, release: &VerifiedRelease) -> Result<Self, UpdateError> {
        #[cfg(windows)]
        {
            return Ok(Self {
                windows: WindowsAcceptedGuard::open(generation, release)?,
            });
        }
        #[cfg(not(windows))]
        {
            let _ = (generation, release);
            Ok(Self {})
        }
    }
}

#[cfg(windows)]
struct WindowsAcceptedGuard {
    _ancestors: Vec<File>,
    desktop_app: File,
    desktop_app_identity: WindowsBoundIdentity,
    sidecar: File,
    sidecar_identity: WindowsBoundIdentity,
}

#[cfg(windows)]
impl WindowsAcceptedGuard {
    fn open(generation: &Path, release: &VerifiedRelease) -> Result<Self, UpdateError> {
        use std::os::windows::fs::OpenOptionsExt;

        let desktop = release
            .descriptor
            .artifact(ArtifactIdentity::DesktopApp)
            .expect("verified descriptor contains desktop app");
        let sidecar_descriptor = release
            .descriptor
            .artifact(ArtifactIdentity::Sidecar)
            .expect("verified descriptor contains sidecar");

        let mut ancestors = Vec::new();
        for path in [
            generation.to_path_buf(),
            generation.join("app"),
            generation.join("runtime"),
        ] {
            let mut options = OpenOptions::new();
            options
                .read(true)
                // FILE_SHARE_READ only denies concurrent write/delete while
                // this accepted generation is handed to the spawner.
                .share_mode(0x0000_0001)
                .custom_flags(0x0220_0000);
            let handle = options.open(&path).map_err(UpdateError::WindowsGuardIo)?;
            let metadata = handle.metadata().map_err(UpdateError::WindowsGuardIo)?;
            if link_or_reparse(&metadata) || !metadata.is_dir() {
                return Err(UpdateError::PublicationIntegrity(path));
            }
            ancestors.push(handle);
        }

        let open_artifact = |identity: ArtifactIdentity, size: u64| {
            let path = generation.join(identity.destination());
            let mut options = OpenOptions::new();
            options
                .read(true)
                .share_mode(0x0000_0001)
                .custom_flags(0x0020_0000);
            let handle = options.open(&path).map_err(UpdateError::WindowsGuardIo)?;
            let metadata = handle.metadata().map_err(UpdateError::WindowsGuardIo)?;
            if link_or_reparse(&metadata) || !metadata.is_file() {
                return Err(UpdateError::PublicationIntegrity(path));
            }
            let identity_bound = windows_bound_identity(&metadata)?;
            let digest = hash_guard(&handle, size).map_err(UpdateError::WindowsGuardIo)?;
            Ok::<_, UpdateError>((handle, identity_bound, digest))
        };

        let (desktop_app, desktop_app_identity, desktop_digest) =
            open_artifact(ArtifactIdentity::DesktopApp, desktop.size)?;
        if desktop_digest != desktop.digest_bytes()? {
            return Err(UpdateError::ArtifactDigestMismatch(
                ArtifactIdentity::DesktopApp,
            ));
        }
        let (sidecar, sidecar_identity, sidecar_digest) =
            open_artifact(ArtifactIdentity::Sidecar, sidecar_descriptor.size)?;
        if sidecar_digest != sidecar_descriptor.digest_bytes()? {
            return Err(UpdateError::ArtifactDigestMismatch(
                ArtifactIdentity::Sidecar,
            ));
        }

        Ok(Self {
            _ancestors: ancestors,
            desktop_app,
            desktop_app_identity,
            sidecar,
            sidecar_identity,
        })
    }
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WindowsBoundIdentity {
    size: u64,
    creation_time: u64,
    last_write_time: u64,
    attributes: u32,
}

#[cfg(windows)]
struct WindowsPublicationGuards {
    // The handles are intentionally retained through the final post-write
    // rehash.  Their share mode denies a second writer/deleter while the
    // no-follow handles remain bound to the original file identities, so a
    // replacement attempt is caught before the handles are released for the
    // Windows directory-rename syscall.
    _ancestors: Vec<File>,
    artifacts: BTreeMap<ArtifactIdentity, File>,
    artifact_identities: BTreeMap<ArtifactIdentity, WindowsBoundIdentity>,
    signed_envelope: File,
    signed_envelope_identity: WindowsBoundIdentity,
    marker: File,
    marker_identity: WindowsBoundIdentity,
}

#[cfg(windows)]
impl WindowsPublicationGuards {
    fn acquire(staging: &Path, release: &VerifiedRelease) -> Result<Self, UpdateError> {
        use std::os::windows::fs::OpenOptionsExt;

        let mut ancestor_paths = BTreeSet::new();
        for identity in FIXED_ARTIFACT_IDENTITIES {
            let parent = staging
                .join(identity.destination())
                .parent()
                .ok_or(UpdateError::UnsafeDestination(identity))?
                .to_path_buf();
            let mut current = parent;
            loop {
                ancestor_paths.insert(current.clone());
                if current == staging {
                    break;
                }
                let Some(next) = current.parent() else {
                    return Err(UpdateError::PublicationIntegrity(staging.to_path_buf()));
                };
                current = next.to_path_buf();
            }
        }

        let mut ancestors = Vec::new();
        for path in ancestor_paths {
            let mut options = OpenOptions::new();
            options
                .read(true)
                // No write or delete sharing is granted while the ancestor
                // identities are bound and rehashed.
                .share_mode(0x0000_0001)
                .custom_flags(0x0220_0000);
            let handle = options.open(&path).map_err(UpdateError::WindowsGuardIo)?;
            let metadata = handle.metadata().map_err(UpdateError::WindowsGuardIo)?;
            if link_or_reparse(&metadata) || !metadata.is_dir() {
                return Err(UpdateError::PublicationIntegrity(path));
            }
            ancestors.push(handle);
        }

        let mut artifacts = BTreeMap::new();
        let mut artifact_identities = BTreeMap::new();
        for identity in FIXED_ARTIFACT_IDENTITIES {
            let path = staging.join(identity.destination());
            let mut options = OpenOptions::new();
            options
                .read(true)
                // Deny write and delete sharing while each no-follow handle
                // remains bound through its post-write rehash.
                .share_mode(0x0000_0001)
                .custom_flags(0x0020_0000);
            let handle = options.open(&path).map_err(UpdateError::WindowsGuardIo)?;
            let metadata = handle.metadata().map_err(UpdateError::WindowsGuardIo)?;
            if link_or_reparse(&metadata) || !metadata.is_file() {
                return Err(UpdateError::PublicationIntegrity(path));
            }
            let bound = windows_bound_identity(&metadata)?;
            artifacts.insert(identity, handle);
            artifact_identities.insert(identity, bound);
        }

        let signed_envelope_path = staging.join(SIGNED_RELEASE_ENVELOPE);
        let mut signed_envelope_options = OpenOptions::new();
        signed_envelope_options
            .read(true)
            .share_mode(0x0000_0001)
            .custom_flags(0x0020_0000);
        let signed_envelope = signed_envelope_options
            .open(&signed_envelope_path)
            .map_err(UpdateError::WindowsGuardIo)?;
        let signed_envelope_metadata = signed_envelope
            .metadata()
            .map_err(UpdateError::WindowsGuardIo)?;
        if link_or_reparse(&signed_envelope_metadata)
            || !signed_envelope_metadata.is_file()
            || signed_envelope_metadata.len() > MAX_ENVELOPE_BYTES as u64
        {
            return Err(UpdateError::PublicationIntegrity(signed_envelope_path));
        }
        let signed_envelope_identity = windows_bound_identity(&signed_envelope_metadata)?;

        let marker_path = staging.join(COMPLETE_MARKER);
        let mut marker_options = OpenOptions::new();
        marker_options
            .read(true)
            .share_mode(0x0000_0001)
            .custom_flags(0x0020_0000);
        let marker = marker_options
            .open(&marker_path)
            .map_err(UpdateError::WindowsGuardIo)?;
        let marker_metadata = marker.metadata().map_err(UpdateError::WindowsGuardIo)?;
        if link_or_reparse(&marker_metadata) || !marker_metadata.is_file() {
            return Err(UpdateError::PublicationIntegrity(marker_path));
        }
        let marker_identity = windows_bound_identity(&marker_metadata)?;
        let guards = Self {
            _ancestors: ancestors,
            artifacts,
            artifact_identities,
            signed_envelope,
            signed_envelope_identity,
            marker,
            marker_identity,
        };
        guards.verify(release)?;
        Ok(guards)
    }

    fn verify(&self, release: &VerifiedRelease) -> Result<(), UpdateError> {
        for identity in FIXED_ARTIFACT_IDENTITIES {
            let file = self
                .artifacts
                .get(&identity)
                .ok_or(UpdateError::PublicationIntegrity(PathBuf::from(
                    identity.destination(),
                )))?;
            let metadata = file.metadata().map_err(UpdateError::WindowsGuardIo)?;
            let current = windows_bound_identity(&metadata)?;
            if self.artifact_identities.get(&identity) != Some(&current) {
                return Err(UpdateError::PublicationIntegrity(PathBuf::from(
                    identity.destination(),
                )));
            }
            let descriptor = release
                .descriptor
                .artifact(identity)
                .expect("verified descriptor contains every fixed identity");
            if current.size != descriptor.size {
                return Err(UpdateError::ArtifactSizeMismatch {
                    artifact: identity,
                    expected: descriptor.size,
                    actual: current.size,
                });
            }
            let digest = hash_guard(file, descriptor.size).map_err(UpdateError::WindowsGuardIo)?;
            if digest != descriptor.digest_bytes()? {
                return Err(UpdateError::ArtifactDigestMismatch(identity));
            }
        }

        let signed_envelope_metadata = self
            .signed_envelope
            .metadata()
            .map_err(UpdateError::WindowsGuardIo)?;
        if windows_bound_identity(&signed_envelope_metadata)? != self.signed_envelope_identity {
            return Err(UpdateError::PublicationIntegrity(PathBuf::from(
                SIGNED_RELEASE_ENVELOPE,
            )));
        }
        let signed_envelope = read_guard(&self.signed_envelope, MAX_ENVELOPE_BYTES)?;
        if signed_envelope != release.signed_envelope {
            return Err(UpdateError::PublicationIntegrity(PathBuf::from(
                SIGNED_RELEASE_ENVELOPE,
            )));
        }

        let marker_metadata = self
            .marker
            .metadata()
            .map_err(UpdateError::WindowsGuardIo)?;
        if windows_bound_identity(&marker_metadata)? != self.marker_identity {
            return Err(UpdateError::PublicationIntegrity(PathBuf::from(
                COMPLETE_MARKER,
            )));
        }
        let marker = read_guard(&self.marker, MAX_MARKER_BYTES)?;
        let expected_marker = CompletionMarker::from_release(release).encode();
        if marker != expected_marker {
            return Err(UpdateError::PublicationIntegrity(PathBuf::from(
                COMPLETE_MARKER,
            )));
        }
        Ok(())
    }
}

#[cfg(windows)]
fn windows_bound_identity(metadata: &fs::Metadata) -> Result<WindowsBoundIdentity, UpdateError> {
    use std::os::windows::fs::MetadataExt;
    if link_or_reparse(metadata) {
        return Err(UpdateError::PublicationIntegrity(PathBuf::from(
            "reparse-point",
        )));
    }
    Ok(WindowsBoundIdentity {
        size: metadata.file_size(),
        creation_time: metadata.creation_time(),
        last_write_time: metadata.last_write_time(),
        attributes: metadata.file_attributes(),
    })
}

#[cfg(windows)]
fn read_guard(file: &File, max_bytes: usize) -> Result<Vec<u8>, UpdateError> {
    let mut clone = file.try_clone().map_err(UpdateError::WindowsGuardIo)?;
    clone
        .seek(SeekFrom::Start(0))
        .map_err(UpdateError::WindowsGuardIo)?;
    let mut bytes = Vec::new();
    clone
        .take(max_bytes as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(UpdateError::WindowsGuardIo)?;
    if bytes.len() > max_bytes {
        return Err(UpdateError::PublicationIntegrity(PathBuf::from(
            COMPLETE_MARKER,
        )));
    }
    Ok(bytes)
}

#[cfg(windows)]
fn hash_guard(file: &File, expected_size: u64) -> io::Result<[u8; 32]> {
    let mut clone = file.try_clone()?;
    clone.seek(SeekFrom::Start(0))?;
    let mut digest = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 32 * 1024];
    loop {
        let read = clone.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > expected_size || total > MAX_ARTIFACT_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "artifact too large",
            ));
        }
        digest.update(&buffer[..read]);
    }
    if total != expected_size {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "artifact size mismatch",
        ));
    }
    let output = digest.finalize();
    let mut bytes = [0_u8; 32];
    bytes.copy_from_slice(&output);
    Ok(bytes)
}

fn is_owned_staging_name(name: &str) -> bool {
    let Some(suffix) = name.strip_prefix(STAGING_PREFIX) else {
        return false;
    };
    suffix.len() == 32 && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn read_staging_marker(path: &Path) -> Result<Option<CompletionMarker>, UpdateError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(UpdateError::Io(error)),
    };
    if link_or_reparse(&metadata) || !metadata.is_file() || metadata.len() > MAX_MARKER_BYTES as u64
    {
        return Ok(None);
    }
    let bytes = fs::read(path).map_err(UpdateError::Io)?;
    Ok(parse_completion_marker(&bytes))
}

fn parse_completion_marker(bytes: &[u8]) -> Option<CompletionMarker> {
    let marker = std::str::from_utf8(bytes).ok()?;
    let mut schema = None;
    let mut platform = None;
    let mut release_id = None;
    let mut sequence = None;
    let mut issued_at = None;
    let mut expires_at = None;
    let mut payload_sha256 = None;
    for line in marker.lines() {
        let (key, value) = line.split_once('=')?;
        match key {
            "schema" if schema.is_none() => schema = value.parse::<u16>().ok(),
            "platform" if platform.is_none() => platform = Some(value.to_owned()),
            "release_id" if release_id.is_none() => release_id = Some(value.to_owned()),
            "sequence" if sequence.is_none() => sequence = value.parse::<u64>().ok(),
            "issued_at" if issued_at.is_none() => issued_at = value.parse::<i64>().ok(),
            "expires_at" if expires_at.is_none() => expires_at = value.parse::<i64>().ok(),
            "payload_sha256" if payload_sha256.is_none() => {
                payload_sha256 = parse_digest(value).ok()
            }
            _ => return None,
        }
    }
    let marker = CompletionMarker {
        schema: schema?,
        platform: platform?,
        release_id: release_id?,
        sequence: sequence?,
        issued_at: issued_at?,
        expires_at: expires_at?,
        payload_sha256: payload_sha256?,
    };
    marker.validate_structure().then_some(marker)
}

fn parse_accepted_head(bytes: &[u8]) -> Option<AcceptedHead> {
    let text = std::str::from_utf8(bytes).ok()?;
    let mut lines = text.splitn(3, '\n');
    let marker_file = lines.next()?.strip_prefix("marker_file=")?;
    let marker_sha256 = lines.next()?.strip_prefix("marker_sha256=")?;
    let marker_text = lines.next()?;
    if !is_owned_marker_name(marker_file) {
        return None;
    }
    let marker_sha256 = parse_digest(marker_sha256).ok()?;
    let marker = parse_completion_marker(marker_text.as_bytes())?;
    Some(AcceptedHead {
        marker_file: marker_file.to_owned(),
        marker_sha256,
        marker,
    })
}

fn read_marker_record(path: &Path) -> Result<Vec<u8>, UpdateError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| UpdateError::InvalidCurrentMarker(path.to_path_buf()))?;
    let max_bytes = MAX_HEAD_BYTES as u64;
    if link_or_reparse(&metadata) || !metadata.is_file() || metadata.len() > max_bytes {
        return Err(UpdateError::InvalidCurrentMarker(path.to_path_buf()));
    }
    fs::read(path).map_err(|_| UpdateError::InvalidCurrentMarker(path.to_path_buf()))
}

fn is_owned_marker_name(name: &str) -> bool {
    let Some(suffix) = name.strip_prefix(CURRENT_MARKER_PREFIX) else {
        return false;
    };
    suffix.len() == 32 && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_owned_marker_temp_name(name: &str) -> bool {
    let Some(suffix) = name.strip_prefix(CURRENT_MARKER_TEMP_PREFIX) else {
        return false;
    };
    suffix.len() == 32 && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_owned_head_name(name: &str) -> bool {
    let Some(suffix) = name.strip_prefix(ACCEPTED_HEAD_PREFIX) else {
        return false;
    };
    suffix.len() == 32 && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_owned_head_temp_name(name: &str) -> bool {
    let Some(suffix) = name.strip_prefix(ACCEPTED_HEAD_TEMP_PREFIX) else {
        return false;
    };
    suffix.len() == 32 && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn sync_directory_tree(path: &Path) -> Result<(), UpdateError> {
    for entry in fs::read_dir(path).map_err(UpdateError::StagingIo)? {
        let entry = entry.map_err(UpdateError::StagingIo)?;
        let child = entry.path();
        let metadata = fs::symlink_metadata(&child).map_err(UpdateError::StagingIo)?;
        if link_or_reparse(&metadata) {
            return Err(UpdateError::UnsafeDestination(ArtifactIdentity::DesktopApp));
        }
        if metadata.is_dir() {
            sync_directory_tree(&child)?;
            sync_directory(&child)?;
        } else if metadata.is_file() {
            #[cfg(windows)]
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&child)
                .map_err(UpdateError::StagingIo)?;
            #[cfg(not(windows))]
            let file = File::open(&child).map_err(UpdateError::StagingIo)?;
            file.sync_all().map_err(UpdateError::StagingIo)?;
        }
    }
    Ok(())
}

#[cfg(windows)]
fn sync_directory(path: &Path) -> Result<(), UpdateError> {
    use std::os::windows::fs::OpenOptionsExt;

    // Windows requires FILE_FLAG_BACKUP_SEMANTICS to open a directory handle.
    // The handle is read-only and shared, and sync_all flushes the directory
    // entry updates before the staging directory is renamed into place.
    let mut options = OpenOptions::new();
    options
        .read(true)
        .share_mode(0x0000_0007)
        .custom_flags(0x0200_0000);
    match options
        .open(path)
        .and_then(|directory| directory.sync_all())
    {
        Ok(()) => Ok(()),
        // Some supported Windows filesystems expose no flushable directory
        // handle even with BACKUP_SEMANTICS. Every file and the completion
        // marker are already individually flushed, so this platform limitation
        // must not turn a complete atomic rename into a false failure.
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::PermissionDenied | io::ErrorKind::InvalidInput
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(UpdateError::StagingIo(error)),
    }
}

#[cfg(not(windows))]
fn sync_directory(path: &Path) -> Result<(), UpdateError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(UpdateError::StagingIo)
}

fn validate_store_path(path: &Path) -> Result<(), UpdateError> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| component == Component::ParentDir)
    {
        return Err(UpdateError::UnsafeStorePath(path.to_path_buf()));
    }
    reject_unsafe_components(path)
}

fn ensure_safe_relative_path(path: &Path) -> Result<(), UpdateError> {
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(UpdateError::UnsafeStorePath(path.to_path_buf()));
    }
    Ok(())
}

fn validate_release_id(value: &str) -> Result<(), UpdateError> {
    if value.is_empty() || value.len() > MAX_RELEASE_ID_BYTES || value == "." || value == ".." {
        return Err(UpdateError::InvalidReleaseId(value.to_owned()));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(UpdateError::InvalidReleaseId(value.to_owned()));
    }
    Ok(())
}

fn validate_key_id(value: &str) -> Result<(), UpdateError> {
    if value.is_empty() || value.len() > MAX_KEY_ID_BYTES {
        return Err(UpdateError::InvalidKeyId(value.to_owned()));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(UpdateError::InvalidKeyId(value.to_owned()));
    }
    Ok(())
}

fn parse_digest(value: &str) -> Result<[u8; 32], UpdateError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(UpdateError::InvalidDigest(value.to_owned()));
    }
    let mut output = [0_u8; 32];
    for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
        output[index] = (hex_value(chunk[0]) << 4) | hex_value(chunk[1]);
    }
    Ok(output)
}

fn hex_value(value: u8) -> u8 {
    match value {
        b'0'..=b'9' => value - b'0',
        b'a'..=b'f' => value - b'a' + 10,
        _ => unreachable!("parse_digest validates hexadecimal input"),
    }
}

fn hex_digest(bytes: &[u8]) -> String {
    hex_bytes(&Sha256::digest(bytes))
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn decode_canonical_base64(value: &str, max_bytes: usize) -> Result<Vec<u8>, ()> {
    let decoded = BASE64.decode(value).map_err(|_| ())?;
    if decoded.len() > max_bytes || BASE64.encode(&decoded) != value {
        return Err(());
    }
    Ok(decoded)
}

fn reject_unsafe_components(path: &Path) -> Result<(), UpdateError> {
    if path.as_os_str().is_empty() {
        return Err(UpdateError::UnsafeStorePath(path.to_path_buf()));
    }
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => current.push(prefix.as_os_str()),
            Component::RootDir => current.push(std::path::MAIN_SEPARATOR.to_string()),
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(UpdateError::UnsafeStorePath(path.to_path_buf()));
            }
            Component::Normal(value) => current.push(value),
        }
        if let Ok(metadata) = fs::symlink_metadata(&current) {
            if link_or_reparse(&metadata) {
                return Err(UpdateError::SymlinkOrReparse(current));
            }
        }
    }
    Ok(())
}

fn ensure_real_directory(path: &Path) -> Result<(), UpdateError> {
    let metadata = fs::symlink_metadata(path).map_err(UpdateError::Io)?;
    if link_or_reparse(&metadata) || !metadata.is_dir() {
        return Err(UpdateError::NotDirectory(path.to_path_buf()));
    }
    Ok(())
}

#[cfg(windows)]
fn link_or_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_type().is_symlink() || metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn link_or_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

/// Errors returned by trust, descriptor, payload, and generation operations.
#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("signed envelope exceeds the bounded input limit ({0} bytes)")]
    EnvelopeTooLarge(usize),
    #[error("signed payload exceeds the bounded input limit ({0} bytes)")]
    PayloadTooLarge(usize),
    #[error("signed envelope is malformed")]
    MalformedEnvelope,
    #[error("signed descriptor is malformed")]
    MalformedDescriptor,
    #[error("unsupported release schema {0}")]
    UnsupportedSchema(u16),
    #[error("unsupported release platform {0:?}")]
    UnsupportedPlatform(String),
    #[error("trusted release key is invalid")]
    InvalidTrustedKey,
    #[error("release key id is invalid: {0:?}")]
    InvalidKeyId(String),
    #[error("signed envelope key id does not match the pinned key")]
    KeyIdMismatch,
    #[error("signed release signature is invalid")]
    SignatureInvalid,
    #[error("release id is invalid: {0:?}")]
    InvalidReleaseId(String),
    #[error("release was issued in the future: {0}")]
    ReleaseIssuedInFuture(i64),
    #[error("release lifetime is invalid")]
    InvalidReleaseLifetime,
    #[error("release expired at {0}")]
    ReleaseExpired(i64),
    #[error("release sequence {sequence} is older than accepted floor {floor}")]
    ReleaseRollback { sequence: u64, floor: u64 },
    #[error("accepted generation marker is invalid: {0}")]
    InvalidCurrentMarker(PathBuf),
    #[error("accepted generation marker history regressed or is incomplete: {0}")]
    MarkerHistoryRegression(PathBuf),
    #[error("no accepted complete generation is available")]
    NoAcceptedGeneration,
    #[error("accepted release sequence conflicts with the existing generation")]
    SequenceConflict,
    #[error("system clock is unavailable")]
    ClockUnavailable,
    #[error("artifact identity is unknown: {0:?}")]
    UnknownArtifact(String),
    #[error("artifact identity {0} appears more than once")]
    DuplicateArtifact(ArtifactIdentity),
    #[error("artifact identity {0} is missing")]
    MissingArtifact(ArtifactIdentity),
    #[error("release has too many artifacts: {0}")]
    TooManyArtifacts(usize),
    #[error("artifact {artifact} has invalid size {size}")]
    InvalidArtifactSize {
        artifact: ArtifactIdentity,
        size: u64,
    },
    #[error("artifact digest is invalid: {0:?}")]
    InvalidDigest(String),
    #[error("artifact {artifact} has size {actual}, expected {expected}")]
    ArtifactSizeMismatch {
        artifact: ArtifactIdentity,
        expected: u64,
        actual: u64,
    },
    #[error("artifact {0} has an unexpected digest")]
    ArtifactDigestMismatch(ArtifactIdentity),
    #[error("unsafe store or destination path: {0}")]
    UnsafeStorePath(PathBuf),
    #[error("store path contains a symlink or reparse point: {0}")]
    SymlinkOrReparse(PathBuf),
    #[error("store path is not a directory: {0}")]
    NotDirectory(PathBuf),
    #[error("fixed destination for {0} is unsafe")]
    UnsafeDestination(ArtifactIdentity),
    #[error("staging directory already exists and may be interrupted: {0}")]
    InterruptedStaging(PathBuf),
    #[error("accepted generation marker filesystem error")]
    CurrentMarkerIo(#[source] io::Error),
    #[error("staging quarantine filesystem error")]
    QuarantineIo(#[source] io::Error),
    #[error("complete generation failed revalidation: {0}")]
    InvalidCompleteGeneration(PathBuf),
    #[error("staged publication integrity check failed: {0}")]
    PublicationIntegrity(PathBuf),
    #[cfg(windows)]
    #[error("Windows publication guard filesystem error")]
    WindowsGuardIo(#[source] io::Error),
    #[error("completion marker is too large")]
    MarkerTooLarge,
    #[error("staging name allocation was exhausted")]
    StagingNameExhausted,
    #[error("filesystem error")]
    Io(#[source] io::Error),
    #[error("staging filesystem error")]
    StagingIo(#[source] io::Error),
    #[error("activation filesystem error")]
    ActivationIo(#[source] io::Error),
}

impl UpdateError {
    /// Returns whether callers should keep the prior complete generation and
    /// block launch/update recovery until a fresh retry is available.
    pub const fn is_blocked(&self) -> bool {
        !matches!(
            self,
            Self::Io(_) | Self::StagingIo(_) | Self::ActivationIo(_)
        )
    }
}
