//! ProofCompleted ingestion gate (convergence plan task T07; frozen edge E7,
//! swe_seed -> realitytrace). Native sxr surface over the v1 semantic-envelope
//! wire family: sxr takes NO dependency on SWE_SEED — the frozen producer
//! vocabulary, identity rules, and canonicalization are mirrored here from the
//! preregistration (`schemas/cep-semantic-envelope.mapping.md` lineage).
//!
//! Fail-closed binding chain, every link checked BEFORE anything is admitted:
//! exclusive `swe_seed` producer authority; canonical identity (structural
//! digest, no placeholder pseudo-hash — the standalone fallback constant
//! `dc144cb…` and the all-zero digest are refused — plus drift against the
//! locally-resolved model); loop namespace on both the payload and
//! `domain_model_ref`; all seven frozen required payload fields parsed and
//! validated; mandatory operational-settlement causal parent bound to the
//! locally-known settlement event id with work_request_id cross-checked; the
//! referenced settlement envelope (when delivered) resolved BY CONTENT — its
//! recomputed canonical digest must equal the cited `operational_settlement_ref`.
//!
//! Expected-versus-observed comparison inputs are preserved BY REFERENCE to
//! immutable upstream facts: admission records content digests alongside the
//! verbatim upstream values, so a later consumer can verify nothing was
//! rewritten. A re-submitted ProofCompleted claiming DIFFERENT expectations,
//! proof status, contract, or settlement reference for the same cycle is a
//! [`Refusal::ClaimMismatch`] — the first claim stands and upstream facts are
//! never silently replaced. Redelivery under stable identity is a
//! consequence-free duplicate.
//!
//! This module is PURE (AC-CORE-001): state is threaded explicitly through
//! [`IngestState`]; durable persistence remains the caller's concern.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::canonical_json::{canonical_json, sha256_canonical};
use crate::world_ref::is_world_ref;

/// The event type owned exclusively by `swe_seed` at edge E7.
pub const EVENT_TYPE: &str = "ProofCompleted";

/// The settlement event type owned exclusively by `sea_forge` at edge E6.
pub const SETTLEMENT_EVENT_TYPE: &str = "OperationalSettlement";

/// Loop namespace of the v1 wire family (mirrored frozen constant).
pub const NAMESPACE: &str = "agentic_capability_loop";

/// v1 schema version label (mirrored frozen constant).
pub const SCHEMA_VERSION: &str = "v1";

/// The standalone fallback pseudo-hash: `sha256("agentic_capability_loop")`.
/// A namespace-derived constant must never masquerade as model identity
/// (ENV-I2).
pub const FALLBACK_PSEUDO_HASH: &str =
    "dc144cbd71a483431301ca1bf32e95c014af4edba8dbcc525f505310e30a107e";

/// The frozen required payload fields of a ProofCompleted (edge E7).
pub const REQUIRED_FIELDS: [&str; 7] = [
    "work_request_id",
    "proof_result_id",
    "proof_contract",
    "proof_status",
    "expected_outcome",
    "operational_settlement_ref",
    "domain_model_ref",
];

/// Governed proof-status vocabulary (mirrored from the emitter surface).
pub const PROOF_STATUSES: [&str; 2] = ["passed", "failed"];

/// Explicit placeholder identities that must never bind an ingestion.
const PLACEHOLDER_IDS: [&str; 5] = ["placeholder", "unknown", "none", "<missing>", "tbd"];

/// Canonical component ids (preregistration scope vocabulary). Unknown agents
/// are `None` — fail closed.
fn canonical_agent(source_agent: &str) -> Option<&'static str> {
    match source_agent {
        "swe-seed" | "swe_seed" => Some("swe_seed"),
        "sea-forge-security-acceptance" | "security_acceptance" | "security-acceptance" => {
            Some("security_acceptance")
        }
        "context-kernel" | "context_kernel" => Some("context_kernel"),
        "sea-forge" | "sea_forge" => Some("sea_forge"),
        "godspeed-agent" | "godspeed_agent" | "gsa" => Some("godspeed_agent"),
        "realitytrace" | "reality-trace" | "sxr" => Some("realitytrace"),
        "memory-ledger" | "memory_ledger" | "agent_memory_ledger" => Some("memory_ledger"),
        "execution-environment" | "execution_environment" => Some("execution_environment"),
        "external-environment" | "external_environment" => Some("external_environment"),
        _ => None,
    }
}

