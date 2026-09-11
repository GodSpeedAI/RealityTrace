use serde::{Deserialize, Serialize};

pub const SXROUTPUT_VERSION: &str = "1";
pub const SXR_RECORD_VERSION: &str = "sxr-record/v1";
pub const SXR_CANONICALIZATION: &str = "sxr-canonical-json/v1";
pub const SXR_HASH_ALGORITHM: &str = "sha256";
pub const CEP_VERSION: &str = "0.1.0";
pub const ENVELOPE_VERSION: &str = "1.0.0";

/// A ULID minted at enrollment. Never derived from any Git property (§9.1).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RepoIdentity {
    pub repo_id: String,
    pub checkout_id: String,
}

/// `{ ledger_id, append_ordinal }` (§10.3). Positions are comparable only when
/// their generations lie on the same LedgerHistory predecessor chain; that
/// check lives in the ledger layer, not here.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LedgerPosition {
    pub ledger_id: String,
    pub append_ordinal: u64,
}

impl LedgerPosition {
    pub fn new(ledger_id: impl Into<String>, append_ordinal: u64) -> Self {
        Self {
            ledger_id: ledger_id.into(),
            append_ordinal,
        }
    }
}

/// Stable declaration key, the spine of the system (§12.1). Deterministic,
/// path-independent, reproducible.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DeclKey {
    Concept(String),
    Application(String),
    Occurrence {
        kind: String,
        content_hash: String,
        duplicate_index: u64,
    },
}

impl DeclKey {
    pub fn as_str(&self) -> String {
        match self {
            DeclKey::Concept(id) => format!("concept:{id}"),
            DeclKey::Application(sym) => format!("application:{sym}"),
            DeclKey::Occurrence {
                kind,
                content_hash,
                duplicate_index,
            } => {
                format!("occurrence:{kind}:{content_hash}:{duplicate_index}")
            }
        }
    }
}

impl std::fmt::Display for DeclKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.as_str())
    }
}

/// The frozen ledger row shape (§10.3). Field names mirror
/// `sea-forge-ledger::LedgerEntry` so a future export is a projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerRow {
    pub ledger_id: String,
    pub append_ordinal: u64,
    pub entry_ulid: String,
    pub record_kind: String,
    pub record_version: String,
    pub idempotency_key: Option<String>,
    pub subject_refs: Vec<String>,
    pub canonicalization: String,
    pub hash_algorithm: String,
    pub payload: String,
    pub payload_hash: String,
    pub previous_entry_hash: Option<String>,
    pub entry_hash: String,
    pub committed_at: String,
    pub writer_identity_ref: String,
}

/// §14.4 resolution inputs, carried through snapshots verbatim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolutionInputs {
    pub source_set_hash: String,
    pub semantic_pack_set_hash: String,
    pub language_schema_version: String,
    pub interpretation_version: String,
    pub producer: String,
    pub entry_logical_path: String,
    pub registry_content_hash: Option<String>,
    pub resolved_namespaces: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub rule: String,
    pub message: String,
}

/// OmissionRecord mirroring CEP-0008's omission item shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmissionRecord {
    pub omission_id: String,
    pub omission_type: String,
    pub description: String,
    pub reason: String,
    pub known_or_suspected: String,
    pub impact: String,
    pub affected_entities: Vec<String>,
}

/// The closed sum of `SemanticSourcePort::resolve` (§7.1). A successful
/// resolution carries both faces of layer 3 (envelope + verbatim D); a failed
/// resolution carries the valid CEP failure envelope and an explicit omission
/// and NEVER fabricates semantic hashes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SemanticContext {
    Resolved {
        cep_envelope_json: String,
        representation_json: String,
        content_hash: String,
        semantic_closure_hash: String,
        resolution_inputs: ResolutionInputs,
        declarations: Vec<DeclKey>,
        diagnostics: Vec<Diagnostic>,
        model_validation_status: String,
    },
    ValidationFailed {
        cep_envelope_json: String,
        invalid_declared_checkpoint_hash: String,
        resolution_inputs: ResolutionInputs,
        diagnostics: Vec<Diagnostic>,
        representation_omission: OmissionRecord,
    },
}

