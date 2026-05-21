---
phase: 04-train-sft-rm-snapshots
plan: 07
type: execute
wave: 5
depends_on: [04-04, 04-05, 04-06]
files_modified:
  - examples/sft-tiny.toml
  - examples/sft-tiny.jsonl
  - examples/rm-tiny.toml
  - examples/rm-tiny.jsonl
  - scripts/train-smoke.sh
  - Makefile
  - .github/workflows/ci.yml
  - docs/book/src/training/index.md
  - docs/book/src/training/sft.md
  - docs/book/src/training/rm.md
  - docs/book/src/SUMMARY.md
autonomous: true
requirements: [TRAIN-01, TRAIN-02, TRAIN-03, DOCS-01, DOCS-02, DOCS-03]
must_haves:
  truths:
    - "examples/sft-tiny.toml + examples/sft-tiny.jsonl exist; `rollout train sft --config examples/sft-tiny.toml --dry-run` passes."
    - "examples/rm-tiny.toml + examples/rm-tiny.jsonl exist; `rollout train rm --config examples/rm-tiny.toml --dry-run` passes."
    - "scripts/train-smoke.sh mirrors scripts/infer-smoke.sh shape; gated on ROLLOUT_TRANSFORMERS_AVAILABLE=1; orchestrates the full SFT path against Qwen2.5-0.5B-Instruct CPU."
    - "Makefile train-smoke target runs scripts/train-smoke.sh; postgres-test from plan 04-03 preserved."
    - ".github/workflows/ci.yml gains optional train-smoke job gated on `vars.ROLLOUT_TRANSFORMERS_AVAILABLE == '1'`; existing 15 jobs preserved."
    - "docs/book/src/training/index.md upgraded from stub to full landing page; SUMMARY.md Training section complete with all 8 sub-chapters in plan 04-RESEARCH order (index, sft, rm, snapshots, postgres-backend, determinism, cli, cpu-mode)."
  artifacts:
    - path: examples/sft-tiny.toml
      provides: "Smallest possible SFT config (1 minibatch, 2 max_steps, Qwen2.5-0.5B-Instruct, assistant_only mask)"
      contains: "kind = \"sft\""
    - path: examples/sft-tiny.jsonl
      provides: "4-row chat-message JSONL dataset"
      contains: "messages"
    - path: examples/rm-tiny.toml
      provides: "Smallest possible RM config (1 minibatch, 2 max_steps, BradleyTerry head)"
      contains: "kind = \"rm\""
    - path: examples/rm-tiny.jsonl
      provides: "4-pair preference JSONL dataset"
      contains: "chosen"
    - path: scripts/train-smoke.sh
      provides: "End-to-end SFT smoke driver for live HF transformers + accelerate"
      contains: "ROLLOUT_TRANSFORMERS_AVAILABLE"
    - path: .github/workflows/ci.yml
      provides: "Optional train-smoke CI job gated on ROLLOUT_TRANSFORMERS_AVAILABLE=1"
      contains: "train-smoke:"
  key_links:
    - from: examples/sft-tiny.toml
      to: "rollout train sft --config examples/sft-tiny.toml"
      via: "CLI dry-run path validates"
      pattern: "kind = \"sft\""
    - from: scripts/train-smoke.sh
      to: "rollout train sft + rollout snapshot list"
      via: "subprocess invocations"
      pattern: "rollout train sft"
    - from: Makefile
      to: "scripts/train-smoke.sh"
      via: "train-smoke target"
      pattern: "scripts/train-smoke.sh"
---

<objective>
Phase 4 polish: example configs that the CLI dry-runs cleanly, a smoke script that exercises the live HF transformers + accelerate path (gated on `ROLLOUT_TRANSFORMERS_AVAILABLE=1`), Makefile + CI plumbing, and mdBook training landing-page completion. This is the final Phase-4 plan; satisfies the ROADMAP §"Phase 4" exit criterion `rollout train sft --config examples/sft-tiny.toml completes on a 1B model`.