/// Why a ProofCompleted delivery could not be ingested. Every refusal happens
/// BEFORE any state changes; a refused delivery settles nothing.
#[derive(Debug, Clone, PartialEq)]
pub enum Refusal {
    /// Not the canonical v1 wire shape this gate ingests.
    MalformedEnvelope(String),
    /// Right shape, wrong event type.
    WrongType { expected: String, got: String },
    /// Exclusive-producer authority violation: any component other than
    /// `swe_seed` attempting ProofCompleted, or an unknown stamp.
    ProducerForge { authoritative: String, got: String },
    /// Structurally invalid or missing domain_model_hash (ENV-I1).
    MalformedIdentity { got: String },
    /// A structurally valid but placeholder identity (ENV-I2).
    PlaceholderIdentity { got: String },
    /// Cross-repo drift against the locally-resolved model hash.
    HashDrift { expected: String, got: String },
    /// Payload or domain_model_ref namespace tampering.
    WrongNamespace { expected: String, got: String },
    /// domain_model_ref rewrites the declared identity (ENV-I5).
    RefIdentityMismatch { declared: String, ref_hash: String },
    /// Required payload fields absent, empty, or wrong-shaped; the complete
    /// list is reported together (T04 lesson D1).
    OpaquePayload { missing: Vec<&'static str> },
    /// A required identity/correlation field is blank or a literal placeholder.
    PlaceholderField { field: String },
    /// `proof_status` outside the governed vocabulary.
    InvalidProofStatus { got: String },
    /// An optional-but-typed field arrived with the wrong shape.
    InvalidOptionalField { field: &'static str },
    /// Malformed reference (empty, whitespace, scheme-less, bad digest).
    InvalidRef { got: String },
    /// The envelope does not cite the locally-known settlement as a causal
    /// parent, or cites malformed parent ids (ENV-I4).
    CausalityMissing {
        expected_settlement_event_id: String,
        recorded: Vec<String>,
    },
    /// Correlation mismatch with the locally-tracked cycle (ENV-I3).
    CrossWiredWorkRequest { expected: String, got: String },
    /// The delivered settlement envelope does not resolve to what the proof
    /// cites: digest mismatch, event-id mismatch, correlation mismatch, or a
    /// non-`sea_forge` settlement stamp (falsifier: forged parent reference).
    SettlementBindingMismatch(String),
    /// The payload carries no `world_ref`, or one that is not a pinned
    /// `world:<name>@sha256:<digest>` (an alias or label is not a world identity).
    MalformedWorldRef { got: String },
    /// The payload names a different semantic-world revision than the one this
    /// deployment is pinned to. Never reconciled implicitly; a move between
    /// worlds is a governed `world-transition`.
    WorldRefDrift { expected: String, got: String },
    /// A NEW envelope re-claims an ALREADY-ingested cycle with divergent
    /// claims. The original comparison inputs stand untouched (ENV-I5;
    /// plan tooth 2). Names every diverged field.
    ClaimMismatch { diverged: Vec<&'static str> },
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MalformedEnvelope(m) => write!(f, "malformed canonical envelope: {m}"),
            Self::WrongType { expected, got } => {
                write!(f, "expected {expected} event, got '{got}'")
            }
            Self::ProducerForge { authoritative, got } => write!(
                f,
                "producer forge: only '{authoritative}' may produce this event, got '{got}'"
            ),
            Self::MalformedIdentity { got } => {
                write!(
                    f,
                    "malformed domain_model_hash (want 64 lowercase hex): {got}"
                )
            }
            Self::PlaceholderIdentity { got } => write!(
                f,
                "fallback/placeholder pseudo-hash rejected as domain identity: {got}"
            ),
            Self::HashDrift { expected, got } => {
                write!(f, "domain_model_hash drift: expected {expected}, got {got}")
            }
            Self::WrongNamespace { expected, got } => {
                write!(f, "namespace mismatch: want {expected}, got '{got}'")
            }
            Self::RefIdentityMismatch { declared, ref_hash } => write!(
                f,
                "domain_model_ref names {ref_hash} but the envelope declares {declared}"
            ),
            Self::OpaquePayload { missing } => write!(
                f,
                "opaque ProofCompleted payload rejected — missing/invalid fields: {}",
                missing.join(", ")
            ),
            Self::PlaceholderField { field } => {
                write!(f, "{field} is a placeholder or blank identity")
            }
            Self::InvalidProofStatus { got } => {
                write!(f, "proof_status outside the governed vocabulary: {got}")
            }
            Self::InvalidOptionalField { field } => {
                write!(f, "optional field {field} has the wrong shape")
            }
            Self::InvalidRef { got } => write!(f, "malformed reference: {got:?}"),
            Self::CausalityMissing {
                expected_settlement_event_id,
                recorded,
            } => write!(
                f,
                "delivery does not cite the known operational settlement as causal parent: expected {expected_settlement_event_id}, recorded {recorded:?}"
            ),
            Self::CrossWiredWorkRequest { expected, got } => write!(
                f,
                "cross-wired delivery: tracked work_request_id {expected:?}, envelope carries {got:?}"
            ),
            Self::SettlementBindingMismatch(reason) => write!(
                f,
                "referenced settlement does not resolve to what the proof cites: {reason}"
            ),
            Self::MalformedWorldRef { got } => write!(
                f,
                "world_ref must be world:<name>@sha256:<64 lowercase hex>; got {got:?}"
            ),
            Self::WorldRefDrift { expected, got } => {
                write!(
                    f,
                    "world_ref drift: pinned {expected}, envelope names {got}"
                )
            }
            Self::ClaimMismatch { diverged } => write!(
                f,
                "re-submission contradicts the already-recorded cycle claims (first claim stands): diverged fields {:?}",
                diverged.to_vec()
            ),
        }
    }
}

impl std::error::Error for Refusal {}

/// The component a deployment binds as the authoritative ProofCompleted
/// producer for a cycle. Each producer is exclusive for the cycles bound to
/// it: a `swe_seed` cycle refuses every other stamp, and a frozen
/// security-acceptance cycle refuses `swe_seed` and, in particular, the
/// actor under test (`sea_forge`), whose claims are never proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProofProducer {
    /// The SWE_SEED harness (edge E7 as frozen).
    SweSeed,
    /// A frozen security-acceptance suite: expectations come from a pinned,
    /// independently governed suite, not from the system under test.
    SecurityAcceptance,
}

