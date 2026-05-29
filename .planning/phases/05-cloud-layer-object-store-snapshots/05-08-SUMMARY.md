---
phase: 05-cloud-layer-object-store-snapshots
plan: 08
subsystem: cli
tags: [cloud, doctor, cli, clap, aws, gcp, localstack, fake-gcs-server, pubsub-emulator, tokio-rustls, blake3, ci-gates]

# Dependency graph
requires:
  - phase: 05-05-stage2-cloud-aws-impl
    provides: S3ObjectStore + SqsQueue + SecretsManagerSecretStore + Ec2MetadataComputeHint behind the `aws` feature
  - phase: 05-06-stage3-cloud-gcp-impl
    provides: GcsObjectStore + PubSubQueue + SecretManagerSecretStore + GceMetadataComputeHint behind the `gcp` feature; cloud_factory::build_gcp_runtime; load_gcs_client_with_endpoint
  - phase: 05-04-stage1-trait-extensions-ci-gates
    provides: ObjectStore::put_stream/get_stream + Queue::dequeue_with_lease + CloudConfig::{Aws,Gcp} tagged enum + validate_cross_fields
  - phase: 05-07-snapshot-streaming-witnesses
    provides: example TOMLs (sft-tiny-aws/gcp) + always-on cloud-emulator-{aws,gcp} CI shape this plan extends
provides:
  - rollout-cli `rollout cloud doctor --provider <aws|gcp> --config <toml> [--format <human|json>]` subcommand
  - 7 named checks (reachability, auth, object_store, queue, secret_store, compute_hint, content_id_roundtrip) over the four cloud traits
  - Unix exit codes 0/1/2 (D-DOCTOR-03) + human (colored) + json output (D-DOCTOR-02)
  - emulator-aware cloud_factory (AWS_ENDPOINT_URL/LOCALSTACK_ENDPOINT + STORAGE/PUBSUB_EMULATOR_HOST overrides)
  - doctor_smoke integration tests wired into cloud-emulator-{aws,gcp} CI jobs (always-on, no live cloud)
  - mdBook chapter docs/book/src/cloud/doctor.md
affects: [06-multi-node-distribution, distribution, operator-tooling]

# Tech tracking
tech-stack:
  added: [tokio-rustls 0.26 (ring), webpki-roots 1.0]
  patterns:
    - "CLI subcommand group nested under src/commands/cloud/doctor/* (new layout; older Phase 2-4 subcommands stay flat at src/*.rs)"
    - "Doctor `run(args) -> !` is the sole sanctioned std::process::exit site (CLI binary, AGENTS.md §8); exit 0/1/2 per D-DOCTOR-03"
    - "Internal `_doctor` Cargo feature (enabled by `aws`|`gcp`) gates TLS-probe + blake3 deps so default build stays SDK-free"
    - "Emulator-aware factory: same production code path, honors *_ENDPOINT_URL / *_EMULATOR_HOST env overrides so doctor smoke runs end-to-end with no live cloud"
    - "TCP+TLS reachability probe via tokio-rustls + webpki-roots to surface DNS/firewall distinctly from auth"

key-files:
  created:
    - crates/rollout-cli/src/commands/mod.rs
    - crates/rollout-cli/src/commands/cloud/mod.rs
    - crates/rollout-cli/src/commands/cloud/doctor/mod.rs
    - crates/rollout-cli/src/commands/cloud/doctor/checks.rs
    - crates/rollout-cli/src/commands/cloud/doctor/config.rs
    - crates/rollout-cli/src/commands/cloud/doctor/output/mod.rs
    - crates/rollout-cli/src/commands/cloud/doctor/output/human.rs
    - crates/rollout-cli/src/commands/cloud/doctor/output/json.rs
    - crates/rollout-cli/tests/doctor_smoke.rs
    - docs/book/src/cloud/doctor.md
  modified:
    - crates/rollout-cli/src/main.rs
    - crates/rollout-cli/src/cloud_factory.rs
    - crates/rollout-cli/Cargo.toml
    - Cargo.toml
    - Cargo.lock
    - docs/book/src/SUMMARY.md
    - .github/workflows/ci.yml
    - crates/rollout-cloud-aws/src/lib.rs
    - crates/rollout-cloud-aws/src/sqs/mod.rs
    - crates/rollout-cloud-gcp/src/lib.rs
    - crates/rollout-cloud-gcp/src/pubsub/mod.rs
    - crates/rollout-cloud-gcp/src/mds/mod.rs
    - crates/rollout-cloud-gcp/src/secret_manager/mod.rs

