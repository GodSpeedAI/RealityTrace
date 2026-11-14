//! Classify a CEP `execution_trace` envelope carrying
//! `extensions["godspeed.interaction_event"]` as accepted or rejected (E section 5).
//!
//! Order of checks: envelope shape, identity (`world_ref`, `declaration_key`),
//! extension schema, signature hook, source-record resolution. A rejected
//! event is returned with its code; the caller records it as `ingest.rejected`.
//! Nothing here counts a rejected event as an observation.

use crate::canon;
use jsonschema::JSONSchema;
use serde_json::{Value, json};
use std::collections::HashSet;
use sxr_core::LedgerRow;
use sxr_core::world_ref::is_world_ref;

pub const EXTENSION_KEY: &str = "godspeed.interaction_event";
/// Key under `extensions` on records this crate writes.
pub const INGEST_NOTE_KEY: &str = "sxr.ingest";
pub const REJECTION_NOTE_KEY: &str = "sxr.ingest_rejection";

const SCHEMA_TEXT: &str = include_str!("../schemas/godspeed.interaction_event.v1.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectCode {
    /// No usable `world_ref` or `declaration_key`.
    Identity,
    /// Valid identity, but `source.record_id` resolves to no evidence record.
    Evidence,
    /// Not an `execution_trace` envelope with the extension, or the extension
    /// violates its schema.
    Schema,
    /// The signature hook refused the event.
    Signature,
}

impl RejectCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Identity => "INGEST_REJECTED_IDENTITY",
            Self::Evidence => "INGEST_REJECTED_EVIDENCE",
            Self::Schema => "INGEST_REJECTED_SCHEMA",
            Self::Signature => "INGEST_REJECTED_SIGNATURE",
        }
    }
}

/// Best-effort selectors read from a candidate, so derive can attribute a
/// rejection to a window without trusting it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hints {
    pub world_ref: Option<String>,
    pub case_id: Option<String>,
    pub episode_ref: Option<String>,
    pub kind: Option<String>,
}

impl Hints {
    pub fn from_event(ext: &Value) -> Self {
        let s = |v: &Value| v.as_str().map(str::to_owned);
        Self {
            world_ref: s(&ext["world_ref"]),
            case_id: s(&ext["context"]["case_id"]),
            episode_ref: s(&ext["context"]["episode_ref"]),
            kind: s(&ext["kind"]),
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "world_ref": self.world_ref,
            "case_id": self.case_id,
            "episode_ref": self.episode_ref,
            "kind": self.kind,
        })
    }

    pub fn from_json(v: &Value) -> Self {
        let s = |k: &str| v[k].as_str().map(str::to_owned);
        Self {
            world_ref: s("world_ref"),
            case_id: s("case_id"),
            episode_ref: s("episode_ref"),
            kind: s("kind"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejection {
    pub code: RejectCode,
    pub reason: String,
    /// `sha256:` of the candidate envelope (canonical when canonicalisable).
    pub candidate_sha256: String,
    pub hints: Hints,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignatureStatus {
    /// No `emitter.signature`. Never counts as verified.
    Unsigned,
    /// A hook checked the signature successfully.
    Verified,
    /// A signature is present and no verifier could check it.
    Unverified(String),
}

impl SignatureStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unsigned => "unsigned",
            Self::Verified => "verified",
            Self::Unverified(_) => "unverified",
        }
    }
}

/// Signature-check hook. `Err` rejects the event (`INGEST_REJECTED_SIGNATURE`).
/// The signature is over the canonical extension without `emitter`
/// (CEP-0008 section 16.5); WP-33 supplies a real implementation.
pub trait SignatureCheck {
    fn check(&self, event: &Value) -> Result<SignatureStatus, String>;
}

/// S1 default: accepts explicitly unsigned events and records them as
/// `unsigned`; a present signature it cannot check is `unverified`.
pub struct UnsignedOnly;

impl SignatureCheck for UnsignedOnly {
    fn check(&self, event: &Value) -> Result<SignatureStatus, String> {
        if event["emitter"]["signature"].is_object() {
            Ok(SignatureStatus::Unverified(
                "no signature verifier configured".into(),
            ))
        } else {
            Ok(SignatureStatus::Unsigned)
        }
    }
}

/// Evidence record ids present in a set of ledger rows.
#[derive(Debug, Default, Clone)]
pub struct EvidenceIndex(HashSet<String>);

impl EvidenceIndex {
    /// Index `evidence.recorded` / `evidence.created` rows by evidence id,
    /// also under the id without its `evidence:` / `evidence:recorded:`
    /// prefix (the owning system's record id).
    pub fn from_rows<'a>(rows: impl IntoIterator<Item = &'a LedgerRow>) -> Self {
        let mut ids = HashSet::new();
        for row in rows {
            if row.record_kind != "evidence.recorded" && row.record_kind != "evidence.created" {
                continue;
            }
            let Ok(payload) = serde_json::from_str::<Value>(&row.payload) else {
                continue;
            };
            for item in payload["evidence"].as_array().into_iter().flatten() {
                let Some(id) = item["evidence_id"].as_str() else {
                    continue;
                };
                ids.insert(id.to_owned());
                for prefix in ["evidence:recorded:", "evidence:"] {
                    if let Some(rest) = id.strip_prefix(prefix) {
                        ids.insert(rest.to_owned());
                    }
                }
            }
        }
        Self(ids)
    }

    pub fn resolves(&self, record_id: &str) -> bool {
        self.0.contains(record_id)
    }
}

