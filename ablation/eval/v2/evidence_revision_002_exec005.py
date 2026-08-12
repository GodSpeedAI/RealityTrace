"""Exec005 coordinator: isolated settlement shards for frozen Evidence Revision 002.

This module deliberately has no command that can launch confirmatory FULL calls.
`run-shard` is an offline/test primitive unless a future owner-authorized release
adds a separate, reviewed authority adapter.  Qualification uses synthetic prompts only.
"""
from __future__ import annotations
import argparse, hashlib, json, os, re, shutil, tempfile, time, subprocess
from functools import lru_cache
from concurrent.futures import ThreadPoolExecutor, ProcessPoolExecutor, as_completed
from pathlib import Path
from typing import Callable

from .case_inputs_v2 import initial_state_for
from .controller import DiagnosticController, ModelClient, UnitResult
from .evidence_revision_002 import (ARMS, EVIDENCE_SELECTION_EXPERIMENT_ID, EXPERIMENT_ID,
                                    FULL_NAMESPACE, FULL_REPETITIONS, PRIMARY_ENDPOINT,
                                    contract_fingerprint, freeze as freeze_science)
from .evidence_revision_002_freeze import (FULL_MATRIX_NAME as OLD_MATRIX_NAME, OUT, ROOT,
                                            build_artifacts as old_build_artifacts)
from .evidence_revision_run import (CheckpointStore, DeterministicFakeClient, ExecutionRefused,
                                    InjectedInterruption, _canonical, _file_hash, _sha256)
from .ids import unit_id
from .ledger import CommitLedger, CommitViolation
from .marker_neutralization import assert_marker_free
from .replication import COSTS, DESCRIPTIONS, PROBES, VOCAB
from .sufficiency import LEGACY_CONTRACT
from .validity_revision import DIRECT_CANDIDATE_UNIVERSE, EVIDENCE_POLICY, neutralized_observations
from .opencode_runtime_provider import (ENDPOINT, MODEL, PROVIDER_ROUTE, OpenCodeRuntimeClient,
                                        AmbiguousTransport, ProviderFailure, installed_runtime_version,
                                        RESTRICTED_AGENT_CONFIG)

EXECUTION_REVISION = "005"
SHARD_COUNT = 16
CONCURRENCY_TARGET = 4
SCHEMA = "evidence-revision-002-exec005-v1"
ARTIFACT_PREFIX = "evidence-revision-002-exec005-"
IMPL_FILES = (
 "ablation/eval/v2/evidence_revision_002.py", "ablation/eval/v2/evidence_revision_002_freeze.py",
 "ablation/eval/v2/controller.py", "ablation/eval/v2/channel.py", "ablation/eval/v2/dstate.py",
 "ablation/eval/v2/decision.py", "ablation/eval/v2/execution.py", "ablation/eval/v2/ledger.py",
 "ablation/eval/v2/ids.py", "ablation/eval/v2/case_inputs_v2.py", "ablation/eval/v2/marker_neutralization.py",
 "ablation/eval/v2/sufficiency.py", "ablation/eval/v2/replication.py", "ablation/eval/v2/validity_revision.py",
 "ablation/eval/v2/opencode_runtime_provider.py", "ablation/eval/v2/evidence_revision_002_exec004.py",
 "ablation/eval/v2/evidence_revision_002_exec005.py")

class ShardAborted(RuntimeError): pass
class InvariantViolation(RuntimeError): pass

def canonical(value): return json.dumps(value, sort_keys=True, separators=(",", ":"), default=str)
def sha(value): return hashlib.sha256((value if isinstance(value, bytes) else canonical(value).encode())).hexdigest()
def _atomic(path: Path, value) -> None:
    path.parent.mkdir(parents=True, exist_ok=True); payload = canonical(value).encode()+b"\n"
    fd, temp = tempfile.mkstemp(prefix="."+path.name+".", dir=path.parent)
    try:
        with os.fdopen(fd, "wb") as f: f.write(payload); f.flush(); os.fsync(f.fileno())
        os.replace(temp, path); dfd=os.open(path.parent, os.O_DIRECTORY)
        try: os.fsync(dfd)
        finally: os.close(dfd)
    finally:
        if os.path.exists(temp): os.unlink(temp)

@lru_cache(maxsize=1)
def _old_artifacts():
    return old_build_artifacts()

@lru_cache(maxsize=1)
def frozen_runtime_version() -> str | None:
    # Local binary identity is execution provenance; cache within an artifact
    # build so repeated matrix/plan derivation does not change it mid-freeze.
    return installed_runtime_version()

def implementation_manifest():
    files={}
    for relative in IMPL_FILES:
        path=ROOT/relative
        if not path.is_file(): raise FileNotFoundError(relative)
        files[relative]=_file_hash(path)
    return {"schema":"evidence-revision-002-exec005-runner-implementation-v1", "files":files}

