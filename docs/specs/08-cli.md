# Spec 08 — CLI

The `rollout` CLI is the primary user surface for v1. It is also the implementation specification for the future UI (whose primary backend is the same set of commands wrapped in a JSON/RPC layer).

This spec is the contract between user expectations and the runtime.

## 1. Purpose

The CLI exists to:

- Convert a config file into a validated plan.
- Drive a plan to completion (or report typed failure).
- Provide read access to runs, snapshots, plugins, and cloud diagnostics.
- Emit machine-readable output (`--format json`) so users can script.

It is **not** an orchestrator (that's the runtime) and **not** a config editor (that's the user's editor).

## 2. Command surface

```
rollout [GLOBAL FLAGS] <SUBCOMMAND> [ARGS]
```

### Global flags

| Flag | Default | Description |
|---|---|---|
| `--format` | `text` | Output: `text`, `json`, `yaml` |
| `--quiet` / `-q` | off | Suppress non-essential output |
| `--verbose` / `-v` (repeatable) | 0 | Increase log verbosity |
| `--no-telemetry` | off | Disable observability emission (must be paired with a config justification) |
| `--config` | none | Override config file location for commands that take one |
| `--profile` | from `~/.config/rollout/profile.toml` | Named profile (combines cloud + storage defaults) |

### Subcommands

```
rollout validate    Validate a config file (schema only, no plugin loading)
rollout plan        Validate + load plugins + reach substrate → emit plan.lock
rollout run         Execute a plan
rollout train       (Convenience) plan + run for a training config
rollout infer       (Convenience) plan + run for an inference config
rollout eval        Run a bundled eval suite against a checkpoint (D-EVAL-02)
rollout snapshot    Snapshot ops
rollout runs        Read-only run queries
rollout logs        Tail / search structured logs
rollout plugins     Plugin discovery + reload
rollout cloud       Cloud doctor + auth helpers
rollout schema      Emit JSON Schema / Python stubs from current Rust types
rollout help        Help / man pages
rollout version     Version info
```

Every subcommand has `-h/--help` with concrete examples.

### `rollout validate`

```bash
rollout validate --config run.toml
```

Output (success):

```
✓ Config schema valid
✓ Required fields present
✓ Field types correct
✓ Cross-field constraints satisfied
```

Exit codes:

- `0` — valid
- `64` (`EX_USAGE`) — schema invalid
- `65` (`EX_DATAERR`) — data invalid (cross-field violation)

### `rollout plan`

```bash
rollout plan --config run.toml [--out plan.lock]
```

Performs:

1. `validate`
2. Plugin discovery + manifest validation
3. Reachability checks (storage, queue, object store)
4. DAG validation
5. Resource budget computation
6. Write `plan.lock` (content-addressed)

Output (success):

```
✓ Schema valid
✓ Plugins discovered: 4 (3 in-process, 1 sidecar)
✓ Storage reachable: postgres://...
✓ Object store reachable: s3://my-rollout/
✓ Queue reachable: sqs://...
✓ Harness DAG: acyclic, 3 nodes    [†]
✓ Resources fit: 4 GPUs (have 8), 32 GiB RAM (have 256)
✓ Plan written: ./plan.lock  (id: plan-01HX3Q...)
```

Exit codes match `validate` plus:

- `66` (`EX_NOINPUT`) — plugin not found
- `69` (`EX_UNAVAILABLE`) — substrate unreachable

> [†] **Harness DAG validation is deferred to v1.2 (D-CORE-02).** v1.1 ships the
> three harnesses standalone (no env↔tool edges to validate); the `HarnessGraph`
> composition config + plan-time acyclicity check land with the composed harness
> work in v1.2. The line above is forward-looking, not yet enforced.

### `rollout run`

```bash
rollout run --plan plan.lock
# OR
rollout run --config run.toml    # implicit plan
```

Streams structured progress to stderr; structured events to the event stream (per spec 09).

On Ctrl-C:

- First SIGINT → graceful drain (workers finish in-flight, snapshot if policy says so).
- Second SIGINT within `drain_deadline` → hard cancel.

Exit codes:

- `0` — run completed
- `70` (`EX_SOFTWARE`) — recoverable error retries exhausted
- `73` (`EX_CANTCREAT`) — failed to start (resources, auth)

### `rollout train <algorithm>`

Convenience over `plan` + `run`. Equivalent to:

```bash
rollout plan --config $CFG --out /tmp/plan.lock && rollout run --plan /tmp/plan.lock
```

```bash
rollout train ppo --config ppo.toml
rollout train grpo --config grpo.toml
rollout train dpo --config dpo.toml
rollout train sft --config sft.toml
rollout train rm --config rm.toml
```

### `rollout infer <mode>`

```bash
rollout infer batch  --config batch-infer.toml
rollout infer online --config serve.toml
```

### `rollout eval`

Top-level sibling to `infer`/`train`/`snapshot` (D-EVAL-02). Runs a bundled eval
suite against a checkpoint and emits per-task scores + aggregate metrics.

```bash
rollout eval --suite mmlu --checkpoint <snapshot-id>
rollout eval --suite gsm8k --checkpoint <snapshot-id> --format json
rollout eval --suite ifeval --checkpoint <snapshot-id> --dry-run
```

Flags: `--suite <mmlu|ifeval|gsm8k>`, `--checkpoint <snapshot-id>` (resolved from
local storage to a `ModelRef`, else treated as a content-id pin), `--config <toml>`
(optional), `--dry-run` (validate + resolve, no backend), `--format json`.
Eval runs as `WorkQueue` jobs (D-EVAL-05); offline-default fixtures (`HF_OFFLINE=1`).

### `rollout snapshot`

```bash
rollout snapshot save    --run <run-id> --kind train-state [--label final]
rollout snapshot restore --from <snapshot-id> --to <new-run>
rollout snapshot list    [--run <id>] [--kind ...] [--label ...]
rollout snapshot show    <snapshot-id>
rollout snapshot prune   --policy keep_last=5
```

## 2.5a. Phase 4 implementation notes

Phase 4 ships:

- `rollout train sft --config <toml> [--resume <snapshot_id>] [--dry-run]`
- `rollout train rm  --config <toml> [--resume <snapshot_id>] [--dry-run]`
- `rollout snapshot list --run-id <ulid> [--kind <kind>] [--limit <n>]`
- `rollout snapshot show <snapshot_id>`
- `rollout snapshot prune --run-id <ulid> [--keep-last <n>] [--keep-labeled]`

Clap derive surface mirrors Phase 3's `rollout infer batch` (see
`crates/rollout-cli/src/main.rs` after plan 04-06). Backend selection follows
the same Cargo-feature pattern as Phase 3:

