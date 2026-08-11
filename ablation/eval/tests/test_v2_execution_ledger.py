"""R-04/R-07/R-08 regression: truncation observability, provider provenance,
and the raw-hash-before-score binding."""

import json
import os

import pytest

from ..v2.decision import ParseStatus
from ..v2.execution import (EffectiveCeilingDetector, OutputClass,
                            classify_output, record_from_provider)
from ..v2.ledger import (HASH_UNVERIFIABLE, HASH_VERIFIED, CommitLedger,
                         CommitViolation)


def rec(call=1, requested_max=8192, provider=None, text="ok"):
    return record_from_provider(
        call_num=call, requested_model="glm-5.2",
        requested_max_tokens=requested_max, requested_temperature=0.0,
        endpoint="https://example.invalid/v4", raw_text=text,
        provider=provider or {})


# ── NEW-02: truncation observability ─────────────────────────────────────────

def test_NEW02_route_ceiling_below_request_is_detected():
    """The exact historical situation: 8192 requested, nothing above 4096
    returned, and the report claimed 8192 governed the run."""
    det = EffectiveCeilingDetector()
    for i in range(270):
        det.observe(rec(i, 8192, {"completion_tokens": 4096, "resolved_model": "glm-5.2"}))
    for i in range(100):
        det.observe(rec(i, 8192, {"completion_tokens": 900, "resolved_model": "glm-5.2"}))
    r = det.report()
    assert r["route_ceiling_below_request"] is True
    assert r["effective_ceiling"] == 4096
    assert r["requested_max_tokens"] == [8192]
    assert "do NOT claim the requested value governed the run" in r["claim_permitted"]


def test_no_false_ceiling_alarm_when_the_request_is_actually_honoured():
    det = EffectiveCeilingDetector()
    det.observe(rec(1, 4096, {"completion_tokens": 4096}))
    assert det.report()["route_ceiling_below_request"] is False


def test_truncation_uses_finish_reason_when_the_provider_reports_it():
    r = rec(provider={"completion_tokens": 10, "finish_reason": "length"})
    assert r.truncated is True
    r2 = rec(provider={"completion_tokens": 4096, "finish_reason": "stop"},
             requested_max=4096)
    assert r2.hit_requested_ceiling is True and r2.truncated is False


def test_four_output_classes_are_never_collapsed():
    trunc = rec(provider={"completion_tokens": 4096}, requested_max=4096, text="")
    assert classify_output(trunc, ParseStatus.EMPTY_OUTPUT,
                           False) is OutputClass.OUTPUT_TRUNCATED
    ok = rec(provider={"completion_tokens": 100, "finish_reason": "stop"})
    assert classify_output(ok, ParseStatus.VALID,
                           False) is OutputClass.VALID_DECISION
    bad = rec(provider={"completion_tokens": 100, "finish_reason": "stop"})
    assert classify_output(bad, ParseStatus.SCHEMA_VIOLATION,
                           True) is OutputClass.MODEL_OUTPUT_CONTRACT_FAILURE
    assert OutputClass.PARSER_FAILURE.value == "PARSER_FAILURE"


# ── NEW-03: genuine provider provenance ──────────────────────────────────────

def test_NEW03_resolved_model_comes_from_the_provider_not_a_constant():
    r = rec(provider={"resolved_model": "zai/glm-5.2-0731",
                      "generation_id": "gen_01ABC", "retries": 2,
                      "prompt_tokens": 1900, "completion_tokens": 812,
                      "finish_reason": "stop"})
    d = r.to_dict()
    assert d["requested"]["model"] == "glm-5.2"
    assert d["returned"]["resolved_model"] == "zai/glm-5.2-0731"
    assert d["returned"]["generation_id"] == "gen_01ABC"
    assert d["returned"]["retries"] == 2


def test_unavailable_fields_are_null_with_a_reason_never_zero():
    """The legacy sidecars wrote 0 for prompt/completion tokens and retries,
    which is indistinguishable from a real zero."""
    r = rec(provider={"completion_tokens": 5})
    d = r.to_dict()
    assert d["returned"]["retries"] is None
    assert d["returned"]["prompt_tokens"] is None
    assert "retries" in d["unavailable_fields"]
    assert d["unavailable_reason"] == "provider_did_not_report"
    assert 0 not in (d["returned"]["retries"], d["returned"]["prompt_tokens"])


def test_raw_hash_is_recorded_for_every_call():
    r = rec(text="hello")
    assert r.raw_hash == __import__("hashlib").sha256(b"hello").hexdigest()


# ── NEW-04: commit-before-score ──────────────────────────────────────────────

def test_NEW04_scoring_before_commit_is_refused(tmp_path):
    led = CommitLedger(str(tmp_path / "commits.jsonl"))
    with pytest.raises(CommitViolation, match="no commit entry"):
        led.assert_scorable("u1")


def test_commit_then_score_verifies_content(tmp_path):
    led = CommitLedger(str(tmp_path / "commits.jsonl"))
    raw = str(tmp_path / "raw" / "u1.txt")
    e = led.commit_raw("u1", raw, "payload")
    assert e.hash_provenance == HASH_VERIFIED
    assert led.assert_scorable("u1") == e.raw_hash


def test_post_commit_tampering_is_caught(tmp_path):
    led = CommitLedger(str(tmp_path / "commits.jsonl"))
    raw = str(tmp_path / "raw" / "u1.txt")
    led.commit_raw("u1", raw, "payload")
    with open(raw, "w") as f:
        f.write("tampered")
    with pytest.raises(CommitViolation, match="hash mismatch"):
        led.assert_scorable("u1")


def test_deleting_the_raw_artifact_blocks_scoring(tmp_path):
    led = CommitLedger(str(tmp_path / "commits.jsonl"))
    raw = str(tmp_path / "raw" / "u1.txt")
    led.commit_raw("u1", raw, "payload")
    os.remove(raw)
    with pytest.raises(CommitViolation, match="raw artifact missing"):
        led.assert_scorable("u1")


def test_reconstruction_may_not_invent_a_hash(tmp_path):
    """The historical failure mode: 527/540 units were marked
    output_committed by a reconstruction that had no content hash."""
    led = CommitLedger(str(tmp_path / "commits.jsonl"))
    raw = tmp_path / "raw" / "u9.txt"
    raw.parent.mkdir(parents=True, exist_ok=True)
    raw.write_text("recovered bytes")
    e = led.reconstruct("u9", str(raw), note="parallel-worker concurrency loss")
    assert e.raw_hash is None and e.hash_provenance == HASH_UNVERIFIABLE
    with pytest.raises(CommitViolation, match="hash provenance is UNVERIFIABLE"):
        led.assert_scorable("u9")


def test_ledger_reloads_and_summarises_provenance(tmp_path):
    p = str(tmp_path / "commits.jsonl")
    led = CommitLedger(p)
    led.commit_raw("a", str(tmp_path / "a.txt"), "x")
    led.reconstruct("b", str(tmp_path / "b.txt"), "lost")
    again = CommitLedger(p)
    assert again.provenance_summary() == {"total": 2, "verified": 1,
                                          "unverifiable": 1}
    assert json.loads(open(p).readline())["hash_provenance"] == HASH_VERIFIED