def executable_contract():
    old=_old_artifacts(); science=freeze_science(); manifest=implementation_manifest()
    # The old scientific object is referenced and hash-bound, never copied or altered.
    old_science=old["evidence-revision-002-contract.json"]
    return {"schema":"evidence-revision-002-exec005-executable-contract-v1", "experiment_id":EXPERIMENT_ID,
      "execution_revision":EXECUTION_REVISION, "status":"FROZEN_NOT_AUTHORIZED",
      "scientific_contract_fingerprint":contract_fingerprint(), "scientific_contract_sha256":sha(old_science),
      "scientific_contract_artifact":"evidence-revision-002-contract.json",
      "old_exec002_executable_fingerprint":old["evidence-revision-002-executable-contract.json"]["executable_contract_fingerprint"],
      "provider_contract":{"route":PROVIDER_ROUTE,"provider_id":"opencode","requested_model":MODEL,"endpoint":ENDPOINT,
                           "transport":"documented_opencode_serve_loopback_session_message_api","credential_env":None,
                           "runtime_version_at_freeze":frozen_runtime_version(),
                           "runtime_version_requirement":"opencode serve runtime reports its version during qualification",
                           "session_isolation":"one_new_session_per_RealityTrace_model_call",
                           "agent":"er002-transport","tools":"explicit_false_map_plus_deny_all_permissions",
                           "required_runtime_config":RESTRICTED_AGENT_CONFIG},
      "exec004_lineage":{"candidate_commit":"54467bd3d566ac82bbcc759cdff12ac240d99503",
                           "scientific_contract_fingerprint":"056284062a2ef6b2a6f6dc54ae63eceeb8802b66341d42ced7d305067d9e1d99",
                           "executable_contract_fingerprint":"9d2e665ebd8c9dee0bb438b5d1414bc1553d0c3d22578b012b934c440c0fff48",
                           "matrix_sha256":"3236cea03abca4cfaae31f698ec60af5f4813e8722a66a0fc9fbd5ba6965eff4",
                           "shard_plan_sha256":"d71b3ceb9177dcb5fe93980b74e972d816b060c4c43ceee9618d56c5edcd8255",
                           "qualification_sha256":"8f343bcb48377746a036aa2ad9d1fdcd3e4b5ae175dcfaee0d3d5b6bb7a2e843",
                           "provenance_audit_sha256":"9b3520b6676b3b021646620251e2317ec927ee9e99257a1461f0e9e3db5ff94a",
                           "qualification_disposition":"LINEAGE_ONLY_DOES_NOT_AUTHORIZE_EXEC005"},
      "settlement":{"shard_count":SHARD_COUNT,"frozen_concurrency_target":CONCURRENCY_TARGET,"replacement_policy":"OUTCOME_BLIND_EXECUTION_STATE_ONLY"},
      "runner_implementation":dict(implementation_manifest(), sha256=sha(manifest)),
      "excluded_attempts":[{"attempt":"001","terminal_state":"ABORTED_AMBIGUOUS_TRANSPORT","committed_units_excluded":75},
                           {"attempt":"002","terminal_state":"ABORTED_AMBIGUOUS_TRANSPORT","committed_units_excluded":222,
                            "ambiguous_unit":"eb51aaaeab572962","call":2}],
      "execution_authorized":False, "full_execution_authorized":False,
      "provider_calls_performed":0, "model_calls_performed":0}

def executable_fingerprint(): return sha(executable_contract())
def coordinates():
    old=_old_artifacts()[OLD_MATRIX_NAME]
    return tuple((case,arm,rep) for case in old["cases"] for arm in old["arms"] for rep in range(1,old["repetitions"]+1))
def full_matrix_document():
    exe=executable_fingerprint(); science=freeze_science(); coords=coordinates()
    uids=[unit_id(EXPERIMENT_ID,FULL_NAMESPACE,c,a,r,exe) for c,a,r in coords]
    doc={"schema":"evidence-revision-002-exec005-matrix-v1","experiment_id":EXPERIMENT_ID,"execution_revision":EXECUTION_REVISION,
         "stage":"full","namespace":FULL_NAMESPACE,"cases":list(_old_artifacts()[OLD_MATRIX_NAME]["cases"]),"arms":list(ARMS),
         "repetitions":FULL_REPETITIONS,"n_cases":43,"n_arms":5,"total_units":len(uids),"coordinates":[list(x) for x in coords],"unit_ids":uids,
         "design_fingerprint":science["design_fingerprint"],"executable_contract_fingerprint":exe,
         "predecessor_matrix_sha256":_old_artifacts()[OLD_MATRIX_NAME]["matrix_sha256"],"execution_authorized":False}
    doc["matrix_sha256"]=sha(doc); return doc

def shard_plan_document(matrix=None):
    matrix=matrix or full_matrix_document(); rows=[]
    # Hash ordering is coordinate-only; round-robin allocation makes the frozen
    # shards exactly balanced (14×27, 2×26), unlike direct modulo buckets.
    ranked=[]
    for coord,uid in zip(matrix["coordinates"],matrix["unit_ids"]):
        case,arm,rep=coord
        key=hashlib.sha256(canonical({"design_fingerprint":matrix["design_fingerprint"],"case_id":case,"arm":arm,"rep":rep}).encode()).hexdigest()
        ranked.append((key,case,arm,rep,uid))
    for index,(_,case,arm,rep,uid) in enumerate(sorted(ranked)):
        rows.append({"case_id":case,"arm":arm,"rep":rep,"unit_id":uid,"shard":index % SHARD_COUNT})
    rows.sort(key=lambda x: matrix["unit_ids"].index(x["unit_id"]))
    counts=[sum(x["shard"]==n for x in rows) for n in range(SHARD_COUNT)]
    if len(rows)!=430 or len({x["unit_id"] for x in rows})!=430 or any(n==0 for n in counts): raise InvariantViolation("invalid deterministic shard coverage")
    doc={"schema":"evidence-revision-002-exec005-shard-plan-v1","experiment_id":EXPERIMENT_ID,"design_fingerprint":matrix["design_fingerprint"],
         "matrix_sha256":matrix["matrix_sha256"],"shard_count":SHARD_COUNT,"membership":rows,"shard_sizes":counts}
    doc["shard_plan_sha256"]=sha(doc); return doc

