# Spec 07 — Harnesses

A **harness** wraps an environment, a tool/action sandbox, or an evaluation suite. From the algorithm's point of view, a harness is a black box that produces observations and accepts actions (or, for eval, produces scored verdicts).

v1 ships three harness families, all implementable as plugins:

1. **Environment harnesses** (`EnvHarness`) — rollout environments.
2. **Tool / action harnesses** (`ToolHarness`) — sandboxed capabilities invoked mid-rollout.
3. **Evaluation harnesses** (`EvalHarness`) — benchmark suites.

## 1. Why three traits, not one

These three look superficially similar (input → output) but have different lifecycles, different state guarantees, and different security requirements:

- An **env harness** owns episode state, lives for the duration of an episode, and is stepped many times.
- A **tool harness** is invoked ad-hoc, idempotently, with no episode-level state on the tool side.
- An **eval harness** is invoked once per checkpoint, produces a structured report, and is stateless across runs.

Conflating them into one trait makes every implementation pay for capabilities it doesn't need and tempts implementors to leak concerns across boundaries.

---

## 2. Environment harnesses

### Trait

```rust
#[async_trait]
pub trait EnvHarness: Send + Sync {
    type Settings: DeserializeOwned + JsonSchema + Send + Sync + 'static;

    fn from_settings(settings: Self::Settings, deps: HarnessDependencies) -> Result<Self, CoreError>
    where Self: Sized;

    /// Open a batch of new episodes. Returns initial observations for each.
    async fn reset(&self, prompts: Vec<Prompt>) -> Result<Vec<Episode>, CoreError>;

    /// Step a batch of episodes with actions. Returns next observations + rewards + done flags.
    async fn step(&self, batch: Vec<EpisodeStep>) -> Result<Vec<StepResult>, CoreError>;

    /// Close episodes (terminal cleanup).
    async fn close(&self, episode_ids: Vec<EpisodeId>) -> Result<(), CoreError>;

    /// Optional: per-episode snapshot for episodic-memory snapshots.
    async fn snapshot_episode(&self, episode_id: EpisodeId) -> Result<Option<Snapshot>, CoreError> {
        Ok(None)  // default: no per-episode state
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeStep {
    pub episode_id: EpisodeId,
    pub action:     Action,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub episode_id: EpisodeId,
    pub observation: Observation,
    pub reward:      Option<Reward>,
    pub done:        bool,
    pub info:        serde_json::Value,    // free-form per-env metadata
}
```

**Note:** every method takes a batch (principle 2). A single-step harness is impossible to make perform well at scale; the batched shape forces the issue.

### Bundled env harnesses

- `rollout-harness-text` — text completion env. Reset takes a prompt; step takes a token sequence; done is set when EOS is generated or `max_tokens` is hit.
- `rollout-harness-tool` — tool-using env. Composes `EnvHarness` with one or more `ToolHarness` plugins to provide a tool-calling environment.

### Lifecycle

```
Actor:
  episodes ← env.reset(prompts)
  loop:
    actions ← policy.generate(episodes.observations)
    results ← env.step(actions.zip(episodes))
    if any results.done: env.close(...)
    trajectory.extend(results)
```

The actor batches as much as possible. The harness is free to internally parallelize per-episode work.

### Local-test parity

`EnvHarness::reset` and `step` must work with `mock_inference_backend()`. No GPU, no network. A test for an env harness consists of: reset, step a few times with canned actions, assert observations have the declared schema.

---

## 3. Tool / action harnesses

### Trait

```rust
#[async_trait]
pub trait ToolHarness: Send + Sync {
    type Settings: DeserializeOwned + JsonSchema + Send + Sync + 'static;

    fn from_settings(settings: Self::Settings, deps: HarnessDependencies) -> Result<Self, CoreError>
    where Self: Sized;

    /// Describe the tools this harness offers. Used at plan time to validate prompts.
    fn descriptor(&self) -> ToolDescriptor;

    /// Invoke a batch of tool calls. Each is independent — no shared state assumed.
    async fn invoke(&self, calls: Vec<ToolCall>) -> Result<Vec<ToolResult>, CoreError>;
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolDescriptor {
    pub tools: Vec<ToolSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolSpec {
    pub name:        SmolStr,
    pub description: String,
    pub input_schema: serde_json::Value,    // JSON Schema
    pub side_effects: SideEffectClass,      // pure | filesystem | network | exec
    pub timeout:     Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub call_id:  ToolCallId,
    pub tool:     SmolStr,
    pub args:     serde_json::Value,
    pub context:  ToolContext,             // worker id, episode id (if any), span id
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub call_id:  ToolCallId,
    pub outcome:  ToolOutcome,             // success | error | timed_out
    pub output:   serde_json::Value,
    pub stderr:   Option<String>,
    pub duration: Duration,
}
```

### Bundled tools

The `rollout-harness-tool` crate bundles these tools, each gated behind a feature flag:

| Tool | Side effect | Sandbox |
|---|---|---|
| `python_exec` | exec | Linux namespaces + seccomp; no network unless allowlisted |
| `shell` | exec | namespaces + seccomp + whitelist of commands |
| `file_read` | filesystem | chroot to per-episode workdir |
| `file_write` | filesystem | chroot to per-episode workdir |
| `http_get` | network | egress allowlist |
| `http_post` | network | egress allowlist |

Each tool can be enabled / disabled / configured independently in the harness's settings.

### Sandboxing v1 boundary

