"""Frozen analysis-contract conformance for revision-002 pilot adjudication."""
from __future__ import annotations
import hashlib
import importlib.util
import json
from pathlib import Path

ROOT=Path(__file__).resolve().parents[2]
SCRIPT=ROOT/'analysis/post-audit-v2/pilot_adjudication_revision_002.py'
SPEC=importlib.util.spec_from_file_location('pilot_adjudication_revision_002',SCRIPT)
analysis=importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(analysis)


def _units(rep1=True,rep2=False,governed=False):
    return [
      {'case_id':'case-a','arm':'R0','repetition':1,
       'final_gate_blind_correct':rep1,'final_governed_correct':governed,
       'initial_claim_status':'CORRECT_CONCRETE'},
      {'case_id':'case-a','arm':'R0','repetition':2,
       'final_gate_blind_correct':rep2,'final_governed_correct':governed,
       'initial_claim_status':'WRONG_CONCRETE'}]


def test_primary_r_correct_is_not_conditioned_on_initial_correctness():
    out=analysis.aggregate_case_primary(_units(True,True),('case-a',),('R0',))
    assert out['R0']['denominator']==1
    assert out['R0']['numerator']==1


def test_primary_inferential_unit_is_case_not_case_rep():
    assert analysis.PRIMARY_ENDPOINT['unit']=='case'
    assert analysis.STATISTICAL_PLAN['primary_unit']=='case'
    out=analysis.aggregate_case_primary(_units(True,True),('case-a',),('R0',))
    assert out['R0']['denominator']==1


def test_repetitions_are_aggregated_within_case():
    out=analysis.aggregate_case_primary(_units(True,True),('case-a',),('R0',))
    row=out['R0']['cases'][0]
    assert row['correct_reps']==2
    assert row['aggregated_case_correct'] is True


def test_one_one_repetition_tie_is_incorrect():
    out=analysis.aggregate_case_primary(_units(True,False),('case-a',),('R0',))
    assert out['R0']['cases'][0]['aggregated_case_correct'] is False
    assert out['R0']['numerator']==0


def test_primary_uses_gate_blind_not_governed_settlement():
    out=analysis.aggregate_case_primary(_units(True,True,governed=False),
                                        ('case-a',),('R0',))
    assert out['R0']['numerator']==1


def test_c3_is_frozen_r3_r4_legibility_upper():
    specs=analysis.frozen_comparison_specs()
    assert specs['C3_legibility_upper']['arms']==['R3','R4']
    assert specs['C3_legibility_upper']['family']=='primary'


def test_c4_is_frozen_r1_r3_coverage_headroom():
    specs=analysis.frozen_comparison_specs()
    assert specs['C4_coverage_headroom']['arms']==['R1','R3']
    assert specs['C4_coverage_headroom']['family']=='secondary'


def test_c4_never_claims_single_variable_isolation():
    c4=analysis.frozen_comparison_specs()['C4_coverage_headroom']
    assert c4['isolates_single_variable'] is False
    assert c4['declared_differences'].startswith('TWO')
    assert 'EXEMPT_UPPER_BOUND' in c4['constraint']


def test_conditional_revision_is_diagnostic_not_primary():
    artifact=json.loads((ROOT/'analysis/post-audit-v2/pilot-adjudication-revision-002.json').read_text())
    assert analysis.PRIMARY_ENDPOINT['name']=='R_correct'
    assert artifact['endpoint_definition']['frozen_primary_name']=='R_correct'
    assert 'revision_to_correct_conditional' in artifact['endpoint_definition']['conditional_diagnostic']
    assert 'primary' not in artifact['endpoint_definition']['conditional_diagnostic'].lower()


def test_analysis_does_not_change_frozen_scientific_contract():
    contract=ROOT/'eval/v2/evidence_revision.py'
    assert hashlib.sha256(contract.read_bytes()).hexdigest()==(
      'fb5f8516ffae99af1318878b0d2c82de7b9dc595a9642c08e3afa12b1dbc0810')
    assert analysis.contract_fingerprint()==(
      '7edd93a3a503b9395c0d39e79540cb20fe084fc8c020413c306db88e1029d3e3')


def test_corrected_artifact_has_golden_case_primary_and_pair_tables():
    artifact=json.loads((ROOT/'analysis/post-audit-v2/pilot-adjudication-revision-002.json').read_text())
    assert {a:x['numerator'] for a,x in artifact['canonical_case_level_primary'].items()} == {
        'R0':0,'R1':2,'R2':1,'R3':3,'R4':3}
    assert all(x['denominator']==6 for x in artifact['canonical_case_level_primary'].values())
    paired=artifact['canonical_paired_comparisons']
    assert [paired['C1_evidence_effect'][k] for k in (
        'both_correct','both_incorrect','first_only_correct','second_only_correct','discordant_count')] == [0,4,0,2,2]
    assert [paired['C2_legibility_governed'][k] for k in (
        'both_correct','both_incorrect','first_only_correct','second_only_correct','discordant_count')] == [1,4,1,0,1]
    assert [paired['C3_legibility_upper'][k] for k in (
        'both_correct','both_incorrect','first_only_correct','second_only_correct','discordant_count')] == [2,2,1,1,2]
    assert [paired['C4_coverage_headroom'][k] for k in (
        'both_correct','both_incorrect','first_only_correct','second_only_correct','discordant_count')] == [2,3,0,1,1]
