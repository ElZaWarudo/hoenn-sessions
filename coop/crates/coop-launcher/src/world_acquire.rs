//! Crash-safe persistence for an in-flight world-aware lease acquisition.
//!
//! The acquisition request is the retry identity.  A launcher must write it
//! before sending `POST /v1/sessions/acquire-world` and must keep it until the
//! resulting lease has been explicitly released or the handoff commit has
//! been confirmed.  This store deliberately never rotates an idempotency key
//! when a different request is supplied: the caller must reconcile the
//! existing intent with the server first.
//!
//! File contents are flushed before sending the request. Directory entries
//! are flushed where the platform supports it; on Windows filesystems that
//! reject directory flushing, durability is file-only across power loss.

use std::{
    fs::{self, File, OpenOptions},
    io,
    io::{Read, Write},
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;

use coop_cloud::{AcquireLeaseRequest, CharacterId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const FORMAT_VERSION: u16 = 1;
const MAX_STATE_BYTES: usize = 4096;
const MAX_VERSIONED_STATE_FILES: usize = 64;
const LOCK_WAIT: Duration = Duration::from_millis(25);
const LOCK_ATTEMPTS: usize = 120;

const INTENT_FILE: &str = "acquire-world.json";

/// Errors returned by [`WorldAcquireIntentStore`].
#[derive(Debug, Error)]
pub enum WorldAcquireIntentError {
    #[error("world acquire intent storage is unavailable")]
    Io(#[source] io::Error),
    #[error("world acquire intent is corrupt")]
    Corrupt,
    #[error("world acquire intent belongs to another character")]
    IdentityMismatch,
    #[error(
        "world acquire intent already exists with a different request; caller must reconcile before retrying"
    )]
    Conflict {
        existing: AcquireLeaseRequest,
        requested: AcquireLeaseRequest,
    },
    #[error("world acquire intent storage is busy")]
    Busy,
    #[error("world acquire intent is too large")]
    TooLarge,
}

/// The durable request identity for one character.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldAcquireIntent {
    pub format_version: u16,
    pub character_id: CharacterId,
    pub request: AcquireLeaseRequest,
    pub generation: u64,
}

impl WorldAcquireIntent {
    fn validate(&self) -> Result<(), WorldAcquireIntentError> {
        if self.format_version != FORMAT_VERSION
            || self.generation == 0
            || self.request.character_id != self.character_id
        {
            return Err(WorldAcquireIntentError::Corrupt);
        }
        Ok(())
    }
}

/// Persists exactly one pending world-aware acquisition request for a
/// character.  `root` must be a caller-owned private directory; all of its
/// existing ancestors and the store's path are checked for symlinks/reparse
/// points before use.
#[derive(Clone, Debug)]
pub struct WorldAcquireIntentStore {
    path: PathBuf,
    character_id: CharacterId,
}

