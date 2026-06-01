# Env harness (HARNESS-01)

`rollout-harness-text` is the bundled `EnvHarness`: a text-in / text-out
environment where `Observation = prompt` and `Action = completion`.

## Batched lifecycle

```text
reset(Vec<Prompt>)      -> Vec<Episode>       # open episodes
step(Vec<EpisodeStep>)  -> Vec<StepResult>    # one turn each
close(Vec<EpisodeId>)   -> ()                 # release episode state
```

All three methods are batched (spec 07 principle 2). Episode state lives in an
in-memory store keyed by `EpisodeId` (a fresh ULID per `reset`).

## Multi-turn capable (D-ENV-01)

The step loop supports **N steps per episode**. The bundled text env is
single-turn text completion, but the contract carries `EpisodeStep` +
per-episode state across multiple steps so v1.2 conversational / tool-using
environments need no contract change.

## Reward via the plugin host (D-ENV-03)

There is **no built-in reward trait**. On `step`, if a reward plugin is
configured, the env encodes a postcard `RewardInput { prompt, completion }`,
calls `PluginHost::call(handle, "score", payload)`, and decodes a `Reward`. A
decode failure surfaces as a typed `Fatal(PluginContract)`.

This reuses the Phase-2 `rollout-plugin-host` (cdylib / PyO3 / sidecar) — the
env crate never embeds scoring logic.

## No trajectory persistence in v1.1 (D-ENV-02)

`step` returns `StepResult`s **in memory**. There is no `ObjectStore`
persistence of trajectories in v1.1; the content-addressed `Trajectory` type +
serializer lands with RL-03 in v1.2.

## Witnesses

- **`EchoEnv`** — canned reward, no plugin (the trivial happy path).
- **`MockRewardEnv`** — wires a deterministic mock plugin handle to exercise the
  plugin-host reward path end-to-end.
- **`env_deterministic_replay`** — a seeded `SplitMix64` RNG (no `rand` dep)
  folds a nonce into `StepResult.info` so the seed materially changes the
  trajectory; same seed reproduces the trajectory `(observation, reward, done,
  info)`, a different seed diverges. (`episode_id` is normalized out of the
  equality check since it is a fresh ULID per reset.)

All three are GPU-free and need no cloud credentials.
