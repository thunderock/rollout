//! TOML config loading + provider-match validation for `cloud doctor` (D-DOCTOR-04).
//!
//! Config source is the TOML `[cloud]` block only — no `--bucket`/`--queue`/
//! `--secret-id` flag overrides in v1.1.

use crate::commands::cloud::doctor::ProviderArg;
use rollout_core::config::{CloudConfig, RunConfig};
use std::path::Path;

/// Load + validate the `[cloud]` block from a `RunConfig` TOML at `path`.
///
/// # Errors
/// Returns a human-readable error string on read failure, TOML parse failure,
/// or cross-field validation failure (mapped to exit code 2 by the caller).
pub fn cloud_config_from_toml(path: &Path) -> Result<CloudConfig, String> {
    let s = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let run: RunConfig = toml::from_str(&s).map_err(|e| format!("toml parse: {e}"))?;
    run.cloud
        .validate_cross_fields()
        .map_err(|e| format!("config invalid: {e}"))?;
    Ok(run.cloud)
}

/// True iff the `--provider` flag matches the deserialized `CloudConfig` variant.
#[must_use]
pub fn provider_matches(cfg: &CloudConfig, arg: ProviderArg) -> bool {
    matches!(
        (cfg, arg),
        (CloudConfig::Aws(_), ProviderArg::Aws) | (CloudConfig::Gcp(_), ProviderArg::Gcp)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_tmp(body: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(body.as_bytes()).unwrap();
        f
    }

    const AWS_TOML: &str = r#"
schema_version = 1
[storage]
backend = "embedded"
path = "/tmp/x.db"
[cloud]
provider = "aws"
region = "us-west-2"
[cloud.s3]
bucket = "rollout-snapshots-prod"
[cloud.sqs]
queue_url = "https://sqs.us-west-2.amazonaws.com/1/q"
[cloud.secrets]
allowlist = ["rollout/hf_token"]
[algorithm]
kind = "sft"
minibatch_size = 1
gradient_accumulation = 1
[algorithm.base_model]
uri = "Qwen/Qwen2.5-0.5B-Instruct"
[algorithm.optimizer]
kind = "adam_w"
lr = 1e-5
weight_decay = 0.0
betas = [0.9, 0.999]
eps = 1e-8
warmup_steps = 0
schedule = "constant"
[algorithm.budget]
max_steps = 2
[algorithm.dataset]
kind = "jsonl_path"
path = "examples/sft-tiny.jsonl"
[algorithm.packing]
kind = "concat"
max_seq_len = 512
[algorithm.loss_on]
kind = "assistant_only"
"#;

    #[test]
    fn doctor_config_loads_from_aws_toml() {
        let f = write_tmp(AWS_TOML);
        let cfg = cloud_config_from_toml(f.path()).unwrap();
        let CloudConfig::Aws(aws) = &cfg else {
            panic!("expected aws variant");
        };
        assert_eq!(aws.s3.bucket, "rollout-snapshots-prod");
        assert_eq!(aws.region, "us-west-2");
    }

    #[test]
    fn doctor_config_provider_match_aws_ok_gcp_mismatch() {
        let f = write_tmp(AWS_TOML);
        let cfg = cloud_config_from_toml(f.path()).unwrap();
        assert!(provider_matches(&cfg, ProviderArg::Aws));
        assert!(!provider_matches(&cfg, ProviderArg::Gcp));
    }

    #[test]
    fn doctor_config_missing_file_is_err() {
        let err = cloud_config_from_toml(Path::new("/no/such/file.toml")).unwrap_err();
        assert!(err.contains("read"), "got: {err}");
    }
}