impl WorldAcquireIntentStore {
    /// Creates a store below the caller-provided private directory.
    ///
    /// The directory is created if it does not exist.  Existing symlink or
    /// reparse-point components are rejected before directory creation.
    pub fn new(
        root: impl Into<PathBuf>,
        character_id: CharacterId,
    ) -> Result<Self, WorldAcquireIntentError> {
        let root = root.into();
        reject_symlink_ancestors(&root).map_err(WorldAcquireIntentError::Io)?;
        fs::create_dir_all(&root).map_err(WorldAcquireIntentError::Io)?;
        reject_symlink_ancestors(&root).map_err(WorldAcquireIntentError::Io)?;

        let character_dir = root.join("characters").join(character_id.to_string());
        reject_symlink_ancestors(&character_dir).map_err(WorldAcquireIntentError::Io)?;
        fs::create_dir_all(&character_dir).map_err(WorldAcquireIntentError::Io)?;
        reject_symlink_ancestors(&character_dir).map_err(WorldAcquireIntentError::Io)?;

        let path = character_dir.join(INTENT_FILE);
        reject_symlink_ancestors(&path).map_err(WorldAcquireIntentError::Io)?;
        Ok(Self { path, character_id })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub const fn character_id(&self) -> CharacterId {
        self.character_id
    }

    /// Loads the existing exact request, or durably creates `requested` when
    /// no acquisition is pending.
    ///
    /// A request mismatch is always an explicit conflict.  This includes a
    /// possibly expired server key: expiry cannot be proven locally, so the
    /// caller must reconcile the old request instead of silently inventing a
    /// replacement key.
    pub fn load_or_create(
        &self,
        requested: AcquireLeaseRequest,
    ) -> Result<AcquireLeaseRequest, WorldAcquireIntentError> {
        self.validate_request(requested)?;
        let _lock = FileLock::acquire(&self.lock_path())?;
        let existing = read_latest_record(&self.path)?;
        self.validate_stored_identity(existing.as_ref())?;
        let Some(existing) = existing else {
            let record = WorldAcquireIntent {
                format_version: FORMAT_VERSION,
                character_id: self.character_id,
                request: requested,
                generation: 1,
            };
            persist_record(&self.path, &record)?;
            return Ok(requested);
        };
        if existing.request == requested {
            return Ok(existing.request);
        }
        Err(WorldAcquireIntentError::Conflict {
            existing: existing.request,
            requested,
        })
    }

    /// Reads the pending request without changing it.  A missing record means
    /// that there is no acquisition intent to replay.
    pub fn read(&self) -> Result<Option<WorldAcquireIntent>, WorldAcquireIntentError> {
        let _lock = FileLock::acquire(&self.lock_path())?;
        let record = read_latest_record(&self.path)?;
        self.validate_stored_identity(record.as_ref())?;
        Ok(record)
    }

    /// Clears a request only when it exactly matches the durable request.
    ///
    /// The caller must invoke this only after the server has confirmed the
    /// corresponding release or handoff commit.  A missing record is already
    /// clear and returns `Ok(false)`; a different record remains a conflict.
    pub fn clear_exact(
        &self,
        request: AcquireLeaseRequest,
    ) -> Result<bool, WorldAcquireIntentError> {
        self.validate_request(request)?;
        let _lock = FileLock::acquire(&self.lock_path())?;
        let Some(existing) = read_latest_record(&self.path)? else {
            return Ok(false);
        };
        self.validate_stored_identity(Some(&existing))?;
        if existing.request != request {
            return Err(WorldAcquireIntentError::Conflict {
                existing: existing.request,
                requested: request,
            });
        }
        remove_record_files(&self.path).map_err(WorldAcquireIntentError::Io)?;
        Ok(true)
    }

    fn validate_request(
        &self,
        request: AcquireLeaseRequest,
    ) -> Result<(), WorldAcquireIntentError> {
        if request.character_id != self.character_id {
            return Err(WorldAcquireIntentError::IdentityMismatch);
        }
        Ok(())
    }

    fn validate_stored_identity(
        &self,
        record: Option<&WorldAcquireIntent>,
    ) -> Result<(), WorldAcquireIntentError> {
        if record.is_some_and(|record| record.character_id != self.character_id) {
            return Err(WorldAcquireIntentError::IdentityMismatch);
        }
        Ok(())
    }

    fn lock_path(&self) -> PathBuf {
        let mut lock = self.path.as_os_str().to_os_string();
        lock.push(".lock");
        PathBuf::from(lock)
    }
}

fn read_latest_record(path: &Path) -> Result<Option<WorldAcquireIntent>, WorldAcquireIntentError> {
    let mut candidates = Vec::new();
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if is_symlink_or_reparse(&metadata) || !metadata.is_file() {
                return Err(WorldAcquireIntentError::Corrupt);
            }
            candidates.push(path.to_path_buf());
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(WorldAcquireIntentError::Io(error)),
    }

