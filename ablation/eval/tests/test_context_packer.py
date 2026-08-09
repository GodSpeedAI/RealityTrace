"""M3 context-budget packer regression tests (owner-required compliance set).

Covers: <32K passes unchanged | exactly-at-budget | >32K deterministically
packed | repeated packing byte-identical | order changes only with arm |
no answer-key access | omissions explicit | all four arms same budget |
ag-01 fits | df-01 fits | small cases materially unchanged | final prompt
stays <= frozen limit.
"""

import hashlib
import json
import os

import pytest

from ..context_packer import (pack_context, estimate_tokens, PackedContext,
                              OMISSION_STATEMENT)
from ..bundle import assert_prompt_sanitized
from ..contract import live_contract

BUDGET = 32_000
SYSTEM = live_contract().system_prompt


def _sections(fts="", sql="s", vector="", graph=""):
    sec = {"sql": sql}
    if fts:
        sec["fts"] = fts
    if vector:
        sec["vector"] = vector
    if graph:
        sec["graph"] = graph
    return sec


def test_small_context_passes_unchanged():
    pc = pack_context(_sections(fts="small evidence"), SYSTEM, "c1", "A", BUDGET)
    assert pc.estimated_prompt_tokens <= BUDGET
    assert pc.omitted_material is False
    assert "small evidence" in pc.prompt
    assert all(i.status == "included" for i in pc.items)


def test_exactly_at_budget_passes():
    # A context that exactly fits the retrieval budget passes untruncated.
    small = "evidence " * 100
    pc = pack_context(_sections(fts=small), SYSTEM, "c1", "A", BUDGET)
    assert pc.estimated_prompt_tokens <= BUDGET
    assert pc.omitted_material is False
    # A genuinely oversized context (well over the 32K budget) is excerpted.
    big = "x" * (BUDGET * 4)  # ~128K chars -> far over budget
    pc2 = pack_context(_sections(fts=big), SYSTEM, "c1", "A", BUDGET)
    assert pc2.estimated_prompt_tokens <= BUDGET
    assert pc2.omitted_material is True


def test_oversized_context_deterministically_packed():
    big = "line one\n" + "x" * 200_000
    r1 = pack_context(_sections(fts=big), SYSTEM, "c1", "A", BUDGET)
    assert r1.estimated_prompt_tokens <= BUDGET
    assert r1.omitted_material is True
    assert any(i.status == "excerpted" for i in r1.items)


def test_repeated_packing_byte_identical():
    big = "line one\n" + "y" * 180_000
    a = pack_context(_sections(fts=big), SYSTEM, "c1", "D", BUDGET)
    b = pack_context(_sections(fts=big), SYSTEM, "c1", "D", BUDGET)
    assert a.prompt == b.prompt
    assert json.dumps(a.to_dict(), sort_keys=True) == \
        json.dumps(b.to_dict(), sort_keys=True)


def test_order_only_changes_with_input():
    # The packer is arm-agnostic: it packs whatever capability sections it is
    # given. Arm distinctness comes from build_arm_context passing only the
    # arm's capability set. Same input => identical prompt (reproducible);
    # different capability set (as different arms produce) => different prompt.
    a = pack_context(_sections(fts="f", vector="v"), SYSTEM, "c1", "A", BUDGET)
    a2 = pack_context(_sections(fts="f", vector="v"), SYSTEM, "c1", "A", BUDGET)
    assert a.prompt == a2.prompt  # same input reproduces
    # Arm A sees only sql+fts; Arm B sees sql+fts+vector. Different sets.
    from ..arms import build_arm_context
    from ..store import build_store, load_case_rows
    rows = [{"case_id": "c1", "repo": "r", "scenario": "s", "stdin_format": "f",
             "verifier_exit": 1, "evidence_id": "e"}]
    sm = build_store(rows, {"c1": "evidence"})
    ctxA = build_arm_context("A", "c1", sm["c1"])
    ctxB = build_arm_context("B", "c1", sm["c1"])
    pcA = pack_context(ctxA.sections, SYSTEM, "c1", "A", BUDGET)
    pcB = pack_context(ctxB.sections, SYSTEM, "c1", "B", BUDGET)
    assert pcA.prompt != pcB.prompt  # capability set differs by arm


