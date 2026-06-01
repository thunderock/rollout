---
phase: 07-harnesses-env-tool-eval
plan: 00
subsystem: api
tags: [rust, traits, harness, seccomp, landlock, eval, workspace, ci, json-schema]

# Dependency graph
requires:
  - phase: 02-local-substrate
    provides: PluginHost, Storage, ObjectStore, Queue, EventEmitter, Clock traits + work_item CAS pattern
  - phase: 04-train-sft-rm-snapshots
    provides: Snapshot/ModelRef/SamplingParams types reused by the harness surface
provides:
  - "rollout-core spec-07 EnvHarness/ToolHarness/EvalHarness traits + ~20 associated types"
  - "HarnessDependencies #[non_exhaustive] injection struct + new() constructor"
  - "Three registered workspace member crates: rollout-harness-{text,tool,eval}"
  - "workspace.dependencies pins for the sandbox + eval-dataset stack"
  - "eval_reports Storage row type + key/prefix/encode helpers"
  - "harness-linux CI lane + mandated strace seccomp baseline (artifact for 07-02)"
affects: [07-01-text-env, 07-02-tool-sandbox, 07-03-eval-suites, harness, seccomp, eval]

# Tech tracking
tech-stack:
  added: [rustix=1.1.4, landlock=0.4.5, seccompiler=0.5.0, cap-std=4.0.2, hf-hub@0.4, parquet@55, arrow-array@55]
  patterns: [HarnessDependencies injection struct, non_exhaustive+new() constructor, Linux-gated target deps, eval_reports CAS-row mirror of work_item]

key-files:
  created:
    - crates/rollout-harness-text/{Cargo.toml,src/lib.rs}
    - crates/rollout-harness-tool/{Cargo.toml,src/lib.rs}
    - crates/rollout-harness-eval/{Cargo.toml,src/lib.rs,src/eval_reports.rs}
    - crates/rollout-core/tests/harness_surface.rs
  modified:
    - crates/rollout-core/src/traits/harness.rs
    - crates/rollout-core/src/traits/mod.rs
    - crates/rollout-core/src/lib.rs
    - crates/rollout-core/tests/trait_surface.rs
    - Cargo.toml
    - .github/workflows/ci.yml

key-decisions:
  - "hf-hub BUMPED 0.3 -> 0.4: 0.3.x exposes no rustls-tls feature (pulls native-tls/openssl, cargo-deny ban); 0.4.x adds rustls-tls"
  - "Harness traits carry an associated Settings type, so they are no longer object-safe; Send+Sync checked via generic bounds (PolicyAlgorithm precedent)"
  - "Duration fields serialize via std native serde/JsonSchema (no humantime_serde dep added to rollout-core)"
  - "Sandbox deps Linux-gated via [target.'cfg(target_os = \"linux\")'.dependencies]; eval-dataset deps declared but not yet consumed (Wave-1 07-03)"

patterns-established:
  - "HarnessDependencies: #[non_exhaustive] superset injection struct + new() constructor (mirrors PluginDependencies/AlgoDependencies)"
  - "eval_reports: namespace eval_reports, run-scoped, path [report, <content_id hex>], postcard — exact work_item.rs mirror"
  - "harness-tool overrides workspace unsafe_code=forbid -> deny so the sandbox boundary can #[allow(unsafe_code)]"

requirements-completed: [HARNESS-01, HARNESS-02, HARNESS-03]

# Metrics
duration: 10min
completed: 2026-06-01
---

# Phase 7 Plan 00: Harness Trait Surface + Crate Skeletons + Linux CI Summary

**Replaced the thin v1.0 harness stub with the full spec-07 batched EnvHarness/ToolHarness/EvalHarness surface (~20 typed associated types + HarnessDependencies), materialized the three feature crates as compiling workspace members, pinned the sandbox + eval-dataset stack, added the eval_reports Storage row, and stood up the ubuntu strace/enforcement CI lane.**

## Performance

- **Duration:** ~10 min
- **Started:** 2026-06-01T19:57:45Z
- **Completed:** 2026-06-01T20:07:30Z
- **Tasks:** 3
- **Files modified/created:** 13

