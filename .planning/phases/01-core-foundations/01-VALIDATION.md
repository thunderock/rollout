---
phase: 1
slug: core-foundations
status: planned
nyquist_compliant: true
wave_0_complete: false
created: 2026-05-19
updated: 2026-05-19
---

# Phase 1 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Wave Overview

| Wave | Plans |
|------|-------|
| 1 | 01-workspace-skeleton, 02-makefile, 07-docs-site |
| 2 | 03-rollout-core-content |
| 3 | 04-schema-gen-pipeline, 05-dep-direction-and-deny |
| 4 | 06-github-actions-ci |

Plan 07 (docs-site bootstrap) joins Wave 1 because it has no code dependencies — it only touches `docs/book/`, `.gitignore`, and the two binary crates' `main.rs` files (crate-level `//!` lines, which Plan 01's stubs do not block). Plan 06 depends on Plan 07 so the `docs-build` job has a real book to render.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `#[test]` + `cargo test` (no external test framework) |
| **Config file** | None (workspace `Cargo.toml` declares any shared `[profile.test]` settings) |
| **Quick run command** | `cargo test -p rollout-core` |
| **Full suite command** | `cargo test --workspace --tests` |
| **Estimated runtime** | ~30 seconds quick / ~90 seconds full (cold) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p rollout-core`
- **After every plan wave:** Run `cargo test --workspace --tests` + `make lint`
- **Before `/gsd:verify-work`:** Full suite + `cargo xtask schema-gen` + `check-jsonschema --check-metaschema schemas/rollout.schema.json` + `git diff --exit-code schemas/ python/` + `make docs` + `RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" cargo doc --workspace --no-deps --all-features`
- **Max feedback latency:** ~30 seconds (quick) / ~90 seconds (full)

---

## Per-Task Verification Map

