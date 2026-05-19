//! Workspace dev tasks (schema-gen, schema-check, dep checks). Not published.

mod check;
mod schema_gen;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let rest: Vec<String> = args.iter().skip(2).cloned().collect();
    let code = match args.get(1).map(String::as_str) {
        Some("schema-gen") => schema_gen::run(&rest),
        Some("schema-check") => check::run(),
        Some("check-deps") => {
            eprintln!("check-deps: not yet implemented (plan 05)");
            0
        }
        _ => {
            eprintln!("Usage: cargo xtask <schema-gen [--out-dir PATH]|schema-check|check-deps>");
            1
        }
    };
    std::process::exit(code);
}
