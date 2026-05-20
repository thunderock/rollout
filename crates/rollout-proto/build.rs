//! build.rs — populated by plan 02-01 to invoke tonic-build on .proto files.
fn main() {
    println!("cargo:rerun-if-changed=proto/");
}
