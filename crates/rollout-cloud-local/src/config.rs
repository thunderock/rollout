//! `rollout-cloud-local` configuration types.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Configuration for the cloud-local substrate Layer-1 impls (D-LOCAL-01..04).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CloudLocalConfig {
    /// Filesystem root for `FsObjectStore`. Default: `./data/object-store`.
    #[serde(default = "default_obj_root")]
    pub object_store_root: PathBuf,
    /// Allowlist of secret names (without the `ROLLOUT_SECRET_` prefix) that
    /// `EnvSecretStore` may read.
    #[serde(default)]
    pub secret_allowlist: Vec<String>,
}

fn default_obj_root() -> PathBuf {
    PathBuf::from("./data/object-store")
}

impl Default for CloudLocalConfig {
    fn default() -> Self {
        Self {
            object_store_root: default_obj_root(),
            secret_allowlist: Vec::new(),
        }
    }
}
