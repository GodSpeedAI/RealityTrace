"""Offline structural gates for Evidence Revision 002 execution revision 003."""
from __future__ import annotations
import hashlib, json, os
from pathlib import Path
import pytest
from eval.v2 import evidence_revision_002 as science
from eval.v2 import evidence_revision_002_exec004 as x
from eval.v2.evidence_revision_002_freeze import build_artifacts, FULL_MATRIX_NAME
from eval.v2.evidence_revision_run import DeterministicFakeClient, InjectedInterruption
from eval.v2.opencode_runtime_provider import (AmbiguousTransport, OpenCodeRuntimeClient, ProviderFailure,
                                                PROVIDER_ROUTE, MODEL, ENDPOINT, DENIED_TOOLS, DENY_ALL_PERMISSIONS,
                                                PROVIDER_ID, SESSION_AGENT, RESTRICTED_AGENT_CONFIG)


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


def test_exec004_freeze_artifacts_reference_unchanged_science(tmp_path):
    docs=x.write_artifacts(tmp_path); exe=docs[x.ARTIFACT_PREFIX+"executable-contract.json"]
    assert exe["scientific_contract_fingerprint"]==science.contract_fingerprint()
    provider=exe["provider_contract"]
    assert provider["route"]==PROVIDER_ROUTE and provider["provider_id"]==PROVIDER_ID
    assert provider["requested_model"]==MODEL and provider["endpoint"]==ENDPOINT
    assert provider["credential_env"] is None and provider["session_isolation"]=="one_new_session_per_RealityTrace_model_call"
    assert exe["exec003_qualification_lineage"]["artifact"]=="evidence-revision-002-exec003-qualification.json"
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



class _FakeOpenCodeServer:
    """Thread-safe documented session/message HTTP fixture; never calls a model."""
    def __init__(self): self.sessions=[]; self.prompts=[]; self._server=None; self._thread=None
    def __enter__(self):
        import threading
        from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
        fixture=self
        class Handler(BaseHTTPRequestHandler):
            def log_message(self,*args): pass
            def do_POST(self):
                try: body=json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                except Exception: self.send_error(400); return
                if self.path.startswith("/session?"):
                    sid=f"ses_{len(fixture.sessions)+1:03d}"; fixture.sessions.append({"id":sid,"body":body,"path":self.path})
                    answer={"id":sid,"directory":"/tmp/sxr-opencode-transport","model":body.get("model"),"version":"fake-opencode"}
                elif self.path.startswith("/session/") and "/message?" in self.path:
                    sid=self.path.split("/")[2]; mid=f"msg_{len(fixture.prompts)+1:03d}"; fixture.prompts.append({"session_id":sid,"body":body,"path":self.path})
                    answer={"info":{"id":mid,"model":body.get("model")},"parts":[{"type":"text","text":'{"candidate_loci":["authority","environment"],"unresolved_distinction":null,"selected_action":null,"expected_information":null,"proposed_settlement":"authority"}'}]}
                else: self.send_error(404); return
                data=json.dumps(answer).encode(); self.send_response(200);self.send_header("Content-Type","application/json");self.send_header("Content-Length",str(len(data)));self.end_headers();self.wfile.write(data)
        self._server=ThreadingHTTPServer(("127.0.0.1",0),Handler); self._thread=threading.Thread(target=self._server.serve_forever,daemon=True);self._thread.start();return self
    @property
    def endpoint(self): return f"http://127.0.0.1:{self._server.server_port}"
    def client(self):
        c=OpenCodeRuntimeClient(endpoint=self.endpoint,workspace_directory="/tmp/sxr-opencode-transport")
        c.offline_only=True # test fixture cannot leave loopback fake server
        return c
    def __exit__(self,*args): self._server.shutdown();self._thread.join()


def test_runtime_provider_needs_no_zen_credential_and_local_validation_is_model_free(monkeypatch):
    monkeypatch.delenv("SXR_OPENCODE_ZEN_API_KEY",raising=False)
    c=OpenCodeRuntimeClient(opencode_binary="definitely-absent-opencode")
    assert c.endpoint == ENDPOINT and c.provider_route == PROVIDER_ROUTE
    with pytest.raises(ProviderFailure,match="runtime binary unavailable"): c.validate_local_configuration()


