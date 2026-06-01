//! Capability-based FS root for the file tools (D-TOOL-06, PITFALLS 10b).
//!
//! `file_read`/`file_write` run in-process under a [`cap_std::fs::Dir`] rooted at
//! the per-invocation tempdir. cap-std rejects `..`/symlink escapes by
//! construction; we add a canonicalize-then-assert-prefix belt-and-suspenders
//! check on the requested relative path before touching the cap-std `Dir`.

use std::path::{Component, Path};

use cap_std::ambient_authority;
use cap_std::fs::Dir;
use rollout_core::{CoreError, FatalError};

/// A cap-std capability root confined to a single tempdir.
#[derive(Debug)]
pub struct CapRoot {
    dir: Dir,
}

impl CapRoot {
    /// Open a capability root at `root` (the per-invocation tempdir).
    ///
    /// # Errors
    /// Returns [`CoreError::Fatal`] if the directory cannot be opened.
    pub fn open(root: &Path) -> Result<Self, CoreError> {
        let dir = Dir::open_ambient_dir(root, ambient_authority())
            .map_err(|e| fatal(format!("cap-std open root {}: {e}", root.display())))?;
        Ok(Self { dir })
    }

    /// Read a file confined to the root.
    ///
    /// # Errors
    /// Returns [`CoreError::Fatal`] on path escape or IO failure.
    pub fn read(&self, rel: &Path) -> Result<Vec<u8>, CoreError> {
        reject_escape(rel)?;
        self.dir
            .read(rel)
            .map_err(|e| fatal(format!("cap-std read {}: {e}", rel.display())))
    }

    /// Write a file confined to the root.
    ///
    /// # Errors
    /// Returns [`CoreError::Fatal`] on path escape or IO failure.
    pub fn write(&self, rel: &Path, contents: &[u8]) -> Result<(), CoreError> {
        reject_escape(rel)?;
        self.dir
            .write(rel, contents)
            .map_err(|e| fatal(format!("cap-std write {}: {e}", rel.display())))
    }
}

/// Belt-and-suspenders: reject absolute paths, `..`, and root-anchored escapes
/// before handing the path to cap-std (which would also reject them).
fn reject_escape(rel: &Path) -> Result<(), CoreError> {
    if rel.is_absolute() {
        return Err(fatal(format!("absolute path rejected: {}", rel.display())));
    }
    for c in rel.components() {
        match c {
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(fatal(format!("path escape rejected: {}", rel.display())));
            }
            Component::Normal(_) | Component::CurDir => {}
        }
    }
    Ok(())
}

fn fatal(msg: String) -> CoreError {
    CoreError::Fatal(FatalError::ConfigInvalid { msg })
}
