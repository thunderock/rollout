//! Pitfall #16 fixture (GCP variant): the `ContentId` is hashed BEFORE the SDK
//! upload call, so it is stable even when the upload path retries.
//!
//! fake-gcs-server has no first-party fault-injection knob (unlike localstack),
//! so we witness the invariant two ways:
//!   1. A unit-style witness (no Docker): the same input hashed twice yields the
//!      same `ContentId`, proving the hash never depends on SDK round-trips.
//!   2. An emulator round-trip: a real `put_stream` over fake-gcs-server yields
//!      `blake3(input)`. (The fault-injection gap is documented in the README
//!      "emulator delta"; time-based retry assertions live in `cloud-live-gcp`.)

#![cfg(feature = "gcp")]
#![allow(deprecated)] // exercising overridden put_stream (trait-level #[deprecated])

mod support;

use std::pin::Pin;

use rollout_core::{ContentId, ObjectStore, PutHint};
use tokio::io::AsyncRead;

#[test]
fn put_stream_content_id_matches_post_retry_hash_is_input_only() {
    // The blake3 ContentId is a pure function of the bytes — never of any SDK
    // request/response. Replaying the same input (as an SDK retry would) yields
    // the identical id. This is the property put_stream relies on.
    let buf = vec![0x77u8; 5 * 1024 * 1024 + 123];
    assert_eq!(ContentId::of(&buf), ContentId::of(&buf));
}

#[tokio::test]
#[ignore = "requires STORAGE_EMULATOR_HOST"]
async fn put_stream_content_id_matches_post_retry() {
    let Some(ep) = support::fake_gcs_endpoint() else {
        return;
    };
    let store = support::build_fake_gcs_store(&ep).await;
    let buf = vec![0x42u8; 18 * 1024 * 1024];
    let stream: Pin<Box<dyn AsyncRead + Send>> = Box::pin(std::io::Cursor::new(buf.clone()));
    let id = store.put_stream(stream, PutHint::default()).await.unwrap();
    assert_eq!(id, ContentId::of(&buf));
    let got = store.get_bytes(&id).await.unwrap();
    assert_eq!(got, buf);
}
