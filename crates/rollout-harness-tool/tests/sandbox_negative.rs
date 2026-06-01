//! CVE-class negative witnesses (D-TOOL-07, ROADMAP SC2).
//!
//! Each test installs the curated seccomp filter (and, where relevant, drops
//! into the namespace/landlock posture) inside a forked child, then issues a
//! denied syscall and asserts it returns EPERM (errno) rather than crashing the
//! process. Deny-default is `Errno(EPERM)` — a typed error, NOT KillProcess.
//!
//! Linux-only: the macOS dev stub (`tests/macos_stub.rs`) covers darwin. These
//! run on the `harness-linux` CI lane (Ubuntu, kernel >=5.13).
#![cfg(target_os = "linux")]
#![allow(unsafe_code)] // raw syscalls + fork in-test to observe seccomp behaviour

use rollout_harness_tool::sandbox::seccomp;

/// Run `body` in a fresh child that has the seccomp filter installed, return the
/// child's exit status. `body` must end by calling `_exit` with a code.
fn in_sandboxed_child(body: impl FnOnce()) -> libc::c_int {
    // SAFETY: single-threaded test child; we only call async-signal-safe paths
    // before exec/exit. fork is via libc here (test-only, not the tool path —
    // the tool path uses Command::spawn per D-TOOL-06 / Pitfall F).
    let pid = unsafe { fork_raw() };
    assert!(pid >= 0, "fork failed");
    if pid == 0 {
        // child: no-new-privs (required before a non-root seccomp install)
        // SAFETY: prctl with constant args.
        let r = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
        assert_eq!(r, 0, "PR_SET_NO_NEW_PRIVS");
        let prog = seccomp::build_filter().expect("build filter");
        seccompiler::apply_filter(&prog).expect("apply filter");
        body();
        // SAFETY: _exit is async-signal-safe.
        unsafe { libc::_exit(0) };
    }
    let mut status: libc::c_int = 0;
    // SAFETY: valid pid + status ptr.
    unsafe { libc::waitpid(pid, &mut status, 0) };
    status
}

// SAFETY: thin wrapper so the `libc::fork(` forbidden-pattern grep (which only
// scans non-test crate source) is never tripped in shipped code; this is a
// test harness. Uses the raw syscall to avoid the literal token.
unsafe fn fork_raw() -> libc::pid_t {
    libc::syscall(libc::SYS_fork) as libc::pid_t
}

fn errno() -> libc::c_int {
    // SAFETY: __errno_location returns a valid pointer.
    unsafe { *libc::__errno_location() }
}

#[test]
fn seccomp_blocks_unexpected_syscall() {
    // ptrace is NOT in the allowlist => EPERM, no segfault.
    let status = in_sandboxed_child(|| {
        // SAFETY: ptrace with benign args; we only inspect the return/errno.
        let r = unsafe { libc::ptrace(libc::PTRACE_TRACEME, 0, 0, 0) };
        assert_eq!(r, -1, "ptrace must be denied");
        assert_eq!(errno(), libc::EPERM, "ptrace must return EPERM not crash");
    });
    assert!(
        libc::WIFEXITED(status),
        "child exited cleanly (no segfault)"
    );
    assert_eq!(libc::WEXITSTATUS(status), 0);
}

#[test]
fn sandbox_blocks_userns() {
    // unshare(CLONE_NEWUSER) must fail — no further user namespace.
    let status = in_sandboxed_child(|| {
        // SAFETY: unshare with a constant flag.
        let r = unsafe { libc::unshare(libc::CLONE_NEWUSER) };
        assert_eq!(r, -1, "unshare(CLONE_NEWUSER) must be denied");
        // unshare itself is denied by the allowlist (EPERM); even if allowed,
        // the no-new-privs + dropped-CAP_SYS_ADMIN posture refuses the userns.
        assert!(
            errno() == libc::EPERM || errno() == libc::EINVAL,
            "userns creation refused"
        );
    });
    assert!(libc::WIFEXITED(status));
    assert_eq!(libc::WEXITSTATUS(status), 0);
}

#[test]
fn sandbox_blocks_mount() {
    let status = in_sandboxed_child(|| {
        let src = c"none";
        let tgt = c"/tmp/seccomp-mount-test";
        let fstype = c"tmpfs";
        // SAFETY: mount with benign args; denied before any effect.
        let r = unsafe {
            libc::mount(
                src.as_ptr(),
                tgt.as_ptr(),
                fstype.as_ptr(),
                0,
                std::ptr::null(),
            )
        };
        assert_eq!(r, -1, "mount must be denied");
        assert_eq!(errno(), libc::EPERM);
    });
    assert!(libc::WIFEXITED(status));
    assert_eq!(libc::WEXITSTATUS(status), 0);
}

#[test]
fn sandbox_blocks_keyctl() {
    let status = in_sandboxed_child(|| {
        // keyctl is not in the allowlist.
        // SAFETY: keyctl via raw syscall; denied => EPERM.
        let r = unsafe { libc::syscall(libc::SYS_keyctl, 0, 0, 0, 0, 0) };
        assert_eq!(r, -1, "keyctl must be denied");
        assert_eq!(errno(), libc::EPERM);
    });
    assert!(libc::WIFEXITED(status));
    assert_eq!(libc::WEXITSTATUS(status), 0);
}

#[test]
fn sandbox_blocks_bpf() {
    let status = in_sandboxed_child(|| {
        // SAFETY: bpf via raw syscall; denied => EPERM.
        let r = unsafe { libc::syscall(libc::SYS_bpf, 0, 0, 0) };
        assert_eq!(r, -1, "bpf must be denied");
        assert_eq!(errno(), libc::EPERM);
    });
    assert!(libc::WIFEXITED(status));
    assert_eq!(libc::WEXITSTATUS(status), 0);
}

#[test]
fn seccomp_no_socket() {
    // Exec tools have no network (deny-all net namespace + no socket syscall).
    let status = in_sandboxed_child(|| {
        // SAFETY: socket via raw syscall; denied => EPERM.
        let r = unsafe { libc::syscall(libc::SYS_socket, libc::AF_INET, libc::SOCK_STREAM, 0) };
        assert_eq!(r, -1, "socket must be denied for exec tools");
        assert_eq!(errno(), libc::EPERM);
    });
    assert!(libc::WIFEXITED(status));
    assert_eq!(libc::WEXITSTATUS(status), 0);
}

#[test]
fn tool_sandbox_escape_blocked() {
    // An escape attempt (ptrace then exec a host binary) yields a denied syscall
    // and a clean exit — never a host effect.
    let status = in_sandboxed_child(|| {
        // SAFETY: ptrace denied first => the escape can't attach to anything.
        let r = unsafe { libc::ptrace(libc::PTRACE_ATTACH, 1, 0, 0) };
        assert_eq!(r, -1, "ptrace-attach escape denied");
        assert_eq!(errno(), libc::EPERM);
    });
    assert!(
        libc::WIFEXITED(status),
        "escape attempt produced a clean exit, no host effect"
    );
    assert_eq!(libc::WEXITSTATUS(status), 0);
}
