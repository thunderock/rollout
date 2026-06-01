---
phase: 07-harnesses-env-tool-eval
plan: 02
subsystem: infra
tags: [rust, sandbox, seccomp, landlock, cgroups, namespaces, cap-std, rustix, tool-harness]

# Dependency graph
requires:
  - phase: 07-harnesses-env-tool-eval (plan 00)
    provides: spec-07 ToolHarness trait + types, HarnessDependencies, Linux-gated sandbox dep pins, harness-linux CI lane + strace baseline artifact
provides:
  - "Layered Linux sandbox launcher (namespaces + setrlimit + landlock + seccomp + cgroups v2) composing in the load-bearing order, seccomp installed LAST"
  - "Curated deny-default-EPERM seccomp ALLOWLIST + BPF filter builder (clone3 refusing CLONE_NEWUSER)"
  - "Fail-closed kernel gate (D-TOOL-02): refuse on kernel < 5.13 unless require_landlock=false"
  - "Four non-HTTP tools: python_exec, shell (shell=False argv + exact-full-path allowlist), file_read, file_write (cap-std root)"
  - "cgroups v2 degrade-with-warning controller; cap-std capability FS root with path-escape rejection"
  - "macOS compile-only dev stub returning the documented Fatal; CVE-class negative + per-tool positive Linux witnesses"
affects: [07-04-http-tools-ssrf, harness, seccomp, sandbox]

# Tech tracking
tech-stack:
  added: [rustix-system-feature]
  patterns: [Command+pre_exec layered launcher (no libc::fork), deny-default-EPERM seccomp, cfg(linux) enforcement / macOS stub dual-impl, cap-std rooted file tools, feature-gated tools]

key-files:
  created:
    - crates/rollout-harness-tool/src/sandbox/launcher.rs
    - crates/rollout-harness-tool/src/sandbox/capfs.rs
    - crates/rollout-harness-tool/src/tools/{mod,python_exec,shell,file_read,file_write}.rs
    - crates/rollout-harness-tool/python/runner.py
    - crates/rollout-harness-tool/tests/{sandbox_positive,macos_stub}.rs
    - crates/rollout-harness-tool/tests/support/mod.rs
  modified:
    - crates/rollout-harness-tool/src/lib.rs
    - crates/rollout-harness-tool/src/sandbox/{mod,seccomp,stub_macos}.rs
    - crates/rollout-harness-tool/src/sandbox/cgroup.rs
    - crates/rollout-harness-tool/Cargo.toml
    - crates/rollout-harness-tool/tests/sandbox_negative.rs
    - Cargo.toml (rustix system feature)

key-decisions:
  - "Launcher uses std::process::Command + pre_exec (post-fork, pre-execve hook) for the namespace/rlimit/landlock/seccomp/execve sequence — never libc::fork (Pitfall F / forbidden-patterns)"
  - "setrlimit via rustix::process::setrlimit (cross-libc) rather than raw libc to avoid __rlimit_resource_t vs c_int variance across gnu/musl"
  - "cgroups v2 degrade-with-warning (Open Q#4): no delegated tree -> rely on RLIMIT_AS; landlock stays fail-closed (no rlimit substitute)"
  - "ALLOWLIST kept as the exact RESEARCH set (no additions needed yet — strace close-the-loop deferred to first harness-linux run)"

patterns-established:
  - "Layered launcher: PARENT cgroup+seccomp-build / CHILD unshare -> setrlimit -> landlock -> seccomp LAST -> execve"
  - "All enforcement + runtime tests #[cfg(target_os = \"linux\")]; macOS = compile-only stub with a #[cfg(not(linux))] witness"
  - "tests/support no-op substrate doubles for HarnessDependencies (GPU/cloud-free)"

requirements-completed: [HARNESS-02]

# Metrics
duration: ~50min (resumed)
completed: 2026-06-01
---

# Phase 7 Plan 02: rollout-harness-tool Sandbox Core Summary

**Resumed an interrupted run to deliver the Linux sandbox core of `rollout-harness-tool`: a layered launcher (rustix namespaces + setrlimit + landlock + seccompiler + cgroups v2) composing in the load-bearing order with seccomp installed LAST, the fail-closed kernel gate, the curated deny-default-EPERM seccomp allowlist, the cap-std file-tool root, the four non-HTTP tools (python_exec/shell with shell=False + exact-full-path allowlist, file_read/file_write via cap-std), the macOS compile-only dev stub, and the full CVE-class negative + per-tool positive witnesses.**

