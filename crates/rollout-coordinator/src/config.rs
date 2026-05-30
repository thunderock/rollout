//! Coordinator TOML config (Phase-2 minimal surface).

use std::time::Duration;

use rollout_storage::EmbeddedStorageConfig;
use rollout_transport::TransportConfig;
use serde::{Deserialize, Serialize};

/// Coordinator run configuration (Phase-2 minimal).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoordinatorConfig {
    /// Run ID this coordinator serves. ULID format.
    pub run_id: String,
    /// Embedded storage location.
    #[serde(default)]
    pub storage: EmbeddedStorageConfig,
    /// Transport (listen addr + TLS dir + heartbeat timings).
    #[serde(default)]
    pub transport: TransportConfig,
}

impl CoordinatorConfig {
    /// Coordinator-lease TTL: a dead coordinator's lease expires exactly when
    /// workers would declare it failed — so it reuses `coordinator_failure_timeout`
    /// (do NOT re-derive timing; the transport config is the source of truth).
    #[must_use]
    pub fn lease_ttl(&self) -> Duration {
        self.transport.coordinator_failure_timeout
    }

    /// Lease-renew cadence: one renewal per `heartbeat_interval` (~10 per TTL window).
    #[must_use]
    pub fn lease_renew_interval(&self) -> Duration {
        self.transport.heartbeat_interval
    }

    /// Validate cross-field invariants at plan-time.
    ///
    /// Delegates to [`TransportConfig::validate_cross_fields`] (`self_fence <
    /// coord_failure`, skew bound) and additionally asserts the lease timing
    /// against the transport bounds: the lease TTL must equal
    /// `coordinator_failure_timeout` and the renew cadence must be strictly
    /// shorter than the TTL (so a missed renewal is not immediately fatal).
    ///
    /// # Errors
    /// Returns the accumulated list of violation strings.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut violations = match self.transport.validate_cross_fields() {
            Ok(()) => Vec::new(),
            Err(v) => v,
        };
        if self.lease_ttl() != self.transport.coordinator_failure_timeout {
            violations.push(
                "coordinator lease TTL must equal transport.coordinator_failure_timeout"
                    .to_string(),
            );
        }
        if self.lease_renew_interval() >= self.lease_ttl() {
            violations.push(
                "coordinator lease renew cadence must be strictly less than the lease TTL \
                 (coordinator_failure_timeout)"
                    .to_string(),
            );
        }
        if violations.is_empty() {
            Ok(())
        } else {
            Err(violations)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> CoordinatorConfig {
        CoordinatorConfig {
            run_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string(),
            storage: EmbeddedStorageConfig::default(),
            transport: TransportConfig::default(),
        }
    }

    #[test]
    fn lease_ttl_equals_coord_failure() {
        let c = cfg();
        assert_eq!(c.lease_ttl(), c.transport.coordinator_failure_timeout);
        assert_eq!(c.lease_renew_interval(), c.transport.heartbeat_interval);
        assert!(c.lease_renew_interval() < c.lease_ttl());
        c.validate().expect("default lease timing is valid");
    }

    #[test]
    fn renew_cadence_not_below_ttl_is_rejected() {
        let mut c = cfg();
        // Force renew cadence == TTL (no slack) -> rejected.
        c.transport.heartbeat_interval = c.transport.coordinator_failure_timeout;
        let errs = c
            .validate()
            .expect_err("renew cadence == TTL must be rejected");
        assert!(errs.iter().any(|e| e.contains("renew cadence")));
    }
}
