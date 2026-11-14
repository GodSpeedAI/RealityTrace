//! `sxr` binary: manual lifecycle commands with `--json` output and §15 exit
//! codes (0 ok, 2 usage, 3 precondition, 4 CEP validation, 5 integrity,
//! 6 verifier, 75 spooled).

use anyhow::{Result, anyhow};
use clap::{Parser, Subcommand};
use serde_json::{Value, json};

mod agent_event;
mod commands;
mod daemon;
mod determinism;
mod durability;
mod envelopes;
mod ledger_io;
mod observed;
mod output;
mod projections;
mod state;

use state::Ctx;

/// RealityTrace — the Semantic Exhaust Runtime CLI.
#[derive(Parser)]
#[command(
    name = "realitytrace",
    version,
    about = "RealityTrace — the Semantic Exhaust Runtime CLI"
)]
struct Cli {
    /// Emit machine-readable JSON on stdout (always on for slice 1).
    #[arg(long, global = true, action = clap::ArgAction::SetTrue)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Repository enrollment and identity.
    #[command(subcommand)]
    Repo(RepoCmds),
    /// Semantic context configuration.
    #[command(subcommand)]
    Semantic(SemanticCmds),
    /// Create a semantic snapshot from DomainForge.
    #[command(subcommand)]
    Snapshot(SnapshotCmds),
    /// Declare a Question.
    #[command(subcommand)]
    Question(QuestionCmds),
    /// Declare a Threshold.
    #[command(subcommand)]
    Threshold(ThresholdCmds),
    /// Run lifecycle.
    #[command(subcommand)]
    Run(RunCmds),
    /// Build a verification plan.
    #[command(subcommand)]
    Plan(PlanCmds),
    /// Import a DomainForge verification contract (§14.0).
    #[command(subcommand)]
    Contract(ContractCmds),
    /// Execute the plan's verifier (cargo-test or command); other verifier ids are refused.
    #[command(subcommand)]
    Verify(VerifyCmds),
    /// Derive Evidence from an observation.
    #[command(subcommand)]
    Evidence(EvidenceCmds),
    /// Create a relation Claim (§12.4a).
    #[command(subcommand)]
    Claim(ClaimCmds),
    /// Register a Settlement.
    #[command(subcommand)]
    Settlement(SettlementCmds),
    /// Ingest a verifier stream from stdin.
    Ingest(IngestCmd),
    /// Ledger integrity / export / history.
    #[command(subcommand)]
    Ledger(LedgerCmds),
    /// Query derived state.
    #[command(subcommand)]
    Query(QueryCmds),
    /// Projection lifecycle.
    #[command(subcommand)]
    Projections(ProjectionsCmds),
    /// Spool inspection.
    #[command(subcommand)]
    Spool(SpoolCmds),
    /// Await a run's records.
    Await(AwaitCmd),
    /// Diagnostics.
    Doctor,
    /// Durable commit queue recovery (§16.4, F-19c).
    #[command(subcommand)]
    Queue(QueueCmds),
    /// Optional daemon (§16).
    Daemon(DaemonCmd),
    /// Delta inspection.
    Delta(DeltaCmd),
    /// Diagnostic attribution loop (§12.5a).
    #[command(subcommand)]
    Diagnostic(DiagnosticCmds),
    /// Commit residual.computed records (§12.5a).
    #[command(subcommand)]
    Residual(ResidualCmds),
    /// Explain a record's provenance.
    Explain(ExplainCmd),
    /// Failure context pack.
    Context(ContextCmd),
    /// Interaction-event ingest and the observed representation (WP-16).
    #[command(subcommand)]
    Observed(ObservedCmds),
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)] // clap argument bag, built once per process
enum ObservedCmds {
    /// Classify CEP `execution_trace` envelopes carrying
    /// `extensions["godspeed.interaction_event"]` (NDJSON, `-` for stdin) and
    /// record each as `observation.recorded` or `ingest.rejected`. Exits 4 if any
    /// line was rejected (the rejections are recorded).
    Ingest {
        #[arg(long)]
        repo: String,
        /// NDJSON file, or `-` for stdin.
        #[arg(long)]
        file: String,
    },
    /// Derive the observed representation from the verified ledger. Read-only.
    /// Fails with CHAIN_VERIFY_FAILED if the hash chain does not verify.
    Derive {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        world: String,
        /// `case:<id>`.
        #[arg(long)]
        scope: String,
        #[arg(long)]
        episode: String,
        /// `evaluated` or `exploratory`.
        #[arg(long)]
        kind: String,
        /// Window end, inclusive: append ordinal, entry id, or RFC 3339
        /// ledger commit time. Default: ledger head.
        #[arg(long)]
        until: Option<String>,
        /// Comparison profile id recorded in the representation.
        #[arg(long, default_value = "profile:s1-default")]
        profile: String,
        /// JSON array of the pinned world's declaration keys; observed keys
        /// outside it become `undeclared:` nodes.
        #[arg(long)]
        declared: Option<String>,
        /// JSON array of `{emitter, event_kind}` frozen with the episode.
        #[arg(long)]
        coverage_manifest: Option<String>,
        /// Source system known independent of the executor (repeatable).
        #[arg(long = "independent-system")]
        independent_system: Vec<String>,
        /// Write the representation here instead of stdout.
        #[arg(long)]
        out: Option<String>,
        /// `entry_hash` of the last ledger row at anchor time. With it, a
        /// ledger that lost that row fails with TAIL_TRUNCATED; without it
        /// (and without --expected-record-count) the tail is `unverified`.
        #[arg(long)]
        expected_head: Option<String>,
        /// Number of ledger rows at anchor time.
        #[arg(long)]
        expected_record_count: Option<u64>,
    },
    /// Compare an observed representation with the declared expectations and
    /// print residual records (episode and structural lists, never mixed).
    /// Writes nothing but `--facts-out`. Refuses on a profile digest that is
    /// not the one pinned in the frozen reference.
    Compare {
        #[arg(long)]
        repo: String,
        /// The `observed derive` output file.
        #[arg(long)]
        observed: String,
        /// The frozen reference `{episode_ref, body}` (JSON file).
        #[arg(long)]
        reference: String,
        /// The comparison profile document (its digest must equal the pin).
        #[arg(long)]
        profile: String,
        /// The key and expectation mapping (compare-mapping.v1).
        #[arg(long)]
        mapping: String,
        #[arg(long)]
        world: String,
        /// Declared snapshot id the comparison runs against.
        #[arg(long = "declared")]
        declared: String,
        /// The declared model (.sea) given to DomainForge `evaluate`.
        #[arg(long)]
        model: Option<String>,
        /// DomainForge binary (else REALITYTRACE_DOMAINFORGE_BIN). Needs `evaluate`.
        #[arg(long)]
        domainforge_bin: Option<String>,
        /// Write the generated facts file here (it is generated, never edited).
        #[arg(long)]
        facts_out: Option<String>,
        #[arg(long)]
        expected_head: Option<String>,
        #[arg(long)]
        expected_record_count: Option<u64>,
        /// Write the result here instead of stdout.
        #[arg(long)]
        out: Option<String>,
    },
}

