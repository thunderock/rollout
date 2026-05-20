"""Raw-vLLM tokens/sec baseline for the BACKEND-02 perf exit criterion.

Run this on the same host as
``cargo bench -p rollout-backend-vllm --features vllm --bench throughput``
and compare the tokens/sec numbers. The exit criterion is a ratio >= 0.9
(rollout overhead <= 10%); CI does not gate on it — the comparison lives
on the self-hosted GPU runner per CONTEXT D-CLI-05.

Usage:
    python scripts/raw_vllm_baseline.py [model_uri]

The default model is facebook/opt-125m to match the criterion bench.
"""

from __future__ import annotations

import sys
import time


def main(argv: list[str]) -> int:
    model_uri = argv[1] if len(argv) > 1 else "facebook/opt-125m"

    # Imported lazily so `python scripts/raw_vllm_baseline.py --help` works
    # in the absence of vllm.
    try:
        from vllm import LLM, SamplingParams
    except ImportError as e:
        print(f"vllm not installed: {e}", file=sys.stderr)
        return 2

    llm = LLM(model=model_uri)
    sp = SamplingParams(max_tokens=64, seed=42)
    prompts = [f"hello {i}" for i in range(64)]

    t0 = time.perf_counter()
    outs = llm.generate(prompts, sp)
    t1 = time.perf_counter()

    n_tokens = sum(len(o.outputs[0].token_ids) for o in outs)
    elapsed = t1 - t0
    rate = n_tokens / elapsed if elapsed > 0 else 0.0
    print(f"{rate:.2f}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
