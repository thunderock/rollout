//! Deterministic tar build for snapshot blobs (TRAIN-03 byte-stability).
//!
//! Implements 04-RESEARCH Pitfall 9: `tar::HeaderMode::Deterministic` does NOT
//! zero file mode bits — set 0o644 (file) / 0o755 (dir) explicitly along with
//! mtime=0, uid=0, gid=0. Sort entries by path so the byte stream is
//! reproducible across runs and platforms.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use rollout_core::{CoreError, FatalError, RecoverableError, RetryHint};

/// Build a byte-identical tar archive of `src_dir`. Suitable for
/// content-addressing via blake3.
///
/// Invariants:
/// - Entry order: sorted by relative path.
/// - No compression.
/// - Per-entry headers: mtime=0, uid=0, gid=0, mode=0o644 (file) / 0o755 (dir).
/// - GNU header format.
///
/// # Errors
/// Returns `Recoverable(Transient)` on filesystem I/O errors, `Fatal(Internal)`
/// on invariant breaches (e.g. `strip_prefix` failure).
pub fn build_deterministic_tar(src_dir: &Path) -> Result<Vec<u8>, CoreError> {
    let mut entries: Vec<PathBuf> = walkdir::WalkDir::new(src_dir)
        .into_iter()
        .filter_map(Result::ok)
        .map(walkdir::DirEntry::into_path)
        .filter(|p| p != src_dir)
        .collect();
    entries.sort();

    let mut buf = Vec::new();
    {
        let mut tar_builder = tar::Builder::new(&mut buf);
        tar_builder.mode(tar::HeaderMode::Deterministic);

        for path in entries {
            let rel = path
                .strip_prefix(src_dir)
                .map_err(|e| fatal_internal(&format!("strip_prefix: {e}")))?;
            let meta = std::fs::metadata(&path).map_err(|e| io_err(&e))?;
            let is_dir = meta.is_dir();

            let mut header = tar::Header::new_gnu();
            header.set_size(if is_dir { 0 } else { meta.len() });
            header.set_mode(if is_dir { 0o755 } else { 0o644 });
            header.set_mtime(0);
            header.set_uid(0);
            header.set_gid(0);
            header.set_entry_type(if is_dir {
                tar::EntryType::Directory
            } else {
                tar::EntryType::Regular
            });
            header.set_cksum();

            if is_dir {
                tar_builder
                    .append_data(&mut header, rel, std::io::empty())
                    .map_err(|e| io_err(&e))?;
            } else {
                let mut file = File::open(&path).map_err(|e| io_err(&e))?;
                let mut contents = Vec::with_capacity(usize::try_from(meta.len()).unwrap_or(0));
                file.read_to_end(&mut contents).map_err(|e| io_err(&e))?;
                tar_builder
                    .append_data(&mut header, rel, &contents[..])
                    .map_err(|e| io_err(&e))?;
            }
        }

        tar_builder.finish().map_err(|e| io_err(&e))?;
    }
    Ok(buf)
}

/// Extract a tar archive built by `build_deterministic_tar` into `dst_dir`.
///
/// # Errors
/// Returns `Recoverable(Transient)` on filesystem I/O errors.
pub fn extract_tar(tar_bytes: &[u8], dst_dir: &Path) -> Result<(), CoreError> {
    std::fs::create_dir_all(dst_dir).map_err(|e| io_err(&e))?;
    let mut archive = tar::Archive::new(tar_bytes);
    archive.unpack(dst_dir).map_err(|e| io_err(&e))?;
    Ok(())
}

fn io_err(e: &std::io::Error) -> CoreError {
    CoreError::Recoverable(RecoverableError::Transient {
        msg: e.to_string(),
        hint: RetryHint::Never,
    })
}

fn fatal_internal(msg: &str) -> CoreError {
    CoreError::Fatal(FatalError::Internal {
        msg: msg.to_string(),
    })
}
