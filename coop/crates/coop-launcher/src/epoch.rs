//! Crash-safe, monotonic server-issued session epoch persistence.

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

#[cfg(windows)]
use std::time::{SystemTime, UNIX_EPOCH};

use coop_cloud::{CharacterId, SessionEpoch, SessionId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const FORMAT_VERSION: u16 = 1;
const LOCK_WAIT: Duration = Duration::from_millis(25);
const LOCK_ATTEMPTS: usize = 120;
const MAX_STATE_BYTES: usize = 4096;
const MAX_VERSIONED_STATE_FILES: usize = 64;
#[cfg(windows)]
const LOCK_STALE_AFTER: Duration = Duration::from_secs(10);

#[derive(Debug, Error)]
pub enum EpochError {
    #[error("epoch state is unavailable")]
    Io(#[source] io::Error),
    #[error("epoch state is corrupt")]
    Corrupt,
    #[error("epoch state belongs to another character or session")]
    IdentityMismatch,
    #[error("server returned an invalid or stale session epoch")]
    Stale,
    #[error("server-issued session epoch is exhausted")]
    Exhausted,
    #[error("epoch state lock is busy")]
    Busy,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpochRecord {
    pub format_version: u16,
    pub character_id: CharacterId,
    pub session_id: SessionId,
    pub greatest_epoch: u32,
}

/// Persists the greatest server-issued epoch for one character/session.
#[derive(Clone, Debug)]
pub struct EpochStore {
    path: PathBuf,
}

impl EpochStore {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Read without mutating the state. Missing state means no accepted epoch.
    ///
    /// # Errors
    ///
    /// Returns an error when state is unreadable, corrupt, or belongs to a
    /// different character/session.
    pub fn read(
        &self,
        character_id: CharacterId,
        session_id: SessionId,
    ) -> Result<Option<EpochRecord>, EpochError> {
        let record = self.read_record()?;
        let Some(record) = record else {
            return Ok(None);
        };
        if record.character_id != character_id || record.session_id != session_id {
            return Err(EpochError::IdentityMismatch);
        }
        Ok(Some(record))
    }

    /// Accept exactly one strictly newer nonzero epoch returned by the server.
    /// The lock is held across read, compare, write and atomic rename.
    ///
    /// # Errors
    ///
    /// Returns an error for stale, exhausted, corrupt, or concurrently locked state.
    pub fn accept(
        &self,
        character_id: CharacterId,
        session_id: SessionId,
        server_epoch: SessionEpoch,
    ) -> Result<EpochRecord, EpochError> {
        let _lock = FileLock::acquire(&self.lock_path())?;
        let previous = self.read_record()?;
        if previous
            .as_ref()
            .is_some_and(|record| record.character_id != character_id)
        {
            return Err(EpochError::IdentityMismatch);
        }
        let value = server_epoch.value();
        if value == 0 {
            return Err(EpochError::Stale);
        }
        if value == u32::MAX {
            return Err(EpochError::Exhausted);
        }
        if previous
            .as_ref()
            .is_some_and(|record| value <= record.greatest_epoch)
        {
            return Err(if value == u32::MAX {
                EpochError::Exhausted
            } else {
                EpochError::Stale
            });
        }
        let record = EpochRecord {
            format_version: FORMAT_VERSION,
            character_id,
            session_id,
            greatest_epoch: value,
        };
        let bytes = serde_json::to_vec(&record).map_err(|_| EpochError::Corrupt)?;
        persist_record(&self.path, &bytes, value).map_err(EpochError::Io)?;
        Ok(record)
    }

    /// Atomically validates a reconnect response against the in-memory epoch
    /// and the persisted greatest epoch.  A transport retry may replay the
    /// already accepted epoch exactly once; that replay is accepted only when
    /// the persisted record still proves the same epoch while this lock is
    /// held.  Normal reconnects persist the newer server epoch in this same
    /// critical section, so a concurrent writer cannot make a stale response
    /// appear fresh between read and accept.
    ///
    /// # Errors
    ///
    /// Returns an error when the response is stale, corrupt, exhausted, or
    /// does not belong to the requested character/session.
    pub fn accept_reconnect(
        &self,
        character_id: CharacterId,
        session_id: SessionId,
        current_epoch: SessionEpoch,
        server_epoch: SessionEpoch,
        exact_replay: bool,
    ) -> Result<EpochRecord, EpochError> {
        let _lock = FileLock::acquire(&self.lock_path())?;
        let previous = self.read_record()?;
        if previous.as_ref().is_some_and(|record| {
            record.character_id != character_id || record.session_id != session_id
        }) {
            return Err(EpochError::IdentityMismatch);
        }
        let value = server_epoch.value();
        if value == 0 {
            return Err(EpochError::Stale);
        }
        if value == u32::MAX {
            return Err(EpochError::Exhausted);
        }
        if exact_replay {
            let Some(previous) = previous.as_ref() else {
                return Err(EpochError::Stale);
            };
            if previous.greatest_epoch == value && value >= current_epoch.value() {
                return Ok(previous.clone());
            }
            // A reconnect response can be lost after the cloud rotates its
            // epoch but before the client receives it. A retry with the same
            // idempotency key may commit exactly that newer epoch when local
            // and persisted state still agree on the prior one.
            if previous.greatest_epoch == current_epoch.value() && value > current_epoch.value() {
                let record = EpochRecord {
                    format_version: FORMAT_VERSION,
                    character_id,
                    session_id,
                    greatest_epoch: value,
                };
                let bytes = serde_json::to_vec(&record).map_err(|_| EpochError::Corrupt)?;
                persist_record(&self.path, &bytes, value).map_err(EpochError::Io)?;
                return Ok(record);
            }
            return Err(EpochError::Stale);
        }
        if value <= current_epoch.value()
            || previous
                .as_ref()
                .is_some_and(|record| value <= record.greatest_epoch)
        {
            return Err(EpochError::Stale);
        }
        let record = EpochRecord {
            format_version: FORMAT_VERSION,
            character_id,
            session_id,
            greatest_epoch: value,
        };
        let bytes = serde_json::to_vec(&record).map_err(|_| EpochError::Corrupt)?;
        persist_record(&self.path, &bytes, value).map_err(EpochError::Io)?;
        Ok(record)
    }

    fn read_record(&self) -> Result<Option<EpochRecord>, EpochError> {
        let mut greatest = read_record_file(&self.path)?;
        let parent = self
            .path
            .parent()
            .ok_or_else(|| EpochError::Io(io::Error::other("epoch path has no parent")))?;
        let prefix = format!(
            "{}.epoch-",
            self.path.file_name().unwrap_or_default().to_string_lossy()
        );
        let entries = match fs::read_dir(parent) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(greatest),
            Err(error) => return Err(EpochError::Io(error)),
        };
        let mut versioned_count = 0;
        for entry in entries {
            let entry = entry.map_err(EpochError::Io)?;
            let name = entry.file_name();
            if !name.to_string_lossy().starts_with(&prefix) {
                continue;
            }
            versioned_count += 1;
            if versioned_count > MAX_VERSIONED_STATE_FILES {
                return Err(EpochError::Corrupt);
            }
            let path = entry.path();
            let candidate = read_record_file(&path)?.ok_or(EpochError::Corrupt)?;
            if greatest
                .as_ref()
                .is_none_or(|record| candidate.greatest_epoch > record.greatest_epoch)
            {
                greatest = Some(candidate);
            }
        }
        Ok(greatest)
    }

    fn lock_path(&self) -> PathBuf {
        let mut lock = self.path.as_os_str().to_os_string();
        lock.push(".lock");
        PathBuf::from(lock)
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("epoch path has no parent"))?;
    fs::create_dir_all(parent)?;
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
    if let Ok(directory) = File::open(parent) {
        let _ = directory.sync_all();
    }
    Ok(())
}

fn persist_record(path: &Path, bytes: &[u8], epoch: u32) -> io::Result<()> {
    #[cfg(not(windows))]
    let _ = epoch;
    #[cfg(windows)]
    {
        if fs::symlink_metadata(path).is_ok() {
            let parent = path
                .parent()
                .ok_or_else(|| io::Error::other("epoch path has no parent"))?;
            fs::create_dir_all(parent)?;
            let name = format!(
                "{}.epoch-{epoch}",
                path.file_name().unwrap_or_default().to_string_lossy()
            );
            let versioned = parent.join(name);
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&versioned)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            prune_versioned_records(path, epoch)?;
            return Ok(());
        }
    }
    atomic_write(path, bytes)
}

#[cfg(windows)]
fn prune_versioned_records(path: &Path, newest_epoch: u32) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("epoch path has no parent"))?;
    let prefix = format!(
        "{}.epoch-",
        path.file_name().unwrap_or_default().to_string_lossy()
    );
    let mut entries = Vec::new();
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(epoch) = name
            .to_string_lossy()
            .strip_prefix(&prefix)
            .and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        entries.push((epoch, entry.path()));
    }
    entries.sort_unstable_by_key(|entry| entry.0);
    while entries.len() > MAX_VERSIONED_STATE_FILES {
        let (_, old) = entries.remove(0);
        fs::remove_file(old)?;
    }
    debug_assert!(entries.iter().any(|entry| entry.0 == newest_epoch));
    Ok(())
}

