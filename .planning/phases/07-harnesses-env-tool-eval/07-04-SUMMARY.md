---
phase: 07-harnesses-env-tool-eval
plan: 04
subsystem: infra
tags: [rust, ssrf, http, hyper, rustls, sandbox, tool-harness, security]

# Dependency graph
requires:
  - phase: 07-harnesses-env-tool-eval (plan 02)
    provides: rollout-harness-tool crate, ToolHarnessImpl + ToolSettings + ToolHarness dispatch, the four exec/file tools, macOS dev stub, fail-closed kernel gate
provides:
  - "http_get/http_post tools (SideEffectClass::Network), feature-gated, platform-independent (in-process hyper)"
  - "SSRF-hardened HTTP driver: post-DNS IP filter (RFC1918/CGNAT/link-local/IMDS/loopback/v6-link-local/unique-local/multicast/v4-mapped) + IP pinning + manual redirect loop re-filtering each hop"
  - "ToolSettings.egress_allowlist + enable_http_get/post; async HTTP dispatch alongside the sync exec/file dispatch"
  - "Honest sandbox-depth matrix README (D-TOOL-08): process-isolated NOT VM-isolated, gVisor/Firecracker out, kernel-5.13 gate, macOS stub, clone3 limitation"
affects: [harness, ssrf, http-tools, sandbox-docs]

# Tech tracking
tech-stack:
  added: [hyper-client, tokio-rustls-client, webpki-roots, http-body-util]
  patterns: [self-resolving + IP-pinning hyper http1 driver (no high-level client), manual redirect loop with per-hop re-filter, test-only allow_loopback escape hatch that never relaxes IMDS/private blocks, scripted+counting mock resolver for rebinding witness, raw-TCP mock HTTP server in tests]

key-files:
  created:
    - crates/rollout-harness-tool/src/http/mod.rs
    - crates/rollout-harness-tool/src/http/connector.rs
    - crates/rollout-harness-tool/src/tools/http_get.rs
    - crates/rollout-harness-tool/src/tools/http_post.rs
    - crates/rollout-harness-tool/tests/http_ssrf.rs
    - crates/rollout-harness-tool/README.md
  modified:
    - crates/rollout-harness-tool/src/lib.rs
    - crates/rollout-harness-tool/src/tools/mod.rs
    - crates/rollout-harness-tool/Cargo.toml
    - scripts/check-forbidden-patterns.sh
    - Cargo.lock

key-decisions:
  - "Drive hyper::client::conn::http1 directly over a raw TcpStream connected to the PINNED SocketAddr, instead of the high-level client crate — the latter auto-follows redirects with no per-hop IP-refilter hook (RESEARCH mandate). Manual redirect loop re-runs resolve+filter+pin for each Location."
  - "IP pinning = one DNS resolution per hop; the resolved IP becomes the connect target, so a rebinding resolver returning IMDS on the second lookup is never reached. Witness asserts exactly one resolution per hop."
  - "TLS via tokio-rustls (rustls 0.23 + webpki-roots) NOT hyper-rustls/high-level client — keeps the connect target under our control so the IP filter cannot be bypassed; no openssl (cargo-deny ban)."
  - "Test-only EgressConfig.allow_loopback escape hatch lets witnesses point at a 127.0.0.1 mock server; it relaxes ONLY the Loopback block, never link-local/IMDS/private/CGNAT — proven by a connector unit test. Production egress() always sets it false."
  - "HTTP tools are platform-independent (in-process hyper, not a syscall sandbox) — they run on the macOS test lane unlike the Linux-only exec tools; cfg(feature='http') not cfg(target_os='linux')."

patterns-established:
  - "Async HTTP dispatch (dispatch_http) routed from invoke() alongside the sync exec/file dispatch; HttpError::TimedOut -> ToolOutcome::TimedOut, all other HttpError -> ToolOutcome::Error"
  - "forbidden-patterns: a security filter that must NAME the IMDS address to block it is added to the imds-aws-raw allowed-paths (precedent: AWS imds module)"

requirements-completed: [HARNESS-02]

# Metrics
duration: ~40min
completed: 2026-06-01
---

# Phase 7 Plan 04: HTTP tools (SSRF) + sandbox-depth matrix Summary

**Completed `rollout-harness-tool` (HARNESS-02, part 2 of 2): the two SSRF-hardened HTTP tools (`http_get`/`http_post`) backed by a self-resolving, IP-pinning hyper 1.x + tokio-rustls driver that filters resolved IPs post-DNS and re-applies the filter on every redirect — the defense the high-level client crate cannot give — plus the honest sandbox-depth-matrix README (D-TOOL-08). The redirect-to-IMDS and DNS-rebinding witnesses, the highest-value security tests in the phase, pass on macOS (in-process, no real network).**

## Task Commits