- `--features vllm,train` → production live HF transformers + accelerate path.
- `--features test-mock-backend` → deterministic SGD against fake `ndarray`
  weights (used by CI; no HF transformers required).

Runtime backend selection (config-driven, no Cargo feature) defers to
Phase 8 (`INFER-01`).

**Lands in:** plan `04-06-cli-train-snapshot`.

### `rollout runs`

```bash
rollout runs list  [--state running|completed|failed] [--limit N]
rollout runs show  <run-id>             # summary + last events
rollout runs cancel <run-id> [--hard]   # graceful by default
rollout runs export <run-id> --to <path>    # for migration (e.g., embedded → postgres)
rollout runs import <path>                  # inverse
```

### `rollout logs`

```bash
rollout logs tail <run-id> [--worker <id>] [--span <span-id>] [--since 5m]
rollout logs search <run-id> --query 'level >= warn AND span.role = "actor"'
```

Structured event stream. Supports the small query DSL above.

### `rollout plugins`

```bash
rollout plugins list  [--scope local|global]
rollout plugins show  <name>
rollout plugins reload <name>          # dev only; rejected if production mode
rollout plugins doctor <name>          # run the plugin's local test
```

### `rollout cloud`

```bash
rollout cloud doctor [--provider aws|gcp|local]
rollout cloud whoami [--provider ...]
```

### `rollout schema`

```bash
rollout schema --format json  > rollout.schema.json
rollout schema --format python > rollout/_config_stubs.pyi
rollout schema --format markdown > docs/schema.md
```

Always re-emit after changing Rust config types. CI enforces `git diff` is clean after running `rollout schema --format json` (drift detection).

## 3. Config files

