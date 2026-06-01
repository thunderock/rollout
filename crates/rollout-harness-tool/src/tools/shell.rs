//! `shell` (`SideEffectClass::Exec`): run an allowlisted command argv-vector
//! through the sandbox. shell=False — argv vector only, never a shell-interpreter
//! string (D-TOOL-06). The command name must resolve to an exact full path in the
//! configured allowlist.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use rollout_core::CoreError;
use serde::Deserialize;

use crate::sandbox::launcher::{self, ExecRequest, Rlimits};
use crate::ExecRun;

/// `shell` args: an argv vector (`argv[0]` is the command name to resolve).
#[derive(Debug, Deserialize)]
pub struct Args {
    /// argv vector — `argv[0]` resolved against the exact-full-path allowlist.
    pub argv: Vec<String>,
}

/// Run an allowlisted command argv-vector sandboxed.
///
/// # Errors
/// Returns [`CoreError`] if args are empty, the command is not allowlisted, or
/// the launcher errors.
#[allow(clippy::too_many_arguments)]
pub fn run(
    allowlist: &BTreeMap<String, PathBuf>,
    args: serde_json::Value,
    timeout: Duration,
    tempdir: PathBuf,
    rlimits: Rlimits,
    memory_max: Option<u64>,
    pids_max: Option<u64>,
) -> Result<ExecRun, CoreError> {
    let args: Args = serde_json::from_value(args)
        .map_err(|e| crate::config_invalid(format!("shell args: {e}")))?;
    let Some(cmd) = args.argv.first() else {
        return Err(crate::config_invalid(
            "shell argv must be non-empty".to_owned(),
        ));
    };
    let Some(exact_path) = allowlist.get(cmd) else {
        return Err(crate::config_invalid(format!(
            "command not in allowlist: {cmd}"
        )));
    };
    let mut resolved = args.argv.clone();
    resolved[0] = exact_path.to_string_lossy().into_owned();
    let req = ExecRequest {
        exact_path: exact_path.clone(),
        argv: resolved,
        timeout,
        tempdir,
        rlimits,
        memory_max,
        pids_max,
    };
    let out = launcher::launch(req)?;
    Ok(ExecRun::from(out))
}
