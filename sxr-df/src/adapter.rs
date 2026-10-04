//! `CliDomainForgeAdapter`: the §7.1 `SemanticSourcePort` normal adapter.
//!
//! Classification is **by envelope shape, never by exit code**: a Slice 0B
//! `envelope --emit cep` run exits 0 for a fully valid model, 1 for *any*
//! model-validation failure — but that set of failures has two distinct
//! envelope shapes:
//!
//! - D was constructed (`validation_status: "validated"` + `representations`)
//!   → `SemanticContext::Resolved`, whatever the diagnostics say. `content_hash`
//!   and `semantic_closure_hash` identify an actual D (§7.1 requires them
//!   exactly then).
//! - D could not be constructed (`conformance_status: "non_conformant"`, no
//!   representations, missing-D omission) → `SemanticContext::ValidationFailed`
//!   with the exact-failed-inputs checkpoint hash (§14.4).
//!
//! Any other shape is an error (§10.4a: malformed output is never
//! misclassified into the closed sum).

use crate::error::SemanticError;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use sxr_core::canonical_json::{canonical_json, sha256_hex};
use sxr_core::embedded_schema::EnvelopeSchemaValidator;
use sxr_core::types::{DeclKey, Diagnostic, OmissionRecord, ResolutionInputs, SemanticContext};

/// What the adapter can report about itself (§7.1 `capabilities()`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AdapterCapabilities {
    pub producer: String,
    pub cep_envelope_mode: String,
    pub supports_semantic_packs: bool,
    pub supports_default_namespace: bool,
    pub verifies_d_hashes: bool,
}

/// The request shape for `resolve` (§7.1 `SemanticContextRequest`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticContextRequest {
    /// The repository's semantic root; the domainforge process runs with this
    /// as its working directory so the logical entry path stays stable.
    pub semantic_root: PathBuf,
    /// Logical path of the entry `.sea` relative to the root. Never absolute:
    /// the producer embeds it in the envelope scope verbatim.
    pub entry_logical_path: String,
    /// Registry file (`registry_path: Option`); Slice 0B auto-discovers the
    /// registry from the entry's directory, so this is advisory only.
    pub registry_path: Option<PathBuf>,
    pub default_namespace: Option<String>,
    /// `(pack_id, content_hash)` pairs, passed through as `--pack id=hash`.
    pub semantic_packs: Vec<(String, String)>,
    /// `cep_scope` object, passed through as `--scope` (replaces the default).
    pub cep_scope: Value,
    /// CAS store root (the snapshot layer's artifact store). When set, the
    /// adapter materializes CAS-carried representations (`content_ref`) into
    /// this store — verified, content-addressed by `content_hash` — so the
    /// Authoritative Replay Set's "durable CAS representations" hold. When
    /// unset, CAS-carried D is still materialized from the producer's
    /// `--emit both` representation for the adapter contract, but nothing is
    /// persisted.
    pub cas_root: Option<PathBuf>,
}

/// The normal adapter: one configured `domainforge` binary path.
pub struct CliDomainForgeAdapter {
    binary: PathBuf,
    required_version: Option<String>,
    validator: EnvelopeSchemaValidator,
}

impl std::fmt::Debug for CliDomainForgeAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CliDomainForgeAdapter")
            .field("binary", &self.binary)
            .finish_non_exhaustive()
    }
}

impl CliDomainForgeAdapter {
    pub fn new(binary: impl Into<PathBuf>) -> Self {
        Self {
            binary: binary.into(),
            required_version: None,
            validator: EnvelopeSchemaValidator::new(),
        }
    }

    /// Fail closed unless the configured binary reports exactly this
    /// DomainForge version (`domainforge --version`). Unset, no check runs.
    pub fn with_required_version(mut self, version: impl Into<String>) -> Self {
        self.required_version = Some(version.into());
        self
    }

