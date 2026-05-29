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

// ---------------------------------------------------------------------------
// Pub/Sub conformance (Task 2)
// ---------------------------------------------------------------------------

use rollout_core::traits::cloud::Queue;
use std::time::Duration;

#[tokio::test]
#[ignore = "requires PUBSUB_EMULATOR_HOST"]
async fn pubsub_queue_enqueue_dequeue_round_trip() {
    let Some(_) = support::pubsub_emulator_host() else {
        return;
    };
    let q = support::build_emulator_pubsub_queue().await;
    let id = q.enqueue(b"x".to_vec()).await.unwrap();
    let (got_id, payload) = q.dequeue().await.unwrap().expect("a message");
    assert_eq!(got_id, id);
    assert_eq!(payload, b"x");
}

#[tokio::test]
#[ignore = "requires PUBSUB_EMULATOR_HOST"]
async fn pubsub_queue_ack_consumes_message() {
    let Some(_) = support::pubsub_emulator_host() else {
        return;
    };
    let q = support::build_emulator_pubsub_queue().await;
    q.enqueue(b"x".to_vec()).await.unwrap();
    let (id, _) = q.dequeue().await.unwrap().expect("a message");
    q.ack(id).await.unwrap();
    // After ack the message is consumed; a follow-up pull yields nothing.
    let again = q.dequeue().await.unwrap();
    assert!(again.is_none(), "acked message must not redeliver");
}

#[tokio::test]
#[ignore = "requires PUBSUB_EMULATOR_HOST"]
async fn pubsub_queue_nack_makes_message_visible() {
    let Some(_) = support::pubsub_emulator_host() else {
        return;
    };
    let q = support::build_emulator_pubsub_queue().await;
    let enq = q.enqueue(b"y".to_vec()).await.unwrap();
    let (id, _) = q.dequeue().await.unwrap().expect("a message");
    q.nack(id).await.unwrap();
    // nack sets the ack deadline to 0 -> immediate redelivery.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let (id2, _) = q.dequeue().await.unwrap().expect("redelivered");
    assert_eq!(id2, enq);
}

#[tokio::test]
#[ignore = "requires PUBSUB_EMULATOR_HOST"]
async fn pubsub_queue_dequeue_with_lease_returns_ack_id_as_token() {
    let Some(_) = support::pubsub_emulator_host() else {
        return;
    };
    let q = support::build_emulator_pubsub_queue().await;
    q.enqueue(b"z".to_vec()).await.unwrap();
    let (_, _, token) = q
        .dequeue_with_lease(Duration::from_secs(30))
        .await
        .unwrap()
        .expect("a message");
    let ack_id = std::str::from_utf8(&token.0).expect("ack_id is utf-8");
    assert!(!ack_id.is_empty(), "lease token must carry an ack_id");
}

#[tokio::test]
#[ignore = "requires PUBSUB_EMULATOR_HOST"]
async fn pubsub_queue_extend_lease_succeeds_via_modify_ack_deadline() {
    // Emulator caveat (README emulator delta): we assert the modify_ack_deadline
    // call SUCCEEDS, not the time-based redelivery side effect (that lives in
    // cloud-live-gcp). The emulator's ack-deadline redelivery is unreliable.
    let Some(_) = support::pubsub_emulator_host() else {
        return;
    };
    let q = support::build_emulator_pubsub_queue().await;
    q.enqueue(b"lease".to_vec()).await.unwrap();
    let (id, _, token) = q
        .dequeue_with_lease(Duration::from_secs(30))
        .await
        .unwrap()
        .expect("a message");
    q.extend_lease(id, token, Duration::from_secs(60))
        .await
        .unwrap();
}

// ---------------------------------------------------------------------------
// Secret Manager conformance (Task 3) — Docker-free via in-process hyper mock.
// ---------------------------------------------------------------------------

use rollout_core::traits::cloud::SecretStore;
use rollout_core::FatalError;
use std::collections::HashMap;

#[tokio::test]
async fn secret_manager_get_returns_secret_value_for_allowed_name() {
    let mut secrets = HashMap::new();
    secrets.insert("test-secret".to_owned(), "hello".to_owned());
    let mock = support::mock_secret_manager::spawn(secrets).await;
    let store = rollout_cloud_gcp::SecretManagerSecretStore::with_endpoint(
        &mock.endpoint,
        "test".to_owned(),
        vec!["test-secret".to_owned()],
    );
    assert_eq!(store.get("test-secret").await.unwrap(), "hello");
}

