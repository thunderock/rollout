//! End-to-end coordinator boot: open Storage, bootstrap TLS dev CA, build the
//! `CoordinatorImpl`, mount the transport services, and spawn the
//! failure-scan loop. Re-exported as `rollout_coordinator::run` so the
//! `rollout-cli` `coordinator run` subcommand can delegate without duplicating
//! logic.

use std::path::Path;
use std::sync::Arc;

use rollout_core::{CoreError, FatalError, RunId};
use rollout_storage::EmbeddedStorage;

use crate::config::CoordinatorConfig;
use crate::emitter::StdoutJsonEmitter;
use crate::failure_scan::failure_scan_loop;
use crate::heartbeat::CoordinatorImpl;

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
