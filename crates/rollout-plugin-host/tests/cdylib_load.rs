//! cdylib load + call round-trip against the in-tree sample.
//!
//! Ignored by default: requires the sample to be pre-built via
//! `cargo build --manifest-path tests/smoke/plugins/rust_cdylib_sample/Cargo.toml --release`.
//! Plan 02-07 (smoke) wires the build into the smoke test driver.

use rollout_core::{EntrySpec, PluginHost, PluginKind, PluginManifest, PluginMode, RuntimeHints};
use rollout_plugin_host::PluginHostImpl;
use std::path::PathBuf;

fn sample_dylib_path() -> PathBuf {
    // The sample lives outside the main workspace and emits into its own
    // target/ dir (see tests/smoke/plugins/rust_cdylib_sample/Cargo.toml).
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let target = PathBuf::from(manifest_dir)
        .join("..")
        .join("..")
        .join("tests/smoke/plugins/rust_cdylib_sample/target/release");
    if cfg!(target_os = "macos") {
        target.join("librust_cdylib_sample.dylib")
    } else if cfg!(target_os = "linux") {
        target.join("librust_cdylib_sample.so")
    } else {
        target.join("rust_cdylib_sample.dll")
    }
}

#[tokio::test]
#[ignore = "requires pre-built rust_cdylib_sample; wired by plan 02-07 smoke driver"]
async fn cdylib_load_and_call_roundtrip() {
    let dylib = sample_dylib_path();
    assert!(
        dylib.exists(),
        "sample dylib not found at {}; build with `cargo build --manifest-path tests/smoke/plugins/rust_cdylib_sample/Cargo.toml --release`",
        dylib.display()
    );

    let host = PluginHostImpl::new();
    let manifest = PluginManifest {
        name: "rust-cdylib-sample".to_owned(),
        version: "0.1.0".to_owned(),
        kind: PluginKind::Custom("test".to_owned()),
        trait_id: "rollout_core::Plugin".to_owned(),
        mode: PluginMode::RustCdylib,
        runtime: RuntimeHints {
            python_min: None,
            gpu: false,
            memory_mib: 32,
        },
        entry: EntrySpec::Cdylib {
            path: dylib.to_string_lossy().into_owned(),
            symbol: "rollout_plugin_factory".to_owned(),
        },
        config_schema_path: None,
        network_allowlist: vec![],
    };
    let handle = host.load(manifest).await.expect("load");
    let out = host
        .call(&handle, "echo", b"hello".to_vec())
        .await
        .expect("call");
    assert_eq!(out, b"hello");
    host.unload(handle).await.expect("unload");
}