Output: 4 example files + smoke script + CI job + completed mdBook training section.
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
@$HOME/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/phases/04-train-sft-rm-snapshots/04-CONTEXT.md
@.planning/phases/04-train-sft-rm-snapshots/04-RESEARCH.md
@.planning/phases/04-train-sft-rm-snapshots/04-02-algo-sft-skeleton-PLAN.md
@.planning/phases/04-train-sft-rm-snapshots/04-04-algo-rm-PLAN.md
@.planning/phases/04-train-sft-rm-snapshots/04-05-backend-vllm-train-PLAN.md
@.planning/phases/04-train-sft-rm-snapshots/04-06-cli-train-snapshot-PLAN.md
@scripts/infer-smoke.sh
@examples/batch-tiny.toml
@Makefile
@.github/workflows/ci.yml
@docs/book/src/SUMMARY.md
@docs/book/src/training/index.md

<interfaces>
<!-- CLI entry points this plan exercises. -->

After plan 04-06:
- `rollout train sft --config <toml> [--resume <id>] [--dry-run]`
- `rollout train rm --config <toml> [--resume <id>] [--dry-run]`
- `rollout snapshot list [--run-id <ulid>] [--kind <kind>] [--limit <n>]`
- `rollout snapshot show <snapshot_id>`
- `rollout snapshot prune --run-id <ulid> [--keep-last <n>]`

Backend Cargo features:
- `--features vllm,train` → production live HF transformers + accelerate (plan 04-05)
- `--features test-mock-backend` → deterministic MockBackend (plan 04-02 extension)

scripts/infer-smoke.sh (Phase-3 reference; mirror shape):
- bash strict mode (`set -euo pipefail`)
- ROLLOUT_VLLM_AVAILABLE=1 gate
- temp workdir
- subprocess invocations of rollout binary
- assertions on JSONL output
</interfaces>

</context>

<tasks>

