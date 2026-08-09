"""Corpus integrity regression tests (repair milestone + seal invariants)."""

import hashlib
import json
import os

import pytest

CORPUS = os.path.join(os.path.dirname(__file__), "..", "..", "corpus")


@pytest.fixture
def rows():
    cases = os.path.join(CORPUS, "cases.jsonl")
    return [json.loads(l) for l in open(cases) if l.strip()]


def test_sixty_unique_cases(rows):
    ids = [r["case_id"] for r in rows]
    assert len(rows) == 60
    assert len(set(ids)) == 60


def test_repairs_preserve_superseded_evidence():
    path = os.path.join(CORPUS, "repairs.jsonl")
    reps = [json.loads(l) for l in open(path) if l.strip()]
    assert len(reps) == 3
    ids = {r["case_id"] for r in reps}
    assert ids == {"df-04", "df-11", "df-12"}
    for r in reps:
        assert r["type"] == "post-seal-evidence-repair"
        assert r["superseded"]["run_id"] != r["recorded"]["run_id"]
        assert r["recorded"]["verifier_exit"] != 0


def test_repaired_cases_are_failures(rows):
    for c, expected_exit in (("df-04", 101), ("df-11", 1), ("df-12", 1)):
        row = next(r for r in rows if r["case_id"] == c)
        assert row["verifier_exit"] == expected_exit
        # repaired evidence ids must match the repairs.jsonl "recorded" ids
        for l in open(os.path.join(CORPUS, "repairs.jsonl")):
            rep = json.loads(l)
            if rep["case_id"] == c:
                assert row["run_id"] == rep["recorded"]["run_id"]
                assert row["observation_id"] == rep["recorded"]["observation_id"]
                assert row["evidence_id"] == rep["recorded"]["evidence_id"]


def test_sealed_key_hash_matches_manifest():
    manifest = open(os.path.join(CORPUS, "keys.sha256")).read().strip()
    stored_hash = manifest.split()[0]
    actual = hashlib.sha256(
        open(os.path.join(CORPUS, "keys.json"), "rb").read()).hexdigest()
    assert stored_hash == actual
    assert actual == "10f677535412ef52b3150ca3c28865aec560da970ac2ff27c73b50f40b5c8e17"


def test_key_files_are_gitignored():
    gi = open(os.path.join(CORPUS, ".gitignore")).read()
    assert "keys.json" in gi
    assert "scenario-keys.json" in gi


def test_unknown_locus_cases_are_eight():
    keys = json.load(open(os.path.join(CORPUS, "keys.json")))
    unknown = sorted(c for c, v in keys.items()
                     if v.get("true_locus") == "unknown")
    assert unknown == ["ag-15", "cep-15", "df-13", "df-14", "df-15",
                       "sr-13", "sr-14", "sr-15"]
