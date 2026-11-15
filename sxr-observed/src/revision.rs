//! Governed structural revision, RealityTrace half (WP-18, E section 7).
//!
//! Three pure computations over `observed compare` result documents, no I/O:
//!
//! - [`propose`]: a revision proposal built from residuals, with the held-out
//!   rule. The rule is a frozen document whose digest comes from outside; the
//!   proposer does not pick the held-out set. A diagnosis input that overlaps
//!   the held-out set is refused `HELDOUT_CONTAMINATED`.
//! - [`regression_matrix`]: the prior episodes' residuals under the old and
//!   under the candidate world, judged by the total transition table of E
//!   section 7 ([`transition`]).
//! - [`capability_delta`]: AT-13, `GAIN` or `NO_GAIN`, from the same records
//!   under both worlds plus a control variant, against a frozen falsifier.
//!
//! Every output is canonical JSON, validated against its contract in
//! `contracts/` and deterministic for the same inputs.

use crate::{ObservedError, canon};
use jsonschema::JSONSchema;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

const HELDOUT_RULE_SCHEMA: &str = include_str!("../schemas/heldout-rule.v1.json");
const PROPOSAL_SCHEMA: &str = include_str!("../schemas/revision-proposal.v1.json");
const MATRIX_SCHEMA: &str = include_str!("../schemas/regression-matrix.v1.json");
const FROZEN_SCHEMA: &str = include_str!("../schemas/capability-frozen.v1.json");
const DELTA_SCHEMA: &str = include_str!("../schemas/capability-delta.v1.json");

pub const CLASSES: [&str; 5] = [
    "confirmed",
    "contradicted",
    "observed_undeclared",
    "declared_unobserved",
    "insufficient",
];
/// Matrix vocabulary: the five classes plus an element a world does not have.
pub const MATRIX_CLASSES: [&str; 6] = [
    "confirmed",
    "contradicted",
    "observed_undeclared",
    "declared_unobserved",
    "insufficient",
    "absent",
];

fn err(code: &'static str, msg: impl Into<String>) -> ObservedError {
    ObservedError::new(code, msg)
}

fn validate(
    schema_text: &str,
    value: &Value,
    code: &'static str,
    what: &str,
) -> Result<(), ObservedError> {
    let schema: Value = serde_json::from_str(schema_text).expect("vendored schema is JSON");
    let compiled = JSONSchema::compile(&schema).expect("vendored schema compiles");
    if let Err(it) = compiled.validate(value) {
        let msgs: Vec<String> = it
            .take(5)
            .map(|e| format!("{}: {e}", e.instance_path))
            .collect();
        return Err(err(code, format!("{what}: {}", msgs.join("; "))));
    }
    Ok(())
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| x.as_str().map(str::to_owned))
        .collect()
}

fn hex_of(v: &Value) -> Result<String, ObservedError> {
    Ok(canon::digest(v)?.trim_start_matches("sha256:").to_owned())
}

/// `sha256:<hex>` of a document, with the named self-id field removed.
fn digest_without(v: &Value, field: &str) -> Result<String, ObservedError> {
    let mut body = v.clone();
    if let Some(m) = body.as_object_mut() {
        m.remove(field);
    }
    hex_of(&body)
}

/// The digest of a frozen held-out rule document (what the pre-registration
/// records and `observed propose --rule-sha256` is checked against).
pub fn rule_digest(rule: &Value) -> Result<String, ObservedError> {
    canon::digest(rule)
}

/// The digest of a frozen capability-delta definition.
pub fn frozen_digest(frozen: &Value) -> Result<String, ObservedError> {
    canon::digest(frozen)
}

// ------------------------------------------------------------ comparison docs

/// One element of one compare result: `(comparison, expectation) -> class`.
fn elements(doc: &Value) -> Result<BTreeMap<(String, String), String>, ObservedError> {
    let mut out = BTreeMap::new();
    for list in ["episode", "structural"] {
        for r in doc["residuals"][list].as_array().into_iter().flatten() {
            let rid = r["residual_id"].as_str().unwrap_or_default();
            let expectation = doc["annotations"][rid]["expectation"]
                .as_str()
                .ok_or_else(|| {
                    err(
                        "COMPARE_DOCUMENT_INVALID",
                        format!("residual {rid} has no annotation.expectation"),
                    )
                })?;
            let class = r["class"].as_str().unwrap_or_default().to_owned();
            if !CLASSES.contains(&class.as_str()) {
                return Err(err(
                    "COMPARE_DOCUMENT_INVALID",
                    format!("residual {rid} has class {class:?}"),
                ));
            }
            if out
                .insert((list.to_owned(), expectation.to_owned()), class)
                .is_some()
            {
                return Err(err(
                    "COMPARE_DOCUMENT_INVALID",
                    format!("expectation {expectation} appears twice in {list}"),
                ));
            }
        }
    }
    Ok(out)
}

