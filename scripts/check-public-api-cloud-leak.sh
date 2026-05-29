#!/usr/bin/env bash
# Source: PITFALLS.md §1 prevention + 05-CONTEXT.md D-CI-03.
# Asserts rollout-core's public API contains zero AWS/GCP SDK symbols.
# Usage: scripts/check-public-api-cloud-leak.sh [path-to-public-api-dump]
set -euo pipefail
FILE="${1:-rollout-core.public-api.txt}"

if [ ! -f "$FILE" ]; then
    echo "ERROR: public-api dump not found at $FILE."
    echo "       Run: cargo public-api -p rollout-core --simplified > $FILE"
    exit 2
fi

# Forbidden prefixes (regex-OR alternation). Any hit fails.
FORBIDDEN_REGEX='\b(aws_sdk_|aws_smithy_|aws_config|aws_credential_types|gcloud_|google_cloud_|googleapis_)'

if grep -E "$FORBIDDEN_REGEX" "$FILE"; then
    echo ""
    echo "ERROR: rollout-core public API contains AWS/GCP SDK types. See Pitfall #1 in"
    echo "       .planning/research/PITFALLS.md. Collapse SDK errors to CoreError(String)"
    echo "       inside the rollout-cloud-aws / rollout-cloud-gcp crate boundaries; do NOT"
    echo "       expose SDK types on trait method signatures or error #[source] chains."
    exit 1
fi
echo "OK: rollout-core public API contains no SDK types."