fn head_from(
    entry_hash: Option<String>,
    record_count: Option<u64>,
) -> Option<sxr_observed::tail::ExpectedHead> {
    (entry_hash.is_some() || record_count.is_some()).then_some(sxr_observed::tail::ExpectedHead {
        entry_hash,
        record_count,
    })
}

#[derive(Subcommand)]
enum RepoCmds {
    Enroll { path: String },
    List,
    Identity { repo_id: String },
}

#[derive(Subcommand)]
enum SemanticCmds {
    Configure {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        entry: String,
    },
    Status {
        #[arg(long)]
        repo: String,
    },
}

#[derive(Subcommand)]
enum SnapshotCmds {
    Create {
        #[arg(long)]
        repo: String,
    },
}

#[derive(Subcommand)]
enum QuestionCmds {
    Declare {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        file: String,
    },
}

#[derive(Subcommand)]
enum ThresholdCmds {
    Declare {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        question: String,
        #[arg(long)]
        file: String,
    },
}

#[derive(Subcommand)]
enum RunCmds {
    Begin {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        snapshot: String,
        #[arg(long)]
        question: String,
        #[arg(long)]
        threshold: String,
        /// Opt-in: fail unless the question and threshold are already
        /// declared in the ledger (default: not checked).
        #[arg(long)]
        require_declared: bool,
    },
    Finish {
        #[arg(long)]
        run: String,
    },
}

