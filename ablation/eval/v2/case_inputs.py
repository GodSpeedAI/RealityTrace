#!/usr/bin/env python3
"""Frozen per-case model-visible initial diagnostic state (`evidence-revision-001`).

Why this module exists
----------------------
Audit A-11 measured the historical chain under context amnesia: the initial
failure evidence was rendered in round 1 and never again, and the follow-up
scaffolds asked the model to choose a probe while showing it neither the failure
nor any observation. The v2 repair made state continuity a harness invariant
(`dstate.DiagnosticState`), but the *content* of the initial state was still
assembled ad hoc at execution time. A runner that derives the model-visible
question and failure evidence on the fly can silently change what the experiment
shows the model between arms, between reruns, or between machines.

So the initial state is frozen here, per case, and hash-bound:

    question            a frozen template instantiated with the case id ONLY
    failure_evidence    the recorded verifier stream, by ledger identity and
                        content hash
    initial_context     the four public capability sections (sql / fts / vector
                        / graph), which is the historical Arm-D context
    initial_evidence    the rendered `initial_context`, i.e. exactly the string
                        handed to `DiagnosticState.initial_evidence`

The map is ARM-INVARIANT by construction: nothing here takes an arm, a
disclosure policy, or a rendering mode. The frozen arm intervention is the only
thing permitted to differ between R0 and R4 on the same case, and
`assert_arm_invariant_initial_state` proves it rather than asserting it.

Answer-blindness
----------------
Every input is a pure function of public corpus material — `cases.jsonl` and the
recorded evidence artifact. `governed-keys.json`, `acquisition-keys.json`,
`scenario-keys.json` and `acquisition-hidden-meta.json` are never read here, and
`assert_no_answer_metadata` checks the built document for their field names.

That check passes. It is NOT the same as saying the inputs carry no answer
signal — see BLOCKER-01 in `executable_contract.py`. The corpus was constructed
by injecting NAMED faults, and the injected test's name survives into the public
verifier stream: 16 of the 60 corpus cases, and 7 of the 43 primary cases, carry
such a marker in the model-visible initial context. Each one maps to exactly one
true locus. That is recorded per case rather than cleaned, because cleaning a
corpus is a scientific change and is not authorized.

Offline
-------
`build_case_inputs` reads the sxr ledger through the local CLI; `load_case_inputs`
reads the frozen artifact and needs nothing but this repository. Execution uses
the frozen artifact. Neither path performs a model or provider call.
"""

from __future__ import annotations

import collections
import hashlib
import json
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))          # .../ablation
sys.path.insert(0, os.path.dirname(ROOT))

from eval.context_packer import (OMISSION_STATEMENT, PACKING_VERSION,  # noqa: E402
                                 pack_context)
from eval.store import (graph_section, load_case_rows, sql_section,  # noqa: E402
                        vector_section)
from eval.v2.controller import SYSTEM_PROMPT                          # noqa: E402

CASE_INPUT_VERSION = "case-input-v1"

#: The model-visible question. Frozen; instantiated with the case id and
#: nothing else, so it cannot vary with the arm, the locus, or the corpus.
QUESTION_TEMPLATE = (
    "Which failure locus does the recorded failure for case {case_id} belong "
    "to? Attribute the failure to exactly one locus from the vocabulary in the "
    "diagnostic state.")

#: The historical Arm-D retrieval profile: four capability sections composed
#: from public corpus material only (`eval/store.py`), packed by the FROZEN
#: `pack-v1` rule at the frozen 32,000-token total-input budget
#: (`eval/context_packer.py`).
#:
#: The packing rule is reused rather than reinvented on purpose. Inventing a
#: fresh truncation rule here would change what the model sees relative to the
#: historical chain, in a way that is invisible in the results and that nobody
#: adjudicated. `pack-v1` excerpts an over-long `fts` section by leading whole
#: lines and emits a neutral omission statement, so material is never dropped
#: silently.
INITIAL_CONTEXT_PROFILE = "arm-D-four-capability/store-v1/pack-v1"
SECTION_ORDER = ("sql", "fts", "vector", "graph")
CONTEXT_BUDGET_TOKENS = 32_000

CORPUS = os.path.join(ROOT, "corpus")
CASES_JSONL = os.path.join(CORPUS, "cases.jsonl")
FROZEN_ARTIFACT = os.path.join(ROOT, "analysis", "post-audit-v2",
                               "case-inputs-v1.json")

