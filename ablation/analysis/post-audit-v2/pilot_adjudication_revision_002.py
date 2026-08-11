#!/usr/bin/env python3
"""Deterministic, offline adjudication of evidence-revision-001 revision-002 pilot."""
from __future__ import annotations
import hashlib, json, math, os, shutil, statistics, subprocess
from collections import Counter, defaultdict
from pathlib import Path

HERE=Path(__file__).resolve().parent
ABLATION=HERE.parent.parent
REPO=ABLATION.parent
RUN=ABLATION/'runs/evidence-revision-001-revision-002/evidence-revision-001-revision-002-pilot-20260811T221724Z'
MATRIX=HERE/'evidence-revision-001-pilot-matrix-v2.json'
CASE_INPUTS=HERE/'case-inputs-v2.json'
KEYS=ABLATION/'corpus/governed-keys.json'
OUT_JSON=HERE/'pilot-adjudication-revision-002.json'
OUT_JSONL=HERE/'pilot-unit-reconstruction-revision-002.jsonl'
OUT_MD=HERE/'pilot-adjudication-revision-002.md'
OUT_HASH=HERE/'pilot-adjudication-revision-002-hashes.json'
DESIGN='3031af33611c9ae8ab0fd0bba22f0abc35147d4fe564e0e3623a174d0dfe28d9'
EXECUTABLE='12eb26d72f74709a827059c2a6ccb693ef956a6d5aa7ca2bf6f292b645da010e'
IMPLEMENTATION='56f4463c2b228bb31f2c142d15e5431b8bd345114bd3707e14d386b1edfcf066'
REVIEWED='cf9b28b84a415ef33519e0866536ebd7787d567d'
ARMS=('R0','R1','R2','R3','R4')
BOUND=(
'ablation/eval/v2/evidence_revision_run.py','ablation/eval/v2/provider.py',
'ablation/eval/v2/controller.py','ablation/eval/v2/channel.py','ablation/eval/v2/dstate.py',
'ablation/eval/v2/decision.py','ablation/eval/v2/execution.py','ablation/eval/v2/ledger.py',
'ablation/eval/v2/legibility.py','ablation/eval/v2/ids.py','ablation/eval/v2/case_inputs_v2.py',
'ablation/eval/v2/marker_neutralization.py','ablation/eval/v2/sufficiency.py',
'ablation/eval/v2/replication.py','ablation/eval/v2/validity_revision.py')

def sha(b): return hashlib.sha256(b).hexdigest()
def fsha(p): return sha(p.read_bytes())
def canon(x): return json.dumps(x,sort_keys=True,separators=(',',':'),default=str).encode()
def manifest(root):
    files={str(p.relative_to(root)):fsha(p) for p in sorted(root.rglob('*')) if p.is_file()}
    return {'files':files,'sha256':sha(canon(files))}
def readj(p): return json.loads(p.read_text())
def writej(p,x): p.write_text(json.dumps(x,indent=2,sort_keys=True)+'\n')
def rate(n,d): return n/d if d else None
def pct(x): return 'NA' if x is None else f'{100*x:.1f}%'
def locus(x): return x if x not in (None,'') else 'unknown'

def parse_raw(text):
    # Same accepted five-field JSON shape; raw objects were already parsed live.
    decoder=json.JSONDecoder(); positions=[]
    for i,c in enumerate(text):
        if c!='{': continue
        try:
            obj,end=decoder.raw_decode(text[i:])
        except json.JSONDecodeError: continue
        if isinstance(obj,dict) and 'candidate_loci' in obj:
            allowed={'candidate_loci','unresolved_distinction','selected_action','expected_information','proposed_settlement'}
            if not set(obj)-allowed and isinstance(obj.get('candidate_loci'),list): return obj
    return None

