//! Pitfall #5 fixture: a preempted resumable upload re-uploads cleanly with no
//! `upload_id` persistence.
//!
//! We start a `put_stream`, abandon it mid-stream (drop the future at ~1 MiB),
//! then start a brand-new `put_stream` with the same input. Because the GCS
//! resumable session URL is never persisted across the (simulated) process
//! boundary, the second attempt re-uploads from byte 0 and the final object hash
//! still matches `blake3(input)`. Content addressing makes the re-upload
//! idempotent. `#[ignore]`'d; runs in `cloud-emulator-gcp` against fake-gcs-server.

#![cfg(feature = "gcp")]
#![allow(deprecated)] // exercising overridden put_stream (trait-level #[deprecated])

mod support;

use std::pin::Pin;

use rollout_core::{ContentId, ObjectStore, PutHint};
use tokio::io::AsyncRead;

#[tokio::test]
#[ignore = "requires STORAGE_EMULATOR_HOST"]
async fn gcs_resumable_upload_preempted_mid_stream_reuploads_cleanly() {
    let Some(ep) = support::fake_gcs_endpoint() else {
        return;
    };
    let store = support::build_fake_gcs_store(&ep).await;

    let buf = vec![0xC3u8; 24 * 1024 * 1024];

    // Attempt 1: abandon the upload after ~1 MiB by dropping the future.
    {
        let stream: Pin<Box<dyn AsyncRead + Send>> = Box::pin(std::io::Cursor::new(buf.clone()));
        let fut = store.put_stream(stream, PutHint::default());
        tokio::pin!(fut);
        // Poll briefly then drop — simulates a spot preemption mid-stream. No
        // upload_id is persisted, so nothing leaks into the next attempt.
        let _ = tokio::time::timeout(std::time::Duration::from_millis(5), &mut fut).await;
    }

    // Attempt 2 (fresh "process"): re-upload from byte 0 with the same input.
    let stream: Pin<Box<dyn AsyncRead + Send>> = Box::pin(std::io::Cursor::new(buf.clone()));
    let id = store.put_stream(stream, PutHint::default()).await.unwrap();

    assert_eq!(id, ContentId::of(&buf));
    assert!(store.exists(&id).await.unwrap());
    let got = store.get_bytes(&id).await.unwrap();
    assert_eq!(got, buf);
}
