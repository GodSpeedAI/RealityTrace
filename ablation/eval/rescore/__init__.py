"""Post-audit rescoring of IMMUTABLE historical evidence.

This package is read-only with respect to every historical artifact. It never
writes into `ablation/runs/**` or `ablation/corpus/**`. All derived output goes
to `ablation/analysis/post-audit-v2/` under a new scorer version, carrying the
SHA-256 of every source artifact it consumed.

No model calls. Nothing here can produce a number that did not already exist in
the recorded outputs.
"""

SCORER_VERSION = "post-audit-v2"
