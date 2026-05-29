//! `SqsQueue` — AWS SQS impl of `Queue` (RESEARCH.md Pattern 2).
//!
//! Lease semantics map onto SQS visibility timeouts: `dequeue_with_lease` sets
//! `VisibilityTimeout`, `extend_lease` / `nack` call `ChangeMessageVisibility`,
//! `ack` calls `DeleteMessage`. The trait gives `ack`/`nack` only a
//! `QueueItemId`, but SQS needs a `ReceiptHandle`, so the queue keeps an
//! in-memory `QueueItemId -> ReceiptHandle` inflight table (see the `lease` module).

pub(crate) mod lease;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use aws_sdk_sqs::Client;
use base64::Engine;
use tokio::sync::Mutex;

use rollout_core::traits::cloud::{LeaseToken, Queue, QueueItemId};
use rollout_core::CoreError;

use crate::error::{fatal_internal, map_sqs_sdk_error};

/// SQS-backed work queue with lease support.
pub struct SqsQueue {
    pub(crate) client: Arc<Client>,
    pub(crate) queue_url: String,
    default_visibility_timeout_secs: i32,
    pub(crate) inflight: Arc<Mutex<HashMap<QueueItemId, String>>>,
}

impl SqsQueue {
    /// Construct an `SqsQueue`.
    #[must_use]
    pub fn new(
        client: Arc<Client>,
        queue_url: String,
        default_visibility_timeout_secs: u32,
    ) -> Self {
        Self {
            client,
            queue_url,
            default_visibility_timeout_secs: i32::try_from(default_visibility_timeout_secs)
                .unwrap_or(300),
            inflight: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn dequeue_internal(
        &self,
        visibility_secs: i32,
    ) -> Result<Option<(QueueItemId, Vec<u8>, LeaseToken)>, CoreError> {
        let resp = self
            .client
            .receive_message()
            .queue_url(&self.queue_url)
            .visibility_timeout(visibility_secs)
            .wait_time_seconds(20)
            .max_number_of_messages(1)
            .send()
            .await
            .map_err(map_sqs_sdk_error)?;
        let Some(msg) = resp.messages.unwrap_or_default().into_iter().next() else {
            return Ok(None);
        };
        let receipt = msg
            .receipt_handle()
            .ok_or_else(|| fatal_internal("ReceiveMessage missing receipt_handle"))?
            .to_owned();
        let mid = msg
            .message_id()
            .ok_or_else(|| fatal_internal("ReceiveMessage missing message_id"))?
            .to_owned();
        let body_b64 = msg
            .body()
            .ok_or_else(|| fatal_internal("ReceiveMessage missing body"))?;
        let payload = base64::engine::general_purpose::STANDARD
            .decode(body_b64)
            .map_err(|e| fatal_internal(&format!("base64 decode: {e}")))?;
        let id = QueueItemId::from_message_id_string(&mid);
        let token = LeaseToken(receipt.into_bytes());
        lease::register_inflight(self, id, &token).await;
        Ok(Some((id, payload, token)))
    }
}

#[async_trait]
impl Queue for SqsQueue {
    async fn enqueue(&self, payload: Vec<u8>) -> Result<QueueItemId, CoreError> {
        // Base64 so arbitrary bytes survive the SQS UTF-8 message-body contract.
        let body = base64::engine::general_purpose::STANDARD.encode(&payload);
        let resp = self
            .client
            .send_message()
            .queue_url(&self.queue_url)
            .message_body(body)
            .send()
            .await
            .map_err(map_sqs_sdk_error)?;
        let mid = resp
            .message_id()
            .ok_or_else(|| fatal_internal("SendMessage missing message_id"))?;
        Ok(QueueItemId::from_message_id_string(mid))
    }

    async fn dequeue(&self) -> Result<Option<(QueueItemId, Vec<u8>)>, CoreError> {
        self.dequeue_internal(self.default_visibility_timeout_secs)
            .await
            .map(|opt| opt.map(|(id, payload, _token)| (id, payload)))
    }

    async fn ack(&self, id: QueueItemId) -> Result<(), CoreError> {
        lease::ack_via_inflight_table(self, id).await
    }

    async fn nack(&self, id: QueueItemId) -> Result<(), CoreError> {
        lease::nack_via_inflight_table(self, id).await
    }

    async fn dequeue_with_lease(
        &self,
        lease: Duration,
    ) -> Result<Option<(QueueItemId, Vec<u8>, LeaseToken)>, CoreError> {
        let secs = i32::try_from(lease.as_secs()).unwrap_or(self.default_visibility_timeout_secs);
        self.dequeue_internal(secs).await
    }

    async fn extend_lease(
        &self,
        _id: QueueItemId,
        token: LeaseToken,
        extend_by: Duration,
    ) -> Result<(), CoreError> {
        let secs =
            i32::try_from(extend_by.as_secs()).unwrap_or(self.default_visibility_timeout_secs);
        let receipt = std::str::from_utf8(&token.0)
            .map_err(|e| fatal_internal(&format!("LeaseToken not UTF-8: {e}")))?;
        self.client
            .change_message_visibility()
            .queue_url(&self.queue_url)
            .receipt_handle(receipt)
            .visibility_timeout(secs)
            .send()
            .await
            .map_err(map_sqs_sdk_error)?;
        Ok(())
    }
}
