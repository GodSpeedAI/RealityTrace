"""Frozen marker-neutral case inputs for evidence-revision-001 revision 002."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path

from .case_inputs import ANSWER_METADATA_FIELDS, CaseInputError
from .marker_neutralization import (MARKER_CLASS, NEUTRALIZATION_VERSION,
                                    assert_marker_free, neutralize_text)

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
SOURCE_ARTIFACT = ROOT / "analysis" / "post-audit-v2" / "case-inputs-v1.json"
FROZEN_ARTIFACT = ROOT / "analysis" / "post-audit-v2" / "case-inputs-v2.json"
CASE_INPUT_VERSION = "case-input-v2"


def _canonical(value) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), default=str)


def _sha256(text: str) -> str:
    return hashlib.sha256(text.encode()).hexdigest()


def case_inputs_hash(doc: dict) -> str:
    return _sha256(_canonical({k: v for k, v in doc.items()
                               if k != "case_inputs_sha256"}))


def build_case_inputs_v2(source: dict | None = None) -> tuple[dict, list[dict]]:
    source = source or json.loads(SOURCE_ARTIFACT.read_text())
    cases, manifest = {}, []
    for case_id, old in sorted(source["cases"].items()):
        record = json.loads(json.dumps(old))
        original = old["initial_evidence"]
        clean, hits = neutralize_text(original)
        assert_marker_free(clean)
        source_sha, neutral_sha = _sha256(original), _sha256(clean)
        applied = bool(hits)
        # Source identities remain provenance; only question/initial_evidence
        # are returned by initial_state_for and may cross the prompt boundary.
        record["initial_evidence"] = clean
        record["initial_evidence_sha256"] = neutral_sha
        record["initial_context"]["rendered_sha256"] = neutral_sha
        record.pop("construction_markers", None)
        record.pop("answer_bearing_marker_present", None)
        record["non_model_visible_covariates"] = {
            "source_marker_present": applied,
            "marker_neutralization_applied": applied,
            "marker_type": MARKER_CLASS if applied else None,
        }
        record["neutralization_provenance"] = {
            "source_case_input_sha256": source_sha,
            "neutralized_case_input_sha256": neutral_sha,
            "transformation_version": NEUTRALIZATION_VERSION,
        }
        cases[case_id] = record
        if applied:
            manifest.append({
                "artifact_kind": "case_initial_evidence",
                "source_artifact_identity": old["failure_evidence"]["artifact_identity"],
                "case_id": case_id,
                "source_sha256": source_sha,
                "neutralized_sha256": neutral_sha,
                "exact_transformation": "replace every full injected-test identifier with test_observed_failure",
                "marker_class": MARKER_CLASS,
                "replacement": "test_observed_failure",
                "occurrences": len(hits),
                "source_identifiers": sorted({h["source_identifier"] for h in hits}),
                "key_blindness_proof": "neutralize_text accepts text only and imports no corpus key/hidden-metadata module",
                "semantic_preservation_rationale": "only the benchmark-manufacturing test identifier changes; exit status, trace, paths, messages, and behavioral evidence remain byte-identical",
            })
    doc = {
        "schema": CASE_INPUT_VERSION,
        "experiment_id": source["experiment_id"],
        "supersedes": {"schema": source["schema"],
                       "case_inputs_sha256": source["case_inputs_sha256"],
                       "artifact": "case-inputs-v1.json"},
        "transformation_version": NEUTRALIZATION_VERSION,
        "affected_cases": [x["case_id"] for x in manifest],
        "n_cases": len(cases),
        "arm_invariant": True,
        "arm_invariance_basis": "one case record is used unchanged for R0-R4",
        "model_visible_fields": ["question", "initial_evidence"],
        "non_model_visible_covariates": ["source_marker_present", "marker_neutralization_applied", "marker_type"],
        "answer_metadata_fields_checked": list(ANSWER_METADATA_FIELDS),
        "cases": cases,
        "model_calls_performed": 0,
        "provider_calls_performed": 0,
    }
    doc["case_inputs_sha256"] = case_inputs_hash(doc)
    return doc, manifest


def load_case_inputs(path: Path = FROZEN_ARTIFACT) -> dict:
    doc = json.loads(Path(path).read_text())
    if doc.get("schema") != CASE_INPUT_VERSION:
        raise CaseInputError("wrong revised case-input schema")
    if case_inputs_hash(doc) != doc.get("case_inputs_sha256"):
        raise CaseInputError("revised case-input binding failed its hash")
    for record in doc["cases"].values():
        assert_marker_free(record["question"])
        assert_marker_free(record["initial_evidence"])
    return doc


def initial_state_for(case_id: str, doc: dict | None = None) -> dict:
    doc = doc or load_case_inputs()
    try:
        record = doc["cases"][case_id]
    except KeyError as exc:
        raise CaseInputError(f"{case_id} has no revised frozen initial state") from exc
    return {"question": record["question"],
            "initial_evidence": record["initial_evidence"],
            "binding_sha256": record["initial_evidence_sha256"]}


def covariates_for(case_id: str, doc: dict | None = None) -> dict:
    doc = doc or load_case_inputs()
    return dict(doc["cases"][case_id]["non_model_visible_covariates"])
