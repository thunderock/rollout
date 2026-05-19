# Spec 02 — Algorithms

This spec defines the `PolicyAlgorithm` trait surface and the five algorithm families that ship in v1: PPO, GRPO, DPO/IPO/KTO, SFT, and reward-model training. Each family lives in its own crate (`rollout-algo-*`) and is independently publishable.

## 1. Purpose

Algorithms own the **policy update**. Everything else — rollout collection, reward computation, dataset loading, snapshotting — is delegated to traits and resolved at plan time.

An algorithm crate is small (~1–3 kloc of logic in v1). If a crate grows past that, it's almost certainly absorbing responsibilities it should delegate.

## 2. Shared trait surface (`rollout-core`)

```rust
#[async_trait]
pub trait PolicyAlgorithm: Send + Sync {
    /// Stable identifier — used in plan files and config.
    fn id() -> AlgorithmId where Self: Sized;

    /// The algorithm's config type.
    type Settings: DeserializeOwned + Serialize + JsonSchema + Send + Sync + 'static;

    /// Construct from validated settings + injected dependencies.
    fn from_settings(settings: Self::Settings, deps: AlgoDependencies) -> Result<Self, CoreError>
    where Self: Sized;

    /// What worker roles this algorithm requires. Read at plan time.
    fn required_roles(&self) -> Vec<WorkerRole>;

    /// Plan-time validation hook. Receives the full plan; may reject.
    fn validate_plan(&self, plan: &Plan) -> Result<(), Vec<ConfigViolation>>;

    /// The training entry point. The runtime calls this once per learner worker.
    /// Implementations drive the training loop by pulling rollouts / data via deps,
    /// computing updates, and persisting checkpoints.
    async fn run(&mut self, ctx: &AlgoContext) -> Result<RunOutcome, CoreError>;

    /// Convert an external snapshot into algorithm state. Inverse of snapshot_save.
    async fn snapshot_restore(&mut self, snapshot: Snapshot) -> Result<(), CoreError>;

    /// Snapshot the algorithm's current training state.
    async fn snapshot_save(&self) -> Result<Snapshot, CoreError>;
}

/// Dependencies injected at construction. Each is a trait object so the algorithm
/// is decoupled from substrate impls.
pub struct AlgoDependencies {
    pub backend:     Arc<dyn InferenceBackend>,
    pub storage:     Arc<dyn Storage>,
    pub object:      Arc<dyn ObjectStore>,
    pub snapshots:   Arc<dyn Snapshotter>,
    pub events:      Arc<EventEmitter>,
}

/// Per-call context. Provided by the runtime on each `run`.
pub struct AlgoContext<'a> {
    pub plan:    &'a Plan,
    pub worker:  WorkerId,
    pub cancel:  CancellationToken,
    pub clock:   &'a dyn Clock,
}
```

