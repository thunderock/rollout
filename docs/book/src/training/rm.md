# Reward-model training (RM)

`rollout-algo-rm` implements the Bradley-Terry reward-model training algorithm
(TRAIN-02). It mirrors the SFT algorithm's structure — `PolicyAlgorithm` impl
driven by a `TrainableBackend`, JSONL data loader, and a TRAIN-03
byte-compare resume proof — but consumes pairwise preferences instead of
single sequences.

## Overview

A reward model learns to score responses on a scalar "quality" axis. Training
data is a stream of preference pairs `(prompt, chosen, rejected)`: the model
should learn to rank `chosen` higher than `rejected` for the given `prompt`.

The Bradley-Terry objective formalizes this as a pairwise logistic regression
on the reward gap:

```text
L = -E[ ln σ(r_chosen - r_rejected) ]
```

where `σ` is the logistic function and `r_*` are the scalar reward outputs.
Spec 02 §7 carries the contract.

## `RmSettings` (TOML)

```toml
[algorithm.rm]
base_model = "Qwen/Qwen2.5-0.5B-Instruct"
head = "bradley_terry"          # Phase 4 supports BradleyTerry only
minibatch_size = 8

[algorithm.rm.optimizer]
kind = "sgd"
lr = 1.0e-5

[algorithm.rm.budget]
max_steps = 100

[algorithm.rm.dataset]
type = "jsonl_path"
path = "examples/data/pairs.jsonl"
```

Other `RmSettings` fields (`base_model`, `optimizer`, `budget`, `dataset`)
mirror `SftSettings`. Head selection is `bradley_terry` only in Phase 4;
`pairwise_logistic` is a `Fatal(ConfigInvalid)` with a `Phase 9` sentinel
until the RL pipeline lands.

## Bradley-Terry loss math

Implemented in `crates/rollout-algo-rm/src/loss.rs`:

- `logsigmoid(x) = ln σ(x)`. Numerically stable via the softplus trick —
  `logsigmoid(50)` and `logsigmoid(-50)` both return finite values within `1e-4`
  of the true asymptote.
- `bradley_terry_loss(r_chosen, r_rejected) = -logsigmoid(r_chosen - r_rejected)`.
- `bradley_terry_batch_mean(pairs)` — mean over a slice of `(r_chosen, r_rejected)`
  pairs. Returns `0.0` for empty batches; callers should validate non-empty
  upstream when needed.

Pinned golden values (`tests/bradley_terry_loss.rs`):

| Case                       | Inputs              | Expected                              |
| -------------------------- | ------------------- | ------------------------------------- |
| Zero diff                  | `(1.0, 1.0)`        | `ln 2 ≈ 0.6931`                       |
| Strong preference          | `(5.0, -5.0)`       | near 0 (≪ 1e-3)                       |
| Inverted preference        | `(-5.0, 5.0)`       | ≈ 10.0 (± 1e-3)                       |
| Mixed batch                | `[(2,1), (1,2)]`    | mean ≈ 0.8133                         |
| Empty batch                | `&[]`               | exactly 0.0                           |
| Numerical stability        | `logsigmoid(±50)`   | finite; asymptotic value within 1e-4  |

## JSONL data shape (D-DATA-01)

Phase 4 supports one row shape:

```jsonl
{"prompt": "What is 2+2?", "chosen": "4", "rejected": "5"}
{"prompt": "Capital of France?", "chosen": "Paris", "rejected": "London"}
```

`load_pairs(&path)` parses line-by-line, skipping blank lines and rejecting
malformed rows with `Fatal(ConfigInvalid)` prefixed `<file>:<lineno>:`. A row
missing any of the three fields is malformed.

## `PolicyAlgorithm` surface

| Method            | Behavior                                                                                       |
| ----------------- | ---------------------------------------------------------------------------------------------- |
| `id()`            | `AlgorithmId("rm")`                                                                            |
| `Settings`        | `rollout_core::config::training::RmSettings`                                                    |
| `from_settings`   | clones `deps.backend` into the algo; `step = 0`                                                |
| `required_roles`  | `vec![WorkerRole::LearnerWorker]`                                                              |
| `validate_plan`   | rejects `RmHeadKind::PairwiseLogistic` (Phase 9); rejects `minibatch_size == 0`; `lr <= 0`     |
| `run`             | loads pairs once; loops `step_once` up to `budget.max_steps`, honoring `ctx.cancel`            |
| `snapshot_save`   | meta = `{step, weights_id}`; one `SnapshotPart { role: "weights" }`                            |
| `snapshot_restore`| restores `self.step` from `meta.step`; backend weights restored separately                     |

`step_once` synthesizes a 2-row `TrainBatch` (one row per side of a pair) and
drives `forward_with_loss` → `optimizer_step`. In the Phase-4 `MockBackend`
test path the loss is a constant; the real Bradley-Terry loss fires under
plan 04-05's HF transformers integration.

## TRAIN-03 second-witness — byte-compare resume

`tests/snapshot_resume.rs::bit_identical_resume_at_step_5` is the Bradley-Terry
twin of the SFT byte-compare proof. Structure:

1. **Run A.** 10 `step_once` iterations with `seed = 42`; capture weights.
2. **Run B Phase 1.** 5 steps; capture mid-run weights; `snapshot_save()`.
3. **Run B Phase 2.** Rebuild `MockBackend::new_train_with_weights(42, …)`;
   push step counter to 5; restore algo step from snapshot meta; 5 more steps.
4. **Assert.** `weights_a == weights_b` byte-for-byte.

This is the second-witness for TRAIN-03 (the SFT proof is the first witness);
together they discharge the "deterministic resume" exit criterion across both
Phase-4 algorithms.

## Content-addressed final checkpoint

`tests/checkpoint_roundtrip.rs` proves that `TrainableBackend::save_weights`
returns a `ContentId` that is stable when the backend is idle (two calls →
identical hash) and different after a non-trivial `optimizer_step`. This
matches the TRAIN-02 contract: the final checkpoint is content-addressed by
the blake3 hash of the postcard-encoded weights.

## Phase 4 head support

Only `RmHeadKind::BradleyTerry` is wired in Phase 4. `PairwiseLogistic` exists
in the enum so the config schema can be cross-validated end-to-end, but
selecting it returns a `Fatal(ConfigInvalid)` with the string `Phase 9` in the
message — Phase 9 lands the full RL pipeline including alternate preference
heads.

## What lands later

- **Plan 04-05** swaps `MockBackend` for the real HF transformers / accelerate
  training loop on `Qwen/Qwen2.5-0.5B-Instruct` (CPU), wiring the Python-side
  `F.logsigmoid(r_chosen - r_rejected).neg().mean()` and producing real reward
  models.
- **Plan 04-06** mounts `rollout train rm --config <toml>` on `RmAlgo::run`.
- **Phase 9** lands `PairwiseLogistic` and the RL-* algorithms (PPO/GRPO) that
  consume reward models trained here.

## See also

- [Snapshots](./snapshots.md) — `SnapshotterImpl` / `SnapshotKind::TrainState`
- [SFT](./sft.md) — sibling algorithm sharing the same trait surface
- Spec 02 §7 — RM contract
