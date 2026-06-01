#!/usr/bin/env python3
"""Stdlib-only python_exec sidecar (D-TOOL-06, AGENTS.md §7).

The tool path execs ``python3 -I -c <code>`` directly (see tools/python_exec.rs),
so this helper is the optional framed-stdin runner kept for parity with the
plugin-host sidecar pattern. Stdlib only: NO third-party imports, NO shell
interpreter (argv vector, not a shell string), NO pip install — runs under the
curated seccomp allowlist.
"""
import json
import sys


def main() -> int:
    raw = sys.stdin.read()
    try:
        req = json.loads(raw) if raw else {}
    except json.JSONDecodeError as exc:
        json.dump({"ok": False, "error": f"bad request: {exc}"}, sys.stdout)
        return 2
    code = req.get("code", "")
    env: dict = {}
    try:
        exec(compile(code, "<python_exec>", "exec"), env)  # noqa: S102
    except Exception as exc:  # noqa: BLE001
        json.dump({"ok": False, "error": str(exc)}, sys.stdout)
        return 1
    json.dump({"ok": True}, sys.stdout)
    return 0


if __name__ == "__main__":
    sys.exit(main())
