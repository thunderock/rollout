//! Colored, step-by-step human output (D-DOCTOR-02 default format).

use crate::commands::cloud::doctor::checks::{CheckResult, CheckStatus};
use std::fmt::Write as _;

/// Print the check results to stdout with `✓`/`✗` icons + per-check latency.
pub fn print(results: &[CheckResult]) {
    print!("{}", render(results));
}

/// Render to a `String` (testable without stdout).
#[must_use]
pub fn render(results: &[CheckResult]) -> String {
    let mut s = String::new();
    let pass = results
        .iter()
        .filter(|c| matches!(c.status, CheckStatus::Pass))
        .count();
    let fail = results.len() - pass;
    for c in results {
        let icon = if matches!(c.status, CheckStatus::Pass) {
            "\x1b[32m✓\x1b[0m"
        } else {
            "\x1b[31m✗\x1b[0m"
        };
        let _ = write!(s, "  {icon} {:30} {:>7}ms", c.name, c.latency_ms);
        if let Some(e) = &c.error {
            let _ = write!(s, "  \x1b[31m{e}\x1b[0m");
        }
        s.push('\n');
    }
    let total: u128 = results.iter().map(|c| c.latency_ms).sum();
    s.push('\n');
    let _ = writeln!(s, "  {pass} pass, {fail} fail — total {total}ms");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_human_format_renders_check_name_status_latency() {
        let results = vec![
            CheckResult {
                name: "reachability",
                status: CheckStatus::Pass,
                latency_ms: 12,
                error: None,
            },
            CheckResult {
                name: "queue",
                status: CheckStatus::Fail,
                latency_ms: 34,
                error: Some("boom".to_owned()),
            },
        ];
        let out = render(&results);
        assert!(out.contains("reachability"));
        assert!(out.contains("queue"));
        assert!(out.contains('✓'));
        assert!(out.contains('✗'));
        assert!(out.contains("boom"));
        assert!(out.contains("1 pass, 1 fail"));
        // exactly 2 check lines + blank + summary = 4 newlines
        assert_eq!(out.matches('\n').count(), 4);
    }
}
