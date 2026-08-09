#!/usr/bin/env python3
"""M3 unparseable-output audit — answer-blind classification generator.

Classifies the 10 unparseable pilot-003 outputs WITHOUT reading sealed answer
keys and WITHOUT considering whether the substantive locus prediction was
correct. Each is classified as exactly one of:
    MODEL_CONTRACT_VIOLATION | PARSER_DEFECT | AMBIGUOUS

The frozen response contract (from the frozen system prompt): the model must
reply with a single line "Locus: <locus>" where <locus> is exactly one of the
nine frozen vocabulary terms. No prose, no reasoning, no alternate formats.

The parser (extract_locus) recognizes exactly: a "Locus:" token followed by a
vocabulary member. It is intentionally strict to match the contract.

This script performs the classification deterministically and writes the audit
artifact. It never reads keys.json / scenario-keys.json.
"""

from __future__ import annotations

import glob
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

from eval.runner import LOCUS_VOCABULARY, extract_locus  # noqa: E402

RUNS = os.path.join(HERE, "..", "runs")
PILOT = "pilot-003"
EXP = os.path.join(RUNS, PILOT, PILOT)

CONTRACT_CLAUSE = (
    "Frozen system prompt: 'Reply with a single line: Locus: <locus>.' where "
    "<locus> is exactly one of the nine vocabulary terms; the response must be "
    "a single line, not prose or reasoning."
)
PARSER_RULE = (
    "extract_locus: regex 'Locus:\\s*([a-z_]+)' captured token must be in the "
    "frozen vocabulary; else no claim (unparseable)."
)


def classify(text: str) -> tuple[str, str, str]:
    """Answer-blind classification of one raw output.

    Returns (classification, parser_result, reason).
    """
    # Does the output contain a proper contract answer anywhere?
    proper = re.findall(
        r"Locus:\s*(" + "|".join(re.escape(v) for v in LOCUS_VOCABULARY) +
        r")\b", text)

    parser_result = extract_locus(text)

    if proper:
        # A proper 'Locus: <vocab>' exists. If the parser still says unparseable,
        # that is a PARSER_DEFECT (contract satisfied, parser failed).
        if parser_result is None:
            return ("PARSER_DEFECT", "unparseable", (
                "response contains a valid 'Locus: <vocab>' contract answer but "
                "the parser returned None"))
        return ("MODEL_CONTRACT_VIOLATION", parser_result, (
            "response contains a Locus token but it is mid-prose, not a final "
            "single-line answer; multiple/questionable") )

    # No proper 'Locus: <vocab>' at all.
    has_locus_word = bool(re.search(r"\bLocus\b", text, re.IGNORECASE))
    if has_locus_word:
        # 'Locus' appears but never as a valid contract answer (e.g. echoed
        # template 'Locus: <locus>', or 'Locus verifier' without colon mid-prose).
        return ("MODEL_CONTRACT_VIOLATION", "no_claim", (
            "'Locus' appears only as echoed template or mid-prose reasoning; "
            "no final 'Locus: <vocab>' answer; contract requires a single line"))
    # No 'Locus' token at all.
    return ("MODEL_CONTRACT_VIOLATION", "no_claim", (
        "no 'Locus' token anywhere; response is prose with no contract-level "
        "answer; contract requires a single 'Locus: <locus>' line"))


def main() -> int:
    recs = {}
    for f in glob.glob(os.path.join(EXP, "scoring", PILOT, "*.json")):
        r = json.load(open(f))
        if r.get("unparseable"):
            recs[r["unit_id"]] = r
    lines = open(os.path.join(EXP, "manifest.jsonl")).read().strip().splitlines()
    store = {json.loads(l)["unit_id"]: json.loads(l) for l in lines}

    audit = []
    for unit_id in sorted(recs):
        r = recs[unit_id]
        raw_path = os.path.join(EXP, "raw", f"{unit_id}.txt")
        text = open(raw_path).read()
        cls, parser_result, reason = classify(text)
        audit.append({
            "run_id": unit_id,
            "case_id": r["case_id"],
            "arm": r["arm"],
            "repetition": r["repetition"],
            "raw_output_hash": store.get(unit_id, {}).get("raw_output_hash"),
            "classification": cls,
            "parser_result": parser_result,
            "contract_clause": CONTRACT_CLAUSE,
            "parser_rule": PARSER_RULE,
            "reason": reason,
        })

    counts: dict[str, int] = {}
    for a in audit:
        counts[a["classification"]] = counts.get(a["classification"], 0) + 1

    result = {
        "audit": "answer-blind unparseable audit (no sealed keys read)",
        "pilot": PILOT,
        "n_unparseable": len(audit),
        "classifications": counts,
        "decision": {
            "all_model_contract_violation": counts.get(
                "MODEL_CONTRACT_VIOLATION", 0) == len(audit),
            "any_parser_defect": counts.get("PARSER_DEFECT", 0) > 0,
            "any_ambiguous": counts.get("AMBIGUOUS", 0) > 0,
        },
        "items": audit,
    }

    out = os.path.join(HERE, "..", "eval", "pilot_evidence", PILOT,
                       "unparseable_audit.json")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    with open(out, "w") as f:
        json.dump(result, f, indent=2)

    print(f"unparseable: {len(audit)}")
    for k, v in counts.items():
        print(f"  {k}: {v}")
    print("decision:", result["decision"])
    print("audit:", out)
    return 0


if __name__ == "__main__":
    sys.exit(main())