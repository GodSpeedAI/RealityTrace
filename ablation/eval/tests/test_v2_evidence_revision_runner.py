"""Offline execution semantics for the frozen evidence-revision runner."""
from __future__ import annotations

import dataclasses
import hashlib
import json
from pathlib import Path

import pytest

from ..v2 import controller as controller_module
from ..v2.channel import AnswerLeak, PromptLeakGuard, unit_seed
from ..v2.controller import DiagnosticController
from ..v2.decision import ParseStatus
from ..v2.evidence_revision import ARMS, OWNER_STATE, contract_fingerprint
from ..v2.evidence_revision_run import (
    COSTS, DESCRIPTIONS, EXPECTED_DESIGN_FINGERPRINT,
    EXPECTED_EXECUTABLE_FINGERPRINT, EXPECTED_FINGERPRINT, EXPERIMENT_ID,
    FULL_NAMESPACE, PILOT_NAMESPACE, PROBES, REVISED_CASE_INPUTS,
    REVISED_PILOT_MATRIX, VOCAB, _cmd_run, _live_client,
    CheckpointStore, DeterministicFakeClient, DurableReplayClient, ExecutionGate,
    ExecutionRefused, FrozenMatrix, GateRequest, InjectedInterruption,
    ResumeViolation, RunNamespace, _fixture, dry_run, eligible_cases,
    frozen_hashes, inspect_matrix, load_frozen_matrix, owner_state_digest,
    run_offline_unit, score_committed_result)
from ..v2.execution import OutputClass
from ..v2.ids import unit_id
from ..v2.ledger import CommitViolation
from ..v2.legibility import assert_semantic_equivalence
from ..v2.replication import ARMS as REPLICATION_ARMS
from ..v2.sufficiency import LEGACY_CONTRACT


def _authorized(stage="pilot"):
    state = dict(OWNER_STATE)
    state.update(status=("PILOT_AUTHORIZED" if stage == "pilot" else "FULL_AUTHORIZED"),
                 execution_authorized=True,
                 additional_provider_calls_authorized=True,
                 authorized_stage=stage)
    return state


def _unauthorized():
    state = dict(OWNER_STATE)
    state.update(status="FROZEN_NOT_STARTED", execution_authorized=False,
                 additional_provider_calls_authorized=False,
                 authorized_stage=None)
    return state


def _matrix(stage="pilot"):
    frozen = json.loads(REVISED_PILOT_MATRIX.read_text())
    namespace = PILOT_NAMESPACE if stage == "pilot" else FULL_NAMESPACE
    return FrozenMatrix(stage, namespace, ("cep-01",), tuple(ARMS), 1,
                        EXPECTED_DESIGN_FINGERPRINT,
                        EXPECTED_EXECUTABLE_FINGERPRINT,
                        frozen["case_inputs_sha256"], frozen["matrix_sha256"])


def _marker(tmp_path, owner, token, stage="pilot", mutate=None):
    hashes = frozen_hashes()
    matrix = _matrix(stage)
    doc = {"experiment_id": EXPERIMENT_ID, "stage": stage,
           "design_fingerprint": EXPECTED_DESIGN_FINGERPRINT,
           "executable_contract_fingerprint": EXPECTED_EXECUTABLE_FINGERPRINT,
           "authorization_state": "PILOT_AUTHORIZED" if stage == "pilot" else "FULL_AUTHORIZED",
           "owner_state_digest": owner_state_digest(owner),
           "release_token_sha256": hashlib.sha256(token.encode()).hexdigest(),
           "corpus_hash": hashes["corpus_hash"],
           "evidence_action_hash": hashes["evidence_action_hash"],
           "matrix_sha256": matrix.matrix_hash,
           "case_inputs_sha256": matrix.case_inputs_hash}
    if mutate:
        doc.update(mutate)
    path = tmp_path / "release.json"
    path.write_text(json.dumps(doc))
    return path


def _request(tmp_path, *, stage="pilot", owner=OWNER_STATE, token="fake-token",
             marker=None, fingerprint=EXPECTED_FINGERPRINT, client=None, matrix=None):
    return GateRequest(stage, token, marker, client or DeterministicFakeClient(),
                       matrix or _matrix(stage), "run-001", tmp_path,
                       owner_state=owner, observed_fingerprint=fingerprint)


