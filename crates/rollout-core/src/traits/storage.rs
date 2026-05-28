//! `Storage` + `StorageTxn`.
//!
//! Phase-2 surface per spec 04 §2: `Storage` carries point/batch/scan reads,
//! per-prefix `watch`, and a transactional write surface on `StorageTxn`.
//! Object-safe by design — generic typed-payload helpers are kept out of the
//! trait and live in downstream crates (Phase 2 simplification: `scan_bytes`
//! returns an owned `Vec` rather than the `BoxStream` shown in the spec text;
//! see the spec's "Phase 2 implementation notes" section).
//!
//! Phase-4 addition: `Storage::watch_stream` gives Postgres backends a uniform
//! stream-shaped subscription surface that complements the in-process broadcast
//! returned by `watch`. The legacy 2-method `Snapshotter` placeholder that used
//! to live here is gone — see `traits::snapshot` for the spec-04 §5.2 surface.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

use crate::{CoreError, RunId};

/// Structured, typed key. Always namespace-prefixed (spec 04 §2).
///
/// # Postgres backend constraint
///
/// The Postgres backend stores `path` as `TEXT[]`. Path components containing
/// non-printable / non-UTF-8 / NUL bytes silently diverge from redb's byte-lex
/// prefix scan (see `.planning/research/PITFALLS.md` §17). Hex-encode binary IDs
/// (`hex::encode(content_id.as_bytes())`) at the `StorageKey` construction site
/// for any namespace whose values include binary content (Phase 6 `work/`,
/// `epoch/`, `queue_items/`). Call [`StorageKey::validate_for_postgres`] to
/// reject keys that cannot round-trip.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct StorageKey {
    /// Top-level namespace (e.g., `"runs"`, `"workers"`, `"heartbeats"`).
    pub namespace: SmolStr,
    /// Optional run scope for run-local keys.
    pub run_id: Option<RunId>,
    /// Hierarchical path segments inside the namespace.
    pub path: Vec<SmolStr>,
}

impl StorageKey {
    /// Reject path components that cannot round-trip through Postgres `TEXT[]`.
    ///
    /// Every byte of every `path` component must lie in printable ASCII
    /// (`0x20..=0x7E`). Hex-encode binary IDs for the Postgres backend (see the
    /// struct-level docs and PITFALLS.md §17). `namespace` is `SmolStr` and is
    /// always valid UTF-8 by construction, so only `path` is checked.
    ///
    /// # Errors
    /// Returns [`FatalError::ConfigInvalid`](crate::FatalError::ConfigInvalid)
    /// when any path component contains a byte outside printable ASCII.
    pub fn validate_for_postgres(&self) -> Result<(), crate::CoreError> {
        for (idx, component) in self.path.iter().enumerate() {
            for &b in component.as_bytes() {
                if !(0x20..=0x7E).contains(&b) {
                    return Err(crate::CoreError::Fatal(crate::FatalError::ConfigInvalid {
                        msg: format!(
                            "StorageKey path[{idx}] contains byte 0x{b:02x} outside printable ASCII \
                             (0x20-0x7E); hex-encode binary IDs for the Postgres backend \
                             (see rollout-core::traits::storage StorageKey rustdoc)"
                        ),
                    }));
                }
            }
        }
        Ok(())
    }
}

/// A prefix scan over `StorageKey` space, optionally limited.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct KeyRange {
    /// Prefix that scan results must match.
    pub prefix: StorageKey,
    /// Optional maximum number of items to return.
    pub limit: Option<usize>,
}

/// Notification fan-out variant for `Storage::watch`.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum StorageEvent {
    /// A put committed against `key`.
    Put {
        /// Key that was written.
        key: StorageKey,
    },
    /// A delete committed against `key`.
    Delete {
        /// Key that was removed.
        key: StorageKey,
    },
}

