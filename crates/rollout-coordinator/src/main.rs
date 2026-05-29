//! `rollout-coordinator` binary: minimal Phase-2 control plane.
#![forbid(unsafe_code)]

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "rollout-coordinator", version)]
struct Cli {
    #[command(subcommand)]
    cmd: Sub,
}

#[derive(clap::Subcommand)]
enum Sub {
    /// Boot the coordinator from a TOML config file.
    Run {
        /// Path to coordinator TOML config.
        #[arg(long)]
        config: PathBuf,
    },
    /// Hidden test edge: emit one `coordinator_fenced` event then `abort()`.
    ///
    /// Exercises the REAL `std::process::abort()` fence path (D-FENCE-03) in a
    /// CHILD process so the in-process witness never kills the test runner. Not
    /// an operator command — hidden from `--help`. Driven by the SC4 subprocess
    /// abort harness (`tests/support/abort_harness.rs`).
    #[command(hide = true)]
    TestFence {
        /// The stale epoch this (deposed) coordinator held.
        stale: u64,
        /// The higher epoch that deposed it.
        observed: u64,
    },
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .init();

    let cli = Cli::parse();
    match cli.cmd {
        Sub::Run { config } => match rollout_coordinator::run::load_config(&config) {
            Ok(cfg) => match rollout_coordinator::run::run(cfg).await {
                Ok(()) => std::process::ExitCode::SUCCESS,
                Err(e) => {
                    tracing::error!(error = ?e, "coordinator exited with error");
                    std::process::ExitCode::from(2)
                }
            },
            Err(e) => {
                tracing::error!(error = ?e, "coordinator failed to load config");
                std::process::ExitCode::from(2)
            }
        },
        Sub::TestFence { stale, observed } => {
            use rollout_coordinator::emitter::StdoutJsonEmitter;
            use rollout_coordinator::fence::{fence_old_coordinator, FenceDecision};
            use rollout_core::{CoordEpoch, RunId, WorkerId};

            let emitter = StdoutJsonEmitter::default();
            let decision = fence_old_coordinator(
                &emitter,
                WorkerId(ulid::Ulid::new()),
                RunId(ulid::Ulid::new()),
                CoordEpoch(stale),
                CoordEpoch(observed),
            )
            .await;
            match decision {
                // The real abort edge (D-FENCE-03): violent, no Drop, no flush —
                // hence the emit above is awaited first (Pitfall 5).
                FenceDecision::Abort => std::process::abort(),
            }
        }
    }
}