def artifact_documents():
    exe=executable_contract(); fp=sha(exe); exe["executable_contract_fingerprint"]=fp
    # Fingerprint includes the complete executable declaration. Matrix must bind that final value.
    # Rebuild directly to avoid self-reference in the declaration.
    matrix=full_matrix_document(); plan=shard_plan_document(matrix)
    qualification={"schema":"evidence-revision-002-exec005-qualification-v1","status":"NOT_RUN","scientific_prompts_used":False,"calls":0,"required_workers":4,"calls_per_worker":5}
    equivalence={"schema":"evidence-revision-002-exec005-equivalence-v1","status":"OFFLINE_TEST_GATE_REQUIRED","scientific_prompts_used":False,"required":{"coordinates":430,"concurrency":[1,4],"parallel_randomized":True}}
    # executable_fingerprint() already returns the binding definition; expose it consistently.
    exe["executable_contract_fingerprint"]=matrix["executable_contract_fingerprint"]
    return {ARTIFACT_PREFIX+"executable-contract.json":exe, ARTIFACT_PREFIX+"full-matrix.json":matrix,
            ARTIFACT_PREFIX+"shard-plan.json":plan, ARTIFACT_PREFIX+"provider-contract.json":exe["provider_contract"],
            ARTIFACT_PREFIX+"qualification.json":qualification, ARTIFACT_PREFIX+"equivalence.json":equivalence,
            ARTIFACT_PREFIX+"runtime-config.json":RESTRICTED_AGENT_CONFIG,
            ARTIFACT_PREFIX+"runner-implementation.json":exe["runner_implementation"]}

def write_artifacts(out: Path=OUT):
    docs=artifact_documents(); out.mkdir(parents=True,exist_ok=True)
    for name,doc in docs.items(): _atomic(out/name,doc)
    return docs

def _attempt_dir(run: Path, shard: int, attempt: int): return run/"shards"/f"{shard:02d}"/"attempts"/f"{attempt:03d}"
def _rows(plan, shard): return [x for x in plan["membership"] if x["shard"]==shard]
def _binding(matrix,row): return sha({"experiment":EXPERIMENT_ID,"execution_revision":EXECUTION_REVISION,"fingerprint":matrix["executable_contract_fingerprint"],"coordinate":[row["case_id"],row["arm"],row["rep"]]})

class UnitReplay:
    """One writer owns all state below a unit directory; raw is committed before return."""
    def __init__(self, client, unit_dir:Path, row, matrix, interrupt=None):
        self.client,self.unit_dir,self.row,self.matrix=client,unit_dir,row,matrix; self.calls=0; self.interrupt=interrupt or (lambda where,n:None)
        self.state_path=unit_dir/"checkpoint.json"; self.ledger=CommitLedger(str(unit_dir/"call-commits.jsonl"))
    def _state(self): return json.loads(self.state_path.read_text()) if self.state_path.exists() else {"status":"PLANNED","binding":_binding(self.matrix,self.row)}
    def _set(self,status,**extra):
        old=self._state(); binding=_binding(self.matrix,self.row)
        if old.get("binding")!=binding: raise InvariantViolation("unit binding drift")
        _atomic(self.state_path,dict(old, status=status, **extra))
    def complete(self,system,user,requested_max_tokens,temperature):
        self.calls+=1; n=self.calls; call_id=f"{self.row['unit_id']}:call:{n:03d}"; raw=self.unit_dir/"raw"/f"{n:03d}.txt"; calls=self.unit_dir/"calls"/f"{n:03d}"; intent=calls/"001.json"
        entry=self.ledger.get(call_id)
        if entry:
            digest=self.ledger.assert_scorable(call_id); side=json.loads((calls/"response.json").read_text())
            if digest!=side.get("raw_hash"): raise InvariantViolation("raw sidecar drift")
            return {"text":raw.read_text(),"provider":side["provider"]}
        if intent.exists() or raw.exists() or (calls/"response.json").exists():
            # An uncommitted intent could have reached a provider. Never issue a replacement call.
            raise AmbiguousTransport("uncommitted durable call intent requires shard replacement")
        self._set("CALLING",call_num=n); self.interrupt("before_call_intent",n)
        request={"schema":"exec005-call-intent-v1","unit_id":self.row["unit_id"],"call_num":n,"attempt_num":1,"status":"INTENT","system_sha256":hashlib.sha256(system.encode()).hexdigest(),"user_sha256":hashlib.sha256(user.encode()).hexdigest(),"requested_max_tokens":requested_max_tokens,"temperature":temperature}
        _atomic(intent,request)
        # The OpenCode runtime adapter emits intent before *every* loopback HTTP send.  This sink
        # adds the unit binding and writes one immutable per-attempt record.
        def attempt_sink(event):
            number=event.get("attempt_num")
            if not isinstance(number,int) or number < 1: raise InvariantViolation("invalid transport attempt")
            # Intent and terminal outcome are separate immutable records.  The
            # outcome never overwrites the pre-send durable intent.
            operation=event.get("operation")
            if not isinstance(operation,str) or not operation: raise InvariantViolation("transport operation absent")
            path=calls/"transport"/(f"{number:03d}-{operation}-{str(event.get('status')).lower()}.json")
            doc={"schema":"exec005-transport-attempt-v1","unit_id":self.row["unit_id"],"unit_binding":_binding(self.matrix,self.row),"call_num":n,"request_sha256":sha(request),**event}
            if path.exists() and json.loads(path.read_text()) != doc: raise InvariantViolation("transport attempt mutation")
            _atomic(path,doc)
        binder=getattr(self.client,"set_attempt_sink",None)
        if callable(binder): binder(attempt_sink)
        self.interrupt("after_call_intent_before_transport",n)
        response=self.client.complete(system,user,requested_max_tokens,temperature)
        self.interrupt("after_provider_response_before_raw_commit",n)
        if not isinstance(response,dict) or not isinstance(response.get("text"),str) or not isinstance(response.get("provider",{}),dict): raise InvariantViolation("invalid provider response")
        # response sidecar is durable, then CommitLedger writes/fsyncs raw, hashes disk bytes and fsyncs commit.
        _atomic(calls/"response.json",{"raw_hash":hashlib.sha256(response["text"].encode()).hexdigest(),"provider":response["provider"]})
        raw.parent.mkdir(parents=True,exist_ok=True); raw.write_bytes(response["text"].encode())
        # The deliberate pre-commit boundary is observable to crash injection.
        with raw.open("ab") as f: f.flush(); os.fsync(f.fileno())
        self.interrupt("after_raw_write_before_commit_record",n)
        committed=self.ledger.commit_raw(call_id,str(raw),response["text"])
        if committed.raw_hash != hashlib.sha256(response["text"].encode()).hexdigest(): raise InvariantViolation("on-disk raw hash mismatch")
        self.interrupt("after_raw_commit_before_parse",n); self._set("RAW_COMMITTED",call_num=n,raw_hash=committed.raw_hash)
        return {"text":raw.read_text(),"provider":response["provider"]}

