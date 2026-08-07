//! `LedgerHistory` (§10.3): the logical developmental history for one
//! repo_id. `ledger-history.json` is a reconstructible catalog — immutable
//! generation metadata and generation-start records are the source for
//! rebuilding it. This module maintains the catalog append-only.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryCatalogEntry {
    pub ledger_id: String,
    pub generation_number: u64,
    pub continuity: String,
    pub previous_generation_ref: Option<String>,
    pub previous_generation_verified_head: Option<String>,
    /// `generations/<ledger_id>/ledger.db` relative to the repo root.
    pub ledger_db_rel: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerHistoryCatalog {
    pub ledger_history_id: String,
    pub generations: Vec<HistoryCatalogEntry>,
}

impl LedgerHistoryCatalog {
    pub fn new(ledger_history_id: &str) -> Self {
        Self {
            ledger_history_id: ledger_history_id.to_string(),
            generations: Vec::new(),
        }
    }

    /// Load the catalog, or start a fresh one (it is reconstructible).
    pub fn load(path: &Path) -> Result<Self> {
        match fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| Error::Generation(format!("{}: {e}", path.display()))),
            Err(_) => Ok(Self::new("")),
        }
    }

    /// Append a generation to the in-memory catalog (persist with
    /// [`write_catalog`]).
    pub fn push(&mut self, entry: HistoryCatalogEntry) -> Result<()> {
        if self.ledger_history_id.is_empty() {
            return Err(Error::Generation(
                "catalog has no ledger_history_id; must be initialized".into(),
            ));
        }
        if let Some(existing) = self
            .generations
            .iter()
            .find(|g| g.ledger_id == entry.ledger_id)
        {
            return Err(Error::Generation(format!(
                "catalog already contains generation {}",
                existing.ledger_id
            )));
        }
        self.generations.push(entry);
        Ok(())
    }
}

/// Write the catalog to `history/ledger-history.json` (append-only: read-modify-write
/// under the writer lock; reconstructible from generation.json files).
pub fn write_catalog(dir: &Path, catalog: &LedgerHistoryCatalog) -> Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let path = dir.join("ledger-history.json");
    let json = serde_json::to_string_pretty(catalog).expect("serializable");
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(&tmp, &path)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn catalog_is_append_only_and_rejects_duplicates() {
        let mut catalog = LedgerHistoryCatalog::new("01JHIST");
        catalog
            .push(HistoryCatalogEntry {
                ledger_id: "01JA".into(),
                generation_number: 1,
                continuity: "continuous".into(),
                previous_generation_ref: None,
                previous_generation_verified_head: None,
                ledger_db_rel: "generations/01JA/ledger.db".into(),
            })
            .unwrap();
        let err = catalog
            .push(HistoryCatalogEntry {
                ledger_id: "01JA".into(),
                generation_number: 2,
                continuity: "broken".into(),
                previous_generation_ref: None,
                previous_generation_verified_head: None,
                ledger_db_rel: "generations/01JA/ledger.db".into(),
            })
            .unwrap_err();
        assert!(err.to_string().contains("already contains"), "{err}");
    }

    #[test]
    fn roundtrip_via_catalog_file() {
        let dir = tempdir().unwrap();
        let mut catalog = LedgerHistoryCatalog::new("01JHIST");
        catalog
            .push(HistoryCatalogEntry {
                ledger_id: "01JA".into(),
                generation_number: 1,
                continuity: "continuous".into(),
                previous_generation_ref: None,
                previous_generation_verified_head: None,
                ledger_db_rel: "generations/01JA/ledger.db".into(),
            })
            .unwrap();
        let path = write_catalog(dir.path(), &catalog).unwrap();
        let loaded = LedgerHistoryCatalog::load(&path).unwrap();
        assert_eq!(loaded.generations.len(), 1);
        assert_eq!(loaded.generations[0].ledger_id, "01JA");
    }
}
