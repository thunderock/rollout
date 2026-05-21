---
phase: 04-train-sft-rm-snapshots
plan: 04
type: execute
wave: 3
depends_on: [04-00-a, 04-00-b, 04-01]
files_modified:
  - crates/rollout-algo-rm/src/lib.rs
  - crates/rollout-algo-rm/src/algo.rs
  - crates/rollout-algo-rm/src/loss.rs
  - crates/rollout-algo-rm/src/data.rs
  - crates/rollout-algo-rm/Cargo.toml
  - crates/rollout-algo-rm/tests/bradley_terry_loss.rs
  - crates/rollout-algo-rm/tests/snapshot_resume.rs
  - crates/rollout-algo-rm/tests/checkpoint_roundtrip.rs
  - docs/book/src/training/rm.md
  - docs/book/src/SUMMARY.md
autonomous: true
requirements: [TRAIN-02, TRAIN-03, DOCS-01, DOCS-02, DOCS-03]
must_haves:
  truths:
    - "rollout-algo-rm::RmAlgo impls PolicyAlgorithm with the spec 02 §7 surface; mirrors SftAlgo structure for symmetry."
    - "Bradley-Terry pairwise loss = -ln σ(r_chosen - r_rejected); the unit test pins canonical-input → known-output values."
    - "JSONL data loader parses {prompt, chosen, rejected} per D-DATA-01; rejects malformed rows with line numbers."
    - "snapshot_resume.rs mirrors the SFT byte-compare proof but for RM (TRAIN-03 second exit-criterion proof point)."
    - "Final checkpoint is content-addressed: save_weights returns a ContentId that's stable across calls when nothing has changed."
    - "RmHeadKind::PairwiseLogistic enumerated; Phase 4 returns Fatal(ConfigInvalid) when selected (head is BradleyTerry only in Phase 4)."
  artifacts:
    - path: crates/rollout-algo-rm/src/algo.rs
      provides: "RmAlgo + PolicyAlgorithm impl"
      contains: "impl PolicyAlgorithm for RmAlgo"
    - path: crates/rollout-algo-rm/src/loss.rs
      provides: "Bradley-Terry pairwise loss math"
      contains: "fn bradley_terry_loss"
    - path: crates/rollout-algo-rm/src/data.rs
      provides: "JSONL pair loader for {prompt, chosen, rejected}"
      contains: "load_pairs"
    - path: crates/rollout-algo-rm/tests/bradley_terry_loss.rs
      provides: "Loss math correctness — golden-value test cases"
      contains: "bradley_terry_known_values"
    - path: crates/rollout-algo-rm/tests/snapshot_resume.rs
      provides: "TRAIN-03 byte-compare proof — RM variant"
      contains: "bit_identical_resume_at_step_5"
  key_links:
    - from: crates/rollout-algo-rm/src/algo.rs
      to: "rollout_core::{PolicyAlgorithm, AlgoDependencies, TrainableBackend}"
      via: "trait impl + dependency injection"
      pattern: "impl PolicyAlgorithm for RmAlgo"
    - from: crates/rollout-algo-rm/src/loss.rs
      to: "f32 sigmoid + log"
      via: "BT formula -ln σ(r_chosen - r_rejected)"
      pattern: "logsigmoid"
---

<objective>
Implement `rollout-algo-rm` (TRAIN-02): the Bradley-Terry reward-model training algorithm. Mirrors the SFT plan structure — PolicyAlgorithm impl + JSONL loader + snapshot_resume byte-compare proof, but with pairwise preferences instead of single sequences.

Like 04-02, this plan does NOT pull HF transformers. The MockBackend extension from 04-02 Task 1 already provides everything we need to prove the control flow + the TRAIN-03 byte-compare second-witness.

Purpose: deliver TRAIN-02 + the RM half of TRAIN-03 proof.
Output: `rollout-algo-rm` crate + 3 tests + mdBook chapter.
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
@$HOME/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/phases/04-train-sft-rm-snapshots/04-CONTEXT.md
@.planning/phases/04-train-sft-rm-snapshots/04-RESEARCH.md
@docs/specs/02-algorithms.md
@.planning/phases/04-train-sft-rm-snapshots/04-00-a-wave0-trait-surface-PLAN.md
@.planning/phases/04-train-sft-rm-snapshots/04-01-rollout-snapshots-PLAN.md
@.planning/phases/04-train-sft-rm-snapshots/04-02-algo-sft-skeleton-PLAN.md
@crates/rollout-algo-rm/src/lib.rs
@crates/rollout-algo-rm/Cargo.toml