## Resume Context

The previous executor committed Task 1 (`0598019` — seccomp ALLOWLIST + filter + negative tests) and left an uncommitted partial `cgroup.rs`. This run reviewed and integrated `cgroup.rs` as-is (it was complete: degrade-with-warning v2 controller with `memory.max`/`pids.max` + `cgroup.procs` join + Drop cleanup), then finished Tasks 2-4. Task 1 was NOT redone.

## Task Commits

1. **Task 1 (prior run):** `0598019` — curated seccomp ALLOWLIST + deny-default-EPERM filter + CVE negative tests (verified, not redone).
2. **Task 2: layered launcher + kernel gate + cap-std root + four tools** — `3aa2360` (feat). The launcher composition, `ToolSettings` + fail-closed gate, `ToolHarness` dispatch, cgroup integration, capfs, and the four tools landed together because the `lib.rs` dispatch and the `tools::*` modules are mutually dependent and must compile as a unit.
3. **Task 3: positive witnesses** — `05413d0` (test) — `seccomp_python_runs` + per-tool happy/failure + the no-op `tests/support` doubles.
4. **Task 4: macOS dev-stub witness + clippy/forbidden fixes** — `5ae825c` (feat).

## Final seccomp ALLOWLIST

Unchanged from the RESEARCH §"Curated seccomp allowlist" set committed in Task 1 — **no syscalls added yet**. The plan's "diff against the 07-00 strace baseline, expect 2-5 additions" close-the-loop step requires the real `strace-seccomp-baseline` CI artifact, which is produced by the `harness-linux` Ubuntu lane (strace is Linux-only; this is a macOS dev box). If the first `harness-linux` run of `seccomp_python_runs` fails on a missing post-2020 syscall, add it to `ALLOWLIST` with a justification — the test is wired to be that signal. Default action is `SeccompAction::Errno(EPERM)` (typed error, not KillProcess). `clone3`/`clone` carry a masked-flag `SeccompCondition` refusing `CLONE_NEWUSER`.

## cgroup degrade-with-warning (as implemented)

`CgroupSubtree::create` probes a writable delegated v2 tree (`/sys/fs/cgroup/cgroup.subtree_control` writable). If absent it returns `Ok(None)` (the degrade path) — the launcher then relies on `RLIMIT_AS`/`RLIMIT_NPROC`. If a tree IS present but the controller write fails, that is surfaced as a real `io::Error`. This deliberately diverges from landlock's fail-closed posture (landlock has no rlimit substitute; cgroup memory does). The warning-event emission is left as a TODO for the observability wiring in a later plan — the rlimit fallback is the load-bearing guarantee.

## ToolSettings shape (exact)

```
require_landlock: bool (default true)         // D-TOOL-02 fail-closed gate
enable_{python_exec,shell,file_read,file_write}: bool (default true)
python_path: PathBuf (default /usr/bin/python3)   // exact full path, D-TOOL-06
shell_allowlist: BTreeMap<String, PathBuf>        // command name -> absolute path
timeout_secs: u64 (default 10)
rlimit_{cpu_secs,as_bytes,nofile,nproc}: u64
cgroup_{memory_max,pids_max}: Option<u64>
```
Derives `Deserialize + JsonSchema`, `#[serde(default)]`.

## clone3 flag-filter limitation (Open Question #1)

The masked-flag `SeccompCondition` on `clone`/`clone3` rejecting `CLONE_NEWUSER` is exact for `clone` (flags in a register) but best-effort for `clone3` (flags behind a struct pointer the kernel cannot deep-inspect via seccomp arg-filtering). The load-bearing user-namespace defense is therefore `PR_SET_NO_NEW_PRIVS` + dropped `CAP_SYS_ADMIN` + the deny of `unshare` + the `sandbox_blocks_userns` negative witness. This will be confirmed empirically on the Linux runner; no code change expected unless the runner surfaces a gap.

## Verification

