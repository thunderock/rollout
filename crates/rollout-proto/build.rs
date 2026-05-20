//! Compile transport.proto + plugin.proto via tonic-build.
//! Per CONTEXT D-PROTO-01 this is the ONLY tonic-build invocation in the workspace.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/transport.proto");
    println!("cargo:rerun-if-changed=proto/plugin.proto");
    // Vendored protoc avoids a system protobuf-compiler install (RESEARCH.md §Environment).
    if std::env::var_os("PROTOC").is_none() {
        std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path()?);
    }
    tonic_prost_build::configure()
        .build_client(true)
        .build_server(true)
        .compile_protos(
            &["proto/transport.proto", "proto/plugin.proto"],
            &["proto"],
        )?;
    Ok(())
}
