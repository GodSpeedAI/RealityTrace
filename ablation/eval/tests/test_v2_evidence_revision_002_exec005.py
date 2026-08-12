import json
from pathlib import Path
import pytest

from eval.v2 import evidence_revision_002_exec004 as x4
from eval.v2 import evidence_revision_002_exec005 as x
from eval.v2.evidence_revision_002 import contract_fingerprint
from eval.v2.evidence_revision_run import DeterministicFakeClient, ExecutionRefused, InjectedInterruption
from eval.v2.opencode_runtime_provider import AmbiguousTransport, OpenCodeRuntimeClient


def _evidence(tmp_path, **changes):
    provider=x.executable_contract()["provider_contract"]
    qualification={"schema":x.QUALIFICATION_SCHEMA,"execution_revision":"005","status":"PASS","run_id":"synthetic-exec005-q001",
      "calls":20,"delivered":20,"ambiguous":0,"scientific_prompts_used":False,"provider":"opencode",
      "model":"deepseek-v4-flash-free","agent":"er002-transport","runtime_version":provider["runtime_version_at_freeze"]}
    provenance={"schema":x.PROVENANCE_SCHEMA,"execution_revision":"005","pass":True,"qualification_run_id":qualification["run_id"],
      "sessions_audited":20,"assistant_route_match":20,"user_route_match":20,"agent_match":20,"tool_parts_total":0,
      "provider":"opencode","model":"deepseek-v4-flash-free","agent":"er002-transport","runtime_version":provider["runtime_version_at_freeze"]}
    for key,value in changes.items():
        target,key=key.split("__",1) if "__" in key else ("qualification",key)
        (qualification if target=="qualification" else provenance)[key]=value
    q,p=tmp_path/"qualification.json",tmp_path/"provenance-audit.json"; x._atomic(q,qualification);x._atomic(p,provenance)
    return q,p,qualification,provenance


def _authority(tmp_path, **changes):
    q,p,_,_=_evidence(tmp_path,**changes)
    doc=x.mint_full_authority(q,p,"54467bd3d566ac82bbcc759cdff12ac240d99503",owner_authorized=True,authorization_timestamp="2026-08-13T00:00:00Z")
    a=tmp_path/"full-authority.json";x._atomic(a,doc);return a,q,p,doc


def test_science_coordinates_and_uids_successor_identities_are_frozen_and_disjoint(tmp_path):
    m4,m5=x4.full_matrix_document(),x.full_matrix_document()
    assert contract_fingerprint()==x4.contract_fingerprint()==x.contract_fingerprint()
    assert m5["coordinates"]==m4["coordinates"] and len(m5["coordinates"])==430
    assert set(m5["unit_ids"]).isdisjoint(m4["unit_ids"])
    docs=x.write_artifacts(tmp_path); exe=docs[x.ARTIFACT_PREFIX+"executable-contract.json"]
    assert exe["execution_revision"]=="005" and exe["executable_contract_fingerprint"]==m5["executable_contract_fingerprint"]
    assert docs[x.ARTIFACT_PREFIX+"shard-plan.json"]["matrix_sha256"]==m5["matrix_sha256"]
    assert exe["exec004_lineage"]["qualification_disposition"]=="LINEAGE_ONLY_DOES_NOT_AUTHORIZE_EXEC005"
    assert x.full_matrix_document()["matrix_sha256"]==m5["matrix_sha256"]
    assert x.shard_plan_document(m5)["shard_plan_sha256"]==x.shard_plan_document(m5)["shard_plan_sha256"]


def test_preflight_is_pass_but_frozen_artifacts_never_authorize_full(tmp_path):
    x.write_artifacts(tmp_path)
    assert x.preflight(tmp_path)=={"status":"PASS","failures":[],"full_execution_authorized":False,"qualification_required":True}
    exe=x.artifact_documents()[x.ARTIFACT_PREFIX+"executable-contract.json"]
    assert exe["status"]=="FROZEN_NOT_AUTHORIZED" and exe["full_execution_authorized"] is False


def test_live_without_authority_refuses_before_client_construction(tmp_path):
    x.initialize_run(tmp_path); constructed=[]
    def live(): constructed.append(True); return OpenCodeRuntimeClient()
    with pytest.raises(ExecutionRefused,match="before client construction"): x.run_shard(tmp_path,0,1,live)
    assert constructed==[]

@pytest.mark.parametrize("changes",[
    {"qualification__delivered":19},{"qualification__ambiguous":1},{"qualification__scientific_prompts_used":True},
    {"provenance__assistant_route_match":19},{"provenance__agent_match":19},{"provenance__tool_parts_total":1},
])
def test_bad_fresh_qualification_or_provenance_cannot_mint_authority(tmp_path,changes):
    q,p,_,_=_evidence(tmp_path,**changes)
    with pytest.raises(ExecutionRefused): x.mint_full_authority(q,p,"54467bd3d566ac82bbcc759cdff12ac240d99503",owner_authorized=True)