    fn verify_version(&self) -> Result<(), SemanticError> {
        let Some(expected) = &self.required_version else {
            return Ok(());
        };
        let found = Command::new(&self.binary)
            .arg("--version")
            .output()
            .map_err(|e| SemanticError::BinaryUnavailable(e.to_string()))
            .map(|out| {
                String::from_utf8_lossy(&out.stdout)
                    .split_whitespace()
                    .last()
                    .unwrap_or_default()
                    .to_string()
            })?;
        if &found == expected {
            Ok(())
        } else {
            Err(SemanticError::UnsupportedVersion {
                expected: expected.clone(),
                found,
            })
        }
    }

    pub fn binary(&self) -> &Path {
        &self.binary
    }

    pub fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            producer: "domainforge (Slice 0B)".into(),
            cep_envelope_mode: "semantic_snapshot".into(),
            supports_semantic_packs: true,
            supports_default_namespace: true,
            verifies_d_hashes: true,
        }
    }

    /// Resolve one entry into the closed sum (§7.1).
    pub fn resolve(
        &self,
        request: &SemanticContextRequest,
    ) -> Result<SemanticContext, SemanticError> {
        self.verify_version()?;
        let mut argv: Vec<String> = vec!["envelope".into(), "--emit".into(), "both".into()];
        for (id, hash) in &request.semantic_packs {
            argv.push("--pack".into());
            argv.push(format!("{id}={hash}"));
        }
        if let Some(ns) = &request.default_namespace {
            argv.push("--default-namespace".into());
            argv.push(ns.clone());
        }
        if let Some(reg) = &request.registry_path {
            argv.push("--registry".into());
            argv.push(reg.to_string_lossy().to_string());
        }
        argv.push("--scope".into());
        argv.push(canonical_json(&request.cep_scope));
        argv.push(request.entry_logical_path.clone());

        let mut child = Command::new(&self.binary)
            .current_dir(&request.semantic_root)
            .args(&argv)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| SemanticError::BinaryUnavailable(e.to_string()))?;

        let mut stdout_reader = child.stdout.take().expect("piped stdout");
        let mut stderr_reader = child.stderr.take().expect("piped stderr");

        let h_out = std::thread::spawn(move || {
            use std::io::Read;
            let mut buf = Vec::new();
            let _ = (&mut stdout_reader)
                .take(16 * 1024 * 1024)
                .read_to_end(&mut buf);
            buf
        });
        let h_err = std::thread::spawn(move || {
            use std::io::Read;
            let mut buf = Vec::new();
            let _ = (&mut stderr_reader)
                .take(16 * 1024 * 1024)
                .read_to_end(&mut buf);
            buf
        });

        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(30);
        let status = loop {
            match child
                .try_wait()
                .map_err(|e| SemanticError::BinaryUnavailable(e.to_string()))?
            {
                Some(status) => break status,
                None => {
                    if start.elapsed() > timeout {
                        let _ = child.kill();
                        return Err(SemanticError::UnexpectedExit {
                            code: None,
                            stderr: "domainforge process timed out after 30 seconds".into(),
                        });
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
        };

        let stdout_buf = h_out.join().unwrap_or_default();
        let stderr_buf = h_err.join().unwrap_or_default();

        let stderr = String::from_utf8_lossy(&stderr_buf).trim().to_string();
        let code = status.code();
        if !matches!(code, Some(0) | Some(1)) {
            return Err(SemanticError::UnexpectedExit { code, stderr });
        }
        if stdout_buf.is_empty() {
            return Err(SemanticError::MalformedOutput(format!(
                "empty stdout (exit {code:?}): {stderr}"
            )));
        }
        self.parse_both(&stdout_buf, request.cas_root.as_deref())
    }

    /// Parse the `--emit both` output: `{"representation": D, "cep_envelope":
    /// E}` on the success-with-D path; the bare failure envelope on the
    /// non-conformant path. CAS-carried D (`content_ref`) is materialized here
    /// through the snapshot/runtime boundary (§10.7a): verified against
    /// `content_hash`, persisted into the CAS store when one is configured,
    /// and handed to the envelope parse as inline bytes — carriage mode never
    /// changes semantic meaning.
    pub fn parse_both(
        &self,
        raw: &[u8],
        cas_root: Option<&Path>,
    ) -> Result<SemanticContext, SemanticError> {
        let both: Value = serde_json::from_slice(raw)
            .map_err(|e| SemanticError::MalformedOutput(format!("stdout is not JSON: {e}")))?;
        let (envelope, representation) = match both.get("cep_envelope") {
            Some(env) => (env.clone(), both.get("representation").cloned()),
            None => (both, None),
        };
        self.parse_envelope_with_cas(&envelope, representation.as_ref(), cas_root)
    }

    /// Parse, schema-validate, classify, and extract one raw stdout payload
    /// (the `--emit cep` shape, or the bare envelope from `--emit both`).
    /// Shared by tests (no process involved).
    pub fn parse_json(&self, raw: &[u8]) -> Result<SemanticContext, SemanticError> {
        let envelope: Value = serde_json::from_slice(raw)
            .map_err(|e| SemanticError::MalformedOutput(format!("stdout is not JSON: {e}")))?;
        self.parse_envelope_with_cas(&envelope, None, None)
    }

    /// Validate + classify + extract one envelope `Value`. Classification is
    /// by shape only (see module docs); `resolve` applies the exit-code check.
    /// CAS-carried representations are materialized from `representation` (the
    /// producer's D) and the CAS store when configured.
    pub fn parse_envelope(&self, envelope: &Value) -> Result<SemanticContext, SemanticError> {
        self.parse_envelope_with_cas(envelope, None, None)
    }

    fn parse_envelope_with_cas(
        &self,
        envelope: &Value,
        representation: Option<&Value>,
        cas_root: Option<&Path>,
    ) -> Result<SemanticContext, SemanticError> {
        if !envelope.is_object() {
            return Err(SemanticError::MalformedOutput(
                "stdout JSON is not an object".into(),
            ));
        }

        // §52/§53 validation at ingest (F-05): a malformed envelope is never
        // stored as an envelope, and never misclassified into the closed sum.
        if let Err(errors) = self.validator.validate(envelope) {
            return Err(SemanticError::EnvelopeSchemaViolation(errors.join("; ")));
        }

        if envelope.get("validation_status").and_then(Value::as_str) == Some("validated")
            && envelope.get("representations").is_some()
        {
            self.extract_resolved(envelope, representation, cas_root)
        } else if envelope.get("representations").is_none()
            && (envelope.get("conformance_status").and_then(Value::as_str)
                == Some("non_conformant")
                || envelope.get("conformance_status").and_then(Value::as_str) == Some("conformant")
                || envelope.get("omissions").is_some())
        {
            self.extract_validation_failed(envelope)
        } else {
            Err(SemanticError::MalformedOutput(
                "envelope matches neither the resolved shape (validation_status \
                 + representations) nor the failure shape (omissions)"
                    .into(),
            ))
        }
    }

    /// `Resolved` extraction: D verbatim (inline or materialized from the
    /// both-mode representation / CAS store), both stable hashes verified
    /// against the canonical bytes, declarations from the preserved-entities
    /// list, resolution inputs from the provenance record (§10.7a).
    fn extract_resolved(
        &self,
        envelope: &Value,
        representation: Option<&Value>,
        cas_root: Option<&Path>,
    ) -> Result<SemanticContext, SemanticError> {
        let rep = &envelope["representations"][0];
        let content_hash = string_field(rep, "content_hash", "representations[0]")?;
        let semantic_closure_hash = string_field(rep, "semantic_hash", "representations[0]")?;

        // Carriage mode must not change semantic meaning: inline D is used
        // verbatim; CAS-carried D is materialized through the snapshot/runtime
        // boundary and handed to the same verification path.
        let representation_json = match rep.get("inline_content") {
            Some(Value::String(bytes)) => bytes.clone(),
            Some(_) => {
                return Err(SemanticError::MalformedOutput(
                    "representations[0].inline_content is not a string".into(),
                ));
            }
            None => self.materialize_cas_carried(rep, representation, cas_root, &content_hash)?,
        };

        // AC-REPLAY-001 half: D's content hash is recomputed over the
        // canonical document bytes (self_hash excluded from its own digest),
        // and the closure hash over the canonical envelope payload.
        verify_d_hashes(&representation_json, &content_hash, &semantic_closure_hash)?;

        let resolution_inputs = resolution_inputs_from(rep)?;
        let declarations = declarations_from(rep)?;
        let diagnostics = diagnostics_from(envelope)?;

        let model_validation_status = if envelope
            .get("extensions")
            .and_then(|ext| ext.get("domainforge"))
            .and_then(|df| df.get("model_validation_status"))
            .and_then(Value::as_str)
            == Some("invalid")
            || rep.get("validation_status").and_then(Value::as_str) == Some("invalid")
        {
            "invalid".to_string()
        } else {
            "valid".to_string()
        };

        Ok(SemanticContext::Resolved {
            cep_envelope_json: serde_json::to_string(envelope)
                .expect("validated envelope serializes"),
            representation_json,
            content_hash,
            semantic_closure_hash,
            resolution_inputs,
            declarations,
            diagnostics,
            model_validation_status,
        })
    }

    /// Snapshot-layer materialization for CAS-carried representations (§10.7a,
    /// CEP-0008 §14.2 Referenced Carriage).
    ///
    /// `content_ref` names the CAS object (`cas://sha256:<hex>`); the
    /// representation's own `content_hash` must match that address and the
    /// materialized bytes must validate as the expected DomainForge
    /// representation D. Source of D bytes, in order:
    ///
    /// 1. The producer's `--emit both` representation (the deterministic D),
    ///    verified against the declared hash — used on the create path and
    ///    never persisted unless a CAS root is configured.
    /// 2. The CAS store (`cas_root`), content-addressed by `content_hash`.
    ///
    /// Missing, expired, or hash-mismatched CAS content fails loudly; the
    /// adapter never silently falls back to a different snapshot or re-resolves
    /// different semantics.
    fn materialize_cas_carried(
        &self,
        rep: &Value,
        representation: Option<&Value>,
        cas_root: Option<&Path>,
        content_hash: &str,
    ) -> Result<String, SemanticError> {
        let content_ref = string_field(rep, "content_ref", "representations[0]")?;
        let (scheme, ref_hash) = content_ref.split_once("://").ok_or_else(|| {
            SemanticError::MalformedOutput(format!("malformed content_ref {content_ref}"))
        })?;
        if scheme != "cas" {
            return Err(SemanticError::MalformedOutput(format!(
                "unsupported content_ref scheme {scheme}"
            )));
        }
        // `cas://sha256:<hex>` — the address hash includes the `sha256:`
        // prefix and must equal `content_hash` exactly.
        if ref_hash != content_hash {
            return Err(SemanticError::HashMismatch(format!(
                "content_ref {content_ref} does not match content_hash {content_hash}"
            )));
        }
        let expected_hex = content_hash.strip_prefix("sha256:").ok_or_else(|| {
            SemanticError::MalformedOutput(format!(
                "content_hash {content_hash} is not a sha256 ref"
            ))
        })?;
        if rep.get("content_availability").and_then(Value::as_str) == Some("expired") {
            return Err(SemanticError::MalformedOutput(
                "representation content_availability is expired; CAS object cannot \
                 be materialized"
                    .into(),
            ));
        }

        // 1. Producer-carried D (deterministic, canonical): verify the declared
        //    content hash against the canonical bytes before use. The digest
        //    follows AC-REPLAY-001: `self_hash` is excluded from its own
        //    digest, exactly as the inline path's `verify_d_hashes` computes it.
        if let Some(d) = representation {
            let mut selfless = d.clone();
            selfless
                .as_object_mut()
                .expect("representation is a document object")
                .remove("self_hash");
            let computed = sha256_hex(canonical_json(&selfless).as_bytes());
            if computed != content_hash {
                return Err(SemanticError::HashMismatch(format!(
                    "producer representation {computed} != content_hash {content_hash}"
                )));
            }
            // Persist the full canonical D into the CAS store when configured,
            // so the ARS's durable-CAS-representation requirement holds for
            // this snapshot.
            let canonical = canonical_json(d);
            if let Some(root) = cas_root {
                persist_cas_object(root, expected_hex, canonical.as_bytes())?;
            }
            return Ok(canonical);
        }

        // 2. CAS store lookup by content address.
        let root = cas_root.ok_or_else(|| {
            SemanticError::MalformedOutput(
                "CAS-carried representation has no producer D and no cas_root is \
                 configured; cannot materialize"
                    .into(),
            )
        })?;
        let path = cas_object_path(root, expected_hex);
        let bytes = std::fs::read(&path).map_err(|_| {
            SemanticError::MalformedOutput(format!(
                "CAS object missing for content_ref {content_ref} (expected {})",
                path.display()
            ))
        })?;
        // The object is the canonical D document (with its own self_hash, as
        // domainforge emits it); the content address covers the selfless
        // canonical bytes (AC-REPLAY-001). Parse, strip self_hash, recompute.
        let doc: Value = serde_json::from_slice(&bytes).map_err(|e| {
            SemanticError::MalformedOutput(format!(
                "CAS object {} is not JSON: {e}",
                path.display()
            ))
        })?;
        let mut selfless = doc.clone();
        selfless
            .as_object_mut()
            .expect("D is a document object")
            .remove("self_hash");
        let computed = sha256_hex(canonical_json(&selfless).as_bytes());
        if computed != content_hash {
            return Err(SemanticError::HashMismatch(format!(
                "CAS object {} computed {computed} != content_hash {content_hash}",
                path.display()
            )));
        }
        String::from_utf8(bytes)
            .map_err(|e| SemanticError::MalformedOutput(format!("CAS object is not UTF-8: {e}")))
    }

    /// `ValidationFailed` extraction: the §14.4 checkpoint hash identifies the
    /// exact failed inputs; the omission record is the envelope's; resolution
    /// inputs are populated only from what the failure boundary actually
    /// carries (source set ref + the envelope's own entry path) — never
    /// fabricated.
    fn extract_validation_failed(
        &self,
        envelope: &Value,
    ) -> Result<SemanticContext, SemanticError> {
        let ext = envelope
            .get("extensions")
            .and_then(|e| e.get("domainforge"))
            .ok_or_else(|| {
                SemanticError::MalformedOutput(
                    "failure envelope lacks extensions.domainforge".into(),
                )
            })?;
        let invalid_declared_checkpoint_hash = string_field(
            ext,
            "invalid_declared_checkpoint_hash",
            "extensions.domainforge",
        )?;
        let diagnostics = diagnostics_from(envelope)?;

        let omission = envelope
            .get("omissions")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
            .ok_or_else(|| {
                SemanticError::MalformedOutput("failure envelope lacks omissions[0]".into())
            })?;
        let representation_omission = OmissionRecord {
            omission_id: string_field(omission, "omission_id", "omissions[0]")?,
            omission_type: string_field(omission, "omission_type", "omissions[0]")?,
            description: string_field(omission, "description", "omissions[0]")?,
            reason: string_field(omission, "reason", "omissions[0]")?,
            known_or_suspected: string_field(omission, "known_or_suspected", "omissions[0]")?,
            impact: string_field(omission, "impact", "omissions[0]")?,
            affected_entities: string_array(omission, "affected_entities", "omissions[0]")?,
        };

        // The failure envelope carries only source_set_hash (provenance_refs)
        // and entry_logical_path (scope or boundary record); the checkpoint
        // hash is the authoritative exact-failed-inputs identity (§14.4).
        // Fields the boundary does not carry are empty, never invented.
        let source_set_hash = envelope
            .get("provenance_refs")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
            .and_then(Value::as_str)
            .and_then(|r| r.strip_prefix("source-set:"))
            .unwrap_or("")
            .to_string();
        let entry_logical_path = envelope
            .get("scope")
            .and_then(|s| s.get("entry_logical_path"))
            .and_then(Value::as_str)
            .or_else(|| {
                envelope
                    .get("boundary_record")
                    .and_then(|b| b.get("scope"))
                    .and_then(Value::as_str)
            })
            .unwrap_or("")
            .to_string();
        let resolution_inputs = ResolutionInputs {
            source_set_hash,
            semantic_pack_set_hash: String::new(),
            language_schema_version: String::new(),
            interpretation_version: String::new(),
            producer: String::new(),
            entry_logical_path,
            registry_content_hash: None,
            resolved_namespaces: Vec::new(),
        };

        Ok(SemanticContext::ValidationFailed {
            cep_envelope_json: serde_json::to_string(envelope)
                .expect("validated envelope serializes"),
            invalid_declared_checkpoint_hash,
            resolution_inputs,
            diagnostics,
            representation_omission,
        })
    }
}

/// AC-REPLAY-001 hash verification: `content_hash` must equal the sha256 of
/// the canonical document bytes (with `self_hash` excluded from its own
/// digest — the producer's `envelope_document_self_hash` rule), and
/// `semantic_closure_hash` the sha256 of the canonical envelope payload.
fn verify_d_hashes(
    representation_json: &str,
    declared_content_hash: &str,
    declared_closure_hash: &str,
) -> Result<(), SemanticError> {
    let doc: Value = serde_json::from_str(representation_json)
        .map_err(|e| SemanticError::MalformedOutput(format!("inline_content is not JSON: {e}")))?;
    let envelope = doc
        .get("envelope")
        .ok_or_else(|| SemanticError::MalformedOutput("D lacks an envelope payload".into()))?;

    let mut selfless = doc.clone();
    selfless
        .as_object_mut()
        .expect("document is an object")
        .remove("self_hash");
    let computed_content = sha256_hex(canonical_json(&selfless).as_bytes());
    if computed_content != declared_content_hash {
        return Err(SemanticError::HashMismatch(format!(
            "content_hash {declared_content_hash} != sha256(canonical D) {computed_content}"
        )));
    }
    let computed_closure = sha256_hex(canonical_json(envelope).as_bytes());
    if computed_closure != declared_closure_hash {
        return Err(SemanticError::HashMismatch(format!(
            "semantic_closure_hash {declared_closure_hash} != sha256(canonical envelope) {computed_closure}"
        )));
    }
    Ok(())
}

/// Resolution inputs from the representation's provenance record (§10.7a).
fn resolution_inputs_from(rep: &Value) -> Result<ResolutionInputs, SemanticError> {
    let record = rep.get("provenance_record").ok_or_else(|| {
        SemanticError::MalformedOutput("representations[0] lacks provenance_record".into())
    })?;
    let inputs = record
        .get("inputs")
        .ok_or_else(|| SemanticError::MalformedOutput("provenance_record lacks inputs".into()))?;
    let producer = record
        .get("producer")
        .ok_or_else(|| SemanticError::MalformedOutput("provenance_record lacks producer".into()))?;
    let name = string_field(producer, "name", "provenance_record.producer")?;
    let version = string_field(producer, "version", "provenance_record.producer")?;

    let mut resolved_namespaces = Vec::new();
    if let Some(Value::Object(map)) = record
        .get("resolution_inputs")
        .and_then(|r| r.get("resolved_namespaces"))
    {
        let mut keys: Vec<&String> = map.keys().collect();
        keys.sort();
        for key in keys {
            if let Some(Value::String(ns)) = map.get(key) {
                resolved_namespaces.push((key.clone(), ns.clone()));
            }
        }
    }

    Ok(ResolutionInputs {
        source_set_hash: string_field(inputs, "source_set_hash", "provenance_record.inputs")?,
        semantic_pack_set_hash: string_field(
            inputs,
            "semantic_pack_set_hash",
            "provenance_record.inputs",
        )?,
        language_schema_version: string_field(
            inputs,
            "language_schema_version",
            "provenance_record.inputs",
        )?,
        interpretation_version: string_field(
            inputs,
            "interpretation_version",
            "provenance_record.inputs",
        )?,
        producer: format!("{name} {version}"),
        entry_logical_path: string_field(
            record.get("resolution_inputs").unwrap_or(&Value::Null),
            "entry_logical_path",
            "provenance_record.resolution_inputs",
        )?,
        registry_content_hash: record
            .get("resolution_inputs")
            .and_then(|r| r.get("registry_content_hash"))
            .and_then(Value::as_str)
            .map(str::to_string),
        resolved_namespaces,
    })
}

/// DeclKeys from the preserved-entities refs. The ref formats are exactly
/// `sxr-core::DeclKey::as_str` (`concept:…`, `application:…`,
/// `occurrence:<kind>:<content_hash>:<duplicate_index>`).
fn declarations_from(rep: &Value) -> Result<Vec<DeclKey>, SemanticError> {
    let mut out = Vec::new();
    for entity in string_array(rep, "preserved_entities", "representations[0]")? {
        if let Some(id) = entity.strip_prefix("concept:") {
            out.push(DeclKey::Concept(id.to_string()));
        } else if let Some(sym) = entity.strip_prefix("application:") {
            out.push(DeclKey::Application(sym.to_string()));
        } else if let Some(rest) = entity.strip_prefix("occurrence:") {
            let (kind, rest) = rest.split_once(':').ok_or_else(|| {
                SemanticError::MalformedOutput(format!("malformed occurrence ref {entity}"))
            })?;
            let (content_hash, duplicate_index) = rest.rsplit_once(':').ok_or_else(|| {
                SemanticError::MalformedOutput(format!("malformed occurrence ref {entity}"))
            })?;
            let duplicate_index: u64 = duplicate_index.parse().map_err(|_| {
                SemanticError::MalformedOutput(format!("malformed occurrence ref {entity}"))
            })?;
            out.push(DeclKey::Occurrence {
                kind: kind.to_string(),
                content_hash: content_hash.to_string(),
                duplicate_index,
            });
        } else {
            return Err(SemanticError::MalformedOutput(format!(
                "unrecognized preserved entity ref {entity}"
            )));
        }
    }
    Ok(out)
}

/// Diagnostics from `extensions.domainforge.diagnostics` when present. Each
/// entry is `"<rule>: <message>"`; a missing separator keeps the whole string
/// as the message.
fn diagnostics_from(envelope: &Value) -> Result<Vec<Diagnostic>, SemanticError> {
    let mut out = Vec::new();
    if let Some(diags) = envelope
        .get("extensions")
        .and_then(|e| e.get("domainforge"))
        .and_then(|d| d.get("diagnostics"))
        .and_then(Value::as_array)
    {
        for item in diags {
            let text = item.as_str().ok_or_else(|| {
                SemanticError::MalformedOutput("diagnostics entry is not a string".into())
            })?;
            let (rule, message) = match text.split_once(':') {
                Some((rule, message)) => (rule.trim().to_string(), message.trim().to_string()),
                None => (String::new(), text.to_string()),
            };
            out.push(Diagnostic { rule, message });
        }
    }
    Ok(out)
}

fn string_field(value: &Value, field: &str, path: &str) -> Result<String, SemanticError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| SemanticError::MalformedOutput(format!("{path}.{field} is not a string")))
}

