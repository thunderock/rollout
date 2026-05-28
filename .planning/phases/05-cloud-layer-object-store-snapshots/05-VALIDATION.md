---
phase: 5
slug: cloud-layer-object-store-snapshots
status: planned
nyquist_compliant: true
wave_0_complete: false
created: 2026-05-28
last_updated: 2026-05-28
---

# Phase 5 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Populated by the planner from RESEARCH.md §Validation Architecture.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` (Rust workspace) + `cargo deny` + `cargo public-api` + Docker (localstack + fake-gcs-server + pubsub-emulator) |
| **Config file** | `Cargo.toml` (workspace), `deny.toml`, `xtask/src/architecture_lint.rs`, `crates/rollout-core/tests/dependency_direction.rs`, `docker-compose.test.yml`, `.github/workflows/ci.yml` |
| **Quick run command** | `cargo test --workspace --tests` |
| **Full suite command** | `cargo test --workspace --tests --features aws,gcp && cargo deny check && cargo public-api -p rollout-core --simplified \| bash scripts/check-public-api-cloud-leak.sh /dev/stdin && bash scripts/check-forbidden-patterns.sh && cargo test -p rollout-core --test dependency_direction` |
| **Estimated runtime** | ~90 seconds (quick, Docker-free) / ~6 minutes (full incl. emulator jobs) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --workspace --tests` (default features, no Docker).
- **After every plan wave:** Run full suite incl. cloud-emulator-aws + cloud-emulator-gcp jobs on a Docker-enabled runner.
- **Before `/gsd:verify-work`:** Full suite green incl. `bit_identical_resume_at_step_5_via_{s3,gcs}` + `snapshot_resume_s3_to_gcs_via_manual_copy` witnesses + doctor smoke.
- **Max feedback latency:** 90s (default `cargo test`); 6 min (full).

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 05-01 / Task 1 | 01 | 1 | precursor | unit | `cargo test -p rollout-core --lib validate_for_postgres` | ❌ W0 new (crates/rollout-core/src/traits/storage.rs gains validate_for_postgres impl) | ⬜ pending |
| 05-01 / Task 2 | 01 | 1 | precursor | integration | `cargo test -p rollout-storage --features postgres --test postgres_integration -- --include-ignored --test-threads=1 pg_scan_bytes_ascii_only_round_trip pg_put_bytes_rejects_nonprintable_path pg_scan_bytes_rejects_nonprintable_prefix` | ✅ existing (crates/rollout-storage/tests/postgres_integration.rs from Phase 4) — add 3 tests | ⬜ pending |
| 05-01 / Task 3 | 01 | 1 | precursor | property (proptest, 256 cases) | `cargo test -p rollout-storage --features postgres --test postgres_scan_bytes_parity -- --include-ignored --test-threads=1` | ❌ W0 new (crates/rollout-storage/tests/postgres_scan_bytes_parity.rs) | ⬜ pending |
| 05-02 / Task 1 | 02 | 1 | precursor | lint (architecture-lint) | `cargo test -p rollout-core --test dependency_direction` | ✅ existing | ⬜ pending |
| 05-03 / Task 1 | 03 | 1 | precursor | smoke (build + test + clippy + deny on 1.91 spike) | manual: `cargo +1.91 test --workspace --tests && cargo +1.91 clippy --workspace --all-targets --all-features -- -D warnings && cargo +1.91 deny check` | ❌ W0 new (.planning/research/PRECURSOR-C-MSRV-DECISION.md) | ⬜ pending |
| 05-03 / Task 2 | 03 | 1 | precursor | checkpoint:decision (user sign-off) | manual: user selects BUMP or STAY | n/a | ⬜ pending |
| 05-03 / Task 3 | 03 | 1 | precursor | smoke (full CI matrix post-decision) | `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace --tests && cargo deny check` | ✅ existing (rust-toolchain.toml + Cargo.toml + .github/workflows/ci.yml) | ⬜ pending |
| 05-04 / Task 1 | 04 | 2 | infra (foundation for CLOUD-01..04) | unit | `cargo test -p rollout-core --lib cloud` (9 tests: 4 trait default impls + 5 CloudConfig serde/validators) + `cargo xtask schema-gen && git diff --exit-code schemas/ python/` | ❌ W0 new (crates/rollout-core/src/config/cloud.rs + 4 new trait methods + LeaseToken on crates/rollout-core/src/traits/cloud.rs) | ⬜ pending |
| 05-04 / Task 2 | 04 | 2 | infra | unit (6 tests on rollout-cloud-local overrides) | `cargo test -p rollout-cloud-local --tests` (must not emit #[deprecated] warnings) | ✅ existing — overrides added to crates/rollout-cloud-local/src/{object_store,queue}.rs | ⬜ pending |
| 05-04 / Task 3 | 04 | 2 | infra (D-CI-04 dep-direction invariants #11-14) | lint (architecture-lint) | `cargo test -p rollout-core --test dependency_direction` (14 invariants + 4 new deliberate-violation tests) | ❌ W0 new (4 violation fixture crates under crates/rollout-core/tests/fixtures/violation_* + stub crates/rollout-cloud-{aws,gcp}/) | ⬜ pending |
| 05-04 / Task 4 | 04 | 2 | infra (D-CI-03 CI gates) | lint (custom CI gates) | `bash scripts/check-public-api-cloud-leak.sh <(cargo public-api -p rollout-core --simplified)` + `bash scripts/check-forbidden-patterns.sh` | ❌ W0 new (scripts/check-public-api-cloud-leak.sh + scripts/check-forbidden-patterns.sh + 2 new CI jobs in .github/workflows/ci.yml) | ⬜ pending |
| 05-05 / Task 1 | 05 | 3 | CLOUD-01 | integration (cloud-emulator-aws CI job) + 4 fixture tests | `cargo test -p rollout-cloud-aws --features aws --tests -- --include-ignored --test-threads=1` (with LOCALSTACK_ENDPOINT set) | ❌ W0 new (crates/rollout-cloud-aws/src/s3/{mod,put_stream,get_stream}.rs + 4 test files + bucket-setup.md) | ⬜ pending |
| 05-05 / Task 2 | 05 | 3 | CLOUD-01 | integration (cloud-emulator-aws) — 6 SQS tests | `cargo test -p rollout-cloud-aws --features aws --tests sqs -- --include-ignored` | ❌ W0 new (crates/rollout-cloud-aws/src/sqs/{mod,lease}.rs + tests) | ⬜ pending |
| 05-05 / Task 3 | 05 | 3 | CLOUD-01 | integration (cloud-emulator-aws) — 4 SecretsManager tests | `cargo test -p rollout-cloud-aws --features aws --tests secrets_manager -- --include-ignored` | ❌ W0 new (crates/rollout-cloud-aws/src/secrets_manager/mod.rs + tests) | ⬜ pending |
| 05-05 / Task 4 | 05 | 3 | CLOUD-01 | integration (mock IMDS in-test) | `cargo test -p rollout-cloud-aws --features aws --tests imds` (no external Docker; mock IMDS bound to 127.0.0.1) | ❌ W0 new (crates/rollout-cloud-aws/src/imds/mod.rs + tests/imds_v1_disabled_falls_back_gracefully.rs) | ⬜ pending |
| 05-05 / Task 5 | 05 | 3 | CLOUD-01 + infra | smoke (CI job + CLI feature compile) | `cargo build -p rollout-cli --features aws` + `mdbook build docs/book` | ❌ W0 new (docker-compose.test.yml + cloud-emulator-aws / cloud-live-aws CI jobs + crates/rollout-cli/src/cloud_factory.rs build_aws_runtime + docs/book/src/cloud/aws.md) | ⬜ pending |
| 05-06 / Task 1 | 06 | 3 | CLOUD-02 | integration (cloud-emulator-gcp CI job) + 2 fixture tests | `cargo test -p rollout-cloud-gcp --features gcp --tests -- --include-ignored` (with STORAGE_EMULATOR_HOST set) | ❌ W0 new (crates/rollout-cloud-gcp/src/gcs/{mod,put_stream,get_stream}.rs + tests + bucket-setup.md + README.md) | ⬜ pending |
| 05-06 / Task 2 | 06 | 3 | CLOUD-02 | integration (cloud-emulator-gcp) — 5 Pub/Sub tests | `cargo test -p rollout-cloud-gcp --features gcp --tests pubsub -- --include-ignored` | ❌ W0 new (crates/rollout-cloud-gcp/src/pubsub/{mod,lease}.rs) | ⬜ pending |
| 05-06 / Task 3 | 06 | 3 | CLOUD-02 | unit (in-test mock secret manager) — 4 tests | `cargo test -p rollout-cloud-gcp --features gcp --tests secret_manager` (no Docker needed) | ❌ W0 new (crates/rollout-cloud-gcp/src/secret_manager/mod.rs + tests/support/mock_secret_manager.rs) | ⬜ pending |
| 05-06 / Task 4 | 06 | 3 | CLOUD-02 | integration (mock MDS in-test) — 4 tests | `cargo test -p rollout-cloud-gcp --features gcp --tests mds` (no Docker; mock MDS bound to 127.0.0.1) | ❌ W0 new (crates/rollout-cloud-gcp/src/mds/mod.rs + tests/support/mock_mds.rs) | ⬜ pending |
| 05-06 / Task 5 | 06 | 3 | CLOUD-02 + infra | smoke (CI job + CLI feature compile) | `cargo build -p rollout-cli --features gcp` + `cargo build -p rollout-cli --features 'aws,gcp'` + `mdbook build docs/book` | ❌ W0 new (docker-compose.test.yml gcp services + cloud-emulator-gcp / cloud-live-gcp CI jobs + cloud_factory build_gcp_runtime + docs/book/src/cloud/gcp.md) | ⬜ pending |
| 05-07 / Task 1 | 07 | 4 | CLOUD-03 | integration (cloud-emulator-aws CI job) — MockBackend SFT witness via S3 | `cargo test -p rollout-snapshots --test bit_identical_resume_at_step_5_via_s3 -- --include-ignored` | ❌ W0 new (crates/rollout-snapshots/tests/bit_identical_resume_at_step_5_via_s3.rs + tests/support/mod.rs) | ⬜ pending |
| 05-07 / Task 2 | 07 | 4 | CLOUD-03 | integration (cloud-emulator-gcp CI job) — MockBackend SFT witness via GCS | `cargo test -p rollout-snapshots --test bit_identical_resume_at_step_5_via_gcs -- --include-ignored` | ❌ W0 new (crates/rollout-snapshots/tests/bit_identical_resume_at_step_5_via_gcs.rs) | ⬜ pending |
| 05-07 / Task 3 | 07 | 4 | CLOUD-03 (D-XPROV-01) | integration (cloud-emulator-aws + fake-gcs side-by-side) — cross-provider portability witness | `cargo test -p rollout-snapshots --test snapshot_resume_s3_to_gcs_via_manual_copy -- --include-ignored` | ❌ W0 new (crates/rollout-snapshots/tests/snapshot_resume_s3_to_gcs_via_manual_copy.rs) | ⬜ pending |
| 05-07 / Task 4 | 07 | 4 | CLOUD-03 | smoke (plan-time validation + mdBook build) | `cargo run -p rollout-cli --features aws -- plan --config examples/sft-tiny-aws.toml --dry-run` + `mdbook build docs/book` | ❌ W0 new (examples/sft-tiny-aws.toml + examples/sft-tiny-gcp.toml + docs/book/src/cloud/snapshots.md) | ⬜ pending |
| 05-08 / Task 1 | 08 | 5 | CLOUD-04 | unit (rollout-cli doctor module) — 8 tests | `cargo test -p rollout-cli --features 'aws,gcp' --lib commands::cloud::doctor` | ❌ W0 new (crates/rollout-cli/src/commands/cloud/doctor/{mod,checks,config,output/{mod,human,json}}.rs) | ⬜ pending |
| 05-08 / Task 2 | 08 | 5 | CLOUD-04 | smoke (CARGO_BIN_EXE doctor invocation) + integration (emulator-backed doctor) | `cargo test -p rollout-cli --features 'aws,gcp' --test doctor_smoke doctor_help_lists_all_flags` (always-on) + `cargo test -p rollout-cli --features 'aws,gcp' --test doctor_smoke -- --include-ignored` (cloud-emulator jobs) | ❌ W0 new (crates/rollout-cli/tests/doctor_smoke.rs + docs/book/src/cloud/doctor.md + CI pre-create steps in cloud-emulator-{aws,gcp} jobs) | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

Stage 1 trait extensions + dep-direction invariants #11-14 + `public-api-cloud-leak` + `forbidden-patterns` CI gates land BEFORE any cloud SDK crate (Plan 04).

- [ ] `crates/rollout-core/src/traits/cloud.rs` — `ObjectStore::put_stream`/`get_stream` + `Queue::dequeue_with_lease`/`extend_lease` + `LeaseToken` type with `#[deprecated]` default impls
- [ ] `crates/rollout-core/src/config/cloud.rs` — CloudConfig + AwsConfig + GcpConfig + plan-time validators
- [ ] `crates/rollout-core/tests/dependency_direction.rs` — 14 invariants (added #11-14)
- [ ] `crates/rollout-core/tests/fixtures/violation_{algo_uses_cloud_aws,algo_uses_cloud_gcp,cloud_aws_uses_gcp,core_pulls_sdk}/` — 4 new violation fixture crates
- [ ] `crates/rollout-cloud-aws/` + `crates/rollout-cloud-gcp/` — stub workspace members (Plan 04 creates skeleton; Plans 05/06 flesh out)
- [ ] `scripts/check-public-api-cloud-leak.sh` + `scripts/check-forbidden-patterns.sh` — executable CI gate scripts
- [ ] `.github/workflows/ci.yml` — `public-api-cloud-leak` + `forbidden-patterns` always-on jobs (14 → 16 jobs)
- [ ] `crates/rollout-cloud-local/src/{object_store,queue}.rs` — overrides for the 4 new trait methods (FsObjectStore streaming + InMemQueue lease)
- [ ] `schemas/rollout.schema.json` + `python/rollout/_config_stubs.pyi` — regenerated via `cargo xtask schema-gen` to include CloudConfig
- [ ] `docs/book/src/cloud/traits.md` — operator-facing chapter for the new trait surface

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Live AWS smoke (`make smoke-aws` or equivalent) | CLOUD-01 success criterion #1 (operator runs `rollout train sft --config examples/sft-tiny-aws.toml` against real AWS) | Requires real AWS account + IAM creds | `aws configure && rollout train sft --config examples/sft-tiny-aws.toml` |
| Live GCP smoke (`make smoke-gcp` or equivalent) | CLOUD-02 success criterion #1 (operator runs `rollout train sft --config examples/sft-tiny-gcp.toml` against real GCP) | Requires real GCP project + ADC creds + WIF setup | `gcloud auth application-default login && rollout train sft --config examples/sft-tiny-gcp.toml` |
| Cross-provider snapshot resume (real cloud) | D-XPROV-01 (operator-managed `aws s3 cp` + `gsutil cp` between real buckets, then resume) | Requires both AWS + GCP creds + actual cross-cloud transfer | `aws s3 cp s3://aws-bucket/cas/ab/cd/<rest> /tmp/blob && gsutil cp /tmp/blob gs://gcs-bucket/cas/ab/cd/<rest>` then `rollout train sft --config <gcp-toml> --resume <snapshot_id>` |
| Doctor against freshly-bootstrapped real cloud | CLOUD-04 success criterion (operator pre-deploy check) | Validates the entire operator workflow including IAM/WIF/bucket-lifecycle policy | `rollout cloud doctor --provider aws --config production.toml` |
| MSRV-1.91 spike outcome sign-off | Plan 03 precursor C checkpoint:decision | User judgment on BUMP vs STAY (Plan 03 Task 2) | User reviews `.planning/research/PRECURSOR-C-MSRV-DECISION.md` and selects option-a or option-b |
| `aws-lc-rs` license audit | Plan 05 Task 1 + Pitfall #14 | Legal review on the `ISC OR (Apache-2.0 AND OpenSSL)` triple before v1.1 public release | `cargo metadata --format-version 1 \| jq -r '.packages[] \| select(.name == "aws-lc-rs") \| .license'` paste into PR description; confirm `OpenSSL` stays in `[licenses].deny` |
| `gcloud-*` SDK exact-version verification | Plan 06 Task 1 + STACK.md Risk Flag #1 | Per RESEARCH.md, the gcloud monorepo cohort version is a PLACEHOLDER; planner verifies at integration time | `cargo search gcloud-storage gcloud-pubsub gcloud-secretmanager-v1 gcloud-auth` — paste latest stable version + publish date in PR |

---

## Validation Sign-Off

- [x] All tasks have `<verify>` with `<automated>` block (or explicit checkpoint declaration for the one decision checkpoint in Plan 03 Task 2)
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references (`scripts/check-*`, trait extensions, fixture crates, stub cloud crates)
- [x] No watch-mode flags
- [x] Feedback latency < 90s for quick, < 6 min for full
- [ ] `bit_identical_resume_at_step_5_via_s3` + `bit_identical_resume_at_step_5_via_gcs` both run on every CI commit (no live creds) — verified at Plan 07 completion
- [ ] `snapshot_resume_s3_to_gcs_via_manual_copy` runs on every CI commit (cross-provider portability witness) — verified at Plan 07 completion
- [ ] `public-api-cloud-leak` gate exits 0 (no AWS/GCP SDK symbols in `rollout-core` public API) — verified continuously from Plan 04 onward
- [ ] `forbidden-patterns` gate exits 0 (no `169.254.169.254` / `metadata.google.internal` / `shell=True` / `libc::fork(` outside designated paths) — verified continuously from Plan 04 onward
- [ ] `architecture-lint` reaches 14 invariants (with 4 new violation fixtures) — verified at Plan 04 completion
- [ ] `rollout cloud doctor` exit-code contract (0/1/2) witnessed end-to-end via doctor_smoke tests — verified at Plan 08 completion
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** pending phase execution.
