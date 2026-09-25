#!/usr/bin/env bash
set -uo pipefail

readonly BLOCKED_STATUS="BLOCKED_OFFLINE_DEPENDENCY"
readonly PINNED_TOOLCHAIN="1.90.0"

report_missing_toolchain() {
    printf '%s\n' "$BLOCKED_STATUS" >&2
    printf 'missing crates/packages:\n' >&2
    printf '%s\n' "- rust-toolchain@$PINNED_TOOLCHAIN:cargo,rustc,rustfmt,clippy" >&2
}

report_missing_crates() {
    local output_path="$1"
    local missing

    missing="$({
        sed -n 's/.*no matching package named `\([^`]*\)`.*/\1/p' "$output_path"
        sed -n 's/.*failed to download `\([^`]*\)`.*/\1/p' "$output_path"
        sed -n 's/.*failed to get `\([^`]*\)` as a dependency.*/\1/p' "$output_path"
    } | LC_ALL=C sort -u)"

    if [[ -z "$missing" ]]; then
        return 1
    fi

    printf '%s\n' "$BLOCKED_STATUS" >&2
    printf 'missing crates/packages:\n' >&2
    while IFS= read -r package; do
        printf '%s\n' "- $package" >&2
    done <<<"$missing"
}

run_offline() {
    local label="$1"
    shift
    local output_path="$temporary_directory/$label.log"

    if "$@" >"$output_path" 2>&1; then
        cat "$output_path"
        return 0
    fi

    cat "$output_path" >&2
    if report_missing_crates "$output_path"; then
        return 2
    fi
    return 1
}

if ! command -v cargo >/dev/null 2>&1; then
    report_missing_toolchain
    exit 2
fi

temporary_directory="$(mktemp -d)"
readonly temporary_directory
trap 'rm -rf "$temporary_directory"' EXIT

run_offline metadata cargo metadata --workspace --locked --offline --format-version 1 --no-deps || exit $?
run_offline build cargo build --workspace --locked --offline || exit $?
run_offline test cargo test --workspace --locked --offline || exit $?
