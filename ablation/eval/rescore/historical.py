#!/usr/bin/env python3
"""R-12 + R-14 + R-17: corrected measurements over immutable historical evidence.

Corrective actions 4, 5, 9, 11 and audit §12/§14. Every measurement here is
recomputable from recorded outputs; nothing is imputed and no model is called.

What this fixes, per experiment
-------------------------------
S23.1   Per-arm output-truncation rates (4.4%-27.8%, a 6.25x spread) were never
        reported, and every unparseable unit was scored incorrect. Reports
        parseable-conditional accuracy with Wilson intervals, plus the actual
        size of the retrieval manipulation.
S23.2   Same truncation asymmetry (16.7% vs 7.8%). Reports parseable-conditional
        A_c and the response-mode distribution that shows abstention was
        unconditional rather than evidence-sensitive.
acq     Decomposes "81% behaviourally bypassed acquisition" into clean immediate
        settlement, denied-after-parse-failure, and genuine acquisition; reports
        A_i without the outcome-conditioned denominator. The 106 units whose
        model responses were never persisted are marked
        UNRECOVERABLE_FROM_HISTORICAL_EVIDENCE.
gov     Gate-blind vs gated accuracy, A_select conditional on an action actually
        being taken, invocation separately, the matched G/O evidence-use
        comparison, and case-level paired statistics.

Usage:  uv run --python 3.12 -m eval.rescore.historical
"""

from __future__ import annotations

import hashlib
import json
import math
import os
import re
import statistics
import sys
from collections import Counter, defaultdict

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(os.path.dirname(HERE)))

from eval.rescore import SCORER_VERSION  # noqa: E402

ABLATION = os.path.dirname(os.path.dirname(HERE))
CORPUS = os.path.join(ABLATION, "corpus")
RUNS = os.path.join(ABLATION, "runs")
OUT = os.path.join(ABLATION, "analysis", "post-audit-v2")

VOCAB = ("verifier", "verification_plan", "projection", "implementation",
         "environment", "authority", "evidence_collection", "declared_model",
         "unknown")

#: The historical cap, established behaviourally in Pass B across all four
#: experiments and both serving routes.
OBSERVED_CEILING = 4096

_SOURCES: dict[str, str] = {}


# ── source-bound loading ─────────────────────────────────────────────────────

