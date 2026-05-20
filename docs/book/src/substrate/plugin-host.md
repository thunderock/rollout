# Plugin host

`rollout-plugin-host` is the Phase-2 implementation of `rollout_core::PluginHost`. It wires three loading modes against the trait surface delivered in Wave-0 and persists manifests through `rollout-storage` when constructed via [`PluginHostImpl::with_storage`](../../../rustdoc/rollout_plugin_host/struct.PluginHostImpl.html).

## Three modes

| Mode | Crate feature | Where it runs | Hot reload |
| --- | --- | --- | --- |
| Rust cdylib (`PluginMode::RustCdylib`) | `cdylib` (default) | In-process, native code | **Unsupported.** Returns `Fatal(PluginContract)` per [spec 03 §7](../../../specs/03-plugin-system.md). |
| PyO3 in-process (`PluginMode::Pyo3`) | `pyo3` (default) | Dedicated Python OS thread | `importlib.reload` (requires `dev-hot-reload`). |
| Python sidecar (`PluginMode::Sidecar`) | `sidecar` (default) | Subprocess over Unix Domain Socket | SIGTERM + respawn (requires `dev-hot-reload`). |

The host exposes one type — `PluginHostImpl` — that dispatches on `manifest.mode` at `load()` time. Each mode owns a small state struct kept in a parallel `HashMap<PluginId, HandleState>` so the public `PluginHandle` remains `Clone + Send + Sync` POD.

## Manifest

Plugins ship a `rollout-plugin.toml` parsed by [`parse_manifest`](../../../rustdoc/rollout_plugin_host/fn.parse_manifest.html). The schema matches `rollout_core::PluginManifest`:

```toml
name             = "sample-inproc"
version          = "0.1.0"
kind             = "env-harness"           # PluginKind variant (kebab-case)
trait_id         = "rollout_core::Plugin"
mode             = "pyo3"                  # pyo3 | sidecar | rust-cdylib
network_allowlist = []

[runtime]
python_min = "3.11"      # required for pyo3
gpu        = false
memory_mib = 64

[entry.pyo3]
module  = "sample_inproc.plugin"
factory = "create_plugin"
```

`validate_manifest` runs cheap plan-time checks: pyo3 plugins must declare `python_min >= 3.11` (stdlib `tomllib` + PyO3 abi3-py311), names/versions must be non-empty, and the manifest must parse against `serde(rename_all = "kebab-case")`.

## C-ABI shim (cdylib mode)

Phase 2 keeps the shim as an **internal module** (`src/modes/abi.rs`) per RESEARCH Open Question 3 — it graduates to a standalone `rollout-plugin-abi` crate when an external plugin ecosystem emerges (likely Phase 7+).

The contract:

```rust
#[repr(C)]
pub struct Buf { pub ptr: *mut u8, pub len: usize, pub cap: usize }

#[repr(C)]
pub struct RolloutPluginVtable {
    pub abi_version: u32,                  // must equal ABI_VERSION (1)
    pub name: *const c_char,
    pub call: extern "C" fn(
        method: *const u8, method_len: usize,
        payload: *const u8, payload_len: usize,
        out: *mut Buf
    ) -> i32,
    pub free_buf: extern "C" fn(buf: Buf),
}

#[no_mangle]
pub extern "C" fn rollout_plugin_factory() -> *mut RolloutPluginVtable;
```

The host copies the returned `Buf` before calling `free_buf` so allocator mismatches across the cdylib boundary cannot corrupt anything. ABI mismatches surface as `Fatal(PluginContract { msg: "cdylib ABI mismatch: got N, expected 1" })`.

The in-tree example lives at [`tests/smoke/plugins/rust_cdylib_sample/`](https://github.com/astiwari/rollout/tree/main/tests/smoke/plugins/rust_cdylib_sample). It's an out-of-workspace cdylib crate (its own `[workspace]`) so `cargo build --workspace` doesn't churn on it; the smoke driver (plan 02-07) builds it with `cargo build --manifest-path … --release`.

## Sidecar IPC

The in-tree sample uses **stdlib-only length-prefixed JSON over `AF_UNIX`** per RESEARCH Pitfall 9 + AGENTS.md §7 ("every plugin testable locally — without cloud creds / GPUs / external services"). The wire format is intentionally tiny:

```
request:  [u32 BE length][utf-8 JSON {"method": str, "payload": str}]
response: [u32 BE length][utf-8 JSON {...}]
```

This avoids forcing `pip install grpcio` into the `cargo test` path. Real production sidecars that need gRPC can ingest the same `rollout-proto::plugin::v1` service stubs (`make protos` regenerates the Python side); the host supports both via the `SidecarProtocol` enum (`FramedJsonUds` and `GrpcUds`). Phase 2 only ships the `FramedJsonUds` code path; `GrpcUds` lands when a sidecar consumer needs it.

UDS paths default to `./data/sidecars/<plugin>-<pid>.sock` and are `chmod 600` on the Python side. The host removes the socket on unload + on respawn.

## Hot reload

Behind the `dev-hot-reload` Cargo feature only:

- **PyO3.** The dedicated Python thread runs `importlib.reload(module)` and re-invokes `factory()` on the same channel. In-flight calls running on the old object complete naturally; subsequent calls hit the new object.
- **Sidecar.** SIGTERM the child (via `nix::sys::signal::kill`), wait 2 s, SIGKILL on holdout, then respawn with the same argv on a fresh UDS path.
- **cdylib.** Returns `Fatal(PluginContract { msg: "cdylib reload unsupported per spec 03 §7" })` — Rust has no stable ABI; `dlclose` while another task holds a `Box<dyn Plugin>` is UB. Production should not reload cdylibs.

The feature is off by default. Per spec 03 §7, production deployments ignore reload flags entirely (with a warning) — that surfacing lands with the coordinator in plan 02-06.

## Sandboxing (Phase 2 scope)

Network allowlist only (D-SANDBOX-01). cgroups + seccomp + FD limits + fs write restrictions are tracked as TODOs referencing Phase 7. The allowlist surfaces as `PluginManifest::network_allowlist`; the egress proxy lives in the worker / cloud-local layer and lands when the tool harness needs adversarial isolation (Phase 7).

## Dependency direction

`rollout-plugin-host` **does not** depend on `rollout-transport`. The Wave-0 dep-direction lint enforces this — sidecar IPC uses UDS framing via `rollout-proto`, not the QUIC/HTTP/2 transport from `rollout-transport`. Forgetting this rule means the layered architecture (AGENTS.md §9) drifts; the integration test in `crates/rollout-core/tests/dependency_direction.rs` catches drift on every PR.

## Observability

Every public op emits a structured event with `target = "plugin_host"` and a `plugin_id` field:

| Event | Fields | When |
| --- | --- | --- |
| `plugin_loaded` | `plugin_id`, `mode`, `path`/`module`/`socket` | `load()` succeeds |
| `plugin_reloaded` | `plugin_id`, `reason` | `reload()` succeeds (or is rejected) |
| `plugin_call` (span) | `plugin_id`, `method` | wraps every `call()` |
| `plugin_call_error` | `plugin_id`, `error` | call returns Err |

Subscribers configure formatting via `tracing-subscriber` (e.g., the stdout-JSON sink that the coordinator binary ships in plan 02-06).
