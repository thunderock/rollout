//! Load the in-tree `PyO3` sample and exercise one call round-trip.
//!
//! Requires Python 3.11+ available to the linker (pyo3 abi3-py311). The
//! per-process Python init is implicit via `auto-initialize`.

use std::path::PathBuf;

use rollout_core::{EntrySpec, PluginHost, PluginKind, PluginManifest, PluginMode, RuntimeHints};
use rollout_plugin_host::PluginHostImpl;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn manifest() -> PluginManifest {
    PluginManifest {
        name: "sample-inproc".to_owned(),
        version: "0.1.0".to_owned(),
        kind: PluginKind::EnvHarness,
        trait_id: "rollout_core::Plugin".to_owned(),
        mode: PluginMode::Pyo3,
        runtime: RuntimeHints {
            python_min: Some("3.11".to_owned()),
            gpu: false,
            memory_mib: 64,
        },
        entry: EntrySpec::Pyo3 {
            module: "sample_inproc.plugin".to_owned(),
            factory: "create_plugin".to_owned(),
        },
        config_schema_path: None,
        network_allowlist: vec![],
    }
}

#[tokio::test]
#[ignore = "requires Python 3.11+ shared library at link time; smoke driver gates this"]
async fn pyo3_load_and_call_roundtrip() {
    let ws = workspace_root();
    let python_path = vec![ws.join("python/examples").display().to_string()];
    let host = PluginHostImpl::new().with_python_path(python_path);

    let handle = host.load(manifest()).await.expect("load pyo3");
    let out = host
        .call(&handle, "echo", b"hello".to_vec())
        .await
        .expect("call echo");
    assert_eq!(out, b"hello");
    let pong = host
        .call(&handle, "ping", Vec::new())
        .await
        .expect("call ping");
    assert_eq!(pong, b"pong");

    host.unload(handle).await.expect("unload");
}