def _run_unit(unit_dir,row,matrix,client,interrupt=None):
    state_path=unit_dir/"checkpoint.json"; existing=json.loads(state_path.read_text()) if state_path.exists() else {"status":"PLANNED"}
    result_path=unit_dir/"result.json"; commit_path=unit_dir/"result-commit.json"
    if existing.get("status")=="RESULT_COMMITTED":
        commit=json.loads(commit_path.read_text()); actual=_file_hash(result_path)
        if actual!=commit.get("result_sha256"): raise InvariantViolation("result commit hash mismatch")
        return json.loads(result_path.read_text())
    replay=UnitReplay(client,unit_dir,row,matrix,interrupt)
    initial=initial_state_for(row["case_id"]); observations=neutralized_observations()[0][row["case_id"]]; assert_marker_free(initial); assert_marker_free(observations)
    hidden=json.loads((ROOT / "ablation/corpus/acquisition-hidden-meta.json").read_text())
    discriminating=tuple(hidden[row["case_id"]]["discriminating_probes"])
    ctrl=DiagnosticController(ARMS[row["arm"]],LEGACY_CONTRACT,PROBES,DESCRIPTIONS,COSTS,VOCAB,replay,
       endpoint=ENDPOINT,requested_model=MODEL,evidence_selection_policy=EVIDENCE_POLICY[row["arm"]],disclosure_k=3,
       masked_candidate_universe=(DIRECT_CANDIDATE_UNIVERSE if EVIDENCE_POLICY[row["arm"]]=="MASKED_K" else ()))
    result=ctrl.run_unit(row["unit_id"],EVIDENCE_SELECTION_EXPERIMENT_ID,FULL_NAMESPACE,row["case_id"],row["rep"],initial["question"],initial["initial_evidence"],observations,discriminating)
    replay._set("PARSED")
    doc=result.to_dict(); doc["exec005"]={"coordinate":[row["case_id"],row["arm"],row["rep"]],"executable_fingerprint":matrix["executable_contract_fingerprint"],"matrix_sha256":matrix["matrix_sha256"],"unit_binding":_binding(matrix,row)}
    _atomic(result_path,doc); interrupt and interrupt("after_result_write_before_result_commit",0)
    raw_hashes=[]
    for execution in doc["executions"]:
        cid=f"{row['unit_id']}:call:{execution['call_num']:03d}"; digest=replay.ledger.assert_scorable(cid)
        if digest!=execution["raw_hash"]: raise InvariantViolation("result raw binding lost")
        raw_hashes.append(digest)
    _atomic(commit_path,{"schema":"exec005-result-commit-v1","unit_id":row["unit_id"],"result_sha256":_file_hash(result_path),"raw_hashes":raw_hashes})
    replay._set("RESULT_COMMITTED",result_sha256=_file_hash(result_path)); interrupt and interrupt("after_result_commit",0)
    return doc

AUTHORITY_SCHEMA = "evidence-revision-002-exec005-full-authority-v1"
QUALIFICATION_SCHEMA = "evidence-revision-002-exec005-qualification-v1"
PROVENANCE_SCHEMA = "evidence-revision-002-exec005-provenance-audit-v1"


def _read_doc(path: Path) -> dict:
    try:
        value=json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as exc:
        raise ExecutionRefused("authority evidence is unreadable") from exc
    if not isinstance(value,dict): raise ExecutionRefused("authority evidence must be an object")
    return value

def _required(value:dict, expected:dict, label:str) -> None:
    for key,want in expected.items():
        if value.get(key) != want: raise ExecutionRefused(f"{label} binding failed: {key}")

def _current_provider() -> dict:
    return executable_contract()["provider_contract"]

def validate_qualification(qualification:dict, provenance:dict) -> None:
    """Validate the *contents* of fresh exec005 synthetic evidence, not hashes alone."""
    provider=_current_provider()
    _required(qualification,{"schema":QUALIFICATION_SCHEMA,"execution_revision":EXECUTION_REVISION,
        "status":"PASS","calls":20,"delivered":20,"ambiguous":0,"scientific_prompts_used":False,
        "provider":"opencode","model":MODEL,"agent":"er002-transport",
        "runtime_version":provider["runtime_version_at_freeze"]},"qualification")
    run_id=qualification.get("run_id")
    if not isinstance(run_id,str) or not run_id: raise ExecutionRefused("qualification binding failed: run_id")
    _required(provenance,{"schema":PROVENANCE_SCHEMA,"execution_revision":EXECUTION_REVISION,"pass":True,
        "qualification_run_id":run_id,"sessions_audited":20,"assistant_route_match":20,
        "user_route_match":20,"agent_match":20,"tool_parts_total":0,"provider":"opencode",
        "model":MODEL,"agent":"er002-transport","runtime_version":provider["runtime_version_at_freeze"]},"provenance")