fn read_record_file(path: &Path) -> Result<Option<EpochRecord>, EpochError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(EpochError::Io(error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(EpochError::Corrupt);
    }
    let file = open_record_read(path).map_err(EpochError::Io)?;
    let mut bytes = Vec::new();
    file.take(MAX_STATE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(EpochError::Io)?;
    if bytes.len() > MAX_STATE_BYTES {
        return Err(EpochError::Corrupt);
    }
    let record: EpochRecord = serde_json::from_slice(&bytes).map_err(|_| EpochError::Corrupt)?;
    if record.format_version != FORMAT_VERSION || record.greatest_epoch == 0 {
        return Err(EpochError::Corrupt);
    }
    Ok(Some(record))
}

fn open_record_read(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000);
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x0002_0000);
    }
    options.open(path)
}

struct FileLock {
    file: Option<File>,
}

impl FileLock {
    fn acquire(path: &Path) -> Result<Self, EpochError> {
        reject_symlink_ancestors(path).map_err(EpochError::Io)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(EpochError::Io)?;
        }
        reject_symlink_ancestors(path).map_err(EpochError::Io)?;
        #[cfg(windows)]
        {
            for _ in 0..LOCK_ATTEMPTS {
                let mut options = OpenOptions::new();
                options.write(true).create_new(true);
                // Keep an active owner handle open while denying delete/write
                // sharing. DELETE_ON_CLOSE removes it automatically when the
                // owner exits, so a contender cannot delete a live lock and
                // replace it between a compare and remove operation.
                options.share_mode(0x0000_0001).custom_flags(0x0420_0000);
                match options.open(path) {
                    Ok(mut file) => {
                        let created = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis();
                        let token = format!(
                            "created_millis={created}\nowner_token={}\n",
                            uuid::Uuid::new_v4().simple()
                        )
                        .into_bytes();
                        let _ = file.write_all(&token);
                        let _ = file.sync_all();
                        return Ok(Self { file: Some(file) });
                    }
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                        if let Some(snapshot) = stale_lock(path)
                            && reclaim_stale_lock(path, &snapshot)
                        {
                            continue;
                        }
                        thread::sleep(LOCK_WAIT);
                    }
                    Err(error) => return Err(EpochError::Io(error)),
                }
            }
            Err(EpochError::Busy)
        }
        #[cfg(not(windows))]
        {
            // Unix keeps one persistent pathname and lets the kernel own the
            // lock.  This removes the stale-check/delete handoff race: a
            // crashed owner releases the advisory lock on close, while
            // contenders never remove or replace the lock file itself.
            let mut options = OpenOptions::new();
            options.read(true).write(true).create(true);
            for _ in 0..LOCK_ATTEMPTS {
                let file = options.open(path).map_err(EpochError::Io)?;
                match file.try_lock() {
                    Ok(()) => return Ok(Self { file: Some(file) }),
                    Err(std::fs::TryLockError::WouldBlock) => {
                        drop(file);
                        thread::sleep(LOCK_WAIT);
                    }
                    Err(std::fs::TryLockError::Error(error)) => {
                        return Err(EpochError::Io(error));
                    }
                }
            }
            Err(EpochError::Busy)
        }
    }
}

