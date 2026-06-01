---
phase: 07-harnesses-env-tool-eval
plan: 01
subsystem: harness
tags: [rust, harness, env, rl, plugin-host, reward, determinism, seeded-rng]

# Dependency graph
requires:
  - phase: 07-harnesses-env-tool-eval
    plan: 00
    provides: spec-07 EnvHarness trait + Episode/EpisodeStep/StepResult/Observation/Action/Reward/EpisodeId + HarnessDependencies
  - phase: 02-local-substrate
    provides: PluginHost::call (reward plugin path D-ENV-03)
provides:
  - "rollout-harness-text: TextCompletionEnv (EnvHarness impl — batched reset/step/close)"
  - "EpisodeStore (in-memory Mutex<HashMap>, per-episode SplitMix64 seeded RNG)"
  - "reward::compute_reward — plugin-host reward path + RewardInput postcard contract"
  - "Three witnesses: EchoEnv, MockRewardEnv, env_deterministic_replay"
affects: [07-04-text-env-wiring, rl-loop-v1.2, harness]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "SplitMix64 inline deterministic RNG (no `rand` dep — avoids cargo-deny churn)"
    - "seed XOR episode_index per-episode seeding (v1.0 determinism pattern)"
    - "test-only mock PluginHost + no-op substrate stubs as dev-deps (dep-direction lint ignores dev deps)"

key-files:
  created:
    - crates/rollout-harness-text/src/episode.rs
    - crates/rollout-harness-text/src/reward.rs
    - crates/rollout-harness-text/tests/echo_env.rs
    - crates/rollout-harness-text/tests/mock_reward_env.rs
    - crates/rollout-harness-text/tests/env_deterministic_replay.rs
    - crates/rollout-harness-text/tests/support/mod.rs
  modified:
    - crates/rollout-harness-text/src/lib.rs
    - crates/rollout-harness-text/Cargo.toml
    - Cargo.lock

key-decisions:
  - "Inline SplitMix64 RNG instead of adding `rand` — keeps the dependency graph and cargo-deny allowlist unchanged; gives a fixed seed a fully reproducible stream"
  - "Trajectory determinism is compared over (observation, reward, done, info) — episode_id is a fresh ULID per reset and is normalized out of the replay equality check"
  - "step folds a seeded-RNG nonce into StepResult.info so the seed materially changes the trajectory (otherwise canned/plugin reward is seed-independent and `different seed differs` would be vacuous)"
  - "Tests wire a mock PluginHost + no-op ObjectStore/Storage/Queue/EventEmitter/Clock as dev-dependencies; the dep-direction lint only inspects Normal deps so this stays lint-clean"

requirements-completed: [HARNESS-01]

# Metrics
duration: 25min
completed: 2026-06-01
---

# Phase 7 Plan 01: rollout-harness-text (HARNESS-01) Summary

**Built `rollout-harness-text`: a multi-turn-capable text-completion `EnvHarness` (`Observation = prompt`, `Action = completion`) with an in-memory episode store, a plugin-host reward path (postcard `RewardInput` → `call("score")` → decode `Reward`, decode-failure → typed `Fatal(PluginContract)`), and per-episode seeded determinism — shipped with the three HARNESS-01 witnesses (EchoEnv, MockRewardEnv, env_deterministic_replay) all green, GPU-free and plugin-free.**

## Performance

- **Duration:** ~25 min
- **Started:** 2026-06-01T20:12:31Z
- **Completed:** 2026-06-01T20:37:27Z
- **Tasks:** 2
- **Files modified/created:** 9

## Accomplishments
- `TextCompletionEnv` impls the spec-07 `EnvHarness` surface: batched `reset` (mints `EpisodeId(Ulid::new())` per prompt, observation echoes the prompt), `step` (per-`EpisodeStep` turn increment, canned-or-plugin reward, `done = turn >= max_turns`), `close` (removes from the map).
- `EpisodeStore` = `tokio::sync::Mutex<HashMap<EpisodeId, EpisodeState>>` with `EpisodeState { prompt, turn, max_turns, rng: SplitMix64 }`. No blob-store persistence (D-ENV-02) — `StepResult`s are in-memory only.
- Multi-turn capability (D-ENV-01): `max_turns` budget + turn counter; a single episode steps N times before `done`.
- Per-episode error isolation (spec §7): a missing/closed id yields an error entry (`info.error`, `done=true`, `reward=None`) while sibling episodes in the same batch succeed.
- Reward via plugin host (D-ENV-03): `reward::compute_reward(deps, handle, prompt, completion)` exactly per RESEARCH Pattern 2 — `postcard::to_stdvec(RewardInput)` → `plugin_host.call(handle, "score", payload)` → `postcard::from_bytes::<Reward>` with both encode and decode failures mapped to `Fatal(PluginContract { plugin, msg })`.
- Determinism (D-ENV-03): `Settings.seed: Option<u64>` threaded into each episode's `SplitMix64` via `seed XOR episode_index`; `step` draws a nonce into `StepResult.info` so the trajectory is a function of the seed.

## Task Commits

