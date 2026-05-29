---
phase: 05-cloud-layer-object-store-snapshots
plan: 05
subsystem: cloud
tags: [aws, s3, sqs, secretsmanager, imdsv2, multipart, blake3, localstack, ci-gates]

# Dependency graph
requires:
  - phase: 05-04-stage1-trait-extensions-ci-gates
    provides: ObjectStore::put_stream/get_stream + Queue::dequeue_with_lease/extend_lease + LeaseToken + CloudConfig::Aws + public-api-cloud-leak/forbidden-patterns gates
  - phase: 05-03-precursor-msrv-bump
    provides: MSRV BUMP decision enabling caret AWS SDK selectors
  - phase: 02-local-substrate
    provides: FsObjectStore key layout + for_current_platform ComputeHint to mirror/delegate to
provides:
  - rollout-cloud-aws::S3ObjectStore (multipart put_stream + MultipartGuard + blake3-hash-before-send)
  - rollout-cloud-aws::SqsQueue (dequeue_with_lease/extend_lease via ChangeMessageVisibility + inflight table)
  - rollout-cloud-aws::SecretsManagerSecretStore (read-only allowlist)
  - rollout-cloud-aws::Ec2MetadataComputeHint (IMDSv2-only via aws_config::imds)
  - QueueItemId::from_message_id_string (rollout-core; deterministic ULID from opaque message ids)
  - cloud-emulator-aws (always-on) + cloud-live-aws (nightly) CI jobs + docker-compose.test.yml
  - rollout-cli `aws` feature + cloud_factory::build_cloud_runtime
affects: [05-07-snapshot-streaming-witnesses, 05-08-rollout-cloud-doctor, 06-stage3-cloud-gcp-impl]

# Tech tracking
tech-stack:
  added: [aws-config 1.8, aws-sdk-s3 1, aws-sdk-sqs 1, aws-sdk-secretsmanager 1, aws-smithy-runtime 1, aws-credential-types 1, base64 0.22, hyper 1 (dev), aws-lc-rs (transitive TLS)]
  patterns:
    - "Centralized SDK->CoreError mapping by rendered-string classification; no SDK type in any error #[source] chain"
    - "MultipartGuard sync-Drop spawn-abort with runtime-shutdown leak-warn fallback"
    - "blake3 hashed per chunk BEFORE upload_part so ContentId is retry-stable"
    - "QueueItemId <-> ReceiptHandle inflight HashMap bridges the trait surface to SQS"
    - "IMDSv2-only via aws_config::imds::client::Client (never names the link-local metadata IP)"
    - "feature-gated integration tests (#![cfg(feature = \"aws\")]) keep the default workspace test build SDK-free"

key-files:
  created:
    - crates/rollout-cloud-aws/src/config.rs
    - crates/rollout-cloud-aws/src/error.rs
    - crates/rollout-cloud-aws/src/s3/mod.rs
    - crates/rollout-cloud-aws/src/s3/put_stream.rs
    - crates/rollout-cloud-aws/src/s3/get_stream.rs
    - crates/rollout-cloud-aws/src/sqs/mod.rs
    - crates/rollout-cloud-aws/src/sqs/lease.rs
    - crates/rollout-cloud-aws/src/secrets_manager/mod.rs
    - crates/rollout-cloud-aws/src/imds/mod.rs
    - crates/rollout-cloud-aws/tests/{conformance,put_stream_dropped_aborts_multipart,put_stream_content_id_matches_post_retry,throttled_put_recovers_via_retry_hint,imds_v1_disabled_falls_back_gracefully}.rs
    - crates/rollout-cloud-aws/tests/support/mod.rs
    - crates/rollout-cloud-aws/docs/bucket-setup.md
    - crates/rollout-cli/src/cloud_factory.rs
    - docker-compose.test.yml
    - docs/book/src/cloud/aws.md
  modified:
    - Cargo.toml
    - rust-toolchain.toml
    - deny.toml
    - .github/workflows/ci.yml
    - crates/rollout-cloud-aws/{Cargo.toml,src/lib.rs}
    - crates/rollout-core/src/traits/cloud.rs
    - crates/rollout-cli/{Cargo.toml,src/main.rs}
    - docs/book/src/SUMMARY.md