def test_owner_state_unauthorized_refuses_even_with_token_and_marker(tmp_path):
    owner = _unauthorized()
    marker = _marker(tmp_path, owner, "fake-token")
    with pytest.raises(ExecutionRefused, match="owner state"):
        ExecutionGate.validate(_request(tmp_path, owner=owner, marker=marker))


def test_wrong_contract_fingerprint_refused(tmp_path):
    owner = _authorized()
    marker = _marker(tmp_path, owner, "fake-token")
    with pytest.raises(ExecutionRefused, match="fingerprint"):
        ExecutionGate.validate(_request(tmp_path, owner=owner, marker=marker,
                                        fingerprint="wrong"))


def test_release_token_or_marker_alone_cannot_bypass_owner(tmp_path):
    owner = _unauthorized()
    marker = _marker(tmp_path, owner, "fake-token")
    for token, mark in (("fake-token", None), (None, marker), ("fake-token", marker)):
        with pytest.raises(ExecutionRefused):
            ExecutionGate.validate(_request(tmp_path, owner=owner, token=token, marker=mark))


def test_valid_synthetic_pilot_authorization_passes_single_gate(tmp_path):
    owner = _authorized("pilot")
    token = "deterministic-test-token"
    marker = _marker(tmp_path, owner, token)
    report = ExecutionGate.validate(_request(tmp_path, owner=owner, token=token,
                                             marker=marker))
    assert report["authorized"] is True


def test_pilot_authorization_and_completion_cannot_unlock_full(tmp_path):
    pilot_owner = dict(_authorized("pilot"), pilot_complete=True)
    marker = _marker(tmp_path, pilot_owner, "fake-token", stage="full",
                     mutate={"authorization_state": "FULL_AUTHORIZED"})
    with pytest.raises(ExecutionRefused, match="FULL_AUTHORIZED"):
        ExecutionGate.validate(_request(tmp_path, stage="full", owner=pilot_owner,
                                        marker=marker, matrix=_matrix("full")))


def test_full_remains_refused_while_full_authorization_is_false(tmp_path):
    owner = _authorized("full")
    owner["full_execution_authorized"] = False
    marker = _marker(tmp_path, owner, "full-token", stage="full")
    with pytest.raises(ExecutionRefused, match="FULL execution authorization is false"):
        ExecutionGate.validate(_request(tmp_path, stage="full", owner=owner,
                                        token="full-token", marker=marker,
                                        matrix=_matrix("full")))


def test_contract_arms_are_imported_not_duplicated():
    assert REPLICATION_ARMS is ARMS
    source = Path(__file__).parents[1] / "v2" / "evidence_revision_run.py"
    text = source.read_text()
    assert "from .evidence_revision import (ARMS" in text
    assert '"R1": ConditionSpec' not in text


def test_matrix_inspection_is_deterministic_and_contract_complete():
    a, b = inspect_matrix("pilot"), inspect_matrix("pilot")
    assert a == b
    assert a["design_fingerprint"] == EXPECTED_DESIGN_FINGERPRINT
    assert a["executable_contract_fingerprint"] == EXPECTED_EXECUTABLE_FINGERPRINT
    assert a["cases"] == ["cep-07", "df-11", "ag-04", "sr-10", "df-06", "cep-05"]
    assert a["arms"] == list(ARMS)
    assert a["repetitions"] == 2 and a["total_units"] == 60
    assert len(a["uids"]) == 60 and a["executable"] is True
    assert a["blocker"] is None


def test_arbitrary_matrix_file_cannot_become_frozen_by_existence(tmp_path):
    p = tmp_path / "matrix.json"
    p.write_text(json.dumps({"schema": "evidence-revision-matrix-v1"}))
    with pytest.raises(ExecutionRefused, match="canonical hash mismatch"):
        load_frozen_matrix(p, "pilot", OWNER_STATE)


def test_pilot_and_full_uids_cannot_collide():
    fp = contract_fingerprint()
    pilot = {unit_id(EXPERIMENT_ID, "pilot:run", "cep-01", a, 1, fp) for a in ARMS}
    full = {unit_id(EXPERIMENT_ID, "full:run", "cep-01", a, 1, fp) for a in ARMS}
    assert pilot.isdisjoint(full)


def test_run_namespace_rejects_stage_or_fingerprint_drift(tmp_path):
    hashes = frozen_hashes()["sources"]
    RunNamespace(tmp_path, "same", "pilot", hashes).create_or_verify()
    with pytest.raises(ResumeViolation):
        RunNamespace(tmp_path, "same", "full", hashes).create_or_verify()
    changed = dict(hashes, contract_module="bad")
    with pytest.raises(ResumeViolation):
        RunNamespace(tmp_path, "same", "pilot", changed).create_or_verify()