impl ProofProducer {
    /// Canonical component id this producer must be stamped with.
    pub fn canonical(self) -> &'static str {
        match self {
            Self::SweSeed => "swe_seed",
            Self::SecurityAcceptance => "security_acceptance",
        }
    }
}

fn default_proof_producer() -> String {
    ProofProducer::SweSeed.canonical().to_string()
}

/// Locally-resolved facts the gate binds against. Mirrors the T06/T04
/// convention: consumers bind to independently declared identities rather
/// than trusting the delivery.
#[derive(Debug, Clone)]
pub struct IngestParams<'a> {
    /// THIS deployment's resolved canonical model digest (drift anchor).
    pub local_model_sha256: &'a str,
    /// THIS deployment's pinned semantic world. Checked for syntax and
    /// equality only; the digest is recomputed by SEA-Forge, not here.
    pub local_world_ref: &'a str,
    /// The work request RealityTrace tracks for this cycle (ENV-I3).
    pub expected_work_request_id: &'a str,
    /// The settlement event id RealityTrace independently knows (ENV-I4).
    pub expected_settlement_event_id: &'a str,
}

/// Expected-versus-observed comparison inputs preserved BY REFERENCE: every
/// upstream value travels next to the content digest of its exact source
/// bytes, so downstream comparison (T08) can verify immutability instead of
/// trusting editable copies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComparisonInputs {
    /// Canonical id of the producer whose ProofCompleted was admitted.
    /// Defaults to `swe_seed` on state persisted before producers were
    /// distinguished.
    #[serde(default = "default_proof_producer")]
    pub proof_producer: String,
    /// The semantic world this comparison was made in (verbatim, pinned).
    /// Defaults to empty on state persisted before world pinning; emission
    /// refuses an empty or malformed value.
    #[serde(default)]
    pub world_ref: String,
    /// Content digest of the ingested ProofCompleted envelope itself.
    pub proof_envelope_sha256: String,
    /// The cited content-addressed settlement reference (verbatim).
    pub operational_settlement_ref: String,
    /// The settlement envelope id bound as causal parent.
    pub settlement_event_id: String,
    /// The declared expectation, VERBATIM as received.
    pub expected_outcome: Value,
    /// Observed execution effects projected from the referenced settlement —
    /// present only when the settlement envelope was delivered and resolved.
    pub observed_effects: Option<Value>,
    /// Content digest of the referenced settlement envelope when delivered.
    pub referenced_settlement_sha256: Option<String>,
}