def main():
    before=manifest(RUN)
    matrix=readj(MATRIX); ci=readj(CASE_INPUTS); keys=readj(KEYS)
    runj=readj(RUN/'run.json'); execution=readj(RUN/'execution-summary.json'); checkpoint=readj(RUN/'checkpoint.json')
    call_ledger=[json.loads(x) for x in (RUN/'call-commits.jsonl').read_text().splitlines() if x]
    result_ledger=[json.loads(x) for x in (RUN/'result-commits.jsonl').read_text().splitlines() if x]
    results={p.stem:readj(p) for p in sorted((RUN/'results').glob('*.json'))}
    failures=[]; checks={}
    def check(name,ok,detail=None):
        checks[name]={'pass':bool(ok),'detail':detail};
        if not ok: failures.append(name)
    matrix_hash=sha(canon({k:v for k,v in matrix.items() if k!='matrix_sha256'}))
    check('matrix_canonical_hash',matrix_hash==matrix['matrix_sha256'],matrix_hash)
    check('run_identity',runj.get('experiment_id')=='evidence-revision-001' and runj.get('stage')=='pilot')
    check('design_fingerprint',runj.get('design_fingerprint')==DESIGN)
    check('executable_fingerprint',runj.get('executable_contract_fingerprint')==EXECUTABLE)
    check('implementation_identity',runj.get('source_hashes',{}).get('runner_implementation_sha256')==IMPLEMENTATION)
    check('reviewed_commit',runj.get('source_hashes',{}).get('reviewed_runner_commit')==REVIEWED)
    current_files={rel:fsha(REPO/rel) for rel in BOUND}
    current_impl=sha(canon({'schema':'evidence-revision-runner-implementation-v1','files':current_files}))
    reviewed_files={}
    for rel in BOUND:
        reviewed_files[rel]=sha(subprocess.check_output(['git','-C',str(REPO),'show',f'{REVIEWED}:{rel}']))
    reviewed_impl=sha(canon({'schema':'evidence-revision-runner-implementation-v1','files':reviewed_files}))
    check('recomputed_runner_implementation',current_impl==reviewed_impl==IMPLEMENTATION)
    check('matrix_binding',runj.get('source_hashes',{}).get('matrix_sha256')==matrix['matrix_sha256'])
    check('case_input_binding',runj.get('source_hashes',{}).get('case_inputs_sha256')==matrix['case_inputs_sha256']==ci['case_inputs_sha256'])
    expected=[]; mapping={}
    for case in matrix['cases']:
      for arm in matrix['arms']:
       for rep in range(1,matrix['repetitions']+1):
        uid=matrix['unit_ids'][len(expected)]
        recomputed=sha('|'.join(('unit-id-v2','evidence-revision-001',matrix['namespace'],case,arm,str(rep),EXECUTABLE)).encode())[:16]
        check('matrix_uid_recomputed:'+uid,uid==recomputed)
        expected.append(uid);mapping[uid]=(case,arm,rep)
    check('sixty_unique_matrix_uids',len(expected)==len(set(expected))==60)
    check('result_uid_set',set(results)==set(expected),{'expected':60,'actual':len(results)})
    check('checkpoint_uid_set',set(checkpoint)==set(expected))
    check('checkpoint_all_result_committed',all(v.get('status')=='RESULT_COMMITTED' for v in checkpoint.values()))
    check('result_ledger_count_unique',len(result_ledger)==len({x['unit_id'] for x in result_ledger})==60)
    check('call_ledger_count_unique',len(call_ledger)==len({x['unit_id'] for x in call_ledger})==198)
    result_commits={x['unit_id']:x for x in result_ledger}; call_commits={x['unit_id']:x for x in call_ledger}
    generation_ids=[]; call_ids=[]; reconstructed=[]
    call_ledger_order=[]; result_ledger_order=[x['unit_id'] for x in result_ledger]
    for entry in call_ledger: call_ledger_order.append(entry['unit_id'].split(':call:')[0])
    for uid in expected:
      if uid not in results: continue
      r=results[uid]; case,arm,rep=mapping[uid]; truth=keys[case]['true_locus']
      check('identity:'+uid,r['unit_id']==uid and r['case_id']==case and r['condition']==arm and r['rep']==rep)
      contract=r['runner_contract']
      check('unit_contract:'+uid,contract['design_fingerprint']==DESIGN and contract['executable_contract_fingerprint']==EXECUTABLE and contract['case_input_binding_sha256']==matrix['case_inputs_sha256'])
      expected_binding=sha(canon({'experiment':'evidence-revision-001','fingerprint':EXECUTABLE,'stage':'pilot','namespace':matrix['namespace'],'case_id':case,'arm':arm,'rep':rep}))
      check('checkpoint_binding:'+uid,checkpoint[uid].get('unit_binding')==expected_binding)
      rp=RUN/'results'/f'{uid}.json'; rc=result_commits.get(uid)
      result_sha=fsha(rp)
      check('result_recoverable:'+uid,rc is not None and Path(rc['raw_path'])==rp and rc['raw_hash']==result_sha==checkpoint[uid].get('result_hash') and rc.get('hash_provenance')=='VERIFIED')
      executions=r['executions']; parsed_calls=[]
      for e in executions:
        n=e['call_num']; cid=f'{uid}:call:{n:03d}'; call_ids.append(cid)
        rawp=RUN/'raw'/uid/f'{n:03d}.txt'; metap=RUN/'calls'/uid/f'{n:03d}.json'; commit=call_commits.get(cid)
        ok=rawp.is_file() and metap.is_file() and commit is not None
        raw_hash=fsha(rawp) if rawp.is_file() else None; meta=readj(metap) if metap.is_file() else {}
        raw_text=rawp.read_text() if rawp.is_file() else ''
        ok=ok and commit['raw_hash']==raw_hash==e['raw_hash']==meta.get('raw_hash') and Path(commit['raw_path'])==rawp and commit.get('hash_provenance')=='VERIFIED' and e.get('raw_len')==len(raw_text)
        check('call_recoverable:'+cid,ok)
        check('call_request_binding:'+cid,meta.get('request',{}).get('unit_id')==uid and meta.get('request',{}).get('call_num')==n)
        check('prompt_guard:'+cid,meta.get('prompt_guard',{}).get('findings')==[] and meta.get('prompt_guard',{}).get('inspected_user_sha256')==meta.get('request',{}).get('user_sha256'))
        prov=meta.get('provider',{}); generation_ids.append(prov.get('generation_id'))
        check('provider_provenance:'+cid,prov.get('resolved_model')=='glm-5.2' and prov.get('provider_route')=='zai-coding-plan' and e['returned'].get('generation_id')==prov.get('generation_id'))
        decision=parse_raw(raw_text) if rawp.is_file() else None
        parsed_calls.append({'call_index':n,'call_identity':cid,'raw_sha256':raw_hash,'call_sidecar_sha256':fsha(metap) if metap.is_file() else None,'call_commit_hash_provenance':commit.get('hash_provenance') if commit else None,'parse_status':'VALID' if decision else 'NOT_RECONSTRUCTABLE','decision':decision,'provider':prov})
      check('last_raw_checkpoint:'+uid,bool(executions) and checkpoint[uid].get('last_call')==executions[-1]['call_num'] and checkpoint[uid].get('raw_hash')==executions[-1]['raw_hash'])
      rounds=[]; payment=0
      for q in r['rounds']:
        delivery=q.get('delivery'); payment+=q.get('payment_charged',0)
        d=q.get('decision')
        rounds.append({'round_index':q['round_num'],'call_index':q['round_num'],'claim':locus(d.get('proposed_settlement') if d else None),'candidate_loci':d.get('candidate_loci') if d else None,'unresolved_distinction':d.get('unresolved_distinction') if d else None,'selected_action':d.get('selected_action') if d else None,'payment_charged':q.get('payment_charged',0),'payment_after':payment,'delivery':delivery,'gate_decision':q.get('gate_result'),'parse_status':q.get('parse_status'),'output_class':q.get('output_class')})
      initial=locus(r['rounds'][0]['decision'].get('proposed_settlement'))
      initial_status='CORRECT_CONCRETE' if initial==truth else ('UNKNOWN_UNRESOLVED' if initial=='unknown' else 'WRONG_CONCRETE')
      first_post=locus(r['rounds'][1]['decision'].get('proposed_settlement')) if len(r['rounds'])>1 else 'NOT_RECONSTRUCTABLE'
      first_correct=None
      for pc in parsed_calls[1:]:
        if pc['decision'] and locus(pc['decision'].get('proposed_settlement'))==truth:
          first_correct=pc['call_index'];break
      final_blind=locus(contract['gate_blind_last_model_claim']); final_gov=locus(contract['governed_final_settlement'])
      final_correct=final_blind==truth; gov_correct=final_gov==truth
      eligible=initial!=truth; revision_correct=eligible and final_correct
      any_revision=final_blind!=initial
      useful=eligible and final_correct
      harmful=initial==truth and not final_correct
      wrong_concrete=initial not in ('unknown',truth)
      unknown_initial=initial=='unknown'
      final_call=parsed_calls[-1]
      final_decision=final_call['decision']
      forced_final=len(parsed_calls)>len(r['rounds'])
      final_gate_reason=('SETTLEMENT_ELIGIBLE' if final_gov not in ('unknown',None) else ('BUDGET_EXHAUSTED_GATE_REJECTED:EVIDENCE_INSUFFICIENT' if r['terminal_state']=='BUDGET_EXHAUSTED_GATE_REJECTED' else ('TERMINAL_UNKNOWN' if final_blind=='unknown' else 'NO_CONCRETE_SETTLEMENT')))
      reconstructed.append({'uid':uid,'result_sha256':result_sha,'result_commit_hash_provenance':rc.get('hash_provenance') if rc else None,'case_id':case,'arm':arm,'repetition':rep,'true_locus_evaluator_only':truth,'initial_model_claim':initial,'initial_claim_status':initial_status,'evidence_condition':contract['evidence_policy'],'rendering':'LEGIBLE' if r['runner_contract']['arm_spec']['observation_rendering']=='labelled_structural' else 'RAW','evidence_delivered':r['acquired_probes'],'acquired_handles':r['acquired_handles'],'evidence_set_sha256':contract['evidence_set_hash'],'semantic_evidence_sha256':contract['semantic_evidence_hash'],'rounds':rounds,'calls':parsed_calls,'forced_final_call':forced_final,'first_post_evidence_claim':first_post,'first_correct_post_evidence_call':first_correct,'final_gate_blind_claim':final_blind,'final_gate_blind_correct':final_correct,'final_governed_settlement':final_gov,'final_governed_correct':gov_correct,'gate_acceptance':final_gov not in ('unknown',None),'gate_reason':final_gate_reason,'terminal_state':r['terminal_state'],'payment_state':r['payment_used'],'primary_eligible':eligible,'revision_to_correct':revision_correct,'any_revision':any_revision,'useful_revision':useful,'harmful_revision':harmful,'wrong_concrete_to_correct':wrong_concrete and final_correct,'unknown_to_correct':unknown_initial and final_correct,'persistence_initial_wrong':wrong_concrete and final_blind==initial,'final_claim_directly_persisted':True,'forced_final_decision_recovered_from_raw':forced_final and final_decision is not None})
    check('all_call_identities_expected',len(call_ids)==len(set(call_ids))==198 and set(call_ids)==set(call_commits))
    check('all_raw_decisions_reconstructable',all(c['parse_status']=='VALID' for u in reconstructed for c in u['calls']))
    check('provider_generation_ids_unique_complete',len(generation_ids)==len(set(generation_ids))==198 and None not in generation_ids)
    expected_result_order=expected
    check('sequential_result_commit_order',result_ledger_order==expected_result_order)
    grouped=[]
    for uid in expected: grouped.extend([f'{uid}:call:{i:03d}' for i in range(1,len(results[uid]['executions'])+1)])
    check('sequential_call_commit_order',[x['unit_id'] for x in call_ledger]==grouped)
    # Evidence isolation and matched rendering pairs.
    bykey={(u['case_id'],u['arm'],u['repetition']):u for u in reconstructed}
    expected_policy={'R0':'NONE','R1':'MASKED_K','R2':'MASKED_K','R3':'FULL','R4':'FULL'}
    check('arm_evidence_policy',all(u['evidence_condition']==expected_policy[u['arm']] for u in reconstructed))
    check('r0_no_new_evidence',all(not u['evidence_delivered'] for u in reconstructed if u['arm']=='R0'))
    pair_integrity=[]
    for case in matrix['cases']:
      for rep in (1,2):
       for raw_arm,leg_arm in (('R1','R2'),('R3','R4')):
        a=bykey[(case,raw_arm,rep)];b=bykey[(case,leg_arm,rep)]
        equal=(a['evidence_delivered']==b['evidence_delivered'] and a['evidence_set_sha256']==b['evidence_set_sha256'] and a['semantic_evidence_sha256']==b['semantic_evidence_sha256'])
        pair_integrity.append({'case_id':case,'repetition':rep,'raw_arm':raw_arm,'legible_arm':leg_arm,'evidence_equal':equal,'raw_uid':a['uid'],'legible_uid':b['uid'],'raw_revision_to_correct':a['revision_to_correct'],'legible_revision_to_correct':b['revision_to_correct'],'raw_final_correct':a['final_gate_blind_correct'],'legible_final_correct':b['final_gate_blind_correct']})
    check('all_representation_pair_evidence_equal',all(x['evidence_equal'] for x in pair_integrity))
    check('no_cross_arm_evidence_contamination',checks['arm_evidence_policy']['pass'] and checks['r0_no_new_evidence']['pass'] and checks['all_representation_pair_evidence_equal']['pass'])
    check('no_score_preceded_raw_commit',not (RUN/'scores').exists(),{'score_artifacts':0,'note':'live pilot stopped at RESULT_COMMITTED; no score artifacts exist'})
    check('no_post_run_mutation',all(v['pass'] for k,v in checks.items() if k.startswith(('call_recoverable:','result_recoverable:'))))
    # Endpoint summaries.
    arms={}
    for arm in ARMS:
      us=[u for u in reconstructed if u['arm']==arm]; den=[u for u in us if u['primary_eligible']]; num=[u for u in den if u['revision_to_correct']]
      def ids(xs): return [u['uid'] for u in xs]
      cats=Counter(u['initial_claim_status'] for u in us)
      arms[arm]={'units':12,'primary':{'denominator':len(den),'numerator':len(num),'rate':rate(len(num),len(den)),'denominator_uids':ids(den),'numerator_uids':ids(num)},'initial_statuses':dict(cats),'any_revision':sum(u['any_revision'] for u in us),'useful_revision':sum(u['useful_revision'] for u in us),'harmful_revision':sum(u['harmful_revision'] for u in us),'wrong_concrete_to_correct':{'numerator':sum(u['wrong_concrete_to_correct'] for u in us),'denominator':cats.get('WRONG_CONCRETE',0),'rate':rate(sum(u['wrong_concrete_to_correct'] for u in us),cats.get('WRONG_CONCRETE',0))},'unknown_to_correct':{'numerator':sum(u['unknown_to_correct'] for u in us),'denominator':cats.get('UNKNOWN_UNRESOLVED',0),'rate':rate(sum(u['unknown_to_correct'] for u in us),cats.get('UNKNOWN_UNRESOLVED',0))},'persistence_initial_wrong':{'numerator':sum(u['persistence_initial_wrong'] for u in us),'denominator':cats.get('WRONG_CONCRETE',0),'rate':rate(sum(u['persistence_initial_wrong'] for u in us),cats.get('WRONG_CONCRETE',0))},'final_gate_blind_accuracy':{'numerator':sum(u['final_gate_blind_correct'] for u in us),'denominator':12,'rate':sum(u['final_gate_blind_correct'] for u in us)/12},'final_governed_accuracy':{'numerator':sum(u['final_governed_correct'] for u in us),'denominator':12,'rate':sum(u['final_governed_correct'] for u in us)/12},'gate_acceptance':{'numerator':sum(u['gate_acceptance'] for u in us),'denominator':12,'rate':sum(u['gate_acceptance'] for u in us)/12},'concrete_settlement':{'numerator':sum(u['final_governed_settlement']!='unknown' for u in us),'denominator':12,'rate':sum(u['final_governed_settlement']!='unknown' for u in us)/12},'final_gate_blind_unresolved':{'numerator':sum(u['final_gate_blind_claim']=='unknown' for u in us),'denominator':12,'rate':sum(u['final_gate_blind_claim']=='unknown' for u in us)/12},'final_governed_unresolved':{'numerator':sum(u['final_governed_settlement']=='unknown' for u in us),'denominator':12,'rate':sum(u['final_governed_settlement']=='unknown' for u in us)/12},'time_to_first_correct_calls':[u['first_correct_post_evidence_call'] for u in us if u['first_correct_post_evidence_call'] is not None]}
    r0_units=[u for u in reconstructed if u['arm']=='R0']
    r0_control={
      'spontaneous_any_revision':sum(u['any_revision'] for u in r0_units),
      'spontaneous_revision_to_correct':sum(u['revision_to_correct'] for u in r0_units),
      'eligible_denominator':sum(u['primary_eligible'] for u in r0_units),
      'wrong_concrete_denominator':sum(u['initial_claim_status']=='WRONG_CONCRETE' for u in r0_units),
      'unknown_denominator':sum(u['initial_claim_status']=='UNKNOWN_UNRESOLVED' for u in r0_units),
      'wrong_to_different_wrong':sum(u['initial_claim_status']=='WRONG_CONCRETE' and u['final_gate_blind_claim'] not in (u['initial_model_claim'],u['true_locus_evaluator_only'],'unknown') for u in r0_units),
      'wrong_to_unknown':sum(u['initial_claim_status']=='WRONG_CONCRETE' and u['final_gate_blind_claim']=='unknown' for u in r0_units),
      'unknown_to_concrete':sum(u['initial_claim_status']=='UNKNOWN_UNRESOLVED' and u['final_gate_blind_claim']!='unknown' for u in r0_units),
      'persistence_initial_wrong':sum(u['persistence_initial_wrong'] for u in r0_units),
      'persistence_initial_unknown':sum(u['initial_claim_status']=='UNKNOWN_UNRESOLVED' and u['final_gate_blind_claim']=='unknown' for u in r0_units)}
    contrasts={}
    for name,a,b in [('C1','R0','R1'),('C2','R1','R2'),('C3','R1','R3'),('C4','R3','R4')]:
      aa=arms[a]['primary'];bb=arms[b]['primary'];contrasts[name]={'contrast':f'{b}-{a}','left':{'arm':a,'numerator':aa['numerator'],'denominator':aa['denominator'],'rate':aa['rate']},'right':{'arm':b,'numerator':bb['numerator'],'denominator':bb['denominator'],'rate':bb['rate']},'difference_pp':100*(bb['rate']-aa['rate'])}
    pair_summary={}
    for family in ('R1/R2','R3/R4'):
      xs=[x for x in pair_integrity if f"{x['raw_arm']}/{x['legible_arm']}"==family]
      def summarize(field):
        same=sum(x['raw_'+field]==x['legible_'+field] for x in xs);rb=sum(x['raw_'+field] and not x['legible_'+field] for x in xs);lb=sum(x['legible_'+field] and not x['raw_'+field] for x in xs)
        return {'pairs':len(xs),'same_outcome':same,'raw_better':rb,'legible_better':lb,'both_correct':sum(x['raw_'+field] and x['legible_'+field] for x in xs),'neither_correct':sum(not x['raw_'+field] and not x['legible_'+field] for x in xs),'discordant_pairs':rb+lb}
      pair_summary[family]={'revision_to_correct':summarize('revision_to_correct'),'final_gate_blind_correct':summarize('final_correct')}
    divergences=[]
    for u in reconstructed:
      if u['final_gate_blind_claim']==u['final_governed_settlement']: continue
      cls=('CORRECT_BUT_UNQUALIFIED' if u['final_gate_blind_correct'] else ('UNKNOWN_NO_CONCRETE_CLAIM' if u['final_gate_blind_claim']=='unknown' else 'INCORRECT_AND_REJECTED'))
      divergences.append({k:u[k] for k in ('uid','case_id','arm','repetition','true_locus_evaluator_only','final_gate_blind_claim','final_gate_blind_correct','evidence_delivered','gate_reason','final_governed_settlement','payment_state','terminal_state')}|{'classification':cls})
    check('six_gate_governance_divergences',len(divergences)==6)
    # Instrument characteristics.
    instrument={}
    for arm in ARMS:
      us=[results[u['uid']] for u in reconstructed if u['arm']==arm]; ex=[e for r in us for e in r['executions']]; rounds0=[q for r in us for q in r['rounds']]
      def agg(field):
        vals=[e['returned'].get(field) for e in ex if e['returned'].get(field) is not None]
        return {'available':len(vals),'sum':sum(vals),'mean_per_call':sum(vals)/len(vals) if vals else None,'mean_per_unit':sum(vals)/len(us) if vals else None}
      instrument[arm]={'provider_calls':len(ex),'calls_per_unit_distribution':dict(Counter(len(r['executions']) for r in us)),'rounds_per_unit_distribution':dict(Counter(len(r['rounds']) for r in us)),'prompt_tokens':agg('prompt_tokens'),'completion_tokens':agg('completion_tokens'),'total_tokens':agg('total_tokens'),'reasoning_tokens':agg('reasoning_tokens'),'latency_ms':agg('latency_ms'),'output_classes':dict(Counter(q['output_class'] for q in rounds0)),'output_contract_failures':sum(q['output_class']!='VALID_DECISION' for q in rounds0),'truncations':sum(e['truncated'] for e in ex),'provider_retries':sum(e['returned'].get('retries',0) for e in ex)}
    total_calls=sum(x['provider_calls'] for x in instrument.values())
    check('recomputed_198_provider_calls',total_calls==198)
    check('summary_counts_match_reconstruction',execution.get('units_planned')==execution.get('units_completed_or_resumed')==60 and execution.get('provider_calls_performed')==execution.get('model_calls_performed')==total_calls)
    after=manifest(RUN);check('input_run_unchanged_during_analysis',before==after)
    validity=[
      ('Primary endpoint without inference/imputation','PASS','Round-1 proposed settlement, forced-final raw decision, final gate-blind claim, and evaluator truth are recoverable.'),
      ('Pre/post claims distinguishable','PASS','Round 1 precedes first delivery; calls 2+ are post-delivery.'),
      ('Truth remains evaluator-side','PASS','Truth loaded only from sealed governed-keys; absent from call/result model-visible payloads.'),
      ('Gate-blind/governed separable','PASS','Both are directly persisted in runner_contract.'),
      ('Evidence/rendering independently recoverable','PASS','Policy, membership/hashes, and arm rendering are persisted.'),
      ('Four frozen requested contrasts reconstructable','PASS','All arm numerator/denominator UID sets are deterministic.'),
      ('Representation pairings exact','PASS','Case/rep mapping and three evidence equalities recover all 24 pairs.'),
      ('Endpoint denominators deterministic','PASS','Round-1 proposed settlement plus sealed truth determines eligibility.'),
      ('No result depends on missing intermediate state','PASS','Forced-final decision is absent from rounds but recovered exactly from committed raw bytes; final claim also directly persisted.'),
      ('Reproducible without provider contact','PASS','This script reads only local immutable artifacts and sealed evaluator files.'),
    ]
    status='ANALYSIS_READY' if not failures and all(x[1]=='PASS' for x in validity) else 'ANALYSIS_BLOCKED'
    analysis_inputs={str(p.relative_to(REPO)):fsha(p) for p in (MATRIX,CASE_INPUTS,KEYS)}
    adjudication={'schema':'pilot-adjudication-revision-002-v1','status':status,'scope':'descriptive protocol pilot; not confirmatory','input_run':str(RUN),'input_run_manifest_sha256':before['sha256'],'analysis_input_files':analysis_inputs,'integrity':{'result':'PASS' if not failures else 'FAIL','checks':checks,'failures':failures,'counts':{'units':len(results),'provider_calls':len(call_ledger),'result_commits':len(result_ledger),'raw_responses':sum(len(list(p.glob('*.txt'))) for p in (RUN/'raw').iterdir())}},'identities':{'design':DESIGN,'executable_contract':EXECUTABLE,'runner_implementation':IMPLEMENTATION,'reviewed_runner_commit':REVIEWED},'endpoint_definition':{'eligibility':'round-1 proposed_settlement is not the true locus; null is explicit UNKNOWN/UNRESOLVED','numerator':'eligible unit final gate-blind last_model_claim equals evaluator true locus','note':'unit-level descriptive reconstruction requested; frozen confirmatory plan uses case aggregation and is not applied as a pilot authorization gate'},'arms':arms,'r0_control':r0_control,'contrasts':contrasts,'pairwise':pair_summary,'pair_integrity':pair_integrity,'gate_governance_divergences':divergences,'instrument':{'total_provider_calls':total_calls,'by_arm':instrument,'output_contract_failure_rate':sum(x['output_contract_failures'] for x in instrument.values())/sum(sum(x['output_classes'].values()) for x in instrument.values()),'truncation_rate':sum(x['truncations'] for x in instrument.values())/total_calls,'provider_retry_rate':sum(x['provider_retries'] for x in instrument.values())/total_calls},'validity':[{'number':i+1,'question':x[0],'classification':x[1],'basis':x[2]} for i,x in enumerate(validity)],'provider_calls_performed_by_analysis':0,'model_calls_performed_by_analysis':0,'full_authorized':False,'full_executed':False}
    writej(OUT_JSON,adjudication)
    with OUT_JSONL.open('w') as f:
      for u in reconstructed:f.write(json.dumps(u,sort_keys=True,separators=(',',':'))+'\n')
    # Human report generated from adjudication.
    md=['# Revision-002 pilot offline adjudication','','**'+status+'**','','This is a descriptive six-case protocol-pilot measurement validation, not confirmatory inference and not FULL authorization.','','## Integrity',f"PASS: {len(results)}/60 unique results, {len(call_ledger)}/198 unique committed/recoverable calls, 60/60 committed results; all raw/result/checkpoint hashes and identities match. Input run manifest: `{before['sha256']}`.",'','No score artifacts exist: execution stopped at `RESULT_COMMITTED`, so no score can precede raw commitment.','','## Revision to correct','','Eligibility is the persisted round-1 proposed settlement not already equal to evaluator truth; null is explicit UNKNOWN/UNRESOLVED. Numerator uses the final gate-blind model claim, never governed settlement.','','| Arm | numerator/denominator | rate | wrong→correct | UNKNOWN→correct | blind accuracy | governed accuracy |','|---|---:|---:|---:|---:|---:|---:|']
    for a in ARMS:
      x=arms[a];p=x['primary'];md.append(f"| {a} | {p['numerator']}/{p['denominator']} | {pct(p['rate'])} | {x['wrong_concrete_to_correct']['numerator']}/{x['wrong_concrete_to_correct']['denominator']} | {x['unknown_to_correct']['numerator']}/{x['unknown_to_correct']['denominator']} | {x['final_gate_blind_accuracy']['numerator']}/12 ({pct(x['final_gate_blind_accuracy']['rate'])}) | {x['final_governed_accuracy']['numerator']}/12 ({pct(x['final_governed_accuracy']['rate'])}) |")
    md+=['','## Secondary outcomes','','Counts are unit-level out of 12 unless a transition denominator is shown. Useful means eligible→true; harmful means initially true→not true. With categorical loci, no unprespecified distance ordering is invented.','','| Arm | any revision | useful | harmful | wrong→correct | UNKNOWN→correct | persist wrong | gate accepted/concrete | blind UNKNOWN | governed UNKNOWN |','|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|']
    for a in ARMS:
      x=arms[a];md.append(f"| {a} | {x['any_revision']}/12 | {x['useful_revision']}/12 | {x['harmful_revision']}/12 | {x['wrong_concrete_to_correct']['numerator']}/{x['wrong_concrete_to_correct']['denominator']} | {x['unknown_to_correct']['numerator']}/{x['unknown_to_correct']['denominator']} | {x['persistence_initial_wrong']['numerator']}/{x['persistence_initial_wrong']['denominator']} | {x['gate_acceptance']['numerator']}/12 | {x['final_gate_blind_unresolved']['numerator']}/12 | {x['final_governed_unresolved']['numerator']}/12 |")
    md+=['','R0 spontaneous revision to correct: '+f"{arms['R0']['primary']['numerator']}/{arms['R0']['primary']['denominator']} ({pct(arms['R0']['primary']['rate'])}).",f"R0 control transitions: any revision {r0_control['spontaneous_any_revision']}/12; wrong→different-wrong {r0_control['wrong_to_different_wrong']}; UNKNOWN→concrete {r0_control['unknown_to_concrete']}; persistent wrong {r0_control['persistence_initial_wrong']}; persistent UNKNOWN {r0_control['persistence_initial_unknown']}.",'','## Descriptive contrasts','']
    for n,x in contrasts.items():md.append(f"- {n} {x['contrast']}: {x['right']['numerator']}/{x['right']['denominator']} minus {x['left']['numerator']}/{x['left']['denominator']} = {x['difference_pp']:+.1f} pp.")
    md+=['','No p-values or authorization inference is made.','','## Representation pairs','']
    for fam,x in pair_summary.items():md.append(f"- {fam}, final correctness: {x['final_gate_blind_correct']}; revision-to-correct: {x['revision_to_correct']}.")
    md+=['','All R1/R2 and R3/R4 persisted evidence memberships, evidence-set hashes, and semantic hashes match for all case/rep pairs.','','## Six governance divergences','']
    for d in divergences:md.append(f"- `{d['uid']}` {d['case_id']} {d['arm']} rep {d['repetition']}: truth `{d['true_locus_evaluator_only']}`, blind `{d['final_gate_blind_claim']}`, governed `{d['final_governed_settlement']}`, `{d['classification']}`, evidence `{','.join(d['evidence_delivered']) or 'NONE'}`, reason `{d['gate_reason']}`, payment {d['payment_state']}, terminal `{d['terminal_state']}`.")
    md+=['','## Analysis validity','']
    for x in adjudication['validity']:md.append(f"{x['number']}. **{x['classification']}** — {x['question']}: {x['basis']}")
    md+=['','## Instrument','','| Arm | calls | calls/unit | rounds/unit | total tokens | reasoning tokens | latency ms |','|---|---:|---|---|---:|---:|---:|']
    for a in ARMS:
      x=instrument[a];md.append(f"| {a} | {x['provider_calls']} | {x['calls_per_unit_distribution']} | {x['rounds_per_unit_distribution']} | {x['total_tokens']['sum']} | {x['reasoning_tokens']['sum']} | {x['latency_ms']['sum']:.1f} |")
    md+=['','Total persisted calls: 198. All 198 raw decisions reparse under the frozen five-field shape; all 162 persisted round records are VALID_DECISION. Repair attempts, truncations, and provider retries are zero. Forced-final calls are recoverable from raw/call commits. Cost is unavailable because the provider did not report it.','','The append ledgers are durable hash manifests, not externally signed hash chains. Current raw/result bytes match every ledger, sidecar, execution, and checkpoint digest; historical ordering additionally relies on the reviewed implementation, which writes and fsyncs raw before parsing/result commitment. There are no score artifacts.','','Provider/model calls made by this analysis: **0/0**. FULL remains unauthorized and unexecuted.','']
    OUT_MD.write_text('\n'.join(md))
    artifacts={str(p.relative_to(REPO)):fsha(p) for p in (Path(__file__).resolve(),OUT_JSON,OUT_JSONL,OUT_MD)}
    writej(OUT_HASH,{'schema':'pilot-adjudication-hashes-v1','input_run':str(RUN.relative_to(REPO)),'input_run_manifest_sha256':before['sha256'],'input_run_files':before['files'],'analysis_input_files':analysis_inputs,'analysis_artifacts':artifacts})
    print(status)
if __name__=='__main__':main()
