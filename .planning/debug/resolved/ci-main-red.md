---
status: awaiting_human_verify
trigger: "ci-main-red: 5 failing CI jobs on main — lint, test, unused-deps, deny, docs-build"
created: 2026-05-26T00:00:00Z
updated: 2026-05-26T09:42:00Z
---

## Current Focus

hypothesis: All 5 root causes addressed by 5 atomic commits
test: Local repro for each — `cargo fmt --check`, `cargo test`, `cargo machete`, `cargo deny check advisories`, `mdbook build docs/book`, plus `cargo clippy --workspace --all-targets -- -D warnings`
expecting: all locally green
next_action: human-verify on next push-to-main; if Pages-enablement still 404s, manual repo-settings opt-in required

## Symptoms

expected: All CI jobs on main are green
actual: 5 jobs fail (ci/lint, ci/test, ci/unused-deps, ci/deny, ci/docs-build)
errors: |
  - ci/lint: cargo fmt --check shows drift across rollout-algo-rm, rollout-algo-sft, rollout-backend-vllm, rollout-cli, rollout-coordinator, rollout-core, rollout-runtime-batch, rollout-snapshots, rollout-storage (broader than the orchestrator's preview)
  - ci/test: schema_drift::schema_json_matches_committed + schema_drift::python_stubs_match_committed panic at line 22 — `cargo xtask schema-gen` exits 2 because `datamodel-codegen` is not on PATH in the default test job (macos-14)
  - ci/unused-deps: cargo-machete flags 13 real crates + 9 violation_* fixtures
  - ci/deny: RUSTSEC-2025-0111 / CVE-2025-62518 — tokio-tar 0.3.1 unmaintained+unsound, transitively via testcontainers (dev-dep)
  - ci/docs-build: `Configure Pages` step returns `HttpError: Not Found` because GitHub Pages was never enabled on the repo
reproduction: |
  GitHub Actions run https://github.com/thunderock/rollout/actions/runs/26298884787
  Local: cargo fmt --all -- --check; cargo test --workspace --tests; cargo machete; cargo deny check advisories; mdbook build docs/book
started: known-since main; Pages enablement deferred to v1.0 tech_debt

## Eliminated

(none — all 5 hypotheses confirmed and addressed)

## Evidence

- timestamp: 2026-05-26T09:25:00Z
  checked: cargo fmt --all -- --check
  found: drift across 38 files (broader than orchestrator's preview list); root cause = scaffolding commits never ran fmt
  implication: cargo fmt --all is sufficient

- timestamp: 2026-05-26T09:28:00Z
  checked: schema_drift.rs test source + CI ci.yml `test` job
  found: test invokes `cargo xtask schema-gen` which shells out to `datamodel-codegen` (Python); the `test` job does NOT install Python tooling; the dedicated `schema-drift` CI job DOES install datamodel-code-generator==0.57.0 and runs `git diff --exit-code schemas/ python/` as the authoritative gate
  implication: mark the two tests #[ignore] in default test job; schema-drift job remains the authoritative gate

- timestamp: 2026-05-26T09:30:00Z
  checked: cargo machete output + per-crate grep for actual usage
  found: 13 real crates have ≥1 truly-unused dep (zero use/path/derive); rollout-proto's flagged deps are macro-codegen false positives; rollout-transport's humantime-serde is used via attribute-macro; violation_* fixtures intentionally declare the forbidden edge under test
  implication: remove truly-unused deps; ignore codegen + attribute-macro false-positives; ignore fixture deps with rationale comment

- timestamp: 2026-05-26T09:33:00Z
  checked: cargo deny check advisories + dependency tree
  found: tokio-tar reaches workspace only via testcontainers (dev-dep, `--features postgres`, all related tests `#[ignore]`d). No safe upgrade available.
  implication: add `ignore = ["RUSTSEC-2025-0111"]` to deny.toml [advisories] with rationale + track in v1.0 tech_debt

- timestamp: 2026-05-26T09:35:00Z
  checked: ci.yml `docs-build` job (lines 140-160) + actions/configure-pages docs
  found: `Configure Pages` step hits the Pages site API which returns 404 if Pages was never enabled on the repo
  implication: set `with: enablement: true` to let the action auto-enable Pages (works given the `pages: write` workflow permission is already set)

- timestamp: 2026-05-26T09:42:00Z
  checked: post-fix `cargo fmt --all -- --check`, `cargo test --workspace --tests`, `cargo machete`, `cargo deny check advisories`, `mdbook build docs/book`, `cargo clippy --workspace --all-targets -- -D warnings`
  found: all six commands exit 0; tests show 2 ignored (schema-drift tests, as designed); no regressions
  implication: ready for human-verify on next CI run

## Resolution

root_cause: |
  Five independent regressions on main:
  1. ci/lint:        rustfmt drift across 38 files (scaffolding commits never ran fmt).
  2. ci/test:        schema_drift tests require `datamodel-codegen` on PATH; the default macos-14 test job does not install it. The dedicated `schema-drift` job already handles this authoritatively.
  3. ci/unused-deps: 13 real crates carried scaffolded but never-used dependencies; violation_* fixtures intentionally declare forbidden edges; tonic-prost-build codegen + humantime-serde attribute macro are invisible to cargo-machete.
  4. ci/deny:        tokio-tar 0.3.1 (CVE-2025-62518) reached the workspace as a transitive dev-dep via testcontainers. No safe upgrade exists.
  5. ci/docs-build:  GitHub Pages was never enabled on the repo; `actions/configure-pages` returned 404 on the API call.

fix: |
  Five atomic commits (newest last):
  - c8dc8b9 fix(ci): cargo fmt --all to resolve rustfmt drift
  - e8568dd fix(ci): mark schema-drift tests #[ignore] in default test job
  - 6bfa330 chore(machete): remove unused deps + ignore macro/codegen false-positives
  - ab4b8dd chore(deny): ignore RUSTSEC-2025-0111 tokio-tar advisory (dev-only)
  - aef2f86 ci(docs-build): auto-enable Pages via configure-pages enablement: true

verification: |
  All six local checks exit 0:
  - cargo fmt --all -- --check
  - cargo clippy --workspace --all-targets -- -D warnings (matches CI lint job)
  - cargo test --workspace --tests (schema_drift tests now show as 2 ignored, as designed)
  - cargo machete
  - cargo deny check advisories
  - mdbook build docs/book

  Human-verify required: confirm the next push-to-main run is green at
  https://github.com/thunderock/rollout/actions. Specifically:
  - If `Configure Pages` step still 404s on first run, the org/account policy
    is blocking automatic Pages enablement — operator must manually enable
    Pages in repo Settings → Pages → Source: GitHub Actions.

files_changed:
  - crates/rollout-algo-rm/Cargo.toml
  - crates/rollout-algo-rm/src/algo.rs
  - crates/rollout-algo-rm/src/loss.rs
  - crates/rollout-algo-rm/tests/snapshot_resume.rs
  - crates/rollout-algo-sft/Cargo.toml
  - crates/rollout-algo-sft/src/algo.rs
  - crates/rollout-algo-sft/tests/happy_path.rs
  - crates/rollout-algo-sft/tests/snapshot_resume.rs
  - crates/rollout-backend-vllm/Cargo.toml
  - crates/rollout-backend-vllm/benches/throughput.rs
  - crates/rollout-backend-vllm/src/backend.rs
  - crates/rollout-backend-vllm/src/engine.rs
  - crates/rollout-backend-vllm/src/python_glue.rs
  - crates/rollout-backend-vllm/src/train.rs
  - crates/rollout-backend-vllm/tests/pyo3_bridge_smoke.rs
  - crates/rollout-backend-vllm/tests/qwen25_assistant_mask.rs
  - crates/rollout-backend-vllm/tests/snapshot_resume_live.rs
  - crates/rollout-backend-vllm/tests/vllm_generate.rs
  - crates/rollout-cli/Cargo.toml
  - crates/rollout-cli/src/infer.rs
  - crates/rollout-cli/src/main.rs
  - crates/rollout-cli/src/snapshot.rs
  - crates/rollout-cli/src/train.rs
  - crates/rollout-cli/src/worker.rs
  - crates/rollout-cli/tests/snapshot_subcommands.rs
  - crates/rollout-cloud-local/Cargo.toml
  - crates/rollout-coordinator/Cargo.toml
  - crates/rollout-coordinator/src/failure_scan.rs
  - crates/rollout-coordinator/src/heartbeat.rs
  - crates/rollout-coordinator/src/main.rs
  - crates/rollout-coordinator/src/run.rs
  - crates/rollout-coordinator/tests/failure_scan.rs
  - crates/rollout-core/Cargo.toml
  - crates/rollout-core/src/lib.rs
  - crates/rollout-core/tests/dependency_direction.rs
  - crates/rollout-core/tests/fixtures/violation/Cargo.toml
  - crates/rollout-core/tests/fixtures/violation_algo_uses_cloud/Cargo.toml
  - crates/rollout-core/tests/fixtures/violation_algo_uses_transport/Cargo.toml
  - crates/rollout-core/tests/fixtures/violation_backend_uses_cloud/Cargo.toml
  - crates/rollout-core/tests/fixtures/violation_backend_uses_transport/Cargo.toml
  - crates/rollout-core/tests/fixtures/violation_coord_uses_plugin_host/Cargo.toml
  - crates/rollout-core/tests/fixtures/violation_plugin_host_transport/Cargo.toml
  - crates/rollout-core/tests/fixtures/violation_snapshots_uses_algo/Cargo.toml
  - crates/rollout-core/tests/fixtures/violation_transport_cloud/Cargo.toml
  - crates/rollout-core/tests/sampling_params_postcard.rs
  - crates/rollout-core/tests/schema_drift.rs
  - crates/rollout-plugin-host/Cargo.toml
  - crates/rollout-proto/Cargo.toml
  - crates/rollout-runtime-batch/Cargo.toml
  - crates/rollout-runtime-batch/src/worker.rs
  - crates/rollout-runtime-batch/tests/cas_state_machine.rs
  - crates/rollout-runtime-batch/tests/content_id_derivation.rs
  - crates/rollout-runtime-batch/tests/resume_skips_done.rs
  - crates/rollout-runtime-batch/tests/worker_happy_path.rs
  - crates/rollout-snapshots/Cargo.toml
  - crates/rollout-snapshots/src/lib.rs
  - crates/rollout-snapshots/tests/list_and_prune.rs
  - crates/rollout-snapshots/tests/save_restore_roundtrip.rs
  - crates/rollout-storage/Cargo.toml
  - crates/rollout-storage/tests/postgres_integration.rs
  - crates/rollout-transport/Cargo.toml
  - xtask/Cargo.toml
  - Cargo.lock
  - deny.toml
  - .github/workflows/ci.yml
  - .planning/v1.0-MILESTONE-AUDIT.md