key-decisions:
  - "Auth check (#2) is a credential-chain probe via the runtime's compute_hint.inventory() rather than adding an aws-sdk-sts dep — keeps the dep tree slim while still surfacing a broken credential chain."
  - "Added an internal `_doctor` feature (enabled transitively by aws|gcp) so tokio-rustls/webpki-roots/blake3 only compile when a cloud feature is on; the default workspace build stays SDK-free."
  - "Made cloud_factory emulator-aware (honors AWS_ENDPOINT_URL/LOCALSTACK_ENDPOINT + STORAGE/PUBSUB_EMULATOR_HOST) so the doctor binary runs end-to-end against localstack + fake-gcs-server + pubsub-emulator with no live cloud and no separate test-only client wiring."
  - "GCP Secret Manager has no first-party emulator; in emulator mode the factory substitutes the env-backed local SecretStore so runtime construction does not fail on absent ADC — the GCP doctor smoke seeds ROLLOUT_SECRET_DOCTOR_SECRET."

patterns-established:
  - "RESEARCH Pattern 10: rollout cloud doctor — 7 checks + 2 output formats + 3 exit codes over build_cloud_runtime"
  - "Doctor smoke tests #[ignore]'d for the Docker-free loop; cloud-emulator-{aws,gcp} CI opts in via --include-ignored after pre-creating resources"

requirements-completed: [CLOUD-04, DOCS-01, DOCS-02, DOCS-03]

# Metrics
duration: 40min
completed: 2026-05-28
---

# Phase 5 Plan 08: Stage 5 — `rollout cloud doctor` Summary

**Operator pre-flight CLI `rollout cloud doctor --provider <aws|gcp> --config <toml> [--format human|json]` that exercises all four cloud traits via the existing `build_cloud_runtime` through 7 named checks (incl. a 64 MiB `put_stream`/`get_stream` blake3 roundtrip), with colored + JSON output and Unix exit codes 0/1/2 — proven end-to-end against localstack + fake-gcs-server + pubsub-emulator on every PR, and closing out the phase by also resolving two pre-existing CI-gate failures from 05-06.**

## Performance

- **Duration:** ~40 min
- **Tasks:** 2 (plus the mandated phase-closeout cleanup)
- **Files created:** 10; **modified:** 13

## Accomplishments
- `rollout cloud doctor` ships end-to-end: clap surface (`Cmd::Cloud(CloudCmd)` → `CloudSub::Doctor(DoctorArgs)`), 7 named checks, human (colored `✓`/`✗` + per-check latency + summary) + JSON (`{checks:[...], summary:{pass_count,fail_count,total_latency_ms}}`) output, and exit codes 0 (all pass) / 1 (any fail) / 2 (invocation/config) per D-DOCTOR-03. Checks 3-6 run concurrently via `tokio::join!`; check 7 forces the multipart/resumable path with a 64 MiB buffer and verifies the blake3 `ContentId`.
- TOML-config-only source (D-DOCTOR-04): `--provider` must match `[cloud].provider` or doctor exits 2; cross-field validation runs at load.
- Made `cloud_factory` emulator-aware so the same production code path runs the doctor against localstack (S3 path-style + test creds) and fake-gcs-server + pubsub-emulator (anonymous), keyed off standard env overrides.
- `doctor_smoke.rs`: 8 test functions — AWS/GCP all-pass (`#[ignore]`), unreachable→exit-1 (`#[ignore]`), provider-mismatch→exit-2, bad-config→exit-2, human + JSON shape (`#[ignore]`), and an always-on `--help` golden. The 3 config-layer/help tests run Docker-free on every PR; the 5 emulator tests run via `--include-ignored` in CI after the jobs pre-create the bucket/queue/secret (AWS) and bucket/topic/subscription (GCP).
- mdBook chapter `cloud/doctor.md` (checks, exit codes, output schema, limitations, CI coverage) + SUMMARY nav entry.
- **Phase closeout cleanup:** resolved the two pre-existing CI-gate failures introduced by 05-06 (feature-gated, default-feature hooks missed them): rustfmt drift in `rollout-cloud-gcp` (`cargo fmt -p rollout-cloud-gcp`, 11 hunks) and the workspace rustdoc gate (feature-gated `[error]`/`[lease]` intra-doc links in cloud-aws + cloud-gcp replaced with inline prose). Both `deferred-items.md` entries marked RESOLVED.

