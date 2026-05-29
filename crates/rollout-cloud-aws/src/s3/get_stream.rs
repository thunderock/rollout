//! Streaming `get_stream`: unwrap the S3 `GetObject` body into an `AsyncRead`,
//! `Box::pin`-wrapped so no SDK type escapes the trait return.

use std::pin::Pin;

use tokio::io::AsyncRead;

use rollout_core::{ContentId, CoreError};

use crate::error::{fatal_internal, map_s3_sdk_error};

pub(crate) async fn get_stream_impl(
    store: &super::S3ObjectStore,
    id: &ContentId,
) -> Result<Pin<Box<dyn AsyncRead + Send>>, CoreError> {
    let key = store.key_for(id);
    let resp = store
        .client
        .get_object()
        .bucket(&store.bucket)
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
    Ok(Box::pin(resp.body.into_async_read()))
}
