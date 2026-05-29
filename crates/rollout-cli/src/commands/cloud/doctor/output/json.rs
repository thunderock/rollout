//! Machine-readable JSON output (D-DOCTOR-02): `{checks: [...], summary: {...}}`.

use crate::commands::cloud::doctor::checks::{CheckResult, CheckStatus};
use serde::Serialize;

#[derive(Serialize)]
struct Summary {
    pass_count: usize,
    fail_count: usize,
    total_latency_ms: u128,
}

#[derive(Serialize)]
struct DoctorReport<'a> {
    checks: &'a [CheckResult],
    summary: Summary,
}

/// Print the pretty JSON report to stdout.
pub fn print(results: &[CheckResult]) {
    println!("{}", render(results));
}

/// Render the report to a JSON `String` (testable without stdout).
#[must_use]
pub fn render(results: &[CheckResult]) -> String {
    let pass_count = results
        .iter()
        .filter(|c| matches!(c.status, CheckStatus::Pass))
        .count();
    let fail_count = results.len() - pass_count;
    let total_latency_ms = results.iter().map(|c| c.latency_ms).sum();
    let report = DoctorReport {
        checks: results,
        summary: Summary {
            pass_count,
            fail_count,
            total_latency_ms,
        },
    };
    serde_json::to_string_pretty(&report).expect("doctor report serializes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_json_format_matches_d_doctor_02_schema() {
        let results = vec![
            CheckResult {
                name: "object_store",
                status: CheckStatus::Pass,
                latency_ms: 10,
                error: None,
            },
            CheckResult {
                name: "secret_store",
                status: CheckStatus::Fail,
                latency_ms: 5,
                error: Some("nope".to_owned()),
            },
        ];
        let json = render(&results);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["summary"]["pass_count"], 1);
        assert_eq!(v["summary"]["fail_count"], 1);
        assert_eq!(v["summary"]["total_latency_ms"], 15);
        assert_eq!(v["checks"].as_array().unwrap().len(), 2);
        assert_eq!(v["checks"][0]["name"], "object_store");
        assert_eq!(v["checks"][0]["status"], "pass");
        // error omitted on pass, present on fail
        assert!(v["checks"][0].get("error").is_none());
        assert_eq!(v["checks"][1]["error"], "nope");
    }
}
