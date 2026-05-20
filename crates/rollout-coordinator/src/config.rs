//! Coordinator TOML config (Phase-2 minimal surface).

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
    /// Validate cross-field invariants at plan-time (delegates to `TransportConfig`).
    ///
    /// # Errors
    /// Returns the list of violation strings from `TransportConfig::validate_cross_fields`.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        self.transport.validate_cross_fields()
    }
}
