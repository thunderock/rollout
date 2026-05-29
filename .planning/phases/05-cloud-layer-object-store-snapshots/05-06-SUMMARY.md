---
phase: 05-cloud-layer-object-store-snapshots
plan: 06
subsystem: cloud
tags: [gcp, gcs, pubsub, secretmanager, gce-mds, resumable-upload, blake3, fake-gcs-server, pubsub-emulator, ci-gates]

# Dependency graph
requires:
  - phase: 05-04-stage1-trait-extensions-ci-gates
    provides: ObjectStore::put_stream/get_stream + Queue::dequeue_with_lease/extend_lease + LeaseToken + CloudConfig::Gcp + public-api-cloud-leak/forbidden-patterns gates + dep-direction invariants #12/#13
  - phase: 05-05-stage2-cloud-aws-impl
    provides: reference impl pattern (per-trait SDK mapping, inflight lease table, blake3-hash-before-send, emulator CI shape) + 1.91.1 toolchain
  - phase: 02-local-substrate
    provides: FsObjectStore key layout + for_current_platform ComputeHint to mirror/delegate to
provides:
  - rollout-cloud-gcp::GcsObjectStore (resumable put_stream + blake3-hash-before-chunk + temp-then-rename, no upload_id persistence)
  - rollout-cloud-gcp::PubSubQueue (dequeue_with_lease/extend_lease via modify_ack_deadline + inflight ReceivedMessage table)
  - rollout-cloud-gcp::SecretManagerSecretStore (read-only allowlist over Secret Manager v1 REST + ADC bearer)
  - rollout-cloud-gcp::GceMetadataComputeHint (GCE MDS reader; no raw metadata host in source)
  - cloud-emulator-gcp (always-on) + cloud-live-gcp (nightly, WIF) CI jobs + docker-compose fake-gcs-server/pubsub-emulator
  - rollout-cli `gcp` feature + cloud_factory::build_gcp_runtime
affects: [05-07-snapshot-streaming-witnesses, 05-08-rollout-cloud-doctor]

# Tech tracking
tech-stack:
  added: [gcloud-storage 1.3, gcloud-pubsub 1.7, gcloud-googleapis 1.3, gcloud-auth 1.3, gcloud-metadata 1.0, token-source 1.0, reqwest 0.13, futures-util 0.3, tokio-util (io)]
  patterns:
    - "Centralized GCP SDK->CoreError mapping by rendered-string classification (ResourceExhausted->Throttled; Unavailable/timeout->Transient; PermissionDenied/404->Config); no SDK type in any #[source] chain"
    - "Resumable upload session created+used+discarded within one put_stream call; never persisted across processes (Pitfall 5)"
    - "blake3 hashed per chunk BEFORE upload_multiple_chunk so ContentId is retry-stable (Pitfall 16)"
    - "QueueItemId <-> ReceivedMessage inflight HashMap bridges the trait surface to Pub/Sub ack_id"
    - "GCE MDS host + Metadata-Flavor header sourced from gcloud-metadata SDK constants (no raw metadata.google.internal in source)"
    - "Secret Manager reached over v1 REST (cohort ships no SM client) with Docker-free in-test hyper mock"
    - "feature-gated everything behind `gcp` (#![cfg_attr(not(feature = \"gcp\"), allow(unused_crate_dependencies))]) keeps the default workspace build SDK-free"

key-files:
  created:
    - crates/rollout-cloud-gcp/src/config.rs
    - crates/rollout-cloud-gcp/src/error.rs
    - crates/rollout-cloud-gcp/src/gcs/mod.rs
    - crates/rollout-cloud-gcp/src/gcs/put_stream.rs
    - crates/rollout-cloud-gcp/src/gcs/get_stream.rs
    - crates/rollout-cloud-gcp/src/pubsub/mod.rs
    - crates/rollout-cloud-gcp/src/pubsub/lease.rs
    - crates/rollout-cloud-gcp/src/secret_manager/mod.rs
    - crates/rollout-cloud-gcp/src/mds/mod.rs
    - crates/rollout-cloud-gcp/tests/{conformance,gcs_resumable_upload_preempted_mid_stream_reuploads_cleanly,put_stream_content_id_matches_post_retry}.rs
    - crates/rollout-cloud-gcp/tests/support/{mod,mock_secret_manager,mock_mds}.rs
    - crates/rollout-cloud-gcp/docs/bucket-setup.md
    - crates/rollout-cloud-gcp/README.md
    - docs/book/src/cloud/gcp.md
  modified:
    - Cargo.toml
    - Cargo.lock
    - crates/rollout-cloud-gcp/{Cargo.toml,src/lib.rs}
    - crates/rollout-cli/{Cargo.toml,src/cloud_factory.rs}
    - docker-compose.test.yml
    - .github/workflows/ci.yml
    - docs/book/src/SUMMARY.md

