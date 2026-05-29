//! localstack-backed conformance suite for `rollout-cloud-aws`.
//!
//! All tests `#[ignore]` by default and early-return unless `LOCALSTACK_ENDPOINT`
//! is set. The `cloud-emulator-aws` CI job runs them via `--include-ignored`.

#![allow(deprecated)] // exercising overridden put_stream/get_stream (trait-level #[deprecated])

mod support;

use std::pin::Pin;

use rollout_core::{ContentId, ObjectStore, PutHint};
use tokio::io::AsyncRead;

#[tokio::test]
#[ignore = "requires LOCALSTACK_ENDPOINT"]
async fn s3_object_store_put_bytes_get_bytes_round_trip() {
    let Some(ep) = support::localstack_endpoint() else {
        return;
    };
    let store = support::build_localstack_store(&ep).await;
    let id = store
        .put_bytes(b"hello".to_vec(), PutHint::default())
        .await
        .unwrap();
    assert_eq!(id, ContentId::of(b"hello"));
    let got = store.get_bytes(&id).await.unwrap();
    assert_eq!(got, b"hello");
}

#[tokio::test]
#[ignore = "requires LOCALSTACK_ENDPOINT"]
async fn s3_object_store_exists_returns_false_for_missing() {
    let Some(ep) = support::localstack_endpoint() else {
        return;
    };
    let store = support::build_localstack_store(&ep).await;
    let id = ContentId::of(b"never-written-payload");
    assert!(!store.exists(&id).await.unwrap());
}

#[tokio::test]
#[ignore = "requires LOCALSTACK_ENDPOINT"]
async fn s3_object_store_put_stream_content_id_matches_put_bytes() {
    let Some(ep) = support::localstack_endpoint() else {
        return;
    };
    let store = support::build_localstack_store(&ep).await;
    // 32 MiB forces the multipart path (>16 MiB chunk).
    let buf = vec![0xABu8; 32 * 1024 * 1024];
    let stream: Pin<Box<dyn AsyncRead + Send>> = Box::pin(std::io::Cursor::new(buf.clone()));
    let id = store.put_stream(stream, PutHint::default()).await.unwrap();
    assert_eq!(id, ContentId::of(&buf));
    assert!(store.exists(&id).await.unwrap());
}

#[tokio::test]
#[ignore = "requires LOCALSTACK_ENDPOINT"]
async fn s3_object_store_get_stream_yields_full_payload() {
    use tokio::io::AsyncReadExt;
    let Some(ep) = support::localstack_endpoint() else {
        return;
    };
    let store = support::build_localstack_store(&ep).await;
    let buf = vec![0x5Au8; 20 * 1024 * 1024];
    let stream: Pin<Box<dyn AsyncRead + Send>> = Box::pin(std::io::Cursor::new(buf.clone()));
    let id = store.put_stream(stream, PutHint::default()).await.unwrap();
    let mut rd = store.get_stream(&id).await.unwrap();
    let mut out = Vec::new();
    rd.read_to_end(&mut out).await.unwrap();
    assert_eq!(out, buf);
}

// ---------------------------------------------------------------------------
// SQS conformance (Task 2)
// ---------------------------------------------------------------------------

use rollout_core::traits::cloud::Queue;
use std::time::Duration;

fn build_sqs_queue(
    client: std::sync::Arc<aws_sdk_sqs::Client>,
    url: String,
) -> rollout_cloud_aws::SqsQueue {
    rollout_cloud_aws::SqsQueue::new(client, url, 30)
}

#[tokio::test]
#[ignore = "requires LOCALSTACK_ENDPOINT"]
async fn sqs_queue_enqueue_dequeue_round_trip() {
    let Some(ep) = support::localstack_endpoint() else {
        return;
    };
    let (client, url) = support::build_localstack_queue_url(&ep).await;
    let q = build_sqs_queue(client, url);
    let id = q.enqueue(b"payload".to_vec()).await.unwrap();
    let (got_id, payload) = q.dequeue().await.unwrap().expect("a message");
    assert_eq!(got_id, id);
    assert_eq!(payload, b"payload");
}

