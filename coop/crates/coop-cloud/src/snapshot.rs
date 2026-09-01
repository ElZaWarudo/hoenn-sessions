//! Revisioned snapshot contracts with CAS and session fencing.

use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, SeqAccess, Visitor},
};
use sha2::{Digest, Sha256};
use std::{
    fmt,
    net::{Ipv4Addr, Ipv6Addr},
    str::FromStr,
};
use thiserror::Error;
use zeroize::Zeroize;

use crate::{
    ApiVersion, CharacterId, ClientInstanceId, CommitId, IdempotencyKey, Revision, SessionEpoch,
    SessionId, Sha256Digest, UnixTimestampMillis, ids::IdError, ids::deserialize_bounded_string,
};

/// Snapshot validation and compare-and-swap failures.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum SnapshotError {
    #[error("snapshot path is not a safe canonical relative path")]
    InvalidPath,
    #[error("snapshot file size does not match its content")]
    SizeMismatch,
    #[error("finalized revision must be exactly one greater than its parent")]
    NonMonotonicRevision,
    #[error("revision zero is valid only for an initial character")]
    InvalidZeroRevision,
    #[error("snapshot contains duplicate file paths")]
    DuplicateFile,
    #[error("snapshot must contain character.sav")]
    MissingSav,
    #[error("snapshot must contain pending_commits.json")]
    MissingPendingCommits,
    #[error("snapshot/package artifact collection exceeds three items")]
    TooManyArtifacts,
    #[error("snapshot must declare character.sav and pending_commits.json")]
    TooFewArtifacts,
    #[error("pending_commits_sha256 does not match pending_commits.json")]
    PendingDigestMismatch,
    #[error("upload target URL is invalid")]
    InvalidUploadUrl,
    #[error("upload target method must be PUT")]
    InvalidUploadMethod,
    #[error("upload target expires_at must be non-zero")]
    InvalidUploadExpiry,
    #[error("snapshot validation failed: {0}")]
    Invalid(String),
    #[error("identifier validation failed: {0}")]
    Identifier(#[from] IdError),
}

/// The fixed package artifact names accepted by the cloud boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ArtifactIdentity {
    CharacterSav,
    PendingCommits,
    ResumeSs1,
}

impl ArtifactIdentity {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CharacterSav => "character.sav",
            Self::PendingCommits => "pending_commits.json",
            Self::ResumeSs1 => "resume.ss1",
        }
    }
}

impl Serialize for ArtifactIdentity {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ArtifactIdentity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match deserialize_bounded_string(deserializer, 32, "artifact identity")?.as_str() {
            "character.sav" => Ok(Self::CharacterSav),
            "pending_commits.json" => Ok(Self::PendingCommits),
            "resume.ss1" => Ok(Self::ResumeSs1),
            _ => Err(serde::de::Error::custom("unknown package artifact")),
        }
    }
}

/// The only upload operation supported by the snapshot contract.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum UploadMethod {
    Put,
}

impl Serialize for UploadMethod {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str("PUT")
    }
}

impl<'de> Deserialize<'de> for UploadMethod {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let method = deserialize_bounded_string(deserializer, 3, "upload method")?;
        if method == "PUT" {
            Ok(Self::Put)
        } else {
            Err(serde::de::Error::custom("upload method must be PUT"))
        }
    }
}

/// A complete, credential-bearing upload URL.
///
/// The URL itself is the upload capability.  It is intentionally redacted by
/// formatting implementations so a presigned query cannot leak into logs.
#[derive(Clone, Eq, PartialEq)]
pub struct UploadCapabilityUrl(String);

