//! Spawn the in-tree Python sidecar sample and exchange one Call round-trip.
//!
//! Requires `python3 >= 3.11` on PATH (preflight gates this; skip cleanly if
//! python3 isn't available).

use std::path::PathBuf;
use std::process::Command;

use rollout_core::{
    EntrySpec, PluginHost, PluginKind, PluginManifest, PluginMode, RuntimeHints, SidecarProtocol,
};
use rollout_plugin_host::PluginHostImpl;

fn python3_available() -> bool {
    Command::new("python3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

#[tokio::test]
async fn sidecar_spawn_call_shutdown() {
    if !python3_available() {
        eprintln!("python3 not available; skipping sidecar_spawn_call_shutdown");
        return;
    }
    let ws = workspace_root();
    let python_examples = ws.join("python/examples");
    let tmpdir = tempfile::tempdir().expect("tempdir");
    let sidecar_root = tmpdir.path().join("sidecars");

    let host = PluginHostImpl::new().with_sidecar_root(sidecar_root.clone());
    let manifest = PluginManifest {
        name: "sample-sidecar".to_owned(),
        version: "0.1.0".to_owned(),
        kind: PluginKind::EnvHarness,
        trait_id: "rollout_core::Plugin".to_owned(),
        mode: PluginMode::Sidecar,
        runtime: RuntimeHints {
            python_min: None,
            gpu: false,
            memory_mib: 64,
        },
        entry: EntrySpec::Sidecar {
            command: vec![
                "python3".to_owned(),
                "-c".to_owned(),
                format!(
                    "import sys; sys.path.insert(0, '{}'); import sample_sidecar.__main__ as m; m.serve(sys.argv[1])",
                    python_examples.display()
                ),
            ],
            protocol: SidecarProtocol::FramedJsonUds,
            socket_template: "{name}-{pid}.sock".to_owned(),
        },
        config_schema_path: None,
        network_allowlist: vec![],
    };
    let handle = host.load(manifest).await.expect("load sidecar");
    let resp = host
        .call(&handle, "echo", b"hello".to_vec())
        .await
        .expect("call echo");
    // Response is a JSON envelope `{"payload": "hello"}`.
    let v: serde_json::Value = serde_json::from_slice(&resp).expect("json");
    assert_eq!(v["payload"], "hello");

    host.unload(handle).await.expect("unload");
}