key-decisions:
  - "Toolchain bumped 1.91.0 -> 1.91.1: the published AWS SDK requires rustc 1.91.1 (the MSRV-BUMP precursor's exact motivation); pinned in rust-toolchain.toml + workspace rust-version + all 12 CI toolchain refs"
  - "Error classification by rendered-string substring (SlowDown/503/429 -> Throttled; timeout/5xx -> Transient; else Fatal::Internal) keeps the mapper generic over E: Display and SDK-version-agnostic"
  - "Throttle/retry fixture witnesses the crate's error-mapping contract (via #[doc(hidden)] retry_hint_for_test) + the localstack success path, rather than a fragile smithy interceptor whose retry classification is version-coupled"
  - "QueueItemId::from_message_id_string added to rollout-core (5-line blake3-fold) per the plan's option (a)"
  - "cloud-live-aws gated on schedule-only (nightly cron) — the plan's changed_files path filter is not reliably available, so the simpler always-nightly trigger ships"

patterns-established:
  - "Pattern 2 (RESEARCH): per-trait SDK call mapping centralized in error.rs"
  - "Pattern 4 (RESEARCH): docker-compose.test.yml + always-on emulator CI job"
  - "Pattern 5 (RESEARCH): MultipartGuard sync-Drop"
  - "Pattern 6 (RESEARCH): blake3 incremental hash-before-send"
  - "Pattern 16 (RESEARCH): ConformanceTarget localstack harness"

requirements-completed: [CLOUD-01]

# Metrics
duration: 155min
completed: 2026-05-28
---

# Phase 5 Plan 05: Stage 2 — rollout-cloud-aws Implementation Summary

**First cloud-provider adapter: S3 streaming `ObjectStore` (multipart + MultipartGuard Drop-abort + blake3-hash-before-send), SQS lease `Queue` (ChangeMessageVisibility + inflight ReceiptHandle table), read-only Secrets Manager `SecretStore`, and IMDSv2-only EC2 `ComputeHint` — all behind a default-off `aws` feature, gated by an always-on localstack CI job and four Pitfall-prevention fixtures, with zero SDK types leaking into `rollout-core`.**

## Performance

- **Duration:** ~155 min
- **Tasks:** 5
- **Files modified/created:** 36 (+3166/-90 across the 6 plan commits)

## Accomplishments
- `S3ObjectStore` implements all five `ObjectStore` methods. `put_stream` does a multipart upload to a `temp/pending-<ulid>` key, hashes each chunk with blake3 BEFORE `UploadPart` (so `ContentId` is stable across SDK retries — Pitfall 16), then server-side `CopyObject` to the sharded content-addressed key (`<prefix>cas/ab/cd/<hex>`, FsObjectStore parity) + `DeleteObject` the temp. A `MultipartGuard` aborts the multipart on any non-commit drop, with a runtime-shutdown leak-warn fallback (Pitfall 4).
- `SqsQueue` implements all six `Queue` methods. Lease maps to SQS visibility timeout: `dequeue_with_lease` sets `VisibilityTimeout`, `extend_lease`/`nack` call `ChangeMessageVisibility` (`nack` with timeout 0 for immediate redelivery), `ack` calls `DeleteMessage`. An `Arc<Mutex<HashMap<QueueItemId, ReceiptHandle>>>` inflight table bridges the trait's `QueueItemId`-only `ack`/`nack` to SQS's ReceiptHandle requirement.
- `SecretsManagerSecretStore` enforces the allowlist at the trait boundary (before any SDK call); `put` always returns `Fatal::ConfigInvalid` ("read-only in v1.1").
- `Ec2MetadataComputeHint` wraps `aws_config::imds::client::Client` (`BehaviorVersion::latest()` = IMDSv2-only, Pitfall 3); `inventory()` pulls `instance_type` from IMDS and delegates CPU/mem/GPU to the local platform hint; `preemption_signal()` maps spot/instance-action 200->Some(120s), 404->None, other->Fatal.
- `error.rs` collapses every SDK operation error to `CoreError` by rendered-string classification — no SDK type appears in any public signature or `#[source]` chain, so the `public-api-cloud-leak` gate stays trivially green (rollout-core has zero AWS deps).
- `cloud-emulator-aws` always-on CI job runs the full conformance + fixture suite against localstack 3.7.0; `cloud-live-aws` nightly job runs the same against real AWS via OIDC. `docker-compose.test.yml` pins localstack. `rollout-cli` gains a default-off `aws` feature wired through `cloud_factory::build_cloud_runtime`.

