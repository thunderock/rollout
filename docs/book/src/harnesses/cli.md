# CLI: rollout eval

`rollout eval` is a top-level subcommand (D-EVAL-02), sibling to `infer` /
`train` / `snapshot`. It runs a bundled eval suite against a checkpoint and emits
per-task scores plus aggregate metrics.

```bash
rollout eval --suite mmlu  --checkpoint <snapshot-id>
rollout eval --suite gsm8k --checkpoint <snapshot-id> --format json
rollout eval --suite ifeval --checkpoint <snapshot-id> --dry-run
```

## Flags

| Flag | Meaning |
|---|---|
| `--suite <mmlu\|ifeval\|gsm8k>` | which bundled suite to run |
| `--checkpoint <snapshot-id>` | resolved from local storage to a `ModelRef`; if no snapshot row matches, treated as a direct content-id pin |
| `--config <toml>` | optional (reserved for future eval settings) |
| `--storage-path` / `--object-path` | local embedded storage + object store roots |
| `--seed` | deterministic sampling/task-order seed |
| `--dry-run` | validate args + resolve the checkpoint, but construct no backend |
| `--format json` | pretty-printed `EvalReport` (default) |

## Checkpoint resolution

`--checkpoint` parses as a `ContentId`. If a `Snapshot` row in local storage has
that id, its `"tar"` part's content-id pins `ModelRef.content_id` (spec 04).
Otherwise the value is used directly as a content-id pin. `--dry-run`
short-circuits before any backend is built, so it works on a build with neither
backend feature.

## Spec-08 reconciliation (D-EVAL-02)

Earlier drafts of `docs/specs/08-cli.md` listed `rollout infer eval`. Phase 7
reconciled the spec: that form is removed and `rollout eval` is documented as a
top-level subcommand. The spec's `rollout plan` "Harness DAG: acyclic, 3 nodes"
line is footnoted as a **v1.2** concern (D-CORE-02) — DAG validation is not
implemented in v1.1, which ships the three harnesses standalone.

## Backend selection

Mirrors `rollout infer batch`: with the `test-mock-backend` feature +
`ROLLOUT_TEST_MOCK_BACKEND=1` the GPU-free `MockEvalBackend` runs the offline
fixtures; the live backend path lands with the full-split download wiring.