fn compare_doc_head(doc: &Value) -> Result<(String, String, String), ObservedError> {
    let get = |f: &str| {
        doc[f].as_str().map(str::to_owned).ok_or_else(|| {
            err(
                "COMPARE_DOCUMENT_INVALID",
                format!("compare document has no {f}"),
            )
        })
    };
    Ok((
        get("episode_ref")?,
        get("world_ref")?,
        get("comparison_id")?,
    ))
}

fn episode_matches(entry: &str, episode_ref: &str) -> bool {
    episode_ref == entry
        || episode_ref
            .strip_prefix(entry)
            .is_some_and(|rest| rest.starts_with('@'))
}

// ----------------------------------------------------------------- propose

pub struct ProposeInput<'a> {
    pub proposer: &'a str,
    pub scope: &'a str,
    pub old_world_ref: &'a str,
    pub candidate_world_ref: &'a str,
    /// The frozen `heldout-rule.v1` document.
    pub rule: &'a Value,
    /// Its digest as frozen outside this command (`sha256:<hex>`). Required.
    pub rule_sha256: Option<&'a str>,
    /// Every compare result available at proposal time (old world).
    pub compared: &'a [Value],
    /// Residual ids the proposer cites as the diagnosis input.
    pub diagnosis_residuals: &'a [String],
    /// `[{element, change, supersedes?}]`.
    pub intended_changes: &'a Value,
    pub semantic_diff_sha256: &'a str,
}

