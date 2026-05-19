---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: milestone
current_plan: Not started
status: Milestone complete
stopped_at: Completed 01-06-github-actions-ci-PLAN.md (last plan of Phase 1)
last_updated: "2026-05-19T23:20:07.298Z"
progress:
  total_phases: 1
  completed_phases: 1
  total_plans: 7
  completed_plans: 7
---

# STATE — Project Memory

This file tracks current project state. Updated at phase transitions.

## Current Phase

**Phase 1 — Core foundations (COMPLETE).**

All 7 plans shipped across 4 waves. Workspace builds cleanly, `cargo xtask schema-gen` regenerates deterministic schema artifacts, dep-direction lint enforced via integration test, `cargo deny` configured, `make docs` succeeds end-to-end, §9.3 rustdoc gate passes for all binary crates, and `.github/workflows/ci.yml` with 11 jobs (lint, test, deny, commitlint, schema-drift, architecture-lint, unused-deps, rustdoc-check, docs-build, docs-deploy, docs-test-policy) is armed for branch-protection on first PR.

**Current Plan:** Not started
**Last completed plan:** 01-06-github-actions-ci (2026-05-19) — Wave 4 complete

## Next Step

Phase 1 complete. Run `/gsd:verify-work` to confirm exit criteria. Next: kick off Phase 2 (Local substrate) — embedded KV `Storage` backend (sled vs redb vs rocksdb benchmark), `rollout-cloud-local` filesystem object store + in-memory queue + env-var secret store, gRPC-over-QUIC `rollout-transport` deadline-based heartbeats, `rollout-plugin-host` (PyO3 + subprocess RPC).

## Progress

| Phase | State | Notes |
|---|---|---|
| 1 — Core foundations | complete | All 4 waves shipped (01/02/07 + 03 + 04+05 + 06). 11-job CI workflow armed. Pending /gsd:verify-work. |
| 2 — Local substrate | not started | |
| 3 — Inference backend + batch | not started | |
| 4 — SFT + RM + train-state snapshots | not started | |
| 5 — Cloud layer + object-store snapshots | not started | |
| 6 — Multi-node distribution | not started | |
| 7 — Harnesses | not started | |
| 8 — Online inference + episodic memory | not started | |
| 9 — PPO + GRPO + buffer snapshots | not started | the headline phase |
| 10 — DPO / IPO / KTO | not started | |
| 11 — Process snapshots + spot recovery | not started | |
| 12 — Hardening + 1.0 | not started | |

## Open Decisions (waiting on phase signals)

| Decision | Deadline | Owner |
|---|---|---|
| Embedded KV store: sled vs redb vs rocksdb | Phase 2 (benchmark on heartbeat-write workload) | Phase 2 lead |
| Async runtime pinning policy | Phase 1 | Phase 1 lead |
| Process snapshot tool: CRIU vs custom | Phase 11 | Phase 11 lead |
| Logical clock vs NTP for split-brain prevention | Phase 6 | Phase 6 lead |

## Recent Changes

