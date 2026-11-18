//! Read-only ledger commands never create a ledger. Before this, `ledger verify` on an unknown repo
//! or on an enrolled repo whose ledger.db was lost created an empty ledger and reported `ok: true`,
//! so total data loss passed an integrity check.
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_realitytrace"))
}

fn sxr(xdg: &Path, args: &[&str]) -> Output {
    Command::new(bin())
        .args(args)
        .env("XDG_DATA_HOME", xdg.join("data"))
        .env("XDG_STATE_HOME", xdg.join("state"))
        .env_remove("SXR_DAEMON_SOCKET")
        .output()
        .expect("run realitytrace")
}

fn git_repo(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("README"), "fixture\n").unwrap();
    for args in [
        vec!["init", "-q"],
        vec!["add", "-A"],
        vec![
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "init",
        ],
    ] {
        assert!(
            Command::new("git")
                .args(&args)
                .current_dir(dir)
                .status()
                .unwrap()
                .success()
        );
    }
}

fn enroll(root: &Path) -> String {
    let repo = root.join("repo");
    git_repo(&repo);
    let enrolled = sxr(root, &["repo", "enroll", repo.to_str().unwrap(), "--json"]);
    assert!(
        enrolled.status.success(),
        "{}",
        String::from_utf8_lossy(&enrolled.stderr)
    );
    serde_json::from_slice::<Value>(&enrolled.stdout).unwrap()["repo_id"]
        .as_str()
        .unwrap()
        .to_string()
}

const READ_ONLY: [&str; 3] = ["verify", "history", "export"];

#[test]
fn unknown_repo_is_refused_and_nothing_is_created() {
    let tmp = tempfile::tempdir().unwrap();
    for cmd in READ_ONLY {
        let out = sxr(
            tmp.path(),
            &["ledger", cmd, "--repo", "01NOTENROLLED", "--json"],
        );
        assert!(
            !out.status.success(),
            "ledger {cmd} on an unknown repo must fail"
        );
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("not enrolled"),
            "ledger {cmd}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    assert!(
        !tmp.path().join("data/sxr/repos/01NOTENROLLED").exists(),
        "a read-only command created a repo dir"
    );
}

#[test]
fn lost_ledger_is_refused_not_verified_as_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let repo_id = enroll(tmp.path());

    // Control: the intact ledger verifies and has entries.
    let ok = sxr(
        tmp.path(),
        &["ledger", "verify", "--repo", &repo_id, "--json"],
    );
    assert!(ok.status.success());
    let report: Value = serde_json::from_slice(&ok.stdout).unwrap();
    assert_eq!(report["ok"], true);
    assert!(report["verified_count"].as_u64().unwrap() >= 1, "{report}");

    // Lose the ledger (database and its WAL/SHM side files).
    let db = tmp
        .path()
        .join("data/sxr/repos")
        .join(&repo_id)
        .join("ledger.db");
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", db.display()));
    }
    for cmd in READ_ONLY {
        let out = sxr(tmp.path(), &["ledger", cmd, "--repo", &repo_id, "--json"]);
        assert!(
            !out.status.success(),
            "ledger {cmd} must fail on a lost ledger, got {}",
            String::from_utf8_lossy(&out.stdout)
        );
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("not found"),
            "ledger {cmd}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    assert!(!db.exists(), "a read-only command recreated the ledger");
}

#[test]
fn recorded_world_survives_ledger_append_and_export() {
    use sxr_core::chain::verify_entry_hashes;
    use sxr_core::envelope::{build_envelope, runtime_scope, validate_record_kind_conformance};
    use sxr_core::evidence_emit::{EvidenceEmission, emit_evidence_recorded};
    use sxr_core::evidence_record::{RecordContext, evidence_recorded_parts};
    use sxr_core::proof_ingest::ComparisonInputs;
    use sxr_ledger::ledger::{AppendOutcome, Ledger, LedgerConfig, PendingEntry};

    let tmp = tempfile::tempdir().unwrap();
    let repo_id = enroll(tmp.path());
    let world = format!("world:original@sha256:{}", "a".repeat(64));
    let settlement = format!("sha256:{}", "b".repeat(64));
    let emission = EvidenceEmission {
        local_model_sha256: &"c".repeat(64),
        local_world_ref: &world,
        work_request_id: "wr-1",
        affordance_id: "aff-1",
        proof_event_id: "0000000a-0000-4000-8000-000000000001",
        event_id: "0000000b-0000-4000-8000-000000000002",
        occurred_at: "2026-08-25T12:00:00Z",
        context_bundle_ref: None,
    };
    let e8 = emit_evidence_recorded(
        &ComparisonInputs {
            proof_producer: "swe_seed".into(),
            world_ref: world.clone(),
            proof_envelope_sha256: format!("sha256:{}", "d".repeat(64)),
            operational_settlement_ref: settlement.clone(),
            settlement_event_id: "0000000c-0000-4000-8000-000000000003".into(),
            expected_outcome: serde_json::json!({"status": "green"}),
            observed_effects: Some(serde_json::json!([{"status": "green"}])),
            referenced_settlement_sha256: Some(settlement),
        },
        &emission,
    )
    .unwrap();
    let (parts, subject_refs) = evidence_recorded_parts(
        &e8,
        RecordContext {
            envelope_id: e8["event_id"].as_str().unwrap().into(),
            created_at: e8["occurred_at"].as_str().unwrap().parse().unwrap(),
            created_by: "sxr test".into(),
            scope: runtime_scope(&repo_id, "checkout-test", None, None, None, None),
            provenance_refs: vec![e8["payload"]["proof_result_ref"].as_str().unwrap().into()],
            lineage_refs: vec![],
        },
    )
    .unwrap();
    let envelope = build_envelope(&parts).unwrap();
    validate_record_kind_conformance(&parts.record_kind, &envelope, &subject_refs).unwrap();

    let db = tmp
        .path()
        .join("data/sxr/repos")
        .join(&repo_id)
        .join("ledger.db");
    let mut ledger = Ledger::open(&db, &repo_id, LedgerConfig::default()).unwrap();
    let AppendOutcome::Committed { row, .. } = ledger
        .append(&PendingEntry {
            record_kind: parts.record_kind.as_str().into(),
            idempotency_key: e8["idempotency_key"].as_str().map(str::to_owned),
            subject_refs,
            payload: sxr_core::canonical_json::canonical_json(&envelope),
            entry_ulid: None,
            committed_at: None,
        })
        .unwrap()
    else {
        panic!("fresh evidence must commit");
    };
    let mut stored = ledger.read(row.append_ordinal).unwrap();
    assert!(ledger.verify_full().unwrap().complete);
    drop(ledger);

    let out = sxr(
        tmp.path(),
        &["ledger", "export", "--repo", &repo_id, "--json"],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    let exported = rows
        .iter()
        .find(|r| r["append_ordinal"].as_u64() == Some(row.append_ordinal))
        .unwrap();
    assert_eq!(exported["record_kind"], "evidence.recorded");
    assert_eq!(exported["entry_hash"], stored.entry_hash);
    assert_eq!(exported["payload"]["scope"]["world_ref"], world);

    // Editing only the world in the read-back envelope invalidates the stored hashes.
    let mut changed: Value = serde_json::from_str(&stored.payload).unwrap();
    changed["scope"]["world_ref"] =
        serde_json::json!(format!("world:other@sha256:{}", "a".repeat(64)));
    stored.payload = sxr_core::canonical_json::canonical_json(&changed);
    assert!(
        verify_entry_hashes(&stored)
            .unwrap_err()
            .contains("payload_hash mismatch")
    );
}
