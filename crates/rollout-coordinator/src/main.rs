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
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).json().init();

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
    }
}
