//! rollout CLI binary. Subcommands wired progressively across phases; only `schema` is real in Phase 1.
#![forbid(unsafe_code)]
use clap::{Parser, Subcommand, ValueEnum};
use rollout_core::config::RunConfig;
use std::process::ExitCode;

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
        Cmd::Schema { format } => {
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
    }
}
