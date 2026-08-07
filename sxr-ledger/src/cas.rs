//! Content-addressed artifact store (§10.8, §11.3): `cas/sha256/<aa>/<bb>/
//! <full-hash>`, writes are temp → fsync → rename. Every reference carries a
//! `replay_requirement` derived from projector registrations, tracked in a
//! manifest (semantic objects like DomainForge Representation D are always
//! `semantic`, never GC-eligible while referenced).

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

/// §10.8 replay requirement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayRequirement {
    Semantic,
    Query,
    None,
}

/// A content reference `cas://sha256:<hex>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CasRef {
    pub hash: String,
    /// Relative path under the store root, e.g.
    /// `sha256/aa/bb/<full-hex>`.
    pub rel: String,
    pub size: u64,
    pub replay_requirement: ReplayRequirement,
}

/// Manifest mapping content hash → replay requirement (derived registry of
/// projector inputs, §10.8). Rebuildable.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RequirementManifest {
    pub requirements: BTreeMap<String, ReplayRequirement>,
}

pub struct CasStore {
    root: PathBuf,
}

impl CasStore {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn path_for(&self, hash: &str) -> PathBuf {
        let hex = hash.strip_prefix("sha256:").unwrap_or(hash);
        self.root
            .join("sha256")
            .join(&hex[..2])
            .join(&hex[2..4])
            .join(hex)
    }

    /// Put bytes: write temp → fsync → rename (atomic where §10.2a's probe
    /// passed). Idempotent: same bytes → same hash → existing object reused.
    pub fn put(&self, bytes: &[u8], requirement: ReplayRequirement) -> Result<CasRef> {
        use sha2::Digest;
        let hash = format!("sha256:{}", hex::encode(sha2::Sha256::digest(bytes)));
        let final_path = self.path_for(&hash);
        if final_path.exists() {
            return Ok(CasRef {
                hash,
                size: bytes.len() as u64,
                rel: rel_path(&final_path, &self.root),
                replay_requirement: requirement,
            });
        }
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = final_path.with_extension("tmp");
        let mut f = File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::rename(&tmp, &final_path)?;
        Ok(CasRef {
            hash,
            size: bytes.len() as u64,
            rel: rel_path(&final_path, &self.root),
            replay_requirement: requirement,
        })
    }

    /// Read an object by `sha256:<hex>`; the stored bytes are re-hashed on
    /// read so a torn object is detected rather than trusted (AC-REPLAY-001).
    pub fn get(&self, hash: &str) -> Result<Vec<u8>> {
        use sha2::Digest;
        let path = self.path_for(hash);
        let bytes =
            fs::read(&path).map_err(|e| Error::NotFound(format!("{}: {e}", path.display())))?;
        let actual = format!("sha256:{}", hex::encode(sha2::Sha256::digest(&bytes)));
        if actual != hash {
            return Err(Error::Integrity(format!(
                "CAS object {} failed content re-hash (stored {actual})",
                path.display()
            )));
        }
        Ok(bytes)
    }

    pub fn exists(&self, hash: &str) -> bool {
        self.path_for(hash).exists()
    }

    /// Record/update the replay requirement of an object in the manifest.
    pub fn set_requirement(&self, hash: &str, requirement: ReplayRequirement) -> Result<()> {
        let mut manifest = self.load_manifest()?;
        manifest.requirements.insert(hash.to_string(), requirement);
        self.write_manifest(&manifest)
    }

    pub fn requirement(&self, hash: &str) -> Result<ReplayRequirement> {
        let manifest = self.load_manifest()?;
        manifest
            .requirements
            .get(hash)
            .copied()
            .ok_or_else(|| Error::NotFound(format!("no replay requirement recorded for {hash}")))
    }

    fn manifest_path(&self) -> PathBuf {
        self.root.join("requirements.json")
    }

    pub fn load_manifest(&self) -> Result<RequirementManifest> {
        match fs::read_to_string(self.manifest_path()) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| Error::Integrity(format!("requirements.json: {e}"))),
            Err(_) => Ok(RequirementManifest::default()),
        }
    }

    fn write_manifest(&self, manifest: &RequirementManifest) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        let path = self.manifest_path();
        let tmp = path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(manifest).expect("serializable");
        fs::write(&tmp, json)?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }
}

fn rel_path(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn put_get_roundtrip_and_content_addressing() {
        let dir = tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let bytes = b"domainforge canonical D";
        let r1 = store.put(bytes, ReplayRequirement::Semantic).unwrap();
        assert!(r1.hash.starts_with("sha256:"));
        assert_eq!(r1.size, bytes.len() as u64);
        let r2 = store.put(bytes, ReplayRequirement::Query).unwrap();
        assert_eq!(r1.hash, r2.hash, "same bytes must address the same object");
        assert_eq!(store.get(&r1.hash).unwrap(), bytes);
        // Path layout: cas/sha256/<aa>/<bb>/<full>.
        let hex = r1.hash.strip_prefix("sha256:").unwrap();
        let path = dir
            .path()
            .join("sha256")
            .join(&hex[..2])
            .join(&hex[2..4])
            .join(hex);
        assert!(path.exists());
        assert_eq!(
            r1.rel,
            path.strip_prefix(dir.path()).unwrap().to_string_lossy()
        );
    }

    #[test]
    fn torn_object_is_detected_on_read() {
        let dir = tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let r = store.put(b"payload", ReplayRequirement::None).unwrap();
        let path = store.path_for(&r.hash);
        fs::write(&path, b"tampered-bytes").unwrap();
        let err = store.get(&r.hash).unwrap_err();
        assert!(err.to_string().contains("content re-hash"), "{err}");
    }

    #[test]
    fn requirement_manifest_roundtrip() {
        let dir = tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let r = store.put(b"d", ReplayRequirement::Semantic).unwrap();
        store
            .set_requirement(&r.hash, ReplayRequirement::Semantic)
            .unwrap();
        assert_eq!(
            store.requirement(&r.hash).unwrap(),
            ReplayRequirement::Semantic
        );
        // Missing requirement is an error (never defaulted).
        assert!(
            store
                .requirement(
                    "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                )
                .is_err()
        );
    }

    #[test]
    fn put_is_idempotent_and_does_not_rewrite() {
        let dir = tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let r1 = store.put(b"x", ReplayRequirement::Semantic).unwrap();
        let path1 = store.path_for(&r1.hash);
        let mtime1 = fs::metadata(&path1).unwrap().modified().unwrap();
        let r2 = store.put(b"x", ReplayRequirement::Semantic).unwrap();
        assert_eq!(r1.hash, r2.hash);
        let mtime2 = fs::metadata(&path1).unwrap().modified().unwrap();
        assert_eq!(mtime1, mtime2, "existing object must not be rewritten");
    }
}
