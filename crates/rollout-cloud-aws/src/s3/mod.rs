//! `S3ObjectStore` — AWS S3 impl of `ObjectStore` (RESEARCH.md Pattern 2).
//!
//! Content-addressed sharded key layout `<prefix>cas/<ab>/<cd>/<hex>` mirrors
//! `FsObjectStore` (Phase 2). `put_stream` uses multipart upload with
//! incremental blake3 hashing + a Drop-spawn-abort `MultipartGuard` (see
//! [`put_stream`]).

pub(crate) mod get_stream;
pub(crate) mod put_stream;

use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;
use tokio::io::AsyncRead;

use rollout_core::{ContentId, CoreError, ObjectStore, PutHint};

use crate::error::{fatal_internal, map_s3_sdk_error, render_sdk};

/// S3-backed content-addressed object store.
pub struct S3ObjectStore {
    pub(crate) client: Arc<Client>,
    pub(crate) bucket: String,
    pub(crate) prefix: String,
    pub(crate) multipart_chunk_bytes: usize,
}

impl S3ObjectStore {
    /// Construct an `S3ObjectStore`.
    ///
    /// `prefix` is an empty string for no prefix. `multipart_chunk_bytes` is the
    /// per-part size for `put_stream` (S3 minimum is 5 MiB).
    #[must_use]
    pub fn new(
        client: Arc<Client>,
        bucket: String,
        prefix: String,
        multipart_chunk_bytes: usize,
    ) -> Self {
        Self {
            client,
            bucket,
            prefix,
            multipart_chunk_bytes,
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

#[async_trait]
impl ObjectStore for S3ObjectStore {
    async fn put_bytes(&self, bytes: Vec<u8>, _hint: PutHint) -> Result<ContentId, CoreError> {
        let id = ContentId::of(&bytes);
        let key = self.key_for(&id);
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(ByteStream::from(bytes))
            .send()
            .await
            .map_err(map_s3_sdk_error)?;
        Ok(id)
    }

    async fn get_bytes(&self, id: &ContentId) -> Result<Vec<u8>, CoreError> {
        let key = self.key_for(id);
        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|e| {
                let rendered = format!("{e}");
                if rendered.contains("NoSuchKey") || rendered.contains("404") {
                    fatal_internal(&format!("object not found: {id}"))
                } else {
                    map_s3_sdk_error(e)
                }
            })?;
        let data = resp
            .body
            .collect()
            .await
            .map_err(|e| fatal_internal(&format!("s3 body collect: {e}")))?;
        Ok(data.into_bytes().to_vec())
    }

    async fn exists(&self, id: &ContentId) -> Result<bool, CoreError> {
        let key = self.key_for(id);
        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(e) => {
                let rendered = render_sdk(&e);
                if rendered.contains("NotFound") || rendered.contains("404") {
                    Ok(false)
                } else {
                    Err(map_s3_sdk_error(rendered))
                }
            }
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
