//! `observed compare` (WP-17): the observed representation against the
//! declared expectations, as residual records of `contracts/residual.v1.json`.
//!
//! Three-way decision split (E section 6):
//! - policy-evaluable expectations: a generated, hashed facts file and the
//!   DomainForge `evaluate` verb (true to `confirmed`, false to `contradicted`,
//!   unknown to `insufficient`);
//! - structural expectations (declared flows, operation preconditions,
//!   triggered obligations): decided here over declared relations and observed
//!   occurrences; undeclared behaviour is `observed_undeclared`;
//! - anything neither can decide: `insufficient`, basis `not_evaluable`.
//!
//! Compare reads and writes nothing but its own result document. It never
//! adds to the declared model: an `undeclared:` node stays undeclared. Missing
//! evidence is never success: a frozen evidence source that emitted nothing
//! makes its expectations `insufficient` / `evidence_source_silent`, and a
//! `confirmed` without a single occurrence from an independent emitter is
//! `insufficient` / `executor_only_evidence`.
//!
//! Every input is checked before anything is decided: the ledger hash chain,
//! the expected head (tail), the representation's own id and input-set hash
//! against the ledger it came from, the frozen reference digest, and the
//! pinned profile digest.

use crate::ObservedError;
use crate::canon;
use crate::ingest::EXTENSION_KEY;
use crate::tail::{self, ExpectedHead};
use jsonschema::JSONSchema;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use sxr_core::LedgerRow;
use sxr_core::canonical_json::sha256_hex;
use sxr_core::chain::chain_verify;

const RESIDUAL_SCHEMA: &str = include_str!("../schemas/residual.v1.json");
const PROFILE_SCHEMA: &str = include_str!("../schemas/comparison-profile.v1.json");
const REPRESENTATION_SCHEMA: &str = include_str!("../schemas/observed-representation.v1.json");
const EVALUATE_FACTS_SCHEMA: &str = include_str!("../schemas/evaluate-facts.v1.json");
const EVALUATE_OUTPUT_SCHEMA: &str = include_str!("../schemas/evaluate-output.v1.json");

/// Who ran `evaluate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluateIdentity {
    pub binary: String,
    pub version: String,
}

/// `domainforge evaluate` as a port. The model is the evaluator's own.
pub trait Evaluator {
    fn identity(&self) -> EvaluateIdentity;
    /// The model bytes `evaluate` was given (hashed into the result).
    fn model_bytes(&self) -> Result<Vec<u8>, ObservedError>;
    /// Run `evaluate --facts <facts_text> --world-name <world_name>`; return
    /// the parsed JSON output. A failure is an error, never a default.
    fn evaluate(&self, facts_text: &str, world_name: &str) -> Result<Value, ObservedError>;
}

pub struct CompareInput<'a> {
    /// The whole ledger, in append order.
    pub rows: &'a [LedgerRow],
    /// The `observed derive` document (`representation_id`, `window`, ...).
    pub observed: &'a Value,
    /// The frozen reference `{episode_ref, body}`.
    pub reference: &'a Value,
    /// The comparison profile document named by `reference.body.comparison_profile`.
    pub profile: &'a Value,
    /// The key and expectation mapping (see `contracts/README.md` section 5).
    pub mapping: &'a Value,
    pub world_ref: &'a str,
    /// Declared snapshot id the comparison runs against.
    pub snapshot: &'a str,
    /// A ledger head recorded outside the ledger. `None`: tail `unverified`.
    pub expected_head: Option<ExpectedHead>,
    /// Required when any policy-evaluable expectation has occurrences.
    pub evaluator: Option<&'a dyn Evaluator>,
}

#[derive(Debug, Clone)]
pub struct Compared {
    /// The result document.
    pub document: Value,
    /// Canonical bytes of `document`: what the CLI prints.
    pub canonical: String,
    /// The generated facts file, byte for byte (None: `evaluate` not run).
    pub facts: Option<String>,
}

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

// ---------------------------------------------------------------- profile

#[derive(Debug, Clone)]
struct ProfileEntry {
    comparison: String,
    quantifier: String,
    min_support: u64,
    allow_vacuous_truth: bool,
}

fn profile_entries(profile: &Value) -> Result<BTreeMap<String, ProfileEntry>, ObservedError> {
    let mut out = BTreeMap::new();
    for e in profile["kinds"].as_array().into_iter().flatten() {
        let kind = e["kind"].as_str().unwrap_or_default().to_owned();
        let entry = ProfileEntry {
            comparison: e["comparison"].as_str().unwrap_or_default().to_owned(),
            quantifier: e["quantifier"].as_str().unwrap_or_default().to_owned(),
            min_support: e["min_support"].as_u64().unwrap_or(1),
            allow_vacuous_truth: e["allow_vacuous_truth"].as_bool().unwrap_or(false),
        };
        if out.insert(kind.clone(), entry).is_some() {
            return Err(err(
                "PROFILE_DUPLICATE_KIND",
                format!("two profile entries for kind {kind}"),
            ));
        }
    }
    Ok(out)
}

// ------------------------------------------------------------ occurrences

#[derive(Debug, Clone)]
struct Occ {
    id: String,
    system: String,
    record_id: String,
    sequence: u64,
    causation: Option<String>,
    ext: Value,
}

