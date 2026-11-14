//! `observed derive`: the observed representation of
//! `contracts/observed-representation.v1.json`, computed from a verified ledger.
//!
//! Verification comes first: the whole ledger chain is checked and any
//! failure is `CHAIN_VERIFY_FAILED`. The result depends only on the rows and
//! the options; nothing is cached, and in particular nothing is keyed by case id.

use crate::ObservedError;
use crate::canon;
use crate::ingest::{
    EXTENSION_KEY, EventValidator, EvidenceIndex, Hints, INGEST_NOTE_KEY, REJECTION_NOTE_KEY,
};
use crate::tail::{self, ExpectedHead};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use sxr_core::LedgerRow;
use sxr_core::canonical_json::sha256_hex;
use sxr_core::chain::chain_verify;

/// Where the derive window ends (inclusive).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Until {
    Head,
    Ordinal(u64),
    Entry(String),
    Timestamp(String),
}

impl Until {
    /// `head`, an append ordinal, an entry ULID, or an RFC 3339 timestamp
    /// (compared with the ledger's own `committed_at`, never emitter clocks).
    pub fn parse(s: &str) -> Result<Self, ObservedError> {
        if s == "head" {
            Ok(Self::Head)
        } else if let Ok(n) = s.parse::<u64>() {
            Ok(Self::Ordinal(n))
        } else if chrono::DateTime::parse_from_rfc3339(s).is_ok() {
            Ok(Self::Timestamp(s.to_owned()))
        } else if s.contains(['-', ':']) {
            Err(ObservedError::new(
                "BAD_SELECTOR",
                format!("--until {s:?} is not an RFC 3339 timestamp"),
            ))
        } else if !s.is_empty() {
            Ok(Self::Entry(s.to_owned()))
        } else {
            Err(ObservedError::new("BAD_SELECTOR", "--until is empty"))
        }
    }

