//! rollout CLI binary.
//!
//! Phase-2 subcommands: `schema` (Phase-1 carryover), `coordinator run`,
//! `worker run`. The worker loop registers via implicit-first-heartbeat,
//! beats every `heartbeat_interval`, loads plugins, and awaits SIGTERM.
#![forbid(unsafe_code)]

use clap::{Parser, Subcommand, ValueEnum};
use rollout_core::config::RunConfig;
use std::path::PathBuf;
use std::process::ExitCode;

mod worker;

#[derive(Parser)]
#[command(name = "rollout", version, about = "rollout CLI")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Print the JSON Schema for the run config.
    Schema {
        /// Output format: compact `json` or `pretty` JSON.
        #[arg(long, value_enum, default_value_t = SchemaFormat::Json)]
        format: SchemaFormat,
    },
    /// Worker process: implicit-register with the coordinator + emit heartbeats + load plugins.
    Worker {
        #[command(subcommand)]
        sub: WorkerSub,
    },
    /// Coordinator process (Phase-2 minimal control plane).
    Coordinator {
        #[command(subcommand)]
        sub: CoordSub,
    },
}

#[derive(Subcommand)]
enum WorkerSub {
    /// Boot a worker from a TOML config file.
    Run(WorkerRunArgs),
}

#[derive(Subcommand)]
enum CoordSub {
    /// Boot the coordinator from a TOML config file.
    Run(CoordRunArgs),
}

#[derive(clap::Args)]
struct WorkerRunArgs {
    /// Path to worker TOML config.
    #[arg(long)]
    config: PathBuf,
    /// Worker ID (ULID). If omitted, one is generated at startup.
    #[arg(long)]
    worker_id: Option<String>,
    /// Plugin manifest path(s) to load. Repeatable.
    #[arg(long = "plugin")]
    plugins: Vec<PathBuf>,
    /// Enable hot-reload for `PyO3` + sidecar plugins (dev-only).
    #[arg(long)]
    hot_reload: bool,
}

#[derive(clap::Args)]
struct CoordRunArgs {
    /// Path to coordinator TOML config.
    #[arg(long)]
    config: PathBuf,
}

/// Output format selector for `rollout schema`.
#[derive(Copy, Clone, ValueEnum)]
enum SchemaFormat {
    /// Compact single-line JSON.
    Json,
    /// Pretty-printed JSON.
    Pretty,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Schema { format } => schema(format),
        Cmd::Coordinator { sub: CoordSub::Run(a) } => coord_run(a),
        Cmd::Worker { sub: WorkerSub::Run(a) } => worker_run(a),
    }
}

fn schema(format: SchemaFormat) -> ExitCode {
    let schema = schemars::schema_for!(RunConfig);
    let out = match format {
        SchemaFormat::Json => serde_json::to_string(&schema),
        SchemaFormat::Pretty => serde_json::to_string_pretty(&schema),
    };
    match out {
        Ok(s) => {
            println!("{s}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("schema serialize failed: {e}");
            ExitCode::from(2)
        }
    }
}

fn coord_run(args: CoordRunArgs) -> ExitCode {
    init_tracing();
    let Ok(rt) = tokio::runtime::Runtime::new() else {
        eprintln!("failed to start tokio runtime");
        return ExitCode::from(2);
    };
    rt.block_on(async move {
        let cfg = match rollout_coordinator::run::load_config(&args.config) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("coordinator config: {e}");
                return ExitCode::from(2);
            }
        };
        match rollout_coordinator::run::run(cfg).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("coordinator exit: {e}");
                ExitCode::from(2)
            }
        }
    })
}

fn worker_run(args: WorkerRunArgs) -> ExitCode {
    init_tracing();
    let Ok(rt) = tokio::runtime::Runtime::new() else {
        eprintln!("failed to start tokio runtime");
        return ExitCode::from(2);
    };
    rt.block_on(async move {
        match worker::run(args.config, args.worker_id, args.plugins, args.hot_reload).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("worker exit: {e}");
                ExitCode::from(2)
            }
        }
    })
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .try_init();
}