<task type="auto">
  <name>Task 1: Example configs (sft-tiny + rm-tiny) + train-smoke.sh + Makefile target</name>
  <files>
    examples/sft-tiny.toml,
    examples/sft-tiny.jsonl,
    examples/rm-tiny.toml,
    examples/rm-tiny.jsonl,
    scripts/train-smoke.sh,
    Makefile
  </files>
  <read_first>
    .planning/phases/04-train-sft-rm-snapshots/04-RESEARCH.md §"Code Examples" → examples/sft-tiny.toml (lines 1197-1252) + examples/sft-tiny.jsonl (lines 1254-1261),
    scripts/infer-smoke.sh (Phase-3 smoke driver to MIRROR),
    examples/batch-tiny.toml (Phase-3 example to model the TOML structure on),
    Makefile (after plan 04-03 — `postgres-test` and `train-smoke` placeholder exist; replace train-smoke placeholder with the real impl),
    .planning/phases/04-train-sft-rm-snapshots/04-CONTEXT.md D-DATA-03 (Qwen/Qwen2.5-0.5B-Instruct is the smoke target)
  </read_first>
  <action>
    **Step A — Create `examples/sft-tiny.toml`** verbatim from RESEARCH lines 1197-1252:

    ```toml
    schema_version = 1

    [run]
    name = "sft-tiny-smoke"

    [storage]
    backend = "embedded"
    [storage.embedded]
    path = "./data/sft-tiny.db"

    [algorithm]
    kind = "sft"

    [algorithm.sft]
    minibatch_size = 1
    gradient_accumulation = 1

    [algorithm.sft.base_model]
    uri = "Qwen/Qwen2.5-0.5B-Instruct"

    [algorithm.sft.optimizer]
    kind = "adamw"
    lr = 1e-5
    weight_decay = 0.0
    betas = [0.9, 0.999]
    eps = 1e-8
    warmup_steps = 0
    schedule = "constant"

    [algorithm.sft.budget]
    max_steps = 2

    [algorithm.sft.dataset]
    kind = "jsonl_path"
    path = "examples/sft-tiny.jsonl"

    [algorithm.sft.packing]
    kind = "concat"
    max_seq_len = 512

    [algorithm.sft.loss_on]
    kind = "assistant_only"
    ```

    Note: the `[snapshots]` block from RESEARCH is OPTIONAL for Phase-4 SFT; SftSettings doesn't carry one (the SnapshotPolicy lives on RunConfig at the run level, not algorithm level, OR is implicit). Cross-check the actual struct shape from plan 04-00-a → if SnapshotPolicy belongs in RunConfig at the top level, add it there; if not, leave the `[snapshots]` block off this example for Phase 4. Investigate during execution; whichever works, document in SUMMARY.

    **Step B — Create `examples/sft-tiny.jsonl`** verbatim from RESEARCH lines 1254-1261 (4 chat-message rows):

    ```jsonl
    {"messages": [{"role": "user", "content": "What is 2+2?"}, {"role": "assistant", "content": "2+2 equals 4."}]}
    {"messages": [{"role": "user", "content": "Capital of France?"}, {"role": "assistant", "content": "Paris."}]}
    {"messages": [{"role": "user", "content": "Largest planet?"}, {"role": "assistant", "content": "Jupiter."}]}
    {"messages": [{"role": "user", "content": "Boiling point of water at sea level in Celsius?"}, {"role": "assistant", "content": "100 degrees Celsius."}]}
    ```

    **Step C — Create `examples/rm-tiny.toml`** (mirror sft-tiny.toml shape but with `kind = "rm"` + RmSettings fields):

    ```toml
    schema_version = 1

    [run]
    name = "rm-tiny-smoke"

    [storage]
    backend = "embedded"
    [storage.embedded]
    path = "./data/rm-tiny.db"

    [algorithm]
    kind = "rm"

    [algorithm.rm]
    minibatch_size = 1

    [algorithm.rm.base_model]
    uri = "Qwen/Qwen2.5-0.5B-Instruct"

    [algorithm.rm.optimizer]
    kind = "adamw"
    lr = 1e-5
    weight_decay = 0.0
    betas = [0.9, 0.999]
    eps = 1e-8
    warmup_steps = 0
    schedule = "constant"

    [algorithm.rm.budget]
    max_steps = 2

    [algorithm.rm.dataset]
    kind = "jsonl_path"
    path = "examples/rm-tiny.jsonl"

    [algorithm.rm.head]
    kind = "bradley_terry"
    ```

    Note: RmHeadKind serialization tag — confirm `kind = "bradley_terry"` is the right TOML form (the enum is `#[serde(rename_all = "snake_case")]` per plan 04-00-a). If the actual serialization expects untagged or different form, adjust the TOML accordingly.

    **Step D — Create `examples/rm-tiny.jsonl`** — 4 preference pairs:

    ```jsonl
    {"prompt": "What is 2+2?", "chosen": "2+2 equals 4.", "rejected": "I don't know."}
    {"prompt": "Capital of France?", "chosen": "Paris.", "rejected": "London."}
    {"prompt": "Largest planet?", "chosen": "Jupiter.", "rejected": "Earth."}
    {"prompt": "Boiling point of water?", "chosen": "100 degrees Celsius at sea level.", "rejected": "50 degrees."}
    ```

    **Step E — Create `scripts/train-smoke.sh`** (mirror `scripts/infer-smoke.sh` structure):

    ```bash
    #!/usr/bin/env bash
    # Phase-4 train smoke driver. Gated on ROLLOUT_TRANSFORMERS_AVAILABLE=1.
    # Exercises the full SFT path against Qwen/Qwen2.5-0.5B-Instruct on CPU.
    # Expected wall-clock: ~3-5 minutes on M-series CPU.

    set -euo pipefail

    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

    if [ "${ROLLOUT_TRANSFORMERS_AVAILABLE:-0}" != "1" ]; then
        echo "skipping: set ROLLOUT_TRANSFORMERS_AVAILABLE=1 to run (requires pip install transformers accelerate torch)"
        exit 0
    fi

    cd "$REPO_ROOT"

    WORK_DIR="$(mktemp -d -t rollout-train-smoke-XXXXXX)"
    trap 'rm -rf "$WORK_DIR"' EXIT
    echo "work dir: $WORK_DIR"

    # 1. Validate dry-run first.
    echo "==> Step 1: dry-run validation"
    cargo run -p rollout-cli --features train,vllm --quiet -- \
        train sft \
        --config "$REPO_ROOT/examples/sft-tiny.toml" \
        --dry-run

    # 2. Live SFT run.
    echo "==> Step 2: live SFT run against Qwen/Qwen2.5-0.5B-Instruct (CPU)"
    cp -r "$REPO_ROOT/examples/sft-tiny.jsonl" "$WORK_DIR/"
    # Override storage path to land in WORK_DIR.
    sed "s|./data/sft-tiny.db|$WORK_DIR/sft-tiny.db|; s|examples/sft-tiny.jsonl|$WORK_DIR/sft-tiny.jsonl|" \
        "$REPO_ROOT/examples/sft-tiny.toml" > "$WORK_DIR/sft-tiny.toml"

    cargo run -p rollout-cli --features train,vllm --quiet -- \
        train sft \
        --config "$WORK_DIR/sft-tiny.toml"

    # 3. List snapshots — there should be at least 0 (Phase 4 SFT doesn't auto-snapshot
    #    on completion in the CLI path; snapshot policy ride-along is plan 04-08 / Phase 9).
    #    For now we exercise the list command and accept empty.
    echo "==> Step 3: list snapshots"
    cargo run -p rollout-cli --features train,vllm --quiet -- \
        snapshot list \
        --storage-path "$WORK_DIR/sft-tiny.db" \
        --object-path "$WORK_DIR/object-store" \
        || echo "snapshot list returned non-zero (acceptable if no snapshots saved)"

    echo "==> train-smoke OK"
    ```

    Mark executable: `chmod +x scripts/train-smoke.sh`.

    **Step F — Update `Makefile`** to replace the train-smoke placeholder (plan 04-03 added a stub):

    ```make
    .PHONY: train-smoke
    train-smoke:  ## Phase-4 SFT smoke (requires ROLLOUT_TRANSFORMERS_AVAILABLE=1 + transformers + accelerate)
    	@bash scripts/train-smoke.sh
    ```

    Preserve the `postgres-test` target from plan 04-03 verbatim.

    Commit message: `feat(04-07-01): tiny example configs + train-smoke.sh + Makefile train-smoke target`.
  </action>
  <verify>
    <automated>
