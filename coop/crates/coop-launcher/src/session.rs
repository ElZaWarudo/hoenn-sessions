//! Fenced cloud session materialization and checkpoint orchestration.

use std::{
    fs::{File, OpenOptions},
    future::Future,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use coop_cloud::{
    AcquireLeaseRequest, ArtifactIdentity, CharacterId, ClientInstanceId, HeartbeatLeaseRequest,
    IdempotencyKey, LeaseContract, PrepareSnapshotRequest, ReconnectLeaseRequest,
    ReleaseLeaseRequest, ResumePackageManifest, ResumeSelection, Revision, Sha256Digest,
    SignedManifestEnvelope, SnapshotFile, SnapshotFinalizeFence, SnapshotFinalizeRequest,
    SnapshotId, SnapshotListRequest, SnapshotListResponse, SnapshotPrepareFence,
    SnapshotPrepareResponse, SnapshotRecord, SnapshotRestoreRequest, SnapshotRestoreResponse,
    TrustedManifestKey, UploadTarget,
};
use coop_sidecar::control::{CheckpointGrant, CommandStatus, ControlCommand, ControlEvent};
use tempfile::TempDir;
use thiserror::Error;

const MAX_SESSION_FILE_BYTES: usize = 64 * 1024 * 1024;
/// Discovery may be short, but once a checkpoint starts it gets enough time
/// for the sidecar save notification and the bounded cloud round trips.
const SHUTDOWN_READY_DISCOVERY: Duration = Duration::from_millis(100);
const CHECKPOINT_PROTOCOL_DEADLINE: Duration = Duration::from_secs(10);

use crate::{
    auth::{AuthApi, AuthError, AuthSession},
    compat::BuildCompatibility,
    epoch::{EpochError, EpochStore},
    keychain::{KeychainError, RefreshTokenStore},
    process::{ControlChannel, ProcessError, SupervisedChildren, SupervisorEvent, new_command_id},
};

pub type CloudFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, SessionError>> + Send + 'a>>;

/// HTTP or deterministic fake cloud adapter. Wire values are all coop-cloud DTOs.
pub trait CloudApi: AuthApi {
    fn acquire<'a>(
        &'a self,
        auth: &'a AuthSession,
        request: AcquireLeaseRequest,
    ) -> CloudFuture<'a, LeaseContract>;
    fn heartbeat<'a>(
        &'a self,
        auth: &'a AuthSession,
        request: HeartbeatLeaseRequest,
    ) -> CloudFuture<'a, LeaseContract>;
    fn reconnect<'a>(
        &'a self,
        auth: &'a AuthSession,
        request: ReconnectLeaseRequest,
    ) -> CloudFuture<'a, LeaseContract>;
    fn release<'a>(
        &'a self,
        auth: &'a AuthSession,
        request: ReleaseLeaseRequest,
    ) -> CloudFuture<'a, coop_cloud::LogoutResponse>;
    fn resume_package<'a>(
        &'a self,
        auth: &'a AuthSession,
        character: CharacterId,
        revision: Revision,
    ) -> CloudFuture<'a, Option<SignedManifestEnvelope>>;
    fn artifact<'a>(
        &'a self,
        auth: &'a AuthSession,
        character: CharacterId,
        artifact: ArtifactIdentity,
        revision: Revision,
    ) -> CloudFuture<'a, Vec<u8>>;
    fn list_snapshots<'a>(
        &'a self,
        auth: &'a AuthSession,
        request: SnapshotListRequest,
    ) -> CloudFuture<'a, SnapshotListResponse>;
    fn restore<'a>(
        &'a self,
        auth: &'a AuthSession,
        request: SnapshotRestoreRequest,
    ) -> CloudFuture<'a, SnapshotRestoreResponse>;
    fn prepare<'a>(
        &'a self,
        auth: &'a AuthSession,
        request: PrepareSnapshotRequest,
    ) -> CloudFuture<'a, SnapshotPrepareResponse>;
    fn upload<'a>(&'a self, target: &'a UploadTarget, bytes: Vec<u8>) -> CloudFuture<'a, ()>;
    fn finalize<'a>(
        &'a self,
        auth: &'a AuthSession,
        request: SnapshotFinalizeRequest,
    ) -> CloudFuture<'a, SnapshotRecord>;
}

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("cloud request failed")]
    Cloud,
    #[error("cloud authorization expired")]
    Unauthorized,
    #[error("requested artifact was not found")]
    ArtifactNotFound,
    #[error("invalid or stale lease")]
    Lease,
    #[error("epoch persistence failed")]
    Epoch(#[from] EpochError),
    #[error("authentication failed")]
    Auth(#[from] AuthError),
    #[error("signed resume package is invalid")]
    Package,
    #[error("active character.sav does not match its signed digest")]
    CorruptActiveSav,
    #[error("resume package is missing for a nonzero revision")]
    MissingPackage,
    #[error("only an owned revision-zero lease can bootstrap")]
    InvalidBootstrap,
    #[error("snapshot history is invalid or corrupt")]
    History,
    #[error("session filesystem operation failed")]
    Filesystem(#[source] std::io::Error),
    #[error("sidecar control protocol failed")]
    Control(#[from] ProcessError),
    #[error("checkpoint was not authorized")]
    CheckpointNotAuthorized,
    #[error("checkpoint response did not correlate to the active request")]
    CheckpointCorrelation,
    #[error("checkpoint protocol deadline exceeded")]
    CheckpointTimeout,
    #[error("snapshot finalization conflicted with a newer revision")]
    FinalizeConflict,
}

#[derive(Clone)]
pub struct SessionConfig {
    pub client_instance_id: ClientInstanceId,
    pub manifest: BuildCompatibility,
    pub trusted_manifest_key: TrustedManifestKey,
    pub epoch_store: EpochStore,
    pub workspace_parent: PathBuf,
    pub bridge_lua_dir: PathBuf,
}

impl std::fmt::Debug for SessionConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SessionConfig")
            .field("client_instance_id", &self.client_instance_id)
            .field("manifest", &self.manifest)
            .field("trusted_manifest_key", &"[PINNED]")
            .field("epoch_store", &self.epoch_store)
            .field("workspace_parent", &self.workspace_parent)
            .field("bridge_lua_dir", &self.bridge_lua_dir)
            .finish()
    }
}

pub struct SessionWorkspace {
    #[cfg(windows)]
    // Keep deny-delete handles for the complete parent and workspace
    // ancestry.  Holding only the immediate parent permits an ancestor
    // reparse/rename swap while a fixed file is being materialized.
    ancestor_guards: Vec<File>,
    temp: Option<TempDir>,
    stable_path: PathBuf,
    recovery_shadow: Option<RecoveryShadow>,
}

/// A secret-free SAV copy created as soon as an authorized checkpoint reads a
/// stable save. It remains detached from the session workspace only when
/// recovery is required; ordinary successful finalization drops it.
struct RecoveryShadow {
    temp: Option<TempDir>,
}

impl RecoveryShadow {
    fn create(parent: &Path, sav: &[u8]) -> Result<Self, SessionError> {
        reject_symlink_ancestors(parent).map_err(SessionError::Filesystem)?;
        let temp = tempfile::Builder::new()
            .prefix("coop-recovery-")
            .tempdir_in(parent)
            .map_err(SessionError::Filesystem)?;
        write_new_private_file(temp.path(), "character.sav", sav)?;
        write_new_private_file(temp.path(), "recovery.marker", b"coop-recovery-v1\n")?;
        Ok(Self { temp: Some(temp) })
    }

    fn keep(mut self) {
        if let Some(temp) = self.temp.take() {
            let _ = temp.keep();
        }
    }
}

impl std::fmt::Debug for SessionWorkspace {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SessionWorkspace")
            .field("path", &self.path())
            .finish()
    }
}

impl SessionWorkspace {
    /// Creates a new owner-private temporary session directory.
    ///
    /// # Errors
    ///
    /// Returns an error when the workspace parent cannot be created or opened.
    pub fn create(parent: &Path) -> Result<Self, SessionError> {
        reject_symlink_ancestors(parent).map_err(SessionError::Filesystem)?;
        if let Ok(metadata) = std::fs::symlink_metadata(parent)
            && metadata.file_type().is_symlink()
        {
            return Err(SessionError::Package);
        }
        std::fs::create_dir_all(parent).map_err(SessionError::Filesystem)?;
        reject_symlink_ancestors(parent).map_err(SessionError::Filesystem)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
                .map_err(SessionError::Filesystem)?;
        }
        let temp = tempfile::Builder::new()
            .prefix("coop-session-")
            .tempdir_in(parent)
            .map_err(SessionError::Filesystem)?;
        let stable_path = temp.path().to_path_buf();
        #[cfg(windows)]
        let mut ancestor_guards =
            open_directory_ancestor_guards(parent).map_err(SessionError::Filesystem)?;
        #[cfg(windows)]
        ancestor_guards.extend(
            open_directory_ancestor_guards(&stable_path).map_err(SessionError::Filesystem)?,
        );
        Ok(Self {
            #[cfg(windows)]
            ancestor_guards,
            temp: Some(temp),
            stable_path,
            recovery_shadow: None,
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.stable_path
    }

    /// Removes all bridge, resume, pending, and control material while
    /// retaining only the active SAV and a non-secret recovery marker.  The
    /// directory is deliberately detached from `TempDir` so it survives
    /// normal shutdown for operator recovery.
    ///
    /// # Errors
    ///
    /// Returns an error when sensitive material cannot be removed or the
    /// recovery marker cannot be written.
    pub fn preserve_recovery(&mut self) -> Result<PathBuf, SessionError> {
        for name in [
            "pending_commits.json",
            "resume.input.ss1",
            "resume.ss1",
            "session.lua",
            "main.lua",
            "memory.lua",
            "protocol.lua",
            "generated_addresses.lua",
        ] {
            let path = fixed_path(self.path(), name)?;
            match std::fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                    return Err(SessionError::Package);
                }
                Ok(_) => std::fs::remove_file(&path).map_err(SessionError::Filesystem)?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(SessionError::Filesystem(error)),
            }
        }
        let entries = std::fs::read_dir(self.path()).map_err(SessionError::Filesystem)?;
        for entry in entries {
            let entry = entry.map_err(SessionError::Filesystem)?;
            let name = entry.file_name();
            if name == "character.sav" || name == "recovery.marker" {
                continue;
            }
            let metadata =
                std::fs::symlink_metadata(entry.path()).map_err(SessionError::Filesystem)?;
            if metadata.file_type().is_symlink() || metadata.is_file() {
                std::fs::remove_file(entry.path()).map_err(SessionError::Filesystem)?;
            } else {
                return Err(SessionError::Package);
            }
        }
        // Recovery is useful only when it contains a regular, bounded,
        // non-empty SAV.  Do the final no-follow read after scrubbing so a
        // zero-byte placeholder or an over-sized replacement is never kept.
        let sav = self.read_fixed("character.sav")?;
        if sav.is_empty() {
            return Err(SessionError::Package);
        }
        self.write_atomic("recovery.marker", b"coop-recovery-v1\n")?;
        if let Some(temp) = self.temp.take() {
            let kept = temp.keep();
            debug_assert_eq!(kept, self.stable_path);
        }
        self.recovery_shadow.take();
        Ok(self.stable_path.clone())
    }

    fn create_recovery_shadow(&mut self, sav: &[u8]) -> Result<(), SessionError> {
        let parent = self
            .stable_path
            .parent()
            .ok_or_else(|| SessionError::Filesystem(io::Error::other("workspace has no parent")))?;
        let shadow = RecoveryShadow::create(parent, sav)?;
        self.recovery_shadow = Some(shadow);
        Ok(())
    }

    fn discard_recovery_shadow(&mut self) {
        let _ = self.recovery_shadow.take();
    }

    /// Detaches only a separately scrubbed recovery copy when recovery
    /// scrubbing itself failed. The original workspace is never retained,
    /// because it may still contain loopback or control secrets. The caller
    /// must abort the cloud release.
    fn retain_private_on_scrub_failure(&mut self) {
        // The shadow was fully written before any cloud upload. Keep it
        // directly, without copying from a workspace whose scrub may have
        // failed halfway through and still contain session/control secrets.
        if let Some(shadow) = self.recovery_shadow.take() {
            shadow.keep();
            #[cfg(windows)]
            let _ = std::mem::take(&mut self.ancestor_guards);
            let _ = self.temp.take();
            return;
        }
        // Never detach the partially scrubbed original.  It may still contain
        // Lua/control secrets or unknown files when an injected removal fails
        // halfway through. Instead quarantine only a bounded, no-follow SAV
        // copy in a fresh private directory, then drop the original.
        if let Some(parent) = self.stable_path.parent()
            && let Ok(mut quarantine) = Self::create(parent)
            && let Ok(sav) = self.read_fixed("character.sav")
            && !sav.is_empty()
            && quarantine.write_atomic("character.sav", &sav).is_ok()
            && quarantine
                .write_atomic("recovery.marker", b"coop-recovery-v1\n")
                .is_ok()
        {
            if let Some(temp) = quarantine.temp.take() {
                let _ = temp.keep();
            }
            #[cfg(windows)]
            let _ = std::mem::take(&mut self.ancestor_guards);
            let _ = self.temp.take();
            return;
        }

        // No bounded quarantine could be created. Confidentiality wins over
        // retaining a possibly secret-bearing directory: do not keep any
        // unknown file, partially scrubbed Lua, or control material.
        #[cfg(windows)]
        let _ = std::mem::take(&mut self.ancestor_guards);
        let _ = self.temp.take();
    }

    /// Atomically writes one fixed session filename.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown filename or filesystem failure.
    pub fn write_atomic(&self, name: &str, bytes: &[u8]) -> Result<PathBuf, SessionError> {
        if bytes.len() > MAX_SESSION_FILE_BYTES {
            return Err(SessionError::Package);
        }
        let path = fixed_path(self.path(), name)?;
        reject_symlink_ancestors(&path).map_err(SessionError::Filesystem)?;
        let temporary = self
            .path()
            .join(format!(".{name}.{}.tmp", uuid::Uuid::new_v4().simple()));
        reject_symlink(&path)?;
        reject_symlink(&temporary)?;
        #[cfg(windows)]
        if path.exists() {
            // Windows' portable rename operation is not an atomic replace.
            // Refuse to replace an existing fixed file rather than creating
            // a remove-then-rename window in which readers see no file.  The
            // launcher writes each materialized artifact once; callers that
            // need a new snapshot receive a fresh private workspace.
            return Err(SessionError::Filesystem(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "fixed session file already exists",
            )));
        }
        let result = (|| {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)
                .map_err(SessionError::Filesystem)?;
            file.write_all(bytes).map_err(SessionError::Filesystem)?;
            file.sync_all().map_err(SessionError::Filesystem)?;
            drop(file);
            reject_symlink_ancestors(&path).map_err(SessionError::Filesystem)?;
            std::fs::rename(&temporary, &path).map_err(SessionError::Filesystem)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result?;
        Ok(path)
    }

    fn read_fixed(&self, name: &str) -> Result<Vec<u8>, SessionError> {
        let path = fixed_path(self.path(), name)?;
        let metadata = std::fs::symlink_metadata(&path).map_err(SessionError::Filesystem)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(SessionError::Package);
        }
        read_bounded_file(&path, MAX_SESSION_FILE_BYTES)
    }

    /// Writes the non-secret control file consumed by the checked-in Lua bridge.
    ///
    /// # Errors
    ///
    /// Returns an error when the endpoint or secret is not a validated bridge value.
    pub fn write_session_lua(
        &self,
        host: &str,
        port: u16,
        secret: &str,
    ) -> Result<PathBuf, SessionError> {
        if host != "127.0.0.1"
            || port == 0
            || secret.len() != 32
            || !secret
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err(SessionError::Package);
        }
        let content =
            format!("return {{ host = \"127.0.0.1\", port = {port}, secret = \"{secret}\" }}\n");
        self.write_atomic("session.lua", content.as_bytes())
    }

    /// Copies only checked-in Lua bridge inputs into the private workspace.
    ///
    /// # Errors
    ///
    /// Returns an error when an input is missing or cannot be written.
    pub fn copy_bridge_inputs(&self, source: &Path) -> Result<(), SessionError> {
        reject_symlink_ancestors(source).map_err(SessionError::Filesystem)?;
        #[cfg(windows)]
        let _bridge_guards =
            open_directory_ancestor_guards(source).map_err(SessionError::Filesystem)?;
        if !is_canonical_path(source).map_err(SessionError::Filesystem)? {
            return Err(SessionError::Package);
        }
        for name in ["main.lua", "memory.lua", "protocol.lua"] {
            let source_file = source.join(name);
            let metadata =
                std::fs::symlink_metadata(&source_file).map_err(SessionError::Filesystem)?;
            if metadata.file_type().is_symlink()
                || !metadata.is_file()
                || metadata.len() > 1_048_576
            {
                return Err(SessionError::Package);
            }
            let bytes = read_bounded_file(&source_file, 1_048_576)?;
            let expected: &[u8] = match name {
                "main.lua" => include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../bridge/main.lua"
                )),
                "memory.lua" => include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../bridge/memory.lua"
                )),
                "protocol.lua" => include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../bridge/protocol.lua"
                )),
                _ => unreachable!("fixed bridge input list"),
            };
            if bytes != expected {
                return Err(SessionError::Package);
            }
            self.write_atomic(name, &bytes)?;
        }
        Ok(())
    }
}