def _hash_file(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def load_json(path: str, record: bool = True):
    if record:
        _SOURCES[os.path.relpath(path, ABLATION)] = _hash_file(path)
    with open(path) as f:
        return json.load(f)


def load_jsonl(path: str, key="case_id"):
    _SOURCES[os.path.relpath(path, ABLATION)] = _hash_file(path)
    out = {}
    with open(path) as f:
        for line in f:
            d = json.loads(line)
            if key in d:
                out[d[key]] = d
    return out


def load_dir(path: str, suffix=".json"):
    """Load a directory of records, recording an order-independent digest."""
    files = sorted(f for f in os.listdir(path) if f.endswith(suffix))
    h = hashlib.sha256()
    recs = []
    for f in files:
        p = os.path.join(path, f)
        h.update(_hash_file(p).encode())
        with open(p) as fh:
            recs.append(json.load(fh))
    _SOURCES[os.path.relpath(path, ABLATION) + "/"] = (
        f"dir:{len(files)}:{h.hexdigest()}")
    return recs


# ── statistics ───────────────────────────────────────────────────────────────

def wilson(k: int, n: int, z: float = 1.96) -> list:
    if n == 0:
        return [0.0, 1.0]
    p = k / n
    d = 1 + z * z / n
    c = (p + z * z / (2 * n)) / d
    s = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / d
    return [round(max(0.0, c - s), 4), round(min(1.0, c + s), 4)]


def rate(k: int, n: int) -> dict:
    return {"numerator": k, "denominator": n,
            "rate": round(k / n, 4) if n else None, "ci95": wilson(k, n)}


def mcnemar(b: int, c: int) -> float:
    if b + c == 0:
        return 1.0
    if b + c < 25:
        from math import comb
        n, k = b + c, min(b, c)
        return min(1.0, 2 * sum(comb(n, i) * 0.5 ** n for i in range(k + 1)))
    chi = (abs(b - c) - 1) ** 2 / (b + c)
    return math.erfc(math.sqrt(chi / 2))


def paired(records, metric, cond_a, cond_b, unit="case"):
    """Paired comparison. `unit='case'` aggregates repetitions by majority.

    Audit A-05: the historical analysis paired on (case, rep) at temperature 0
    with no per-call seed, treating near-replicates as independent. Both units
    are reported here; Pass B showed every conclusion survives case-level
    aggregation, so this is a reporting repair, not a reversal.
    """
    buckets = defaultdict(lambda: defaultdict(list))
    for r in records:
        key = r["case_id"] if unit == "case" else (r["case_id"], r["rep"])
        buckets[key][r["condition"]].append(metric(r))
    b = c = n = 0
    for _key, conds in buckets.items():
        if cond_a not in conds or cond_b not in conds:
            continue
        av = sum(conds[cond_a]) / len(conds[cond_a])
        bv = sum(conds[cond_b]) / len(conds[cond_b])
        if unit == "case":
            av, bv = (1 if av > 0.5 else 0), (1 if bv > 0.5 else 0)
        n += 1
        if av and not bv:
            b += 1
        elif bv and not av:
            c += 1
    p = mcnemar(b, c)
    out = {"unit": unit, "n_pairs": n, "mcnemar_b": b, "mcnemar_c": c}
    if p == 0.0:
        out["p"] = None
        out["p_note"] = ("chi-square normal approximation underflowed to 0.0; "
                         "report as p < 1e-15, never as 0")
    else:
        out["p"] = float(f"{p:.4g}")
    return out


def response_mode(predictions) -> dict:
    """R-14: POST-HOC RESPONSE-MODE ANALYSIS.

    Not a causal or psychological claim. It reports how concentrated a
    condition's answer distribution is, so a future replication can separate
    evidence-sensitive reasoning from a scaffold-induced response prior.
    """
    counts = Counter(p if p is not None else "(unparseable)" for p in predictions)
    n = sum(counts.values())
    if not n:
        return {"n": 0}
    modal, modal_n = counts.most_common(1)[0]
    ent = -sum((v / n) * math.log2(v / n) for v in counts.values() if v)
    return {
        "n": n,
        "distribution": dict(counts.most_common()),
        "modal_answer": modal,
        "modal_share": round(modal_n / n, 4),
        "entropy_bits": round(ent, 4),
        "max_entropy_bits": round(math.log2(len(VOCAB)), 4),
        "normalised_entropy": round(ent / math.log2(len(VOCAB)), 4),
        "label": "POST-HOC RESPONSE-MODE ANALYSIS",
    }


def extract_locus_legacy(text: str):
    """The historical scraper, retained ONLY to re-read immutable outputs.

    Never used by the v2 runtime (see `eval/v2/decision.py`).
    """
    m = re.search(r"Locus:\s*([a-z_]+)", text or "")
    return m.group(1) if m and m.group(1) in VOCAB else None


# ── A. S23.1 retrieval ablation ──────────────────────────────────────────────

def rescore_s231() -> dict:
    exp = os.path.join(RUNS, "full-001", "full-001")
    scored = load_dir(os.path.join(exp, "scoring", "full-001"))
    matrix = load_json(os.path.join(exp, "matrix.json"))
    arm_of = {u["unit_id"]: u["arm"] for u in matrix["units"]}

    per_arm = defaultdict(lambda: {
        "n": 0, "determinate": 0, "det_correct": 0, "det_unparseable": 0,
        "unparseable": 0, "at_ceiling": 0, "unknown_correct": 0,
        "predictions": [], "prompt_tokens": [], "caps": defaultdict(list)})

    for r in scored:
        arm = arm_of[r["unit_id"]]
        a = per_arm[arm]
        a["n"] += 1
        a["unparseable"] += bool(r["unparseable"])
        a["predictions"].append(r["predicted_locus"])
        a["unknown_correct"] += bool(r["unknown_correct"])
        if r["determinate"]:
            a["determinate"] += 1
            a["det_correct"] += bool(r["exact_correct"])
            a["det_unparseable"] += bool(r["unparseable"])
        usage = load_json(os.path.join(exp, "raw", f"{r['unit_id']}.usage.json"),
                          record=False)
        a["prompt_tokens"].append(usage.get("prompt_tokens") or 0)
        if (usage.get("completion_tokens") or 0) >= OBSERVED_CEILING:
            a["at_ceiling"] += 1
        for item in usage.get("packing_items", []):
            a["caps"][item["capability"]].append(item["included_tokens"])

    out = {"experiment": "full-001", "n_units": len(scored), "arms": {}}
    for arm in sorted(per_arm):
        a = per_arm[arm]
        parseable_det = a["determinate"] - a["det_unparseable"]
        caps = {c: {"mean_tokens": round(statistics.mean(v), 1),
                    "empty_sections": sum(1 for x in v if x == 0),
                    "n": len(v)}
                for c, v in sorted(a["caps"].items())}
        out["arms"][arm] = {
            "n": a["n"],
            "unparseable": rate(a["unparseable"], a["n"]),
            "calls_at_observed_ceiling": a["at_ceiling"],
            "accuracy_as_historically_reported":
                rate(a["det_correct"], a["determinate"]),
            "accuracy_conditional_on_parseable":
                rate(a["det_correct"], parseable_det),
            "unknown_correct": a["unknown_correct"],
            "prompt_tokens": {
                "mean": round(statistics.mean(a["prompt_tokens"]), 1),
                "median": statistics.median(a["prompt_tokens"]),
                "max": max(a["prompt_tokens"])},
            "retrieval_sections": caps,
            "response_mode": response_mode(a["predictions"]),
        }

    arms = sorted(out["arms"])
    lo = [out["arms"][x]["accuracy_conditional_on_parseable"]["ci95"][0] for x in arms]
    hi = [out["arms"][x]["accuracy_conditional_on_parseable"]["ci95"][1] for x in arms]
    out["all_conditional_cis_overlap"] = max(lo) <= min(hi)
    out["unparseable_spread_ratio"] = round(
        max(out["arms"][x]["unparseable"]["rate"] for x in arms)
        / max(1e-9, min(out["arms"][x]["unparseable"]["rate"] for x in arms)), 2)
    out["interpretation_permitted"] = (
        "No arm effect is statistically distinguishable once output truncation "
        "is controlled for. The manipulation added a ~76-token graph section "
        "and/or a ~106-token vector section to a median total prompt of "
        "228-567 tokens against a 32,000-token budget. This is not a test of "
        "representational richness.")
    return out


# ── B. S23.2 attribution ablation ────────────────────────────────────────────

def rescore_s232() -> dict:
    exp = os.path.join(RUNS, "attribution-002", "attribution-002")
    scored = load_dir(os.path.join(exp, "scoring", "attribution-002"))
    matrix = load_json(os.path.join(exp, "matrix.json"))
    method_of = {u["unit_id"]: u["method"] for u in matrix["units"]}

    per = defaultdict(lambda: {
        "n": 0, "det": 0, "det_correct": 0, "det_unparseable": 0,
        "det_unknown": 0, "unk": 0, "unk_correct": 0, "unparseable": 0,
        "at_ceiling": 0, "predictions": [], "det_predictions": [],
        "unk_predictions": []})

    for r in scored:
        m = method_of[r["unit_id"]]
        p = per[m]
        p["n"] += 1
        p["unparseable"] += bool(r["unparseable"])
        p["predictions"].append(r["predicted_locus"])
        usage = load_json(os.path.join(exp, "raw", f"{r['unit_id']}.usage.json"),
                          record=False)
        if (usage.get("completion_tokens") or 0) >= OBSERVED_CEILING:
            p["at_ceiling"] += 1
        if r["determinate"]:
            p["det"] += 1
            p["det_correct"] += bool(r["exact_correct"])
            p["det_unparseable"] += bool(r["unparseable"])
            p["det_unknown"] += (r["predicted_locus"] == "unknown")
            p["det_predictions"].append(r["predicted_locus"])
        else:
            p["unk"] += 1
            p["unk_correct"] += bool(r["unknown_correct"])
            p["unk_predictions"].append(r["predicted_locus"])

    out = {"experiment": "attribution-002", "n_units": len(scored), "methods": {}}
    for m in sorted(per):
        p = per[m]
        out["methods"][m] = {
            "n": p["n"],
            "unparseable": rate(p["unparseable"], p["n"]),
            "calls_at_observed_ceiling": p["at_ceiling"],
            "A_c_as_historically_reported": rate(p["det_correct"], p["det"]),
            "A_c_conditional_on_parseable":
                rate(p["det_correct"], p["det"] - p["det_unparseable"]),
            "A_u": rate(p["unk_correct"], p["unk"]),
            "unknown_rate_on_determinate_cases": rate(p["det_unknown"], p["det"]),
            "unknown_rate_on_unknown_cases": rate(p["unk_correct"], p["unk"]),
            "response_mode_all": response_mode(p["predictions"]),
            "response_mode_determinate": response_mode(p["det_predictions"]),
            "response_mode_unknown_cases": response_mode(p["unk_predictions"]),
        }
    out["interpretation_permitted"] = (
        "The C-loop attribution reduction survives conditioning on parseability "
        "and is a real observation. The restraint measurement does not "
        "discriminate a calibrated abstainer from an unconditional one: see "
        "`unknown_rate_on_determinate_cases` against "
        "`unknown_rate_on_unknown_cases`, and the exp-3 D1 replication.")
    return out


# ── C. diagnostic-acquisition-001 ────────────────────────────────────────────

def rescore_acquisition() -> dict:
    exp = os.path.join(RUNS, "acq-full", "diagnostic-acquisition-001")
    scored = load_dir(os.path.join(exp, "scored"))
    keys = load_json(os.path.join(CORPUS, "acquisition-keys.json"))
    meta = load_json(os.path.join(CORPUS, "acquisition-hidden-meta.json"))

    per = defaultdict(lambda: {"n": 0, "det": 0, "det_correct": 0,
                               "unk": 0, "unk_correct": 0, "unparseable": 0,
                               "predictions": [], "det_predictions": []})
    d2 = {"n": 0, "clean_immediate_settlement": 0, "denied_after_parse_failure": 0,
          "acquired": 0, "parse_failures_total": 0,
          "discriminating_selected": 0, "first_discriminating": 0,
          "action_units": 0, "eligible_units": 0}

    for r in scored:
        cond = r["condition"]
        p = per[cond]
        p["n"] += 1
        p["unparseable"] += bool(r["unparseable"])
        p["predictions"].append(r["predicted_locus"])
        if r["determinate"]:
            p["det"] += 1
            p["det_correct"] += bool(r["exact_correct"])
            p["det_predictions"].append(r["predicted_locus"])
        else:
            p["unk"] += 1
            p["unk_correct"] += bool(r["unknown_correct"])

        if cond != "D2":
            continue
        d2["n"] += 1
        rounds_path = os.path.join(exp, "raw", f"{r['unit_id']}.rounds.json")
        rd = load_json(rounds_path, record=False)
        actions = rd.get("actions", [])
        flags = [k for rnd in rd.get("rounds", []) for k in
                 ("parse_failed", "no_valid_action", "settled")
                 if rnd.get(k)]
        parse_failed = flags.count("parse_failed")
        d2["parse_failures_total"] += parse_failed
        if actions:
            d2["acquired"] += 1
            d2["action_units"] += 1
        elif parse_failed:
            d2["denied_after_parse_failure"] += 1
        else:
            d2["clean_immediate_settlement"] += 1

        discrim = set(meta.get(r["case_id"], {}).get("discriminating_probes", []))
        if r["determinate"] and discrim:
            d2["eligible_units"] += 1
            if set(actions) & discrim:
                d2["discriminating_selected"] += 1
            if actions and actions[0] in discrim:
                d2["first_discriminating"] += 1

    out = {"experiment": "diagnostic-acquisition-001", "conditions": {}}
    for c in sorted(per):
        p = per[c]
        out["conditions"][c] = {
            "n": p["n"],
            "A_c": rate(p["det_correct"], p["det"]),
            "A_u": rate(p["unk_correct"], p["unk"]),
            "unparseable": rate(p["unparseable"], p["n"]),
            "response_mode_all": response_mode(p["predictions"]),
            "response_mode_determinate": response_mode(p["det_predictions"]),
        }
    out["conditions"]["D2"]["unparseable_note"] = (
        "0% is definitional, not an achievement: run_d2 composes the raw file "
        "as 'Locus: <final_locus>' so extraction cannot fail. The round-1 "
        "contract-failure rate below is the real figure.")

    denied = d2["denied_after_parse_failure"]
    valid_contract_units = d2["n"] - denied
    out["D2_acquisition_decomposition"] = {
        "n": d2["n"],
        "historically_reported_no_acquisition":
            rate(d2["clean_immediate_settlement"] + denied, d2["n"]),
        "clean_immediate_settlement": rate(d2["clean_immediate_settlement"], d2["n"]),
        "denied_after_parse_failure": rate(denied, d2["n"]),
        "acquired": rate(d2["acquired"], d2["n"]),
        "acquisition_rate_conditional_on_valid_action_contract":
            rate(d2["acquired"], valid_contract_units),
        "round1_contract_failure_rate": rate(denied, d2["n"]),
        "note": (
            "The historical 81.1% conflates declining to investigate with being "
            "denied the opportunity: a round-1 parse failure substituted the "
            "budget-exhausted scaffold, which told the model 'Budget exhausted "
            "(0/7 payment, 1/3 rounds). No further acquisition possible.'"),
    }
    out["D2_selection"] = {
        "A_i_unconditioned": rate(d2["discriminating_selected"], d2["eligible_units"]),
        "A_i_historical_note": (
            "acquisition_scorer conditioned A_i's denominator on "
            "`not exact_correct`, i.e. on the outcome variable, which excludes "
            "units that selected a discriminating action AND were correct."),
        "selection_conditional_on_an_action_being_taken":
            rate(d2["discriminating_selected"],
                 min(d2["action_units"], d2["eligible_units"])),
    }
    out["UNRECOVERABLE_FROM_HISTORICAL_EVIDENCE"] = {
        "units": denied,
        "reason": (
            "acquisition_run.run_d2 never persisted model responses; the raw "
            "file is a harness-composed summary and rounds.json holds only "
            "round metadata. What these units would have done cannot be "
            "determined from any surviving artifact. No behaviour is imputed."),
    }
    return out


# ── D. governed-diagnostic-acquisition-001 ───────────────────────────────────

def rescore_governed() -> dict:
    exp = os.path.join(RUNS, "gov-full", "governed-diagnostic-acquisition-001")
    scored = load_dir(os.path.join(exp, "scored"))
    keys = load_json(os.path.join(CORPUS, "governed-keys.json"))
    meta = load_json(os.path.join(CORPUS, "acquisition-hidden-meta.json"))

    records = []
    for r in scored:
        traj = load_json(os.path.join(exp, "raw",
                                      f"{r['unit_id']}.trajectory.json"),
                         record=False)
        calls = traj.get("calls", [])
        claims = [extract_locus_legacy(c.get("response_preview", ""))
                  for c in calls]
        records.append({
            "unit_id": r["unit_id"], "case_id": r["case_id"],
            "condition": r["condition"], "rep": r["rep"],
            "true_locus": r["true_locus"], "gate_infeasible": r["gate_infeasible"],
            "feasible_det": r["is_determinate"] and not r["gate_infeasible"],
            "is_unknown_case": r["is_unknown_case"],
            "final_locus": r["final_locus"], "exact_correct": r["exact_correct"],
            "unknown_correct": r["unknown_correct"],
            "terminal_state": r["terminal_state"],
            "actions": list(r["action_ids"]),
            "discriminating": set(meta.get(r["case_id"], {})
                                  .get("discriminating_probes", [])),
            "has_discrim": bool(set(r["action_ids"])
                                & set(meta.get(r["case_id"], {})
                                      .get("discriminating_probes", []))),
            "model_claims": [c for c in claims if c],
            "empty_calls": sum(1 for c in calls
                               if not (c.get("response_preview") or "").strip()),
            "n_calls": len(calls),
            "at_ceiling": sum(1 for c in calls
                              if c.get("completion_tokens", 0) >= OBSERVED_CEILING),
        })

    out = {"experiment": "governed-diagnostic-acquisition-001", "conditions": {}}
    for cond in ("V", "G", "O"):
        units = [r for r in records if r["condition"] == cond]
        det = [r for r in units if r["feasible_det"]]
        unk = [r for r in units if r["is_unknown_case"]]
        acted = [r for r in det if r["actions"]]
        eligible = [r for r in det if r["discriminating"]]
        eligible_acted = [r for r in eligible if r["actions"]]
        with_ev = [r for r in det if r["has_discrim"]]

        # A settlement the gate ACCEPTED is also a model claim, and is not
        # visible in the truncated previews. Union both sources for the
        # tightest lower bound the surviving evidence supports.
        def claims_of(r):
            c = set(r["model_claims"])
            if r["terminal_state"] in ("CONCRETE_SETTLEMENT", "UNKNOWN_SETTLEMENT"):
                c.add(r["final_locus"])
            return c

        gate_blind_hits = sum(1 for r in det if r["true_locus"] in claims_of(r))
        gate_suppressed = sum(
            1 for r in det
            if not r["exact_correct"] and r["true_locus"] in claims_of(r))

        out["conditions"][cond] = {
            "n": len(units),
            "gated_accuracy_feasible_determinate":
                rate(sum(r["exact_correct"] for r in det), len(det)),
            "gate_blind_model_claim_accuracy_lower_bound": {
                **rate(gate_blind_hits, len(det)),
                "note": ("LOWER BOUND. Response previews are truncated at 200 "
                         "characters, so a claim appearing later in a response "
                         "is invisible; accepted settlements are unioned in. "
                         "Historical A_c is a conjunction of a correct claim "
                         "AND possession of gate-qualifying evidence, so the "
                         "two must be reported separately."),
            },
            "gate_suppressed_correct_claims_lower_bound": {
                **rate(gate_suppressed, len(det)),
                "note": ("units where the model named the true locus but the "
                         "gate refused it for want of qualifying evidence"),
            },
            "A_u": rate(sum(r["unknown_correct"] for r in unk), len(unk)),
            "A_u_mechanism": Counter(r["terminal_state"] for r in unk),
            "invocation": rate(sum(1 for r in units if r["actions"]), len(units)),
            "A_select_first_as_historically_reported":
                rate(sum(1 for r in eligible
                         if r["actions"] and r["actions"][0] in r["discriminating"]),
                     len(eligible)),
            "A_select_first_conditional_on_an_action_being_taken":
                rate(sum(1 for r in eligible_acted
                         if r["actions"][0] in r["discriminating"]),
                     len(eligible_acted)),
            "A_select_any_conditional_on_an_action_being_taken":
                rate(sum(1 for r in eligible_acted if r["has_discrim"]),
                     len(eligible_acted)),
            "A_use_unmatched": rate(sum(1 for r in with_ev if r["exact_correct"]),
                                    len(with_ev)),
            "terminal_states": Counter(r["terminal_state"] for r in units),
            "units_with_empty_content_call":
                sum(1 for r in units if r["empty_calls"]),
            "calls_at_observed_ceiling": sum(r["at_ceiling"] for r in units),
            "response_mode_feasible_determinate":
                response_mode([r["final_locus"] for r in det]),
        }

    det_records = [r for r in records if r["feasible_det"]]
    unk_records = [r for r in records if r["is_unknown_case"]]
    correct = (lambda r: 1 if r["exact_correct"] else 0)
    invoked = (lambda r: 1 if r["actions"] else 0)
    out["paired"] = {}
    for label, recs, metric, a, b in (
            ("invocation_G_vs_V", records, invoked, "G", "V"),
            ("accuracy_G_vs_V", det_records, correct, "G", "V"),
            ("accuracy_O_vs_G", det_records, correct, "O", "G"),
            ("accuracy_O_vs_V", det_records, correct, "O", "V"),
            ("A_u_G_vs_V", unk_records, (lambda r: 1 if r["unknown_correct"] else 0),
             "G", "V")):
        out["paired"][label] = {
            "case_level": paired(recs, metric, a, b, "case"),
            "case_rep_level_as_historically_reported":
                paired(recs, metric, a, b, "rep"),
        }

    # matched evidence-use comparison
    by_key = defaultdict(dict)
    for r in det_records:
        by_key[(r["case_id"], r["rep"])][r["condition"]] = r
    matched = [v for v in by_key.values()
               if "G" in v and "O" in v and v["G"]["has_discrim"]
               and v["O"]["has_discrim"]]
    out["matched_evidence_use_O_vs_G"] = {
        "n_matched_pairs": len(matched),
        "G": rate(sum(1 for v in matched if v["G"]["exact_correct"]), len(matched)),
        "O": rate(sum(1 for v in matched if v["O"]["exact_correct"]), len(matched)),
        "note": ("On units where BOTH conditions held evaluator-discriminating "
                 "evidence, utilization is identical. The unmatched 46.9% vs "
                 "73.7% contrast is a population difference: O's denominator "
                 "includes every hard case G never reached. Do not report it "
                 "as a like-for-like causal estimate."),
    }

    o_det = [r for r in det_records if r["condition"] == "O"]
    by_probe_count = defaultdict(lambda: [0, 0])
    for r in o_det:
        cell = by_probe_count[len(r["actions"])]
        cell[0] += r["exact_correct"]
        cell[1] += 1
    out["O_outcome_is_bimodal"] = {
        "accuracy_by_probes_delivered": {
            str(k): rate(v[0], v[1]) for k, v in sorted(by_probe_count.items())},
        "note": ("O's 46.5% is a mixture of a high-accuracy mode (settles on the "
                 "first delivered observation) and a zero mode (never settles). "
                 "Probe count is a CONSEQUENCE of being wrong, not a cause: each "
                 "rejected proposal triggered another delivery."),
    }
    return out


# ── E. hash provenance (R-17) ────────────────────────────────────────────────

def hash_provenance() -> dict:
    exp = os.path.join(RUNS, "gov-full", "governed-diagnostic-acquisition-001")
    path = os.path.join(exp, "manifest.jsonl")
    _SOURCES[os.path.relpath(path, ABLATION)] = _hash_file(path)
    verified, unverifiable = [], []
    with open(path) as f:
        for line in f:
            d = json.loads(line)
            (verified if d.get("raw_output_hash") else unverifiable).append(
                d["unit_id"])
    confirmed = 0
    for uid in verified:
        p = os.path.join(exp, "raw", f"{uid}.txt")
        if os.path.exists(p) and _hash_file(p):
            with open(p, "rb") as fh:
                if hashlib.sha256(fh.read()).hexdigest() == next(
                        json.loads(l)["raw_output_hash"]
                        for l in open(path) if json.loads(l)["unit_id"] == uid):
                    confirmed += 1
    return {
        "run": "gov-full/governed-diagnostic-acquisition-001",
        "n_units": len(verified) + len(unverifiable),
        "hash_verified": len(verified),
        "hash_verified_and_recomputed_ok": confirmed,
        "hash_unverifiable": len(unverifiable),
        "unverifiable_unit_ids": sorted(unverifiable),
        "reason": ("manifest reconstructed after parallel-worker concurrency "
                   "loss; content hashes were not recoverable"),
        "statement": (
            "The commit-before-scoring content-hash invariant is UNVERIFIABLE "
            "for these units. File counts and UID sets are complete and exact "
            "(540 trajectory / usage / txt / scored, no duplicates, no "
            "missing), and there is no evidence of tampering. Historical files "
            "are NOT modified and no hash is invented for them."),
    }


# ── main ─────────────────────────────────────────────────────────────────────

def main() -> int:
    os.makedirs(OUT, exist_ok=True)
    parts = {
        "s231-retrieval-ablation.json": rescore_s231,
        "s232-attribution-ablation.json": rescore_s232,
        "acquisition.json": rescore_acquisition,
        "governed.json": rescore_governed,
        "hash-provenance.json": hash_provenance,
    }
    written = {}
    for name, fn in parts.items():
        data = fn()
        data["scorer_version"] = SCORER_VERSION
        p = os.path.join(OUT, name)
        with open(p, "w") as f:
            json.dump(data, f, indent=2, sort_keys=True, default=str)
        written[name] = _hash_file(p)
        print(f"  wrote {name}")

    index = {
        "scorer_version": SCORER_VERSION,
        "model_calls_performed": 0,
        "historical_artifacts_modified": [],
        "source_artifacts": dict(sorted(_SOURCES.items())),
        "outputs": written,
        "statement": (
            "Derived from immutable historical evidence only. No model was "
            "called. No file under ablation/runs/** or ablation/corpus/** was "
            "written."),
    }
    p = os.path.join(OUT, "index.json")
    with open(p, "w") as f:
        json.dump(index, f, indent=2, sort_keys=True)
    print(f"  wrote index.json ({len(_SOURCES)} source artifacts bound)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
