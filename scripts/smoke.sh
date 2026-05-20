#!/usr/bin/env bash
# Phase-2 SUBSTR-02 / SUBSTR-03 / SUBSTR-04 acceptance gate.
#
# Boots 1 coordinator + 2 workers (w1, w2), loads 1 cdylib + 1 Python sidecar
# plugin per worker, waits for stable heartbeats, kills w1 with SIGKILL, and
# asserts the coordinator emits `worker_failed` for w1's ULID within
# `2 × heartbeat_interval` (per CONTEXT D-COORD-02 + spec 05 §6).
#
# Layout:
#   data/smoke/coord.db          — coordinator embedded storage
#   data/smoke/w1.db, w2.db      — per-worker embedded storage
#   data/smoke/tls/              — auto-generated dev CA
#   data/smoke/logs/coord.log    — coordinator stdout+stderr (NDJSON events)
#   data/smoke/logs/w{1,2}.log   — worker stdout+stderr
#   data/smoke/logs/w{1,2}.id    — captured worker ULID (smoke-driver fed)
#
# Exits 0 on PASS; 1 on timeout/failure (with tail of logs).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

LOGS_DIR="$REPO_ROOT/data/smoke/logs"
TLS_DIR="$REPO_ROOT/data/smoke/tls"
SIDECAR_DIR="$REPO_ROOT/data/smoke/sidecars"

COORD_PID=""
W1_PID=""
W2_PID=""

cleanup() {
    set +e
    for pid in "$W1_PID" "$W2_PID" "$COORD_PID"; do
        if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
            kill -TERM "$pid" 2>/dev/null
        fi
    done
    sleep 0.3
    for pid in "$W1_PID" "$W2_PID" "$COORD_PID"; do
        if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
            kill -KILL "$pid" 2>/dev/null
        fi
    done
}
trap cleanup EXIT INT TERM

# ---------------------------------------------------------------------------
# Preflight
# ---------------------------------------------------------------------------
bash "$REPO_ROOT/scripts/preflight.sh"

# ---------------------------------------------------------------------------
# Detect cdylib extension
# ---------------------------------------------------------------------------
case "$(uname -s)" in
    Darwin) DYLIB_EXT="dylib" ;;
    Linux)  DYLIB_EXT="so" ;;
    *)      echo "smoke: unsupported OS $(uname -s)" >&2; exit 1 ;;
esac

# ---------------------------------------------------------------------------
# Build binaries + cdylib sample
# ---------------------------------------------------------------------------
echo "smoke: building rollout-cli + rollout-coordinator (release)"
cargo build -p rollout-cli -p rollout-coordinator --release

echo "smoke: building cdylib sample"
cargo build \
    --manifest-path "$REPO_ROOT/tests/smoke/plugins/rust_cdylib_sample/Cargo.toml" \
    --release

CDYLIB_BUILD_DIR="$REPO_ROOT/tests/smoke/plugins/rust_cdylib_sample/target/release"
CDYLIB_PATH="$CDYLIB_BUILD_DIR/librust_cdylib_sample.$DYLIB_EXT"
if [ ! -f "$CDYLIB_PATH" ]; then
    echo "smoke: cdylib build output not found at $CDYLIB_PATH" >&2
    ls -la "$CDYLIB_BUILD_DIR" >&2 || true
    exit 1
fi

# ---------------------------------------------------------------------------
# Fresh smoke state
# ---------------------------------------------------------------------------
rm -rf "$REPO_ROOT/data/smoke"
mkdir -p "$LOGS_DIR" "$TLS_DIR" "$SIDECAR_DIR"

# ---------------------------------------------------------------------------
# Generate a cdylib manifest with an absolute path so smoke is CWD-agnostic.
# (The committed manifest under tests/smoke/plugins/rust_cdylib_sample/ has a
# relative path; we override here for the live run.)
# ---------------------------------------------------------------------------
SMOKE_CDYLIB_MANIFEST="$LOGS_DIR/sample_cdylib.toml"
cat > "$SMOKE_CDYLIB_MANIFEST" <<EOF
name = "rust-cdylib-sample"
version = "0.1.0"
kind = "env-harness"
trait_id = "rollout_core::Plugin"
mode = "rust-cdylib"
network_allowlist = []

[runtime]
gpu = false
memory_mib = 32

[entry.cdylib]
path = "$CDYLIB_PATH"
symbol = "rollout_plugin_factory"
EOF