> Updated by planner on 2026-05-19. Tasks reference plan/task IDs as `{plan-NN}/{task-N}`.

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 01/1 | 01-workspace-skeleton | 1 | CORE-01..05 (infra) | structural | `cd /Users/ashutosh/personal/rollout && test -f Cargo.toml && test -f rust-toolchain.toml && grep -q 'schemars = "1.2.1"' Cargo.toml` | created in task | ⬜ pending |
| 01/2 | 01-workspace-skeleton | 1 | CORE-01..05 (infra) | structural | `cd /Users/ashutosh/personal/rollout && cargo build --workspace && cargo xtask schema-gen 2>&1 \| grep -q "not yet implemented"` | created in task | ⬜ pending |
| 02/1 | 02-makefile | 1 | CORE-01, CORE-04, DOCS-01 (infra) | structural | `cd /Users/ashutosh/personal/rollout && make -n help && make -n lint && make -n test && make -n schema-gen && make -n validate-schema && make -n docs && grep -qE '^docs:' Makefile && grep -q 'mdbook build docs/book' Makefile` | created in task | ⬜ pending |
| 02/2 | 02-makefile | 1 | CORE-01 (infra) | structural | `grep -q 'make help' /Users/ashutosh/personal/rollout/README.md && grep -q 'make docs' /Users/ashutosh/personal/rollout/README.md` | created in task | ⬜ pending |
| 02/3 | 02-makefile | 1 | dev-tooling (graphify-ts) | structural | `cd /Users/ashutosh/personal/rollout && test -f package.json && grep -q '@mohammednagy/graphify-ts' package.json && grep -q '^node_modules/$' .gitignore && grep -q '^graphify-out/$' .gitignore && test -x node_modules/.bin/graphify-ts && grep -qE '^graphify:' Makefile && grep -q 'npx graphify-ts generate' Makefile && make -n graphify >/dev/null` | created in task | ⬜ pending |
| 03/1 | 03-rollout-core-content | 2 | CORE-03, CORE-05 | unit | `cd /Users/ashutosh/personal/rollout && cargo test -p rollout-core --test id_types && cargo test -p rollout-core --test error_taxonomy` | created in task | ⬜ pending |
| 03/2 | 03-rollout-core-content | 2 | CORE-01, DOCS-03 | integration (compile) | `cd /Users/ashutosh/personal/rollout && cargo test -p rollout-core --test trait_surface && cargo clippy -p rollout-core --all-targets -- -D warnings && [ $(grep -c 'pub trait' crates/rollout-core/src/traits/*.rs \| awk -F: '{s+=$2} END {print s}') -eq 19 ]` | created in task | ⬜ pending |
| 03/3 | 03-rollout-core-content | 2 | CORE-04, DOCS-03 (source) | unit | `cd /Users/ashutosh/personal/rollout && cargo test -p rollout-core && cargo clippy -p rollout-core --all-targets -- -D warnings && grep -q 'pub struct RunConfig' crates/rollout-core/src/config/mod.rs && RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" cargo doc -p rollout-core --no-deps --all-features` | created in task | ⬜ pending |
| 04/1 | 04-schema-gen-pipeline | 3 | CORE-04 | integration (drift + smoke + meta-schema) | `cd /Users/ashutosh/personal/rollout && cargo xtask schema-gen && cargo test -p rollout-core --test schema_drift && check-jsonschema --check-metaschema schemas/rollout.schema.json && cargo xtask schema-gen && git diff --exit-code schemas/ python/` | created in task | ⬜ pending |
| 04/1b | 04-schema-gen-pipeline | 3 | CORE-04 | integration (python stub drift) | `cargo test -p rollout-core --test schema_drift -- python_stubs_match_committed` | created in task | ⬜ pending |
| 04/2 | 04-schema-gen-pipeline | 3 | CORE-04 | integration (CLI) | `cd /Users/ashutosh/personal/rollout && cargo run -p rollout-cli -- schema --format json \| python3 -m json.tool >/dev/null && cargo run -p rollout-cli -- schema --format pretty \| python3 -m json.tool >/dev/null && bash scripts/check-schema.sh` | created in task | ⬜ pending |
| 05/1 | 05-dep-direction-and-deny | 3 | CORE-02 | structural | `grep -q '\[licenses\]' /Users/ashutosh/personal/rollout/deny.toml && grep -q '"Unicode-DFS-2016"' /Users/ashutosh/personal/rollout/deny.toml && grep -q 'name\s*=\s*"openssl"' /Users/ashutosh/personal/rollout/deny.toml` | created in task | ⬜ pending |
| 05/2 | 05-dep-direction-and-deny | 3 | CORE-02 | integration (dep-direction + negative fixture) | `cd /Users/ashutosh/personal/rollout && cargo test -p rollout-core --test dependency_direction && cargo build --workspace && cargo clippy -p rollout-core --all-targets -- -D warnings` | created in task | ⬜ pending |
| 06/1 | 06-github-actions-ci | 4 | CORE-02, CORE-04 (CI gates) | structural | `cd /Users/ashutosh/personal/rollout && test -f .github/workflows/ci.yml && python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))" && grep -q 'dtolnay/rust-toolchain@1.88.0' .github/workflows/ci.yml && grep -q 'cargo test -p rollout-core --test dependency_direction' .github/workflows/ci.yml && grep -q 'check-jsonschema --check-metaschema' .github/workflows/ci.yml` | created in task | ⬜ pending |
| 06/2 | 06-github-actions-ci | 4 | DOCS-01, DOCS-02, DOCS-03 (CI gates) | structural | `cd /Users/ashutosh/personal/rollout && grep -q '^  rustdoc-check:' .github/workflows/ci.yml && grep -q '^  docs-build:' .github/workflows/ci.yml && grep -q '^  docs-deploy:' .github/workflows/ci.yml && grep -q '^  docs-test-policy:' .github/workflows/ci.yml && test -x scripts/check-docs-tests-touched.sh && grep -qF '[skip-docs-check]' scripts/check-docs-tests-touched.sh` | created in task | ⬜ pending |
| 07/1 | 07-docs-site | 1 | DOCS-01 | structural | `cd /Users/ashutosh/personal/rollout && mdbook build docs/book && test -f docs/book/book/index.html && grep -q 'examples' docs/book/src/SUMMARY.md` | created in task | ⬜ pending |
| 07/2 | 07-docs-site | 1 | DOCS-03 | structural | `grep -q '^//!' /Users/ashutosh/personal/rollout/crates/rollout-cli/src/main.rs && grep -q '^//!' /Users/ashutosh/personal/rollout/xtask/src/main.rs` | created in task | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

### Requirement coverage check