v1 sandboxing uses **Linux namespaces + seccomp + cgroups**. This is a real boundary against accidental escape, not a production-grade malicious-code defense. Documents and metrics clearly state the boundary: "tool harnesses defend against accidental damage; they are not a security perimeter for actively malicious code."

A production-grade sandbox (microVM / gVisor) is a post-v1 capability.

### Local-test parity

Every tool ships with a local test that exercises happy path + at least one failure mode (timeout, schema violation, sandbox denial). Tests pass without external services.

---

## 4. Evaluation harnesses

### Trait

```rust
#[async_trait]
pub trait EvalHarness: Send + Sync {
    type Settings: DeserializeOwned + JsonSchema + Send + Sync + 'static;

    fn from_settings(settings: Self::Settings, deps: HarnessDependencies) -> Result<Self, CoreError>
    where Self: Sized;

    /// Describe what this eval measures and the metrics it emits.
    fn descriptor(&self) -> EvalDescriptor;

    /// Run the eval against a model. Returns a structured report.
    async fn run(&self, model: ModelRef, ctx: EvalContext) -> Result<EvalReport, CoreError>;
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EvalDescriptor {
    pub name:           SmolStr,
    pub version:        SmolStr,
    pub metrics:        Vec<MetricSpec>,
    pub task_count:     Option<u64>,
    pub estimated_cost: ResourceEstimate,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EvalReport {
    pub eval_name:   SmolStr,
    pub eval_version: SmolStr,
    pub model_ref:   ModelRef,
    pub started_at:  DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub metrics:     HashMap<SmolStr, MetricValue>,
    pub per_task:    Vec<TaskResult>,
}
```

### Bundled evals

v1 ships these in-tree as exemplars:

- `rollout-eval-mmlu` — MMLU (knowledge benchmark).
- `rollout-eval-ifeval` — IFEval (instruction following).
- `rollout-eval-gsm8k` — GSM8K (math reasoning).

User plugins implement `EvalHarness` for custom evals.

### Composition with checkpoints

```bash
rollout infer eval --config eval-suite.toml --checkpoint <snapshot-id>
```

The eval harness is invoked once per checkpoint. Reports are persisted to `Storage` (table: `eval_reports`) and to the object store (full report blob, content-addressed).

### Eval gating

A training run config can include an **eval gate**: pause training, run an eval, decide whether to continue. Gates support:

- Stop on regression (any metric drops below a baseline).
- Stop on convergence (rolling N-eval improvement < epsilon).
- Continue unconditionally (just log).

---

## 5. Composition (Env + Tool harnesses)

The bundled `rollout-harness-tool` env composes any number of `ToolHarness` impls:

```toml
[harness]
kind = "tool-env"

[harness.tools]
python = { plugin = "rollout-harness-tool-python" }
file   = { plugin = "rollout-harness-tool-file" }
http   = { plugin = "rollout-harness-tool-http", allowlist = ["api.example.com"] }
```

The env routes tool calls (extracted from model output) to the right `ToolHarness`. The tools themselves know nothing about episodes; the env handles per-episode context.

---

## 6. Configuration

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HarnessGraph {
    pub nodes: Vec<HarnessNode>,
    pub edges: Vec<HarnessEdge>,    // for composition (e.g., env routes calls to tools)
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HarnessNode {
    Env  { id: HarnessId, plugin: PluginRef, config: serde_json::Value },
    Tool { id: HarnessId, plugin: PluginRef, config: serde_json::Value },
    Eval { id: HarnessId, plugin: PluginRef, config: serde_json::Value },
}
```

Validated at plan time:

- Acyclic.
- Edges connect compatible kinds (env → tools allowed; tool → env not allowed).
- Every harness referenced by an algorithm exists in the graph.

---

## 7. Failure modes

| Failure | Detection | Recovery |
|---|---|---|
| Tool timeout | per-call timer | result emits `outcome: timed_out`; the model sees a typed error |
| Sandbox escape detected (seccomp violation) | kernel signal | tool result `outcome: error`; alarm |
| Env step crashes for one episode in a batch | per-episode error in batch result | other episodes in batch unaffected; failed episode marked `done` with `info.error` |
| Eval harness OOM | watchdog | report missing for that eval; gate decides per `on_eval_failure` policy |
| Tool plugin reload during call | host queues new calls | in-flight call completes against old code |

---

## 8. Test contract

For each bundled harness:

- **Unit:** descriptor matches actual behavior (declared metrics emitted, declared schema honored).
- **Integration:** end-to-end with a mock inference backend.
- **Sandbox tests** (for tool harnesses): seccomp violation, namespace escape attempt, timeout, malformed args — each produces the expected typed error.
- **Compatibility tests:** the env + tool composition works end-to-end with a small text-completion mock model.

---

## 9. Open questions

- **Vectorized vs async env harness:** v1 chooses async batches; a vectorized (one process handles many envs in a single tick loop) variant could be useful for very small / cheap envs (e.g., text-grid worlds). Decision deferred to a post-v1 ADR.
- **Tool result schema:** unsigned JSON in v1; should it be typed (e.g., per-tool result struct)? Default: JSON for flexibility; add per-tool result types as a v2 ergonomics pass.
- **Multi-tenant tool harnesses:** out of v1 scope. Single-tenant per worker.
- **Eval caching:** if an eval is run twice against the same model, can we reuse results? v1: opt-in via `eval_cache.enabled = true` with content-addressed keys.