<interfaces>
<!-- Same trait + types as 04-02; type re-stated here for executor convenience. -->

From rollout-core::config::training (after 04-00-a):
```rust
pub struct RmSettings {
    pub base_model: ModelRef,
    pub optimizer: OptimizerSettings,
    pub budget: TrainingBudget,
    pub dataset: DatasetRef,
    pub head: RmHeadKind,
    pub minibatch_size: u32,
}
pub enum RmHeadKind { BradleyTerry, PairwiseLogistic }
```

From .planning/phases/04-train-sft-rm-snapshots/04-RESEARCH.md §"Don't Hand-Roll":
- Bradley-Terry pairwise loss: `torch.nn.functional.logsigmoid(r_chosen - r_rejected).neg().mean()` (Python side)
- Rust side (MockBackend path): same formula, hand-rolled scalar version for tests.

From rollout-algo-sft (plan 04-02) — mirror structure:
- src/algo.rs hosts PolicyAlgorithm impl
- src/data.rs hosts JSONL loader
- tests/snapshot_resume.rs hosts byte-compare proof
- mdBook chapter docs/book/src/training/sft.md
</interfaces>

</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: Bradley-Terry loss math + JSONL pair loader (unit-tested)</name>
  <files>
    crates/rollout-algo-rm/src/lib.rs,
    crates/rollout-algo-rm/src/loss.rs,
    crates/rollout-algo-rm/src/data.rs,
    crates/rollout-algo-rm/Cargo.toml,
    crates/rollout-algo-rm/tests/bradley_terry_loss.rs
  </files>
  <read_first>
    crates/rollout-algo-rm/src/lib.rs (skeleton from 04-00-b),
    crates/rollout-algo-rm/Cargo.toml (skeleton from 04-00-b — extend with the same dev-deps SftAlgo uses),
    .planning/phases/04-train-sft-rm-snapshots/04-CONTEXT.md D-DATA-01 (RM schema: {"prompt", "chosen", "rejected"} per line),
    .planning/phases/04-train-sft-rm-snapshots/04-RESEARCH.md §"Don't Hand-Roll" → Bradley-Terry one-liner,
    crates/rollout-algo-sft/src/data.rs (after plan 04-02 — mirror the JSONL loader pattern + line-number error reporting),
    docs/specs/02-algorithms.md §7 (RM contract: pairwise preferences, BT objective)
  </read_first>
  <behavior>
    - Test 1 (bradley_terry_zero_diff): when chosen == rejected, BT loss = -ln σ(0) = -ln 0.5 = ln 2 ≈ 0.6931. Tolerance 1e-6.
    - Test 2 (bradley_terry_strong_preference): chosen=5.0, rejected=-5.0, diff=10 → loss ≈ -ln(σ(10)) ≈ 4.5398e-5; near-zero loss.
    - Test 3 (bradley_terry_inverted): chosen=-5.0, rejected=5.0, diff=-10 → loss ≈ -ln(σ(-10)) ≈ 10.0000454; large loss.
    - Test 4 (bradley_terry_batch_mean): batch of [(2.0, 1.0), (1.0, 2.0)] → mean of two losses; numerical correctness within 1e-6.
    - Test 5 (data_loader_pairs): parses `{"prompt":"P","chosen":"C","rejected":"R"}` → PairRow { prompt: "P", chosen: "C", rejected: "R" }.
    - Test 6 (data_loader_rejects_missing_fields): row without `rejected` → Fatal(ConfigInvalid) with line number.
  </behavior>
  <action>
    **Step A — `crates/rollout-algo-rm/Cargo.toml`** (extend skeleton):

    Same dependency shape as `rollout-algo-sft` Cargo.toml (plan 04-02 Task 2 Step A). Specifically:

    - `[dependencies]`: rollout-core, async-trait, serde, serde_json, schemars, smol_str, thiserror, tokio (workspace + fs + io-util), tracing, chrono.
    - `[dev-dependencies]`: tempfile, tokio (macros + rt + rt-multi-thread), rollout-runtime-batch (test-mock-backend feature), rollout-snapshots, rollout-storage, rollout-cloud-local, ndarray (for test-side weight comparisons).

    **Step B — `crates/rollout-algo-rm/src/loss.rs`** — Bradley-Terry loss math (Rust-side reference impl; the real backend computes it Python-side via `F.logsigmoid`):

    ```rust
    //! Bradley-Terry pairwise loss math.
    //!
    //! Spec 02 §7: `L = -E[ ln σ(r_chosen - r_rejected) ]` where σ is the logistic function.
    //!
    //! Numerically-stable form via `logsigmoid(x) = -softplus(-x) = -ln(1 + exp(-x))`
    //! for x > 0 and `logsigmoid(x) = x - ln(1 + exp(x))` for x ≤ 0.
    //! (Standard "softplus trick" to avoid exp(large positive) overflow.)

    /// `logsigmoid(x) = ln σ(x)`. Numerically stable.
    #[must_use]
    pub fn logsigmoid(x: f32) -> f32 {
        // Equivalent to `-softplus(-x)`. Branchless-ish via max trick.
        // For very negative x → near 0, for very positive x → near -large.
        if x >= 0.0 {
            -((-x).exp().ln_1p())
        } else {
            x - x.exp().ln_1p()
        }
    }

    /// Bradley-Terry pairwise loss for a single preference pair.
    /// `r_chosen` / `r_rejected` are the scalar reward-model outputs for the
    /// chosen and rejected responses respectively.
    #[must_use]
    pub fn bradley_terry_loss(r_chosen: f32, r_rejected: f32) -> f32 {
        -logsigmoid(r_chosen - r_rejected)
    }

    /// Batched Bradley-Terry loss — mean over `pairs`.
    /// Returns 0.0 for an empty batch (callers should validate non-empty upstream).
    #[must_use]
    pub fn bradley_terry_batch_mean(pairs: &[(f32, f32)]) -> f32 {
        if pairs.is_empty() { return 0.0; }
        let sum: f32 = pairs.iter().map(|(c, r)| bradley_terry_loss(*c, *r)).sum();
        sum / (pairs.len() as f32)
    }
    ```

    **Step C — `crates/rollout-algo-rm/src/data.rs`** — pair-shaped JSONL loader (mirrors SFT loader):

    ```rust
    //! RM JSONL data loader. Phase 4: `{prompt, chosen, rejected}` schema per D-DATA-01.

    use std::path::Path;

    use rollout_core::{CoreError, Fatal};
    use serde::Deserialize;
    use tokio::io::AsyncBufReadExt;

    /// One preference pair.
    #[derive(Debug, Clone, PartialEq, Deserialize)]
    pub struct PairRow {
        /// Prompt presented to the model.
        pub prompt: String,
        /// Preferred response (higher reward target).
        pub chosen: String,
        /// Dispreferred response (lower reward target).
        pub rejected: String,
    }

    /// Read `path` as JSONL; each line MUST contain `{prompt, chosen, rejected}`.
    /// Malformed lines produce Fatal(ConfigInvalid) with `<file>:<line>`.
    pub async fn load_pairs(path: &Path) -> Result<Vec<PairRow>, CoreError> {
        let file = tokio::fs::File::open(path).await.map_err(|e| {
            CoreError::Fatal(Fatal::ConfigInvalid {
                msg: format!("open {}: {e}", path.display()).into(),
            })
        })?;
        let reader = tokio::io::BufReader::new(file);
        let mut lines = reader.lines();

        let mut out = Vec::new();
        let mut lineno: usize = 0;
        while let Some(line) = lines.next_line().await.map_err(|e| {
            CoreError::Fatal(Fatal::ConfigInvalid { msg: format!("read line: {e}").into() })
        })? {
            lineno += 1;
            if line.trim().is_empty() { continue; }
            let row: PairRow = serde_json::from_str(&line).map_err(|e| {
                CoreError::Fatal(Fatal::ConfigInvalid {
                    msg: format!("{}:{lineno}: {e}", path.display()).into(),
                })
            })?;
            out.push(row);
        }
        Ok(out)
    }
    ```

    **Step D — `crates/rollout-algo-rm/src/lib.rs`** — wire modules:

    ```rust
    //! `rollout-algo-rm` — Bradley-Terry reward-model training (TRAIN-02).

    #![doc(html_root_url = "https://docs.rs/rollout-algo-rm/0.1.0")]

    pub mod algo;
    pub mod data;
    pub mod loss;

    pub use algo::RmAlgo;
    pub use data::{load_pairs, PairRow};
    pub use loss::{bradley_terry_batch_mean, bradley_terry_loss, logsigmoid};
    ```

    (algo.rs lands in Task 2 — for now, leave a placeholder module so this compiles. Alternatively: include the full algo.rs in Task 2 and stage compilation across the two tasks.)

    Actually, to keep Task 1 self-contained: declare `pub mod algo;` here BUT make `algo.rs` a minimal placeholder for Task 1 (`pub struct RmAlgo;`) and complete it in Task 2.

    **Step E — `crates/rollout-algo-rm/tests/bradley_terry_loss.rs`** (golden values; 6 tests):

    ```rust
    //! Bradley-Terry loss correctness.

    use rollout_algo_rm::{bradley_terry_batch_mean, bradley_terry_loss, logsigmoid, load_pairs, PairRow};
    use std::fs;
    use tempfile::tempdir;

    fn approx_eq(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() < tol
    }

    #[test]
    fn bradley_terry_known_values_zero_diff() {
        // chosen = rejected → diff = 0 → loss = -ln σ(0) = -ln 0.5 = ln 2 ≈ 0.6931.
        let l = bradley_terry_loss(1.0, 1.0);
        assert!(approx_eq(l, std::f32::consts::LN_2, 1e-6), "got {l}");
    }

    #[test]
    fn bradley_terry_strong_preference_near_zero() {
        // diff = 10 → σ(10) ≈ 0.99995 → -ln ≈ 4.5398e-5.
        let l = bradley_terry_loss(5.0, -5.0);
        assert!(l < 1e-3, "got {l}; expected near zero");
        assert!(l >= 0.0, "loss must be non-negative; got {l}");
    }

    #[test]
    fn bradley_terry_inverted_preference_large() {
        // diff = -10 → σ(-10) ≈ 4.5398e-5 → -ln ≈ 10.0000.
        let l = bradley_terry_loss(-5.0, 5.0);
        assert!(approx_eq(l, 10.0, 1e-3), "got {l}");
    }

    #[test]
    fn bradley_terry_batch_mean_balances_two_pairs() {
        // Pair 1: diff=+1 → -ln σ(1) ≈ 0.3133
        // Pair 2: diff=-1 → -ln σ(-1) ≈ 1.3133
        // Mean ≈ 0.8133.
        let l = bradley_terry_batch_mean(&[(2.0, 1.0), (1.0, 2.0)]);
        assert!(approx_eq(l, 0.8133, 1e-3), "got {l}");
    }

    #[test]
    fn bradley_terry_batch_mean_empty_returns_zero() {
        assert_eq!(bradley_terry_batch_mean(&[]), 0.0);
    }

    #[test]
    fn logsigmoid_numerical_stability() {
        // logsigmoid(-50) must NOT be -inf or NaN.
        let v = logsigmoid(-50.0);
        assert!(v.is_finite(), "logsigmoid(-50) = {v}");
        assert!(approx_eq(v, -50.0, 1e-4));
        // logsigmoid(+50) must NOT be NaN.
        let v = logsigmoid(50.0);
        assert!(v.is_finite() && v < 0.0);
        assert!(approx_eq(v, 0.0, 1e-4));
    }

    #[tokio::test]
    async fn data_loader_parses_pair_row() {
        let tmp = tempdir().unwrap();
        let p = tmp.path().join("pairs.jsonl");
        fs::write(&p, r#"{"prompt":"P","chosen":"C","rejected":"R"}"#).unwrap();
        let rows = load_pairs(&p).await.unwrap();
        assert_eq!(rows, vec![PairRow {
            prompt: "P".into(), chosen: "C".into(), rejected: "R".into()
        }]);
    }

    #[tokio::test]
    async fn data_loader_rejects_missing_field() {
        let tmp = tempdir().unwrap();
        let p = tmp.path().join("pairs.jsonl");
        fs::write(&p, r#"{"prompt":"P","chosen":"C"}"#).unwrap();
        let err = load_pairs(&p).await.unwrap_err();
        assert!(format!("{err:?}").contains(":1:"));
    }
    ```

    Commit message: `feat(04-04-01): Bradley-Terry pairwise loss + RM JSONL pair loader`.
  </action>
  <verify>
    <automated>
