# Spec 03 — Plugin system

The plugin system is what makes `rollout` extensible without forcing extensions into core. It supports plugins written in **Rust** (cdylib) and **Python** (PyO3 in-process or subprocess RPC sidecar), discovered at plan time, loaded at run time, and hot-reloadable in dev.

## 1. Purpose

A plugin is a user-supplied unit that implements one of the framework's open trait points:

- `EnvHarness`, `ToolHarness`, `EvalHarness`
- `RewardModel`
- `InferenceBackend` (when a user wants their own engine)
- `Storage`, `Queue`, `ObjectStore` (rare; usually only for niche infra)
- Custom: any algorithm-defined extension trait declared via the plugin manifest

Plugins are the only sanctioned way to extend the framework. **There is no "edit core" path** for users.

## 2. Plugin manifest

Every plugin ships a `rollout-plugin.toml` at its package root:

```toml
[plugin]
name        = "my-reward"
version     = "0.3.1"
kind        = "reward-model"        # one of: env-harness | tool-harness | eval-harness |
                                    #         reward-model | inference-backend |
                                    #         storage | queue | object-store | custom
trait       = "rollout_core::RewardModel"
mode        = "pyo3"                # one of: pyo3 | sidecar | rust-cdylib

[runtime]
python_min  = "3.10"                # only for pyo3 / sidecar
gpu         = false                 # plan-time resource hint
memory_mib  = 512                   # plan-time resource hint

[entry]
# For Rust cdylib:
cdylib   = "libmy_reward.so"
symbol   = "rollout_plugin_factory"

# For PyO3:
module   = "my_reward"
factory  = "create_plugin"

# For sidecar:
command  = ["python", "-m", "my_reward.server"]
protocol = "grpc"                   # always grpc in v1

[config]
schema = "schema.json"              # JSON Schema for the plugin's config block
```

**Plan-time validation:**

- Manifest parses cleanly.
- Declared `trait` matches what the plugin actually implements (checked by loading metadata, *not* full code).
- `kind` is one of the allowed kinds.
- `config.schema` validates the user's config block.
- `runtime.gpu` / `runtime.memory_mib` are consistent with the resource budget.

## 3. Loading modes

Three modes. Each has different perf/safety/dev-ergonomics tradeoffs.

### 3.1 Rust cdylib

A Rust plugin compiled to a `cdylib`. Loaded via `libloading` at worker init. Pointer call; zero serialization overhead. Crashes take down the worker.

**Use when:** the plugin is performance-critical and authored by a trusted maintainer.

**Stability:** Rust does not have a stable ABI. We pin a workspace-shared toolchain and pass everything through a versioned C-ABI shim (`rollout-plugin-abi`).

### 3.2 PyO3 in-process

A Python plugin loaded via `pyo3` and embedded in the worker process. Function calls cross the FFI boundary; data passed as numpy / pyarrow zero-copy where possible.

**Use when:** the plugin is moderately performance-sensitive and Python-only.

**Caveats:**

- GIL contention: the host runs Python code on a single OS thread per worker. Heavy-CPU plugins should release the GIL (`Py::allow_threads`) where possible.
- Crash blast-radius is the whole worker. Plugin panics terminate the worker.

### 3.3 Sidecar / subprocess RPC

A separate process, spawned by the worker, communicating via gRPC over a UNIX socket. Data is serialized; per-call overhead is higher than the other two modes.

