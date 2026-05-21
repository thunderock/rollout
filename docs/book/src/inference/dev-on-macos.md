# Dev loop on macOS (Apple Silicon)

vLLM has no Apple-Silicon wheel as of Phase 3 (see `cpu-mode.md`). That makes the full `rollout infer batch` smoke loop unavailable natively on macOS dev boxes. This chapter documents the two supported workarounds and the substantial **default-CI surface** that *does* run on macOS so you don't need a Linux box for routine work.

## What runs natively on macOS

The following all pass on `darwin-aarch64` with `PYO3_PYTHON=/opt/homebrew/bin/python3.13` (or a 3.11+ python on PATH) — no vLLM required:

```bash
cargo test --workspace --tests              # 100+ tests across Phase 2 + Phase 3
cargo test -p rollout-cli --features test-mock-backend --test restart_no_duplicates
make smoke                                  # Phase 2 substrate smoke
cargo clippy --workspace --all-targets -- -D warnings
cargo doc   --workspace --no-deps           # rustdoc gate (without --all-features; see ci.yml)
mdbook build docs/book
```

What does **not** run natively:

```bash
make infer-smoke ROLLOUT_VLLM_AVAILABLE=1   # needs `pip install vllm`, which has no aarch64-darwin wheel
cargo test -p rollout-backend-vllm --features vllm --test vllm_init -- --include-ignored
cargo bench -p rollout-backend-vllm --bench throughput
```

For the load-bearing exit-criterion-(b) proof (zero-duplicate restart), the `MockBackend`-driven test runs natively in ~1.5 s. Live vLLM is only needed for exit criterion (a) (the canonical `rollout infer batch --config examples/batch-tiny.toml`) and (c) (the <10 % overhead benchmark).

## Workaround 1: Docker (recommended)

Run rollout inside a Linux container that has vLLM pre-installed. The repo is mounted via a bind volume so edits flow through immediately.

Sample `Dockerfile.devcontainer`:

```dockerfile
FROM rust:1.88-slim-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
      python3.11 python3-pip git make pkg-config libssl-dev protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

RUN pip3 install --break-system-packages 'vllm>=0.10,<0.22'

WORKDIR /workspace
CMD ["bash"]
```

```bash
docker build -t rollout-dev -f Dockerfile.devcontainer .
docker run --rm -it \
  -v "$PWD":/workspace \
  -v "$HOME/.cargo/registry":/root/.cargo/registry \
  -v "$HOME/.cache/huggingface":/root/.cache/huggingface \
  rollout-dev
```

Inside the container:

```bash
cargo test --workspace --tests
ROLLOUT_VLLM_AVAILABLE=1 make infer-smoke
cargo run -p rollout-cli --features vllm -- infer batch --config examples/batch-tiny.toml
```

Caveats:

- `linux/arm64` and `linux/amd64` images both work; `linux/arm64` is faster on M-series silicon.
- The first run downloads `Qwen/Qwen2.5-0.5B-Instruct` (~1 GiB). The `~/.cache/huggingface` bind volume above persists it across container restarts.
- A pure-CPU Docker run will not exercise CUDA-specific code paths. For those, use Workaround 2 or a cloud GPU runner.

## Workaround 2: Cloud GPU runner

For exit criterion (c) (the <10 % overhead benchmark) you need a real CUDA GPU. The repo's CI `infer-smoke` job is gated on `vars.ROLLOUT_VLLM_AVAILABLE == '1'` and `needs: test` — set the repo variable on a self-hosted runner with a GPU, then push, and the bench captures throughput against `python scripts/raw_vllm_baseline.py` automatically.

For local development without committing, a Lambda Labs / Runpod / Vast.ai box rented by the hour works well:

```bash
# On the GPU box:
git clone <this-repo>
cd rollout
pip install 'vllm>=0.10,<0.22'
ROLLOUT_VLLM_AVAILABLE=1 make infer-smoke
cargo bench -p rollout-backend-vllm --bench throughput
```

## Workaround 3: Source-built vLLM (not recommended)

`VLLM_TARGET_DEVICE=cpu pip install -e .` against a freshly cloned `vllm-project/vllm` repo works on macOS in principle. In practice the build takes 10–30 min, brittle-fails on Apple-clang version mismatches in ways that drift between vLLM releases, and produces a `vllm` binding ~3× slower than the Linux CPU wheel. Prefer Docker.

## What CI tests on macOS

The `lint` and `test` workflow jobs run on `macos-14` (the GitHub `macos-14` runner is Apple-Silicon). They cover the entire Rust workspace surface and the `MockBackend`-driven `restart_no_duplicates` test. The `infer-smoke` job runs on `ubuntu-latest` and is opt-in; the `lint` job uses default features only (no `quic`, no `vllm`) per the `.github/workflows/ci.yml` comment.

## See also

- `cpu-mode.md` — where CPU mode is selected and the expected throughput numbers.
- `cli.md` — full CLI reference.
- `resume.md` — how `MockBackend` proves the resume contract without vLLM.
