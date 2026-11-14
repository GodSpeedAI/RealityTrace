//! `realitytrace observed ingest|derive` (WP-16): the CLI half over `sxr-observed`.
//! `derive` only reads the ledger. `ingest` appends `observation.recorded`
//! (accepted) or `ingest.rejected` (refused) and no other kind.

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::Path;
use sxr_core::record_kinds::RecordKind;
use sxr_ledger::ledger::{Ledger, LedgerConfig};
use sxr_observed::derive::{Options, Until, derive};
use sxr_observed::ingest::{
    EXTENSION_KEY, EventValidator, EvidenceIndex, INGEST_NOTE_KEY, REJECTION_NOTE_KEY,
    UnsignedOnly, classify,
};

use crate::state::{self, Ctx};
use crate::{envelopes, ledger_io};

fn sha_hex(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

/// Classify and record every envelope in `input` (NDJSON, one per line).
/// Returns the per-line results and whether any line was rejected.
pub fn ingest(ctx: &Ctx, repo_id: &str, input: &str) -> Result<(Value, bool)> {
    let enrollment = state::require_enrollment(ctx, repo_id)?;
    let mut repo = ledger_io::open_repo_ledger(ctx, repo_id)?;
    let evidence =
        EvidenceIndex::from_rows(&repo.ledger.read_all().map_err(|e| anyhow!("read: {e}"))?);
    let validator = EventValidator::new();
    let mut results = vec![];
    let mut any_rejected = false;
    for (n, line) in input
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
    {
        let candidate: Value =
            serde_json::from_str(line).unwrap_or_else(|_| Value::String(line.into()));
        match classify(&validator, &candidate, &evidence, &UnsignedOnly) {
            Ok(acc) => {
                let oid = format!("interaction:{}", &sha_hex(&acc.occurrence_id)[..24]);
                let run = acc.event["correlation"]["run_id"]
                    .as_str()
                    .unwrap_or("unscoped");
                let key = acc.event["declaration_key"].as_str().unwrap_or_default();
                let sections = json!({
                    "observations": [{"observation_id": oid, "scope": format!("run:{run}")}],
                    "extensions": {
                        EXTENSION_KEY: acc.event,
                        INGEST_NOTE_KEY: {"accepted": true, "signature_status": acc.signature.as_str(),
                                          "occurrence_id": acc.occurrence_id},
                    },
                });
                let subject_refs = vec![
                    format!("observation:{oid}"),
                    format!("run:{run}"),
                    format!("target:{key}"),
                ];
                let env = envelopes::build(
                    RecordKind::ObservationRecorded,
                    repo_id,
                    &enrollment.checkout_id,
                    None,
                    None,
                    None,
                    vec![],
                    sections,
                    subject_refs.clone(),
                    None,
                    None,
                )?;
                let pos = ledger_io::append_record(
                    &mut repo,
                    RecordKind::ObservationRecorded,
                    Some(sha_hex(&format!("interaction-event+{}", acc.occurrence_id))),
                    subject_refs,
                    &env,
                )?;
                results.push(json!({"line": n + 1, "status": "accepted", "occurrence_id": acc.occurrence_id,
                    "signature_status": acc.signature.as_str(), "position": ledger_io::ledger_position_json(&pos)}));
            }
            Err(rej) => {
                any_rejected = true;
                let h = rej
                    .candidate_sha256
                    .trim_start_matches("sha256:")
                    .to_owned();
                let sections = json!({
                    "representations": [{"representation_id": format!("representation:rejected:{h}"),
                        "representation_kind": "atomic", "representation_type": "rejected_envelope_candidate",
                        "validation_status": "invalid", "content_hash": rej.candidate_sha256}],
                    "states": [{"state_id": format!("state:ingest-rejected:{h}"), "state_type": "validation",
                        "scope": format!("ingest:{h}")}],
                    "extensions": {REJECTION_NOTE_KEY: {"code": rej.code.as_str(), "reason": rej.reason,
                        "candidate_sha256": rej.candidate_sha256, "hints": rej.hints.to_json()}},
                });
                let subject_refs = vec![
                    format!("representation:rejected:{h}"),
                    format!("ingest:{h}"),
                ];
                let env = envelopes::build(
                    RecordKind::IngestRejected,
                    repo_id,
                    &enrollment.checkout_id,
                    None,
                    None,
                    None,
                    vec![],
                    sections,
                    subject_refs.clone(),
                    None,
                    None,
                )?;
                let pos = ledger_io::append_record(
                    &mut repo,
                    RecordKind::IngestRejected,
                    Some(sha_hex(&format!(
                        "ingest-rejected+{h}+{}",
                        rej.code.as_str()
                    ))),
                    subject_refs,
                    &env,
                )?;
                results.push(
                    json!({"line": n + 1, "status": "rejected", "code": rej.code.as_str(),
                    "reason": rej.reason, "position": ledger_io::ledger_position_json(&pos)}),
                );
            }
        }
    }
    Ok((json!({"results": results}), any_rejected))
}

pub struct DeriveArgs<'a> {
    pub repo: &'a str,
    pub world: &'a str,
    pub scope: &'a str,
    pub episode: &'a str,
    pub kind: &'a str,
    pub until: Option<&'a str>,
    pub profile: &'a str,
    pub declared: Option<&'a Path>,
    pub coverage_manifest: Option<&'a Path>,
    pub independent_systems: &'a [String],
}

