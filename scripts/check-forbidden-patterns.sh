#!/usr/bin/env bash
# Source: PITFALLS.md §3 (IMDSv1) + §10a (shell=True) + §13 (libc::fork).
# Greps the workspace for hard-coded cloud metadata URLs and dangerous
# Python/Rust patterns. Each check has an allowed-paths regex.
set -euo pipefail

EXIT=0

check() {
    local label="$1"; shift
    local regex="$1"; shift
    local allowed_paths="$1"; shift
    local results
    # `git ls-files` lists tracked files only — avoids scanning target/ and node_modules/.
    results=$(git ls-files | grep -v -E "$allowed_paths" | xargs grep -nE "$regex" 2>/dev/null || true)
    if [ -n "$results" ]; then
        echo "FAIL [$label]:"
        echo "$results"
        echo ""
        EXIT=1
    fi
}

# IMDSv1 raw URL (PITFALLS §3): allowed only in the AWS IMDS module.
check "imds-aws-raw"     "169\.254\.169\.254"           '^(crates/rollout-cloud-aws/src/imds/|docs/|\.planning/|\.github/|scripts/check-forbidden-patterns\.sh)'

# GCP metadata raw URL (PITFALLS §3): allowed only in the GCP MDS module.
check "metadata-gcp-raw" "metadata\.google\.internal"   '^(crates/rollout-cloud-gcp/src/mds/|docs/|\.planning/|\.github/|scripts/check-forbidden-patterns\.sh)'

# Python shell=True (PITFALLS §10a): not allowed anywhere outside docs / planning.
check "shell-true"       "shell=True"                   '^(docs/|\.planning/|\.github/|tests/.*\.md$|scripts/check-forbidden-patterns\.sh)'

# libc::fork (PITFALLS §13): not allowed anywhere outside docs / planning.
check "libc-fork"        "libc::fork\("                 '^(docs/|\.planning/|\.github/|scripts/check-forbidden-patterns\.sh)'

if [ $EXIT -ne 0 ]; then
    echo ""
    echo "See .planning/research/PITFALLS.md for prevention details."
fi
exit $EXIT