/// One ingested cycle's recorded claims (what "first claim stands" protects).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct RecordedClaim {
    work_request_id: String,
    proof_result_id: String,
    operational_settlement_ref: String,
    expected_outcome_canonical: String,
    proof_status: String,
    proof_contract_canonical: String,
    model_hash: String,
    comparison: ComparisonInputs,
}

/// Explicitly-threaded ingestion state (pure core; durability is the
/// caller's concern). Serializable so restart-survival is testable without
/// I/O in this crate.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct IngestState {
    seen_identities: Vec<String>,
    claims: Vec<RecordedClaim>,
}

impl IngestState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of durably-recorded identities (envelope + content identities).
    pub fn len(&self) -> usize {
        self.seen_identities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seen_identities.is_empty()
    }
}

/// What happened when a delivery reached this gate.
#[derive(Debug, Clone, PartialEq)]
pub enum Admission {
    /// First admission of this delivery; carries the preserved comparison
    /// inputs for the later expected-versus-observed evaluation. Boxed: the
    /// inputs dwarf the other variant.
    First(Box<ComparisonInputs>),
    /// Redelivery under stable identity (event id or content digest already
    /// recorded). Consequence-free: nothing is rewritten, nothing appended.
    DuplicateDelivery,
}

/// Content digest of a JSON value: `sha256:<hex>` over canonical JSON — the
/// same scheme the SWE_SEED emitter uses for `operational_settlement_ref`,
/// so references computed there resolve here byte-identically.
pub fn content_digest(value: &Value) -> String {
    sha256_canonical(value)
}

fn well_formed_event_id(id: &str) -> bool {
    if uuid::Uuid::parse_str(id).is_ok() {
        return true;
    }
    // ULID shape: 26 Crockford base32 characters.
    id.len() == 26
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'A'..=b'H' | b'J' | b'K' | b'M' | b'N' | b'P'..=b'T' | b'V' | b'W' | b'X' | b'Z'))
}