pub fn propose(inp: &ProposeInput) -> Result<Value, ObservedError> {
    validate(
        HELDOUT_RULE_SCHEMA,
        inp.rule,
        "HELDOUT_RULE_INVALID",
        "held-out rule",
    )?;
    let rule_digest = canon::digest(inp.rule)?;
    match inp.rule_sha256 {
        None => {
            return Err(err(
                "HELDOUT_RULE_NOT_FROZEN",
                "the held-out rule digest from the pre-registration is required",
            ));
        }
        Some(d) if d != rule_digest => {
            return Err(err(
                "HELDOUT_RULE_DIGEST_MISMATCH",
                format!("rule digest is {rule_digest}, frozen digest is {d}"),
            ));
        }
        Some(_) => {}
    }
    if inp.old_world_ref == inp.candidate_world_ref {
        return Err(err(
            "PROPOSAL_INVALID",
            "candidate world equals the old world",
        ));
    }
    let held_entries = strs(&inp.rule["held_out_episodes"]);

    // Partition the compared episodes by the rule, never by the proposer.
    let mut diagnosable: Vec<&Value> = vec![];
    let mut held_docs: Vec<&Value> = vec![];
    let mut matched_entries: BTreeSet<&str> = BTreeSet::new();
    for doc in inp.compared {
        let (ep, world, _) = compare_doc_head(doc)?;
        if world != inp.old_world_ref {
            return Err(err(
                "WORLD_REF_MISMATCH",
                format!("{ep} was compared under {world}, not {}", inp.old_world_ref),
            ));
        }
        match held_entries.iter().find(|h| episode_matches(h, &ep)) {
            Some(h) => {
                matched_entries.insert(h.as_str());
                held_docs.push(doc);
            }
            None => diagnosable.push(doc),
        }
    }
    if let Some(missing) = held_entries
        .iter()
        .find(|h| !matched_entries.contains(h.as_str()))
    {
        return Err(err(
            "HELDOUT_UNAVAILABLE",
            format!(
                "held-out episode {missing} has no compare result: the held-out set must be recorded before the proposal"
            ),
        ));
    }

    // Diagnosis input = the cited residuals; each must come from a non-held-out episode.
    if inp.diagnosis_residuals.is_empty() {
        return Err(err("DIAGNOSIS_EMPTY", "no residual is cited as diagnosis"));
    }
    let mut diag_eps: BTreeSet<String> = BTreeSet::new();
    let mut diag_res: BTreeSet<String> = BTreeSet::new();
    let mut diag_occ: BTreeSet<String> = BTreeSet::new();
    for rid in inp.diagnosis_residuals {
        let in_held = held_docs
            .iter()
            .any(|d| residual_ids(d).contains(rid.as_str()));
        if in_held {
            return Err(err(
                "HELDOUT_CONTAMINATED",
                format!("diagnosis cites {rid}, a residual of a held-out episode"),
            ));
        }
        let Some(doc) = diagnosable
            .iter()
            .find(|d| residual_ids(d).contains(rid.as_str()))
        else {
            return Err(err(
                "RESIDUAL_UNKNOWN",
                format!("{rid} is in no compare result"),
            ));
        };
        diag_eps.insert(doc["episode_ref"].as_str().unwrap_or_default().to_owned());
        diag_res.insert(rid.clone());
        for list in ["episode", "structural"] {
            for r in doc["residuals"][list].as_array().into_iter().flatten() {
                if r["residual_id"] == json!(rid) {
                    diag_occ.extend(strs(&r["occurrence_ids"]));
                }
            }
        }
    }
    let mut held_eps: BTreeSet<String> = BTreeSet::new();
    let mut held_res: BTreeSet<String> = BTreeSet::new();
    let mut held_occ: BTreeSet<String> = BTreeSet::new();
    for d in &held_docs {
        held_eps.insert(d["episode_ref"].as_str().unwrap_or_default().to_owned());
        held_res.extend(residual_ids(d).into_iter().map(str::to_owned));
        for list in ["episode", "structural"] {
            for r in d["residuals"][list].as_array().into_iter().flatten() {
                held_occ.extend(strs(&r["occurrence_ids"]));
            }
        }
    }
    if let Some(o) = diag_occ.intersection(&held_occ).next() {
        return Err(err(
            "HELDOUT_CONTAMINATED",
            format!("occurrence {o} is in both the diagnosis input and the held-out set"),
        ));
    }
    if diag_eps.intersection(&held_eps).next().is_some()
        || diag_res.intersection(&held_res).next().is_some()
    {
        return Err(err(
            "HELDOUT_CONTAMINATED",
            "diagnosis and held-out sets overlap",
        ));
    }

    let mut doc = json!({
        "contract": "revision-proposal.v1",
        "proposer": inp.proposer,
        "scope": inp.scope,
        "old_world_ref": inp.old_world_ref,
        "candidate_world_ref": inp.candidate_world_ref,
        "held_out_rule": {"rule_id": inp.rule["rule_id"], "sha256": rule_digest},
        "diagnosis": {"episodes": diag_eps, "residual_ids": diag_res, "occurrence_ids": diag_occ},
        "held_out": {"episodes": held_eps, "residual_ids": held_res, "occurrence_ids": held_occ},
        "intended_changes": inp.intended_changes,
        "semantic_diff_sha256": inp.semantic_diff_sha256,
    });
    let id = digest_without(&doc, "proposal_id")?;
    doc["proposal_id"] = json!(format!("proposal:{id}"));
    validate(
        PROPOSAL_SCHEMA,
        &doc,
        "PROPOSAL_INVALID",
        "revision proposal",
    )?;
    Ok(doc)
}

fn residual_ids(doc: &Value) -> BTreeSet<&str> {
    ["episode", "structural"]
        .iter()
        .flat_map(|l| doc["residuals"][*l].as_array().into_iter().flatten())
        .filter_map(|r| r["residual_id"].as_str())
        .collect()
}

/// Recompute and check a proposal's id and schema.
pub fn check_proposal(proposal: &Value) -> Result<String, ObservedError> {
    validate(
        PROPOSAL_SCHEMA,
        proposal,
        "PROPOSAL_INVALID",
        "revision proposal",
    )?;
    let want = format!("proposal:{}", digest_without(proposal, "proposal_id")?);
    if proposal["proposal_id"] != json!(want) {
        return Err(err(
            "PROPOSAL_INVALID",
            format!("proposal_id is not the digest of the document (want {want})"),
        ));
    }
    Ok(format!("sha256:{}", &want["proposal:".len()..]))
}

// ----------------------------------------------------------------- matrix

/// The result of one matrix row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub pass: bool,
    pub code: Option<&'static str>,
    pub reason: &'static str,
    pub escalated: bool,
    pub reported: Option<&'static str>,
}