impl UploadCapabilityUrl {
    /// Validates and wraps a complete, bounded upload capability URL.
    ///
    /// # Errors
    ///
    /// Returns an error when the URL is malformed, lacks a non-empty query
    /// credential, contains userinfo or a fragment, uses an unsafe scheme, or
    /// exceeds the wire bound.
    pub fn new(value: impl Into<String>) -> Result<Self, SnapshotError> {
        let value = value.into();
        validate_upload_url(&value)?;
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for UploadCapabilityUrl {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Drop for UploadCapabilityUrl {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for UploadCapabilityUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

impl fmt::Display for UploadCapabilityUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

impl Serialize for UploadCapabilityUrl {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for UploadCapabilityUrl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(deserialize_bounded_string(
            deserializer,
            2048,
            "upload capability URL",
        )?)
        .map_err(serde::de::Error::custom)
    }
}

/// A server-signed upload destination for one fixed artifact.
#[derive(Clone, Eq, PartialEq)]
pub struct UploadTarget {
    pub artifact: ArtifactIdentity,
    pub method: UploadMethod,
    pub url: UploadCapabilityUrl,
    pub expires_at: UnixTimestampMillis,
}

#[derive(Serialize)]
struct SerializableUploadTarget<'a> {
    artifact: ArtifactIdentity,
    method: UploadMethod,
    url: &'a UploadCapabilityUrl,
    expires_at: UnixTimestampMillis,
}

impl UploadTarget {
    /// Validates a bounded, complete presigned/ticket-bearing URL.
    ///
    /// # Errors
    ///
    /// Returns an error for unsupported URL forms, fragments, userinfo,
    /// oversized URLs or zero expiry.
    pub fn new(
        artifact: ArtifactIdentity,
        method: UploadMethod,
        url: impl Into<String>,
        expires_at: UnixTimestampMillis,
    ) -> Result<Self, SnapshotError> {
        if method != UploadMethod::Put {
            return Err(SnapshotError::InvalidUploadMethod);
        }
        let url = UploadCapabilityUrl::new(url.into())?;
        if expires_at.value() == 0 {
            return Err(SnapshotError::InvalidUploadExpiry);
        }
        Ok(Self {
            artifact,
            method,
            url,
            expires_at,
        })
    }

    /// Explicit constructor for callers that model the method in their own
    /// request code.  `PUT` is the only accepted method.
    ///
    /// # Errors
    ///
    /// Returns an error when the method is unsupported, the URL is invalid, or
    /// the expiry timestamp is zero.
    pub fn new_with_method(
        artifact: ArtifactIdentity,
        method: UploadMethod,
        url: impl Into<String>,
        expires_at: UnixTimestampMillis,
    ) -> Result<Self, SnapshotError> {
        if method != UploadMethod::Put {
            return Err(SnapshotError::InvalidUploadMethod);
        }
        Self::new(artifact, method, url, expires_at)
    }

    /// Builds an upload target from an already validated capability URL.
    ///
    /// # Errors
    ///
    /// Returns an error for an unsupported method or zero expiry.
    pub fn new_with_capability_url(
        artifact: ArtifactIdentity,
        method: UploadMethod,
        url: UploadCapabilityUrl,
        expires_at: UnixTimestampMillis,
    ) -> Result<Self, SnapshotError> {
        if method != UploadMethod::Put {
            return Err(SnapshotError::InvalidUploadMethod);
        }
        if expires_at.value() == 0 {
            return Err(SnapshotError::InvalidUploadExpiry);
        }
        Ok(Self {
            artifact,
            method,
            url,
            expires_at,
        })
    }

    /// Convenience constructor for the sole supported upload method.
    ///
    /// # Errors
    ///
    /// Returns an error when the URL is invalid or the expiry timestamp is
    /// zero.
    pub fn new_put(
        artifact: ArtifactIdentity,
        url: impl Into<String>,
        expires_at: UnixTimestampMillis,
    ) -> Result<Self, SnapshotError> {
        Self::new(artifact, UploadMethod::Put, url, expires_at)
    }

    /// Revalidates the method, URL, and server expiry before execution.
    ///
    /// # Errors
    ///
    /// Returns an error when the method is not `PUT`, the URL is invalid, or
    /// the expiry timestamp is zero.
    pub fn validate(&self) -> Result<(), SnapshotError> {
        if self.method != UploadMethod::Put {
            return Err(SnapshotError::InvalidUploadMethod);
        }
        validate_upload_url(self.url.as_str())?;
        if self.expires_at.value() == 0 {
            return Err(SnapshotError::InvalidUploadExpiry);
        }
        Ok(())
    }

    #[must_use]
    pub const fn method(&self) -> UploadMethod {
        self.method
    }

    #[must_use]
    pub fn url(&self) -> &UploadCapabilityUrl {
        &self.url
    }
}

impl Serialize for UploadTarget {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.validate().map_err(serde::ser::Error::custom)?;
        SerializableUploadTarget {
            artifact: self.artifact,
            method: self.method,
            url: &self.url,
            expires_at: self.expires_at,
        }
        .serialize(serializer)
    }
}

impl fmt::Debug for UploadTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UploadTarget")
            .field("artifact", &self.artifact)
            .field("method", &self.method)
            .field("url", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

impl fmt::Display for UploadTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireUploadTarget {
    artifact: ArtifactIdentity,
    method: UploadMethod,
    url: UploadCapabilityUrl,
    expires_at: UnixTimestampMillis,
}

impl<'de> Deserialize<'de> for UploadTarget {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = WireUploadTarget::deserialize(deserializer)?;
        Self::new_with_capability_url(wire.artifact, wire.method, wire.url, wire.expires_at)
            .map_err(serde::de::Error::custom)
    }
}

fn validate_upload_url(url: &str) -> Result<(), SnapshotError> {
    if url.len() > 2048
        || url.contains('#')
        || !has_valid_percent_encoding(url)
        || url
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte == b' ' || byte == b'\\')
    {
        return Err(SnapshotError::InvalidUploadUrl);
    }
    let (scheme, remainder) = url
        .split_once("://")
        .ok_or(SnapshotError::InvalidUploadUrl)?;
    if scheme != "https" && scheme != "http" {
        return Err(SnapshotError::InvalidUploadUrl);
    }
    let query_start = remainder.find('?').ok_or(SnapshotError::InvalidUploadUrl)?;
    let query = &remainder[query_start + 1..];
    if query.is_empty()
        || query.contains('?')
        || query.split('&').any(|pair| match pair.split_once('=') {
            Some((name, value)) => name.is_empty() || value.is_empty(),
            None => true,
        })
    {
        return Err(SnapshotError::InvalidUploadUrl);
    }
    let authority = remainder[..query_start]
        .split('/')
        .next()
        .unwrap_or_default();
    if authority.is_empty() || authority.contains('@') {
        return Err(SnapshotError::InvalidUploadUrl);
    }
    let (host, is_ipv6) = parse_upload_authority(authority)?;
    if scheme == "http" && !((!is_ipv6 && host == "127.0.0.1") || (is_ipv6 && host == "::1")) {
        return Err(SnapshotError::InvalidUploadUrl);
    }
    Ok(())
}

fn has_valid_percent_encoding(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
            {
                return false;
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    true
}

fn parse_upload_authority(authority: &str) -> Result<(&str, bool), SnapshotError> {
    if authority.starts_with('[') {
        let closing = authority.find(']').ok_or(SnapshotError::InvalidUploadUrl)?;
        let host = &authority[1..closing];
        if host.parse::<Ipv6Addr>().is_err() {
            return Err(SnapshotError::InvalidUploadUrl);
        }
        let suffix = &authority[closing + 1..];
        if !suffix.is_empty() {
            let port = suffix
                .strip_prefix(':')
                .ok_or(SnapshotError::InvalidUploadUrl)?;
            validate_upload_port(port)?;
        }
        return Ok((host, true));
    }
    if authority.contains('[') || authority.contains(']') {
        return Err(SnapshotError::InvalidUploadUrl);
    }
    let (host, port) = match authority.split_once(':') {
        Some((host, port)) if !port.is_empty() => (host, Some(port)),
        Some(_) => return Err(SnapshotError::InvalidUploadUrl),
        None => (authority, None),
    };
    if authority.matches(':').count() > 1 || host.is_empty() {
        return Err(SnapshotError::InvalidUploadUrl);
    }
    if let Some(port) = port {
        validate_upload_port(port)?;
    }
    if host.starts_with('.')
        || host.ends_with('.')
        || host.starts_with('-')
        || host.ends_with('-')
        || host.contains("..")
        || host
            .split('.')
            .any(|label| label.starts_with('-') || label.ends_with('-'))
        || !host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-')
    {
        return Err(SnapshotError::InvalidUploadUrl);
    }
    if host
        .bytes()
        .all(|byte| byte.is_ascii_digit() || byte == b'.')
        && Ipv4Addr::from_str(host).is_err()
    {
        return Err(SnapshotError::InvalidUploadUrl);
    }
    Ok((host, false))
}

fn validate_upload_port(port: &str) -> Result<(), SnapshotError> {
    if port.is_empty() || !port.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(SnapshotError::InvalidUploadUrl);
    }
    let value = port
        .parse::<u16>()
        .map_err(|_| SnapshotError::InvalidUploadUrl)?;
    if value == 0 {
        return Err(SnapshotError::InvalidUploadUrl);
    }
    Ok(())
}

