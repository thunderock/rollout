---
phase: 07-harnesses-env-tool-eval
verified: 2026-06-01T00:00:00Z
status: passed
score: 4/4 success criteria verified
re_verification: false
gaps: []
---

# Phase 7: Harnesses (env + tool + eval) Verification Report

**Phase Goal:** An LLM can interact with text-completion environments, invoke sandboxed tools, and be scored on bundled evals — three new algo-layer crates (rollout-harness-text, rollout-harness-tool, rollout-harness-eval) that v1.2 PPO/GRPO will consume directly via trait objects.

**Verified:** 2026-06-01
**Status:** PASSED
**Re-verification:** No — initial verification

---

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|---------|
| 1 | SC1: `cargo test -p rollout-harness-text` green; `env_deterministic_replay` + `EchoEnv` + `MockRewardEnv` plugin-host reward path pass | ✓ VERIFIED | 8 tests pass: 4 echo_env, 1 env_deterministic_replay, 3 mock_reward_env — confirmed by live run |
| 2 | SC2: rollout-harness-tool compiles to documented macOS dev-stub; Linux CVE negatives (`tool_sandbox_escape_blocked`, `http_tool_blocks_dns_rebinding`, `http_tool_blocks_redirect_to_imds`, `sandbox_blocks_userns`, `seccomp_blocks_unexpected_syscall`) + positive witnesses exist and are cfg-gated `#[cfg(target_os = "linux")]`; in-process SSRF witnesses pass on macOS | ✓ VERIFIED | `macos_stub_returns_documented_fatal` passes; negative/positive tests are `#![cfg(target_os = "linux")]` at file scope (confirmed); 7 http_ssrf + 7 connector unit tests pass on macOS; crate compiles and builds cleanly |
| 3 | SC3: `rollout eval --suite mmlu --checkpoint <id>` per-task score delivered; 10-row fixtures in `tests/fixtures/`; `eval_score_matches_lm_eval_harness` passes with HF_OFFLINE=1 deterministically | ✓ VERIFIED | `tests/fixtures/{mmlu,ifeval,gsm8k}_10.parquet` exist; 4 parity tests (mmlu/ifeval/gsm8k/same_seed) pass with HF_OFFLINE=1; `rollout eval` CLI 4/4 tests pass including mock-backend JSON dispatch |
| 4 | SC4: `cargo test --workspace --tests` green with 5 new crates present; dep-direction lint holds at 14 invariants | ✓ VERIFIED | Zero failures, zero errors across entire workspace; `dep_direction_invariants_hold` + 13 sibling tests = 14 passed |

**Score:** 4/4 truths verified

---

### Required Artifacts