#[derive(Subcommand)]
enum PlanCmds {
    Create {
        #[arg(long)]
        run: String,
        #[arg(long)]
        verifier: String,
        #[arg(long)]
        target: String,
        /// Build the plan from a §14.0 verification contract obligation.
        #[arg(long)]
        from_contract: Option<String>,
        /// Obligation id to bind when --from-contract is set.
        #[arg(long)]
        obligation: Option<String>,
        /// `--verifier command` only: the exact argv to run, as a JSON array of strings.
        #[arg(long)]
        argv_json: Option<String>,
        /// `--verifier command` only: working directory relative to the enrolled repo (default ".").
        #[arg(long)]
        cwd: Option<String>,
        /// `--verifier command` only: timeout in seconds (default 900, at most 86400).
        #[arg(long)]
        timeout_s: Option<u64>,
        /// `--verifier command` only: a launcher prefix (e.g. a sandbox command), as a JSON array.
        #[arg(long)]
        launcher_json: Option<String>,
    },
}

#[derive(Subcommand)]
enum ContractCmds {
    Import {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        file: String,
    },
}

#[derive(Subcommand)]
enum VerifyCmds {
    Run {
        #[arg(long)]
        run: String,
    },
}

#[derive(Subcommand)]
enum EvidenceCmds {
    Create {
        #[arg(long)]
        run: String,
        #[arg(long)]
        observation: String,
        #[arg(long, action = clap::ArgAction::SetTrue)]
        derive: bool,
        /// Evidence type for the record (§26). Defaults to "test_result".
        #[arg(long, default_value = "test_result")]
        evidence_type: String,
    },
    /// Create evidence targeting a relation claim (§12.4a condition 2).
    ForClaim {
        #[arg(long)]
        run: String,
        #[arg(long)]
        question: String,
        #[arg(long)]
        claim: String,
        #[arg(long)]
        direction: String,
    },
    /// Create discriminating evidence targeting a diagnostic hypothesis
    /// (§12.5a; `supports` = discriminating, `contradicts` = exclusion).
    ForHypothesis {
        #[arg(long)]
        run: String,
        #[arg(long)]
        question: String,
        #[arg(long)]
        hypothesis: String,
        #[arg(long)]
        direction: String,
    },
}

#[derive(Subcommand)]
enum ResidualCmds {
    /// Commit residual.computed records for a question's snapshot delta
    /// (§12.5a). `declared_unobserved` residuals never enter the loop.
    Compute {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        question: String,
        #[arg(long)]
        snapshot: String,
    },
}

#[derive(Subcommand)]
enum DiagnosticCmds {
    /// Create the diagnostic question + threshold and one hypothesis per
    /// candidate failure locus (§12.5a). Idempotent.
    Enumerate {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        question: String,
        #[arg(long)]
        snapshot: String,
        #[arg(long)]
        residual: String,
    },
    /// Promote a hypothesis to diagnostic.settled — only under all four
    /// §12.5a conditions (else exits 3).
    Settle {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        question: String,
        #[arg(long)]
        hypothesis: String,
    },
    /// Raise a repair proposal bound to a diagnostic settlement. Model-locus
    /// proposals require cheap loci evidence-excluded first (AC-DIAG-001).
    Propose {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        question: String,
        #[arg(long)]
        hypothesis: String,
        #[arg(long)]
        locus: String,
        #[arg(long)]
        target: String,
    },
    /// §17.2-shaped diagnostic loop state for one residual.
    Status {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        residual: String,
    },
}

#[derive(Subcommand)]
enum ClaimCmds {
    Create {
        #[arg(long)]
        run: String,
        #[arg(long)]
        question: String,
        #[arg(long)]
        src: String,
        #[arg(long)]
        dst: String,
        #[arg(long, default_value = "relation")]
        kind: String,
    },
}

