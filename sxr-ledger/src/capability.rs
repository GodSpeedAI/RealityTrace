//! §10.2a filesystem capability probes — capability-based, never
//! filesystem-name-based. Probes are cached at first use by the CLI; here
//! they run against a scratch directory inside the candidate state root.

use crate::error::{Error, Result};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

/// §10.2a classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FilesystemClass {
    Safe,
    Degraded,
    Unsafe,
    Unknown,
}

impl FilesystemClass {
    pub fn as_str(self) -> &'static str {
        match self {
            FilesystemClass::Safe => "safe",
            FilesystemClass::Degraded => "degraded",
            FilesystemClass::Unsafe => "unsafe",
            FilesystemClass::Unknown => "unknown",
        }
    }
}

/// Per-probe results, kept so failures can name the probe (§10.2a `unsafe`
/// behaviour: refuse, naming the failed probe).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ProbeReport {
    pub locking: Option<String>,
    pub atomic_rename: Option<String>,
    pub wal_shm: Option<String>,
    pub fsync: Option<String>,
    pub case_sensitivity: Option<String>,
}

impl ProbeReport {
    pub fn all_passed(&self) -> bool {
        [
            &self.locking,
            &self.atomic_rename,
            &self.wal_shm,
            &self.fsync,
            &self.case_sensitivity,
        ]
        .iter()
        .all(|p| p.is_some())
    }

    pub fn failed_probe(&self) -> Option<&'static str> {
        for (name, result) in [
            ("locking", &self.locking),
            ("atomic_rename", &self.atomic_rename),
            ("wal_shm", &self.wal_shm),
            ("fsync", &self.fsync),
            ("case_sensitivity", &self.case_sensitivity),
        ] {
            if result.is_none() {
                return Some(name);
            }
        }
        None
    }
}

/// Run every probe inside `dir` (a scratch directory the caller owns; the
/// CLI uses `<state>/probes/`). Returns the class and the per-probe report.
pub fn probe(dir: &Path) -> Result<(FilesystemClass, ProbeReport)> {
    fs::create_dir_all(dir)?;
    let mut report = ProbeReport::default();

    match probe_locking(dir) {
        Ok(()) => report.locking = Some("exclusive flock acquired and contended".into()),
        Err(_) => report.locking = None,
    }

    match probe_atomic_rename(dir) {
        Ok(()) => report.atomic_rename = Some("rename observed atomic".into()),
        Err(_) => report.atomic_rename = None,
    }

    match probe_wal_shm(dir) {
        Ok(()) => report.wal_shm = Some("journal_mode=wal returned wal".into()),
        Err(_) => report.wal_shm = None,
    }

    match probe_fsync(dir) {
        Ok(()) => report.fsync = Some("file and parent directory fsync ok".into()),
        Err(_e) => report.fsync = None,
    }

    match probe_case_sensitivity(dir) {
        Ok(true) => report.case_sensitivity = Some("case-sensitive (CAS hex paths safe)".into()),
        Ok(false) => report.case_sensitivity = None,
        Err(_) => report.case_sensitivity = None,
    }

    let class = classify(&report);
    Ok((class, report))
}

/// §10.2a classification table.
fn classify(report: &ProbeReport) -> FilesystemClass {
    match report.failed_probe() {
        Some("locking") | Some("wal_shm") => {
            // degraded requires rename atomicity; without it: unsafe.
            if report.atomic_rename.is_some() {
                FilesystemClass::Degraded
            } else {
                FilesystemClass::Unsafe
            }
        }
        Some("atomic_rename") | Some("fsync") | Some("case_sensitivity") => FilesystemClass::Unsafe,
        Some(_) => FilesystemClass::Unknown,
        None => FilesystemClass::Safe,
    }
}

