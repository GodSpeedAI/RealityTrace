//! Command implementations for the sxr CLI. Each fn returns a JSON output
//! Value (the `sxr_output_version` envelope) that the caller prints.

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Map, Value, json};
use std::path::PathBuf;
use sxr_core::record_kinds::RecordKind;
use sxr_verify::VerifierPort;

use crate::ledger_io::{self, append_record, ledger_position_json, open_repo_ledger};
use crate::output;
use crate::projections;
use crate::state::{self, Ctx, Enrollment, RunMeta, SemanticConfig};

fn ulid() -> String {
    ulid::Ulid::new().to_string()
}

// ── repo commands ────────────────────────────────────────────────────────

pub fn repo_enroll(ctx: &Ctx, path: &str) -> Result<Value> {
    let fs_path = std::path::absolute(path).context("resolve path")?;
    if !fs_path.is_dir() {
        bail!("not a directory: {}", fs_path.display());
    }
    let evidence = sxr_git::discover(&sxr_git::GitCli, &fs_path)
        .map_err(|e| anyhow!("discover {}: {e}", fs_path.display()))?;
    let (repo_id, checkout_id) = sxr_git::mint_identities();
    let identity_evidence = sxr_git::evidence_of(&evidence, &fs_path);
    let enrollment = Enrollment {
        repo_id: repo_id.clone(),
        checkout_id,
        filesystem_path: fs_path.clone(),
        created_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
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
    std::env::var("SXR_DOMAINFORGE_BIN")
        .map(PathBuf::from)
        .ok()
        .or_else(|| {
            let p = PathBuf::from(
                "/home/sprime01/projects/domainforge-slice0b/target/debug/domainforge",
            );
            if p.is_file() { Some(p) } else { None }
        })
}

pub fn snapshot_create(ctx: &Ctx, repo_id: &str) -> Result<Value> {
    let e = state::require_enrollment(ctx, repo_id)?;
    let cfg = state::require_semantic(ctx, repo_id)?;
    let bin = domainforge_bin()
        .ok_or_else(|| anyhow!("SXR_DOMAINFORGE_BIN not set and default binary not found"))?;
    let adapter = sxr_df::CliDomainForgeAdapter::new(bin);
    let req = sxr_df::SemanticContextRequest {
        semantic_root: state::semantic_root_of(&e),
        entry_logical_path: cfg.entry_logical_path.clone(),
        registry_path: cfg.registry_path.clone(),
        default_namespace: cfg.default_namespace.clone(),
        semantic_packs: vec![],
        cep_scope: json!({}),
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
                "valid",
                json!([]),
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
                Some(
                    json!({"omission_id": "om:df", "omission_type": "input_failure", "description": "D not constructed"}),
                ),
                vec![],
                resolution_inputs.clone(),
            )
        }
    };
    let _ = semantic; // envelope kept producer-preserved below.

    let mut sections = Map::new();
    sections.insert("states".into(), json!([{
        "state_id": "state:snap",
        "state_type": "declared_model",
        "scope": format!("checkpoint:{}", content_hash.clone().unwrap_or_else(|| "invalid".into())),
    }]));
    sections.insert(
        "representations".into(),
        json!([{
            "representation_id": "representation:snap",
            "representation_kind": "atomic",
            "representation_type": "domainforge_canonical_d",
            "content_hash": content_hash,
            "semantic_hash": semantic_hash,
        }]),
    );
    sections.insert(
        "extensions".into(),
        json!([{
            "extension_id": "ext:df",
            "extension_type": "domainforge",
            "diagnostics": diagnostics,
            "omission": omission_opt,
        }]),
    );

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
        &json!(sections),
    )?;

    let mut out = vec![
        ("snapshot_id", json!(snapshot_id)),
        ("declaration_count", json!(declarations.len())),
        (
            "resolution_inputs",
            serde_json::to_value(&resolution_inputs)?,
        ),
        ("drift_class", Value::Null),
        ("durability", json!("committed")),
        ("ledger_position", ledger_position_json(&pos)),
    ];
    if model_status == "valid" {
        out.push(("semantic_closure_hash", json!(semantic_hash)));
        out.push(("content_hash", json!(content_hash)));
    } else {
        out.push(("model_validation_status", json!("invalid")));
        out.push(("diagnostics", diagnostics));
        out.push((
            "representation_omission",
            omission_opt.unwrap_or(Value::Null),
        ));
    }
    Ok(output::output(out))
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