cargo build -p rollout-algo-rm &&
cargo test -p rollout-algo-rm --test bradley_terry_loss &&
cargo clippy -p rollout-algo-rm --all-targets -- -D warnings &&
grep -q 'pub fn bradley_terry_loss' crates/rollout-algo-rm/src/loss.rs &&
grep -q 'pub fn bradley_terry_batch_mean' crates/rollout-algo-rm/src/loss.rs &&
grep -q 'pub async fn load_pairs' crates/rollout-algo-rm/src/data.rs
    </automated>
  </verify>
  <acceptance_criteria>
    - `cargo build -p rollout-algo-rm` exits 0.
    - `cargo test -p rollout-algo-rm --test bradley_terry_loss` exits 0 and reports ≥ 8 tests (6 BT cases + 2 data loader cases).
    - `cargo clippy -p rollout-algo-rm --all-targets -- -D warnings` exits 0.
    - `grep -q 'pub fn bradley_terry_loss' crates/rollout-algo-rm/src/loss.rs` exits 0.
    - `grep -q 'pub fn bradley_terry_batch_mean' crates/rollout-algo-rm/src/loss.rs` exits 0.
    - `grep -q 'pub fn logsigmoid' crates/rollout-algo-rm/src/loss.rs` exits 0 (numerical-stability helper).
    - `grep -q 'pub async fn load_pairs' crates/rollout-algo-rm/src/data.rs` exits 0.
    - HEAD commit message matches `^feat\(04-04-01\):`.
    - DOCS-02 satisfied: same commit ships tests + code. Rustdoc on `bradley_terry_loss` references spec 02 §7 (DOCS-03 — meaningful per-fn docs).
  </acceptance_criteria>
  <done>
    Bradley-Terry loss math is correct (golden values pinned, numerically stable). JSONL pair loader parses the Phase-4 schema + rejects malformed rows. Both are unit-tested with ≥ 8 tests.
  </done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: RmAlgo PolicyAlgorithm impl + snapshot_resume + content-addressed checkpoint test</name>
  <files>
    crates/rollout-algo-rm/src/algo.rs,
    crates/rollout-algo-rm/tests/snapshot_resume.rs,
    crates/rollout-algo-rm/tests/checkpoint_roundtrip.rs,
    docs/book/src/training/rm.md,
    docs/book/src/SUMMARY.md
  </files>
  <read_first>
    crates/rollout-algo-rm/src/lib.rs (after Task 1 — pub mod algo; declared),
    crates/rollout-algo-sft/src/algo.rs (after plan 04-02 — STRUCTURE TO MIRROR; the RM impl differs only in: id = "rm", required_roles unchanged, validate_plan adds head==BradleyTerry check, run signature unchanged but step_once consumes pairs not single sequences, snapshot_save/restore identical to SFT),
    crates/rollout-algo-sft/tests/snapshot_resume.rs (after plan 04-02 — mirror structure exactly for the byte-compare proof),
    docs/specs/02-algorithms.md §7 (RmAlgo contract),
    .planning/phases/04-train-sft-rm-snapshots/04-CONTEXT.md D-DATA-01 (RM dataset shape; PairwiseLogistic head deferred — return Fatal in Phase 4 if selected)
  </read_first>
  <behavior>
    - Test 1 (rm_id_is_stable): `RmAlgo::id() == AlgorithmId("rm")`.
    - Test 2 (validate_plan_rejects_pairwise_logistic): RmSettings with `head: RmHeadKind::PairwiseLogistic` → ≥ 1 ConfigViolation referencing "PairwiseLogistic Phase 4 deferred".
    - Test 3 (validate_plan_rejects_zero_minibatch): minibatch_size=0 → ConfigViolation on "algorithm.rm.minibatch_size".
    - Test 4 (happy_path_two_steps): RmAlgo with MockBackend runs 2 steps; final step counter is 2.
    - Test 5 (checkpoint_roundtrip): RmAlgo save_weights returns ContentId; re-call returns same ContentId (no SGD step in between).
    - Test 6 (bit_identical_resume_at_step_5) — **TRAIN-03 SECOND-WITNESS**: same shape as SFT's byte-compare proof.
  </behavior>
  <action>
    **Step A — `crates/rollout-algo-rm/src/algo.rs`** — full RmAlgo impl. Same shape as SftAlgo with these differences:

    - `id() = AlgorithmId("rm")`.
    - Settings type = `rollout_core::RmSettings`.
    - validate_plan adds: `if head == PairwiseLogistic → ConfigViolation { locator: "algorithm.rm.head", message: "RmHeadKind::PairwiseLogistic lands in Phase 9 (RL-*); Phase 4 supports BradleyTerry only" }`.
    - validate_plan adds: minibatch_size > 0, optimizer.lr > 0 (same as SFT).
    - step_once synthesizes a 2-row TrainBatch (chosen + rejected per pair); loss path conceptually computes BT loss but MockBackend returns constant 0.5 — we document this as a known limitation and the real BT loss only fires under plan 04-05's HF integration.
    - snapshot_save / snapshot_restore: same structure as SFT — meta carries `{ "step": u64, "weights_id": String }`.

    Concrete code:

    ```rust
    //! RmAlgo — PolicyAlgorithm impl for Bradley-Terry reward-model training.

    use std::sync::Arc;

    use async_trait::async_trait;
    use rollout_core::{
        AlgoContext, AlgoDependencies, AlgorithmId, ConfigViolation, CoreError, Fatal, LossScope,
        Plan, PolicyAlgorithm, RmHeadKind, RmSettings, RunOutcome, Snapshot, SnapshotKind,
        TrainBatch, TrainableBackend, WorkerRole,
    };

    use crate::data;

    /// Bradley-Terry reward-model training algorithm.
    pub struct RmAlgo {
        settings: RmSettings,
        backend: Arc<dyn TrainableBackend>,
        #[allow(dead_code)]
        deps: AlgoDependencies,
        step: u64,
    }

    impl RmAlgo {
        /// Drive one optimizer step. Test helper.
        pub async fn step_once(&mut self) -> Result<(), CoreError> {
            // RM batch carries pair-shaped rows: ["<chosen>", "<rejected>"] alternating.
            let batch = TrainBatch {
                n_sequences: 2,
                n_tokens: 32,
                rows: vec!["[chosen]".into(), "[rejected]".into()],
            };
            // Loss scope is conceptually "full" for RM — every token in chosen/rejected
            // contributes to its reward score. (Real implementation: scalar reward per
            // sequence; the BT loss aggregates pairwise.)
            let loss = self.backend.forward_with_loss(&batch, &LossScope::Full).await?;

            let backend_mut = Arc::get_mut(&mut self.backend).ok_or_else(|| {
                CoreError::Fatal(Fatal::PluginContract {
                    plugin: "rollout-algo-rm".into(),
                    msg: "RmAlgo expects exclusive backend ownership (Arc::get_mut failed)".into(),
                })
            })?;
            backend_mut.optimizer_step(loss.grad_handle, &self.settings.optimizer).await?;
            self.step += 1;
            Ok(())
        }

        #[must_use] pub fn step(&self) -> u64 { self.step }
    }

    #[async_trait]
    impl PolicyAlgorithm for RmAlgo {
        fn id() -> AlgorithmId { AlgorithmId(smol_str::SmolStr::new_inline("rm")) }

        type Settings = RmSettings;

        fn from_settings(settings: Self::Settings, deps: AlgoDependencies) -> Result<Self, CoreError> {
            let backend = Arc::clone(&deps.backend);
            Ok(Self { settings, backend, deps, step: 0 })
        }

        fn required_roles(&self) -> Vec<WorkerRole> {
            vec![WorkerRole::LearnerWorker]
        }

        fn validate_plan(&self, _plan: &Plan) -> Result<(), Vec<ConfigViolation>> {
            let mut violations = Vec::new();
            if !matches!(self.settings.head, RmHeadKind::BradleyTerry) {
                violations.push(ConfigViolation {
                    locator: "algorithm.rm.head".into(),
                    message: "RmHeadKind::PairwiseLogistic lands in Phase 9 (RL-*); \
                              Phase 4 supports BradleyTerry only".into(),
                });
            }
            if self.settings.minibatch_size == 0 {
                violations.push(ConfigViolation {
                    locator: "algorithm.rm.minibatch_size".into(),
                    message: "minibatch_size must be >= 1".into(),
                });
            }
            if self.settings.optimizer.lr <= 0.0 {
                violations.push(ConfigViolation {
                    locator: "algorithm.rm.optimizer.lr".into(),
                    message: "lr must be > 0".into(),
                });
            }
            if violations.is_empty() { Ok(()) } else { Err(violations) }
        }

        async fn run(&mut self, ctx: &AlgoContext<'_>) -> Result<RunOutcome, CoreError> {
            let path = match &self.settings.dataset {
                rollout_core::DatasetRef::JsonlPath { path } => path.clone(),
                rollout_core::DatasetRef::Other(_) => {
                    return Err(CoreError::Fatal(Fatal::ConfigInvalid {
                        msg: "DatasetRef::Other lands in Phase 7 (HARNESS-*)".into(),
                    }));
                }
            };
            let _pairs = data::load_pairs(&path).await?;

            let max_steps = self.settings.budget.max_steps.unwrap_or(0);
            for _ in 0..max_steps {
                if ctx.cancel.is_cancelled() { return Ok(RunOutcome::Preempted); }
                self.step_once().await?;
            }
            Ok(RunOutcome::Completed)
        }

        async fn snapshot_save(&self) -> Result<Snapshot, CoreError> {
            let weights_id = self.backend.save_weights().await?;
            let meta = serde_json::json!({
                "step": self.step,
                "weights_id": format!("{weights_id}"),
            });
            Ok(Snapshot {
                id: rollout_core::SnapshotId::from(weights_id),
                kind: SnapshotKind::TrainState,
                run_id: rollout_core::RunId::new(),
                created_at: chrono::Utc::now(),
                label: None,
                parts: vec![rollout_core::SnapshotPart {
                    role: smol_str::SmolStr::new_inline("weights"),
                    content: weights_id,
                    size: 0,
                }],
                algorithm_id: Self::id(),
                meta,
            })
        }

        async fn snapshot_restore(&mut self, snapshot: Snapshot) -> Result<(), CoreError> {
            let step = snapshot.meta.get("step")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| CoreError::Fatal(Fatal::PluginContract {
                    plugin: "rollout-algo-rm".into(),
                    msg: format!("snapshot.meta.step missing or not u64: {}", snapshot.meta).into(),
                }))?;
            self.step = step;
            Ok(())
        }
    }
    ```

    **Step B — `crates/rollout-algo-rm/tests/checkpoint_roundtrip.rs`** (TRAIN-02 content-addressed checkpoint test):

    ```rust
    //! TRAIN-02 content-addressed checkpoint round-trip:
    //! save_weights returns a stable ContentId when nothing changed.

    use std::sync::Arc;

    use rollout_core::{ContentId, TrainableBackend};
    use rollout_runtime_batch::MockBackend;

    #[tokio::test]
    async fn checkpoint_content_id_stable_when_idle() {
        let backend = MockBackend::new_train(99);
        let id1: ContentId = backend.save_weights().await.unwrap();
        let id2: ContentId = backend.save_weights().await.unwrap();
        assert_eq!(id1, id2, "save_weights should be stable when no SGD step occurred");
    }

    #[tokio::test]
    async fn checkpoint_content_id_changes_after_step() {
        use rollout_core::{LossScope, OptimizerKind, OptimizerSettings, TrainBatch};

        let mut backend = MockBackend::new_train(99);
        let id1: ContentId = backend.save_weights().await.unwrap();

        let opt = OptimizerSettings {
            kind: OptimizerKind::Sgd, lr: 0.01, weight_decay: 0.0,
            betas: [0.9, 0.999], eps: 1e-8, warmup_steps: 0,
            schedule: rollout_core::LrSchedule::Constant,
        };
        let batch = TrainBatch { n_sequences: 1, n_tokens: 4, rows: vec!["x".into()] };
        let l = backend.forward_with_loss(&batch, &LossScope::Full).await.unwrap();
        backend.optimizer_step(l.grad_handle, &opt).await.unwrap();

        let id2: ContentId = backend.save_weights().await.unwrap();
        assert_ne!(id1, id2, "save_weights should differ after a non-trivial step");
    }
    ```

    Note: `MockBackend::save_weights` takes `&self`, not `&mut self` (per Step C of plan 04-02 Task 1; the trait method is `&self`). The arrangement above works.

    **Step C — `crates/rollout-algo-rm/tests/snapshot_resume.rs`** — same byte-compare as SFT but with RmAlgo. Verbatim shape from plan 04-02 Task 2 Step G, with `SftAlgo` → `RmAlgo` and `SftSettings → RmSettings` (use `head: RmHeadKind::BradleyTerry`). Drop the unused `loss_on` / `packing` fields; RM doesn't have them.

    Build the `build_algo` helper with the same dependency injection chain (EmbeddedStorage + FsObjectStore + SnapshotterImpl + NoopEmitter), then prove `weights_a == weights_b` after the 5-snapshot-5 split.

    **Step D — Write `docs/book/src/training/rm.md`** (~80 lines). Sections:

    1. Reward-model training overview (BT objective, pairwise preferences).
    2. RmSettings TOML shape.
    3. Bradley-Terry loss math (formula, numerical stability via logsigmoid).
    4. Phase-4 head support: BradleyTerry only; PairwiseLogistic deferred to Phase 9.
    5. JSONL data shape ({prompt, chosen, rejected}).
    6. snapshot_resume pattern (TRAIN-03 second-witness).
    7. Content-addressed final checkpoint (ContentId of postcard-encoded weights).
    8. Forward pointer to plan 04-05 (HF transformers integration for real reward models on Qwen2.5-0.5B-Instruct CPU).

    Add `rm.md` to `docs/book/src/SUMMARY.md` under the Training section.

    Commit message: `feat(04-04-02): RmAlgo PolicyAlgorithm impl + snapshot_resume + checkpoint roundtrip`.
  </action>
  <verify>
    <automated>