| Requirement | Covered by plan(s) | Task rows |
|---|---|---|
| CORE-01 | 01, 03 | 01/2, 03/2 |
| CORE-02 | 05, 06 | 05/1, 05/2, 06/1 |
| CORE-03 | 03 | 03/1 |
| CORE-04 | 03, 04, 06 | 03/3, 04/1, 04/1b, 04/2, 06/1 |
| CORE-05 | 03 | 03/1 |
| DOCS-01 | 02, 06, 07 | 02/1, 06/2, 07/1 |
| DOCS-02 | 06 | 06/2 |
| DOCS-03 | 03, 06, 07 | 03/2, 03/3, 06/2, 07/2 |

All 8 requirements covered by ≥ 1 task.

---

## Wave 0 Requirements

Wave 0 stubs (red tests authored **before** any implementation in each plan; tests fail with compile errors until the impl lands within the same task per task-level TDD):

- [ ] `crates/rollout-core/Cargo.toml` — manifest exists so `cargo test -p rollout-core` resolves *(produced by Plan 01 Task 2)*
- [ ] `crates/rollout-core/src/lib.rs` — empty/skeleton lib so the crate compiles *(produced by Plan 01 Task 2)*
- [ ] `crates/rollout-core/tests/trait_surface.rs` — all 19 traits public + `Send + Sync` *(produced by Plan 03 Task 2 — RED before traits land)*
- [ ] `crates/rollout-core/tests/error_taxonomy.rs` — `CoreError` outer/inner variants + `#[from]` propagation *(produced by Plan 03 Task 1 — RED before errors.rs lands)*
- [ ] `crates/rollout-core/tests/id_types.rs` — `RunId` / `WorkerId` / `ContentId` round-trips + serde + determinism *(produced by Plan 03 Task 1 — RED before ids.rs lands)*
- [ ] `crates/rollout-core/tests/dependency_direction.rs` — dep-boundary lint + deliberate-violation negative fixture using `cargo_metadata` *(produced by Plan 05 Task 2 — RED before lint impl)*
- [ ] `crates/rollout-core/tests/schema_drift.rs` — drift check between committed `schemas/` + `python/rollout/_config_stubs.py` and freshly regenerated artifacts *(produced by Plan 04 Task 1 — RED before xtask schema-gen impl)*
- [ ] `xtask/Cargo.toml` + `xtask/src/main.rs` — xtask binary stub (workspace member, `publish = false`) *(produced by Plan 01 Task 2)*
- [ ] `.cargo/config.toml` with `[alias] xtask = "run --package xtask --"` *(produced by Plan 01 Task 1)*
- [ ] `crates/rollout-cli/Cargo.toml` + `src/main.rs` skeleton *(produced by Plan 01 Task 2 — stub; real schema impl in Plan 04 Task 2)*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| GitHub Actions workflow runs successfully on a real PR | CORE-02, CORE-04, DOCS-01..03 (CI gating) | CI is GitHub-side; cannot be fully validated locally without `act` | After Plan 06 lands, open a draft PR; confirm `lint`, `test`, `deny`, `commitlint`, `schema-drift`, `architecture-lint`, `unused-deps`, `rustdoc-check`, `docs-build`, `docs-test-policy` jobs all turn green; on merge to `main`, confirm `docs-deploy` publishes the site |
| Deliberate-violation lint actually fails CI (not just locally) | CORE-02 exit criterion | "Enforced in CI" means it must fail there | On a throwaway branch, temporarily add `rollout-cloud-aws = "0.1"` to a real (future) algo crate's Cargo.toml; confirm CI architecture-lint job fails; revert |
| docs-test-policy actually fails on a code-only PR | DOCS-02 exit criterion | Bypass + diff parsing only meaningful against a real PR | On a throwaway branch, edit `crates/rollout-core/src/lib.rs` without touching docs/tests/inline-doc-comments; open a PR; confirm `docs-test-policy` job fails. Then add `[skip-docs-check]` to the head commit and confirm it bypasses. |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references (all listed Wave 0 files produced in plans 01–05)
- [x] No watch-mode flags (`cargo watch` never used in CI commands)
- [x] Feedback latency < 90s (quick suite ~30s)
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** approved by planner 2026-05-19 (amended 2026-05-19 for DOCS-01..03 + Plan 07 docs-site)
