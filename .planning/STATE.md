# STATE — Project Memory

This file tracks current project state. Updated at phase transitions.

## Current Phase

**Phase 0 — Pre-implementation (specs only).**

All v1 specs and planning artifacts written. No code in `crates/`, `python/`, or `database/` yet.

## Next Step

Run `/gsd:plan-phase 1` to plan **Phase 1: Core foundations** (REQ-IDs: CORE-01 through CORE-05).

## Progress

| Phase | State | Notes |
|---|---|---|
| 1 — Core foundations | not started | next |
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

## Things Not To Forget

- **No commits yet.** Git repo is initialized but nothing is committed. User decides when to commit.
- **No external repo / remote.** GitHub remote is not configured.
- **No CI yet.** Will be scaffolded in Phase 1.
- **No `Cargo.toml` workspace file yet.** Phase 1 creates it.
