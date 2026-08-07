//! §11.3a corruption recovery: quarantine the damaged generation byte-for-byte,
//! fsync the corruption report BEFORE creating the successor, and anchor the
//! successor on the exact verified head. Emergency destructive recovery starts
//! an unanchored `continuity: broken` root.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// §11.3a corruption report; immutable and never GC-eligible.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorruptionReport {
    pub detected_at: String,
    pub method: String,
    pub first_bad_ordinal: u64,
    pub last_verified_ordinal: u64,
    pub last_verified_hash: String,
    pub original_ledger_id: String,
    pub quarantined_file_sha256: String,
    pub verified_count: u64,
    pub unreadable_count: u64,
    pub filesystem_class: String,
    pub recovery_action: String,
}

/// Move `generation_dir` byte-for-byte to `quarantine_root/<ledger_id>/`, make
/// it read-only, and write + fsync the corruption report BEFORE anything else
/// (§11.3a steps 2–3). Returns the quarantine path and the report hash.
pub fn quarantine(
    generation_dir: &Path,
    quarantine_root: &Path,
    report: &CorruptionReport,
) -> Result<(PathBuf, String)> {
    fs::create_dir_all(quarantine_root)?;
    let dst = quarantine_root.join(&report.original_ledger_id);
    if dst.exists() {
        return Err(Error::Generation(format!(
            "quarantine target {} already exists; refusing to overwrite immutable evidence",
            dst.display()
        )));
    }
    fs::rename(generation_dir, &dst)?;

    // Step 3 (§11.3a): fsync the corruption report BEFORE creating B.
    let report_path = dst.join("corruption-report.json");
    let json = serde_json::to_string_pretty(report).expect("serializable");
    fs::write(&report_path, &json)?;
    fsync_file(&report_path)?;
    // The parent directory must be fsynced so the report survives a crash.
    fsync_dir(&dst)?;
    let report_hash = sha256_of(json.as_bytes());

    // Immutable after the report is durable: read-only, byte-for-byte.
    set_read_only_recursive(&dst)?;
    Ok((dst, report_hash))
}

fn set_read_only_recursive(dir: &Path) -> Result<()> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            entries.push(path);
        } else {
            let mut perms = fs::metadata(&path)?.permissions();
            perms.set_readonly(true);
            fs::set_permissions(&path, perms)?;
        }
    }
    for d in entries {
        set_read_only_recursive(&d)?;
    }
    let mut perms = fs::metadata(dir)?.permissions();
    perms.set_readonly(true);
    fs::set_permissions(dir, perms)?;
    Ok(())
}

fn fsync_file(path: &Path) -> Result<()> {
    fs::File::open(path)?.sync_all()?;
    Ok(())
}

fn fsync_dir(dir: &Path) -> Result<()> {
    fs::File::open(dir)?.sync_all()?;
    Ok(())
}

/// sha256 of raw bytes, `sha256:`-prefixed (re-export of the canonical
/// content hash used across the ledger).
pub fn sha256_of(bytes: &[u8]) -> String {
    sxr_core::canonical_json::sha256_hex(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn report(original: &str) -> CorruptionReport {
        CorruptionReport {
            detected_at: "2026-08-07T00:00:00.000000Z".into(),
            method: "chain-verify".into(),
            first_bad_ordinal: 4,
            last_verified_ordinal: 3,
            last_verified_hash: "sha256:old-head".into(),
            original_ledger_id: original.into(),
            quarantined_file_sha256: "sha256:file".into(),
            verified_count: 3,
            unreadable_count: 1,
            filesystem_class: "safe".into(),
            recovery_action: "successor generation".into(),
        }
    }

    #[test]
    fn quarantine_moves_readonly_and_writes_report_first() {
        let dir = tempdir().unwrap();
        let gen_dir = dir.path().join("generations").join("01JA");
        fs::create_dir_all(&gen_dir).unwrap();
        fs::write(gen_dir.join("ledger.db"), b"corrupt-bytes").unwrap();
        let quarantine_root = dir.path().join("quarantine");

        let (dst, hash) = quarantine(&gen_dir, &quarantine_root, &report("01JA")).unwrap();
        assert!(!gen_dir.exists());
        assert!(dst.join("ledger.db").exists());
        assert!(dst.join("corruption-report.json").exists());
        assert!(
            fs::metadata(dst.join("ledger.db"))
                .unwrap()
                .permissions()
                .readonly()
        );
        assert!(hash.starts_with("sha256:"));
        // Report content roundtrip.
        let loaded: CorruptionReport =
            serde_json::from_str(&fs::read_to_string(dst.join("corruption-report.json")).unwrap())
                .unwrap();
        assert_eq!(loaded.first_bad_ordinal, 4);
        assert_eq!(loaded.last_verified_hash, "sha256:old-head");
    }

    #[test]
    fn quarantine_refuses_overwrite_of_immutable_evidence() {
        let dir = tempdir().unwrap();
        let gen_dir = dir.path().join("gen");
        fs::create_dir_all(&gen_dir).unwrap();
        let qroot = dir.path().join("q");
        quarantine(&gen_dir, &qroot, &report("01JA")).unwrap();
        // Second attempt with a fresh generation dir under the same id.
        fs::create_dir_all(&gen_dir).unwrap();
        fs::write(gen_dir.join("ledger.db"), b"more").unwrap();
        let err = quarantine(&gen_dir, &qroot, &report("01JA")).unwrap_err();
        assert!(err.to_string().contains("refusing to overwrite"), "{err}");
    }

    #[test]
    fn sha256_known_vector() {
        assert_eq!(
            sha256_of(b"abc"),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
