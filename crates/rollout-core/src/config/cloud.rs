//! Cloud-provider config (D-BUILD-01 stage 1 + RESEARCH.md Pattern 15).
//!
//! `CloudConfig` is a `#[serde(tag = "provider")]` enum: `local` | `aws` | `gcp`.
//! Because the provider is a tagged enum, a single TOML cannot express two
//! providers at once — cross-cloud (`[cloud.aws]` AND `[cloud.gcp]`) is
//! structurally rejected at deserialize time. Per-field bounds (S3 5 MiB
//! multipart floor, 10 GiB part hard cap) are enforced by `validate_cross_fields`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

const FIVE_MIB: u64 = 5 * 1024 * 1024;
const TEN_GIB: u64 = 10 * 1024 * 1024 * 1024;

const fn default_multipart_chunk() -> u64 {
    16 * 1024 * 1024
}
const fn default_max_snapshot_part() -> u64 {
    5 * 1024 * 1024 * 1024
}
const fn default_visibility_timeout() -> u32 {
    300
}
const fn default_ack_deadline_secs() -> u32 {
    30
}

/// Cloud provider selection. `local` is the default so v1.0 TOMLs deserialize unchanged.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "provider", rename_all = "lowercase")]
pub enum CloudConfig {
    /// Local filesystem + in-memory backends (no cloud creds).
    #[default]
    Local,
    /// Amazon Web Services (S3 + SQS + Secrets Manager + EC2 `IMDSv2`).
    Aws(AwsConfig),
    /// Google Cloud Platform (GCS + Pub/Sub + Secret Manager + GCE metadata).
    Gcp(GcpConfig),
}

impl CloudConfig {
    /// Plan-time cross-field validation: reject sub-5-MiB multipart chunks and
    /// above-10-GiB snapshot parts. Cross-cloud is already structurally impossible.
    ///
    /// # Errors
    /// Returns `Fatal(ConfigInvalid)` if any bound is violated.
    pub fn validate_cross_fields(&self) -> Result<(), crate::CoreError> {
        let invalid = |msg: &str| {
            crate::CoreError::Fatal(crate::FatalError::ConfigInvalid {
                msg: msg.to_owned(),
            })
        };
        match self {
            Self::Aws(aws) => {
                if aws.s3.max_snapshot_part_bytes > TEN_GIB {
                    return Err(invalid(
                        "cloud.aws.s3.max_snapshot_part_bytes exceeds 10 GiB hard cap (D-SNAP-03)",
                    ));
                }
                if aws.s3.multipart_chunk_bytes < FIVE_MIB {
                    return Err(invalid(
                        "cloud.aws.s3.multipart_chunk_bytes below S3 5 MiB minimum",
                    ));
                }
            }
            Self::Gcp(gcp) => {
                if gcp.gcs.max_snapshot_part_bytes > TEN_GIB {
                    return Err(invalid(
                        "cloud.gcp.gcs.max_snapshot_part_bytes exceeds 10 GiB hard cap (D-SNAP-03)",
                    ));
                }
                if gcp.gcs.resumable_chunk_bytes < FIVE_MIB {
                    return Err(invalid(
                        "cloud.gcp.gcs.resumable_chunk_bytes below 5 MiB minimum",
                    ));
                }
            }
            Self::Local => {}
        }
        Ok(())
    }
}

/// AWS provider config.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AwsConfig {
    /// AWS region (e.g., `us-west-2`).
    pub region: String,
    /// S3 object-store settings.
    pub s3: AwsS3Config,
    /// SQS queue settings.
    pub sqs: AwsSqsConfig,
    /// Secrets Manager settings.
    #[serde(default)]
    pub secrets: AwsSecretsConfig,
}

/// AWS S3 object-store settings.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AwsS3Config {
    /// Target bucket name.
    pub bucket: String,
    /// Optional key prefix for all objects.
    #[serde(default)]
    pub prefix: Option<String>,
    /// Multipart upload chunk size in bytes (S3 minimum 5 MiB).
    #[serde(default = "default_multipart_chunk")]
    pub multipart_chunk_bytes: u64,
    /// Hard cap on a single snapshot part (10 GiB ceiling).
    #[serde(default = "default_max_snapshot_part")]
    pub max_snapshot_part_bytes: u64,
}

/// AWS SQS queue settings.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AwsSqsConfig {
    /// Fully-qualified SQS queue URL.
    pub queue_url: String,
    /// Visibility timeout in seconds (default 300).
    #[serde(default = "default_visibility_timeout")]
    pub visibility_timeout_secs: u32,
}

/// AWS Secrets Manager settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AwsSecretsConfig {
    /// Read-allowlisted secret names.
    #[serde(default)]
    pub allowlist: Vec<String>,
}

