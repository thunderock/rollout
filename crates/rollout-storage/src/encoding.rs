//! postcard helpers + `StorageKey` byte encoding.
//!
//! Namespace lives in the redb table choice (see `embedded::tables`), so the
//! key bytes only encode `(run_id, path)` via a single postcard-encoded tuple.

use rollout_core::{CoreError, FatalError, StorageKey};

/// Encode `StorageKey` to bytes for use as a redb key (namespace excluded —
/// it picks the table). Uses a single `postcard::to_allocvec` over a tuple so
/// decoding is unambiguous regardless of inner `0x00` bytes.
///
/// # Panics
/// Panics if postcard's in-memory serializer fails — only possible on
/// allocation failure, which we treat as fatal.
#[must_use]
pub fn encode_key(key: &StorageKey) -> Vec<u8> {
    postcard::to_allocvec(&(&key.run_id, &key.path)).expect("infallible: in-memory")
}

/// Decode redb key bytes back to `(run_id, path)`.
///
/// # Errors
/// Returns `Fatal(Internal)` if `bytes` is not a postcard-encoded
/// `(Option<RunId>, Vec<SmolStr>)` tuple.
pub fn decode_key_payload(
    bytes: &[u8],
) -> Result<(Option<rollout_core::RunId>, Vec<smol_str::SmolStr>), CoreError> {
    postcard::from_bytes(bytes).map_err(|e| internal(format!("postcard key: {e}")))
}

/// Whether `candidate`'s `(namespace, run_id, path)` is a prefix-extension of
/// `prefix`. Namespace must match exactly; `prefix.run_id = None` acts as a
/// wildcard (any `candidate.run_id` matches), otherwise `run_ids` must be equal;
/// path must start with `prefix.path`.
#[must_use]
pub fn key_has_prefix(candidate: &StorageKey, prefix: &StorageKey) -> bool {
    if candidate.namespace != prefix.namespace {
        return false;
    }
    let run_id_ok = match &prefix.run_id {
        None => true,
        Some(_) => candidate.run_id == prefix.run_id,
    };
    run_id_ok && candidate.path.starts_with(&prefix.path[..])
}

fn internal<S: Into<String>>(s: S) -> CoreError {
    CoreError::Fatal(FatalError::Internal { msg: s.into() })
}
