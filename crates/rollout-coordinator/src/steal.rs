//! Coordinator-mediated work-stealing with CAS-on-state dedup (DIST-02).
//!
//! Protocol (spec 05 §5, D-STEAL-01..04): an idle worker asks the coordinator
//! for work; the coordinator picks the busiest peer as victim and reassigns
//! `ceil(victim_backlog / 2)` items (capped at [`MAX_STEAL_BATCH`]) from the
//! victim to the thief. Workers never talk peer-to-peer — the coordinator is
//! the sole broker.
//!
//! Each reassignment is a two-step CAS over the SAME prior `Running(victim)`
//! bytes: `try_repending` (`Running(victim) -> Pending`) then `try_claim`
//! (`Pending -> Running(thief)`). The CAS expected-bytes drift is what makes a
//! stolen-then-reclaimed item never double-execute (RESEARCH "Why this is
//! idempotent"):
//!
//! - If the victim's ack wins the race (`Running(victim) -> Done`), the steal's
//!   `try_repending` sees stale `expected` bytes, returns `false`, and the item
//!   is skipped — never stolen, never double-run.
//! - If the steal wins (`Running(victim) -> Pending`), the victim's late ack
//!   sees stale `expected`, returns `false`, and its result is dropped — the
//!   victim never committed a terminal transition, so no double-execute.
//!
//! Exactly one of the two CAS attempts can win, the same single-winner property
//! proven in `cas_state_machine.rs` and witnessed by
//! `concurrent_ack_and_steal_no_double_execute`.

use rollout_core::{
    ContentId, CoreError, FatalError, KeyRange, RunId, Storage, StorageKey, WorkerId,
};
use smol_str::SmolStr;

use crate::ledger;
use crate::work_item::{self, WorkItemRecord, WorkState};

/// Maximum items reassigned in a single steal.
///
/// Fixed for v1.1 (D-STEAL); NOT a config knob — revisit if a tuning need
/// emerges. Bounds steal churn so a single idle worker cannot drain a busy peer
/// in one request.
pub const MAX_STEAL_BATCH: usize = 32;

const WORK_NAMESPACE: &str = "work";

fn work_prefix(run_id: &RunId) -> KeyRange {
    KeyRange {
        prefix: StorageKey {
            namespace: SmolStr::new_static(WORK_NAMESPACE),
            run_id: Some(*run_id),
            path: vec![SmolStr::new_static("item")],
        },
        limit: None,
    }
}

fn decode(bytes: &[u8]) -> Result<WorkItemRecord, CoreError> {
    postcard::from_bytes(bytes).map_err(|e| {
        CoreError::Fatal(FatalError::Internal {
            msg: format!("postcard WorkItemRecord decode: {e}"),
        })
    })
}

