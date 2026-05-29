# Streaming + lease trait methods

Phase 5 extends two `rollout-core` cloud traits with four new methods. All four ship
with **backward-compatible default implementations** so every v1.0 caller and impl
compiles unchanged — only the streaming methods carry a `#[deprecated]` tag whose sole
purpose is to nudge cloud backends into overriding them.

## `ObjectStore::put_stream` / `get_stream`

```rust
#[deprecated(note = "Cloud impls MUST override; default buffers entire stream into RAM")]
async fn put_stream(
    &self,
    stream: Pin<Box<dyn AsyncRead + Send>>,
    hint: PutHint,
) -> Result<ContentId, CoreError>;

#[deprecated(note = "Cloud impls MUST override; default buffers entire blob into RAM")]
async fn get_stream(&self, id: &ContentId) -> Result<Pin<Box<dyn AsyncRead + Send>>, CoreError>;
```

The default `put_stream` reads the whole stream into a `Vec<u8>` and calls `put_bytes`;
the default `get_stream` fetches via `get_bytes` and hands back a `Cursor`. That is fine
for small blobs but **OOMs on multi-GiB snapshots** (Pitfall 16 / D-SNAP-04), which is why
the `#[deprecated]` warning fires at any call site that did not override the method. Cloud
backends (S3 multipart, GCS resumable) and the local FS store override both with true
streaming, so the warning never fires from a correct impl.

## `Queue::dequeue_with_lease` / `extend_lease`

```rust
async fn dequeue_with_lease(
    &self,
    lease: Duration,
) -> Result<Option<(QueueItemId, Vec<u8>, LeaseToken)>, CoreError>;

async fn extend_lease(
    &self,
    id: QueueItemId,
    token: LeaseToken,
    extend_by: Duration,
) -> Result<(), CoreError>;
```

`LeaseToken(Vec<u8>)` is an opaque per-backend handle: SQS ReceiptHandle bytes, Pub/Sub
ack_id bytes, or — for the in-memory queue — the `QueueItemId` bytes. The default
`dequeue_with_lease` ignores the lease duration and synthesizes a token from the item id;
the default `extend_lease` returns `Recoverable::Transient { hint: Never }` so a backend
that forgot to override it fails loudly rather than silently dropping the extension. These
two are **not** `#[deprecated]` — the default fallback is a correct (if conservative)
behavior for queues without visibility timeouts.
