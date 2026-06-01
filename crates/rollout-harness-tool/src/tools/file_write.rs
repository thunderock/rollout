//! `file_write` (`SideEffectClass::Filesystem`): write a file confined to the
//! per-invocation cap-std tempdir root. In-process, no subprocess; cap-std
//! rejects `..`/symlink escapes by construction (PITFALLS 10b).

use std::path::{Path, PathBuf};

use rollout_core::CoreError;
use serde::Deserialize;

use crate::sandbox::capfs::CapRoot;

/// `file_write` args: a relative path + contents.
#[derive(Debug, Deserialize)]
pub struct Args {
    /// Relative path within the tempdir root.
    pub path: PathBuf,
    /// Bytes to write (UTF-8).
    pub contents: String,
}

/// Write a file within the cap-std root; returns the byte count written.
///
/// # Errors
/// Returns [`CoreError`] on bad args, path escape, or IO failure.
pub fn run(root: &Path, args: serde_json::Value) -> Result<usize, CoreError> {
    let args: Args = serde_json::from_value(args)
        .map_err(|e| crate::config_invalid(format!("file_write args: {e}")))?;
    let cap = CapRoot::open(root)?;
    let bytes = args.contents.into_bytes();
    cap.write(&args.path, &bytes)?;
    Ok(bytes.len())
}