fn read_json(p: &Path) -> Result<Value> {
    serde_json::from_slice(&std::fs::read(p).with_context(|| format!("read {}", p.display()))?)
        .with_context(|| format!("parse {}", p.display()))
}

/// Derive the observed representation (read-only). Returns canonical JSON text.
pub fn derive_command(ctx: &Ctx, a: &DeriveArgs) -> Result<String> {
    state::require_enrollment(ctx, a.repo)?;
    let db = ctx.repo_dir(a.repo).join("ledger.db");
    if !db.is_file() {
        bail!(
            "ledger for repo {} not found at {}; refusing to treat a missing ledger as empty",
            a.repo,
            db.display()
        );
    }
    // The startup tail check is off: derive verifies the whole chain itself and
    // reports CHAIN_VERIFY_FAILED, the same code for every verification failure.
    let cfg = LedgerConfig {
        tail_verify_on_open: 0,
        ..LedgerConfig::default()
    };
    let rows = Ledger::open(&db, a.repo, cfg)
        .and_then(|l| l.read_all())
        .map_err(|e| anyhow!("open ledger: {e}"))?;
    let declared_keys = a
        .declared
        .map(|p| -> Result<BTreeSet<String>> {
            serde_json::from_value(read_json(p)?).context("--declared must be a JSON array of keys")
        })
        .transpose()?;
    let coverage_manifest = a
        .coverage_manifest
        .map(|p| -> Result<Vec<(String, String)>> {
            let v: Vec<Value> = serde_json::from_value(read_json(p)?)
                .context("--coverage-manifest must be a JSON array")?;
            v.iter()
                .map(
                    |r| match (r["emitter"].as_str(), r["event_kind"].as_str()) {
                        (Some(e), Some(k)) => Ok((e.to_owned(), k.to_owned())),
                        _ => bail!("--coverage-manifest rows are {{emitter, event_kind}}"),
                    },
                )
                .collect()
        })
        .transpose()?;
    let opts = Options {
        world_ref: a.world.into(),
        scope: a.scope.into(),
        episode_ref: a.episode.into(),
        kind: a.kind.into(),
        profile_id: a.profile.into(),
        until: a
            .until
            .map(Until::parse)
            .transpose()
            .map_err(|e| anyhow!("{e}"))?
            .unwrap_or(Until::Head),
        declared_keys,
        coverage_manifest,
        independent_systems: a.independent_systems.iter().cloned().collect(),
    };
    Ok(derive(&rows, &opts).map_err(|e| anyhow!("{e}"))?.canonical)
}
