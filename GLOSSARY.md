# RealityTrace observation and evidence

RealityTrace records expectations and observations so a consumer can evaluate differences under declared conditions.

## Language

**Expectation**:
A declaration, recorded in advance, of what change or observation would answer a question.

**Observation**:
An attributable record of what was observed in relation to an execution or state.
_Avoid_: Settlement, evidence (before binding to a question)

**Difference**:
A comparison of recorded observation against a specific expectation, including evidence of discrepancy or absence.

**Verification Result**:
A finding from checking a declared expectation against observations under the relevant procedure and criteria.

**Observation Ledger**:
The durable ordered record from which observation provenance and the evidentiary basis can be checked.

## Sources and boundary

The [RealityTrace model](sxr-core/src/types.rs) and [record kinds](sxr-core/src/record_kinds.csv) govern these terms. An Observation contributes to evidence once it is bound to an expectation or claim. The Verification Result reports the resulting check. An authority may use it to issue a separate Settlement Receipt. The observation ledger and any receipt observation preserve source and provenance.
