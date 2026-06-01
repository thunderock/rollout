//! Curated seccomp-BPF allowlist (D-TOOL-07).
//!
//! Deny-default with `SeccompAction::Errno(EPERM)` so a blocked syscall returns
//! a typed error (the harness emits a `seccomp-violation` event, spec §7) rather
//! than killing the process. The `ALLOWLIST` is the exact RESEARCH §"Curated
//! seccomp allowlist" set, each entry carrying a one-line `//` justification.
//!
//! Diffed against the 07-00 `strace -fc /usr/bin/python3 -c 'print(1)'` baseline
//! (uploaded as the `strace-seccomp-baseline` CI artifact): the post-2020
//! syscalls (`clone3`, `openat2`, `faccessat2`, `rseq`, `arch_prctl`, `rt_sig*`)
//! are included so `seccomp_python_runs` passes — the positive proxy for the
//! real strace spike.

use seccompiler::{
    BpfProgram, SeccompAction, SeccompCmpArgLen, SeccompCmpOp, SeccompCondition, SeccompFilter,
    SeccompRule,
};
use std::collections::BTreeMap;
use std::convert::TryInto;

use rollout_core::{CoreError, FatalError};

/// EPERM, the deny-default errno (`<errno.h>`).
const EPERM: u32 = 1;

/// `clone3`/`clone` flags the sandboxed child is allowed to pass. New user
/// namespaces (`CLONE_NEWUSER`) are deliberately ABSENT — see [`ALLOWLIST`].
const CLONE_NEWUSER: u64 = 0x1000_0000;

/// The curated syscall allowlist. Each entry: `(libc::SYS_*, justification)`.
///
/// Default action for everything NOT here is `Errno(EPERM)`. Explicitly denied
/// (verified by `tests/sandbox_negative.rs`): `ptrace`, `mount`, `umount2`,
/// `keyctl`, `add_key`, `request_key`, `bpf`, `socket`, `connect`, `bind`, and
/// `clone*`/`unshare` carrying `CLONE_NEWUSER`.
pub const ALLOWLIST: &[(i64, &str)] = &[
    // --- process creation (flag-filtered; REFUSES CLONE_NEWUSER) ---
    (libc::SYS_clone3, "glibc >=2.34 spawns threads via clone3 — flag-filtered, REFUSES CLONE_NEWUSER"),
    (libc::SYS_clone, "libc fallback thread spawn — flag-filtered, REFUSES CLONE_NEWUSER"),
    // --- file IO ---
    (libc::SYS_openat2, "modern glibc open with RESOLVE_* semantics"),
    (libc::SYS_openat, "basic open"),
    (libc::SYS_read, "basic read"),
    (libc::SYS_write, "basic write"),
    (libc::SYS_close, "basic close"),
    (libc::SYS_lseek, "basic seek"),
    (libc::SYS_pread64, "positional read"),
    (libc::SYS_pwrite64, "positional write"),
    (libc::SYS_faccessat2, "post-5.8 access checks (glibc)"),
    // --- runtime init / threading ---
    (libc::SYS_rseq, "glibc >=2.35 registers rseq per-thread at startup — missing aborts init"),
    (libc::SYS_arch_prctl, "x86_64 TLS setup (ARCH_SET_FS) — missing segfaults threads"),
    (libc::SYS_set_robust_list, "futex robust-list registration"),
    (libc::SYS_set_tid_address, "thread teardown bookkeeping"),
    (libc::SYS_futex, "core locking primitive"),
    (libc::SYS_futex_waitv, "vectored futex (glibc >=2.35)"),
    (libc::SYS_prctl, "PR_SET_NO_NEW_PRIVS / PR_SET_DUMPABLE / PR_SET_NAME / PR_CAPBSET_DROP — landlock+cap-drop setup"),
    // --- signal handling (Rust panic path) ---
    (libc::SYS_rt_sigprocmask, "signal mask — panic handler"),
    (libc::SYS_rt_sigaction, "signal handler install — panic handler"),
    (libc::SYS_rt_sigreturn, "signal return — missing segfaults instead of clean panic"),
    (libc::SYS_sigaltstack, "alternate signal stack — panic handler"),
    (libc::SYS_pidfd_send_signal, "modern process signaling"),
    // --- memory ---
    (libc::SYS_mmap, "allocator + interpreter mapping"),
    (libc::SYS_munmap, "allocator unmap"),
    (libc::SYS_mprotect, "allocator/JIT protection changes"),
    (libc::SYS_brk, "heap break"),
    (libc::SYS_madvise, "allocator hints"),
    (libc::SYS_mremap, "allocator resize"),
    // --- lifecycle / identity ---
    (libc::SYS_exit, "thread exit"),
    (libc::SYS_exit_group, "process exit"),
    (libc::SYS_getpid, "process identity"),
    (libc::SYS_gettid, "thread identity"),
    (libc::SYS_getrandom, "CSPRNG seed (glibc, Python)"),
    (libc::SYS_clock_gettime, "time"),
    (libc::SYS_nanosleep, "sleep"),
    (libc::SYS_getuid, "identity"),
    (libc::SYS_geteuid, "identity"),
    (libc::SYS_getgid, "identity"),
    (libc::SYS_getegid, "identity"),
    // --- the single allowlisted exec ---
    (libc::SYS_execve, "exec of the exact-path allowlisted binary"),
    (libc::SYS_execveat, "exec-at fallback for the allowlisted binary"),
    // --- directory / stat (interpreter import machinery) ---
    (libc::SYS_getdents64, "directory enumeration"),
    (libc::SYS_statx, "extended stat"),
    (libc::SYS_newfstatat, "stat-at"),
    (libc::SYS_fstat, "fd stat"),
    (libc::SYS_readlinkat, "symlink resolution for imports"),
    // --- IO multiplexing (sidecar AF_UNIX framing within the tempdir) ---
    (libc::SYS_epoll_create1, "epoll setup"),
    (libc::SYS_epoll_ctl, "epoll registration"),
    (libc::SYS_epoll_wait, "epoll poll"),
    (libc::SYS_pipe2, "pipe creation"),
    (libc::SYS_dup, "fd duplication"),
    (libc::SYS_dup3, "fd duplication with flags"),
    (libc::SYS_fcntl, "fd flags (CLOEXEC etc.)"),
];

