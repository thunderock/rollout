---
phase: 02-local-substrate
plan: 05
type: execute
wave: 4
depends_on: [02-00, 02-01, 02-02, 02-04]
files_modified:
  - crates/rollout-plugin-host/Cargo.toml
  - crates/rollout-plugin-host/src/lib.rs
  - crates/rollout-plugin-host/src/config.rs
  - crates/rollout-plugin-host/src/manifest.rs
  - crates/rollout-plugin-host/src/handle.rs
  - crates/rollout-plugin-host/src/host.rs
  - crates/rollout-plugin-host/src/modes/mod.rs
  - crates/rollout-plugin-host/src/modes/cdylib.rs
  - crates/rollout-plugin-host/src/modes/pyo3.rs
  - crates/rollout-plugin-host/src/modes/sidecar.rs
  - crates/rollout-plugin-host/src/modes/abi.rs
  - crates/rollout-plugin-host/tests/manifest.rs
  - crates/rollout-plugin-host/tests/cdylib_load.rs
  - crates/rollout-plugin-host/tests/pyo3_load.rs
  - crates/rollout-plugin-host/tests/sidecar_load.rs
  - crates/rollout-plugin-host/tests/reload_pyo3.rs
  - crates/rollout-plugin-host/tests/reload_sidecar.rs
  - crates/rollout-plugin-host/tests/reload_cdylib_unsupported.rs
  - crates/rollout-plugin-host/tests/storage_integration.rs
  - python/examples/sample_inproc/__init__.py
  - python/examples/sample_inproc/plugin.py
  - python/examples/sample_sidecar/__init__.py
  - python/examples/sample_sidecar/__main__.py
  - tests/smoke/plugins/rust_cdylib_sample/Cargo.toml
  - tests/smoke/plugins/rust_cdylib_sample/src/lib.rs
  - tests/smoke/plugins/rust_cdylib_sample/rollout-plugin.toml
  - tests/smoke/plugins/sample_inproc.toml
  - tests/smoke/plugins/sample_sidecar.toml
  - docs/book/src/substrate/plugin-host.md
  - docs/book/src/substrate/python-bridge.md
  - docs/book/src/SUMMARY.md
autonomous: true
requirements: [SUBSTR-03, DOCS-01, DOCS-02, DOCS-03]
must_haves:
  truths:
    - "rollout-plugin-host implements rollout_core::PluginHost with three modes: Rust cdylib (libloading), PyO3 in-process (dedicated Python OS thread), Python sidecar (stdlib framing over UDS — NOT tonic gRPC for the in-tree sample per AGENTS.md §7)."
    - "Plugin manifests (rollout-plugin.toml) parse via toml + validate via plan-time checks."
    - "Hot-reload works for PyO3 (importlib.reload) and sidecar (SIGTERM + respawn); cdylib reload returns Fatal(PluginContract) per spec 03 §7."
    - "Hot-reload is gated behind the `dev-hot-reload` Cargo feature."
    - "An in-tree Rust cdylib sample + in-tree Python in-process sample + in-tree Python sidecar sample all load and exchange one Call() round-trip."
    - "Plugin host does NOT depend on rollout-transport (dep-direction lint Wave 0 enforces this) — sidecar IPC uses UDS only."
  artifacts:
    - path: crates/rollout-plugin-host/src/host.rs
      provides: "Pyo3PluginHost / CdylibPluginHost / SidecarPluginHost OR a single PluginHostImpl with mode dispatch"
      contains: "impl PluginHost for"
    - path: crates/rollout-plugin-host/src/modes/abi.rs
      provides: "rollout-plugin-abi: versioned C-ABI shim for cdylib plugins (per RESEARCH Open Question 3: ship as internal module, NOT a separate crate, in Phase 2)"
      contains: "rollout_plugin_factory"
    - path: python/examples/sample_sidecar/__main__.py
      provides: "stdlib-only sidecar sample (length-prefixed JSON over UDS) per RESEARCH Pitfall 9"
      contains: "AF_UNIX"
    - path: tests/smoke/plugins/rust_cdylib_sample/src/lib.rs
      provides: "In-tree cdylib sample loaded by smoke test (plan 02-07)"
  key_links:
    - from: crates/rollout-plugin-host/src/modes/pyo3.rs
      to: "dedicated Python OS thread + tokio mpsc channel"
      via: "pyo3-async-runtimes::tokio"
      pattern: "prepare_freethreaded_python"
    - from: crates/rollout-plugin-host/src/modes/sidecar.rs
      to: "./data/sidecars/<plugin>-<pid>.sock"
      via: "tokio::net::UnixListener + 4-byte length-prefixed framing"
      pattern: "AF_UNIX|UnixListener"
---

<objective>
Implement `rollout-plugin-host` — the SUBSTR-03 deliverable. Per CONTEXT D-PLUGIN-01..04 + RESEARCH:

