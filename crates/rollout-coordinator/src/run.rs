//! End-to-end coordinator boot: open Storage, bootstrap TLS dev CA, build the
//! `CoordinatorImpl`, mount the transport services, and spawn the
//! failure-scan loop. Re-exported as `rollout_coordinator::run` so the
//! `rollout-cli` `coordinator run` subcommand can delegate without duplicating
//! logic.
//!
//! ## Stateless-replayer boot (DIST-03)
//!
//! A fresh coordinator is stateless: it reconstructs its in-flight assignment
//! map from Storage rather than holding it in memory (D-RESTART-02). The boot
//! order is `lease acquire -> adopt epoch -> replay ledger -> resume
//! failure_scan -> serve` (06-RESEARCH §5), factored into [`replay_and_serve`]
//! so the Sim harness can drive it in-process. The replay step `scan_bytes`es
//! the `work` namespace and:
//!
//! - `Running{worker}` -> reconstruct the in-flight assignment (do NOT requeue
//!   — the worker may still hold it; only the `failure_scan` stale path
//!   re-pends after `coordinator_failure_timeout`);
//! - `Pending` -> push onto the dispatch queue;
//! - `Done` / `Failed` -> terminal, skip (idempotent, never re-execute).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use rollout_core::{
    CoordEpoch, CoordinatorLease, CoreError, FatalError, KeyRange, RunId, Storage, StorageKey,
    WorkerId,
};
use rollout_storage::EmbeddedStorage;
use smol_str::SmolStr;

use crate::config::CoordinatorConfig;
use crate::emitter::StdoutJsonEmitter;
use crate::failure_scan::failure_scan_loop;
use crate::heartbeat::CoordinatorImpl;
use crate::lease::StorageLease;
use crate::work_item::{WorkItemRecord, WorkState};

/// Load `CoordinatorConfig` from a TOML file at `path`.
///
/// # Errors
/// Returns `Fatal(Internal)` on I/O or parse failure; `Fatal(ConfigInvalid)`
/// when `TransportConfig::validate_cross_fields` rejects the file.
pub fn load_config(path: &Path) -> Result<CoordinatorConfig, CoreError> {
    let raw = std::fs::read_to_string(path).map_err(|e| {
        CoreError::Fatal(FatalError::Internal {
            msg: format!("read coordinator config {}: {e}", path.display()),
        })
    })?;
    let cfg: CoordinatorConfig = toml::from_str(&raw).map_err(|e| {
        CoreError::Fatal(FatalError::Internal {
            msg: format!("parse coordinator config: {e}"),
        })
    })?;
    cfg.validate().map_err(|errs| {
        CoreError::Fatal(FatalError::ConfigInvalid {
            msg: format!("coordinator config invalid: {errs:?}"),
        })
    })?;
    Ok(cfg)
}

/// Prefix for scanning the whole `work` ledger of a run (`work/<run>/item/*`).
fn work_prefix(run_id: &RunId) -> KeyRange {
    KeyRange {
        prefix: StorageKey {
            namespace: SmolStr::new_static("work"),
            run_id: Some(*run_id),
            path: vec![SmolStr::new_static("item")],
        },
        limit: None,
    }
}

/// The outcome of replaying the work ledger on a fresh coordinator's boot.
///
/// This is the in-memory state a stateless replayer reconstructs from Storage
/// — it is NOT persisted; a restart rebuilds it by scanning `work` again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayState {
    /// The adopted authoritative epoch (the lease's epoch).
    pub epoch: CoordEpoch,
    /// In-flight assignment map: `work_id -> owning worker`. Reconstructed from
    /// `Running` rows WITHOUT requeuing — the worker may still hold the item.
    pub in_flight: HashMap<rollout_core::ContentId, WorkerId>,
    /// `work_id`s found `Pending` — to be (re)pushed onto the dispatch queue.
    pub pending: Vec<rollout_core::ContentId>,
    /// `work_id`s found terminal (`Done`/`Failed`) — skipped (no re-execute).
    pub terminal: usize,
}

