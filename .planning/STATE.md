---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: milestone
current_plan: 04 of 7
status: in-progress
stopped_at: Completed 01-03-rollout-core-content-PLAN.md
last_updated: "2026-05-19T22:42:41.821Z"
progress:
  total_phases: 1
  completed_phases: 0
  total_plans: 7
  completed_plans: 4
---

# STATE — Project Memory

This file tracks current project state. Updated at phase transitions.

## Current Phase

**Phase 1 — Core foundations (in progress).**

Wave 1 complete: plans 01 (workspace skeleton), 02 (Makefile + graphify dev dep), and 07 (mdBook docs site + crate-level //! docs) shipped. Workspace builds cleanly, `cargo xtask` alias resolves, all 9 Makefile targets parse, `make help` runs locally, `node_modules/.bin/graphify-ts` resolves, `make docs` succeeds end-to-end (mdBook 0.4.52 + workspace rustdoc), and the §9.3 rustdoc gate passes for all three Phase 1 crates' binaries.

**Current Plan:** 04 of 7
**Last completed plan:** 01-03-rollout-core-content (2026-05-19) — Wave 2 finished

## Next Step

Phase 1 Wave 2 complete (plan 01-03 — rollout-core content). Start Wave 3 in parallel: plan 01-04 (schema-gen pipeline) + plan 01-05 (dep-direction + cargo-deny). Then Wave 4: 01-06 (CI).

## Progress

| Phase | State | Notes |
|---|---|---|
| 1 — Core foundations | in progress | Waves 1+2 complete: 01/02/07 (skeleton + makefile + docs) + 03 (trait surface + errors + IDs + RunConfig) |
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

## Performance Metrics

| Phase | Plan | Duration | Tasks | Files | Completed |
|---|---|---|---|---|---|
| 01-core-foundations | 01 | 2min | 2 | 13 | 2026-05-19 |
| 01-core-foundations | 02 | pre-executed | 3 | 5 | 2026-05-19 |
| 01-core-foundations | 07 | 2min | 2 | 9 | 2026-05-19 |
| 01-core-foundations | 03 | 5min | 3 | 16 | 2026-05-19 |

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

## Last Session

- **Last session:** 2026-05-19T22:42:11.995Z
- **Stopped at:** Completed 01-03-rollout-core-content-PLAN.md

## Things Not To Forget

- **No external repo / remote.** GitHub remote is not configured.
- **No CI yet.** CI lands in plan 01-06.
- `make lint` / `make check` end-to-end success deferred until plan 01-07 closes the `missing_docs` gap on rollout-cli/xtask binaries.
- `make docs` end-to-end success deferred until plan 01-07 ships `docs/book/`.