/// A content-addressed file in a snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotFile {
    pub artifact: ArtifactIdentity,
    pub sha256: Sha256Digest,
    pub size_bytes: u64,
}

impl SnapshotFile {
    /// Creates a validated content-addressed file record.
    ///
    /// # Errors
    ///
    /// Returns an error for an unsafe or non-relative path.
    pub fn new(
        artifact: ArtifactIdentity,
        sha256: Sha256Digest,
        size_bytes: u64,
    ) -> Result<Self, SnapshotError> {
        Ok(Self {
            artifact,
            sha256,
            size_bytes,
        })
    }

    /// Creates a file record and computes its SHA-256 digest.
    ///
    /// # Errors
    ///
    /// Returns an error for an unsafe artifact path.
    pub fn from_bytes(artifact: ArtifactIdentity, bytes: &[u8]) -> Result<Self, SnapshotError> {
        Self::new(artifact, Sha256Digest::of_bytes(bytes), bytes.len() as u64)
    }

    /// Checks the content address and byte length.
    ///
    /// # Errors
    ///
    /// Returns an error when the bytes do not match the recorded size or digest.
    pub fn verify_bytes(&self, bytes: &[u8]) -> Result<(), SnapshotError> {
        if self.size_bytes != bytes.len() as u64 {
            return Err(SnapshotError::SizeMismatch);
        }
        let digest: [u8; 32] = Sha256::digest(bytes).into();
        if digest != *self.sha256.as_bytes() {
            return Err(SnapshotError::Invalid("file digest mismatch".to_owned()));
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireSnapshotFile {
    artifact: ArtifactIdentity,
    sha256: Sha256Digest,
    size_bytes: u64,
}

impl<'de> Deserialize<'de> for SnapshotFile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = WireSnapshotFile::deserialize(deserializer)?;
        Self::new(wire.artifact, wire.sha256, wire.size_bytes).map_err(serde::de::Error::custom)
    }
}

fn validate_files(files: &[SnapshotFile]) -> Result<(), SnapshotError> {
    if files.len() > 3 {
        return Err(SnapshotError::TooManyArtifacts);
    }
    if files.len() < 2 {
        return Err(SnapshotError::TooFewArtifacts);
    }
    let mut seen = [false; 3];
    for file in files {
        let slot = match file.artifact {
            ArtifactIdentity::CharacterSav => 0,
            ArtifactIdentity::PendingCommits => 1,
            ArtifactIdentity::ResumeSs1 => 2,
        };
        if seen[slot] {
            return Err(SnapshotError::DuplicateFile);
        }
        seen[slot] = true;
    }
    if !files
        .iter()
        .any(|file| file.artifact == ArtifactIdentity::CharacterSav)
    {
        return Err(SnapshotError::MissingSav);
    }
    if !files
        .iter()
        .any(|file| file.artifact == ArtifactIdentity::PendingCommits)
    {
        return Err(SnapshotError::MissingPendingCommits);
    }
    Ok(())
}

fn validate_declaration(
    files: &[SnapshotFile],
    pending_commits_sha256: Sha256Digest,
) -> Result<(), SnapshotError> {
    validate_files(files)?;
    if pending_digest(files)? != pending_commits_sha256 {
        return Err(SnapshotError::PendingDigestMismatch);
    }
    Ok(())
}

fn deserialize_snapshot_files<'de, D>(deserializer: D) -> Result<Vec<SnapshotFile>, D::Error>
where
    D: Deserializer<'de>,
{
    struct SnapshotFilesVisitor;

    impl<'de> Visitor<'de> for SnapshotFilesVisitor {
        type Value = Vec<SnapshotFile>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("at most three snapshot artifacts")
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut files = Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(3));
            while let Some(file) = sequence.next_element()? {
                if files.len() == 3 {
                    return Err(de::Error::custom(
                        "snapshot artifact collection exceeds three items",
                    ));
                }
                files.push(file);
            }
            Ok(files)
        }
    }

    deserializer.deserialize_seq(SnapshotFilesVisitor)
}

fn deserialize_snapshot_records<'de, D>(deserializer: D) -> Result<Vec<SnapshotRecord>, D::Error>
where
    D: Deserializer<'de>,
{
    struct SnapshotRecordsVisitor;

    impl<'de> Visitor<'de> for SnapshotRecordsVisitor {
        type Value = Vec<SnapshotRecord>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("at most one hundred snapshots")
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut records = Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(100));
            while let Some(record) = sequence.next_element()? {
                if records.len() == 100 {
                    return Err(de::Error::custom("snapshot list exceeds 100 items"));
                }
                records.push(record);
            }
            Ok(records)
        }
    }

    deserializer.deserialize_seq(SnapshotRecordsVisitor)
}

fn deserialize_upload_targets<'de, D>(deserializer: D) -> Result<Vec<UploadTarget>, D::Error>
where
    D: Deserializer<'de>,
{
    struct UploadTargetsVisitor;

    impl<'de> Visitor<'de> for UploadTargetsVisitor {
        type Value = Vec<UploadTarget>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("at most three upload targets")
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut targets = Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(3));
            while let Some(target) = sequence.next_element()? {
                if targets.len() == 3 {
                    return Err(de::Error::custom(
                        "upload target collection exceeds three items",
                    ));
                }
                targets.push(target);
            }
            Ok(targets)
        }
    }

    deserializer.deserialize_seq(UploadTargetsVisitor)
}