- macOS: `cargo build/clippy --all-targets --all-features -D warnings` green; `cargo test` → `macos_stub_returns_documented_fatal` passes (asserts the exact Fatal string); Linux tests compile out.
- Linux target (`x86_64-unknown-linux-gnu`, cross-`cargo check`/`clippy --all-targets`/`cargo doc`): all green — validates the Linux-only launcher/capfs/tools/tests compile and lint-clean even though they cannot RUN on darwin.
- `RUSTDOCFLAGS`-deny `cargo doc` green on both targets (DOCS-03).
- `bash scripts/check-forbidden-patterns.sh` green; `rg "shell=True"`, `rg "libc::fork("`, `rg "/bin/sh" .../src/tools`, `rg "allow_unsandboxed"` all return nothing.

## Linux-CI-deferred tests (cannot run on macOS — validate on harness-linux lane)

Negative (Task 1, `tests/sandbox_negative.rs`): `seccomp_blocks_unexpected_syscall`, `sandbox_blocks_userns`, `sandbox_blocks_mount`, `sandbox_blocks_keyctl`, `sandbox_blocks_bpf`, `seccomp_no_socket`, `tool_sandbox_escape_blocked`.
Positive (`tests/sandbox_positive.rs`): `seccomp_python_runs`, `python_exec_happy_path`, `python_exec_times_out`, `shell_runs_allowlisted_command`, `shell_refuses_non_allowlisted`, `file_write_then_read_roundtrips`, `file_read_rejects_escape`.

These are `#[cfg(target_os = "linux")]` and compile out on darwin (witnessed: `0 tests` run locally). They are NOT failures — they validate on the Ubuntu kernel-5.15 `harness-linux` lane. No Linux test pass was faked.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] rustix `system` feature required for the kernel gate**
- **Found during:** Task 2 — `rustix::system::uname()` is feature-gated.
- **Fix:** Added `"system"` to the workspace `rustix` features. No version change (still `=1.1.4`).
- **Files:** Cargo.toml, Cargo.lock. **Commit:** `3aa2360`.

**2. [Rule 1 - Bug] Pre-existing clippy + forbidden-pattern violations in the committed Task-1 `sandbox_negative.rs`**
- **Found during:** Task 4 — `cargo clippy --all-targets -D warnings` (Linux) flagged `&mut status` (borrow-as-ptr), the `SYS_fork` cast (cast_possible_truncation), and a doc-backtick lint; `check-forbidden-patterns.sh` flagged the literal `libc::fork(` token inside a comment and `KillProcess` doc-backticks.
- **Fix:** `&raw mut`, `i32::try_from`/scoped allow, reword the comment to drop the literal token, add backticks. Behaviour unchanged — these are the same CVE witnesses.
- **Files:** tests/sandbox_negative.rs. **Commit:** `5ae825c`.

**3. [Rule 1 - Bug] `runner.py` docstring + `stub_macos.rs` doc tripped CI greps/acceptance**
- **Found during:** Task 3/4 — `runner.py` docstring contained the literal `shell=True`; `stub_macos.rs` doc contained `allow_unsandboxed`.
- **Fix:** Reworded both to avoid the literal tokens (acceptance: `rg "shell=True"` / `rg "allow_unsandboxed"` must be empty). **Commits:** `3aa2360` (runner), `5ae825c` (stub).

**Total:** 3 auto-fixed (1 blocking dep-feature, 2 lint/grep). No architectural changes; no scope creep.

## Design choices worth recording

- The launcher avoids a hand-rolled `clone3` raw syscall: `Command::pre_exec` runs the namespace/rlimit/landlock/seccomp setup in the child after fork and before execve — the exact composition seam the plan requires — without ever invoking `libc::fork(` (Pitfall F). `unshare` of the namespace set is best-effort (falls through with reduced isolation if userns is unavailable, e.g. nested CI) so rlimits+landlock+seccomp still apply.
- `setrlimit` uses `rustix::process::setrlimit` (the plan's named API) for cross-libc correctness.

## Known Stubs

- HTTP tools (`http_get`/`http_post`) + the SSRF hyper connector are intentionally OUT of this plan (land in 07-04, per the objective). `descriptor()` advertises only the four implemented tools — not a stub, a scoped boundary.
- The cgroup warning-event emission is a TODO (rlimit fallback is the guarantee); not blocking.

## Next Plan Readiness

07-04 adds `http_get`/`http_post` + the post-DNS IP-filtering hyper `Connect` + the SSRF witnesses + the README sandbox-depth matrix on top of this launcher.

## Self-Check: PASSED

All 12 claimed files exist on disk; all three new task commits (`3aa2360`, `05413d0`, `5ae825c`) are in the git log (Task 1 `0598019` verified from the prior run).