/// Lease-gated stateless-replayer boot (DIST-03 / D-RESTART-02), the in-process
/// half of [`run`] the Sim harness drives directly.
///
/// Boot order (06-RESEARCH §5):
///
/// 1. **lease**: `try_acquire(me, ttl)`. `None` -> another coordinator is live;
///    the loser exits cleanly (`Ok(None)`, spec 05 §8). The caller wires
///    `ttl = coordinator_failure_timeout`.
/// 2. **epoch**: adopt `lease.epoch` (advanced by a prior steal). The transport
///    layer stamps it on every RPC response (via [`crate::epoch::stamp_epoch`]).
/// 3. **replay**: `scan_bytes` the `work` namespace and reconstruct the
///    in-flight assignment map. `Running` items are reconstructed but NOT
///    requeued (Pitfall 4 — the worker may still hold them); `Pending` items
///    are collected for dispatch; `Done`/`Failed` are skipped (idempotent).
///
/// Returns `Some(ReplayState)` when this caller won the lease, `None` when it
/// lost (a live coordinator already holds it). Resuming the failure-scan loop
/// and serving the transport are the caller's responsibility (see [`run`]);
/// `replay_and_serve` is the boot-decision half so a test can assert the
/// reconstruct-without-requeue invariant without binding a socket.
///
/// # Errors
/// Propagates storage / CAS errors from the lease acquire or the ledger scan.
pub async fn replay_and_serve(
    storage: Arc<dyn Storage>,
    run_id: RunId,
    me: WorkerId,
    ttl: Duration,
) -> Result<Option<ReplayState>, CoreError> {
    // 1. lease: win the lease or exit cleanly (loser exits, spec 05 §8).
    let lease = StorageLease::new(storage.clone(), run_id);
    let Some(held) = lease.try_acquire(me, ttl).await? else {
        return Ok(None);
    };

    // 2. epoch: adopt the advanced epoch the lease handed us.
    let mut state = ReplayState {
        epoch: held.epoch,
        in_flight: HashMap::new(),
        pending: Vec::new(),
        terminal: 0,
    };

    // 3. replay ledger: reconstruct in-flight, collect Pending, skip terminal.
    //    Do NOT requeue Running items here — the worker may still hold them; the
    //    failure_scan stale path is the only requeue path (Pitfall 4).
    let entries = storage.scan_bytes(work_prefix(&run_id)).await?;
    for (_key, bytes) in entries {
        let Ok(rec) = postcard::from_bytes::<WorkItemRecord>(&bytes) else {
            continue;
        };
        match rec.state {
            WorkState::Running { worker_id, .. } => {
                // reconstruct the in-flight assignment (in-flight, no requeue).
                state.in_flight.insert(rec.id, worker_id);
            }
            WorkState::Pending => state.pending.push(rec.id),
            WorkState::Done { .. } | WorkState::Failed { .. } => state.terminal += 1,
        }
    }

    Ok(Some(state))
}

/// Spawn the lease-renew loop. Renews at `interval` (< `ttl`); on a lost renew
/// (the epoch advanced under us -> we were fenced) emits exactly one
/// `coordinator_fenced` event and `std::process::abort()`s at the binary edge
/// (D-FENCE-01..03). The renewing identity is `me`; `start_epoch` is the epoch
/// the replayer adopted.
fn spawn_lease_renew_loop(
    storage: Arc<dyn Storage>,
    emitter: Arc<dyn rollout_core::EventEmitter>,
    run_id: RunId,
    me: WorkerId,
    interval: Duration,
    ttl: Duration,
    start_epoch: CoordEpoch,
) {
    tokio::spawn(async move {
        let lease = StorageLease::new(storage, run_id);
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            let Ok(Some(held)) = lease.current().await else {
                continue;
            };
            if held.holder != me {
                // someone else holds the lease — we were deposed (fenced).
                let _ = crate::fence::fence_old_coordinator(
                    emitter.as_ref(),
                    me,
                    run_id,
                    start_epoch,
                    held.epoch,
                )
                .await;
                std::process::abort();
            }
            match lease.renew(&held, ttl).await {
                Ok(true) => {}
                Ok(false) => {
                    let observed = lease
                        .current()
                        .await
                        .ok()
                        .flatten()
                        .map_or(held.epoch, |c| c.epoch);
                    let _ = crate::fence::fence_old_coordinator(
                        emitter.as_ref(),
                        me,
                        run_id,
                        held.epoch,
                        observed,
                    )
                    .await;
                    std::process::abort();
                }
                Err(e) => {
                    tracing::warn!(target: "coordinator", error = %format!("{e:?}"), "lease_renew_error");
                }
            }
        }
    });
}

