# Roadmap (planning index)

The narrative roadmap lives at the repo root: [`../ROADMAP.md`](../ROADMAP.md). This document is the planning index that maps requirements (REQ-IDs in `REQUIREMENTS.md`) to phases.

## Phase → Requirements

| Phase | Name | Requirements delivered |
|---|---|---|
| 1 | Core foundations | CORE-01, CORE-02, CORE-03, CORE-04, CORE-05 |
| 2 | Local substrate | SUBSTR-01, SUBSTR-02, SUBSTR-03, SUBSTR-04 |
| 3 | Inference backend + batch | BACKEND-01, BACKEND-02 |
| 4 | SFT + RM + train-state snapshots | TRAIN-01, TRAIN-02, TRAIN-03, TRAIN-04 |
| 5 | Cloud layer + object-store snapshots | CLOUD-01, CLOUD-02, CLOUD-03, CLOUD-04 |
| 6 | Multi-node distribution | DIST-01, DIST-02, DIST-03, DIST-04, DIST-05 |
| 7 | Harnesses (env + tool + eval) | HARNESS-01, HARNESS-02, HARNESS-03, HARNESS-04 |
| 8 | Online inference + episodic memory | INFER-01, INFER-02, INFER-03, INFER-04 |
| 9 | PPO + GRPO + buffer snapshots | RL-01, RL-02, RL-03, RL-04 |
| 10 | DPO / IPO / KTO | OFFLINE-01, OFFLINE-02, OFFLINE-03 |
| 11 | Process snapshots + spot recovery | SNAPSHOT-01 |
| 12 | Hardening + 1.0 | SHIP-01, SHIP-02, SHIP-03, SHIP-04 |

## Exit criteria

Each phase has measurable exit criteria stated in the narrative roadmap. They are not duplicated here to avoid drift; this index is purely the mapping.

## Coverage

100% — every v1 requirement maps to exactly one phase.

The narrative `../ROADMAP.md` is authoritative for goals, risks, and exit criteria. The phase-detail `.planning/phase-N/` directories (created later by `/gsd:plan-phase N`) are authoritative for tasks.
