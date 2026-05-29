//! `GcsObjectStore` — GCS impl of `ObjectStore` (RESEARCH.md Pattern 3).
//!
//! Content-addressed sharded key layout `<prefix>cas/<ab>/<cd>/<hex>` mirrors
//! `FsObjectStore` (Phase 2) and `S3ObjectStore` (Plan 05). `put_stream` uses a
//! GCS resumable upload session with incremental blake3 hashing, then a
//! server-side `copy_object` to the content-addressed final key (see
//! [`put_stream`]). No `upload_id` (resumable session URL) is persisted across
//! processes — a preempted worker re-uploads from byte 0 (Pitfall #5).

pub(crate) mod get_stream;
pub(crate) mod put_stream;

use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use gcloud_storage::client::Client;
use gcloud_storage::http::objects::download::Range;
use gcloud_storage::http::objects::get::GetObjectRequest;
use gcloud_storage::http::objects::upload::{Media, UploadObjectRequest, UploadType};
use gcloud_storage::http::Error as GcsError;
use tokio::io::AsyncRead;

use rollout_core::{ContentId, CoreError, ObjectStore, PutHint};

use crate::error::{fatal_internal, map_gcs_error};

/// GCS-backed content-addressed object store.
pub struct GcsObjectStore {
    pub(crate) client: Arc<Client>,
    pub(crate) bucket: String,
    pub(crate) prefix: String,
    pub(crate) resumable_chunk_bytes: usize,
}

impl GcsObjectStore {
    /// Construct a `GcsObjectStore`.
    ///
    /// `prefix` is an empty string for no prefix. `resumable_chunk_bytes` is the
    /// per-chunk size for `put_stream` resumable uploads (GCS requires chunks be
    /// a multiple of 256 KiB; callers pass a 5 MiB+ value).
    #[must_use]
    pub fn new(
        client: Arc<Client>,
        bucket: String,
        prefix: String,
        resumable_chunk_bytes: usize,
    ) -> Self {
        Self {
            client,
            bucket,
            prefix,
            resumable_chunk_bytes,
        }
    }

    /// The bucket this store writes to.
    #[must_use]
    pub fn bucket(&self) -> &str {
        &self.bucket
    }

    /// Sharded content-addressed key: `<prefix>cas/<ab>/<cd>/<hex>`.
    pub(crate) fn key_for(&self, id: &ContentId) -> String {
        sharded_key(&self.prefix, id)
    }
}

/// Sharded content-addressed key, free-standing so it is unit-testable without a client.
fn sharded_key(prefix: &str, id: &ContentId) -> String {
    let hex = id.to_string();
    format!("{prefix}cas/{}/{}/{}", &hex[0..2], &hex[2..4], hex)
}

/// True if a GCS error is a 404 (object / resource not found).
pub(crate) fn is_not_found(err: &GcsError) -> bool {
    match err {
        GcsError::Response(resp) => resp.code == 404,
        other => {
            let rendered = format!("{other}").to_ascii_lowercase();
            rendered.contains("404") || rendered.contains("not found")
        }
    }
}

#[async_trait]
impl ObjectStore for GcsObjectStore {
    async fn put_bytes(&self, bytes: Vec<u8>, hint: PutHint) -> Result<ContentId, CoreError> {
        let id = ContentId::of(&bytes);
        let key = self.key_for(&id);
        let mut media = Media::new(key);
        media.content_type = hint
            .content_type
            .unwrap_or_else(|| "application/octet-stream".to_owned())
            .into();
        media.content_length = Some(bytes.len() as u64);
        self.client
            .upload_object(
                &UploadObjectRequest {
                    bucket: self.bucket.clone(),
                    ..Default::default()
                },
                bytes,
                &UploadType::Simple(media),
            )
            .await
            .map_err(map_gcs_error)?;
        Ok(id)
    }

    async fn get_bytes(&self, id: &ContentId) -> Result<Vec<u8>, CoreError> {
        let key = self.key_for(id);
        self.client
            .download_object(
                &GetObjectRequest {
                    bucket: self.bucket.clone(),
                    object: key,
                    ..Default::default()
                },
                &Range::default(),
            )
            .await
            .map_err(|e| {
                if is_not_found(&e) {
                    fatal_internal(&format!("object not found: {id}"))
                } else {
                    map_gcs_error(e)
                }
            })
    }

    async fn exists(&self, id: &ContentId) -> Result<bool, CoreError> {
        let key = self.key_for(id);
        match self
            .client
            .get_object(&GetObjectRequest {
                bucket: self.bucket.clone(),
                object: key,
                ..Default::default()
            })
            .await
        {
            Ok(_) => Ok(true),
            Err(e) if is_not_found(&e) => Ok(false),
            Err(e) => Err(map_gcs_error(e)),
        }
    }

    async fn put_stream(
        &self,
        stream: Pin<Box<dyn AsyncRead + Send>>,
        hint: PutHint,
    ) -> Result<ContentId, CoreError> {
        put_stream::put_stream_impl(self, stream, hint).await
    }

    async fn get_stream(
        &self,
        id: &ContentId,
    ) -> Result<Pin<Box<dyn AsyncRead + Send>>, CoreError> {
        get_stream::get_stream_impl(self, id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sharded_key_layout_matches_fs_object_store() {
        let id = ContentId::of(b"hello");
        let hex = id.to_string();
        assert_eq!(
            sharded_key("snap/", &id),
            format!("snap/cas/{}/{}/{}", &hex[0..2], &hex[2..4], hex)
        );
        assert_eq!(
            sharded_key("", &id),
            format!("cas/{}/{}/{}", &hex[0..2], &hex[2..4], hex)
        );
    }
}