    let parent = path.parent().ok_or_else(|| {
        WorldAcquireIntentError::Io(io::Error::other("intent path has no parent"))
    })?;
    let prefix = format!(
        "{}.g",
        path.file_name().unwrap_or_default().to_string_lossy()
    );
    match fs::read_dir(parent) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(WorldAcquireIntentError::Io)?;
                let name = entry.file_name();
                if !name.to_string_lossy().starts_with(&prefix) {
                    continue;
                }
                let metadata =
                    fs::symlink_metadata(entry.path()).map_err(WorldAcquireIntentError::Io)?;
                if is_symlink_or_reparse(&metadata) || !metadata.is_file() {
                    return Err(WorldAcquireIntentError::Corrupt);
                }
                candidates.push(entry.path());
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(WorldAcquireIntentError::Io(error)),
    }

    let mut latest = None;
    for candidate in candidates {
        let record = read_record_file(&candidate)?;
        if latest
            .as_ref()
            .is_none_or(|current: &WorldAcquireIntent| record.generation > current.generation)
        {
            latest = Some(record);
        }
    }
    Ok(latest)
}

fn read_record_file(path: &Path) -> Result<WorldAcquireIntent, WorldAcquireIntentError> {
    let file = open_record_read(path).map_err(WorldAcquireIntentError::Io)?;
    let mut bytes = Vec::new();
    file.take(MAX_STATE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(WorldAcquireIntentError::Io)?;
    if bytes.len() > MAX_STATE_BYTES {
        return Err(WorldAcquireIntentError::TooLarge);
    }
    let record: WorldAcquireIntent =
        serde_json::from_slice(&bytes).map_err(|_| WorldAcquireIntentError::Corrupt)?;
    record.validate()?;
    Ok(record)
}

fn persist_record(path: &Path, record: &WorldAcquireIntent) -> Result<(), WorldAcquireIntentError> {
    let bytes = serde_json::to_vec(record).map_err(|_| WorldAcquireIntentError::Corrupt)?;
    if bytes.len() > MAX_STATE_BYTES {
        return Err(WorldAcquireIntentError::TooLarge);
    }

    #[cfg(windows)]
    if fs::symlink_metadata(path).is_ok() {
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::other("intent path has no parent"))
            .map_err(WorldAcquireIntentError::Io)?;
        let versioned = parent.join(format!(
            "{}.g{}",
            path.file_name().unwrap_or_default().to_string_lossy(),
            record.generation
        ));
        atomic_write(&versioned, &bytes).map_err(WorldAcquireIntentError::Io)?;
        prune_versioned_records(path).map_err(WorldAcquireIntentError::Io)?;
        return Ok(());
    }

    atomic_write(path, &bytes).map_err(WorldAcquireIntentError::Io)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    reject_symlink_ancestors(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("intent path has no parent"))?;
    fs::create_dir_all(parent)?;
    reject_symlink_ancestors(path)?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        uuid::Uuid::new_v4().simple()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result?;
    sync_directory(parent)?;
    Ok(())
}

fn remove_record_files(path: &Path) -> io::Result<()> {
    reject_symlink_ancestors(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("intent path has no parent"))?;
    let prefix = format!(
        "{}.g",
        path.file_name().unwrap_or_default().to_string_lossy()
    );
    let mut files = Vec::new();
    match fs::symlink_metadata(path) {
        Ok(metadata) if is_symlink_or_reparse(&metadata) || !metadata.is_file() => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "intent record is not a regular file",
            ));
        }
        Ok(_) => files.push(path.to_path_buf()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        if !entry.file_name().to_string_lossy().starts_with(&prefix) {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path())?;
        if is_symlink_or_reparse(&metadata) || !metadata.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "versioned intent record is not a regular file",
            ));
        }
        files.push(entry.path());
    }
    for file in files {
        fs::remove_file(file)?;
    }
    sync_directory(parent)?;
    Ok(())
}

