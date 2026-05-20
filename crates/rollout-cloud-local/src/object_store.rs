//! Content-addressed sharded filesystem `ObjectStore` (D-LOCAL-01).
//!
//! Layout: `<root>/<hex[0..2]>/<hex[2..4]>/<hex>` for the blob, plus a sibling
//! `<hex>.meta.json` carrying size + content-type + created-at.

use async_trait::async_trait;
use rollout_core::{ContentId, CoreError, FatalError, ObjectStore, PutHint};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
struct ObjectMeta {
    size: u64,
    content_type: Option<String>,
    created_at_ms: u128,
}

/// Local-filesystem `ObjectStore` with two-level sharded content-addressed layout.
pub struct FsObjectStore {
    root: PathBuf,
}

impl FsObjectStore {
    /// Open or create the object-store root at `root`.
    ///
    /// # Errors
    /// Returns `Fatal(Internal)` if the root directory cannot be created.
    pub async fn open(root: impl AsRef<Path>) -> Result<Self, CoreError> {
        let root = root.as_ref().to_path_buf();
        tokio::fs::create_dir_all(&root).await.map_err(internal)?;
        Ok(Self { root })
    }

    fn path_for(&self, id: &ContentId) -> PathBuf {
        let hex = id.to_string();
        self.root.join(&hex[0..2]).join(&hex[2..4]).join(&hex)
    }

    fn meta_path_for(&self, id: &ContentId) -> PathBuf {
        let hex = id.to_string();
        self.root
            .join(&hex[0..2])
            .join(&hex[2..4])
            .join(format!("{hex}.meta.json"))
    }
}

#[async_trait]
impl ObjectStore for FsObjectStore {
    async fn put_bytes(&self, bytes: Vec<u8>, hint: PutHint) -> Result<ContentId, CoreError> {
        let id = ContentId::of(&bytes);
        let final_path = self.path_for(&id);
        if let Some(parent) = final_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(internal)?;
        }
        // Idempotent: skip the blob write when the file already exists.
        if !tokio::fs::try_exists(&final_path).await.map_err(internal)? {
            let tmp = final_path.with_extension("tmp");
            tokio::fs::write(&tmp, &bytes).await.map_err(internal)?;
            tokio::fs::rename(&tmp, &final_path)
                .await
                .map_err(internal)?;
        }
        let meta = ObjectMeta {
            size: bytes.len() as u64,
            content_type: hint.content_type,
            created_at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
        };
        let meta_path = self.meta_path_for(&id);
        let meta_bytes = serde_json::to_vec(&meta).map_err(internal)?;
        tokio::fs::write(&meta_path, meta_bytes)
            .await
            .map_err(internal)?;
        Ok(id)
    }

    async fn get_bytes(&self, id: &ContentId) -> Result<Vec<u8>, CoreError> {
        let p = self.path_for(id);
        match tokio::fs::read(&p).await {
            Ok(bytes) => Ok(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(CoreError::Fatal(FatalError::Internal {
                    msg: format!("object not found: {id}"),
                }))
            }
            Err(e) => Err(internal(e)),
        }
    }

    async fn exists(&self, id: &ContentId) -> Result<bool, CoreError> {
        tokio::fs::try_exists(self.path_for(id))
            .await
            .map_err(internal)
    }
}

fn internal<E: std::fmt::Display>(e: E) -> CoreError {
    CoreError::Fatal(FatalError::Internal { msg: e.to_string() })
}
