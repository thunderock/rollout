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
    /// Hidden `test-fence` edge: emit one `coordinator_fenced` event then `abort()`.
    ///
    /// Invoked as `rollout-coordinator test-fence <stale> <observed>` (clap
    /// kebab-cases the variant). Exercises the REAL `std::process::abort()` fence
    /// path (D-FENCE-03) in a CHILD process so the in-process witness never kills
    /// the test runner. Not an operator command — hidden from `--help`. Driven by
    /// the SC4 subprocess abort harness (`tests/support/abort_harness.rs`);
    /// landed in 06-01, only consumed (never redefined) by the 06-04 smoke.
    #[command(hide = true)]
    TestFence {
        /// The stale epoch this (deposed) coordinator held.
        stale: u64,
        /// The higher epoch that deposed it.
        observed: u64,
    },
    /// Hidden smoke edge: drive the mock-backend ledger (dispatch + steal +
    /// complete) over a fresh Storage and emit NDJSON `run_done`/`work_stolen`.
    ///
    /// No GPU, no inference backend. Used by `scripts/smoke-3node.sh` on the
    /// local-transport wiring path to assert the assembled 06-02/06-03 ledger
    /// reports `done` and observes a real steal within 30s. Hidden from `--help`.
    #[command(hide = true)]
    MockRun {
        /// Path to the (fresh) embedded storage for the ledger.
        #[arg(long)]
        storage: PathBuf,
        /// Run ID (ULID).
        #[arg(long)]
        run_id: String,
        /// Number of work items to dispatch + complete.
        #[arg(long, default_value_t = 8)]
        items: usize,
        /// Number of logical workers (>= 2 so a steal can occur).
        #[arg(long, default_value_t = 3)]
        workers: usize,
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
        Sub::MockRun {
            storage,
            run_id,
            items,
            workers,
        } => {
            use rollout_coordinator::emitter::StdoutJsonEmitter;
            use rollout_coordinator::mock_run::mock_run;
            use rollout_core::RunId;
            use std::sync::Arc;

            let run_ulid: ulid::Ulid = match run_id.parse() {
                Ok(u) => u,
                Err(e) => {
                    tracing::error!(error = %e, "run_id is not a valid ULID");
                    return std::process::ExitCode::from(2);
                }
            };
            if let Some(parent) = storage.parent() {
                if !parent.as_os_str().is_empty() {
                    let _ = std::fs::create_dir_all(parent);
                }
            }
            let store: Arc<dyn rollout_core::Storage> =
                match rollout_storage::EmbeddedStorage::open(&storage).await {
                    Ok(s) => Arc::new(s),
                    Err(e) => {
                        tracing::error!(error = ?e, "open mock-run storage failed");
                        return std::process::ExitCode::from(2);
                    }
                };
            let emitter = StdoutJsonEmitter::default();
            match mock_run(store, RunId(run_ulid), items, workers, &emitter).await {
                Ok(done) if done == items => std::process::ExitCode::SUCCESS,
                Ok(done) => {
                    tracing::error!(done, items, "mock-run did not complete all items");
                    std::process::ExitCode::from(1)
                }
                Err(e) => {
                    tracing::error!(error = ?e, "mock-run failed");
                    std::process::ExitCode::from(2)
                }
            }
        }
    }
}