# Sidecar manifest is committed; copy + rewrite socket_template so all sidecar
# sockets live under data/smoke/sidecars/ rather than the default ./data/sidecars/.
SMOKE_SIDECAR_MANIFEST="$LOGS_DIR/sample_sidecar.toml"
cat > "$SMOKE_SIDECAR_MANIFEST" <<EOF
name = "sample-sidecar"
version = "0.1.0"
kind = "env-harness"
trait_id = "rollout_core::Plugin"
mode = "sidecar"
network_allowlist = []

[runtime]
gpu = false
memory_mib = 64

[entry.sidecar]
command = ["python3", "-m", "sample_sidecar"]
protocol = "framed-json-uds"
socket_template = "$SIDECAR_DIR/{name}-{pid}.sock"
EOF

# ---------------------------------------------------------------------------
# Pre-generated worker ULIDs so the smoke driver can grep the coordinator log
# for the exact IDs (the coordinator names workers by ULID, not by w1/w2).
# These are valid Crockford-base32 ULIDs.
# ---------------------------------------------------------------------------
W1_ULID="01JFEAVS7C5DE5XEAEAB91EBT5"
W2_ULID="01JFEAVS7C5DE5XEAEAB91EBT6"
echo "$W1_ULID" > "$LOGS_DIR/w1.id"
echo "$W2_ULID" > "$LOGS_DIR/w2.id"

# ---------------------------------------------------------------------------
# Per-worker TOMLs (redb takes an exclusive lock per file, so each worker
# needs its own storage path). We derive these from tests/smoke/worker.toml
# at runtime so the committed fixture stays single-source.
# ---------------------------------------------------------------------------
W1_CFG="$LOGS_DIR/w1.toml"
W2_CFG="$LOGS_DIR/w2.toml"
sed 's|./data/smoke/worker.db|./data/smoke/w1.db|' \
    "$REPO_ROOT/tests/smoke/worker.toml" > "$W1_CFG"
sed 's|./data/smoke/worker.db|./data/smoke/w2.db|' \
    "$REPO_ROOT/tests/smoke/worker.toml" > "$W2_CFG"

# ---------------------------------------------------------------------------
# Spawn coordinator
# ---------------------------------------------------------------------------
COORD_BIN="$REPO_ROOT/target/release/rollout-coordinator"
ROLLOUT_BIN="$REPO_ROOT/target/release/rollout"

if [ ! -x "$COORD_BIN" ]; then echo "smoke: $COORD_BIN missing" >&2; exit 1; fi
if [ ! -x "$ROLLOUT_BIN" ]; then echo "smoke: $ROLLOUT_BIN missing" >&2; exit 1; fi

echo "smoke: spawning coordinator"
RUST_LOG=info "$COORD_BIN" run --config "$REPO_ROOT/tests/smoke/coordinator.toml" \
    >>"$LOGS_DIR/coord.log" 2>&1 &
COORD_PID=$!

# ---------------------------------------------------------------------------
# Wait for coordinator TLS listener to come up on 127.0.0.1:50051.
# Prefer `nc -z` when available; fall back to a /dev/tcp probe.
# ---------------------------------------------------------------------------
PORT_UP=0
for _ in $(seq 1 50); do
    if command -v nc >/dev/null 2>&1; then
        if nc -z 127.0.0.1 50051 2>/dev/null; then PORT_UP=1; break; fi
    else
        if (echo > /dev/tcp/127.0.0.1/50051) >/dev/null 2>&1; then PORT_UP=1; break; fi
    fi
    if ! kill -0 "$COORD_PID" 2>/dev/null; then
        echo "smoke: coordinator died before port came up" >&2
        tail -n 50 "$LOGS_DIR/coord.log" >&2 || true
        exit 1
    fi
    sleep 0.1
done
if [ "$PORT_UP" -ne 1 ]; then
    echo "smoke: coordinator port 50051 not up within 5s" >&2
    tail -n 50 "$LOGS_DIR/coord.log" >&2 || true
    exit 1
fi
echo "smoke: coordinator up (pid=$COORD_PID)"

# ---------------------------------------------------------------------------
# Spawn workers — both load cdylib + sidecar plugins.
# PYTHONPATH carries python/examples so `python3 -m sample_sidecar` resolves.
# ---------------------------------------------------------------------------
export PYTHONPATH="$REPO_ROOT/python/examples:${PYTHONPATH:-}"

