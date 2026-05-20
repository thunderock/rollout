//! Hot-reload a `PyO3` plugin via `importlib.reload`. Requires the
//! `dev-hot-reload` Cargo feature.

#![cfg(feature = "dev-hot-reload")]

use std::path::PathBuf;

use rollout_core::{EntrySpec, PluginHost, PluginKind, PluginManifest, PluginMode, RuntimeHints};
use rollout_plugin_host::PluginHostImpl;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

#[tokio::test]
#[ignore = "requires Python 3.11+ + writable tempdir on PYTHONPATH; smoke driver gates"]
async fn reload_pyo3_invokes_importlib() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let module_dir = tmp.path().join("hotreload_pkg");
    std::fs::create_dir_all(&module_dir).unwrap();
    std::fs::write(module_dir.join("__init__.py"), "").unwrap();
    let plugin_py = module_dir.join("plugin.py");
    std::fs::write(
        &plugin_py,
        r#"
class _P:
    def call(self, method, payload):
        return b"v1"
def create_plugin():
    return _P()
"#,
    )
    .unwrap();

    let host = PluginHostImpl::new().with_python_path(vec![tmp.path().display().to_string()]);
    let manifest = PluginManifest {
        name: "hotreload-pkg".to_owned(),
        version: "0.1.0".to_owned(),
        kind: PluginKind::EnvHarness,
        trait_id: "rollout_core::Plugin".to_owned(),
        mode: PluginMode::Pyo3,
        runtime: RuntimeHints {
            python_min: Some("3.11".to_owned()),
            gpu: false,
            memory_mib: 32,
        },
        entry: EntrySpec::Pyo3 {
            module: "hotreload_pkg.plugin".to_owned(),
            factory: "create_plugin".to_owned(),
        },
        config_schema_path: None,
        network_allowlist: vec![],
    };
    let handle = host.load(manifest).await.expect("load pyo3");

    let before = host.call(&handle, "x", Vec::new()).await.expect("call v1");
    assert_eq!(before, b"v1");

    std::fs::write(
        &plugin_py,
        r#"
class _P:
    def call(self, method, payload):
        return b"v2"
def create_plugin():
    return _P()
"#,
    )
    .unwrap();
    let _ = workspace_root();
    host.reload(&handle, "test-reload").await.expect("reload");

    let after = host.call(&handle, "x", Vec::new()).await.expect("call v2");
    assert_eq!(after, b"v2");
    host.unload(handle).await.expect("unload");
}
