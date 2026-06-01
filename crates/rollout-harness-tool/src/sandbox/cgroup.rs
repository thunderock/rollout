//! cgroups v2 `memory.max` / `pids.max` plumbing (D-TOOL-04).
//!
//! Degrade-with-warning, NOT fail-closed (RESEARCH Open Question #4, Pitfall B):
//! CI runners rarely provide a delegated v2 tree, and cgroup memory limits have a
//! working `RLIMIT_AS` fallback (applied in the launcher). This deliberately
//! diverges from landlock's fail-closed posture — landlock has no rlimit
//! substitute, cgroup memory does. If no writable delegated tree is found we emit
//! a warning and rely on rlimits.

use std::fs;
use std::path::{Path, PathBuf};

/// A per-invocation cgroup v2 subtree, or `None` when no delegated tree exists.
#[derive(Debug)]
pub struct CgroupSubtree {
    dir: PathBuf,
}

impl CgroupSubtree {
    /// Best-effort: create a per-invocation subtree under a writable delegated
    /// v2 tree and write `memory.max` + `pids.max`. Returns `Ok(None)` (the
    /// degrade path) when no delegated tree is writable — the caller relies on
    /// rlimits and should emit a warning event.
    ///
    /// # Errors
    /// Returns the underlying `io::Error` only when a delegated tree IS present
    /// but writing the controller files fails (a real misconfiguration).
    pub fn create(
        name: &str,
        memory_max: Option<u64>,
        pids_max: Option<u64>,
    ) -> std::io::Result<Option<Self>> {
        let Some(parent) = delegated_tree() else {
            return Ok(None);
        };
        let dir = parent.join(format!("rollout-tool-{name}"));
        fs::create_dir_all(&dir)?;
        if let Some(m) = memory_max {
            fs::write(dir.join("memory.max"), m.to_string())?;
        }
        if let Some(p) = pids_max {
            fs::write(dir.join("pids.max"), p.to_string())?;
        }
        Ok(Some(Self { dir }))
    }

    /// Move `pid` into this subtree by writing `cgroup.procs`.
    ///
    /// # Errors
    /// Returns the underlying `io::Error` if the write fails.
    pub fn join_pid(&self, pid: i32) -> std::io::Result<()> {
        fs::write(self.dir.join("cgroup.procs"), pid.to_string())
    }
}

impl Drop for CgroupSubtree {
    fn drop(&mut self) {
        // Best-effort cleanup; the subtree only removes once empty.
        let _ = fs::remove_dir(&self.dir);
    }
}

/// Find a writable delegated cgroup v2 tree: `cgroup.subtree_control` writable
/// under the unified mount. systemd user delegation (`Delegate=yes`) and
/// container runtimes provide this; bare CI runners usually do not.
fn delegated_tree() -> Option<PathBuf> {
    let root = Path::new("/sys/fs/cgroup");
    let sc = root.join("cgroup.subtree_control");
    // Writable subtree_control => we can create child cgroups here.
    if is_writable(&sc) {
        return Some(root.to_path_buf());
    }
    None
}

fn is_writable(p: &Path) -> bool {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_CLOEXEC)
        .open(p)
        .is_ok()
}
