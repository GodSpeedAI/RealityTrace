//! Build schema-valid runtime CEP envelopes for sxr-authored records, mirroring
//! the entity-section shapes proven valid in sxr-core's envelope_build_tests.

use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};
use sxr_core::clock::{ClockPort, SystemClock};
use sxr_core::envelope::{
    EnvelopeParts, build_envelope, new_envelope_id, runtime_scope, sxr_created_by,
    validate_record_kind_conformance,
};
use sxr_core::record_kinds::RecordKind;

use crate::ledger_io::RepoLedger;
use crate::state::SXR_VERSION;

/// Common boundary for sxr runtime records.
fn boundary(extra: Value) -> Value {
    let obj = extra.as_object().cloned().unwrap_or_default();
    let mut hashes = serde_json::Map::new();
    hashes.insert("included_sections".into(), json!([]));
    hashes.insert("excluded_sections".into(), json!([]));
    hashes.insert("known_omissions".into(), json!([]));
    hashes.insert("unknowns".into(), json!([]));
    hashes.insert("limitations".into(), json!(["sxr slice-1 runtime record"]));
    for (k, v) in obj {
        hashes.insert(k, v);
    }
    json!(hashes)
}

/// Build an envelope and validate kind conformance against the subject refs.
#[allow(clippy::too_many_arguments)]
pub fn build(
    kind: RecordKind,
    repo_id: &str,
    checkout_id: &str,
    filesystem_path: Option<&str>,
    git_commit: Option<&str>,
    run_id: Option<&str>,
    provenance_refs: Vec<String>,
    sections: Value,
    subject_refs: Vec<String>,
    content_hash: Option<String>,
    semantic_hash: Option<String>,
) -> Result<Value> {
    let parts = EnvelopeParts {
        envelope_id: new_envelope_id(),
        created_at: SystemClock.now_utc(),
        created_by: sxr_created_by(SXR_VERSION),
        record_kind: kind,
        scope: runtime_scope(
            repo_id,
            checkout_id,
            filesystem_path,
            git_commit,
            None,
            run_id,
        ),
        boundary_record: boundary(json!({})),
        completeness_status: "complete_for_declared_state",
        provenance_refs: if provenance_refs.is_empty() {
            vec!["ledger:gen:1".into()]
        } else {
            provenance_refs
        },
        lineage_refs: vec![],
        entity_sections: sections,
        content_hash,
        semantic_hash,
    };
    let envelope =
        build_envelope(&parts).map_err(|e| anyhow!("build envelope for {}: {e}", kind.as_str()))?;
    validate_record_kind_conformance(&kind, &envelope, &subject_refs)
        .map_err(|e| anyhow!("kind conformance for {}: {e}", kind.as_str()))?;
    Ok(envelope)
}

/// Build and append a record to the caller's open repo ledger.
#[allow(clippy::too_many_arguments)]
pub fn append(
    repo: &mut RepoLedger,
    kind: RecordKind,
    repo_id: &str,
    checkout_id: &str,
    filesystem_path: Option<&str>,
    git_commit: Option<&str>,
    run_id: Option<&str>,
    provenance_refs: Vec<String>,
    sections: Value,
    subject_refs: Vec<String>,
    idempotency_key: Option<String>,
) -> Result<sxr_core::LedgerPosition> {
    let envelope = build(
        kind,
        repo_id,
        checkout_id,
        filesystem_path,
        git_commit,
        run_id,
        provenance_refs,
        sections,
        subject_refs.clone(),
        None,
        None,
    )?;
    crate::ledger_io::append_record(repo, kind, idempotency_key, subject_refs, &envelope)
        .context("append envelope")
}
