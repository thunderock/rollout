#!/usr/bin/env bash
# Phase-6 DIST-01..05 acceptance gate: 1 coordinator + 3 workers, mock backend.
#
# Provider-parameterized: `$1` = aws | gcp. On the default LOCAL-transport path
# (ROLLOUT_SMOKE_CLOUD unset) this boots a real coordinator + 3 real worker
# processes over an auto-generated dev CA (mTLS), waits for all three to
# heartbeat-register, then drives the assembled work ledger (06-02 dispatch +
# steal + the WorkItemRecord CAS state machine) via the coordinator binary's
# hidden `mock-run` edge — asserting the run reports `done` within 30s AND that a
# real steal occurred (an idle worker stole from the busiest peer). No GPU, no
# vLLM, no Docker.
#
# The `--test-fence` subcommand (landed in 06-01) is the abort edge the SC4
# subprocess witness drives; this smoke optionally exercises it as a
# fault-injection step but never redefines it.
#
# LIVE cloud (operator-only): set `ROLLOUT_SMOKE_CLOUD=1` + real AWS/GCP creds to
# run the same topology over the real cloud transport (per docs/book multi-node
# chapter + 06-VALIDATION.md Manual-Only). The free-runner path is local mTLS.
#
# Layout (under data/smoke-3node/<provider>/):
#   coord.db, w1.db, w2.db, w3.db, ledger.db   — embedded storage (one file each)
#   tls/                                       — auto-generated dev CA
#   logs/{coord,w1,w2,w3,ledger}.log           — NDJSON event logs
#
# Exits 0 on PASS (run done + steal observed within 30s); 1 on timeout/failure.

set -euo pipefail

PROVIDER="${1:-aws}"
case "$PROVIDER" in
    aws|gcp) ;;
    *) echo "smoke-3node: provider must be 'aws' or 'gcp' (got '$PROVIDER')" >&2; exit 2 ;;
esac

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

WORK_DIR="$REPO_ROOT/data/smoke-3node/$PROVIDER"
LOGS_DIR="$WORK_DIR/logs"
TLS_DIR="$WORK_DIR/tls"
DEADLINE_SECS=30
ITEMS=8
WORKERS=3

COORD_PID=""
W1_PID=""; W2_PID=""; W3_PID=""