def test_dry_run_cannot_accept_network_capable_client(tmp_path):
    class NotOffline:
        def complete(self, *args):
            raise AssertionError("transport must never be reached")
    with pytest.raises(ExecutionRefused, match="offline_only"):
        run_offline_unit(tmp_path, NotOffline())


def test_dry_run_exercises_raw_parse_transition_and_score(tmp_path):
    report = dry_run(tmp_path)
    assert report["network_transport_instantiated"] is False
    assert report["model_calls_performed"] == report["provider_calls_performed"] == 0
    assert report["fake_client_calls"] == report["raw_calls_committed"] >= 1
    assert report["scoring_handoff"]["analysis_handoff"] is True


def test_raw_response_is_committed_before_parser_observes_it(tmp_path, monkeypatch):
    seen = []
    original = controller_module.parse_decision
    def observing_parser(text, vocabulary, action_ids):
        ledgers = list(tmp_path.rglob("call-commits.jsonl"))
        assert ledgers and text.encode() in next(tmp_path.rglob("raw/*/*.txt")).read_bytes()
        seen.append(True)
        return original(text, vocabulary, action_ids)
    monkeypatch.setattr(controller_module, "parse_decision", observing_parser)
    run_offline_unit(tmp_path, DeterministicFakeClient())
    assert seen


def test_attempt_to_score_before_result_or_raw_commit_fails(tmp_path):
    with pytest.raises(CommitViolation):
        score_committed_result(tmp_path, "never-committed", lambda x: x)


def test_malformed_output_uses_bounded_repair_without_unknown_or_budget_exhaustion(tmp_path):
    bad = DeterministicFakeClient(["not json"] * 3)
    result, replay = run_offline_unit(tmp_path, bad)
    assert bad.call_count == ARMS["R1"].max_repair_attempts + 1
    assert result.terminal_state == "MODEL_OUTPUT_CONTRACT_FAILURE"
    assert result.final_locus is None and result.last_model_claim is None
    assert result.payment_used == 0 and result.acquired_handles == []
    assert all("budget exhausted" not in p.read_text().lower()
               for p in tmp_path.rglob("raw/*.txt"))
    assert replay.ledger.provenance_summary()["verified"] == bad.call_count


def test_truncation_remains_separate_from_contract_failure(tmp_path):
    provider = {"finish_reason": "length", "completion_tokens": 17,
                "resolved_model": "fake", "provider_route": "offline"}
    bad = DeterministicFakeClient(["not json"] * 3, provider=provider)
    result, _ = run_offline_unit(tmp_path, bad)
    assert all(e["truncated"] is True for e in result.executions)
    # Per-call classification machinery retains a distinct taxonomy.
    assert OutputClass.OUTPUT_TRUNCATED.value != OutputClass.MODEL_OUTPUT_CONTRACT_FAILURE.value


def test_provider_metadata_is_consumed_and_unavailable_is_null(tmp_path):
    provider = {"resolved_model": "provider/returned", "generation_id": "g-9",
                "finish_reason": "stop", "completion_tokens": 44,
                "latency_ms": 12.5}
    result, _ = run_offline_unit(tmp_path, DeterministicFakeClient(provider=provider))
    returned = result.executions[0]["returned"]
    assert returned["resolved_model"] == "provider/returned"
    assert returned["generation_id"] == "g-9" and returned["completion_tokens"] == 44
    assert returned["prompt_tokens"] is None and returned["retries"] is None
    assert "prompt_tokens" in result.executions[0]["unavailable_fields"]


def test_final_rendered_prompt_is_guarded_and_canonical_probe_leak_never_transports(tmp_path):
    fake = DeterministicFakeClient()
    replay = DurableReplayClient(fake, "u", tmp_path)
    with pytest.raises(AnswerLeak, match="final transport prompt"):
        replay.complete("system", "inspect_permissions\n[question]\nq", 10, 0.0)
    assert fake.call_count == 0


