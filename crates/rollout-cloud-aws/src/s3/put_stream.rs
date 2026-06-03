//! Streaming multipart `put_stream` with incremental blake3 hashing
//! (D-SNAP-04/05 + PITFALLS.md §4 + §16).
//!
//! - Chunks are hashed BEFORE the SDK `upload_part` call, so SDK retries replay
//!   the identical `Bytes` buffer and the final `ContentId` is stable across
//!   throttle-driven retries (Pitfall 16).
//! - A [`MultipartGuard`] aborts the in-flight multipart on any drop path other
//!   than `.commit()` (Pitfall 4). After tokio shutdown it logs and leaks,
//!   relying on the bucket `AbortIncompleteMultipartUpload` lifecycle policy.

use std::pin::Pin;
use std::sync::Arc;

use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
use aws_sdk_s3::Client;
use blake3::Hasher;
use bytes::Bytes;
use tokio::io::{AsyncRead, AsyncReadExt};
use tracing::warn;

use rollout_core::{ContentId, CoreError, PutHint};

use crate::error::{fatal_internal, map_s3_sdk_error, recoverable_transient, render_sdk};

/// Guard that aborts an in-flight multipart upload unless `.commit()` ran.
pub(crate) struct MultipartGuard {
    client: Arc<Client>,
    bucket: String,
    key: String,
    upload_id: String,
    committed: bool,
}

impl MultipartGuard {
    pub(crate) fn new(client: Arc<Client>, bucket: String, key: String, upload_id: String) -> Self {
        Self {
            client,
            bucket,
            key,
            upload_id,
            committed: false,
        }
    }

    /// Complete the multipart upload, defusing the Drop abort.
    pub(crate) async fn commit(mut self, parts: Vec<CompletedPart>) -> Result<(), CoreError> {
        self.client
            .complete_multipart_upload()
            .bucket(&self.bucket)
            .key(&self.key)
            .upload_id(&self.upload_id)
            .multipart_upload(
                CompletedMultipartUpload::builder()
                    .set_parts(Some(parts))
                    .build(),
            )
            .send()
            .await
            .map_err(|e| map_s3_sdk_error(render_sdk(&e)))?;
        self.committed = true;
        Ok(())
    }
}

impl Drop for MultipartGuard {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        let client = Arc::clone(&self.client);
        let bucket = std::mem::take(&mut self.bucket);
        let key = std::mem::take(&mut self.key);
        let upload_id = std::mem::take(&mut self.upload_id);

        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                handle.spawn(async move {
                    if let Err(e) = client
                        .abort_multipart_upload()
                        .bucket(&bucket)
                        .key(&key)
                        .upload_id(&upload_id)
                        .send()
                        .await
                    {
                        warn!(error = ?e, %bucket, %key, "MultipartGuard abort failed; bucket lifecycle policy will clean up");
                    }
                });
            }
            Err(_) => {
                warn!(%bucket, %key, "MultipartGuard dropped after tokio runtime shutdown; orphan multipart leaked, relying on bucket lifecycle");
            }
        }
    }
}

/// Streaming multipart upload to a temp key, then server-side copy to the
/// content-addressed final key. Returns the blake3 `ContentId`.
pub(crate) async fn put_stream_impl(
    store: &super::S3ObjectStore,
    mut stream: Pin<Box<dyn AsyncRead + Send>>,
    hint: PutHint,
) -> Result<ContentId, CoreError> {
    let chunk_size = store.multipart_chunk_bytes.max(5 * 1024 * 1024);
    let temp_key = format!("{}temp/pending-{}", store.prefix, ulid::Ulid::new());

    let create = store
        .client
        .create_multipart_upload()
        .bucket(&store.bucket)
        .key(&temp_key)
        .content_type(
            hint.content_type
                .as_deref()
                .unwrap_or("application/octet-stream"),
        )
        .send()
        .await
        .map_err(|e| map_s3_sdk_error(render_sdk(&e)))?;
    let upload_id = create
        .upload_id()
        .ok_or_else(|| fatal_internal("CreateMultipartUpload missing upload_id"))?
        .to_owned();

    let guard = MultipartGuard::new(
        Arc::clone(&store.client),
        store.bucket.clone(),
        temp_key.clone(),
        upload_id.clone(),
    );

    let mut hasher = Hasher::new();
    let mut parts: Vec<CompletedPart> = Vec::new();
    let mut part_number: i32 = 1;
    let mut chunk_buf = vec![0u8; chunk_size];
    let mut filled: usize = 0;

    loop {
        // Fill a full chunk before sending: AsyncRead may return short reads.
        let n = stream
            .read(&mut chunk_buf[filled..])
            .await
            .map_err(|e| recoverable_transient(format!("stream read: {e}")))?;
        if n == 0 {
            // EOF — flush any partial trailing chunk (also handles the single-chunk case).
            if filled > 0 || part_number == 1 {
                upload_one_part(
                    store,
                    &temp_key,
                    &upload_id,
                    part_number,
                    &chunk_buf[..filled],
                    &mut hasher,
                    &mut parts,
                )
                .await?;
            }
            break;
        }
        filled += n;
        if filled == chunk_size {
            upload_one_part(
                store,
                &temp_key,
                &upload_id,
                part_number,
                &chunk_buf[..filled],
                &mut hasher,
                &mut parts,
            )
            .await?;
            part_number += 1;
            filled = 0;
        }
    }

    let content_id = ContentId(*hasher.finalize().as_bytes());
    guard.commit(parts).await?;

    // S3 has no rename: copy temp -> final content-addressed key, then delete temp.
    let final_key = store.key_for(&content_id);
    store
        .client
        .copy_object()
        .bucket(&store.bucket)
        .key(&final_key)
        .copy_source(format!("{}/{}", store.bucket, temp_key))
        .send()
        .await
        .map_err(|e| map_s3_sdk_error(render_sdk(&e)))?;
    store
        .client
        .delete_object()
        .bucket(&store.bucket)
        .key(&temp_key)
        .send()
        .await
        .map_err(|e| map_s3_sdk_error(render_sdk(&e)))?;

    Ok(content_id)
}

/// Hash a chunk (before the SDK call) and upload it as one part.
async fn upload_one_part(
    store: &super::S3ObjectStore,
    temp_key: &str,
    upload_id: &str,
    part_number: i32,
    chunk: &[u8],
    hasher: &mut Hasher,
    parts: &mut Vec<CompletedPart>,
) -> Result<(), CoreError> {
    hasher.update(chunk);
    let bytes = Bytes::copy_from_slice(chunk);
    let resp = store
        .client
        .upload_part()
        .bucket(&store.bucket)
        .key(temp_key)
        .upload_id(upload_id)
        .part_number(part_number)
        .body(ByteStream::from(bytes))
        .send()
        .await
        .map_err(|e| map_s3_sdk_error(render_sdk(&e)))?;
    parts.push(
        CompletedPart::builder()
            .e_tag(resp.e_tag().unwrap_or_default())
            .part_number(part_number)
            .build(),
    );
    Ok(())
}