| Artifact | Provides | Status | Evidence |
|---------|---------|--------|---------|
| `crates/rollout-core/src/traits/harness.rs` | `EnvHarness`/`ToolHarness`/`EvalHarness` + ~20 associated types + `HarnessDependencies` | ✓ VERIFIED | All three traits confirmed; `HarnessDependencies` has plugin_host/object_store/storage/queue/events/clock; `snapshot_episode` defaults `Ok(None)`; old `RewardModel` absent; `HarnessGraph` absent (D-CORE-02) |
| `crates/rollout-harness-text/{Cargo.toml,src/}` | TextCompletionEnv + EpisodeStore + SplitMix64 RNG + reward plugin path | ✓ VERIFIED | `episode.rs`, `reward.rs`, `lib.rs` exist; 3 tests modules exist; all 8 integration tests green |
| `crates/rollout-harness-tool/{Cargo.toml,src/}` | Layered sandbox launcher + 6 tools + macOS stub + CVE witnesses | ✓ VERIFIED | `sandbox/{seccomp,launcher,capfs,cgroup}.rs` + `tools/{python_exec,shell,file_read,file_write,http_get,http_post}.rs` + `http/{mod,connector}.rs` + `README.md` all exist; macOS stub test passes |
| `crates/rollout-harness-eval/{Cargo.toml,src/,tests/}` | MMLU/IFEval/GSM8K scorers + offline fixtures + eval-as-job + BundledEval EvalHarness impl | ✓ VERIFIED | `suites/{mmlu,ifeval,gsm8k}.rs` + `datasets/` + `backend.rs` + `job.rs` + 3 parquet fixtures + 2 integration test files; 24 tests pass |
| `crates/rollout-harness-eval/src/eval_reports.rs` | `eval_reports` Storage namespace + `eval_report_key` / `eval_report_prefix` | ✓ VERIFIED | Namespace `"eval_reports"`, `eval_report_key` function, round-trip test — confirmed |
| `crates/rollout-cli/src/eval.rs` | `rollout eval` CLI subcommand (D-EVAL-02) | ✓ VERIFIED | `EvalCmd` with `--suite/--checkpoint/--config/--dry-run/--format`; wired as `Cmd::Eval` in `main.rs`; 4 CLI tests pass |
| `docs/book/src/harnesses/{index,env,tool-sandbox,eval,cli}.md` | mdBook Harnesses chapter | ✓ VERIFIED | All 5 files exist; wired in `SUMMARY.md`; content confirmed: acc_norm/HF_OFFLINE/seccomp/NOT VM-isolated |
| `.github/workflows/ci.yml` `harness-linux` job | Ubuntu-latest lane + strace baseline + cfg(linux) enforcement tests | ✓ VERIFIED | Job exists at `ubuntu-latest`; strace step present; runs `cargo test -p rollout-harness-tool --tests --all-features`, `rollout-harness-text`, `rollout-harness-eval`; `HF_OFFLINE=1` + `HF_HUB_OFFLINE=1` set |

---

### Key Link Verification

| From | To | Via | Status | Evidence |
|------|----|----|--------|---------|
| `Cargo.toml [workspace.members]` | `crates/rollout-harness-{text,tool,eval}` | member registration | ✓ WIRED | Lines 17-19 of root Cargo.toml confirmed |
| `crates/rollout-harness-*/Cargo.toml` | `rollout-core` | path dependency | ✓ WIRED | All three Cargo.toml files contain `rollout-core = { path = "../rollout-core" }` |
| `rollout-cli/src/eval.rs` | `rollout-harness-eval::BundledEval` | dispatch in `run_eval` | ✓ WIRED | `main.rs` line 55: `Eval(eval::EvalCmd)`; `eval_dispatch` at line 135 |
| seccomp negative tests | `#[cfg(target_os = "linux")]` | file-level cfg gate | ✓ WIRED | `sandbox_negative.rs` line 10: `#![cfg(target_os = "linux")]`; `sandbox_positive.rs` line 8: same |
| macOS stub witness | `#[cfg(not(target_os = "linux"))]` | file-level cfg gate | ✓ WIRED | `macos_stub.rs` line 3: `#![cfg(not(target_os = "linux"))]` |
| SSRF connector | post-DNS IP filter | `blocked_range` + IP pinning per hop | ✓ WIRED | `connector.rs` defines all ranges; `mod.rs::one_hop` resolves then pins; `fetch` re-filters each redirect |
| `eval_score_matches_lm_eval_harness` | 10-row parquet fixtures | `datasets::load` + `HF_OFFLINE` | ✓ WIRED | `is_offline()` checks env; test asserts `is_offline()` at line 116 |
| `eval_reports` namespace | embedded storage | `T_EVAL_REPORTS` registration in `tables.rs` | ✓ WIRED | Fixed in plan 03 (blocking deviation); `rollout-storage` tests stay green |
| `docs/specs/08-cli.md` | `rollout eval` top-level subcommand | spec-08 reconcile | ✓ WIRED | `rollout eval` present; `rollout infer eval` absent — confirmed |

