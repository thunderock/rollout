//! In-memory `QueueItemId -> ReceiptHandle` table.
//!
//! The `Queue` trait surface hands `ack`/`nack` only a `QueueItemId`, but SQS
//! `DeleteMessage` / `ChangeMessageVisibility` require the per-receive
//! `ReceiptHandle`. The table is populated on every dequeue and consumed on
//! ack/nack.

use rollout_core::traits::cloud::{LeaseToken, QueueItemId};
use rollout_core::{CoreError, RecoverableError, RetryHint};

use crate::error::map_sqs_sdk_error;

pub(crate) async fn register_inflight(
    queue: &super::SqsQueue,
    id: QueueItemId,
    token: &LeaseToken,
) {
    let receipt = std::str::from_utf8(&token.0).unwrap_or_default().to_owned();
    queue.inflight.lock().await.insert(id, receipt);
}

fn not_in_flight(op: &str, id: QueueItemId) -> CoreError {
    CoreError::Recoverable(RecoverableError::Transient {
        msg: format!("{op}: QueueItemId {id:?} not in-flight (lease expired or never dequeued)"),
        hint: RetryHint::Never,
    })
}

pub(crate) async fn ack_via_inflight_table(
    queue: &super::SqsQueue,
    id: QueueItemId,
) -> Result<(), CoreError> {
    let Some(receipt) = queue.inflight.lock().await.remove(&id) else {
        return Err(not_in_flight("ack", id));
    };
    queue
        .client
        .delete_message()
        .queue_url(&queue.queue_url)
        .receipt_handle(receipt)
        .send()
        .await
        .map_err(map_sqs_sdk_error)?;
    Ok(())
}

pub(crate) async fn nack_via_inflight_table(
    queue: &super::SqsQueue,
    id: QueueItemId,
) -> Result<(), CoreError> {
    let Some(receipt) = queue.inflight.lock().await.remove(&id) else {
        return Err(not_in_flight("nack", id));
    };
    queue
        .client
        .change_message_visibility()
        .queue_url(&queue.queue_url)
        .receipt_handle(receipt)
        .visibility_timeout(0) // make visible immediately for redelivery
        .send()
        .await
        .map_err(map_sqs_sdk_error)?;
    Ok(())
}
