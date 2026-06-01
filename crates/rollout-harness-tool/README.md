# rollout-harness-tool (HARNESS-02)

A sandboxed `ToolHarness` exposing six tools — `python_exec`, `shell`,
`file_read`, `file_write` (Linux-enforced) and `http_get`, `http_post`
(platform-independent, SSRF-filtered) — behind a best-effort layered Linux
sandbox.

## Threat-model boundary (D-TOOL-08)

> **Tool harnesses defend against accidental damage; they are NOT a security perimeter for actively malicious code.**

The sandbox is **process-isolated**, **NOT VM-isolated**. It is a real boundary
against accidental escape and run-away resource use — a buggy tool, a runaway
script, a path-traversal mistake, an SSRF to cloud metadata. It is **not** a
production-grade defense against an adversary who is actively trying to break
out. A VM-grade sandbox (**gVisor** / **Firecracker** microVM isolation) is a
**v1.2+** capability and is explicitly **out of scope** here (spec 07 §3).

If you need to run untrusted, actively-hostile code, run this harness *inside* a
microVM or a dedicated throwaway host — the in-process layers below are
defense-in-depth, not the perimeter.

## Sandbox-depth matrix (implemented layers)

| Layer | Mechanism | Linux | macOS | Defeats |
|---|---|---|---|---|
| Namespaces | `rustix`/`unshare` user+pid+net+mount+uts+ipc (`CLONE_NEWUSER`…) | yes (best-effort: degrades if userns unavailable, e.g. nested CI) | stub | ambient process/network visibility; net ns = default-deny egress for exec tools |
| Resource limits | `rustix::process::setrlimit` — `RLIMIT_CPU` / `RLIMIT_AS` / `RLIMIT_NOFILE` / `RLIMIT_NPROC` | yes (always) | stub | CPU spin, memory blow-up, fd/pid exhaustion |
| cgroups v2 | `memory.max` + `pids.max` in a delegated subtree | yes, **degrade-with-warning** (falls back to `RLIMIT_AS`/`RLIMIT_NPROC` when no delegated tree, e.g. GitHub Actions) | stub | memory/pid exhaustion (defense-in-depth over rlimits) |
| Filesystem | `landlock` kernel FS allowlist (rw tempdir + ro tool binary) | yes, **fail-closed** on kernel ≥ 5.13 | stub | reads/writes outside the per-invocation tempdir |
| Syscalls | `seccompiler` deny-default-`EPERM` curated allowlist, installed **LAST** | yes (always) | stub | `mount`/`keyctl`/`bpf`/`ptrace`/`socket` + `clone*`/`unshare` with `CLONE_NEWUSER` |
| File-tool FS root | `cap-std::fs::Dir` rooted at the tempdir (rejects `..`/symlink by construction) | yes | yes | path traversal / TOCTOU for `file_read`/`file_write` |
| HTTP egress | SSRF-filtered hyper connector: post-DNS IP filter + IP pinning + per-redirect re-filter | yes | yes | SSRF / DNS-rebinding / redirect-to-IMDS / RFC1918 / loopback |

The HTTP tools run **in-process** with the SSRF-filtered connector — they are
**NOT** under the exec seccomp filter (exec tools run in a deny-all net
namespace with `socket` blocked; the network surface is deliberately separate).

### Load-bearing layer order (exec tools)

The child applies, after `unshare`: `setrlimit` → landlock → **seccomp LAST** →
`execve`. seccomp is installed last because its own setup syscalls
(`landlock_*`, `prctl`, `setrlimit`) must be permitted; landlock precedes seccomp
because landlock enforcement needs `PR_SET_NO_NEW_PRIVS` + `landlock_restrict_self`.

## Fail-closed kernel gate (D-TOOL-02)

`require_landlock = true` by default. On kernel **< 5.13** the harness refuses to
start: `Fatal::ConfigInvalid("landlock requires kernel >= 5.13, found {n}")`.

| Distro / kernel | Posture |
|---|---|
| Ubuntu 22.04 (5.15), modern kernels ≥ 5.13 | full isolation |
| RHEL 8 (4.18), Amazon Linux 2 (5.10) | **reduced isolation** — must set `require_landlock = false` to run; no landlock FS enforcement (rlimits + seccomp + namespaces still apply) |

There is no silent fallback: either landlock enforces or the operator explicitly
accepts reduced isolation.

## macOS = compile-only dev stub (D-TOOL-05)

On macOS the exec/file sandbox compiles to a stub; a sandboxed `invoke` returns
`Fatal::ConfigInvalid("sandbox unavailable on macOS — dev stub")`. **Linux is the
only enforced surface.** The HTTP tools (`http_get`/`http_post`) DO run on macOS
because their SSRF defense is an in-process hyper connector, not a syscall
sandbox — so the `http_tool_blocks_*` witnesses run on both lanes.

## Known limitation: clone3 flag filtering (Open Question #1)

The seccomp masked-flag condition rejecting `CLONE_NEWUSER` is exact for `clone`
(flags live in a register) but **best-effort for `clone3`** (its flags sit behind
a `struct clone_args` pointer that seccomp arg-filtering cannot deep-inspect).
The load-bearing user-namespace defense is therefore **not** clone3 arg-filtering
but the combination of `PR_SET_NO_NEW_PRIVS` + dropped `CAP_SYS_ADMIN` + the deny
of `unshare` — confirmed by the `sandbox_blocks_userns` witness.

## Tools

| Tool | Class | Surface |
|---|---|---|
| `python_exec` | Exec | `python3 -c <code>`, exact-full-path, `shell=False` argv, Linux sandbox |
| `shell` | Exec | allowlisted command (argv vector, never `/bin/sh -c`), Linux sandbox |
| `file_read` / `file_write` | Filesystem | in-process via cap-std tempdir root |
| `http_get` / `http_post` | Network | in-process SSRF-filtered hyper connector |

See the crate-level rustdoc and `docs/specs/07-harnesses.md` for the full
contract.