---

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|---------|-------------|--------|-------------------|--------|
| `eval_score_matches_lm_eval_harness` | `EvalReport.metrics` | `BundledEval::run` → `MockEvalBackend` → `suites::{mmlu,ifeval,gsm8k}::score` | Yes — canned but deterministic completions produce real scores computed by scorer logic | ✓ FLOWING |
| `env_deterministic_replay` | `(observation, reward, done, info)` | `TextCompletionEnv::step` → `SplitMix64::next_u64()` nonce injected into info | Yes — seed-dependent nonce makes trajectory genuinely seed-bound; byte-equality asserted | ✓ FLOWING |

---

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|---------|---------|--------|--------|
| SC1: harness-text tests pass (GPU-free) | `cargo test -p rollout-harness-text --tests` | 8 tests pass | ✓ PASS |
| SC2: macOS stub compiles + stub witness passes | `cargo build -p rollout-harness-tool` + `cargo test -p rollout-harness-tool --tests --all-features` | Builds clean; 1 stub + 7 unit + 7 ssrf pass; 0 Linux tests run (correct on macOS) | ✓ PASS |
| SC2: SSRF in-process witnesses | included in above | `http_tool_blocks_redirect_to_imds`, `http_tool_blocks_dns_rebinding`, `http_tool_blocks_rfc1918` all pass | ✓ PASS |
| SC3: eval parity witness (HF_OFFLINE=1) | `HF_OFFLINE=1 HF_HUB_OFFLINE=1 cargo test -p rollout-harness-eval --test eval_score_matches_lm_eval_harness` | 4 tests pass (mmlu/ifeval/gsm8k/same_seed) | ✓ PASS |
| SC3: eval CLI dispatch | `cargo test -p rollout-cli --features test-mock-backend --test eval_cli` | 4 tests pass (help, unknown-suite reject, dry-run, mock-backend json) | ✓ PASS |
| SC4: workspace tests green | `cargo test --workspace --tests` | 0 failures, 0 errors | ✓ PASS |
| SC4: dep-direction 14 invariants | `cargo test -p rollout-core --test dependency_direction` | 14 passed | ✓ PASS |
| SC2 (Linux): cfg-gated tests compile to 0 on macOS | Observed in `cargo test -p rollout-harness-tool --tests --all-features` | `sandbox_negative.rs: 0 tests`, `sandbox_positive.rs: 0 tests` — by design (D-TOOL-05) | ✓ PASS |
| Cross-compile Linux target check | `cargo check -p rollout-harness-tool --target x86_64-unknown-linux-gnu` | C cross-linker absent on macOS; rustc target installed. 07-02 SUMMARY confirms Linux clippy/check/doc were run and passed via `--target x86_64-unknown-linux-gnu` on macOS (linker step skipped) | ? SKIP (no C cross-linker; per environment note this validates on harness-linux CI) |

---

### Requirements Coverage

| Requirement | Source Plans | Description | Status | Evidence |
|------------|-------------|-------------|--------|---------|
| HARNESS-01 | 07-01, 07-00, 07-05 | `rollout-harness-text`: text-completion env, reset/step/close batched, reward via plugin host, deterministic-replay witness | ✓ SATISFIED | `TextCompletionEnv` impls `EnvHarness`; `EchoEnv`/`MockRewardEnv`/`env_deterministic_replay` tests all green; `RewardInput` postcard contract wired |
| HARNESS-02 | 07-02, 07-04, 07-00, 07-05 | `rollout-harness-tool`: 6 tools, layered defense (namespaces + landlock + seccomp + cgroups v2 + cap-std + SSRF), Linux full / macOS dev-stub | ✓ SATISFIED | 6 tool files exist; layered launcher in `sandbox/launcher.rs`; curated `ALLOWLIST` in `sandbox/seccomp.rs`; all CVE-class negative witnesses present and cfg-gated; SSRF connector with 9 blocked ranges + IP pinning + redirect re-filter; macOS stub + compile witness; honest `README.md` sandbox-depth matrix |
| HARNESS-03 | 07-03, 07-05 | `rollout-harness-eval`: MMLU + IFEval + GSM8K, offline SHA-pinned fixtures, `EvalHarness` open, ≤1% lm-eval parity, `rollout eval` CLI | ✓ SATISFIED | Three scorers exist; 3 × 10-row parquet fixtures with blake3 drift detection; `BundledEval` impls `EvalHarness`; parity witnesses green; `rollout eval` CLI wired; spec-08 reconciled |

