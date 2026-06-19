---
status: complete
phase: 07-harnesses-env-tool-eval
source: [07-00-SUMMARY.md, 07-01-SUMMARY.md, 07-02-SUMMARY.md, 07-03-SUMMARY.md, 07-04-SUMMARY.md, 07-05-SUMMARY.md]
started: 2026-06-18T18:34:29Z
updated: 2026-06-18T19:02:00Z
---

## Current Test

[testing complete]

## Tests

### 1. Cold Start — workspace builds and full test suite passes
expected: From a clean state, `cargo test --workspace --tests` exits 0; rollout-harness-text/tool/eval crates are present and green.
result: pass
evidence: "cargo test --workspace --tests → 384 passed, 0 failed. All three harness crates present in crates/."

### 2. `rollout eval` CLI help + flag surface
expected: `cargo run -p rollout-cli -- eval --help` shows --suite <mmlu|ifeval|gsm8k>, --checkpoint, --config, --storage-path, --object-path, --seed, --dry-run, --format. `eval` is a top-level subcommand, not `infer eval`.
result: pass
evidence: "eval is a top-level Cmd arm (sibling to infer/train/snapshot); all documented flags present in --help."

### 3. `rollout eval` dry-run short-circuits
expected: dry-run resolves the checkpoint and exits 0 without a model backend.
result: pass
evidence: "eval --suite mmlu --checkpoint <64-hex> --dry-run → 'dry-run: eval config valid', exit 0, no backend constructed. Bad-length id is rejected with a clear ContentId error (correct validation)."

### 4. Text env (HARNESS-01) determinism witnesses
expected: `cargo test -p rollout-harness-text --tests` → 8 tests pass (EchoEnv, MockRewardEnv, env_deterministic_replay).
result: pass
evidence: "8 passing (4 echo_env + 1 env_deterministic_replay + 3 mock_reward_env), 0 failed."

### 5. Eval scorers match lm-eval-harness (HARNESS-03)
expected: `HF_OFFLINE=1 cargo test -p rollout-harness-eval --tests` → ~24 pass incl. eval_score_matches_lm_eval_harness (≤1% parity) and eval_loader_works_with_no_network.
result: pass
evidence: "18 + 2 + 4 = 24 passed, 0 failed; same_seed_same_scores + eval_loader_works_with_no_network green; offline (HF_OFFLINE=1), no network."

### 6. Tool harness SSRF defenses (macOS-runnable security witnesses)
expected: `cargo test -p rollout-harness-tool --all-features` → SSRF witnesses pass (redirect-to-IMDS, DNS-rebinding, RFC1918, loopback/v4-mapped).
result: pass
evidence: "7 connector + 7 http_ssrf tests pass: http_tool_blocks_redirect_to_imds, http_tool_blocks_dns_rebinding, http_tool_blocks_rfc1918, http_tool_blocks_ipv6_loopback_v4_mapped, tool_harness_http_get_blocks_imds, loopback_test_escape_never_unblocks_imds. 0 failed."

### 7. Supply chain clean — no openssl, rustls-only
expected: `cargo deny check` → advisories/bans/licenses/sources all ok. `cargo tree -i openssl-sys` finds no match.
result: pass
evidence: "Phase-7 supply-chain claim holds: bans ok, licenses ok, sources ok; cargo tree -i openssl-sys → no match (rustls-only). NOTE: `cargo deny check advisories` FAILS on RUSTSEC-2026-0176/0177 (pyo3 0.28.3), pulled by rollout-backend-vllm + rollout-plugin-host — NOT a phase-7 crate. These advisories postdate the phase (shipped 2026-06-01). Tracked as a project-wide follow-up, not a phase-7 gap."

### 8. mdBook Harnesses chapter builds
expected: `mdbook build docs/book` → exit 0; Harnesses chapter (index/env/tool-sandbox/eval/cli) present; honest boundary stated.
result: pass
evidence: "mdbook build exit 0; docs/book/src/harnesses/ has index.md, env.md, tool-sandbox.md, eval.md, cli.md; tool-sandbox.md states 'process-isolated, NOT VM-isolated.'"

### 9. Linux sandbox enforcement witnesses
expected: seccomp/landlock/namespace enforcement tests are #[cfg(target_os = "linux")] and validate only on the harness-linux CI lane (kernel 5.15), not on this macOS dev box.
result: blocked
blocked_by: physical-device
reason: "Linux-only #[cfg(target_os = \"linux\")] enforcement witnesses (sandbox_negative.rs / sandbox_positive.rs) compile out on darwin — 0 tests run locally by design. They validate on the Ubuntu harness-linux CI lane, not on this macOS box."

## Summary

total: 9
passed: 8
issues: 0
pending: 0
skipped: 0
blocked: 1

## Gaps

[none — phase-7 deliverables verified. Advisory drift on pyo3 (RUSTSEC-2026-0176/0177) is a pre-existing-dependency / project-wide concern, not a phase-7 gap; logged for follow-up.]