struct Rep<'a> {
    doc: &'a Value,
    /// node id -> node.
    nodes: BTreeMap<String, &'a Value>,
    occs: BTreeMap<String, Occ>,
    evidence: HashMap<String, String>,
}

impl Rep<'_> {
    fn node_occs(&self, key: &str) -> Vec<&Occ> {
        self.nodes
            .get(key)
            .map(|n| {
                n["occurrence_ids"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|i| i.as_str().and_then(|i| self.occs.get(i)))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn evidence_for<'b>(&self, ids: impl IntoIterator<Item = &'b String>) -> BTreeSet<String> {
        ids.into_iter()
            .filter_map(|i| self.occs.get(i.as_str()))
            .filter_map(|o| self.evidence.get(&o.record_id).cloned())
            .collect()
    }

    fn independent(&self, keys: &[String]) -> u64 {
        keys.iter()
            .filter_map(|k| self.nodes.get(k))
            .map(|n| n["independent_occurrence_count"].as_u64().unwrap_or(0))
            .sum()
    }

    fn context(&self, keys: &[String]) -> Value {
        let mut roles: BTreeMap<String, u64> = BTreeMap::new();
        let mut items: BTreeMap<String, u64> = BTreeMap::new();
        for n in keys.iter().filter_map(|k| self.nodes.get(k)) {
            for (src, dst) in [("roles", &mut roles), ("plan_items", &mut items)] {
                for (k, v) in n["context_distribution"][src]
                    .as_object()
                    .into_iter()
                    .flatten()
                {
                    *dst.entry(k.clone()).or_default() += v.as_u64().unwrap_or(0);
                }
            }
        }
        json!({"roles": roles, "plan_items": items})
    }
}

fn evidence_index<'a>(rows: impl IntoIterator<Item = &'a LedgerRow>) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for row in rows {
        if row.record_kind != "evidence.recorded" && row.record_kind != "evidence.created" {
            continue;
        }
        let Ok(p) = serde_json::from_str::<Value>(&row.payload) else {
            continue;
        };
        for item in p["evidence"].as_array().into_iter().flatten() {
            let Some(id) = item["evidence_id"].as_str() else {
                continue;
            };
            let eref = if id.starts_with("evidence:") {
                id.to_owned()
            } else {
                format!("evidence:{id}")
            };
            m.entry(id.to_owned()).or_insert_with(|| eref.clone());
            for prefix in ["evidence:recorded:", "evidence:"] {
                if let Some(rest) = id.strip_prefix(prefix) {
                    m.entry(rest.to_owned()).or_insert_with(|| eref.clone());
                }
            }
        }
    }
    m
}

// ---------------------------------------------------------------- residuals

#[derive(Debug, Clone)]
struct Res {
    expectation: String,
    class: &'static str,
    basis: &'static str,
    comparison: String,
    keys: Vec<String>,
    occ: BTreeSet<String>,
    evidence: BTreeSet<String>,
    rule_kind: String,
    rule_ref: String,
    support: u64,
    question: String,
    note: Option<String>,
    extra: Map<String, Value>,
}

/// A decided class and basis before the overlays.
#[derive(Debug, Clone)]
struct Verdict {
    class: &'static str,
    basis: &'static str,
    occ: BTreeSet<String>,
    support: u64,
    note: Option<String>,
}

fn verdict(
    class: &'static str,
    basis: &'static str,
    occ: BTreeSet<String>,
    support: u64,
) -> Verdict {
    Verdict {
        class,
        basis,
        occ,
        support,
        note: None,
    }
}

fn not_evaluable(note: impl Into<String>) -> Verdict {
    Verdict {
        note: Some(note.into()),
        ..verdict("insufficient", "not_evaluable", BTreeSet::new(), 0)
    }
}

/// Universal / existential / threshold decision over three-valued rows.
fn decide_rows(
    e: &ProfileEntry,
    trues: BTreeSet<String>,
    falses: BTreeSet<String>,
    unknowns: BTreeSet<String>,
    policy: bool,
) -> Verdict {
    let (t_basis, f_basis, u_basis) = if policy {
        ("policy_true", "policy_false", "policy_unknown")
    } else {
        (
            "structural_satisfied",
            "structural_violated",
            "not_evaluable",
        )
    };
    let support = trues.len() as u64;
    let total = trues.len() + falses.len() + unknowns.len();
    if e.quantifier == "existential" {
        if support >= e.min_support {
            return verdict("confirmed", t_basis, trues, support);
        }
        if !falses.is_empty() && unknowns.is_empty() {
            return verdict("contradicted", f_basis, falses, support);
        }
    } else if !falses.is_empty() {
        return verdict("contradicted", f_basis, falses, support);
    }
    if !unknowns.is_empty() {
        return verdict("insufficient", u_basis, unknowns, support);
    }
    if total == 0 && e.allow_vacuous_truth && e.quantifier == "universal" {
        // A confirmed residual must carry an occurrence (NT-45), so a vacuous
        // truth cannot be written as `confirmed`; it stays insufficient.
        let mut v = verdict("insufficient", "below_minimum_support", BTreeSet::new(), 0);
        v.note = Some("allow_vacuous_truth is set, but a confirmed residual needs an occurrence and evidence (residual.v1); recorded as below_minimum_support".into());
        return v;
    }
    if support < e.min_support {
        return verdict("insufficient", "below_minimum_support", trues, support);
    }
    verdict("confirmed", t_basis, trues, support)
}