fn write_new_private_file(root: &Path, name: &str, bytes: &[u8]) -> Result<(), SessionError> {
    if bytes.len() > MAX_SESSION_FILE_BYTES {
        return Err(SessionError::Package);
    }
    let path = fixed_path(root, name)?;
    reject_symlink(&path)?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(SessionError::Filesystem)?;
    file.write_all(bytes).map_err(SessionError::Filesystem)?;
    file.sync_all().map_err(SessionError::Filesystem)
}

#[cfg(windows)]
fn open_directory_guard(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    let mut options = OpenOptions::new();
    options
        .read(true)
        // Permit child-file creation/rename while denying directory
        // replacement or deletion by omitting FILE_SHARE_DELETE.
        .share_mode(0x0000_0003)
        .custom_flags(0x0220_0000);
    options.open(path)
}

#[cfg(windows)]
fn open_directory_ancestor_guards(path: &Path) -> io::Result<Vec<File>> {
    let mut guards = Vec::new();
    let mut current = path;
    loop {
        guards.push(open_directory_guard(current)?);
        let Some(parent) = current.parent() else {
            break;
        };
        if parent == current {
            break;
        }
        current = parent;
    }
    Ok(guards)
}

fn fixed_path(root: &Path, name: &str) -> Result<PathBuf, SessionError> {
    if !matches!(
        name,
        "character.sav"
            | "pending_commits.json"
            | "resume.input.ss1"
            | "resume.ss1"
            | "session.lua"
            | "main.lua"
            | "memory.lua"
            | "protocol.lua"
            | "generated_addresses.lua"
            | "recovery.marker"
    ) {
        return Err(SessionError::Filesystem(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "unsupported session filename",
        )));
    }
    Ok(root.join(name))
}

fn reject_symlink(path: &Path) -> Result<(), SessionError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(SessionError::Package),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(SessionError::Filesystem(error)),
    }
}

fn reject_symlink_ancestors(path: &Path) -> io::Result<()> {
    let mut current = path;
    loop {
        match std::fs::symlink_metadata(current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "symlink or reparse ancestor",
                ));
            }
            Ok(metadata) if current != path && !metadata.is_dir() => {
                return Err(io::Error::new(
                    io::ErrorKind::NotADirectory,
                    "non-directory path ancestor",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound && current == path => {}
            Err(error) => return Err(error),
        }
        let Some(parent) = current.parent() else {
            break;
        };
        if parent == current {
            break;
        }
        current = parent;
    }
    Ok(())
}

#[cfg(windows)]
fn canonical_path_key(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text))
}

#[cfg(not(windows))]
fn canonical_path_key(path: &Path) -> PathBuf {
    path.to_owned()
}

fn is_canonical_path(path: &Path) -> io::Result<bool> {
    let canonical = std::fs::canonicalize(path)?;
    Ok(canonical_path_key(&canonical) == canonical_path_key(path))
}

fn read_bounded_file(path: &Path, maximum: usize) -> Result<Vec<u8>, SessionError> {
    reject_symlink_ancestors(path).map_err(SessionError::Filesystem)?;
    let metadata = std::fs::symlink_metadata(path).map_err(SessionError::Filesystem)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(SessionError::Package);
    }
    let file = open_read_nofollow(path).map_err(SessionError::Filesystem)?;
    let mut bytes = Vec::new();
    file.take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(SessionError::Filesystem)?;
    if bytes.len() > maximum {
        return Err(SessionError::Package);
    }
    Ok(bytes)
}

fn open_read_nofollow(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_FLAG_OPEN_REPARSE_POINT prevents a final reparse point from
        // being followed between validation and opening the fixed file.
        options.custom_flags(0x0020_0000);
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Linux O_NOFOLLOW rejects a final symlink at the kernel boundary.
        options.custom_flags(0x0002_0000);
    }
    options.open(path)
}

pub struct SessionLifecycle {
    pub auth: AuthSession,
    pub lease: LeaseContract,
    pub revision: Revision,
    pub workspace: SessionWorkspace,
    config: SessionConfig,
    reconnect_key: IdempotencyKey,
    keychain: Option<Arc<dyn RefreshTokenStore>>,
    checkpoint_authorized: bool,
    checkpoint_key: Option<(u32, u32)>,
    fresh_resume_save_digest: Option<Sha256Digest>,
}

impl std::fmt::Debug for SessionLifecycle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SessionLifecycle")
            .field("character_id", &self.lease.character_id)
            .field("session_id", &self.lease.session_id)
            .field("revision", &self.revision)
            .field("auth", &self.auth)
            .field("config", &self.config)
            .field("reconnect_key", &"[REDACTED]")
            .field("keychain", &self.keychain.as_ref().map(|_| "[CONFIGURED]"))
            .field("checkpoint_authorized", &self.checkpoint_authorized)
            .field("checkpoint_key", &self.checkpoint_key)
            .field("fresh_resume_save_digest", &self.fresh_resume_save_digest)
            .field("workspace", &self.workspace)
            .finish_non_exhaustive()
    }
}