## Accomplishments
- spec-07 D-CORE-01 trait surface: batched `reset`/`step`/`close`/`invoke`/`run`, associated `Settings`, `from_settings(settings, deps)`, defaulted `snapshot_episode -> Ok(None)`; old `RewardModel` + single-method stub deleted; no `HarnessGraph`/eval-gate (D-CORE-02/03).
- `HarnessDependencies` `#[non_exhaustive]` struct (plugin_host/object_store/storage/queue/events/clock) + `new()` constructor.
- Three registered workspace members compile; 14-invariant dep-direction lint stays green with them physically present; cargo-deny licenses+bans green (zero new allowlist entries, no openssl).
- `eval_reports` Storage row + key/prefix/encode/decode helpers (work_item.rs mirror) with round-trip + Postgres-key-validity tests.
- `harness-linux` ubuntu-latest CI lane: real `strace -fc /usr/bin/python3 -c 'print(1)'` baseline (uploaded as `strace-seccomp-baseline` artifact) + cfg(linux) harness-tool/text/eval tests; non-gated, HF_OFFLINE set.

## Task Commits

1. **Task 1: spec-07 trait surface + HarnessDependencies** - `f28a271` (feat; TDD — test + impl folded into one commit since the witness test file and surface co-evolved)
2. **Task 2: three crate skeletons + workspace deps + eval_reports** - `a01ae3d` (feat)
3. **Task 3: harness-linux CI lane + strace baseline** - `7462992` (ci)

## Files Created/Modified
- `crates/rollout-core/src/traits/harness.rs` - full spec-07 §2-4 surface + all associated types + HarnessDependencies (replaced 33-line stub)
- `crates/rollout-core/src/traits/mod.rs`, `src/lib.rs` - re-export all new public types; drop `RewardModel`
- `crates/rollout-core/tests/harness_surface.rs` - batched-step / defaulted-snapshot / JsonSchema / typed-ToolCall witnesses
- `crates/rollout-core/tests/trait_surface.rs` - harness traits moved to generic Send+Sync checks (no longer object-safe)
- `crates/rollout-harness-text/*` - HARNESS-01 skeleton (forbid unsafe)
- `crates/rollout-harness-tool/*` - HARNESS-02 skeleton (unsafe deny override + Linux-gated sandbox deps)
- `crates/rollout-harness-eval/*` - HARNESS-03 skeleton + `eval_reports.rs`
- `Cargo.toml` - three members + sandbox/eval-dataset workspace.dependencies pins
- `.github/workflows/ci.yml` - `harness-linux` lane

## Final spec-07 field layouts (Claude's-discretion, recorded per <output>)
- `EpisodeId(Ulid)`, `ToolCallId(Ulid)`; `Observation(String)`, `Action(String)`, `Reward(f32)` — all `#[serde(transparent)]` newtypes.
- `Episode { id, observation, info: Value }`; `EpisodeStep { episode_id, action }`; `StepResult { episode_id, observation, reward: Option<Reward>, done, info: Value }` (verbatim spec §2).
- `SideEffectClass { Pure, Filesystem, Network, Exec }`; `ToolOutcome { Success, Error, TimedOut }`.
- `ToolSpec { name: SmolStr, description, input_schema: Value, side_effects, timeout: Duration }`; `ToolDescriptor { tools }`.
- `ToolContext { worker_id: WorkerId, episode_id: Option<EpisodeId> }` (no span_id in v1.1); `ToolCall { call_id, tool: SmolStr, args: Value, context }`; `ToolResult { call_id, outcome, output: Value, stderr: Option<String>, duration: Duration }`.
- `MetricSpec { name: SmolStr, higher_is_better: bool }`; `MetricValue { Scalar(f64) }`; `ResourceEstimate { est_tasks: Option<u64> }`.
- `EvalDescriptor { name, version: SmolStr, metrics, task_count: Option<u64>, estimated_cost }`; `TaskResult { task_id: SmolStr, score: f64 }`; `EvalContext { sampling: SamplingParams, seed: u64 }`.
- `EvalReport { eval_name, eval_version: SmolStr, model_ref: ModelRef, started_at/completed_at: DateTime<Utc>, metrics: HashMap<SmolStr, MetricValue>, per_task: Vec<TaskResult> }`.

## Resolved dependency versions on MSRV 1.91.1 (recorded per <output>)
Verified against crates.io 2026-06-01:
- `rustix = "=1.1.4"`, `landlock = "=0.4.5"`, `seccompiler = "=0.5.0"`, `cap-std = "=4.0.2"` — all current max-stable, match STACK exactly.
- `parquet = "55"` (resolves 55.2.0, rust_version 1.81 ≤ MSRV), `arrow-array = "55"` (55.2.0) — STACK cohort kept; exposes `async` + `arrow` features.
- **`hf-hub` BUMPED `0.3` -> `0.4`** (deviation, see below). 0.4.x exposes `rustls-tls`; declared `default-features = false, features = ["tokio", "rustls-tls"]`.

