---
phase: 7
slug: harnesses-env-tool-eval
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-06-01
---

# Phase 7 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Derived from `07-RESEARCH.md` § Validation Architecture.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `#[test]` / `#[tokio::test]` (workspace standard); `proptest` available for parity/property tests |
| **Config file** | none (cargo); CI jobs in `.github/workflows/*.yml` |
| **Quick run command** | `cargo test -p <crate> --tests` (e.g. `-p rollout-harness-text`) |
| **Full suite command** | `cargo test --workspace --tests` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo deny check` + `cargo doc --workspace --no-deps --all-features` + `cargo xtask schema-gen` drift check |
| **Estimated runtime** | ~90s quick (per-crate); ~6-10 min full workspace + lint lanes |

---

## Sampling Rate

- **After every task commit:** `cargo test -p <touched-crate> --tests` + `cargo clippy -p <crate> -- -D warnings` + `cargo fmt --check`
- **After every plan wave:** `cargo test --workspace --tests` + `cargo deny check` + `cargo doc` (RUSTDOCFLAGS deny) + `cargo xtask schema-gen` drift + `forbidden-patterns` grep (`shell=True`, `libc::fork(`, raw IMDS IPs)
- **Before `/gsd:verify-work`:** Full suite green on BOTH macOS lane (compile + stub) and Linux lane (all sandbox enforcement witnesses + real-strace allowlist validation); SC4 workspace count + 14-invariant lint green
- **Max feedback latency:** ~90 seconds (per-crate quick run)

---

## Per-Task Verification Map

> Populated by the planner per task. Test names below are the ROADMAP-named CI witnesses, all ❌ Wave 0 (crates do not exist yet).

| Plan/Task | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|-----------|------|-------------|-----------|-------------------|-------------|--------|
| TBD | 0 | D-CORE-01 | compile/lint | `cargo test -p rollout-core dep_direction_invariants_hold` | partial (lint exists; eval crate must be created) | ⬜ pending |
| TBD | — | HARNESS-01 | integration | `cargo test -p rollout-harness-text env_deterministic_replay` | ❌ W0 | ⬜ pending |
| TBD | — | HARNESS-01 | unit | `cargo test -p rollout-harness-text echo_env` | ❌ W0 | ⬜ pending |
| TBD | — | HARNESS-01 | integration | `cargo test -p rollout-harness-text mock_reward_env` | ❌ W0 | ⬜ pending |
| TBD | — | HARNESS-02 | integration (Linux) | `cargo test -p rollout-harness-tool tool_sandbox_escape_blocked` | ❌ W0 | ⬜ pending |
| TBD | — | HARNESS-02 | integration (Linux) | `cargo test -p rollout-harness-tool http_tool_blocks_dns_rebinding` | ❌ W0 | ⬜ pending |
| TBD | — | HARNESS-02 | integration (Linux) | `cargo test -p rollout-harness-tool http_tool_blocks_redirect_to_imds` | ❌ W0 | ⬜ pending |
| TBD | — | HARNESS-02 | integration (Linux) | `cargo test -p rollout-harness-tool sandbox_blocks_userns` | ❌ W0 | ⬜ pending |
| TBD | — | HARNESS-02 | integration (Linux) | `cargo test -p rollout-harness-tool seccomp_blocks_unexpected_syscall` | ❌ W0 | ⬜ pending |
| TBD | — | HARNESS-02 | integration (Linux) | `cargo test -p rollout-harness-tool sandbox_blocks_mount sandbox_blocks_keyctl sandbox_blocks_bpf` | ❌ W0 | ⬜ pending |
| TBD | — | HARNESS-02 | integration (Linux) | `cargo test -p rollout-harness-tool seccomp_no_socket` | ❌ W0 | ⬜ pending |
| TBD | — | HARNESS-02 | integration (Linux) | `cargo test -p rollout-harness-tool seccomp_python_runs` (positive — validates allowlist) | ❌ W0 | ⬜ pending |
| TBD | — | HARNESS-02 | integration (Linux) | `cargo test -p rollout-harness-tool <per-tool happy-path>` | ❌ W0 | ⬜ pending |
| TBD | — | HARNESS-02 | compile (macOS) | `cargo build -p rollout-harness-tool` (stub returns documented error) | ❌ W0 | ⬜ pending |
| TBD | — | HARNESS-03 | integration | `cargo test -p rollout-harness-eval eval_score_matches_lm_eval_harness` (≤1% parity, HF_OFFLINE=1) | ❌ W0 | ⬜ pending |
| TBD | — | HARNESS-03 | integration | `cargo test -p rollout-harness-eval eval_loader_works_with_no_network` | ❌ W0 | ⬜ pending |
| TBD | — | HARNESS-03 | integration/CLI | `cargo test -p rollout-cli <eval dispatch>` + `--dry-run` snapshot | ❌ W0 | ⬜ pending |
| TBD | — | SC4 | workspace | `cargo test --workspace --tests` (5 new crates green; 14 invariants) | partial | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/rollout-core/src/traits/harness.rs` — replace v1.0 stub with spec-07 surface + `HarnessDependencies` (gates all three crates + dep-direction/public-api lints)
- [ ] `crates/rollout-harness-eval/` — **create the crate** (does not exist on disk); register as workspace member (names already in `ALGO_AND_ABOVE` — needs the physical crate present)
- [ ] `crates/rollout-harness-text/` + `crates/rollout-harness-tool/` — create crate skeletons + register as members
- [ ] `eval_reports` Storage namespace/row type (does NOT exist; mirror `WorkItemRecord` storage-key pattern)
- [ ] `crates/rollout-harness-eval/tests/fixtures/{mmlu_10,ifeval_10,gsm8k_10}` — SHA-pinned 10-row fixtures + expected scores from the pinned lm-eval version
- [ ] `forbidden-patterns` CI extension: `shell=True` over `crates/rollout-harness-tool/**/*.py`; `libc::fork(` workspace-wide
- [ ] `[workspace.dependencies]`: rustix, landlock, seccompiler, cap-std (Linux-gated), hf-hub, parquet, arrow-array
- [ ] schema-gen regen for the new per-crate `Settings` + descriptor types (HarnessGraph NOT added — D-CORE-02 defers it)
- [ ] Real `strace -c /usr/bin/python3 -c 'print(1)'` on the Linux CI lane to validate/extend `seccomp::ALLOWLIST` (the mandated spike, deferred to execution because macOS cannot run it; expect 2-5 syscall additions)

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Real `strace -c` syscall baseline on kernel 5.15 | HARNESS-02 (D-TOOL-07) | strace is Linux-only; macOS dev box cannot run it | On the Ubuntu 22.04 CI runner: `strace -fc /usr/bin/python3 -c 'print(1)'` + shell/file/http tool invocations; diff captured syscalls against `seccomp::ALLOWLIST`; the `seccomp_python_runs` positive test is the automated proxy that gates this |

*All other phase behaviors have automated verification.*

---

## Validation Sign-Off

- [ ] All tasks have automated verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 90s (quick) / < 10min (full)
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
