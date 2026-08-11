"""Repaired experimental instrument (post-audit v2).

This package is a NEW namespace. It does not modify, import-patch, or shadow
any frozen historical contract (`eval.contract`, `eval.acquisition`,
`eval.attribution`, `eval.governed`) or any historical run artifact.

Every module here exists to close a specific validity defect recorded in
`.agents/audits/cognitiveos-diagnostic-experiments/opus-audit/FINAL-ASSURANCE-REPORT.md`
and tracked in `.agents/plans/10-experimental-instrument-repair.md`:

    decision.py     R-05  typed decision contract, no prose scraping
    dstate.py       R-02  diagnostic state continuity across rounds
    channel.py      R-01  answer-isolated observation channel
    execution.py    R-04/R-07  provider provenance + truncation observability
    ledger.py       R-08  raw-hash-before-score binding
    semantics.py    R-09  three distinct discrimination constructs
    sufficiency.py  R-10  composable sufficiency predicates
    ids.py          R-15  globally unique unit identity
    controller.py   R-16  replication-ready controller
    integrity.py    R-06  integrity checks that can actually fail
"""

VERSION = "instrument-v2"