#[derive(Subcommand)]
enum SettlementCmds {
    Register {
        #[arg(long)]
        run: String,
        #[arg(long)]
        question: String,
        #[arg(long)]
        threshold: String,
        #[arg(long)]
        transformation: String,
        /// Record the settlement as `settled_with_debt` and append a
        /// `debt.recorded` record naming this debt (§26/CEP-0007 §18).
        #[arg(long)]
        debt: Option<String>,
    },
}

#[derive(clap::Args)]
struct IngestCmd {
    #[arg(long)]
    run: String,
    #[arg(long)]
    verifier: String,
    #[arg(long)]
    stdin_format: String,
    /// Opt-in spooling (§11.1a): write the unvalidated request to the spool
    /// and exit 75 — never a silent fallback on lock contention.
    #[arg(long)]
    spool: bool,
    /// Submit through the daemon queue (§16.1): exit 0 with
    /// `queued_for_commit` only after a fsynced receipt; transparently falls
    /// back to a direct commit when the daemon is unavailable (F-15).
    #[arg(long)]
    async_mode: bool,
    /// Caller-owned CEP §39 extensions: a JSON object stored verbatim as the
    /// record's `extensions` section (e.g. `{"cognate":{"eventId":"…"}}`).
    /// Not part of the idempotency key; the `domainforge` key is reserved.
    #[arg(long)]
    extensions: Option<String>,
}

#[derive(Subcommand)]
enum LedgerCmds {
    Verify {
        #[arg(long)]
        repo: String,
    },
    Export {
        #[arg(long)]
        repo: String,
    },
    History {
        #[arg(long)]
        repo: String,
    },
}

#[derive(Subcommand)]
enum QueryCmds {
    Evidence {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        entity: String,
    },
}

#[derive(Subcommand)]
enum ProjectionsCmds {
    Status {
        #[arg(long)]
        repo: String,
    },
    Rebuild {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        all: bool,
    },
    Export {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        canonical_json: bool,
    },
    /// Export sxr records as an external anti-collapse corpus (§24 Slice 4 gate).
    ExportAntiCollapseCorpus {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        question: String,
        #[arg(long)]
        snapshot: String,
    },
}

#[derive(Subcommand)]
enum SpoolCmds {
    Status {
        #[arg(long)]
        repo: String,
    },
    Drain {
        #[arg(long)]
        repo: String,
    },
}

#[derive(clap::Args)]
struct AwaitCmd {
    /// Wait for a specific run's records (alternative to --repo --all).
    #[arg(long)]
    run: Option<String>,
    /// Wait until the repo's queue and spool are fully drained.
    #[arg(long)]
    repo: Option<String>,
    #[arg(long)]
    all: bool,
    /// Fail with the remaining records listed after this many seconds.
    #[arg(long, default_value_t = 30)]
    timeout_secs: u64,
    #[arg(long, default_value_t = 100)]
    interval_ms: u64,
}

#[derive(clap::Args)]
struct DeltaCmd {
    #[arg(long)]
    repo: String,
    #[arg(long)]
    question: String,
    #[arg(long)]
    snapshot: String,
}

#[derive(Subcommand)]
enum QueueCmds {
    /// Drain pending daemon-queue segments into the ledger with the daemon
    /// stopped; duplicate replay is a no-op by idempotency key (§16.4).
    Recover {
        /// Restrict to one repo; default: every enrolled repo.
        #[arg(long)]
        repo: Option<String>,
    },
}

#[derive(clap::Args)]
struct DaemonCmd {
    /// Socket path (default `$XDG_RUNTIME_DIR/sxr.sock`, or
    /// `$SXR_DAEMON_SOCKET`).
    #[arg(long)]
    socket: Option<String>,
    /// Pacing of the queue→ledger drain loop (test hook; a long delay makes
    /// kill-after-receipt deterministic).
    #[arg(long, default_value_t = 250)]
    drain_delay_ms: u64,
}

#[derive(clap::Args)]
struct ExplainCmd {
    record_id: String,
}

#[derive(clap::Args)]
struct ContextCmd {
    #[arg(long)]
    repo: String,
    #[arg(long)]
    failure: String,
}