- **Three loaders ship wired:** Rust cdylib (libloading + internal `abi` module), PyO3 in-process (dedicated Python OS thread + `pyo3-async-runtimes 0.28`), Python sidecar (stdlib-only framing on UDS for the in-tree sample, per RESEARCH Pitfall 9).
- **Hot reload** behind `dev-hot-reload` feature: PyO3 via `importlib.reload`, sidecar via SIGTERM + respawn, cdylib returns `Fatal(PluginContract)` per spec 03 §7.
- **Manifest parsing** for `rollout-plugin.toml` per RESEARCH §"Plugin manifest TOML".
- **Three in-tree samples** committed under `python/examples/` and `tests/smoke/plugins/` so the smoke test (plan 02-07) has something to load.

Purpose: This is the riskiest Phase-2 crate (PyO3 + Tokio entanglement per ROADMAP risk callout). Pinning `pyo3 = 0.28` + `pyo3-async-runtimes = 0.28` (workspace pin from plan 02-00) is load-bearing.

Output: All seven `cargo test -p rollout-plugin-host --test *` files green; Python in-tree samples runnable via `python -m sample_sidecar` and `python -c 'from sample_inproc import plugin'` with stdlib only.
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
@$HOME/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/STATE.md
@.planning/phases/02-local-substrate/02-CONTEXT.md
@.planning/phases/02-local-substrate/02-RESEARCH.md
@.planning/phases/02-local-substrate/02-VALIDATION.md
@.planning/phases/02-local-substrate/02-00-wave0-trait-extensions-PLAN.md
@.planning/phases/02-local-substrate/02-01-rollout-proto-PLAN.md
@.planning/phases/02-local-substrate/02-02-rollout-storage-PLAN.md
@docs/specs/03-plugin-system.md
@crates/rollout-core/src/traits/plugin.rs
@crates/rollout-core/src/lib.rs
@Cargo.toml

<interfaces>
Trait surface (post-Wave-0):
```rust
pub struct PluginManifest { name, version, kind: PluginKind, trait_id: String, mode: PluginMode, runtime: RuntimeHints, entry: EntrySpec, config_schema_path: Option<String>, network_allowlist: Vec<String> }
pub enum PluginMode { Pyo3, Sidecar, RustCdylib }
pub enum EntrySpec { Cdylib { path, symbol }, Pyo3 { module, factory }, Sidecar { command: Vec<String>, protocol, socket_template } }
pub enum SidecarProtocol { GrpcUds, FramedJsonUds }   // Phase 2 uses FramedJsonUds for in-tree sample
pub struct PluginHandle { pub id: PluginId, pub manifest: PluginManifest /* impl-specific state behind enum */ }
pub struct PluginId(pub String);
pub struct PluginDependencies; // unit struct in Phase 2; fleshed out later

#[async_trait] pub trait PluginHost: Send + Sync {
    async fn load(&self, manifest: PluginManifest) -> Result<PluginHandle, CoreError>;
    async fn call(&self, handle: &PluginHandle, method: &str, payload: Vec<u8>) -> Result<Vec<u8>, CoreError>;
    async fn reload(&self, handle: &PluginHandle, reason: &str) -> Result<(), CoreError>;
    async fn unload(&self, handle: PluginHandle) -> Result<(), CoreError>;
}
```

Versions (workspace pins from plan 02-00):
- pyo3 = 0.28 (features: auto-initialize, abi3-py311)
- pyo3-async-runtimes = 0.28 (features: tokio-runtime)
- libloading = 0.8
- tokio = workspace
- toml = 0.8