pub fn plan_create(ctx: &Ctx, run_id: &str, verifier: &str, target: &str) -> Result<Value> {
    let mut meta = repo_for_run(ctx, run_id)?;
    let e = state::require_enrollment(ctx, &meta.repo_id)?;
    let plan_id = ulid();
    let transformation_ref = format!("transformation:plan:{plan_id}");
    let plan = json!({
        "plan_id": plan_id,
        "plan_version": "sxr-plan/v1",
        "semantic_verification_contract_ref": null,
        "evaluation_profile_ref": "profile://slice2-policy@1",
        "question_ref": meta.question_id,
        "threshold_ref": meta.threshold_id,
        "snapshot_ref": meta.snapshot_id,
        "transformation_ref": transformation_ref,
        "steps": [{
            "step_id": "unit",
            "verifier_id": verifier,
            "invocation": {
                "argv": ["cargo", "test", "--offline", "--message-format", "json"],
                "cwd": ".",
                "env_allowlist": ["PATH", "HOME", "CARGO_HOME"],
                "timeout_s": 900,
            },
            "produces_evidence_type": "test_result",
            "targets": [target],
            "expected_answer_shape": "boolean_with_failure_list",
            "relevance_to_question": "high",
        }],
        "evidence_requirements": [{
            "evidence_type": "test_result",
            "min_count": 1,
            "min_relevance": "high",
            "min_reliability": "medium",
        }],
        "boundary": {
            "excluded": ["browser", "performance", "security"],
            "limitations": ["offline only", "slice-1 cargo-test"],
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
    let subject_refs = vec![
        format!("plan:{plan_id}"),
        format!("run:{}", run_id),
        format!("question:{}", meta.question_id),
        format!("threshold:{}", meta.threshold_id),
        format!("snapshot:{}", meta.snapshot_id),
        format!("target:{target}"),
    ];
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

// ── verify run ───────────────────────────────────────────────────────────

fn step_from_plan(
    _meta: &RunMeta,
    target: &str,
    cwd: &std::path::Path,
) -> sxr_verify::VerificationStep {
    sxr_verify::VerificationStep {
        step_id: "unit".into(),
        obligation_ref: None,
        claim_ref: None,
        evidence_requirement_ref: "contract://test#evidence/unit".into(),
        verifier_id: "cargo-test".into(),
        invocation: sxr_verify::Invocation {
            argv: vec![
                "cargo".into(),
                "test".into(),
                "--offline".into(),
                "--message-format".into(),
                "json".into(),
            ],
            cwd: cwd.into(),
            env_allowlist: vec!["PATH".into(), "HOME".into(), "CARGO_HOME".into()],
            timeout_s: 900,
        },
        produces_evidence_type: "test_result".into(),
        targets: vec![target.into()],
        expected_answer_shape: "boolean_with_failure_list".into(),
        on_crash: "record_omission".into(),
    }
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

    let step = step_from_plan(&meta, &target, &e.filesystem_path);
    let artifacts_dir = ctx.ensure_repo_dir(&meta.repo_id)?.join("artifacts");
    let verifier = sxr_verify::CargoTestVerifier;
    let outcome = verifier
        .execute(
            &step,
            &sxr_verify::ExecutionContext::new(artifacts_dir.clone()),
        )
        .map_err(|e| anyhow!("cargo-test verifier failed: {e}"))?;
    let observation_id = ulid();
    let sections = json!({
        "observations": [{ "observation_id": observation_id, "scope": format!("run:{run_id}") }],
        "representations": [{
            "representation_id": format!("rep:obs:{observation_id}"),
            "representation_kind": "atomic",
            "representation_type": "verifier_stdout",
            "content_hash": outcome.stdout_ref,
            "inline_content": {
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
                let targets = content
                    .get("steps")
                    .and_then(Value::as_array)
                    .and_then(|a| a.first())
                    .and_then(|s| s.get("targets"))
                    .cloned()
                    .unwrap_or(json!([]));
                return Ok(json!({ "targets": targets, "plan_id": plan_id }));
            }
        }
    }
    bail!("plan for run {} not found; run `sxr plan create`", run_id)
}

// ── evidence create ──────────────────────────────────────────────────────

pub fn evidence_create(ctx: &Ctx, run_id: &str, observation_id: &str) -> Result<Value> {
    let meta = repo_for_run(ctx, run_id)?;
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
            "verifier-profile://cargo-test@1#exit_code_to_direction",
            &json!({"exit_code": obs_row.exit_code, "crashed": obs_row.crashed}),
            &plan_pos,
            precedes,
        ),
        deriv(
            "evidence_type",
            "verifier-profile://cargo-test@1#evidence_types",
            &json!({"evidence_types": ["test_result"]}),
            &plan_pos,
            precedes,
        ),
        deriv(
            "reliability",
            "verifier-profile://cargo-test@1#reliability_model",
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
            "verifier-profile://cargo-test@1",
            &json!({"id": "cargo-test", "version": "1"}),
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
            "evidence_type": "test_result",
            "question_ref": meta.question_id,
            "target_entity_ref": format!("concept:{}", target.trim_start_matches("concept:")),
            "representation_ref": format!("rep:obs:{observation_id}"),
            "relevance": "high",
            "reliability": "unknown",
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
            return Ok(ObsInfo {
                pos,
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
) -> Result<Value> {
    let meta = repo_for_run(ctx, run_id)?;
    let e = state::require_enrollment(ctx, &meta.repo_id)?;
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

    let has_evidence = !evidence_positions.is_empty();

    // §14.4/AC-SETTLEMENT-002: if snapshot is validation-failed, only negative
    // settlement is permitted.
    let snapshot_status = snapshot_model_status(ctx, &meta.repo_id, &meta.snapshot_id)?;
    let requested: &str = if has_evidence { "settled" } else { "unsettled" };
    if snapshot_status.as_deref() == Some("invalid")
        && requested == "settled"
        && !question_is_invalidity(question_id)
    {
        bail!("AC-SETTLEMENT-002: invalid declared model cannot support positive acceptance");
    }

    let settlement_status = if has_evidence { requested } else { "unsettled" };
    let settlement_id = ulid();
    let sections = json!({
        "settlements": [{
            "settlement_id": settlement_id,
            "settlement_status": settlement_status,
            "transformation_ref": transformation_ref,
            "question_ref": question_id,
            "threshold_ref": threshold_id,
        }]
    });
    let subject_refs = vec![
        format!("settlement:{}", settlement_id),
        format!("question:{}", question_id),
        format!("run:{}", run_id),
        format!(
            "transformation:{}",
            transformation_ref.trim_start_matches("transformation:")
        ),
        format!("snapshot:{}", meta.snapshot_id),
    ];
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
    Ok(output::output(vec![
        ("settlement_id", json!(settlement_id)),
        ("settlement_status", json!(settlement_status)),
        (
            "missing_evidence",
            json!(if has_evidence {
                vec![]
            } else {
                vec!["no supporting evidence".to_string()]
            }),
        ),
        ("unresolved_contradictions", json!([])),
        ("state_update_recommendation", json!("none")),
        ("durability", json!("committed")),
        ("ledger_position", ledger_position_json(&pos)),
    ]))
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

fn snapshot_model_status(ctx: &Ctx, repo_id: &str, snapshot_id: &str) -> Result<Option<String>> {
    let rows = ledger_io::read_all(ctx, repo_id)?;
    for r in &rows {
        if r.record_kind == "semantic.snapshot"
            && r.subject_refs
                .iter()
                .any(|s| s.as_str() == format!("snapshot:{snapshot_id}"))
        {
            let payload: Value =
                serde_json::from_str(&r.payload).context("parse snapshot payload")?;
            let has_hashes = payload
                .get("representations")
                .and_then(Value::as_array)
                .and_then(|a| a.first())
                .map(|rep| rep.get("semantic_hash").is_some() || rep.get("content_hash").is_some())
                .unwrap_or(false);
            return Ok(Some(if has_hashes { "valid" } else { "invalid" }.into()));
        }
    }
    Ok(None)
}

fn question_is_invalidity(_qid: &str) -> bool {
    false
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
    let dir = ctx.ensure_repo_dir(repo_id)?;
    let db_path = dir.join("ledger.db");
    let ledger = sxr_ledger::ledger::Ledger::open(
        &db_path,
        repo_id,
        sxr_ledger::ledger::LedgerConfig::default(),
    )
    .map_err(|e| anyhow!("open ledger: {e}"))?;
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
    let rows = ledger_io::read_all(ctx, repo_id)?;
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
    let rows = ledger_io::read_all(ctx, repo_id)?;
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
    let dir = ctx.ensure_repo_dir(repo_id)?;
    let conn = rusqlite::Connection::open(dir.join("ledger.db")).context("open ledger db")?;
    // Ensure projections are present.
    projections::rebuild(ctx, repo_id)?;
    let mut stmt = conn.prepare(
        "SELECT e.evidence_id, e.evidence_type, e.direction, e.relevance, e.reliability
         FROM sxr_evidence_target t
         JOIN sxr_evidence e ON e.evidence_id = t.evidence_id
         WHERE t.decl_key = ?1
         ORDER BY e.append_ordinal",
    )?;
    let evidence: Vec<Value> = stmt
        .query_map([entity], |r| {
            Ok(json!({
                "evidence_id": r.get::<_, String>(0)?,
                "evidence_type": r.get::<_, String>(1)?,
                "direction": r.get::<_, String>(2)?,
                "relevance": r.get::<_, String>(3)?,
                "reliability": r.get::<_, String>(4)?,
            }))
        })?
        .filter_map(|r| r.ok())
        .collect();
    if evidence.is_empty() {
        Ok(output::output1("unobserved", json!(true)))
    } else {
        Ok(output::output1("evidence", json!(evidence)))
    }
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

// ── doctor ───────────────────────────────────────────────────────────────

pub fn doctor(ctx: &Ctx) -> Result<Value> {
    let enrollments = state::list_enrollments(ctx)?;
    let repos: Vec<Value> = enrollments
        .iter()
        .map(|e| {
            json!({
                "repo_id": e.repo_id,
                "filesystem_path": e.filesystem_path.to_string_lossy(),
            })
        })
        .collect();
    Ok(output::output(vec![
        ("data_root", json!(ctx.data_root.to_string_lossy())),
        ("state_root", json!(ctx.state_root.to_string_lossy())),
        ("enrolled_repos", json!(repos)),
        ("filesystem_class", json!("safe")),
        ("daemon_active", json!(false)),
    ]))
}

// ── misc ─────────────────────────────────────────────────────────────────

pub fn delta(_ctx: &Ctx, repo_id: &str, question_id: &str, snapshot_id: &str) -> Result<Value> {
    Ok(output::output(vec![
        ("repo_id", json!(repo_id)),
        ("question_id", json!(question_id)),
        ("snapshot_id", json!(snapshot_id)),
        ("note", json!("slice-1 deferred")),
    ]))
}

pub fn spool_status(_ctx: &Ctx, repo_id: &str) -> Result<Value> {
    Ok(output::output(vec![
        ("repo_id", json!(repo_id)),
        ("spooled_count", json!(0)),
    ]))
}

pub fn spool_drain(_ctx: &Ctx, repo_id: &str) -> Result<Value> {
    Ok(output::output(vec![
        ("repo_id", json!(repo_id)),
        ("drained", json!(0)),
    ]))
}

pub fn await_run(_ctx: &Ctx, run_id: &str) -> Result<Value> {
    Ok(output::output(vec![
        ("run_id", json!(run_id)),
        ("state", json!("committed")),
    ]))
}

pub fn ingest(_ctx: &Ctx, run_id: &str, verifier: &str) -> Result<Value> {
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut std::io::stdin(), &mut buf).context("read stdin")?;
    let parsed = sxr_verify::parse_cargo_json(&buf);
    Ok(output::output(vec![
        ("run_id", json!(run_id)),
        ("verifier", json!(verifier)),
        ("bytes_received", json!(buf.len())),
        (
            "parsed",
            if let Some(p) = parsed {
                json!({"test_count": p.test_count, "passed": p.passed, "failed": p.failed, "ignored": p.ignored})
            } else {
                Value::Null
            },
        ),
    ]))
}

pub fn explain(_ctx: &Ctx, record_id: &str) -> Result<Value> {
    Ok(output::output(vec![
        ("record_id", json!(record_id)),
        ("note", json!("slice-1 slim")),
    ]))
}

pub fn context(_ctx: &Ctx, repo_id: &str, run_id: &str) -> Result<Value> {
    Ok(output::output(vec![
        ("repo_id", json!(repo_id)),
        ("run_id", json!(run_id)),
    ]))
}
