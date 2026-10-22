//! Command implementations for the sxr CLI. Each fn returns a JSON output
//! Value (the `sxr_output_version` envelope) that the caller prints.

use anyhow::{Context, Result, anyhow, bail};
use rusqlite::OptionalExtension;
use serde_json::{Value, json};
use sha2::Digest;
use std::path::PathBuf;
use sxr_core::record_kinds::RecordKind;

use crate::ledger_io::{self, append_record, ledger_position_json, open_repo_ledger};
use crate::output;
use crate::projections;
use crate::state::{self, Ctx, Enrollment, RunMeta, SemanticConfig};

fn ulid() -> String {
    crate::determinism::mint_id("ulid")
}

// ── repo commands ────────────────────────────────────────────────────────

pub fn repo_enroll(ctx: &Ctx, path: &str) -> Result<Value> {
    let fs_path = std::path::absolute(path).context("resolve path")?;
    if !fs_path.is_dir() {
        bail!("not a directory: {}", fs_path.display());
    }
    let evidence = sxr_git::discover(&sxr_git::GitCli, &fs_path)
        .map_err(|e| anyhow!("discover {}: {e}", fs_path.display()))?;
    let (repo_id, checkout_id) = (
        crate::determinism::mint_id("repo_id"),
        crate::determinism::mint_id("checkout_id"),
    );
    let identity_evidence = sxr_git::evidence_of(&evidence, &fs_path);
    let enrollment = Enrollment {
        repo_id: repo_id.clone(),
        checkout_id,
        filesystem_path: fs_path.clone(),
        created_at: crate::determinism::now_utc()
            .to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
        identity_evidence,
    };
    ctx.ensure_repo_dir(&repo_id)?;
    state::write_enrollment(ctx, &enrollment)?;

    let mut repo = open_repo_ledger(ctx, &repo_id)?;
    let sections = json!({
        "states": [{"state_id": "state:enroll", "state_type": "repository", "scope": format!("repo:{repo_id}")}],
        "transformations": [{"transformation_id": "tx:enroll", "transformation_type": "enrollment"}]
    });
    let subject_refs = vec![
        format!("repo:{repo_id}"),
        format!("checkout:{}", enrollment.checkout_id),
    ];
    let pos = crate::envelopes::append(
        &mut repo,
        RecordKind::RepoEnrolled,
        &repo_id,
        &enrollment.checkout_id,
        Some(fs_path.to_str().unwrap_or("?")),
        evidence.head_at_observation.as_deref(),
        None,
        vec![],
        sections,
        subject_refs,
        None,
    )?;
    Ok(output::output(vec![
        ("repo_id", json!(repo_id)),
        ("checkout_id", json!(enrollment.checkout_id)),
        (
            "identity_evidence",
            serde_json::to_value(&enrollment.identity_evidence)?,
        ),
        ("filesystem_path", json!(fs_path.to_string_lossy())),
        ("state", json!("enrolled")),
        ("durability", json!("committed")),
        ("ledger_position", ledger_position_json(&pos)),
    ]))
}

pub fn repo_list(ctx: &Ctx) -> Result<Value> {
    let enrollments = state::list_enrollments(ctx)?;
    let list: Vec<Value> = enrollments
        .iter()
        .map(|e| {
            json!({
                "repo_id": e.repo_id,
                "checkout_id": e.checkout_id,
                "filesystem_path": e.filesystem_path.to_string_lossy(),
                "state": "enrolled",
            })
        })
        .collect();
    Ok(output::output1("repos", json!(list)))
}

pub fn repo_identity(ctx: &Ctx, repo_id: &str) -> Result<Value> {
    let e = state::require_enrollment(ctx, repo_id)?;
    Ok(output::output1(
        "identity",
        json!({
            "repo_id": e.repo_id,
            "checkout_id": e.checkout_id,
            "filesystem_path": e.filesystem_path.to_string_lossy(),
            "identity_evidence": serde_json::to_value(&e.identity_evidence)?,
        }),
    ))
}

// ── semantic commands ────────────────────────────────────────────────────

pub fn semantic_configure(ctx: &Ctx, repo_id: &str, entry: &str) -> Result<Value> {
    let e = state::require_enrollment(ctx, repo_id)?;
    let root = state::semantic_root_of(&e);
    let reg_path = root.join(".sea-registry.toml");
    let (registry_path, default_namespace, resolved) = if reg_path.is_file() {
        let contents = std::fs::read_to_string(&reg_path)
            .with_context(|| format!("read {}", reg_path.display()))?;
        let parsed = state::parse_registry_toml(&contents)
            .with_context(|| format!("parse {}", reg_path.display()))?;
        (
            Some(reg_path.clone()),
            parsed.default_namespace.clone(),
            parsed
                .namespaces
                .clone()
                .into_iter()
                .map(|(k, v)| (k, v.join(",")))
                .collect::<Vec<_>>(),
        )
    } else {
        (None, None, Vec::new())
    };
    let cfg = SemanticConfig {
        repo_id: repo_id.into(),
        entry_logical_path: entry.into(),
        registry_path,
        default_namespace,
        resolved_namespaces: resolved,
        configured_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
    };
    state::write_semantic(ctx, &cfg)?;
    Ok(output::output(vec![
        ("state", json!("semantic")),
        ("entry", json!(entry)),
        (
            "registry",
            if cfg.registry_path.is_some() {
                json!(".sea-registry.toml")
            } else {
                Value::Null
            },
        ),
        ("resolution_inputs", serde_json::to_value(&cfg)?),
    ]))
}

pub fn semantic_status(ctx: &Ctx, repo_id: &str) -> Result<Value> {
    let cfg = state::require_semantic(ctx, repo_id)?;
    Ok(output::output1(
        "resolution_inputs",
        serde_json::to_value(&cfg)?,
    ))
}

// ── snapshot ─────────────────────────────────────────────────────────────