fn pending_digest(files: &[SnapshotFile]) -> Result<Sha256Digest, SnapshotError> {
    files
        .iter()
        .find(|file| file.artifact == ArtifactIdentity::PendingCommits)
        .map(|file| file.sha256)
        .ok_or(SnapshotError::MissingPendingCommits)
}

/// A prepared snapshot's immutable metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotRecord {
    pub api_version: ApiVersion,
    pub snapshot_id: crate::SnapshotId,
    pub session_id: SessionId,
    pub character_id: CharacterId,
    pub parent_revision: Revision,
    pub revision: Revision,
    pub session_epoch: SessionEpoch,
    pub files: Vec<SnapshotFile>,
    pub pending_commits_sha256: Sha256Digest,
    pub last_applied_commit: Option<CommitId>,
    pub created_at: UnixTimestampMillis,
}

#[derive(Serialize)]
struct SerializableSnapshotRecord<'a> {
    api_version: ApiVersion,
    snapshot_id: crate::SnapshotId,
    session_id: SessionId,
    character_id: CharacterId,
    parent_revision: Revision,
    revision: Revision,
    session_epoch: SessionEpoch,
    files: &'a [SnapshotFile],
    pending_commits_sha256: Sha256Digest,
    last_applied_commit: Option<CommitId>,
    created_at: UnixTimestampMillis,
}

impl Serialize for SnapshotRecord {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.validate().map_err(serde::ser::Error::custom)?;
        SerializableSnapshotRecord {
            api_version: self.api_version,
            snapshot_id: self.snapshot_id,
            session_id: self.session_id,
            character_id: self.character_id,
            parent_revision: self.parent_revision,
            revision: self.revision,
            session_epoch: self.session_epoch,
            files: &self.files,
            pending_commits_sha256: self.pending_commits_sha256,
            last_applied_commit: self.last_applied_commit,
            created_at: self.created_at,
        }
        .serialize(serializer)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireSnapshotRecord {
    api_version: ApiVersion,
    snapshot_id: crate::SnapshotId,
    session_id: SessionId,
    character_id: CharacterId,
    parent_revision: Revision,
    revision: Revision,
    session_epoch: SessionEpoch,
    #[serde(deserialize_with = "deserialize_snapshot_files")]
    files: Vec<SnapshotFile>,
    pending_commits_sha256: Sha256Digest,
    last_applied_commit: Option<CommitId>,
    created_at: UnixTimestampMillis,
}

impl<'de> Deserialize<'de> for SnapshotRecord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = WireSnapshotRecord::deserialize(deserializer)?;
        let record = Self {
            api_version: wire.api_version,
            snapshot_id: wire.snapshot_id,
            session_id: wire.session_id,
            character_id: wire.character_id,
            parent_revision: wire.parent_revision,
            revision: wire.revision,
            session_epoch: wire.session_epoch,
            files: wire.files,
            pending_commits_sha256: wire.pending_commits_sha256,
            last_applied_commit: wire.last_applied_commit,
            created_at: wire.created_at,
        };
        record.validate().map_err(serde::de::Error::custom)?;
        Ok(record)
    }
}

impl SnapshotRecord {
    /// Builds finalized immutable snapshot metadata with retained lineage.
    ///
    /// # Errors
    ///
    /// Returns an error when `revision` is not exactly one greater than
    /// `parent_revision`, or when artifacts are incomplete.
    #[expect(
        clippy::too_many_arguments,
        reason = "the snapshot record constructor mirrors its fixed wire fields"
    )]
    pub fn new(
        snapshot_id: crate::SnapshotId,
        fence: SnapshotFence,
        parent_revision: Revision,
        revision: Revision,
        files: Vec<SnapshotFile>,
        pending_commits_sha256: Sha256Digest,
        last_applied_commit: Option<CommitId>,
        created_at: UnixTimestampMillis,
    ) -> Result<Self, SnapshotError> {
        let expected_revision = parent_revision.next().map_err(SnapshotError::Identifier)?;
        if revision != expected_revision {
            return Err(SnapshotError::NonMonotonicRevision);
        }
        validate_files(&files)?;
        if pending_digest(&files)? != pending_commits_sha256 {
            return Err(SnapshotError::PendingDigestMismatch);
        }
        Ok(Self {
            api_version: ApiVersion::V1,
            snapshot_id,
            session_id: fence.session_id,
            character_id: fence.character_id,
            parent_revision,
            revision,
            session_epoch: fence.session_epoch,
            files,
            pending_commits_sha256,
            last_applied_commit,
            created_at,
        })
    }

    /// Revalidates finalized snapshot metadata.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid revision or file list.
    pub fn validate(&self) -> Result<(), SnapshotError> {
        ApiVersion::new(self.api_version.value())
            .map_err(|error| SnapshotError::Invalid(error.to_string()))?;
        SessionEpoch::new(self.session_epoch.value()).map_err(SnapshotError::Identifier)?;
        let expected_revision = self
            .parent_revision
            .next()
            .map_err(SnapshotError::Identifier)?;
        if self.revision != expected_revision {
            return Err(SnapshotError::NonMonotonicRevision);
        }
        validate_declaration(&self.files, self.pending_commits_sha256)
    }

    #[must_use]
    pub fn fence(&self) -> SnapshotFence {
        SnapshotFence {
            session_id: self.session_id,
            character_id: self.character_id,
            session_epoch: self.session_epoch,
        }
    }
}

/// The session portion of snapshot fencing.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotFence {
    pub session_id: SessionId,
    pub character_id: CharacterId,
    pub session_epoch: SessionEpoch,
}

impl SnapshotFence {
    #[must_use]
    pub const fn new(
        session_id: SessionId,
        character_id: CharacterId,
        session_epoch: SessionEpoch,
    ) -> Self {
        Self {
            session_id,
            character_id,
            session_epoch,
        }
    }
}

/// Prepare a snapshot against an expected parent revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotPrepareRequest {
    pub api_version: ApiVersion,
    pub snapshot_id: crate::SnapshotId,
    pub session_id: SessionId,
    pub character_id: CharacterId,
    pub expected_parent_revision: Revision,
    pub session_epoch: SessionEpoch,
    pub client_instance_id: ClientInstanceId,
    pub idempotency_key: IdempotencyKey,
    pub files: Vec<SnapshotFile>,
    pub pending_commits_sha256: Sha256Digest,
}

