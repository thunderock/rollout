//! PITFALLS.md §4 witness: dropping an in-flight `put_stream` future aborts the
//! multipart upload via `MultipartGuard::drop`, leaving zero orphan multiparts.

#![cfg(feature = "aws")] // SDK-backed tests compile only under the `aws` feature
#![allow(deprecated)] // exercising overridden put_stream (trait-level #[deprecated])

mod support;

use std::pin::Pin;

use rollout_core::{ObjectStore, PutHint};
use tokio::io::{AsyncRead, AsyncReadExt};

/// A reader that yields `total` bytes slowly, blocking forever after `stall_at`,
/// so the surrounding `put_stream` future is still mid-upload when we drop it.
struct StallingReader {
    served: usize,
    stall_at: usize,
}

impl AsyncRead for StallingReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        if self.served >= self.stall_at {
            // Never completes — keeps the multipart open until the future is dropped.
            cx.waker().wake_by_ref();
            return std::task::Poll::Pending;
        }
        let chunk = (self.stall_at - self.served)
            .min(buf.remaining())
            .min(1024 * 1024);
        buf.initialize_unfilled_to(chunk);
        buf.advance(chunk);
        self.served += chunk;
        std::task::Poll::Ready(Ok(()))
    }
}

#[tokio::test]
#[ignore = "requires LOCALSTACK_ENDPOINT"]
async fn put_stream_dropped_aborts_multipart() {
    let Some(ep) = support::localstack_endpoint() else {
        return;
    };
    let store = support::build_localstack_store(&ep).await;
    let bucket = store.bucket().to_owned();
    let client = support::s3_client(&ep).await;

    // Stall after ~20 MiB so at least one upload_part has happened and the
    // multipart is open, then drop the future.
    let reader: Pin<Box<dyn AsyncRead + Send>> = Box::pin(StallingReader {
        served: 0,
        stall_at: 20 * 1024 * 1024,
    });
    // Poll put_stream until it stalls (multipart open, first part uploaded),
    // then drop the future to trigger MultipartGuard::drop.
    {
        let fut = store.put_stream(reader, PutHint::default());
        tokio::pin!(fut);
        let _ = tokio::time::timeout(std::time::Duration::from_millis(1500), &mut fut).await;
        // `fut` is dropped here at end of scope (timeout elapsed; it never completed).
    }

    // Wait for the Drop-spawned abort task to run.
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let listed = client
        .list_multipart_uploads()
        .bucket(&bucket)
        .send()
        .await
        .unwrap();
    let count = listed.uploads().len();
    assert_eq!(count, 0, "expected zero orphan multiparts, found {count}");
}

// Silence unused import in the no-endpoint path.
#[allow(unused_imports)]
use AsyncReadExt as _;
