"""Offline structural gates for Evidence Revision 002 execution revision 003."""
from __future__ import annotations
import hashlib, json, os
from pathlib import Path
import pytest
from eval.v2 import evidence_revision_002 as science
from eval.v2 import evidence_revision_002_exec003 as x
from eval.v2.evidence_revision_002_freeze import build_artifacts, FULL_MATRIX_NAME
from eval.v2.evidence_revision_run import DeterministicFakeClient, InjectedInterruption
from eval.v2.opencode_zen_provider import (AmbiguousTransport, OpenCodeZenClient, ProviderFailure,
                                           PROVIDER_ROUTE, MODEL, ENDPOINT)


def test_coordinate_science_preserved_and_successor_uids_are_new():
    old=build_artifacts()[FULL_MATRIX_NAME]; new=x.full_matrix_document()
    assert science.contract_fingerprint()==new["design_fingerprint"]
    assert len(new["coordinates"])==43*5*2==430
    assert {tuple(c) for c in new["coordinates"]}=={(c,a,r) for c in old["cases"] for a in old["arms"] for r in range(1,old["repetitions"]+1)}
    assert set(new["unit_ids"]).isdisjoint(old["unit_ids"])
    assert new["executable_contract_fingerprint"] != old["executable_contract_fingerprint"]


def test_shard_plan_is_deterministic_complete_and_balanced():
    a,b=x.shard_plan_document(),x.shard_plan_document()
    assert a==b and a["shard_count"]==16 and len(a["membership"])==430
    assert len({r["unit_id"] for r in a["membership"]})==430
    assert a["shard_sizes"]==[27]*14+[26]*2
    assert all(n > 0 for n in a["shard_sizes"])


def test_exec003_freeze_artifacts_reference_unchanged_science(tmp_path):
    docs=x.write_artifacts(tmp_path); exe=docs[x.ARTIFACT_PREFIX+"executable-contract.json"]
    assert exe["scientific_contract_fingerprint"]==science.contract_fingerprint()
    assert exe["provider_contract"]=={"route":PROVIDER_ROUTE,"requested_model":MODEL,"endpoint":ENDPOINT,"credential_env":"SXR_OPENCODE_ZEN_API_KEY"}
    assert exe["execution_authorized"] is False
    assert x.preflight(tmp_path)["status"]=="PASS"


def test_full_fake_sequential_parallel_and_ledger_equivalence(tmp_path):
    one,four=tmp_path/"one",tmp_path/"four"
    x.run_fake_plan(one,1); x.merge(one)
    x.run_fake_plan(four,4); x.merge(four)
    assert x.scientific_projection(one)==x.scientific_projection(four)
    assert (one/"merged/call-commits.jsonl").read_bytes()==(four/"merged/call-commits.jsonl").read_bytes()
    assert (one/"merged/result-commits.jsonl").read_bytes()==(four/"merged/result-commits.jsonl").read_bytes()
    assert len(list((four/"merged/results").glob("*.json")))==430


def test_concurrent_hammer_has_exact_durable_coverage(tmp_path):
    x.run_fake_plan(tmp_path,4); summary=x.merge(tmp_path)
    assert summary["results"]==430
    for path in tmp_path.rglob("*.json"):
        assert isinstance(json.loads(path.read_text()),dict)
    calls=(tmp_path/"merged/call-commits.jsonl").read_text().splitlines()
    assert len(calls)==len({(json.loads(v)["unit_id"],json.loads(v)["call_num"]) for v in calls})


@pytest.mark.parametrize("boundary",["before_call_intent","after_call_intent_before_transport","after_provider_response_before_raw_commit","after_raw_write_before_commit_record","after_raw_commit_before_parse","after_result_write_before_result_commit","after_result_commit"])
def test_crash_boundaries_never_manufacture_settlement(tmp_path,boundary):
    def crash(where,n):
        if where==boundary: raise InjectedInterruption(boundary)
    with pytest.raises(InjectedInterruption): x.run_shard(tmp_path,0,1,DeterministicFakeClient,crash)
    state=x.status(tmp_path)["shards"][0]["state"]
    assert state in {"ACTIVE","EXITED_FAILED","STALE"}
    with pytest.raises(x.InvariantViolation): x.merge(tmp_path)


def test_ambiguous_transport_aborts_only_one_shard_and_replacement_is_outcome_blind(tmp_path):
    class Ambiguous(DeterministicFakeClient):
        def complete(self,*args): raise AmbiguousTransport("test timeout")
    failed=x.run_shard(tmp_path,3,1,Ambiguous)
    assert failed["state"]=="ABORTED_AMBIGUOUS_TRANSPORT"
    x.run_shard(tmp_path,2,1,DeterministicFakeClient)
    assert x.status(tmp_path)["shards"][2]["state"]=="SETTLED"
    with pytest.raises(x.InvariantViolation): x.merge(tmp_path)
    x.run_shard(tmp_path,3,2,DeterministicFakeClient)
    for n in range(16):
        if n not in (2,3): x.run_shard(tmp_path,n,1,DeterministicFakeClient)
    merged=x.merge(tmp_path)
    assert merged["selected_attempts"]["03"].endswith("002")


def test_status_is_liveness_aware_not_summary_aware(tmp_path):
    x.initialize_run(tmp_path); d=x._attempt_dir(tmp_path,0,1); d.mkdir(parents=True)
    x._atomic(d/"shard.json",{"identity":"shard-00-a001","attempt":1,"shard":0,"state":"ACTIVE","pid":999999999,"matrix_sha256":x.full_matrix_document()["matrix_sha256"]})
    assert x.status(tmp_path)["shards"][0]["state"]=="EXITED_FAILED"


def test_provider_has_no_constructor_or_local_validation_network(monkeypatch):
    calls=[]
    c=OpenCodeZenClient(opener=lambda *a,**k: calls.append(1))
    assert calls==[]
    monkeypatch.setenv("SXR_OPENCODE_ZEN_API_KEY","a-safe-test-key")
    assert c.validate_local_configuration()["network_requests"]==0 and calls==[]


def test_provider_classifies_explicit_retry_and_ambiguous(monkeypatch):
    monkeypatch.setenv("SXR_OPENCODE_ZEN_API_KEY","a-safe-test-key")
    events=[]
    import urllib.error
    seq=[urllib.error.HTTPError(ENDPOINT,429,"",None,None), urllib.error.HTTPError(ENDPOINT,401,"",None,None)]
    def opener(*a,**k): raise seq.pop(0)
    with pytest.raises(ProviderFailure) as caught: OpenCodeZenClient(opener=opener,attempt_sink=events.append).complete("s","u",1,0)
    assert caught.value.classification=="REJECTED_FATAL"
    assert [e["classification"] for e in events if e["status"]=="REJECTED"]==["REJECTED_RETRYABLE","REJECTED_FATAL"]
    with pytest.raises(AmbiguousTransport): OpenCodeZenClient(opener=lambda *a,**k: (_ for _ in ()).throw(TimeoutError("timeout"))).complete("s","u",1,0)


def test_run_shard_refuses_network_capable_client_before_call(tmp_path):
    class NetworkCapable:
        def complete(self,*args): raise AssertionError("must not be called")
    got=x.run_shard(tmp_path,0,1,NetworkCapable)
    assert got["state"]=="ABORTED_INVARIANT"
    assert "forbidden" in got["reason"]
