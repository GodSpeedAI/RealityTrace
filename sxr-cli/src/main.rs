//! `sxr` binary: manual lifecycle commands with `--json` output and §15 exit
//! codes (0 ok, 2 usage, 3 precondition, 4 CEP validation, 5 integrity,
//! 6 verifier, 75 spooled).

use anyhow::{Result, anyhow};
use clap::{Parser, Subcommand};
use serde_json::Value;

mod commands;
mod envelopes;
mod ledger_io;
mod output;
mod projections;
mod state;

use state::Ctx;

/// sxr — the DomainForge Semantic Exhaust Runtime CLI.
#[derive(Parser)]
#[command(name = "sxr", version, about)]
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
    /// Execute plan steps (cargo-test).
    #[command(subcommand)]
    Verify(VerifyCmds),
    /// Derive Evidence from an observation.
    #[command(subcommand)]
    Evidence(EvidenceCmds),
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
    /// Delta inspection.
    Delta(DeltaCmd),
    /// Explain a record's provenance.
    Explain(ExplainCmd),
    /// Failure context pack.
    Context(ContextCmd),
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
    #[arg(long)]
    run: String,
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
    let cli = Cli::parse();
    let ctx = match Ctx::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("sxr: {e:#}");
            std::process::exit(2);
        }
    };
    let result = dispatch(&ctx, cli.command);
    match result {
        Ok(Dispatch::Json(v)) => output::print_json(&v),
        Ok(Dispatch::Raw(s)) => {
            println!("{s}");
        }
        Err(e) => {
            eprintln!("sxr: {e:#}");
            std::process::exit(exit_for_error(&e));
        }
    }
}

enum Dispatch {
    Json(Value),
    Raw(String),
}

fn exit_for_error(e: &anyhow::Error) -> i32 {
    // Map known error contexts to §15 exit codes by message prefix.
    let msg = e.to_string();
    if msg.contains("AC-SETTLEMENT")
        || msg.contains("AC-EVIDENCE")
        || msg.contains("precondition")
        || msg.contains("not found")
        || msg.contains("not enrolled")
        || msg.contains("no semantic configuration")
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
            } => commands::run_begin(ctx, &repo, &snapshot, &question, &threshold)?,
            RunCmds::Finish { run } => commands::run_finish(ctx, &run)?,
        },
        Command::Plan(c) => match c {
            PlanCmds::Create {
                run,
                verifier,
                target,
            } => commands::plan_create(ctx, &run, &verifier, &target)?,
        },
        Command::Verify(c) => match c {
            VerifyCmds::Run { run } => commands::verify_run(ctx, &run)?,
        },
        Command::Evidence(c) => match c {
            EvidenceCmds::Create {
                run,
                observation,
                derive,
            } => {
                if !derive {
                    return Err(anyhow!("only --derive is supported in slice 1"));
                }
                commands::evidence_create(ctx, &run, &observation)?
            }
        },
        Command::Settlement(c) => match c {
            SettlementCmds::Register {
                run,
                question,
                threshold,
                transformation,
            } => commands::settlement_register(ctx, &run, &question, &threshold, &transformation)?,
        },
        Command::Ingest(c) => commands::ingest(ctx, &c.run, &c.verifier)?,
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
        },
        Command::Spool(c) => match c {
            SpoolCmds::Status { repo } => commands::spool_status(ctx, &repo)?,
            SpoolCmds::Drain { repo } => commands::spool_drain(ctx, &repo)?,
        },
        Command::Await(c) => commands::await_run(ctx, &c.run)?,
        Command::Doctor => commands::doctor(ctx)?,
        Command::Delta(c) => commands::delta(ctx, &c.repo, &c.question, &c.snapshot)?,
        Command::Explain(c) => commands::explain(ctx, &c.record_id)?,
        Command::Context(c) => commands::context(ctx, &c.repo, &c.failure)?,
    };
    Ok(Dispatch::Json(v))
}