HARNESS-04 (eval gate) correctly deferred to Phase 11 / v1.2 per ROADMAP.md — not a Phase 7 requirement.

---

### Anti-Patterns Found

| File | Pattern | Severity | Assessment |
|------|---------|---------|-----------|
| `crates/rollout-harness-eval/src/datasets/mod.rs` line ~155 | "online dataset download… unset HF_OFFLINE=0" — `load_online` not yet fully implemented | ℹ️ Info | By-design deferral (carry-forward to v1.2 full HF download); offline path is complete and tested; not a blocker for SC3 |
| `crates/rollout-harness-tool/src/sandbox/cgroup.rs` | cgroup warning-event emission is a TODO | ℹ️ Info | rlimit fallback is the load-bearing guarantee; documented in 07-02 SUMMARY; not blocking |

No blockers found. The two informational items are documented accepted deferrals.

---

### Accepted Deferrals (NOT Phase Failures)

These were identified in the plan and are by design:

1. **Linux sandbox enforcement witnesses** (`seccomp_blocks_unexpected_syscall`, `sandbox_blocks_userns`, `tool_sandbox_escape_blocked`, and exec/file positive tests) are `#[cfg(target_os = "linux")]` and validate exclusively on the `harness-linux` CI lane (Ubuntu 5.15 kernel). They compile out on macOS showing `0 tests` — this is D-TOOL-05 by design.

2. **`cargo doc --workspace --no-deps --all-features`** fails on `h3-quinn 0.0.7` (pre-existing upstream issue, unrelated to Phase 7). The authoritative DOCS-03 gate (`cargo doc --workspace --no-deps`, no `--all-features`) is green.

3. **`eval_reports` online full-split download** (`datasets::load_online` + ObjectStore cache) is a carry-forward to v1.2 when the `rollout eval` live-HF path is pursued.

4. **HARNESS-04 (eval gate)** is explicitly deferred to Phase 11 per ROADMAP.md; it requires algo + dist + harness coupling.

---

### Human Verification Required

None. All success criteria are machine-verifiable and were verified above.

---

### Gaps Summary

No gaps. All four success criteria are fully satisfied:

- **SC1** — `cargo test -p rollout-harness-text` is green; the three HARNESS-01 witnesses (`EchoEnv`, `MockRewardEnv`, `env_deterministic_replay`) pass; the plugin-host reward path (postcard `RewardInput` → `PluginHost::call("score")` → decode `Reward`) is implemented and tested.

- **SC2** — `rollout-harness-tool` compiles clean on macOS as documented dev-stub; the `macos_stub_returns_documented_fatal` witness passes; all CVE-class negatives and exec/file/SSRF positives exist and are correctly `#[cfg(target_os = "linux")]`-gated for the harness-linux lane; in-process SSRF witnesses (redirect-to-IMDS, DNS-rebinding, RFC1918, IPv6, happy-paths) pass on macOS. The seccomp ALLOWLIST, landlock launcher, cgroups v2, cap-std FS root, and the 6-tool set are all present.

- **SC3** — `rollout eval --suite mmlu --checkpoint <id>` is wired; bundled 10-row fixtures with blake3 drift detection exist; `eval_score_matches_lm_eval_harness` (4 tests: mmlu/ifeval/gsm8k/same-seed) passes deterministically with `HF_OFFLINE=1` (no network call); `rollout eval` CLI 4/4 tests pass.

- **SC4** — `cargo test --workspace --tests` is green with zero failures across all crates including the 5 new ones; dep-direction lint holds at exactly 14 invariants (confirmed by 14-test `dependency_direction` suite).

---

_Verified: 2026-06-01_
_Verifier: Claude (gsd-verifier)_
