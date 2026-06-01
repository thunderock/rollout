# Tool sandbox (HARNESS-02)

`rollout-harness-tool` exposes six tools — `python_exec`, `shell`, `file_read`,
`file_write` (Linux-enforced) and `http_get`, `http_post` (platform-independent,
SSRF-filtered) — behind a best-effort layered Linux sandbox.

> The crate `README.md` is the single source of truth for the matrix below; this
> chapter summarizes it.

## Threat-model boundary (D-TOOL-08)

> **Tool harnesses defend against accidental damage; they are NOT a security
> perimeter for actively malicious code.**

The sandbox is **process-isolated, NOT VM-isolated.** It is a real boundary
against accidental escape and run-away resource use — a buggy tool, a runaway
script, a path-traversal mistake, an SSRF to cloud metadata. It is **not** a
production-grade defense against an adversary actively trying to break out.
gVisor / Firecracker microVM isolation is a **v1.2+** capability, explicitly out
of scope. To run actively-hostile code, run this harness *inside* a microVM or a
throwaway host.

## Sandbox-depth matrix

| Layer | Mechanism | Linux | macOS | Defeats |
|---|---|---|---|---|
| Namespaces | `rustix`/`unshare` user+pid+net+mount+uts+ipc | yes (best-effort) | stub | ambient process/network visibility; net ns = default-deny egress for exec tools |
| Resource limits | `rustix::process::setrlimit` (CPU/AS/NOFILE/NPROC) | yes (always) | stub | CPU spin, memory blow-up, fd/pid exhaustion |
| cgroups v2 | `memory.max` + `pids.max` in a delegated subtree | yes, degrade-with-warning | stub | memory/pid exhaustion (defense-in-depth over rlimits) |
| Filesystem | `landlock` kernel FS allowlist | yes, fail-closed on kernel ≥ 5.13 | stub | reads/writes outside the per-invocation tempdir |
| Syscalls | `seccompiler` deny-default-`EPERM` curated allowlist, installed **LAST** | yes (always) | stub | `mount`/`keyctl`/`bpf`/`ptrace`/`socket` + `clone*`/`unshare` with `CLONE_NEWUSER` |
| File-tool FS root | `cap-std::fs::Dir` rooted at the tempdir | yes | yes | path traversal / TOCTOU for `file_read`/`file_write` |
| HTTP egress | SSRF-filtered hyper connector (post-DNS IP filter + IP pin + per-redirect re-filter) | yes | yes | SSRF / DNS-rebinding / redirect-to-IMDS / RFC1918 / loopback |

## Layer order (exec tools)

After `unshare`, the child applies, in order: `setrlimit` → landlock →
**seccomp LAST** → `execve`. seccomp is last because its own setup syscalls
(`landlock_*`, `prctl`, `setrlimit`) must be permitted; landlock precedes seccomp
because landlock enforcement needs `PR_SET_NO_NEW_PRIVS` + `landlock_restrict_self`.

The curated **seccomp** allowlist is deny-by-default with action `ERRNO(EPERM)`
(not `KILL`) so a blocked tool returns a typed error and the harness can emit a
`seccomp-violation` event rather than segfaulting. The allowlist is post-2020
syscall-aware (`clone3`, `openat2`, `faccessat2`, `rseq`, `arch_prctl`, the
`rt_sig*` family) — validated on the Linux CI lane against a real `strace -c`
baseline plus the `seccomp_python_runs` positive witness.

## Fail-closed kernel gate (D-TOOL-02)

`require_landlock = true` by default. On kernel **< 5.13** the harness refuses to
start: `Fatal::ConfigInvalid("landlock requires kernel >= 5.13, found {n}")`.
RHEL 8 (4.18) / Amazon Linux 2 (5.10) must set `require_landlock = false` to run
with reduced isolation (rlimits + seccomp + namespaces still apply). No silent
fallback.

## macOS dev stub (D-TOOL-05)

On macOS the exec/file sandbox compiles to a stub; a sandboxed `invoke` returns
`Fatal::ConfigInvalid("sandbox unavailable on macOS — dev stub")`. **Linux is the
only enforced surface.** The HTTP tools DO run on macOS — their SSRF defense is
an in-process hyper connector, not a syscall sandbox, so the `http_tool_blocks_*`
witnesses run on both lanes.

## HTTP SSRF connector

`http_get` / `http_post` run **in-process** with a custom hyper connector (not
`reqwest`, whose redirect-follow gives no per-hop injection point). The connector
resolves DNS itself, rejects private/link-local/loopback/IMDS IPs, pins the
chosen IP for the connection (defeating DNS rebinding), and re-applies the filter
on every redirect (defeating redirect-to-IMDS). Exec tools run in a deny-all net
namespace with `socket` blocked — the network surface is deliberately separate.
