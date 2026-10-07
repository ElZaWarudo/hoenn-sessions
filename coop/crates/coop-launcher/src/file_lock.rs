//! Exclusive advisory locks on whole files.
//!
//! `std::fs::File::lock` and `try_lock` are not implemented for
//! `target_os = "android"` (Rust 1.93 always reports `Unsupported`), so Android
//! takes the same kernel-owned `flock` lock directly. On every platform the lock
//! belongs to the open descriptor and is released when it closes.

use std::fs::{File, TryLockError};
use std::io;

/// Blocks until this descriptor holds an exclusive lock on the file.
///
/// # Errors
///
/// Returns the underlying I/O error.
pub fn lock_file(file: &File) -> io::Result<()> {
    #[cfg(target_os = "android")]
    {
        rustix::fs::flock(file, rustix::fs::FlockOperation::LockExclusive).map_err(io::Error::from)
    }
    #[cfg(not(target_os = "android"))]
    file.lock()
}

/// Takes an exclusive lock without waiting.
///
/// # Errors
///
/// Returns `WouldBlock` when another descriptor holds the lock, or the
/// underlying I/O error.
pub fn try_lock_file(file: &File) -> Result<(), TryLockError> {
    #[cfg(target_os = "android")]
    {
        rustix::fs::flock(file, rustix::fs::FlockOperation::NonBlockingLockExclusive).map_err(
            |error| {
                if error == rustix::io::Errno::WOULDBLOCK {
                    TryLockError::WouldBlock
                } else {
                    TryLockError::Error(error.into())
                }
            },
        )
    }
    #[cfg(not(target_os = "android"))]
    file.try_lock()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::OpenOptions;

    fn open(path: &std::path::Path) -> File {
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .unwrap()
    }

    #[test]
    fn exclusive_lock_is_held_until_its_descriptor_closes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(".lock");
        let first = open(&path);
        lock_file(&first).unwrap();
        let second = open(&path);
        assert!(matches!(
            try_lock_file(&second),
            Err(TryLockError::WouldBlock)
        ));
        drop(first);
        try_lock_file(&second).unwrap();
        let third = open(&path);
        assert!(matches!(
            try_lock_file(&third),
            Err(TryLockError::WouldBlock)
        ));
        drop(second);
        lock_file(&third).unwrap();
    }
}