- 2026-05-19: Project initialized. All v1 specs written to `docs/specs/`. Root governance docs (`AGENTS.md`, `SKILLS.md`, `README.md`, `ARCHITECTURE.md`, `ROADMAP.md`, `LICENSE`) in place. Planning artifacts in `.planning/`.
- 2026-05-19: Plan 01-01 (workspace skeleton) complete. Workspace `Cargo.toml`, `rust-toolchain.toml`, `.cargo/config.toml`, and three crate skeletons (`rollout-core`, `rollout-cli`, `xtask`) added. `cargo build --workspace` and `cargo xtask schema-gen` both succeed.
- 2026-05-19: Plan 01-02 (top-level Makefile + graphify dev dep) complete. `Makefile` ships all 9 targets (lint/test/build/check/schema-gen/validate-schema/docs/graphify/help) — `make -n` parses every target, `make help` runs. Root `package.json` declares `@mohammednagy/graphify-ts ^0.22.9` as a dev dep; `.gitignore` excludes `node_modules/`, `graphify-out/`, `*.tsbuildinfo`. README quick-start points users to `make help`. SUMMARY authored separately from the three pre-existing feat commits (3cb1b07, f047b1e, 7af8903).
- 2026-05-19: Plan 01-07 (docs-site bootstrap + crate-level //! docs) complete. `docs/book/` mdBook scaffold (book.toml + SUMMARY + introduction + architecture stub + reserved examples landing page) renders cleanly via `mdbook build docs/book`. `make docs` succeeds end-to-end. Crate-level `//!` doc comments added on `rollout-cli` and `xtask` binaries — the §9.3 rustdoc gate (`-D rustdoc::missing_crate_level_docs`) passes for both. mdBook 0.4.52 installed locally via `cargo install mdbook --locked`. Commits: b3899ea (Task 1 — scaffold), 4620795 (Task 2 — //! docs). Wave 1 closes here.
- 2026-05-19: Plan 01-03 (rollout-core content) complete. All 19 traits from CORE-01 (PolicyAlgorithm, Worker, Coordinator, Scheduler, Plugin, PluginHost, EnvHarness, ToolHarness, EvalHarness, RewardModel, InferenceBackend, Storage, StorageTxn, Snapshotter, ObjectStore, SecretStore, ComputeHint, Queue, Clock) public from `rollout-core` with one-line rustdocs + Send+Sync + object-safe; CoreError taxonomy (Recoverable | Fatal, RetryHint, #[from] propagation) per CORE-03; RunId(Ulid), WorkerId(Ulid), ContentId(blake3 [u8;32]) per CORE-05; RunConfig type tree with `JsonSchema` + `deny_unknown_fields` + `schemars(range(min=1,max=1))` for CORE-04 foundation. Wave 0 RED-first tests (id_types, error_taxonomy, trait_surface — 10 tests total) all green. `cargo test -p rollout-core` + `cargo clippy -p rollout-core --all-targets -- -D warnings` + DOCS-03 rustdoc gate all pass. Commits: 87143f1 (Task 1 — IDs + errors), ee41907 (Task 2 — traits), 13cb09b (Task 3 — RunConfig). Wave 2 closes here.
- 2026-05-19: Plan 01-04 (schema-gen pipeline) complete. `cargo xtask schema-gen` now regenerates 3 artifacts deterministically (`schemas/rollout.schema.json`, `python/rollout/_config_stubs.py`, `docs/schema-reference.md`) with a `--out-dir` flag; xtask invokes `datamodel-codegen 0.57.0` subprocess + strips the embedded `#   timestamp:` line for byte-deterministic regeneration. `rollout schema --format json|pretty` is fully wired in `rollout-cli` via `clap::ValueEnum`. `scripts/check-schema.sh` wraps `check-jsonschema --check-metaschema` and exits 0 (CORE-04 exit criterion). `crates/rollout-core/tests/schema_drift.rs` ships 3 drift tests (JSON + Python stubs + structural defense), all green. `cargo clippy --workspace --all-targets -- -D warnings` clean. Commits: 1bfa115 (Task 1 — drift tests + xtask pipeline + initial artifacts), 857d659 (Task 2 — CLI + check-schema.sh), 5fe912f (clippy fix + Cargo.lock refresh).
- 2026-05-19: Plan 01-06 (GitHub Actions CI) complete. `.github/workflows/ci.yml` with 11 jobs landed (lint/test/deny/commitlint/schema-drift/architecture-lint/unused-deps/rustdoc-check/docs-build/docs-deploy/docs-test-policy). Pinned action versions per RESEARCH.md (dtolnay/rust-toolchain@1.88.0, Swatinem/rust-cache@v2, EmbarkStudios/cargo-deny-action@v2, bnjbvr/cargo-machete@v0.9.2, peaceiris/actions-mdbook@v2 + mdbook 0.4.40, actions/configure-pages@v5 + upload-pages-artifact@v3 + deploy-pages@v4). Per-job rust-cache shared-keys (ci-lint/ci-test/ci-schema-drift/ci-arch-lint/ci-rustdoc/ci-docs-build). Top-level `permissions: { contents: read, pages: write, id-token: write }` + `concurrency: pages-${{ github.ref }}`. `scripts/check-docs-tests-touched.sh` shipped (executable, `[skip-docs-check]` bypass, inline `///`/`//!`/`"""` doc-comment fallback). CORE-02 + CORE-04 + DOCS-01..03 CI gates operational. YAML parses cleanly; 11/11 jobs present via python set-comparison. Commits: 9b3a372 (Task 1 — 7 core jobs), d26870b (Task 2 — 4 docs-policy jobs + script). Wave 4 closes here; Phase 1 complete (pending /gsd:verify-work).
- 2026-05-19: Plan 01-05 (dep-direction + cargo-deny) complete. `deny.toml` at workspace root with `version = 2` on `[advisories]` + `[licenses]`, full vector-style allowlist (Apache/MIT/BSD/ISC/Unicode-DFS-2016+Unicode-3.0/CC0/Zlib/0BSD/MPL-2.0/CDLA-Permissive-2.0), and `[bans] openssl + openssl-sys` (use rustls when TLS arrives). `crates/rollout-core/tests/dependency_direction.rs` ships 2 tests via `cargo_metadata`: positive workspace scan (vacuously green in Phase 1) + load-bearing negative test against `tests/fixtures/violation/Cargo.toml` (hand-rolled manifest simulating rollout-algo-ppo → rollout-cloud-aws, not a workspace member, not auto-discovered). 15 workspace tests now green (13 + 2 new). `cargo build --workspace` + `cargo clippy --workspace --all-targets -- -D warnings` clean. CORE-02 closed locally; functional `cargo deny check` runs in CI (Plan 01-06). Commits: c5c15a3 (Task 1 — deny.toml), a251c79 (Task 2 RED — failing test + fixture), f9b323b (Task 2 GREEN — cargo_metadata dev-dep + clippy fix). Wave 3 closes here.

## Performance Metrics

| Phase | Plan | Duration | Tasks | Files | Completed |
|---|---|---|---|---|---|
| 01-core-foundations | 01 | 2min | 2 | 13 | 2026-05-19 |
| 01-core-foundations | 02 | pre-executed | 3 | 5 | 2026-05-19 |
| 01-core-foundations | 07 | 2min | 2 | 9 | 2026-05-19 |
| 01-core-foundations | 03 | 5min | 3 | 16 | 2026-05-19 |
| 01-core-foundations | 04 | 4min | 2 | 8 | 2026-05-19 |
| 01-core-foundations | 05 | 2min | 2 | 6 | 2026-05-19 |
| 01-core-foundations | 06 | 2min | 2 | 2 | 2026-05-19 |

## Decisions

- **2026-05-19 (01-01):** Include `tracing = "0.1"` in `[workspace.dependencies]` even though no crate uses it yet — RESEARCH.md §Claude's Discretion: single workspace pin, rollout-core will re-export in plan 01-03.
- **2026-05-19 (01-01):** `Cargo.lock` committed (workspace contains binary crates `rollout` and `xtask`); standard Rust practice.
- **2026-05-19 (01-01):** CORE-01..CORE-05 were marked complete per plan 01-01's `requirements:` frontmatter list, but the actual trait surface / error taxonomy / IDs / config schema land progressively across plans 01-03 (content), 01-04 (schema-gen), 01-05 (dep-lint). The frontmatter likely intended these as "Phase 1 requirements this plan participates in." Treat the REQUIREMENTS.md checkboxes as "scaffolded, not fully delivered" until those plans ship — re-verify at phase exit.
- **2026-05-19 (01-02):** `make lint` will not pass end-to-end until plan 01-07 adds crate-level `//!` doc comments to `rollout-cli/src/main.rs` and `xtask/src/main.rs` — `missing_docs` is `-D warning` under `cargo clippy`. The Makefile target body is exactly per D-LOCAL-02; the gap is in workspace content, not Makefile. Acceptance gate for plan 01-02 is `make -n <target>` parses (passes), not `make check` succeeds (deferred to 01-07).
- **2026-05-19 (01-02):** `make docs` won't run end-to-end until plan 01-07 ships `docs/book/book.toml` + `src/SUMMARY.md`. Target body matches AGENTS.md §9.1 exactly; consumer lands with 01-07.
- **2026-05-19 (01-02):** `make validate-schema` requires `check-jsonschema` on PATH (`pip install check-jsonschema`); environment provisioning is plan 01-06's responsibility.
- **2026-05-19 (01-02):** Plan 01-02 frontmatter lists `requirements: [CORE-01, CORE-04, DOCS-01]`. CORE-01 and CORE-04 are already `[x]` (claimed by plan 01-01's frontmatter; same "participates in" pattern). DOCS-01 stays `[ ]` because the docs-site bootstrap (mdBook scaffold + GitHub Pages workflow) lands in plan 01-07; plan 01-02 only ships the Makefile-side `make docs` target. REQUIREMENTS.md checkbox status remains accurate.
- **2026-05-19 (01-07):** mdBook theme set to `default-theme = "light"` only — minimal, no custom theming until later phases need it. book.toml does not pin a specific mdBook version (the 0.4.x config shape is stable); local install is 0.4.52.
- **2026-05-19 (01-07):** Architecture page is a one-line cross-link to root `ARCHITECTURE.md` rather than duplicating content. Matches AGENTS.md §2 single-source-of-truth.
- **2026-05-19 (01-07):** Examples landing page explicitly names SHIP-03 + spells out the progressive landing path (Phase 4 → 9 → 12) so future planners do not re-derive the contract. Reserved per AGENTS.md §9.4 / D-V1-EXAMPLE.
- **2026-05-19 (01-07):** DOCS-01 + DOCS-03 (rustdoc gate for binaries) are now satisfied for Phase 1's two binary crates. DOCS-02 (per-commit doc/test policy CI script) and the GitHub Pages deploy workflow land in plan 01-06.
- [Phase 01-core-foundations]: 2026-05-19 (01-03): Clock trait kept sync (no async_trait) per RESEARCH Pattern 2 exception; all other I/O traits use #[async_trait] for dyn-compatibility.
- [Phase 01-core-foundations]: 2026-05-19 (01-03): WorkerContext + DrainReason added as Phase 1 stub types in traits/worker.rs to preserve spec-shaped Worker signatures; full types arrive in Phase 2.
- [Phase 01-core-foundations]: 2026-05-19 (01-03): schemars 1.2.1 #[schemars(range(min = 1, max = 1))] compiled without fallback — RESEARCH Open Question Q2 resolved positively.
- [Phase 01-core-foundations]: 2026-05-19 (01-03): not_serializable test scans errors.rs source via include_bytes! rather than nightly negative trait bounds — stable-Rust enforcement of RESEARCH Anti-Pattern 4.
- [Phase 01-core-foundations]: 2026-05-19 (01-04): Python stubs committed as `_config_stubs.py` (not `.pyi`) in Phase 1 — datamodel-codegen emits a real `.py` with Pydantic v2 class bodies, not a stub-only `.pyi`. Rename / split to `.pyi` deferred to Phase 12 SHIP-02.
- [Phase 01-core-foundations]: 2026-05-19 (01-04): xtask strips the `#   timestamp:` header line datamodel-codegen embeds in its output so byte-deterministic regeneration is achievable. Required for `cargo test -p rollout-core --test schema_drift` to be stable.
- [Phase 01-core-foundations]: 2026-05-19 (01-04): Drift authority lives in `crates/rollout-core/tests/schema_drift.rs` (workspace test); `cargo xtask schema-check` stays a thin shim pointing devs at the test. Rationale: workspace test runs under the same `cargo test --workspace` + rust-cache pass as everything else.
- [Phase 01-core-foundations]: 2026-05-19 (01-04): `rollout schema --format` uses `clap::ValueEnum` (`Json` | `Pretty`) instead of stringly-typed parsing — clap rejects unknown values + emits clean `--help`.
- [Phase 01-core-foundations]: 2026-05-19 (01-05): Dep-direction lint lives in crates/rollout-core/tests/dependency_direction.rs (integration test) per D-LINT-01, not xtask — workspace test inherits the existing cargo test --workspace + rust-cache pass used by CI; no parallel xtask invocation required.
- [Phase 01-core-foundations]: 2026-05-19 (01-05): Violation fixture under crates/rollout-core/tests/fixtures/violation/ is intentionally not a workspace member and not picked up by cargo's tests/ auto-discovery (no main.rs); fake rollout-cloud-aws dep cannot be resolved but never is, because cargo build --workspace never tries to build it.
- [Phase 01-core-foundations]: 2026-05-19 (01-05): deny.toml [bans] multiple-versions = warn (not deny) because schemars/serde/tokio transitives frequently dupe in early phases; tighten to deny in Phase 12 (1.0 hardening). wildcards = deny, unknown-registry = deny, unknown-git = deny stay strict from day 1.
- [Phase 01-core-foundations]: 2026-05-19 (01-05): Hand-rolled TOML extraction (toml_pkg_name/toml_dep_names) in the dep-direction test instead of pulling toml as a dev-dep — fixture content is hand-controlled so a forgiving parse is correct; keeps rollout-core dev-deps to one crate (cargo_metadata).
- [Phase 01-core-foundations]: 2026-05-19 (01-06): All 11 CI jobs land in a single `.github/workflows/ci.yml` (not split into `docs.yml`). D-DOCS-02 permits "within ci.yml"; consolidates top-level `permissions:` + `concurrency:` declaration and produces one branch-protection list. mdBook in CI pinned to 0.4.40 (local install 0.4.52 per Plan 07 — both 0.4.x stable config shape).
- [Phase 01-core-foundations]: 2026-05-19 (01-06): `docs-build` always builds on PRs for verification but only uploads the Pages artifact on `push:main`; `docs-deploy` `needs: [docs-build, test, lint]` so a green book never ships while tests are red.
- [Phase 01-core-foundations]: 2026-05-19 (01-06): `docs-test-policy` gated `if: github.event_name == 'pull_request'` only — skipped entirely on direct main pushes (bootstrap exemption per D-DOCS-03 / AGENTS.md §9.2). `[skip-docs-check]` trailer matched via `grep -qF` (literal, not regex) against the head commit message.
- [Phase 01-core-foundations]: 2026-05-19 (01-06): `scripts/check-docs-tests-touched.sh` inline doc-comment fallback uses `git diff -U0` and matches added lines containing `///`, `//!`, or `"""` — a commit editing only rustdoc/Python docstrings on changed code files passes without a separate `docs/` or `tests/` file change.

## Last Session

- **Last session:** 2026-05-19T23:01:23Z
- **Stopped at:** Completed 01-06-github-actions-ci-PLAN.md (last plan of Phase 1)

## Things Not To Forget

- **No external repo / remote.** GitHub remote is not configured. CI workflow exists but won't run until the repo is pushed to a GitHub remote.
- **CI armed but unverified end-to-end.** `.github/workflows/ci.yml` parses cleanly + all action versions pinned per RESEARCH.md, but the 3 Manual-Only Verifications from `.planning/phases/01-core-foundations/01-VALIDATION.md` (real-PR job greens, deliberate-violation negative test, docs-test-policy negative test) must be exercised on the first PR after the remote is configured.
- **Branch-protection setup TODO** once the GitHub remote is configured: require the 10 PR-time jobs (everything except `docs-deploy`, which is push-only) as branch-protection checks on `main`.
