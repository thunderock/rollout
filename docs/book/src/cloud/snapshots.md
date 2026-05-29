# Cloud-backed snapshots

Training-state snapshots stream to whichever object store your `[cloud]` block
selects. `rollout-snapshots` takes an injected `Arc<dyn ObjectStore>`, so the
same `SnapshotterImpl` works unchanged over the local filesystem, S3, or GCS —
only the injected store differs (CLOUD-03).

## Configuration

`CloudConfig` is a `#[serde(tag = "provider")]` enum, so the provider's fields
live directly under `[cloud]`. A single TOML cannot name two providers —
cross-cloud single-run is structurally impossible (D-XPROV-02).

See [`examples/sft-tiny-aws.toml`](https://github.com/thunderock/rollout/blob/main/examples/sft-tiny-aws.toml)
and [`examples/sft-tiny-gcp.toml`](https://github.com/thunderock/rollout/blob/main/examples/sft-tiny-gcp.toml)
for the minimal `[cloud]` flip from `examples/sft-tiny.toml`:

```toml
# AWS
[cloud]
provider = "aws"
region = "us-west-2"

[cloud.s3]
bucket = "rollout-snapshots-prod"
prefix = "sft-tiny/"
```

```toml
# GCP
[cloud]
provider = "gcp"
project = "rollout-prod-123"

[cloud.gcs]
bucket = "rollout-snapshots-prod"
prefix = "sft-tiny/"
```

## Streaming semantics

A snapshot is a deterministic tar of the accelerate-style state directory
(weights + optimizer + RNG + step), content-addressed by blake3. The upload path:

1. Builds the tar deterministically (stable file order + zeroed mtime) so the
   same state always produces the same bytes.
2. Hashes each chunk with `blake3::Hasher` **before** the SDK call, so the
   resulting `ContentId` is stable across SDK retries (S3 multipart / GCS
   resumable — Pitfall #16).
3. Uploads to a `temp/pending-<ulid>` key (S3 multipart upload / GCS resumable
   session).
4. Server-side copies the temp object to the sharded content-addressed key
   `<prefix>cas/<ab>/<cd>/<hex>` (identical layout on FS, S3, and GCS), then
   deletes the temp.
5. On failure the temp upload is aborted (S3 `MultipartGuard` Drop) or expires
   via the bucket's 7-day lifecycle rule (GCS); no orphaned partial blob is
   ever read.

Restore fetches the blob by `ContentId`, re-verifies blake3, and extracts the
tar — a mismatch is a hard `Fatal` error, never a silent partial restore.

## Byte-identical resume

The CLOUD-03 acceptance criterion — byte-identical SFT resume holds over the
cloud streaming path — is witnessed by two always-on tests:

- `bit_identical_resume_at_step_5_via_s3` (localstack-backed `S3ObjectStore`),
- `bit_identical_resume_at_step_5_via_gcs` (fake-gcs-server-backed `GcsObjectStore`).

Each snapshots a MockBackend SFT run at step 5, restores off the cloud round-trip,
runs five more steps, and asserts the final weights are byte-equal to a ten-step
uninterrupted run. They run on every CI PR via the `cloud-emulator-aws` /
`cloud-emulator-gcp` jobs — no GPU, no live cloud creds.

## Cross-provider portability

Snapshots are content-addressed by blake3, so the same bytes produce the same
`ContentId` on any provider. To migrate a snapshot from S3 to GCS, an operator
copies the blob (rollout does **not** automate cross-provider transfer in v1.1):

```bash
# Operator-managed transfer between buckets:
aws s3 cp s3://aws-bucket/cas/ab/cd/<hex> /tmp/blob
gsutil cp /tmp/blob gs://gcs-bucket/cas/ab/cd/<hex>
```

The restore code path on either provider takes a `SnapshotId` and reads by
`ContentId`; the provider is whichever `ObjectStore` is injected per
`[cloud].provider`. The runnable witness is
`crates/rollout-snapshots/tests/snapshot_resume_s3_to_gcs_via_manual_copy.rs`:
it saves via S3, copies each blob into a GCS bucket asserting the `ContentId` is
identical across providers, then restores + resumes on GCS byte-for-byte
(D-XPROV-01).

Active-active cross-cloud single run is **out of scope** in v1.1 (PROJECT.md);
the tagged-enum `CloudConfig` makes a config naming both `[cloud.s3]` and
`[cloud.gcs]` structurally un-representable.