C-ABI shim shape (Phase 2 = internal module per RESEARCH Open Question 3):
```rust
// crates/rollout-plugin-host/src/modes/abi.rs
#[no_mangle]
pub extern "C" fn rollout_plugin_abi_version() -> u32 { 1 }

// Plugin authors export:
// #[no_mangle] pub extern "C" fn rollout_plugin_factory() -> *mut RolloutPluginVtable;
#[repr(C)]
pub struct RolloutPluginVtable {
    pub abi_version: u32,        // must equal rollout_plugin_abi_version()
    pub name: *const std::os::raw::c_char,
    pub call: extern "C" fn(method: *const u8, method_len: usize, payload: *const u8, payload_len: usize, out: *mut Buf) -> i32,
    pub free_buf: extern "C" fn(buf: Buf),
}
#[repr(C)] pub struct Buf { pub ptr: *mut u8, pub len: usize, pub cap: usize }
```
</interfaces>
</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: Crate scaffolding + manifest parsing + cdylib loader + in-tree Rust sample + reload-cdylib-unsupported</name>
  <files>
    crates/rollout-plugin-host/Cargo.toml,
    crates/rollout-plugin-host/src/lib.rs,
    crates/rollout-plugin-host/src/config.rs,
    crates/rollout-plugin-host/src/manifest.rs,
    crates/rollout-plugin-host/src/handle.rs,
    crates/rollout-plugin-host/src/modes/mod.rs,
    crates/rollout-plugin-host/src/modes/cdylib.rs,
    crates/rollout-plugin-host/src/modes/abi.rs,
    crates/rollout-plugin-host/src/host.rs,
    crates/rollout-plugin-host/tests/manifest.rs,
    crates/rollout-plugin-host/tests/cdylib_load.rs,
    crates/rollout-plugin-host/tests/reload_cdylib_unsupported.rs,
    tests/smoke/plugins/rust_cdylib_sample/Cargo.toml,
    tests/smoke/plugins/rust_cdylib_sample/src/lib.rs,
    tests/smoke/plugins/rust_cdylib_sample/rollout-plugin.toml
  </files>
  <read_first>
    - crates/rollout-plugin-host/Cargo.toml (Wave-0 stub)
    - crates/rollout-plugin-host/src/lib.rs (Wave-0 stub)
    - crates/rollout-core/src/traits/plugin.rs (post-Wave-0 surface)
    - docs/specs/03-plugin-system.md §2 (manifest), §3.1 (cdylib mode), §7 (hot reload — cdylib unsupported)
    - .planning/phases/02-local-substrate/02-RESEARCH.md §"Plugin manifest TOML" + §"Pitfall 4: cdylib hot reload appears to work" + §"Anti-Patterns / async fn inside libloading cdylib boundary"
    - .planning/phases/02-local-substrate/02-CONTEXT.md D-PLUGIN-01..04
  </read_first>
  <behavior>
    RED first:

    `tests/manifest.rs`:
    - `parse_pyo3_manifest_succeeds`: TOML string for a pyo3 plugin parses into PluginManifest; mode == Pyo3; entry == EntrySpec::Pyo3 {...}.
    - `parse_sidecar_manifest_succeeds`: TOML for a sidecar plugin parses; entry == EntrySpec::Sidecar.
    - `parse_cdylib_manifest_succeeds`: TOML for a cdylib plugin parses; entry == EntrySpec::Cdylib.
    - `manifest_validation_rejects_unknown_kind`: kind="unknown-kind" returns Err(Fatal(ConfigInvalid)).
    - `manifest_validation_requires_python_min_for_pyo3`: pyo3 manifest without python_min returns Err.
    - `manifest_validation_rejects_invalid_python_version`: python_min="2.7" rejected (must be 3.11+).

    `tests/cdylib_load.rs` (`#[ignore]` if the sample isn't pre-built; `make` is responsibility of plan 02-07; if the sample crate is built as a workspace dev-dependency it'll be in target/ automatically):
    - `cdylib_load_and_call_roundtrip`: build the in-tree `rust_cdylib_sample` (use `escargot::CargoBuild` OR a manual `cargo build -p rust-cdylib-sample` invocation in the test setup); load via the manifest; call("echo", b"hi"); receive Ok(b"hi"). Mark this `#[ignore]` if dependent on out-of-band build; otherwise run inline.

    `tests/reload_cdylib_unsupported.rs`:
    - `reload_cdylib_returns_fatal_plugin_contract`: load a (mocked) cdylib handle; reload returns `Err(Fatal(PluginContract("cdylib reload unsupported")))`.

    GREEN: implement modules.
  </behavior>
  <action>
    **Step 1 — `crates/rollout-plugin-host/Cargo.toml`:**
    ```toml
    [package]
    name = "rollout-plugin-host"
    version.workspace = true
    edition.workspace = true
    license.workspace = true
    rust-version.workspace = true
    repository.workspace = true

    [lints]
    workspace = true

    [features]
    default = ["cdylib", "pyo3", "sidecar"]
    cdylib = ["dep:libloading"]
    pyo3 = ["dep:pyo3", "dep:pyo3-async-runtimes"]
    sidecar = []
    dev-hot-reload = []

    [dependencies]
    rollout-core    = { path = "../rollout-core" }
    rollout-storage = { path = "../rollout-storage" }
    async-trait     = { workspace = true }
    serde           = { workspace = true }
    serde_json      = { workspace = true }
    schemars        = { workspace = true }
    thiserror       = { workspace = true }
    tracing         = { workspace = true }
    tokio           = { workspace = true }
    smol_str        = { workspace = true }
    toml            = { workspace = true }
    libloading           = { workspace = true, optional = true }
    pyo3                 = { workspace = true, optional = true }
    pyo3-async-runtimes  = { workspace = true, optional = true }

    [dev-dependencies]
    tempfile = { workspace = true }
    tokio = { workspace = true, features = ["macros", "rt-multi-thread", "process"] }
    ```
    NOTE: `rollout-plugin-host` does NOT depend on `rollout-transport` — dep-direction lint from Wave 0 forbids it (sidecar IPC uses UDS framing, NOT the QUIC/H2 transport).

    **Step 2 — `src/manifest.rs`** — `pub fn parse_manifest(path)` + `pub fn validate_manifest(&PluginManifest)`. Use `toml::from_str::<PluginManifest>`. The `PluginManifest` struct lives in `rollout-core` (Wave 0) but parsing logic lives here. Add serde rename_all = "kebab-case" if the TOML uses kebab.

    NOTE: PluginManifest in `rollout-core` (Wave 0) must derive `Deserialize`. Verify in Wave 0; if not, this plan adds the derive in `rollout-core` as a small Wave-0-extension nit (alternative: wrap with a local `ManifestWire` shape and convert).

    **Step 3 — `src/handle.rs`** — Define an enum that wraps mode-specific state:
    ```rust
    pub enum HandleState {
        Cdylib(cdylib_state::CdylibState),
        #[cfg(feature = "pyo3")] Pyo3(pyo3_state::Pyo3State),
        Sidecar(sidecar_state::SidecarState),
    }
    ```
    `PluginHandle` (in `rollout-core`) has `pub id: PluginId, pub manifest: PluginManifest` — the mode-specific state lives in a parallel `HashMap<PluginId, HandleState>` inside the host (not on the public PluginHandle, to keep `PluginHandle` POD + Send/Sync without unsafe).

    **Step 4 — `src/modes/abi.rs`** — the C-ABI shim per `<interfaces>` block. `#[forbid(unsafe_code)]` workspace lint blocks `unsafe`, so we add `#![allow(unsafe_code)]` to THIS file ONLY with a comment explaining why (FFI is unavoidable; per AGENTS.md §1 unsafe_code is forbid at workspace; this is the documented exception for the cdylib boundary; consider adding `unsafe_code = "deny"` workspace-wide and explicit `#[allow]` here — verify the workspace lint table; if `forbid` is used, downgrade to `deny` in `crates/rollout-plugin-host/Cargo.toml [lints.rust] unsafe_code = "deny"` so file-level `#[allow]` works).

    Actually simpler: per workspace `Cargo.toml` line 18, `unsafe_code = "forbid"` is set; `forbid` cannot be relaxed at the file level. Override in this crate's `[lints]`:
    ```toml
    [lints.rust]
    unsafe_code = { level = "deny", priority = 1 }
    missing_docs = "warn"
    [lints.clippy]
    all = { level = "warn", priority = -1 }
    pedantic = { level = "warn", priority = -1 }
    ```
    Then `#[allow(unsafe_code)]` at the boundary file (abi.rs + cdylib.rs).

    Document this clearly in the crate `//!` doc: "This crate downgrades the workspace `unsafe_code` lint from `forbid` to `deny` because the C-ABI cdylib boundary is unavoidably unsafe (libloading::Symbol::get returns an unsafe pointer-cast). All `unsafe` blocks are confined to `src/modes/cdylib.rs` and `src/modes/abi.rs` and have safety comments."

    **Step 5 — `src/modes/cdylib.rs`:**
    ```rust
    #![allow(unsafe_code)]
    use libloading::{Library, Symbol};
    use std::os::raw::c_char;
    use std::sync::Arc;
    use crate::modes::abi::{RolloutPluginVtable, Buf};

    /// SAFETY: we keep `Library` alive for the lifetime of the handle; never dlclose
    /// (cdylib reload is unsupported per spec 03 §7).
    pub struct CdylibState {
        _lib: Arc<Library>,     // keep alive
        vtable: *const RolloutPluginVtable,
    }
    unsafe impl Send for CdylibState {}
    unsafe impl Sync for CdylibState {}

    impl CdylibState {
        pub fn load(path: &std::path::Path, symbol: &str) -> Result<Self, rollout_core::CoreError> {
            // SAFETY: caller has validated path; Library::new only returns errors,
            // not undefined behavior.
            let lib = unsafe { Library::new(path) }
                .map_err(|e| internal(format!("cdylib load: {e}")))?;
            let lib = Arc::new(lib);
            // SAFETY: symbol returns a function pointer; we cast to vtable factory.
            let vtable: *const RolloutPluginVtable = unsafe {
                let sym: Symbol<unsafe extern "C" fn() -> *mut RolloutPluginVtable> =
                    lib.get(symbol.as_bytes()).map_err(|e| internal(format!("cdylib symbol: {e}")))?;
                sym() as *const _
            };
            if vtable.is_null() {
                return Err(internal("cdylib factory returned null"));
            }
            // SAFETY: vtable is non-null by check above.
            let abi = unsafe { (*vtable).abi_version };
            if abi != crate::modes::abi::ABI_VERSION {
                return Err(internal(format!("cdylib ABI mismatch: got {abi}, expected {}", crate::modes::abi::ABI_VERSION)));
            }
            Ok(Self { _lib: lib, vtable })
        }

        pub fn call(&self, method: &str, payload: &[u8]) -> Result<Vec<u8>, rollout_core::CoreError> {
            let vt = self.vtable;
            let mut out = Buf { ptr: std::ptr::null_mut(), len: 0, cap: 0 };
            // SAFETY: vtable is alive while Library is alive; we hold an Arc.
            let rc = unsafe {
                ((*vt).call)(method.as_ptr(), method.len(), payload.as_ptr(), payload.len(), &mut out)
            };
            if rc != 0 {
                return Err(internal(format!("cdylib call returned non-zero rc={rc}")));
            }
            // SAFETY: vtable promises out.ptr/len/cap are a valid Vec layout if rc==0.
            let v = unsafe { Vec::from_raw_parts(out.ptr, out.len, out.cap) };
            Ok(v)
        }
    }
    fn internal(s: impl Into<String>) -> rollout_core::CoreError {
        rollout_core::CoreError::Fatal(rollout_core::FatalError::Internal(s.into()))
    }
    ```

    **Step 6 — `src/host.rs`** — `PluginHostImpl { handles: Mutex<HashMap<PluginId, HandleState>> }`; `impl PluginHost for PluginHostImpl` dispatches on `manifest.mode`. For Phase 2 + this task, only the cdylib branch fully works; pyo3 + sidecar are stubs returning `Fatal(Internal("not yet wired"))` until Task 2.

    `reload(&handle, reason)`:
    - If `handle.manifest.mode == RustCdylib` → return `Fatal(PluginContract("cdylib reload unsupported per spec 03 §7"))`.
    - else → for Task 1 also returns "not yet wired"; Task 2 wires PyO3 + sidecar reload behind the `dev-hot-reload` feature.

    **Step 7 — In-tree Rust cdylib sample at `tests/smoke/plugins/rust_cdylib_sample/`:**

    `Cargo.toml`:
    ```toml
    [package]
    name = "rust-cdylib-sample"
    version = "0.1.0"
    edition = "2021"
    publish = false

    [lib]
    crate-type = ["cdylib"]

    [dependencies]
    # No rollout-core dep — keep the sample self-contained. The ABI shim shape is
    # documented in docs/book/src/substrate/plugin-host.md; users copy the small
    # #[repr(C)] struct definitions into their own plugin crate.
    ```
    Note: this crate is NOT a workspace member (avoids cluttering the main workspace; plan 02-07 smoke builds it via `cargo build --manifest-path tests/smoke/plugins/rust_cdylib_sample/Cargo.toml --release`).

    `src/lib.rs`:
    ```rust
    //! In-tree Rust cdylib plugin sample. Implements ABI v1 with one method: echo.
    use std::os::raw::c_char;

    #[repr(C)] pub struct Buf { pub ptr: *mut u8, pub len: usize, pub cap: usize }
    #[repr(C)]
    pub struct RolloutPluginVtable {
        pub abi_version: u32,
        pub name: *const c_char,
        pub call: extern "C" fn(method: *const u8, method_len: usize, payload: *const u8, payload_len: usize, out: *mut Buf) -> i32,
        pub free_buf: extern "C" fn(buf: Buf),
    }

    static NAME: &[u8] = b"rust-cdylib-sample\0";

    extern "C" fn sample_call(method: *const u8, method_len: usize, payload: *const u8, payload_len: usize, out: *mut Buf) -> i32 {
        // SAFETY: caller promises pointers point to method_len / payload_len readable bytes.
        let method = unsafe { std::slice::from_raw_parts(method, method_len) };
        let payload = unsafe { std::slice::from_raw_parts(payload, payload_len) };
        let result_bytes: Vec<u8> = match method {
            b"echo" => payload.to_vec(),
            _ => return 1,
        };
        let mut v = result_bytes;
        let (ptr, len, cap) = (v.as_mut_ptr(), v.len(), v.capacity());
        std::mem::forget(v);
        // SAFETY: caller passed a writable *mut Buf.
        unsafe { *out = Buf { ptr, len, cap }; }
        0
    }

    extern "C" fn sample_free(buf: Buf) {
        // SAFETY: matched pair with sample_call's Vec::into_raw_parts.
        if !buf.ptr.is_null() { unsafe { Vec::from_raw_parts(buf.ptr, buf.len, buf.cap); } }
    }

    static VTABLE: RolloutPluginVtable = RolloutPluginVtable {
        abi_version: 1,
        name: NAME.as_ptr() as *const c_char,
        call: sample_call,
        free_buf: sample_free,
    };

    #[no_mangle]
    pub extern "C" fn rollout_plugin_factory() -> *mut RolloutPluginVtable {
        &VTABLE as *const _ as *mut _
    }
    ```

    `rollout-plugin.toml` (alongside the sample crate):
    ```toml
    [plugin]
    name    = "rust-cdylib-sample"
    version = "0.1.0"
    kind    = "custom"
    trait   = "rollout_core::Plugin"
    mode    = "rust-cdylib"

    [runtime]
    gpu        = false
    memory_mib = 32

    [entry]
    cdylib = "../../../target/release/librust_cdylib_sample.dylib"   # macOS; smoke handles .so / .dylib
    symbol = "rollout_plugin_factory"
    ```

    **Step 8 — `src/lib.rs`:** crate-level `//!` doc explaining the `unsafe_code` downgrade; re-export `pub use host::PluginHostImpl;`.

    **Step 9 — RED tests** per `<behavior>`.
  </action>
  <verify>
    <automated>cargo build -p rollout-plugin-host &amp;&amp; cargo test -p rollout-plugin-host --test manifest &amp;&amp; cargo test -p rollout-plugin-host --test reload_cdylib_unsupported &amp;&amp; cargo clippy -p rollout-plugin-host --all-targets -- -D warnings</automated>
  </verify>
  <acceptance_criteria>
    - `crates/rollout-plugin-host/Cargo.toml` `[features]` declares default = ["cdylib", "pyo3", "sidecar"] + dev-hot-reload
    - `crates/rollout-plugin-host/src/modes/cdylib.rs` exists and compiles
    - `crates/rollout-plugin-host/src/modes/abi.rs` declares `RolloutPluginVtable` and `ABI_VERSION = 1`
    - `crates/rollout-plugin-host/src/host.rs` contains `impl PluginHost for PluginHostImpl`
    - `tests/smoke/plugins/rust_cdylib_sample/src/lib.rs` exports `rollout_plugin_factory`
    - `cargo test -p rollout-plugin-host --test manifest` exits 0 (6 tests pass)
    - `cargo test -p rollout-plugin-host --test reload_cdylib_unsupported` exits 0
    - `cargo build --manifest-path tests/smoke/plugins/rust_cdylib_sample/Cargo.toml --release` exits 0
    - `cargo test -p rollout-plugin-host --test cdylib_load -- --ignored` (or unignored) succeeds when the sample is pre-built; passing this test in CI is plan 02-07's responsibility (the smoke job builds the sample first)
    - `cargo clippy -p rollout-plugin-host --all-targets -- -D warnings` exits 0
    - Per-crate `[lints]` table downgrades `unsafe_code` from forbid to deny with explanatory crate-level doc
    - DOCS-02: tests + inline docs + crate-level rationale all in the commit
  </acceptance_criteria>
  <done>
    Manifest parsing works; cdylib loader exists and dispatches Call via the C-ABI vtable; cdylib reload returns the documented Fatal error; in-tree Rust sample compiles to a cdylib.
  </done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: PyO3 in-process loader + sidecar (stdlib UDS framing) loader + hot-reload + Python samples + Storage integration + mdBook chapters</name>
  <files>
    crates/rollout-plugin-host/src/modes/pyo3.rs,
    crates/rollout-plugin-host/src/modes/sidecar.rs,
    crates/rollout-plugin-host/tests/pyo3_load.rs,
    crates/rollout-plugin-host/tests/sidecar_load.rs,
    crates/rollout-plugin-host/tests/reload_pyo3.rs,
    crates/rollout-plugin-host/tests/reload_sidecar.rs,
    crates/rollout-plugin-host/tests/storage_integration.rs,
    python/examples/sample_inproc/__init__.py,
    python/examples/sample_inproc/plugin.py,
    python/examples/sample_sidecar/__init__.py,
    python/examples/sample_sidecar/__main__.py,
    tests/smoke/plugins/sample_inproc.toml,
    tests/smoke/plugins/sample_sidecar.toml,
    docs/book/src/substrate/plugin-host.md,
    docs/book/src/substrate/python-bridge.md,
    docs/book/src/SUMMARY.md
  </files>
  <read_first>
    - crates/rollout-plugin-host/src/host.rs (Task 1 output)
    - crates/rollout-plugin-host/src/handle.rs (Task 1 output)
    - .planning/phases/02-local-substrate/02-RESEARCH.md §"Pattern 3: PyO3 ↔ Tokio bridge with dedicated OS thread" — authoritative pattern
    - .planning/phases/02-local-substrate/02-RESEARCH.md §"Pattern 4: tonic over UDS for sidecar IPC" — for tonic-UDS (NOTE: in-tree sample uses stdlib framing, not tonic — see Pitfall 9)
    - .planning/phases/02-local-substrate/02-RESEARCH.md §"Python sidecar IPC: avoid pip" — stdlib framing sample
    - .planning/phases/02-local-substrate/02-RESEARCH.md §"Pitfall 3: PyO3 + Tokio runtime entanglement"
    - .planning/phases/02-local-substrate/02-CONTEXT.md D-PLUGIN-02, D-PLUGIN-03, D-PLUGIN-04, D-SANDBOX-01
    - docs/specs/03-plugin-system.md §3.2 + §3.3 + §7 (hot reload)
    - crates/rollout-storage/src/lib.rs (Storage for integration test)
  </read_first>
  <behavior>
    RED first:

    `tests/pyo3_load.rs`:
    - `pyo3_load_and_call_roundtrip`: ship `python/examples/sample_inproc/plugin.py` with a `create_plugin()` factory that returns an object with a `call(method, payload)` method; load via manifest; call("echo", b"hi"); receive b"hi".
    - `pyo3_runs_on_dedicated_thread`: assert that calls execute on a non-Tokio thread (use `tokio::task::yield_now()` race patterns or check `std::thread::current().name() == Some("rollout-py-...")`).
    - May `#[ignore]` if `python3 < 3.11` not available; emit a clear skip message.

    `tests/sidecar_load.rs`:
    - `sidecar_spawn_call_shutdown`: load the in-tree sample_sidecar manifest; spawn the child; send a Call("Init", {}); receive InitResponse; send Shutdown; verify child exits cleanly within 5s.
    - Uses tokio::process::Command + stdlib framing over a tokio::net::UnixStream.

    `tests/reload_pyo3.rs` (`#[cfg(feature = "dev-hot-reload")]`):
    - `reload_pyo3_invokes_importlib`: load plugin; modify the .py file on disk; call reload; subsequent call returns the new behavior.

    `tests/reload_sidecar.rs` (`#[cfg(feature = "dev-hot-reload")]`):
    - `reload_sidecar_sigterm_respawns`: load sidecar; capture child PID; call reload; assert old PID exits (SIGTERM) and new child has different PID; subsequent call succeeds.

    `tests/storage_integration.rs`:
    - `host_persists_manifest_to_storage`: PluginHostImpl with a real EmbeddedStorage in tempdir; load → writes manifest bytes to namespace "plugins" with key path [plugin.name]; reload of host (new instance) reads them back via list_loaded_manifests() or similar.

    GREEN: implement.
  </behavior>
  <action>
    **Step 1 — `src/modes/pyo3.rs`** per RESEARCH Pattern 3:
    - Dedicated OS thread per worker (one PluginHostImpl typically wraps one worker → one Python thread is correct for Phase 2).
    - The thread runs `pyo3::prepare_freethreaded_python()` once at startup.
    - A `tokio::sync::mpsc::channel<PyTask>` lets async callers hop to the Python thread.
    - `PyTask::Call { module, method, args_bytes, reply: oneshot::Sender<Result<Vec<u8>, CoreError>> }`.
    - The Python factory contract: `module.create_plugin()` returns an object; `obj.call(method: str, payload: bytes) -> bytes`.
    - For hot reload (behind `#[cfg(feature = "dev-hot-reload")]`): `PyTask::Reload { module }` executes `importlib.reload(<module>)` and re-creates the plugin instance.

    **Step 2 — `src/modes/sidecar.rs`:**
    - Manage child process via `tokio::process::Command`; capture stdout/stderr to log files.
    - Generate UDS path `./data/sidecars/<plugin_name>-<pid>.sock`; pass it as the FIRST argument to the child command per the sample.
    - The HOST side uses stdlib-equivalent framing in Rust: write `[u32 BE length][payload bytes]`, read same. Use `tokio::net::UnixStream`.
    - Connect with a 5-second timeout + 100ms retry loop because the child needs time to `bind()`.
    - For hot reload: `child.kill()` (which sends SIGKILL — switch to using the `nix` crate to send SIGTERM, OR write a small unsafe wrapper around libc::kill; OR add a tokio::process feature. Simplest: use `child.start_kill()` for SIGKILL on the existing handle, or — preferred — `unsafe { libc::kill(child.id() as i32, libc::SIGTERM) }` — but that requires the `libc` dep. **Decision: add `libc = "0.2"` as a workspace pin in plan 02-00; if missing at exec time, add it via a new commit before this task.**

    Actually simplest: use `nix = "0.30"` as a small dep just for `nix::sys::signal::{kill, Signal}`. Adding a dep in this plan is allowed; deny.toml should not complain (nix is MIT). Add `nix = { version = "0.30", features = ["signal"] }` to the crate's `[dependencies]`.

    On respawn: SIGTERM the child; wait_with_timeout(2s); if still alive, SIGKILL; spawn a fresh child with the same command line; reconnect to a new UDS path.

    **Step 3 — `python/examples/sample_inproc/`:**
    - `__init__.py`: `from . import plugin`.
    - `plugin.py`:
      ```python
      """In-process PyO3 plugin sample. Stdlib only."""

      class _Plugin:
          def call(self, method: str, payload: bytes) -> bytes:
              if method == "echo": return payload
              raise ValueError(f"unknown method: {method!r}")

      def create_plugin():
          return _Plugin()
      ```

    **Step 4 — `python/examples/sample_sidecar/`:**
    - `__init__.py`: `"""Sidecar gRPC stubs (placeholder)."""` (one line).
    - `__main__.py`: VERBATIM from RESEARCH §"Python sidecar IPC: avoid pip". Stdlib-only `socket` + `struct` framing.

    **Step 5 — `tests/smoke/plugins/sample_inproc.toml` and `sample_sidecar.toml`:** TOML manifests pointing at the Python paths.

    **Step 6 — RED tests** per `<behavior>`. If Python integration tests prove flaky on macOS (PyO3 sometimes does), mark `pyo3_load_and_call_roundtrip` with `#[ignore]` and document the skip path — the smoke test (plan 02-07) will validate the cross-crate flow.

    **Step 7 — Wire Storage integration into PluginHostImpl:** add a constructor `PluginHostImpl::with_storage(storage: Arc<dyn Storage>)` and persist manifests under `plugins/<name>`.

    **Step 8 — `docs/book/src/substrate/plugin-host.md`** (NEW, ~150 lines):
    - **Three modes** and their tradeoffs (perf vs isolation).
    - **Manifest schema** — link to spec 03 §2.
    - **Hot reload semantics** — gated behind `dev-hot-reload`; cdylib unsupported (spec 03 §7).
    - **C-ABI shim** — version 1 vtable; this is the contract for cdylib authors; the shim lives as an internal module in Phase 2 and graduates to a separate `rollout-plugin-abi` crate later (RESEARCH OQ 3).
    - **Sidecar IPC** — stdlib framing for the in-tree sample (RESEARCH Pitfall 9 + AGENTS.md §7); users may opt into `grpclib`.
    - **Sandboxing today** — network allowlist only (D-SANDBOX-01); full sandbox lands in Phase 7.
    - **Dep-direction lint** — plugin-host does NOT depend on transport.

    **Step 9 — `docs/book/src/substrate/python-bridge.md`** (NEW, ~80 lines):
    - **pyo3 0.28 + pyo3-async-runtimes 0.28 pin rationale** (RESEARCH §"Code Examples / Pattern 3" + §"Standard Stack").
    - **Dedicated Python OS thread** — why this isolation matters (RESEARCH Pitfall 3).
    - **abi3-py311** — single-wheel strategy; minimum Python 3.11 (stdlib tomllib for manifest parsing on the Python side if needed).
    - **`pip install` policy** — in-tree samples use stdlib only; user plugins are free to bring their own venv.

    **Step 10 — `docs/book/src/SUMMARY.md`:**
    ```markdown
    - [Substrate](./substrate/index.md)
      - [Proto crate](./substrate/proto.md)
      - [Storage](./substrate/storage.md)
      - [Cloud-local](./substrate/cloud-local.md)
      - [Transport](./substrate/transport.md)
      - [Plugin host](./substrate/plugin-host.md)
      - [Python bridge](./substrate/python-bridge.md)
    ```
  </action>
  <verify>
    <automated>cargo test -p rollout-plugin-host --tests --features "cdylib pyo3 sidecar" &amp;&amp; cargo test -p rollout-plugin-host --tests --features "cdylib pyo3 sidecar dev-hot-reload" &amp;&amp; python3 -m sample_inproc.plugin 2>/dev/null || python3 -c "import sys; sys.path.insert(0,'python/examples'); from sample_inproc import plugin; print(plugin.create_plugin().call('echo', b'hi'))" &amp;&amp; mdbook build docs/book</automated>
  </verify>
  <acceptance_criteria>
    - `crates/rollout-plugin-host/src/modes/pyo3.rs` contains `pub struct Pyo3State` and `prepare_freethreaded_python`
    - `crates/rollout-plugin-host/src/modes/sidecar.rs` contains UDS framing with 4-byte BE length prefix
    - `python/examples/sample_sidecar/__main__.py` works with stdlib only (no `import grpc`)
    - `python/examples/sample_inproc/plugin.py` defines `create_plugin()` returning an object with `.call(method, payload)`
    - `cargo test -p rollout-plugin-host --test sidecar_load` exits 0 (may need python3 ≥ 3.11)
    - `cargo test -p rollout-plugin-host --test pyo3_load` exits 0 (or `#[ignore]` cleanly with documented skip path; smoke test gates the real verification)
    - `cargo test -p rollout-plugin-host --test storage_integration` exits 0
    - `cargo test -p rollout-plugin-host --test reload_pyo3 --features dev-hot-reload` exits 0 (or `#[ignore]` documented)
    - `cargo test -p rollout-plugin-host --test reload_sidecar --features dev-hot-reload` exits 0
    - `docs/book/src/substrate/plugin-host.md` and `docs/book/src/substrate/python-bridge.md` exist; `mdbook build docs/book` exits 0
    - `docs/book/src/SUMMARY.md` lists plugin-host + python-bridge chapters
    - DOCS-02 satisfied: each test + chapter touched in the commit
  </acceptance_criteria>
  <done>
    SUBSTR-03 satisfied: all three loader modes wired; hot-reload working under `dev-hot-reload`; in-tree samples runnable with stdlib-only Python; manifest persisted via Storage; substrate plugin-host + python-bridge mdBook chapters ship.
  </done>
</task>

</tasks>

<verification>
```bash
cargo build -p rollout-plugin-host
cargo test -p rollout-plugin-host --tests
cargo test -p rollout-plugin-host --tests --features dev-hot-reload
cargo clippy -p rollout-plugin-host --all-targets --all-features -- -D warnings
cargo doc -p rollout-plugin-host --no-deps --all-features
cargo build --manifest-path tests/smoke/plugins/rust_cdylib_sample/Cargo.toml --release
python3 -c "import sys; sys.path.insert(0,'python/examples'); from sample_inproc import plugin; print(plugin.create_plugin().call('echo', b'hi'))"
python3 -c "import sys; sys.path.insert(0,'python/examples'); import sample_sidecar.__main__"
mdbook build docs/book
```
All exit 0.
</verification>

<success_criteria>
- SUBSTR-03 satisfied
- Three loader modes (cdylib / pyo3 / sidecar) work
- Hot reload behind `dev-hot-reload` feature for PyO3 + sidecar; cdylib reload Fatal by design
- In-tree samples use stdlib only (no `pip install` in the cargo-test path)
- plugin-host crate does NOT depend on rollout-transport (dep-direction lint passes)
- Substrate plugin-host + python-bridge chapters published
</success_criteria>

<output>
After completion, create `.planning/phases/02-local-substrate/02-05-rollout-plugin-host-SUMMARY.md` documenting:
- Final mode dispatch architecture (single PluginHostImpl vs three structs)
- C-ABI version chosen (likely 1)
- PyO3 thread-naming convention
- Sidecar respawn signal choice (SIGTERM vs nix vs libc raw)
- Hot-reload tests skipped vs running
- Decisions under "Claude's Discretion" (PluginDependencies shape, search-path precedence, etc.)
- Open questions for plan 02-07 (smoke test) — particularly how the smoke script builds the cdylib sample
</output>
