//! `http_get` (D-TOOL-03) — SSRF-filtered GET via the hyper connector.
//!
//! `SideEffectClass::Network`. Runs in-process (NOT under the exec seccomp
//! filter — exec tools have no sockets; the network surface is separate,
//! RESEARCH Architecture note). The SSRF defense is in [`crate::http`].

use std::time::Duration;

use crate::http::{self, EgressConfig, HttpError, Method, Resolver};

/// Run an `http_get`. `args` must contain `{ "url": "https://…" }`.
///
/// Returns `(output_json, body_was_truncated)`-shaped output as a JSON value:
/// `{ "status": u16, "body": String }`.
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
    let resp = http::fetch(Method::Get, url, None, egress, resolver, timeout).await?;
    Ok(serde_json::json!({
        "status": resp.status,
        "body": String::from_utf8_lossy(&resp.body),
    }))
}