/// The compiled extension schema.
pub struct EventValidator(JSONSchema);

impl Default for EventValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl EventValidator {
    pub fn new() -> Self {
        let schema: Value = serde_json::from_str(SCHEMA_TEXT).expect("vendored schema is JSON");
        Self(JSONSchema::compile(&schema).expect("vendored schema compiles"))
    }

    /// Schema errors plus the one rule the schema cannot state.
    pub fn errors(&self, ext: &Value) -> Vec<String> {
        let mut out: Vec<String> = match self.0.validate(ext) {
            Ok(()) => vec![],
            Err(it) => it.map(|e| format!("{}: {e}", e.instance_path)).collect(),
        };
        if let Some(refs) = ext["expectation_refs"].as_array() {
            let mut seen = HashSet::new();
            if !refs
                .iter()
                .all(|r| seen.insert(r.as_str().unwrap_or_default()))
            {
                out.push("IE_EXPECTATION_REFS_DUPLICATE".into());
            }
        }
        out
    }
}

/// A `concept:<uuidv5>` declaration key. Shape only: resolving it against the
/// pinned world's snapshot is not done here.
pub fn is_declaration_key(s: &str) -> bool {
    s.strip_prefix("concept:")
        .and_then(|u| uuid::Uuid::parse_str(u).ok())
        .is_some_and(|u| u.get_version_num() == 5 && u.to_string() == s["concept:".len()..])
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accepted {
    /// The extension object, untouched.
    pub event: Value,
    pub signature: SignatureStatus,
    /// `<system>:<record_id>` (E section 2).
    pub occurrence_id: String,
}

fn candidate_digest(envelope: &Value) -> String {
    canon::digest(envelope).unwrap_or_else(|_| {
        sxr_core::canonical_json::sha256_hex(
            serde_json::to_string(envelope)
                .unwrap_or_default()
                .as_bytes(),
        )
    })
}

/// Classify one envelope.
pub fn classify(
    validator: &EventValidator,
    envelope: &Value,
    evidence: &EvidenceIndex,
    signature: &dyn SignatureCheck,
) -> Result<Accepted, Box<Rejection>> {
    let ext = &envelope["extensions"][EXTENSION_KEY];
    let reject = |code: RejectCode, reason: String| {
        Box::new(Rejection {
            code,
            reason,
            candidate_sha256: candidate_digest(envelope),
            hints: Hints::from_event(ext),
        })
    };
    if envelope["envelope_kind"] != "execution_trace" {
        return Err(reject(
            RejectCode::Schema,
            "envelope_kind is not execution_trace".into(),
        ));
    }
    if !ext.is_object() {
        return Err(reject(
            RejectCode::Schema,
            format!("extensions[{EXTENSION_KEY:?}] is absent or not an object"),
        ));
    }
    if !ext["world_ref"].as_str().is_some_and(is_world_ref) {
        return Err(reject(
            RejectCode::Identity,
            "world_ref is absent or not a pinned world_ref".into(),
        ));
    }
    if !ext["declaration_key"]
        .as_str()
        .is_some_and(is_declaration_key)
    {
        return Err(reject(
            RejectCode::Identity,
            "declaration_key is absent or not concept:<uuidv5>".into(),
        ));
    }
    let errors = validator.errors(ext);
    if !errors.is_empty() {
        return Err(reject(RejectCode::Schema, errors.join("; ")));
    }
    let status = signature
        .check(ext)
        .map_err(|m| reject(RejectCode::Signature, m))?;
    let system = ext["source"]["system"].as_str().unwrap_or_default();
    let record_id = ext["source"]["record_id"].as_str().unwrap_or_default();
    if !evidence.resolves(record_id) {
        return Err(reject(
            RejectCode::Evidence,
            format!("source.record_id {record_id:?} resolves to no evidence record"),
        ));
    }
    Ok(Accepted {
        event: ext.clone(),
        signature: status,
        occurrence_id: format!("{system}:{record_id}"),
    })
}

/// The pinned extension schema digest (WP-11).
pub fn schema_file_sha256() -> String {
    sxr_core::canonical_json::sha256_hex(SCHEMA_TEXT.as_bytes())
}