key-decisions:
  - "Used the verified yoshidan `gcloud-*` cohort (storage 1.3 / pubsub 1.7 / auth 1.3 / metadata 1.0 / googleapis 1.3), NOT the planned `gcloud-secretmanager-v1` + `gcloud_auth::credentials::mds::Client` — those names belong to the separate, not-yet-stabilized official googleapis cohort that pulls an incompatible tonic/gax tree."
  - "Secret Manager reached over its v1 REST API via the cohort's reqwest (the gcloud-* cohort ships no Secret Manager client) — keeps the dep tree slim and the public-api-cloud-leak gate trivially green."
  - "GceMetadataComputeHint built on gcloud-metadata's METADATA_* constants + a small reqwest GET; the SDK owns the metadata host string so the forbidden-patterns gate stays green with zero raw metadata.google.internal in source."
  - "PubSubQueue stores the whole ReceivedMessage in the inflight table (its ack/nack/modify_ack_deadline are self-methods carrying the SubscriberClient), so ack/nack with a QueueItemId-only handle works without rebuilding a client."
  - "cloud-live-gcp gated schedule-only (nightly cron) — mirrors the 05-05 cloud-live-aws decision; the plan's changed_files path filter is not reliably available in github.event."

patterns-established:
  - "Pattern 3 (RESEARCH): per-trait GCP SDK call mapping centralized in error.rs"
  - "Pattern 4 (RESEARCH): docker-compose.test.yml + always-on emulator CI job; in-test mock for SM (no first-party emulator)"
  - "Pattern 6 (RESEARCH): blake3 incremental hash-before-chunk, GCP resumable variant"

requirements-completed: [CLOUD-02, DOCS-01, DOCS-02, DOCS-03]

# Metrics
duration: 27min
completed: 2026-05-28
---

# Phase 5 Plan 06: Stage 3 — rollout-cloud-gcp Implementation Summary

**Second cloud-provider adapter: GCS streaming `ObjectStore` (resumable upload + blake3-hash-before-chunk + temp-then-rename, zero `upload_id` persistence), Pub/Sub lease `Queue` (`modify_ack_deadline` + inflight `ReceivedMessage` table), read-only Secret Manager `SecretStore` over the v1 REST API, and a GCE-MDS `ComputeHint` — all behind a default-off `gcp` feature, gated by an always-on fake-gcs-server + pubsub-emulator CI job and two Pitfall-prevention fixtures, with zero GCP SDK types leaking into `rollout-core` and the `gcp ↮ aws` invariant intact.**

## Performance

- **Duration:** ~27 min
- **Tasks:** 5
- **Files modified/created:** 27 (+2556/-12 across the 5 plan commits)

