//! AWS `SdkConfig` construction. `BehaviorVersion::latest()` is IMDSv2-only
//! (PITFALLS.md §3) — we never construct a v1-tolerant client.

use aws_config::BehaviorVersion;

/// Load default AWS config for `region` (credentials from the standard chain).
pub async fn load_aws_config(region: &str) -> aws_config::SdkConfig {
    aws_config::defaults(BehaviorVersion::latest())
        .region(aws_config::Region::new(region.to_owned()))
        .load()
        .await
}

/// Load AWS config with an explicit endpoint override + static test credentials.
///
/// Used by the localstack-backed conformance suite (`LOCALSTACK_ENDPOINT`).
pub async fn load_aws_config_with_endpoint(
    region: &str,
    endpoint_url: &str,
) -> aws_config::SdkConfig {
    aws_config::defaults(BehaviorVersion::latest())
        .region(aws_config::Region::new(region.to_owned()))
        .endpoint_url(endpoint_url)
        .test_credentials()
        .load()
        .await
}
