#!/usr/bin/env bash
set -uo pipefail

readonly BLOCKED_STATUS="BLOCKED_OFFLINE_DEPENDENCY"
readonly PINNED_TOOLCHAIN="1.90.0"

report_blockers() {
    printf '%s\n' "$BLOCKED_STATUS" >&2
    printf 'missing crates/packages:\n' >&2
    printf -- '- %s\n' "$@" >&2
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

    mapfile -t missing_packages <<<"$missing"
    report_blockers "${missing_packages[@]}"
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

missing_components=()

if ! command -v cargo >/dev/null 2>&1 || ! cargo --version >/dev/null 2>&1; then
    missing_components+=("cargo")
fi

if ! command -v rustc >/dev/null 2>&1; then
    missing_components+=("rustc@$PINNED_TOOLCHAIN")
else
    rustc_version=""
    if ! rustc_version="$(rustc --version 2>/dev/null)"; then
        missing_components+=("rustc@$PINNED_TOOLCHAIN")
    elif [[ "$rustc_version" != "rustc $PINNED_TOOLCHAIN "* ]]; then
        if [[ -n "$rustc_version" ]]; then
            missing_components+=("rustc@$PINNED_TOOLCHAIN (found: $rustc_version)")
        else
            missing_components+=("rustc@$PINNED_TOOLCHAIN")
        fi
    fi
    readonly rustc_version
fi

if ! command -v rustfmt >/dev/null 2>&1 || ! rustfmt --version >/dev/null 2>&1; then
    missing_components+=("rustfmt")
fi

if ! command -v clippy-driver >/dev/null 2>&1 || ! clippy-driver --version >/dev/null 2>&1; then
    missing_components+=("clippy")
fi

if [[ "${#missing_components[@]}" -ne 0 ]]; then
    report_blockers "${missing_components[@]}"
    exit 2
fi

temporary_directory="$(mktemp -d)"
readonly temporary_directory
trap 'rm -rf "$temporary_directory"' EXIT

run_offline metadata cargo metadata --locked --offline --format-version 1 --no-deps || exit $?
run_offline build cargo build --workspace --locked --offline || exit $?
run_offline test cargo test --workspace --locked --offline || exit $?
