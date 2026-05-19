#!/usr/bin/env bash
# scripts/check-docs-tests-touched.sh
# Enforces AGENTS.md §9.2 / DOCS-02:
#   Every commit modifying code under crates/, python/, or xtask/
#   must also touch docs/, tests/, or inline doc comments.
# Bypass via [skip-docs-check] trailer in the latest commit message.

set -euo pipefail

BASE="${BASE_SHA:-}"
HEAD="${HEAD_SHA:-HEAD}"

if [[ -z "$BASE" ]]; then
  echo "::error::BASE_SHA env var is required (PR base ref)."
  exit 2
fi

# Bypass: [skip-docs-check] in the most recent commit message on the PR head.
if git log -1 --format=%B "$HEAD" | grep -qF '[skip-docs-check]'; then
  echo "docs-test-policy: bypassed via [skip-docs-check] trailer"
  exit 0
fi

CHANGED_FILES=$(git diff --name-only "${BASE}...${HEAD}")

code_changed=false
docs_or_tests_changed=false

while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  case "$f" in
    crates/*|python/*|xtask/*)
      code_changed=true
      ;;
    docs/*|*/tests/*|tests/*)
      docs_or_tests_changed=true
      ;;
  esac
done <<< "$CHANGED_FILES"

if ! $code_changed; then
  echo "docs-test-policy: no code changes; nothing to enforce."
  exit 0
fi

if $docs_or_tests_changed; then
  echo "docs-test-policy: code change accompanied by docs/ or tests/ change."
  exit 0
fi

# Fallback: inline doc-comment edits in the diff hunks.
# git diff -U0 yields hunk lines prefixed with `+` or `-`; look for /// or //!.
if git diff -U0 "${BASE}...${HEAD}" -- 'crates/**' 'python/**' 'xtask/**' \
     | grep -qE '^\+.*(///|//!|""")'; then
  echo "docs-test-policy: code change accompanied by inline doc-comment edits."
  exit 0
fi

echo "::error::Code under crates/, python/, or xtask/ changed without accompanying docs/, tests/, or inline doc-comment changes. See AGENTS.md §9.2. To bypass for bootstrap or mechanical refactors, add '[skip-docs-check]' to the most recent commit message."
exit 1