def _authority_core(qualification:dict, provenance:dict, candidate_commit:str, timestamp:str,
                    qualification_sha256:str|None=None, provenance_audit_sha256:str|None=None) -> dict:
    exe=executable_contract(); matrix=full_matrix_document(); plan=shard_plan_document(matrix)
    return {"schema":AUTHORITY_SCHEMA,"experiment_id":EXPERIMENT_ID,"execution_revision":EXECUTION_REVISION,
      "scientific_contract_fingerprint":exe["scientific_contract_fingerprint"],
      "executable_contract_fingerprint":matrix["executable_contract_fingerprint"],
      "runner_implementation_manifest_sha256":exe["runner_implementation"]["sha256"],
      "matrix_sha256":matrix["matrix_sha256"],"shard_plan_sha256":plan["shard_plan_sha256"],
      "reviewed_candidate_git_commit":candidate_commit,"qualification_sha256":qualification_sha256 or sha(qualification),
      "provenance_audit_sha256":provenance_audit_sha256 or sha(provenance),"qualification_run_id":qualification["run_id"],
      "provider":"opencode","model":MODEL,"agent":"er002-transport",
      "opencode_runtime_version":exe["provider_contract"]["runtime_version_at_freeze"],
      "owner_authorized":True,"authorization_timestamp":timestamp}

def mint_full_authority(qualification_path:Path, provenance_path:Path, candidate_commit:str, *,
                        owner_authorized:bool=False, authorization_timestamp:str|None=None) -> dict:
    """Mint an immutable, hash-bound authority only after fresh PASS evidence."""
    if not owner_authorized: raise ExecutionRefused("explicit --owner-authorized is required")
    if not isinstance(candidate_commit,str) or not re.fullmatch(r"[0-9a-f]{40}",candidate_commit):
        raise ExecutionRefused("reviewed candidate commit must be a full SHA-1")
    try: current_commit=subprocess.check_output(["git","-C",str(ROOT),"rev-parse","HEAD"],text=True,stderr=subprocess.DEVNULL).strip()
    except (OSError, subprocess.SubprocessError) as exc: raise ExecutionRefused("cannot verify reviewed candidate commit") from exc
    if candidate_commit != current_commit: raise ExecutionRefused("reviewed candidate commit is not the checked-out candidate")
    qualification,provenance=_read_doc(qualification_path),_read_doc(provenance_path)
    validate_qualification(qualification,provenance)
    timestamp=authorization_timestamp or time.strftime("%Y-%m-%dT%H:%M:%SZ",time.gmtime())
    core=_authority_core(qualification,provenance,candidate_commit,timestamp,
                         _file_hash(qualification_path),_file_hash(provenance_path))
    return dict(core,authority_sha256=sha(core))

def validate_full_authority(authority:dict, qualification:dict, provenance:dict,
                            qualification_sha256:str|None=None, provenance_audit_sha256:str|None=None) -> dict:
    """Fail closed on every identity/evidence/hash mismatch before client construction."""
    if not isinstance(authority,dict) or authority.get("schema")!=AUTHORITY_SCHEMA:
        raise ExecutionRefused("invalid FULL authority schema")
    value=dict(authority); received=value.pop("authority_sha256",None)
    if not isinstance(received,str) or received!=sha(value): raise ExecutionRefused("authority document hash mismatch")
    if authority.get("owner_authorized") is not True: raise ExecutionRefused("FULL authority lacks owner authorization")
    validate_qualification(qualification,provenance)
    expected=_authority_core(qualification,provenance,authority.get("reviewed_candidate_git_commit"),
                             authority.get("authorization_timestamp"),qualification_sha256,provenance_audit_sha256)
    if authority != dict(expected,authority_sha256=sha(expected)):
        raise ExecutionRefused("authority binding drift")
    return authority

def _copy_sealed(source:Path, destination:Path) -> None:
    payload=source.read_bytes(); destination.parent.mkdir(parents=True,exist_ok=True)
    fd,temp=tempfile.mkstemp(prefix="."+destination.name+".",dir=destination.parent)
    try:
        with os.fdopen(fd,"wb") as f: f.write(payload); f.flush(); os.fsync(f.fileno())
        os.replace(temp,destination); dfd=os.open(destination.parent,os.O_DIRECTORY)
        try: os.fsync(dfd)
        finally: os.close(dfd)
    finally:
        if os.path.exists(temp): os.unlink(temp)

def initialize_run(run:Path,matrix=None,plan=None,*,authority_path:Path|None=None,
                   qualification_path:Path|None=None,provenance_path:Path|None=None):
    """Coordinator-only identity creation; workers only verify these sealed bytes."""
    matrix=matrix or full_matrix_document(); plan=plan or shard_plan_document(matrix)
    authorized=authority_path is not None or qualification_path is not None or provenance_path is not None
    if authorized:
        if not all((authority_path,qualification_path,provenance_path)): raise ExecutionRefused("authority, qualification and provenance paths are all required")
        authority=_read_doc(authority_path); qualification=_read_doc(qualification_path); provenance=_read_doc(provenance_path)
        validate_full_authority(authority,qualification,provenance,_file_hash(qualification_path),_file_hash(provenance_path))
        identity={"schema":SCHEMA,"experiment_id":EXPERIMENT_ID,"execution_revision":EXECUTION_REVISION,
          "matrix_sha256":matrix["matrix_sha256"],"shard_plan_sha256":plan["shard_plan_sha256"],
          "authority_sha256":authority["authority_sha256"],"execution_authorized":True}
    else:
        identity={"schema":SCHEMA,"experiment_id":EXPERIMENT_ID,"execution_revision":EXECUTION_REVISION,
          "matrix_sha256":matrix["matrix_sha256"],"shard_plan_sha256":plan["shard_plan_sha256"],"execution_authorized":False}
    if (run/"run.json").exists() and json.loads((run/"run.json").read_text())!=identity: raise InvariantViolation("run identity drift")
    _atomic(run/"run.json",identity); _atomic(run/"plan.json",plan)
    if authorized:
        _copy_sealed(authority_path,run/"authority"/"full-authority.json")
        _copy_sealed(qualification_path,run/"authority"/"qualification.json")
        _copy_sealed(provenance_path,run/"authority"/"provenance-audit.json")
    return identity

