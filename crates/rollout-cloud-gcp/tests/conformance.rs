//! Emulator-backed conformance suite for `rollout-cloud-gcp`.
//!
//! GCS tests `#[ignore]` by default and early-return unless `STORAGE_EMULATOR_HOST`
//! is set; Pub/Sub tests need `PUBSUB_EMULATOR_HOST`. The Secret Manager tests
//! use the Docker-free in-process mock and run on every `--features gcp` build.
//! The `cloud-emulator-gcp` CI job runs the `#[ignore]`'d set via `--include-ignored`.

#![cfg(feature = "gcp")]
#![allow(deprecated)] // exercising overridden put_stream/get_stream (trait-level #[deprecated])

mod support;

use std::pin::Pin;

use rollout_core::{ContentId, ObjectStore, PutHint};
use tokio::io::AsyncRead;

// ---------------------------------------------------------------------------
// GCS conformance (Task 1)
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires STORAGE_EMULATOR_HOST"]
async fn gcs_object_store_put_bytes_get_bytes_round_trip() {
    let Some(ep) = support::fake_gcs_endpoint() else {
        return;
    };
    let store = support::build_fake_gcs_store(&ep).await;
    let id = store
        .put_bytes(b"hello".to_vec(), PutHint::default())
        .await
        .unwrap();
    assert_eq!(id, ContentId::of(b"hello"));
    let got = store.get_bytes(&id).await.unwrap();
    assert_eq!(got, b"hello");
}

#[tokio::test]
#[ignore = "requires STORAGE_EMULATOR_HOST"]
async fn gcs_object_store_exists_returns_false_for_missing() {
    let Some(ep) = support::fake_gcs_endpoint() else {
        return;
    };
    let store = support::build_fake_gcs_store(&ep).await;
    let id = ContentId::of(b"never-written-payload");
    assert!(!store.exists(&id).await.unwrap());
}

#[tokio::test]
#[ignore = "requires STORAGE_EMULATOR_HOST"]
async fn gcs_object_store_put_stream_content_id_matches_put_bytes() {
    let Some(ep) = support::fake_gcs_endpoint() else {
        return;
    };
    let store = support::build_fake_gcs_store(&ep).await;
    // 32 MiB forces the multi-chunk resumable path (>16 MiB chunk).
    let buf = vec![0xABu8; 32 * 1024 * 1024];
    let stream: Pin<Box<dyn AsyncRead + Send>> = Box::pin(std::io::Cursor::new(buf.clone()));
    let id = store.put_stream(stream, PutHint::default()).await.unwrap();
    assert_eq!(id, ContentId::of(&buf));
    assert!(store.exists(&id).await.unwrap());
}

#[tokio::test]
#[ignore = "requires STORAGE_EMULATOR_HOST"]
async fn gcs_object_store_get_stream_yields_full_payload() {
    use tokio::io::AsyncReadExt;
    let Some(ep) = support::fake_gcs_endpoint() else {
        return;
    };
    let store = support::build_fake_gcs_store(&ep).await;
    let buf = vec![0x5Au8; 20 * 1024 * 1024];
    let stream: Pin<Box<dyn AsyncRead + Send>> = Box::pin(std::io::Cursor::new(buf.clone()));
    let id = store.put_stream(stream, PutHint::default()).await.unwrap();
    let mut rd = store.get_stream(&id).await.unwrap();
    let mut out = Vec::new();
    rd.read_to_end(&mut out).await.unwrap();
    assert_eq!(out, buf);
}