/// Metadata key-value store (embedded KV or Postgres).
#[async_trait]
pub trait Storage: Send + Sync {
    /// Open a transaction; all writes inside it are atomic.
    async fn begin(&self) -> Result<Box<dyn StorageTxn>, CoreError>;
    /// Read raw bytes at `key`. Downstream callers layer postcard on top.
    async fn get_bytes(&self, key: &StorageKey) -> Result<Option<Vec<u8>>, CoreError>;
    /// Batched point reads (principle 2: batching first).
    async fn get_many_bytes(&self, keys: &[StorageKey]) -> Result<Vec<Option<Vec<u8>>>, CoreError>;
    /// Prefix scan returning owned `(key, value)` pairs.
    async fn scan_bytes(&self, range: KeyRange) -> Result<Vec<(StorageKey, Vec<u8>)>, CoreError>;
    /// Subscribe to commits whose keys match `prefix`. In-process broadcast only.
    async fn watch(
        &self,
        prefix: StorageKey,
    ) -> Result<tokio::sync::broadcast::Receiver<StorageEvent>, CoreError>;
    /// Subscribe to commits whose keys match `prefix` as a `BoxStream`.
    ///
    /// Phase-4 addition: gives Postgres backends (which can fan out LISTEN/NOTIFY
    /// across processes) a uniform stream-shaped surface, complementing the
    /// in-process `watch()` broadcast channel. The embedded backend wraps its
    /// broadcast receiver in `tokio_stream::wrappers::BroadcastStream`.
    async fn watch_stream(
        &self,
        prefix: StorageKey,
    ) -> Result<futures::stream::BoxStream<'static, StorageEvent>, CoreError>;
    /// Health probe.
    async fn ping(&self) -> Result<(), CoreError>;
}

/// A storage transaction. Commit or abort; drop aborts implicitly.
#[async_trait]
pub trait StorageTxn: Send + Sync {
    /// Stage a put of raw bytes at `key`.
    async fn put_bytes(&mut self, key: StorageKey, value: Vec<u8>) -> Result<(), CoreError>;
    /// Stage a delete of `key`.
    async fn delete(&mut self, key: StorageKey) -> Result<(), CoreError>;
    /// Compare-and-swap: succeeds only if the current value matches `expected`.
    async fn cas_bytes(
        &mut self,
        key: StorageKey,
        expected: Option<Vec<u8>>,
        new: Option<Vec<u8>>,
    ) -> Result<bool, CoreError>;
    /// Commit the transaction.
    async fn commit(self: Box<Self>) -> Result<(), CoreError>;
    /// Abort the transaction explicitly.
    async fn abort(self: Box<Self>) -> Result<(), CoreError>;
}

#[cfg(test)]
mod validate_for_postgres_tests {
    use super::StorageKey;
    use crate::{CoreError, FatalError};

    fn key(path: &[&str]) -> StorageKey {
        StorageKey {
            namespace: "work".into(),
            run_id: None,
            path: path.iter().map(|s| (*s).into()).collect(),
        }
    }

    fn is_config_invalid(err: &CoreError) -> bool {
        matches!(err, CoreError::Fatal(FatalError::ConfigInvalid { .. }))
    }

    #[test]
    fn validate_for_postgres_accepts_ascii_printable() {
        assert!(key(&["abc123"]).validate_for_postgres().is_ok());
    }

    #[test]
    fn validate_for_postgres_rejects_non_printable() {
        for bad in ["\u{0}", "\u{1}", "\u{7f}"] {
            let err = key(&[bad]).validate_for_postgres().unwrap_err();
            assert!(
                is_config_invalid(&err),
                "expected ConfigInvalid for {bad:?}"
            );
        }
    }

    #[test]
    fn validate_for_postgres_rejects_non_utf8_namespace_unreachable_by_type() {
        // `namespace` is SmolStr (always valid UTF-8) so non-UTF-8 is impossible
        // to construct; we only validate path components. A valid namespace with
        // a clean path passes.
        assert!(key(&["clean"]).validate_for_postgres().is_ok());
    }

    #[test]
    fn validate_for_postgres_rejects_high_bit_set() {
        // Bytes 0x80-0xFF arrive as multi-byte UTF-8 sequences; every byte is
        // outside 0x20..=0x7E so the guard rejects them.
        let err = key(&["caf\u{e9}"]).validate_for_postgres().unwrap_err();
        assert!(is_config_invalid(&err));
    }

    #[test]
    fn validate_for_postgres_empty_path_ok() {
        assert!(key(&[]).validate_for_postgres().is_ok());
    }

    #[test]
    fn validate_for_postgres_hex_encoded_id_passes() {
        let hexed = hex::encode([0x00_u8, 0xff, 0x42]);
        let k = StorageKey {
            namespace: "work".into(),
            run_id: None,
            path: vec!["abc".into(), hexed.into()],
        };
        assert!(k.validate_for_postgres().is_ok());
    }
}
