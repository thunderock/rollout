//! Streaming `get_stream`: adapt the GCS `download_streamed_object` byte stream
//! into an `AsyncRead`, `Box::pin`-wrapped so no SDK type escapes the trait return.

use std::pin::Pin;

use futures_util::TryStreamExt;
use gcloud_storage::http::objects::download::Range;
use gcloud_storage::http::objects::get::GetObjectRequest;
use tokio::io::AsyncRead;
use tokio_util::io::StreamReader;

use rollout_core::{ContentId, CoreError};

use crate::error::{fatal_internal, map_gcs_error};

pub(crate) async fn get_stream_impl(
    store: &super::GcsObjectStore,
    id: &ContentId,
) -> Result<Pin<Box<dyn AsyncRead + Send>>, CoreError> {
    let key = store.key_for(id);
    let stream = store
        .client
        .download_streamed_object(
            &GetObjectRequest {
                bucket: store.bucket.clone(),
                object: key,
                ..Default::default()
            },
            &Range::default(),
        )
        .await
        .map_err(|e| {
            if super::is_not_found(&e) {
                fatal_internal(&format!("object not found: {id}"))
            } else {
                map_gcs_error(e)
            }
        })?;
    // Render each SDK stream error to an io::Error so no SDK type escapes.
    let io_stream = stream.map_err(|e| std::io::Error::other(format!("gcs download: {e}")));
    Ok(Box::pin(StreamReader::new(io_stream)))
}
