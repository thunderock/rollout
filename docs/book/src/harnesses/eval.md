# Eval suites (HARNESS-03)

`rollout-harness-eval` bundles three scorers that mirror
lm-evaluation-harness at a pinned version
(`LM_EVAL_VERSION = "lm-eval-harness-v0.4.9"`, cited in the scorer rustdoc). The
pin removes scoring ambiguity: published MMLU/GSM8K conventions diverge, so the
authoritative convention is declared and version-pinned.

## MMLU — report both `acc` and `acc_norm` (D-EVAL-03)

- **`acc`** — predicted answer = argmax over the four choice continuations' total
  log-likelihood; correct iff it matches the gold letter (A–D). Raw exact-match,
  `temperature = 0`.
- **`acc_norm`** — same argmax, but each choice's log-likelihood is
  length-normalized before the argmax.

Both are reported (lm-eval's headline pair). Prompt format follows lm-eval's
`mmlu` task default (`A.`/`B.`/`C.`/`D.` + `Answer:`).

## IFEval — strict, non-language (D-EVAL-04)

Pure-Rust strict checks for the verifiable non-language constraints (word /
sentence count, JSON format, bullet count, keyword existence/frequency, case,
placeholders). **Language-detection constraints (`language:*`) are SKIPPED** —
no langdetect/langid dependency — and a load-time warning fires:
`"IFEval: skipping N language-detection constraints (unsupported in v1.1)"`.
A prompt whose instructions are *all* skipped is excluded from both strict
denominators; a partially-skipped prompt is scored on the remainder. Reported
metrics: `inst_strict_acc`, `prompt_strict_acc`.

## GSM8K — `####` numeric extraction (D-EVAL-04)

Gold answer = the number after the final `####` (strip `,`/`$`). Model answer =
the last match of the lm-eval `gsm8k` filter regex
`(-?[$0-9.,]{2,})|(-?[0-9]+)` in the generation. Score = exact numeric
equivalence, `temperature = 0`.

## Datasets: offline-default (D-EVAL-01)

`HF_OFFLINE=1` is the **test default**. Loaders read vendored SHA-pinned 10-row
parquet fixtures under `crates/rollout-harness-eval/tests/fixtures/`, so
`eval_score_matches_lm_eval_harness` (≤1% parity) and
`eval_loader_works_with_no_network` run on every commit with zero network. Each
fixture has a hardcoded `blake3` constant that fails loudly on drift.

Online runs (`HF_OFFLINE=0`) download full splits via `hf-hub` (pure-Rust,
rustls — never openssl) and persist to the v1.0 `ObjectStore` under a `ContentId`
hash-checked cache. `google/IFEval` has stricter anonymous rate limits — set
`HF_TOKEN` for full-split runs.

## Eval as a WorkQueue job (D-EVAL-05)

One example = one queue item, reusing the Phase-6 `rollout-coordinator::work_item`
CAS state machine (`Pending → Running → Done{result_id}`, single-winner via
`try_claim`/`try_complete`). The item id is
`blake3(suite, version, idx, model_id)` — idempotent, so re-enqueue is a no-op.
Per-task determinism comes from a fixed seeded order + `temperature = 0`;
`MockEvalBackend` makes the whole path GPU-free.

The aggregate `EvalReport` is persisted to the `eval_reports` storage namespace
(postcard row) **and** content-addressed as a full blob in the object store.

> This is eval **execution** as a job — NOT the eval *gate* (pause training →
> eval → continue/stop), which lands with HARNESS-04 in v1.2.
