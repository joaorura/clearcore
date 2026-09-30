#!/usr/bin/env bash
set -uo pipefail

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

usage() {
    echo "Usage: $0 --evidence <evidence_matrix.md>"
    exit 1
}

if [[ $# -lt 2 ]] || [[ "$1" != "--evidence" ]]; then
    usage
fi

evidence_file="$2"

if [[ ! -f "$evidence_file" ]]; then
    echo "GA_BLOCKED: Evidence matrix file not found: $evidence_file" >&2
    exit 1
fi

echo "Evaluating GA Qualification Matrix from $evidence_file..."

# Check for blocking tags
if grep -q "GA_BLOCKED" "$evidence_file" || grep -q "BLOCKED_" "$evidence_file"; then
    echo "GA_BLOCKED: Active blockers detected in qualification matrix." >&2
    grep -E "BLOCKED_[A-Z0-9_]+" "$evidence_file" >&2 || true
    exit 1
fi

if grep -q "GA_APPROVED" "$evidence_file"; then
    echo "GA_APPROVED: All release criteria across all 4 platforms verified."
    exit 0
fi

echo "GA_BLOCKED: Evidence matrix does not conclude GA_APPROVED." >&2
exit 1
