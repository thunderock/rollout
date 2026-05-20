//! `cargo xtask gen-protos` — regenerate Python protobuf stubs for `rollout-proto`.
//!
//! Opt-in: requires `grpcio-tools` on the dev machine. The in-tree Python sidecar
//! sample uses stdlib length-prefixed framing (no `pip install grpcio` required)
//! per AGENTS.md §7 + RESEARCH.md Pitfall 9. Only run this when authoring a
//! Python plugin that uses real gRPC.
use std::path::{Path, PathBuf};
use std::process::Command;

/// Default output directory for generated Python protobuf stubs.
const DEFAULT_OUT_DIR: &str = "python/rollout/_proto";

/// Default proto search directory inside the workspace.
const PROTO_DIR: &str = "crates/rollout-proto/proto";

/// Run gen-protos. Supported flag: `--out-dir <PATH>` (default: `python/rollout/_proto`).
pub fn run(args: &[String]) -> i32 {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return 0;
    }
    let out_dir = parse_out_dir(args).unwrap_or_else(|| PathBuf::from(DEFAULT_OUT_DIR));
    let root = workspace_root();
    let proto_dir = root.join(PROTO_DIR);
    let out_dir_abs = if out_dir.is_absolute() {
        out_dir
    } else {
        root.join(out_dir)
    };

    if !proto_dir.is_dir() {
        eprintln!("gen-protos: proto dir not found: {}", proto_dir.display());
        return 2;
    }

    if !has_grpc_tools() {
        eprintln!(
            "gen-protos: `python3 -m grpc_tools.protoc` not available.\n\
             The in-tree Python sample sidecar uses stdlib length-prefixed framing\n\
             and does NOT need this. Only run gen-protos if you are authoring a\n\
             Python plugin that uses real gRPC. To enable:\n\
             \n    pip install grpcio-tools\n"
        );
        return 0; // not an error — opt-in by design
    }

    std::fs::create_dir_all(&out_dir_abs).expect("mkdir out_dir");
    ensure_init_py(&out_dir_abs);

    let status = Command::new("python3")
        .arg("-m")
        .arg("grpc_tools.protoc")
        .arg(format!("--proto_path={}", proto_dir.display()))
        .arg(format!("--python_out={}", out_dir_abs.display()))
        .arg(format!("--grpc_python_out={}", out_dir_abs.display()))
        .arg("transport.proto")
        .arg("plugin.proto")
        .status();
    match status {
        Ok(s) if s.success() => {
            eprintln!("wrote {} (transport + plugin stubs)", out_dir_abs.display());
            0
        }
        Ok(s) => {
            eprintln!("gen-protos: grpc_tools.protoc exited {s}");
            1
        }
        Err(e) => {
            eprintln!("gen-protos: failed to spawn python3: {e}");
            1
        }
    }
}

fn print_help() {
    eprintln!(
        "cargo xtask gen-protos [--out-dir PATH]\n\
         \n\
         Regenerate Python protobuf stubs for rollout-proto into the given directory.\n\
         Default: {DEFAULT_OUT_DIR}\n\
         \n\
         Requires `grpcio-tools` on the dev machine (opt-in, not run in CI). Missing\n\
         grpc_tools is NOT an error — the in-tree Python sample sidecar uses stdlib\n\
         framing and does not require this step. See AGENTS.md §7."
    );
}

fn parse_out_dir(args: &[String]) -> Option<PathBuf> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--out-dir" {
            return it.next().map(PathBuf::from);
        }
        if let Some(rest) = a.strip_prefix("--out-dir=") {
            return Some(PathBuf::from(rest));
        }
    }
    None
}

fn workspace_root() -> PathBuf {
    // Walk up from cwd looking for Cargo.toml that declares [workspace].
    let mut here = std::env::current_dir().expect("cwd");
    loop {
        let cargo = here.join("Cargo.toml");
        if cargo.is_file() {
            if let Ok(s) = std::fs::read_to_string(&cargo) {
                if s.contains("[workspace]") {
                    return here;
                }
            }
        }
        if !here.pop() {
            return std::env::current_dir().expect("cwd");
        }
    }
}

fn has_grpc_tools() -> bool {
    Command::new("python3")
        .args(["-c", "import grpc_tools.protoc"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn ensure_init_py(dir: &Path) {
    let p = dir.join("__init__.py");
    if !p.exists() {
        std::fs::write(
            &p,
            "\"\"\"Generated gRPC stubs (opt-in; populated by `make protos`).\"\"\"\n",
        )
        .expect("write __init__.py");
    }
}
