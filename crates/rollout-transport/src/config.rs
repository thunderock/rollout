//! Transport configuration with plan-time cross-field invariants (D-TIME-02).
//!
//! `JsonSchema` derive is intentionally not implemented in Phase 2 — the
//! `with = "humantime_serde"` attribute clashes with schemars 1.x derive
//! expectations. The schema-gen pipeline only consumes top-level CLI config
//! types; downstream consumers can wrap this struct if they need schemas.

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

/// Phase-2 transport configuration. Defaults match CONTEXT D-TIME-01.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransportConfig {
    /// Address the coordinator binds. Default `127.0.0.1:50051`.
    #[serde(default = "defaults::listen")]
    pub listen_addr: SocketAddr,
    /// Directory holding the dev CA + per-host certs (gitignored).
    #[serde(default = "defaults::tls_dir")]
    pub tls_dir: PathBuf,
    /// Heartbeat publish interval (D-TIME-01: 500ms).
    #[serde(default = "defaults::hb_interval", with = "humantime_serde")]
    pub heartbeat_interval: Duration,
    /// Worker self-fences after this many missed heartbeats (D-TIME-01: 4s).
    #[serde(default = "defaults::self_fence", with = "humantime_serde")]
    pub worker_self_fence_timeout: Duration,
    /// Coordinator marks worker failed past `due_at + this` (D-TIME-01: 5s).
    #[serde(default = "defaults::coord_timeout", with = "humantime_serde")]
    pub coordinator_failure_timeout: Duration,
    /// Allowed clock-skew between worker and coordinator (D-TIME-01: 250ms).
    #[serde(default = "defaults::skew", with = "humantime_serde")]
    pub clock_skew_budget: Duration,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            listen_addr: defaults::listen(),
            tls_dir: defaults::tls_dir(),
            heartbeat_interval: defaults::hb_interval(),
            worker_self_fence_timeout: defaults::self_fence(),
            coordinator_failure_timeout: defaults::coord_timeout(),
            clock_skew_budget: defaults::skew(),
        }
    }
}

impl TransportConfig {
    /// Plan-time invariants per D-TIME-02. Runs at `rollout plan`, never at runtime.
    ///
    /// Enforces:
    /// 1. `worker_self_fence_timeout < coordinator_failure_timeout` (split-brain
    ///    prevention; spec 05 §6).
    /// 2. `clock_skew_budget < heartbeat_interval × 2`.
    ///
    /// # Errors
    /// Returns a list of violation strings when any invariant fails.
    pub fn validate_cross_fields(&self) -> Result<(), Vec<String>> {
        let mut errs = Vec::new();
        if self.worker_self_fence_timeout >= self.coordinator_failure_timeout {
            errs.push(
                "transport.worker_self_fence_timeout must be strictly less than \
                 transport.coordinator_failure_timeout (split-brain prevention)"
                    .into(),
            );
        }
        if self.clock_skew_budget >= self.heartbeat_interval * 2 {
            errs.push(
                "transport.clock_skew_budget must be less than 2 × transport.heartbeat_interval"
                    .into(),
            );
        }
        if errs.is_empty() {
            Ok(())
        } else {
            Err(errs)
        }
    }
}

mod defaults {
    use super::{Duration, PathBuf, SocketAddr};

    pub fn listen() -> SocketAddr {
        "127.0.0.1:50051".parse().expect("static literal")
    }
    pub fn tls_dir() -> PathBuf {
        PathBuf::from("./data/tls")
    }
    pub fn hb_interval() -> Duration {
        Duration::from_millis(500)
    }
    pub fn self_fence() -> Duration {
        Duration::from_secs(4)
    }
    pub fn coord_timeout() -> Duration {
        Duration::from_secs(5)
    }
    pub fn skew() -> Duration {
        Duration::from_millis(250)
    }
}
