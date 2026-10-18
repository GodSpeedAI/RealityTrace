use serde_json::json;
use sxr_core::clock::FixedClock;
use sxr_core::embedded_schema::EnvelopeSchemaValidator;
use sxr_core::envelope::{
    EnvelopeParts, build_envelope, new_envelope_id, runtime_scope, sxr_created_by,
    validate_record_kind_conformance,
};
use sxr_core::record_kinds::RecordKind;

fn parts(kind: RecordKind) -> EnvelopeParts {
    EnvelopeParts {
        envelope_id: new_envelope_id(),
        created_at: FixedClock(
            chrono::DateTime::parse_from_rfc3339("2026-08-07T12:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        )
        .0,
        created_by: sxr_created_by("0.1.0-test"),
        record_kind: kind,
        scope: runtime_scope("repo-test", "checkout-test", None, None, None, None),
        boundary_record: json!({
            "included_sections": ["states", "transformations"],
            "excluded_sections": [],
            "known_omissions": [],
            "unknowns": [],
            "limitations": ["test envelope"]
        }),
        completeness_status: "complete_for_declared_state",
        provenance_refs: vec!["ledger:gen-test:1".into()],
        lineage_refs: vec![],
        entity_sections: json!({
            "states": [{"state_id": "state:1", "state_type": "repository", "scope": "repo:repo-test"}],
            "transformations": [{"transformation_id": "tx:1", "transformation_type": "enrollment"}]
        }),
        content_hash: None,
        semantic_hash: None,
    }
}

fn subject_refs(kind: RecordKind) -> Vec<String> {
    match kind {
        RecordKind::RepoEnrolled | RecordKind::RepoRelocated | RecordKind::RepoForgotten => {
            vec!["repo:repo-test".into(), "checkout:checkout-test".into()]
        }
        RecordKind::SemanticSnapshot => vec![
            "snapshot:snap-1".into(),
            "repo:repo-test".into(),
            "checkout:checkout-test".into(),
            "checkpoint:sha256:aa".into(),
        ],
        RecordKind::RunStarted => vec![
            "run:run-1".into(),
            "repo:repo-test".into(),
            "checkout:checkout-test".into(),
            "snapshot:snap-1".into(),
            "target:concept:abc".into(),
        ],
        RecordKind::RunFinished => vec!["run:run-1".into(), "target:concept:abc".into()],
        RecordKind::QuestionDeclared => vec!["question:q-1".into(), "target:concept:abc".into()],
        RecordKind::ThresholdDeclared => vec!["threshold:t-1".into(), "question:q-1".into()],
        RecordKind::PlanCreated => vec![
            "plan:p-1".into(),
            "question:q-1".into(),
            "threshold:t-1".into(),
            "snapshot:snap-1".into(),
            "target:concept:abc".into(),
        ],
        RecordKind::ObservationRecorded => {
            vec![
                "observation:o-1".into(),
                "run:run-1".into(),
                "target:concept:abc".into(),
            ]
        }
        RecordKind::EvidenceCreated => vec![
            "evidence:e-1".into(),
            "question:q-1".into(),
            "target:concept:abc".into(),
        ],
        RecordKind::SettlementRegistered => vec![
            "settlement:s-1".into(),
            "question:q-1".into(),
            "transformation:tx-1".into(),
            "snapshot:snap-1".into(),
        ],
        RecordKind::IngestRejected => vec!["representation:rep-1".into(), "ingest:stdin".into()],
        RecordKind::LedgerGenerationStarted => {
            vec!["history:h-1".into(), "ledger:gen-1".into()]
        }
        RecordKind::LedgerDestructiveRecovery => vec![
            "history:h-1".into(),
            "ledger:gen-9".into(),
            "generation:gen-8".into(),
        ],
        RecordKind::SemanticDiff | RecordKind::SemanticIdentityDrift => {
            vec![
                "snapshot:s1".into(),
                "snapshot:s2".into(),
                "declaration:concept:abc".into(),
            ]
        }
        RecordKind::VerificationContractImported => vec![
            "contract:ct-1".into(),
            "semantic:sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .into(),
            "obligation:ob-1".into(),
            "question:q-1".into(),
            "declaration:concept:abc".into(),
        ],
        RecordKind::ClaimMade => vec![
            "claim:c-1".into(),
            "question:q-1".into(),
            "target:concept:abc".into(),
        ],
        RecordKind::RepresentationRecorded => {
            vec!["representation:rep-1".into(), "subject:subject-1".into()]
        }
        RecordKind::DebtRecorded => vec!["debt:d-1".into(), "settlement:s-1".into()],
        RecordKind::ResidualComputed => vec![
            "residual:r-1".into(),
            "question:q-1".into(),
            "snapshot:s1".into(),
            "target:concept:abc".into(),
        ],
        RecordKind::DiagnosticHypothesis => vec![
            "hypothesis:h-1".into(),
            "question:q-2".into(),
            "residual:r-1".into(),
        ],
        RecordKind::DiagnosticSettled => vec![
            "settlement:s-2".into(),
            "question:q-2".into(),
            "hypothesis:h-1".into(),
        ],
        RecordKind::ProposalRaised => vec![
            "proposal:p-1".into(),
            "locus:concept:abc".into(),
            "target:concept:abc".into(),
        ],
        RecordKind::ConformanceClaimed => vec!["target:concept:abc".into(), "claim:c-3".into()],
        RecordKind::EvidenceRecorded => recorded_parts().1,
    }
}

/// §5.3b entity sections for each kind the test emits.
fn sections_for(kind: RecordKind) -> serde_json::Value {
    match kind {
        RecordKind::RepoEnrolled | RecordKind::RepoRelocated => json!({
            "states": [{"state_id": "state:1", "state_type": "repository", "scope": "repo:repo-test"}],
            "transformations": [{"transformation_id": "tx:1", "transformation_type": "enrollment"}]
        }),
        RecordKind::RepoForgotten => json!({
            "transformations": [{"transformation_id": "tx:2", "transformation_type": "forget"}]
        }),
        RecordKind::SemanticSnapshot => json!({
            "states": [{"state_id": "state:2", "state_type": "declared_model", "scope": "checkpoint:sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}],
            "representations": [{
                "representation_id": "representation:sha256:aa",
                "representation_kind": "atomic",
                "representation_type": "domainforge_canonical_d",
                "content_hash": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "semantic_hash": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            }]
        }),
        RecordKind::RunStarted => json!({
            "transformations": [{"transformation_id": "tx:3", "transformation_type": "run_start"}],
            "questions": [{"question_id": "q-1", "question_form": "is_true"}]
        }),
        RecordKind::RunFinished => json!({
            "transformations": [{"transformation_id": "tx:4", "transformation_type": "run_finish"}],
            "states": [{"state_id": "state:3", "state_type": "run", "scope": "run:run-1"}]
        }),
        RecordKind::QuestionDeclared => json!({
            "questions": [{"question_id": "q-1", "question_form": "is_true", "question_type": "declared_semantics"}]
        }),
        RecordKind::ThresholdDeclared => json!({
            "states": [{"state_id": "state:4", "state_type": "settlement_threshold", "scope": "question:q-1"}]
        }),
        RecordKind::PlanCreated => json!({
            "representations": [{"representation_id": "representation:plan-1", "representation_kind": "atomic", "representation_type": "verification_plan"}],
            "questions": [{"question_id": "q-1", "question_form": "is_true"}]
        }),
        RecordKind::ObservationRecorded => json!({
            "observations": [{"observation_id": "o-1", "scope": "run:run-1"}],
            "representations": [{"representation_id": "representation:o-1-out", "representation_kind": "atomic", "representation_type": "verifier_stdout"}]
        }),
        RecordKind::EvidenceCreated => json!({
            "evidence": [{
                "evidence_id": "e-1",
                "evidence_type": "test_result",
                "question_ref": "q-1",
                "target_entity_ref": "concept:abc",
                "representation_ref": "representation:o-1-out",
                "relevance": "high",
                "reliability": "medium",
                "direction": "supports",
                "confidence_impact": "increases"
            }]
        }),
        RecordKind::SettlementRegistered => json!({
            "settlements": [{
                "settlement_id": "s-1",
                "settlement_status": "settled",
                "transformation_ref": "tx-1",
                "question_ref": "q-1",
                "threshold_ref": "t-1"
            }]
        }),
        RecordKind::IngestRejected => json!({
            "representations": [{"representation_id": "representation:bad-1", "representation_kind": "atomic", "representation_type": "rejected_envelope_candidate", "validation_status": "invalid"}],
            "states": [{"state_id": "state:5", "state_type": "validation", "scope": "ingest:stdin"}]
        }),
        RecordKind::LedgerGenerationStarted => json!({
            "states": [{"state_id": "state:6", "state_type": "ledger_generation", "scope": "ledger:gen-1"}]
        }),
        RecordKind::LedgerDestructiveRecovery => json!({
            "transformations": [{"transformation_id": "tx:5", "transformation_type": "destructive_recovery", "scope": "ledger:gen-9"}]
        }),
        RecordKind::SemanticDiff => json!({
            "states": [{"state_id": "state:7", "state_type": "difference", "scope": "snapshot:s1"}],
            "relations": [{"relation_id": "rel:1"}]
        }),
        RecordKind::SemanticIdentityDrift => json!({
            "states": [{"state_id": "state:8", "state_type": "identity_binding_difference", "scope": "snapshot:s1"}]
        }),
        RecordKind::VerificationContractImported => json!({
            "questions": [{"question_id": "q-1", "question_form": "is_true"}],
            "claims": [{"claim_id": "c-1", "claim_type": "obligation", "subject_ref": "concept:abc"}],
            "representations": [{"representation_id": "representation:contract-1", "representation_kind": "atomic", "representation_type": "verification_contract"}]
        }),
        RecordKind::ClaimMade => json!({
            "claims": [{"claim_id": "c-1", "claim_type": "assertion", "subject_ref": "concept:abc", "question_refs": ["q-1"]}]
        }),
        RecordKind::RepresentationRecorded => json!({
            "representations": [{"representation_id": "representation:rep-1", "representation_kind": "atomic", "representation_type": "artifact", "content_hash": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}],
            "states": [{"state_id": "state:9", "state_type": "represented_subject", "scope": "subject:subject-1"}]
        }),
        RecordKind::DebtRecorded => json!({
            "settlements": [{"settlement_id": "s-1", "settlement_status": "settled_with_debt", "transformation_ref": "tx-1", "question_ref": "q-1", "threshold_ref": "t-1"}],
            "debt": [{"debt_id": "d-1", "settlement_ref": "s-1"}]
        }),
        RecordKind::ResidualComputed => json!({
            "states": [{"state_id": "state:10", "state_type": "residual", "scope": "question:q-1"}],
            "relations": [{"relation_id": "rel:2"}]
        }),
        RecordKind::DiagnosticHypothesis => json!({
            "questions": [{"question_id": "q-2", "question_form": "which_hypothesis"}],
            "claims": [{"claim_id": "c-2", "claim_type": "hypothesis", "subject_ref": "concept:abc"}]
        }),
        RecordKind::DiagnosticSettled => json!({
            "settlements": [{"settlement_id": "s-2", "settlement_status": "settled", "transformation_ref": "tx-6", "question_ref": "q-2", "threshold_ref": "t-2"}],
            "claims": [{"claim_id": "c-2", "claim_type": "hypothesis", "subject_ref": "concept:abc"}]
        }),
        RecordKind::ProposalRaised => json!({
            "transformations": [{"transformation_id": "tx:7", "transformation_type": "proposed_change", "scope": "locus:concept:abc"}],
            "questions": [{"question_id": "q-3", "question_form": "should_change"}]
        }),
        RecordKind::EvidenceRecorded => recorded_parts().0.entity_sections,
        RecordKind::ConformanceClaimed => json!({
            "conformance": [{"conformance_id": "conf-1", "evaluated_entity_refs": ["concept:abc"]}],
            "claims": [{"claim_id": "c-3", "claim_type": "conformance", "subject_ref": "concept:abc"}]
        }),
    }
}

/// AC-SXR-003 core loop: every registered kind builds a schema-valid envelope
/// with its mapped envelope_kind and required subject refs.
#[test]
fn every_record_kind_builds_a_conformant_schema_valid_envelope() {
    let validator = EnvelopeSchemaValidator::new();
    for kind in RecordKind::ALL {
        let mut parts = parts(*kind);
        parts.entity_sections = sections_for(*kind);
        let envelope = build_envelope(&parts).unwrap_or_else(|e| panic!("{}: {e}", kind));
        validator
            .validate(&envelope)
            .unwrap_or_else(|errs| panic!("{} failed schema: {:?}", kind, errs));
        validate_record_kind_conformance(kind, &envelope, &subject_refs(*kind))
            .unwrap_or_else(|e| panic!("{} failed conformance: {e}", kind));
    }
}

/// AC-SXR-003: wrong-kind pairings fail.
#[test]
fn wrong_envelope_kind_pairing_is_rejected() {
    for kind in RecordKind::ALL {
        let mut parts = parts(*kind);
        parts.entity_sections = sections_for(*kind);
        let mut envelope = build_envelope(&parts).unwrap();
        // Force a wrong envelope_kind.
        let wrong = if kind.def().envelope_kind == "semantic_snapshot" {
            "execution_trace"
        } else {
            "semantic_snapshot"
        };
        envelope["envelope_kind"] = json!(wrong);
        let err =
            validate_record_kind_conformance(kind, &envelope, &subject_refs(*kind)).unwrap_err();
        assert!(
            format!("{err}").contains("envelope_kind mismatch"),
            "{}: {err}",
            kind
        );
    }
}

/// AC-SXR-003: missing required subject refs fail.
#[test]
fn missing_required_subject_ref_is_rejected() {
    for kind in RecordKind::ALL {
        let mut parts = parts(*kind);
        parts.entity_sections = sections_for(*kind);
        let envelope = build_envelope(&parts).unwrap();
        let err = validate_record_kind_conformance(kind, &envelope, &[]).unwrap_err();
        assert!(
            format!("{err}").contains("required subject ref missing"),
            "{}: {err}",
            kind
        );
    }
}

#[test]
fn envelope_without_entity_sections_is_rejected() {
    let mut parts = parts(RecordKind::RepoEnrolled);
    parts.entity_sections = json!({});
    assert!(matches!(
        build_envelope(&parts),
        Err(sxr_core::envelope::EnvelopeError::NoEntitySections)
    ));
}

#[test]
fn extensions_section_is_an_object_per_schema() {
    let mut parts = parts(RecordKind::RepoEnrolled);
    parts.entity_sections["extensions"] = json!({"caller": {"id": "c-1"}});
    let envelope = build_envelope(&parts).expect("object extensions validate");
    assert_eq!(envelope["extensions"], json!({"caller": {"id": "c-1"}}));
    parts.entity_sections["extensions"] = json!([{"caller": 1}]);
    assert!(matches!(
        build_envelope(&parts),
        Err(sxr_core::envelope::EnvelopeError::ExtensionsNotObject)
    ));
}

#[test]
fn unknown_boundary_key_is_rejected() {
    let mut parts = parts(RecordKind::RepoEnrolled);
    parts.boundary_record = json!({"made_up_key": true});
    let err = build_envelope(&parts).unwrap_err();
    assert!(format!("{err}").contains("boundary_record key"));
}

#[test]
fn validation_status_is_always_validated() {
    for kind in RecordKind::ALL {
        let mut parts = parts(*kind);
        parts.entity_sections = sections_for(*kind);
        let envelope = build_envelope(&parts).unwrap();
        assert_eq!(envelope["validation_status"], "validated", "{}", kind);
        assert!(envelope.get("conformance_status").is_none(), "{}", kind);
    }
}

#[test]
fn envelope_id_and_created_at_vary_but_shape_is_stable() {
    let a = parts(RecordKind::QuestionDeclared);
    let b = parts(RecordKind::QuestionDeclared);
    let ea = build_envelope(&a).unwrap();
    let eb = build_envelope(&b).unwrap();
    assert_ne!(ea["envelope_id"], eb["envelope_id"]);
    assert_eq!(ea["cep_version"], eb["cep_version"]);
    assert_eq!(ea["envelope_version"], eb["envelope_version"]);
    assert_eq!(ea["envelope_kind"], eb["envelope_kind"]);
}

/// The production `evidence.recorded` mapping of a fixed E8.
fn recorded_parts() -> (sxr_core::envelope::EnvelopeParts, Vec<String>) {
    let e8 = json!({
        "schema_version": "v1", "event_id": "0000000b-0000-4000-8000-0000000c0001",
        "source_agent": "realitytrace", "event_type": "EvidenceRecorded",
        "occurred_at": "2026-08-07T12:00:00Z", "idempotency_key": "a".repeat(64),
        "payload": {
            "work_request_id": "wr-1", "affordance_id": "aff-1",
            "proof_result_ref": format!("sha256:{}", "b".repeat(64)),
            "operational_settlement_ref": format!("sha256:{}", "c".repeat(64)),
            "difference": {"verdict": "divergent", "expected_sha256": "d".repeat(64),
                           "observed_sha256": "e".repeat(64)}
        },
        "provenance": {"origin": "realitytrace", "chain": ["caused_by:s-1", "caused_by:p-1"]}
    });
    let p = parts(RecordKind::EvidenceRecorded);
    sxr_core::evidence_record::evidence_recorded_parts(
        &e8,
        sxr_core::evidence_record::RecordContext {
            envelope_id: p.envelope_id,
            created_at: p.created_at,
            created_by: p.created_by,
            scope: p.scope,
            provenance_refs: p.provenance_refs,
            lineage_refs: p.lineage_refs,
        },
    )
    .unwrap()
}

/// CEP-0008 §34/§52: a declared omission is marked in `omission_status`;
/// without one the status says so. Never a silent "no omissions".
#[test]
fn omission_status_follows_declared_known_omissions() {
    let mut parts = parts(RecordKind::RepoEnrolled);
    parts.entity_sections = sections_for(RecordKind::RepoEnrolled);
    assert_eq!(
        build_envelope(&parts).unwrap()["omission_status"],
        "no_known_omissions"
    );
    parts.boundary_record["known_omissions"] = json!(["checkout history before enrollment"]);
    assert_eq!(
        build_envelope(&parts).unwrap()["omission_status"],
        "known_omissions_recorded"
    );
}