#: Field names that only ever appear in sealed evaluator material. Their
#: presence anywhere in a built document is a hard failure.
ANSWER_METADATA_FIELDS = (
    "true_locus", "original_locus", "discriminating_probes",
    "non_discriminating", "gate_infeasible", "gate_infeasible_reason",
    "answer_key", "answer key",
)

#: The corpus' scenarios were built by injecting a named fault, and the
#: injected test's NAME survives into the public verifier stream. This scanner
#: is a declared, public string property of the evidence — it reads no key file.
CONSTRUCTION_MARKER_RE = re.compile(r"test_corpus_scenario_[A-Za-z0-9_]+")

SXR_BIN = os.path.join(os.path.dirname(ROOT), "target", "debug", "sxr")
LEDGER_STORE = os.path.expanduser("~/.local/share/sxr/repos")


class CaseInputError(RuntimeError):
    """A case-input binding is missing, ambiguous, or answer-bearing."""


def _sha256(text: str) -> str:
    return hashlib.sha256(text.encode()).hexdigest()


def _canonical(value) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), default=str)


def construction_markers(text: str) -> list[str]:
    """Injected-fault names visible in a public artifact. Declared, not cleaned."""
    return sorted({m for m in CONSTRUCTION_MARKER_RE.findall(text)})


def render_initial_context(packed_sections: dict, omitted: bool) -> str:
    """Deterministic assembly of the packed capability sections.

    Fixed order; a section that `pack-v1` omitted entirely is still rendered,
    as an explicit empty marker, because an absent section is indistinguishable
    from lost context — the failure mode R-02 exists to prevent.
    """
    parts = [f"[{cap}]\n{packed_sections.get(cap) or '(omitted: context budget)'}"
             for cap in SECTION_ORDER]
    if omitted:
        parts.append(OMISSION_STATEMENT)
    return "\n\n".join(parts)


# ── build (needs the local ledger; offline, zero model calls) ────────────────

def _evidence_artifacts(rows) -> dict:
    """Resolve each case's recorded observation to its content-addressed bytes."""
    by_repo = collections.defaultdict(list)
    for row in rows:
        by_repo[row["repo_id"]].append(row)
    out = {}
    for repo_id, group in by_repo.items():
        result = subprocess.run([SXR_BIN, "ledger", "export", "--repo", repo_id],
                                capture_output=True, text=True)
        try:
            doc = json.loads(result.stdout)
        except json.JSONDecodeError as exc:
            raise CaseInputError(f"ledger export failed for {repo_id}") from exc
        records = doc if isinstance(doc, list) else doc.get("records", [])
        index = {}
        for record in records:
            if record.get("record_kind") != "observation.recorded":
                continue
            payload = record["payload"]
            if not isinstance(payload, dict):
                payload = json.loads(payload)
            for rep in payload.get("representations", []):
                rid = rep.get("representation_id", "")
                if rid.startswith("rep:obs:"):
                    index[rid] = rep.get("content_hash", "").replace("sha256:", "")
        for row in group:
            digest = index.get(f"rep:obs:{row['observation_id']}")
            if not digest:
                continue
            path = os.path.join(LEDGER_STORE, repo_id, "artifacts",
                                f"{digest}.stdout")
            if os.path.exists(path):
                with open(path, errors="replace") as handle:
                    out[row["case_id"]] = (handle.read(), digest, path)
    return out