## Task Commits

1. **Phase-closeout cleanup (05-06 gate drift)** — `7ed055f` (fix)
2. **Task 1: CLI surface + 7 checks + 2 output formats + unit tests** — `fe71675` (feat)
3. **Task 2: doctor smoke tests + emulator-aware factory + mdBook + CI** — `410cfe6` (test)

**Plan metadata:** see final docs commit.

## Files Created/Modified
- `crates/rollout-cli/src/commands/cloud/doctor/mod.rs` — `DoctorArgs` + `run(args) -> !` dispatch + exit codes
- `crates/rollout-cli/src/commands/cloud/doctor/checks.rs` — 7 named check fns + concurrent 3-6 + 64 MiB roundtrip
- `crates/rollout-cli/src/commands/cloud/doctor/config.rs` — TOML load + provider-match
- `crates/rollout-cli/src/commands/cloud/doctor/output/{human,json}.rs` — two renderers (testable `render()` + `print()`)
- `crates/rollout-cli/tests/doctor_smoke.rs` — CARGO_BIN_EXE invocation smoke + exit-code/JSON assertions
- `crates/rollout-cli/src/cloud_factory.rs` — emulator-aware AWS/GCP runtime construction
- `crates/rollout-cli/src/main.rs` — `Cmd::Cloud(CloudCmd)` + `mod commands` + `cloud_dispatch`
- `.github/workflows/ci.yml` — pre-create + doctor-smoke steps in cloud-emulator-{aws,gcp}
- `docs/book/src/cloud/doctor.md` + `SUMMARY.md` — operator playbook + nav
- cloud-aws/gcp crate docs + gcp fmt — phase-closeout gate cleanup

## Decisions Made
- **Auth check via inventory probe, not aws-sdk-sts.** The plan sketched an STS `GetCallerIdentity` for AWS; rather than pull a new SDK crate, check #2 reuses the runtime's `compute_hint.inventory()` (which exercises the resolved credential chain) — slimmer dep tree, still surfaces broken creds.
- **`_doctor` internal feature.** TLS-probe + blake3 deps are gated behind an internal `_doctor` feature enabled by `aws`|`gcp`, keeping the default SDK-free build untouched.
- **Emulator-aware factory over test-only clients.** Honoring `AWS_ENDPOINT_URL`/`LOCALSTACK_ENDPOINT` + `STORAGE`/`PUBSUB_EMULATOR_HOST` in the real factory means the doctor binary exercises the production path against emulators — no parallel test wiring, and the smoke test proves the real dispatch.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] cloud_factory was not emulator-aware; doctor all-pass smoke could not reach localstack / fake-gcs-server**
- **Found during:** Task 2 (designing the all-pass smoke tests)
- **Issue:** `build_cloud_runtime` built production clients only — `aws_config::defaults` ignores `LOCALSTACK_ENDPOINT`, S3 needed path-style addressing, GCS `with_auth()`/PubSub `.with_auth()` fail without ADC on emulators. Without an endpoint override the plan's truths #1/#2 ("exits 0 on a green localstack / fake-gcs-server") were unreachable.
- **Fix:** Added emulator overrides to `build_aws_runtime` (AWS_ENDPOINT_URL/LOCALSTACK_ENDPOINT → `.endpoint_url().test_credentials()` + S3 `force_path_style(true)`) and `build_gcp_runtime` (STORAGE_EMULATOR_HOST → `load_gcs_client_with_endpoint`; PUBSUB_EMULATOR_HOST → anonymous `PubSubClientConfig::default()`; emulator-mode SecretStore falls back to the env-backed local store since GCP SM has no emulator). Production paths unchanged (overrides absent).
- **Files modified:** crates/rollout-cli/src/cloud_factory.rs
- **Verification:** doctor_smoke non-ignored tests pass; build + clippy green with `aws,gcp`; CI jobs pre-create resources + set the overrides.
- **Committed in:** 410cfe6