fn pass(reason: &'static str) -> Outcome {
    Outcome {
        pass: true,
        code: None,
        reason,
        escalated: false,
        reported: None,
    }
}
fn fail(code: &'static str, reason: &'static str) -> Outcome {
    Outcome {
        pass: false,
        code: Some(code),
        reason,
        escalated: false,
        reported: None,
    }
}

/// What the proposal says about an element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Listing {
    /// The element is in the proposal's intended changes.
    pub listed: bool,
    /// ... as a `weaken` or `remove`.
    pub weakening: bool,
    /// The two worlds' comparisons have the same coverage (same profile, same
    /// silent evidence sources).
    pub coverage_unchanged: bool,
}

/// The total transition table of E section 7 (RD-25). `new` may be `absent`
/// (the candidate has no such element) and `old` may be `absent` (a new element).
/// An element missing in the candidate counts as `insufficient`, except a
/// `contradicted` one, whose removal is a weakening.
pub fn transition(old: &str, new: &str, l: Listing) -> Outcome {
    if old == "absent" {
        let mut o = pass("new_element");
        o.reported = Some("new_element");
        return o;
    }
    if old == new {
        return pass("unchanged");
    }
    // contradicted -> confirmed / removed: a weakening or removal of a policy.
    if old == "contradicted" && (new == "confirmed" || new == "absent") {
        return if l.listed && l.weakening {
            Outcome {
                pass: true,
                code: None,
                reason: "listed_weakening",
                escalated: true,
                reported: Some("weakening"),
            }
        } else {
            fail("UNLISTED_WEAKENING", "weakened_or_removed_without_listing")
        };
    }
    let new = if new == "absent" { "insufficient" } else { new };
    match (old, new) {
        ("confirmed", "contradicted") => {
            if l.listed {
                pass("intended_break")
            } else {
                fail("REGRESSION_FAILED", "confirmed_element_broken")
            }
        }
        ("confirmed", "insufficient" | "observed_undeclared") => {
            fail("LOST_CAPABILITY", "confirmed_capability_lost")
        }
        ("confirmed", "declared_unobserved") => {
            fail("REGRESSION_FAILED", "confirmed_now_declared_unobserved")
        }
        ("observed_undeclared", "confirmed") => {
            let mut o = pass("intended_gain");
            o.reported = Some("gain");
            o
        }
        ("observed_undeclared", "contradicted") => {
            if l.listed {
                pass("declared_as_violation")
            } else {
                fail("REGRESSION_FAILED", "undeclared_now_violation_unlisted")
            }
        }
        ("insufficient", _) => {
            let mut o = pass("evidence_gained");
            o.reported = Some("evidence_gained");
            o
        }
        (_, "insufficient") => {
            if l.listed && l.coverage_unchanged {
                pass("listed_evidence_change")
            } else {
                fail("REGRESSION_FAILED", "evidence_lost")
            }
        }
        _ => pass("other_transition"),
    }
}

pub struct MatrixInput<'a> {
    pub proposal: &'a Value,
    /// Compare results of the prior episodes under the old world.
    pub old: &'a [Value],
    /// The same episodes compared under the candidate world.
    pub new: &'a [Value],
}