1. **Task 1: SSRF connector + http_get/http_post + witnesses** — `f350f36` (feat).
2. **Task 2: sandbox-depth matrix README + crate-doc boundary** — `746a730` (docs).

## The SSRF defense (RESEARCH Pattern 4, as implemented)

**Filtered IP ranges** (`src/http/connector.rs::blocked_range`), each returning a typed `BlockReason`:

| Range | Reason |
|---|---|
| `127.0.0.0/8`, `::1` | Loopback |
| `0.0.0.0`, `::` | Unspecified |
| `169.254.0.0/16` (incl. the cloud IMDS address) | LinkLocal |
| `10/8`, `172.16/12`, `192.168/16` | Private (RFC1918) |
| `100.64.0.0/10` | Cgnat |
| v4 multicast/broadcast, v6 `ff00::/8` | Multicast |
| `fe80::/10` | Ipv6LinkLocal |
| `fc00::/7` | Ipv6UniqueLocal |
| `::ffff:x.x.x.x` wrapping any blocked v4 | MappedV4 |
| not in egress allowlist (when allowlist non-empty) | NotAllowlisted |

**Per-hop flow** (`src/http/mod.rs::one_hop`): parse URL → resolve host via the injectable `Resolver` → `pick_safe_ip` (filter + pick first safe IP) → connect a raw `TcpStream` to `SocketAddr::new(pinned_ip, port)` → hyper http1 handshake (TLS via tokio-rustls for https) → issue request with redirects DISABLED.

**Redirect handling** (`fetch`): a 3xx with `Location` re-enters the loop after `resolve_redirect` (absolute or relative), and the full resolve+filter+pin runs again for the new URL. **Redirect chain cap = `MAX_REDIRECTS = 5`** → `HttpError::TooManyRedirects`.

**IP pinning defeats rebinding:** exactly one DNS resolution happens per hop and that IP is the connect target, so a resolver that returns a public IP first and IMDS second never connects to IMDS. The `http_tool_blocks_dns_rebinding` witness asserts `resolver.calls() == 1` and the IMDS tripwire saw zero connections.

## egress-allowlist config shape

`ToolSettings.egress_allowlist: Vec<IpAddr>` (defaults empty = block-list only; private/link-local/IMDS/loopback/CGNAT are *always* blocked regardless). When non-empty, a resolved IP must additionally appear in the list (defends split-horizon DNS, Pattern 4 step 3). Plus `enable_http_get` / `enable_http_post` (default true). The harness builds `http::EgressConfig { allowlist, allow_loopback: false }` per call — `allow_loopback` is a test-only field; production is always `false`.

## Witnesses (SC2) — all run on macOS + Linux (in-process, no real network)

| Test | Asserts |
|---|---|
| `http_tool_blocks_redirect_to_imds` | 302 → `http://169.254.169.254:.../latest/...` re-filtered → `Blocked(LinkLocal)`; IMDS tripwire saw 0 connections |
| `http_tool_blocks_dns_rebinding` | public IP pinned on hop 1; second lookup (IMDS) never reached; `resolver.calls()==1`; IMDS tripwire 0 |
| `http_tool_blocks_rfc1918` | `10.0.0.1` / `192.168.1.1` / `172.16.0.1` → `Blocked(Private)` |
| `http_tool_blocks_ipv6_loopback_v4_mapped` | `::1`→Loopback, `fe80::1`→Ipv6LinkLocal, `::ffff:127.0.0.1`→MappedV4 |
| `http_get_happy_path` / `http_post_happy_path` | GET 200 + POST 201 against a loopback mock (allow_loopback) |
| `tool_harness_http_get_blocks_imds` | the `ToolHarness::invoke` path returns `ToolOutcome::Error` for a GET to the IMDS address |
| 7 connector unit tests | range classification incl. the test-escape never unblocking IMDS |

These are platform-independent (the SSRF logic is a connector + resolver filter, not a syscall sandbox), so they run on BOTH lanes — **none cfg-gated Linux-only, nothing Linux-CI-deferred for this plan.**

## Final sandbox-layer matrix as shipped (README)

