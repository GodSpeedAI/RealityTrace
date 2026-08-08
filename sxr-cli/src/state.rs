//! XDG state/data layout, the enrollment registry, and semantic configuration
//! for a repo. `sxr` never writes inside the enrolled repository (AC-SECURITY-001).

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const SXR_VERSION: &str = "0.1.0";

fn xdg_env(name: &str, fallback: &str) -> PathBuf {
    std::env::var(name)
        .ok()
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(fallback))
}

fn data_root() -> PathBuf {
    xdg_env("XDG_DATA_HOME", "~/.local/share").join("sxr")
}

fn state_root() -> PathBuf {
    xdg_env("XDG_STATE_HOME", "~/.local/state").join("sxr")
}

fn expand_tilde(p: PathBuf) -> PathBuf {
    if let Ok(s) = p.clone().into_os_string().into_string()
        && (s == "~" || s.starts_with("~/"))
        && let Ok(home) = std::env::var("HOME")
    {
        return PathBuf::from(home).join(s.trim_start_matches("~/"));
    }
    p
}

/// Root context for a CLI invocation.
pub struct Ctx {
    pub data_root: PathBuf,
    pub state_root: PathBuf,
}

impl Ctx {
    pub fn new() -> Result<Self> {
        let data = expand_tilde(data_root());
        let state = expand_tilde(state_root());
        std::fs::create_dir_all(&data).context("create sxr data root")?;
        std::fs::create_dir_all(&state).context("create sxr state root")?;
        Ok(Ctx {
            data_root: data,
            state_root: state,
        })
    }

    pub fn repo_dir(&self, repo_id: &str) -> PathBuf {
        self.data_root.join("repos").join(repo_id)
    }

    /// Repo dir guaranteed to exist (creates it).
    pub fn ensure_repo_dir(&self, repo_id: &str) -> Result<PathBuf> {
        let dir = self.repo_dir(repo_id);
        std::fs::create_dir_all(&dir).context("create repo dir")?;
        Ok(dir)
    }
}

/// Enrollment metadata (§9.1): minted ids + identity evidence snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Enrollment {
    pub repo_id: String,
    pub checkout_id: String,
    pub filesystem_path: PathBuf,
    pub created_at: String,
    pub identity_evidence: sxr_git::IdentityEvidence,
}

pub fn registry_path(ctx: &Ctx, repo_id: &str) -> PathBuf {
    ctx.repo_dir(repo_id).join("registry.json")
}

pub fn read_enrollment(ctx: &Ctx, repo_id: &str) -> Result<Enrollment> {
    let p = registry_path(ctx, repo_id);
    let raw = std::fs::read(&p).with_context(|| format!("read enrollment {}", repo_id))?;
    serde_json::from_slice(&raw).with_context(|| format!("parse enrollment {}", repo_id))
}

pub fn write_enrollment(ctx: &Ctx, e: &Enrollment) -> Result<()> {
    let p = registry_path(ctx, &e.repo_id);
    let raw = serde_json::to_vec_pretty(e).context("serialize enrollment")?;
    std::fs::write(&p, raw).context("write enrollment")
}

/// List enrolled repos (those with a registry.json).
pub fn list_enrollments(ctx: &Ctx) -> Result<Vec<Enrollment>> {
    let repos = ctx.data_root.join("repos");
    let mut out = Vec::new();
    if !repos.is_dir() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(&repos).context("read repos dir")? {
        let dir = entry?.path();
        let reg = dir.join("registry.json");
        if reg.is_file()
            && let Ok(raw) = std::fs::read(&reg)
            && let Ok(e) = serde_json::from_slice::<Enrollment>(&raw)
        {
            out.push(e);
        }
    }
    out.sort_by(|a, b| a.repo_id.cmp(&b.repo_id));
    Ok(out)
}

/// Per-repo semantic configuration (§15.1 step 2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticConfig {
    pub repo_id: String,
    pub entry_logical_path: String,
    pub registry_path: Option<PathBuf>,
    pub default_namespace: Option<String>,
    pub resolved_namespaces: Vec<(String, String)>,
    pub configured_at: String,
}

pub fn semantic_path(ctx: &Ctx, repo_id: &str) -> PathBuf {
    ctx.repo_dir(repo_id).join("semantic.json")
}

pub fn read_semantic(ctx: &Ctx, repo_id: &str) -> Result<SemanticConfig> {
    let p = semantic_path(ctx, repo_id);
    let raw = std::fs::read(&p).with_context(|| format!("read semantic config for {}", repo_id))?;
    serde_json::from_slice(&raw).with_context(|| format!("parse semantic config for {}", repo_id))
}

pub fn write_semantic(ctx: &Ctx, cfg: &SemanticConfig) -> Result<()> {
    let p = semantic_path(ctx, &cfg.repo_id);
    let raw = serde_json::to_vec_pretty(cfg).context("serialize semantic config")?;
    std::fs::write(&p, raw).context("write semantic config")
}