    fn label(&self) -> String {
        match self {
            Self::Head => "head".into(),
            Self::Ordinal(n) => format!("ordinal:{n}"),
            Self::Entry(e) => format!("entry:{e}"),
            Self::Timestamp(t) => format!("timestamp:{t}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Options {
    pub world_ref: String,
    /// `case:<id>`.
    pub scope: String,
    pub episode_ref: String,
    /// `evaluated` or `exploratory`.
    pub kind: String,
    pub profile_id: String,
    pub until: Until,
    /// Declaration keys of the pinned world. An observed key outside the set
    /// becomes an `undeclared:` node. `None`: every observed key is declared.
    pub declared_keys: Option<BTreeSet<String>>,
    /// Frozen coverage manifest as `(emitter, event_kind)`; `event_kind` is the
    /// event `operation`. `None`: rows are the observed pairs.
    pub coverage_manifest: Option<Vec<(String, String)>>,
    /// Source systems known independent of the executor (E section 5). Empty:
    /// no occurrence counts as independent.
    pub independent_systems: BTreeSet<String>,
    /// A ledger head recorded outside the ledger (anchor, frozen reference).
    /// `None`: the tail is reported `unverified`.
    pub expected_head: Option<ExpectedHead>,
}

#[derive(Debug, Clone)]
pub struct Derived {
    /// The representation (validates against the contract schema).
    pub representation: Value,
    /// `representation` plus `representation_id`, `input_set_sha256`, window.
    pub document: Value,
    /// Canonical bytes of `document`: what the CLI prints.
    pub canonical: String,
    pub representation_id: String,
}

struct Occ {
    id: String,
    node_id: String,
    semantic_ref: Option<String>,
    system: String,
    record_id: String,
    operation: String,
    sequence: u64,
    causation: Option<String>,
    role: String,
    plan_item: String,
    authority: &'static str,
    outcome: String,
    independent: bool,
    entry_hash: String,
}

type Counts = BTreeMap<String, u64>;

#[derive(Default)]
struct Dist {
    roles: Counts,
    plan_items: Counts,
    authority: [u64; 3],
    outcome: BTreeMap<String, u64>,
}

impl Dist {
    fn add(&mut self, o: &Occ) {
        *self.roles.entry(o.role.clone()).or_default() += 1;
        *self.plan_items.entry(o.plan_item.clone()).or_default() += 1;
        let i = match o.authority {
            "decided" => 0,
            "not_required" => 1,
            _ => 2,
        };
        self.authority[i] += 1;
        *self.outcome.entry(o.outcome.clone()).or_default() += 1;
    }

    fn context(&self) -> Value {
        json!({"roles": self.roles, "plan_items": self.plan_items})
    }

    fn authority_json(&self) -> Value {
        json!({"decided": self.authority[0], "not_required": self.authority[1], "unknown": self.authority[2]})
    }

    fn outcome_json(&self) -> Value {
        let g = |k: &str| self.outcome.get(k).copied().unwrap_or(0);
        json!({"succeeded": g("succeeded"), "failed": g("failed"), "denied": g("denied"),
               "interrupted": g("interrupted"), "unknown": g("unknown")})
    }
}

fn parse_payload(row: &LedgerRow) -> Option<Value> {
    serde_json::from_str(&row.payload).ok()
}

fn hints_match(h: &Hints, o: &Options, case_id: &str) -> bool {
    h.world_ref.as_deref().is_none_or(|w| w == o.world_ref)
        && h.case_id.as_deref().is_none_or(|c| c == case_id)
        && h.episode_ref.as_deref().is_none_or(|e| e == o.episode_ref)
        && h.kind.as_deref().is_none_or(|k| k == o.kind)
}

fn window_len(rows: &[LedgerRow], until: &Until) -> Result<usize, ObservedError> {
    match until {
        Until::Head => Ok(rows.len()),
        Until::Ordinal(n) => Ok(rows.iter().filter(|r| r.append_ordinal <= *n).count()),
        Until::Entry(e) => rows
            .iter()
            .position(|r| &r.entry_ulid == e)
            .map(|i| i + 1)
            .ok_or_else(|| {
                ObservedError::new(
                    "BAD_SELECTOR",
                    format!("--until record {e:?} is not in the ledger"),
                )
            }),
        Until::Timestamp(t) => {
            let bound = chrono::DateTime::parse_from_rfc3339(t).expect("validated in Until::parse");
            Ok(rows
                .iter()
                .take_while(|r| {
                    chrono::DateTime::parse_from_rfc3339(&r.committed_at).is_ok_and(|c| c <= bound)
                })
                .count())
        }
    }
}

/// Derive the observed representation. Verifies the chain first.
pub fn derive(rows: &[LedgerRow], opts: &Options) -> Result<Derived, ObservedError> {
    let refs: Vec<&LedgerRow> = rows.iter().collect();
    chain_verify(&refs).map_err(|m| ObservedError::new("CHAIN_VERIFY_FAILED", m))?;

    let case_id = opts
        .scope
        .strip_prefix("case:")
        .filter(|c| !c.is_empty())
        .ok_or_else(|| {
            ObservedError::new(
                "BAD_SELECTOR",
                format!("--scope {:?} is not case:<id>", opts.scope),
            )
        })?;
    if opts.kind != "evaluated" && opts.kind != "exploratory" {
        return Err(ObservedError::new(
            "BAD_SELECTOR",
            "--kind is evaluated or exploratory",
        ));
    }
    let tail_verification = tail::check(rows, opts.expected_head.as_ref())?;
    let window = &rows[..window_len(rows, &opts.until)?];
    let evidence = EvidenceIndex::from_rows(window);
    let validator = EventValidator::new();

    let mut occs: Vec<Occ> = Vec::new();
    let mut seen_ids = BTreeSet::new();
    let (mut rejected, mut unverified): (Vec<&str>, Vec<&str>) = (vec![], vec![]);
    for row in window {
        let Some(payload) = parse_payload(row) else {
            continue;
        };
        match row.record_kind.as_str() {
            "ingest.rejected" => {
                let note = &payload["extensions"][REJECTION_NOTE_KEY];
                if note.is_object() && hints_match(&Hints::from_json(&note["hints"]), opts, case_id)
                {
                    rejected.push(&row.entry_hash);
                }
            }
            "observation.recorded" => {
                let ext = &payload["extensions"][EXTENSION_KEY];
                if !ext.is_object() {
                    continue;
                }
                let hints = Hints::from_event(ext);
                if !hints_match(&hints, opts, case_id) {
                    continue;
                }
                // Re-verify what ingest verified: schema, and source resolution.
                let record_id = ext["source"]["record_id"].as_str().unwrap_or_default();
                let note = &payload["extensions"][INGEST_NOTE_KEY];
                if !validator.errors(ext).is_empty()
                    || !evidence.resolves(record_id)
                    || note["accepted"] != json!(true)
                {
                    unverified.push(&row.entry_hash);
                    continue;
                }
                if hints.world_ref.as_deref() != Some(&opts.world_ref)
                    || hints.case_id.as_deref() != Some(case_id)
                    || hints.episode_ref.as_deref() != Some(&opts.episode_ref)
                    || hints.kind.as_deref() != Some(&opts.kind)
                {
                    continue;
                }
                let system = ext["source"]["system"].as_str().unwrap_or_default();
                let id = format!("{system}:{record_id}");
                if !seen_ids.insert(id.clone()) {
                    continue;
                }
                let key = ext["declaration_key"].as_str().unwrap_or_default();
                let declared = opts.declared_keys.as_ref().is_none_or(|d| d.contains(key));
                let (node_id, semantic_ref) = if declared {
                    (
                        key.to_owned(),
                        ext["semantic_ref"].as_str().map(str::to_owned),
                    )
                } else {
                    let basis = format!(
                        "{}\n{}",
                        ext["operation"].as_str().unwrap_or_default(),
                        ext["resource"].as_str().unwrap_or_default()
                    );
                    (
                        format!("undeclared:{}", hex_of(&sha256_hex(basis.as_bytes()))),
                        None,
                    )
                };
                let dec = ext["authority"]["decision_id"]
                    .as_str()
                    .unwrap_or("unknown");
                occs.push(Occ {
                    id,
                    node_id,
                    semantic_ref,
                    system: system.to_owned(),
                    record_id: record_id.to_owned(),
                    operation: ext["operation"].as_str().unwrap_or_default().to_owned(),
                    sequence: ext["source"]["sequence"].as_u64().unwrap_or(0),
                    causation: ext["causation_id"].as_str().map(str::to_owned),
                    role: ext["actor"]["role"].as_str().unwrap_or_default().to_owned(),
                    plan_item: ext["context"]["plan_item_id"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                    authority: match dec {
                        "not_required" => "not_required",
                        "unknown" => "unknown",
                        _ => "decided",
                    },
                    outcome: ext["outcome"]["status"]
                        .as_str()
                        .unwrap_or("unknown")
                        .to_owned(),
                    independent: opts.independent_systems.contains(system),
                    entry_hash: row.entry_hash.clone(),
                });
            }
            _ => {}
        }
    }
    occs.sort_by(|a, b| a.id.cmp(&b.id));

    let nodes = build_nodes(&occs);
    let edges = build_edges(&occs);

    let coverage_rows: Vec<(String, String)> = match &opts.coverage_manifest {
        Some(m) => m
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        None => occs
            .iter()
            .map(|o| (o.system.clone(), o.operation.clone()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
    };
    if coverage_rows.is_empty() {
        return Err(ObservedError::new(
            "COVERAGE_UNDEFINED",
            "no occurrences and no coverage manifest: coverage has no rows",
        ));
    }
    let coverage: Vec<Value> = coverage_rows
        .iter()
        .map(|(emitter, kind)| {
            let n = occs
                .iter()
                .filter(|o| &o.system == emitter && &o.operation == kind)
                .count() as u64;
            json!({"emitter": emitter, "event_kind": kind, "observed_count": n,
                   "status": if n > 0 { "covered" } else { "silent" }})
        })
        .collect();

    let head = window.last();
    let coverage_rows_for_hash: Option<&Vec<(String, String)>> =
        opts.coverage_manifest.as_ref().map(|_| &coverage_rows);
    let mut inputs: Vec<&str> = occs.iter().map(|o| o.entry_hash.as_str()).collect();
    inputs.sort_unstable();
    rejected.sort_unstable();
    unverified.sort_unstable();
    let window_doc = json!({
        "until": opts.until.label(),
        "record_count": window.len(),
        "first_ordinal": window.first().map(|r| r.append_ordinal),
        "last_ordinal": head.map(|r| r.append_ordinal),
        "head_entry_hash": head.map(|r| r.entry_hash.as_str()),
        "tail_verification": tail_verification,
    });
    let input_set_hash = canon::digest(&json!({
        "contract": "observed-representation.v1",
        "selectors": {"world_ref": opts.world_ref, "scope": opts.scope, "episode_ref": opts.episode_ref,
                      "kind": opts.kind, "profile_id": opts.profile_id},
        "options": {"declared_keys": opts.declared_keys, "coverage_manifest": coverage_rows_for_hash,
                    "independent_systems": opts.independent_systems},
        "window": window_doc,
        "inputs": inputs, "rejected": rejected, "unverified": unverified,
    }))?;

    let representation = json!({
        "contract": "observed-representation.v1",
        "world_ref": opts.world_ref,
        "episode_ref": opts.episode_ref,
        "kind": opts.kind,
        "profile_id": opts.profile_id,
        "input_set_hash": input_set_hash,
        "nodes": nodes,
        "edges": edges,
        "coverage": coverage,
        "uncertainty": {"rejected_event_count": rejected.len(), "unverified_event_count": unverified.len()},
    });
    let representation_hash = canon::digest(&representation)?;
    let representation_id = format!("representation:{representation_hash}");
    let mut doc = Map::new();
    doc.insert("representation_id".into(), json!(representation_id));
    doc.insert("representation_hash".into(), json!(representation_hash));
    doc.insert("input_set_sha256".into(), json!(input_set_hash));
    doc.insert("window".into(), window_doc);
    doc.insert("representation".into(), representation.clone());
    let document = Value::Object(doc);
    let canonical = canon::canonical(&document)?;
    Ok(Derived {
        representation,
        document,
        canonical,
        representation_id,
    })
}

fn hex_of(prefixed: &str) -> &str {
    prefixed.strip_prefix("sha256:").unwrap_or(prefixed)
}

fn build_nodes(occs: &[Occ]) -> Vec<Value> {
    let mut by_node: BTreeMap<&str, Vec<&Occ>> = BTreeMap::new();
    for o in occs {
        by_node.entry(&o.node_id).or_default().push(o);
    }
    by_node
        .into_iter()
        .map(|(node_id, group)| {
            let mut dist = Dist::default();
            let mut ordering: BTreeMap<&str, (u64, u64)> = BTreeMap::new();
            for o in &group {
                dist.add(o);
                let e = ordering.entry(&o.system).or_insert((o.sequence, o.sequence));
                e.0 = e.0.min(o.sequence);
                e.1 = e.1.max(o.sequence);
            }
            let mut node = json!({
                "node_id": node_id,
                "occurrence_ids": group.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
                "frequency": group.len(),
                "independent_occurrence_count": group.iter().filter(|o| o.independent).count(),
                "ordering": ordering.into_iter().map(|(s, (a, b))|
                    json!({"system": s, "first_sequence": a, "last_sequence": b})).collect::<Vec<_>>(),
                "context_distribution": dist.context(),
                "authority_outcomes": dist.authority_json(),
                "outcome_status": dist.outcome_json(),
            });
            if let Some(sr) = &group[0].semantic_ref {
                node["semantic_ref"] = json!(sr);
            }
            node
        })
        .collect()
}

fn build_edges(occs: &[Occ]) -> Vec<Value> {
    // Links by owning-system order (adjacent) and by causation.
    // `true` marks a causation link; a pair found both ways is a causation link.
    let mut links: BTreeMap<(usize, usize), bool> = BTreeMap::new();
    let mut by_system: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, o) in occs.iter().enumerate() {
        by_system.entry(&o.system).or_default().push(i);
    }
    for idxs in by_system.values_mut() {
        idxs.sort_by(|&a, &b| {
            (occs[a].sequence, &occs[a].record_id).cmp(&(occs[b].sequence, &occs[b].record_id))
        });
        for w in idxs.windows(2) {
            links.insert((w[0], w[1]), false);
        }
    }
    for (b, o) in occs.iter().enumerate() {
        let Some(cause) = &o.causation else { continue };
        if let Some(a) = occs
            .iter()
            .position(|x| x.system == o.system && &x.record_id == cause)
            && a != b
        {
            links.insert((a, b), true);
        }
    }
    struct Edge<'a> {
        ids: BTreeSet<&'a str>,
        freq: u64,
        via_seq: u64,
        via_cause: u64,
        dist: Dist,
    }
    let mut edges: BTreeMap<(&str, &str), Edge> = BTreeMap::new();
    for ((a, b), causal) in links {
        let e = edges
            .entry((&occs[a].node_id, &occs[b].node_id))
            .or_insert_with(|| Edge {
                ids: BTreeSet::new(),
                freq: 0,
                via_seq: 0,
                via_cause: 0,
                dist: Dist::default(),
            });
        e.ids.insert(&occs[a].id);
        e.ids.insert(&occs[b].id);
        e.freq += 1;
        if causal {
            e.via_cause += 1;
        } else {
            e.via_seq += 1;
        }
        e.dist.add(&occs[b]);
    }
    edges
        .into_iter()
        .map(|((from, to), e)| {
            json!({
                "from": from, "to": to,
                "occurrence_ids": e.ids.into_iter().collect::<Vec<_>>(),
                "frequency": e.freq,
                "ordering": {"via_sequence": e.via_seq, "via_causation": e.via_cause},
                "context_distribution": e.dist.context(),
                "authority_outcomes": e.dist.authority_json(),
                "outcome_status": e.dist.outcome_json(),
            })
        })
        .collect()
}