def test_runtime_provider_uses_isolated_sessions_exact_messages_and_tool_denial():
    with _FakeOpenCodeServer() as server:
        c=server.client()
        a=c.complete("SYSTEM-A","USER-A",8192,0.0); b=c.complete("SYSTEM-B","USER-B",8192,0.0)
    assert a["provider"]["session_id"] != b["provider"]["session_id"]
    assert [s["id"] for s in server.sessions] == ["ses_001","ses_002"]
    assert all(s["body"]["agent"]==SESSION_AGENT and s["body"]["model"]=={"id":MODEL,"providerID":PROVIDER_ID} for s in server.sessions)
    assert all(s["body"]["permission"]==DENY_ALL_PERMISSIONS for s in server.sessions)
    assert [p["body"]["system"] for p in server.prompts] == ["SYSTEM-A","SYSTEM-B"]
    assert [p["body"]["parts"] for p in server.prompts] == [[{"type":"text","text":"USER-A"}],[{"type":"text","text":"USER-B"}]]
    assert all(p["body"]["agent"]==SESSION_AGENT and p["body"]["tools"]==DENIED_TOOLS for p in server.prompts)
    assert all(value is False for value in server.prompts[0]["body"]["tools"].values())


def test_fake_opencode_server_parallel_equivalence_uses_four_isolated_workers(tmp_path):
    def run(root_,workers,server):
        x.initialize_run(root_)
        def one(shard): return x.run_shard(root_,shard,1,server.client)
        if workers==1: [one(n) for n in range(16)]
        else:
            from concurrent.futures import ThreadPoolExecutor
            with ThreadPoolExecutor(max_workers=workers) as pool: list(pool.map(one,reversed(range(16))))
        x.merge(root_)
    with _FakeOpenCodeServer() as server:
        one,four=tmp_path/"one",tmp_path/"four";run(one,1,server);run(four,4,server)
        assert x.scientific_projection(one)==x.scientific_projection(four)
        # One session per model call proves no prompt shares conversation state.
        assert len(server.sessions)==len(server.prompts)==3248
        assert len({s["id"] for s in server.sessions})==3248


def test_runtime_provider_ambiguous_loopback_failure_never_retries(monkeypatch):
    import urllib.error
    events=[]
    def opener(*args,**kwargs): raise TimeoutError("fixture timeout")
    c=OpenCodeRuntimeClient(opener=opener,attempt_sink=events.append)
    with pytest.raises(AmbiguousTransport): c.complete("s","u",1,0)
    assert [e["status"] for e in events] == ["INTENT","AMBIGUOUS"]



def test_runtime_provider_never_contains_direct_zen_route_or_credentials():
    source=(Path(__file__).parents[1]/"v2"/"opencode_runtime_provider.py").read_text()
    assert "https://opencode.ai/zen/v1/chat/completions" not in source
    assert "SXR_OPENCODE_ZEN_API_KEY" not in source
    assert "POST /session" in source and "one new restricted session" in source


def test_runtime_provider_explicit_retry_is_journaled_not_silent():
    import urllib.error
    events=[]
    sequence=[urllib.error.HTTPError(ENDPOINT,429,"",None,None),urllib.error.HTTPError(ENDPOINT,401,"",None,None)]
    def opener(*args,**kwargs): raise sequence.pop(0)
    with pytest.raises(ProviderFailure) as error:
        OpenCodeRuntimeClient(opener=opener,attempt_sink=events.append).complete("s","u",1,0)
    assert error.value.classification=="REJECTED_FATAL"
    rejected=[e for e in events if e["status"]=="REJECTED"]
    assert [(e["attempt_num"],e["classification"]) for e in rejected]==[(1,"REJECTED_RETRYABLE"),(3,"REJECTED_FATAL")]


def test_full_and_qualification_remain_fail_closed(tmp_path):
    docs=x.artifact_documents()
    assert docs[x.ARTIFACT_PREFIX+"executable-contract.json"]["full_execution_authorized"] is False
    assert docs[x.ARTIFACT_PREFIX+"qualification.json"]["status"]=="NOT_RUN"
    with pytest.raises(Exception): x.qualification_bench()



def test_required_runtime_config_defines_named_deny_all_agent():
    agent=RESTRICTED_AGENT_CONFIG["agent"][SESSION_AGENT]
    assert agent["permission"]=={"*":"deny"}
    assert set(agent["tools"]) >= {"bash","read","edit","write","task","webfetch","websearch","skill"}
    assert not any(agent["tools"].values())
    assert x.artifact_documents()[x.ARTIFACT_PREFIX+"runtime-config.json"] == RESTRICTED_AGENT_CONFIG
