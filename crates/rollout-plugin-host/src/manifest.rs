//! `rollout-plugin.toml` manifest parsing + plan-time validation.

use std::path::Path;

use rollout_core::{CoreError, FatalError, PluginManifest, PluginMode};

fn cfg_invalid(msg: impl Into<String>) -> CoreError {
    CoreError::Fatal(FatalError::ConfigInvalid { msg: msg.into() })
}

/// Parse a `rollout-plugin.toml` from disk.
pub fn parse_manifest(path: &Path) -> Result<PluginManifest, CoreError> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| cfg_invalid(format!("read manifest {}: {e}", path.display())))?;
    parse_manifest_str(&raw)
}

/// Parse a manifest from a TOML string.
pub fn parse_manifest_str(s: &str) -> Result<PluginManifest, CoreError> {
    let manifest: PluginManifest =
        toml::from_str(s).map_err(|e| cfg_invalid(format!("manifest toml: {e}")))?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

/// Plan-time validation: cheap structural checks before any I/O.
pub fn validate_manifest(m: &PluginManifest) -> Result<(), CoreError> {
    if m.name.trim().is_empty() {
        return Err(cfg_invalid("manifest.name must be non-empty"));
    }
    if m.version.trim().is_empty() {
        return Err(cfg_invalid("manifest.version must be non-empty"));
    }
    match m.mode {
        PluginMode::Pyo3 => {
            let py = m
                .runtime
                .python_min
                .as_ref()
                .ok_or_else(|| cfg_invalid("pyo3 plugins require runtime.python_min"))?;
            require_python_311(py)?;
        }
        PluginMode::Sidecar | PluginMode::RustCdylib => {}
    }
    Ok(())
}

fn require_python_311(v: &str) -> Result<(), CoreError> {
    let mut parts = v.split('.');
    let major: u32 = parts
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| cfg_invalid(format!("python_min: bad version {v}")))?;
    let minor: u32 = parts
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| cfg_invalid(format!("python_min: bad version {v}")))?;
    if (major, minor) < (3, 11) {
        return Err(cfg_invalid(format!(
            "python_min {v} below 3.11 (required for stdlib tomllib + PyO3 abi3)"
        )));
    }
    Ok(())
}
