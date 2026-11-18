//! NT-07 cross-repo proof fixture. Use only with an isolated, disposable
//! RealityTrace enrollment and an E8 already emitted by the proof harness.
//! Build with `cargo build --locked -p realitytrace --example nt07_persist`.

use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use realitytrace::ledger_io::{RepoLedger, append_record};
use realitytrace::state::Enrollment;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use sxr_core::chain::verify_entry_hashes;
use sxr_core::envelope::{build_envelope, runtime_scope, validate_record_kind_conformance};
use sxr_core::evidence_emit::SCHEMA_VERSION;
use sxr_core::evidence_record::{RecordContext, evidence_recorded_parts};
use sxr_core::world_ref::is_world_ref;
use sxr_ledger::writer_lock::WriterLock;
use sxr_ledger::{Ledger, LedgerConfig};

const MAX_FIXTURE_BYTES: u64 = 1024 * 1024;

fn require_file(path: &Path) -> Result<()> {
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("inspect {}", path.display()))?;
    ensure!(
        metadata.is_file(),
        "expected a regular file: {}",
        path.display()
    );
    Ok(())
}

fn read_fixture<T: DeserializeOwned>(path: &Path) -> Result<T> {
    require_file(path)?;
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_FIXTURE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_FIXTURE_BYTES,
        "fixture exceeds {MAX_FIXTURE_BYTES} bytes: {}",
        path.display()
    );
    serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let [db_path, repo_id, checkout_id, e8_path] = args.as_slice() else {
        bail!("usage: nt07_persist DB_PATH REPO_ID CHECKOUT_ID E8_JSON_FILE");
    };
    let repo_id = repo_id.to_str().context("REPO_ID must be UTF-8")?;
    let checkout_id = checkout_id.to_str().context("CHECKOUT_ID must be UTF-8")?;
    let db_path = Path::new(db_path);
    let dir = db_path.parent().context("DB_PATH must have a parent")?;
    ensure!(
        db_path.file_name() == Some(OsStr::new("ledger.db"))
            && dir.file_name() == Some(OsStr::new(repo_id)),
        "DB_PATH must name the enrolled repository's REPO_ID/ledger.db"
    );
    require_file(db_path)?;
    let enrollment: Enrollment = read_fixture(&dir.join("registry.json"))?;
    ensure!(
        enrollment.repo_id == repo_id && enrollment.checkout_id == checkout_id,
        "REPO_ID or CHECKOUT_ID disagrees with the enrollment"
    );

    let e8: Value = read_fixture(Path::new(e8_path))?;
    ensure!(e8["schema_version"] == SCHEMA_VERSION, "expected a v1 E8");
    let world = e8["payload"]["world_ref"]
        .as_str()
        .filter(|world| is_world_ref(world))
        .context("E8 must carry a pinned payload.world_ref")?;
    let idempotency_key = e8["idempotency_key"]
        .as_str()
        .filter(|key| {
            key.len() == 64
                && key
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        .context("E8 must carry a lowercase SHA-256 idempotency key")?;
    let (parts, subject_refs) = evidence_recorded_parts(
        &e8,
        RecordContext {
            envelope_id: e8["event_id"].as_str().context("E8 event_id")?.into(),
            created_at: e8["occurred_at"]
                .as_str()
                .context("E8 occurred_at")?
                .parse()
                .context("parse E8 occurred_at")?,
            created_by: "sxr NT-07 proof fixture".into(),
            scope: runtime_scope(repo_id, checkout_id, None, None, None, None),
            provenance_refs: vec![
                e8["payload"]["proof_result_ref"]
                    .as_str()
                    .context("E8 proof_result_ref")?
                    .into(),
            ],
            lineage_refs: vec![],
        },
    )?;
    let envelope = build_envelope(&parts)?;
    validate_record_kind_conformance(&parts.record_kind, &envelope, &subject_refs)?;
    ensure!(
        envelope["scope"]["world_ref"] == world,
        "envelope lost its E8 pin"
    );

    let lock = WriterLock::acquire(&dir.join("writer.lock"), 30)?;
    require_file(db_path)?;
    let mut repo = RepoLedger {
        ledger: Ledger::open(db_path, repo_id, LedgerConfig::default())?,
        _lock: lock,
    };
    ensure!(repo.ledger.head()?.is_some(), "fixture ledger is empty");
    let position = append_record(
        &mut repo,
        parts.record_kind,
        Some(idempotency_key.to_owned()),
        subject_refs,
        &envelope,
    )?;

    let row = repo.ledger.read(position.append_ordinal)?;
    verify_entry_hashes(&row).map_err(anyhow::Error::msg)?;
    let stored: Value = serde_json::from_str(&row.payload)?;
    ensure!(
        row.record_kind == parts.record_kind.as_str() && stored == envelope,
        "ledger idempotency key already names a different record"
    );
    println!(
        "{}",
        json!({
            "envelope_id": stored["envelope_id"],
            "world_ref": stored["scope"]["world_ref"],
            "append_ordinal": row.append_ordinal,
            "entry_hash": row.entry_hash,
        })
    );
    Ok(())
}