def test_exec004_qualification_can_never_authorize_exec005(tmp_path):
    q,p,_,_=_evidence(tmp_path)
    doc=json.loads(q.read_text());doc["schema"]="evidence-revision-002-exec004-qualification-v1";doc["execution_revision"]="004";x._atomic(q,doc)
    with pytest.raises(ExecutionRefused): x.mint_full_authority(q,p,"54467bd3d566ac82bbcc759cdff12ac240d99503",owner_authorized=True)


def test_valid_fresh_evidence_and_owner_intent_mint_authority_and_authorized_run_constructs_client(tmp_path):
    a,q,p,authority=_authority(tmp_path)
    assert authority["authority_sha256"]==x.sha({k:v for k,v in authority.items() if k!="authority_sha256"})
    run=tmp_path/"run";x.initialize_run(run,authority_path=a,qualification_path=q,provenance_path=p)
    assert json.loads((run/"run.json").read_text())["execution_authorized"] is True
    constructed=[]
    class StoppingOpenCode(OpenCodeRuntimeClient):
        def __init__(self): super().__init__(); constructed.append(True)
        def complete(self,*args): raise InjectedInterruption("test stops after authorized construction")
    with pytest.raises(InjectedInterruption): x.run_shard(run,0,1,StoppingOpenCode)
    assert constructed==[True]

@pytest.mark.parametrize("field",["matrix_sha256","executable_contract_fingerprint","qualification_sha256"])
def test_tampered_authority_binding_refuses_before_client_construction(tmp_path,field):
    a,q,p,doc=_authority(tmp_path);doc[field]="0"*64;x._atomic(a,doc);run=tmp_path/"run"
    with pytest.raises(ExecutionRefused): x.initialize_run(run,authority_path=a,qualification_path=q,provenance_path=p)


def test_malformed_authority_refuses(tmp_path):
    a,q,p,_=_authority(tmp_path);a.write_text("[]")
    with pytest.raises(ExecutionRefused): x.initialize_run(tmp_path/"run",authority_path=a,qualification_path=q,provenance_path=p)


def test_offline_fake_topology_and_merge_verify_430_are_equivalent(tmp_path):
    one,four=tmp_path/"one",tmp_path/"four"
    x.run_fake_plan(one,1);x.run_fake_plan(four,4)
    assert x.verify(one)["results"]==430 and x.verify(four)["results"]==430
    assert x.scientific_projection(one)==x.scientific_projection(four)


def test_crash_and_ambiguous_replacement_preserve_prior_attempt_and_merge_rejects_two_settled(tmp_path):
    x.initialize_run(tmp_path)
    def crash(where,n):
        if where=="after_raw_write_before_commit_record": raise InjectedInterruption("crash")
    with pytest.raises(InjectedInterruption): x.run_shard(tmp_path,0,1,DeterministicFakeClient,crash)
    # The pre-commit intent remains ambiguous and cannot be replayed.
    class Ambiguous:
        offline_only=True
        def complete(self,*args): raise AmbiguousTransport("fixture")
    assert x.run_shard(tmp_path,1,1,Ambiguous)["state"]=="ABORTED_AMBIGUOUS_TRANSPORT"
    assert x.run_shard(tmp_path,1,2,DeterministicFakeClient)["state"]=="SETTLED"
    assert json.loads((x._attempt_dir(tmp_path,1,1)/"shard.json").read_text())["state"]=="ABORTED_AMBIGUOUS_TRANSPORT"
    # A second settled attempt is an authoritative-overlap violation at merge.
    x.run_shard(tmp_path,2,1,DeterministicFakeClient);x.run_shard(tmp_path,2,2,DeterministicFakeClient)
    for shard in range(3,16): x.run_shard(tmp_path,shard,1,DeterministicFakeClient)
    with pytest.raises(x.InvariantViolation,match="exactly one SETTLED"): x.merge(tmp_path)


def test_qualification_bench_needs_explicit_intent_and_never_runs_here():
    with pytest.raises(ExecutionRefused): x.qualification_bench()



def test_changed_qualification_bytes_after_mint_refuse_by_artifact_sha(tmp_path):
    a,q,p,_=_authority(tmp_path)
    changed=json.loads(q.read_text());changed["audit_note"]="tampered";x._atomic(q,changed)
    with pytest.raises(ExecutionRefused,match="authority binding drift"):
        x.initialize_run(tmp_path/"run",authority_path=a,qualification_path=q,provenance_path=p)
