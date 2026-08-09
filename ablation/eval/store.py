"""Retrieval store: the four capability sections, composed honestly from the
recorded corpus evidence (ledger artifacts + public case metadata).

    sql    — structured case row (public metadata: repo, scenario, format,
             exit code; evidence ids) — the run's own record
    fts    — full text of the recorded evidence artifact (verifier stream)
    vector — deterministic embedding fingerprints over the evidence text
             (rebuild-deterministic: pure function of the artifact bytes)
    graph  — typed node/edge summary derived from the evidence stream
             (failure signature -> evidence -> repo/locus-adjacent nodes)

All sections are pure functions of public corpus material; nothing here reads
keys.json, scenario-keys.json, or any locus label.
"""

from __future__ import annotations

import hashlib
import json
import os
import re

SECTION_NAMES = ("sql", "fts", "vector", "graph")


def _artifact_path(case_row: dict, artifact_root: str) -> str | None:
    # artifact_root/<repo_id>/artifacts/<sha>.stdout — resolved via ledger
    # content hash; the corpus build stores artifacts under the repo id.
    rid = case_row["repo_id"]
    d = os.path.join(artifact_root, rid, "artifacts")
    if not os.path.isdir(d):
        return None
    # exact sha unknown here; match by observation marker is not possible, so
    # callers pass the artifact file explicitly via build_store(artifacts=...)
    return None


def _tokens(text: str) -> list[str]:
    return re.findall(r"[a-z0-9_]+", text.lower())


def sql_section(case_row: dict) -> str:
    """Structured row: public metadata only (no loci, no answers)."""
    return json.dumps({
        "case_id": case_row["case_id"],
        "repo": case_row["repo"],
        "scenario": case_row["scenario"],
        "stdin_format": case_row["stdin_format"],
        "verifier_exit": case_row["verifier_exit"],
        "evidence_id": case_row["evidence_id"],
    }, sort_keys=True)


def fts_section(evidence_text: str) -> str:
    """Full-text: the recorded verifier stream (public evidence)."""
    return evidence_text


def vector_section(evidence_text: str) -> str:
    """Deterministic embedding fingerprints over the evidence vocabulary.

    Pure function of the artifact bytes: identical evidence => identical
    vectors across builds. No network, no daemon (frozen §7.4 fallback).
    """
    tokens = _tokens(evidence_text)
    buckets: dict[int, int] = {}
    for tok in tokens:
        h = hashlib.sha256(tok.encode()).digest()
        dim = int.from_bytes(h[:4], "big") % 64
        sign = 1 if h[4] % 2 == 0 else -1
        buckets[dim] = buckets.get(dim, 0) + sign
    vec = [buckets.get(d, 0) for d in range(64)]
    norm = sum(v * v for v in vec) ** 0.5 or 1.0
    return "vector64=" + ",".join(str(round(v / norm, 6)) for v in vec)


def graph_section(evidence_text: str, case_row: dict) -> str:
    """Typed node/edge summary derived from the evidence stream.

    Nodes: case, repo, evidence artifact, failure-signature tokens.
    Edges: case->repo (belongs_to), case->artifact (recorded_evidence),
    artifact->signature tokens (manifests). Deterministic from inputs.
    """
    sig_tokens = _tokens(evidence_text)
    top = sorted(set(sig_tokens), key=lambda t: -sig_tokens.count(t))[:12]
    lines = [
        f"node case:{case_row['case_id']}",
        f"node repo:{case_row['repo']}",
        f"node artifact:{case_row['evidence_id']}",
    ]
    lines.append(f"edge {case_row['case_id']} --belongs_to--> {case_row['repo']}")
    lines.append(
        f"edge {case_row['case_id']} --recorded_evidence--> {case_row['evidence_id']}")
    for t in top:
        lines.append(f"node signature:{t}")
        lines.append(
            f"edge {case_row['evidence_id']} --manifests--> signature:{t}")
    return "\n".join(lines)


def build_store(case_rows: list[dict], artifacts: dict[str, str]) -> dict[str, dict[str, str]]:
    """Store keyed by case_id -> capability -> section text.

    `artifacts`: case_id -> raw evidence artifact text (from the ledger CAS).
    Pure and deterministic; consumed by arms.build_arm_context.
    """
    store: dict[str, dict[str, str]] = {}
    for row in case_rows:
        cid = row["case_id"]
        ev = artifacts.get(cid, "")
        store[cid] = {
            "sql": sql_section(row),
            "fts": fts_section(ev),
            "vector": vector_section(ev),
            "graph": graph_section(ev, row),
        }
    return store


def load_case_rows(cases_path: str) -> list[dict]:
    """Public corpus metadata from cases.jsonl (never keys)."""
    rows = []
    for line in open(cases_path):
        line = line.strip()
        if line:
            rows.append(json.loads(line))
    return rows
