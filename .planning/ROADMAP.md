# Roadmap (planning index)

The narrative roadmap lives at the repo root: [`../ROADMAP.md`](../ROADMAP.md). This document is the planning index that maps requirements (REQ-IDs in `REQUIREMENTS.md`) to phases, plus the active-milestone phase detail consumed by `/gsd:plan-phase`.

## Milestones

| Milestone | Status | Phases | Shipped |
|---|---|---|---|
| **v1.0 — substrate + train** | ✓ SHIPPED | 1, 2, 3, 4 | 2026-05-27 |
| **v1.1 — cloud + multi-node + harnesses** | active | 5, 6, 7 | — |
| v1.2 — online inference + RL + offline + spot | planned | 8, 9, 10, 11 | — |
| v1.0 release ship | planned | 12 | — |

<details>
<summary><strong>v1.0 — substrate + train (✓ SHIPPED 2026-05-27)</strong></summary>

- 4 phases · 30 plans · 59 tasks · 112 commits · 18.6k LOC · 7-day cycle
- 18/18 v1.0 requirements satisfied (CORE-01..05, SUBSTR-01..04, BACKEND-01..02, TRAIN-01..04, DOCS-01..03)
- Archive: [`milestones/v1.0-ROADMAP.md`](milestones/v1.0-ROADMAP.md), [`milestones/v1.0-REQUIREMENTS.md`](milestones/v1.0-REQUIREMENTS.md), [`milestones/v1.0-MILESTONE-AUDIT.md`](milestones/v1.0-MILESTONE-AUDIT.md)
- Retrospective: [`RETROSPECTIVE.md`](RETROSPECTIVE.md)

</details>

## Phase → Requirements

| Phase | Name | Requirements delivered | Status |
|---|---|---|---|
| 1 | Core foundations | CORE-01, CORE-02, CORE-03, CORE-04, CORE-05, DOCS-01, DOCS-02, DOCS-03 | ✓ v1.0 |
| 2 | Local substrate | SUBSTR-01, SUBSTR-02, SUBSTR-03, SUBSTR-04 | ✓ v1.0 |
| 3 | Inference backend + batch | BACKEND-01, BACKEND-02 | ✓ v1.0 |
| 4 | SFT + RM + train-state snapshots | TRAIN-01, TRAIN-02, TRAIN-03, TRAIN-04 | ✓ v1.0 |
| 5 | Cloud layer + object-store snapshots | CLOUD-01, CLOUD-02, CLOUD-03, CLOUD-04 | ✓ v1.1 |
| 6 | Multi-node distribution | DIST-01, DIST-02, DIST-03, DIST-04, DIST-05 | ✓ v1.1 |
| 7 | Harnesses (env + tool + eval) | HARNESS-01, HARNESS-02, HARNESS-03 | ✓ v1.1 |
| 8 | Online inference + episodic memory | INFER-01, INFER-02, INFER-03, INFER-04 | v1.2 planned |
| 9 | PPO + GRPO + buffer snapshots | RL-01, RL-02, RL-03, RL-04 | v1.2 planned |
| 10 | DPO / IPO / KTO | OFFLINE-01, OFFLINE-02, OFFLINE-03 | v1.2 planned |
| 11 | Process snapshots + spot recovery | SNAPSHOT-01, HARNESS-04 | v1.2 planned |
| 12 | Hardening + 1.0 | SHIP-01, SHIP-02, SHIP-03, SHIP-04 | planned |

> **Note:** `HARNESS-04` (eval gate) moved from Phase 7 to Phase 11 (v1.2) — it needs algo+dist+harness coupling and lands with PPO consumers. See `REQUIREMENTS.md` for the deferral rationale.

## Exit criteria

Each phase has measurable exit criteria stated in the narrative roadmap. They are not duplicated here to avoid drift; this index is purely the mapping plus the active-milestone phase detail below.

## Coverage

100% — every v1 requirement maps to exactly one phase.

**v1.1 coverage:** 12/12 in-scope requirements mapped (Phase 5: 4 · Phase 6: 5 · Phase 7: 3).

**Cross-cutting requirements:** `DOCS-01` (docs site bootstrap), `DOCS-02` (per-commit doc/test policy), and `DOCS-03` (rustdoc CI gate) bootstrap in Phase 1 but apply to **every** phase thereafter — every phase's plans must enforce doc + test updates per commit.

**v1 release gate:** `SHIP-03` is hardened — v1 cannot ship without at least one end-to-end working model example. Recipe lands progressively (Phase 4 stub → Phase 9 real → Phase 12 documented).

The narrative `../ROADMAP.md` is authoritative for goals, risks, and exit criteria. The phase-detail `.planning/phase-N/` directories (created later by `/gsd:plan-phase N`) are authoritative for tasks.

---

## Milestone v1.1 — cloud + distribution + harnesses (✅ SHIPPED 2026-06-01)

**Goal:** Lift v1.0's local substrate to real multi-host runs on real cloud, with the harness surface needed to feed RL training (RL phases stay v1.2).

**Phases:** 3 (5, 6, 7) — continued from v1.0 phase numbering.
**In-scope requirements:** 12 (CLOUD-01..04, DIST-01..05, HARNESS-01..03).
**Build order:** Cloud (Phase 5) → Distribution (Phase 6) → Harnesses (Phase 7). DIST consumes Phase 5's `ObjectStore::put_stream` + `Queue::dequeue_with_lease`; Harnesses consume both.
**Proof bar:** 3+ node setup runs `make smoke` against real AWS/GCP; spot-preempt signal triggers graceful drain. Every phase ships a load-bearing CI witness that runs without GPU or cloud creds.

### Phases (shipped ✅)

All three phases shipped and verified. Full phase detail — goals, success criteria, per-plan breakdown, key decisions, and tech debt — is archived at [`milestones/v1.1-ROADMAP.md`](milestones/v1.1-ROADMAP.md). Requirements: [`milestones/v1.1-REQUIREMENTS.md`](milestones/v1.1-REQUIREMENTS.md). Audit: [`milestones/v1.1-MILESTONE-AUDIT.md`](milestones/v1.1-MILESTONE-AUDIT.md).

| Phase | Plans | Status | Completed |
|-------|-------|--------|-----------|
| 5. Cloud layer + object-store snapshots | 8/8 | ✅ Complete | 2026-05-29 |
| 6. Multi-node distribution | 5/5 | ✅ Complete | 2026-05-29 |
| 7. Harnesses (env + tool + eval) | 6/6 | ✅ Complete | 2026-06-01 |

## Milestone v1.0 (archived — preserved for reference)

The full v1.0 phase detail (Phases 1–4 shipped 2026-05-27) lives in the root narrative roadmap at [`../ROADMAP.md`](../ROADMAP.md) and the archive at [`milestones/v1.0-ROADMAP.md`](milestones/v1.0-ROADMAP.md). Summary above.