### Common config building blocks

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[non_exhaustive]
pub struct ModelRef {
    /// Either a HuggingFace-style ID, a local path, or an object-store URI.
    pub uri:        String,
    /// Optional content-addressed pin for reproducibility.
    pub content_id: Option<ContentId>,
    /// Tokenizer override; if None, infer from model.
    pub tokenizer:  Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OptimizerSettings {
    pub kind:           OptimizerKind,
    pub lr:             f64,
    pub weight_decay:   f64,
    pub betas:          (f64, f64),
    pub eps:            f64,
    pub warmup_steps:   u32,
    pub schedule:       LrSchedule,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TrainingBudget {
    pub max_steps:      Option<u64>,
    pub max_tokens:     Option<u64>,
    pub max_walltime:   Option<Duration>,
}
```

Every algorithm reuses these. They live in `rollout-core` so config shape is identical across crates.

## 3. PPO (`rollout-algo-ppo`)

Online, on-policy, KL-constrained policy gradient. The workhorse.

### Config

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PpoSettings {
    pub policy:           ModelRef,
    pub reference_policy: Option<ModelRef>,    // for KL; defaults to a frozen copy of policy
    pub reward_model:     RewardSpec,          // see below
    pub optimizer:        OptimizerSettings,
    pub budget:           TrainingBudget,
    pub ppo:              PpoCoreSettings,
    pub rollout:          RolloutSettings,
    pub harness:          HarnessRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PpoCoreSettings {
    pub clip_ratio:        f64,           // typical 0.2
    pub kl_coef_init:      f64,           // typical 0.1
    pub kl_target:         f64,           // typical 6.0; adaptive controller
    pub gamma:             f64,           // typical 1.0
    pub lam:               f64,           // typical 0.95 (GAE)
    pub value_coef:        f64,           // 0.5
    pub entropy_coef:      f64,           // 0.0–0.01
    pub minibatch_size:    u32,
    pub epochs_per_batch:  u32,           // typical 2–4
    pub max_grad_norm:     f64,           // 1.0
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RolloutSettings {
    pub group_size:        u32,           // samples per prompt
    pub max_response_tokens: u32,
    pub temperature:       f64,
    pub top_p:             f64,
    pub batch_size:        u32,           // prompts per inference call
}
```

### Lifecycle

1. **Plan time:** load policy + reference policy refs, validate reward-model plugin, validate harness.
2. **Init:** spin up actor workers (rollout) and learner workers (update). Configurable ratio.
3. **Loop:**
   a. Actors pull prompts, generate rollouts, score with reward model, send trajectories to learner.
   b. Learner pulls a batch of trajectories, computes advantages (GAE), runs PPO update epochs.
   c. Learner periodically broadcasts policy weights to actors.
4. **Snapshot:** training-state (weights, optimizer, RNG, KL controller, step) + optional buffer.
5. **Termination:** budget exhausted, eval gate, or user cancel.

### Failure modes

- **Reward model OOM** → backoff, reduce reward batch size; if persistent, mark fatal.
- **KL divergence explodes** → trigger emergency snapshot, alarm, optionally auto-rollback to last snapshot.
- **Actor drift from learner** → actors stale beyond `max_actor_staleness` requeue their work; learner sends fresh policy.

## 4. GRPO (`rollout-algo-grpo`)

Group-relative policy optimization. Same actor/learner shape as PPO but:

- No value head; advantage is computed group-relatively.
- No GAE.
- Reward is per-group (often per-prompt with `group_size` completions).

### Config

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GrpoSettings {
    pub policy:           ModelRef,
    pub reference_policy: Option<ModelRef>,
    pub reward_model:     RewardSpec,
    pub optimizer:        OptimizerSettings,
    pub budget:           TrainingBudget,
    pub grpo:             GrpoCoreSettings,
    pub rollout:          RolloutSettings,
    pub harness:          HarnessRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GrpoCoreSettings {
    pub clip_ratio:           f64,
    pub kl_coef:              f64,
    pub group_size:           u32,         // sampled completions per prompt
    pub normalize_advantages: bool,        // typical true
    pub minibatch_size:       u32,
    pub epochs_per_batch:     u32,
    pub max_grad_norm:        f64,
}
```

### Lifecycle

Same shape as PPO. Differences:

- Actors sample `group_size` completions per prompt instead of one.
- Learner update uses group-relative advantages: `(r_i - mean(r_group)) / std(r_group)`.
- Reference policy is required for the KL term (not optional like PPO).

## 5. DPO / IPO / KTO (`rollout-algo-dpo`)

Offline preference optimization. **No actor workers.** A single learner consumes a preference dataset.

### Config

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DpoSettings {
    pub policy:           ModelRef,
    pub reference_policy: ModelRef,
    pub objective:        DpoObjective,
    pub optimizer:        OptimizerSettings,
    pub budget:           TrainingBudget,
    pub dataset:          DatasetRef,
    pub minibatch_size:   u32,
    pub beta:             f64,           // KL strength; typical 0.1
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DpoObjective {
    Dpo,
    Ipo  { tau: f64 },
    Kto  { desirable_weight: f64, undesirable_weight: f64 },
}
```

### Lifecycle

1. **Plan time:** validate dataset schema (chosen / rejected for DPO/IPO; binary signal for KTO).
2. **Init:** spin up learner(s). No actors. Data-loader workers stream preference batches.
3. **Loop:** batch → forward both policy and reference policy → compute DPO/IPO/KTO loss → backward → step.
4. **Snapshot:** training-state only (no buffer to snapshot).

### Failure modes

- **Reference policy mismatch** (different tokenizer or vocab from policy) → fatal at plan time.
- **Dataset schema violation** → fatal at plan time.

## 6. SFT (`rollout-algo-sft`)

Supervised fine-tuning. Token-level cross-entropy on instruction data. **First-class**, not a prerequisite afterthought.

### Config

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SftSettings {
    pub base_model:   ModelRef,
    pub optimizer:    OptimizerSettings,
    pub budget:       TrainingBudget,
    pub dataset:      DatasetRef,
    pub packing:      PackingPolicy,        // sample-packing for throughput
    pub loss_on:      LossScope,            // assistant-only vs full
    pub minibatch_size: u32,
    pub gradient_accumulation: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LossScope {
    AssistantOnly,
    Full,
    Custom(MaskSpec),
}
```

### Lifecycle

Standard supervised loop. Sample packing for throughput. Optional loss masking by role.

### Failure modes

- **Tokenizer / model mismatch** → fatal at plan time.
- **Empty assistant span with `LossScope::AssistantOnly`** → sample skipped, counted in `samples_skipped` metric.

## 7. Reward model (`rollout-algo-rm`)

Bradley-Terry head over a base model. Outputs a checkpoint consumable as a `RewardSpec::Plugin` by PPO/GRPO.

### Config

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RmSettings {
    pub base_model:   ModelRef,
    pub optimizer:    OptimizerSettings,
    pub budget:       TrainingBudget,
    pub dataset:      DatasetRef,         // (prompt, chosen, rejected) triples
    pub head:         RmHeadKind,
    pub minibatch_size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RmHeadKind {
    BradleyTerry,
    PairwiseLogistic,
}
```

### Lifecycle

Standard pairwise loss training loop. Output checkpoint is content-addressed and registered in the run's artifact index, where PPO/GRPO can consume it.

## 8. Reward specification (shared)

`RewardSpec` is the common abstraction over "where rewards come from":

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RewardSpec {
    /// A trained reward-model checkpoint.
    Model { checkpoint: ModelRef },

    /// An in-tree or plugin reward function (e.g., string match, regex, code execution).
    Plugin { name: PluginId, config: serde_json::Value },

    /// A composite: weighted sum of multiple rewards.
    Composite { components: Vec<RewardComponent> },

    /// Hand-written DSL expression. Compiled at plan time.
    Expression { expr: String },
}
```

`RewardSpec::Plugin` is what makes rewards extensible: code-execution rewards, regex rewards, "answer-matches-ground-truth" rewards, all are plugins.

## 9. Composition

Algorithm crates depend on `rollout-core` and Layer 3 capability traits. They do **not** depend on `rollout-cloud-*`, `rollout-backend-*`, or any concrete substrate.

```
rollout-algo-ppo
   ├─ rollout-core         (traits, types)
   ├─ rollout-snapshots    (snapshot traits)
   └─ rollout-plugin-host  (plugin invocation)

   *not* aws-sdk-*, *not* vllm bindings directly
```

## 10. Test contract

For each algorithm:

- **Unit:** loss math (PPO clipping, GAE, DPO objective, etc.) on synthetic tensors.
- **Property:** invariants (PPO loss = 0 when ratios = 1; DPO loss = 0 when chosen=rejected with equal reference logprobs).
- **Integration:** end-to-end run on a tiny model (e.g., a 70M parameter test model) with a stubbed reward, asserting that loss decreases over N steps.
- **Snapshot determinism:** train → snapshot at step N → restore → continue → weights at step `N+K` bit-identical to a non-interrupted run.

## 11. Open questions

- **Mixed-precision policy:** bf16 vs fp16 default? Default: bf16 where available, fp16 fallback.
- **Distributed training primitive:** FSDP via accelerate vs DeepSpeed vs in-house. v1 default: FSDP via the inference backend's training mode. Revisit in v2.
- **KL controller for PPO:** adaptive (Schulman et al.) vs fixed. Default: adaptive; expose `ppo.kl_coef_fixed` override.
