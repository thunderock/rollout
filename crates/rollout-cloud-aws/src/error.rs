//! Centralized AWS SDK -> `CoreError` mapping (PITFALLS.md §1).
//!
//! SDK error types never escape this crate: every helper renders the SDK error
//! to a `String` and collapses it into a `CoreError`. No `#[source]` chain to an
//! SDK type — that is what keeps the `public-api-cloud-leak` gate green.
//!
//! Mapping policy: throttle (`SlowDown` / `Throttling` / 503 / 429) ->
//! `Recoverable::Throttled` with a non-zero `RetryHint`; other 5xx /
//! timeout / connector failures -> `Recoverable::Transient`; everything else ->
//! `Fatal::Internal`.

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
    r.contains("slowdown")
        || r.contains("throttl")
        || r.contains("too many requests")
        || r.contains("toomanyrequests")
        || r.contains("requestlimitexceeded")
        || r.contains("503")
        || r.contains("429")
        || r.contains("serviceunavailable")
        || r.contains("service unavailable")
}

/// True if the rendered SDK error looks transient (retryable but not a throttle).
fn is_transient(rendered: &str) -> bool {
    let r = rendered.to_ascii_lowercase();
    r.contains("timeout")
        || r.contains("timed out")
        || r.contains("connection")
        || r.contains("dispatch")
        || r.contains("io error")
        || r.contains("500")
        || r.contains("502")
        || r.contains("504")
        || r.contains("internalerror")
        || r.contains("internal server error")
}

/// Generic SDK-error mapper shared by every operation error variant.
pub(crate) fn map_sdk_error<E: Display>(op: &str, err: E) -> CoreError {
    let rendered = format!("{op}: {err}");
    if is_throttle(&rendered) {
        CoreError::Recoverable(RecoverableError::Throttled { hint: backoff() })
    } else if is_transient(&rendered) {
        CoreError::Recoverable(RecoverableError::Transient {
            msg: rendered,
            hint: backoff(),
        })
    } else {
        CoreError::Fatal(FatalError::Internal { msg: rendered })
    }
}

/// S3 operation-error mapper.
pub(crate) fn map_s3_sdk_error<E: Display>(err: E) -> CoreError {
    map_sdk_error("s3", err)
}

/// SQS operation-error mapper. `ReceiptHandleIsInvalid` collapses to `Transient`.
pub(crate) fn map_sqs_sdk_error<E: Display>(err: E) -> CoreError {
    let rendered = format!("sqs: {err}");
    if rendered.to_ascii_lowercase().contains("receipthandle") {
        return CoreError::Recoverable(RecoverableError::Transient {
            msg: rendered,
            hint: RetryHint::Never,
        });
    }
    map_sdk_error("sqs", err)
}

/// Secrets Manager operation-error mapper. `ResourceNotFound` -> `ConfigInvalid`.
#[allow(dead_code)] // wired in by the secrets_manager module (same plan, Task 3)
pub(crate) fn map_sm_sdk_error<E: Display>(err: E) -> CoreError {
    let rendered = format!("secretsmanager: {err}");
    if rendered.to_ascii_lowercase().contains("resourcenotfound")
        || rendered
            .to_ascii_lowercase()
            .contains("can't find the specified secret")
    {
        return CoreError::Fatal(FatalError::ConfigInvalid {
            msg: format!("secret not found: {rendered}"),
        });
    }
    map_sdk_error("secretsmanager", err)
}

/// Build a `Fatal::Internal` error from a static-ish message.
pub(crate) fn fatal_internal(msg: &str) -> CoreError {
    CoreError::Fatal(FatalError::Internal {
        msg: msg.to_owned(),
    })
}

/// Test-only shim: classify a rendered SDK-error string into a `CoreError`.
///
/// Lets the localstack fixtures witness the throttle -> `Recoverable::Throttled`
/// mapping without exposing the SDK-generic mappers.
#[doc(hidden)]
#[must_use]
pub fn retry_hint_for_test(rendered: &str) -> CoreError {
    map_sdk_error("test", rendered)
}

/// Build a `Recoverable::Transient` error.
pub(crate) fn recoverable_transient(msg: String) -> CoreError {
    CoreError::Recoverable(RecoverableError::Transient {
        msg,
        hint: backoff(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn throttle_maps_to_recoverable_throttled() {
        let e = map_sdk_error("put", "SlowDown: please slow down (503)");
        assert!(matches!(
            e,
            CoreError::Recoverable(RecoverableError::Throttled { .. })
        ));
    }

    #[test]
    fn timeout_maps_to_transient() {
        let e = map_sdk_error("get", "request timeout after 30s");
        assert!(matches!(
            e,
            CoreError::Recoverable(RecoverableError::Transient { .. })
        ));
    }

    #[test]
    fn unknown_maps_to_fatal_internal() {
        let e = map_sdk_error("get", "NoSuchBucket: bucket does not exist");
        assert!(matches!(e, CoreError::Fatal(FatalError::Internal { .. })));
    }

    #[test]
    fn sm_resource_not_found_maps_to_config_invalid() {
        let e = map_sm_sdk_error("ResourceNotFoundException: secret missing");
        assert!(matches!(
            e,
            CoreError::Fatal(FatalError::ConfigInvalid { .. })
        ));
    }

    #[test]
    fn sqs_invalid_receipt_handle_maps_to_transient_never() {
        let e = map_sqs_sdk_error("ReceiptHandleIsInvalid: stale");
        match e {
            CoreError::Recoverable(RecoverableError::Transient { hint, .. }) => {
                assert!(matches!(hint, RetryHint::Never));
            }
            other => panic!("expected Transient/Never, got {other:?}"),
        }
    }
}