#[cfg(windows)]
fn sync_directory(path: &Path) -> io::Result<()> {
    // Windows can reject FlushFileBuffers for a directory even with
    // FILE_FLAG_BACKUP_SEMANTICS. Match the travel journal's documented
    // FileOnly durability in that case; still report other I/O failures.
    match OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0x0000_0007)
        .custom_flags(0x0200_0000)
        .open(path)
        .and_then(|directory| directory.sync_all())
    {
        Ok(()) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::PermissionDenied | io::ErrorKind::InvalidInput
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(error),
    }
}

#[cfg(not(windows))]
fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(windows)]
fn prune_versioned_records(path: &Path) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("intent path has no parent"))?;
    let prefix = format!(
        "{}.g",
        path.file_name().unwrap_or_default().to_string_lossy()
    );
    let mut entries = Vec::new();
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(generation) = name
            .to_string_lossy()
            .strip_prefix(&prefix)
            .and_then(|value| value.parse::<u64>().ok())
        else {
            continue;
        };
        entries.push((generation, entry.path()));
    }
    entries.sort_unstable_by_key(|entry| entry.0);
    while entries.len() > MAX_VERSIONED_STATE_FILES {
        let (_, old) = entries.remove(0);
        fs::remove_file(old)?;
    }
    Ok(())
}

fn open_record_read(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    options.custom_flags(0x0020_0000);
    #[cfg(target_os = "linux")]
    options.custom_flags(0x0002_0000);
    options.open(path)
}

struct FileLock {
    file: Option<File>,
}

impl FileLock {
    fn acquire(path: &Path) -> Result<Self, WorldAcquireIntentError> {
        reject_symlink_ancestors(path).map_err(WorldAcquireIntentError::Io)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(WorldAcquireIntentError::Io)?;
        }
        reject_symlink_ancestors(path).map_err(WorldAcquireIntentError::Io)?;
        #[cfg(windows)]
        {
            for _ in 0..LOCK_ATTEMPTS {
                let mut options = OpenOptions::new();
                options.write(true).create_new(true);
                options.share_mode(0x0000_0001).custom_flags(0x0420_0000);
                match options.open(path) {
                    Ok(file) => return Ok(Self { file: Some(file) }),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                        thread::sleep(LOCK_WAIT);
                    }
                    Err(error) => return Err(WorldAcquireIntentError::Io(error)),
                }
            }
            Err(WorldAcquireIntentError::Busy)
        }
        #[cfg(not(windows))]
        {
            let mut options = OpenOptions::new();
            options.read(true).write(true).create(true);
            for _ in 0..LOCK_ATTEMPTS {
                let file = options.open(path).map_err(WorldAcquireIntentError::Io)?;
                match try_lock_file(&file) {
                    Ok(()) => return Ok(Self { file: Some(file) }),
                    Err(std::fs::TryLockError::WouldBlock) => {
                        drop(file);
                        thread::sleep(LOCK_WAIT);
                    }
                    Err(std::fs::TryLockError::Error(error)) => {
                        return Err(WorldAcquireIntentError::Io(error));
                    }
                }
            }
            Err(WorldAcquireIntentError::Busy)
        }
    }
}

#[cfg(not(windows))]
fn try_lock_file(file: &File) -> Result<(), std::fs::TryLockError> {
    #[cfg(target_os = "android")]
    {
        rustix::fs::flock(file, rustix::fs::FlockOperation::NonBlockingLockExclusive).map_err(
            |error| {
                if error == rustix::io::Errno::WOULDBLOCK {
                    std::fs::TryLockError::WouldBlock
                } else {
                    std::fs::TryLockError::Error(error.into())
                }
            },
        )
    }
    #[cfg(not(target_os = "android"))]
    file.try_lock()
}

