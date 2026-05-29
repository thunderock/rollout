# GCP

`rollout-cloud-gcp` implements the four cloud traits against GCP: Cloud Storage
(`ObjectStore`), Pub/Sub (`Queue`), Secret Manager (`SecretStore`, read-only),
and the GCE metadata server (`ComputeHint`). It is built behind a Cargo feature
so non-GCP builds pull no GCP SDK crates.

## Build

```bash
cargo build -p rollout-cli --features gcp
```

The `gcp` feature is default-off. A binary built without it that loads a
`[cloud] provider = "gcp"` config returns a fatal error telling you to rebuild
with `--features gcp`. The `aws` and `gcp` features compose — `--features
aws,gcp` builds a binary that dispatches on the TOML `[cloud].provider` value.

## Config

```toml
[cloud]
provider = "gcp"
project  = "my-gcp-project"

[cloud.gcp.gcs]
bucket                  = "my-rollout-bucket"
prefix                  = "runs/"          # optional
resumable_chunk_bytes   = 16777216         # 16 MiB; 5 MiB minimum
max_snapshot_part_bytes = 5368709120       # 5 GiB; 10 GiB hard cap

[cloud.gcp.pubsub]
topic            = "rollout-work"
subscription     = "rollout-workers"
ack_deadline_secs = 30

[cloud.gcp.secrets]
allowlist = ["hf-token"]
```

Cross-cloud is structurally impossible: the `provider` tag is an enum, so a
single config cannot name both AWS and GCP.

## IAM / Workload Identity Federation

GCS object admin, Pub/Sub publisher + subscriber, and Secret Manager
`secretAccessor` are required; GCE `compute.viewer` is needed only when running
on GCE/GKE. The full role matrix and the (informational) lifecycle policy live
in [`crates/rollout-cloud-gcp/docs/bucket-setup.md`](https://github.com/astiwari/rollout/blob/main/crates/rollout-cloud-gcp/docs/bucket-setup.md).
The `cloud-live-gcp` CI job authenticates via Workload Identity Federation (WIF)
— no long-lived service-account keys.

## Testing matrix

| Tier | What runs | When |
|------|-----------|------|
| unit | error mapping + key layout + Secret Manager allowlist + MDS mock | every `cargo test` (no Docker) |
| emulator (`cloud-emulator-gcp`) | GCS + Pub/Sub conformance against fake-gcs-server + pubsub-emulator + in-test Secret Manager mock | every PR — always-on |
| live (`cloud-live-gcp`) | same suite against real GCP via WIF | nightly (cron) |

Locally, bring up the emulators and run the ignored conformance suite:

```bash
docker compose -f docker-compose.test.yml up -d
STORAGE_EMULATOR_HOST=http://localhost:4443 \
PUBSUB_EMULATOR_HOST=localhost:8085 PUBSUB_PROJECT_ID=rollout-test \
cargo test -p rollout-cloud-gcp --features gcp --tests -- --include-ignored --test-threads=1
```

## Emulator delta

Some production semantics are not reproducible on the emulators (resumable
upload status, time-based Pub/Sub redelivery, real fault injection). Those tests
are `#[ignore]`d locally and run in `cloud-live-gcp`. There is **no first-party
Secret Manager emulator**; the Secret Manager conformance tests use an in-test
hyper mock and run Docker-free on every build. See the crate
[README](https://github.com/astiwari/rollout/blob/main/crates/rollout-cloud-gcp/README.md)
for the full delta table.

## Common errors

- **Throttled / `ResourceExhausted` / 429 / 503** — surfaces as
  `Recoverable::Throttled` with a backoff `RetryHint`. The streaming `ContentId`
  is stable across retries because chunks are hashed before each resumable
  upload chunk.
- **Spot / preemptible reclaim** — `ComputeHint::preemption_signal` reads
  `instance/preempted` from the GCE metadata server and reports a ~30s lead.
- **Secret not in allowlist** — `Fatal::ConfigInvalid`; add the name to
  `[cloud.gcp.secrets].allowlist`. `SecretStore::put` is read-only in v1.1.
- **Orphan resumable sessions** — never persisted across processes (Pitfall #5);
  GCS auto-expires incomplete sessions after 7 days, and a preempted worker
  re-uploads from byte 0 idempotently.