#[cfg(windows)]
fn stale_lock(path: &Path) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    if let Ok(file) = File::open(path) {
        let _ = file.take(128).read_to_end(&mut bytes);
    }
    if let Ok(text) = std::str::from_utf8(&bytes)
        && let Some(value) = text.trim().strip_prefix("created_millis=")
        && let Ok(created) = value.parse::<u128>()
    {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        if now.saturating_sub(created) >= LOCK_STALE_AFTER.as_millis() {
            return Some(bytes);
        }
    }
    if fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age >= LOCK_STALE_AFTER)
    {
        Some(bytes)
    } else {
        None
    }
}

#[cfg(windows)]
fn reclaim_stale_lock(path: &Path, expected: &[u8]) -> bool {
    // A contender may only remove the exact stale owner record it observed.
    // The second read is deliberately adjacent to remove_file; a live owner
    // cannot replace a lock while the old pathname is present, and a handoff
    // that already occurred changes the token and is left untouched.
    let mut current = Vec::new();
    let Ok(file) = File::open(path) else {
        return false;
    };
    if file.take(128).read_to_end(&mut current).is_err() || current != expected {
        return false;
    }
    let mut confirmed = Vec::new();
    let Ok(file) = File::open(path) else {
        return false;
    };
    if file.take(128).read_to_end(&mut confirmed).is_err() || confirmed != expected {
        return false;
    }
    fs::remove_file(path).is_ok()
}

fn reject_symlink_ancestors(path: &Path) -> io::Result<()> {
    let mut current = path;
    loop {
        match fs::symlink_metadata(current) {
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

impl Drop for FileLock {
    fn drop(&mut self) {
        // Unix releases the advisory lock on close; Windows DELETE_ON_CLOSE
        // removes the owner pathname when its exclusive handle closes.
        let _ = self.file.take();
    }
}
