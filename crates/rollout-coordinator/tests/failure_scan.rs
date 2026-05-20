//! Failure-scan loop tests — deadline-based health (CONTEXT D-COORD-01 + D-TIME-01).

use async_trait::async_trait;
use rollout_coordinator::failure_scan::failure_scan_loop;
use rollout_coordinator::registry::{heartbeat_key, HeartbeatRecord};
use rollout_core::{CoreError, Event, EventEmitter, EventKind, Storage, WorkerId};
use rollout_storage::EmbeddedStorage;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Test emitter that records every `worker_failed` topic to a `Mutex<Vec<_>>`.
#[derive(Default, Clone)]
struct CaptureEmitter {
    failed: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl EventEmitter for CaptureEmitter {
    async fn emit(&self, event: Event) -> Result<(), CoreError> {
        if let EventKind::Domain { topic } = &event.kind {
            if topic.as_str() == "worker_failed" {
                let wid = event
                    .worker_id
                    .map(|w| w.0.to_string())
                    .unwrap_or_default();
                self.failed.lock().unwrap().push(wid);
            }
        }
        Ok(())
    }
}

async fn write_heartbeat(
    storage: &Arc<dyn Storage>,
    worker_id: WorkerId,
    due_at: SystemTime,
) {
    let due_ms = due_at
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let rec = HeartbeatRecord {
        worker_id: worker_id.0.to_string(),
        run_id: ulid::Ulid::new().to_string(),
        state: 2,
        due_at_ms: due_ms,
        received_at_ms: 0,
    };
    let mut txn = storage.begin().await.unwrap();
    txn.put_bytes(heartbeat_key(&worker_id), postcard::to_allocvec(&rec).unwrap())
        .await
        .unwrap();
    txn.commit().await.unwrap();
}

async fn open_storage(path: &std::path::Path) -> Arc<dyn Storage> {
    Arc::new(EmbeddedStorage::open(path).await.unwrap())
}

#[tokio::test]
async fn failure_scan_marks_late_workers() {
    let tmp = tempfile::tempdir().unwrap();
    let storage = open_storage(&tmp.path().join("rollout.db")).await;
    let emitter = CaptureEmitter::default();
    let w1 = WorkerId(ulid::Ulid::new());
    // Overdue by 10 seconds — well past both skew + coord_timeout.
    let due = SystemTime::now() - Duration::from_secs(10);
    write_heartbeat(&storage, w1, due).await;

    let (tx, rx) = tokio::sync::watch::channel(false);
    let handle = tokio::spawn(failure_scan_loop(
        storage.clone(),
        Arc::new(emitter.clone()),
        Duration::from_millis(50),
        Duration::from_millis(250),
        Duration::from_secs(5),
        rx,
    ));
    tokio::time::sleep(Duration::from_millis(150)).await;
    let _ = tx.send(true);
    let _ = handle.await;

    let captured = emitter.failed.lock().unwrap().clone();
    assert!(
        captured.contains(&w1.0.to_string()),
        "expected worker_failed for {w1:?}, captured = {captured:?}",
    );
}

#[tokio::test]
async fn failure_scan_does_not_mark_healthy_workers() {
    let tmp = tempfile::tempdir().unwrap();
    let storage = open_storage(&tmp.path().join("rollout.db")).await;
    let emitter = CaptureEmitter::default();
    let w1 = WorkerId(ulid::Ulid::new());
    let due = SystemTime::now() + Duration::from_secs(10);
    write_heartbeat(&storage, w1, due).await;

    let (tx, rx) = tokio::sync::watch::channel(false);
    let handle = tokio::spawn(failure_scan_loop(
        storage.clone(),
        Arc::new(emitter.clone()),
        Duration::from_millis(50),
        Duration::from_millis(250),
        Duration::from_secs(5),
        rx,
    ));
    tokio::time::sleep(Duration::from_millis(150)).await;
    let _ = tx.send(true);
    let _ = handle.await;

    let captured = emitter.failed.lock().unwrap().clone();
    assert!(captured.is_empty(), "expected no failures, got {captured:?}");
}

#[tokio::test]
async fn failure_scan_respects_skew_budget() {
    // Overdue by less than skew_budget → should NOT mark failed.
    let tmp = tempfile::tempdir().unwrap();
    let storage = open_storage(&tmp.path().join("rollout.db")).await;
    let emitter = CaptureEmitter::default();
    let w1 = WorkerId(ulid::Ulid::new());
    let due = SystemTime::now() - Duration::from_millis(50); // less than 250ms skew
    write_heartbeat(&storage, w1, due).await;

    let (tx, rx) = tokio::sync::watch::channel(false);
    let handle = tokio::spawn(failure_scan_loop(
        storage.clone(),
        Arc::new(emitter.clone()),
        Duration::from_millis(50),
        Duration::from_millis(250),
        Duration::from_secs(5),
        rx,
    ));
    tokio::time::sleep(Duration::from_millis(150)).await;
    let _ = tx.send(true);
    let _ = handle.await;

    let captured = emitter.failed.lock().unwrap().clone();
    assert!(
        captured.is_empty(),
        "within skew budget — expected no failure, got {captured:?}",
    );
}

#[tokio::test]
async fn failure_scan_loop_runs_periodically() {
    let tmp = tempfile::tempdir().unwrap();
    let storage = open_storage(&tmp.path().join("rollout.db")).await;
    let emitter = CaptureEmitter::default();
    let w1 = WorkerId(ulid::Ulid::new());
    // Strongly overdue.
    let due = SystemTime::now() - Duration::from_secs(20);
    write_heartbeat(&storage, w1, due).await;

    let (tx, rx) = tokio::sync::watch::channel(false);
    let handle = tokio::spawn(failure_scan_loop(
        storage.clone(),
        Arc::new(emitter.clone()),
        Duration::from_millis(50),
        Duration::from_millis(250),
        Duration::from_secs(5),
        rx,
    ));
    // Within 200ms the loop should have ticked at least once.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let _ = tx.send(true);
    let _ = handle.await;

    let captured = emitter.failed.lock().unwrap().clone();
    assert_eq!(
        captured.len(),
        1,
        "deduped: exactly one event for an overdue worker, got {captured:?}",
    );
}