## Decisions Made
- Harness traits gained an associated `Settings` type → not object-safe → `trait_surface.rs` switched from `Arc<dyn EnvHarness>` to generic `fn env_harness<T: EnvHarness>()` Send+Sync checks (same pattern Phase 4 used for `PolicyAlgorithm`).
- `Duration` fields use std-native serde + schemars 1.x built-in `JsonSchema` (no new `humantime_serde` dep on rollout-core).
- Eval-dataset deps (hf-hub/parquet/arrow-array) are declared in `[workspace.dependencies]` but NOT yet consumed by any crate — keeps the Wave-0 build slim and cargo-deny clean; Wave-1 07-03 wires them into the eval crate and owns the final openssl-free resolution proof.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] hf-hub 0.3 pin is unsatisfiable under the rustls-only / no-openssl policy**
- **Found during:** Task 2 (workspace dependency pins)
- **Issue:** The plan/STACK pin `hf-hub = { version = "0.3", features = ["tokio", "rustls-tls"] }` does not resolve — `hf-hub 0.3.x` has NO `rustls-tls` feature (its features are `default`/`online`/`tokio`, and `online`/`tokio` pull `native-tls`/`reqwest`-default → openssl), violating the cargo-deny ban (RESEARCH Pitfall G). The plan explicitly authorized: "if `0.3`/`55` do not resolve cleanly on 1.91, bump to the nearest resolvable cohort and record the exact pin in the SUMMARY."
- **Fix:** Bumped to `hf-hub = { version = "0.4", default-features = false, features = ["tokio", "rustls-tls"] }`. 0.4.x (verified 0.4.0–0.4.3 on crates.io) adds the `rustls-tls` feature; `default-features = false` drops `default-tls`/native-tls.
- **Files modified:** Cargo.toml
- **Verification:** `cargo deny check licenses bans` → `bans ok, licenses ok` (no openssl in the graph; hf-hub is declared-but-unconsumed in Wave 0). Final openssl-free resolution is re-asserted by Wave-1 07-03 when the eval crate actually consumes hf-hub.
- **Committed in:** a01ae3d (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking/version-resolution).
**Impact on plan:** The bump was pre-authorized by the plan's integration-verification clause; no scope creep. All other pins match STACK exactly.

## Issues Encountered
- None beyond the hf-hub pin above. `cargo fmt` reordered imports/members (expected); clippy `doc_markdown` flagged two `HarnessGraph` mentions needing backticks — fixed before the Task 1 commit.

## Strace seccomp baseline — handoff to Wave-1 07-02
The D-TOOL-07-mandated `strace -c` baseline **cannot be produced on this macOS dev box** (strace is Linux-only). Task 3 instead wired the real strace into the `harness-linux` ubuntu-latest CI lane:
```
strace -fc /usr/bin/python3 -c 'print(1)' 2>strace.txt; cat strace.txt
```
Its output is uploaded as the CI artifact **`strace-seccomp-baseline`**. **07-02 executor:** pull that artifact from the first `harness-linux` run and diff its syscall list against `rollout-harness-tool::seccomp::ALLOWLIST` (RESEARCH expects 2-5 missing post-2020 syscalls — `clone3`/`openat2`/`faccessat2`/`rseq`/`arch_prctl`/`rt_sig*` — to add).

## Known Stubs
The three crates are intentional Wave-0 skeletons (documented in each crate's `//!` doc + this plan's objective):
- `rollout-harness-text/src/lib.rs` — placeholder unit test; `TextCompletionEnv` impl lands in 07-01.
- `rollout-harness-tool/src/lib.rs` — placeholder unit test; sandbox launcher/seccomp/cgroup + six tools + macOS stub land in 07-02.
- `rollout-harness-eval/src/lib.rs` — `eval_reports` is real (with tests); MMLU/IFEval/GSM8K scorers, `rollout eval` CLI, hf-hub loader land in 07-03.

These are by-design per the plan (Wave-0 enabler; all three Wave-1 plans depend on this one). Not blocking — the plan's goal is the contract + skeletons + CI, all delivered and verified.

## Next Phase Readiness
- Wave-1 (07-01 text env, 07-02 tool sandbox, 07-03 eval suites) can now proceed in parallel against the locked trait surface.
- 07-02 needs the CI strace artifact from the first `harness-linux` run to finalize the seccomp allowlist.
- 07-03 owns the final hf-hub/parquet rustls-only resolution proof when it consumes those deps.

## Self-Check: PASSED
All claimed files exist on disk; all three task commits (f28a271, a01ae3d, 7462992) are in the git log.

---
*Phase: 07-harnesses-env-tool-eval*
*Completed: 2026-06-01*