def test_state_continuity_missing_original_or_acquired_evidence_rejected_before_transport(tmp_path):
    fake = DeterministicFakeClient()
    replay = DurableReplayClient(fake, "u", tmp_path)
    for prompt in ("[question]\nq\n[initial_evidence]\n[redacted]",
                   "[question]\nq\n[initial_evidence]\ne\n[acquired_observations]\n[redacted]"):
        with pytest.raises(ResumeViolation, match="lacks state"):
            replay.complete("s", prompt, 10, 0.0)
    assert fake.call_count == 0


def test_opaque_handle_seed_ignores_truth_and_hidden_metadata():
    args = (EXPERIMENT_ID, "pilot:n", "cep-01", "R1", 1)
    assert unit_seed(*args) == unit_seed(*args)
    assert "true_locus" not in unit_seed.__code__.co_varnames
    assert "hidden" not in " ".join(unit_seed.__code__.co_varnames)


def test_raw_legible_semantic_binding_and_arm_pair_isolation():
    payload = {"missing": [], "exit": 101}
    proof = assert_semantic_equivalence(payload)
    assert proof["content_equivalent"] is True
    assert set(ARMS["R1"].diff(ARMS["R2"])) == {"observation_rendering"}
    assert set(ARMS["R3"].diff(ARMS["R4"])) == {"observation_rendering"}


def test_full_governance_and_payment_are_explicit_in_result(tmp_path):
    result, _ = run_offline_unit(tmp_path, DeterministicFakeClient(), arm="R3")
    doc = json.loads((tmp_path / "results" / f"{result.unit_id}.json").read_text())
    rc = doc["runner_contract"]
    assert rc["governance_class"] == rc["budget_semantics"] == "EXEMPT_UPPER_BOUND"
    assert rc["actual_implied_evidence_payment"] > 0
    assert doc["payment_used"] == rc["actual_implied_evidence_payment"]


def test_full_membership_is_invariant_across_truth_independent_units(tmp_path):
    out = []
    for i, case in enumerate(("cep-07", "df-11")):
        result, _ = run_offline_unit(tmp_path / str(i), DeterministicFakeClient(),
                                     arm="R3", case_id=case)
        first_delivery = next(r.delivery for r in result.rounds if r.delivery)
        out.append((first_delivery["n_actions"], len(first_delivery["handles"]),
                    first_delivery["consistency_set_size"]))
    assert out[0] == out[1] == (len(PROBES), len(PROBES), len(PROBES))


def test_gate_blind_claim_and_governed_settlement_are_distinct_and_reconstructable(tmp_path):
    # authority remains a model claim but can be rejected by the sufficiency gate.
    result, _ = run_offline_unit(tmp_path, DeterministicFakeClient())
    doc = json.loads((tmp_path / "results" / f"{result.unit_id}.json").read_text())
    assert doc["runner_contract"]["gate_blind_last_model_claim"] == result.last_model_claim
    assert "governed_final_settlement" in doc["runner_contract"]
    decisions = [r["decision"] for r in doc["rounds"]]
    assert decisions and decisions[0]["proposed_settlement"] is not None
    assert any(r.get("delivery") for r in doc["rounds"])  # pre/post evidence recoverable


@pytest.mark.parametrize("boundary", ["after_raw_persist", "after_raw_hash", "after_manifest_commit"])
def test_resume_replays_committed_raw_without_duplicate_transport(tmp_path, boundary):
    fake = DeterministicFakeClient()
    fired = set()
    def interrupt(where, n):
        if where == boundary and n == 1 and where not in fired:
            fired.add(where)
            raise InjectedInterruption(where)
    with pytest.raises(InjectedInterruption):
        run_offline_unit(tmp_path, fake, interrupt=interrupt)
    assert fake.call_count == 1
    result, _ = run_offline_unit(tmp_path, fake)
    assert result.terminal_state
    # call 1 was replayed; only a scientifically subsequent call may be new.
    assert fake.call_count == len(result.executions)



def test_after_parse_interruption_resumes_without_duplicate_calls(tmp_path):
    fake = DeterministicFakeClient()
    fired = False
    def interrupt(where, n):
        nonlocal fired
        if where == "after_parse" and not fired:
            fired = True
            raise InjectedInterruption(where)
    with pytest.raises(InjectedInterruption):
        run_offline_unit(tmp_path, fake, interrupt=interrupt)
    before = fake.call_count
    result, _ = run_offline_unit(tmp_path, fake)
    assert fake.call_count == before == len(result.executions)