pub fn regression_matrix(inp: &MatrixInput) -> Result<Value, ObservedError> {
    let proposal_digest = check_proposal(inp.proposal)?;
    let old_world = inp.proposal["old_world_ref"].as_str().unwrap_or_default();
    let cand_world = inp.proposal["candidate_world_ref"]
        .as_str()
        .unwrap_or_default();
    let mut old_docs: BTreeMap<String, &Value> = BTreeMap::new();
    let mut new_docs: BTreeMap<String, &Value> = BTreeMap::new();
    for (docs, world, map) in [
        (inp.old, old_world, &mut old_docs),
        (inp.new, cand_world, &mut new_docs),
    ] {
        for d in docs {
            let (ep, w, _) = compare_doc_head(d)?;
            if w != world {
                return Err(err(
                    "WORLD_REF_MISMATCH",
                    format!("{ep} was compared under {w}, expected {world}"),
                ));
            }
            if map.insert(ep.clone(), d).is_some() {
                return Err(err("MATRIX_INPUT_MISMATCH", format!("{ep} is given twice")));
            }
        }
    }
    if old_docs.is_empty() {
        return Err(err("MATRIX_INPUT_INCOMPLETE", "no prior episode compared"));
    }
    if old_docs.keys().ne(new_docs.keys()) {
        return Err(err(
            "MATRIX_INPUT_MISMATCH",
            "the old and candidate comparisons cover different episodes",
        ));
    }
    for set in ["diagnosis", "held_out"] {
        for ep in strs(&inp.proposal[set]["episodes"]) {
            if !old_docs.contains_key(&ep) {
                return Err(err(
                    "MATRIX_INPUT_INCOMPLETE",
                    format!("proposal {set} episode {ep} is not in the compared set"),
                ));
            }
        }
    }

    // Intended changes: which elements are listed, which are weakenings, and
    // the explicit old -> new element mapping (`supersedes`).
    let mut listed: BTreeMap<String, bool> = BTreeMap::new(); // element -> weakening
    let mut alias: BTreeMap<String, String> = BTreeMap::new();
    for c in inp.proposal["intended_changes"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let element = c["element"].as_str().unwrap_or_default().to_owned();
        let weakening = matches!(c["change"].as_str(), Some("weaken" | "remove"));
        let e = listed.entry(element.clone()).or_insert(false);
        *e = *e || weakening;
        for old in strs(&c["supersedes"]) {
            alias.insert(old, element.clone());
        }
    }

    let mut rows: Vec<Value> = vec![];
    let mut keyed: Vec<(String, Value)> = vec![];
    for (ep, od) in &old_docs {
        let nd = new_docs[ep];
        let (oe, ne) = (elements(od)?, elements(nd)?);
        let coverage_unchanged = od["profile"]["sha256"] == nd["profile"]["sha256"]
            && od["silent_evidence_sources"] == nd["silent_evidence_sources"];
        let mut seen_new: BTreeSet<(String, String)> = BTreeSet::new();
        for ((cmp, old_id), oel) in &oe {
            let target = alias.get(old_id).cloned().unwrap_or_else(|| old_id.clone());
            let nel = ne.get(&(cmp.clone(), target.clone()));
            let new_class = nel.map_or("absent", String::as_str);
            seen_new.insert((cmp.clone(), target.clone()));
            let is_listed = listed.contains_key(&target) || listed.contains_key(old_id);
            let weakening = listed.get(&target).copied().unwrap_or(false)
                || listed.get(old_id).copied().unwrap_or(false);
            let o = transition(
                oel,
                new_class,
                Listing {
                    listed: is_listed,
                    weakening,
                    coverage_unchanged,
                },
            );
            keyed.push((
                format!("{ep}|{cmp}|{old_id}"),
                matrix_row(
                    ep,
                    cmp,
                    Some(old_id),
                    nel.map(|_| target.as_str()),
                    oel,
                    new_class,
                    &o,
                    is_listed,
                ),
            ));
        }
        for ((cmp, new_id), nel) in &ne {
            if seen_new.contains(&(cmp.clone(), new_id.clone())) {
                continue;
            }
            let is_listed = listed.contains_key(new_id);
            let o = transition(
                "absent",
                nel,
                Listing {
                    listed: is_listed,
                    weakening: false,
                    coverage_unchanged,
                },
            );
            keyed.push((
                format!("{ep}|{cmp}|~{new_id}"),
                matrix_row(ep, cmp, None, Some(new_id), "absent", nel, &o, is_listed),
            ));
        }
    }
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    rows.extend(keyed.into_iter().map(|(_, r)| r));

    let count = |f: &dyn Fn(&Value) -> bool| rows.iter().filter(|r| f(r)).count();
    let failures: BTreeSet<&str> = rows
        .iter()
        .filter(|r| r["result"] == json!("fail"))
        .filter_map(|r| r["code"].as_str())
        .collect();
    let refusal_code = if failures.contains("REGRESSION_FAILED") {
        Some("REGRESSION_FAILED")
    } else {
        failures.iter().next().copied()
    };
    let summary = json!({
        "pass": count(&|r| r["result"] == json!("pass")),
        "fail": count(&|r| r["result"] == json!("fail")),
        "escalated": count(&|r| r["escalated"] == json!(true)),
        "gains": count(&|r| r["reported"] == json!("gain")),
    });
    let comparison_ids = |m: &BTreeMap<String, &Value>| -> Vec<String> {
        let mut v: Vec<String> = m
            .values()
            .filter_map(|d| d["comparison_id"].as_str().map(str::to_owned))
            .collect();
        v.sort();
        v
    };
    let mut doc = json!({
        "contract": "regression-matrix.v1",
        "proposal_id": inp.proposal["proposal_id"],
        "proposal_digest": proposal_digest,
        "old_world_ref": old_world,
        "candidate_world_ref": cand_world,
        "old_comparisons": comparison_ids(&old_docs),
        "new_comparisons": comparison_ids(&new_docs),
        "rows": rows,
        "summary": summary,
        "verdict": if failures.is_empty() { "pass" } else { "fail" },
        "failure_codes": failures,
        "refusal_code": refusal_code,
        "escalated": count(&|r| r["escalated"] == json!(true)) > 0,
    });
    let id = digest_without(&doc, "matrix_id")?;
    doc["matrix_id"] = json!(format!("matrix:{id}"));
    validate(MATRIX_SCHEMA, &doc, "MATRIX_INVALID", "regression matrix")?;
    Ok(doc)
}

