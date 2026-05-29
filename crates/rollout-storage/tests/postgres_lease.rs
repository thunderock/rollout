//! D-LEASE-01/02 LOAD-BEARING — the single-row CAS coordinator-lease PRIMITIVE
//! proven on the real Postgres backend.
//!
//! `StorageLease` (rollout-coordinator) is ONE impl over `Arc<dyn Storage>` that
//! wraps `StorageTxn::cas_bytes` with TTL/epoch semantics — D-LEASE-01's claim is
//! "two backends for free over one lease, because `cas_bytes` is dual-backed."
//! The embedded redb half is the every-commit witness (lease.rs inline tests in
//! 06-01). This file proves the OTHER half: the SAME CAS-on-exact-prior-bytes
//! lease semantics hold on the Postgres `cas_bytes` (SELECT ... FOR UPDATE
//! value-compare) path, in the `postgres-integration` CI lane.
//!
//! `rollout-storage` cannot depend on `rollout-coordinator` (dep-direction lint),
//! so this exercises the lease PRIMITIVE directly: it builds the same
//! `LeaseRecord` (rollout-core) rows `StorageLease` writes and CASes them on the
//! exact prior bytes — single-winner acquire, monotonic-on-steal epoch, and
//! renew-after-steal-fails. The wrapper itself is covered by the embedded
//! every-commit witness; here we prove the dual-backed CAS the wrapper rides on.
//!
//! Default-fire on `ubuntu-latest` in CI when invoked with `--include-ignored`.
//! Locally: `make postgres-test`.

#![cfg(feature = "postgres")]

use std::time::Duration;

use rollout_core::{CoordEpoch, LeaseRecord, RunId, Storage, StorageKey, WorkerId};
use rollout_storage::PostgresStorage;
use smol_str::SmolStr;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;
use ulid::Ulid;