struct Ctx<'a> {
    rep: &'a Rep<'a>,
    profile: &'a BTreeMap<String, ProfileEntry>,
    silent_sources: &'a BTreeSet<String>,
    coverage: &'a Value,
    eval_rows: &'a [Value],
    facts_occ: &'a BTreeMap<String, BTreeSet<String>>,
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| s.as_str().map(str::to_owned))
        .collect()
}

fn need<'a>(e: &'a Value, f: &str) -> Result<&'a str, ObservedError> {
    e[f].as_str().filter(|s| !s.is_empty()).ok_or_else(|| {
        err(
            "MAPPING_INVALID",
            format!("expectation {} lacks {f}", e["id"].as_str().unwrap_or("?")),
        )
    })
}

fn is_key(s: &str) -> bool {
    crate::ingest::is_declaration_key(s)
        || s.strip_prefix("undeclared:").is_some_and(|h| {
            h.len() == 64
                && h.bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        })
}

/// Decide one mapping expectation (before the silent-source and independence overlays).
fn decide(
    c: &Ctx,
    e: &Value,
    entry: &ProfileEntry,
) -> Result<(Vec<String>, String, Verdict), ObservedError> {
    let kind = need(e, "kind")?;
    match kind {
        "policy_evaluable" => {
            let policy = need(e, "policy")?;
            let op = need(e, "operation")?;
            let node = need(e, "node")?.to_owned();
            let occs: BTreeSet<String> = c.facts_occ.get(op).cloned().unwrap_or_default();
            let rows: Vec<&Value> = c
                .eval_rows
                .iter()
                .filter(|r| {
                    r["policy"] == json!(policy)
                        && r["operation"] == json!(op)
                        && r["operation_bound"] == json!(true)
                })
                .collect();
            let v = if occs.is_empty() {
                decide_rows(
                    entry,
                    BTreeSet::new(),
                    BTreeSet::new(),
                    BTreeSet::new(),
                    true,
                )
            } else if rows.is_empty() {
                not_evaluable(format!(
                    "evaluate returned no row for policy {policy} on operation {op}"
                ))
            } else {
                let (mut t, mut f, mut u) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
                for r in rows {
                    let id = r["input_set_id"].as_str().unwrap_or_default().to_owned();
                    match r["result"].as_str() {
                        Some("true") => t.insert(id),
                        Some("false") => f.insert(id),
                        _ => u.insert(id),
                    };
                }
                decide_rows(entry, t, f, u, true)
            };
            Ok((vec![node], policy.to_owned(), v))
        }
        "structural" => match need(e, "relation")? {
            "flow" => {
                let (from, to) = (need(e, "from")?.to_owned(), need(e, "to")?.to_owned());
                let mut keys = vec![from.clone(), to.clone()];
                keys.sort();
                let edge = |a: &str, b: &str| -> Option<(u64, BTreeSet<String>)> {
                    c.rep.doc["edges"]
                        .as_array()?
                        .iter()
                        .find(|x| x["from"] == json!(a) && x["to"] == json!(b))
                        .map(|x| {
                            (
                                x["frequency"].as_u64().unwrap_or(0),
                                strs(&x["occurrence_ids"]).into_iter().collect(),
                            )
                        })
                };
                let v = if !c.rep.nodes.contains_key(&from) || !c.rep.nodes.contains_key(&to) {
                    // Declared and not exercised is reported by the caller; an
                    // expectation whose element was never seen has no support.
                    decide_rows(
                        entry,
                        BTreeSet::new(),
                        BTreeSet::new(),
                        BTreeSet::new(),
                        false,
                    )
                } else if let Some((n, ids)) = edge(&from, &to) {
                    if n >= entry.min_support {
                        verdict("confirmed", "structural_satisfied", ids, n)
                    } else {
                        verdict("insufficient", "below_minimum_support", ids, n)
                    }
                } else if let Some((_, ids)) = edge(&to, &from) {
                    verdict("contradicted", "structural_violated", ids, 0)
                } else {
                    verdict("insufficient", "below_minimum_support", BTreeSet::new(), 0)
                };
                Ok((keys, need(e, "ref")?.to_owned(), v))
            }
            "precondition" => {
                let (node, prior) = (
                    need(e, "node")?.to_owned(),
                    need(e, "requires_prior")?.to_owned(),
                );
                let mut keys = vec![node.clone(), prior.clone()];
                keys.sort();
                let (mut ok, mut bad, mut undecidable) =
                    (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
                let priors = c.rep.node_occs(&prior);
                for o in c.rep.node_occs(&node) {
                    let same: Vec<&&Occ> = priors.iter().filter(|p| p.system == o.system).collect();
                    let caused = priors.iter().any(|p| {
                        o.causation.as_deref() == Some(p.record_id.as_str()) && p.system == o.system
                    });
                    if caused || same.iter().any(|p| p.sequence < o.sequence) {
                        ok.insert(o.id.clone());
                    } else if !same.is_empty() || priors.is_empty() {
                        // The owning system saw `prior` and none before this occurrence, or never saw it.
                        bad.insert(o.id.clone());
                    } else {
                        undecidable.insert(o.id.clone());
                    }
                }
                let v = if !bad.is_empty() {
                    verdict("contradicted", "structural_violated", bad, ok.len() as u64)
                } else if !undecidable.is_empty() {
                    let mut v = not_evaluable(
                        "the prior occurrence is reported by another system and sequences are never compared across systems",
                    );
                    v.occ = undecidable;
                    v
                } else {
                    decide_rows(entry, ok, BTreeSet::new(), BTreeSet::new(), false)
                };
                Ok((keys, need(e, "ref")?.to_owned(), v))
            }
            other => Err(err(
                "MAPPING_INVALID",
                format!("structural relation {other:?} is not flow or precondition"),
            )),
        },
        "absence" => {
            let trigger = need(e, "trigger")?.to_owned();
            let req = &e["required"];
            let (rnode, remitter, rkind) = (
                need(req, "node")?.to_owned(),
                need(req, "emitter")?.to_owned(),
                need(req, "event_kind")?.to_owned(),
            );
            let mut keys = vec![trigger.clone(), rnode.clone()];
            keys.sort();
            let triggers = c.rep.node_occs(&trigger);
            let v = if triggers.is_empty() {
                // Untriggered: not exercised, no class. The caller drops it.
                verdict("insufficient", "not_evaluable", BTreeSet::new(), 0)
            } else {
                let row =
                    c.coverage.as_array().into_iter().flatten().find(|r| {
                        r["emitter"] == json!(remitter) && r["event_kind"] == json!(rkind)
                    });
                match row.and_then(|r| r["status"].as_str()) {
                    None => not_evaluable(format!(
                        "({remitter}, {rkind}) is not in the frozen coverage manifest: absence there is outside the coverage window"
                    )),
                    Some("silent") => verdict(
                        "insufficient",
                        "evidence_source_silent",
                        triggers.iter().map(|o| o.id.clone()).collect(),
                        0,
                    ),
                    Some(_) => {
                        let reqs = c.rep.node_occs(&rnode);
                        let (mut ok, mut missing, mut undecidable) =
                            (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
                        for t in &triggers {
                            let same: Vec<&&Occ> =
                                reqs.iter().filter(|r| r.system == t.system).collect();
                            if same.iter().any(|r| {
                                r.sequence > t.sequence
                                    || r.causation.as_deref() == Some(t.record_id.as_str())
                            }) {
                                ok.insert(t.id.clone());
                            } else if !reqs.is_empty() && same.is_empty() {
                                undecidable.insert(t.id.clone());
                            } else {
                                missing.insert(t.id.clone());
                            }
                        }
                        if !missing.is_empty() {
                            verdict(
                                "declared_unobserved",
                                "trigger_observed_record_absent",
                                missing,
                                ok.len() as u64,
                            )
                        } else if !undecidable.is_empty() {
                            let mut v = not_evaluable(
                                "the required record is reported by another system than the trigger and sequences are never compared across systems",
                            );
                            v.occ = undecidable;
                            v
                        } else {
                            decide_rows(entry, ok, BTreeSet::new(), BTreeSet::new(), false)
                        }
                    }
                }
            };
            Ok((keys, need(e, "ref")?.to_owned(), v))
        }
        "ordering" | "frequency" | "timing" => {
            let keys: Vec<String> = strs(&e["keys"]);
            if keys.is_empty() || !keys.iter().all(|k| is_key(k)) {
                return Err(err(
                    "MAPPING_INVALID",
                    format!("expectation {} of kind {kind} needs keys", need(e, "id")?),
                ));
            }
            Ok((
                keys,
                need(e, "ref")?.to_owned(),
                not_evaluable(format!(
                    "comparison kind {kind} is not implemented in sxr-observed (WP-17 decides policy_evaluable, structural and absence)"
                )),
            ))
        }
        other => Err(err(
            "MAPPING_INVALID",
            format!("expectation kind {other:?} is unknown"),
        )),
    }
}

// ------------------------------------------------------------------ compare

fn digest_hex(v: &Value) -> Result<String, ObservedError> {
    Ok(canon::digest(v)?.trim_start_matches("sha256:").to_owned())
}

fn typed_value(spec: &Value, ext: &Value) -> Option<Value> {
    if let Some(t) = spec.get("typed") {
        return Some(t.clone());
    }
    let kind = spec["kind"].as_str()?;
    if let Some(p) = spec["nonempty"].as_str() {
        let present = match ext.pointer(p) {
            Some(Value::Array(a)) => !a.is_empty(),
            Some(Value::String(s)) => !s.is_empty(),
            Some(Value::Null) | None => false,
            Some(_) => true,
        };
        return (kind == "bool").then(|| json!({"kind": "bool", "data": present}));
    }
    let raw = match (spec.get("const"), spec["pointer"].as_str()) {
        (Some(c), _) => c.clone(),
        (None, Some(p)) => ext.pointer(p)?.clone(),
        _ => return None,
    };
    let ok = match kind {
        "string" => raw.is_string(),
        "int" => raw.is_i64(),
        "bool" => raw.is_boolean(),
        _ => false,
    };
    ok.then(|| json!({"kind": kind, "data": raw}))
}

fn world_name(world_ref: &str) -> &str {
    world_ref
        .strip_prefix("world:")
        .and_then(|r| r.split('@').next())
        .unwrap_or_default()
}

pub fn compare(inp: &CompareInput) -> Result<Compared, ObservedError> {
    // ---- the ledger, then everything derived from it
    let refs: Vec<&LedgerRow> = inp.rows.iter().collect();
    chain_verify(&refs).map_err(|m| err("CHAIN_VERIFY_FAILED", m))?;
    let tail_verification = tail::check(inp.rows, inp.expected_head.as_ref())?;

    // ---- the representation must be what derive printed, from this ledger
    let doc = &inp.observed["representation"];
    validate(
        REPRESENTATION_SCHEMA,
        doc,
        "REPRESENTATION_INVALID",
        "representation",
    )?;
    let rep_hash = canon::digest(doc)?;
    let rep_id = inp.observed["representation_id"]
        .as_str()
        .unwrap_or_default();
    if rep_id != format!("representation:{rep_hash}")
        || inp.observed["representation_hash"] != json!(rep_hash)
    {
        return Err(err(
            "REPRESENTATION_INVALID",
            "representation_id is not the hash of the representation: it was edited or is not derive output",
        ));
    }
    let input_set = doc["input_set_hash"].as_str().unwrap_or_default();
    if inp.observed["input_set_sha256"] != json!(input_set) {
        return Err(err(
            "REPRESENTATION_INVALID",
            "input_set_sha256 differs from the representation's input_set_hash",
        ));
    }
    let window = &inp.observed["window"];
    let (last_ord, head_hash) = (
        window["last_ordinal"].as_u64(),
        window["head_entry_hash"].as_str(),
    );
    let in_window: Vec<&LedgerRow> = inp
        .rows
        .iter()
        .filter(|r| Some(r.append_ordinal) <= last_ord)
        .collect();
    if in_window.last().map(|r| r.entry_hash.as_str()) != head_hash
        || in_window.len() as u64 != window["record_count"].as_u64().unwrap_or(u64::MAX)
    {
        return Err(err(
            "LEDGER_MISMATCH",
            "the representation's window does not end at a row of this ledger: it was derived from another ledger or the ledger was rewritten",
        ));
    }

    // ---- the frozen reference, the world and the pinned profile
    let body = &inp.reference["body"];
    let ep = inp.reference["episode_ref"].as_str().unwrap_or_default();
    if ep.rsplit_once("@sha256:").map(|x| x.1) != Some(digest_hex(body)?.as_str()) {
        return Err(err(
            "REFERENCE_DIGEST_MISMATCH",
            "episode_ref is not the digest of the reference body",
        ));
    }
    if doc["episode_ref"] != json!(ep) || doc["kind"] != body["kind"] {
        return Err(err(
            "REFERENCE_MISMATCH",
            "the representation was derived for another episode or kind than the frozen reference",
        ));
    }
    let world = inp.world_ref;
    if doc["world_ref"] != json!(world) || body["world_ref"] != json!(world) {
        return Err(err(
            "WORLD_REF_MISMATCH",
            format!(
                "--world {world} differs from the representation or the frozen reference: a classification stays with its world"
            ),
        ));
    }
    validate(PROFILE_SCHEMA, inp.profile, "PROFILE_INVALID", "profile")?;
    let pin = &body["comparison_profile"];
    let profile_hex = digest_hex(inp.profile)?;
    if pin["sha256"] != json!(profile_hex) {
        return Err(err(
            "PROFILE_DIGEST_MISMATCH",
            format!(
                "profile digest {profile_hex} differs from the digest {} pinned in the frozen reference",
                pin["sha256"]
            ),
        ));
    }
    let profile_id = inp.profile["profile_id"].as_str().unwrap_or_default();
    if pin["id"] != json!(profile_id) || doc["profile_id"] != json!(profile_id) {
        return Err(err(
            "PROFILE_MISMATCH",
            "the profile id differs from the pin in the frozen reference or from the representation's profile_id",
        ));
    }
    let entries = profile_entries(inp.profile)?;

    // ---- occurrences behind the representation
    let rep_occ_ids: BTreeSet<&str> = doc["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|n| n["occurrence_ids"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .collect();
    let mut occs: BTreeMap<String, Occ> = BTreeMap::new();
    for row in &in_window {
        if row.record_kind != "observation.recorded" {
            continue;
        }
        let Ok(p) = serde_json::from_str::<Value>(&row.payload) else {
            continue;
        };
        let ext = &p["extensions"][EXTENSION_KEY];
        let (system, rid) = (
            ext["source"]["system"].as_str().unwrap_or_default(),
            ext["source"]["record_id"].as_str().unwrap_or_default(),
        );
        let id = format!("{system}:{rid}");
        if rep_occ_ids.contains(id.as_str()) && !occs.contains_key(&id) {
            occs.insert(
                id.clone(),
                Occ {
                    system: system.to_owned(),
                    record_id: rid.to_owned(),
                    sequence: ext["source"]["sequence"].as_u64().unwrap_or(0),
                    causation: ext["causation_id"].as_str().map(str::to_owned),
                    ext: ext.clone(),
                    id,
                },
            );
        }
    }
    if let Some(missing) = rep_occ_ids.iter().find(|i| !occs.contains_key(**i)) {
        return Err(err(
            "LEDGER_MISMATCH",
            format!("occurrence {missing} of the representation is not in the ledger window"),
        ));
    }
    let rep = Rep {
        doc,
        nodes: doc["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|n| (n["node_id"].as_str().unwrap_or_default().to_owned(), n))
            .collect(),
        occs,
        evidence: evidence_index(in_window.iter().copied()),
    };

    // ---- frozen evidence sources that emitted nothing (NT-06)
    let emitted: BTreeSet<&str> = rep.occs.values().map(|o| o.system.as_str()).collect();
    let mut source_system: BTreeMap<String, String> = BTreeMap::new();
    let mut silent_sources: BTreeSet<String> = BTreeSet::new();
    for s in body["evidence_sources"].as_array().into_iter().flatten() {
        let (id, sys) = (
            s["source_id"].as_str().unwrap_or_default().to_owned(),
            s["system"].as_str().unwrap_or_default(),
        );
        if !emitted.contains(sys) {
            silent_sources.insert(id.clone());
        }
        source_system.insert(id, sys.to_owned());
    }

    // ---- the expectations
    let expectations: Vec<&Value> = inp.mapping["expectations"]
        .as_array()
        .into_iter()
        .flatten()
        .collect();
    if inp.mapping["contract"] != json!("compare-mapping.v1") || expectations.is_empty() {
        return Err(err(
            "MAPPING_INVALID",
            "the mapping is not compare-mapping.v1 or has no expectations",
        ));
    }
    let mut seen_ids = BTreeSet::new();
    for e in &expectations {
        let id = need(e, "id")?;
        if !seen_ids.insert(id) {
            return Err(err(
                "MAPPING_INVALID",
                format!("duplicate expectation id {id}"),
            ));
        }
        if !need(e, "question")?.starts_with("question:") {
            return Err(err(
                "MAPPING_INVALID",
                format!("expectation {id}: question is not question:<id>"),
            ));
        }
        for s in strs(&e["evidence_sources"]) {
            if !source_system.contains_key(&s) {
                return Err(err(
                    "MAPPING_INVALID",
                    format!(
                        "expectation {id} names evidence source {s}, which the frozen reference does not list"
                    ),
                ));
            }
        }
    }

    // ---- facts, then evaluate (only when a policy expectation has occurrences)
    let mut facts_occ: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut inputs: Vec<(String, String, Value)> = vec![];
    let ops: BTreeSet<&str> = expectations
        .iter()
        .filter(|e| e["kind"] == json!("policy_evaluable"))
        .filter_map(|e| e["operation"].as_str())
        .collect();
    for op in &ops {
        let spec = &inp.mapping["operations"][*op];
        let node = spec["node"]
            .as_str()
            .ok_or_else(|| err("MAPPING_INVALID", format!("operations.{op} lacks node")))?;
        for o in rep.node_occs(node) {
            let mut input = Map::new();
            for (field, s) in spec["input"].as_object().into_iter().flatten() {
                if let Some(v) = typed_value(s, &o.ext) {
                    input.insert(field.clone(), v);
                }
            }
            inputs.push(((*op).to_owned(), o.id.clone(), Value::Object(input)));
            facts_occ
                .entry((*op).to_owned())
                .or_default()
                .insert(o.id.clone());
        }
    }
    inputs.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    let mut facts_text = None;
    let mut eval_rows: Vec<Value> = vec![];
    let mut evaluate_doc = json!({"used": false});
    if !inputs.is_empty() {
        let facts = json!({
            "schema": "domainforge.evaluate-facts.v1",
            "operation_inputs": inputs.iter().map(|(op, id, input)| json!({"id": id, "operation": op, "input": input})).collect::<Vec<_>>(),
        });
        validate(
            EVALUATE_FACTS_SCHEMA,
            &facts,
            "EVALUATE_INPUT",
            "generated facts",
        )?;
        let text = canon::canonical(&facts)?;
        let facts_sha = sha256_hex(text.as_bytes());
        let ev = inp.evaluator.ok_or_else(|| err("EVALUATOR_REQUIRED", "policy-evaluable expectations have occurrences and no DomainForge binary and model were configured; they are never decided by anything else"))?;
        let out = ev.evaluate(&text, world_name(world))?;
        validate(
            EVALUATE_OUTPUT_SCHEMA,
            &out,
            "EVALUATE_OUTPUT_INVALID",
            "evaluate output",
        )?;
        if out["facts_sha256"] != json!(facts_sha) {
            return Err(err(
                "EVALUATE_OUTPUT_INVALID",
                "evaluate reports a different facts_sha256 than the generated file: it ran on other bytes",
            ));
        }
        if out["world_ref"] != json!(world) {
            return Err(err(
                "WORLD_REF_MISMATCH",
                format!(
                    "evaluate reports world_ref {} for the model, expected {world}",
                    out["world_ref"]
                ),
            ));
        }
        let id = ev.identity();
        evaluate_doc = json!({
            "used": true,
            "binary": id.binary, "version": id.version,
            "compiler": out["compiler"], "world_ref": out["world_ref"],
            "model_sha256": out["model_sha256"], "source_set_hash": out["source_set_hash"],
            "facts_sha256": facts_sha, "facts": facts,
        });
        eval_rows = out["policies"].as_array().cloned().unwrap_or_default();
        facts_text = Some(text);
    }

    // ---- decide
    let ctx = Ctx {
        rep: &rep,
        profile: &entries,
        silent_sources: &silent_sources,
        coverage: &doc["coverage"],
        eval_rows: &eval_rows,
        facts_occ: &facts_occ,
    };
    let mut results: Vec<Res> = vec![];
    let mut not_exercised: Vec<Value> = vec![];
    let entry_for = |kind: &str| -> Result<&ProfileEntry, ObservedError> {
        ctx.profile.get(kind).ok_or_else(|| {
            err(
                "PROFILE_KIND_MISSING",
                format!("the profile has no entry for comparison kind {kind}"),
            )
        })
    };
    for e in &expectations {
        let kind = need(e, "kind")?;
        let entry = entry_for(kind)?;
        let (keys, rule_ref, mut v) = decide(&ctx, e, entry)?;
        if kind == "absence" && rep.node_occs(need(e, "trigger")?).is_empty() {
            not_exercised.push(json!({"expectation": e["id"], "keys": keys, "reason": "trigger not observed in the coverage window; an untriggered obligation has no class"}));
            continue;
        }
        let dep: Vec<String> = strs(&e["evidence_sources"])
            .into_iter()
            .filter(|s| ctx.silent_sources.contains(s))
            .collect();
        let mut extra = Map::new();
        if !dep.is_empty() && (v.class == "confirmed" || v.class == "insufficient") {
            extra.insert("silent_sources".into(), json!(dep));
            v = Verdict {
                note: Some(format!(
                    "frozen evidence source(s) {} emitted nothing",
                    dep.join(", ")
                )),
                ..verdict("insufficient", "evidence_source_silent", v.occ, v.support)
            };
        }
        if v.class == "confirmed" && rep.independent(&keys) == 0 {
            v = Verdict {
                note: Some("no occurrence behind this element comes from an emitter independent of the executor".into()),
                ..verdict("insufficient", "executor_only_evidence", v.occ, v.support)
            };
        }
        results.push(Res {
            expectation: e["id"].as_str().unwrap_or_default().to_owned(),
            class: v.class,
            basis: v.basis,
            comparison: entry.comparison.clone(),
            keys,
            evidence: rep.evidence_for(&v.occ),
            occ: v.occ,
            rule_kind: kind.to_owned(),
            rule_ref,
            support: v.support,
            question: need(e, "question")?.to_owned(),
            note: v.note,
            extra,
        });
    }

    // ---- undeclared behaviour (never added to the model)
    let st = entry_for("structural")?;
    let default_q = inp.mapping["undeclared_question"]
        .as_str()
        .filter(|q| q.starts_with("question:"))
        .ok_or_else(|| {
            err(
                "MAPPING_INVALID",
                "undeclared_question (question:<id>) is required",
            )
        })?;
    for (id, n) in &rep.nodes {
        if id.starts_with("undeclared:") {
            let occ: BTreeSet<String> = strs(&n["occurrence_ids"]).into_iter().collect();
            results.push(Res {
                expectation: format!("undeclared:{id}"),
                class: "observed_undeclared",
                basis: "structural_undeclared",
                comparison: st.comparison.clone(),
                keys: vec![id.clone()],
                evidence: rep.evidence_for(&occ),
                support: occ.len() as u64,
                occ,
                rule_kind: "structural".into(),
                rule_ref: "declared_model_closure".into(),
                question: default_q.to_owned(),
                note: None,
                extra: Map::new(),
            });
        }
    }
    let flows: BTreeSet<(String, String)> = expectations
        .iter()
        .filter(|e| e["kind"] == json!("structural") && e["relation"] == json!("flow"))
        .filter_map(|e| Some((e["from"].as_str()?.to_owned(), e["to"].as_str()?.to_owned())))
        .collect();
    let flow_nodes: BTreeSet<&String> = flows.iter().flat_map(|(a, b)| [a, b]).collect();
    for edge in doc["edges"].as_array().into_iter().flatten() {
        let (a, b) = (
            edge["from"].as_str().unwrap_or_default().to_owned(),
            edge["to"].as_str().unwrap_or_default().to_owned(),
        );
        if flow_nodes.contains(&a)
            && flow_nodes.contains(&b)
            && !flows.contains(&(a.clone(), b.clone()))
            && !flows.contains(&(b.clone(), a.clone()))
        {
            let occ: BTreeSet<String> = strs(&edge["occurrence_ids"]).into_iter().collect();
            let mut keys = vec![a.clone(), b.clone()];
            keys.sort();
            results.push(Res {
                expectation: format!("undeclared-edge:{a}->{b}"),
                class: "observed_undeclared",
                basis: "structural_undeclared",
                comparison: st.comparison.clone(),
                keys,
                evidence: rep.evidence_for(&occ),
                support: edge["frequency"].as_u64().unwrap_or(0),
                occ,
                rule_kind: "structural".into(),
                rule_ref: "declared_flows".into(),
                question: default_q.to_owned(),
                note: None,
                extra: Map::new(),
            });
        }
    }
    for k in strs(&inp.mapping["declared_keys"]) {
        if !rep.nodes.contains_key(&k) {
            not_exercised.push(
                json!({"keys": [k], "reason": "declared element not observed in the episode"}),
            );
        }
    }

    // ---- write the residuals
    let mut annotations = Map::new();
    let mut lists: BTreeMap<&str, Vec<(String, String, Value)>> =
        BTreeMap::from([("episode", vec![]), ("structural", vec![])]);
    for r in results {
        let mut keys = r.keys.clone();
        keys.sort();
        keys.dedup();
        let rid = format!(
            "residual:{}",
            digest_hex(&json!({
                "representation_id": rep_id, "expectation": r.expectation,
                "rule": {"kind": r.rule_kind, "ref": r.rule_ref}, "keys": keys, "question": r.question,
            }))?
        );
        let residual = json!({
            "contract": "residual.v1",
            "residual_id": rid, "class": r.class, "basis": r.basis, "comparison": r.comparison,
            "keys": keys, "occurrence_ids": r.occ, "evidence_refs": r.evidence,
            "rule": {"kind": r.rule_kind, "ref": r.rule_ref},
            "support_count": r.support, "representation_id": rep_id, "input_set_sha256": input_set,
            "profile_id": profile_id, "question": r.question, "snapshot": inp.snapshot, "world_ref": world,
        });
        validate(
            RESIDUAL_SCHEMA,
            &residual,
            "RESIDUAL_BASIS_MISSING",
            &format!("residual for {}", r.expectation),
        )?;
        let mut ann = json!({"expectation": r.expectation, "context": rep.context(&keys)});
        if let Some(n) = r.note {
            ann["note"] = json!(n);
        }
        if evaluate_doc["used"] == json!(true) && r.rule_kind == "policy_evaluable" {
            ann["facts_sha256"] = evaluate_doc["facts_sha256"].clone();
            ann["evaluate_world_ref"] = evaluate_doc["world_ref"].clone();
        }
        for (k, v) in r.extra {
            ann[k] = v;
        }
        annotations.insert(rid.clone(), ann);
        lists
            .get_mut(r.comparison.as_str())
            .expect("comparison is episode or structural")
            .push((r.rule_kind, rid, residual));
    }
    let mut out_lists = Map::new();
    for (name, mut l) in lists {
        l.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
        out_lists.insert(
            name.into(),
            Value::Array(l.into_iter().map(|x| x.2).collect()),
        );
    }
    not_exercised.sort_by_key(|n| n.to_string());

    // ---- the episode result (a proposal: settlement belongs to the case keeper)
    let episode_class = |c: &str| {
        out_lists["episode"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|r| r["class"] == json!(c))
    };
    let exploratory = body["kind"] == json!("exploratory");
    let (result, basis): (Value, &str) = if exploratory {
        (Value::Null, "exploratory episodes are not settleable")
    } else if episode_class("contradicted") {
        (json!("F"), "an episode expectation is contradicted")
    } else if episode_class("insufficient") || !silent_sources.is_empty() {
        (
            json!("U"),
            "an expectation is insufficient or a frozen evidence source emitted nothing",
        )
    } else if episode_class("confirmed") {
        (json!("S"), "every episode expectation is confirmed")
    } else {
        (json!("U"), "no episode expectation was decided")
    };
    let model_sha = evaluate_doc["model_sha256"].clone();
    let comparison_id = format!(
        "comparison:{}",
        digest_hex(&json!({
            "representation_id": rep_id, "episode_ref": ep, "profile_sha256": profile_hex,
            "mapping_sha256": digest_hex(inp.mapping)?, "snapshot": inp.snapshot,
            "facts_sha256": evaluate_doc["facts_sha256"], "model_sha256": model_sha,
        }))?
    );
    let document = json!({
        "comparison_id": comparison_id,
        "episode_ref": ep,
        "kind": body["kind"],
        "world_ref": world,
        "snapshot": inp.snapshot,
        "representation_id": rep_id,
        "input_set_sha256": input_set,
        "profile": {"id": profile_id, "sha256": format!("sha256:{profile_hex}")},
        "mapping_sha256": format!("sha256:{}", digest_hex(inp.mapping)?),
        "tail_verification": tail_verification,
        "settleable": !exploratory,
        "flags": if exploratory { json!(["EXPLORATORY_NOT_SETTLEABLE"]) } else { json!([]) },
        "proposed_result": {"result": result, "basis": basis},
        "silent_evidence_sources": silent_sources,
        "residuals": out_lists,
        "not_exercised": not_exercised,
        "annotations": annotations,
        "evaluate": evaluate_doc,
    });
    let canonical = canon::canonical(&document)?;
    Ok(Compared {
        document,
        canonical,
        facts: facts_text,
    })
}