/// Handle a steal request from an idle `thief`.
///
/// Returns the work ids actually reassigned to `thief` (possibly empty). Steps:
///
/// 1. **Guard (D-STEAL-01):** if `thief` already holds `Running` items, this is
///    not an idle worker — return empty (no-op). A worker steals only when its
///    local queue drains to empty.
/// 2. **Victim (D-STEAL-03):** the busiest peer by `Running` backlog, excluding
///    `thief`. If none, return empty.
/// 3. **Batch (D-STEAL-02):** `n = min(ceil(victim_backlog / 2), MAX_STEAL_BATCH)`.
/// 4. **Reassign (D-STEAL-04):** for the first `n` of the victim's `Running`
///    items, in one transaction per item: `try_repending` then `try_claim` for
///    `thief`. A lost `try_repending` (the victim acked first) skips the item —
///    no double-execute.
///
/// # Errors
/// Propagates storage / CAS errors from the underlying calls.
pub async fn handle_steal_request(
    storage: &dyn Storage,
    run_id: &RunId,
    thief: WorkerId,
    now_ms: u128,
) -> Result<Vec<ContentId>, CoreError> {
    // 1. Guard: only idle thieves steal.
    if ledger::backlog(storage, run_id, thief).await? > 0 {
        return Ok(Vec::new());
    }

    // 2. Victim = busiest peer.
    let Some((victim, victim_backlog)) = ledger::busiest(storage, run_id, thief).await? else {
        return Ok(Vec::new());
    };

    // 3. ceil(victim_backlog / 2), capped.
    let n = victim_backlog.div_ceil(2).min(MAX_STEAL_BATCH);
    if n == 0 {
        return Ok(Vec::new());
    }

    // Gather the victim's Running items (deterministic order by work id).
    let entries = storage.scan_bytes(work_prefix(run_id)).await?;
    let mut victim_items: Vec<WorkItemRecord> = entries
        .into_iter()
        .filter_map(|(_, bytes)| decode(&bytes).ok())
        .filter(|rec| matches!(rec.state, WorkState::Running { worker_id, .. } if worker_id == victim))
        .collect();
    victim_items.sort_by(|a, b| a.id.0.cmp(&b.id.0));

    // 4. Reassign the first n via CAS (try_repending -> try_claim).
    let mut stolen = Vec::new();
    for running in victim_items.into_iter().take(n) {
        let mut txn = storage.begin().await?;
        // Running(victim) -> Pending. Lost CAS == victim acked first; skip.
        if !work_item::try_repending(&mut txn, run_id, &running).await? {
            txn.abort().await?;
            continue;
        }
        // Pending -> Running(thief). The pending record we re-claim is `running`
        // with state flipped to Pending (the bytes try_repending just wrote).
        let pending = WorkItemRecord {
            id: running.id,
            state: WorkState::Pending,
        };
        if work_item::try_claim(&mut txn, run_id, &pending, thief, now_ms).await? {
            txn.commit().await?;
            stolen.push(running.id);
        } else {
            // Re-claim lost (another dispatcher beat us). Roll back the repend so
            // we never strand an item Pending; a later steal/dispatch retries.
            txn.abort().await?;
        }
    }

    Ok(stolen)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger;
    use crate::work_item::work_key;
    use rollout_storage::EmbeddedStorage;
    use std::sync::Arc;
    use ulid::Ulid;

    async fn open() -> Arc<dyn Storage> {
        let tmp = tempfile::tempdir().unwrap();
        let storage = EmbeddedStorage::open(tmp.path().join("rollout.redb"))
            .await
            .unwrap();
        std::mem::forget(tmp);
        Arc::new(storage)
    }

    /// Seed `n` `Running(worker)` items with distinct ids.
    async fn seed_running(
        storage: &Arc<dyn Storage>,
        run_id: &RunId,
        worker: WorkerId,
        n: usize,
        tag: &str,
    ) {
        let mut txn = storage.begin().await.unwrap();
        for k in 0..n {
            let id = ContentId::of(format!("{tag}-{k}").as_bytes());
            let rec = WorkItemRecord {
                id,
                state: WorkState::Running {
                    worker_id: worker,
                    started_at_ms: 1,
                },
            };
            txn.put_bytes(work_key(run_id, &id), postcard::to_stdvec(&rec).unwrap())
                .await
                .unwrap();
        }
        txn.commit().await.unwrap();
    }

    #[tokio::test]
    async fn steal_takes_ceil_half_capped() {
        // 7 items -> ceil(7/2) = 4.
        let storage = open().await;
        let run_id = RunId(Ulid::new());
        let victim = WorkerId(Ulid::new());
        let thief = WorkerId(Ulid::new());
        seed_running(&storage, &run_id, victim, 7, "v").await;

        let stolen = handle_steal_request(storage.as_ref(), &run_id, thief, 10)
            .await
            .unwrap();
        assert_eq!(stolen.len(), 4, "ceil(7/2) = 4 items stolen");
        assert_eq!(ledger::backlog(storage.as_ref(), &run_id, thief).await.unwrap(), 4);
        assert_eq!(ledger::backlog(storage.as_ref(), &run_id, victim).await.unwrap(), 3);

        // 100 items -> capped at MAX_STEAL_BATCH (32).
        let storage2 = open().await;
        let run2 = RunId(Ulid::new());
        let victim2 = WorkerId(Ulid::new());
        let thief2 = WorkerId(Ulid::new());
        seed_running(&storage2, &run2, victim2, 100, "w").await;
        let stolen2 = handle_steal_request(storage2.as_ref(), &run2, thief2, 10)
            .await
            .unwrap();
        assert_eq!(stolen2.len(), MAX_STEAL_BATCH, "capped at MAX_STEAL_BATCH");
    }

    #[tokio::test]
    async fn steal_only_when_local_empty() {
        // Thief already has a Running item -> not idle -> no-op (D-STEAL-01).
        let storage = open().await;
        let run_id = RunId(Ulid::new());
        let victim = WorkerId(Ulid::new());
        let thief = WorkerId(Ulid::new());
        seed_running(&storage, &run_id, victim, 6, "v").await;
        seed_running(&storage, &run_id, thief, 1, "t").await;

        let stolen = handle_steal_request(storage.as_ref(), &run_id, thief, 10)
            .await
            .unwrap();
        assert!(stolen.is_empty(), "a non-idle thief steals nothing");
        // Victim untouched.
        assert_eq!(ledger::backlog(storage.as_ref(), &run_id, victim).await.unwrap(), 6);
    }

    #[tokio::test]
    async fn steal_reassigns_via_cas() {
        let storage = open().await;
        let run_id = RunId(Ulid::new());
        let victim = WorkerId(Ulid::new());
        let thief = WorkerId(Ulid::new());
        seed_running(&storage, &run_id, victim, 2, "v").await;

        let stolen = handle_steal_request(storage.as_ref(), &run_id, thief, 99)
            .await
            .unwrap();
        assert_eq!(stolen.len(), 1, "ceil(2/2) = 1");

        // The stolen item is Running(thief), not Running(victim).
        let x = stolen[0];
        let bytes = storage.get_bytes(&work_key(&run_id, &x)).await.unwrap().unwrap();
        let rec: WorkItemRecord = decode(&bytes).unwrap();
        assert!(
            matches!(rec.state, WorkState::Running { worker_id, .. } if worker_id == thief),
            "stolen item must be Running(thief): {:?}",
            rec.state
        );
    }
}