#[tokio::test]
async fn secret_manager_get_rejects_non_allowlisted_name() {
    let mock = support::mock_secret_manager::spawn(HashMap::new()).await;
    let store = rollout_cloud_gcp::SecretManagerSecretStore::with_endpoint(
        &mock.endpoint,
        "test".to_owned(),
        vec!["only-this".to_owned()],
    );
    let err = store.get("other").await.expect_err("must reject");
    match err {
        rollout_core::CoreError::Fatal(FatalError::ConfigInvalid { msg }) => {
            assert!(msg.contains("not in allowlist"), "got: {msg}");
        }
        other => panic!("expected ConfigInvalid, got {other:?}"),
    }
}

#[tokio::test]
async fn secret_manager_get_missing_secret_returns_fatal() {
    let mock = support::mock_secret_manager::spawn(HashMap::new()).await;
    let store = rollout_cloud_gcp::SecretManagerSecretStore::with_endpoint(
        &mock.endpoint,
        "test".to_owned(),
        vec!["nope".to_owned()],
    );
    let err = store.get("nope").await.expect_err("missing secret");
    assert!(
        matches!(
            err,
            rollout_core::CoreError::Fatal(FatalError::ConfigInvalid { .. })
        ),
        "missing secret -> Fatal::ConfigInvalid, got {err:?}"
    );
}

#[tokio::test]
async fn secret_manager_put_returns_read_only_error() {
    let store = rollout_cloud_gcp::SecretManagerSecretStore::with_endpoint(
        "http://unused",
        "test".to_owned(),
        vec![],
    );
    let err = store.put("any", "v").await.expect_err("read-only");
    match err {
        rollout_core::CoreError::Fatal(FatalError::ConfigInvalid { msg }) => {
            assert!(msg.contains("read-only in v1.1"), "got: {msg}");
        }
        other => panic!("expected ConfigInvalid, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// GCE MDS conformance (Task 4) — Docker-free via in-process mock MDS server.
// ---------------------------------------------------------------------------

use rollout_cloud_gcp::GceMetadataComputeHint;
use rollout_core::traits::cloud::ComputeHint;
use std::sync::atomic::Ordering;
use support::mock_mds::MdsFixture;

#[tokio::test]
async fn gce_metadata_compute_hint_inventory_yields_instance_type() {
    let mock = support::mock_mds::spawn(MdsFixture {
        machine_type: Some("projects/123/machineTypes/n1-standard-4".to_owned()),
        preempted: None,
    })
    .await;
    let hint = GceMetadataComputeHint::with_host(
        &mock.host,
        rollout_cloud_local::hints::for_current_platform(),
    );
    let inv = hint.inventory().await.unwrap();
    assert_eq!(inv.instance_type.as_deref(), Some("n1-standard-4"));
}

#[tokio::test]
async fn gce_metadata_compute_hint_preemption_signal_observes_preempt_flag() {
    let mock = support::mock_mds::spawn(MdsFixture {
        machine_type: None,
        preempted: Some("TRUE".to_owned()),
    })
    .await;
    let hint = GceMetadataComputeHint::with_host(
        &mock.host,
        rollout_cloud_local::hints::for_current_platform(),
    );
    assert!(hint.preemption_signal().await.unwrap().is_some());
}

#[tokio::test]
async fn gce_metadata_compute_hint_preemption_signal_no_notice_yet() {
    // 404 on instance/preempted -> Ok(None).
    let mock = support::mock_mds::spawn(MdsFixture::default()).await;
    let hint = GceMetadataComputeHint::with_host(
        &mock.host,
        rollout_cloud_local::hints::for_current_platform(),
    );
    assert!(hint.preemption_signal().await.unwrap().is_none());
}

#[tokio::test]
async fn gce_metadata_compute_hint_uses_metadata_flavor_header() {
    let mock = support::mock_mds::spawn(MdsFixture {
        machine_type: Some("projects/1/machineTypes/e2-medium".to_owned()),
        preempted: None,
    })
    .await;
    let hint = GceMetadataComputeHint::with_host(
        &mock.host,
        rollout_cloud_local::hints::for_current_platform(),
    );
    let _ = hint.inventory().await.unwrap();
    assert!(
        mock.saw_flavor_header.load(Ordering::SeqCst),
        "every MDS request must carry Metadata-Flavor: Google"
    );
}