def build_case_inputs(case_ids=None) -> dict:
    """Rebuild the binding from the ledger. Offline; no model or provider call."""
    rows = {row["case_id"]: row for row in load_case_rows(CASES_JSONL)}
    artifacts = _evidence_artifacts(rows.values())
    wanted = tuple(case_ids) if case_ids is not None else tuple(sorted(rows))

    cases = {}
    for case_id in wanted:
        row = rows[case_id]
        if case_id not in artifacts:
            raise CaseInputError(
                f"{case_id}: no recorded evidence artifact resolves from the "
                f"ledger; the initial state cannot be bound")
        evidence_text, content_hash, _path = artifacts[case_id]
        sections = {
            "sql": sql_section(row),
            "fts": evidence_text,
            "vector": vector_section(evidence_text),
            "graph": graph_section(evidence_text, row),
        }
        packed = pack_context(sections, SYSTEM_PROMPT, case_id, "D",
                              budget=CONTEXT_BUDGET_TOKENS)
        rendered = render_initial_context(packed.sections, packed.omitted_material)
        question = QUESTION_TEMPLATE.format(case_id=case_id)
        markers = construction_markers(rendered)
        cases[case_id] = {
            "case_id": case_id,
            "question_id": f"Q-{case_id}",
            "question": question,
            "question_sha256": _sha256(question),
            "failure_evidence": {
                "artifact_identity": row["evidence_id"],
                "observation_id": row["observation_id"],
                "repo_id": row["repo_id"],
                "ledger_content_sha256": content_hash,
                "bytes": len(evidence_text.encode()),
                "content_sha256": _sha256(evidence_text),
            },
            "initial_context": {
                "profile": INITIAL_CONTEXT_PROFILE,
                "packing_version": PACKING_VERSION,
                "sections": list(SECTION_ORDER),
                "source_section_sha256": {cap: _sha256(sections[cap])
                                          for cap in SECTION_ORDER},
                "packed_section_sha256": {cap: _sha256(packed.sections[cap])
                                          for cap in sorted(packed.sections)},
                "packing_record": packed.to_dict(),
                "material_omitted": packed.omitted_material,
                "rendered_sha256": _sha256(rendered),
                "bytes": len(rendered.encode()),
            },
            #: exactly the string handed to DiagnosticState.initial_evidence
            "initial_evidence": rendered,
            "initial_evidence_sha256": _sha256(rendered),
            "construction_markers": markers,
            "answer_bearing_marker_present": bool(markers),
            "public_metadata": {
                "repo": row["repo"],
                "scenario": row["scenario"],
                "stdin_format": row["stdin_format"],
                "verifier_exit": row["verifier_exit"],
            },
        }

    doc = {
        "schema": CASE_INPUT_VERSION,
        "experiment_id": "evidence-revision-001",
        "question_template": QUESTION_TEMPLATE,
        "question_template_sha256": _sha256(QUESTION_TEMPLATE),
        "initial_context_profile": INITIAL_CONTEXT_PROFILE,
        "section_order": list(SECTION_ORDER),
        "context_budget_tokens": CONTEXT_BUDGET_TOKENS,
        "packing_version": PACKING_VERSION,
        "arm_invariant": True,
        "arm_invariance_basis": (
            "nothing in this document is a function of an arm, a disclosure "
            "policy, or a rendering mode; the same record is handed to R0-R4"),
        "answer_metadata_fields_checked": list(ANSWER_METADATA_FIELDS),
        "construction_marker_pattern": CONSTRUCTION_MARKER_RE.pattern,
        "cases": cases,
        "n_cases": len(cases),
        "model_calls_performed": 0,
        "provider_calls_performed": 0,
    }
    doc["case_inputs_sha256"] = case_inputs_hash(doc)
    assert_no_answer_metadata(doc)
    return doc


def case_inputs_hash(doc: dict) -> str:
    """Identity of the complete case-input map, excluding its own hash field."""
    body = {k: v for k, v in doc.items() if k != "case_inputs_sha256"}
    return hashlib.sha256(_canonical(body).encode()).hexdigest()


def assert_no_answer_metadata(doc: dict) -> None:
    """No sealed evaluator field name may appear anywhere in the binding."""
    # The checklist itself names the forbidden fields; scan the payload only.
    body = {k: v for k, v in doc.items()
            if k not in ("answer_metadata_fields_checked",)}
    blob = _canonical(body).lower()
    hits = [f for f in ANSWER_METADATA_FIELDS if f.lower() in blob]
    if hits:
        raise CaseInputError(f"answer metadata in case inputs: {sorted(hits)}")


# ── load (needs nothing but this repository) ─────────────────────────────────

_CACHE: dict | None = None


def load_case_inputs(path: str = FROZEN_ARTIFACT) -> dict:
    """Load and verify the frozen binding. This is what execution must use."""
    global _CACHE
    if _CACHE is not None and path == FROZEN_ARTIFACT:
        return _CACHE
    if not os.path.isfile(path):
        raise CaseInputError(
            f"frozen case-input binding is absent: {path}. Rebuild it offline "
            f"with `python -m eval.v2.case_inputs freeze`.")
    with open(path) as handle:
        doc = json.load(handle)
    recomputed = case_inputs_hash(doc)
    if recomputed != doc.get("case_inputs_sha256"):
        raise CaseInputError("frozen case-input binding failed its own hash")
    assert_no_answer_metadata(doc)
    if path == FROZEN_ARTIFACT:
        _CACHE = doc
    return doc


