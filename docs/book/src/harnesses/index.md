# Harnesses

Three algo-layer crates that v1.2 PPO/GRPO consume via trait objects:

- **`rollout-harness-text`** (HARNESS-01) — a text-completion RL environment.
- **`rollout-harness-tool`** (HARNESS-02) — six sandboxed tools behind a layered
  best-effort Linux sandbox.
- **`rollout-harness-eval`** (HARNESS-03) — bundled MMLU / IFEval / GSM8K scorers
  mirroring lm-evaluation-harness, driven by the `rollout eval` CLI.

## Three traits, one principle (spec 07 §1)

The three harness kinds share one design rule: **every method is batched.**
`EnvHarness::reset/step/close`, `ToolHarness::invoke`, and `EvalHarness::run`
all take and return vectors so the GPU stays saturated — the harness never
forces a one-at-a-time round trip.

Each trait is constructed the same way: `from_settings(settings, deps)` where
`deps: HarnessDependencies` injects the substrate handles (plugin host, object
store, storage, queue, event emitter, clock). The struct is the stable seam that
keeps the traits unchanged as later phases add capabilities.

## Dependency direction

The harness crates are **algo-layer**: they depend *down* onto `rollout-core`
traits and the shipped substrate, never *up* or *sideways* onto the cloud /
transport crates. The `dep_direction_invariants_hold` lint (14 invariants)
enforces this — `rollout-harness-{text,tool,eval}` are in the `ALGO_AND_ABOVE`
set that may not reach the cloud crates.

## Local-first constraint

Every harness is **testable locally without cloud credentials or a GPU**:

- the env reward path runs against a mock plugin host;
- the tool sandbox compiles to a macOS dev stub and enforces on the Linux lane;
- the eval suites run against offline SHA-pinned fixtures (`HF_OFFLINE=1`) with a
  GPU-free `MockEvalBackend`.

- [Env harness](./env.md)
- [Tool sandbox](./tool-sandbox.md)
- [Eval suites](./eval.md)
- [CLI: rollout eval](./cli.md)