**2. [Rule 2 - Missing Critical] Auth check needed a real probe, not the plan's `Ok(())` placeholder**
- **Found during:** Task 1 (check implementations)
- **Issue:** The plan's `check_auth` was a literal `Ok(())` placeholder — it would always pass even with broken credentials, defeating the check's purpose (D-DOCTOR-01 step 2).
- **Fix:** Implemented check #2 as a credential-chain probe via `compute_hint.inventory()`.
- **Files modified:** crates/rollout-cli/src/commands/cloud/doctor/checks.rs
- **Verification:** check runs; surfaces failure when the runtime's credential chain is broken.
- **Committed in:** fe71675

**3. [Rule 3 - Blocking] `rollout-cli` is a binary-only crate — the plan's `--lib` test invocation does not apply**
- **Found during:** Task 1 verification
- **Issue:** Plan acceptance used `cargo test -p rollout-cli --lib ...`; `rollout-cli` has no lib target, so `--lib` errors.
- **Fix:** Ran the same unit tests via the bin target: `cargo test -p rollout-cli --features 'aws,gcp' --bin rollout commands::cloud::doctor` (9 tests pass).
- **Verification:** 9 unit tests green (≥8 required).
- **Committed in:** fe71675

**4. [Rule 1 - Bug] Plan sketch used non-existent `ContentId::from(blake3::hash(..))`**
- **Found during:** Task 1 (check 7)
- **Issue:** `ContentId` has no `From<blake3::Hash>`; the API is `ContentId(*blake3::hash(&buf).as_bytes())` / `ContentId::of`.
- **Fix:** Used `ContentId(*blake3::hash(&buf).as_bytes())` to match the streamed `put_stream` hash.
- **Files modified:** crates/rollout-cli/src/commands/cloud/doctor/checks.rs
- **Committed in:** fe71675

---

**Total deviations:** 4 (2 blocking, 1 missing-critical, 1 bug). **Impact:** all necessary for a working, honest doctor; no scope creep, no trait-surface changes.

## Issues Encountered
- clippy pedantic flagged `cast_sign_loss` (64 MiB buffer fill) and `uninlined_format_args` (smoke TOML builders) + several `doc_markdown` backticks — all fixed inline (`u8::try_from`, inlined `{ALGO_BLOCK}`, backticked identifiers).
- The phase-closeout cleanup commit (code under `crates/`) satisfies docs-test-policy via the inline rustdoc edits on the changed files; Task 1/2 satisfy it via unit + integration tests.

## User Setup Required
None new. The existing repo-operator setup for the cloud-emulator/cloud-live CI jobs (branch protection, WIF/IAM) carries over; the doctor smoke steps run inside the already-configured emulator jobs.

## Next Phase Readiness
- Phase 5 (CLOUD-01..04) is complete: AWS + GCP adapters, object-store-backed snapshots, and the `rollout cloud doctor` operator tool, all with always-on emulator-backed CI gates green and the two 05-06 gate-drift items resolved.
- Phase 6 (multi-node distribution) can lean on `cloud doctor` as the pre-flight gate before a real multi-host run.

---
*Phase: 05-cloud-layer-object-store-snapshots*
*Completed: 2026-05-28*

## Self-Check: PASSED

All 7 created files verified present on disk; all 3 task commits (7ed055f, fe71675, 410cfe6) exist in the git log.
