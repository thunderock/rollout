//! `http_post` (D-TOOL-03) — SSRF-filtered POST via the hyper connector.
//!
//! `SideEffectClass::Network`. Shares the SSRF defense + redirect re-filter in
//! [`crate::http`] with `http_get`.

use std::time::Duration;

use crate::http::{self, EgressConfig, HttpError, Method, Resolver};

/// Run an `http_post`. `args` must contain `{ "url": "…", "body": "…"? }`.
///
/// Output JSON: `{ "status": u16, "body": String }`.
///
/// # Errors
/// Returns [`HttpError`] on a blocked SSRF target, bad URL, timeout, or transport error.
pub async fn run(
    args: &serde_json::Value,
    egress: &EgressConfig,
    resolver: &dyn Resolver,
    timeout: Duration,
) -> Result<serde_json::Value, HttpError> {
    let url = args
        .get("url")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| HttpError::BadUrl("missing string field `url`".to_owned()))?;
    let body = args
        .get("body")
        .and_then(serde_json::Value::as_str)
        .map(|s| s.as_bytes().to_vec());
    let resp = http::fetch(Method::Post, url, body, egress, resolver, timeout).await?;
    Ok(serde_json::json!({
        "status": resp.status,
        "body": String::from_utf8_lossy(&resp.body),
    }))
}