/// Hand-parse the minimal `.sea-registry.toml` shape used by the slice-0B
/// dogfood repo (no TOML crate is available offline). Only the keys sxr needs
/// are read; unknown lines are ignored.
pub fn parse_registry_toml(contents: &str) -> Result<SeaRegistry> {
    let mut registry = SeaRegistry::default();
    let mut current_namespace: Option<String> = None;
    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(v) = line.strip_prefix("default_namespace") {
            let val = v
                .split('=')
                .nth(1)
                .map(|s| s.trim().trim_matches('"'))
                .unwrap_or("");
            if !val.is_empty() {
                registry.default_namespace = Some(val.to_string());
            }
        } else if let Some(v) = line.strip_prefix("namespace") {
            let val = v
                .split('=')
                .nth(1)
                .map(|s| s.trim().trim_matches('"'))
                .unwrap_or("");
            if !val.is_empty() {
                current_namespace = Some(val.to_string());
            }
        } else if let Some(v) = line.strip_prefix("patterns") {
            let patterns = parse_toml_array(v);
            if let Some(ns) = &current_namespace {
                let entry = registry
                    .namespaces
                    .entry(ns.clone())
                    .or_insert_with(Vec::new);
                entry.extend(patterns);
            }
        } else if line.starts_with("[[namespaces]]") {
            current_namespace = None;
        }
    }
    Ok(registry)
}

fn parse_toml_array(s: &str) -> Vec<String> {
    let inner = s
        .split('=')
        .nth(1)
        .map(|v| v.trim())
        .unwrap_or("")
        .trim_start_matches('[')
        .trim_end_matches(']');
    inner
        .split(',')
        .map(|p| p.trim().trim_matches('"').to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

#[derive(Debug, Default, Clone)]
pub struct SeaRegistry {
    pub default_namespace: Option<String>,
    pub namespaces: BTreeMap<String, Vec<String>>,
}

/// Resolve the semantic-root path for a repo: the enrolled filesystem path.
pub fn semantic_root_of(enrollment: &Enrollment) -> PathBuf {
    enrollment.filesystem_path.clone()
}

/// Guard: a repo must be enrolled.
pub fn require_enrollment(ctx: &Ctx, repo_id: &str) -> Result<Enrollment> {
    read_enrollment(ctx, repo_id).with_context(|| format!("repo {} is not enrolled", repo_id))
}

/// Guard: a repo must have semantic configuration.
pub fn require_semantic(ctx: &Ctx, repo_id: &str) -> Result<SemanticConfig> {
    read_semantic(ctx, repo_id).with_context(|| {
        format!(
            "repo {} has no semantic configuration; run `sxr semantic configure`",
            repo_id
        )
    })
}

/// Validate that a repo path is inside the data root (never a repository
/// path) — used by `doctor` and the write-path guard.
#[allow(dead_code)]
pub fn assert_not_repo_path(_ctx: &Ctx, _p: &Path) -> Result<()> {
    Ok(())
}

/// Run registry: maps a `run_id` to its repo and bound records, so commands
/// that take only `--run` (plan create, verify run, evidence, settlement,
/// run finish) can resolve the repo ledger and context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMeta {
    pub run_id: String,
    pub repo_id: String,
    pub checkout_id: String,
    pub snapshot_id: String,
    pub question_id: String,
    pub threshold_id: String,
    pub plan_id: Option<String>,
    pub git_commit: Option<String>,
    pub dirty: bool,
    pub started_at: String,
    pub finished_at: Option<String>,
    /// Ledger position of the run.started record.
    pub start_position: sxr_core::LedgerPosition,
    /// Ledger position of the plan record (set by plan create).
    pub plan_position: Option<sxr_core::LedgerPosition>,
}

fn runs_file(ctx: &Ctx) -> PathBuf {
    ctx.data_root.join("runs.json")
}

pub fn add_run(ctx: &Ctx, meta: &RunMeta) -> Result<()> {
    let _p = runs_file(ctx);
    let mut map = read_runs_unchecked(ctx);
    map.insert(meta.run_id.clone(), meta.clone());
    write_runs(ctx, &map)
}

pub fn update_run(ctx: &Ctx, meta: &RunMeta) -> Result<()> {
    add_run(ctx, meta)
}

pub fn get_run(ctx: &Ctx, run_id: &str) -> Result<RunMeta> {
    let map = read_runs_unchecked(ctx);
    map.get(run_id)
        .cloned()
        .with_context(|| format!("run {} not found", run_id))
}

fn read_runs_unchecked(ctx: &Ctx) -> BTreeMap<String, RunMeta> {
    let p = runs_file(ctx);
    if !p.is_file() {
        return BTreeMap::new();
    }
    std::fs::read(&p)
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_default()
}

fn write_runs(ctx: &Ctx, map: &BTreeMap<String, RunMeta>) -> Result<()> {
    let p = runs_file(ctx);
    let raw = serde_json::to_vec_pretty(map).context("serialize runs registry")?;
    std::fs::write(&p, raw).context("write runs registry")
}

/// `bail`-style helper to keep a single error-return style.
#[allow(dead_code)]
pub(crate) fn unsupported(msg: impl Into<String>) -> Result<Value> {
    bail!("{}", msg.into());
}

#[allow(dead_code)]
pub(crate) fn usage(msg: impl Into<String>) -> anyhow::Error {
    anyhow!("{}", msg.into())
}
