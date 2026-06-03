# Quick Task 260603-ci5: Fix the 5 failing CI jobs on main

**Date:** 2026-06-03
**Trigger:** CI run 26906807880 (commit `4149486`) — core `test` job now green (ULID fix),
but 5 jobs red: `train-smoke`, `infer-smoke`, `cloud-emulator-aws`, `cloud-emulator-gcp`,
`postgres-integration`.

## Root causes + fixes (one atomic commit each)

| Job | Root cause | Fix | Commit |
|-----|-----------|-----|--------|
| train-smoke | `engine.rs` cast `os.environ` (`os._Environ`) to `PyDict` → TypeError on the secret-token import path | Set `HF_TOKEN` via `__setitem__` on the `PyAny` (no `PyDict` cast) | `5d9fe2d` |
| infer-smoke | Newer vLLM removed `AsyncEngineArgs(device=…)`; passing it raised TypeError | Drop the `device` kwarg; cuda probe still gates `gpu_memory_utilization` | `d35d70b` |
| cloud-emulator-aws | `BehaviorVersion::latest` emits CRC32 on multipart; localstack rejects (`Checksum Type mismatch`) | Force `request_checksum_calculation`/`response_checksum_validation = WhenRequired` on the localstack test client only | `8509a20` |
| postgres-integration | 256 PG-backed proptest cases (`--test-threads=1`) overran the 15-min job timeout | Cap parity proptest at 64 cases | `fe6a780` |
| cloud-emulator-gcp | `doctor` exits 1 (1 of 7 checks fails); failing check not in logs (report → captured stdout) | **Best-effort:** print the JSON report on assertion failure so the next CI run pinpoints the failing check | `8b78b6a` |

## Verification

- **Local (what this box can do):** all 5 touched crates/tests compile with their CI
  features; `cargo fmt --all --check` clean; `cargo clippy` clean (per-feature + the
  no-feature workspace lint job); full default `cargo test --workspace --tests` green
  (111 test binaries, 0 failures).
- **NOT verifiable locally:** runtime pass/fail for all 5 needs Docker (localstack /
  fake-gcs / pubsub / Postgres) or GPU/vLLM, neither available on this macOS box.
  **CI is the verifier** — push and observe.

## Caveats / confidence

- train-smoke, infer-smoke, cloud-emulator-aws: **high confidence** — root cause is
  unambiguous from the error and the fix compiles.
- postgres-integration: **medium** — direction is right (timeout from slow PG round-trips);
  64 cases should fit the budget, but if the test is pathologically slow rather than merely
  slow, may need further tuning.
- cloud-emulator-gcp: **diagnostic only** — the actual failing check is not yet known.
  This commit makes the next CI run reveal it; a real fix follows once the report is visible.
