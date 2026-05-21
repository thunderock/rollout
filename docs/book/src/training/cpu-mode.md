# CPU mode

The Phase-4 training surface runs on CPU end-to-end. This is the integration
test path on dev boxes (including Apple Silicon) and the smoke recipe target
in plan 04-07.

## When to use

- Local dev loop on a laptop without CUDA.
- CI smoke that exercises the full HF transformers + accelerate path against
  a tiny model (`Qwen/Qwen2.5-0.5B-Instruct`).
- Reproducing CUDA bugs that turn out to be deterministic-flag misconfiguration.

## Expected throughput

| Model                       | Hardware            | Steps/sec |
| --------------------------- | ------------------- | --------- |
| `Qwen/Qwen2.5-0.5B-Instruct` | Apple M2 Max (CPU)  | ~0.1–0.3  |
| `Qwen/Qwen2.5-0.5B-Instruct` | Linux x86_64 16-core | ~0.3–1.0 |

Roughly one to ten seconds per step for the 0.5B model. Anything larger is
impractical on CPU; the per-token cost grows superlinearly. CPU mode exists
to prove the pipeline, not to train.

## Required env

None beyond default. The Phase-4 determinism preamble
(`CUBLAS_WORKSPACE_CONFIG`, `PYTHONHASHSEED`) is written by the Rust side
before `import torch`; CPU runs ignore CUBLAS settings without complaint.

The live tests gate on `ROLLOUT_TRANSFORMERS_AVAILABLE=1`:

```bash
pip install transformers>=4.45 accelerate>=0.34 torch>=2.4
ROLLOUT_TRANSFORMERS_AVAILABLE=1 \
  cargo test -p rollout-backend-vllm --features train \
  --test snapshot_resume_live -- --ignored --nocapture
```

## Performance caveats

- **No streaming.** Phase 4 rejects `sampling.stream = true` at the boundary
  (D-BACKEND-03); training has no streaming surface.
- **No multi-GPU.** CPU mode is single-process. The FSDP plugin in
  `init_train` only activates when `torch.cuda.device_count() >= 2`.
- **Slow.** The 0.5B model at one step per ~5 seconds on M-series silicon
  means a 10-step smoke takes a minute. Plan accordingly.
- **Determinism still holds.** Two CPU runs with the same seed produce
  byte-identical `accelerate.save_state` output. The `MockBackend` variant in
  `rollout-algo-sft::tests::snapshot_resume::bit_identical_resume_at_step_5`
  proves the Phase-4 contract holds on CPU without HF transformers installed.

## Smoke recipe (plan 04-07)

`make train-smoke` (lands in plan 04-07) runs the live witness on dev boxes
where `ROLLOUT_TRANSFORMERS_AVAILABLE=1` is set. CI does not install
transformers/accelerate; the `MockBackend` test is the unconditional gate.

## Related

- [Determinism](./determinism.md) — the determinism contract Phase-4 commits to.
- [SFT](./sft.md) — algorithm-side overview.
- [Snapshots](./snapshots.md) — snapshot pipeline.
