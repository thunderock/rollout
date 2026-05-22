# Deferred items (Phase 04)

## 04-06 plan (cli-train-snapshot)

**Pre-existing clippy issues in `crates/rollout-cli/tests/restart_no_duplicates.rs`** (introduced by plan 03-05 commit `b09e652`):

- `clippy::format_push_string` at line ~44: `prompts.push_str(&format!(...))` — should use `write!`.
- `clippy::too_many_lines` at line ~82: `restart_resumes_with_zero_duplicates` is 115 lines (limit 100).

These trip `--features test-mock-backend -- -D warnings`. Out of scope for plan 04-06; address via a focused clippy-hygiene commit on the Phase-3 surface.
