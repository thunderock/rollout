//! Streaming resumable `put_stream` with incremental blake3 hashing
//! (D-SNAP-04/05 + PITFALLS.md §5 + §16).
//!
//! - Chunks are hashed BEFORE the SDK `upload_multiple_chunk` call, so the final
//!   `ContentId` is stable across retries (Pitfall 16).
//! - The resumable session URL is created, used, and discarded WITHIN this single
//!   call. We never persist it across processes (Pitfall 5): a preempted worker
//!   simply re-runs `put_stream` from byte 0, which is idempotent because the key
//!   is content-addressed. Orphaned sessions auto-expire after 7 days (GCS
//!   default; see `docs/bucket-setup.md`).
//! - We upload to a `temp/pending-<ulid>` key, compute the `ContentId`, then
//!   server-side `copy_object` to the content-addressed key and delete the temp.

use std::pin::Pin;

use blake3::Hasher;
use gcloud_storage::http::objects::copy::CopyObjectRequest;
use gcloud_storage::http::objects::delete::DeleteObjectRequest;
use gcloud_storage::http::objects::upload::{Media, UploadObjectRequest, UploadType};
use gcloud_storage::http::resumable_upload_client::{ChunkSize, UploadStatus};
use tokio::io::{AsyncRead, AsyncReadExt};

use rollout_core::{ContentId, CoreError, PutHint};

use crate::error::{fatal_internal, map_gcs_error, recoverable_transient};

/// GCS requires resumable chunks (other than the last) be a multiple of 256 KiB.
const RESUMABLE_MULTIPLE: usize = 256 * 1024;

pub(crate) async fn put_stream_impl(
    store: &super::GcsObjectStore,
    mut stream: Pin<Box<dyn AsyncRead + Send>>,
    hint: PutHint,
) -> Result<ContentId, CoreError> {
    // Round the chunk size down to a 256 KiB multiple, floored at 256 KiB.
    let chunk_size = (store.resumable_chunk_bytes / RESUMABLE_MULTIPLE).max(1) * RESUMABLE_MULTIPLE;
    let temp_key = format!("{}temp/pending-{}", store.prefix, ulid::Ulid::new());

    let mut media = Media::new(temp_key.clone());
    media.content_type = hint
        .content_type
        .unwrap_or_else(|| "application/octet-stream".to_owned())
        .into();

    // Start a resumable session (URL lives only for this call — Pitfall 5).
    let session = store
        .client
        .prepare_resumable_upload(
            &UploadObjectRequest {
                bucket: store.bucket.clone(),
                ..Default::default()
            },
            &UploadType::Simple(media),
        )
        .await
        .map_err(map_gcs_error)?;

    let mut hasher = Hasher::new();
    let mut offset: u64 = 0;
    let mut chunk_buf = vec![0u8; chunk_size];
    let mut filled: usize = 0;

    loop {
        let n = stream
            .read(&mut chunk_buf[filled..])
            .await
            .map_err(|e| recoverable_transient(format!("stream read: {e}")))?;
        if n == 0 {
            // EOF — flush the final (possibly short, possibly empty) chunk with a
            // known total size so the session commits.
            let total = offset + filled as u64;
            hasher.update(&chunk_buf[..filled]);
            let last_byte = if total == 0 { 0 } else { total - 1 };
            let size = ChunkSize::new(offset, last_byte, Some(total));
            let status = session
                .upload_multiple_chunk(chunk_buf[..filled].to_vec(), &size)
                .await
                .map_err(map_gcs_error)?;
            if !matches!(status, UploadStatus::Ok(_)) {
                return Err(fatal_internal(
                    "resumable upload did not finalize after final chunk",
                ));
            }
            break;
        }
        filled += n;
        if filled == chunk_size {
            hasher.update(&chunk_buf[..filled]);
            let size = ChunkSize::new(offset, offset + filled as u64 - 1, None);
            session
                .upload_multiple_chunk(chunk_buf[..filled].to_vec(), &size)
                .await
                .map_err(map_gcs_error)?;
            offset += filled as u64;
            filled = 0;
        }
    }

    let content_id = ContentId(*hasher.finalize().as_bytes());

    // GCS has no atomic rename: copy temp -> final content-addressed key, then delete temp.
    let final_key = store.key_for(&content_id);
    store
        .client
        .copy_object(&CopyObjectRequest {
            source_bucket: store.bucket.clone(),
            source_object: temp_key.clone(),
            destination_bucket: store.bucket.clone(),
            destination_object: final_key,
            ..Default::default()
        })
        .await
        .map_err(map_gcs_error)?;
    store
        .client
        .delete_object(&DeleteObjectRequest {
            bucket: store.bucket.clone(),
            object: temp_key,
            ..Default::default()
        })
        .await
        .map_err(map_gcs_error)?;

    Ok(content_id)
}
