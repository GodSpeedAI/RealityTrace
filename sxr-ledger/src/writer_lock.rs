//! The writer role is claimed by an exclusive whole-file OS lock on
//! `repos/<repo_id>/writer.lock` (§11.1) — `flock(2)` on Unix, `LockFileEx`
//! on Windows, via the std file-lock API (stable since 1.89). Blocking
//! default with a bounded wait; `spool` is opt-in at the CLI layer and never
//! a silent fallback on contention (§11.1a).

use crate::error::{Error, Result};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// One claimed writer lock. Dropping the guard releases it (kernel-backed).
pub struct WriterLock {
    file: File,
    path: PathBuf,
}

impl std::fmt::Debug for WriterLock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WriterLock")
            .field("path", &self.path)
            .finish()
    }
}

impl WriterLock {
    /// Acquire an exclusive whole-file lock on `writer.lock`, waiting up to
    /// `timeout_secs` (blocking default 30 s, §11.1a).
    pub fn acquire(path: &Path, timeout_secs: u64) -> Result<WriterLock> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        let deadline = Instant::now() + std::time::Duration::from_secs(timeout_secs);
        loop {
            match file.try_lock() {
                Ok(()) => {
                    return Ok(WriterLock {
                        file,
                        path: path.to_path_buf(),
                    });
                }
                Err(std::fs::TryLockError::WouldBlock) => {}
                Err(std::fs::TryLockError::Error(err)) => return Err(Error::Io(err)),
            }
            if Instant::now() >= deadline {
                return Err(Error::LockTimeout {
                    timeout_secs,
                    reason: format!("{} is held by another writer", path.display()),
                });
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for WriterLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;
    use tempfile::tempdir;

    #[test]
    fn lock_is_exclusive() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("writer.lock");
        let _first = WriterLock::acquire(&path, 1).unwrap();
        // Same process: flock is per-open-file-description, so a second fd
        // must block.
        let err = WriterLock::acquire(&path, 1).unwrap_err();
        assert!(err.to_string().contains("held by another writer"), "{err}");
    }

    #[test]
    fn lock_releases_on_drop_and_blocks_until_free() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("writer.lock");
        let released = Arc::new(AtomicBool::new(false));
        let path2 = path.clone();
        let released2 = released.clone();
        let holder = thread::spawn(move || {
            let lock = WriterLock::acquire(&path2, 5).unwrap();
            thread::sleep(std::time::Duration::from_millis(300));
            drop(lock);
            released2.store(true, Ordering::SeqCst);
        });
        // Wait until the holder is known to hold the lock (approx by sleep).
        thread::sleep(std::time::Duration::from_millis(100));
        let acquired = WriterLock::acquire(&path, 5).unwrap();
        assert!(released.load(Ordering::SeqCst));
        holder.join().unwrap();
        drop(acquired);
    }
}
