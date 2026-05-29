# AWS

`rollout-cloud-aws` implements the four cloud traits against AWS: S3
(`ObjectStore`), SQS (`Queue`), Secrets Manager (`SecretStore`), and EC2
instance metadata (`ComputeHint`, IMDSv2-only). It is built behind a Cargo
feature so non-AWS builds pull no AWS SDK crates.

## Build

```bash
cargo build -p rollout-cli --features aws
```

The `aws` feature is default-off. A binary built without it that loads an
`[cloud] provider = "aws"` config returns a fatal error telling you to rebuild
with `--features aws`.

## Config

```toml
[cloud]
provider = "aws"
region   = "us-west-2"

[cloud.aws.s3]
bucket                = "my-rollout-bucket"
prefix                = "runs/"          # optional
multipart_chunk_bytes = 16777216         # 16 MiB; S3 minimum is 5 MiB
max_snapshot_part_bytes = 5368709120     # 5 GiB; 10 GiB hard cap

[cloud.aws.sqs]
queue_url              = "https://sqs.us-west-2.amazonaws.com/123456789012/rollout"
visibility_timeout_secs = 300

[cloud.aws.secrets]
allowlist = ["rollout/hf-token"]
```

Cross-cloud is structurally impossible: the `provider` tag is an enum, so a
single config cannot name both AWS and GCP.

## IAM permissions

S3 object + multipart operations, SQS send/receive/delete/visibility, and
Secrets Manager `GetSecretValue` are required. The full IAM matrix and the
**mandatory** `AbortIncompleteMultipartUpload` lifecycle rule live in
[`crates/rollout-cloud-aws/docs/bucket-setup.md`](https://github.com/astiwari/rollout/blob/main/crates/rollout-cloud-aws/docs/bucket-setup.md).

## Testing matrix

| Tier | What runs | When |
|------|-----------|------|
| unit | error mapping + key layout + IMDSv2 mock handshake | every `cargo test` (no Docker) |
| emulator (`cloud-emulator-aws`) | full S3 + SQS + Secrets Manager conformance against localstack | every PR — always-on |
| live (`cloud-live-aws`) | same suite against real AWS via OIDC | nightly (cron) |

Locally, bring up the emulator and run the ignored conformance suite:

```bash
docker compose -f docker-compose.test.yml up -d
LOCALSTACK_ENDPOINT=http://localhost:4566 \
AWS_ACCESS_KEY_ID=test AWS_SECRET_ACCESS_KEY=test AWS_REGION=us-east-1 \
cargo test -p rollout-cloud-aws --features aws --tests -- --include-ignored --test-threads=1
```

## Common errors

- **Throttled / `SlowDown` / 503** — surfaces as `Recoverable::Throttled` with a
  backoff `RetryHint`; the SDK retries internally and the snapshotter retries on
  top. Streaming `ContentId` is stable across retries because chunks are hashed
  before each `UploadPart`.
- **IMDSv1 disabled (`HttpTokens=required`)** — handled transparently: the SDK
  performs the IMDSv2 `PUT /latest/api/token` handshake. No raw metadata IP is
  ever used.
- **Secret not in allowlist** — `Fatal::ConfigInvalid`; add the name to
  `[cloud.aws.secrets].allowlist`. `SecretStore::put` is read-only in v1.1.
- **Orphan multipart uploads** — best-effort aborted on drop; the bucket
  lifecycle rule reclaims any that leak on SIGKILL.