fn main() {
    let prog = std::env::args()
        .next()
        .as_ref()
        .and_then(|s| {
            std::path::Path::new(s)
                .file_name()?
                .to_str()
                .map(String::from)
        })
        .unwrap_or_else(|| "realitytrace".to_string());
    let cli = Cli::parse();
    let ctx = match Ctx::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{prog}: {e:#}");
            std::process::exit(2);
        }
    };
    let result = dispatch(&ctx, cli.command);
    match result {
        Ok(Dispatch::Json(v)) => output::print_json(&v),
        Ok(Dispatch::JsonExit(v, code)) => {
            output::print_json(&v);
            std::process::exit(code);
        }
        Ok(Dispatch::Raw(s)) => {
            println!("{s}");
        }
        Err(e) => {
            eprintln!("{prog}: {e:#}");
            std::process::exit(exit_for_error(&e));
        }
    }
}

enum Dispatch {
    Json(Value),
    /// A JSON output followed by a specific exit code (75 spooled, doctor 2).
    JsonExit(Value, i32),
    Raw(String),
}

fn exit_for_error(e: &anyhow::Error) -> i32 {
    // Map known error contexts to §15 exit codes by message prefix.
    let msg = e.to_string();
    if msg.contains("CHAIN_VERIFY_FAILED") || msg.contains("TAIL_TRUNCATED") {
        return 5;
    }
    if msg.contains("AC-SETTLEMENT")
        || msg.contains("AC-EVIDENCE")
        || msg.contains("AC-DIAG-001")
        || msg.contains("precondition")
        || msg.contains("predecessor rule")
        || msg.contains("not found")
        || msg.contains("not enrolled")
        || msg.contains("no semantic configuration")
        || msg.contains("queued_for_commit")
        || msg.contains("spooled")
        || msg.contains("not committed")
        || msg.contains("corrupt segments")
    {
        3
    } else {
        2
    }
}

