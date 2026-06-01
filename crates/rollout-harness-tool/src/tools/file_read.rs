//! `file_read` (`SideEffectClass::Filesystem`): read a file confined to the
//! per-invocation cap-std tempdir root. In-process, no subprocess; cap-std
//! rejects `..`/symlink escapes by construction (PITFALLS 10b).

use std::path::{Path, PathBuf};

use rollout_core::CoreError;
use serde::Deserialize;

use crate::sandbox::capfs::CapRoot;

/// `file_read` args: a path relative to the sandbox tempdir root.
#[derive(Debug, Deserialize)]
pub struct Args {
    /// Relative path within the tempdir root.
    pub path: PathBuf,
}

/// Read a file within the cap-std root; returns its UTF-8-lossy contents.
///
/// # Errors
/// Returns [`CoreError`] on bad args, path escape, or IO failure.
pub fn run(root: &Path, args: serde_json::Value) -> Result<String, CoreError> {
    let args: Args = serde_json::from_value(args)
        .map_err(|e| crate::config_invalid(format!("file_read args: {e}")))?;
    let cap = CapRoot::open(root)?;
    let bytes = cap.read(&args.path)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
