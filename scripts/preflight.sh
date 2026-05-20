#!/usr/bin/env bash
# Preflight check for Phase-2 substrate. Run before `make smoke`.
set -euo pipefail

fail() { echo "preflight FAIL: $*" >&2; exit 1; }

command -v cargo >/dev/null 2>&1 || fail "cargo not on PATH"
command -v make  >/dev/null 2>&1 || fail "make not on PATH"
command -v python3 >/dev/null 2>&1 || fail "python3 not on PATH (need >= 3.11 for sidecar sample)"

PY_VER=$(python3 -c 'import sys; print(f"{sys.version_info[0]}.{sys.version_info[1]}")')
PY_MAJ=${PY_VER%.*}; PY_MIN=${PY_VER#*.}
if [ "$PY_MAJ" -lt 3 ] || { [ "$PY_MAJ" -eq 3 ] && [ "$PY_MIN" -lt 11 ]; }; then
    fail "python3 $PY_VER detected; need >= 3.11"
fi

# protoc is preferred but tonic-build vendors it for most targets.
if ! command -v protoc >/dev/null 2>&1; then
    echo "preflight note: protoc not found on PATH; tonic-build bundles one but install protobuf-compiler if compilation fails" >&2
fi

echo "preflight OK: cargo $(cargo --version | awk '{print $2}'), python3 $PY_VER"