cargo build -p rollout-algo-rm &&
cargo test -p rollout-algo-rm --test snapshot_resume &&
cargo test -p rollout-algo-rm --test checkpoint_roundtrip &&
cargo clippy -p rollout-algo-rm --all-targets -- -D warnings &&
grep -q 'impl PolicyAlgorithm for RmAlgo' crates/rollout-algo-rm/src/algo.rs &&
grep -q 'PairwiseLogistic' crates/rollout-algo-rm/src/algo.rs &&
grep -q 'Phase 9' crates/rollout-algo-rm/src/algo.rs &&
grep -q 'bit_identical_resume_at_step_5' crates/rollout-algo-rm/tests/snapshot_resume.rs &&
test -f docs/book/src/training/rm.md &&
grep -q 'training/rm.md' docs/book/src/SUMMARY.md
    </automated>
  </verify>
  <acceptance_criteria>
    - `cargo build -p rollout-algo-rm` exits 0.
    - `cargo test -p rollout-algo-rm --test snapshot_resume` exits 0 and contains `bit_identical_resume_at_step_5`.
    - `cargo test -p rollout-algo-rm --test checkpoint_roundtrip` exits 0 with both tests green.
    - `cargo clippy -p rollout-algo-rm --all-targets -- -D warnings` exits 0.
    - `grep -q 'impl PolicyAlgorithm for RmAlgo' crates/rollout-algo-rm/src/algo.rs` exits 0.
    - `grep -q 'RmHeadKind::BradleyTerry' crates/rollout-algo-rm/src/algo.rs` exits 0.
    - `grep -q 'Phase 9' crates/rollout-algo-rm/src/algo.rs` exits 0 (PairwiseLogistic Fatal sentinel).
    - `grep -q 'assert_eq!(weights_a, weights_b' crates/rollout-algo-rm/tests/snapshot_resume.rs` exits 0 (TRAIN-03 byte-compare assertion present).
    - `test -f docs/book/src/training/rm.md` exits 0.
    - `grep -q 'Bradley-Terry' docs/book/src/training/rm.md` exits 0.
    - `grep -q 'training/rm.md' docs/book/src/SUMMARY.md` exits 0.
    - `mdbook build docs/book` exits 0.
    - HEAD commit message matches `^feat\(04-04-02\):`.
  </acceptance_criteria>
  <done>
    RmAlgo impls PolicyAlgorithm + validates plan (rejects PairwiseLogistic and zero minibatch). Bradley-Terry snapshot_resume byte-compare proof passes — TRAIN-03 second-witness. Checkpoint content-ID round-trip verified. mdBook RM chapter ships.
  </done>