fn reject_symlink_ancestors(path: &Path) -> io::Result<()> {
    let mut current = path;
    loop {
        match fs::symlink_metadata(current) {
            Ok(metadata) if is_symlink_or_reparse(&metadata) => {
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
            Err(error) if error.kind() == io::ErrorKind::NotFound => break,
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

fn is_symlink_or_reparse(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        metadata.file_type().is_symlink()
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = self.file.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use uuid::Uuid;

    fn request(character: CharacterId, client: u128, key: u128) -> AcquireLeaseRequest {
        AcquireLeaseRequest::new(
            character,
            coop_cloud::ClientInstanceId::new(Uuid::from_u128(client)).unwrap(),
            coop_cloud::IdempotencyKey::new(Uuid::from_u128(key)).unwrap(),
        )
    }

    #[test]
    fn restart_replays_exact_request_and_rejects_key_rotation() {
        let root = TempDir::new().unwrap();
        let character = CharacterId::new(Uuid::from_u128(11)).unwrap();
        let first = request(character, 12, 13);
        let rotated = request(character, 12, 14);
        let store = WorldAcquireIntentStore::new(root.path(), character).unwrap();

        assert_eq!(store.load_or_create(first).unwrap(), first);
        let restarted = WorldAcquireIntentStore::new(root.path(), character).unwrap();
        assert_eq!(restarted.load_or_create(first).unwrap(), first);
        assert!(matches!(
            restarted.load_or_create(rotated),
            Err(WorldAcquireIntentError::Conflict { .. })
        ));
        assert_eq!(restarted.read().unwrap().unwrap().request, first);
        assert!(restarted.clear_exact(first).unwrap());
        assert!(!restarted.clear_exact(first).unwrap());
    }

    #[test]
    fn mismatched_character_cannot_create_or_clear() {
        let root = TempDir::new().unwrap();
        let character = CharacterId::new(Uuid::from_u128(21)).unwrap();
        let other = CharacterId::new(Uuid::from_u128(22)).unwrap();
        let store = WorldAcquireIntentStore::new(root.path(), character).unwrap();
        let other_request = request(other, 23, 24);

        assert!(matches!(
            store.load_or_create(other_request),
            Err(WorldAcquireIntentError::IdentityMismatch)
        ));
        assert!(matches!(
            store.clear_exact(other_request),
            Err(WorldAcquireIntentError::IdentityMismatch)
        ));
    }

    #[test]
    fn misplaced_valid_record_cannot_be_replayed_or_cleared() {
        let root = TempDir::new().unwrap();
        let character = CharacterId::new(Uuid::from_u128(25)).unwrap();
        let other = CharacterId::new(Uuid::from_u128(26)).unwrap();
        let store = WorldAcquireIntentStore::new(root.path(), character).unwrap();
        let foreign = WorldAcquireIntent {
            format_version: FORMAT_VERSION,
            character_id: other,
            request: request(other, 27, 28),
            generation: 1,
        };
        fs::write(store.path(), serde_json::to_vec(&foreign).unwrap()).unwrap();
        let local = request(character, 27, 29);
        assert!(matches!(
            store.load_or_create(local),
            Err(WorldAcquireIntentError::IdentityMismatch)
        ));
        assert!(matches!(
            store.clear_exact(local),
            Err(WorldAcquireIntentError::IdentityMismatch)
        ));
    }

    #[test]
    fn oversized_state_fails_closed() {
        let root = TempDir::new().unwrap();
        let character = CharacterId::new(Uuid::from_u128(31)).unwrap();
        let store = WorldAcquireIntentStore::new(root.path(), character).unwrap();
        fs::write(store.path(), vec![b'x'; MAX_STATE_BYTES + 1]).unwrap();

        assert!(matches!(
            store.read(),
            Err(WorldAcquireIntentError::TooLarge)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_store_root_is_rejected() {
        use std::os::unix::fs::symlink;

        let root = TempDir::new().unwrap();
        let target = root.path().join("target");
        fs::create_dir(&target).unwrap();
        let link = root.path().join("link");
        symlink(&target, &link).unwrap();
        let character = CharacterId::new(Uuid::from_u128(41)).unwrap();

        assert!(matches!(
            WorldAcquireIntentStore::new(&link, character),
            Err(WorldAcquireIntentError::Io(error))
                if error.kind() == io::ErrorKind::PermissionDenied
        ));
    }
}
