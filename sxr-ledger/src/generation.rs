//! `LedgerGeneration` identity and metadata (§10.3): a fresh ULID `ledger_id`,
//! the stable parent `ledger_history_id`, predecessor anchors, and the
//! `continuity` verdict. `generation.json` is immutable once written.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Continuity {
    Continuous,
    Broken,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationMeta {
    pub ledger_id: String,
    pub ledger_history_id: String,
    pub generation_number: u64,
    pub previous_generation_ref: Option<String>,
    pub previous_generation_verified_head: Option<String>,
    pub previous_generation_last_verified_ordinal: Option<u64>,
    pub transition_report_ref: Option<String>,
    pub continuity: Continuity,
    pub created_at: String,
}

impl GenerationMeta {
    /// Root generation: fresh ULID, no predecessor, `continuity: continuous`
    /// (§10.3, ordinal 1 = `ledger.generation_started`).
    pub fn root(ledger_history_id: &str) -> Self {
        Self {
            ledger_id: ulid::Ulid::new().to_string(),
            ledger_history_id: ledger_history_id.to_string(),
            generation_number: 1,
            previous_generation_ref: None,
            previous_generation_verified_head: None,
            previous_generation_last_verified_ordinal: None,
            transition_report_ref: None,
            continuity: Continuity::Continuous,
            created_at: now_rfc3339(),
        }
    }

    /// Anchored successor (§11.3a step 4): exact old head H anchors the new
    /// generation; the two entry chains never join.
    pub fn successor(
        ledger_history_id: &str,
        generation_number: u64,
        previous_generation_ref: &str,
        previous_generation_verified_head: &str,
        previous_generation_last_verified_ordinal: u64,
        transition_report_ref: &str,
    ) -> Self {
        Self {
            ledger_id: ulid::Ulid::new().to_string(),
            ledger_history_id: ledger_history_id.to_string(),
            generation_number,
            previous_generation_ref: Some(previous_generation_ref.to_string()),
            previous_generation_verified_head: Some(previous_generation_verified_head.to_string()),
            previous_generation_last_verified_ordinal: Some(
                previous_generation_last_verified_ordinal,
            ),
            transition_report_ref: Some(transition_report_ref.to_string()),
            continuity: Continuity::Continuous,
            created_at: now_rfc3339(),
        }
    }

    /// Destructive recovery root (§11.3a): unanchored, `continuity: broken`,
    /// ordinal 1 = `ledger.destructive_recovery`.
    pub fn destructive_root(ledger_history_id: &str, generation_number: u64) -> Self {
        Self {
            ledger_id: ulid::Ulid::new().to_string(),
            ledger_history_id: ledger_history_id.to_string(),
            generation_number,
            previous_generation_ref: None,
            previous_generation_verified_head: None,
            previous_generation_last_verified_ordinal: None,
            transition_report_ref: None,
            continuity: Continuity::Broken,
            created_at: now_rfc3339(),
        }
    }

    pub fn write(&self, dir: &Path) -> Result<PathBuf> {
        fs::create_dir_all(dir)?;
        let path = dir.join("generation.json");
        let json = serde_json::to_string_pretty(self).expect("serializable");
        fs::write(&path, json)?;
        Ok(path)
    }

    pub fn read(dir: &Path) -> Result<Self> {
        let path = dir.join("generation.json");
        let text = fs::read_to_string(&path)
            .map_err(|e| Error::Generation(format!("{}: {e}", path.display())))?;
        serde_json::from_str(&text)
            .map_err(|e| Error::Generation(format!("{}: {e}", path.display())))
    }
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn root_has_no_predecessor_and_mints_ulid() {
        let meta = GenerationMeta::root("01JHIST");
        assert!(meta.ledger_id.len() == 26);
        assert_eq!(meta.previous_generation_ref, None);
        assert_eq!(meta.generation_number, 1);
        assert_eq!(meta.continuity, Continuity::Continuous);
    }

    #[test]
    fn successor_anchors_exact_old_head() {
        let meta =
            GenerationMeta::successor("01JHIST", 2, "01JA", "sha256:deadbeef", 7, "sha256:report");
        assert_eq!(meta.previous_generation_ref.as_deref(), Some("01JA"));
        assert_eq!(
            meta.previous_generation_verified_head.as_deref(),
            Some("sha256:deadbeef")
        );
        assert_eq!(meta.previous_generation_last_verified_ordinal, Some(7));
        assert_eq!(meta.continuity, Continuity::Continuous);
    }

    #[test]
    fn destructive_root_marks_broken() {
        let meta = GenerationMeta::destructive_root("01JHIST", 3);
        assert_eq!(meta.continuity, Continuity::Broken);
        assert_eq!(meta.previous_generation_ref, None);
    }

    #[test]
    fn roundtrip_via_generation_json() {
        let dir = tempdir().unwrap();
        let meta = GenerationMeta::root("01JHIST");
        meta.write(dir.path()).unwrap();
        let loaded = GenerationMeta::read(dir.path()).unwrap();
        assert_eq!(loaded.ledger_id, meta.ledger_id);
        assert_eq!(loaded.continuity, Continuity::Continuous);
    }
}