fn string_array(value: &Value, field: &str, path: &str) -> Result<Vec<String>, SemanticError> {
    value
        .get(field)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|i| {
                    i.as_str().map(str::to_string).ok_or_else(|| {
                        SemanticError::MalformedOutput(format!(
                            "{path}.{field} contains a non-string"
                        ))
                    })
                })
                .collect()
        })
        .transpose()
        .map(|v| v.unwrap_or_default())
}

/// CAS object path for a content address: `<cas_root>/<hex>`. CAS writes are
/// atomic (`write-temp → fsync → rename`, §11.3): a concurrent reader sees
/// either no object or the complete, verified object.
fn cas_object_path(root: &Path, hex: &str) -> PathBuf {
    root.join(hex)
}

fn persist_cas_object(root: &Path, hex: &str, bytes: &[u8]) -> Result<(), SemanticError> {
    std::fs::create_dir_all(root)?;
    let final_path = cas_object_path(root, hex);
    if final_path.exists() {
        // Object already present: it must match the content address. A
        // mismatch means the store is corrupt or the object is stale — fail
        // loudly, never overwrite content-addressed bytes.
        let existing = std::fs::read(&final_path)?;
        if existing.as_slice() != bytes {
            return Err(SemanticError::HashMismatch(format!(
                "CAS object {} exists but differs from the producer's D",
                final_path.display()
            )));
        }
        return Ok(());
    }
    let tmp = root.join(format!(".tmp-{hex}-{}", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    let f = std::fs::File::open(&tmp)?;
    f.sync_all()?;
    std::fs::rename(&tmp, &final_path)?;
    Ok(())
}