impl SessionLifecycle {
    /// Returns the server-selected heartbeat cadence for this lease.
    #[must_use]
    pub const fn heartbeat_interval(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.lease.heartbeat_interval_ms as u64)
    }

    /// Reports whether a heartbeat should be sent at the supplied Unix time.
    /// The calculation uses only server-owned expiry and interval values.
    #[must_use]
    pub fn heartbeat_due(&self, now_millis: u64) -> bool {
        now_millis.saturating_add(u64::from(self.lease.heartbeat_interval_ms))
            >= self.lease.expires_at.value()
    }

    /// Proactively rotates the access/refresh pair when the server-provided
    /// access expiry is within the auth safety window. Callers can invoke this
    /// from their heartbeat scheduler without exposing token material.
    ///
    /// # Errors
    ///
    /// Returns an error when the refresh response or keychain rotation fails.
    pub async fn refresh_auth<A: AuthApi, K: RefreshTokenStore>(
        &mut self,
        api: &A,
        keychain: &K,
        now_millis: u64,
    ) -> Result<bool, SessionError> {
        if !self.auth.should_refresh_at(now_millis) {
            return Ok(false);
        }
        self.auth.refresh_at(api, keychain, now_millis).await?;
        Ok(true)
    }

    /// Attaches the OS-keychain implementation used by the running session.
    /// Keeping this as a trait object avoids persisting or copying credential
    /// material into the lifecycle and lets deterministic tests provide a
    /// fake vault.
    pub fn attach_keychain(&mut self, keychain: Arc<dyn RefreshTokenStore>) {
        self.keychain = Some(keychain);
    }

    async fn refresh_if_needed<A: AuthApi>(&mut self, api: &A) -> Result<(), SessionError> {
        if self.auth.should_refresh_at(now_millis()) {
            self.refresh_required(api).await?;
        }
        Ok(())
    }

    async fn refresh_required<A: AuthApi>(&mut self, api: &A) -> Result<(), SessionError> {
        let keychain = self
            .keychain
            .clone()
            .ok_or(SessionError::Auth(AuthError::Keychain(
                KeychainError::Unavailable,
            )))?;
        self.auth.refresh(api, keychain.as_ref()).await?;
        Ok(())
    }

    async fn authenticated_artifact<A: CloudApi>(
        &mut self,
        api: &A,
        character: CharacterId,
        artifact: ArtifactIdentity,
        revision: Revision,
    ) -> Result<Vec<u8>, SessionError> {
        self.refresh_if_needed(api).await?;
        let first = self
            .run_with_heartbeats(api, |auth| {
                api.artifact(auth, character, artifact, revision)
            })
            .await;
        match first {
            Ok((bytes, lease)) => {
                self.install_heartbeat_lease(lease, self.revision)?;
                Ok(bytes)
            }
            Err(SessionError::Unauthorized) => {
                self.refresh_required(api).await?;
                let (bytes, lease) = self
                    .run_with_heartbeats(api, |auth| {
                        api.artifact(auth, character, artifact, revision)
                    })
                    .await?;
                self.install_heartbeat_lease(lease, self.revision)?;
                Ok(bytes)
            }
            Err(error) => Err(error),
        }
    }

    async fn resume_package_retry<A: CloudApi>(
        &mut self,
        api: &A,
        character: CharacterId,
        revision: Revision,
    ) -> Result<Option<SignedManifestEnvelope>, SessionError> {
        self.refresh_if_needed(api).await?;
        let first = self
            .run_with_heartbeats(api, |auth| api.resume_package(auth, character, revision))
            .await;
        match first {
            Ok((package, lease)) => {
                self.install_heartbeat_lease(lease, self.revision)?;
                Ok(package)
            }
            Err(SessionError::Unauthorized) => {
                self.refresh_required(api).await?;
                let (package, lease) = self
                    .run_with_heartbeats(api, |auth| api.resume_package(auth, character, revision))
                    .await?;
                self.install_heartbeat_lease(lease, self.revision)?;
                Ok(package)
            }
            Err(error) => Err(error),
        }
    }

    async fn list_snapshots_with_heartbeat<A: CloudApi>(
        &mut self,
        api: &A,
        request: SnapshotListRequest,
    ) -> Result<SnapshotListResponse, SessionError> {
        self.refresh_if_needed(api).await?;
        let deadline = tokio::time::Instant::now() + CHECKPOINT_PROTOCOL_DEADLINE;
        let first = tokio::time::timeout_at(
            deadline,
            self.run_with_heartbeats(api, |auth| api.list_snapshots(auth, request)),
        )
        .await
        .map_err(|_| SessionError::History)?;
        match first {
            Ok((response, lease)) => {
                self.install_heartbeat_lease(lease, self.revision)?;
                Ok(response)
            }
            Err(SessionError::Unauthorized) => {
                self.refresh_required(api).await?;
                let (response, lease) = tokio::time::timeout_at(
                    deadline,
                    self.run_with_heartbeats(api, |auth| api.list_snapshots(auth, request)),
                )
                .await
                .map_err(|_| SessionError::History)??;
                self.install_heartbeat_lease(lease, self.revision)?;
                Ok(response)
            }
            Err(error) => Err(error),
        }
    }

    /// Runs one potentially slow cloud operation while sending fenced
    /// heartbeats.  The operation and heartbeat borrow the immutable auth
    /// session concurrently; the returned lease is installed only after the
    /// operation future has finished, so a checkpoint cannot observe a
    /// partially-mutated fence.  A heartbeat failure aborts the operation and
    /// is deliberately not converted into a successful checkpoint.
    async fn run_with_heartbeats<'a, A, T, F>(
        &'a self,
        api: &'a A,
        operation: F,
    ) -> Result<(T, LeaseContract), SessionError>
    where
        A: CloudApi,
        F: FnOnce(&'a AuthSession) -> CloudFuture<'a, T>,
    {
        self.run_with_heartbeats_mode(api, operation, false, |_| true)
            .await
    }

    /// Runs a revision-mutating operation while allowing one old-fence lease
    /// response to race with the server's commit.  A finalize or restore can
    /// commit before its response reaches us; in that narrow window the
    /// heartbeat for revision N quite correctly reports the new head N+1.
    /// Keep polling the idempotent operation so its response (or a retry using
    /// the same request key) can prove the commit instead of discarding it as
    /// an unrelated lease failure.  Read-only artifact operations remain
    /// strict through `run_with_heartbeats`.
    async fn run_with_mutating_heartbeats<'a, A, T, F>(
        &'a self,
        api: &'a A,
        operation: F,
        committed: impl FnOnce(&T) -> bool,
    ) -> Result<(T, LeaseContract), SessionError>
    where
        A: CloudApi,
        F: FnOnce(&'a AuthSession) -> CloudFuture<'a, T>,
    {
        self.run_with_heartbeats_mode(api, operation, true, committed)
            .await
    }

    async fn run_with_heartbeats_mode<'a, A, T, F, V>(
        &'a self,
        api: &'a A,
        operation: F,
        allow_commit_race: bool,
        committed: V,
    ) -> Result<(T, LeaseContract), SessionError>
    where
        A: CloudApi,
        F: FnOnce(&'a AuthSession) -> CloudFuture<'a, T>,
        V: FnOnce(&T) -> bool,
    {
        let expected = self.lease;
        let expected_revision = self.revision;
        let mut lease = expected;
        let mut commit_race = false;
        let mut committed = Some(committed);
        let operation = operation(&self.auth);
        tokio::pin!(operation);
        let heartbeat_interval =
            Duration::from_millis((u64::from(expected.heartbeat_interval_ms) / 2).max(1));
        // Do not generate an extra heartbeat for every short request. The
        // first tick is deliberately half the server interval in the future;
        // subsequent ticks keep a slow operation from crossing the lease
        // expiry window.
        let mut heartbeat = tokio::time::interval_at(
            tokio::time::Instant::now() + heartbeat_interval,
            heartbeat_interval,
        );
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                result = &mut operation => {
                    let value = result?;
                    if commit_race
                        && !committed
                            .take()
                            .is_some_and(|is_committed| is_committed(&value))
                    {
                        return Err(SessionError::Lease);
                    }
                    return Ok((value, lease));
                },
                _ = heartbeat.tick(), if !commit_race => {
                    let request = HeartbeatLeaseRequest::new(lease.fence());
                    match api.heartbeat(&self.auth, request).await {
                        Ok(next) => {
                            next.validate().map_err(|_| SessionError::Lease)?;
                            if next.session_id != expected.session_id
                                || next.character_id != expected.character_id
                                || next.session_epoch != expected.session_epoch
                                || next.client_instance_id != expected.client_instance_id
                            {
                                return Err(SessionError::Lease);
                            }
                            if next.current_revision == expected_revision {
                                lease = next;
                            } else if allow_commit_race
                                && expected_revision.next().ok() == Some(next.current_revision)
                            {
                                // A mutating operation may have committed
                                // the next revision before its response was
                                // observed. Stop sending the old-fence
                                // heartbeat, but keep the idempotent
                                // operation alive to obtain proof.
                                commit_race = true;
                            } else {
                                return Err(SessionError::Lease);
                            }
                        }
                        Err(SessionError::Unauthorized) => {
                            return Err(SessionError::Unauthorized);
                        }
                        Err(SessionError::Lease) if allow_commit_race => {
                            // The generic API error does not reveal whether an
                            // old fence expired or a mutation just committed.
                            // Keep the idempotent operation alive, then accept
                            // it only if its typed response proves the exact
                            // next revision and identity. Otherwise the
                            // operation fails closed below.
                            commit_race = true;
                        }
                        Err(error) => return Err(error),
                    }
                }
            }
        }
    }

    fn install_heartbeat_lease(
        &mut self,
        lease: LeaseContract,
        expected_revision: Revision,
    ) -> Result<(), SessionError> {
        lease.validate().map_err(|_| SessionError::Lease)?;
        if lease.session_id != self.lease.session_id
            || lease.character_id != self.lease.character_id
            || lease.session_epoch != self.lease.session_epoch
            || lease.current_revision != expected_revision
            || lease.client_instance_id != self.lease.client_instance_id
        {
            return Err(SessionError::Lease);
        }
        self.lease = lease;
        self.auth.set_active_fence(self.lease.fence());
        Ok(())
    }

    /// Acquire a lease without inventing a revision or epoch, then verify or
    /// bootstrap exactly the server-returned current head.
    ///
    /// # Errors
    ///
    /// Returns an error when the lease, epoch, package, or workspace is invalid.
    pub async fn acquire<A: CloudApi>(
        api: &A,
        auth: AuthSession,
        config: SessionConfig,
    ) -> Result<Self, SessionError> {
        Self::acquire_inner(api, auth, config, None).await
    }

    /// Acquires a session with rotating-token support enabled.  Every
    /// authenticated cloud call made by the lifecycle will proactively refresh
    /// near access expiry and retry exactly once after a typed 401 response.
    ///
    /// # Errors
    ///
    /// Returns an error when authentication, lease fencing, package
    /// verification, or workspace materialization fails.
    pub async fn acquire_with_keychain<A: CloudApi>(
        api: &A,
        auth: AuthSession,
        config: SessionConfig,
        keychain: Arc<dyn RefreshTokenStore>,
    ) -> Result<Self, SessionError> {
        Self::acquire_inner(api, auth, config, Some(keychain)).await
    }

    async fn acquire_inner<A: CloudApi>(
        api: &A,
        mut auth: AuthSession,
        config: SessionConfig,
        keychain: Option<Arc<dyn RefreshTokenStore>>,
    ) -> Result<Self, SessionError> {
        let idempotency_key = random_idempotency_key()?;
        let request = AcquireLeaseRequest::new(
            auth.character_id,
            config.client_instance_id,
            idempotency_key,
        );
        refresh_if_needed(&mut auth, api, keychain.as_ref()).await?;
        let lease = match api.acquire(&auth, request).await {
            Err(SessionError::Unauthorized) => {
                refresh_required(&mut auth, api, keychain.as_ref()).await?;
                api.acquire(&auth, request).await?
            }
            Err(SessionError::Cloud) => {
                // The first response may have been lost after the server
                // committed the idempotent acquire. Replay the exact request
                // once; never mint a new operation key here.
                api.acquire(&auth, request).await?
            }
            result => result?,
        };
        if lease.validate().is_err() {
            release_best_effort(api, &mut auth, &lease, keychain.as_ref()).await;
            return Err(SessionError::Lease);
        }
        if lease.character_id != auth.character_id
            || lease.client_instance_id != config.client_instance_id
        {
            release_best_effort(api, &mut auth, &lease, keychain.as_ref()).await;
            return Err(SessionError::Lease);
        }
        if let Err(error) =
            config
                .epoch_store
                .accept(lease.character_id, lease.session_id, lease.session_epoch)
        {
            release_best_effort(api, &mut auth, &lease, keychain.as_ref()).await;
            return Err(error.into());
        }
        let workspace = match SessionWorkspace::create(&config.workspace_parent) {
            Ok(workspace) => workspace,
            Err(error) => {
                release_best_effort(api, &mut auth, &lease, keychain.as_ref()).await;
                return Err(error);
            }
        };
        let mut lifecycle = Self {
            revision: lease.current_revision,
            reconnect_key: random_idempotency_key()?,
            auth,
            lease,
            workspace,
            config,
            keychain,
            checkpoint_authorized: false,
            checkpoint_key: None,
            fresh_resume_save_digest: None,
        };
        lifecycle.auth.set_active_fence(lifecycle.lease.fence());
        if let Err(error) = lifecycle.restore_or_bootstrap(api).await {
            // Drop workspace before release, and never release while a child
            // process could still emit traffic (children are not started here).
            let _ = lifecycle.release(api).await;
            return Err(error);
        }
        Ok(lifecycle)
    }

    async fn restore_or_bootstrap<A: CloudApi>(&mut self, api: &A) -> Result<(), SessionError> {
        let character = self.lease.character_id;
        let revision = self.revision;
        let package = self.resume_package_retry(api, character, revision).await?;
        if self.revision.is_initial() {
            if package.is_some() {
                return Err(SessionError::InvalidBootstrap);
            }
            self.workspace.write_atomic("pending_commits.json", b"[]")?;
            self.copy_generated_addresses()?;
            return Ok(());
        }
        let Some(envelope) = package else {
            return Err(SessionError::MissingPackage);
        };
        let verified = self.fetch_verified_at(api, envelope, self.revision).await;
        match verified {
            Ok(package) => {
                self.heartbeat(api).await?;
                self.materialize_package(&package)?;
                self.heartbeat(api).await?;
                Ok(())
            }
            Err(SessionError::CorruptActiveSav) => self.recover_corrupt_active(api).await,
            Err(error) => Err(error),
        }
    }

    async fn fetch_verified_at<A: CloudApi>(
        &mut self,
        api: &A,
        envelope: SignedManifestEnvelope,
        revision: Revision,
    ) -> Result<VerifiedPackage, SessionError> {
        envelope
            .verify(&self.config.trusted_manifest_key)
            .map_err(|_| SessionError::Package)?;
        let manifest = &envelope.manifest;
        if manifest.character_id != self.lease.character_id
            || manifest.revision != revision
            || !has_expected_lineage(manifest, revision)
            || manifest.session_epoch.value() == 0
        {
            return Err(SessionError::Package);
        }
        let target = coop_cloud::CompatibilityTarget::new(
            self.config.manifest.target.game_build_id.clone(),
            self.config.manifest.target.rom_sha256,
            self.config.manifest.target.mgba_version.clone(),
            self.config.manifest.target.bridge_abi,
            self.config.manifest.target.protocol_version,
            revision,
        );
        if !target.matches(manifest) {
            return Err(SessionError::Package);
        }
        let character = self.lease.character_id;
        let sav = self
            .authenticated_artifact(api, character, ArtifactIdentity::CharacterSav, revision)
            .await?;
        let pending = self
            .authenticated_artifact(api, character, ArtifactIdentity::PendingCommits, revision)
            .await?;
        if sav.len() > MAX_SESSION_FILE_BYTES || pending.len() > MAX_SESSION_FILE_BYTES {
            return Err(SessionError::Package);
        }
        if Sha256Digest::of_bytes(&sav) != manifest.save_sha256 {
            return Err(SessionError::CorruptActiveSav);
        }
        if Sha256Digest::of_bytes(&pending) != manifest.pending_commits_sha256 {
            return Err(SessionError::Package);
        }
        let resume = if manifest.savestate_compatible {
            match self
                .authenticated_artifact(api, character, ArtifactIdentity::ResumeSs1, revision)
                .await
            {
                Ok(bytes)
                    if bytes.len() <= MAX_SESSION_FILE_BYTES
                        && manifest.savestate_sha256 == Some(Sha256Digest::of_bytes(&bytes)) =>
                {
                    Some(bytes)
                }
                // SAV has already been verified above.  An optional
                // savestate is an optimization, so an absent, oversized, or
                // transiently unavailable transport must never strand a
                // character that can safely resume from SAV.
                Ok(_) | Err(_) => None,
            }
        } else {
            None
        };
        match manifest.select_resume(&target, &sav, resume.as_deref()) {
            Ok(ResumeSelection::UseSavestate) => Ok(VerifiedPackage {
                manifest: manifest.clone(),
                sav,
                pending,
                resume,
            }),
            Ok(ResumeSelection::FallbackToSav(_)) => Ok(VerifiedPackage {
                manifest: manifest.clone(),
                sav,
                pending,
                resume: None,
            }),
            Err(_) => Err(SessionError::Package),
        }
    }

    fn materialize_package(&mut self, package: &VerifiedPackage) -> Result<(), SessionError> {
        if package.manifest.revision != self.revision {
            return Err(SessionError::Package);
        }
        // A downloaded savestate is useful for a manual resume, but it was
        // captured before this session's next SAVE_DATA_UPDATED event. It is
        // never eligible for a future upload until an operator/adapter marks
        // a fresh capture correlated to the current SAV.
        self.fresh_resume_save_digest = None;
        self.workspace.write_atomic("character.sav", &package.sav)?;
        self.workspace
            .write_atomic("pending_commits.json", &package.pending)?;
        if let Some(resume) = &package.resume {
            self.workspace.write_atomic("resume.input.ss1", resume)?;
        } else {
            self.retire_optional_resume()?;
        }
        self.copy_generated_addresses()
    }

    /// Marks an emulator-produced savestate as eligible for the next
    /// checkpoint. The save digest is captured at the same moment so a later
    /// SAV mutation automatically retires the state instead of uploading a
    /// stale pair.
    ///
    /// # Errors
    ///
    /// Returns an error when either fixed artifact is absent, malformed, or
    /// empty.
    pub fn mark_fresh_resume_capture(&mut self) -> Result<(), SessionError> {
        let sav = self.workspace.read_fixed("character.sav")?;
        if sav.is_empty() {
            return Err(SessionError::Package);
        }
        let resume = self.read_optional_resume()?.ok_or(SessionError::Package)?;
        if resume.is_empty() {
            return Err(SessionError::Package);
        }
        self.fresh_resume_save_digest = Some(Sha256Digest::of_bytes(&sav));
        Ok(())
    }

    /// Removes an optional savestate that is no longer correlated with the
    /// active SAV.  A stale savestate must never be re-uploaded as if it were
    /// produced by the next authorized checkpoint.
    fn retire_optional_resume(&self) -> Result<(), SessionError> {
        for name in ["resume.input.ss1", "resume.ss1"] {
            let path = fixed_path(self.workspace.path(), name)?;
            match std::fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                    return Err(SessionError::Package);
                }
                Ok(_) => std::fs::remove_file(path).map_err(SessionError::Filesystem)?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(SessionError::Filesystem(error)),
            }
        }
        Ok(())
    }

    fn copy_generated_addresses(&self) -> Result<(), SessionError> {
        reject_symlink_ancestors(&self.config.bridge_lua_dir).map_err(SessionError::Filesystem)?;
        #[cfg(windows)]
        let _bridge_guard =
            open_directory_guard(&self.config.bridge_lua_dir).map_err(SessionError::Filesystem)?;
        if !is_canonical_path(&self.config.bridge_lua_dir).map_err(SessionError::Filesystem)? {
            return Err(SessionError::Package);
        }
        let source = self.config.bridge_lua_dir.join("generated_addresses.lua");
        let metadata = std::fs::symlink_metadata(&source).map_err(SessionError::Filesystem)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > 1_048_576 {
            return Err(SessionError::Package);
        }
        let bytes = read_bounded_file(&source, 1_048_576)?;
        self.workspace
            .write_atomic("generated_addresses.lua", &bytes)?;
        Ok(())
    }

    async fn recover_corrupt_active<A: CloudApi>(&mut self, api: &A) -> Result<(), SessionError> {
        let request = SnapshotListRequest::new(
            self.lease.session_id,
            self.lease.character_id,
            self.lease.session_epoch,
            self.lease.client_instance_id,
            20,
        )
        .map_err(|_| SessionError::History)?;
        let response = self.list_snapshots_with_heartbeat(api, request).await?;
        response.validate().map_err(|_| SessionError::History)?;
        validate_history_records(&response.snapshots, self.revision, self.lease.character_id)?;
        for record in response.snapshots {
            let revision = record.revision;
            let character = self.lease.character_id;
            if let Some(envelope) = self.resume_package_retry(api, character, revision).await?
                && let Ok(package) = self.fetch_verified_at(api, envelope, revision).await
                && history_record_matches(&record, &package)
            {
                self.restore_and_materialize_historical(api, &record)
                    .await?;
                return Ok(());
            }
        }
        Err(SessionError::History)
    }

    async fn restore_and_materialize_historical<A: CloudApi>(
        &mut self,
        api: &A,
        record: &SnapshotRecord,
    ) -> Result<(), SessionError> {
        let expected_revision = self.revision;
        let restore = SnapshotRestoreRequest::new(
            record.snapshot_id,
            self.lease.session_id,
            self.lease.character_id,
            expected_revision,
            self.lease.session_epoch,
            self.lease.client_instance_id,
            random_idempotency_key()?,
        );
        let restored = self
            .restore_with_retry(api, restore, record.snapshot_id, expected_revision)
            .await?;
        restored.validate().map_err(|_| SessionError::History)?;
        if restored.snapshot.snapshot_id != record.snapshot_id
            || restored.snapshot.character_id != self.lease.character_id
            || restored.snapshot.session_id != self.lease.session_id
            || restored.snapshot.session_epoch != self.lease.session_epoch
            || restored.snapshot.parent_revision != expected_revision
            || restored.snapshot.revision.value() != expected_revision.value().saturating_add(1)
        {
            return Err(SessionError::History);
        }
        self.revision = restored.snapshot.revision;
        self.lease.current_revision = self.revision;
        self.auth.set_active_fence(self.lease.fence());
        let active = self
            .resume_package_retry(api, self.lease.character_id, self.revision)
            .await?
            .ok_or(SessionError::History)?;
        let mut active_package = self
            .fetch_verified_at(api, active, self.revision)
            .await
            .map_err(|_| SessionError::History)?;
        if !history_record_matches(&restored.snapshot, &active_package) {
            return Err(SessionError::History);
        }
        // The SAV is newly restored, so any historical optional savestate is
        // stale and must be retired before the next checkpoint.
        active_package.resume = None;
        self.heartbeat(api)
            .await
            .map_err(|_| SessionError::History)?;
        self.materialize_package(&active_package)?;
        self.retire_optional_resume()?;
        self.heartbeat(api)
            .await
            .map_err(|_| SessionError::History)?;
        Ok(())
    }

    async fn restore_with_retry<A: CloudApi>(
        &mut self,
        api: &A,
        request: SnapshotRestoreRequest,
        snapshot_id: SnapshotId,
        expected_revision: Revision,
    ) -> Result<SnapshotRestoreResponse, SessionError> {
        let expected_lease = self.lease;
        self.refresh_if_needed(api).await?;
        let first = self
            .run_with_mutating_heartbeats(
                api,
                |auth| api.restore(auth, request.clone()),
                move |response| {
                    restore_response_proves_commit(
                        response,
                        snapshot_id,
                        expected_lease,
                        expected_revision,
                    )
                },
            )
            .await;
        let (restored, lease) = match first {
            Ok(result) => result,
            Err(SessionError::Unauthorized) => {
                self.refresh_required(api).await?;
                self.run_with_mutating_heartbeats(
                    api,
                    |auth| api.restore(auth, request.clone()),
                    move |response| {
                        restore_response_proves_commit(
                            response,
                            snapshot_id,
                            expected_lease,
                            expected_revision,
                        )
                    },
                )
                .await?
            }
            Err(SessionError::Cloud | SessionError::Lease) => {
                self.run_with_mutating_heartbeats(
                    api,
                    |auth| api.restore(auth, request.clone()),
                    move |response| {
                        restore_response_proves_commit(
                            response,
                            snapshot_id,
                            expected_lease,
                            expected_revision,
                        )
                    },
                )
                .await?
            }
            Err(error) => return Err(error),
        };
        self.install_heartbeat_lease(lease, expected_revision)?;
        Ok(restored)
    }

    /// Renews the server-owned lease using the complete active fence.
    ///
    /// # Errors
    ///
    /// Returns an error when the response loses session, epoch, or revision fencing.
    pub async fn heartbeat<A: CloudApi>(&mut self, api: &A) -> Result<(), SessionError> {
        let request = HeartbeatLeaseRequest::new(self.lease.fence());
        self.refresh_if_needed(api).await?;
        let lease = match api.heartbeat(&self.auth, request).await {
            Err(SessionError::Unauthorized) => {
                self.refresh_required(api).await?;
                api.heartbeat(&self.auth, request).await?
            }
            result => result?,
        };
        lease.validate().map_err(|_| SessionError::Lease)?;
        if lease.session_id != self.lease.session_id
            || lease.character_id != self.lease.character_id
            || lease.session_epoch != self.lease.session_epoch
            || lease.current_revision != self.revision
            || lease.client_instance_id != self.lease.client_instance_id
        {
            return Err(SessionError::Lease);
        }
        self.lease = lease;
        self.auth.set_active_fence(self.lease.fence());
        Ok(())
    }

    /// Reconnects while retaining `SessionId` and accepting a newer server epoch.
    ///
    /// # Errors
    ///
    /// Returns an error when reconnect fencing or epoch persistence fails.
    pub async fn reconnect<A: CloudApi>(&mut self, api: &A) -> Result<(), SessionError> {
        // One idempotency key belongs to exactly one logical reconnect.  It
        // may be reused by a transport/auth retry below, but is rotated as
        // soon as that operation has completed so a later reconnect cannot be
        // mistaken for a replay of the earlier one.
        let reconnect_key = self.reconnect_key;
        let request = ReconnectLeaseRequest::new(self.lease.fence(), reconnect_key);
        self.refresh_if_needed(api).await?;
        let mut transport_retry = false;
        let lease = loop {
            match api.reconnect(&self.auth, request).await {
                Ok(lease) => break (lease, transport_retry),
                Err(SessionError::Unauthorized) => {
                    self.refresh_required(api).await?;
                    break (api.reconnect(&self.auth, request).await?, false);
                }
                Err(SessionError::Cloud) if !transport_retry => {
                    transport_retry = true;
                }
                Err(error) => return Err(error),
            }
        };
        let (lease, transport_replay) = lease;
        lease.validate().map_err(|_| SessionError::Lease)?;
        if lease.session_id != self.lease.session_id
            || lease.character_id != self.lease.character_id
            || lease.client_instance_id != self.lease.client_instance_id
            || lease.current_revision != self.revision
        {
            return Err(SessionError::Lease);
        }
        // The compare and (for a new epoch) persistence happen under one lock;
        // no concurrent reconnect can advance the record between validation
        // and acceptance of this response.
        if let Err(error) = self.config.epoch_store.accept_reconnect(
            lease.character_id,
            lease.session_id,
            self.lease.session_epoch,
            lease.session_epoch,
            transport_replay,
        ) {
            return Err(match error {
                EpochError::Stale | EpochError::IdentityMismatch => SessionError::Lease,
                other => SessionError::Epoch(other),
            });
        }
        self.lease = lease;
        self.auth.set_active_fence(self.lease.fence());
        self.reconnect_key = random_idempotency_key()?;
        Ok(())
    }

    /// Runs the online session until a child exits or a lease/checkpoint
    /// failure occurs.  Heartbeats are scheduled from the server-provided
    /// cadence; all authenticated requests pass through the proactive/401
    /// refresh policy.  A post-grant failure leaves
    /// `checkpoint_authorized` set so release can preserve a recovery SAV.
    ///
    /// # Errors
    ///
    /// Returns an error for heartbeat expiry, authentication failure,
    /// checkpoint correlation, or child/control failure.
    pub async fn run_until_exit<A: CloudApi>(
        &mut self,
        api: &A,
        children: &mut SupervisedChildren,
    ) -> Result<(), SessionError> {
        self.run_until_shutdown(api, children, std::future::pending())
            .await
    }

    /// Runs the fenced lifecycle until either a child exits, a lifecycle
    /// failure occurs, or the caller requests a graceful shutdown.  On a
    /// shutdown request, the control stream is drained for a short bounded
    /// interval and an already-authenticated ready checkpoint is completed
    /// while both children are still alive; no synthetic checkpoint is made.
    ///
    /// # Errors
    ///
    /// Returns an error for heartbeat, control, or safe-point failures. Any
    /// post-grant error stops and reaps children so [`Self::release`] can keep
    /// the recovery SAV.
    pub async fn run_until_shutdown<A, F>(
        &mut self,
        api: &A,
        children: &mut SupervisedChildren,
        shutdown: F,
    ) -> Result<(), SessionError>
    where
        A: CloudApi,
        F: std::future::Future<Output = ()> + Send,
    {
        let mut heartbeat = tokio::time::interval(self.heartbeat_interval());
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut shutdown = Box::pin(shutdown);
        let mut shutdown_requested = false;
        let result = loop {
            let step = tokio::select! {
                () = &mut shutdown => {
                    shutdown_requested = true;
                    break self.drain_shutdown_checkpoint(api, children).await;
                },
                _ = heartbeat.tick() => self.heartbeat(api).await,
                event = children.next_event() => match event.map_err(SessionError::Control) {
                    Ok(SupervisorEvent::ChildExited) => break Ok(()),
                    Ok(SupervisorEvent::Control(ready @ ControlEvent::CheckpointReady { .. })) => {
                        self.checkpoint_with_deadline(api, &mut children.control, ready)
                            .await
                            .map(|_| ())
                    }
                    Ok(SupervisorEvent::Control(_)) => Ok(()),
                    Err(error) => Err(error),
                },
            };
            if let Err(error) = step {
                break Err(error);
            }
        };
        if (shutdown_requested || result.is_err())
            && let Err(error) = children.stop_in_place().await
            && result.is_ok()
        {
            return Err(SessionError::Control(error));
        }
        result
    }

    async fn drain_shutdown_checkpoint<A: CloudApi>(
        &mut self,
        api: &A,
        children: &mut SupervisedChildren,
    ) -> Result<(), SessionError> {
        // A ready event already in the authenticated FIFO is safe to complete;
        // one absolute deadline prevents an immediately-ready untrusted stream
        // from starving child termination and reaping.
        let discovery_deadline = tokio::time::Instant::now() + SHUTDOWN_READY_DISCOVERY;
        loop {
            let next = tokio::time::timeout_at(discovery_deadline, children.next_event()).await;
            let Ok(next) = next else {
                return Ok(());
            };
            match next.map_err(SessionError::Control)? {
                SupervisorEvent::ChildExited => return Ok(()),
                SupervisorEvent::Control(ready @ ControlEvent::CheckpointReady { .. }) => {
                    return self
                        .checkpoint_with_deadline(api, &mut children.control, ready)
                        .await
                        .map(|_| ());
                }
                SupervisorEvent::Control(_) => {}
            }
        }
    }

    /// Compatibility alias for callers that model the launcher as a single
    /// long-lived `run` operation.
    ///
    /// # Errors
    ///
    /// Returns an error when the supervised children, control channel,
    /// heartbeat, or checkpoint lifecycle fails.
    pub async fn run<A: CloudApi>(
        &mut self,
        api: &A,
        children: &mut SupervisedChildren,
    ) -> Result<(), SessionError> {
        self.run_until_exit(api, children).await
    }

    /// Complete one checkpoint after the sidecar has emitted an authenticated ready event.
    /// Completes a correlated ready/grant/save-updated checkpoint and CAS finalize.
    ///
    /// # Errors
    ///
    /// Returns an error when control correlation, artifact verification, upload,
    /// or server finalization fails.
    pub async fn checkpoint<A: CloudApi>(
        &mut self,
        api: &A,
        control: &mut ControlChannel,
        ready: ControlEvent,
    ) -> Result<Revision, SessionError> {
        self.checkpoint_with_deadline(api, control, ready).await
    }

    async fn checkpoint_until<A: CloudApi>(
        &mut self,
        api: &A,
        control: &mut ControlChannel,
        ready: ControlEvent,
        deadline: tokio::time::Instant,
    ) -> Result<Revision, SessionError> {
        let ControlEvent::CheckpointReady {
            session_epoch,
            ready_sequence,
        } = ready
        else {
            return Err(SessionError::CheckpointCorrelation);
        };
        if session_epoch != self.lease.session_epoch.value() || ready_sequence == 0 {
            return Err(SessionError::CheckpointCorrelation);
        }
        if self.lease.expires_at.value() <= now_millis()
            || self.checkpoint_key == Some((session_epoch, ready_sequence))
        {
            return Err(SessionError::CheckpointNotAuthorized);
        }
        self.refresh_if_needed(api).await?;
        if self.lease.expires_at.value() <= now_millis() {
            return Err(SessionError::CheckpointNotAuthorized);
        }
        let command_id = new_command_id();
        // Once a grant is attempted, the transport outcome is ambiguous: the
        // sidecar may have applied it even if the write or response is lost.
        // Mark this before sending so every such path preserves recovery.
        self.checkpoint_authorized = true;
        self.checkpoint_key = Some((session_epoch, ready_sequence));
        tokio::time::timeout_at(
            deadline,
            control.send(&ControlCommand::CheckpointGrant(CheckpointGrant {
                command_id,
                session_epoch,
                ready_sequence,
            })),
        )
        .await
        .map_err(|_| SessionError::CheckpointTimeout)??;
        match self
            .receive_checkpoint_event(api, control, deadline)
            .await?
        {
            ControlEvent::CommandResult {
                command_id: echoed,
                status: CommandStatus::Applied | CommandStatus::Replayed,
                ..
            } if echoed == command_id => {}
            ControlEvent::CommandResult {
                command_id: echoed,
                status: CommandStatus::Rejected | CommandStatus::Conflict,
                ..
            } if echoed == command_id => {
                // A matching explicit rejection proves that no grant was
                // applied, so recovery preservation is no longer required.
                self.checkpoint_authorized = false;
                self.checkpoint_key = None;
                return Err(SessionError::CheckpointNotAuthorized);
            }
            _ => return Err(SessionError::CheckpointNotAuthorized),
        }
        let updated = self
            .receive_checkpoint_event(api, control, deadline)
            .await?;
        let ControlEvent::SaveDataUpdated {
            session_epoch: updated_epoch,
            ready_sequence: updated_ready,
            ..
        } = updated
        else {
            return Err(SessionError::CheckpointCorrelation);
        };
        if updated_epoch != session_epoch || updated_ready != ready_sequence {
            return Err(SessionError::CheckpointCorrelation);
        }
        match self.checkpoint_files(api).await {
            Ok(revision) => {
                self.checkpoint_authorized = false;
                Ok(revision)
            }
            Err(error) => Err(error),
        }
    }

    async fn checkpoint_with_deadline<A: CloudApi>(
        &mut self,
        api: &A,
        control: &mut ControlChannel,
        ready: ControlEvent,
    ) -> Result<Revision, SessionError> {
        let deadline = tokio::time::Instant::now() + CHECKPOINT_PROTOCOL_DEADLINE;
        tokio::time::timeout_at(
            deadline,
            self.checkpoint_until(api, control, ready, deadline),
        )
        .await
        .map_err(|_| SessionError::CheckpointTimeout)?
    }

    /// Receives checkpoint responses without abandoning the lease while the
    /// sidecar is slow. `receive_bounded` preserves its persistent framing
    /// buffer when this select is cancelled for a heartbeat tick.
    async fn receive_checkpoint_event<A: CloudApi>(
        &mut self,
        api: &A,
        control: &mut ControlChannel,
        deadline: tokio::time::Instant,
    ) -> Result<ControlEvent, SessionError> {
        let interval =
            Duration::from_millis((u64::from(self.lease.heartbeat_interval_ms) / 2).max(1));
        let mut heartbeat =
            tokio::time::interval_at(tokio::time::Instant::now() + interval, interval);
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                event = tokio::time::timeout_at(deadline, control.receive_bounded()) => {
                    match event {
                        Ok(Ok(event)) => return Ok(event),
                        Ok(Err(ProcessError::Control(error)))
                            if error.kind() == io::ErrorKind::TimedOut => {}
                        Ok(Err(error)) => return Err(SessionError::Control(error)),
                        Err(_) => return Err(SessionError::CheckpointTimeout),
                    }
                }
                _ = heartbeat.tick() => self.heartbeat(api).await?,
            }
        }
    }

    async fn checkpoint_files<A: CloudApi>(&mut self, api: &A) -> Result<Revision, SessionError> {
        let sav = self.workspace.read_fixed("character.sav")?;
        if sav.is_empty() {
            return Err(SessionError::Package);
        }
        // Establish a secret-free recovery copy before reading any other
        // artifact or contacting the cloud. Later scrub/upload/finalize
        // failures can retain this shadow without re-reading a compromised
        // workspace.
        self.workspace.create_recovery_shadow(&sav)?;
        let pending = self.workspace.read_fixed("pending_commits.json")?;
        if self.revision.is_initial() && pending != b"[]" {
            return Err(SessionError::CheckpointCorrelation);
        }
        let mut files = vec![
            SnapshotFile::from_bytes(ArtifactIdentity::CharacterSav, &sav)
                .map_err(|_| SessionError::Package)?,
            SnapshotFile::from_bytes(ArtifactIdentity::PendingCommits, &pending)
                .map_err(|_| SessionError::Package)?,
        ];
        let resume = match self.fresh_resume_save_digest {
            Some(expected) if expected == Sha256Digest::of_bytes(&sav) => {
                self.read_optional_resume()?
            }
            _ => {
                // A resume package downloaded during startup (or left by a
                // prior revision) is not a fresh capture for this SAV. Keep
                // the canonical SAV checkpoint and retire the stale state.
                self.retire_optional_resume()?;
                self.fresh_resume_save_digest = None;
                None
            }
        };
        if let Some(resume) = &resume {
            files.push(
                SnapshotFile::from_bytes(ArtifactIdentity::ResumeSs1, resume)
                    .map_err(|_| SessionError::Package)?,
            );
        }
        let snapshot_id =
            SnapshotId::new(uuid::Uuid::new_v4()).map_err(|_| SessionError::Package)?;
        let idem = random_idempotency_key()?;
        let fence = SnapshotPrepareFence::new(
            self.lease.session_id,
            self.lease.character_id,
            self.revision,
            self.lease.session_epoch,
            self.lease.client_instance_id,
            idem,
        );
        let pending_digest = Sha256Digest::of_bytes(&pending);
        let request =
            PrepareSnapshotRequest::new(snapshot_id, fence, files.clone(), pending_digest)
                .map_err(|_| SessionError::Package)?;
        let response = self.prepare_with_retry(api, request.clone()).await?;
        if !response.matches_request(&request) {
            return Err(SessionError::CheckpointCorrelation);
        }
        for target in &response.upload_targets {
            let bytes = match target.artifact {
                ArtifactIdentity::CharacterSav => sav.clone(),
                ArtifactIdentity::PendingCommits => pending.clone(),
                ArtifactIdentity::ResumeSs1 => resume.clone().ok_or(SessionError::Package)?,
            };
            self.upload_with_retry(api, target, bytes).await?;
        }
        let finalize_fence = SnapshotFinalizeFence::new(
            self.lease.session_id,
            self.lease.character_id,
            self.revision,
            self.lease.session_epoch,
            self.lease.client_instance_id,
            idem,
        );
        let finalize = SnapshotFinalizeRequest::new(
            snapshot_id,
            finalize_fence,
            files.clone(),
            pending_digest,
            None,
        )
        .map_err(|_| SessionError::Package)?;
        let record = self.finalize_with_retry(api, finalize).await?;
        if record.validate().is_err()
            || record.snapshot_id != snapshot_id
            || record.session_id != self.lease.session_id
            || record.character_id != self.lease.character_id
            || record.parent_revision != self.revision
            || record.revision != response.next_revision
            || record.session_epoch != self.lease.session_epoch
            || record.files != files
            || record.pending_commits_sha256 != pending_digest
        {
            return Err(SessionError::FinalizeConflict);
        }
        self.revision = record.revision;
        self.lease.current_revision = record.revision;
        self.auth.set_active_fence(self.lease.fence());
        self.workspace.discard_recovery_shadow();
        Ok(record.revision)
    }

    fn read_optional_resume(&self) -> Result<Option<Vec<u8>>, SessionError> {
        let path = self.workspace.path().join("resume.ss1");
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                Err(SessionError::Package)
            }
            Ok(_) => self.workspace.read_fixed("resume.ss1").map(Some),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(SessionError::Filesystem(error)),
        }
    }

    async fn prepare_with_retry<A: CloudApi>(
        &mut self,
        api: &A,
        request: PrepareSnapshotRequest,
    ) -> Result<SnapshotPrepareResponse, SessionError> {
        self.refresh_if_needed(api).await?;
        let first = self
            .run_with_heartbeats(api, |auth| api.prepare(auth, request.clone()))
            .await;
        match first {
            Ok((response, lease)) => {
                self.install_heartbeat_lease(lease, self.revision)?;
                Ok(response)
            }
            Err(SessionError::Unauthorized) => {
                self.refresh_required(api).await?;
                let (response, lease) = self
                    .run_with_heartbeats(api, |auth| api.prepare(auth, request.clone()))
                    .await?;
                self.install_heartbeat_lease(lease, self.revision)?;
                Ok(response)
            }
            Err(SessionError::Cloud) => {
                let (response, lease) = self
                    .run_with_heartbeats(api, |auth| api.prepare(auth, request.clone()))
                    .await?;
                self.install_heartbeat_lease(lease, self.revision)?;
                Ok(response)
            }
            Err(error) => Err(error),
        }
    }

    async fn finalize_with_retry<A: CloudApi>(
        &mut self,
        api: &A,
        request: SnapshotFinalizeRequest,
    ) -> Result<SnapshotRecord, SessionError> {
        self.refresh_if_needed(api).await?;
        let first = self
            .run_with_mutating_heartbeats(
                api,
                |auth| api.finalize(auth, request.clone()),
                |record| finalize_response_proves_commit(record, &request),
            )
            .await;
        match first {
            Ok((record, lease)) => {
                self.install_heartbeat_lease(lease, self.revision)?;
                Ok(record)
            }
            Err(SessionError::Unauthorized) => {
                self.refresh_required(api).await?;
                let (record, lease) = self
                    .run_with_mutating_heartbeats(
                        api,
                        |auth| api.finalize(auth, request.clone()),
                        |record| finalize_response_proves_commit(record, &request),
                    )
                    .await?;
                self.install_heartbeat_lease(lease, self.revision)?;
                Ok(record)
            }
            Err(SessionError::Cloud | SessionError::Lease) => {
                let (record, lease) = self
                    .run_with_mutating_heartbeats(
                        api,
                        |auth| api.finalize(auth, request.clone()),
                        |record| finalize_response_proves_commit(record, &request),
                    )
                    .await?;
                self.install_heartbeat_lease(lease, self.revision)?;
                Ok(record)
            }
            Err(error) => Err(error),
        }
    }

    async fn upload_with_retry<A: CloudApi>(
        &mut self,
        api: &A,
        target: &UploadTarget,
        bytes: Vec<u8>,
    ) -> Result<(), SessionError> {
        self.refresh_if_needed(api).await?;
        let first = self
            .run_with_heartbeats(api, |_auth| api.upload(target, bytes.clone()))
            .await;
        match first {
            Ok(((), lease)) => {
                self.install_heartbeat_lease(lease, self.revision)?;
                Ok(())
            }
            Err(SessionError::Unauthorized) => Err(SessionError::Unauthorized),
            Err(SessionError::Cloud) => {
                let ((), lease) = self
                    .run_with_heartbeats(api, |_auth| api.upload(target, bytes.clone()))
                    .await?;
                self.install_heartbeat_lease(lease, self.revision)?;
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    async fn logout_credentials<A: AuthApi>(&mut self, api: &A) -> Result<(), SessionError> {
        let Some(keychain) = self.keychain.take() else {
            return Ok(());
        };
        self.auth
            .logout(api, keychain.as_ref())
            .await
            .map_err(SessionError::Auth)
    }

    /// Preserves an authorized checkpoint when child supervision cannot prove
    /// that both processes were reaped.  The caller must not release the
    /// remote lease in that case: a still-live child could emit fenced
    /// traffic.  Recovery scrubbing remains fail-closed and never detaches a
    /// workspace containing an unverified loopback secret.
    ///
    /// # Errors
    ///
    /// Returns the scrub error when recovery preservation is incomplete.
    pub fn preserve_recovery_after_child_failure(&mut self) -> Result<(), SessionError> {
        if !self.checkpoint_authorized {
            return Ok(());
        }
        match self.workspace.preserve_recovery() {
            Ok(_) => Ok(()),
            Err(error) => {
                self.workspace.retain_private_on_scrub_failure();
                Err(error)
            }
        }
    }

    /// Releases the exact active lease fence and revokes the rotating auth
    /// family.  Recovery is scrubbed before either remote mutation, and the
    /// keychain logout is attempted even when lease release fails.
    ///
    /// # Errors
    ///
    /// Returns an error when the server cannot accept the release.
    pub async fn release<A: CloudApi>(mut self, api: &A) -> Result<(), SessionError> {
        if self.checkpoint_authorized
            && let Err(error) = self.workspace.preserve_recovery()
        {
            // Prefer the pre-created, already secret-free shadow. This path
            // performs no scrub-time read/copy and therefore remains durable
            // even when the original workspace was only partially scrubbed.
            if let Some(shadow) = self.workspace.recovery_shadow.take() {
                shadow.keep();
                #[cfg(windows)]
                let _ = std::mem::take(&mut self.workspace.ancestor_guards);
                let _ = self.workspace.temp.take();
                let _ = self.logout_credentials(api).await;
                return Err(error);
            }
            self.workspace.retain_private_on_scrub_failure();
            let _ = self.logout_credentials(api).await;
            return Err(error);
        }
        let release_result = match random_idempotency_key() {
            Err(error) => Err(error),
            Ok(idempotency_key) => {
                let request = ReleaseLeaseRequest::new(self.lease.fence(), idempotency_key);
                match self.refresh_if_needed(api).await {
                    Err(error) => Err(error),
                    Ok(()) => match api.release(&self.auth, request).await {
                        Err(SessionError::Unauthorized) => match self.refresh_required(api).await {
                            Err(error) => Err(error),
                            Ok(()) => api.release(&self.auth, request).await.map(|_| ()),
                        },
                        Err(SessionError::Cloud) => {
                            // The release may have committed after the
                            // transport was lost. Replay the same request and
                            // idempotency key exactly once.
                            api.release(&self.auth, request).await.map(|_| ())
                        }
                        Err(error) => Err(error),
                        Ok(_) => Ok(()),
                    },
                }
            }
        };
        let logout_result = self.logout_credentials(api).await;
        match release_result {
            Err(error) => Err(error),
            Ok(()) => logout_result,
        }
    }
}

fn validate_history_records(
    records: &[SnapshotRecord],
    current_revision: Revision,
    character_id: CharacterId,
) -> Result<(), SessionError> {
    if records.len() > 20 {
        return Err(SessionError::History);
    }
    // Recovery candidates must be strictly older than the corrupt active
    // revision and strictly decreasing thereafter. A same-revision record is
    // not historical, even if its bytes happen to validate.
    let mut last = current_revision.value();
    for record in records {
        if record.validate().is_err()
            || record.character_id != character_id
            || record.session_epoch.value() == 0
            || record.revision.value() == 0
            || record.revision.value() >= last
        {
            return Err(SessionError::History);
        }
        last = record.revision.value();
    }
    Ok(())
}

fn history_record_matches(record: &SnapshotRecord, package: &VerifiedPackage) -> bool {
    // Historical epochs and session IDs are provenance, not the current lease
    // fence. The detached signature and both mandatory artifact digests still
    // have to identify this exact character snapshot.
    record.session_epoch == package.manifest.session_epoch
        && record.revision == package.manifest.revision
        && record.snapshot_id == package.manifest.snapshot_id
        && record.parent_revision == package.manifest.parent_revision
        && record.pending_commits_sha256 == package.manifest.pending_commits_sha256
        && record.files.iter().any(|file| {
            file.artifact == ArtifactIdentity::CharacterSav
                && file.sha256 == package.manifest.save_sha256
        })
        && record.files.iter().any(|file| {
            file.artifact == ArtifactIdentity::PendingCommits
                && file.sha256 == package.manifest.pending_commits_sha256
        })
}

fn restore_response_proves_commit(
    response: &SnapshotRestoreResponse,
    snapshot_id: SnapshotId,
    lease: LeaseContract,
    expected_revision: Revision,
) -> bool {
    response.validate().is_ok()
        && response.snapshot.snapshot_id == snapshot_id
        && response.snapshot.session_id == lease.session_id
        && response.snapshot.character_id == lease.character_id
        && response.snapshot.session_epoch == lease.session_epoch
        && response.snapshot.parent_revision == expected_revision
        && expected_revision
            .next()
            .ok()
            .is_some_and(|next| response.snapshot.revision == next)
}

fn finalize_response_proves_commit(
    response: &SnapshotRecord,
    request: &SnapshotFinalizeRequest,
) -> bool {
    response.validate().is_ok()
        && response.snapshot_id == request.snapshot_id
        && response.session_id == request.session_id
        && response.character_id == request.character_id
        && response.session_epoch == request.session_epoch
        && response.parent_revision == request.expected_parent_revision
        && response.revision == request.revision
        && response.files == request.files
        && response.pending_commits_sha256 == request.pending_commits_sha256
}

fn has_expected_lineage(manifest: &ResumePackageManifest, revision: Revision) -> bool {
    manifest.parent_revision.next().ok() == Some(revision)
}

#[derive(Clone, Debug)]
struct VerifiedPackage {
    manifest: ResumePackageManifest,
    sav: Vec<u8>,
    pending: Vec<u8>,
    resume: Option<Vec<u8>>,
}

async fn refresh_if_needed<A: AuthApi>(
    auth: &mut AuthSession,
    api: &A,
    keychain: Option<&Arc<dyn RefreshTokenStore>>,
) -> Result<(), SessionError> {
    if auth.should_refresh_at(now_millis()) {
        refresh_required(auth, api, keychain).await?;
    }
    Ok(())
}

async fn refresh_required<A: AuthApi>(
    auth: &mut AuthSession,
    api: &A,
    keychain: Option<&Arc<dyn RefreshTokenStore>>,
) -> Result<(), SessionError> {
    let store = keychain.ok_or(SessionError::Auth(AuthError::Keychain(
        KeychainError::Unavailable,
    )))?;
    auth.refresh(api, store.as_ref()).await?;
    Ok(())
}

fn now_millis() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .unwrap_or(u64::MAX)
}

fn random_idempotency_key() -> Result<IdempotencyKey, SessionError> {
    IdempotencyKey::new(uuid::Uuid::new_v4()).map_err(|_| SessionError::Cloud)
}

async fn release_best_effort<A: CloudApi>(
    api: &A,
    auth: &mut AuthSession,
    lease: &LeaseContract,
    keychain: Option<&Arc<dyn RefreshTokenStore>>,
) {
    if let Ok(idempotency_key) = IdempotencyKey::new(uuid::Uuid::new_v4()) {
        if refresh_if_needed(auth, api, keychain).await.is_err() {
            return;
        }
        let request = ReleaseLeaseRequest::new(lease.fence(), idempotency_key);
        match api.release(auth, request).await {
            Err(SessionError::Unauthorized) => {
                if refresh_required(auth, api, keychain).await.is_ok() {
                    let _ = api.release(auth, request).await;
                }
            }
            Err(SessionError::Cloud) => {
                // Preserve the cleanup request identity when the first
                // response is ambiguous; the server owns idempotency.
                let _ = api.release(auth, request).await;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    use coop_cloud::{
        AccessToken, ApiVersion, ArtifactIdentity, BridgeAbiVersion, CharacterId, ClientInstanceId,
        CompatibilityTarget, GameBuildId, HeartbeatLeaseRequest, LeaseContract, LeaseFence,
        LoginRequest, LoginResponse, LogoutRequest, LogoutResponse, MgbaVersion, Password,
        PrepareSnapshotRequest, ProtocolVersion, ReconnectLeaseRequest, RefreshFamilyId,
        RefreshRequest, RefreshResponse, RefreshToken, ReleaseLeaseRequest, ResumePackageManifest,
        Revision, SessionEpoch, SessionId, Sha256Digest, SnapshotFence, SnapshotFinalizeRequest,
        SnapshotListRequest, SnapshotListResponse, SnapshotPrepareResponse, SnapshotRecord,
        SnapshotRestoreRequest, SnapshotRestoreResponse, TrustedManifestKey, UnixTimestampMillis,
        UploadTarget, UserId,
    };
    use serde_json::json;
    use tempfile::{TempDir, tempdir};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        process::Command,
        time::timeout,
    };
    use uuid::Uuid;

    use super::CloudFuture;
    #[cfg(windows)]
    use crate::SessionWorkspace;
    use crate::{
        AuthApi, AuthError, AuthSession, BuildCompatibility, CloudApi, ControlChannel, EpochStore,
        KeychainError, RefreshTokenStore, SessionConfig, SessionError, SessionLifecycle,
        SupervisedChildren, auth::AuthFuture,
    };
    use coop_sidecar::control::{CommandStatus, ControlCommand, ControlEvent};

    #[derive(Default)]
    struct TestKeychain {
        token: Mutex<Option<RefreshToken>>,
    }

    impl RefreshTokenStore for TestKeychain {
        fn load(
            &self,
            _service: &str,
            _username: &str,
        ) -> Result<Option<RefreshToken>, KeychainError> {
            Ok(self.token.lock().unwrap().clone())
        }

        fn store(
            &self,
            _service: &str,
            _username: &str,
            token: &RefreshToken,
        ) -> Result<(), KeychainError> {
            *self.token.lock().unwrap() = Some(token.clone());
            Ok(())
        }

        fn delete(&self, _service: &str, _username: &str) -> Result<(), KeychainError> {
            *self.token.lock().unwrap() = None;
            Ok(())
        }
    }

    struct TestCloud {
        lease: LeaseContract,
        login: LoginResponse,
        fail_prepare: bool,
        reconnect_epoch: Mutex<Option<u32>>,
        reconnect_transport_once: Mutex<bool>,
        reconnect_requests: Mutex<Vec<ReconnectLeaseRequest>>,
        heartbeats: Mutex<usize>,
        heartbeat_requests: Mutex<Vec<HeartbeatLeaseRequest>>,
        heartbeat_unauthorized_once: Mutex<bool>,
        heartbeat_revision: Mutex<Option<Revision>>,
        heartbeat_character: Mutex<Option<CharacterId>>,
        refresh_enabled: Mutex<bool>,
        artifact_bytes: Mutex<Option<Vec<u8>>>,
        artifact_delay: Mutex<Duration>,
        finalize_delay: Mutex<Duration>,
        prepares: Mutex<usize>,
        uploads: Mutex<Vec<(coop_cloud::ArtifactIdentity, Vec<u8>)>>,
        finalizes: Mutex<usize>,
        releases: Mutex<usize>,
        release_requests: Mutex<Vec<ReleaseLeaseRequest>>,
        logouts: Mutex<usize>,
    }

    impl TestCloud {
        fn new(lease: LeaseContract, login: LoginResponse, fail_prepare: bool) -> Self {
            Self {
                lease,
                login,
                fail_prepare,
                reconnect_epoch: Mutex::new(None),
                reconnect_transport_once: Mutex::new(false),
                reconnect_requests: Mutex::new(Vec::new()),
                heartbeats: Mutex::new(0),
                heartbeat_requests: Mutex::new(Vec::new()),
                heartbeat_unauthorized_once: Mutex::new(false),
                heartbeat_revision: Mutex::new(None),
                heartbeat_character: Mutex::new(None),
                refresh_enabled: Mutex::new(false),
                artifact_bytes: Mutex::new(None),
                artifact_delay: Mutex::new(Duration::ZERO),
                finalize_delay: Mutex::new(Duration::ZERO),
                prepares: Mutex::new(0),
                uploads: Mutex::new(Vec::new()),
                finalizes: Mutex::new(0),
                releases: Mutex::new(0),
                release_requests: Mutex::new(Vec::new()),
                logouts: Mutex::new(0),
            }
        }

        fn set_reconnect_epoch(&self, epoch: u32) {
            *self.reconnect_epoch.lock().unwrap() = Some(epoch);
        }

        fn set_reconnect_transport_once(&self) {
            *self.reconnect_transport_once.lock().unwrap() = true;
        }

        fn set_heartbeat_unauthorized_once(&self) {
            *self.heartbeat_unauthorized_once.lock().unwrap() = true;
        }

        fn set_heartbeat_revision(&self, revision: Revision) {
            *self.heartbeat_revision.lock().unwrap() = Some(revision);
        }

        fn set_heartbeat_character(&self, character: CharacterId) {
            *self.heartbeat_character.lock().unwrap() = Some(character);
        }

        fn set_delayed_artifact(&self, bytes: Vec<u8>, delay: Duration) {
            *self.artifact_bytes.lock().unwrap() = Some(bytes);
            *self.artifact_delay.lock().unwrap() = delay;
        }

        fn set_finalize_delay(&self, delay: Duration) {
            *self.finalize_delay.lock().unwrap() = delay;
        }

        fn enable_refresh(&self) {
            *self.refresh_enabled.lock().unwrap() = true;
        }
    }

    impl AuthApi for TestCloud {
        fn login(&self, _request: LoginRequest) -> AuthFuture<'_, LoginResponse> {
            let response = self.login.clone();
            Box::pin(async move { Ok(response) })
        }

        fn refresh(&self, _request: RefreshRequest) -> AuthFuture<'_, RefreshResponse> {
            if !*self.refresh_enabled.lock().unwrap() {
                return Box::pin(async { Err(AuthError::Transport) });
            }
            let response = RefreshResponse::new(
                AccessToken::new("refreshed-access").unwrap(),
                RefreshToken::new("refreshed-refresh").unwrap(),
                self.login.refresh_family_id,
                self.login.access_expires_at,
                self.login.refresh_expires_at,
            )
            .unwrap();
            Box::pin(async move { Ok(response) })
        }

        fn logout(&self, _request: LogoutRequest) -> AuthFuture<'_, LogoutResponse> {
            *self.logouts.lock().unwrap() += 1;
            Box::pin(async { Ok(LogoutResponse::default()) })
        }
    }

    impl CloudApi for TestCloud {
        fn acquire<'a>(
            &'a self,
            _auth: &'a crate::AuthSession,
            _request: coop_cloud::AcquireLeaseRequest,
        ) -> CloudFuture<'a, LeaseContract> {
            let lease = self.lease;
            Box::pin(async move { Ok(lease) })
        }

        fn heartbeat<'a>(
            &'a self,
            _auth: &'a crate::AuthSession,
            request: HeartbeatLeaseRequest,
        ) -> CloudFuture<'a, LeaseContract> {
            *self.heartbeats.lock().unwrap() += 1;
            self.heartbeat_requests.lock().unwrap().push(request);
            if *self.heartbeat_unauthorized_once.lock().unwrap() {
                *self.heartbeat_unauthorized_once.lock().unwrap() = false;
                return Box::pin(async { Err(SessionError::Unauthorized) });
            }
            let mut lease = self.lease;
            if let Some(revision) = *self.heartbeat_revision.lock().unwrap() {
                lease.current_revision = revision;
            }
            if let Some(character) = *self.heartbeat_character.lock().unwrap() {
                lease.character_id = character;
            }
            Box::pin(async move { Ok(lease) })
        }

        fn reconnect<'a>(
            &'a self,
            _auth: &'a crate::AuthSession,
            request: ReconnectLeaseRequest,
        ) -> CloudFuture<'a, LeaseContract> {
            self.reconnect_requests.lock().unwrap().push(request);
            if *self.reconnect_transport_once.lock().unwrap() {
                *self.reconnect_transport_once.lock().unwrap() = false;
                return Box::pin(async { Err(SessionError::Cloud) });
            }
            let lease = if let Some(epoch) = *self.reconnect_epoch.lock().unwrap() {
                LeaseContract::new(
                    LeaseFence::new(
                        self.lease.session_id,
                        self.lease.character_id,
                        self.lease.current_revision,
                        SessionEpoch::new(epoch).unwrap(),
                        self.lease.client_instance_id,
                    ),
                    self.lease.expires_at,
                    self.lease.heartbeat_interval_ms,
                )
                .unwrap()
            } else {
                self.lease
            };
            Box::pin(async move { Ok(lease) })
        }

        fn release<'a>(
            &'a self,
            _auth: &'a crate::AuthSession,
            request: ReleaseLeaseRequest,
        ) -> CloudFuture<'a, LogoutResponse> {
            *self.releases.lock().unwrap() += 1;
            self.release_requests.lock().unwrap().push(request);
            Box::pin(async { Ok(LogoutResponse::default()) })
        }

        fn resume_package<'a>(
            &'a self,
            _auth: &'a crate::AuthSession,
            _character: CharacterId,
            _revision: Revision,
        ) -> CloudFuture<'a, Option<coop_cloud::SignedManifestEnvelope>> {
            Box::pin(async { Ok(None) })
        }

        fn artifact<'a>(
            &'a self,
            _auth: &'a crate::AuthSession,
            _character: CharacterId,
            _artifact: coop_cloud::ArtifactIdentity,
            _revision: Revision,
        ) -> CloudFuture<'a, Vec<u8>> {
            let bytes = self.artifact_bytes.lock().unwrap().clone();
            let delay = *self.artifact_delay.lock().unwrap();
            Box::pin(async move {
                tokio::time::sleep(delay).await;
                bytes.ok_or(SessionError::ArtifactNotFound)
            })
        }

        fn list_snapshots<'a>(
            &'a self,
            _auth: &'a crate::AuthSession,
            _request: SnapshotListRequest,
        ) -> CloudFuture<'a, SnapshotListResponse> {
            Box::pin(async { Err(SessionError::Cloud) })
        }

        fn restore<'a>(
            &'a self,
            _auth: &'a crate::AuthSession,
            _request: SnapshotRestoreRequest,
        ) -> CloudFuture<'a, SnapshotRestoreResponse> {
            Box::pin(async { Err(SessionError::Cloud) })
        }

        fn prepare<'a>(
            &'a self,
            _auth: &'a crate::AuthSession,
            request: PrepareSnapshotRequest,
        ) -> CloudFuture<'a, SnapshotPrepareResponse> {
            *self.prepares.lock().unwrap() += 1;
            if self.fail_prepare {
                return Box::pin(async { Err(SessionError::Cloud) });
            }
            let files = request.files.clone();
            let targets = files
                .iter()
                .map(|file| {
                    UploadTarget::new_put(
                        file.artifact,
                        "http://127.0.0.1:9/upload?capability=test",
                        UnixTimestampMillis::new(4_000_000_000_000),
                    )
                    .unwrap()
                })
                .collect();
            let response = SnapshotPrepareResponse {
                api_version: ApiVersion::V1,
                snapshot_id: request.snapshot_id,
                expected_parent_revision: request.expected_parent_revision,
                next_revision: request.expected_parent_revision.next().unwrap(),
                session_epoch: request.session_epoch,
                idempotency_key: request.idempotency_key,
                files,
                pending_commits_sha256: request.pending_commits_sha256,
                upload_targets: targets,
            };
            Box::pin(async move { Ok(response) })
        }

        fn upload<'a>(&'a self, target: &'a UploadTarget, bytes: Vec<u8>) -> CloudFuture<'a, ()> {
            let artifact = target.artifact;
            self.uploads.lock().unwrap().push((artifact, bytes));
            Box::pin(async { Ok(()) })
        }

        fn finalize<'a>(
            &'a self,
            _auth: &'a crate::AuthSession,
            request: SnapshotFinalizeRequest,
        ) -> CloudFuture<'a, coop_cloud::SnapshotRecord> {
            *self.finalizes.lock().unwrap() += 1;
            let delay = *self.finalize_delay.lock().unwrap();
            let record = coop_cloud::SnapshotRecord::new(
                request.snapshot_id,
                SnapshotFence::new(
                    request.session_id,
                    request.character_id,
                    request.session_epoch,
                ),
                request.expected_parent_revision,
                request.revision,
                request.files,
                request.pending_commits_sha256,
                request.last_applied_commit,
                UnixTimestampMillis::new(4_000_000_000_000),
            )
            .unwrap();
            Box::pin(async move {
                tokio::time::sleep(delay).await;
                Ok(record)
            })
        }
    }

    fn ids() -> (
        CharacterId,
        SessionId,
        ClientInstanceId,
        UserId,
        RefreshFamilyId,
    ) {
        (
            CharacterId::new(Uuid::from_u128(101)).unwrap(),
            SessionId::new(Uuid::from_u128(102)).unwrap(),
            ClientInstanceId::new(Uuid::from_u128(103)).unwrap(),
            UserId::new(Uuid::from_u128(104)).unwrap(),
            RefreshFamilyId::new(Uuid::from_u128(105)).unwrap(),
        )
    }

    fn compatibility() -> BuildCompatibility {
        let manifest = serde_json::from_value(json!({
            "schema_version": 1,
            "game_build": {
                "id": "pokeemerald-coop",
                "numeric_id": 65536,
                "rom_sha256": "0000000000000000000000000000000000000000000000000000000000000000"
            },
            "net_bridge": {
                "symbol": "gCoopNetBridge",
                "address": 33_554_432,
                "size": 9244,
                "magic": 1_347_111_759,
                "abi_version": 1,
                "game_protocol_version": 1,
                "byte_order": "little",
                "checksum": {"algorithm": "CRC-32/IEEE", "covered_bytes": [0, 139], "stored_offset": 140},
                "offsets": {"magic": 0, "abi_version": 4, "game_protocol_version": 6, "game_build_id": 8, "status_flags": 10, "last_sidecar_heartbeat": 12, "game_to_network": 16, "network_to_game": 4632},
                "queue": {"capacity": 32, "size": 4612, "read_index_offset": 0, "write_index_offset": 2, "entries_offset": 4},
                "message": {"size": 144, "payload_size": 128, "offsets": {"type": 0, "length": 2, "sequence": 4, "session_epoch": 8, "payload": 12, "checksum": 140}}
            }
        }))
        .unwrap();
        let target = CompatibilityTarget::new(
            GameBuildId::new("pokeemerald-coop").unwrap(),
            Sha256Digest::of_bytes(b"rom"),
            MgbaVersion::new("0.10.5").unwrap(),
            BridgeAbiVersion::new(1).unwrap(),
            ProtocolVersion::new(1).unwrap(),
            Revision::initial(),
        );
        BuildCompatibility {
            target,
            manifest,
            rom_path: "rom.gba".into(),
            mgba_path: "mgba".into(),
        }
    }

    async fn bootstrap(fail_prepare: bool) -> (TempDir, SessionLifecycle, Arc<TestCloud>) {
        let root = tempdir().unwrap();
        let bridge = root.path().join("bridge");
        std::fs::create_dir_all(&bridge).unwrap();
        std::fs::write(bridge.join("generated_addresses.lua"), b"return {}\n").unwrap();
        let (character_id, session_id, client_instance_id, user_id, family_id) = ids();
        let epoch = SessionEpoch::new(1).unwrap();
        let lease = LeaseContract::new(
            LeaseFence::new(
                session_id,
                character_id,
                Revision::initial(),
                epoch,
                client_instance_id,
            ),
            UnixTimestampMillis::new(4_000_000_000_000),
            1,
        )
        .unwrap();
        let login = LoginResponse::new(
            user_id,
            character_id,
            AccessToken::new("access-token").unwrap(),
            RefreshToken::new("refresh-token").unwrap(),
            family_id,
            UnixTimestampMillis::new(4_000_000_000_000),
            UnixTimestampMillis::new(4_000_000_100_000),
        )
        .unwrap();
        let cloud = Arc::new(TestCloud::new(lease, login, fail_prepare));
        let keychain: Arc<dyn RefreshTokenStore> = Arc::new(TestKeychain::default());
        let auth = AuthSession::login(
            cloud.as_ref(),
            keychain.as_ref(),
            "ash",
            Password::new("password").unwrap(),
        )
        .await
        .unwrap();
        let config = SessionConfig {
            client_instance_id,
            manifest: compatibility(),
            trusted_manifest_key: TrustedManifestKey::new(
                "test",
                ed25519_dalek::SigningKey::from_bytes(&[7; 32])
                    .verifying_key()
                    .to_bytes(),
            )
            .unwrap(),
            epoch_store: EpochStore::new(root.path().join("epoch.json")),
            workspace_parent: root.path().join("sessions"),
            bridge_lua_dir: bridge,
        };
        let session =
            SessionLifecycle::acquire_with_keychain(cloud.as_ref(), auth, config, keychain)
                .await
                .unwrap();
        (root, session, cloud)
    }

    async fn control_pair(
        epoch: u32,
        sequence: u32,
    ) -> (ControlChannel, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut command_bytes = Vec::new();
            loop {
                let mut byte = [0_u8; 1];
                stream.read_exact(&mut byte).await.unwrap();
                if byte[0] == b'\n' {
                    break;
                }
                command_bytes.push(byte[0]);
            }
            let command: coop_sidecar::control::ControlCommand =
                serde_json::from_slice(&command_bytes).unwrap();
            let command_id = match command {
                coop_sidecar::control::ControlCommand::CheckpointGrant(grant) => {
                    assert_eq!(grant.session_epoch, epoch);
                    assert_eq!(grant.ready_sequence, sequence);
                    grant.command_id
                }
                coop_sidecar::control::ControlCommand::CheckpointAbort(_) => {
                    panic!("checkpoint must grant before save capture")
                }
            };
            let result = ControlEvent::CommandResult {
                command_id,
                status: CommandStatus::Applied,
                reason: None,
            };
            let save = ControlEvent::SaveDataUpdated {
                session_epoch: epoch,
                ready_sequence: sequence,
                save_sequence: 1,
            };
            for event in [result, save] {
                let mut line = serde_json::to_vec(&event).unwrap();
                line.push(b'\n');
                stream.write_all(&line).await.unwrap();
            }
        });
        let stream = tokio::net::TcpStream::connect(address).await.unwrap();
        (ControlChannel::from_stream_for_test(stream), server)
    }

    #[tokio::test]
    async fn reconnect_requires_new_epoch_and_rotates_key_per_operation() {
        let (root, mut session, cloud) = bootstrap(false).await;
        std::fs::remove_file(root.path().join("epoch.json")).unwrap();
        assert!(matches!(
            session.reconnect(cloud.as_ref()).await,
            Err(SessionError::Lease)
        ));
        cloud.reconnect_requests.lock().unwrap().clear();

        cloud.set_reconnect_epoch(2);
        let first_key = session.reconnect_key;
        session.reconnect(cloud.as_ref()).await.unwrap();
        assert_ne!(session.reconnect_key, first_key);
        assert_eq!(session.lease.session_epoch.value(), 2);
        let request = cloud.reconnect_requests.lock().unwrap()[0];
        assert_eq!(request.idempotency_key(), first_key);
        assert_eq!(
            request.fence(),
            LeaseFence::new(
                session.lease.session_id,
                session.lease.character_id,
                session.lease.current_revision,
                SessionEpoch::new(1).unwrap(),
                session.lease.client_instance_id,
            )
        );

        cloud.set_reconnect_epoch(3);
        let second_key = session.reconnect_key;
        session.reconnect(cloud.as_ref()).await.unwrap();
        assert_ne!(session.reconnect_key, second_key);
        assert_eq!(session.lease.session_epoch.value(), 3);
        let request = cloud.reconnect_requests.lock().unwrap()[1];
        assert_eq!(request.idempotency_key(), second_key);
        assert_eq!(
            request.fence(),
            LeaseFence::new(
                session.lease.session_id,
                session.lease.character_id,
                session.lease.current_revision,
                SessionEpoch::new(2).unwrap(),
                session.lease.client_instance_id,
            )
        );
    }

    #[tokio::test]
    async fn reconnect_transport_retry_accepts_only_same_key_exact_replay() {
        let (_root, mut session, cloud) = bootstrap(false).await;
        // The cloud rotates before the first response is lost. The retry
        // reuses this operation's key and must commit the already-rotated
        // epoch, while the session rotates its key only after success.
        cloud.set_reconnect_epoch(2);
        cloud.set_reconnect_transport_once();
        let key = session.reconnect_key;
        session.reconnect(cloud.as_ref()).await.unwrap();
        assert_eq!(session.lease.session_epoch.value(), 2);
        assert_ne!(session.reconnect_key, key);
        let requests = cloud.reconnect_requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(
            requests[0], requests[1],
            "transport retry changed reconnect identity"
        );
        assert_eq!(requests[0].idempotency_key(), key);
    }

    #[test]
    fn reconnect_epoch_compare_and_accept_is_atomic_under_race() {
        let root = tempdir().unwrap();
        let store = Arc::new(EpochStore::new(root.path().join("epoch.json")));
        let (character, session, _, _, _) = ids();
        store
            .accept(character, session, SessionEpoch::new(1).unwrap())
            .unwrap();
        let mut workers = Vec::new();
        for _ in 0..8 {
            let store = Arc::clone(&store);
            workers.push(std::thread::spawn(move || {
                store.accept_reconnect(
                    character,
                    session,
                    SessionEpoch::new(1).unwrap(),
                    SessionEpoch::new(2).unwrap(),
                    false,
                )
            }));
        }
        let successes = workers
            .into_iter()
            .filter_map(|worker| worker.join().unwrap().ok())
            .count();
        assert_eq!(successes, 1);
        assert_eq!(
            store
                .read(character, session)
                .unwrap()
                .unwrap()
                .greatest_epoch,
            2
        );
    }

    #[tokio::test]
    async fn checkpoint_runs_heartbeat_grant_save_prepare_upload_finalize_and_release() {
        let (_root, mut session, cloud) = bootstrap(false).await;
        let expected_heartbeat_fence = session.lease.fence();
        session.heartbeat(cloud.as_ref()).await.unwrap();
        session
            .workspace
            .write_atomic("character.sav", b"first-safe-save")
            .unwrap();
        let (mut control, server) = control_pair(1, 7).await;
        let revision = session
            .checkpoint(
                cloud.as_ref(),
                &mut control,
                ControlEvent::CheckpointReady {
                    session_epoch: 1,
                    ready_sequence: 7,
                },
            )
            .await
            .unwrap();
        assert_eq!(revision, Revision::new(1));
        assert!(*cloud.heartbeats.lock().unwrap() >= 1);
        assert_eq!(*cloud.prepares.lock().unwrap(), 1);
        assert_eq!(cloud.uploads.lock().unwrap().len(), 2);
        assert_eq!(*cloud.finalizes.lock().unwrap(), 1);
        server.await.unwrap();
        let expected_release_fence = session.lease.fence();
        session.release(cloud.as_ref()).await.unwrap();
        assert_eq!(*cloud.releases.lock().unwrap(), 1);
        assert_eq!(*cloud.logouts.lock().unwrap(), 1);
        let heartbeat = cloud.heartbeat_requests.lock().unwrap()[0];
        assert_eq!(heartbeat.fence(), expected_heartbeat_fence);
        let release = cloud.release_requests.lock().unwrap()[0];
        assert_eq!(release.session_id, expected_release_fence.session_id);
        assert_eq!(release.character_id, expected_release_fence.character_id);
        assert_eq!(
            release.current_revision,
            expected_release_fence.current_revision
        );
        assert_eq!(release.session_epoch, expected_release_fence.session_epoch);
        assert_eq!(
            release.client_instance_id,
            expected_release_fence.client_instance_id
        );
    }

    #[tokio::test]
    async fn downloaded_resume_is_retired_until_a_fresh_capture_is_marked() {
        let (_root, mut session, cloud) = bootstrap(false).await;
        session
            .workspace
            .write_atomic("character.sav", b"canonical-save")
            .unwrap();
        session
            .workspace
            .write_atomic("resume.input.ss1", b"downloaded-state")
            .unwrap();
        let (mut control, server) = control_pair(1, 11).await;

        session
            .checkpoint(
                cloud.as_ref(),
                &mut control,
                ControlEvent::CheckpointReady {
                    session_epoch: 1,
                    ready_sequence: 11,
                },
            )
            .await
            .unwrap();
        server.await.unwrap();

        let uploads = cloud.uploads.lock().unwrap();
        assert_eq!(uploads.len(), 2);
        assert!(
            uploads
                .iter()
                .all(|(artifact, _)| *artifact != ArtifactIdentity::ResumeSs1)
        );
        assert!(!session.workspace.path().join("resume.input.ss1").exists());
    }

    #[tokio::test]
    async fn delayed_finalize_commit_race_requires_the_exact_next_revision() {
        let (_root, mut session, cloud) = bootstrap(false).await;
        session
            .workspace
            .write_atomic("character.sav", b"commit-race-save")
            .unwrap();
        cloud.set_heartbeat_revision(Revision::new(1));
        cloud.set_finalize_delay(Duration::from_millis(20));
        let (mut control, server) = control_pair(1, 12).await;

        let revision = session
            .checkpoint(
                cloud.as_ref(),
                &mut control,
                ControlEvent::CheckpointReady {
                    session_epoch: 1,
                    ready_sequence: 12,
                },
            )
            .await
            .unwrap();

        assert_eq!(revision, Revision::new(1));
        assert!(*cloud.heartbeats.lock().unwrap() > 0);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn delayed_finalize_rejects_an_unrelated_heartbeat_identity() {
        let (_root, mut session, cloud) = bootstrap(false).await;
        session
            .workspace
            .write_atomic("character.sav", b"identity-race-save")
            .unwrap();
        cloud.set_heartbeat_character(CharacterId::new(Uuid::from_u128(999)).unwrap());
        cloud.set_finalize_delay(Duration::from_millis(20));
        let (mut control, server) = control_pair(1, 13).await;

        let error = session
            .checkpoint(
                cloud.as_ref(),
                &mut control,
                ControlEvent::CheckpointReady {
                    session_epoch: 1,
                    ready_sequence: 13,
                },
            )
            .await
            .unwrap_err();

        assert!(matches!(error, SessionError::Lease));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn slow_checkpoint_responses_keep_the_active_lease_alive() {
        let (_root, mut session, cloud) = bootstrap(false).await;
        session
            .workspace
            .write_atomic("character.sav", b"slow-safe-save")
            .unwrap();
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut line = Vec::new();
            loop {
                let mut byte = [0_u8; 1];
                stream.read_exact(&mut byte).await.unwrap();
                if byte[0] == b'\n' {
                    break;
                }
                line.push(byte[0]);
            }
            let command: ControlCommand = serde_json::from_slice(&line).unwrap();
            let ControlCommand::CheckpointGrant(grant) = command else {
                panic!("test expected checkpoint grant");
            };
            tokio::time::sleep(Duration::from_millis(20)).await;
            let result = ControlEvent::CommandResult {
                command_id: grant.command_id,
                status: CommandStatus::Applied,
                reason: None,
            };
            let updated = ControlEvent::SaveDataUpdated {
                session_epoch: grant.session_epoch,
                ready_sequence: grant.ready_sequence,
                save_sequence: 1,
            };
            for event in [result, updated] {
                let mut bytes = serde_json::to_vec(&event).unwrap();
                bytes.push(b'\n');
                stream.write_all(&bytes).await.unwrap();
            }
        });
        let stream = tokio::net::TcpStream::connect(address).await.unwrap();
        let mut control = ControlChannel::from_stream_for_test(stream);
        let before = *cloud.heartbeats.lock().unwrap();
        session
            .checkpoint(
                cloud.as_ref(),
                &mut control,
                ControlEvent::CheckpointReady {
                    session_epoch: 1,
                    ready_sequence: 9,
                },
            )
            .await
            .unwrap();
        server.await.unwrap();
        assert!(*cloud.heartbeats.lock().unwrap() > before);
    }

    #[tokio::test]
    async fn historical_artifact_fetch_keeps_the_active_revision_fence() {
        let (_root, mut session, cloud) = bootstrap(false).await;
        session.revision = Revision::new(3);
        session.lease.current_revision = Revision::new(3);
        session.auth.set_active_fence(session.lease.fence());
        cloud.set_heartbeat_revision(Revision::new(3));
        cloud.set_delayed_artifact(b"historical-save".to_vec(), Duration::from_millis(20));
        let character = session.lease.character_id;

        let bytes = session
            .authenticated_artifact(
                cloud.as_ref(),
                character,
                ArtifactIdentity::CharacterSav,
                Revision::new(2),
            )
            .await
            .unwrap();

        assert_eq!(bytes, b"historical-save");
        let requests = cloud.heartbeat_requests.lock().unwrap();
        assert!(!requests.is_empty());
        assert!(
            requests
                .iter()
                .all(|request| request.current_revision == Revision::new(3))
        );
    }

    #[tokio::test]
    async fn heartbeat_401_refreshes_and_retries_exactly_once() {
        let (_root, mut session, cloud) = bootstrap(false).await;
        cloud.enable_refresh();
        cloud.set_heartbeat_unauthorized_once();
        session.heartbeat(cloud.as_ref()).await.unwrap();
        assert_eq!(*cloud.heartbeats.lock().unwrap(), 2);
        assert_eq!(
            session
                .auth
                .access_token()
                .expect("test auth session remains active")
                .expose_secret(),
            "refreshed-access"
        );
    }

    fn long_running_test_child() -> tokio::process::Child {
        #[cfg(windows)]
        let mut command = {
            let mut command = Command::new("ping.exe");
            command.args(["-n", "30", "127.0.0.1"]);
            command
        };
        #[cfg(not(windows))]
        let mut command = {
            let mut command = Command::new("sleep");
            command.arg("30");
            command
        };
        command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap()
    }

    #[tokio::test]
    async fn graceful_shutdown_drains_once_and_returns_for_reap() {
        let (_root, mut session, cloud) = bootstrap(false).await;
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        });
        let stream = tokio::net::TcpStream::connect(address).await.unwrap();
        let control = ControlChannel::from_stream_for_test(stream);
        let mut children = SupervisedChildren::for_test(
            long_running_test_child(),
            long_running_test_child(),
            control,
        );
        let result = timeout(
            std::time::Duration::from_secs(1),
            session.run_until_shutdown(cloud.as_ref(), &mut children, async {}),
        )
        .await
        .expect("shutdown must complete after one bounded drain");
        assert!(result.is_ok(), "graceful shutdown result: {result:?}");
        children.stop().await.unwrap();
        server.abort();
    }

    #[tokio::test]
    async fn graceful_shutdown_has_one_global_deadline_for_endless_control_events() {
        let (_root, mut session, cloud) = bootstrap(false).await;
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let event = ControlEvent::CheckpointExpired {
                session_epoch: 1,
                ready_sequence: 1,
            };
            let mut line = serde_json::to_vec(&event).unwrap();
            line.push(b'\n');
            loop {
                if stream.write_all(&line).await.is_err() {
                    break;
                }
            }
        });
        let stream = tokio::net::TcpStream::connect(address).await.unwrap();
        let control = ControlChannel::from_stream_for_test(stream);
        let mut children = SupervisedChildren::for_test(
            long_running_test_child(),
            long_running_test_child(),
            control,
        );
        let result = timeout(
            std::time::Duration::from_secs(1),
            session.run_until_shutdown(cloud.as_ref(), &mut children, async {}),
        )
        .await
        .expect("endless ready control stream must not starve shutdown");
        assert!(result.is_ok(), "shutdown result: {result:?}");
        children.stop().await.unwrap();
        server.abort();
    }

    #[test]
    fn signed_package_lineage_requires_exact_parent_revision() {
        let character = ids().0;
        let manifest = ResumePackageManifest {
            package_version: 1,
            character_id: character,
            parent_revision: Revision::new(1),
            revision: Revision::new(3),
            game_build_id: GameBuildId::new("pokeemerald-coop").unwrap(),
            rom_sha256: Sha256Digest::of_bytes(b"rom"),
            mgba_version: MgbaVersion::new("0.10.5").unwrap(),
            bridge_abi: BridgeAbiVersion::new(1).unwrap(),
            protocol_version: ProtocolVersion::new(1).unwrap(),
            save_sha256: Sha256Digest::of_bytes(b"save"),
            savestate_sha256: None,
            savestate_compatible: false,
            created_at: coop_cloud::CreatedAt::new("2026-09-01T00:00:00Z").unwrap(),
            last_commit_id: None,
            snapshot_id: coop_cloud::SnapshotId::new(Uuid::from_u128(106)).unwrap(),
            session_epoch: SessionEpoch::new(1).unwrap(),
            pending_commits_sha256: Sha256Digest::of_bytes(b"[]"),
        };
        assert!(!super::has_expected_lineage(&manifest, Revision::new(3)));
        let mut contiguous = manifest;
        contiguous.parent_revision = Revision::new(2);
        assert!(super::has_expected_lineage(&contiguous, Revision::new(3)));
    }

    fn historical_record(revision: u32) -> SnapshotRecord {
        let (character, session, _, _, _) = ids();
        let files = vec![
            coop_cloud::SnapshotFile::from_bytes(ArtifactIdentity::CharacterSav, b"save").unwrap(),
            coop_cloud::SnapshotFile::from_bytes(ArtifactIdentity::PendingCommits, b"[]").unwrap(),
        ];
        SnapshotRecord::new(
            coop_cloud::SnapshotId::new(Uuid::new_v4()).unwrap(),
            SnapshotFence::new(session, character, SessionEpoch::new(1).unwrap()),
            Revision::new((revision - 1).into()),
            Revision::new(revision.into()),
            files,
            Sha256Digest::of_bytes(b"[]"),
            None,
            UnixTimestampMillis::new(4_000_000_000_000),
        )
        .unwrap()
    }

    #[test]
    fn historical_validation_rejects_malformed_or_repeated_tail_before_selection() {
        let (character, _, _, _, _) = ids();
        let first = historical_record(2);
        let mut malformed_tail = historical_record(1);
        malformed_tail.revision = Revision::new(0);
        assert!(matches!(
            super::validate_history_records(
                &[first.clone(), malformed_tail],
                Revision::new(3),
                character,
            ),
            Err(SessionError::History)
        ));

        assert!(matches!(
            super::validate_history_records(&[historical_record(3)], Revision::new(3), character,),
            Err(SessionError::History)
        ));

        let repeated_tail = historical_record(2);
        assert!(matches!(
            super::validate_history_records(&[first, repeated_tail], Revision::new(3), character,),
            Err(SessionError::History)
        ));
    }

    #[test]
    fn historical_validation_accepts_a_prior_session_with_verified_lineage() {
        let (character, _, _, _, _) = ids();
        let mut prior = historical_record(2);
        prior.session_id = SessionId::new(Uuid::new_v4()).unwrap();
        assert!(super::validate_history_records(&[prior], Revision::new(3), character).is_ok());
    }

    #[test]
    fn historical_validation_is_bounded_to_twenty_candidates() {
        let (character, _, _, _, _) = ids();
        let records = vec![historical_record(2); 21];
        assert!(matches!(
            super::validate_history_records(&records, Revision::new(3), character),
            Err(SessionError::History)
        ));
    }

    #[test]
    fn historical_snapshot_id_correlation_rejects_a_different_active_package() {
        let (character, _, _, _, _) = ids();
        let record = historical_record(2);
        let manifest = ResumePackageManifest {
            package_version: 1,
            character_id: character,
            parent_revision: record.parent_revision,
            revision: record.revision,
            game_build_id: GameBuildId::new("pokeemerald-coop").unwrap(),
            rom_sha256: Sha256Digest::of_bytes(b"rom"),
            mgba_version: MgbaVersion::new("0.10.5").unwrap(),
            bridge_abi: BridgeAbiVersion::new(1).unwrap(),
            protocol_version: ProtocolVersion::new(1).unwrap(),
            save_sha256: Sha256Digest::of_bytes(b"save"),
            savestate_sha256: None,
            savestate_compatible: false,
            created_at: coop_cloud::CreatedAt::new("2026-09-01T00:00:00Z").unwrap(),
            last_commit_id: None,
            snapshot_id: coop_cloud::SnapshotId::new(Uuid::new_v4()).unwrap(),
            session_epoch: record.session_epoch,
            pending_commits_sha256: Sha256Digest::of_bytes(b"[]"),
        };
        let package = super::VerifiedPackage {
            manifest,
            sav: b"save".to_vec(),
            pending: b"[]".to_vec(),
            resume: None,
        };
        assert!(!super::history_record_matches(&record, &package));
    }

    #[test]
    fn recovery_requires_a_regular_nonempty_bounded_sav() {
        let root = tempdir().unwrap();
        let mut workspace = super::SessionWorkspace::create(root.path()).unwrap();
        workspace.write_atomic("character.sav", b"").unwrap();
        assert!(matches!(
            workspace.preserve_recovery(),
            Err(SessionError::Package)
        ));
    }

    #[cfg(windows)]
    #[test]
    fn workspace_directory_guard_denies_root_replacement() {
        let root = tempdir().unwrap();
        let workspace = SessionWorkspace::create(root.path()).unwrap();
        let replacement = root.path().join("replacement");
        assert!(std::fs::rename(workspace.path(), replacement).is_err());
        let moved_root = root.path().with_extension("moved");
        assert!(std::fs::rename(root.path(), moved_root).is_err());
    }

    #[tokio::test]
    async fn postgrant_prepare_failure_preserves_secret_free_recovery() {
        let (_root, mut session, cloud) = bootstrap(true).await;
        session
            .workspace
            .write_atomic("character.sav", b"recoverable-save")
            .unwrap();
        session
            .workspace
            .write_atomic(
                "session.lua",
                b"return { secret = \"0123456789abcdef0123456789abcdef\" }",
            )
            .unwrap();
        let recovery_path = session.workspace.path().to_owned();
        let (mut control, server) = control_pair(1, 8).await;
        let error = session
            .checkpoint(
                cloud.as_ref(),
                &mut control,
                ControlEvent::CheckpointReady {
                    session_epoch: 1,
                    ready_sequence: 8,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(error, SessionError::Cloud));
        server.await.unwrap();
        session.release(cloud.as_ref()).await.unwrap();
        assert_eq!(
            std::fs::read(recovery_path.join("character.sav")).unwrap(),
            b"recoverable-save"
        );
        assert_eq!(
            std::fs::read(recovery_path.join("recovery.marker")).unwrap(),
            b"coop-recovery-v1\n"
        );
        assert!(!recovery_path.join("session.lua").exists());
        let entries = std::fs::read_dir(&recovery_path).unwrap().count();
        assert_eq!(entries, 2);
    }

    #[tokio::test]
    async fn child_reap_failure_preserves_authorized_recovery_before_returning() {
        let (_root, mut session, _cloud) = bootstrap(false).await;
        session
            .workspace
            .write_atomic("character.sav", b"reap-recovery-save")
            .unwrap();
        session
            .workspace
            .write_atomic(
                "session.lua",
                b"return { secret = \"0123456789abcdef0123456789abcdef\" }",
            )
            .unwrap();
        session.checkpoint_authorized = true;

        session.preserve_recovery_after_child_failure().unwrap();
        assert_eq!(
            std::fs::read(session.workspace.path().join("character.sav")).unwrap(),
            b"reap-recovery-save"
        );
        assert_eq!(
            std::fs::read(session.workspace.path().join("recovery.marker")).unwrap(),
            b"coop-recovery-v1\n"
        );
        assert!(!session.workspace.path().join("session.lua").exists());
    }

    #[tokio::test]
    async fn precreated_recovery_shadow_survives_scrub_failure() {
        let (root, mut session, cloud) = bootstrap(true).await;
        session
            .workspace
            .write_atomic("character.sav", b"shadow-save")
            .unwrap();
        session
            .workspace
            .create_recovery_shadow(b"shadow-save")
            .unwrap();
        let original = session.workspace.path().to_owned();
        std::fs::remove_file(original.join("pending_commits.json")).unwrap();
        std::fs::create_dir(original.join("pending_commits.json")).unwrap();
        session.checkpoint_authorized = true;

        assert!(matches!(
            session.release(cloud.as_ref()).await,
            Err(SessionError::Package)
        ));
        assert!(!original.exists());
        let shadow = std::fs::read_dir(root.path().join("sessions"))
            .unwrap()
            .filter_map(Result::ok)
            .find(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("coop-recovery-")
            })
            .expect("precreated recovery shadow");
        assert_eq!(
            std::fs::read(shadow.path().join("character.sav")).unwrap(),
            b"shadow-save"
        );
        assert_eq!(
            std::fs::read(shadow.path().join("recovery.marker")).unwrap(),
            b"coop-recovery-v1\n"
        );
        assert_eq!(std::fs::read_dir(shadow.path()).unwrap().count(), 2);
    }

    #[tokio::test]
    async fn release_aborts_and_retains_private_workspace_when_scrub_fails() {
        let (root, mut session, cloud) = bootstrap(true).await;
        session
            .workspace
            .write_atomic("character.sav", b"recoverable-save")
            .unwrap();
        let recovery_path = session.workspace.path().to_owned();
        // A directory at a fixed-file path is an injected scrub failure.  The
        // workspace must remain owner-private so its SAV is not lost.
        std::fs::remove_file(recovery_path.join("pending_commits.json")).unwrap();
        std::fs::create_dir(recovery_path.join("pending_commits.json")).unwrap();
        session.checkpoint_authorized = true;
        let error = session.release(cloud.as_ref()).await.unwrap_err();
        assert!(matches!(error, SessionError::Package));
        assert_eq!(*cloud.releases.lock().unwrap(), 0);
        assert_eq!(*cloud.logouts.lock().unwrap(), 1);
        assert!(!recovery_path.exists());
        let quarantined = std::fs::read_dir(root.path().join("sessions"))
            .unwrap()
            .filter_map(Result::ok)
            .find(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("coop-session-")
            })
            .expect("sanitized recovery quarantine");
        assert_eq!(
            std::fs::read(quarantined.path().join("character.sav")).unwrap(),
            b"recoverable-save"
        );
        assert_eq!(
            std::fs::read(quarantined.path().join("recovery.marker")).unwrap(),
            b"coop-recovery-v1\n"
        );
        assert_eq!(std::fs::read_dir(quarantined.path()).unwrap().count(), 2);
    }

    #[tokio::test]
    async fn release_drops_unverified_secret_workspace_instead_of_retaining_it() {
        let (_root, mut session, cloud) = bootstrap(true).await;
        let original = session.workspace.path().to_owned();
        // Make both the quarantine source and the scrub target unverifiable.
        // This models a failure before the pre-created shadow exists and
        // proves the launcher never retains a directory that could contain a
        // loopback secret.
        std::fs::write(original.join("unknown.secret"), b"unclassified-secret").unwrap();
        std::fs::create_dir(original.join("character.sav")).unwrap();
        std::fs::create_dir(original.join("session.lua")).unwrap();
        session.checkpoint_authorized = true;

        assert!(matches!(
            session.release(cloud.as_ref()).await,
            Err(SessionError::Package)
        ));
        assert!(!original.exists());
    }
}
