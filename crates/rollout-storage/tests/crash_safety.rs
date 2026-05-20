//! Crash-safety harness.
//!
//! The active test drops the txn without commit, which redb treats identically
//! to a SIGKILL between `put` and `commit` from the on-disk-state perspective
//! (the new value never reached the redb commit B-tree before fsync). The
//! "true SIGKILL across a separate process" variant is gated `#[ignore]` +
//! `#[cfg(target_os = "linux")]` because it needs `nix`-style raw signals and
//! a helper binary; Phase-2 CI does not yet provide a Linux test job.

use rollout_core::{Storage, StorageKey};
use rollout_storage::EmbeddedStorage;
use smol_str::SmolStr;
use tempfile::TempDir;

fn key(ns: &str, segs: &[&str]) -> StorageKey {
    StorageKey {
        namespace: SmolStr::new(ns),
        run_id: None,
        path: segs.iter().map(|s| SmolStr::new(*s)).collect(),
    }
}

#[tokio::test]
async fn crash_simulation_drop_does_not_corrupt() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("rollout.db");

    // Write 10 keys but DO NOT commit — drop the txn.
    {
        let db = EmbeddedStorage::open(&path).await.expect("open");
        let mut txn = db.begin().await.expect("begin");
        for i in 0u8..10u8 {
            txn.put_bytes(key("workers", &[&format!("w{i}")]), vec![i])
                .await
                .expect("put");
        }
        // explicit drop — equivalent to redb's abort-on-drop semantics.
        drop(txn);
    }

    // Reopen — none of the 10 keys must be visible.
    let db = EmbeddedStorage::open(&path).await.expect("reopen");
    for i in 0..10 {
        let k = key("workers", &[&format!("w{i}")]);
        let got = db.get_bytes(&k).await.expect("get");
        assert_eq!(got, None, "key w{i} visible after non-committed txn");
    }
}

#[tokio::test]
#[ignore = "needs helper binary + raw-signal harness; tracked for Phase-6 (DIST-03)"]
async fn crash_sigkill_helper_does_not_corrupt() {
    // Placeholder: a future PR (Phase 6 DIST-03 restart-from-storage tests)
    // will spawn a helper child via tokio::process::Command, kill -KILL the
    // PID between put and commit, then reopen here and assert no partial
    // writes are visible. Skipped for Phase-2 CI per RESEARCH §"Pitfall —
    // SIGKILL test harness".
}