def initial_state_for(case_id: str, doc: dict | None = None) -> dict:
    """The arm-invariant initial diagnostic state for one case.

    Returns exactly the two model-visible fields a runner may use, so a runner
    cannot reach past them into anything case-specific and unfrozen.
    """
    doc = doc or load_case_inputs()
    try:
        record = doc["cases"][case_id]
    except KeyError as exc:
        raise CaseInputError(f"{case_id} has no frozen initial state") from exc
    return {"question": record["question"],
            "initial_evidence": record["initial_evidence"],
            "binding_sha256": record["initial_evidence_sha256"]}


def assert_arm_invariant_initial_state(case_id: str, arms,
                                       doc: dict | None = None) -> dict:
    """Prove the same case yields a byte-identical initial state in every arm.

    Owner requirement: same case -> byte/semantic-equivalent initial Question
    and failure evidence across R0-R4, with only the frozen arm intervention
    differing. `initial_state_for` takes no arm, so invariance is structural;
    this renders it per arm through the real state object and compares bytes.
    """
    from .dstate import DiagnosticState
    doc = doc or load_case_inputs()
    base = initial_state_for(case_id, doc)
    digests = {}
    for name, spec in arms.items():
        state = DiagnosticState(
            question=base["question"], case_id=case_id,
            initial_evidence=base["initial_evidence"],
            action_schema="(schema is per-unit and not part of this check)",
            locus_vocabulary=(), max_rounds=spec.max_rounds,
            max_payment=spec.max_payment,
            budget_semantics=spec.budget_semantics,
            observation_rendering=spec.observation_rendering)
        digests[name] = {
            "question_sha256": _sha256(state.question),
            "initial_evidence_sha256": _sha256(state.initial_evidence),
        }
    distinct = {(_["question_sha256"], _["initial_evidence_sha256"])
                for _ in digests.values()}
    if len(distinct) != 1:
        raise CaseInputError(
            f"{case_id}: initial state is not arm-invariant: {digests}")
    return {"case_id": case_id, "arms": sorted(digests),
            "question_sha256": base["binding_sha256"] and
            digests[sorted(digests)[0]]["question_sha256"],
            "initial_evidence_sha256":
                digests[sorted(digests)[0]]["initial_evidence_sha256"],
            "arm_invariant": True}


def marker_report(doc: dict | None = None, case_ids=None) -> dict:
    """Which cases carry an injected-fault name in their public evidence."""
    doc = doc or load_case_inputs()
    ids = tuple(case_ids) if case_ids is not None else tuple(sorted(doc["cases"]))
    flagged = {c: doc["cases"][c]["construction_markers"]
               for c in ids if doc["cases"][c]["answer_bearing_marker_present"]}
    return {
        "n_cases": len(ids),
        "n_with_construction_marker": len(flagged),
        "cases": flagged,
        "pattern": doc["construction_marker_pattern"],
        "disposition": ("DECLARED, NOT CLEANED — see BLOCKER-01. Cleaning the "
                        "corpus would be a scientific change and is not "
                        "authorized."),
    }


def main(argv=None) -> int:
    argv = list(sys.argv[1:] if argv is None else argv)
    if argv[:1] == ["freeze"]:
        doc = build_case_inputs()
        os.makedirs(os.path.dirname(FROZEN_ARTIFACT), exist_ok=True)
        with open(FROZEN_ARTIFACT, "w") as handle:
            json.dump(doc, handle, indent=2, sort_keys=True, default=str)
        print(f"  wrote {os.path.relpath(FROZEN_ARTIFACT, ROOT)}  "
              f"({doc['n_cases']} cases)")
    else:
        doc = load_case_inputs()
    report = marker_report(doc)
    print(f"  case-input binding {doc['case_inputs_sha256'][:16]}  "
          f"cases={doc['n_cases']}  model_calls={doc['model_calls_performed']}")
    print(f"  construction markers: {report['n_with_construction_marker']}"
          f"/{report['n_cases']} cases (declared, not cleaned)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
