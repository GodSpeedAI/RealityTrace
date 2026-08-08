//! Slice-3 declared-graph and retrieval types (§12.3, §12.2, §17).
//!
//! `G_declared` nodes and edges are derived solely from `semantic.snapshot`
//! records (§12.3). The types here are pure data — no I/O, no SQLite — so they
//! cross the `SemanticIndexPort` boundary (§7.1) without coupling `sxr-core`
//! to a substrate (AC-CORE-001).

use serde::{Deserialize, Serialize};

/// Slice-3 new type (§24 Slice 3): one declared node in `G_declared`.
///
/// `decl_key` is the stable declaration key; `namespace` and
/// `logical_module_id` are resolved from the snapshot's `namespace_bindings`
/// and each declaration's module. Nodes are scoped by `(snapshot_id, decl_key)`
/// which, combined with the repo the snapshot belongs to, satisfies
/// AC-IDENTITY-002 (a `default::Warehouse` in two repos stays distinct).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclNode {
    pub snapshot_id: String,
    pub decl_key: String,
    pub decl_kind: String,
    pub name: String,
    pub namespace: String,
    pub logical_module_id: String,
}

/// Slice-3 new type (§24 Slice 3): one declared edge in `G_declared`.
///
/// Edge sources (§12.3): `resolved_references` (decl → decl, with `field_path`,
/// `edge_kind == "reference"`), declaration-internal Flow references
/// (`resource`/`from_entity`/`to_entity`), `import_graph` (module → module),
/// and `namespace_bindings`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclEdge {
    pub snapshot_id: String,
    pub src_decl_key: String,
    pub dst_decl_key: String,
    pub edge_kind: String,
    pub field_path: String,
}

/// CEP-0003 §27.2 closed set of reasons a declaration is `not_observed`
/// (§12.4). Never a boolean; `not_observed` always carries a reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotObservedReason {
    NotCollected,
    OutOfScope,
    Redacted,
    Inaccessible,
    Ambiguous,
    IdentityUncertain,
    Unavailable,
}

impl NotObservedReason {
    pub fn as_str(self) -> &'static str {
        match self {
            NotObservedReason::NotCollected => "not_collected",
            NotObservedReason::OutOfScope => "out_of_scope",
            NotObservedReason::Redacted => "redacted",
            NotObservedReason::Inaccessible => "inaccessible",
            NotObservedReason::Ambiguous => "ambiguous",
            NotObservedReason::IdentityUncertain => "identity_uncertain",
            NotObservedReason::Unavailable => "unavailable",
        }
    }
}

impl std::str::FromStr for NotObservedReason {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "not_collected" => Ok(NotObservedReason::NotCollected),
            "out_of_scope" => Ok(NotObservedReason::OutOfScope),
            "redacted" => Ok(NotObservedReason::Redacted),
            "inaccessible" => Ok(NotObservedReason::Inaccessible),
            "ambiguous" => Ok(NotObservedReason::Ambiguous),
            "identity_uncertain" => Ok(NotObservedReason::IdentityUncertain),
            "unavailable" => Ok(NotObservedReason::Unavailable),
            _ => Err(()),
        }
    }
}

/// Three-valued presence returned by the query API (§12.4): a declaration is
/// `observed`, `not_observed` (with a CEP-0003 §27.2 reason), or `unknown`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Presence {
    Observed,
    NotObserved { reason: String },
    Unknown,
}

/// Slice-3 new type (§24 Slice 3): the compact failure context pack returned
/// by `sxr context --failure <run_id> --json` (§17.1/§17.2). Composes the
/// structured-SQL, FTS5, and graph retrievers; the vector retriever is a null
/// adapter in Slice 3 (§12.6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextPack {
    pub run_id: String,
    pub repo_id: String,
    pub question: Option<QuestionSummary>,
    pub threshold: Option<ThresholdSummary>,
    pub failure: Option<FailureSummary>,
    pub declared_neighbourhood: Vec<NeighbourNode>,
    pub prior_settlements: Vec<PriorSettlement>,
    pub fts_matches: Vec<FtsMatch>,
    pub vector_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionSummary {
    pub question_id: String,
    pub question_form: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThresholdSummary {
    pub threshold_id: String,
    pub spec_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailureSummary {
    pub step_id: String,
    pub verifier_id: String,
    pub exit_code: Option<i64>,
}

/// A 1- or 2-hop neighbour of a failing declaration in `G_declared`,
/// annotated with its observed status (§17.2 `declared_neighbourhood`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NeighbourNode {
    pub decl_key: String,
    pub name: String,
    pub kind: String,
    pub namespace: String,
    pub hop: u32,
    pub edge_kind: String,
    pub observed: Presence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriorSettlement {
    pub settlement_id: String,
    pub settlement_status: String,
    pub question_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FtsMatch {
    pub subject_key: String,
    pub kind: String,
    pub title: String,
    pub snippet: String,
}