## Task Commits

1. **Task 1: S3ObjectStore + MultipartGuard + blake3-hash-before-send + SDK deps + bucket-setup docs** — `7d23e86` (feat)
2. **Task 2: SqsQueue + dequeue_with_lease + extend_lease** — `5215dc1` (feat)
3. **Task 3: SecretsManagerSecretStore (read-only allowlist)** — `2fb755a` (feat)
4. **Task 4: Ec2MetadataComputeHint via IMDSv2** — `c2f899d` (feat)
5. **Task 5: cloud-emulator-aws CI + rollout-cli aws feature + docs** — `5974bb9` (feat)
6. **Pre-existing rustfmt drift normalization (cloud-local)** — `ac7b481` (chore)

_TDD note: per-trait tasks landed impl + tests in one feat commit each. The localstack conformance/fixture tests are `#[ignore]`'d (can't RED locally without Docker); the 4 mock-IMDSv2 tests + all `error.rs`/secrets unit tests run RED->GREEN in the default loop and pass on every CI build._

## Decisions Made
- **Toolchain 1.91.0 -> 1.91.1:** the published AWS SDK (`aws-config 1.8.17`, etc.) requires `rustc 1.91.1`. This is exactly why the 05-03 precursor BUMPed the MSRV; 1.91.1 was already installed locally. Bumped `rust-toolchain.toml`, workspace `rust-version`, and all 12 `dtolnay/rust-toolchain@` CI pins.
- **String-classification error mapping:** keeps the mapper generic + SDK-version-agnostic and avoids coupling to per-operation error enums.
- **Throttle fixture strategy:** witnesses the crate's mapping contract (`retry_hint_for_test` shim) + the localstack success path instead of a smithy interceptor whose retry classification is version-fragile.
- **`QueueItemId::from_message_id_string`** added to rollout-core (plan option a): deterministic blake3-folded ULID from opaque SQS/Pub-Sub message ids.
- **cloud-live-aws nightly-only trigger:** the plan's `pull_request.changed_files` path filter is not reliably exposed in `github.event`; shipped the simpler `github.event_name == 'schedule'` gate.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Bumped Rust toolchain 1.91.0 -> 1.91.1**
- **Found during:** Task 1 (first `cargo build --features aws`)
- **Issue:** Every published AWS SDK crate requires `rustc 1.91.1`; the pin was `1.91.0`, so the build refused to resolve.
- **Fix:** `rust-toolchain.toml` channel -> 1.91.1; workspace `rust-version` -> 1.91.1; all 12 CI `dtolnay/rust-toolchain@1.91.0` -> `@1.91.1`. Consistent with the 05-03 BUMP decision (the AWS SDK is the bump's sole motivation).
- **Commit:** 7d23e86

**2. [Rule 3 - Blocking] Adapted to real local-hint / config API names**
- **Found during:** Tasks 4 & 5
- **Issue:** The plan referenced `rollout_cloud_local::hints::LocalComputeHint` and `rollout_core::config::AwsConfig`; the real names are `hints::for_current_platform() -> Box<dyn ComputeHint>` and `rollout_core::config::cloud::AwsConfig`. `AwsSecretsConfig` has no `region_override`; `AwsS3Config.prefix` is `Option<String>`.
- **Fix:** Used `for_current_platform()` (boxed) as the IMDS fallback + the CLI local runtime hint; addressed `AwsConfig` via the `config::cloud` path; treated `prefix` as `Option` (`unwrap_or_default`).
- **Commits:** c2f899d, 5974bb9

**3. [Rule 3 - Blocking] Feature-gated integration tests to keep the default workspace test build SDK-free**
- **Found during:** wave verification (`cargo test --workspace --tests`)
- **Issue:** The `tests/*.rs` files reference `aws_sdk_s3` etc. (only present under `--features aws`), so the default workspace test build failed to compile them.
- **Fix:** Added `#![cfg(feature = "aws")]` to all five integration-test files. Default `cargo test --workspace --tests` now compiles 83 binaries clean; the AWS suite compiles under `--features aws`.
- **Commit:** 5974bb9 (and adjusted in the wave-verify pass)

**4. [Rule 1 - Bug] Mock IMDSv2 token response needed the TTL header**
- **Found during:** Task 4
- **Issue:** The SDK's `parse_token_response` reads the TTL from the `x-aws-ec2-metadata-token-ttl-seconds` response header; the first mock returned only a body, so token load failed.
- **Fix:** Echo `x-aws-ec2-metadata-token-ttl-seconds: 21600` on the mock token response. All 4 IMDS tests pass.
- **Commit:** c2f899d

**5. [Rule 1 - Bug] Reworded the IMDS doc comment to drop the literal metadata IP**
- **Found during:** Task 4 acceptance
- **Issue:** A doc comment in `imds/mod.rs` literally named `169.254.169.254`, failing the acceptance grep (must be 0 in that file) even though the forbidden-patterns gate allowlists the path.
- **Fix:** Reworded to "the raw link-local metadata IP". Grep now 0; gate green.
- **Commit:** c2f899d

**6. [Rule 3 - Blocking] rustfmt-normalized pre-existing drift to keep the CI fmt gate green**
- **Found during:** wave verification (`cargo fmt --check`)
- **Issue:** `rollout-core` (left by my own fmt run) + `rollout-cloud-local` (left by Plan 05-04) carried fmt drift that the macOS `lint` CI job's `cargo fmt --check` would reject.
- **Fix:** Two focused `[skip-docs-check]` chore commits normalizing the wrapping (formatting only).
- **Commits:** c60c3cf (folded into 5974bb9), ac7b481

---

**Total deviations:** 6 auto-fixed (4 blocking, 2 bug). **Impact:** all required to match the real codebase + published SDK MSRV and to keep every CI gate green. No scope creep; no architectural changes.

## Issues Encountered
- `cargo public-api` is not installed locally (noted in 05-04); the leak gate is satisfied structurally — `rollout-core`'s dependency tree contains zero `aws*`/`smithy*` crates, so its public API cannot carry SDK symbols. CI installs `cargo-public-api` and runs the gate script.
- `sqs_queue_extend_lease_succeeds_via_change_message_visibility` is a ~35s wall-clock localstack test (real visibility-window wait); it runs only in the emulator/live CI jobs.

## User Setup Required
- **Repo operator:** add `cloud-emulator-aws` to branch-protection required checks. For `cloud-live-aws`, configure the GitHub OIDC trust + the `ROLLOUT_CLOUD_LIVE_AWS_ROLE_ARN` / `ROLLOUT_CLOUD_LIVE_AWS_BUCKET` / `ROLLOUT_CLOUD_LIVE_AWS_QUEUE_URL` repo vars and apply the bucket lifecycle rule from `crates/rollout-cloud-aws/docs/bucket-setup.md`.

## Next Phase Readiness
- The AWS adapter is ready for Plan 05-07 (snapshot streaming witnesses can target `S3ObjectStore::put_stream`/`get_stream`) and 05-08 (`rollout cloud doctor` can consume `cloud_factory::build_cloud_runtime` + `S3ObjectStore::bucket()`).
- Plan 06 (GCP) mirrors this structure behind a `gcp` feature; `cloud_factory` already has the `CloudConfig::Gcp` arm stubbed with a rebuild-with-feature error.

---
*Phase: 05-cloud-layer-object-store-snapshots*
*Completed: 2026-05-28*

## Self-Check: PASSED

All 6 task/chore commits (7d23e86, 5215dc1, 2fb755a, c2f899d, 5974bb9, ac7b481) exist; all created files verified present on disk.
