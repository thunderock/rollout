# rollout-cloud-gcp

GCP implementations of the `rollout-core` cloud traits, behind a default-off
`gcp` Cargo feature:

- **`GcsObjectStore`** — `ObjectStore` over [`gcloud-storage`], streaming
  `put_stream` via a GCS resumable upload session with blake3-incremental
  hashing + atomic temp-then-rename (no cross-process `upload_id` persistence).
- **`PubSubQueue`** — `Queue` over [`gcloud-pubsub`] with lease semantics mapped
  onto `modify_ack_deadline`.
- **`SecretManagerSecretStore`** — read-only `SecretStore` over the Secret
  Manager v1 REST API with allowlist enforcement.
- **`GceMetadataComputeHint`** — `ComputeHint` over [`gcloud-metadata`] (GCE
  metadata server); no raw `metadata.google.internal` URL appears in this crate.

No GCP SDK type leaks into `rollout-core`'s public API: every SDK error is
collapsed to `CoreError` at this crate's `error.rs` boundary.

## SDK cohort

This crate uses the [`gcloud-*`] cohort (the `yoshidan/google-cloud-rust`
family): `gcloud-storage`, `gcloud-pubsub`, `gcloud-auth`, `gcloud-metadata`.
Secret Manager is reached over its REST API (the cohort ships no
secret-manager client), keeping the dependency tree slim and the leak gate
trivially green.

## Emulator delta vs production GCP

| Behavior | Emulator (fake-gcs-server / pubsub-emulator) | Production GCP |
|----------|----------------------------------------------|----------------|
| Resumable upload status query | No-op (silently succeeds) | Real `PUT ?uploadType=resumable` with `Content-Range bytes */SIZE` |
| Resumable fault injection | Not supported by fake-gcs-server | Real 503/429 retries; witnessed in `cloud-live-gcp` |
| Pub/Sub ack-deadline redelivery | Connection-drop only | Time-based; `modify_ack_deadline` enforced |
| Pub/Sub message ordering | Not guaranteed | Optional (per-topic ordering key) |
| Secret Manager | No first-party emulator; in-test hyper mock | Secret Manager v1 REST API |

Tests that depend on production-only semantics (time-based redelivery, real
resumable fault injection) are `#[ignore]`d locally and run in the
`cloud-live-gcp` nightly CI job. The always-on `cloud-emulator-gcp` job runs the
emulator-safe subset against fake-gcs-server + pubsub-emulator + the in-test
Secret Manager mock.

[`gcloud-storage`]: https://crates.io/crates/gcloud-storage
[`gcloud-pubsub`]: https://crates.io/crates/gcloud-pubsub
[`gcloud-metadata`]: https://crates.io/crates/gcloud-metadata
[`gcloud-*`]: https://github.com/yoshidan/google-cloud-rust