/// GCP provider config.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GcpConfig {
    /// GCP project id.
    pub project: String,
    /// GCS object-store settings.
    pub gcs: GcpGcsConfig,
    /// Pub/Sub queue settings.
    pub pubsub: GcpPubSubConfig,
    /// Secret Manager settings.
    #[serde(default)]
    pub secrets: GcpSecretsConfig,
}

/// GCP GCS object-store settings.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GcpGcsConfig {
    /// Target GCS bucket.
    pub bucket: String,
    /// Optional object-name prefix.
    #[serde(default)]
    pub prefix: Option<String>,
    /// Resumable-upload chunk size in bytes (5 MiB minimum).
    #[serde(default = "default_multipart_chunk")]
    pub resumable_chunk_bytes: u64,
    /// Hard cap on a single snapshot part (10 GiB ceiling).
    #[serde(default = "default_max_snapshot_part")]
    pub max_snapshot_part_bytes: u64,
}

/// GCP Pub/Sub queue settings.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GcpPubSubConfig {
    /// Subscription id to pull from.
    pub subscription: String,
    /// Topic id to publish to.
    pub topic: String,
    /// Ack deadline in seconds (default 30).
    #[serde(default = "default_ack_deadline_secs")]
    pub ack_deadline_secs: u32,
}

/// GCP Secret Manager settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GcpSecretsConfig {
    /// Read-allowlisted secret names.
    #[serde(default)]
    pub allowlist: Vec<String>,
}

#[cfg(test)]
mod cloud_config_tests {
    use super::*;

    #[test]
    fn cloud_config_serde_roundtrip_local() {
        let cfg: CloudConfig = toml::from_str("provider = \"local\"\n").unwrap();
        assert!(matches!(cfg, CloudConfig::Local));
        let back = toml::to_string(&cfg).unwrap();
        let again: CloudConfig = toml::from_str(&back).unwrap();
        assert!(matches!(again, CloudConfig::Local));
    }

    #[test]
    fn cloud_config_serde_roundtrip_aws() {
        let toml_src = r#"
provider = "aws"
region = "us-west-2"
[s3]
bucket = "x"
[sqs]
queue_url = "https://sqs.us-west-2.amazonaws.com/1/x"
[secrets]
allowlist = ["s"]
"#;
        let cfg: CloudConfig = toml::from_str(toml_src).unwrap();
        let CloudConfig::Aws(aws) = &cfg else {
            panic!("expected aws variant");
        };
        assert_eq!(aws.region, "us-west-2");
        assert_eq!(aws.s3.bucket, "x");
        let back = toml::to_string(&cfg).unwrap();
        let again: CloudConfig = toml::from_str(&back).unwrap();
        assert!(matches!(again, CloudConfig::Aws(_)));
    }

    #[test]
    fn cloud_config_rejects_both_aws_and_gcp_blocks() {
        // A tagged-enum cannot carry two providers; a JSON value that tries to
        // smuggle both an aws + gcp body fails to deserialize / validate as cross-cloud.
        let mixed = serde_json::json!({
            "provider": "aws",
            "region": "us-west-2",
            "s3": { "bucket": "x" },
            "sqs": { "queue_url": "u" },
            "gcp": { "project": "p" }
        });
        let res: Result<CloudConfig, _> = serde_json::from_value(mixed);
        let err = res
            .expect_err("mixed aws+gcp body must be rejected")
            .to_string();
        assert!(
            err.contains("gcp") || err.contains("unknown field") || err.contains("cross-cloud"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn cloud_config_rejects_multipart_chunk_below_5mib() {
        let toml_src = r#"
provider = "aws"
region = "us-west-2"
[s3]
bucket = "x"
multipart_chunk_bytes = 1048576
[sqs]
queue_url = "u"
"#;
        let cfg: CloudConfig = toml::from_str(toml_src).unwrap();
        let err = cfg.validate_cross_fields().unwrap_err().to_string();
        assert!(err.contains("below S3 5 MiB minimum"), "got: {err}");
    }

    #[test]
    fn cloud_config_rejects_max_snapshot_part_above_10gib() {
        let toml_src = r#"
provider = "aws"
region = "us-west-2"
[s3]
bucket = "x"
max_snapshot_part_bytes = 11000000000
[sqs]
queue_url = "u"
"#;
        let cfg: CloudConfig = toml::from_str(toml_src).unwrap();
        let err = cfg.validate_cross_fields().unwrap_err().to_string();
        assert!(err.contains("10 GiB hard cap"), "got: {err}");
    }

    #[test]
    fn cloud_config_defaults_match_d_snap_02_and_03() {
        assert_eq!(default_multipart_chunk(), 16 * 1024 * 1024);
        assert_eq!(default_max_snapshot_part(), 5 * 1024 * 1024 * 1024);
    }
}
