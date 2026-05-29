//! PITFALLS.md §16 witness: the final `ContentId` is independent of SDK retries.
//!
//! Because chunks are hashed BEFORE the SDK `upload_part` call, the blake3 digest
//! depends only on the input bytes, never on how many times the SDK replayed a
//! part. We witness this two ways: (1) `put_stream` of identical input produces
//! an identical `ContentId` across independent uploads (determinism — the
//! load-bearing property); (2) the `ContentId` equals `blake3(input)` computed
//! out-of-band. Both run against localstack; neither requires real fault
//! injection because the hash is computed in our code, not by the SDK.

#![cfg(feature = "aws")] // SDK-backed tests compile only under the `aws` feature
#![allow(deprecated)] // exercising overridden put_stream (trait-level #[deprecated])

mod support;

use std::pin::Pin;

use rollout_core::{ContentId, ObjectStore, PutHint};
use tokio::io::AsyncRead;

#[tokio::test]
#[ignore = "requires LOCALSTACK_ENDPOINT"]
async fn put_stream_content_id_matches_post_retry() {
    let Some(ep) = support::localstack_endpoint() else {
        return;
    };
    let store = support::build_localstack_store(&ep).await;

    // 40 MiB → three parts at the 16 MiB chunk size, exercising multi-part hashing.
    let buf: Vec<u8> = (0..40 * 1024 * 1024u32)
        .map(|i| u8::try_from(i % 251).unwrap())
        .collect();
    let expected = ContentId::of(&buf);

    let mk = || -> Pin<Box<dyn AsyncRead + Send>> { Box::pin(std::io::Cursor::new(buf.clone())) };

    let id1 = store.put_stream(mk(), PutHint::default()).await.unwrap();
    let id2 = store.put_stream(mk(), PutHint::default()).await.unwrap();

    assert_eq!(id1, expected, "ContentId must equal blake3(input)");
    assert_eq!(
        id1, id2,
        "ContentId must be stable across independent multipart uploads (hash-before-send)"
    );

    let got = store.get_bytes(&id1).await.unwrap();
    assert_eq!(got.len(), buf.len());
    assert_eq!(ContentId::of(&got), expected);
}
