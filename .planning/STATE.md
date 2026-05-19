# STATE — Project Memory

This file tracks current project state. Updated at phase transitions.

## Current Phase

**Phase 1 — Core foundations (in progress).**

Plan 01 (workspace skeleton) complete. Workspace builds cleanly, `cargo xtask` alias resolves, three crates exist as empty-but-compiles skeletons.

**Current Plan:** 02 of 7
**Last completed plan:** 01-01-workspace-skeleton (2026-05-19)

## Next Step

Continue Phase 1: plans 01-02 (Makefile — already partially committed), 01-03 (rollout-core content), 01-04 (schema-gen), 01-05 (dep-direction), 01-06 (CI), 01-07 (docs site).

## Progress

| Phase | State | Notes |
|---|---|---|
| 1 — Core foundations | in progress | 01-01 workspace skeleton complete |
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

## Performance Metrics

| Phase | Plan | Duration | Tasks | Files | Completed |
|---|---|---|---|---|---|
| 01-core-foundations | 01 | 2min | 2 | 13 | 2026-05-19 |

## Decisions

- **2026-05-19 (01-01):** Include `tracing = "0.1"` in `[workspace.dependencies]` even though no crate uses it yet — RESEARCH.md §Claude's Discretion: single workspace pin, rollout-core will re-export in plan 01-03.
- **2026-05-19 (01-01):** `Cargo.lock` committed (workspace contains binary crates `rollout` and `xtask`); standard Rust practice.
- **2026-05-19 (01-01):** CORE-01..CORE-05 were marked complete per plan 01-01's `requirements:` frontmatter list, but the actual trait surface / error taxonomy / IDs / config schema land progressively across plans 01-03 (content), 01-04 (schema-gen), 01-05 (dep-lint). The frontmatter likely intended these as "Phase 1 requirements this plan participates in." Treat the REQUIREMENTS.md checkboxes as "scaffolded, not fully delivered" until those plans ship — re-verify at phase exit.

## Last Session

- **Last session:** 2026-05-19T22:20:42Z
- **Stopped at:** Completed 01-01-workspace-skeleton-PLAN.md

## Things Not To Forget

- **No external repo / remote.** GitHub remote is not configured.
- **No CI yet.** CI lands in plan 01-06.
- Plan 01-02 (Makefile) has 3 commits but plan SUMMARY not yet written by its executor.