1. **Task 1: TextCompletionEnv + episode store + EchoEnv** — `7e4f656` (feat)
2. **Task 2: plugin-host reward path (MockRewardEnv) + deterministic replay** — `33ddf9f` (feat)

## `TextEnvSettings` shape
```rust
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct TextEnvSettings {
    pub max_turns: u32,        // D-ENV-01 multi-turn budget; Default = 1
    pub echo_reward: Option<f32>, // canned reward (EchoEnv) when no plugin
    pub seed: Option<u64>,     // D-ENV-03 per-episode RNG seed; None ⇒ 0
}
```

## `RewardInput` postcard contract
```rust
#[derive(Serialize, Deserialize, PartialEq, Eq)]
pub struct RewardInput { pub prompt: String, pub completion: String }
// wire: postcard::to_stdvec(RewardInput) → plugin_host.call(handle, "score", _) → postcard Reward(f32)
```

## Seeded-RNG determinism approach
- No `rand` dependency. `episode::SplitMix64` is an inline ~6-line generator (golden-ratio increment + xor-shift-multiply finalizer) seeded per episode with `settings.seed.unwrap_or(0) ^ episode_index`.
- `step` draws `rng.next_u64()` and folds it into `StepResult.info.nonce`, so identical (seed, prompts, actions) ⇒ identical observations/rewards/done/info.
- `env_deterministic_replay` serializes `(observation, reward, done, info)` per step (dropping the non-deterministic `episode_id` ULID) and asserts byte-equality for the same seed, byte-inequality for a different seed.

## Verification (all green, GPU-free, cloud-free)
- `cargo test -p rollout-harness-text --tests` → 8 passing (4 echo_env, 1 env_deterministic_replay, 3 mock_reward_env).
- `cargo clippy -p rollout-harness-text --all-targets -- -D warnings` → clean.
- `RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" cargo doc -p rollout-harness-text --no-deps --all-features` → clean (DOCS-03).
- `cargo fmt -p rollout-harness-text -- --check` → clean.
- `cargo test -p rollout-core --test dependency_direction` → 14 invariants hold (harness-text deps add nothing forbidden; dev-only mocks bypass the lint by design).
- `cargo deny check licenses bans` → `bans ok, licenses ok` (no new deps — postcard/schemars/ulid/futures already in the workspace).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] No `rand` crate in the workspace for the seeded RNG**
- **Found during:** Task 2 (deterministic replay)
- **Issue:** The plan suggested seeding a `StdRng`/`SmallRng`, but `rand` is not a `[workspace.dependencies]` entry and adding it would touch the cargo-deny allowlist + dependency graph for a single 64-bit stream.
- **Fix:** Implemented `SplitMix64` inline in `episode.rs` (deterministic, dependency-free). Same-seed reproducibility is exercised by `env_deterministic_replay`.
- **Files modified:** crates/rollout-harness-text/src/episode.rs
- **Commit:** 7e4f656

**2. [Rule 1 - Bug] Canned/plugin reward is seed-independent → `different seed differs` would be vacuous**
- **Found during:** Task 2
- **Issue:** With a fixed canned reward and echoed observation, the trajectory does not depend on the seed at all, so the "different seed → divergent trajectory" assertion could pass trivially or fail to be meaningful.
- **Fix:** `step` now draws `rng.next_u64()` and folds the nonce into `StepResult.info`, making the trajectory a genuine function of the seed while keeping observations/rewards stable for the canned path.
- **Files modified:** crates/rollout-harness-text/src/lib.rs
- **Commit:** 33ddf9f

**Total deviations:** 2 auto-fixed (1 blocking, 1 correctness). No architectural changes; no scope creep.

## Issues Encountered
- clippy `doc_markdown` flagged `EchoEnv`/`MockRewardEnv`/`SplitMix64` needing backticks, and `float_cmp` flagged exact `assert_eq!` on `f32` rewards — both fixed (backticks + `(x - y).abs() < f32::EPSILON`) before the Task 1 commit. rustfmt reformatted the test support module (expected).

## Known Stubs
None that block the plan's goal. `TextCompletionEnv::from_settings` constructs the plugin-free (canned) variant; the reward-plugin variant is constructed via `with_reward_plugin` (used by `MockRewardEnv`). Wiring a *real* reward plugin handle from TOML config is a later-plan concern (07-04 / RL-03) — the contract and both code paths are exercised here.

## Next Phase Readiness
- HARNESS-01 is complete and witnessed. The locked `EnvHarness` contract + reward-via-plugin path are proven GPU-free / cloud-free.
- Parallel Wave-1 plans 07-02 (tool sandbox) and 07-03 (eval suites) are unaffected by this plan.
- v1.2 PPO/GRPO can consume `TextCompletionEnv` via the `EnvHarness` trait object; the multi-turn step loop needs no contract change for conversational envs.

## Self-Check: PASSED
All six created files + two modified source files exist on disk; both task commits (7e4f656, 33ddf9f) are in the git log.

---
*Phase: 07-harnesses-env-tool-eval*
*Completed: 2026-06-01*
