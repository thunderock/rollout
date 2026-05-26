//! `rollout worker run` body — Phase-2 minimal worker runtime.
//!
//! Opens local Storage + builds a `PluginHostImpl`, dials the coordinator over
//! mTLS using a per-worker client cert issued from the dev CA, sends an
//! implicit-register `Beat` (state=Init), then beats every
//! `heartbeat_interval` until SIGTERM arrives, at which point it sends a final
//! `Beat(Draining)` and exits.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rollout_core::{CoreError, FatalError, PluginHost};
use rollout_plugin_host::PluginHostImpl;
use rollout_proto::transport::v1::{
    heartbeat_client::HeartbeatClient, BeatRequest, WorkerState as ProtoState,
};
use rollout_storage::{EmbeddedStorage, EmbeddedStorageConfig};
use rollout_transport::TransportConfig;
use serde::{Deserialize, Serialize};

/// Worker TOML config. Mirrors `CoordinatorConfig` with a `coordinator_addr`
/// instead of a `listen_addr` to dial.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerConfig {
    /// Run ID this worker joins.
    pub run_id: String,
    /// `host:port` address of the coordinator (e.g. `https://localhost:50051`).
    pub coordinator_addr: String,
    /// SNI name to present to the coordinator. Default `localhost`.
    #[serde(default = "default_domain")]
    pub coordinator_domain: String,
    /// Per-worker storage location.
    #[serde(default = "default_worker_storage")]
    pub storage: EmbeddedStorageConfig,
    /// Transport timings (`heartbeat_interval` drives the beat loop).
    #[serde(default)]
    pub transport: TransportConfig,
}

fn default_domain() -> String {
    "localhost".into()
}

fn default_worker_storage() -> EmbeddedStorageConfig {
    EmbeddedStorageConfig {
        path: PathBuf::from("./data/worker.db"),
    }
}

fn internal<E: std::fmt::Display>(e: E) -> CoreError {
    CoreError::Fatal(FatalError::Internal { msg: e.to_string() })
}

/// Worker entry point invoked by `rollout worker run`.
///
/// # Errors
/// Returns `Fatal(Internal)` or `Fatal(ConfigInvalid)` on config / IO failure.
#[allow(clippy::too_many_lines)]
pub async fn run(
    config_path: PathBuf,
    worker_id_arg: Option<String>,
    plugin_paths: Vec<PathBuf>,
    _hot_reload: bool,
) -> Result<(), CoreError> {
    // 1. Load + validate config.
    let raw = std::fs::read_to_string(&config_path).map_err(internal)?;
    let cfg: WorkerConfig = toml::from_str(&raw).map_err(internal)?;
    cfg.transport.validate_cross_fields().map_err(|errs| {
        CoreError::Fatal(FatalError::ConfigInvalid {
            msg: format!("worker config invalid: {errs:?}"),
        })
    })?;

    let run_id_ulid: ulid::Ulid = cfg.run_id.parse().map_err(|e| {
        CoreError::Fatal(FatalError::ConfigInvalid {
            msg: format!("run_id is not a valid ULID: {e}"),
        })
    })?;
    let worker_ulid: ulid::Ulid = match worker_id_arg {
        Some(s) => s.parse().map_err(|e| {
            CoreError::Fatal(FatalError::ConfigInvalid {
                msg: format!("--worker-id is not a valid ULID: {e}"),
            })
        })?,
        None => ulid::Ulid::new(),
    };
    tracing::info!(worker_id = %worker_ulid, run_id = %run_id_ulid, "worker_starting");

    // 2. Storage + plugin host.
    if let Some(parent) = cfg.storage.path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(internal)?;
        }
    }
    let storage = Arc::new(EmbeddedStorage::open(&cfg.storage.path).await?);
    let host = PluginHostImpl::with_storage(storage);

    // 3. Load any --plugin manifests.
    for path in &plugin_paths {
        let manifest_raw = std::fs::read_to_string(path).map_err(internal)?;
        let manifest = rollout_plugin_host::parse_manifest_str(&manifest_raw).map_err(|e| {
            CoreError::Fatal(FatalError::ConfigInvalid {
                msg: format!("plugin manifest {}: {e}", path.display()),
            })
        })?;
        // Best-effort load; surface errors as Fatal(Internal) for the smoke test.
        let _handle = host.load(manifest).await?;
        tracing::info!(manifest = %path.display(), "plugin_loaded");
    }

    // 4. mTLS channel to coordinator.
    let (ca_cert, ca_key) = rollout_transport::tls::ensure_dev_ca(&cfg.transport.tls_dir)?;
    let (cli_cert, cli_key) = rollout_transport::tls::issue_client_cert(
        &ca_cert,
        &ca_key,
        &[format!("worker-{worker_ulid}"), "localhost".into()],
    )?;
    let channel = rollout_transport::client::build_mtls_channel(
        cfg.coordinator_addr.clone(),
        &cfg.coordinator_domain,
        ca_cert,
        cli_cert,
        cli_key,
    )?;
    let mut hb_client = HeartbeatClient::new(channel);

    // 5. Heartbeat loop.
    let hb_interval = cfg.transport.heartbeat_interval;
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);

    let shutdown_tx_for_signal = shutdown_tx.clone();
    tokio::spawn(async move {
        let Ok(mut sig) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        else {
            return;
        };
        sig.recv().await;
        let _ = shutdown_tx_for_signal.send(true);
    });

    let mut ticker = tokio::time::interval(hb_interval);
    let mut state = ProtoState::Init;
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let beat = BeatRequest {
                    worker_id: worker_ulid.to_string(),
                    run_id: run_id_ulid.to_string(),
                    state: state as i32,
                    due_at: Some(systime_to_prost(SystemTime::now() + hb_interval * 2)),
                };
                if let Err(e) = hb_client.beat(beat).await {
                    tracing::warn!(error = %e, "heartbeat send failed (transient)");
                } else if state == ProtoState::Init {
                    state = ProtoState::Ready;
                }
            }
            Ok(()) = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    break;
                }
            }
        }
    }

    // 6. Drain beat.
    let drain = BeatRequest {
        worker_id: worker_ulid.to_string(),
        run_id: run_id_ulid.to_string(),
        state: ProtoState::Draining as i32,
        due_at: Some(systime_to_prost(SystemTime::now() + hb_interval * 2)),
    };
    let _ = hb_client.beat(drain).await;
    tracing::info!(worker_id = %worker_ulid, "worker_draining_complete");
    Ok(())
}

fn systime_to_prost(t: SystemTime) -> prost_types::Timestamp {
    let d = t.duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO);
    prost_types::Timestamp {
        seconds: i64::try_from(d.as_secs()).unwrap_or(i64::MAX),
        nanos: i32::try_from(d.subsec_nanos()).unwrap_or(0),
    }
}