impl SemanticContext {
    /// The checkpoint hash: the closure hash when resolved, else the
    /// invalid-declared checkpoint hash. Never None on the normal path.
    pub fn checkpoint_hash(&self) -> String {
        match self {
            SemanticContext::Resolved {
                semantic_closure_hash,
                ..
            } => semantic_closure_hash.clone(),
            SemanticContext::ValidationFailed {
                invalid_declared_checkpoint_hash,
                ..
            } => invalid_declared_checkpoint_hash.clone(),
        }
    }

    pub fn envelope_json(&self) -> &str {
        match self {
            SemanticContext::Resolved {
                cep_envelope_json, ..
            } => cep_envelope_json,
            SemanticContext::ValidationFailed {
                cep_envelope_json, ..
            } => cep_envelope_json,
        }
    }

    pub fn model_validation_status(&self) -> &str {
        match self {
            SemanticContext::Resolved {
                model_validation_status,
                ..
            } => model_validation_status.as_str(),
            SemanticContext::ValidationFailed { .. } => "invalid",
        }
    }
}

/// §12.1 `sxr_snapshot` row — the two shapes are mutually exclusive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotRow {
    pub snapshot_id: String,
    pub repo_id: String,
    pub semantic_closure_hash: Option<String>,
    pub content_hash: Option<String>,
    pub invalid_declared_checkpoint_hash: Option<String>,
    pub model_validation_status: String,
    pub semantics_version: String,
    pub representation_omission_json: Option<String>,
    pub source_set_hash: String,
    pub entry_logical_path: String,
    pub registry_content_hash: Option<String>,
    pub resolution_inputs_json: String,
    pub drift_class: Option<String>,
    pub first_seen_ledger_id: String,
    pub first_seen_append_ordinal: u64,
}

/// CEP-0006 §6.1 Evidence fields; every field must be present, never defaulted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub evidence_id: String,
    pub evidence_type: String,
    pub question_ref: String,
    pub target_claim_ref: Option<String>,
    pub target_entity_ref: Option<String>,
    pub representation_ref: String,
    pub source: String,
    pub provenance_refs: Vec<String>,
    pub relevance: String,
    pub reliability: String,
    pub direction: String,
    pub confidence_impact: String,
    pub timestamp: String,
    pub scope: Vec<String>,
    pub limitations: Vec<String>,
}

/// CEP-0007 §11 settlement status — closed list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettlementStatus {
    Unsettled,
    PartiallySettled,
    Settled,
    Failed,
    Blocked,
    Reversed,
    Superseded,
    HarmfulButSettled,
    BeneficialButUnsettled,
    UnauthorizedButSettled,
    AuthorizedButUnsettled,
    SettledWithDebt,
    Unknown,
}

impl SettlementStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            SettlementStatus::Unsettled => "unsettled",
            SettlementStatus::PartiallySettled => "partially_settled",
            SettlementStatus::Settled => "settled",
            SettlementStatus::Failed => "failed",
            SettlementStatus::Blocked => "blocked",
            SettlementStatus::Reversed => "reversed",
            SettlementStatus::Superseded => "superseded",
            SettlementStatus::HarmfulButSettled => "harmful_but_settled",
            SettlementStatus::BeneficialButUnsettled => "beneficial_but_unsettled",
            SettlementStatus::UnauthorizedButSettled => "unauthorized_but_settled",
            SettlementStatus::AuthorizedButUnsettled => "authorized_but_unsettled",
            SettlementStatus::SettledWithDebt => "settled_with_debt",
            SettlementStatus::Unknown => "unknown",
        }
    }
}

/// §12.1 `sxr_settlement` row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettlementRow {
    pub settlement_id: String,
    pub run_id: String,
    pub question_id: String,
    pub transformation_ref: String,
    pub threshold_id: String,
    pub settlement_status: String,
    pub confidence: String,
    pub debt_id: Option<String>,
    pub ledger_id: String,
    pub append_ordinal: u64,
}

/// §12.1 `sxr_evidence` row (projected slice of the CEP evidence payload).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceRow {
    pub evidence_id: String,
    pub run_id: String,
    pub question_id: String,
    pub evidence_type: String,
    pub target_kind: String,
    pub target_ref: String,
    pub relevance: String,
    pub reliability: String,
    pub direction: String,
    pub confidence_impact: String,
    pub representation_ref: String,
    pub ledger_id: String,
    pub append_ordinal: u64,
}

/// A §10.3 LedgerHead for the writer-facing API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerHead {
    pub position: LedgerPosition,
    pub entry_hash: String,
}