/// POSIX advisory locking: a second fd must not be able to take LOCK_EX while
/// the first holds it.
fn probe_locking(dir: &Path) -> Result<()> {
    let path = dir.join("probe.lock");
    let a = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)?;
    let rc = unsafe { libc::flock(a.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if rc != 0 {
        return Err(Error::Io(std::io::Error::last_os_error()));
    }
    let b = OpenOptions::new().read(true).open(&path)?;
    let rc = unsafe { libc::flock(b.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if rc == 0 {
        // Locking silently succeeded without excluding — unsafe, not degraded.
        unsafe { libc::flock(b.as_raw_fd(), libc::LOCK_UN) };
        return Err(Error::Integrity(
            "locking probe: second lock acquired while first held".into(),
        ));
    }
    let err = std::io::Error::last_os_error();
    if err.raw_os_error() != Some(libc::EWOULDBLOCK) {
        return Err(Error::Io(err));
    }
    unsafe { libc::flock(a.as_raw_fd(), libc::LOCK_UN) };
    Ok(())
}

/// Write temp → rename; must be atomic (old name gone, new name present).
fn probe_atomic_rename(dir: &Path) -> Result<()> {
    let src = dir.join("probe-rename-src");
    let dst = dir.join("probe-rename-dst");
    let _ = fs::remove_file(&src);
    let _ = fs::remove_file(&dst);
    fs::write(&src, b"x")?;
    fs::rename(&src, &dst)?;
    if dst.exists() && !src.exists() {
        Ok(())
    } else {
        Err(Error::Integrity(
            "rename probe: destination missing or source present".into(),
        ))
    }
}

/// WAL needs the `-shm` shared-memory mapping: open a temp DB and confirm
/// `journal_mode=wal` returns `wal` (in-memory databases cannot answer this).
fn probe_wal_shm(dir: &Path) -> Result<()> {
    let path = dir.join("probe-wal.db");
    let conn = rusqlite::Connection::open(&path)?;
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(Error::Sqlite)?;
    let mode: String = conn.query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
    if mode.eq_ignore_ascii_case("wal") {
        Ok(())
    } else {
        Err(Error::Integrity(format!(
            "wal probe: journal_mode returned {mode}"
        )))
    }
}

/// fsync the file and its parent directory. The directory fsync may be
/// unsupported on some filesystems; that is recorded, not fatal.
fn probe_fsync(dir: &Path) -> Result<()> {
    let path = dir.join("probe-fsync");
    let mut f = File::create(&path)?;
    f.write_all(b"x")?;
    f.sync_all()?;
    let dir_f = File::open(dir)?;
    dir_f.sync_all()?;
    Ok(())
}

/// CAS hex paths and logical ids are case-sensitive: create `a` and `A`.
fn probe_case_sensitivity(dir: &Path) -> Result<bool> {
    let a = dir.join("probe-case-a");
    let b = dir.join("probe-case-A");
    fs::write(&a, b"a")?;
    fs::write(&b, b"b")?;
    let mut buf = [0u8; 1];
    let mut fa = File::open(&a)?;
    fa.read_exact(&mut buf)?;
    let contents_a = buf[0];
    let mut fb = File::open(&b)?;
    fb.read_exact(&mut buf)?;
    Ok(contents_a == b'a' && buf[0] == b'b')
}

/// Write the probe cache the CLI reuses. Returns its path.
pub fn write_probe_cache(dir: &Path, report: &ProbeReport) -> Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let path = dir.join("capability.json");
    let json = serde_json::to_string_pretty(report).expect("serializable");
    fs::write(&path, json)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn tempdir_classifies_safe() {
        let dir = tempdir().unwrap();
        let (class, report) = probe(dir.path()).unwrap();
        assert_eq!(class, FilesystemClass::Safe, "{report:?}");
        assert!(report.all_passed());
    }

    #[test]
    fn failed_probe_is_named() {
        let _dir = tempdir().unwrap();
        let report = ProbeReport {
            locking: None,
            ..ProbeReport::default()
        };
        assert_eq!(report.failed_probe(), Some("locking"));
    }

    #[test]
    fn degraded_classifies_when_rename_ok_but_shm_fails() {
        let _dir = tempdir().unwrap();
        let report = ProbeReport {
            locking: Some("ok".into()),
            wal_shm: None,
            atomic_rename: Some("ok".into()),
            fsync: Some("ok".into()),
            case_sensitivity: Some("ok".into()),
        };
        let class = classify(&report);
        assert_eq!(class, FilesystemClass::Degraded);
    }

    #[test]
    fn probe_cache_roundtrip() {
        let dir = tempdir().unwrap();
        let (_, report) = probe(dir.path()).unwrap();
        let cache = write_probe_cache(dir.path(), &report).unwrap();
        assert!(cache.exists());
        let text = fs::read_to_string(&cache).unwrap();
        assert!(text.contains("case_sensitivity"));
    }
}