namespaces (best-effort) · `setrlimit` (always) · cgroups v2 (degrade-with-warning) · landlock (fail-closed ≥5.13) · seccomp (deny-default-EPERM, installed LAST) · cap-std FS root (file tools) · SSRF-filtered hyper connector (http tools, in-process, NOT under the exec net-deny seccomp filter). Boundary stated verbatim: "Tool harnesses defend against accidental damage; they are NOT a security perimeter for actively malicious code." Process-isolated, NOT VM-isolated; gVisor/Firecracker = v1.2+ out of scope; macOS = compile-only dev stub for exec/file; clone3 flag-filter limitation (OQ#1) documented honestly.

## Verification

- macOS (this box): `cargo test -p rollout-harness-tool --all-features` → 7 unit + 7 http_ssrf + 1 macos_stub pass; the four named `http_tool_blocks_*` witnesses pass.
- `cargo deny check` (full) → advisories/bans/licenses/sources all ok — **no openssl pulled by the http stack, no new license-allowlist entry needed**.
- `cargo clippy -p rollout-harness-tool --all-targets --all-features -- -D warnings` → clean.
- `RUSTDOCFLAGS`-deny `cargo doc -p rollout-harness-tool --no-deps --all-features` → green (DOCS-03; new public http types + BlockReason/HttpError/EgressConfig/Resolver documented).
- `cargo fmt -p rollout-harness-tool -- --check` → clean.
- `bash scripts/check-forbidden-patterns.sh` → green (IMDS literal allowed only in the http filter/witnesses + AWS imds module).
- `rg "reqwest" Cargo.toml src` → nothing (hyper-only); `mdbook build docs/book` → ok.
- Bare `cargo test --workspace --tests` (no env, default features which now include http_get/http_post) → green.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] forbidden-patterns `imds-aws-raw` grep would reject the new SSRF filter/witnesses**
- **Found during:** Task 1 — the connector + `http_ssrf.rs` must NAME `169.254.169.254` to block/witness it, but the `imds-aws-raw` check only allowed the AWS imds module.
- **Fix:** added `crates/rollout-harness-tool/src/http/` and `tests/http_ssrf.rs` to that check's allowed-paths (precedent: the AWS imds module is already listed). The connector REJECTS the address; the witnesses PROVE it.
- **Files:** scripts/check-forbidden-patterns.sh. **Commit:** `f350f36`.

**2. [Rule 1 - Bug] `reqwest` literal in doc comments tripped the RESEARCH-mandated `rg "reqwest"` acceptance grep**
- **Found during:** Task 1 — explaining WHY the high-level client is avoided named it literally in Cargo.toml + `http/mod.rs` docs.
- **Fix:** reworded to "the high-level HTTP client crate" (precedent: 07-02 reworded literal tokens to satisfy acceptance greps). Behaviour unchanged.
- **Files:** Cargo.toml, src/http/mod.rs. **Commit:** `f350f36`.

**3. [Rule 1 - Bug] clippy `-D warnings` on the test: items-after-statements + doc_markdown**
- **Found during:** Task 1 — a mid-function `RedirectResolver` struct/impl + un-backticked `http_get`/`ToolHarness` in a doc comment.
- **Fix:** hoisted the resolver to module scope; backticked the identifiers.
- **Files:** tests/http_ssrf.rs. **Commit:** `f350f36`.

**4. [Rule 1 - Bug] README blockquote line-wrap split "NOT a security perimeter"**
- **Found during:** Task 2 — the `rg -q "NOT a security perimeter"` acceptance failed because the phrase wrapped across two `>` lines.
- **Fix:** put the verbatim boundary sentence on one line.
- **Files:** README.md. **Commit:** `746a730`.

**Total:** 4 auto-fixed (1 blocking grep allowlist, 3 lint/grep/wrap). No architectural changes; no scope creep; no reqwest, no openssl.

## Design choices worth recording

- The high-level HTTP client crate was deliberately avoided: it auto-follows redirects with no per-hop hook to re-apply the IP filter, so `http_tool_blocks_redirect_to_imds` cannot pass with it. Driving `hyper::client::conn::http1` over a self-connected pinned `TcpStream` keeps the connect target fully under the filter's control.
- The `Resolver` trait is the seam that makes the rebinding witness possible without real DNS — production uses `StdResolver` (getaddrinfo via `ToSocketAddrs`), tests inject a scripted/counting resolver.
- `allow_loopback` is the minimal honest test seam: it relaxes ONLY the loopback block (so a `127.0.0.1` mock server is reachable) and a dedicated connector unit test (`loopback_test_escape_never_unblocks_imds`) proves it never relaxes the IMDS/private blocks; production sets it false.

## Known Stubs

None. Both HTTP tools are fully wired (resolver → filter → pin → connect → redirect re-filter → ToolResult). The `descriptor()` now advertises all six tools when their features are enabled.

## Next Plan Readiness

`rollout-harness-tool` (HARNESS-02) is feature-complete: six tools, the layered Linux sandbox (07-02), the SSRF http surface (this plan), and the honest sandbox-depth matrix. The Linux-only exec/file witnesses still validate on the `harness-linux` lane (07-02 deferred list, unchanged). 07-05 (phase closeout) can proceed.

## Self-Check: PASSED

All 6 created files exist on disk; both task commits (`f350f36`, `746a730`) are in the git log; the four named witnesses + full deny/clippy/doc/fmt/forbidden/mdbook gates are green; bare `cargo test --workspace --tests` is green.