The CLI accepts `.toml`, `.yaml`, or `.json` interchangeably. The **schema** is the same; the file format is a presentation choice. Internally, all are normalized to the same JSON-shaped value before validation.

Example (PPO):

```toml
schema_version = 1

[run]
name = "ppo-7b-experiment-1"

[storage]
backend = "postgres"
[storage.postgres]
url = "postgres://user@db/rollout"

[cloud]
provider = "aws"
[cloud.aws]
region = "us-west-2"
object_store_bucket = "my-rollout"
queue_url = "https://sqs.us-west-2.amazonaws.com/.../rollout-work"

[algorithm]
kind = "ppo"
[algorithm.ppo]
policy = { uri = "meta-llama/Llama-3.1-7B-Instruct" }
reward_model = { kind = "model", checkpoint = { uri = "s3://my-rollout/rm-7b/" } }
budget = { max_steps = 10_000 }

[algorithm.ppo.optimizer]
kind = "adamw"
lr = 1e-6
weight_decay = 0.0
betas = [0.9, 0.95]
eps = 1e-8
warmup_steps = 100
schedule = "cosine"

[algorithm.ppo.ppo]
clip_ratio = 0.2
kl_coef_init = 0.1
kl_target = 6.0
gamma = 1.0
lam = 0.95
value_coef = 0.5
entropy_coef = 0.0
minibatch_size = 16
epochs_per_batch = 2
max_grad_norm = 1.0

[algorithm.ppo.rollout]
group_size = 4
max_response_tokens = 1024
temperature = 0.9
top_p = 0.95
batch_size = 32

[snapshots]
on_completion = true
on_preemption = true
[snapshots.periodic]
interval_steps = 500
kinds = ["train_state", "buffer"]

[snapshots.retention]
keep_last = 5
keep_labeled = true

[plugins]
# Discovered automatically from ./plugins, but may be pinned:
my-reward = { source = { pypi = "my-reward==1.2.0" } }
```

## 4. Profiles

`~/.config/rollout/profile.toml` defines named bundles of defaults:

```toml
[profile.local]
storage.backend = "embedded"
cloud.provider  = "local"

[profile.aws-dev]
storage.backend = "postgres"
cloud.provider  = "aws"
cloud.aws.region = "us-west-2"
```

```bash
rollout --profile aws-dev train ppo --config ppo.toml
```

Profiles compose: profile sets defaults, config overrides. Both go through the same schema.

## 5. Output

### `text` (default)

Human-friendly. Color where the terminal supports it. Progress bars on long ops. Adheres to **no-color and no-emoji** when `NO_COLOR=1` is set; respects `--no-color` flag.

### `json`

Newline-delimited JSON events (NDJSON) on stdout. Each event is self-describing:

```json
{"ts":"2026-01-01T00:00:00Z","level":"info","kind":"plan.ok","plan_id":"plan-...","details":{...}}
```

### `yaml`

Only for one-shot summary outputs (`runs show`, `plugins show`). Not for streams.

## 6. Help

- `rollout --help` shows top-level usage.
- `rollout <subcommand> --help` shows subcommand usage with examples.
- `rollout help <topic>` prints concept docs (e.g., `rollout help snapshots`, `rollout help cloud`).
- Help text is generated from doc comments in `clap`-annotated CLI types so docs cannot drift from flags.

## 7. Exit code policy

We use the `sysexits.h` codes. The full mapping is in `docs/exit-codes.md` and is part of the public API contract — scripts depend on it.

## 8. Test contract

- **Snapshot tests** (golden output) for every subcommand's `--help`.
- **Integration tests** that run the full CLI binary against the local cloud, embedded storage, in-tree plugins.
- **JSON output stability** test: `--format json` output is treated as semver-relevant; an event-shape change is a breaking change before 1.0.
- **No telemetry** without justification: `--no-telemetry` requires a corresponding `[telemetry] disabled_reason = "..."` in the config; CI rejects silent opt-outs.

## 9. Open questions

- **Interactive prompts:** v1 is fully non-interactive (scriptable). A future `rollout init` wizard could be useful but is post-v1.
- **Shell completion:** clap supports bash/zsh/fish/powershell out of the box; ship from day 1.
- **Repl mode (`rollout repl`)**: not in v1. The eval-as-data design (every command is a JSON event) makes a future REPL straightforward.
