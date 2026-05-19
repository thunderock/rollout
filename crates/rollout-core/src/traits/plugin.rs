//! `Plugin` and `PluginHost` — user-supplied trait impls + their loader.

use async_trait::async_trait;

use crate::CoreError;

/// A unit of user-supplied behavior loaded at run time.
#[async_trait]
pub trait Plugin: Send + Sync {
    /// Plugin name as declared in its manifest.
    fn name(&self) -> &str;
    /// Plan-time validation: cheap, no I/O.
    async fn validate(&self) -> Result<(), CoreError>;
}

/// Loads, hot-reloads, and dispatches plugins.
#[async_trait]
pub trait PluginHost: Send + Sync {
    /// Load a plugin by name; returns `Err` on contract violation.
    async fn load(&self, name: &str) -> Result<(), CoreError>;
}
