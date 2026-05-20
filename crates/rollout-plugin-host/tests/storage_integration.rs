//! Verify that `PluginHostImpl::with_storage(...)` persists the manifest into
//! the `plugins` namespace on load.

use std::sync::Arc;

use rollout_core::{
    EntrySpec, PluginHost, PluginKind, PluginManifest, PluginMode, RuntimeHints, Storage,
    StorageKey,
};
use rollout_plugin_host::PluginHostImpl;
use rollout_storage::EmbeddedStorage;
use smol_str::SmolStr;

fn cdylib_manifest_unloadable() -> PluginManifest {
    // We never actually load a dylib — we only need the manifest persisted.
    // To avoid the cdylib load path running, use a sidecar manifest with a
    // command that fails immediately... actually, we want load() to succeed
    // so the persistence runs. Use a cdylib manifest pointing at the in-tree
    // sample dylib.
    PluginManifest {
        name: "sample-rec".to_owned(),
        version: "0.1.0".to_owned(),
        kind: PluginKind::EnvHarness,
        trait_id: "rollout_core::Plugin".to_owned(),
        mode: PluginMode::RustCdylib,
        runtime: RuntimeHints {
            python_min: None,
            gpu: false,
            memory_mib: 16,
        },
        entry: EntrySpec::Cdylib {
            path: dylib_path(),
            symbol: "rollout_plugin_factory".to_owned(),
        },
        config_schema_path: None,
        network_allowlist: vec![],
    }
}

fn dylib_path() -> String {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let target = std::path::PathBuf::from(manifest_dir)
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
    .to_string_lossy()
    .into_owned()
}

#[tokio::test]
async fn host_persists_manifest_to_storage() {
    // Skip if the cdylib sample isn't pre-built.
    if !std::path::Path::new(&dylib_path()).exists() {
        eprintln!(
            "cdylib sample not pre-built at {} — skipping storage_integration",
            dylib_path()
        );
        return;
    }
    let tmp = tempfile::tempdir().expect("tempdir");
    let storage = Arc::new(
        EmbeddedStorage::open(tmp.path().join("rollout.db"))
            .await
            .expect("open"),
    );

    let host = PluginHostImpl::with_storage(storage.clone());
    let manifest = cdylib_manifest_unloadable();
    let handle = host.load(manifest.clone()).await.expect("load");

    // Read back the manifest from Storage.
    let key = StorageKey {
        namespace: SmolStr::new("plugins"),
        run_id: None,
        path: vec![SmolStr::new(&manifest.name)],
    };
    let bytes = storage
        .get_bytes(&key)
        .await
        .expect("storage get")
        .expect("manifest bytes present");
    let read_back: PluginManifest = serde_json::from_slice(&bytes).expect("decode");
    assert_eq!(read_back.name, manifest.name);
    assert_eq!(read_back.version, manifest.version);
    assert_eq!(read_back.mode, PluginMode::RustCdylib);

    host.unload(handle).await.expect("unload");
}