**Use when:** the plugin needs isolation (untrusted code, native deps that don't compose with the worker, hot reload during dev), or when GIL contention with another in-process plugin is the bottleneck.

**Benefits:**

- Crash isolation: a sidecar crash does not kill the worker; the worker respawns it.
- Hot reload by replacing the sidecar process.
- Per-sidecar resource limits (cgroups, memory caps).

## 4. Trait surface

```rust
/// Implemented by Rust cdylib plugins.
pub trait Plugin: Send + Sync {
    /// Stable plugin identity.
    fn manifest(&self) -> &PluginManifest;

    /// Validate the user's config block. Called at plan time before any other method.
    fn validate_config(&self, config: &serde_json::Value) -> Result<(), ConfigViolation>;

    /// One-time initialization. Called once per worker per plugin instance.
    fn init(&mut self, config: serde_json::Value, deps: PluginDependencies) -> Result<(), CoreError>;

    /// Optional preflight check. Called after init, before run.
    fn preflight(&self) -> Result<(), CoreError> { Ok(()) }

    /// Cleanup on worker shutdown.
    fn shutdown(&mut self) -> Result<(), CoreError>;
}

/// The factory symbol exported by every Rust cdylib plugin.
#[no_mangle]
pub extern "C" fn rollout_plugin_factory() -> *mut dyn Plugin;
```

PyO3 plugins implement an analogous Python class:

```python
from rollout_plugin import Plugin, PluginDependencies, ConfigViolation

class MyRewardPlugin(Plugin):
    def manifest(self) -> dict: ...
    def validate_config(self, config: dict) -> None: ...
    def init(self, config: dict, deps: PluginDependencies) -> None: ...
    def preflight(self) -> None: ...
    def shutdown(self) -> None: ...

def create_plugin() -> Plugin:
    return MyRewardPlugin()
```

Sidecar plugins implement a gRPC service defined in `proto/plugin.proto`.

## 5. Plugin host (`rollout-plugin-host`)

The host abstracts the three modes behind a uniform interface:

```rust
#[async_trait]
pub trait PluginHost: Send + Sync {
    /// Load a plugin by manifest path. Returns a handle.
    async fn load(&self, manifest_path: &Path, config: serde_json::Value) -> Result<PluginHandle, CoreError>;

    /// Invoke a typed call against a plugin. The host transparently routes
    /// to in-process / sidecar based on the plugin's declared mode.
    async fn call<Req, Res>(&self, handle: &PluginHandle, method: &str, req: Req) -> Result<Res, CoreError>
    where
        Req: Serialize + Send,
        Res: DeserializeOwned + Send;

    /// Hot reload (dev only).
    async fn reload(&self, handle: &PluginHandle) -> Result<(), CoreError>;

    /// Drop a plugin handle.
    async fn unload(&self, handle: PluginHandle) -> Result<(), CoreError>;
}
```

## 6. Local-test contract

Every plugin must include a CI-runnable local test:

```bash
# Rust plugin
cargo test -p rollout-plugin-my-reward

# Python plugin
uv run pytest python/rollout-plugin-my-reward
```

The test must:

- Run without network access.
- Run without cloud credentials.
- Run without a GPU.
- Run in < 60s on commodity hardware.
- Exercise the plugin's main code path with at least one happy case and one failure case.

The framework provides fixtures to satisfy this:

```rust
use rollout_test_fixtures::{mock_inference_backend, mock_storage, mock_object_store};

#[tokio::test]
async fn reward_returns_score() {
    let plugin = MyRewardPlugin::new();
    plugin.init(test_config(), test_dependencies()).await.unwrap();
    let result = plugin.score(test_samples()).await.unwrap();
    assert_eq!(result.len(), 4);
}
```

Workspace CI runs every plugin's local test in a sandbox with no cloud creds, no GPU, and a network namespace blocking external traffic. A plugin that fails this contract fails workspace CI.

## 7. Hot reload (dev only)

In dev mode (`rollout run --hot-reload`):

- **PyO3:** the host releases the module reference, calls `importlib.reload`, and reinstantiates the plugin.
- **Sidecar:** the host SIGTERMs the sidecar, waits for clean exit, spawns a new one.
- **Rust cdylib:** **not supported.** Rust cdylib hot reload is unsafe in general; we don't pretend otherwise.

In-flight calls complete before reload; new calls block on the new plugin being `init`'d.

Production runs ignore the `--hot-reload` flag with a warning. Hot reload changes the version pinning story; production wants version stability.

## 8. Discovery

Plugins are discovered from three locations, in order:

1. **Plan-declared:** the `plugins = [...]` list in the run config, with explicit paths or PyPI/crates.io references.
2. **Project-local:** `./plugins/` directory.
3. **User-installed:** `$XDG_DATA_HOME/rollout/plugins/` (default `~/.local/share/rollout/plugins/`).

A plugin discovered earlier shadows the same name discovered later.

## 9. Failure modes

| Failure | Detection | Recovery |
|---|---|---|
| Manifest invalid | plan time | fatal at plan; descriptive error |
| Trait mismatch (declared vs actual) | plan time (metadata probe) | fatal at plan |
| Init failure | worker startup | retry per `lifecycle_retries`; then fatal |
| In-process plugin panic / Python exception escapes init | worker startup | worker fails; runtime requeues work |
| Sidecar crash mid-run | host detects via grpc error | host respawns sidecar; in-flight call retried per `RetryHint` |
| Hot reload during in-flight call | reload defers until call completes | new call uses new code |
| GIL deadlock (PyO3 plugin holds GIL forever) | watchdog timer per call | call fails with `Recoverable(Throttled)`; if persistent, worker fails |

## 10. Security

Plugins are user code. The framework enforces:

- **No filesystem write outside the worker's run directory** by default (sidecar-mode plugins, via the host's gRPC interceptor).
- **No outbound network from sidecars** unless explicitly allowlisted in the manifest under `[network]`.
- **No environment-variable inheritance** beyond an allowlist (credentials are passed via the host, never via env).
- **Resource caps** per sidecar (RAM, CPU shares, FD count) when run on Linux with cgroups.

In-process plugins (PyO3 and Rust cdylib) cannot be sandboxed at this level — they run in the worker's address space. If you don't trust the plugin, run it as a sidecar.

## 11. Test contract for the plugin host

`rollout-plugin-host` tests:

- **Unit:** manifest parsing and validation.
- **Integration:** load one plugin in each mode (Rust cdylib, PyO3, sidecar), exercise a call, unload.
- **Failure injection:** sidecar crashes mid-call → host respawns; PyO3 plugin raises → host returns typed error; cdylib symbol missing → load fails at plan time.
- **Hot reload:** reload a sidecar mid-run, verify in-flight calls succeed and new calls use new code.

## 12. Open questions

- **Plugin manifest format:** TOML in v1 is committed. Reconsider JSON or YAML only with hard data showing tooling pain.
- **gRPC vs Cap'n Proto for sidecars:** gRPC in v1 for tooling ubiquity. Cap'n Proto offers zero-copy but the toolchain is heavier.
- **Plugin signing:** out of v1 scope. Will revisit if a plugin ecosystem emerges.
- **Multiple instances of one plugin per worker:** allowed; instances are fully isolated. Useful for ensemble reward models.
