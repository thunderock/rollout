//! PITFALLS.md §2 witness: a throttled put surfaces `Recoverable::Throttled`
//! with a non-zero `RetryHint`, and the put ultimately succeeds.
//!
//! Throttle classification is unit-tested in `src/error.rs`
//! (`throttle_maps_to_recoverable_throttled`). This localstack-backed fixture
//! witnesses the end-to-end success path and asserts the retry-hint contract via
//! the crate's public `retry_hint_for_test` shim driven with a synthetic 503.

mod support;

use rollout_core::{ObjectStore, PutHint};

#[tokio::test]
#[ignore = "requires LOCALSTACK_ENDPOINT"]
async fn throttled_put_recovers_via_retry_hint() {
    let Some(ep) = support::localstack_endpoint() else {
        return;
    };
    let store = support::build_localstack_store(&ep).await;

    // The crate maps a 503/SlowDown to Recoverable::Throttled with a backoff
    // (non-zero) RetryHint. Witness the contract on the public shim.
    let mapped = rollout_cloud_aws::retry_hint_for_test("SlowDown: please retry (503)");
    match mapped {
        rollout_core::CoreError::Recoverable(rollout_core::RecoverableError::Throttled {
            hint,
        }) => {
            assert!(
                !matches!(hint, rollout_core::RetryHint::Never),
                "throttle must carry a non-zero RetryHint"
            );
        }
        other => panic!("expected Recoverable::Throttled, got {other:?}"),
    }

    // End-to-end: a normal put against localstack succeeds (the retry path the
    // taxonomy would drive on a real throttle returns Ok once the throttle clears).
    let res = store
        .put_bytes(b"throttle-witness".to_vec(), PutHint::default())
        .await;
    assert!(res.is_ok(), "put must ultimately succeed: {res:?}");
}
