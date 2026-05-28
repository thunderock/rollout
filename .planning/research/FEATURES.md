# Feature Research — rollout v1.1 (cloud + multi-node + harnesses)

**Domain:** RL-for-LLMs post-training infrastructure (multi-cloud, multi-node)
**Researched:** 2026-05-27
**Confidence:** HIGH on cloud/preemption/eval surfaces (Context7-equivalent official docs, multiple sources); MEDIUM on coordinator-restart shape and tool-harness depth (peer frameworks diverge; rollout's "no cloud creds for plugin tests" constraint forces choices not made by peers).

This research is scoped to seven categories explicitly named in the milestone prompt: (1) cloud abstraction, (2) work-stealing pull queue, (3) coordinator state recovery, (4) spot preemption, (5) env harness, (6) tool harness, (7) eval harness. v1.0 capabilities (`ObjectStore`, `WorkQueue`, `SecretStore`, `Snapshotter`, content-addressed `ContentId`, deadline heartbeats, redb + Postgres `Storage`) are treated as fixed substrate.

---

## Feature Landscape

### 1. Cloud Abstraction (CLOUD-01 / 02 / 03)

#### Table Stakes

| Feature | Why Expected | Complexity | v1.0 Dependency |
|---------|--------------|------------|-----------------|
| `ObjectStore` impl for S3 (`rollout-cloud-aws`) | Mandatory for any production checkpoint flow; algorithm code must not see SDK types | **M** | Must satisfy v1.0 `ObjectStore` trait (content-addressed `put_bytes` / `get_bytes` / `list` / `delete`); FS impl in `rollout-cloud-local` is the reference contract |
| `ObjectStore` impl for GCS (`rollout-cloud-gcp`) | GCP parity is the second-cloud proof of the abstraction | **M** | Same trait contract as S3; if both pass the same conformance suite, abstraction is real |
| `WorkQueue` impl for SQS (`rollout-cloud-aws`) | Durable cross-node queue; replaces `InMemQueue` for real multi-node | **M** | Must satisfy v1.0 `WorkQueue` trait; visibility-timeout maps to deadline semantics already in v1.0 |
| `WorkQueue` impl for Pub/Sub (`rollout-cloud-gcp`) | GCP parity | **M** | Same trait; ack-deadline = visibility-timeout equivalent |
| `SecretStore` impl for AWS Secrets Manager | Production secrets without env-var leakage | **S** | v1.0 `SecretStore` trait (read-only, allowlist) — already designed for this |
| `SecretStore` impl for GCP Secret Manager | GCP parity | **S** | Same trait |
| `ComputeHint` impl using EC2 IMDSv2 (`rollout-cloud-aws`) | Detect instance type, AZ, spot-vs-on-demand for placement decisions | **S** | Extends v1.0 Linux/macOS ComputeHint |
| `ComputeHint` impl using GCE metadata (`rollout-cloud-gcp`) | GCP parity; also surfaces preemption signal (see §4) | **S** | Same |
| Snapshot storage backed by `ObjectStore` (CLOUD-03) | TrainState snapshots live in S3/GCS, not local disk; already content-addressed in v1.0 | **S** | v1.0 `SnapshotterImpl` already writes through `ObjectStore::put_bytes` — this is wiring, not new code |
| Conformance test suite that runs against ANY `ObjectStore` impl | Without it, "shared trait" is a fiction; FS + S3 + GCS must pass identical tests | **M** | Reuse v1.0 FS object-store tests; parameterize over impl |

#### Differentiators (vs peer frameworks)

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **Zero SDK types in algorithm crates** (enforced by v1.0 architecture-lint) | TRL / OpenRLHF / verl all leak `boto3` / `gcsfs` into training code; rollout's 10-invariant lint already prevents this — v1.1 must keep it green | **S** | v1.0 already enforces; v1.1 just adds the cloud impl crates as new allowed leaves |
| **Plugin-testable without cloud creds** (PROJECT.md constraint) | Every cloud impl ships with a `localstack`-or-equivalent test path and the FS impl is the always-available fallback. Peers either require real S3 or skip integration | **M** | FS impl is the dev-loop substrate; cloud impls are CI-gated behind real creds (or LocalStack for AWS) |
| **Single trait surface, feature-flagged crates** (not per-call `cloud=aws` switching) | One run = one cloud (already in PROJECT.md Out-of-Scope); `Cargo` features select the impl crate at link time. Avoids the verl/OpenRLHF pattern of runtime dispatch by string | **S** | Lean on existing v1.0 feature pattern (`postgres`, `vllm`, `dev-hot-reload`) |
| Content-addressed snapshot identity preserved across local↔cloud move | A snapshot taken on FS in dev has the same `ContentId` when re-uploaded to S3; v1.0's blake3 hash chain is the proof | **S** | Free from v1.0 design; v1.1 test must witness it |

#### Anti-Features (do NOT build)

| Anti-Feature | Why Requested | Why Problematic | Alternative |
|--------------|---------------|-----------------|-------------|
| Cross-cloud single run (AWS storage + GCP compute) | "Cloud-agnostic" sounds desirable | Egress costs, dual SDK surface, region-pinning lies; PROJECT.md already calls it out | One cloud per run; cross-cloud is a snapshot-export concern, not a runtime one |
| Azure / OCI / other clouds in v1.1 | "Three is better than two" | Triples conformance burden before AWS/GCP are proven; v1.0 promised AWS + GCP only | Defer to post-v1; the trait surface makes it additive |
| Runtime cloud selection via string config (`provider = "aws"`) | "Configurable!" | Forces all SDK crates linked into every build; defeats feature-flagging | Cargo features `aws` / `gcp` select at compile time |
| Generic `object_store` crate (Apache Arrow) as the v1.1 surface | "Don't reinvent" | We already have `rollout-core::ObjectStore` shipped in v1.0 with content-addressing baked in; switching forces a public-API break for an external dep that doesn't model `ContentId` | Use Apache `object_store` *underneath* the AWS/GCP impls as an implementation detail if it accelerates us; do NOT expose it |

---

### 2. Work-Stealing Pull Queue (DIST-02)

#### Table Stakes

| Feature | Why Expected | Complexity | v1.0 Dependency |
|---------|--------------|------------|-----------------|
| **Pull (worker → coordinator) semantics**, not push | Push assumes coordinator knows worker capacity; pull lets slow workers self-throttle. Ray IMPALA, Slurm, K8s Job all use pull. | **M** | v1.0 batch-inference CAS state machine is already pull-shaped; extend to general work items |
| Per-item deadline propagated from coordinator | Worker must know "if you don't ack within T, I will reclaim" — this is the SQS visibility-timeout pattern | **S** | v1.0 already has deadline-based heartbeats (500ms / 4s / 5s); use the same deadline budget |
| Reclaim-on-deadline (stale claim → re-issue) | Worker death without ack must not lose work; v1.0 batch-inference resume-scan does this for sample-ids | **M** | v1.0 `restart_no_duplicates` test is the existence proof; generalize the state machine |
| Idempotent ack (worker can retry ack on transient network error) | Standard distributed-queue safety; without it, double-completion races | **S** | Content-addressed item IDs (v1.0 `ContentId`) make idempotency free |
| Duplicate-prevention via CAS on item-id | Same item must not produce two results, even under reclaim | **M** | v1.0 batch infer already implements this; generalize |
| Cancellation (coordinator can drain an item even after assignment) | Spot-preemption + user-initiated stop both need this; today v1.0 has only worker-side fence | **M** | Compose with §4 spot-drain |

#### Differentiators

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **Same `WorkQueue` trait across InMem (dev), SQS, Pub/Sub** | Algorithm code is queue-agnostic; tests run on InMem, prod runs on SQS — same code | **S** | v1.0 already shipped the trait; v1.1 is the conformance proof |
| Deterministic dedup test on every PR (no GPU / no cloud) | Like v1.0's `restart_no_duplicates`, but for the multi-item case. Catches reclaim-race regressions in CI. | **M** | Extend v1.0 MockBackend-driven pattern |
| Backpressure via queue depth, not heartbeat | Coordinator knows total in-flight = (issued - acked); worker pulls only when ready | **S** | Falls out of pull design |

#### Anti-Features

| Anti-Feature | Why Requested | Why Problematic | Alternative |
|--------------|---------------|-----------------|-------------|
| Priority queues with N tiers | "Some work matters more!" | v1.1 has one workload type per run; priority is a v2 concern when episodic + batch + online co-exist | Single FIFO with deadline; defer priorities |
| Cross-run shared queue | "Save infra cost!" | Conflicts with PROJECT.md Out-of-Scope (per-run isolation, no KV-cache sharing across runs) | Queue per run; multiplexed only at the SQS/Pub-Sub *account* level, not the logical queue level |
| Coordinator-side load-aware push routing | "Smart!" | Reintroduces the push-model coupling we just removed; workers know their own load better | Pull + per-worker concurrency limit set in worker config |

---

### 3. Coordinator State Recovery (DIST-03)

#### Table Stakes

| Feature | Why Expected | Complexity | v1.0 Dependency |
|---------|--------------|------------|-----------------|
| **Coordinator process can crash and another instance can take over from persistent storage** | Without this, coord is SPOF and the whole "multi-node, day 1" claim is hollow | **L** | Lean on v1.0 `Storage` trait (redb + Postgres already shipped) |
| Work-assignment ledger persisted on assignment + on ack | A coord that crashes mid-issue must not lose the fact that worker X owns item Y until deadline | **M** | New `Storage` table; redb for dev, Postgres for prod (both already shipped) |
| Worker fence epoch persisted | A new coord must know the current fence epoch to reject stale workers; v1.0 has fence semantics in-memory only | **M** | New `Storage` column; coord recovery = read latest epoch + bump |
| In-flight sample-id state survives coord restart | v1.0 batch-infer state machine already writes CAS state to `ObjectStore` — extend so coord can rebuild assignment view on restart | **M** | v1.0 already content-addresses CAS state; v1.1 adds coord-rebuild path |
| Snapshot-id pointer (latest committed checkpoint) persisted | A new coord must know what the run's last good checkpoint was; without it, restart restarts from epoch 0 | **S** | v1.0 `Snapshotter::list` is the read path; v1.1 adds a pointer table |
| Restart proof: kill coord mid-run, bring up new coord, run completes | Load-bearing test in the spirit of v1.0's `bit_identical_resume_at_step_5` | **M** | Pattern: MockBackend-driven, on every CI build, no GPU |

#### Differentiators

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **Storage-backed only — no Raft, no etcd, no leader election infrastructure** | Peers like Ray ship raylet + GCS server + their own consensus. rollout uses the storage backend (redb / Postgres) as the single source of truth; coord is a stateless replayer | **M** | Cleaner than Slurm's slurmctld-with-backup-controller; closer to Temporal's "event-history replay" model |
| Embedded-storage path works in dev (`redb`) with same recovery semantics | Plugin-testable-locally constraint: a dev should be able to kill the coord on their laptop and watch it recover, no Postgres needed | **S** | v1.0 already ships both; v1.1 just exercises the redb path in the recovery test |
| Coord is **passive** — workers continue executing during coord absence | Workers have deadlines, not heartbeats from coord; coord absence ≤ deadline budget is invisible | **M** | Falls out of v1.0's deadline-based design; v1.1 makes this an explicit promise |

#### Anti-Features

| Anti-Feature | Why Requested | Why Problematic | Alternative |
|--------------|---------------|-----------------|-------------|
| Active-active multi-coordinator (HA pair) | "True HA!" | Requires leader election, split-brain handling; massive complexity for the marginal gain over "fast restart" | Single coord + fast restart from storage; storage layer (Postgres) provides the HA story |
| Embedded Raft / etcd / Consul as coord state store | "Industry standard!" | Adds an operational dependency that defeats "plugin-testable locally without cloud creds" | Use the `Storage` trait we already have; redb in dev, Postgres in prod |
| Custom binary checkpoint format for coord state | "Performance!" | redb + Postgres are already fast enough and ship in v1.0 | Reuse them; coord state is bytes-in-rows |

---

### 4. Spot-Preemption Graceful Drain (DIST-04)

#### Table Stakes

| Feature | Why Expected | Complexity | v1.0 Dependency |
|---------|--------------|------------|-----------------|
| AWS interruption notice via IMDSv2 (`/latest/meta-data/spot/instance-action`) | Standard AWS pattern; 2-minute warning | **S** | Polled at ≤5s from a background task in `rollout-cloud-aws::ComputeHint` |
| GCP preemption notice via metadata server (`/computeMetadata/v1/instance/preempted` or ACPI G2 signal) | Standard GCP pattern; 30s warning (120s preview) | **S** | `rollout-cloud-gcp::ComputeHint` polls + listens for SIGTERM |
| **Graceful drain protocol**: stop pulling new work, finish-current, snapshot, ack-back, exit cleanly | The whole point of preemption handling; without it spot is data-loss prone | **M** | Compose: §2 stop-pull + v1.0 snapshot + §2 ack semantics |
| Cancellation of *current* item if not finishable in budget | 30s on GCP is not enough to finish a batch sample → must requeue, not drop | **M** | Worker writes "preempted" status; coord re-issues |
| Snapshot-on-drain (only if step boundary is reachable in budget) | TrainState snapshot is non-negotiable; batch-infer in-flight sample is requeue-not-snapshot | **M** | v1.0 `Snapshotter::save` is already content-addressed and fast |
| **Drain test in CI** (mock preemption signal triggers drain code path) | Without it, drain rots silently | **M** | MockBackend + injected signal; no real cloud needed |

#### Differentiators

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **Same drain contract across AWS (120s) and GCP (30s)** with a per-cloud budget knob | Peer frameworks either don't drain on GCP (too short) or hand-code per-cloud; rollout has one drain state-machine, two budgets | **M** | Budget = max(snapshot_time_estimate, ack_time) — if too short, requeue instead of snapshot |
| Drain reason recorded in coord ledger (spot-preempt vs voluntary-stop vs OOM) | Operators need this; v1.0 has the `Preempted` error variant already | **S** | v1.0 `Recoverable { Preempted }` already exists in error taxonomy |
| Worker `Preempted` → coord automatically requeues remaining work without operator action | Multi-day training run survives spot churn unattended | **S** | Falls out of §2 reclaim semantics |

#### Anti-Features

| Anti-Feature | Why Requested | Why Problematic | Alternative |
|--------------|---------------|-----------------|-------------|
| CRIU-style process snapshots in v1.1 | "Just freeze and restore!" | SNAPSHOT-01 is explicitly v1.2+; CRIU is best-effort and GPU+CUDA contexts make it brittle | Step-boundary TrainState snapshot (v1.0 already shipped) + work requeue |
| Custom "predict preemption before notice" heuristics (spot price watcher) | "We can be smarter than AWS!" | Adds operational complexity; the notice IS the API | Trust the notice; budget conservatively |
| Cross-cloud failover (preempted on GCP → continue on AWS) | "Resilience!" | Conflicts with PROJECT.md Out-of-Scope (one cloud per run) | Stay on cloud; let coord requeue to a fresh same-cloud worker |
| Mid-step snapshot inside a forward/backward pass | "Save EVERYTHING!" | Not numerically reproducible; conflicts with v1.0's byte-identical-resume invariant | Step-boundary only; in-flight = requeue |

---

### 5. Env Harness — Text Completion (HARNESS-01)

The "text completion env" is concrete: **single-turn `prompt: String → completion: String → reward: f32`**, with optional multi-turn extension. This is the **simplest possible** environment surface, deliberately narrower than Gym/LangChain.

#### Table Stakes

| Feature | Why Expected | Complexity | v1.0 Dependency |
|---------|--------------|------------|-----------------|
| `Env` trait with `reset(seed) → Observation`, `step(action) → (Observation, Reward, Done, Info)` | Gym-shaped; familiar to RL practitioners | **S** | New trait in `rollout-core`; mirrors v1.0 trait style |
| Single-turn text-completion env where `Observation = prompt`, `Action = completion`, one step per episode | The MVP; covers SFT-prompt-eval, simple-reward-model use cases | **S** | None — pure data shape |
| Deterministic `reset(seed)` for reproducibility | RL determinism is load-bearing; v1.0 already proved byte-identical training | **S** | Reuse v1.0 RNG-seeding patterns |
| Reward function as a plugin (Rust cdylib / PyO3 / sidecar) | Reward is user-defined; v1.0 plugin host already supports the three modes | **S** | v1.0 `rollout-plugin-host` is the substrate |
| Built-in `EchoEnv` + `MockRewardEnv` (no model needed) | Plugin testable locally without GPU; matches v1.0 MockBackend pattern | **S** | None |

#### Differentiators

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **Env is a `PolicyAlgorithm`-shaped trait, not a Python-only API** | Peers (rLLM, ARES, NeMo Gym) are Python-first; rollout's env runs in Rust with optional Python plugin reward — algorithm code stays Rust | **M** | Compose: Rust trait + plugin-host invocation |
| Trajectory recording goes through v1.0 `ObjectStore` | Replay buffers (RL-03) and offline eval both consume the same content-addressed trajectory format | **M** | New `Trajectory` type in `rollout-core`; serializer behind `Snapshotter`-like pattern |
| **No tool calls in v1.1 env contract** | Tool harness is HARNESS-02; keeping them separate means env can be tested before tool harness ships | **S** | Compose later; v1.1 env is text-in / text-out only |
| Multi-turn extension via `step` loop with bounded `max_steps` | Same trait, no new shape; conversation env is "single-turn env with a loop" | **S** | Optional; v1.1 ships single-turn first |

#### Anti-Features

| Anti-Feature | Why Requested | Why Problematic | Alternative |
|--------------|---------------|-----------------|-------------|
| Full Gym `spaces.Box` / `spaces.Discrete` type system | "Gym compat!" | LLM observations are strings, not tensors; the abstraction is a mismatch | `Observation = Vec<u8>` + per-env decoder |
| LangChain `BaseAgent` / `AgentExecutor` integration | "Standard!" | Couples our env to LangChain's lifecycle, version churn, Python-only surface | Keep env in Rust; reward plugin is the extension point |
| Streaming token-level reward (per-token shaping) | "Fine-grained!" | Doesn't compose with batch inference v1.0; reward at episode end is the v1.1 contract | Episode-end reward only; per-token shaping is RL-01 (v1.2) concern |
| OpenAI tool-call schema baked into env contract | "AGI-shaped!" | INFER-02 (tool calling integrated into streaming gen) is explicitly v1.2+ | Defer; v1.1 env is text-only |

---

### 6. Tool Harness — Best-Effort Sandbox (HARNESS-02)

**Hard constraint from milestone prompt:** gVisor/Firecracker are OUT. v1.1 = **process isolation + resource limits + path/HTTP allowlist** only. Defense-in-depth = unshare namespaces + seccomp-BPF allowlist + cgroups v2 + filesystem/network allowlists.

#### Table Stakes

| Feature | Why Expected | Complexity | v1.0 Dependency |
|---------|--------------|------------|-----------------|
| `Tool` trait with `name`, `schema (JSON Schema)`, `invoke(args) → Result<Output>` | OpenAI tool-schema-shaped; familiar to LLM-tool-use practitioners | **S** | New trait in `rollout-core` |
| **Shell exec tool** with allowlisted commands | Most-requested tool in agent harnesses; without it nothing is real | **M** | Use `std::process::Command` + Linux `unshare` (CLONE_NEWPID, CLONE_NEWNS, CLONE_NEWNET, CLONE_NEWUTS); macOS = no isolation, dev-only |
| **File read/write tool** with path allowlist (deny ../, symlinks, /proc, /sys) | Table-stakes for code-execution agents | **M** | Pure Rust; canonicalize + prefix check |
| **HTTP fetch tool** with domain allowlist + max-bytes + timeout | Read-only by default; needed for any retrieval agent | **S** | `reqwest` (already a likely tree-dep); allowlist enforced in tool, not at network namespace level |
| **Python eval tool** (subprocess to `python -c` with sandboxed env) | Code-execution agents are the killer app for tool harness | **M** | Subprocess + resource limits; NOT in-process PyO3 (that shares our Python interpreter) |
| Per-tool resource limits: CPU time, memory (cgroups v2 `memory.max`, `pids.max`), wall-clock | Without these a tool can take down the worker | **M** | Linux cgroups v2; macOS = `RLIMIT_AS` + `RLIMIT_CPU` fallback (dev-only) |
| Seccomp-BPF allowlist filter blocking `ptrace`, `mount`, `unshare(NEWUSER)`, `keyctl`, `bpf`, `clone3` (with strict flag filter) | Defense-in-depth; common LLM-sandbox 2026 baseline | **L** | `libseccomp-rs` or `seccompiler` crate; Linux-only |
| All tools testable locally with **deterministic Mock variants** | Plugin-testable-locally constraint | **S** | Mirror v1.0 MockBackend pattern; tool harness has `MockShell`, `MockFs`, `MockHttp` |

#### Differentiators

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **"Sandbox depth" labeled honestly in docs** (process-isolated, NOT VM-isolated) | Peers (Composio, OpenSandbox, AutoGen) often blur this; rollout ships a documented threat model | **S** | Docs deliverable; ARCHITECTURE.md should carry the matrix |
| Tool invocation goes through plugin-host (same 3 modes as v1.0 plugins) | Reward fn, env, tool — all share the cdylib/PyO3/sidecar surface | **S** | v1.0 plugin host is already the substrate |
| Tool output is content-addressed (CAS via v1.0 `ContentId`) | Cache hits across runs ("did anyone shell-exec this command before?") | **M** | Free from v1.0 design |
| Domain-allowlist HTTP enforced at the *application* layer + optional network-namespace deny-all | Layered: app allowlist (cheap, portable) + NetNS for hard-isolation when run as root | **M** | NetNS is opt-in; allowlist is always on |

#### Anti-Features

| Anti-Feature | Why Requested | Why Problematic | Alternative |
|--------------|---------------|-----------------|-------------|
| gVisor / Firecracker microVM backend | "Real isolation!" | Explicit milestone OUT-of-scope; adds kernel-version and CRI deps that break "testable locally on macOS" | Process+seccomp+cgroups; document the threat model |
| Docker-in-Docker tool execution | "Standard!" | Requires Docker daemon at runtime, breaks dev-loop on machines without it | Subprocess + namespaces; Docker is an optional backend, not the default |
| In-process Python `eval()` for the Python tool | "Fast!" | Shares our PyO3 interpreter; one malicious tool corrupts the host | Subprocess always; spawning cost is acceptable for tool-call latency |
| Arbitrary syscall allowlist editable by user config | "Flexible!" | Mis-configuration is a security incident; allowlist should be a const | Single hardened default allowlist; if users need more, they write their own `Tool` impl |
| Tool that mutates worker filesystem outside the sandbox dir | "Convenience!" | Pollutes worker, breaks reproducibility | All writes go to a per-invocation tempdir mounted as bind-mount; cleaned on exit |

---

### 7. Eval Harness — Bundled MMLU / IFEval / GSM8K (HARNESS-03)

**Reference ground truth: EleutherAI `lm-evaluation-harness`.** It is THE de-facto standard; NeMo Microservices, vLLM, and almost every model card cite it. The question is not "do we adopt their scoring contract" — yes — it's "do we bundle datasets or download from HF."

#### Table Stakes

| Feature | Why Expected | Complexity | v1.0 Dependency |
|---------|--------------|------------|-----------------|
| `EvalTask` trait with `load_dataset`, `format_prompt(example) → String`, `score(completion, expected) → f32` | lm-eval-harness uses YAML for this; we use Rust trait + plugin reward fn | **M** | New trait in `rollout-core`; reuses v1.0 plugin-host for `score` |
| **MMLU** (Massive Multitask Language Understanding) task built in | The most-cited LLM benchmark; not optional | **M** | 57 subjects, ~14k 4-choice questions; loader + scorer |
| **IFEval** (Instruction Following Eval) task built in | Standard instruction-tuning eval; verifiable constraints (no judge model needed) | **M** | Constraint checker is pure Rust (regex + string-ops); ~500 prompts |
| **GSM8K** (Grade-School Math) task built in | Standard math/reasoning eval; numeric-extract + match scoring | **M** | Pure Rust scorer (regex + numeric parse) |
| Eval results emitted as content-addressed artifact (v1.0 `ObjectStore`) | Reproducibility: "what did checkpoint X score on MMLU at git-sha Y?" | **S** | v1.0 already content-addresses everything |
| `rollout eval` CLI subcommand (matches v1.0 `infer`/`train`/`snapshot` pattern) | Discoverable; consistent with v1.0 CLI shape | **S** | Extend `rollout-cli` |
| Per-task determinism (seeded sampling order, fixed `temperature=0`) | Without it, eval scores wobble; lm-eval-harness gets criticism for this | **S** | v1.0 RNG-seeding patterns |
| Built-in `MockEvalBackend` (no GPU / no vLLM needed) | Plugin-testable-locally constraint; CI must run eval-harness tests | **S** | v1.0 MockBackend extended for completion outputs |

#### Differentiators

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **Datasets bundled or downloaded with content-addressed pinning** | lm-eval-harness depends on HF datasets being available at eval time; rollout pins by hash so a 2-year-old run can be re-evaluated bit-identically | **M** | At first run, download from HF; persist to `ObjectStore` under `ContentId`; subsequent runs are hash-checked. NOT in-tree binaries (size + license). |
| **lm-eval-harness compatibility mode** (read their YAML task defs) | Network-effect adoption; users with custom YAML tasks can bring them | **L** | Optional v1.1 feature; if too complex, defer to v1.2 |
| Scoring contract emits structured `EvalResult` (per-example, not just aggregate) | Enables fine-grained debugging; lm-eval-harness logs per-example only with extra flags | **S** | Just a struct shape |
| Eval runs as a `WorkQueue` job (same substrate as training) | One sample = one queue item = same dedup/reclaim as training | **S** | Reuse §2 |
| **Offline mode is the default** (no HF call without explicit opt-in) | Air-gapped envs (common in enterprise / research clusters); lm-eval-harness defaults to network and pays for it | **S** | Datasets cached in `ObjectStore`; first download is the only network step |

#### Anti-Features

| Anti-Feature | Why Requested | Why Problematic | Alternative |
|--------------|---------------|-----------------|-------------|
| Bundle datasets as binaries inside crate | "Self-contained!" | Crate size explodes (MMLU + GSM8K = tens of MB), license-redistribution concerns on some HF datasets | Download once, hash-pin, persist to `ObjectStore` |
| Re-implement lm-eval-harness scoring from scratch for "purity" | "Don't depend on external!" | Their scoring is the de-facto ground truth; divergence = scores that nobody trusts | Mirror their scoring algorithm exactly; cite the version pin |
| LLM-as-judge tasks (MT-Bench, AlpacaEval) in v1.1 | "Modern eval!" | Requires a judge model = another inference backend at eval time; complexity multiplier | Defer; v1.1 ships verifiable-scoring tasks only (MMLU/IFEval/GSM8K all are) |
| Custom eval metric DSL | "Flexibility!" | YAML-task-DSL is lm-eval-harness's surface; we either adopt theirs or write Rust trait impls | Rust trait impl (table-stakes) + optional YAML reader (differentiator) |
| Continuous eval / live leaderboard | "Dashboards!" | UI is explicitly Out-of-Scope (v1); eval output is artifacts, not dashboards | CLI prints summary; artifacts go to `ObjectStore` |

---

## Feature Dependencies

```
v1.0 substrate (locked):
  Storage (redb + Postgres)  ──┐
  ObjectStore (FS, content-addressed) ──┐
  WorkQueue (InMem)  ──┐                │
  SecretStore (env-var) ──┐             │
  Snapshotter (TrainState) ──┐          │
  Plugin-host (3 modes) ──┐  │  │  │  │ │
  Deadline heartbeats ──┐ │  │  │  │  │ │
                        │ │  │  │  │  │ │
v1.1 features:          ▼ ▼  ▼  ▼  ▼  ▼ ▼

CLOUD-01 (AWS impls)  ──┬─→ ObjectStore (S3) ─────┐
                        ├─→ WorkQueue (SQS) ──────┤
                        ├─→ SecretStore (SM) ─────┤
                        └─→ ComputeHint (IMDS) ───┤
                                                  ├─→ CLOUD-03 (snapshot storage)
CLOUD-02 (GCP impls)  ──┬─→ ObjectStore (GCS) ────┤
                        ├─→ WorkQueue (Pub/Sub) ──┤
                        ├─→ SecretStore (SM) ─────┤
                        └─→ ComputeHint (metadata)┘
                                                  │
DIST-02 (work-stealing)  ─requires─→ WorkQueue trait + ContentId (v1.0)
DIST-03 (coord restart)  ─requires─→ Storage trait + Snapshotter (v1.0)
DIST-04 (spot drain)     ─requires─→ ComputeHint (CLOUD-01/02) + DIST-02 (requeue) + Snapshotter

HARNESS-01 (env)      ─requires─→ Plugin-host + ObjectStore (trajectory)
HARNESS-02 (tool)     ─requires─→ Plugin-host + ContentId (cache) + ObjectStore (output)
                      ─indep─of─→ HARNESS-01 (can ship separately; v1.1 wires them post-MVP)
HARNESS-03 (eval)     ─requires─→ ObjectStore (dataset cache + results) + WorkQueue (per-sample) + Plugin-host (scorer)
```

### Dependency Notes

- **CLOUD-01/02 are leaves**: nothing in v1.1 depends on AWS specifically vs GCP; the trait surface is the contract. The conformance test suite IS the contract.
- **DIST-02 unlocks DIST-04**: graceful drain is "stop pulling + requeue current"; needs the pull/requeue primitives first.
- **DIST-03 is independently shippable**: coord restart can be tested with InMem queue + redb storage, no cloud needed. This is the v1.1 CI-friendly path.
- **HARNESS-01/02/03 are siblings**: env doesn't depend on tool; tool doesn't depend on env; eval doesn't depend on either. They can ship in parallel and be composed by user config (e.g., "env with tool" is a wiring, not a feature).
- **All cloud impls satisfy already-shipped traits**: no breaking change to `rollout-core` v1.0; cloud crates are additive.

---

## MVP Definition (v1.1 scope)

### Launch With (v1.1 proof bar: 3+ node setup runs `make smoke` against real AWS/GCP; spot-preempt triggers graceful drain)

- [ ] **CLOUD-01** S3 + SQS + Secrets Manager + IMDSv2 impls (table-stakes for AWS half of multi-cloud claim)
- [ ] **CLOUD-02** GCS + Pub/Sub + Secret Manager + GCE metadata impls (table-stakes for GCP half)
- [ ] **CLOUD-03** Snapshot storage via `ObjectStore` (wiring; falls out of trait)
- [ ] **DIST-01** Multi-node coord+worker exercised on real cloud (not just `make smoke`)
- [ ] **DIST-02** Work-stealing pull queue with dedup test on every CI build
- [ ] **DIST-03** Coord restart from storage, witnessed by a no-GPU CI test
- [ ] **DIST-04** Spot-preempt graceful drain on both AWS (120s) and GCP (30s)
- [ ] **HARNESS-01** Text-completion env with `EchoEnv` + `MockRewardEnv` + plugin-host reward
- [ ] **HARNESS-02** Tool harness: shell + file r/w + HTTP fetch + Python eval, with seccomp + cgroups v2 + allowlists (Linux full / macOS dev-only stub)
- [ ] **HARNESS-03** Eval harness: MMLU + IFEval + GSM8K with hash-pinned dataset cache, `rollout eval` CLI, offline-mode default

### Add After v1.1 Validation (v1.2+)

- [ ] lm-eval-harness YAML compatibility mode (HARNESS-03 stretch)
- [ ] Multi-turn env (HARNESS-01 extension; same trait)
- [ ] Tool-call integration into streaming inference (INFER-02 — already v1.2 per PROJECT.md)
- [ ] Buffer / Episodic snapshot kinds (RL-03 — v1.2)
- [ ] Active-active coordinator pair (DIST-03 stretch; only if customers ask)

### Out of Scope (v1.1, locked)

- gVisor / Firecracker microVM tool sandboxing
- CRIU process snapshots (SNAPSHOT-01 → v1.2)
- Cross-cloud single run
- Azure / OCI cloud impls
- LLM-as-judge eval tasks
- UI / dashboards
- Online inference (INFER-01 → v1.2)
- PPO/GRPO RL phases (RL-01..04 → v1.2)

---

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority | Rationale |
|---------|------------|---------------------|----------|-----------|
| CLOUD-01 (AWS impls) | HIGH | MEDIUM | **P1** | Without it, "multi-cloud" is one cloud |
| CLOUD-02 (GCP impls) | HIGH | MEDIUM | **P1** | Two clouds proves the abstraction |
| CLOUD-03 (snapshot storage) | HIGH | LOW | **P1** | Falls out of CLOUD-01/02; wiring only |
| DIST-01 (multi-node) | HIGH | MEDIUM | **P1** | The headline of v1.1 |
| DIST-02 (work-stealing) | HIGH | MEDIUM | **P1** | Without it, multi-node is sharded-not-distributed |
| DIST-03 (coord restart) | HIGH | HIGH | **P1** | SPOF removal; load-bearing for "real multi-node" claim |
| DIST-04 (spot drain) | HIGH | MEDIUM | **P1** | Explicit proof-bar item |
| HARNESS-01 (env) | MEDIUM | LOW | **P1** | Cheap and unlocks future RL phases |
| HARNESS-02 (tool) | MEDIUM | HIGH | **P1** | Most complexity in v1.1; must ship for harness story to be real |
| HARNESS-03 (eval) | HIGH | MEDIUM | **P1** | Provides the scoring contract for any downstream RL work |
| lm-eval-harness YAML compat | LOW | HIGH | **P2** | Adoption boost, but not required to land v1.1 |
| Multi-turn env | MEDIUM | LOW | **P2** | Falls out of HARNESS-01 trait; nice-to-have |
| Cgroups v2 process-resource limits in tool harness | HIGH | MEDIUM | **P1** | Without it tool harness can take down workers — table stakes for production |
| Seccomp-BPF allowlist | HIGH | HIGH | **P1** | Defense-in-depth baseline; without it "best-effort sandbox" is hollow |

**Priority key:** P1 = must have for v1.1 launch · P2 = should have, add when possible · P3 = nice to have, defer

---

## Implications for Roadmap

Group these into phases such that each phase has a load-bearing test that runs on every CI build (no GPU / no cloud creds), matching v1.0's discipline (`bit_identical_resume_at_step_5`, `restart_no_duplicates`):

1. **Cloud impls** (CLOUD-01/02/03) — phase witness: ObjectStore + WorkQueue conformance test parameterized over FS, S3 (via LocalStack), GCS (via fake-gcs-server).
2. **Distribution** (DIST-01/02/03/04) — phase witness: `coord_restart_no_duplicates` (kill coord mid-batch, new coord recovers, zero duplicate samples). Drain test = mock IMDS/metadata signal triggers drain path.
3. **Harnesses** (HARNESS-01/02/03) — phase witnesses: `env_deterministic_replay` (same seed → same trajectory), `tool_sandbox_escape_blocked` (negative test: forbidden syscall returns EPERM), `eval_score_matches_lm_eval_harness` (MMLU subset, ≤1% deviation).

The biggest risk is **DIST-03 (coord restart)** — it has the highest complexity-to-precedent ratio and the smallest peer-framework template to copy from (Ray uses GCS+Raft, Slurm uses backup-controller, Temporal uses event-replay — none directly apply). Flag this for deeper architecture research before phase kick-off.

---

## Sources

- [Apache `object_store` crate (Arrow ecosystem) — unified S3/GCS/Azure/FS trait](https://docs.rs/object_store) — HIGH confidence, official docs
- [AWS Spot interruption notice (IMDS, 2-min)](https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/spot-instance-termination-notices.html) — HIGH, official
- [AWS best-practices for handling Spot interruptions](https://aws.amazon.com/blogs/compute/best-practices-for-handling-ec2-spot-instance-interruptions/) — HIGH, official
- [GCP Spot VM preemption (30s default, 120s preview)](https://docs.cloud.google.com/compute/docs/instances/spot) — HIGH, official
- [Ray RLlib distributed architecture (LearnerGroup, EnvRunner queue)](https://deepwiki.com/ray-project/ray/5-ray-rllib) — MEDIUM, secondary doc but consistent with Ray docs
- [Slurm controller HA on Kubernetes patterns](https://blog.character.ai/slonk/) — MEDIUM, vendor blog with operational details
- [Temporal durable execution (event-history replay model)](https://docs.temporal.io/temporal-service/persistence) — HIGH, official; informs DIST-03 design analogy
- [EleutherAI lm-evaluation-harness (MMLU/IFEval/GSM8K source of truth)](https://github.com/EleutherAI/lm-evaluation-harness) — HIGH, official
- [lm-eval-harness offline-mode usage pattern](https://github.com/EleutherAI/lm-evaluation-harness/blob/main/docs/new_task_guide.md) — HIGH, official
- [Agent sandbox isolation depth (seccomp / cgroups v2 / namespaces, 2026 landscape)](https://tianpan.co/blog/2026-03-09-agent-sandboxing-secure-code-execution) — MEDIUM, recent secondary source
- [Understanding sandbox isolation (namespaces, cgroups, seccomp, gVisor)](https://ubos.tech/news/understanding-sandbox-isolation-namespaces-cgroups-seccomp-gvisor-and-webassembly/) — MEDIUM, technical overview
- [RL env taxonomy for LLM agents (text-completion contract shape)](https://leehanchung.github.io/blogs/2026/03/21/rl-environments-for-llm-agents/) — MEDIUM, recent secondary
- [rLLM project (env abstractions for LLM RL)](https://rllm-project.com/) — MEDIUM, peer framework reference
- [RLHF infrastructure comparison: verl, OpenRLHF, TRL (2026)](https://www.spheron.network/blog/rlhf-training-infrastructure-verl-openrlhf-trl-gpu-cloud/) — MEDIUM, vendor comparison
- [NeMo-Aligner scalable model alignment toolkit](https://arxiv.org/html/2405.01481v1) — HIGH, peer-reviewed
- [OpenRLHF architecture paper](https://arxiv.org/html/2501.03262v4) — HIGH, peer-reviewed