/// Boot the coordinator from a parsed config. Listens until the process is
/// signalled (SIGTERM) or the gRPC server errors.
///
/// # Errors
/// Returns `Fatal(Internal)` on storage open, TLS bootstrap, or transport
/// listener failure.
pub async fn run(cfg: CoordinatorConfig) -> Result<(), CoreError> {
    let run_id_ulid: ulid::Ulid = cfg.run_id.parse().map_err(|e| {
        CoreError::Fatal(FatalError::ConfigInvalid {
            msg: format!("run_id is not a valid ULID: {e}"),
        })
    })?;

    // 1. Storage
    if let Some(parent) = cfg.storage.path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                CoreError::Fatal(FatalError::Internal {
                    msg: format!("create storage dir {}: {e}", parent.display()),
                })
            })?;
        }
    }
    let storage: Arc<dyn rollout_core::Storage> =
        Arc::new(EmbeddedStorage::open(&cfg.storage.path).await?);

    // 1b. Stateless-replayer boot (DIST-03 / D-RESTART-02): win the lease, adopt
    //     the advanced epoch, and reconstruct in-flight assignments from the
    //     `work` ledger BEFORE serving. A loser (another coordinator holds a live
    //     lease) exits cleanly per spec 05 §8.
    let me = WorkerId(ulid::Ulid::new());
    let ttl = cfg.lease_ttl();
    let Some(replayed) = replay_and_serve(storage.clone(), RunId(run_id_ulid), me, ttl).await?
    else {
        eprintln!("another coordinator holds the lease; exiting cleanly (spec 05 §8 loser exits)");
        return Ok(());
    };
    tracing::info!(
        epoch = replayed.epoch.0,
        in_flight = replayed.in_flight.len(),
        pending = replayed.pending.len(),
        terminal = replayed.terminal,
        "coordinator_replay_complete",
    );

    // 2. TLS dev CA + server cert
    let ca_pem_existed = cfg.transport.tls_dir.join("ca.pem").exists();
    let (ca_cert, ca_key) = rollout_transport::tls::ensure_dev_ca(&cfg.transport.tls_dir)?;
    if !ca_pem_existed {
        eprintln!(
            "Generated dev CA at {}",
            cfg.transport.tls_dir.join("ca.pem").display()
        );
    }
    let (srv_cert, srv_key) = rollout_transport::tls::issue_server_cert(
        &ca_cert,
        &ca_key,
        &["localhost".into(), "127.0.0.1".into()],
    )?;

    // 3. Emitter + coordinator + transport services (D-OBSERVE-01)
    let emitter: Arc<dyn rollout_core::EventEmitter> = Arc::new(StdoutJsonEmitter::default());
    let coord_impl = Arc::new(CoordinatorImpl::new(
        storage.clone(),
        RunId(run_id_ulid),
        emitter.clone(),
    ));
    let hb_svc = rollout_transport::channels::HeartbeatServiceImpl::new(
        coord_impl.clone() as Arc<dyn rollout_core::Coordinator>
    );
    let ctrl_svc = rollout_transport::channels::ControlServiceImpl::new(
        rollout_transport::channels::control::ControlRouter::new(),
    );
    let work_svc = rollout_transport::channels::WorkServiceImpl::new();

    // 4. Failure-scan loop — tick at heartbeat_interval / 2 so a single missed
    //    beat is detected within `2 × heartbeat_interval` (SUBSTR-02 acceptance).
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let scan_interval = cfg.transport.heartbeat_interval / 2;
    let scan_handle = tokio::spawn(failure_scan_loop(
        storage.clone(),
        emitter.clone(),
        scan_interval,
        cfg.transport.clock_skew_budget,
        cfg.transport.coordinator_failure_timeout,
        shutdown_rx,
    ));

    // 4b. Lease-renew loop — renew at heartbeat cadence (< TTL); on a lost renew
    //     (epoch advanced) self-fence + abort at the binary edge (D-FENCE-01..03).
    spawn_lease_renew_loop(
        storage.clone(),
        emitter.clone(),
        RunId(run_id_ulid),
        me,
        cfg.lease_renew_interval(),
        ttl,
        replayed.epoch,
    );

    // 5. SIGTERM handler — drives the watch channel and stops the scan loop.
    let shutdown_tx_for_signal = shutdown_tx.clone();
    tokio::spawn(async move {
        if let Ok(mut sig) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            sig.recv().await;
            let _ = shutdown_tx_for_signal.send(true);
        }
    });

    // 6. Serve over TLS
    tracing::info!(addr = %cfg.transport.listen_addr, "coordinator_serving");
    let serve_result = rollout_transport::server::serve(
        cfg.transport.listen_addr,
        srv_cert,
        srv_key,
        ca_cert,
        hb_svc,
        ctrl_svc,
        work_svc,
    )
    .await;

    // Stop the failure-scan loop on serve exit.
    let _ = shutdown_tx.send(true);
    let _ = scan_handle.await;
    serve_result
}