def _run_authorization(run:Path, matrix:dict, plan:dict) -> bool:
    if not (run/"run.json").is_file() or not (run/"plan.json").is_file():
        raise ExecutionRefused("coordinator must initialize a run before workers start")
    identity=_read_doc(run/"run.json")
    basic={"schema":SCHEMA,"experiment_id":EXPERIMENT_ID,"execution_revision":EXECUTION_REVISION,
           "matrix_sha256":matrix["matrix_sha256"],"shard_plan_sha256":plan["shard_plan_sha256"]}
    if any(identity.get(k)!=v for k,v in basic.items()) or _read_doc(run/"plan.json") != plan:
        raise InvariantViolation("coordinator run/plan identity drift")
    if identity.get("execution_authorized") is False: return False
    if identity.get("execution_authorized") is not True: raise ExecutionRefused("run authority flag invalid")
    authority=_read_doc(run/"authority"/"full-authority.json")
    qualification=_read_doc(run/"authority"/"qualification.json")
    provenance=_read_doc(run/"authority"/"provenance-audit.json")
    validate_full_authority(authority,qualification,provenance,
                            _file_hash(run/"authority"/"qualification.json"),
                            _file_hash(run/"authority"/"provenance-audit.json"))
    if identity.get("authority_sha256") != authority["authority_sha256"]: raise ExecutionRefused("run authority hash drift")
    return True

def _offline_factory(factory:Callable[[],ModelClient]) -> bool:
    return factory is DeterministicFakeClient or bool(getattr(factory,"offline_only",False))

def run_shard(run:Path, shard:int, attempt:int, client_factory:Callable[[],ModelClient], interrupt=None):
    matrix=full_matrix_document(); plan=shard_plan_document(matrix)
    authorized=_run_authorization(run,matrix,plan)
    # Known live transport and unclassified factories are rejected before the
    # callable is invoked. Offline test factories advertise offline_only.
    if not authorized and not _offline_factory(client_factory):
        raise ExecutionRefused("exec005 FULL authority is required before client construction")
    if shard not in range(SHARD_COUNT) or attempt<1: raise ValueError("invalid shard/attempt")
    directory=_attempt_dir(run,shard,attempt); status=directory/"shard.json"; rows=_rows(plan,shard)
    ident={"schema":"exec005-shard-attempt-v1","identity":f"shard-{shard:02d}-a{attempt:03d}","shard":shard,"attempt":attempt,"matrix_sha256":matrix["matrix_sha256"],"coordinates":[[r["case_id"],r["arm"],r["rep"]] for r in rows],"state":"ACTIVE","pid":os.getpid()}
    if status.exists():
        old=json.loads(status.read_text())
        if {k:old.get(k) for k in ("identity","shard","attempt","matrix_sha256","coordinates")} != {k:ident[k] for k in ("identity","shard","attempt","matrix_sha256","coordinates")}: raise InvariantViolation("shard attempt identity drift")
        if old.get("state")=="SETTLED" or old.get("state","").startswith("ABORTED"): return old
    _atomic(status,ident)
    try:
        # This is the first point where a real OpenCode client can be made.
        client=client_factory()
        if not authorized and not getattr(client,"offline_only",False): raise ExecutionRefused("offline run received live client")
        for row in rows: _run_unit(directory/"units"/row["unit_id"],row,matrix,client,interrupt)
        final=dict(ident,state="SETTLED",completed_units=len(rows)); _atomic(status,final); return final
    except AmbiguousTransport as exc:
        final=dict(ident,state="ABORTED_AMBIGUOUS_TRANSPORT",reason=str(exc)); _atomic(status,final); return final
    except ProviderFailure as exc:
        final=dict(ident,state="ABORTED_FATAL_PROVIDER",reason=str(exc),classification=exc.classification); _atomic(status,final); return final
    except BaseException as exc:
        if isinstance(exc, InjectedInterruption): raise
        final=dict(ident,state="ABORTED_INVARIANT",reason=f"{type(exc).__name__}: {exc}"); _atomic(status,final); return final

def _fake_process_worker(run_text: str, shard: int):
    """Pickle-safe worker entrypoint; each process creates its own fake client."""
    return run_shard(Path(run_text), shard, 1, DeterministicFakeClient)

def run_fake_plan(run:Path, concurrency:int=1, interrupt_factory=None):
    matrix=full_matrix_document(); plan=shard_plan_document(matrix); initialize_run(run,matrix,plan)
    def one(shard): return run_shard(run,shard,1,DeterministicFakeClient, (interrupt_factory(shard) if interrupt_factory else None))
    if concurrency==1: return [one(n) for n in range(SHARD_COUNT)]
    if interrupt_factory is not None: raise ValueError("cross-process fake workers do not accept injected callbacks")
    # This deliberately uses OS processes (not threads), mirroring the frozen
    # four-worker execution topology while retaining an offline-only client.
    with ProcessPoolExecutor(max_workers=concurrency) as pool:
        return list(as_completed([pool.submit(_fake_process_worker,str(run),n) for n in reversed(range(SHARD_COUNT))]))