#[derive(Serialize)]
struct SerializableSnapshotPrepareRequest<'a> {
    api_version: ApiVersion,
    snapshot_id: crate::SnapshotId,
    session_id: SessionId,
    character_id: CharacterId,
    expected_parent_revision: Revision,
    session_epoch: SessionEpoch,
    client_instance_id: ClientInstanceId,
    idempotency_key: IdempotencyKey,
    files: &'a [SnapshotFile],
    pending_commits_sha256: Sha256Digest,
}

impl Serialize for SnapshotPrepareRequest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.validate().map_err(serde::ser::Error::custom)?;
        SerializableSnapshotPrepareRequest {
            api_version: self.api_version,
            snapshot_id: self.snapshot_id,
            session_id: self.session_id,
            character_id: self.character_id,
            expected_parent_revision: self.expected_parent_revision,
            session_epoch: self.session_epoch,
            client_instance_id: self.client_instance_id,
            idempotency_key: self.idempotency_key,
            files: &self.files,
            pending_commits_sha256: self.pending_commits_sha256,
        }
        .serialize(serializer)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireSnapshotPrepareRequest {
    api_version: ApiVersion,
    snapshot_id: crate::SnapshotId,
    session_id: SessionId,
    character_id: CharacterId,
    expected_parent_revision: Revision,
    session_epoch: SessionEpoch,
    client_instance_id: ClientInstanceId,
    idempotency_key: IdempotencyKey,
    #[serde(deserialize_with = "deserialize_snapshot_files")]
    files: Vec<SnapshotFile>,
    pending_commits_sha256: Sha256Digest,
}

impl SnapshotPrepareRequest {
    /// Builds a prepare request with a validated fixed artifact declaration.
    ///
    /// # Errors
    ///
    /// Returns an error when the declaration is incomplete, duplicated, or its
    /// pending-commits digest does not match the declared artifact.
    pub fn new(
        snapshot_id: crate::SnapshotId,
        fence: SnapshotPrepareFence,
        files: Vec<SnapshotFile>,
        pending_commits_sha256: Sha256Digest,
    ) -> Result<Self, SnapshotError> {
        fence
            .expected_parent_revision
            .next()
            .map_err(SnapshotError::Identifier)?;
        validate_declaration(&files, pending_commits_sha256)?;
        Ok(Self {
            api_version: ApiVersion::V1,
            snapshot_id,
            session_id: fence.session_id,
            character_id: fence.character_id,
            expected_parent_revision: fence.expected_parent_revision,
            session_epoch: fence.session_epoch,
            client_instance_id: fence.client_instance_id,
            idempotency_key: fence.idempotency_key,
            files,
            pending_commits_sha256,
        })
    }

    /// Validates the request's immutable artifact declaration.
    ///
    /// # Errors
    ///
    /// Returns an error when the declaration is incomplete, duplicated, or
    /// has a mismatched pending-commits digest.
    pub fn validate(&self) -> Result<(), SnapshotError> {
        ApiVersion::new(self.api_version.value())
            .map_err(|error| SnapshotError::Invalid(error.to_string()))?;
        SessionEpoch::new(self.session_epoch.value()).map_err(SnapshotError::Identifier)?;
        self.expected_parent_revision
            .next()
            .map_err(SnapshotError::Identifier)?;
        validate_declaration(&self.files, self.pending_commits_sha256)
    }

    #[must_use]
    pub const fn fence(&self) -> SnapshotFence {
        SnapshotFence::new(self.session_id, self.character_id, self.session_epoch)
    }
}

impl<'de> Deserialize<'de> for SnapshotPrepareRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = WireSnapshotPrepareRequest::deserialize(deserializer)?;
        let request = Self {
            api_version: wire.api_version,
            snapshot_id: wire.snapshot_id,
            session_id: wire.session_id,
            character_id: wire.character_id,
            expected_parent_revision: wire.expected_parent_revision,
            session_epoch: wire.session_epoch,
            client_instance_id: wire.client_instance_id,
            idempotency_key: wire.idempotency_key,
            files: wire.files,
            pending_commits_sha256: wire.pending_commits_sha256,
        };
        request.validate().map_err(serde::de::Error::custom)?;
        Ok(request)
    }
}

/// Fencing and CAS inputs for snapshot preparation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnapshotPrepareFence {
    pub session_id: SessionId,
    pub character_id: CharacterId,
    pub expected_parent_revision: Revision,
    pub session_epoch: SessionEpoch,
    pub client_instance_id: ClientInstanceId,
    pub idempotency_key: IdempotencyKey,
}

impl SnapshotPrepareFence {
    #[must_use]
    pub const fn new(
        session_id: SessionId,
        character_id: CharacterId,
        expected_parent_revision: Revision,
        session_epoch: SessionEpoch,
        client_instance_id: ClientInstanceId,
        idempotency_key: IdempotencyKey,
    ) -> Self {
        Self {
            session_id,
            character_id,
            expected_parent_revision,
            session_epoch,
            client_instance_id,
            idempotency_key,
        }
    }
}

/// Result of a successful snapshot prepare.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotPrepareResponse {
    pub api_version: ApiVersion,
    pub snapshot_id: crate::SnapshotId,
    pub expected_parent_revision: Revision,
    pub next_revision: Revision,
    pub session_epoch: SessionEpoch,
    pub idempotency_key: IdempotencyKey,
    pub files: Vec<SnapshotFile>,
    pub pending_commits_sha256: Sha256Digest,
    pub upload_targets: Vec<UploadTarget>,
}

#[derive(Serialize)]
struct SerializableSnapshotPrepareResponse<'a> {
    api_version: ApiVersion,
    snapshot_id: crate::SnapshotId,
    expected_parent_revision: Revision,
    next_revision: Revision,
    session_epoch: SessionEpoch,
    idempotency_key: IdempotencyKey,
    files: &'a [SnapshotFile],
    pending_commits_sha256: Sha256Digest,
    upload_targets: &'a [UploadTarget],
}

