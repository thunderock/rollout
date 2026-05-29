//! `rollout cloud doctor` — operator pre-flight tool (D-DOCTOR-01..04 + CLOUD-04).
//!
//! Exercises all four cloud traits against either AWS or GCP via the existing
//! `cloud_factory::build_cloud_runtime`, with human (colored, default) + JSON
//! output and Unix exit codes (0 = all pass, 1 = any fail, 2 = invocation/config).

use clap::{Parser, ValueEnum};
use std::path::PathBuf;

pub mod checks;
pub mod config;
pub mod output;

/// `rollout cloud doctor` flags.
#[derive(Parser, Debug, Clone)]
pub struct DoctorArgs {
    /// Cloud provider to validate. MUST match `[cloud].provider` in `--config`.
    #[arg(long, value_enum)]
    pub provider: ProviderArg,

    /// Path to the TOML config (same shape as `rollout train sft --config`).
    #[arg(long)]
    pub config: PathBuf,

    /// Output format. Default = human (colored).
    #[arg(long, value_enum, default_value = "human")]
    pub format: OutputFormat,
}

/// Cloud provider selector for `--provider`.
#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
pub enum ProviderArg {
    /// Amazon Web Services (S3 / SQS / Secrets Manager / EC2 `IMDSv2`).
    Aws,
    /// Google Cloud Platform (GCS / Pub/Sub / Secret Manager / GCE MDS).
    Gcp,
}

/// Output format selector for `--format`.
#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
pub enum OutputFormat {
    /// Colored, step-by-step (default).
    Human,
    /// Machine-readable `{checks, summary}`.
    Json,
}

/// Doctor entry point. Never returns — terminates via `std::process::exit`
/// (the only acceptable `process::exit` site per AGENTS.md — CLI binary).
///
/// Exit codes (D-DOCTOR-03): `0` all pass · `1` any check failed · `2`
/// invocation/config error.
pub async fn run(args: DoctorArgs) -> ! {
    let cfg = match config::cloud_config_from_toml(&args.config) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Config error: {e}");
            std::process::exit(2);
        }
    };
    if !config::provider_matches(&cfg, args.provider) {
        eprintln!(
            "--provider {:?} does not match [cloud].provider in {}",
            args.provider,
            args.config.display()
        );
        std::process::exit(2);
    }

    let results = checks::run_all_checks(args.provider, &cfg).await;
    let exit_code = i32::from(
        results
            .iter()
            .any(|c| matches!(c.status, checks::CheckStatus::Fail)),
    );

    match args.format {
        OutputFormat::Human => output::human::print(&results),
        OutputFormat::Json => output::json::print(&results),
    }
    std::process::exit(exit_code);
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    /// Wrap `DoctorArgs` so clap parses the `cloud doctor ...` argv shape in tests.
    #[derive(Parser, Debug)]
    struct TestCli {
        #[command(subcommand)]
        cmd: TestSub,
    }
    #[derive(clap::Subcommand, Debug)]
    enum TestSub {
        Doctor(DoctorArgs),
    }

    fn parse(argv: &[&str]) -> Result<DoctorArgs, clap::Error> {
        let mut full = vec!["test", "doctor"];
        full.extend_from_slice(argv);
        TestCli::try_parse_from(full).map(|c| match c.cmd {
            TestSub::Doctor(a) => a,
        })
    }

    #[test]
    fn doctor_args_parse_aws_human_default() {
        let a = parse(&["--provider", "aws", "--config", "foo.toml"]).unwrap();
        assert_eq!(a.provider, ProviderArg::Aws);
        assert_eq!(a.config, PathBuf::from("foo.toml"));
        assert_eq!(a.format, OutputFormat::Human);
    }

    #[test]
    fn doctor_args_parse_gcp_json() {
        let a = parse(&[
            "--provider",
            "gcp",
            "--config",
            "bar.toml",
            "--format",
            "json",
        ])
        .unwrap();
        assert_eq!(a.provider, ProviderArg::Gcp);
        assert_eq!(a.config, PathBuf::from("bar.toml"));
        assert_eq!(a.format, OutputFormat::Json);
    }

    #[test]
    fn doctor_args_reject_unknown_provider() {
        assert!(parse(&["--provider", "azure", "--config", "x.toml"]).is_err());
    }

    #[test]
    fn doctor_args_reject_unknown_format() {
        assert!(parse(&[
            "--provider",
            "aws",
            "--config",
            "x.toml",
            "--format",
            "yaml"
        ])
        .is_err());
    }
}
