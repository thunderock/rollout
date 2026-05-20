//! Manifest TOML parsing + plan-time validation.

use rollout_core::{CoreError, EntrySpec, FatalError, PluginMode};
use rollout_plugin_host::parse_manifest_str;

const PYO3_TOML: &str = r#"
name = "sample-inproc"
version = "0.1.0"
kind = "env-harness"
trait_id = "rollout_core::Plugin"
mode = "pyo3"
network_allowlist = []

[runtime]
python_min = "3.11"
gpu = false
memory_mib = 64

[entry.pyo3]
module = "sample_inproc.plugin"
factory = "create_plugin"
"#;

const SIDECAR_TOML: &str = r#"
name = "sample-sidecar"
version = "0.1.0"
kind = "env-harness"
trait_id = "rollout_core::Plugin"
mode = "sidecar"
network_allowlist = []

[runtime]
gpu = false
memory_mib = 64

[entry.sidecar]
command = ["python3", "-m", "sample_sidecar"]
protocol = "framed-json-uds"
socket_template = "./data/sidecars/{name}.sock"
"#;

const CDYLIB_TOML: &str = r#"
name = "rust-cdylib-sample"
version = "0.1.0"
kind = "env-harness"
trait_id = "rollout_core::Plugin"
mode = "rust-cdylib"
network_allowlist = []

[runtime]
gpu = false
memory_mib = 32

[entry.cdylib]
path = "../../../target/release/librust_cdylib_sample.dylib"
symbol = "rollout_plugin_factory"
"#;

#[test]
fn parse_pyo3_manifest_succeeds() {
    let m = parse_manifest_str(PYO3_TOML).expect("parse pyo3");
    assert_eq!(m.mode, PluginMode::Pyo3);
    match &m.entry {
        EntrySpec::Pyo3 { module, factory } => {
            assert_eq!(module, "sample_inproc.plugin");
            assert_eq!(factory, "create_plugin");
        }
        other => panic!("expected pyo3 entry, got {other:?}"),
    }
}

#[test]
fn parse_sidecar_manifest_succeeds() {
    let m = parse_manifest_str(SIDECAR_TOML).expect("parse sidecar");
    assert_eq!(m.mode, PluginMode::Sidecar);
    matches!(m.entry, EntrySpec::Sidecar { .. });
}

#[test]
fn parse_cdylib_manifest_succeeds() {
    let m = parse_manifest_str(CDYLIB_TOML).expect("parse cdylib");
    assert_eq!(m.mode, PluginMode::RustCdylib);
    match &m.entry {
        EntrySpec::Cdylib { symbol, .. } => assert_eq!(symbol, "rollout_plugin_factory"),
        other => panic!("expected cdylib entry, got {other:?}"),
    }
}

#[test]
fn manifest_validation_rejects_unknown_kind() {
    let bad = r#"
name = "x"
version = "0.1.0"
kind = "not-a-real-kind"
trait_id = "rollout_core::Plugin"
mode = "pyo3"
network_allowlist = []

[runtime]
python_min = "3.11"
gpu = false
memory_mib = 1

[entry.pyo3]
module = "m"
factory = "f"
"#;
    let err = parse_manifest_str(bad).expect_err("bad kind must fail");
    matches!(err, CoreError::Fatal(FatalError::ConfigInvalid { .. }));
}

#[test]
fn manifest_validation_requires_python_min_for_pyo3() {
    let bad = r#"
name = "x"
version = "0.1.0"
kind = "env-harness"
trait_id = "rollout_core::Plugin"
mode = "pyo3"
network_allowlist = []

[runtime]
gpu = false
memory_mib = 1

[entry.pyo3]
module = "m"
factory = "f"
"#;
    let err = parse_manifest_str(bad).expect_err("missing python_min must fail");
    matches!(err, CoreError::Fatal(FatalError::ConfigInvalid { .. }));
}

#[test]
fn manifest_validation_rejects_invalid_python_version() {
    let bad = r#"
name = "x"
version = "0.1.0"
kind = "env-harness"
trait_id = "rollout_core::Plugin"
mode = "pyo3"
network_allowlist = []

[runtime]
python_min = "2.7"
gpu = false
memory_mib = 1

[entry.pyo3]
module = "m"
factory = "f"
"#;
    let err = parse_manifest_str(bad).expect_err("python 2.7 must fail");
    matches!(err, CoreError::Fatal(FatalError::ConfigInvalid { .. }));
}
