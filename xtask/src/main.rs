fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("schema-gen") => eprintln!("schema-gen: not yet implemented (plan 04)"),
        Some("check-deps") => eprintln!("check-deps: not yet implemented (plan 05)"),
        _ => {
            eprintln!("Usage: cargo xtask <schema-gen|check-deps>");
            std::process::exit(1);
        }
    }
}