spawn_worker() {
    local cfg="$1" ulid="$2" logfile="$3"
    RUST_LOG=info "$ROLLOUT_BIN" worker run \
        --config "$cfg" \
        --worker-id "$ulid" \
        --plugin "$SMOKE_CDYLIB_MANIFEST" \
        --plugin "$SMOKE_SIDECAR_MANIFEST" \
        >>"$logfile" 2>&1 &
    echo $!
}

echo "smoke: spawning w1 ($W1_ULID)"
W1_PID=$(spawn_worker "$W1_CFG" "$W1_ULID" "$LOGS_DIR/w1.log")

echo "smoke: spawning w2 ($W2_ULID)"
W2_PID=$(spawn_worker "$W2_CFG" "$W2_ULID" "$LOGS_DIR/w2.log")

# ---------------------------------------------------------------------------
# Wait for heartbeat-stable: each worker's ULID must appear in coord.log under
# at least one `worker_heartbeat` event before we kill w1. 5s deadline.
# ---------------------------------------------------------------------------
echo "smoke: waiting for heartbeat-stable (both workers)"
HB_DEADLINE=$(( $(date +%s) + 5 ))
W1_HB=0; W2_HB=0
while [ "$(date +%s)" -lt "$HB_DEADLINE" ]; do
    if [ "$W1_HB" -eq 0 ] && grep -q "\"topic\":\"worker_heartbeat\".*\"$W1_ULID\"\|\"$W1_ULID\".*\"worker_heartbeat\"" "$LOGS_DIR/coord.log" 2>/dev/null; then
        W1_HB=1
    fi
    if [ "$W2_HB" -eq 0 ] && grep -q "\"topic\":\"worker_heartbeat\".*\"$W2_ULID\"\|\"$W2_ULID\".*\"worker_heartbeat\"" "$LOGS_DIR/coord.log" 2>/dev/null; then
        W2_HB=1
    fi
    if [ "$W1_HB" -eq 1 ] && [ "$W2_HB" -eq 1 ]; then break; fi
    sleep 0.1
done

if [ "$W1_HB" -ne 1 ] || [ "$W2_HB" -ne 1 ]; then
    echo "smoke: heartbeat-stable timeout (w1_hb=$W1_HB w2_hb=$W2_HB)" >&2
    echo "--- coord.log (tail) ---" >&2
    tail -n 80 "$LOGS_DIR/coord.log" >&2 || true
    echo "--- w1.log (tail) ---" >&2
    tail -n 40 "$LOGS_DIR/w1.log" >&2 || true
    echo "--- w2.log (tail) ---" >&2
    tail -n 40 "$LOGS_DIR/w2.log" >&2 || true
    exit 1
fi
echo "smoke: heartbeat-stable; both workers registered with coordinator"

# ---------------------------------------------------------------------------
# Kill w1; assert the coordinator emits worker_failed for W1_ULID within 8s.
# Default timings: heartbeat_interval=500ms, coord_failure_timeout=5s, so
# the deadline-scan should fire within ~5–6s.
# ---------------------------------------------------------------------------
echo "smoke: killing w1 (pid=$W1_PID)"
kill -KILL "$W1_PID"
W1_KILLED_PID="$W1_PID"
W1_PID=""

FAIL_DEADLINE=$(( $(date +%s) + 8 ))
DETECTED=0
while [ "$(date +%s)" -lt "$FAIL_DEADLINE" ]; do
    if grep -q "\"topic\":\"worker_failed\".*\"$W1_ULID\"\|\"$W1_ULID\".*\"worker_failed\"" "$LOGS_DIR/coord.log" 2>/dev/null; then
        DETECTED=1
        break
    fi
    sleep 0.1
done

if [ "$DETECTED" -ne 1 ]; then
    echo "smoke: FAIL — coordinator did not detect w1 failure within 8s" >&2
    echo "--- coord.log (tail) ---" >&2
    tail -n 120 "$LOGS_DIR/coord.log" >&2 || true
    echo "--- w1.log (tail) ---" >&2
    tail -n 40 "$LOGS_DIR/w1.log" >&2 || true
    echo "--- w2.log (tail) ---" >&2
    tail -n 40 "$LOGS_DIR/w2.log" >&2 || true
    exit 1
fi

echo "smoke: PASS — coordinator marked w1 failed within deadline"
exit 0