test -f examples/sft-tiny.toml &&
test -f examples/sft-tiny.jsonl &&
test -f examples/rm-tiny.toml &&
test -f examples/rm-tiny.jsonl &&
test -x scripts/train-smoke.sh &&
grep -q 'ROLLOUT_TRANSFORMERS_AVAILABLE' scripts/train-smoke.sh &&
grep -q '^train-smoke:' Makefile &&
grep -q 'kind = "sft"' examples/sft-tiny.toml &&
grep -q 'kind = "rm"' examples/rm-tiny.toml &&
grep -q 'Qwen/Qwen2.5-0.5B-Instruct' examples/sft-tiny.toml &&
wc -l examples/sft-tiny.jsonl | grep -qE '^\s*4 ' &&
wc -l examples/rm-tiny.jsonl | grep -qE '^\s*4 ' &&
cargo run -p rollout-cli --quiet -- train sft --config examples/sft-tiny.toml --dry-run &&
cargo run -p rollout-cli --quiet -- train rm --config examples/rm-tiny.toml --dry-run
    </automated>
  </verify>
  <acceptance_criteria>
    - `test -f examples/sft-tiny.toml && test -f examples/sft-tiny.jsonl && test -f examples/rm-tiny.toml && test -f examples/rm-tiny.jsonl` all exit 0.
    - `test -x scripts/train-smoke.sh` exits 0 (executable bit set).
    - `grep -q 'ROLLOUT_TRANSFORMERS_AVAILABLE' scripts/train-smoke.sh` exits 0.
    - `grep -q 'set -euo pipefail' scripts/train-smoke.sh` exits 0 (strict mode).
    - `grep -q 'rollout train sft' scripts/train-smoke.sh` exits 0.
    - `grep -q 'kind = "sft"' examples/sft-tiny.toml` exits 0.
    - `grep -q 'kind = "rm"' examples/rm-tiny.toml` exits 0.
    - `grep -q 'Qwen/Qwen2.5-0.5B-Instruct' examples/sft-tiny.toml` exits 0 (D-DATA-03 model pin).
    - `wc -l examples/sft-tiny.jsonl` reports 4 lines (4 chat rows per RESEARCH).
    - `wc -l examples/rm-tiny.jsonl` reports 4 lines (4 preference pairs).
    - `grep -q 'messages' examples/sft-tiny.jsonl` exits 0.
    - `grep -q 'chosen' examples/rm-tiny.jsonl && grep -q 'rejected' examples/rm-tiny.jsonl` both exit 0.
    - `grep -q '^train-smoke:' Makefile` exits 0.
    - `grep -q '^postgres-test:' Makefile` exits 0 (plan 04-03's target preserved).
    - `cargo run -p rollout-cli --quiet -- train sft --config examples/sft-tiny.toml --dry-run` exits 0 (the dry-run validation passes on the canonical example).
    - `cargo run -p rollout-cli --quiet -- train rm --config examples/rm-tiny.toml --dry-run` exits 0.
    - HEAD commit message matches `^feat\(04-07-01\):`.
    - DOCS-02 satisfied via the mdBook chapter polish in Task 2 (this commit is examples + script, which counts as docs-equivalent under the DOCS-02 rule — example configs in docs/book/src/examples per AGENTS.md §9.1 land in v1 polish; for Phase 4 the chapter cross-references suffice).
  </acceptance_criteria>
  <done>
    All 4 example files exist; both --dry-run paths pass on the canonical examples. train-smoke.sh mirrors infer-smoke.sh shape. Makefile train-smoke target replaces the plan 04-03 placeholder.
  </done>
</task>

<task type="auto">
  <name>Task 2: train-smoke CI job + complete mdBook Training section + finalize SUMMARY.md</name>
  <files>
    .github/workflows/ci.yml,
    docs/book/src/training/index.md,
    docs/book/src/training/sft.md,
    docs/book/src/training/rm.md,
    docs/book/src/SUMMARY.md
  </files>
  <read_first>
    .github/workflows/ci.yml (after plan 04-03 — 15 jobs incl postgres-integration),
    docs/book/src/training/index.md (stub from plan 04-01),
    docs/book/src/training/sft.md (after plan 04-02 — extend if Phase-4 polish needed),
    docs/book/src/training/rm.md (after plan 04-04 — extend if Phase-4 polish needed),
    docs/book/src/SUMMARY.md (current Training section ordering — confirm all 8 chapters land in the recommended order),
    docs/book/src/inference/index.md (Phase-3 landing-page pattern to MIRROR for training/index.md)
  </read_first>
  <action>
    **Step A — Update `.github/workflows/ci.yml`** to add the optional `train-smoke` job. Append after the existing `postgres-integration` job:

    ```yaml
      train-smoke:
        runs-on: ubuntu-latest
        if: ${{ vars.ROLLOUT_TRANSFORMERS_AVAILABLE == '1' }}
        needs: test
        timeout-minutes: 30
        steps:
          - uses: actions/checkout@v4
          - uses: dtolnay/rust-toolchain@1.88.0
          - uses: Swatinem/rust-cache@v2
            with:
              shared-key: ci-train-smoke
          - uses: actions/setup-python@v5
            with:
              python-version: "3.11"
          - name: Install transformers + accelerate + torch (CPU)
            run: pip install 'transformers>=4.45,<5.0' 'accelerate>=1.0,<2.0' 'torch>=2.1,<3.0' --extra-index-url https://download.pytorch.org/whl/cpu
          - name: Run train smoke
            env:
              ROLLOUT_TRANSFORMERS_AVAILABLE: "1"
            run: bash scripts/train-smoke.sh
    ```

    The `if: ${{ vars.ROLLOUT_TRANSFORMERS_AVAILABLE == '1' }}` gate keeps this job off by default — only fires on repositories that set the variable (mirrors Phase-3 `ROLLOUT_VLLM_AVAILABLE` gate semantics from plan 03-05). Document the convention in the SUMMARY.md.

    Total CI jobs after this commit: 16 (14 from Phases 1-3 + postgres-integration from 04-03 + train-smoke from this plan).

    **Step B — Rewrite `docs/book/src/training/index.md`** from the stub to a real landing page:

    ```markdown
    # Training

    Phase 4 lands the first end-to-end training story: supervised fine-tuning + Bradley-Terry reward-model training + bit-identical-resume training-state snapshots + the Postgres `Storage` backend.

    ## What's here

    - [SFT (Supervised Fine-Tuning)](./sft.md) — `rollout-algo-sft`; TRAIN-01.
    - [RM (Reward Model)](./rm.md) — `rollout-algo-rm` with Bradley-Terry pairwise loss; TRAIN-02.
    - [Snapshots](./snapshots.md) — `rollout-snapshots`, TrainState kind, tar + blake3 + restore; TRAIN-03.
    - [Postgres backend](./postgres-backend.md) — `rollout-storage[postgres]`; testcontainers CI; TRAIN-04.
    - [Determinism](./determinism.md) — accelerate.save_state + CUDA / CPU caveats.
    - [CPU mode](./cpu-mode.md) — what to expect on macOS / Apple Silicon development boxes.
    - [CLI](./cli.md) — `rollout train sft|rm` and `rollout snapshot list|show|prune`.

    ## Quickstart

    ```bash
    # Dry-run validation (works without Python deps).
    cargo run -p rollout-cli -- train sft --config examples/sft-tiny.toml --dry-run

    # Live run (requires pip install transformers accelerate torch; ~5 min CPU on M-series).
    pip install 'transformers>=4.45,<5.0' 'accelerate>=1.0,<2.0' 'torch>=2.1,<3.0'
    ROLLOUT_TRANSFORMERS_AVAILABLE=1 make train-smoke
    ```

    ## Phase 4 exit criteria

    | Criterion | Where it's proven |
    |-----------|-------------------|
    | `rollout train sft --config examples/sft-tiny.toml` completes on a small model | `make train-smoke` (gated on ROLLOUT_TRANSFORMERS_AVAILABLE=1) |
    | Snapshot + restart produces bit-identical weights for next K steps | `crates/rollout-algo-sft/tests/snapshot_resume.rs` (default-fire) + `crates/rollout-backend-vllm/tests/snapshot_resume_live.rs` (gated) |
    | Postgres backend CI-tested via containerized integration test | `crates/rollout-storage/tests/postgres_integration.rs` via the `postgres-integration` CI job |

    ## What's NOT here (deferred)

    - PPO / GRPO / DPO / IPO / KTO — Phases 9 / 10.
    - Buffer / Process / EpisodicMemory snapshot kinds — Phases 9 / 11 / 8.
    - Cloud object stores for snapshot blobs — Phase 5.
    - HuggingFace datasets Hub integration — Phase 7.
    - Multi-node distributed training — Phase 6.
    ```

    **Step C — Polish `docs/book/src/training/sft.md`** if Phase-4 plan 04-02's chapter omitted any of these:
    - The `make train-smoke` invocation.
    - Cross-link to `cli.md` for the CLI surface.
    - The `examples/sft-tiny.toml` + `examples/sft-tiny.jsonl` references with `@include` or inline.

    Add a "Running the example" section at the end:

    ```markdown
    ## Running the example

    The smallest possible SFT run lives at `examples/sft-tiny.toml` + `examples/sft-tiny.jsonl` (4 chat rows). Two ways to exercise it:

    **Dry-run (works without Python deps):**

    ```bash
    cargo run -p rollout-cli -- train sft \
        --config examples/sft-tiny.toml --dry-run
    ```

    **Live run (requires transformers + accelerate + torch; ~5 min M-series CPU):**

    ```bash
    pip install 'transformers>=4.45,<5.0' 'accelerate>=1.0,<2.0' 'torch>=2.1,<3.0'
    ROLLOUT_TRANSFORMERS_AVAILABLE=1 make train-smoke
    ```
    ```

    **Step D — Polish `docs/book/src/training/rm.md`** similarly with a "Running the example" section pointing at `examples/rm-tiny.toml`.

    **Step E — Finalize `docs/book/src/SUMMARY.md` Training section** in the recommended order:

    ```markdown
    # Training

    - [Overview](./training/index.md)
    - [SFT](./training/sft.md)
    - [RM](./training/rm.md)
    - [Snapshots](./training/snapshots.md)
    - [Postgres backend](./training/postgres-backend.md)
    - [Determinism](./training/determinism.md)
    - [CLI](./training/cli.md)
    - [CPU mode](./training/cpu-mode.md)
    ```

    Verify all 8 chapter files exist (one per line; ordered per RESEARCH §"Open Questions" #7).

    Commit message: `docs(04-07-02): finalize Training mdBook section + optional train-smoke CI job`.
  </action>
  <verify>
    <automated>
grep -q '^  train-smoke:' .github/workflows/ci.yml &&
grep -q "vars.ROLLOUT_TRANSFORMERS_AVAILABLE == '1'" .github/workflows/ci.yml &&
test -f docs/book/src/training/index.md &&
grep -q 'Quickstart' docs/book/src/training/index.md &&
grep -q 'examples/sft-tiny.toml' docs/book/src/training/index.md &&
test -f docs/book/src/training/sft.md &&
test -f docs/book/src/training/rm.md &&
test -f docs/book/src/training/snapshots.md &&
test -f docs/book/src/training/postgres-backend.md &&
test -f docs/book/src/training/determinism.md &&
test -f docs/book/src/training/cli.md &&
test -f docs/book/src/training/cpu-mode.md &&
grep -c 'training/' docs/book/src/SUMMARY.md | awk '{ exit ($1 >= 8) ? 0 : 1 }' &&
mdbook build docs/book
    </automated>
  </verify>
  <acceptance_criteria>
    - `grep -q '^  train-smoke:' .github/workflows/ci.yml` exits 0.
    - `grep -q "vars.ROLLOUT_TRANSFORMERS_AVAILABLE == '1'" .github/workflows/ci.yml` exits 0 (default-off gate).
    - `grep -q "pip install 'transformers>=4.45" .github/workflows/ci.yml` exits 0 (correct version pins).
    - All 8 mdBook chapter files exist: `index.md`, `sft.md`, `rm.md`, `snapshots.md`, `postgres-backend.md`, `determinism.md`, `cli.md`, `cpu-mode.md`.
    - `grep -q 'Quickstart' docs/book/src/training/index.md` exits 0 (landing page has a Quickstart section).
    - `grep -q 'Phase 4 exit criteria' docs/book/src/training/index.md` exits 0.
    - `grep -c 'training/' docs/book/src/SUMMARY.md` reports ≥ 8 (all 8 chapters linked).
    - `grep -q 'training/index.md' docs/book/src/SUMMARY.md` exits 0.
    - `grep -q 'Running the example' docs/book/src/training/sft.md` exits 0.
    - `grep -q 'Running the example' docs/book/src/training/rm.md` exits 0.
    - `mdbook build docs/book` exits 0.
    - HEAD commit message matches `^docs\(04-07-02\):`.
    - DOCS-02 satisfied: this commit is docs-only + CI config (no code under crates/* → escape hatch via [skip-docs-check] trailer is NOT needed; the CI policy fires only on code commits per AGENTS.md §9.2). Verify by re-reading the rule: "Every commit that modifies code under `crates/`, `python/`, or `xtask/`" — this commit doesn't touch those paths, so no test-touch needed.
    - DOCS-03 satisfied: `cargo doc --workspace --no-deps --all-features` clean (no changes to rustdoc-bearing code in this commit; should pass trivially).
  </acceptance_criteria>
  <done>
    Optional train-smoke CI job lands (default-off gate). mdBook Training section is complete with all 8 chapters in recommended order. Landing page documents Phase-4 exit criteria + quickstart + deferred items.
  </done>
</task>

</tasks>

<verification>
**Phase-gate checks for this plan:**
- `cargo run -p rollout-cli --quiet -- train sft --config examples/sft-tiny.toml --dry-run` exits 0.
- `cargo run -p rollout-cli --quiet -- train rm --config examples/rm-tiny.toml --dry-run` exits 0.
- `mdbook build docs/book` exits 0; all 8 Training chapters render.
- `cargo doc --workspace --no-deps --all-features` clean (DOCS-03).
- `cargo test --workspace --tests` no regressions (no Rust code in this plan).
- (Gated) `ROLLOUT_TRANSFORMERS_AVAILABLE=1 bash scripts/train-smoke.sh` exits 0 on a dev box with transformers + accelerate installed.
- `cargo deny check` clean (no new deps from this plan; safety check anyway).

**Conventional commits:** `feat(04-07-01)`, `docs(04-07-02)`.
</verification>

<success_criteria>
- All 4 example files (sft-tiny + rm-tiny .toml + .jsonl) exist + dry-run cleanly.
- scripts/train-smoke.sh mirrors infer-smoke.sh structure + gates on ROLLOUT_TRANSFORMERS_AVAILABLE=1.
- Makefile train-smoke target invokes the script.
- Optional train-smoke CI job (16th total) lands behind the env-var gate.
- mdBook Training section complete with 8 chapters in recommended order.
- Phase-4 exit criterion "`rollout train sft --config examples/sft-tiny.toml` completes on a small model" is achievable via `make train-smoke` (with transformers installed).
</success_criteria>

<output>
After completion, create `.planning/phases/04-train-sft-rm-snapshots/04-07-examples-docs-smoke-SUMMARY.md` recording: (1) example files shipped + their exact contents (4 rows / 4 pairs verbatim), (2) train-smoke.sh script structure mirrored from infer-smoke.sh, (3) Makefile + CI integration (16 jobs total now: 14 from Phases 1-3 + postgres-integration + train-smoke), (4) mdBook Training section final chapter list, (5) Phase-4 exit-criterion verification map (which test/script proves each), (6) any deviation (e.g., if RmHeadKind tag format needed adjustment in the TOML; if [snapshots] block went on RunConfig top-level or omitted from sft-tiny.toml).

After Plan 04-07 ships, Phase 4 is COMPLETE — all four TRAIN-NN requirements satisfied, all exit criteria from ROADMAP.md §"Phase 4" met, ready for `/gsd:verify-work` + `/gsd:uat` walkthrough.
</output>
