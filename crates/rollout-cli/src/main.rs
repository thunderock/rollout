#![forbid(unsafe_code)]
use clap::{Parser, Subcommand};

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
        #[arg(long, default_value = "json")]
        format: String,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Schema { format: _ } => {
            eprintln!("schema subcommand not yet wired (plan 04)");
        }
    }
}