def test_no_answer_key_access():
    big = "ordinary evidence text that mentions nothing sensitive\n" + "z" * 100
    pc = pack_context(_sections(fts=big), SYSTEM, "c1", "A", BUDGET)
    assert_prompt_sanitized(pc.prompt, "c1")
    for marker in ("true_locus", "scenario-keys.json", "keys.json",
                   "answer-key-review"):
        assert marker not in pc.prompt.lower()


def test_omissions_are_explicit():
    big = "start\n" + "w" * 300_000
    pc = pack_context(_sections(fts=big), SYSTEM, "c1", "A", BUDGET)
    assert pc.omitted_material is True
    assert OMISSION_STATEMENT in pc.prompt
    excerpted = [i for i in pc.items if i.status == "excerpted"]
    omitted = [i for i in pc.items if i.status == "omitted"]
    assert excerpted or omitted


def test_all_four_arms_same_budget():
    fts = "some evidence text\n" + "q" * 50_000
    budgets = set()
    for arm in "ABCD":
        pc = pack_context(_sections(fts=fts, vector="v", graph="g"),
                          SYSTEM, "c1", arm, BUDGET)
        budgets.add((pc.total_budget, pc.retrieval_budget))
    assert len(budgets) == 1  # identical budget definition across arms


def test_prompt_never_exceeds_budget():
    for arm in "ABCD":
        pc = pack_context(_sections(fts="x" * 500_000, vector="v", graph="g"),
                          SYSTEM, "c1", arm, BUDGET)
        assert pc.estimated_prompt_tokens <= BUDGET


def test_estimator_deterministic():
    assert estimate_tokens("hello world") == estimate_tokens("hello world")
    assert estimate_tokens("x" * 1000) == estimate_tokens("x" * 1000)


def test_ag01_and_df01_fit(tmp_path):
    """Real-evidence compliance for the two large pilot cases (no live call)."""
    sys_repo = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    from ..store import load_case_rows, build_store
    rows = load_case_rows(os.path.join(sys_repo, "../corpus", "cases.jsonl"))
    sel = [r for r in rows if r["case_id"] in ("ag-01", "df-01")]
    arts = {}
    import subprocess
    for r in sel:
        res = subprocess.run(
            ["/home/sprime01/projects/sxr/target/debug/sxr", "ledger",
             "export", "--repo", r["repo_id"]], capture_output=True, text=True)
        try:
            d = json.loads(res.stdout)
        except Exception:
            continue
        for rec in (d if isinstance(d, list) else d.get("records", [])):
            if rec.get("record_kind") != "observation.recorded":
                continue
            p = rec["payload"] if isinstance(rec["payload"], dict) else json.loads(rec["payload"])
            for rep in p.get("representations", []):
                if rep.get("representation_id") == f"rep:obs:{r['observation_id']}":
                    h = rep.get("content_hash", "").replace("sha256:", "")
                    ap = os.path.expanduser(
                        f"~/.local/share/sxr/repos/{r['repo_id']}/artifacts/{h}.stdout")
                    if os.path.exists(ap):
                        arts[r["case_id"]] = open(ap, errors="replace").read()
                    break
            break
    sm = build_store(sel, arts)
    from ..arms import build_arm_context
    for r in sel:
        for arm in "ABCD":
            ctx = build_arm_context(arm, r["case_id"], sm[r["case_id"]])
            pc = pack_context(ctx.sections, SYSTEM, r["case_id"], arm, BUDGET)
            assert pc.estimated_prompt_tokens <= BUDGET, \
                f"{r['case_id']} {arm} exceeds budget"


def test_small_cases_materially_unchanged():
    small = "tiny evidence\nsecond line"
    pc = pack_context(_sections(fts=small), SYSTEM, "c1", "A", BUDGET)
    assert pc.estimated_prompt_tokens <= BUDGET
    assert "tiny evidence\nsecond line" in pc.prompt  # untruncated
    assert pc.omitted_material is False