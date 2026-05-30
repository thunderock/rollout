//! DIST-05 witness (SC4): an old coordinator whose lease was stolen self-fences.
//!
//! Two parts:
//! - `split_brain_old_coord_self_fences` (in-process): A acquires epoch 0; the
//!   lease expires; B steals (epoch 1); A's `renew(stale)` returns false; calling
//!   `fence_old_coordinator` emits EXACTLY ONE `coordinator_fenced` event and
//!   returns `Abort`; A writes NO shared state (the lease row still reads B's
//!   epoch 1). Runs over `EmbeddedStorage`, Docker-free.
//! - `fence_aborts_within_5s` (subprocess): the `--test-fence` subcommand runs the
//!   REAL `std::process::abort()` in a child and exits non-zero (SIGABRT) within 5s.

use std::sync::Arc;
use std::time::Duration;

use rollout_coordinator::fence::{fence_old_coordinator, FenceDecision};
use rollout_coordinator::lease::{NowFn, StorageLease};
use rollout_core::{CoordinatorLease, RunId, Storage, WorkerId};
use rollout_storage::EmbeddedStorage;
use std::sync::atomic::{AtomicU64, Ordering};
use ulid::Ulid;

#[path = "support/mod.rs"]
mod support;
use support::abort_harness::run_fence_subprocess;
use support::CountingEmitter;

async fn open() -> Arc<dyn Storage> {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rollout.redb");
    let storage = EmbeddedStorage::open(&path).await.unwrap();
    std::mem::forget(tmp);
    Arc::new(storage)
}

fn fake_clock() -> (NowFn, Arc<AtomicU64>) {
    let t = Arc::new(AtomicU64::new(1_000_000));
    let handle = t.clone();
    let now: NowFn = Arc::new(move || u128::from(t.load(Ordering::SeqCst)));
    (now, handle)
}

#[tokio::test]
async fn split_brain_old_coord_self_fences() {
    let storage = open().await;
    let run = RunId(Ulid::new());
    let (now, clock) = fake_clock();
    let ttl = Duration::from_millis(50);
    let lease = StorageLease::with_clock(storage.clone(), run, now);

    // 1. Old coordinator A acquires the lease at epoch 0.
    let coord_a = WorkerId(Ulid::new());
    let held_a = lease
        .try_acquire(coord_a, ttl)
        .await
        .unwrap()
        .expect("A wins");
    assert_eq!(held_a.epoch.0, 0);

    // 2. A's lease expires (it stalled / GC paused).
    clock.fetch_add(100, Ordering::SeqCst);

    // 3. New coordinator B steals the lease -> epoch advances to 1.
    let coord_b = WorkerId(Ulid::new());
    let held_b = lease
        .try_acquire(coord_b, ttl)
        .await
        .unwrap()
        .expect("B steals");
    assert_eq!(held_b.epoch.0, 1, "steal MUST advance epoch monotonically");

    // 4. A wakes up and tries to renew its STALE lease -> CAS fails (fenced).
    let renewed = lease.renew(&held_a, ttl).await.unwrap();
    assert!(
        !renewed,
        "old coordinator must NOT renew after epoch advance"
    );

    // 5. WITNESS: A's fence routine emits EXACTLY ONE coordinator_fenced event
    //    with NO shared-state write, then decides Abort.
    let fenced = CountingEmitter::default();
    let decision = fence_old_coordinator(&fenced, coord_a, run, held_a.epoch, held_b.epoch).await;
    assert_eq!(decision, FenceDecision::Abort, "fenced coord must abort");
    assert_eq!(
        fenced.count("coordinator_fenced"),
        1,
        "exactly one fence event (D-FENCE-02)"
    );

    // 6. Shared state is intact: the lease row is still B's (A wrote nothing).
    let cur = lease.current().await.unwrap().unwrap();
    assert_eq!(
        cur.holder, coord_b,
        "B still holds; A wrote nothing (D-FENCE-01)"
    );
    assert_eq!(cur.epoch.0, 1);
}

#[tokio::test]
async fn fence_writes_no_shared_state() {
    // Snapshot the lease row before fence; assert bytes unchanged after.
    let storage = open().await;
    let run = RunId(Ulid::new());
    let lease = StorageLease::new(storage.clone(), run);
    let coord = WorkerId(Ulid::new());
    let held = lease
        .try_acquire(coord, Duration::from_secs(5))
        .await
        .unwrap()
        .expect("acquire");

    let key = rollout_core::StorageKey {
        namespace: smol_str::SmolStr::new_static("coordinator_lease"),
        run_id: Some(run),
        path: vec![],
    };
    let before = storage.get_bytes(&key).await.unwrap();

    let fenced = CountingEmitter::default();
    let _ = fence_old_coordinator(&fenced, coord, run, held.epoch, held.epoch).await;

    let after = storage.get_bytes(&key).await.unwrap();
    assert_eq!(
        before, after,
        "fence is observability-only; no shared-state write"
    );
}

#[test]
fn fence_aborts_within_5s() {
    // SC4 subprocess witness: the --test-fence subcommand takes the real
    // std::process::abort() path; the child exits non-zero (SIGABRT) within 5s.
    let run = run_fence_subprocess(&["test-fence", "0", "1"]);
    assert!(
        run.aborted(),
        "child must exit abnormally (SIGABRT from abort())"
    );
    assert!(
        run.elapsed < Duration::from_secs(5),
        "fence must abort within 5s (was {:?})",
        run.elapsed
    );
}