def merge(run:Path):
    matrix=full_matrix_document(); plan=shard_plan_document(matrix); selected={}
    for shard in range(SHARD_COUNT):
        attempts=[]
        base=run/"shards"/f"{shard:02d}"/"attempts"
        for d in sorted(base.glob("*")) if base.exists() else []:
            p=d/"shard.json"
            if p.exists() and json.loads(p.read_text()).get("state")=="SETTLED": attempts.append((d,json.loads(p.read_text())))
        if len(attempts)!=1: raise InvariantViolation(f"shard {shard:02d} needs exactly one SETTLED attempt")
        selected[shard]=attempts[0][0]
    results=[]; calls=[]; result_commits=[]
    for row in plan["membership"]:
        unit=selected[row["shard"]]/"units"/row["unit_id"]; result_path=unit/"result.json"; result_commit=unit/"result-commit.json"
        if not result_path.is_file() or not result_commit.is_file(): raise InvariantViolation("missing result commit")
        doc=json.loads(result_path.read_text()); commit=json.loads(result_commit.read_text())
        if _file_hash(result_path)!=commit.get("result_sha256") or doc.get("unit_id")!=row["unit_id"] or doc.get("exec005",{}).get("coordinate")!=[row["case_id"],row["arm"],row["rep"]] or doc["exec005"].get("executable_fingerprint")!=matrix["executable_contract_fingerprint"]: raise InvariantViolation("result identity/hash mismatch")
        ledger=CommitLedger(str(unit/"call-commits.jsonl")); seen=[]
        for execution in doc.get("executions",[]):
            cid=f"{row['unit_id']}:call:{execution['call_num']:03d}"; digest=ledger.assert_scorable(cid)
            if digest!=execution["raw_hash"]: raise InvariantViolation("raw mismatch")
            seen.append(digest); calls.append({"unit_id":row["unit_id"],"call_num":execution["call_num"],"raw_hash":digest,"hash_provenance":"VERIFIED"})
        if seen!=commit.get("raw_hashes"): raise InvariantViolation("result commit raw list mismatch")
        results.append(doc); result_commits.append({"unit_id":row["unit_id"],"result_sha256":commit["result_sha256"]})
    if len(results)!=430 or len({x["unit_id"] for x in results})!=430: raise InvariantViolation("merged coverage invalid")
    out=run/"merged"; out.mkdir(parents=True,exist_ok=True)
    for doc in results: _atomic(out/"results"/(doc["unit_id"]+".json"),doc)
    # JSONL output is written by one merger in canonical matrix/call order.
    for path,rows in ((out/"call-commits.jsonl",calls),(out/"result-commits.jsonl",result_commits)):
        data="".join(canonical(x)+"\n" for x in rows).encode(); path.write_bytes(data)
        with path.open("ab") as f:f.flush();os.fsync(f.fileno())
    summary={"schema":"exec005-merged-v1","matrix_sha256":matrix["matrix_sha256"],"shard_plan_sha256":plan["shard_plan_sha256"],"results":430,"calls":len(calls),"selected_attempts":{f"{k:02d}":str(v.relative_to(run)) for k,v in selected.items()}}
    _atomic(out/"execution-summary.json",summary); return summary

def scientific_projection(run:Path):
    """Canonical scientific result projection, excluding operational provenance.

    Raw/result ledgers remain separately verified by ``merge``.  Per-call
    latency, runtime session/message IDs, worker process IDs and scheduling are
    transport observations, not scientific outputs, so cannot influence this
    equivalence projection.
    """
    rows=[]
    for path in sorted((run/"merged"/"results").glob("*.json")):
        doc=json.loads(path.read_text())
        doc.pop("executions",None)
        rows.append((path.name,canonical(doc).encode()+b"\n"))
    return rows
def status(run:Path):
    answer=[]
    for shard in range(SHARD_COUNT):
        base=run/"shards"/f"{shard:02d}"/"attempts"; rows=[]
        for d in sorted(base.glob("*")) if base.exists() else []:
            p=d/"shard.json"; doc=json.loads(p.read_text()) if p.exists() else {"state":"STALE"}; state=doc.get("state")
            if state=="ACTIVE":
                pid=doc.get("pid"); alive=isinstance(pid,int) and pid>0
                if alive:
                    try: os.kill(pid,0)
                    except OSError: alive=False
                state="ACTIVE" if alive and doc.get("matrix_sha256")==full_matrix_document()["matrix_sha256"] else "EXITED_FAILED" if not alive else "STALE"
            rows.append({"attempt":doc.get("attempt"),"identity":doc.get("identity"),"state":state})
        answer.append({"shard":shard,"attempts":rows,"state":("SETTLED" if any(x["state"]=="SETTLED" for x in rows) else rows[-1]["state"] if rows else "STALE")})
    return {"shards":answer}

def verify(run:Path):
    summary=merge(run); return dict(summary,verification="PASS")
def preflight(out:Path=OUT):
    docs=artifact_documents(); failures=[]
    for name,expected in docs.items():
        p=out/name
        if not p.is_file() or json.loads(p.read_text())!=expected: failures.append(name)
    old=_old_artifacts()["evidence-revision-002-contract.json"]
    if sha(old)!=docs[ARTIFACT_PREFIX+"executable-contract.json"]["scientific_contract_sha256"]: failures.append("scientific contract")
    return {"status":"PASS" if not failures else "FAIL","failures":failures,"full_execution_authorized":False,"qualification_required":True}