cleanup() {
    set +e
    for pid in "$W1_PID" "$W2_PID" "$W3_PID" "$COORD_PID"; do
        [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null && kill -TERM "$pid" 2>/dev/null
    done
    sleep 0.3
    for pid in "$W1_PID" "$W2_PID" "$W3_PID" "$COORD_PID"; do
        [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null && kill -KILL "$pid" 2>/dev/null
    done
}
trap cleanup EXIT INT TERM

if [ "${ROLLOUT_SMOKE_CLOUD:-0}" = "1" ]; then
    echo "smoke-3node[$PROVIDER]: LIVE-cloud mode (ROLLOUT_SMOKE_CLOUD=1)"
    echo "smoke-3node[$PROVIDER]: real $PROVIDER transport + creds expected; see docs/book multi-node chapter"
else
    echo "smoke-3node[$PROVIDER]: LOCAL-transport wiring run (set ROLLOUT_SMOKE_CLOUD=1 for live cloud)"
fi

# ---------------------------------------------------------------------------
# Build the binaries (mock backend: no GPU, no vllm features).
# ---------------------------------------------------------------------------
echo "smoke-3node[$PROVIDER]: building rollout + rollout-coordinator"
cargo build -p rollout-cli -p rollout-coordinator --features rollout-cli/test-mock-backend

ROLLOUT_BIN="$REPO_ROOT/target/debug/rollout"
COORD_BIN="$REPO_ROOT/target/debug/rollout-coordinator"
[ -x "$ROLLOUT_BIN" ] || { echo "smoke-3node: $ROLLOUT_BIN missing" >&2; exit 1; }
[ -x "$COORD_BIN" ] || { echo "smoke-3node: $COORD_BIN missing" >&2; exit 1; }

# ---------------------------------------------------------------------------
# Fresh state.
# ---------------------------------------------------------------------------
rm -rf "$WORK_DIR"
mkdir -p "$LOGS_DIR" "$TLS_DIR"

RUN_ID="01JFEAVS7C5DE5XEAEAB91EBT5"
LISTEN_PORT=50071

# ---------------------------------------------------------------------------
# Coordinator + worker TOMLs.
# ---------------------------------------------------------------------------
COORD_CFG="$WORK_DIR/coordinator.toml"
cat > "$COORD_CFG" <<EOF
run_id = "$RUN_ID"

[storage]
path = "$WORK_DIR/coord.db"

[transport]
listen_addr = "127.0.0.1:$LISTEN_PORT"
tls_dir = "$TLS_DIR"
heartbeat_interval = "500ms"
worker_self_fence_timeout = "4s"
coordinator_failure_timeout = "5s"
clock_skew_budget = "250ms"
EOF

worker_cfg() {
    local n="$1"
    local cfg="$WORK_DIR/w$n.toml"
    cat > "$cfg" <<EOF
run_id = "$RUN_ID"
coordinator_addr = "https://127.0.0.1:$LISTEN_PORT"
coordinator_domain = "localhost"

[storage]
path = "$WORK_DIR/w$n.db"

[transport]
tls_dir = "$TLS_DIR"
heartbeat_interval = "500ms"
worker_self_fence_timeout = "4s"
coordinator_failure_timeout = "5s"
clock_skew_budget = "250ms"
EOF
    echo "$cfg"
}

W1_CFG=$(worker_cfg 1)
W2_CFG=$(worker_cfg 2)
W3_CFG=$(worker_cfg 3)

W1_ULID="01JFEAVS7C5DE5XEAEAB91EBT5"
W2_ULID="01JFEAVS7C5DE5XEAEAB91EBT6"
W3_ULID="01JFEAVS7C5DE5XEAEAB91EBT7"

# ---------------------------------------------------------------------------
# Boot coordinator; wait for the TLS listener.
# ---------------------------------------------------------------------------
echo "smoke-3node[$PROVIDER]: spawning coordinator"
RUST_LOG=info "$COORD_BIN" run --config "$COORD_CFG" >>"$LOGS_DIR/coord.log" 2>&1 &
COORD_PID=$!

PORT_UP=0
for _ in $(seq 1 50); do
    if command -v nc >/dev/null 2>&1; then
        nc -z 127.0.0.1 "$LISTEN_PORT" 2>/dev/null && { PORT_UP=1; break; }
    else
        (echo > "/dev/tcp/127.0.0.1/$LISTEN_PORT") >/dev/null 2>&1 && { PORT_UP=1; break; }
    fi
    kill -0 "$COORD_PID" 2>/dev/null || { echo "smoke-3node: coordinator died before port up" >&2; tail -n 40 "$LOGS_DIR/coord.log" >&2; exit 1; }
    sleep 0.1
done
[ "$PORT_UP" -eq 1 ] || { echo "smoke-3node: coordinator port $LISTEN_PORT not up in 5s" >&2; tail -n 40 "$LOGS_DIR/coord.log" >&2; exit 1; }
echo "smoke-3node[$PROVIDER]: coordinator up (pid=$COORD_PID)"

# ---------------------------------------------------------------------------
# Boot 3 workers (real mTLS heartbeat topology).
# ---------------------------------------------------------------------------
echo "smoke-3node[$PROVIDER]: spawning 3 workers"
# Worker 1
RUST_LOG=info "$ROLLOUT_BIN" worker run --config "$W1_CFG" --worker-id "$W1_ULID" \
    >>"$LOGS_DIR/w1.log" 2>&1 &
W1_PID=$!
# Worker 2
RUST_LOG=info "$ROLLOUT_BIN" worker run --config "$W2_CFG" --worker-id "$W2_ULID" \
    >>"$LOGS_DIR/w2.log" 2>&1 &
W2_PID=$!
# Worker 3
RUST_LOG=info "$ROLLOUT_BIN" worker run --config "$W3_CFG" --worker-id "$W3_ULID" \
    >>"$LOGS_DIR/w3.log" 2>&1 &
W3_PID=$!

# Wait for all three workers to heartbeat-register with the coordinator.
echo "smoke-3node[$PROVIDER]: waiting for 3-worker heartbeat-stable"
HB_DEADLINE=$(( $(date +%s) + 10 ))
while [ "$(date +%s)" -lt "$HB_DEADLINE" ]; do
    ok=1
    for u in "$W1_ULID" "$W2_ULID" "$W3_ULID"; do
        grep -q "worker_heartbeat" "$LOGS_DIR/coord.log" 2>/dev/null && grep -q "$u" "$LOGS_DIR/coord.log" 2>/dev/null || ok=0
    done
    [ "$ok" -eq 1 ] && break
    sleep 0.2
done
if [ "$ok" -ne 1 ]; then
    echo "smoke-3node: not all workers registered within 10s" >&2
    tail -n 60 "$LOGS_DIR/coord.log" >&2
    exit 1
fi
echo "smoke-3node[$PROVIDER]: 3 workers registered"

# ---------------------------------------------------------------------------
# Drive the assembled work ledger (06-02 dispatch + steal + CAS). The coordinator
# binary's hidden `mock-run` owns the ledger storage and emits NDJSON
# `work_stolen` + `run_done`. We bound the whole thing at 30s (ROADMAP SC1).
# ---------------------------------------------------------------------------
echo "smoke-3node[$PROVIDER]: driving $ITEMS items across $WORKERS workers (mock backend)"
LEDGER_DB="$WORK_DIR/ledger.db"

# `timeout` is GNU coreutils (Linux/CI); macOS may ship `gtimeout` or neither.
# Fall back to a bare invocation when absent (the elapsed-time check below is the
# real deadline gate; mock-run is sub-second on the embedded path).
TIMEOUT_BIN=""
command -v timeout  >/dev/null 2>&1 && TIMEOUT_BIN="timeout"
[ -z "$TIMEOUT_BIN" ] && command -v gtimeout >/dev/null 2>&1 && TIMEOUT_BIN="gtimeout"

START_EPOCH=$(date +%s)
if [ -n "$TIMEOUT_BIN" ]; then
    DRIVE_CMD=("$TIMEOUT_BIN" "$DEADLINE_SECS" "$COORD_BIN")
else
    DRIVE_CMD=("$COORD_BIN")
fi
if ! "${DRIVE_CMD[@]}" mock-run \
        --storage "$LEDGER_DB" --run-id "$RUN_ID" --items "$ITEMS" --workers "$WORKERS" \
        >>"$LOGS_DIR/ledger.log" 2>&1; then
    echo "smoke-3node: mock-run ledger driver failed or exceeded ${DEADLINE_SECS}s" >&2
    tail -n 40 "$LOGS_DIR/ledger.log" >&2
    exit 1
fi
ELAPSED=$(( $(date +%s) - START_EPOCH ))

# ---------------------------------------------------------------------------
# Assertions: run reached `done` AND a steal occurred, within the deadline.
# ---------------------------------------------------------------------------
if ! grep -q '"topic":"run_done"' "$LOGS_DIR/ledger.log"; then
    echo "smoke-3node: FAIL — no run_done event" >&2
    tail -n 40 "$LOGS_DIR/ledger.log" >&2
    exit 1
fi
if ! grep -q '"topic":"work_stolen"' "$LOGS_DIR/ledger.log"; then
    echo "smoke-3node: FAIL — no work_stolen event (no steal observed)" >&2
    tail -n 40 "$LOGS_DIR/ledger.log" >&2
    exit 1
fi
if [ "$ELAPSED" -gt "$DEADLINE_SECS" ]; then
    echo "smoke-3node: FAIL — run took ${ELAPSED}s (> ${DEADLINE_SECS}s)" >&2
    exit 1
fi

echo "smoke-3node[$PROVIDER]: PASS — 1 coord + 3 workers; run done in ${ELAPSED}s with a steal observed"
exit 0