#[allow(clippy::too_many_arguments)]
fn matrix_row(
    ep: &str,
    cmp: &str,
    old_exp: Option<&str>,
    new_exp: Option<&str>,
    old_class: &str,
    new_class: &str,
    o: &Outcome,
    intended: bool,
) -> Value {
    let mut r = Map::new();
    r.insert(
        "element".into(),
        json!(format!(
            "{ep}|{cmp}|{}",
            old_exp.or(new_exp).unwrap_or_default()
        )),
    );
    r.insert("episode_ref".into(), json!(ep));
    r.insert("comparison".into(), json!(cmp));
    r.insert("old_expectation".into(), json!(old_exp));
    r.insert("new_expectation".into(), json!(new_exp));
    r.insert("old_class".into(), json!(old_class));
    r.insert("new_class".into(), json!(new_class));
    r.insert("result".into(), json!(if o.pass { "pass" } else { "fail" }));
    r.insert("code".into(), json!(o.code));
    r.insert("reason".into(), json!(o.reason));
    r.insert("intended".into(), json!(intended));
    r.insert("escalated".into(), json!(o.escalated));
    r.insert("reported".into(), json!(o.reported));
    Value::Object(r)
}

/// Recompute nothing, check everything checkable about a matrix document:
/// schema, id, and that the verdict agrees with the rows. Used by consumers
/// that must not trust a matrix's own `verdict`.
pub fn check_matrix(matrix: &Value) -> Result<(), ObservedError> {
    validate(MATRIX_SCHEMA, matrix, "MATRIX_INVALID", "regression matrix")?;
    let want = format!("matrix:{}", digest_without(matrix, "matrix_id")?);
    if matrix["matrix_id"] != json!(want) {
        return Err(err(
            "MATRIX_INVALID",
            "matrix_id is not the digest of the document",
        ));
    }
    let any_fail = matrix["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|r| r["result"] == json!("fail"));
    if any_fail != (matrix["verdict"] == json!("fail")) {
        return Err(err("MATRIX_INVALID", "verdict disagrees with the rows"));
    }
    Ok(())
}

// ------------------------------------------------------- capability delta

pub struct DeltaInput<'a> {
    pub frozen: &'a Value,
    /// The digest of `frozen` recorded in the pre-registration. Required.
    pub frozen_sha256: Option<&'a str>,
    /// The subsequent episode's records under the old world.
    pub old: &'a Value,
    /// ... under the new world.
    pub new: &'a Value,
    /// The control variant (records without the newly declared step) under the old world.
    pub control_old: Option<&'a Value>,
    /// ... and under the new world.
    pub control_new: Option<&'a Value>,
}

fn class_of(doc: &Value, comparison: &str, expectation: &str) -> Result<String, ObservedError> {
    Ok(elements(doc)?
        .get(&(comparison.to_owned(), expectation.to_owned()))
        .map_or("absent", String::as_str)
        .to_owned())
}