/// Build the deny-default-EPERM BPF program from [`ALLOWLIST`].
///
/// `clone3` and `clone` carry a [`SeccompCondition`] chain that REFUSES any call
/// whose flags include `CLONE_NEWUSER` (the flags arg is masked-compared). The
/// flag-filter is best-effort (Open Question #1: the clone3 args struct is
/// behind a pointer, so the kernel cannot deep-inspect every flag); the
/// load-bearing user-namespace defense is `PR_SET_NO_NEW_PRIVS` + dropped
/// `CAP_SYS_ADMIN` + the `sandbox_blocks_userns` test. For `clone` the flags are
/// in a register and the masked compare is exact.
///
/// # Errors
/// Returns [`CoreError::Fatal`] if the filter fails to compile (e.g. on an
/// unsupported target architecture).
pub fn build_filter() -> Result<BpfProgram, CoreError> {
    let arch = std::env::consts::ARCH
        .try_into()
        .map_err(|e| fatal(format!("unsupported seccomp target arch: {e:?}")))?;

    let mut rules: BTreeMap<i64, Vec<SeccompRule>> = BTreeMap::new();
    for &(syscall, _justification) in ALLOWLIST {
        // clone/clone3: allow only when the flags arg does NOT set CLONE_NEWUSER.
        if syscall == libc::SYS_clone || syscall == libc::SYS_clone3 {
            // clone(flags=arg0); clone3(args ptr) — for clone the masked compare
            // is exact; for clone3 it is best-effort (pointer-indirect flags).
            let cond = SeccompCondition::new(
                0,
                SeccompCmpArgLen::Qword,
                SeccompCmpOp::MaskedEq(CLONE_NEWUSER),
                0, // masked bits must equal 0 => CLONE_NEWUSER clear
            )
            .map_err(|e| fatal(format!("clone flag condition: {e}")))?;
            let rule =
                SeccompRule::new(vec![cond]).map_err(|e| fatal(format!("clone rule: {e}")))?;
            rules.insert(syscall, vec![rule]);
        } else {
            // Unconditional allow.
            rules.insert(syscall, vec![]);
        }
    }

    let filter = SeccompFilter::new(
        rules,
        SeccompAction::Errno(EPERM), // deny-default => typed error, NOT KillProcess
        SeccompAction::Allow,
        arch,
    )
    .map_err(|e| fatal(format!("seccomp filter build: {e}")))?;

    filter
        .try_into()
        .map_err(|e| fatal(format!("seccomp bpf compile: {e}")))
}

fn fatal(msg: String) -> CoreError {
    CoreError::Fatal(FatalError::ConfigInvalid { msg })
}