fn dispatch(ctx: &Ctx, cmd: Command) -> Result<Dispatch> {
    let v = match cmd {
        Command::Repo(c) => match c {
            RepoCmds::Enroll { path } => commands::repo_enroll(ctx, &path)?,
            RepoCmds::List => commands::repo_list(ctx)?,
            RepoCmds::Identity { repo_id } => commands::repo_identity(ctx, &repo_id)?,
        },
        Command::Semantic(c) => match c {
            SemanticCmds::Configure { repo, entry } => {
                commands::semantic_configure(ctx, &repo, &entry)?
            }
            SemanticCmds::Status { repo } => commands::semantic_status(ctx, &repo)?,
        },
        Command::Snapshot(c) => match c {
            SnapshotCmds::Create { repo } => commands::snapshot_create(ctx, &repo)?,
        },
        Command::Question(c) => match c {
            QuestionCmds::Declare { repo, file } => commands::question_declare(ctx, &repo, &file)?,
        },
        Command::Threshold(c) => match c {
            ThresholdCmds::Declare {
                repo,
                question,
                file,
            } => commands::threshold_declare(ctx, &repo, &question, &file)?,
        },
        Command::Run(c) => match c {
            RunCmds::Begin {
                repo,
                snapshot,
                question,
                threshold,
                require_declared,
            } => commands::run_begin(
                ctx,
                &repo,
                &snapshot,
                &question,
                &threshold,
                require_declared,
            )?,
            RunCmds::Finish { run } => commands::run_finish(ctx, &run)?,
        },
        Command::Plan(c) => match c {
            PlanCmds::Create {
                run,
                verifier,
                target,
                from_contract,
                obligation,
                argv_json,
                cwd,
                timeout_s,
                launcher_json,
            } => commands::plan_create(
                ctx,
                &run,
                &verifier,
                &target,
                from_contract.as_deref(),
                obligation.as_deref(),
                &commands::CommandArgs {
                    argv_json,
                    cwd,
                    timeout_s,
                    launcher_json,
                },
            )?,
        },
        Command::Contract(c) => match c {
            ContractCmds::Import { repo, file } => commands::contract_import(ctx, &repo, &file)?,
        },
        Command::Verify(c) => match c {
            VerifyCmds::Run { run } => commands::verify_run(ctx, &run)?,
        },
        Command::Evidence(c) => match c {
            EvidenceCmds::Create {
                run,
                observation,
                derive,
                evidence_type,
            } => {
                if !derive {
                    return Err(anyhow!("only --derive is supported in slice 1"));
                }
                commands::evidence_create(ctx, &run, &observation, &evidence_type)?
            }
            EvidenceCmds::ForClaim {
                run,
                question,
                claim,
                direction,
            } => commands::evidence_for_claim(ctx, &run, &question, &claim, &direction)?,
            EvidenceCmds::ForHypothesis {
                run,
                question,
                hypothesis,
                direction,
            } => commands::evidence_for_hypothesis(ctx, &run, &question, &hypothesis, &direction)?,
        },
        Command::Residual(c) => match c {
            ResidualCmds::Compute {
                repo,
                question,
                snapshot,
            } => commands::residual_compute(ctx, &repo, &question, &snapshot)?,
        },
        Command::Diagnostic(c) => match c {
            DiagnosticCmds::Enumerate {
                repo,
                question,
                snapshot,
                residual,
            } => commands::diagnostic_enumerate(ctx, &repo, &question, &snapshot, &residual)?,
            DiagnosticCmds::Settle {
                repo,
                question,
                hypothesis,
            } => commands::diagnostic_settle(ctx, &repo, &question, &hypothesis)?,
            DiagnosticCmds::Propose {
                repo,
                question,
                hypothesis,
                locus,
                target,
            } => commands::diagnostic_propose(ctx, &repo, &question, &hypothesis, &locus, &target)?,
            DiagnosticCmds::Status { repo, residual } => {
                commands::diagnostic_status(ctx, &repo, &residual)?
            }
        },
        Command::Claim(c) => match c {
            ClaimCmds::Create {
                run,
                question,
                src,
                dst,
                kind,
            } => commands::claim_create(ctx, &run, &question, &src, &dst, &kind)?,
        },
        Command::Settlement(c) => match c {
            SettlementCmds::Register {
                run,
                question,
                threshold,
                transformation,
                debt,
            } => commands::settlement_register(
                ctx,
                &run,
                &question,
                &threshold,
                &transformation,
                debt.as_deref(),
            )?,
        },
        Command::Ingest(c) => {
            let (v, code) = commands::ingest(
                ctx,
                &c.run,
                &c.verifier,
                &c.stdin_format,
                c.spool,
                c.async_mode,
                c.extensions.as_deref(),
            )?;
            return Ok(Dispatch::JsonExit(v, code));
        }
        Command::Ledger(c) => match c {
            LedgerCmds::Verify { repo } => commands::ledger_verify(ctx, &repo)?,
            LedgerCmds::Export { repo } => commands::ledger_export(ctx, &repo)?,
            LedgerCmds::History { repo } => commands::ledger_history_json(ctx, &repo)?,
        },
        Command::Query(c) => match c {
            QueryCmds::Evidence { repo, entity } => commands::query_evidence(ctx, &repo, &entity)?,
        },
        Command::Projections(c) => match c {
            ProjectionsCmds::Status { repo } => commands::projections_status(ctx, &repo)?,
            ProjectionsCmds::Rebuild { repo, all } => {
                if !all {
                    return Err(anyhow!("--all required"));
                }
                commands::projections_rebuild(ctx, &repo)?
            }
            ProjectionsCmds::Export {
                repo,
                canonical_json,
            } => {
                if !canonical_json {
                    return Err(anyhow!("--canonical-json required"));
                }
                let s = commands::projections_export(ctx, &repo)?;
                return Ok(Dispatch::Raw(s));
            }
            ProjectionsCmds::ExportAntiCollapseCorpus {
                repo,
                question,
                snapshot,
            } => {
                let s = commands::export_anti_collapse_corpus(ctx, &repo, &question, &snapshot)?;
                return Ok(Dispatch::Raw(s));
            }
        },
        Command::Spool(c) => match c {
            SpoolCmds::Status { repo } => commands::spool_status(ctx, &repo)?,
            SpoolCmds::Drain { repo } => commands::spool_drain(ctx, &repo)?,
        },
        Command::Await(c) => {
            if c.run.is_none() && (c.repo.is_none() || !c.all) {
                return Err(anyhow!("await requires --run <id>, or --repo <id> --all"));
            }
            commands::await_run(
                ctx,
                c.run.as_deref(),
                c.repo.as_deref(),
                c.all,
                c.timeout_secs,
                c.interval_ms,
            )?
        }
        Command::Doctor => {
            let (v, code) = commands::doctor(ctx)?;
            return Ok(Dispatch::JsonExit(v, code));
        }
        Command::Queue(c) => match c {
            QueueCmds::Recover { repo } => {
                let n = match repo {
                    Some(r) => daemon::drain_all_queues_limited(ctx, &r)?,
                    None => daemon::drain_all_queues(ctx)?,
                };
                output::output(vec![("recovered", json!(n))])
            }
        },
        Command::Daemon(c) => {
            daemon::daemon_run(ctx, c.socket.as_deref(), c.drain_delay_ms)?;
            unreachable!("daemon_run only returns on error")
        }
        Command::Delta(c) => commands::delta(ctx, &c.repo, &c.question, &c.snapshot)?,
        Command::Explain(c) => commands::explain(ctx, &c.record_id)?,
        Command::Context(c) => commands::context(ctx, &c.repo, &c.failure)?,
        Command::Observed(c) => match c {
            ObservedCmds::Ingest { repo, file } => {
                let mut input = String::new();
                if file == "-" {
                    std::io::Read::read_to_string(&mut std::io::stdin(), &mut input)?;
                } else {
                    input =
                        std::fs::read_to_string(&file).map_err(|e| anyhow!("read {file}: {e}"))?;
                }
                let (v, any_rejected) = observed::ingest(ctx, &repo, &input)?;
                let out = output::output1("ingest", v);
                return Ok(if any_rejected {
                    Dispatch::JsonExit(out, 4)
                } else {
                    Dispatch::Json(out)
                });
            }
            ObservedCmds::Derive {
                repo,
                world,
                scope,
                episode,
                kind,
                until,
                profile,
                declared,
                coverage_manifest,
                independent_system,
                out,
                expected_head,
                expected_record_count,
            } => {
                let text = observed::derive_command(
                    ctx,
                    &observed::DeriveArgs {
                        repo: &repo,
                        world: &world,
                        scope: &scope,
                        episode: &episode,
                        kind: &kind,
                        until: until.as_deref(),
                        profile: &profile,
                        declared: declared.as_deref().map(std::path::Path::new),
                        coverage_manifest: coverage_manifest.as_deref().map(std::path::Path::new),
                        independent_systems: &independent_system,
                        expected_head: head_from(expected_head, expected_record_count),
                    },
                )?;
                if let Some(path) = out {
                    std::fs::write(&path, format!("{text}\n"))
                        .map_err(|e| anyhow!("write {path}: {e}"))?;
                    return Ok(Dispatch::Json(output::output1(
                        "written",
                        json!({"path": path, "bytes": text.len() + 1}),
                    )));
                }
                return Ok(Dispatch::Raw(text));
            }
            ObservedCmds::Compare {
                repo,
                observed,
                reference,
                profile,
                mapping,
                world,
                declared,
                model,
                domainforge_bin,
                facts_out,
                expected_head,
                expected_record_count,
                out,
            } => {
                let p = |s: &String| std::path::PathBuf::from(s);
                let text = observed::compare_command(
                    ctx,
                    &observed::CompareArgs {
                        repo: &repo,
                        observed: &p(&observed),
                        reference: &p(&reference),
                        profile: &p(&profile),
                        mapping: &p(&mapping),
                        world: &world,
                        declared_snapshot: &declared,
                        model: model.as_ref().map(p).as_deref(),
                        domainforge_bin: domainforge_bin.as_ref().map(p).as_deref(),
                        facts_out: facts_out.as_ref().map(p).as_deref(),
                        expected_head: head_from(expected_head, expected_record_count),
                    },
                )?;
                if let Some(path) = out {
                    std::fs::write(&path, format!("{text}\n"))
                        .map_err(|e| anyhow!("write {path}: {e}"))?;
                    return Ok(Dispatch::Json(output::output1(
                        "written",
                        json!({"path": path, "bytes": text.len() + 1}),
                    )));
                }
                return Ok(Dispatch::Raw(text));
            }
        },
    };
    Ok(Dispatch::Json(v))
}