async fn start_postgres() -> (testcontainers::ContainerAsync<Postgres>, String) {
    let container = Postgres::default()
        .start()
        .await
        .expect("start postgres container");
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("postgres host port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    (container, url)
}

/// Retry loop: the container reports "running" before PG accepts connections.
async fn new_storage_with_retry(url: &str) -> PostgresStorage {
    let mut last_err = None;
    for attempt in 0..30 {
        match PostgresStorage::new(url, 4).await {
            Ok(s) => return s,
            Err(e) => {
                last_err = Some(e);
                tokio::time::sleep(Duration::from_secs(2)).await;
                if attempt == 0 {
                    eprintln!("waiting for postgres readiness...");
                }
            }
        }
    }
    panic!("postgres never became ready: {last_err:?}");
}

/// The single `coordinator_lease` row of a run — the exact key `StorageLease` uses.
fn lease_key(run_id: RunId) -> StorageKey {
    StorageKey {
        namespace: SmolStr::new_static("coordinator_lease"),
        run_id: Some(run_id),
        path: vec![],
    }
}

fn encode(rec: &LeaseRecord) -> Vec<u8> {
    postcard::to_stdvec(rec).expect("encode LeaseRecord")
}

/// CAS the lease row on `expected` prior bytes to `next`. Mirrors
/// `StorageLease::commit_claim` (the lease-row half).
async fn cas_lease(
    storage: &PostgresStorage,
    run_id: RunId,
    expected: Option<Vec<u8>>,
    next: &LeaseRecord,
) -> bool {
    let mut txn = storage.begin().await.expect("begin");
    let won = txn
        .cas_bytes(lease_key(run_id), expected, Some(encode(next)))
        .await
        .expect("cas_bytes");
    if won {
        txn.commit().await.expect("commit");
    } else {
        txn.abort().await.expect("abort");
    }
    won
}

fn record(holder: WorkerId, epoch: u64, expires_at_ms: u128) -> LeaseRecord {
    LeaseRecord {
        holder,
        epoch: CoordEpoch(epoch),
        expires_at_ms,
    }
}

/// SC1 on the PG backend: two `try_acquire` on a FRESH row (`expected = None`) —
/// exactly one wins, one loses. The dual-backed CAS gives single-winner on PG.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires Docker / testcontainers"]
async fn pg_lease_single_winner() {
    let (_c, url) = start_postgres().await;
    let storage = new_storage_with_retry(&url).await;
    let run_id = RunId(Ulid::new());

    let a = WorkerId(Ulid::new());
    let b = WorkerId(Ulid::new());
    let won_a = cas_lease(&storage, run_id, None, &record(a, 0, 10_000)).await;
    // B races the SAME fresh row (expected still None): the row now exists, so
    // B's CAS-on-None must lose. Exactly one winner.
    let won_b = cas_lease(&storage, run_id, None, &record(b, 0, 10_000)).await;

    assert!(won_a, "first acquire on a fresh row wins");
    assert!(!won_b, "second acquire on the same fresh row loses (single winner)");
}

/// D-LEASE steal: acquire at epoch 0, then a steal-on-expiry CASes the EXACT
/// prior bytes to epoch 1 (MONOTONIC). The new holder's epoch advanced by one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires Docker / testcontainers"]
async fn pg_lease_steal_advances_epoch() {
    let (_c, url) = start_postgres().await;
    let storage = new_storage_with_retry(&url).await;
    let run_id = RunId(Ulid::new());

    let old = WorkerId(Ulid::new());
    let thief = WorkerId(Ulid::new());
    let r0 = record(old, 0, 1); // expires_at_ms in the past -> stealable
    assert!(cas_lease(&storage, run_id, None, &r0).await, "fresh acquire epoch 0");

    // Steal: CAS the exact prior bytes (the expired epoch-0 record) to epoch 1.
    let prior = storage
        .get_bytes(&lease_key(run_id))
        .await
        .expect("get")
        .expect("lease row present");
    let r1 = record(thief, 1, 10_000);
    assert!(
        cas_lease(&storage, run_id, Some(prior), &r1).await,
        "steal CAS on exact prior bytes advances epoch 0 -> 1"
    );

    let after: LeaseRecord = postcard::from_bytes(
        &storage
            .get_bytes(&lease_key(run_id))
            .await
            .expect("get")
            .expect("present"),
    )
    .expect("decode");
    assert_eq!(after.epoch, CoordEpoch(1), "epoch advanced monotonically");
    assert_eq!(after.holder, thief, "thief holds the lease");
}

/// D-FENCE: after a steal advances the epoch, the OLD holder's renew (a CAS on
/// its now-stale epoch-0 bytes) must FAIL — it was fenced.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires Docker / testcontainers"]
async fn pg_lease_renew_after_steal_fails() {
    let (_c, url) = start_postgres().await;
    let storage = new_storage_with_retry(&url).await;
    let run_id = RunId(Ulid::new());

    let old = WorkerId(Ulid::new());
    let thief = WorkerId(Ulid::new());
    let r0 = record(old, 0, 1);
    assert!(cas_lease(&storage, run_id, None, &r0).await, "old acquires epoch 0");
    // The old holder snapshots the bytes it believes are current (epoch 0).
    let old_expected = encode(&r0);

    // A thief steals -> epoch 1 (CAS on the exact prior epoch-0 bytes).
    let prior = storage
        .get_bytes(&lease_key(run_id))
        .await
        .expect("get")
        .expect("present");
    assert!(
        cas_lease(&storage, run_id, Some(prior), &record(thief, 1, 10_000)).await,
        "thief steals to epoch 1"
    );

    // The old holder's renew CAS uses its stale epoch-0 expected bytes -> loses.
    let renewed = record(old, 0, 20_000);
    let won = cas_lease(&storage, run_id, Some(old_expected), &renewed).await;
    assert!(!won, "old holder's renew on stale (epoch-0) bytes fails after steal");

    let after: LeaseRecord = postcard::from_bytes(
        &storage
            .get_bytes(&lease_key(run_id))
            .await
            .expect("get")
            .expect("present"),
    )
    .expect("decode");
    assert_eq!(after.holder, thief, "thief still holds the lease");
    assert_eq!(after.epoch, CoordEpoch(1), "epoch unchanged by the failed renew");
}