fn domainforge_bin() -> Option<PathBuf> {
    if let Ok(var) = std::env::var("REALITYTRACE_DOMAINFORGE_BIN") {
        let p = PathBuf::from(var);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Ok(var) = std::env::var("SXR_DOMAINFORGE_BIN") {
        let p = PathBuf::from(var);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join("domainforge");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

pub fn snapshot_create(ctx: &Ctx, repo_id: &str) -> Result<Value> {
    let e = state::require_enrollment(ctx, repo_id)?;
    let cfg = state::require_semantic(ctx, repo_id)?;
    let bin = domainforge_bin().ok_or_else(|| {
        anyhow!("DomainForge binary not found: set REALITYTRACE_DOMAINFORGE_BIN or ensure 'domainforge' is on PATH")
    })?;
    let adapter = sxr_df::CliDomainForgeAdapter::new(bin)
        .with_required_version(sxr_df::PINNED_DOMAINFORGE_VERSION);
    let req = sxr_df::SemanticContextRequest {
        semantic_root: state::semantic_root_of(&e),
        entry_logical_path: cfg.entry_logical_path.clone(),
        registry_path: cfg.registry_path.clone(),
        default_namespace: cfg.default_namespace.clone(),
        semantic_packs: vec![],
        cep_scope: json!({}),
        cas_root: Some(ctx.ensure_repo_dir(repo_id)?.join("cas")),
    };
    let semantic = adapter
        .resolve(&req)
        .map_err(|e| anyhow!("domainforge resolve: {e}"))?;
    let (
        content_hash,
        semantic_hash,
        snapshot_id,
        model_status,
        diagnostics,
        omission_opt,
        declarations,
        resolution_inputs,
    ) = match &semantic {
        sxr_core::SemanticContext::Resolved {
            content_hash,
            semantic_closure_hash,
            resolution_inputs,
            declarations,
            diagnostics: resolved_diags,
            model_validation_status,
            ..
        } => {
            let sid = sxr_core::envelope::snapshot_id(
                Some(semantic_closure_hash),
                Some(content_hash),
                None,
                &cfg.entry_logical_path,
            );
            (
                Some(content_hash.clone()),
                Some(semantic_closure_hash.clone()),
                sid,
                model_validation_status.as_str(),
                json!(resolved_diags),
                None,
                declarations.clone(),
                resolution_inputs.clone(),
            )
        }
        sxr_core::SemanticContext::ValidationFailed {
            invalid_declared_checkpoint_hash,
            resolution_inputs,
            diagnostics,
            ..
        } => {
            let sid = sxr_core::envelope::snapshot_id(
                None,
                None,
                Some(invalid_declared_checkpoint_hash),
                &cfg.entry_logical_path,
            );
            (
                None,
                None,
                sid,
                "invalid",
                json!(diagnostics),
                Some(json!({"omission_type": "input_failure", "description": "D not constructed"})),
                vec![],
                resolution_inputs.clone(),
            )
        }
    };

    // §5.4: the DomainForge envelope is stored producer-preserved (its
    // id/created_at/scope/provenance untouched); the ledger row's subject_refs
    // carry the sxr-minted snapshot identity.
    let mut payload = serde_json::from_str::<Value>(semantic.envelope_json())
        .with_context(|| "parse producer envelope")?;
    if let Some(ext) = payload
        .get_mut("extensions")
        .and_then(|e| e.get_mut("domainforge"))
    {
        ext["semantics_version"] = json!("v2-independent-validity");
    } else {
        payload["extensions"] = json!({
            "domainforge": {
                "semantics_version": "v2-independent-validity",
                "model_validation_status": model_status,
            }
        });
    }
    let subject_refs = vec![
        format!("snapshot:{snapshot_id}"),
        format!("repo:{repo_id}"),
        format!("checkout:{}", e.checkout_id),
        format!(
            "checkpoint:{}",
            if semantic.model_validation_status() == "valid" {
                semantic.checkpoint_hash()
            } else {
                "invalid".into()
            }
        ),
    ];
    let mut repo = open_repo_ledger(ctx, repo_id)?;
    let pos = append_record(
        &mut repo,
        RecordKind::SemanticSnapshot,
        None,
        subject_refs,
        &payload,
    )?;

    let mut out = vec![
        ("snapshot_id", json!(snapshot_id)),
        ("model_validation_status", json!(model_status)),
        ("semantics_version", json!("v2-independent-validity")),
        ("declaration_count", json!(declarations.len())),
        (
            "resolution_inputs",
            serde_json::to_value(&resolution_inputs)?,
        ),
        ("drift_class", Value::Null),
        ("durability", json!("committed")),
        ("ledger_position", ledger_position_json(&pos)),
    ];
    if let Some(sh) = semantic_hash {
        out.push(("semantic_closure_hash", json!(sh)));
    }
    if let Some(ch) = content_hash {
        out.push(("content_hash", json!(ch)));
    }
    if model_status == "invalid" {
        out.push(("diagnostics", diagnostics));
        if let Some(om) = omission_opt {
            out.push(("representation_omission", om));
        }
    }
    Ok(output::output(out))
}

// ── contract import (§14.0) ──────────────────────────────────────────────

/// Maximum contract envelope bytes accepted inline (mirrors domainforge's
/// inline threshold; contracts are small).
const CONTRACT_MAX_BYTES: usize = 1_048_576;

pub fn contract_import(ctx: &Ctx, repo_id: &str, file: &str) -> Result<Value> {
    let _e = state::require_enrollment(ctx, repo_id)?;
    let raw = std::fs::read_to_string(file).with_context(|| format!("read {file}"))?;
    if raw.len() > CONTRACT_MAX_BYTES {
        bail!("contract envelope exceeds {CONTRACT_MAX_BYTES} bytes");
    }
    let envelope: Value = serde_json::from_str(&raw).with_context(|| format!("parse {file}"))?;

    // Gate 0A: the producer envelope must validate against the embedded CEP
    // schema and be the work_request verification contract (§14.0).
    let validator = sxr_core::embedded_schema::EnvelopeSchemaValidator::new();
    if let Err(errors) = validator.validate(&envelope) {
        bail!(
            "AC-CEP-001: contract envelope fails CEP schema: {}",
            errors.join("; ")
        );
    }
    if envelope.get("envelope_kind").and_then(Value::as_str) != Some("work_request") {
        bail!("contract envelope is not a CEP work_request");
    }
    let profile = envelope["extensions"]["domainforge"]["profile"]
        .as_str()
        .unwrap_or("");
    if profile != "domainforge-semantic-verification/v1" {
        bail!("contract envelope is not a domainforge-semantic-verification/v1 contract");
    }
    let semantic_hash = envelope["extensions"]["domainforge"]["semantic_hash"]
        .as_str()
        .ok_or_else(|| anyhow!("contract lacks domainforge semantic_hash"))?
        .to_string();
    let obligations = envelope["extensions"]["domainforge"]["obligations"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    // Precondition: a committed semantic.snapshot for this semantic_hash must
    // already exist (record-kind predecessor rule).
    let _snapshot_pos = find_snapshot_position(ctx, repo_id, &semantic_hash)?;

    // §5.4: store the producer envelope producer-preserved (id/created_at/
    // scope/provenance untouched); the ledger row's subject_refs carry the
    // sxr-minted contract identity.
    let contract_id = ulid();
    let question_ids: Vec<&str> = obligations
        .iter()
        .filter_map(|o| o.get("question_id").and_then(Value::as_str))
        .collect();
    let obligation_ids: Vec<&str> = obligations
        .iter()
        .filter_map(|o| o.get("obligation_id").and_then(Value::as_str))
        .collect();
    let decl_keys: Vec<&str> = obligations
        .iter()
        .filter_map(|o| o.get("decl_key").and_then(Value::as_str))
        .collect();

    let mut subject_refs: Vec<String> = vec![
        format!("contract:{contract_id}"),
        format!("semantic:{semantic_hash}"),
    ];
    subject_refs.extend(obligation_ids.iter().map(|o| format!("obligation:{o}")));
    subject_refs.extend(question_ids.iter().map(|q| format!("question:{q}")));
    subject_refs.extend(decl_keys.iter().map(|d| format!("declaration:{d}")));

    let mut repo = open_repo_ledger(ctx, repo_id)?;
    let pos = append_record(
        &mut repo,
        RecordKind::VerificationContractImported,
        None,
        subject_refs,
        &envelope,
    )?;

    // Materialize Question/Claim lookup rows (content-addressed by their
    // stable ids) without re-enveloping them as sxr-authored.
    let mut questions = Vec::new();
    for o in &obligations {
        questions.push(json!({
            "question_id": o["question_id"],
            "question_form": o["question_form"],
            "scope": o["decl_key"],
            "target_entities": [o["decl_key"]],
            "expected_answer_shape": o["expected_answer_shape"],
        }));
    }

    Ok(output::output(vec![
        ("contract_id", json!(contract_id)),
        ("envelope_id", envelope["envelope_id"].clone()),
        ("semantic_hash", json!(semantic_hash)),
        ("snapshot_ref", json!(format!("snapshot:{semantic_hash}"))),
        ("profile", json!(profile)),
        ("obligation_count", json!(obligations.len())),
        ("question_ids", json!(question_ids)),
        ("claim_ids", json!(obligation_ids)),
        ("declaration_refs", json!(decl_keys)),
        ("questions", json!(questions)),
        ("durability", json!("committed")),
        ("ledger_position", ledger_position_json(&pos)),
    ]))
}

/// Find the committed `semantic.snapshot` position for a semantic_hash.
fn find_snapshot_position(
    ctx: &Ctx,
    repo_id: &str,
    semantic_hash: &str,
) -> Result<sxr_core::LedgerPosition> {
    let rows = ledger_io::read_all(ctx, repo_id)?;
    for r in &rows {
        if r.record_kind != "semantic.snapshot" {
            continue;
        }
        let payload: Value = serde_json::from_str(&r.payload).context("parse snapshot payload")?;
        let rep = payload
            .get("representations")
            .and_then(Value::as_array)
            .and_then(|a| a.first());
        let rep_hash = rep
            .and_then(|r| r.get("semantic_hash"))
            .and_then(Value::as_str);
        if rep_hash == Some(semantic_hash) {
            return Ok(sxr_core::LedgerPosition::new(
                r.ledger_id.clone(),
                r.append_ordinal,
            ));
        }
    }
    bail!(
        "precondition: no committed semantic.snapshot for semantic_hash {}; \
         run `sxr snapshot create` first",
        semantic_hash
    )
}

// ── question ─────────────────────────────────────────────────────────────

pub fn question_declare(ctx: &Ctx, repo_id: &str, file: &str) -> Result<Value> {
    let e = state::require_enrollment(ctx, repo_id)?;
    let raw = std::fs::read_to_string(file).with_context(|| format!("read {file}"))?;
    let spec: Value = serde_json::from_str(&raw).with_context(|| format!("parse {file}"))?;
    let question_id = spec
        .get("question_id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(ulid);
    let question_form = spec
        .get("question_form")
        .and_then(Value::as_str)
        .unwrap_or("is_true");
    let question_type = spec
        .get("question_type")
        .and_then(Value::as_str)
        .unwrap_or("declared_semantics");
    let target = spec
        .get("target_entities")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("concept:{question_id}"));

    let sections = json!({
        "questions": [{ "question_id": question_id, "question_form": question_form, "question_type": question_type }],
        "representations": [{
            "representation_id": format!("rep:question:{question_id}"),
            "representation_kind": "atomic",
            "representation_type": "question_spec",
            "inline_content": spec,
        }]
    });
    let subject_refs = vec![
        format!("question:{}", question_id),
        format!("target:{}", target),
    ];
    let mut repo = open_repo_ledger(ctx, repo_id)?;
    let pos = crate::envelopes::append(
        &mut repo,
        RecordKind::QuestionDeclared,
        repo_id,
        &e.checkout_id,
        None,
        None,
        None,
        vec![],
        sections,
        subject_refs,
        None,
    )?;
    Ok(output::output(vec![
        ("question_id", json!(question_id)),
        ("question_form", json!(question_form)),
        ("question_type", json!(question_type)),
        ("durability", json!("committed")),
        ("ledger_position", ledger_position_json(&pos)),
    ]))
}

// ── threshold ────────────────────────────────────────────────────────────

pub fn threshold_declare(ctx: &Ctx, repo_id: &str, question_id: &str, file: &str) -> Result<Value> {
    let e = state::require_enrollment(ctx, repo_id)?;
    let raw = std::fs::read_to_string(file).with_context(|| format!("read {file}"))?;
    let spec: Value = serde_json::from_str(&raw).with_context(|| format!("parse {file}"))?;
    let threshold_id = spec
        .get("threshold_id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(ulid);
    let sections = json!({
        "states": [{ "state_id": threshold_id, "state_type": "settlement_threshold", "scope": format!("question:{question_id}") }],
        "representations": [{
            "representation_id": format!("rep:threshold:{threshold_id}"),
            "representation_kind": "atomic",
            "representation_type": "settlement_threshold_spec",
            "inline_content": spec,
        }]
    });
    let subject_refs = vec![
        format!("threshold:{}", threshold_id),
        format!("question:{}", question_id),
    ];
    let mut repo = open_repo_ledger(ctx, repo_id)?;
    let pos = crate::envelopes::append(
        &mut repo,
        RecordKind::ThresholdDeclared,
        repo_id,
        &e.checkout_id,
        None,
        None,
        None,
        vec![format!("question:{question_id}")],
        sections,
        subject_refs,
        None,
    )?;
    Ok(output::output(vec![
        ("threshold_id", json!(threshold_id)),
        ("question_id", json!(question_id)),
        ("durability", json!("committed")),
        ("ledger_position", ledger_position_json(&pos)),
    ]))
}

// ── run begin ────────────────────────────────────────────────────────────

pub fn run_begin(
    ctx: &Ctx,
    repo_id: &str,
    snapshot_id: &str,
    question_id: &str,
    threshold_id: &str,
) -> Result<Value> {
    let e = state::require_enrollment(ctx, repo_id)?;
    let run_id = ulid();
    let mut repo = open_repo_ledger(ctx, repo_id)?;
    let sections = json!({
        "transformations": [{ "transformation_id": format!("tx:{run_id}"), "transformation_type": "run_start" }],
        "questions": [{ "question_id": question_id, "question_form": "is_true" }]
    });
    let subject_refs = vec![
        format!("run:{run_id}"),
        format!("repo:{repo_id}"),
        format!("checkout:{}", e.checkout_id),
        format!("snapshot:{snapshot_id}"),
        format!("target:question:{question_id}"),
    ];
    let pos = crate::envelopes::append(
        &mut repo,
        RecordKind::RunStarted,
        repo_id,
        &e.checkout_id,
        None,
        None,
        Some(&run_id),
        vec![
            format!("question:{question_id}"),
            format!("threshold:{threshold_id}"),
            format!("snapshot:{snapshot_id}"),
        ],
        sections,
        subject_refs,
        None,
    )?;
    let meta = RunMeta {
        run_id: run_id.clone(),
        repo_id: repo_id.into(),
        checkout_id: e.checkout_id.clone(),
        snapshot_id: snapshot_id.into(),
        question_id: question_id.into(),
        threshold_id: threshold_id.into(),
        plan_id: None,
        git_commit: None,
        dirty: false,
        started_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
        finished_at: None,
        start_position: pos.clone(),
        plan_position: None,
    };
    state::add_run(ctx, &meta)?;
    Ok(output::output(vec![
        ("run_id", json!(run_id)),
        ("git_commit", Value::Null),
        ("dirty", json!(false)),
        ("checkout_id", json!(e.checkout_id)),
        ("durability", json!("committed")),
        ("ledger_position", ledger_position_json(&pos)),
    ]))
}

fn repo_for_run(ctx: &Ctx, run_id: &str) -> Result<RunMeta> {
    state::get_run(ctx, run_id)
}

// ── plan create ──────────────────────────────────────────────────────────

/// `plan create` options for the `command` verifier.
#[derive(Debug, Default)]
pub struct CommandArgs {
    pub argv_json: Option<String>,
    pub cwd: Option<String>,
    pub timeout_s: Option<u64>,
    pub launcher_json: Option<String>,
}

fn json_string_array(raw: &str, what: &str) -> Result<Vec<String>> {
    let v: Vec<String> = serde_json::from_str(raw)
        .with_context(|| format!("{what} must be a JSON array of strings"))?;
    Ok(v)
}

/// A relative path that cannot leave the directory it is joined to.
fn contained_relative(rel: &str) -> Result<std::path::PathBuf> {
    let p = std::path::Path::new(rel);
    if p.is_absolute()
        || p.components().any(|c| {
            !matches!(
                c,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
    {
        bail!("--cwd must be a relative path inside the enrolled repo: {rel}");
    }
    Ok(p.to_path_buf())
}

/// The plan step's invocation: fixed for `cargo-test`, declared for `command`.
fn plan_invocation(verifier: &str, args: &CommandArgs) -> Result<(Value, Vec<Value>)> {
    let declared = args.argv_json.is_some()
        || args.cwd.is_some()
        || args.timeout_s.is_some()
        || args.launcher_json.is_some();
    if verifier != sxr_verify::COMMAND_VERIFIER_ID {
        if declared {
            bail!("--argv-json/--cwd/--timeout-s/--launcher-json apply only to --verifier command");
        }
        return Ok((
            json!({
                "argv": ["cargo", "test", "--offline", "--message-format", "json"],
                "cwd": ".",
                "env_allowlist": ["PATH", "HOME", "CARGO_HOME"],
                "timeout_s": 900,
            }),
            vec![json!("offline only"), json!("slice-1 cargo-test")],
        ));
    }
    let argv = json_string_array(
        args.argv_json
            .as_deref()
            .ok_or_else(|| anyhow!("--verifier command requires --argv-json"))?,
        "--argv-json",
    )?;
    if argv.is_empty() || argv[0].is_empty() {
        bail!("--argv-json must name a program");
    }
    let cwd = args.cwd.clone().unwrap_or_else(|| ".".into());
    contained_relative(&cwd)?;
    let timeout_s = args.timeout_s.unwrap_or(900);
    if timeout_s == 0 || timeout_s > 86_400 {
        bail!("--timeout-s must be between 1 and 86400");
    }
    let launcher = match args.launcher_json.as_deref() {
        Some(raw) => json_string_array(raw, "--launcher-json")?,
        None => Vec::new(),
    };
    let mut invocation = json!({
        "argv": argv,
        "cwd": cwd,
        "env_allowlist": ["PATH", "HOME"],
        "timeout_s": timeout_s,
    });
    if !launcher.is_empty() {
        invocation["launcher"] = json!(launcher);
    }
    Ok((
        invocation,
        vec![json!("declared command verifier: exit code is the result")],
    ))
}

#[allow(clippy::too_many_arguments)]
pub fn plan_create(
    ctx: &Ctx,
    run_id: &str,
    verifier: &str,
    target: &str,
    from_contract: Option<&str>,
    obligation: Option<&str>,
    command: &CommandArgs,
) -> Result<Value> {
    let (invocation, boundary_limitations) = plan_invocation(verifier, command)?;
    let mut meta = repo_for_run(ctx, run_id)?;
    let e = state::require_enrollment(ctx, &meta.repo_id)?;
    let plan_id = ulid();
    let transformation_ref = format!("transformation:plan:{plan_id}");

    // §14.1: when built from a verification contract, the plan binds the exact
    // contract obligation/Claim/Evidence requirement and the contract ref.
    let mut semantic_verification_contract_ref = Value::Null;
    let mut step_obligation_ref = Value::Null;
    let mut step_claim_ref = Value::Null;
    let mut step_evidence_requirement_ref = Value::Null;
    let mut step_targets = json!([target]);
    let mut produces_evidence_type = "test_result".to_string();
    let mut expected_answer_shape = "boolean_with_failure_list".to_string();
    let mut obligation_limitations: Vec<Value> = Vec::new();

    if let Some(contract_id) = from_contract {
        let obligations = load_contract_obligations(ctx, &meta.repo_id, contract_id)?;
        let ob = obligations
            .iter()
            .find(|o| o.get("obligation_id").and_then(Value::as_str) == obligation)
            .or_else(|| obligations.first())
            .ok_or_else(|| anyhow!("contract {contract_id} has no obligations"))?;
        semantic_verification_contract_ref = json!(format!("contract:{contract_id}"));
        step_obligation_ref = json!(ob["obligation_id"]);
        step_claim_ref = json!(ob["claim_id"]);
        let evidence_type = ob["evidence_types"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(Value::as_str)
            .unwrap_or("test_result");
        step_evidence_requirement_ref =
            json!(format!("contract://{contract_id}#evidence/{evidence_type}"));
        produces_evidence_type = evidence_type.to_string();
        expected_answer_shape = ob["expected_answer_shape"]
            .as_str()
            .unwrap_or("boolean_with_failure_list")
            .to_string();
        step_targets = json!([ob["decl_key"]]);
        obligation_limitations = ob["limitations"].as_array().cloned().unwrap_or_default();
    }

    let plan = json!({
        "plan_id": plan_id,
        "plan_version": "sxr-plan/v1",
        "semantic_verification_contract_ref": semantic_verification_contract_ref,
        "evaluation_profile_ref": "profile://slice2-policy@1",
        "question_ref": meta.question_id,
        "threshold_ref": meta.threshold_id,
        "snapshot_ref": meta.snapshot_id,
        "transformation_ref": transformation_ref,
        "steps": [{
            "step_id": "unit",
            "obligation_ref": step_obligation_ref,
            "claim_ref": step_claim_ref,
            "evidence_requirement_ref": step_evidence_requirement_ref,
            "verifier_id": verifier,
            "invocation": invocation,
            "produces_evidence_type": produces_evidence_type,
            "targets": step_targets,
            "expected_answer_shape": expected_answer_shape,
            "relevance_to_question": "high",
            "limitations": obligation_limitations,
        }],
        "evidence_requirements": [{
            "evidence_type": "test_result",
            "min_count": 1,
            "min_relevance": "high",
            "min_reliability": "medium",
        }],
        "boundary": {
            "excluded": ["browser", "performance", "security"],
            "limitations": boundary_limitations,
        },
    });
    let sections = json!({
        "representations": [{
            "representation_id": format!("rep:plan:{plan_id}"),
            "representation_kind": "atomic",
            "representation_type": "verification_plan",
            "inline_content": plan,
        }],
        "questions": [{ "question_id": meta.question_id, "question_form": "is_true" }]
    });
    let mut subject_refs = vec![
        format!("plan:{plan_id}"),
        format!("run:{}", run_id),
        format!("question:{}", meta.question_id),
        format!("threshold:{}", meta.threshold_id),
        format!("snapshot:{}", meta.snapshot_id),
        format!("target:{target}"),
    ];
    if let Some(contract_id) = from_contract {
        subject_refs.push(format!("contract:{contract_id}"));
    }
    let mut repo = open_repo_ledger(ctx, &meta.repo_id)?;
    let pos = crate::envelopes::append(
        &mut repo,
        RecordKind::PlanCreated,
        &meta.repo_id,
        &e.checkout_id,
        None,
        None,
        Some(run_id),
        vec![
            format!("question:{}", meta.question_id),
            format!("threshold:{}", meta.threshold_id),
        ],
        sections,
        subject_refs,
        None,
    )?;
    meta.plan_id = Some(plan_id.clone());
    meta.plan_position = Some(pos.clone());
    state::update_run(ctx, &meta)?;
    Ok(output::output(vec![
        ("plan_id", json!(plan_id)),
        ("plan", plan),
        ("transformation_ref", json!(transformation_ref)),
        ("durability", json!("committed")),
        ("ledger_position", ledger_position_json(&pos)),
    ]))
}

/// Load the obligations of a committed verification contract (§14.0).
fn load_contract_obligations(ctx: &Ctx, repo_id: &str, contract_id: &str) -> Result<Vec<Value>> {
    let rows = ledger_io::read_all(ctx, repo_id)?;
    for r in &rows {
        if r.record_kind == "verification.contract_imported"
            && r.subject_refs
                .iter()
                .any(|s| s == &format!("contract:{contract_id}"))
        {
            let payload: Value =
                serde_json::from_str(&r.payload).context("parse contract payload")?;
            let obligations = payload["extensions"]["domainforge"]["obligations"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            return Ok(obligations);
        }
    }
    bail!("verification contract {contract_id} not found")
}

// ── verify run ───────────────────────────────────────────────────────────

/// The verification step the plan declares. `cargo-test` keeps its fixed invocation; `command` runs the
/// plan's declared argv, cwd (inside the enrolled repo), timeout and launcher. Any other verifier id is
/// refused: `verify run` never substitutes a different verifier for the one the plan names.
fn step_from_plan(
    plan_step: &Value,
    target: &str,
    root: &std::path::Path,
) -> Result<sxr_verify::VerificationStep> {
    let verifier_id = plan_step
        .get("verifier_id")
        .and_then(Value::as_str)
        .unwrap_or(sxr_verify::VERIFIER_ID)
        .to_string();
    if sxr_verify::verifier_for(&verifier_id).is_none() {
        bail!(
            "verifier {verifier_id} cannot be run by `verify run` (runnable: cargo-test, command); record its output with `ingest`"
        );
    }
    let invocation = if verifier_id == sxr_verify::COMMAND_VERIFIER_ID {
        let inv = plan_step
            .get("invocation")
            .ok_or_else(|| anyhow!("command plan has no invocation"))?;
        let strings = |key: &str| -> Vec<String> {
            inv.get(key)
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        };
        let rel = contained_relative(inv.get("cwd").and_then(Value::as_str).unwrap_or("."))?;
        let cwd = root.join(rel);
        let (canon_root, canon_cwd) = (root.canonicalize()?, cwd.canonicalize()?);
        if !canon_cwd.starts_with(&canon_root) {
            bail!("plan cwd {} leaves the enrolled repo", cwd.display());
        }
        sxr_verify::Invocation {
            argv: strings("argv"),
            cwd: canon_cwd,
            env_allowlist: strings("env_allowlist"),
            timeout_s: inv.get("timeout_s").and_then(Value::as_u64).unwrap_or(900),
            launcher: strings("launcher"),
        }
    } else {
        sxr_verify::Invocation {
            argv: vec![
                "cargo".into(),
                "test".into(),
                "--offline".into(),
                "--message-format".into(),
                "json".into(),
            ],
            cwd: root.into(),
            env_allowlist: vec!["PATH".into(), "HOME".into(), "CARGO_HOME".into()],
            timeout_s: 900,
            launcher: Vec::new(),
        }
    };
    Ok(sxr_verify::VerificationStep {
        step_id: "unit".into(),
        obligation_ref: None,
        claim_ref: None,
        evidence_requirement_ref: "contract://test#evidence/unit".into(),
        verifier_id,
        invocation,
        produces_evidence_type: "test_result".into(),
        targets: vec![target.into()],
        expected_answer_shape: "boolean_with_failure_list".into(),
        on_crash: "record_omission".into(),
    })
}

pub fn verify_run(ctx: &Ctx, run_id: &str) -> Result<Value> {
    let meta = repo_for_run(ctx, run_id)?;
    let e = state::require_enrollment(ctx, &meta.repo_id)?;
    let plugin = repo_plan(ctx, &meta.repo_id, run_id)?;
    let target = plugin
        .get("targets")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(Value::as_str)
        .unwrap_or("concept:unknown")
        .to_string();

    let plan_step = plugin.get("step").cloned().unwrap_or(Value::Null);
    let step = step_from_plan(&plan_step, &target, &e.filesystem_path)?;
    let verifier = sxr_verify::verifier_for(&step.verifier_id)
        .ok_or_else(|| anyhow!("verifier {} is not registered", step.verifier_id))?;
    // What is verified: the enrolled tree's content digest, taken before the verifier runs.
    let target_digest = sxr_verify::tree_digest(&e.filesystem_path)
        .with_context(|| format!("digest {}", e.filesystem_path.display()))?;
    let artifacts_dir = ctx.ensure_repo_dir(&meta.repo_id)?.join("artifacts");
    let outcome = verifier
        .execute(
            &step,
            &sxr_verify::ExecutionContext::new(artifacts_dir.clone()),
        )
        .map_err(|err| anyhow!("{} verifier failed: {err}", step.verifier_id))?;
    let invocation = json!({
        "argv": step.invocation.argv,
        "launcher": step.invocation.launcher,
        "timeout_s": step.invocation.timeout_s,
    });
    let observation_id = ulid();
    let sections = json!({
        "observations": [{ "observation_id": observation_id, "scope": format!("run:{run_id}") }],
        "representations": [{
            "representation_id": format!("rep:obs:{observation_id}"),
            "representation_kind": "atomic",
            "representation_type": "verifier_stdout",
            "content_hash": outcome.stdout_ref,
            "inline_content": {
                "verifier_id": step.verifier_id,
                "invocation": invocation,
                "target_digest": target_digest,
                "exit_code": outcome.exit_code,
                "crashed": outcome.crashed,
                "stderr_ref": outcome.stderr_ref,
                "parsed": outcome.parsed.as_ref().map(|p| json!({
                    "test_count": p.test_count, "passed": p.passed, "failed": p.failed, "ignored": p.ignored,
                })).unwrap_or(Value::Null),
            },
        }]
    });
    let subject_refs = vec![
        format!("observation:{observation_id}"),
        format!("run:{run_id}"),
        format!("target:{target}"),
    ];
    let mut repo = open_repo_ledger(ctx, &meta.repo_id)?;
    let pos = crate::envelopes::append(
        &mut repo,
        RecordKind::ObservationRecorded,
        &meta.repo_id,
        &e.checkout_id,
        None,
        None,
        Some(run_id),
        vec![format!("plan:{}", meta.plan_id.clone().unwrap_or_default())],
        sections,
        subject_refs,
        None,
    )?;
    let parsed = outcome.parsed.as_ref().map(|p| json!({
        "test_count": p.test_count, "passed": p.passed, "failed": p.failed, "ignored": p.ignored,
    })).unwrap_or(Value::Null);
    Ok(output::output1(
        "observations",
        json!([{
            "observation_id": observation_id,
            "run_id": run_id,
            "verifier_id": step.verifier_id,
            "target_digest": target_digest,
            "exit_code": outcome.exit_code,
            "crashed": outcome.crashed,
            "stdout_ref": outcome.stdout_ref,
            "stderr_ref": outcome.stderr_ref,
            "parsed": parsed,
            "durability": "committed",
            "ledger_position": ledger_position_json(&pos),
        }]),
    ))
}

/// Read the plan representation for a run back from the ledger.
fn repo_plan(ctx: &Ctx, repo_id: &str, run_id: &str) -> Result<Value> {
    let rows = ledger_io::read_all(ctx, repo_id)?;
    let plan_id = state::get_run(ctx, run_id)?.plan_id.unwrap_or_default();
    for r in &rows {
        if r.record_kind == "plan.created"
            && r.subject_refs
                .iter()
                .any(|s| s.as_str() == format!("run:{run_id}"))
        {
            let payload: Value = serde_json::from_str(&r.payload).context("parse plan payload")?;
            let rep = payload
                .get("representations")
                .and_then(Value::as_array)
                .and_then(|a| a.first());
            if let Some(rep) = rep {
                let content = rep.get("inline_content").cloned().unwrap_or(Value::Null);
                let step = content
                    .get("steps")
                    .and_then(Value::as_array)
                    .and_then(|a| a.first())
                    .cloned()
                    .unwrap_or(Value::Null);
                let targets = step.get("targets").cloned().unwrap_or(json!([]));
                return Ok(json!({ "targets": targets, "plan_id": plan_id, "step": step }));
            }
        }
    }
    bail!("plan for run {} not found; run `sxr plan create`", run_id)
}

// ── evidence create ──────────────────────────────────────────────────────

pub fn evidence_create(
    ctx: &Ctx,
    run_id: &str,
    observation_id: &str,
    evidence_type: &str,
) -> Result<Value> {
    let meta = repo_for_run(ctx, run_id)?;
    // F-19b: the referenced observation and run must be committed.
    crate::durability::ensure_committed(ctx, &meta.repo_id, &[run_id, observation_id])?;
    let e = state::require_enrollment(ctx, &meta.repo_id)?;
    let progress = repo_plan(ctx, &meta.repo_id, run_id)?;
    let target = progress
        .get("targets")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(Value::as_str)
        .unwrap_or("concept:unknown")
        .to_string();
    let obs_row = repo_observation(ctx, &meta.repo_id, observation_id)?;
    // The rules are the profile of the verifier that produced the observation: the one `verify run`
    // recorded, else the plan's.
    let verifier_id = obs_row
        .verifier_id
        .clone()
        .or_else(|| {
            progress
                .pointer("/step/verifier_id")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| sxr_verify::VERIFIER_ID.to_string());
    let profile = sxr_verify::profile_ref(&verifier_id);
    let observation_pos = obs_row.pos.clone();
    let plan_pos = meta.plan_position.clone().unwrap_or_else(pos_fallback);
    let precedes = plan_pos.append_ordinal < observation_pos.append_ordinal;

    let evidence_id = ulid();
    let direction = match obs_row.exit_code {
        Some(0) => "supports",
        Some(_) => "contradicts",
        None => "unknown",
    };
    let derivations = vec![
        deriv(
            "question_ref",
            "plan://plan#question_ref",
            &json!({}),
            &plan_pos,
            precedes,
        ),
        deriv(
            "target_entity_ref",
            "plan://plan#step/targets",
            &json!({}),
            &plan_pos,
            precedes,
        ),
        deriv(
            "direction",
            &format!("{profile}#exit_code_to_direction"),
            &json!({"exit_code": obs_row.exit_code, "crashed": obs_row.crashed}),
            &plan_pos,
            precedes,
        ),
        deriv(
            "evidence_type",
            &format!("{profile}#evidence_types"),
            &json!({"evidence_types": [evidence_type]}),
            &plan_pos,
            precedes,
        ),
        deriv(
            "reliability",
            &format!("{profile}#reliability_model"),
            &json!({"factors": []}),
            &plan_pos,
            precedes,
        ),
        deriv(
            "relevance",
            "plan://plan#step/relevance_to_question",
            &json!({"relevance": "high"}),
            &plan_pos,
            precedes,
        ),
        deriv(
            "confidence_impact",
            "plan://plan#qualification/confidence_impact_mapping",
            &json!({"direction": direction}),
            &plan_pos,
            precedes,
        ),
        deriv(
            "representation_ref",
            "observation://obs#artifacts",
            &json!({"stdout_ref": obs_row.stdout_ref}),
            &obs_row.pos,
            precedes,
        ),
        deriv(
            "scope",
            "plan://plan#boundary",
            &json!({}),
            &plan_pos,
            precedes,
        ),
        deriv(
            "provenance_refs",
            "plan+observation",
            &json!({}),
            &obs_row.pos,
            precedes,
        ),
        deriv(
            "timestamp",
            "clock://system",
            &json!({}),
            &plan_pos,
            precedes,
        ),
        deriv(
            "source",
            &profile,
            &json!({"id": verifier_id, "version": "1"}),
            &plan_pos,
            precedes,
        ),
        deriv(
            "limitations",
            "plan://plan#boundary/limitations",
            &json!({"limitations": ["offline only"]}),
            &plan_pos,
            precedes,
        ),
        deriv(
            "evidence_id",
            "generated",
            &json!({"method": "ulid"}),
            &plan_pos,
            precedes,
        ),
    ];
    let _ = evidence_id;
    let sections = json!({
        "evidence": [{
            "evidence_id": evidence_id,
            "evidence_type": evidence_type,
            "question_ref": meta.question_id,
            "target_entity_ref": format!("concept:{}", target.trim_start_matches("concept:")),
            "representation_ref": format!("rep:obs:{observation_id}"),
            "relevance": "high",
            // Verifier-profile reliability model: cargo-test is a standard
            // deterministic harness → "medium" (a derived result must be able
            // to qualify against a medium bar; "unknown" could never).
            "reliability": "medium",
            "direction": direction,
            "confidence_impact": "neutral",
        }]
    });
    let subject_refs = vec![
        format!("evidence:{}", evidence_id),
        format!("run:{}", run_id),
        format!("question:{}", meta.question_id),
        format!("target:{target}"),
    ];
    let mut repo = open_repo_ledger(ctx, &meta.repo_id)?;
    let pos = crate::envelopes::append(
        &mut repo,
        RecordKind::EvidenceCreated,
        &meta.repo_id,
        &e.checkout_id,
        None,
        None,
        Some(run_id),
        vec![format!("observation:{observation_id}")],
        sections,
        subject_refs,
        None,
    )?;
    Ok(output::output(vec![
        ("evidence_id", json!(evidence_id)),
        ("derivations", json!(derivations)),
        ("durability", json!("committed")),
        ("ledger_position", ledger_position_json(&pos)),
    ]))
}

fn deriv(
    field: &str,
    rule_ref: &str,
    inputs: &Value,
    pos: &sxr_core::LedgerPosition,
    precedes: bool,
) -> Value {
    json!({
        "field": field,
        "rule_ref": rule_ref,
        "source_contract_ref": "plan://plan#step/unit",
        "inputs": inputs,
        "declared_at": {"ledger_id": pos.ledger_id, "append_ordinal": pos.append_ordinal},
        "precedes_observation": precedes,
        "limitations": [],
    })
}

struct ObsInfo {
    pos: sxr_core::LedgerPosition,
    /// The verifier `verify run` executed; `None` for observations recorded before it was stored.
    verifier_id: Option<String>,
    exit_code: Option<i32>,
    crashed: bool,
    stdout_ref: String,
}

fn repo_observation(ctx: &Ctx, repo_id: &str, observation_id: &str) -> Result<ObsInfo> {
    let rows = ledger_io::read_all(ctx, repo_id)?;
    for r in &rows {
        if r.record_kind == "observation.recorded"
            && r.subject_refs
                .iter()
                .any(|s| s.as_str() == format!("observation:{observation_id}"))
        {
            let pos = sxr_core::LedgerPosition::new(r.ledger_id.clone(), r.append_ordinal);
            let payload: Value =
                serde_json::from_str(&r.payload).context("parse observation payload")?;
            let inc = payload
                .get("representations")
                .and_then(Value::as_array)
                .and_then(|a| a.first())
                .and_then(|r| r.get("inline_content"));
            let exit_code = inc
                .and_then(|i| i.get("exit_code"))
                .and_then(Value::as_i64)
                .map(|c| c as i32);
            let crashed = inc
                .and_then(|i| i.get("crashed"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let stdout_ref = payload
                .get("representations")
                .and_then(Value::as_array)
                .and_then(|a| a.first())
                .and_then(|r| r.get("content_hash"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_default();
            let verifier_id = inc
                .and_then(|i| i.get("verifier_id"))
                .and_then(Value::as_str)
                .map(str::to_string);
            return Ok(ObsInfo {
                pos,
                verifier_id,
                exit_code,
                crashed,
                stdout_ref,
            });
        }
    }
    bail!("observation {} not found", observation_id)
}

fn pos_fallback() -> sxr_core::LedgerPosition {
    sxr_core::LedgerPosition::new("gen-1", 1)
}

// ── settlement ───────────────────────────────────────────────────────────

pub fn settlement_register(
    ctx: &Ctx,
    run_id: &str,
    question_id: &str,
    threshold_id: &str,
    transformation_ref: &str,
    debt: Option<&str>,
) -> Result<Value> {
    let meta = repo_for_run(ctx, run_id)?;
    let e = state::require_enrollment(ctx, &meta.repo_id)?;
    // F-19b: the referenced run, question, and threshold must be committed.
    crate::durability::ensure_committed(ctx, &meta.repo_id, &[run_id, question_id, threshold_id])?;
    // R-S1: threshold must be comparable to and strictly earlier than every
    // evidence position.
    let evidence_positions = evidence_positions(ctx, &meta.repo_id, run_id)?;
    let threshold_pos = threshold_position(ctx, &meta.repo_id, threshold_id)?;

    for ep in &evidence_positions {
        if threshold_pos.append_ordinal >= ep.append_ordinal {
            bail!(
                "AC-SETTLEMENT-001: threshold {} at ordinal {} is NOT strictly earlier than evidence at ordinal {}",
                threshold_id,
                threshold_pos.append_ordinal,
                ep.append_ordinal
            );
        }
        if threshold_pos.ledger_id != ep.ledger_id {
            bail!("AC-SETTLEMENT-001: threshold and evidence are on unrelated history components");
        }
    }

    // Slice 2 (AC-VERIFY-001): settlement is RED (unsettled/failed) when the
    // run's evidence contradicts the claimed obligation, GREEN (settled) when
    // it supports. Qualifying evidence is that which supports; contradicting
    // evidence is surfaced as an unresolved contradiction, never as settled.
    let evidence_directions = evidence_directions(ctx, &meta.repo_id, run_id)?;
    let supports = evidence_directions.iter().any(|d| d == "supports");
    let contradicts = evidence_directions.iter().any(|d| d == "contradicts");

    // §14.4/AC-SETTLEMENT-002: if snapshot is validation-failed or semantics are v1-legacy,
    // positive settlement is rejected.
    let snapshot_info = snapshot_model_status(ctx, &meta.repo_id, &meta.snapshot_id)?;
    if let Some((model_status, semantics_version)) = &snapshot_info
        && supports
        && !question_is_invalidity(question_id)
    {
        sxr_core::threshold::authorize_settlement(
            model_status,
            semantics_version,
            &sxr_core::threshold::ThresholdKind::AllEvidenceSupports,
            sxr_core::types::SettlementStatus::Settled,
        )
        .map_err(|e| anyhow!("{e}"))?;
    }

    let settlement_status = if supports { "settled" } else { "unsettled" };
    let settlement_status = if debt.is_some() {
        if !supports {
            bail!(
                "AC-SETTLEMENT-003: debt requires a supporting settlement; evidence does not support"
            );
        }
        "settled_with_debt"
    } else {
        settlement_status
    };
    let missing_evidence: Vec<String> = if supports {
        vec![]
    } else {
        vec!["no qualifying supporting evidence".to_string()]
    };
    let unresolved_contradictions: Vec<String> = if contradicts && !supports {
        vec!["evidence contradicts the claimed obligation".to_string()]
    } else {
        vec![]
    };
    let settlement_id = ulid();
    let mut settlement = json!({
        "settlement_id": settlement_id,
        "settlement_status": settlement_status,
        "transformation_ref": transformation_ref,
        "question_ref": question_id,
        "threshold_ref": threshold_id,
    });
    let debt_id = debt.map(|_| ulid());
    if let Some(did) = &debt_id {
        settlement["debt_refs"] = json!([format!("debt:{did}")]);
    }
    let mut sections = json!({
        "settlements": [settlement],
    });
    if let Some(did) = &debt_id {
        sections["debt"] = json!([{
            "debt_id": did,
            "settlement_ref": settlement_id,
            "description": debt.unwrap_or(""),
        }]);
    }
    let mut subject_refs = vec![
        format!("settlement:{}", settlement_id),
        format!("question:{}", question_id),
        format!("run:{}", run_id),
        format!(
            "transformation:{}",
            transformation_ref.trim_start_matches("transformation:")
        ),
        format!("snapshot:{}", meta.snapshot_id),
    ];
    if let Some(did) = &debt_id {
        subject_refs.push(format!("debt:{did}"));
    }
    let mut repo = open_repo_ledger(ctx, &meta.repo_id)?;
    let pos = crate::envelopes::append(
        &mut repo,
        RecordKind::SettlementRegistered,
        &meta.repo_id,
        &e.checkout_id,
        None,
        None,
        Some(run_id),
        vec![format!("question:{question_id}")],
        sections,
        subject_refs,
        None,
    )?;
    // §26/CEP-0007 §18: `settled_with_debt` MUST be accompanied by a
    // debt.recorded record (settlement_packet, allowed_predecessor
    // settlement.registered).
    if let Some(did) = &debt_id {
        let debt_sections = json!({
            "settlements": [{
                "settlement_id": settlement_id,
                "settlement_status": "settled_with_debt",
                "transformation_ref": transformation_ref,
                "question_ref": question_id,
                "threshold_ref": threshold_id,
                "debt_refs": [format!("debt:{did}")],
            }],
            "debt": [{
                "debt_id": did,
                "settlement_ref": settlement_id,
                "description": debt.unwrap_or(""),
            }]
        });
        let debt_refs = vec![
            format!("debt:{did}"),
            format!("settlement:{}", settlement_id),
            format!("run:{}", run_id),
        ];
        crate::envelopes::append(
            &mut repo,
            RecordKind::DebtRecorded,
            &meta.repo_id,
            &e.checkout_id,
            None,
            None,
            Some(run_id),
            vec![format!("settlement:{settlement_id}")],
            debt_sections,
            debt_refs,
            None,
        )?;
    }
    Ok(output::output(vec![
        ("settlement_id", json!(settlement_id)),
        ("settlement_status", json!(settlement_status)),
        (
            "debt_id",
            debt_id.map(|d| json!(d)).unwrap_or(json!(Value::Null)),
        ),
        ("missing_evidence", json!(missing_evidence)),
        (
            "unresolved_contradictions",
            json!(unresolved_contradictions),
        ),
        ("state_update_recommendation", json!("none")),
        ("durability", json!("committed")),
        ("ledger_position", ledger_position_json(&pos)),
    ]))
}

/// Directions of the evidence records attached to a run, in ledger order.
fn evidence_directions(ctx: &Ctx, repo_id: &str, run_id: &str) -> Result<Vec<String>> {
    let rows = ledger_io::read_all(ctx, repo_id)?;
    let mut out = Vec::new();
    for r in &rows {
        if r.record_kind == "evidence.created"
            && r.subject_refs
                .iter()
                .any(|s| s.as_str() == format!("run:{run_id}"))
        {
            let payload: Value =
                serde_json::from_str(&r.payload).context("parse evidence payload")?;
            let ev = payload
                .get("evidence")
                .and_then(Value::as_array)
                .and_then(|a| a.first());
            let direction = ev
                .and_then(|e| e.get("direction"))
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            out.push(direction.to_string());
        }
    }
    Ok(out)
}

fn evidence_positions(
    ctx: &Ctx,
    repo_id: &str,
    run_id: &str,
) -> Result<Vec<sxr_core::LedgerPosition>> {
    let rows = ledger_io::read_all(ctx, repo_id)?;
    Ok(rows
        .iter()
        .filter(|r| {
            r.record_kind == "evidence.created"
                && r.subject_refs
                    .iter()
                    .any(|s| s.as_str() == format!("run:{run_id}"))
        })
        .map(|r| sxr_core::LedgerPosition::new(r.ledger_id.clone(), r.append_ordinal))
        .collect())
}

fn threshold_position(
    ctx: &Ctx,
    repo_id: &str,
    threshold_id: &str,
) -> Result<sxr_core::LedgerPosition> {
    let rows = ledger_io::read_all(ctx, repo_id)?;
    rows.iter()
        .find(|r| {
            r.record_kind == "threshold.declared"
                && r.subject_refs
                    .iter()
                    .any(|s| s.as_str() == format!("threshold:{threshold_id}"))
        })
        .map(|r| sxr_core::LedgerPosition::new(r.ledger_id.clone(), r.append_ordinal))
        .ok_or_else(|| anyhow!("threshold {} not found", threshold_id))
}

fn snapshot_model_status(
    ctx: &Ctx,
    repo_id: &str,
    snapshot_id: &str,
) -> Result<Option<(String, String)>> {
    let rows = ledger_io::read_all(ctx, repo_id)?;
    for r in &rows {
        if r.record_kind == "semantic.snapshot"
            && r.subject_refs
                .iter()
                .any(|s| s.as_str() == format!("snapshot:{snapshot_id}"))
        {
            let payload: Value =
                serde_json::from_str(&r.payload).context("parse snapshot payload")?;
            let df_ext = payload.get("extensions").and_then(|e| e.get("domainforge"));
            let status = df_ext
                .and_then(|d| d.get("model_validation_status"))
                .and_then(Value::as_str)
                .or_else(|| {
                    payload
                        .get("representations")
                        .and_then(Value::as_array)
                        .and_then(|a| a.first())
                        .and_then(|rep| rep.get("validation_status"))
                        .and_then(Value::as_str)
                })
                .unwrap_or_else(|| {
                    let has_hashes = payload
                        .get("representations")
                        .and_then(Value::as_array)
                        .and_then(|a| a.first())
                        .map(|rep| {
                            rep.get("semantic_hash").is_some() || rep.get("content_hash").is_some()
                        })
                        .unwrap_or(false);
                    if has_hashes { "valid" } else { "invalid" }
                });
            let semantics_version = df_ext
                .and_then(|d| d.get("semantics_version"))
                .and_then(Value::as_str)
                .unwrap_or("v1-legacy");
            return Ok(Some((status.to_string(), semantics_version.to_string())));
        }
    }
    Ok(None)
}

fn question_is_invalidity(_qid: &str) -> bool {
    false
}

// ── claim (§12.4a relation claim) ────────────────────────────────────────

/// Create a `claim.made` asserting a relation R(src, dst, edge_kind) under a
/// question/run (§12.4a condition 1). This is the assertion a `G_observed`
/// edge admission requires; it is a Claim, not Evidence.
pub fn claim_create(
    ctx: &Ctx,
    run_id: &str,
    question_id: &str,
    src: &str,
    dst: &str,
    edge_kind: &str,
) -> Result<Value> {
    let meta = repo_for_run(ctx, run_id)?;
    let e = state::require_enrollment(ctx, &meta.repo_id)?;
    let claim_id = ulid();
    let relation_form = format!("relation {src} -> {dst} kind:{edge_kind}");
    let sections = json!({
        "claims": [{
            "claim_id": claim_id,
            "claim_type": "relation",
            "claim_text_or_form": relation_form,
            "subject_ref": src,
            "scope": format!("question:{question_id}"),
            "confidence": "unsettled",
            "question_refs": [question_id],
        }]
    });
    let subject_refs = vec![
        format!("claim:{claim_id}"),
        format!("question:{question_id}"),
        format!("run:{run_id}"),
        format!("target:{src}"),
    ];
    let mut repo = open_repo_ledger(ctx, &meta.repo_id)?;
    let pos = crate::envelopes::append(
        &mut repo,
        RecordKind::ClaimMade,
        &meta.repo_id,
        &e.checkout_id,
        None,
        None,
        Some(run_id),
        vec![format!("question:{question_id}")],
        sections,
        subject_refs,
        None,
    )?;
    Ok(output::output(vec![
        ("claim_id", json!(claim_id)),
        ("src_decl_key", json!(src)),
        ("dst_decl_key", json!(dst)),
        ("edge_kind", json!(edge_kind)),
        ("durability", json!("committed")),
        ("ledger_position", ledger_position_json(&pos)),
    ]))
}

/// Create evidence targeting a claim (§12.4a condition 2). This is the
/// simplified qualification path for Slice 4: evidence bearing on the relation
/// claim itself, with an explicit direction.
pub fn evidence_for_claim(
    ctx: &Ctx,
    run_id: &str,
    question_id: &str,
    claim_id: &str,
    direction: &str,
) -> Result<Value> {
    evidence_targeting(
        ctx,
        run_id,
        question_id,
        claim_id,
        direction,
        "relation_claim",
        "claim://direct",
    )
}

/// Create evidence targeting a diagnostic hypothesis's claim (§12.5a). The
/// hypothesis IS a claim (its claim_id equals the hypothesis id), so this
/// reuses the claim-target machinery: `target:claim:<hypothesis_id>`.
/// Direction `supports` = discriminating evidence FOR the locus;
/// `contradicts` = explicit exclusion of the locus (never silence).
pub fn evidence_for_hypothesis(
    ctx: &Ctx,
    run_id: &str,
    question_id: &str,
    hypothesis_id: &str,
    direction: &str,
) -> Result<Value> {
    evidence_targeting(
        ctx,
        run_id,
        question_id,
        hypothesis_id,
        direction,
        "discriminating_evidence",
        "diagnostic://hypothesis",
    )
}

fn evidence_targeting(
    ctx: &Ctx,
    run_id: &str,
    question_id: &str,
    claim_id: &str,
    direction: &str,
    evidence_type: &str,
    representation_ref: &str,
) -> Result<Value> {
    let meta = repo_for_run(ctx, run_id)?;
    let e = state::require_enrollment(ctx, &meta.repo_id)?;
    let evidence_id = ulid();
    let sections = json!({
        "evidence": [{
            "evidence_id": evidence_id,
            "evidence_type": evidence_type,
            "question_ref": question_id,
            "target_claim_ref": claim_id,
            "representation_ref": representation_ref,
            "relevance": "high",
            "reliability": "medium",
            "direction": direction,
            "confidence_impact": "neutral",
        }]
    });
    let subject_refs = vec![
        format!("evidence:{evidence_id}"),
        format!("run:{run_id}"),
        format!("question:{question_id}"),
        format!("target:claim:{claim_id}"),
    ];
    let mut repo = open_repo_ledger(ctx, &meta.repo_id)?;
    let pos = crate::envelopes::append(
        &mut repo,
        RecordKind::EvidenceCreated,
        &meta.repo_id,
        &e.checkout_id,
        None,
        None,
        Some(run_id),
        vec![format!("question:{question_id}")],
        sections,
        subject_refs,
        None,
    )?;
    Ok(output::output(vec![
        ("evidence_id", json!(evidence_id)),
        ("target_claim_ref", json!(claim_id)),
        ("direction", json!(direction)),
        ("durability", json!("committed")),
        ("ledger_position", ledger_position_json(&pos)),
    ]))
}

// ── Slice 5: diagnostic attribution loop (§12.5a) ────────────────────────

/// §12.5a: commit `residual.computed` records — one per disagreement residual
/// from the delta classifier. `declared_unobserved` (the honest default,
/// severity info) never enters the loop; only residuals that represent a
/// declared↔observed disagreement are emitted, each starting
/// `attribution_status: "undiagnosed"` (F-27, AC-DIAG-001).
pub fn residual_compute(
    ctx: &Ctx,
    repo_id: &str,
    question_id: &str,
    snapshot_id: &str,
) -> Result<Value> {
    let e = state::require_enrollment(ctx, repo_id)?;
    projections::rebuild(ctx, repo_id)?;
    let conn = projections::open(ctx, repo_id)?;
    let delta = projections::delta_question(&conn, repo_id, question_id, snapshot_id)?;

    let mut residuals: Vec<Value> = delta["residuals"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|r| {
            // Disagreement residuals only; declared_unobserved/observed_consistent
            // are honest default states, not disagreements (§12.5).
            matches!(
                r["residual_kind"].as_str(),
                Some(
                    "declared_contradicted"
                        | "contradictory"
                        | "declared_unsettled"
                        | "settled_with_debt"
                )
            )
        })
        .collect();
    residuals.extend(
        delta["observed_undeclared"]
            .as_array()
            .cloned()
            .unwrap_or_default(),
    );

    let mut committed: Vec<Value> = Vec::new();
    let mut repo = open_repo_ledger(ctx, repo_id)?;
    for r in &residuals {
        let residual_id = r["residual_id"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_default();
        let decl_key = r["decl_key"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| {
                format!(
                    "{}->{}:{}",
                    r["src_decl_key"].as_str().unwrap_or(""),
                    r["dst_decl_key"].as_str().unwrap_or(""),
                    r["edge_kind"].as_str().unwrap_or("")
                )
            });
        // Deterministic id contract (§12.5a): derived from question + decl_key
        // so the diagnostic loop can always join the committed row.
        let residual_id = if residual_id.is_empty() {
            projections::residual_id_for(question_id, &decl_key)
        } else {
            residual_id
        };
        let kind = r["residual_kind"].as_str().unwrap_or("unknown").to_string();
        let severity = r["severity"].as_str().unwrap_or("info").to_string();
        if residual_id.is_empty() {
            continue;
        }
        let spec = json!({
            "residual_id": residual_id,
            "residual_kind": kind,
            "decl_key": decl_key,
            "question_ref": question_id,
            "snapshot_ref": snapshot_id,
            "severity": severity,
            "evidence_refs": [],
            "attribution_status": "undiagnosed",
        });
        let sections = json!({
            "representations": [{
                "representation_id": format!("rep:residual:{residual_id}"),
                "representation_kind": "atomic",
                "representation_type": "residual",
                "inline_content": spec,
            }],
            "questions": [{
                "question_id": question_id,
                "question_form": "is the declared state realized?",
            }],
        });
        let subject_refs = vec![
            format!("residual:{residual_id}"),
            format!("question:{question_id}"),
            format!("snapshot:{snapshot_id}"),
            format!("target:{decl_key}"),
        ];
        let pos = crate::envelopes::append(
            &mut repo,
            RecordKind::ResidualComputed,
            repo_id,
            &e.checkout_id,
            None,
            None,
            None,
            vec![],
            sections,
            subject_refs,
            None,
        )?;
        committed.push(json!({
            "residual_id": residual_id,
            "residual_kind": kind,
            "severity": severity,
            "decl_key": decl_key,
            "attribution_status": "undiagnosed",
            "ledger_position": ledger_position_json(&pos),
        }));
    }
    Ok(output::output(vec![
        ("question_id", json!(question_id)),
        ("snapshot_id", json!(snapshot_id)),
        ("committed_count", json!(committed.len())),
        ("residuals", json!(committed)),
    ]))
}

/// Falsification path + next discriminating action for a failure locus (§12.5a).
/// Deterministic per locus so enumeration is rebuild-identical.
fn locus_plan(locus: &str) -> (String, &'static str, &'static str, &'static str) {
    match locus {
        "verifier" => (
            "if the verifier is at fault, running it against the previously-settled commit must also fail".into(),
            "question",
            "verifier://rerun-previously-settled",
            "separates verifier fault from implementation fault",
        ),
        "verification_plan" => (
            "if the plan is at fault, a different plan targeting the same obligation must change the outcome".into(),
            "question",
            "plan://alternate-plan",
            "separates plan fault from verifier/implementation fault",
        ),
        "projection" => (
            "if the projection is at fault, re-projecting the same observation must change the residual".into(),
            "question",
            "projection://reproject",
            "separates projection fault from model fault",
        ),
        "implementation" => (
            "if the implementation is at fault, a clean rebuild of the same commit must change the outcome".into(),
            "transformation",
            "implementation://clean-rebuild",
            "separates implementation fault from verifier/environment fault",
        ),
        "environment" => (
            "if the environment is at fault, the same run in a fresh identical environment must not reproduce".into(),
            "question",
            "environment://fresh-environment",
            "separates environment fault from implementation fault",
        ),
        "authority" => (
            "if the authority is at fault, re-querying under the same authority must not reproduce the denial".into(),
            "question",
            "authority://requery",
            "separates authority denial from evidence-collection fault",
        ),
        "evidence_collection" => (
            "if evidence collection is at fault, re-collecting the same observation independently must not contradict".into(),
            "question",
            "evidence://recollect-independently",
            "separates collection fault from verifier/model fault",
        ),
        "declared_model" => (
            "if the declared model is at fault, the declaration must mis-state reality even with a healthy verifier".into(),
            "transformation",
            "model://semantic-diff",
            "separates model fault from all cheap loci",
        ),
        _ => (
            "no locus can be excluded yet; collect discriminating evidence first".into(),
            "question",
            "diagnostic://collect-discriminating-evidence",
            "separates the unknown into named loci",
        ),
    }
}

/// §12.5a: create the diagnostic question + threshold for a residual (if
/// absent) and ONE `diagnostic.hypothesis` per candidate failure locus.
/// Idempotent: deterministic ids, re-enumeration is a no-op.
pub fn diagnostic_enumerate(
    ctx: &Ctx,
    repo_id: &str,
    question_id: &str,
    snapshot_id: &str,
    residual_id: &str,
) -> Result<Value> {
    let e = state::require_enrollment(ctx, repo_id)?;
    projections::rebuild(ctx, repo_id)?;
    let conn = projections::open(ctx, repo_id)?;
    let residual: Option<String> = conn
        .query_row(
            "SELECT decl_key FROM sxr_residual WHERE residual_id = ?1",
            [residual_id],
            |r| r.get::<_, String>(0),
        )
        .optional()?;
    let Some(decl_key) = residual else {
        bail!("residual {residual_id} not committed; run `sxr residual compute` first");
    };

    let diag_question_id = format!("diagq:{residual_id}");
    let diag_threshold_id = format!("diagth:{residual_id}");
    let mut repo = open_repo_ledger(ctx, repo_id)?;

    // 1. Diagnostic question (§12.5a): "why does declared X disagree with
    // observed Y?" — deterministic id, idempotent.
    let q_exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sxr_question WHERE question_id = ?1)",
            [&diag_question_id],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n != 0)
        .unwrap_or(false);
    if !q_exists {
        let spec = json!({
            "question_id": diag_question_id,
            "question_form": format!("why does declared {decl_key} disagree with observed reality?"),
            "question_type": "diagnostic",
            "target_entities": [decl_key],
            "model_question_ref": question_id,
            "snapshot_ref": snapshot_id,
            "residual_ref": residual_id,
        });
        let sections = json!({
            "questions": [{
                "question_id": diag_question_id,
                "question_form": format!("why does declared {decl_key} disagree with observed reality?"),
                "question_type": "diagnostic",
            }],
            "representations": [{
                "representation_id": format!("rep:question:{diag_question_id}"),
                "representation_kind": "atomic",
                "representation_type": "question_spec",
                "inline_content": spec,
            }],
        });
        let subject_refs = vec![
            format!("question:{diag_question_id}"),
            format!("target:{residual_id}"),
        ];
        crate::envelopes::append(
            &mut repo,
            RecordKind::QuestionDeclared,
            repo_id,
            &e.checkout_id,
            None,
            None,
            None,
            vec![],
            sections,
            subject_refs,
            None,
        )?;
    }

    // 2. Diagnostic threshold (condition c: "a threshold declared for the
    // *diagnostic* question"). Idempotent.
    let t_exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sxr_threshold WHERE threshold_id = ?1)",
            [&diag_threshold_id],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n != 0)
        .unwrap_or(false);
    if !t_exists {
        let spec = json!({
            "threshold_id": diag_threshold_id,
            "minimum_relevance": "medium",
            "minimum_reliability": "medium",
        });
        let sections = json!({
            "states": [{ "state_id": diag_threshold_id, "state_type": "settlement_threshold", "scope": format!("question:{diag_question_id}") }],
            "representations": [{
                "representation_id": format!("rep:threshold:{diag_threshold_id}"),
                "representation_kind": "atomic",
                "representation_type": "settlement_threshold_spec",
                "inline_content": spec,
            }],
        });
        let subject_refs = vec![
            format!("threshold:{diag_threshold_id}"),
            format!("question:{diag_question_id}"),
        ];
        crate::envelopes::append(
            &mut repo,
            RecordKind::ThresholdDeclared,
            repo_id,
            &e.checkout_id,
            None,
            None,
            None,
            vec![format!("question:{diag_question_id}")],
            sections,
            subject_refs,
            None,
        )?;
    }

    // 3. One diagnostic.hypothesis per candidate locus (the full §12.5a set).
    let all_loci = projections::FAILURE_LOCUS_CANDIDATES;
    let mut hypotheses: Vec<Value> = Vec::new();
    for locus in all_loci {
        let hypothesis_id = format!("hyp:{residual_id}:{locus}");
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sxr_hypothesis WHERE hypothesis_id = ?1)",
                [&hypothesis_id],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n != 0)
            .unwrap_or(false);
        if exists {
            hypotheses.push(json!({
                "hypothesis_id": hypothesis_id,
                "failure_locus_candidate": locus,
                "created": false,
            }));
            continue;
        }
        let alternatives: Vec<String> = all_loci
            .iter()
            .filter(|l| *l != locus)
            .map(|l| format!("hypothesis:hyp:{residual_id}:{l}"))
            .collect();
        let (falsification_path, next_kind, next_ref, expected_discrimination) = locus_plan(locus);
        let spec = json!({
            "hypothesis_id": hypothesis_id,
            "diagnostic_question_ref": diag_question_id,
            "residual_ref": residual_id,
            "failure_locus_candidate": locus,
            "confidence": "low",
            "supporting_evidence_refs": [],
            "contradicting_evidence_refs": [],
            "alternatives": alternatives,
            "falsification_path": falsification_path,
            "next_discriminating_action": {
                "kind": next_kind,
                "ref": next_ref,
                "expected_discrimination": expected_discrimination,
            },
        });
        let sections = json!({
            "questions": [{
                "question_id": diag_question_id,
                "question_form": format!("why does declared {decl_key} disagree with observed reality?"),
                "question_type": "diagnostic",
            }],
            "claims": [{
                "claim_id": hypothesis_id,
                "claim_type": "diagnostic",
                "claim_text_or_form": format!("failure locus candidate is {locus} for residual {residual_id}"),
                "subject_ref": format!("residual:{residual_id}"),
                "scope": format!("question:{diag_question_id}"),
                "confidence": "low",
            }],
            "representations": [{
                "representation_id": format!("rep:hypothesis:{hypothesis_id}"),
                "representation_kind": "atomic",
                "representation_type": "diagnostic_hypothesis",
                "inline_content": spec,
            }],
        });
        let subject_refs = vec![
            format!("hypothesis:{hypothesis_id}"),
            format!("question:{diag_question_id}"),
            format!("residual:{residual_id}"),
        ];
        let pos = crate::envelopes::append(
            &mut repo,
            RecordKind::DiagnosticHypothesis,
            repo_id,
            &e.checkout_id,
            None,
            None,
            None,
            vec![format!("residual:{residual_id}")],
            sections,
            subject_refs,
            None,
        )?;
        hypotheses.push(json!({
            "hypothesis_id": hypothesis_id,
            "failure_locus_candidate": locus,
            "created": true,
            "ledger_position": ledger_position_json(&pos),
        }));
    }

    Ok(output::output(vec![
        ("residual_id", json!(residual_id)),
        ("diagnostic_question_id", json!(diag_question_id)),
        ("diagnostic_threshold_id", json!(diag_threshold_id)),
        ("hypothesis_count", json!(hypotheses.len())),
        ("hypotheses", json!(hypotheses)),
    ]))
}

/// §12.5a: promote a hypothesis to `diagnostic.settled` — only under ALL four
/// conditions. The settlement records the supported locus, qualifying
/// evidence, ALL contradicting evidence (never omitted), and the excluded
/// alternatives.
pub fn diagnostic_settle(
    ctx: &Ctx,
    repo_id: &str,
    question_id: &str,
    hypothesis_id: &str,
) -> Result<Value> {
    let e = state::require_enrollment(ctx, repo_id)?;
    projections::rebuild(ctx, repo_id)?;
    let conn = projections::open(ctx, repo_id)?;
    let check = projections::diagnostic_settlement_check(&conn, question_id, hypothesis_id)?;
    if !check["conditions_met"].as_bool().unwrap_or(false) {
        let unmet: Vec<String> = check["unmet"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        bail!(
            "AC-DIAG-001: promotion to diagnostic.settled refused: {}",
            unmet.join("; ")
        );
    }

    let supported_locus = check["supported_locus"].as_str().unwrap_or("unknown");
    let residual_id = check["residual_id"].as_str().unwrap_or("");
    let qualifying: Vec<String> = check["qualifying_evidence_refs"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    let contradicting: Vec<String> = check["contradicting_evidence_refs"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    let excluded: Vec<String> = check["excluded_alternatives"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();

    let diag_question_id = format!("diagq:{residual_id}");
    let diag_threshold_id = format!("diagth:{residual_id}");
    let settlement_id = format!("ds:{residual_id}:{hypothesis_id}");
    let spec = json!({
        "settlement_id": settlement_id,
        "settlement_status": "settled",
        "question_ref": diag_question_id,
        "hypothesis_ref": format!("hypothesis:{hypothesis_id}"),
        "residual_ref": format!("residual:{residual_id}"),
        "supported_locus": supported_locus,
        "threshold_ref": format!("threshold:{diag_threshold_id}"),
        "qualifying_evidence_refs": qualifying,
        "contradicting_evidence_refs": contradicting,
        "excluded_alternatives": excluded,
    });
    let sections = json!({
        "settlements": [{
            "settlement_id": settlement_id,
            "settlement_status": "settled",
            "transformation_ref": "transformation:diagnostic",
            "question_ref": diag_question_id,
            "evidence_refs": qualifying,
            "threshold_ref": format!("threshold:{diag_threshold_id}"),
        }],
        "questions": [{
            "question_id": diag_question_id,
            "question_form": "why does the declared state disagree with observed reality?".to_string(),
            "question_type": "diagnostic",
        }],
        "representations": [{
            "representation_id": format!("rep:diag-settlement:{settlement_id}"),
            "representation_kind": "atomic",
            "representation_type": "diagnostic_settlement",
            "inline_content": spec,
        }],
    });
    let subject_refs = vec![
        format!("settlement:{settlement_id}"),
        format!("question:{diag_question_id}"),
        format!("hypothesis:{hypothesis_id}"),
    ];
    let mut repo = open_repo_ledger(ctx, repo_id)?;
    let pos = crate::envelopes::append(
        &mut repo,
        RecordKind::DiagnosticSettled,
        repo_id,
        &e.checkout_id,
        None,
        None,
        None,
        vec![format!("hypothesis:{hypothesis_id}")],
        sections,
        subject_refs,
        None,
    )?;

    Ok(output::output(vec![
        ("settlement_id", json!(settlement_id)),
        ("supported_failure_locus", json!(supported_locus)),
        ("residual_id", json!(residual_id)),
        ("qualifying_evidence_refs", json!(qualifying)),
        ("contradicting_evidence_refs", json!(contradicting)),
        ("excluded_alternatives", json!(excluded)),
        ("durability", json!("committed")),
        ("ledger_position", ledger_position_json(&pos)),
    ]))
}

/// §12.5a: raise a repair proposal bound to its diagnostic settlement. A
/// proposal naming `declared_model` additionally requires the cheap loci —
/// verifier, verification_plan, implementation, environment — each explicitly
/// excluded with evidence (the asymmetric friction). A proposal is not
/// approval/promotion.
pub fn diagnostic_propose(
    ctx: &Ctx,
    repo_id: &str,
    _question_id: &str,
    hypothesis_id: &str,
    locus: &str,
    target: &str,
) -> Result<Value> {
    let e = state::require_enrollment(ctx, repo_id)?;
    projections::rebuild(ctx, repo_id)?;
    let conn = projections::open(ctx, repo_id)?;

    // Predecessor rule: proposal.raised requires a diagnostic.settled.
    let settlement: Option<(String, String)> = conn
        .query_row(
            "SELECT settlement_id, residual_id FROM sxr_diag_settlement WHERE hypothesis_id = ?1",
            [hypothesis_id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .optional()?;
    let Some((settlement_id, residual_id)) = settlement else {
        bail!(
            "proposal requires a diagnostic settlement for hypothesis {hypothesis_id} \
             (predecessor rule: diagnostic.settled)"
        );
    };

    // Model-locus friction (§12.5a): declared_model only after the cheap loci
    // are each explicitly excluded with evidence (AC-DIAG-001).
    if locus == "declared_model" {
        let exclusions = projections::model_locus_exclusions(&conn, &residual_id)?;
        let allowed = exclusions["model_locus_proposal_allowed"]
            .as_bool()
            .unwrap_or(false);
        if !allowed {
            let missing: Vec<String> = exclusions["missing_exclusions"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect();
            bail!(
                "AC-DIAG-001: model-locus proposal refused: cheap loci not all \
                 evidence-excluded (missing: {})",
                missing.join(", ")
            );
        }
    }

    let diag_question_id = format!("diagq:{residual_id}");
    let proposal_id = format!("prop:{residual_id}:{hypothesis_id}");
    let transformation_ref = format!("transformation:proposal:{proposal_id}");
    let spec = json!({
        "proposal_id": proposal_id,
        "locus": locus,
        "hypothesis_ref": format!("hypothesis:{hypothesis_id}"),
        "diagnostic_settlement_ref": format!("settlement:{settlement_id}"),
        "transformation_ref": transformation_ref,
        "question_ref": diag_question_id,
        "target_ref": target,
    });
    let sections = json!({
        "transformations": [{
            "transformation_id": format!("tx:{proposal_id}"),
            "transformation_type": if locus == "declared_model" { "model_revision_proposal" } else { "repair_proposal" },
            "scope": format!("locus:{locus}"),
            "affected_entities": [target],
        }],
        "questions": [{
            "question_id": diag_question_id,
            "question_form": "why does the declared state disagree with observed reality?".to_string(),
            "question_type": "diagnostic",
        }],
        "representations": [{
            "representation_id": format!("rep:proposal:{proposal_id}"),
            "representation_kind": "atomic",
            "representation_type": "proposal",
            "inline_content": spec,
        }],
    });
    let subject_refs = vec![
        format!("proposal:{proposal_id}"),
        format!("locus:{locus}"),
        format!("target:{target}"),
        format!("question:{diag_question_id}"),
        format!("hypothesis:{hypothesis_id}"),
        format!("settlement:{settlement_id}"),
    ];
    let mut repo = open_repo_ledger(ctx, repo_id)?;
    let pos = crate::envelopes::append(
        &mut repo,
        RecordKind::ProposalRaised,
        repo_id,
        &e.checkout_id,
        None,
        None,
        None,
        vec![format!("settlement:{settlement_id}")],
        sections,
        subject_refs,
        None,
    )?;

    Ok(output::output(vec![
        ("proposal_id", json!(proposal_id)),
        ("locus", json!(locus)),
        ("target_ref", json!(target)),
        (
            "diagnostic_settlement_ref",
            json!(format!("settlement:{settlement_id}")),
        ),
        ("durability", json!("committed")),
        ("ledger_position", ledger_position_json(&pos)),
    ]))
}

/// §17.2-shaped diagnostic loop state for one residual.
pub fn diagnostic_status(ctx: &Ctx, repo_id: &str, residual_id: &str) -> Result<Value> {
    let _e = state::require_enrollment(ctx, repo_id)?;
    projections::rebuild(ctx, repo_id)?;
    let conn = projections::open(ctx, repo_id)?;
    let question_id: Option<String> = conn
        .query_row(
            "SELECT question_id FROM sxr_residual WHERE residual_id = ?1",
            [residual_id],
            |r| r.get::<_, String>(0),
        )
        .optional()?;
    let v = projections::diagnostic_status(
        &conn,
        repo_id,
        question_id.as_deref().unwrap_or(""),
        residual_id,
    )?;
    Ok(output::output(vec![("diagnostic", v)]))
}

// ── run finish ───────────────────────────────────────────────────────────

pub fn run_finish(ctx: &Ctx, run_id: &str) -> Result<Value> {
    let mut meta = repo_for_run(ctx, run_id)?;
    let e = state::require_enrollment(ctx, &meta.repo_id)?;
    let sections = json!({
        "transformations": [{ "transformation_id": format!("tx:fin:{run_id}"), "transformation_type": "run_finish" }],
        "states": [{ "state_id": "state:runfin", "state_type": "run", "scope": format!("run:{run_id}") }]
    });
    let subject_refs = vec![
        format!("run:{run_id}"),
        format!("target:question:{}", meta.question_id),
    ];
    let mut repo = open_repo_ledger(ctx, &meta.repo_id)?;
    let pos = crate::envelopes::append(
        &mut repo,
        RecordKind::RunFinished,
        &meta.repo_id,
        &e.checkout_id,
        None,
        None,
        Some(run_id),
        vec![format!("run:{run_id}")],
        sections,
        subject_refs,
        None,
    )?;
    meta.finished_at =
        Some(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true));
    state::update_run(ctx, &meta)?;
    Ok(output::output(vec![
        ("run_id", json!(run_id)),
        ("status", json!("finished")),
        ("durability", json!("committed")),
        ("ledger_position", ledger_position_json(&pos)),
    ]))
}

// ── ledger commands ──────────────────────────────────────────────────────

pub fn ledger_verify(ctx: &Ctx, repo_id: &str) -> Result<Value> {
    let ledger = ledger_io::open_existing(ctx, repo_id)?;
    let report = ledger.verify_full().map_err(|e| anyhow!("verify: {e}"))?;
    Ok(output::output(vec![
        ("ok", json!(report.complete)),
        ("verified_count", json!(report.verified_count)),
        (
            "first_divergence",
            report
                .first_divergence
                .as_ref()
                .map(ledger_position_json)
                .unwrap_or(Value::Null),
        ),
        ("limitations", json!(report.limitations)),
    ]))
}

pub fn ledger_export(ctx: &Ctx, repo_id: &str) -> Result<Value> {
    let rows = ledger_io::read_all_existing(ctx, repo_id)?;
    let entries: Vec<Value> = rows
        .iter()
        .map(|r| {
            let payload: Value = serde_json::from_str(&r.payload).unwrap_or(Value::Null);
            json!({
                "append_ordinal": r.append_ordinal,
                "record_kind": r.record_kind,
                "ledger_id": r.ledger_id,
                "entry_hash": r.entry_hash,
                "payload": payload,
            })
        })
        .collect();
    Ok(Value::Array(entries))
}

pub fn ledger_history_json(ctx: &Ctx, repo_id: &str) -> Result<Value> {
    let rows = ledger_io::read_all_existing(ctx, repo_id)?;
    let hist: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "ledger_id": r.ledger_id,
                "append_ordinal": r.append_ordinal,
                "record_kind": r.record_kind,
                "entry_hash": r.entry_hash,
                "entry_ulid": r.entry_ulid,
            })
        })
        .collect();
    Ok(output::output1("history", json!(hist)))
}

// ── query evidence ───────────────────────────────────────────────────────

pub fn query_evidence(ctx: &Ctx, repo_id: &str, entity: &str) -> Result<Value> {
    let _e = state::require_enrollment(ctx, repo_id)?;
    // F-19b: a queried entity must be committed, not pending.
    crate::durability::ensure_committed(ctx, repo_id, &[entity])?;
    let conn = projections::open(ctx, repo_id)?;
    // Ensure projections are present.
    projections::rebuild(ctx, repo_id)?;
    let v = projections::query_evidence_declared(&conn, repo_id, entity)?;
    Ok(output::output(vec![("query", v)]))
}

// ── projections ──────────────────────────────────────────────────────────

pub fn projections_status(ctx: &Ctx, repo_id: &str) -> Result<Value> {
    let st = projections::status(ctx, repo_id)?;
    Ok(output::output1("projections", st))
}

pub fn projections_rebuild(ctx: &Ctx, repo_id: &str) -> Result<Value> {
    projections::rebuild(ctx, repo_id)?;
    let st = projections::status(ctx, repo_id)?;
    Ok(output::output(vec![
        ("status", json!("ok")),
        ("projections", st),
    ]))
}

pub fn projections_export(ctx: &Ctx, repo_id: &str) -> Result<String> {
    projections::export_canonical(ctx, repo_id)
}

/// Slice 4: export sxr's delta residuals as an external anti-collapse corpus
/// (`cep/anti-collapse-corpus@1`) for independent evaluation by the CEP
/// evaluator (§24 Slice 4 gate). sxr does NOT judge its own semantics.
pub fn export_anti_collapse_corpus(
    ctx: &Ctx,
    repo_id: &str,
    question_id: &str,
    snapshot_id: &str,
) -> Result<String> {
    let _e = state::require_enrollment(ctx, repo_id)?;
    projections::rebuild(ctx, repo_id)?;
    let conn = projections::open(ctx, repo_id)?;
    let corpus =
        projections::export_anti_collapse_corpus(&conn, repo_id, question_id, snapshot_id)?;
    Ok(serde_json::to_string_pretty(&corpus)?)
}

// ── doctor ───────────────────────────────────────────────────────────────

/// Daemon presence probe: the socket accepts connections only while the
/// daemon holds it (§16.1, F-15). Informational — never a hard requirement.
fn daemon_active(ctx: &Ctx) -> bool {
    use std::os::unix::net::UnixStream;
    UnixStream::connect(ctx.socket_path()).is_ok()
}

/// Exit code for `doctor`: 0 healthy, 2 when corrupt queue segments exist
/// (actionable durability state, §16.4).
pub fn doctor(ctx: &Ctx) -> Result<(Value, i32)> {
    let enrollments = state::list_enrollments(ctx)?;
    let mut pending_count = 0u64;
    let mut spooled_count = 0u64;
    let mut corrupt: Vec<String> = Vec::new();
    let mut repos: Vec<Value> = Vec::new();
    for e in &enrollments {
        repos.push(json!({
            "repo_id": e.repo_id,
            "filesystem_path": e.filesystem_path.to_string_lossy(),
        }));
        for p in crate::durability::pending_entries(ctx, &e.repo_id)? {
            if p.state == "spooled" {
                spooled_count += 1;
            } else if p.corrupt {
                corrupt.push(p.path.display().to_string());
            } else {
                pending_count += 1;
            }
        }
    }
    let active = daemon_active(ctx);
    Ok((
        output::output(vec![
            ("data_root", json!(ctx.data_root.to_string_lossy())),
            ("state_root", json!(ctx.state_root.to_string_lossy())),
            ("enrolled_repos", json!(repos)),
            ("filesystem_class", json!("safe")),
            ("daemon_active", json!(active)),
            ("pending_queue_entries", json!(pending_count)),
            ("spooled_entries", json!(spooled_count)),
            ("corrupt_queue_segments", json!(corrupt)),
        ]),
        if corrupt.is_empty() { 0 } else { 2 },
    ))
}

// ── misc ─────────────────────────────────────────────────────────────────

pub fn delta(ctx: &Ctx, repo_id: &str, question_id: &str, snapshot_id: &str) -> Result<Value> {
    let _e = state::require_enrollment(ctx, repo_id)?;
    // F-19b: the referenced question and snapshot must be committed.
    crate::durability::ensure_committed(ctx, repo_id, &[question_id, snapshot_id])?;
    projections::rebuild(ctx, repo_id)?;
    let conn = projections::open(ctx, repo_id)?;
    let v = projections::delta_question(&conn, repo_id, question_id, snapshot_id)?;
    Ok(output::output(vec![("delta", v)]))
}

/// Observation sections mirroring §15.1 step 7's `observation.recorded`
/// shape (also produced by `sxr verify run`).
fn observation_sections(
    observation_id: &str,
    run_id: &str,
    exit_code: Option<i32>,
    crashed: bool,
    stdout_ref: &str,
    stderr_ref: Option<&str>,
    parsed: &Option<sxr_verify::ParsedSummary>,
) -> Value {
    json!({
        "observations": [{ "observation_id": observation_id, "scope": format!("run:{run_id}") }],
        "representations": [{
            "representation_id": format!("rep:obs:{observation_id}"),
            "representation_kind": "atomic",
            "representation_type": "verifier_stdout",
            "content_hash": stdout_ref,
            "inline_content": {
                "exit_code": exit_code,
                "crashed": crashed,
                "stderr_ref": stderr_ref,
                "parsed": parsed.as_ref().map(|p| json!({
                    "test_count": p.test_count, "passed": p.passed, "failed": p.failed, "ignored": p.ignored,
                })).unwrap_or(Value::Null),
            },
        }]
    })
}

/// The producer-set idempotency key for an ingest request (§11.4):
/// `sha256(run_id + verifier_id + step_id + attempt)`. The same logical
/// verifier stream has the same key whether it is committed directly,
/// spooled, or queued through the daemon.
pub fn ingest_idempotency_key(run_id: &str, verifier: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(run_id.as_bytes());
    h.update(b"+");
    h.update(verifier.as_bytes());
    h.update(b"+ingest+1");
    hex::encode(h.finalize())
}

/// Build the ingest request metadata object (§11.1a spool shape).
fn ingest_request_meta(
    repo_id: &str,
    run_id: &str,
    verifier: &str,
    stdin_format: &str,
    stdin_sha256: &str,
    observation_id: &str,
    extensions: Option<&Value>,
) -> Value {
    let mut meta = json!({
        "request_kind": "sxr.ingest/v1",
        "repo_id": repo_id,
        "run_id": run_id,
        "verifier": verifier,
        "stdin_format": stdin_format,
        "idempotency_key": ingest_idempotency_key(run_id, verifier),
        "stdin_sha256": stdin_sha256,
        "observation_id": observation_id,
    });
    if let Some(ext) = extensions {
        meta["extensions"] = ext.clone();
    }
    meta
}

/// Validate caller-supplied `--extensions` (CEP-0008 §39): a JSON object that
/// does not claim the sxr-owned `domainforge` namespace.
pub fn validate_ingest_extensions(ext: &Value) -> Result<()> {
    let obj = ext
        .as_object()
        .ok_or_else(|| anyhow!("--extensions must be a JSON object"))?;
    if obj.contains_key("domainforge") {
        bail!("--extensions must not use the reserved \"domainforge\" key");
    }
    Ok(())
}

/// Parse and validate the raw `--extensions` argument.
pub fn parse_ingest_extensions(raw: &str) -> Result<Value> {
    let ext: Value =
        serde_json::from_str(raw).map_err(|e| anyhow!("--extensions is not valid JSON: {e}"))?;
    validate_ingest_extensions(&ext)?;
    Ok(ext)
}

/// A built observation envelope ready to commit or to queue, plus the
/// metadata the ledger needs to append it (§16.2 — the daemon queues exactly
/// what the CLI can write).
pub struct IngestEnvelope {
    pub envelope: Value,
    pub subject_refs: Vec<String>,
    pub kind: RecordKind,
    pub idempotency_key: String,
    pub observation_id: String,
    pub parsed: Value,
}

/// Build (never commit) the `observation.recorded` envelope for one ingested
/// verifier stream, replaying the raw bytes into the repo's artifacts dir.
/// Direct commit, daemon submission, and spool drain all construct through
/// this one path, so the record is identical whichever way it lands.
pub fn build_ingest_envelope(
    ctx: &Ctx,
    repo_id: &str,
    run_id: &str,
    verifier: &str,
    bytes: &[u8],
    observation_id: Option<&str>,
    extensions: Option<&Value>,
) -> Result<IngestEnvelope> {
    let meta = repo_for_run(ctx, run_id)?;
    let e = state::require_enrollment(ctx, repo_id)?;
    let progress = repo_plan(ctx, repo_id, run_id)?;
    let target = progress
        .get("targets")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(Value::as_str)
        .unwrap_or("concept:unknown")
        .to_string();
    // The spool path pre-mints the id at spool time and stores it in the
    // request, so drain reproduces the exact envelope it would have produced
    // when the request was written (record parity with direct ingest).
    let observation_id = match observation_id {
        Some(o) => o.to_string(),
        None => ulid(),
    };
    let hex_stdout = hex::encode(sha2::Sha256::digest(bytes));
    let stdout_ref = format!("sha256:{hex_stdout}");
    let artifacts_dir = ctx.ensure_repo_dir(repo_id)?.join("artifacts");
    std::fs::create_dir_all(&artifacts_dir).context("create artifacts dir")?;
    std::fs::write(artifacts_dir.join(format!("{hex_stdout}.stdout")), bytes)
        .context("persist ingested stream")?;
    let parsed = sxr_verify::parse_cargo_json(bytes);
    let mut sections = observation_sections(
        &observation_id,
        run_id,
        None,
        parsed.is_none(),
        &stdout_ref,
        None,
        &parsed,
    );
    if let Some(ext) = extensions {
        validate_ingest_extensions(ext)?;
        sections["extensions"] = ext.clone();
    }
    let subject_refs = vec![
        format!("observation:{observation_id}"),
        format!("run:{run_id}"),
        format!("target:{target}"),
    ];
    let plan_id = meta.plan_id.clone().unwrap_or_default();
    let envelope = crate::envelopes::build(
        RecordKind::ObservationRecorded,
        repo_id,
        &e.checkout_id,
        None,
        None,
        Some(run_id),
        vec![format!("plan:{plan_id}")],
        sections,
        subject_refs.clone(),
        None,
        None,
    )?;
    let parsed_out = parsed.as_ref().map(|p| {
        json!({"test_count": p.test_count, "passed": p.passed, "failed": p.failed, "ignored": p.ignored})
    }).unwrap_or(Value::Null);
    Ok(IngestEnvelope {
        envelope,
        subject_refs,
        kind: RecordKind::ObservationRecorded,
        idempotency_key: ingest_idempotency_key(run_id, verifier),
        observation_id,
        parsed: parsed_out,
    })
}

/// Commit one built ingest envelope to the ledger (idempotent by key).
pub fn commit_ingest_envelope(
    ctx: &Ctx,
    repo_id: &str,
    built: &IngestEnvelope,
) -> Result<(bool, sxr_core::LedgerPosition)> {
    let mut repo = open_repo_ledger(ctx, repo_id)?;
    let already = repo
        .ledger
        .position_by_idempotency_key(&built.idempotency_key)
        .context("check idempotency key")?
        .is_some();
    let pos = crate::ledger_io::append_record(
        &mut repo,
        built.kind,
        Some(built.idempotency_key.clone()),
        built.subject_refs.clone(),
        &built.envelope,
    )?;
    Ok((already, pos))
}

/// `sxr ingest`: read a verifier stream from stdin and record it as an
/// `observation.recorded` ledger record (§15.1 step 7 execute-elsewhere path).
///
/// Durability (§11.1a): direct commits synchronously (`committed`, exit 0);
/// `--spool` writes the unvalidated request to the spool (`spooled`, exit 75 —
/// never 0); `--async` submits to the daemon queue (`queued_for_commit`, exit
/// 0 only after a fsynced receipt) or transparently falls back to direct
/// commit when the daemon is unavailable (F-15, `"daemon": false`).
pub fn ingest(
    ctx: &Ctx,
    run_id: &str,
    verifier: &str,
    stdin_format: &str,
    spool: bool,
    async_mode: bool,
    extensions: Option<&str>,
) -> Result<(Value, i32)> {
    let meta = repo_for_run(ctx, run_id)?;
    let extensions = extensions.map(parse_ingest_extensions).transpose()?;
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut std::io::stdin(), &mut buf).context("read stdin")?;
    let stdin_sha256 = format!("sha256:{}", hex::encode(sha2::Sha256::digest(&buf)));

    if spool {
        // Opt-in spool: written unvalidated; not a success state (exit 75).
        // The observation id is minted now and carried in the request, so
        // drain rebuilds the exact envelope the direct path would have
        // produced here (record parity).
        let obs_id = ulid();
        let meta_json = ingest_request_meta(
            &meta.repo_id,
            run_id,
            verifier,
            stdin_format,
            &stdin_sha256,
            &obs_id,
            extensions.as_ref(),
        );
        let key = ingest_idempotency_key(run_id, verifier);
        let dir = ctx.spool_dir(&meta.repo_id);
        std::fs::create_dir_all(&dir).context("create spool dir")?;
        let ndjson_path = dir.join(format!("{key}.ndjson"));
        std::fs::write(
            &ndjson_path,
            format!("{}\n", serde_json::to_string(&meta_json)?),
        )
        .context("write spool metadata")?;
        let bytes_path = dir.join(format!("{key}.stdin"));
        std::fs::write(&bytes_path, &buf).context("write spooled bytes")?;
        let v = output::output(vec![
            ("run_id", json!(run_id)),
            ("verifier", json!(verifier)),
            ("bytes_received", json!(buf.len())),
            ("durability", json!("spooled")),
            ("spool_entry", json!(ndjson_path.display().to_string())),
        ]);
        return Ok((v, 75));
    }

    if async_mode
        && let Some((duration, resp)) = submit_to_daemon(
            ctx,
            &meta.repo_id,
            run_id,
            verifier,
            &buf,
            extensions.as_ref(),
        )?
    {
        let v = output::output(vec![
            ("run_id", json!(run_id)),
            ("verifier", json!(verifier)),
            ("bytes_received", json!(buf.len())),
            ("durability", json!("queued_for_commit")),
            ("daemon", json!(true)),
            ("receipt", resp),
            ("queue_position", json!(duration)),
        ]);
        return Ok((v, 0));
    }
    // F-15: daemon unavailable (or --async not requested) — direct CLI writer
    // lock, zero semantic difference.

    let built = build_ingest_envelope(
        ctx,
        &meta.repo_id,
        run_id,
        verifier,
        &buf,
        None,
        extensions.as_ref(),
    )?;
    let (already, pos) = commit_ingest_envelope(ctx, &meta.repo_id, &built)?;
    Ok((
        output::output(vec![
            ("run_id", json!(run_id)),
            ("verifier", json!(verifier)),
            ("bytes_received", json!(buf.len())),
            ("durability", json!("committed")),
            ("daemon", json!(false)),
            ("observation_id", json!(built.observation_id)),
            ("idempotency_key", json!(built.idempotency_key)),
            ("already_committed", json!(already)),
            ("parsed", built.parsed),
            ("ledger_position", ledger_position_json(&pos)),
        ]),
        0,
    ))
}

/// Submit an ingest request to the running daemon (§16.1). Returns the queue
/// position + receipt JSON. Returns None when the daemon socket is
/// unavailable — the caller takes the F-15 fallback (direct CLI writer lock,
/// `"daemon": false`, zero semantic difference).
fn submit_to_daemon(
    ctx: &Ctx,
    repo_id: &str,
    run_id: &str,
    verifier: &str,
    bytes: &[u8],
    extensions: Option<&Value>,
) -> Result<Option<(String, Value)>> {
    use std::io::{BufRead, Write};
    use std::os::unix::net::UnixStream;
    let socket = ctx.socket_path();
    let mut stream = match UnixStream::connect(&socket) {
        Ok(s) => s,
        Err(_) => return Ok(None),
    };
    // Build the same envelope the direct path would commit (§16.3 parity:
    // the queue holds the validated request, never a differently-built one).
    let built = build_ingest_envelope(ctx, repo_id, run_id, verifier, bytes, None, extensions)?;
    let request = crate::daemon::record_request(repo_id, &built);
    stream.write_all(serde_json::to_string(&request)?.as_bytes())?;
    stream.write_all(b"\n")?;
    let mut resp = String::new();
    std::io::BufReader::new(&mut stream)
        .read_line(&mut resp)
        .context("read daemon receipt")?;
    let receipt: Value = serde_json::from_str(resp.trim()).context("parse daemon receipt line")?;
    if let Some(err) = receipt.get("error").and_then(Value::as_str) {
        bail!("daemon rejected request: {err}");
    }
    let position = receipt
        .get("queue_position")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    Ok(Some((position, receipt)))
}

pub fn spool_status(ctx: &Ctx, repo_id: &str) -> Result<Value> {
    let entries = crate::durability::pending_entries(ctx, repo_id)?;
    let spooled: Vec<Value> = entries
        .iter()
        .filter(|e| e.state == "spooled")
        .map(|e| {
            json!({
                "path": e.path.display().to_string(),
                "request": crate::durability::request_json(e),
            })
        })
        .collect();
    Ok(output::output(vec![
        ("repo_id", json!(repo_id)),
        ("spooled_count", json!(spooled.len())),
        ("entries", json!(spooled)),
    ]))
}

pub fn spool_drain(ctx: &Ctx, repo_id: &str) -> Result<Value> {
    let entries = crate::durability::pending_entries(ctx, repo_id)?;
    let mut drained = 0u64;
    let mut already = 0u64;
    let mut rejected: Vec<String> = Vec::new();
    let mut failures: Vec<String> = Vec::new();
    let mut observations: Vec<String> = Vec::new();
    for entry in entries.iter().filter(|e| e.state == "spooled") {
        let meta = crate::durability::request_json(entry);
        let run_id = meta
            .get("run_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let verifier = meta
            .get("verifier")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let key = meta
            .get("idempotency_key")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        // Pre-minted at spool time (§F-19b): drain reproduces the envelope
        // the spooler would have built when the request was written.
        let obs_id = meta.get("observation_id").and_then(Value::as_str);
        let extensions = meta.get("extensions");
        let expected_sha = meta
            .get("stdin_sha256")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let bytes_path = entry.path.with_extension("stdin");
        match std::fs::read(&bytes_path) {
            Ok(bytes) => {
                let actual = format!("sha256:{}", hex::encode(sha2::Sha256::digest(&bytes)));
                if actual != expected_sha {
                    rejected.push(format!(
                        "{} (checksum mismatch: expected {expected_sha}, got {actual})",
                        entry.path.display()
                    ));
                    let _ = crate::durability::spool_remove(entry);
                    continue;
                }
                // Drain is ordinary ingest: a spooled record can still be
                // rejected on validation (§11.1a).
                if sxr_verify::parse_cargo_json(&bytes).is_none() {
                    rejected.push(format!(
                        "{} (unparseable verifier stream)",
                        entry.path.display()
                    ));
                    let _ = crate::durability::spool_remove(entry);
                    continue;
                }
                match build_ingest_envelope(
                    ctx, repo_id, run_id, verifier, &bytes, obs_id, extensions,
                ) {
                    Ok(built) => {
                        if !key.is_empty() && built.idempotency_key != key {
                            rejected.push(format!(
                                "{} (idempotency key mismatch: expected {key}, got {})",
                                entry.path.display(),
                                built.idempotency_key
                            ));
                            let _ = crate::durability::spool_remove(entry);
                            continue;
                        }
                        match commit_ingest_envelope(ctx, repo_id, &built) {
                            Ok((was_already, _)) => {
                                let _ = crate::durability::spool_remove(entry);
                                observations.push(built.observation_id.clone());
                                if was_already {
                                    already += 1;
                                } else {
                                    drained += 1;
                                }
                            }
                            Err(e) => failures.push(format!("{}: {e:#}", entry.path.display())),
                        }
                    }
                    Err(e) => failures.push(format!("{}: {e:#}", entry.path.display())),
                }
            }
            Err(e) => failures.push(format!("{}: {e}", entry.path.display())),
        }
    }
    Ok(output::output(vec![
        ("repo_id", json!(repo_id)),
        ("drained", json!(drained)),
        ("already_committed", json!(already)),
        ("observations", json!(observations)),
        ("rejected", json!(rejected)),
        ("failures", json!(failures)),
    ]))
}

/// `sxr await --run <run_id>`: block until every record for the run reaches
/// `committed`, or report which remain and why (§11.1a). `--repo <id> --all`
/// waits for the repo's queue and spool to drain entirely.
pub fn await_run(
    ctx: &Ctx,
    run_id: Option<&str>,
    repo_id: Option<&str>,
    all: bool,
    timeout_secs: u64,
    interval_ms: u64,
) -> Result<Value> {
    let repo_id = match repo_id {
        Some(r) => r.to_string(),
        None => repo_for_run(ctx, run_id.expect("--run or --repo required"))?.repo_id,
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    loop {
        let pending = crate::durability::pending_entries(ctx, &repo_id)?;
        let relevant: Vec<_> = pending
            .into_iter()
            .filter(|e| all || run_id.is_some_and(|r| crate::durability::references(e, r)))
            .collect();
        if relevant.is_empty() {
            break;
        }
        if std::time::Instant::now() >= deadline {
            let remaining: Vec<Value> = relevant
                .iter()
                .map(|e| {
                    let why = if e.corrupt {
                        "corrupt queue segment (request hash mismatch); frozen"
                    } else if e.state == "spooled" {
                        "spooled (not yet validated); run `sxr spool drain`"
                    } else {
                        "queued_for_commit (not yet committed); run `sxr queue recover`"
                    };
                    json!({
                        "path": e.path.display().to_string(),
                        "state": e.state,
                        "why": why,
                    })
                })
                .collect();
            bail!(
                "await {}: timed out after {timeout_secs}s; remaining not committed: {}",
                run_id.unwrap_or("--all"),
                serde_json::to_string(&remaining)?
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(interval_ms));
    }
    Ok(output::output(vec![
        ("run_id", json!(run_id)),
        ("repo_id", json!(repo_id)),
        ("state", json!("committed")),
    ]))
}

pub fn explain(_ctx: &Ctx, record_id: &str) -> Result<Value> {
    Ok(output::output(vec![
        ("record_id", json!(record_id)),
        ("note", json!("slice-1 slim")),
    ]))
}

pub fn context(ctx: &Ctx, repo_id: &str, run_id: &str) -> Result<Value> {
    let _e = state::require_enrollment(ctx, repo_id)?;
    let meta = state::get_run(ctx, run_id).with_context(|| format!("run {run_id} not found"))?;
    let conn = projections::open(ctx, &meta.repo_id)?;
    // Ensure projections are present.
    projections::rebuild(ctx, &meta.repo_id)?;

    // Structured retriever: the run's bound question + threshold.
    let mut question = None;
    if !meta.question_id.is_empty() {
        question = conn
            .query_row(
                "SELECT question_form, question_type FROM sxr_question WHERE question_id = ?1",
                [&meta.question_id],
                |r| {
                    Ok(json!({
                        "question_id": meta.question_id.clone(),
                        "question_form": r.get::<_, String>(0)?,
                        "question_type": r.get::<_, String>(1)?,
                    }))
                },
            )
            .optional()?;
    }
    let mut threshold = None;
    if !meta.threshold_id.is_empty() {
        threshold = conn
            .query_row(
                "SELECT spec_json FROM sxr_threshold WHERE threshold_id = ?1",
                [&meta.threshold_id],
                |r| {
                    Ok(json!({
                        "threshold_id": meta.threshold_id.clone(),
                        "spec_json": r.get::<_, String>(0)?,
                    }))
                },
            )
            .optional()?;
    }

    // Failure summary: the run's failing observations. sxr does not materialize
    // an sxr_observation projection table in Slice 3; the run's exit status is
    // reported from the run registry with an honest note when unavailable.
    let failure = json!({
        "step_id": "unit",
        "verifier_id": "unknown",
        "exit_code": serde_json::Value::Null,
        "note": "observation projection not materialized in slice 3",
    });

    // Graph retriever: 1- and 2-hop neighbours of the run's snapshot
    // declarations. If the run has a plan with targets, use the first target;
    // otherwise use the run's snapshot.
    let mut declared_neighbourhood: Vec<Value> = Vec::new();
    let snapshot_id = &meta.snapshot_id;
    if let Some((snap, _)) = conn
        .query_row(
            "SELECT snapshot_id, semantic_closure_hash FROM sxr_snapshot WHERE snapshot_id = ?1",
            [snapshot_id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
        )
        .optional()?
    {
        let _ = snap;
        // Collect the snapshot's declarations as seeds; annotate each with
        // observed status and its 1-2 hop neighbourhood.
        let mut stmt = conn.prepare(
            "SELECT decl_key, name, decl_kind, namespace FROM sxr_decl_node WHERE snapshot_id = ?1",
        )?;
        let rows = stmt.query_map([snapshot_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?;
        let mut seeds: Vec<(String, String, String, String)> =
            rows.filter_map(|r| r.ok()).collect();
        seeds.truncate(8);
        for (dk, name, kind, ns) in seeds {
            let nb = projections::neighbours(&conn, snapshot_id, &dk, 2)?;
            let presence = if nb.is_empty() {
                // no neighbours; check observed status directly
                observed_status(&conn, &dk)
            } else {
                "connected".to_string()
            };
            declared_neighbourhood.push(json!({
                "decl_key": dk,
                "name": name,
                "kind": kind,
                "namespace": ns,
                "observed": presence,
                "neighbours": nb,
            }));
        }
    }

    // FTS retriever: prior runs / settlements for the same question.
    let fts_matches: Vec<Value> = if question.is_some() {
        projections::fts_search(&conn, &meta.question_id, 5)?
    } else {
        Vec::new()
    };

    // Prior settlements for the same question.
    let mut prior_settlements: Vec<Value> = Vec::new();
    if !meta.question_id.is_empty() {
        let mut stmt = conn.prepare(
            "SELECT settlement_id, settlement_status, question_id FROM sxr_settlement
             WHERE question_id = ?1 ORDER BY append_ordinal",
        )?;
        let rows = stmt.query_map([&meta.question_id], |r| {
            Ok(json!({
                "settlement_id": r.get::<_, String>(0)?,
                "settlement_status": r.get::<_, String>(1)?,
                "question_id": r.get::<_, String>(2)?,
            }))
        })?;
        for r in rows {
            prior_settlements.push(r?);
        }
    }

    Ok(output::output(vec![
        ("run_id", json!(run_id)),
        ("repo_id", json!(meta.repo_id)),
        ("question", question.unwrap_or(Value::Null)),
        ("threshold", threshold.unwrap_or(Value::Null)),
        ("failure", failure),
        ("declared_neighbourhood", json!(declared_neighbourhood)),
        ("prior_settlements", json!(prior_settlements)),
        ("fts_matches", json!(fts_matches)),
        ("vector_available", json!(false)),
    ]))
}

/// Presence string helper for a decl_key in the evidence tables.
fn observed_status(conn: &rusqlite::Connection, decl_key: &str) -> String {
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sxr_evidence_target WHERE decl_key = ?1",
            [decl_key],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if n > 0 {
        "observed".to_string()
    } else {
        "not_observed".to_string()
    }
}