pub fn capability_delta(inp: &DeltaInput) -> Result<Value, ObservedError> {
    validate(
        FROZEN_SCHEMA,
        inp.frozen,
        "FALSIFIER_NOT_FROZEN",
        "frozen capability definition",
    )?;
    let digest = canon::digest(inp.frozen)?;
    match inp.frozen_sha256 {
        None => {
            return Err(err(
                "FALSIFIER_NOT_FROZEN",
                "the frozen definition's digest from the pre-registration is required",
            ));
        }
        Some(d) if d != digest => {
            return Err(err(
                "FALSIFIER_NOT_FROZEN",
                format!("definition digest is {digest}, frozen digest is {d}"),
            ));
        }
        Some(_) => {}
    }
    let (Some(control_old), Some(control_new)) = (inp.control_old, inp.control_new) else {
        return Err(err(
            "CONTROL_VARIANT_MISSING",
            "the control variant (records without the newly declared step) was not run under both worlds",
        ));
    };
    let key = inp.frozen["episode_key"].as_str().unwrap_or_default();
    let (old_ep, old_world, old_cid) = compare_doc_head(inp.old)?;
    let (new_ep, new_world, new_cid) = compare_doc_head(inp.new)?;
    let (co_ep, co_world, co_cid) = compare_doc_head(control_old)?;
    let (cn_ep, cn_world, cn_cid) = compare_doc_head(control_new)?;
    if !episode_matches(key, &old_ep) {
        return Err(err(
            "EPISODE_MISMATCH",
            format!("{old_ep} is not the frozen episode {key}"),
        ));
    }
    if old_ep != new_ep || old_world == new_world || old_world != co_world || new_world != cn_world
    {
        return Err(err(
            "EPISODE_MISMATCH",
            "the same episode must be compared under the old world and the new world, and the control under the same two worlds",
        ));
    }
    if co_ep == old_ep || cn_ep == old_ep || co_ep != cn_ep {
        return Err(err(
            "CONTROL_VARIANT_MISSING",
            "the control variant must be its own episode (the records without the step), compared under both worlds",
        ));
    }
    let cmp = inp.frozen["comparison"].as_str().unwrap_or_default();
    let (oe, ne) = (
        inp.frozen["old_expectation"].as_str().unwrap_or_default(),
        inp.frozen["new_expectation"].as_str().unwrap_or_default(),
    );
    let c_old = class_of(inp.old, cmp, oe)?;
    let c_new = class_of(inp.new, cmp, ne)?;
    let c_control_old = class_of(control_old, cmp, oe)?;
    let c_control_new = class_of(control_new, cmp, ne)?;

    let frozen_conditions = strs(&inp.frozen["falsifier"]);
    let mut triggered: BTreeSet<&str> = BTreeSet::new();
    if frozen_conditions.iter().any(|c| c == "old_world_confirmed") && c_old == "confirmed" {
        triggered.insert("old_world_confirmed");
    }
    if frozen_conditions
        .iter()
        .any(|c| c == "new_world_not_confirmed")
        && c_new != "confirmed"
    {
        triggered.insert("new_world_not_confirmed");
    }
    let control_gain = c_control_new == "confirmed";
    if frozen_conditions.iter().any(|c| c == "control_shows_gain") && control_gain {
        triggered.insert("control_shows_gain");
    }
    let old_ok = c_old == "observed_undeclared" || c_old == "absent";
    let gain = old_ok && c_new == "confirmed" && !control_gain && triggered.is_empty();
    let result = if gain { "GAIN" } else { "NO_GAIN" };
    // NT-38: the control gives the same result as the real episode.
    let code = (!gain && control_gain && c_new == "confirmed").then_some("NO_CAPABILITY_GAIN");
    let prediction = inp.frozen["prediction"].as_str().unwrap_or_default();
    let mut doc = json!({
        "contract": "capability-delta.v1",
        "result": result,
        "code": code,
        "frozen": {"frozen_id": inp.frozen["frozen_id"], "sha256": digest, "prediction": prediction},
        "old_world_ref": old_world,
        "new_world_ref": new_world,
        "episode_ref": old_ep,
        "classes": {"old": c_old, "new": c_new, "control_old": c_control_old, "control_new": c_control_new},
        "falsifier_triggered": triggered,
        "prediction_met": prediction == result,
        "r58_supported": gain,
        "comparisons": {"old": old_cid, "new": new_cid, "control_old": co_cid, "control_new": cn_cid},
    });
    let id = digest_without(&doc, "delta_id")?;
    doc["delta_id"] = json!(format!("delta:{id}"));
    validate(DELTA_SCHEMA, &doc, "DELTA_INVALID", "capability delta")?;
    Ok(doc)
}

/// Canonical text of a document (what the CLI prints).
pub fn canonical(doc: &Value) -> Result<String, ObservedError> {
    canon::canonical(doc)
}