## Accomplishments
- `GcsObjectStore` implements all five `ObjectStore` methods over `gcloud-storage`. `put_stream` opens a resumable session, hashes each chunk with blake3 BEFORE `upload_multiple_chunk` (so `ContentId` is retry-stable — Pitfall 16), uploads to a `temp/pending-<ulid>` key, then server-side `copy_object` to the sharded content-addressed key (`<prefix>cas/ab/cd/<hex>`, FsObjectStore/S3 parity) + `delete_object`. The resumable session URL is created, used, and discarded within the single call — never persisted across processes (Pitfall 5); a preempted worker re-uploads from byte 0 idempotently.
- `PubSubQueue` implements all six `Queue` methods. `enqueue` publishes via `Topic::new_publisher().publish()`; `dequeue`/`dequeue_with_lease` `Subscription::pull(1)`; `ack` acknowledges; `nack` and `extend_lease` call `modify_ack_deadline` (`nack` with deadline 0 for immediate redelivery). An `Arc<Mutex<HashMap<QueueItemId, ReceivedMessage>>>` inflight table bridges the trait's `QueueItemId`-only `ack`/`nack` to Pub/Sub's `ack_id`, with a stale-token guard on `extend_lease`.
- `SecretManagerSecretStore` enforces the allowlist at the trait boundary (before any HTTP call) and reads the Secret Manager v1 `versions/latest:access` REST endpoint; `from_adc` resolves the bearer token via gcloud-auth; `put` always returns `Fatal::ConfigInvalid` ("read-only in v1.1").
- `GceMetadataComputeHint` reads `instance/machine-type` and `instance/preempted` from the GCE metadata server (`preempted == "TRUE"` -> `Some(30s)`); the metadata host + `Metadata-Flavor: Google` header come from `gcloud-metadata` constants, so the source never writes the raw metadata host and the `forbidden-patterns` gate stays green. CPU/mem/GPU inventory delegates to the local platform hint.
- `error.rs` collapses every SDK error to `CoreError` by rendered-string classification — no SDK type appears in any public signature or `#[source]` chain, so `public-api-cloud-leak` stays trivially green (`rollout-core`'s dependency tree has zero `gcloud_*`/`google_cloud_*` crates).
- `cloud-emulator-gcp` always-on CI job runs the GCS + Pub/Sub conformance + 2 fixtures against fake-gcs-server 1.50.2 + the pubsub-emulator; the Secret Manager + MDS suites run Docker-free via in-test hyper mocks on every build. `cloud-live-gcp` nightly job runs the same against real GCP via WIF. `rollout-cli` gains a default-off `gcp` feature wired through `cloud_factory::build_gcp_runtime`, composable with `aws`.

## Task Commits

1. **Task 1: GcsObjectStore + resumable upload + blake3-incremental-hash + SDK deps + bucket-setup docs** — `b78e72c` (feat)
2. **Task 2: PubSubQueue + dequeue_with_lease + extend_lease (modify_ack_deadline)** — `681da50` (feat)
3. **Task 3: SecretManagerSecretStore + in-test mock secret manager** — `4748d3d` (feat)
4. **Task 4: GceMetadataComputeHint via GCE MDS reader** — `3dab598` (feat)
5. **Task 5: cloud-emulator-gcp/cloud-live-gcp CI + rollout-cli gcp feature + cloud_factory + mdBook** — `848c765` (feat)

_TDD note: per-trait tasks landed impl + tests in one feat commit each. The fake-gcs-server / pubsub-emulator conformance + fixture tests are `#[ignore]`'d (can't RED locally without Docker); the Secret Manager mock tests (4), the MDS mock tests (4), the error-mapping unit tests, and the retry-hash witness run RED->GREEN in the default loop and pass on every CI build._

## Decisions Made
- **SDK cohort substitution (the headline deviation):** the plan named `gcloud-secretmanager-v1` and `gcloud_auth::credentials::mds::Client`. Those belong to the official `googleapis/google-cloud-rust` cohort, which is not on the same release train as the `gcloud-*` storage/pubsub crates and pulls an incompatible tonic/gax tree. I used the verified, mutually-compatible yoshidan `gcloud-*` cohort and filled the two gaps it leaves: Secret Manager over v1 REST, GCE MDS over `gcloud-metadata`.
- **Secret Manager over REST:** the cohort ships no SM client; the v1 REST `access` endpoint is a 1-request read, so a thin reqwest call (allowlist-guarded, token via ADC) is simpler and lighter than adopting a second SDK family.
- **MDS via gcloud-metadata constants:** keeps the raw metadata host out of our source (`forbidden-patterns` green) while the SDK supplies the `Metadata-Flavor: Google` header — the mock MDS test asserts the header is present on every request.
- **PubSubQueue inflight stores the full ReceivedMessage:** its ack/nack/modify_ack_deadline are self-methods carrying the SubscriberClient, so the trait's id-only ack/nack works without re-deriving a client.
- **cloud-live-gcp schedule-only trigger:** consistent with 05-05's cloud-live-aws; the changed_files path filter is unreliable in `github.event`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Substituted the GCP SDK cohort (planned crates do not exist as a compatible set)**
- **Found during:** Task 1 (SDK version verification)
- **Issue:** `cargo search` returned `gcloud-storage 1.3 / gcloud-pubsub 1.7 / gcloud-auth 1.3 / gcloud-metadata 1.0` (yoshidan cohort) but NO `gcloud-secretmanager-v1`; the only GCP secret-manager crate is `google-cloud-secretmanager-v1` from a different, tonic/gax-heavy cohort. `gcloud_auth::credentials::mds::Client` also does not exist in the available cohort.
- **Fix:** pinned the verified `gcloud-*` cohort; implemented Secret Manager over the v1 REST API (`reqwest`) and GCE MDS over `gcloud-metadata` constants. Documented in the workspace Cargo.toml, the crate README, and inline.
- **Files modified:** Cargo.toml, crates/rollout-cloud-gcp/Cargo.toml, src/secret_manager/mod.rs, src/mds/mod.rs
- **Commits:** b78e72c, 4748d3d, 3dab598