fn is_canonical_digest(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn check_identity_digest(declared: &str) -> Result<(), Refusal> {
    if !is_canonical_digest(declared) {
        return Err(Refusal::MalformedIdentity {
            got: declared.to_string(),
        });
    }
    if declared == FALLBACK_PSEUDO_HASH || declared == "0".repeat(64) {
        return Err(Refusal::PlaceholderIdentity {
            got: declared.to_string(),
        });
    }
    Ok(())
}

fn meaningful(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn check_identity_string(field: &str, value: &str) -> Result<(), Refusal> {
    let trimmed = value.trim();
    if trimmed.is_empty() || PLACEHOLDER_IDS.contains(&trimmed.to_ascii_lowercase().as_str()) {
        return Err(Refusal::PlaceholderField {
            field: field.to_string(),
        });
    }
    Ok(())
}

fn validate_ref(r: &str) -> Result<(), Refusal> {
    let trimmed = r.trim();
    if trimmed.is_empty() || trimmed.contains(char::is_whitespace) || !trimmed.contains(':') {
        return Err(Refusal::InvalidRef { got: r.to_string() });
    }
    if let Some(digest) = trimmed.strip_prefix("sha256:")
        && !is_canonical_digest(digest)
    {
        return Err(Refusal::InvalidRef { got: r.to_string() });
    }
    Ok(())
}

fn validate_string_array(field: &'static str, value: &Value) -> Result<(), Refusal> {
    let items = value
        .as_array()
        .ok_or(Refusal::InvalidOptionalField { field })?;
    for item in items {
        match item.as_str() {
            Some(s) => validate_ref(s)?,
            None => return Err(Refusal::InvalidOptionalField { field }),
        }
    }
    Ok(())
}

/// Ingest one ProofCompleted delivery.
///
/// `referenced_settlement` carries the settlement envelope the delivery
/// cites, when the transport provides it (the golden-fixture flow does).
/// When absent, the gate still enforces causality against the locally-known
/// settlement id plus every other rule; it simply cannot additionally verify
/// content resolution — characterized honestly via
/// [`ComparisonInputs::referenced_settlement_sha256`] staying `None`.
pub fn ingest(
    proof: &Value,
    referenced_settlement: Option<&Value>,
    params: &IngestParams<'_>,
    state: &mut IngestState,
) -> Result<Admission, Refusal> {
    ingest_from(
        ProofProducer::SweSeed,
        proof,
        referenced_settlement,
        params,
        state,
    )
}

/// [`ingest`] for a cycle whose authoritative producer the deployment binds
/// explicitly. Every other gate (identity, namespace, settlement binding,
/// first claim stands) is unchanged.
pub fn ingest_from(
    producer: ProofProducer,
    proof: &Value,
    referenced_settlement: Option<&Value>,
    params: &IngestParams<'_>,
    state: &mut IngestState,
) -> Result<Admission, Refusal> {
    // 1. Structural wire shape.
    let obj = proof
        .as_object()
        .ok_or_else(|| Refusal::MalformedEnvelope("delivery is not a JSON object".into()))?;
    if obj.get("schema_version").and_then(Value::as_str) != Some(SCHEMA_VERSION) {
        return Err(Refusal::MalformedEnvelope(format!(
            "schema_version must be {SCHEMA_VERSION:?}"
        )));
    }
    match obj.get("event_type").and_then(Value::as_str) {
        Some(t) if t == EVENT_TYPE => {}
        Some(other) => {
            return Err(Refusal::WrongType {
                expected: EVENT_TYPE.to_string(),
                got: other.to_string(),
            });
        }
        None => return Err(Refusal::MalformedEnvelope("event_type missing".into())),
    }
    let event_id = meaningful(obj.get("event_id").and_then(Value::as_str))
        .ok_or_else(|| Refusal::MalformedEnvelope("event_id missing".into()))?;
    if !well_formed_event_id(&event_id) {
        return Err(Refusal::MalformedEnvelope(format!(
            "event_id is not a well-formed id: {event_id}"
        )));
    }

    // 2. Exclusive producer authority: the bound producer, nobody else.
    let source_agent = meaningful(obj.get("source_agent").and_then(Value::as_str))
        .ok_or_else(|| Refusal::MalformedEnvelope("source_agent missing".into()))?;
    match canonical_agent(&source_agent) {
        Some(agent) if agent == producer.canonical() => {}
        Some(other) => {
            return Err(Refusal::ProducerForge {
                authoritative: producer.canonical().to_string(),
                got: other.to_string(),
            });
        }
        None => {
            return Err(Refusal::ProducerForge {
                authoritative: producer.canonical().to_string(),
                got: source_agent.clone(),
            });
        }
    }

    let payload = obj
        .get("payload")
        .and_then(Value::as_object)
        .ok_or_else(|| Refusal::MalformedEnvelope("payload missing or not an object".into()))?;

    // 3. Canonical identity: structural, non-placeholder, no drift.
    let declared = payload
        .get("domain_model_hash")
        .and_then(Value::as_str)
        .unwrap_or_default();
    check_identity_digest(declared)?;
    if declared != params.local_model_sha256 {
        return Err(Refusal::HashDrift {
            expected: params.local_model_sha256.to_string(),
            got: declared.to_string(),
        });
    }

    // 3b. Semantic-world identity: required, pinned form, equal to ours.
    let world_ref = payload
        .get("world_ref")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !is_world_ref(world_ref) {
        return Err(Refusal::MalformedWorldRef {
            got: world_ref.to_string(),
        });
    }
    if world_ref != params.local_world_ref {
        return Err(Refusal::WorldRefDrift {
            expected: params.local_world_ref.to_string(),
            got: world_ref.to_string(),
        });
    }

    // 4. Namespace / schema tampering (payload level + domain_model_ref).
    let got_ns = payload
        .get("namespace")
        .and_then(Value::as_str)
        .unwrap_or("");
    if got_ns != NAMESPACE {
        return Err(Refusal::WrongNamespace {
            expected: NAMESPACE.to_string(),
            got: got_ns.to_string(),
        });
    }
    let model_ref = payload
        .get("domain_model_ref")
        .and_then(Value::as_object)
        .ok_or_else(|| Refusal::OpaquePayload {
            missing: vec!["domain_model_ref"],
        })?;
    if model_ref.get("namespace").and_then(Value::as_str) != Some(NAMESPACE) {
        return Err(Refusal::WrongNamespace {
            expected: NAMESPACE.to_string(),
            got: model_ref
                .get("namespace")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        });
    }
    let ref_hash = model_ref
        .get("model_hash")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if ref_hash != declared {
        return Err(Refusal::RefIdentityMismatch {
            declared: declared.to_string(),
            ref_hash: ref_hash.to_string(),
        });
    }

    // 5. Required-field battery — presence of ALL frozen fields together.
    let mut missing: Vec<&'static str> = Vec::new();
    for field in REQUIRED_FIELDS {
        let present = match payload.get(field) {
            None | Some(Value::Null) => false,
            Some(Value::String(s)) => !s.trim().is_empty(),
            Some(_) => true,
        };
        if !present {
            missing.push(field);
        }
    }
    if !missing.is_empty() {
        return Err(Refusal::OpaquePayload { missing });
    }

    // 6. Typed validation of each projected field (T04 lesson D1).
    let work_request_id = meaningful(payload.get("work_request_id").and_then(Value::as_str))
        .ok_or(Refusal::OpaquePayload {
            missing: vec!["work_request_id"],
        })?;
    check_identity_string("work_request_id", &work_request_id)?;
    let proof_result_id = meaningful(payload.get("proof_result_id").and_then(Value::as_str))
        .ok_or(Refusal::OpaquePayload {
            missing: vec!["proof_result_id"],
        })?;
    check_identity_string("proof_result_id", &proof_result_id)?;

    let proof_status = match payload.get("proof_status").and_then(Value::as_str) {
        Some(s) if PROOF_STATUSES.contains(&s) => s.to_string(),
        Some(other) => {
            return Err(Refusal::InvalidProofStatus {
                got: other.to_string(),
            });
        }
        None => unreachable!("presence battery"),
    };

    let contract = payload
        .get("proof_contract")
        .and_then(Value::as_object)
        .ok_or(Refusal::OpaquePayload {
            missing: vec!["proof_contract"],
        })?;
    let criterion = contract
        .get("criterion")
        .and_then(Value::as_str)
        .unwrap_or_default();
    check_identity_string("proof_contract.criterion", criterion)?;
    for key in ["command", "proof_type"] {
        if let Some(v) = contract.get(key)
            && v.as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .is_none()
            && !v.is_null()
        {
            return Err(Refusal::InvalidOptionalField {
                field: match key {
                    "command" => "proof_contract.command",
                    _ => "proof_contract.proof_type",
                },
            });
        }
    }

    let expected_outcome = payload
        .get("expected_outcome")
        .cloned()
        .expect("presence battery");

    let settlement_ref = payload
        .get("operational_settlement_ref")
        .and_then(Value::as_str)
        .unwrap_or_default();
    validate_ref(settlement_ref)?;
    if !settlement_ref.starts_with("sha256:") {
        return Err(Refusal::InvalidRef {
            got: settlement_ref.to_string(),
        });
    }

    if let Some(v) = payload.get("artifact_refs") {
        validate_string_array("artifact_refs", v)?;
    }
    if let Some(v) = payload.get("proof_evidence_refs") {
        validate_string_array("proof_evidence_refs", v)?;
    }
    if let Some(v) = payload.get("trace_root").and_then(Value::as_str)
        && (v.trim().is_empty() || v.contains(char::is_whitespace))
    {
        return Err(Refusal::InvalidOptionalField {
            field: "trace_root",
        });
    }
    if let Some(v) = payload.get("output_ref").and_then(Value::as_str)
        && (v.trim().is_empty() || v.contains(char::is_whitespace))
    {
        return Err(Refusal::InvalidOptionalField {
            field: "output_ref",
        });
    }

    // 7. Causality: the locally-known settlement MUST be among the recorded
    //    parents, and every recorded parent must be a well-formed id (ENV-I4).
    let mut recorded: Vec<String> = Vec::new();
    if let Some(chain) = obj
        .get("provenance")
        .and_then(|p| p.get("chain"))
        .and_then(Value::as_array)
    {
        for entry in chain {
            if let Some(s) = entry.as_str()
                && let Some(id) = s.strip_prefix("caused_by:")
            {
                if !well_formed_event_id(id) {
                    return Err(Refusal::CausalityMissing {
                        expected_settlement_event_id: params
                            .expected_settlement_event_id
                            .to_string(),
                        recorded: vec![format!("malformed parent id {id}")],
                    });
                }
                recorded.push(id.to_string());
            }
        }
    }
    if !recorded
        .iter()
        .any(|id| id == params.expected_settlement_event_id)
    {
        return Err(Refusal::CausalityMissing {
            expected_settlement_event_id: params.expected_settlement_event_id.to_string(),
            recorded: recorded.clone(),
        });
    }

    // 8. Correlation binding to the tracked cycle (ENV-I3).
    if work_request_id != params.expected_work_request_id.trim() {
        return Err(Refusal::CrossWiredWorkRequest {
            expected: params.expected_work_request_id.trim().to_string(),
            got: work_request_id.clone(),
        });
    }

    // 9. Duplicate recognition BEFORE any consequence: stable envelope
    //    identity (event id), content identity, and producer idempotency key.
    let proof_digest = content_digest(proof);
    let mut identities = vec![
        format!("event|{event_id}"),
        format!("content|{proof_digest}"),
    ];
    if let Some(idem) = payload_parent_idempotency(proof) {
        identities.push(idem);
    }
    if identities.iter().any(|i| state.seen_identities.contains(i)) {
        return Ok(Admission::DuplicateDelivery);
    }

    // 10. Referenced-settlement resolution BY CONTENT (when delivered):
    //     the cited digest must be the recomputed digest of the delivered
    //     settlement bytes, the settlement must be sea_forge's own, carry the
    //     same cycle correlation, and BE the bound causal parent.
    let mut observed_effects: Option<Value> = None;
    let mut settlement_digest: Option<String> = None;
    if let Some(settlement) = referenced_settlement {
        let reason = |r: String| Refusal::SettlementBindingMismatch(r);
        let s_obj = settlement
            .as_object()
            .ok_or_else(|| reason("delivered settlement is not a JSON object".to_string()))?;
        if s_obj.get("event_type").and_then(Value::as_str) != Some(SETTLEMENT_EVENT_TYPE) {
            return Err(reason(format!(
                "delivered settlement is not a {SETTLEMENT_EVENT_TYPE} envelope"
            )));
        }
        let s_agent = s_obj
            .get("source_agent")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if canonical_agent(s_agent) != Some("sea_forge") {
            return Err(reason(format!(
                "settlement stamp '{s_agent}' is not the authoritative sea_forge"
            )));
        }
        let s_id = s_obj
            .get("event_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if s_id != params.expected_settlement_event_id {
            return Err(reason(format!(
                "delivered settlement event id {s_id} does not match the locally-known {}",
                params.expected_settlement_event_id
            )));
        }
        if !recorded.iter().any(|id| id == s_id) {
            return Err(reason(
                "the delivered settlement is NOT among the recorded causal parents".to_string(),
            ));
        }
        let recomputed = content_digest(settlement);
        if recomputed != settlement_ref {
            return Err(reason(format!(
                "cited ref {settlement_ref} does not resolve to the delivered settlement bytes ({recomputed})"
            )));
        }
        let s_wr = settlement
            .get("payload")
            .and_then(|p| p.get("work_request_id"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        if s_wr != work_request_id {
            return Err(reason(format!(
                "settlement correlates to {s_wr:?}, proof claims {work_request_id:?}"
            )));
        }
        settlement_digest = Some(recomputed);
        observed_effects = settlement
            .get("payload")
            .and_then(|p| p.get("observed_effects"))
            .cloned();
    }

    // 11. Cycle-claim stability (ENV-I5; plan tooth 2): a FRESH envelope
    //     re-claiming an already-ingested (cycle, proof result) with divergent
    //     expectations/status/contract/settlement reference is refused — the
    //     first claim stands and upstream facts are never silently replaced.
    let claim = RecordedClaim {
        work_request_id: work_request_id.clone(),
        proof_result_id: proof_result_id.clone(),
        operational_settlement_ref: settlement_ref.to_string(),
        expected_outcome_canonical: canonical_json(&expected_outcome),
        proof_status: proof_status.clone(),
        proof_contract_canonical: canonical_json(&Value::Object(contract.clone())),
        model_hash: declared.to_string(),
        comparison: ComparisonInputs {
            proof_producer: producer.canonical().to_string(),
            world_ref: world_ref.to_string(),
            proof_envelope_sha256: proof_digest.clone(),
            operational_settlement_ref: settlement_ref.to_string(),
            settlement_event_id: params.expected_settlement_event_id.to_string(),
            expected_outcome: expected_outcome.clone(),
            observed_effects: observed_effects.clone(),
            referenced_settlement_sha256: settlement_digest.clone(),
        },
    };
    if let Some(prior) = state.claims.iter().find(|c| {
        c.work_request_id == claim.work_request_id && c.proof_result_id == claim.proof_result_id
    }) {
        let mut diverged: Vec<&'static str> = Vec::new();
        if prior.operational_settlement_ref != claim.operational_settlement_ref {
            diverged.push("operational_settlement_ref");
        }
        if prior.expected_outcome_canonical != claim.expected_outcome_canonical {
            diverged.push("expected_outcome");
        }
        if prior.proof_status != claim.proof_status {
            diverged.push("proof_status");
        }
        if prior.proof_contract_canonical != claim.proof_contract_canonical {
            diverged.push("proof_contract");
        }
        if prior.comparison.world_ref != claim.comparison.world_ref {
            diverged.push("world_ref");
        }
        if prior.model_hash != claim.model_hash {
            diverged.push("domain_model_ref");
        }
        if !diverged.is_empty() {
            return Err(Refusal::ClaimMismatch { diverged });
        }
        // Identical claims under a fresh envelope identity: treat as a
        // duplicate of the settled fact (consequence-free redelivery).
        return Ok(Admission::DuplicateDelivery);
    }

    // 12. Admit — identities then claim, atomically over the threaded state.
    let comparison = claim.comparison.clone();
    state.seen_identities.extend(identities);
    state.claims.push(claim);

    Ok(Admission::First(Box::new(comparison)))
}

/// Fold the producer idempotency key into a namespaced ledger identity.
fn payload_parent_idempotency(envelope: &Value) -> Option<String> {
    let k = envelope.get("idempotency_key").and_then(Value::as_str)?;
    if k.len() == 64 && k.bytes().all(|b| b.is_ascii_hexdigit()) {
        Some(format!("idem|{k}"))
    } else {
        None
    }
}
