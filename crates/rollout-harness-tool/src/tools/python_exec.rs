//! `python_exec` (`SideEffectClass::Exec`): run `python3 -c <code>` under the
//! full sandbox via the shared launcher. shell=False — argv vector, exact-path
//! binary (D-TOOL-06). No in-process `PyO3`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use rollout_core::CoreError;
use serde::Deserialize;

use crate::sandbox::launcher::{self, ExecRequest, Rlimits};
use crate::ExecRun;

/// `python_exec` args: the source to run with `python3 -c`.
#[derive(Debug, Deserialize)]
pub struct Args {
    /// Python source executed via `-c` (never a shell string).
    pub code: String,
}

/// Run `python3 -c <code>` sandboxed.
///
/// # Errors
/// Returns [`CoreError`] if args fail to parse or the launcher errors.
pub fn run(
    python_path: &Path,
    args: serde_json::Value,
    timeout: Duration,
    tempdir: PathBuf,
    rlimits: Rlimits,
    memory_max: Option<u64>,
    pids_max: Option<u64>,
) -> Result<ExecRun, CoreError> {
    let args: Args = serde_json::from_value(args)
        .map_err(|e| crate::config_invalid(format!("python_exec args: {e}")))?;
    let req = ExecRequest {
        exact_path: python_path.to_path_buf(),
        argv: vec![
            python_path.to_string_lossy().into_owned(),
            "-I".to_owned(), // isolated mode: ignore env + user site
            "-c".to_owned(),
            args.code,
        ],
        timeout,
        tempdir,
        rlimits,
        memory_max,
        pids_max,
    };
    let out = launcher::launch(req)?;
    Ok(ExecRun::from(out))
}