#[tokio::test]
#[ignore = "requires LOCALSTACK_ENDPOINT"]
async fn sqs_queue_ack_deletes_message() {
    let Some(ep) = support::localstack_endpoint() else {
        return;
    };
    let (client, url) = support::build_localstack_queue_url(&ep).await;
    let q = build_sqs_queue(client, url);
    q.enqueue(b"x".to_vec()).await.unwrap();
    let (id, _) = q.dequeue().await.unwrap().expect("a message");
    q.ack(id).await.unwrap();
    // After ack + visibility window, no message remains.
    let again = q.dequeue().await.unwrap();
    assert!(again.is_none(), "acked message must not redeliver");
}

#[tokio::test]
#[ignore = "requires LOCALSTACK_ENDPOINT"]
async fn sqs_queue_nack_makes_message_visible_immediately() {
    let Some(ep) = support::localstack_endpoint() else {
        return;
    };
    let (client, url) = support::build_localstack_queue_url(&ep).await;
    let q = build_sqs_queue(client, url);
    let enq = q.enqueue(b"y".to_vec()).await.unwrap();
    let (id, _) = q.dequeue().await.unwrap().expect("a message");
    q.nack(id).await.unwrap();
    let (id2, _) = q.dequeue().await.unwrap().expect("redelivered");
    assert_eq!(id2, enq);
}

#[tokio::test]
#[ignore = "requires LOCALSTACK_ENDPOINT"]
async fn sqs_queue_dequeue_with_lease_returns_receipt_handle_as_token() {
    let Some(ep) = support::localstack_endpoint() else {
        return;
    };
    let (client, url) = support::build_localstack_queue_url(&ep).await;
    let q = build_sqs_queue(client, url);
    q.enqueue(b"z".to_vec()).await.unwrap();
    let (_, _, token) = q
        .dequeue_with_lease(Duration::from_secs(30))
        .await
        .unwrap()
        .expect("a message");
    let receipt = std::str::from_utf8(&token.0).expect("receipt handle is utf-8");
    assert!(
        !receipt.is_empty(),
        "lease token must carry a receipt handle"
    );
}

#[tokio::test]
#[ignore = "requires LOCALSTACK_ENDPOINT (long: ~35s)"]
async fn sqs_queue_extend_lease_succeeds_via_change_message_visibility() {
    let Some(ep) = support::localstack_endpoint() else {
        return;
    };
    let (client, url) = support::build_localstack_queue_url(&ep).await;
    let q = build_sqs_queue(client, url);
    q.enqueue(b"lease".to_vec()).await.unwrap();
    let (id, _, token) = q
        .dequeue_with_lease(Duration::from_secs(30))
        .await
        .unwrap()
        .expect("a message");
    q.extend_lease(id, token, Duration::from_secs(60))
        .await
        .unwrap();
    // Within the extended window the message stays invisible.
    tokio::time::sleep(Duration::from_secs(35)).await;
    let still = q.dequeue().await.unwrap();
    assert!(
        still.is_none(),
        "message must stay invisible during the extended lease"
    );
}

#[tokio::test]
#[ignore = "requires LOCALSTACK_ENDPOINT"]
async fn sqs_queue_extend_lease_fails_on_stale_token() {
    let Some(ep) = support::localstack_endpoint() else {
        return;
    };
    let (client, url) = support::build_localstack_queue_url(&ep).await;
    let q = build_sqs_queue(client, url);
    q.enqueue(b"stale".to_vec()).await.unwrap();
    let (id, _, token) = q
        .dequeue_with_lease(Duration::from_secs(30))
        .await
        .unwrap()
        .expect("a message");
    q.ack(id).await.unwrap(); // delete the message; the receipt is now stale
    let err = q
        .extend_lease(id, token, Duration::from_secs(60))
        .await
        .expect_err("stale receipt must fail");
    assert!(
        matches!(
            err,
            rollout_core::CoreError::Recoverable(rollout_core::RecoverableError::Transient { .. })
        ),
        "stale receipt -> Recoverable::Transient, got {err:?}"
    );
}