impl Serialize for SnapshotPrepareResponse {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.validate().map_err(serde::ser::Error::custom)?;
        SerializableSnapshotPrepareResponse {
            api_version: self.api_version,
            snapshot_id: self.snapshot_id,
            expected_parent_revision: self.expected_parent_revision,
            next_revision: self.next_revision,
            session_epoch: self.session_epoch,
            idempotency_key: self.idempotency_key,
            files: &self.files,
            pending_commits_sha256: self.pending_commits_sha256,
            upload_targets: &self.upload_targets,
        }
        .serialize(serializer)
    }
}

impl SnapshotPrepareResponse {
    /// Validates one signed upload target per declared artifact.
    ///
    /// # Errors
    ///
    /// Returns an error for oversized, incomplete, or duplicate artifact lists.
    pub fn validate(&self) -> Result<(), SnapshotError> {
        ApiVersion::new(self.api_version.value())
            .map_err(|error| SnapshotError::Invalid(error.to_string()))?;
        SessionEpoch::new(self.session_epoch.value()).map_err(SnapshotError::Identifier)?;
        let expected_next = self
            .expected_parent_revision
            .next()
            .map_err(SnapshotError::Identifier)?;
        if self.next_revision != expected_next {
            return Err(SnapshotError::NonMonotonicRevision);
        }
        validate_files(&self.files)?;
        if pending_digest(&self.files)? != self.pending_commits_sha256 {
            return Err(SnapshotError::PendingDigestMismatch);
        }
        if self.upload_targets.len() != self.files.len() {
            return Err(SnapshotError::Invalid(
                "upload target count does not match artifact count".to_owned(),
            ));
        }
        for (index, target) in self.upload_targets.iter().enumerate() {
            target.validate()?;
            if self.upload_targets[..index]
                .iter()
                .any(|prior| prior.artifact == target.artifact)
                || !self
                    .files
                    .iter()
                    .any(|file| file.artifact == target.artifact)
            {
                return Err(SnapshotError::DuplicateFile);
            }
        }
        Ok(())
    }

    /// Returns whether this response fully correlates to the originating
    /// prepare request.
    ///
    /// Validation is part of correlation: callers must not act on a response
    /// whose `next_revision` is not the exact successor of its parent or
    /// whose upload targets do not cover the declaration.
    #[must_use]
    pub fn matches_request(&self, request: &SnapshotPrepareRequest) -> bool {
        self.validate().is_ok()
            && request.validate().is_ok()
            && self.snapshot_id == request.snapshot_id
            && self.expected_parent_revision == request.expected_parent_revision
            && self.session_epoch == request.session_epoch
            && self.idempotency_key == request.idempotency_key
            && self.files == request.files
            && self.pending_commits_sha256 == request.pending_commits_sha256
    }

    /// Backwards-compatible name for full request correlation.
    #[must_use]
    pub fn echoes_declaration(&self, request: &SnapshotPrepareRequest) -> bool {
        self.matches_request(request)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireSnapshotPrepareResponse {
    api_version: ApiVersion,
    snapshot_id: crate::SnapshotId,
    expected_parent_revision: Revision,
    next_revision: Revision,
    session_epoch: SessionEpoch,
    idempotency_key: IdempotencyKey,
    #[serde(deserialize_with = "deserialize_snapshot_files")]
    files: Vec<SnapshotFile>,
    pending_commits_sha256: Sha256Digest,
    #[serde(deserialize_with = "deserialize_upload_targets")]
    upload_targets: Vec<UploadTarget>,
}

impl<'de> Deserialize<'de> for SnapshotPrepareResponse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = WireSnapshotPrepareResponse::deserialize(deserializer)?;
        let response = Self {
            api_version: wire.api_version,
            snapshot_id: wire.snapshot_id,
            expected_parent_revision: wire.expected_parent_revision,
            next_revision: wire.next_revision,
            session_epoch: wire.session_epoch,
            idempotency_key: wire.idempotency_key,
            files: wire.files,
            pending_commits_sha256: wire.pending_commits_sha256,
            upload_targets: wire.upload_targets,
        };
        response.validate().map_err(serde::de::Error::custom)?;
        Ok(response)
    }
}

/// Finalize an already prepared snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotFinalizeRequest {
    pub api_version: ApiVersion,
    pub snapshot_id: crate::SnapshotId,
    pub session_id: SessionId,
    pub character_id: CharacterId,
    pub expected_parent_revision: Revision,
    pub revision: Revision,
    pub session_epoch: SessionEpoch,
    pub client_instance_id: ClientInstanceId,
    pub idempotency_key: IdempotencyKey,
    pub files: Vec<SnapshotFile>,
    pub pending_commits_sha256: Sha256Digest,
    pub last_applied_commit: Option<CommitId>,
}

#[derive(Serialize)]
struct SerializableSnapshotFinalizeRequest<'a> {
    api_version: ApiVersion,
    snapshot_id: crate::SnapshotId,
    session_id: SessionId,
    character_id: CharacterId,
    expected_parent_revision: Revision,
    revision: Revision,
    session_epoch: SessionEpoch,
    client_instance_id: ClientInstanceId,
    idempotency_key: IdempotencyKey,
    files: &'a [SnapshotFile],
    pending_commits_sha256: Sha256Digest,
    last_applied_commit: Option<CommitId>,
}

impl Serialize for SnapshotFinalizeRequest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.validate().map_err(serde::ser::Error::custom)?;
        SerializableSnapshotFinalizeRequest {
            api_version: self.api_version,
            snapshot_id: self.snapshot_id,
            session_id: self.session_id,
            character_id: self.character_id,
            expected_parent_revision: self.expected_parent_revision,
            revision: self.revision,
            session_epoch: self.session_epoch,
            client_instance_id: self.client_instance_id,
            idempotency_key: self.idempotency_key,
            files: &self.files,
            pending_commits_sha256: self.pending_commits_sha256,
            last_applied_commit: self.last_applied_commit,
        }
        .serialize(serializer)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireSnapshotFinalizeRequest {
    api_version: ApiVersion,
    snapshot_id: crate::SnapshotId,
    session_id: SessionId,
    character_id: CharacterId,
    expected_parent_revision: Revision,
    revision: Revision,
    session_epoch: SessionEpoch,
    client_instance_id: ClientInstanceId,
    idempotency_key: IdempotencyKey,
    #[serde(deserialize_with = "deserialize_snapshot_files")]
    files: Vec<SnapshotFile>,
    pending_commits_sha256: Sha256Digest,
    last_applied_commit: Option<CommitId>,
}