def test_completed_resume_skips_transport_and_scoring_is_single_authoritative_commit(tmp_path):
    fake = DeterministicFakeClient()
    result, _ = run_offline_unit(tmp_path, fake)
    calls = fake.call_count
    resumed, replay = run_offline_unit(tmp_path, fake)
    assert resumed.unit_id == result.unit_id and replay.calls == 0
    assert fake.call_count == calls
    scorer_calls = []
    first = score_committed_result(tmp_path, result.unit_id,
                                   lambda d: scorer_calls.append(1) or {"value": 1})
    second = score_committed_result(tmp_path, result.unit_id,
                                    lambda d: scorer_calls.append(2) or {"value": 2})
    assert first == second == {"value": 1}
    assert scorer_calls == [1]


def test_post_provider_precommit_ambiguity_fails_closed_without_duplicate(tmp_path):
    fake = DeterministicFakeClient()
    def interrupt(where, n):
        if where == "after_provider_result":
            raise InjectedInterruption(where)
    with pytest.raises(InjectedInterruption):
        run_offline_unit(tmp_path, fake, interrupt=interrupt)
    assert fake.call_count == 1
    with pytest.raises(ResumeViolation, match="partial uncommitted"):
        run_offline_unit(tmp_path, fake)
    assert fake.call_count == 1


def test_before_call_interruption_resumes_with_one_authoritative_call(tmp_path):
    fake = DeterministicFakeClient()
    fired = False
    def interrupt(where, n):
        nonlocal fired
        if where == "before_call" and not fired:
            fired = True
            raise InjectedInterruption(where)
    with pytest.raises(InjectedInterruption):
        run_offline_unit(tmp_path, fake, interrupt=interrupt)
    assert fake.call_count == 0
    result, _ = run_offline_unit(tmp_path, fake)
    assert fake.call_count == len(result.executions)


def test_resume_refuses_different_raw_bytes_for_same_call(tmp_path):
    result, _ = run_offline_unit(tmp_path, DeterministicFakeClient())
    raw = next(tmp_path.rglob("raw/*/*.txt"))
    raw.write_text("tampered")
    with pytest.raises((CommitViolation, ResumeViolation)):
        run_offline_unit(tmp_path, DeterministicFakeClient())


def test_another_arm_checkpoint_and_evidence_cannot_be_reused(tmp_path):
    run_offline_unit(tmp_path, DeterministicFakeClient(), arm="R1")
    # Different arms have distinct unit IDs and artifact directories.
    r2, _ = run_offline_unit(tmp_path, DeterministicFakeClient(), arm="R2")
    raw_dirs = {p.parent.name for p in (tmp_path / "raw").glob("*/*.txt")}
    assert r2.unit_id in raw_dirs and len(raw_dirs) == 2



def test_harness_source_ignores_model_action_for_evidence_selection(tmp_path):
    def action(handle):
        return json.dumps({"candidate_loci": ["authority", "environment"],
                           "unresolved_distinction": "permissions versus environment",
                           "selected_action": handle,
                           "expected_information": "diagnostic evidence",
                           "proposed_settlement": None})
    deliveries = []
    for i, handle in enumerate(("act-01", "act-08")):
        fake = DeterministicFakeClient([action(handle)])
        result, _ = run_offline_unit(tmp_path / str(i), fake, namespace="same")
        first = next(r for r in result.rounds if r.delivery)
        assert first.action_source == "harness"
        assert first.gate_result == "MODEL_ACTION_IGNORED_HARNESS_CONTROLLED"
        deliveries.append(first.handles_delivered)
    assert deliveries[0] == deliveries[1]


def test_governed_harness_delivery_never_overdraws_frozen_budget(tmp_path):
    settle = json.dumps({"candidate_loci": ["verifier", "implementation"],
                         "unresolved_distinction": None, "selected_action": None,
                         "expected_information": None,
                         "proposed_settlement": "verifier"})
    result, _ = run_offline_unit(tmp_path, DeterministicFakeClient([settle] * 6))
    assert result.payment_used <= ARMS["R1"].max_payment
    assert not any(r.gate_result == "HARNESS_DELIVERY_UNAFFORDABLE"
                   for r in result.rounds)
    delivered = [r for r in result.rounds if r.delivery]
    assert len(delivered) == 1 and len(delivered[0].handles_delivered) == 3

def test_contract_module_remains_transport_free():
    source = (Path(__file__).parents[1] / "v2" / "evidence_revision.py").read_text()
    assert "urllib" not in source and "ZaiCodingPlanClient" not in source
    assert "provider.py" not in source
