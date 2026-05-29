//! Content-addressed sharded filesystem `ObjectStore` (D-LOCAL-01).
//!
//! Layout: `<root>/<hex[0..2]>/<hex[2..4]>/<hex>` for the blob, plus a sibling
//! `<hex>.meta.json` carrying size + content-type + created-at.

use async_trait::async_trait;
use blake3::Hasher;
use rollout_core::{
    ContentId, CoreError, FatalError, ObjectStore, PutHint, RecoverableError, RetryHint,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

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

    /// Per-write staging path under `<root>/pending/<ulid>` for atomic streaming puts.
    fn temp_path_for_pending(&self) -> PathBuf {
        self.root
            .join("pending")
            .join(ulid::Ulid::new().to_string())
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

    async fn put_stream(
        &self,
        mut stream: Pin<Box<dyn AsyncRead + Send>>,
        _hint: PutHint,
    ) -> Result<ContentId, CoreError> {
        // Stream to a temp file, hashing incrementally; atomic-rename to the
        // content-addressed path on success. Never buffers the whole payload.
        let temp = self.temp_path_for_pending();
        if let Some(parent) = temp.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| transient(&e))?;
        }
        let mut file = tokio::fs::File::create(&temp)
            .await
            .map_err(|e| transient(&e))?;
        let mut hasher = Hasher::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = stream.read(&mut buf).await.map_err(|e| transient(&e))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            file.write_all(&buf[..n]).await.map_err(|e| transient(&e))?;
        }
        file.flush().await.map_err(|e| transient(&e))?;
        file.sync_all().await.map_err(|e| transient(&e))?;
        drop(file);

        let id = ContentId(*hasher.finalize().as_bytes());
        let final_path = self.path_for(&id);
        if let Some(parent) = final_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| transient(&e))?;
        }
        // Idempotent put: if the blob already exists, discard the temp.
        if tokio::fs::try_exists(&final_path)
            .await
            .map_err(|e| transient(&e))?
        {
            tokio::fs::remove_file(&temp).await.ok();
        } else {
            tokio::fs::rename(&temp, &final_path)
                .await
                .map_err(|e| transient(&e))?;
        }
        Ok(id)
    }

    async fn get_stream(
        &self,
        id: &ContentId,
    ) -> Result<Pin<Box<dyn AsyncRead + Send>>, CoreError> {
        let path = self.path_for(id);
        let file = tokio::fs::File::open(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                CoreError::Fatal(FatalError::Internal {
                    msg: format!("object not found: {id}"),
                })
            } else {
                transient(&e)
            }
        })?;
        Ok(Box::pin(file))
    }
}

fn internal<E: std::fmt::Display>(e: E) -> CoreError {
    CoreError::Fatal(FatalError::Internal { msg: e.to_string() })
}

fn transient(e: &std::io::Error) -> CoreError {
    CoreError::Recoverable(RecoverableError::Transient {
        msg: format!("fs object_store io: {e}"),
        hint: RetryHint::After(std::time::Duration::from_millis(50)),
    })
}