impl<'de> Deserialize<'de> for SnapshotFinalizeRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = WireSnapshotFinalizeRequest::deserialize(deserializer)?;
        let request = Self {
            api_version: wire.api_version,
            snapshot_id: wire.snapshot_id,
            session_id: wire.session_id,
            character_id: wire.character_id,
            expected_parent_revision: wire.expected_parent_revision,
            revision: wire.revision,
            session_epoch: wire.session_epoch,
            client_instance_id: wire.client_instance_id,
            idempotency_key: wire.idempotency_key,
            files: wire.files,
            pending_commits_sha256: wire.pending_commits_sha256,
            last_applied_commit: wire.last_applied_commit,
        };
        request.validate().map_err(serde::de::Error::custom)?;
        Ok(request)
    }
}

impl SnapshotFinalizeRequest {
    /// Builds a finalize request for exactly the next revision.
    ///
    /// # Errors
    ///
    /// Returns an error when the parent revision would overflow or the files
    /// are invalid.
    pub fn new(
        snapshot_id: crate::SnapshotId,
        fence: SnapshotFinalizeFence,
        files: Vec<SnapshotFile>,
        pending_commits_sha256: Sha256Digest,
        last_applied_commit: Option<CommitId>,
    ) -> Result<Self, SnapshotError> {
        let revision = fence
            .expected_parent_revision
            .next()
            .map_err(SnapshotError::Identifier)?;
        validate_declaration(&files, pending_commits_sha256)?;
        Ok(Self {
            api_version: ApiVersion::V1,
            snapshot_id,
            session_id: fence.session_id,
            character_id: fence.character_id,
            expected_parent_revision: fence.expected_parent_revision,
            revision,
            session_epoch: fence.session_epoch,
            client_instance_id: fence.client_instance_id,
            idempotency_key: fence.idempotency_key,
            files,
            pending_commits_sha256,
            last_applied_commit,
        })
    }

    /// Checks the expected-parent CAS and canonical file list.
    ///
    /// # Errors
    ///
    /// Returns an error when the revision is not exactly one greater than its parent.
    pub fn validate(&self) -> Result<(), SnapshotError> {
        ApiVersion::new(self.api_version.value())
            .map_err(|error| SnapshotError::Invalid(error.to_string()))?;
        SessionEpoch::new(self.session_epoch.value()).map_err(SnapshotError::Identifier)?;
        let expected = self
            .expected_parent_revision
            .next()
            .map_err(SnapshotError::Identifier)?;
        if self.revision != expected {
            return Err(SnapshotError::NonMonotonicRevision);
        }
        validate_declaration(&self.files, self.pending_commits_sha256)
    }

    /// Returns whether this finalize request still carries the immutable
    /// declaration recorded during prepare.
    #[must_use]
    pub fn matches_declaration(
        &self,
        files: &[SnapshotFile],
        pending_commits_sha256: Sha256Digest,
    ) -> bool {
        self.files == files && self.pending_commits_sha256 == pending_commits_sha256
    }
}

/// Parameters needed to finalize a prepared snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnapshotFinalizeFence {
    pub session_id: SessionId,
    pub character_id: CharacterId,
    pub expected_parent_revision: Revision,
    pub session_epoch: SessionEpoch,
    pub client_instance_id: ClientInstanceId,
    pub idempotency_key: IdempotencyKey,
}

impl SnapshotFinalizeFence {
    #[must_use]
    pub const fn new(
        session_id: SessionId,
        character_id: CharacterId,
        expected_parent_revision: Revision,
        session_epoch: SessionEpoch,
        client_instance_id: ClientInstanceId,
        idempotency_key: IdempotencyKey,
    ) -> Self {
        Self {
            session_id,
            character_id,
            expected_parent_revision,
            session_epoch,
            client_instance_id,
            idempotency_key,
        }
    }
}

/// List a character's snapshots, fenced to the current session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnapshotListRequest {
    pub api_version: ApiVersion,
    pub session_id: SessionId,
    pub character_id: CharacterId,
    pub session_epoch: SessionEpoch,
    pub client_instance_id: ClientInstanceId,
    pub limit: u16,
}

#[derive(Serialize)]
struct SerializableSnapshotListRequest {
    api_version: ApiVersion,
    session_id: SessionId,
    character_id: CharacterId,
    session_epoch: SessionEpoch,
    client_instance_id: ClientInstanceId,
    limit: u16,
}

impl Serialize for SnapshotListRequest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.validate().map_err(serde::ser::Error::custom)?;
        SerializableSnapshotListRequest {
            api_version: self.api_version,
            session_id: self.session_id,
            character_id: self.character_id,
            session_epoch: self.session_epoch,
            client_instance_id: self.client_instance_id,
            limit: self.limit,
        }
        .serialize(serializer)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireSnapshotListRequest {
    api_version: ApiVersion,
    session_id: SessionId,
    character_id: CharacterId,
    session_epoch: SessionEpoch,
    client_instance_id: ClientInstanceId,
    limit: u16,
}

impl SnapshotListRequest {
    /// Creates a bounded, session-fenced snapshot list request.
    ///
    /// # Errors
    ///
    /// Returns an error when `limit` is outside 1..=100.
    pub fn new(
        session_id: SessionId,
        character_id: CharacterId,
        session_epoch: SessionEpoch,
        client_instance_id: ClientInstanceId,
        limit: u16,
    ) -> Result<Self, SnapshotError> {
        if !(1..=100).contains(&limit) {
            return Err(SnapshotError::Invalid("limit must be 1..=100".to_owned()));
        }
        Ok(Self {
            api_version: ApiVersion::V1,
            session_id,
            character_id,
            session_epoch,
            client_instance_id,
            limit,
        })
    }