def qualification_bench(output: Path | None = None, client_factory: Callable[[], ModelClient] | None = None,
                      *, owner_authorized: bool = False, run_id: str | None = None):
    """Run the 20-call synthetic-only qualification protocol when separately authorized.

    It deliberately does not load a coordinate, case input, evidence delivery, or
    analysis result.  This invocation is unavailable from the CLI until an owner
    supplies a reviewed non-scientific qualification release mechanism.
    """
    if not owner_authorized or output is None or client_factory is None or not isinstance(run_id,str) or not run_id:
        raise ExecutionRefused("qualification needs separate owner authority, output, run ID and client factory")
    output.mkdir(parents=True, exist_ok=True)
    def one(worker):
        client=client_factory(); deliveries=[]
        for number in range(5):
            started=time.monotonic()
            try:
                response=client.complete("Return one JSON object only.",
                    '{"synthetic":true,"call":%d,"shape":"diagnostic-decision"}' % number,8192,0.0)
                text=response.get("text") if isinstance(response,dict) else None
                provider=response.get("provider",{}) if isinstance(response,dict) else {}
                deliveries.append({"worker":worker,"call":number,"classification":"DELIVERED", "latency_ms":(provider.get("latency_ms") if isinstance(provider,dict) else None),"finish_reason":provider.get("finish_reason") if isinstance(provider,dict) else None,"resolved_model":provider.get("resolved_model") if isinstance(provider,dict) else None,"content_is_text":isinstance(text,str)})
            except AmbiguousTransport as exc: deliveries.append({"worker":worker,"call":number,"classification":"AMBIGUOUS_TRANSPORT","reason":str(exc)})
            except ProviderFailure as exc: deliveries.append({"worker":worker,"call":number,"classification":exc.classification,"http_status":exc.status,"reason":str(exc)})
        return deliveries
    with ThreadPoolExecutor(max_workers=4) as pool: rows=[r for f in [pool.submit(one,w) for w in range(4)] for r in f.result()]
    latencies=sorted(r["latency_ms"] for r in rows if isinstance(r.get("latency_ms"),(int,float)))
    delivered=sum(r["classification"]=="DELIVERED" for r in rows); ambiguous=sum(r["classification"]=="AMBIGUOUS_TRANSPORT" for r in rows)
    contract=_current_provider()
    result={"schema":QUALIFICATION_SCHEMA,"execution_revision":EXECUTION_REVISION,"run_id":run_id,"scientific_prompts_used":False,
      "provider":"opencode","model":MODEL,"agent":"er002-transport","runtime_version":contract["runtime_version_at_freeze"],
      "calls":20,"rows":rows,"delivered":delivered,"ambiguous":ambiguous,
      "status":"PASS" if delivered==20 and ambiguous==0 and all(r.get("content_is_text") for r in rows) else "FAIL",
      "latency_ms":{"p50":latencies[len(latencies)//2] if latencies else None,"p95":latencies[min(len(latencies)-1, int(len(latencies)*.95))] if latencies else None,"max":latencies[-1] if latencies else None}}
    _atomic(output/"qualification.json",result); return result

def main(argv=None):
    parser=argparse.ArgumentParser(); sub=parser.add_subparsers(dest="command",required=True)
    for name in ("build-artifacts","inspect","preflight","status","merge","verify","qualification-bench","authorize-full","run-shard"):
        q=sub.add_parser(name); q.add_argument("--run",type=Path); q.add_argument("--shard",type=int); q.add_argument("--attempt",type=int)
        if name == "qualification-bench":
            q.add_argument("--output", type=Path); q.add_argument("--run-id",required=True)
            q.add_argument("--owner-authorized", action="store_true",help="explicit authorization for synthetic-only qualification")
        if name == "authorize-full":
            q.add_argument("--qualification",type=Path,required=True); q.add_argument("--provenance-audit",type=Path,required=True)
            q.add_argument("--candidate-commit",required=True); q.add_argument("--output",type=Path,required=True)
            q.add_argument("--owner-authorized",action="store_true",help="explicit owner intent to mint per-run FULL authority")
        if name == "run-shard": q.add_argument("--authority",type=Path); q.add_argument("--qualification",type=Path); q.add_argument("--provenance-audit",type=Path)
    args=parser.parse_args(argv)
    if args.command=="build-artifacts": result=write_artifacts()
    elif args.command=="inspect": result={"matrix":full_matrix_document(),"plan":shard_plan_document()}
    elif args.command=="preflight": result=preflight()
    elif args.command=="status": result=status(args.run)
    elif args.command in ("merge","verify"): result=(merge if args.command=="merge" else verify)(args.run)
    elif args.command=="qualification-bench":
        result=qualification_bench(args.output,OpenCodeRuntimeClient,owner_authorized=args.owner_authorized,run_id=args.run_id)
    elif args.command=="authorize-full":
        result=mint_full_authority(args.qualification,args.provenance_audit,args.candidate_commit,owner_authorized=args.owner_authorized)
        _atomic(args.output,result)
    else:
        # A coordinator initialization also validates all three sealed authority
        # documents before OpenCodeRuntimeClient is constructed.
        if not args.run or args.shard is None or args.attempt is None: raise ExecutionRefused("run-shard needs --run --shard --attempt")
        if not (args.authority and args.qualification and args.provenance_audit): raise ExecutionRefused("run-shard needs sealed --authority --qualification --provenance-audit")
        # First invocation is the coordinator transition; later workers only
        # verify the existing run identity and copied sealed evidence.
        if not (args.run/"run.json").exists():
            initialize_run(args.run,authority_path=args.authority,qualification_path=args.qualification,provenance_path=args.provenance_audit)
        else:
            _run_authorization(args.run,full_matrix_document(),shard_plan_document())
        result=run_shard(args.run,args.shard,args.attempt,OpenCodeRuntimeClient)
    print(json.dumps(result,sort_keys=True,indent=2)); return 0
if __name__=="__main__": raise SystemExit(main())
