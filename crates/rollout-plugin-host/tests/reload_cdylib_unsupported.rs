//! cdylib reload must return Fatal(PluginContract) per spec 03 §7.

use rollout_core::{
    CoreError, EntrySpec, FatalError, PluginHandle, PluginHost, PluginId, PluginKind,
    PluginManifest, PluginMode, RuntimeHints,
};
use rollout_plugin_host::PluginHostImpl;

fn fake_cdylib_handle() -> PluginHandle {
    PluginHandle {
        id: PluginId("rust-cdylib-sample-0.1.0".to_owned()),
        manifest: PluginManifest {
            name: "rust-cdylib-sample".to_owned(),
            version: "0.1.0".to_owned(),
            kind: PluginKind::Custom("test".to_owned()),
            trait_id: "rollout_core::Plugin".to_owned(),
            mode: PluginMode::RustCdylib,
            runtime: RuntimeHints {
                python_min: None,
                gpu: false,
                memory_mib: 16,
            },
            entry: EntrySpec::Cdylib {
                path: "/dev/null".to_owned(),
                symbol: "rollout_plugin_factory".to_owned(),
            },
            config_schema_path: None,
            network_allowlist: vec![],
        },
    }
}

#[tokio::test]
async fn reload_cdylib_returns_fatal_plugin_contract() {
    let host = PluginHostImpl::new();
    let handle = fake_cdylib_handle();
    // Inject a synthetic cdylib HandleState so reload dispatches on the
    // cdylib branch without requiring a real .dylib on disk.
    rollout_plugin_host::test_inject_cdylib_placeholder(&host, &handle).await;

    let err = host
        .reload(&handle, "test")
        .await
        .expect_err("cdylib reload must be Fatal(PluginContract)");
    match err {
        CoreError::Fatal(FatalError::PluginContract { plugin, msg }) => {
            assert_eq!(plugin, "rust-cdylib-sample");
            assert!(
                msg.contains("cdylib") && msg.contains("unsupported"),
                "msg should explain cdylib reload unsupported: {msg}"
            );
        }
        other => panic!("expected PluginContract, got {other:?}"),
    }
}
