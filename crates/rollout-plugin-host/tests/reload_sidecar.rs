//! Hot-reload a sidecar via SIGTERM + respawn. Requires `dev-hot-reload`.

#![cfg(feature = "dev-hot-reload")]

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
async fn reload_sidecar_sigterm_respawns() {
    if !python3_available() {
        eprintln!("python3 not available; skipping reload_sidecar_sigterm_respawns");
        return;
    }
    let ws = workspace_root();
    let python_examples = ws.join("python/examples");
    let tmpdir = tempfile::tempdir().expect("tempdir");
    let host = PluginHostImpl::new().with_sidecar_root(tmpdir.path().to_path_buf());

    let manifest = PluginManifest {
        name: "sample-sidecar-reload".to_owned(),
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
    let handle = host.load(manifest).await.expect("load");
    let resp = host
        .call(&handle, "echo", b"pre".to_vec())
        .await
        .expect("call pre");
    let v: serde_json::Value = serde_json::from_slice(&resp).unwrap();
    assert_eq!(v["payload"], "pre");

    host.reload(&handle, "rotate").await.expect("reload");

    let resp2 = host
        .call(&handle, "echo", b"post".to_vec())
        .await
        .expect("call post");
    let v2: serde_json::Value = serde_json::from_slice(&resp2).unwrap();
    assert_eq!(v2["payload"], "post");

    host.unload(handle).await.expect("unload");
}