    /// Validates a deserialized list request.
    ///
    /// # Errors
    ///
    /// Returns an error when the limit is outside 1..=100.
    pub fn validate(&self) -> Result<(), SnapshotError> {
        ApiVersion::new(self.api_version.value())
            .map_err(|error| SnapshotError::Invalid(error.to_string()))?;
        SessionEpoch::new(self.session_epoch.value()).map_err(SnapshotError::Identifier)?;
        if !(1..=100).contains(&self.limit) {
            return Err(SnapshotError::Invalid("limit must be 1..=100".to_owned()));
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for SnapshotListRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = WireSnapshotListRequest::deserialize(deserializer)?;
        let request = Self {
            api_version: wire.api_version,
            session_id: wire.session_id,
            character_id: wire.character_id,
            session_epoch: wire.session_epoch,
            client_instance_id: wire.client_instance_id,
            limit: wire.limit,
        };
        request.validate().map_err(serde::de::Error::custom)?;
        Ok(request)
    }
}

/// Snapshot list response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotListResponse {
    pub api_version: ApiVersion,
    pub snapshots: Vec<SnapshotRecord>,
}

#[derive(Serialize)]
struct SerializableSnapshotListResponse<'a> {
    api_version: ApiVersion,
    snapshots: &'a [SnapshotRecord],
}

impl Serialize for SnapshotListResponse {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.validate().map_err(serde::ser::Error::custom)?;
        SerializableSnapshotListResponse {
            api_version: self.api_version,
            snapshots: &self.snapshots,
        }
        .serialize(serializer)
    }
}

impl SnapshotListResponse {
    /// Validates the API version and bounded result collection.
    ///
    /// # Errors
    ///
    /// Returns an error for an unsupported API version or more than one
    /// hundred records.
    pub fn validate(&self) -> Result<(), SnapshotError> {
        ApiVersion::new(self.api_version.value())
            .map_err(|error| SnapshotError::Invalid(error.to_string()))?;
        if self.snapshots.len() > 100 {
            return Err(SnapshotError::TooManyArtifacts);
        }
        for snapshot in &self.snapshots {
            snapshot.validate()?;
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireSnapshotListResponse {
    api_version: ApiVersion,
    #[serde(deserialize_with = "deserialize_snapshot_records")]
    snapshots: Vec<SnapshotRecord>,
}

impl<'de> Deserialize<'de> for SnapshotListResponse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = WireSnapshotListResponse::deserialize(deserializer)?;
        let response = Self {
            api_version: wire.api_version,
            snapshots: wire.snapshots,
        };
        response.validate().map_err(serde::de::Error::custom)?;
        Ok(response)
    }
}

/// Restore a snapshot after checking session and expected revision fencing.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotRestoreRequest {
    pub api_version: ApiVersion,
    pub snapshot_id: crate::SnapshotId,
    pub session_id: SessionId,
    pub character_id: CharacterId,
    pub expected_revision: Revision,
    pub session_epoch: SessionEpoch,
    pub client_instance_id: ClientInstanceId,
    pub idempotency_key: IdempotencyKey,
}

impl SnapshotRestoreRequest {
    #[must_use]
    pub const fn new(
        snapshot_id: crate::SnapshotId,
        session_id: SessionId,
        character_id: CharacterId,
        expected_revision: Revision,
        session_epoch: SessionEpoch,
        client_instance_id: ClientInstanceId,
        idempotency_key: IdempotencyKey,
    ) -> Self {
        Self {
            api_version: ApiVersion::V1,
            snapshot_id,
            session_id,
            character_id,
            expected_revision,
            session_epoch,
            client_instance_id,
            idempotency_key,
        }
    }
}

/// Restore response includes both pending and last-applied commit markers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotRestoreResponse {
    pub api_version: ApiVersion,
    pub snapshot: SnapshotRecord,
    pub pending_commits_sha256: Sha256Digest,
    pub last_applied_commit: Option<CommitId>,
}

#[derive(Serialize)]
struct SerializableSnapshotRestoreResponse<'a> {
    api_version: ApiVersion,
    snapshot: &'a SnapshotRecord,
    pending_commits_sha256: Sha256Digest,
    last_applied_commit: Option<CommitId>,
}

impl Serialize for SnapshotRestoreResponse {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.validate().map_err(serde::ser::Error::custom)?;
        SerializableSnapshotRestoreResponse {
            api_version: self.api_version,
            snapshot: &self.snapshot,
            pending_commits_sha256: self.pending_commits_sha256,
            last_applied_commit: self.last_applied_commit,
        }
        .serialize(serializer)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireSnapshotRestoreResponse {
    api_version: ApiVersion,
    snapshot: SnapshotRecord,
    pending_commits_sha256: Sha256Digest,
    last_applied_commit: Option<CommitId>,
}

impl<'de> Deserialize<'de> for SnapshotRestoreResponse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = WireSnapshotRestoreResponse::deserialize(deserializer)?;
        let response = Self {
            api_version: wire.api_version,
            snapshot: wire.snapshot,
            pending_commits_sha256: wire.pending_commits_sha256,
            last_applied_commit: wire.last_applied_commit,
        };
        response.validate().map_err(serde::de::Error::custom)?;
        Ok(response)
    }
}

impl SnapshotRestoreResponse {
    /// Validates the restored record and echoed commit markers.
    ///
    /// # Errors
    ///
    /// Returns an error when the API version, record, or echoed digest does
    /// not match the immutable snapshot record.
    pub fn validate(&self) -> Result<(), SnapshotError> {
        ApiVersion::new(self.api_version.value())
            .map_err(|error| SnapshotError::Invalid(error.to_string()))?;
        self.snapshot.validate()?;
        if self.pending_commits_sha256 != self.snapshot.pending_commits_sha256
            || self.last_applied_commit != self.snapshot.last_applied_commit
        {
            return Err(SnapshotError::Invalid(
                "restore commit markers do not match snapshot".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Aliases for clients using verb-first names.
pub type PrepareSnapshotRequest = SnapshotPrepareRequest;
pub type PrepareSnapshotResponse = SnapshotPrepareResponse;
pub type FinalizeSnapshotRequest = SnapshotFinalizeRequest;
pub type ListSnapshotsRequest = SnapshotListRequest;
pub type ListSnapshotsResponse = SnapshotListResponse;
pub type RestoreSnapshotRequest = SnapshotRestoreRequest;
pub type RestoreSnapshotResponse = SnapshotRestoreResponse;