**2. [Rule 3 - Blocking] reqwest 0.13 TLS feature is `rustls`, not `rustls-tls`; gcloud-auth needs an explicit JWT backend**
- **Found during:** Task 1 (first `cargo build --features gcp`)
- **Issue:** the workspace dep used `reqwest` feature `rustls-tls` (which 0.13 doesn't have) and `gcloud-auth` errored `Enable one feature: jwt-aws-lc-rs OR jwt-rust-crypto` once defaults were off.
- **Fix:** `reqwest` -> `rustls`; added `jwt-aws-lc-rs` to `gcloud-storage` + `gcloud-auth` feature sets.
- **Commit:** b78e72c

**3. [Rule 3 - Blocking] Added gcloud-googleapis + token-source as direct deps**
- **Found during:** Tasks 2 & 5
- **Issue:** `PubsubMessage` lives in `gcloud-googleapis` (not re-exported by the pubsub crate); the `TokenSourceProvider` trait used by `from_adc` lives in the `token-source` crate (not `gcloud_auth::token_source`).
- **Fix:** added both to `[workspace.dependencies]` and the crate's gated `gcp` feature; imported `gcloud_googleapis::pubsub::v1::PubsubMessage` and `token_source::TokenSourceProvider`.
- **Commits:** 681da50, 848c765

### Unmet acceptance greps (consequence of the cohort substitution)
- `grep 'gcloud_auth::credentials::mds::Client|gcloud_auth.*mds'` in `src/mds/mod.rs` cannot match — that API does not exist in the available cohort. The equivalent capability is provided via `gcloud-metadata` constants; the four MDS tests (including the `Metadata-Flavor: Google` header assertion) pass.
- The plan's `cargo public-api` invocation is not run locally (`cargo-public-api` not installed, same as 05-04/05-05). The leak gate is satisfied **structurally**: `cargo tree -p rollout-core -e normal` shows zero `gcloud_*`/`google_cloud_*` crates, and dep-direction invariant #14 enforces it in CI.

**Total deviations:** 3 blocking (all required to compile against the real, published SDK set). No scope creep; no architectural change to the trait surface — only the SDK adapter behind it.

## Issues Encountered
- `cargo-public-api` is not installed locally; leak gate satisfied structurally (zero SDK crates in `rollout-core`'s dependency closure) + dep-direction invariant #14 in CI.
- The pubsub-emulator image has no auto-start entrypoint; the CI job overrides it with an explicit `docker run … gcloud beta emulators pubsub start`.

## User Setup Required
- **Repo operator:** add `cloud-emulator-gcp` to branch-protection required checks. For `cloud-live-gcp`, configure the Workload Identity Federation trust and the repo vars `ROLLOUT_CLOUD_LIVE_GCP_WIF_PROVIDER`, `ROLLOUT_CLOUD_LIVE_GCP_SA`, `ROLLOUT_CLOUD_LIVE_GCP_{BUCKET,TOPIC,SUBSCRIPTION,PROJECT}`, and grant the IAM roles in `crates/rollout-cloud-gcp/docs/bucket-setup.md`.

## Next Phase Readiness
- The GCP adapter is ready for Plan 05-07 (snapshot streaming witnesses can target `GcsObjectStore::put_stream`/`get_stream`, e.g. `bit_identical_resume_at_step_5_via_gcs`) and 05-08 (`rollout cloud doctor` can consume `cloud_factory::build_gcp_runtime`).
- Both cloud adapters now compose in `rollout-cli` (`--features aws,gcp`), with the factory dispatching on the TOML `[cloud].provider`.

---
*Phase: 05-cloud-layer-object-store-snapshots*
*Completed: 2026-05-28*

## Self-Check: PASSED

All 5 task commits (b78e72c, 681da50, 4748d3d, 3dab598, 848c765) exist; all created files verified present on disk.