</task>

</tasks>

<verification>
- `cargo test -p rollout-algo-rm --tests` green (3 test files: bradley_terry_loss + snapshot_resume + checkpoint_roundtrip).
- `cargo clippy -p rollout-algo-rm --all-targets -- -D warnings` clean.
- `cargo doc -p rollout-algo-rm --no-deps` clean.
- `mdbook build docs/book` clean.
- `cargo test --workspace --tests` no regressions.
**Conventional commits:** `feat(04-04-01)`, `feat(04-04-02)`.
</verification>

<success_criteria>
- TRAIN-02 delivered: RmAlgo + Bradley-Terry loss math + JSONL pair loader.
- TRAIN-03 second-witness: snapshot_resume.rs byte-compare proof passes.
- PairwiseLogistic head returns Fatal with Phase-9 sentinel.
- Content-addressed checkpoint round-trip verified.
- mdBook RM chapter linked from SUMMARY.md.
</success_criteria>

<output>
After completion, create `.planning/phases/04-train-sft-rm-snapshots/04-04-algo-rm-SUMMARY.md` recording: (1) RmAlgo shape + PolicyAlgorithm methods, (2) Bradley-Terry loss math test coverage (golden values pinned), (3) RM JSONL schema accepted + rejected variants, (4) snapshot_resume.rs byte-compare result, (5) any deviation from plan, (6) explicit confirmation: `cargo test -p rollout-algo-rm --test snapshot_resume` exits 0 — TRAIN-03 second-witness GREEN.
</output>
