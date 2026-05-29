//! Centralized GCP SDK -> `CoreError` mapping (PITFALLS.md §1).
//!
//! SDK error types never escape this crate: every helper renders the SDK error
//! to a `String` and collapses it into a `CoreError`. No `#[source]` chain to an
//! SDK type — that is what keeps the `public-api-cloud-leak` gate green (no
//! `gcloud_*` / `google_cloud_*` symbol reaches `rollout-core`).
//!
//! Mapping policy: throttle (`ResourceExhausted` / `RateLimit` / 429 / 503) ->
//! `Recoverable::Throttled`; `Unavailable` / timeout / connection / 5xx ->
//! `Recoverable::Transient`; `PermissionDenied` / `NotFound` (config) ->
//! `Fatal::ConfigInvalid`; everything else -> `Fatal::Internal`.

use std::fmt::Display;
use std::time::Duration;

use rollout_core::{CoreError, FatalError, RecoverableError, RetryHint};

/// Backoff used for throttled / transient retries.
fn backoff() -> RetryHint {
    RetryHint::Backoff {
        base: Duration::from_millis(200),
        max: Duration::from_secs(30),
    }
}

/// True if the rendered SDK error looks like a throttle / rate-limit signal.
fn is_throttle(rendered: &str) -> bool {
    let r = rendered.to_ascii_lowercase();
    r.contains("resourceexhausted")
        || r.contains("resource exhausted")
        || r.contains("ratelimit")
        || r.contains("rate limit")
        || r.contains("too many requests")
        || r.contains("429")
        || r.contains("503")
}

/// True if the rendered SDK error looks transient (retryable but not a throttle).
fn is_transient(rendered: &str) -> bool {
    let r = rendered.to_ascii_lowercase();
    r.contains("unavailable")
        || r.contains("timeout")
        || r.contains("timed out")
        || r.contains("deadline")
        || r.contains("connection")
        || r.contains("connect")
        || r.contains("aborted")
        || r.contains("500")
        || r.contains("502")
        || r.contains("504")
        || r.contains("internal server error")
}

/// True if the rendered SDK error indicates a config / permission problem.
fn is_config(rendered: &str) -> bool {
    let r = rendered.to_ascii_lowercase();
    r.contains("permissiondenied")
        || r.contains("permission denied")
        || r.contains("unauthenticated")
        || r.contains("401")
        || r.contains("403")
}

/// Generic SDK-error mapper shared by every operation.
fn map_sdk_error<E: Display>(op: &str, err: E) -> CoreError {
    let rendered = format!("{op}: {err}");
    if is_throttle(&rendered) {
        CoreError::Recoverable(RecoverableError::Throttled { hint: backoff() })
    } else if is_transient(&rendered) {
        CoreError::Recoverable(RecoverableError::Transient {
            msg: rendered,
            hint: backoff(),
        })
    } else if is_config(&rendered) {
        CoreError::Fatal(FatalError::ConfigInvalid { msg: rendered })
    } else {
        CoreError::Fatal(FatalError::Internal { msg: rendered })
    }
}

/// GCS operation-error mapper.
pub(crate) fn map_gcs_error<E: Display>(err: E) -> CoreError {
    map_sdk_error("gcs", err)
}

/// Pub/Sub operation-error mapper.
#[allow(dead_code)] // wired in Task 2 (PubSubQueue)
pub(crate) fn map_pubsub_error<E: Display>(err: E) -> CoreError {
    map_sdk_error("pubsub", err)
}

/// Secret Manager operation-error mapper. `NotFound` -> `ConfigInvalid`
/// (the secret was not provisioned; out-of-band fix needed).
#[allow(dead_code)] // wired in Task 3 (SecretManagerSecretStore)
pub(crate) fn map_sm_error<E: Display>(err: E) -> CoreError {
    let rendered = format!("secretmanager: {err}");
    let lower = rendered.to_ascii_lowercase();
    if lower.contains("notfound") || lower.contains("not found") || lower.contains("404") {
        return CoreError::Fatal(FatalError::ConfigInvalid {
            msg: format!("secret not found: {rendered}"),
        });
    }
    map_sdk_error("secretmanager", err)
}

/// MDS operation-error mapper.
#[allow(dead_code)] // wired in Task 4 (GceMetadataComputeHint)
pub(crate) fn map_mds_error<E: Display>(err: E) -> CoreError {
    map_sdk_error("mds", err)
}

/// Build a `Fatal::Internal` error from a static-ish message.
pub(crate) fn fatal_internal(msg: &str) -> CoreError {
    CoreError::Fatal(FatalError::Internal {
        msg: msg.to_owned(),
    })
}

/// Build a `Fatal::ConfigInvalid` error.
pub(crate) fn config_invalid(msg: String) -> CoreError {
    CoreError::Fatal(FatalError::ConfigInvalid { msg })
}

/// Build a `Recoverable::Transient` error.
pub(crate) fn recoverable_transient(msg: String) -> CoreError {
    CoreError::Recoverable(RecoverableError::Transient {
        msg,
        hint: backoff(),
    })
}

/// Test-only shim: classify a rendered SDK-error string into a `CoreError`.
///
/// Lets emulator fixtures witness the throttle -> `Recoverable::Throttled`
/// mapping without exposing the SDK-generic mappers.
#[doc(hidden)]
#[must_use]
pub fn retry_hint_for_test(rendered: &str) -> CoreError {
    map_sdk_error("test", rendered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn throttle_maps_to_recoverable_throttled() {
        let e = map_gcs_error("ResourceExhausted: quota (429)");
        assert!(matches!(
            e,
            CoreError::Recoverable(RecoverableError::Throttled { .. })
        ));
    }

    #[test]
    fn unavailable_maps_to_transient() {
        let e = map_pubsub_error("Unavailable: backend connection reset");
        assert!(matches!(
            e,
            CoreError::Recoverable(RecoverableError::Transient { .. })
        ));
    }

    #[test]
    fn permission_denied_maps_to_config_invalid() {
        let e = map_gcs_error("PermissionDenied: caller lacks storage.objects.get (403)");
        assert!(matches!(
            e,
            CoreError::Fatal(FatalError::ConfigInvalid { .. })
        ));
    }

    #[test]
    fn unknown_maps_to_fatal_internal() {
        let e = map_gcs_error("Invalid object metadata");
        assert!(matches!(e, CoreError::Fatal(FatalError::Internal { .. })));
    }

    #[test]
    fn sm_not_found_maps_to_config_invalid() {
        let e = map_sm_error("NotFound: secret version missing");
        assert!(matches!(
            e,
            CoreError::Fatal(FatalError::ConfigInvalid { .. })
        ));
    }
}
