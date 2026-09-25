#!/bin/bash
set -euo pipefail

readonly repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly gate="$repository_root/scripts/check-offline.sh"
temporary_root="$(mktemp -d)"
readonly temporary_root
trap 'rm -rf "$temporary_root"' EXIT

create_case() {
    case_bin="$temporary_root/$1/bin"
    mkdir -p "$case_bin"
    for command_name in cat mktemp rm sed sort; do
        ln -s "$(command -v "$command_name")" "$case_bin/$command_name"
    done
}

create_rustc() {
    local version="$1"
    local status="${2:-0}"
    cat >"$case_bin/rustc" <<EOF
#!/bin/bash
printf '%s\n' 'rustc $version (fake)'
exit $status
EOF
    chmod +x "$case_bin/rustc"
}

create_rustfmt() {
    cat >"$case_bin/rustfmt" <<'EOF'
#!/bin/bash
printf '%s\n' 'rustfmt 1.8.0-stable (fake)'
EOF
    chmod +x "$case_bin/rustfmt"
}

create_clippy() {
    cat >"$case_bin/clippy-driver" <<'EOF'
#!/bin/bash
printf '%s\n' 'clippy 0.1.90 (fake)'
EOF
    chmod +x "$case_bin/clippy-driver"
}

create_cargo() {
    local metadata_result="${1:-success}"
    cat >"$case_bin/cargo" <<EOF
#!/bin/bash
if [[ "\${1:-}" == "--version" ]]; then
    printf '%s\n' 'cargo 1.90.0 (fake)'
    exit 0
fi
if [[ "\${1:-}" == "metadata" && "$metadata_result" == "missing-cache" ]]; then
    printf '%s\n' 'error: no matching package named \`missing-cache\` found' >&2
    exit 101
fi
if [[ "\${1:-}" == "metadata" && "$metadata_result" == "generic-failure" ]]; then
    printf '%s\n' 'error: malformed manifest' >&2
    exit 101
fi
exit 0
EOF
    chmod +x "$case_bin/cargo"
}

assert_blocked() {
    local case_name="$1"
    local expected="$2"
    local output_path="$temporary_root/$case_name.output"
    local status

    set +e
    PATH="$case_bin" /bin/bash "$gate" >"$output_path" 2>&1
    status=$?
    set -e

    if [[ "$status" -ne 2 ]]; then
        printf 'FAIL: %s exited %s, expected 2\n' "$case_name" "$status" >&2
        cat "$output_path" >&2
        return 1
    fi
    if [[ "$(cat "$output_path")" != "$expected" ]]; then
        printf 'FAIL: %s output mismatch\nexpected:\n%s\nactual:\n' "$case_name" "$expected" >&2
        cat "$output_path" >&2
        return 1
    fi
    printf 'PASS: %s\n' "$case_name"
}

assert_failed() {
    local case_name="$1"
    local expected="$2"
    local output_path="$temporary_root/$case_name.output"
    local status

    set +e
    PATH="$case_bin" /bin/bash "$gate" >"$output_path" 2>&1
    status=$?
    set -e

    if [[ "$status" -ne 1 ]]; then
        printf 'FAIL: %s exited %s, expected 1\n' "$case_name" "$status" >&2
        cat "$output_path" >&2
        return 1
    fi
    if [[ "$(cat "$output_path")" != "$expected" ]]; then
        printf 'FAIL: %s output mismatch\nexpected:\n%s\nactual:\n' "$case_name" "$expected" >&2
        cat "$output_path" >&2
        return 1
    fi
    printf 'PASS: %s\n' "$case_name"
}

create_case absent-cargo
create_rustc 1.90.0
create_rustfmt
create_clippy
assert_blocked absent-cargo $'BLOCKED_OFFLINE_DEPENDENCY\nmissing crates/packages:\n- cargo'

create_case absent-rustc
create_cargo
create_rustfmt
create_clippy
assert_blocked absent-rustc $'BLOCKED_OFFLINE_DEPENDENCY\nmissing crates/packages:\n- rustc@1.90.0'

create_case wrong-rustc
create_cargo
create_rustc 1.89.0
create_rustfmt
create_clippy
assert_blocked wrong-rustc $'BLOCKED_OFFLINE_DEPENDENCY\nmissing crates/packages:\n- rustc@1.90.0 (found: rustc 1.89.0 (fake))'

create_case failing-rustc
create_cargo
create_rustc 1.90.0 1
create_rustfmt
create_clippy
assert_blocked failing-rustc $'BLOCKED_OFFLINE_DEPENDENCY\nmissing crates/packages:\n- rustc@1.90.0'

create_case absent-rustfmt
create_cargo
create_rustc 1.90.0
create_clippy
assert_blocked absent-rustfmt $'BLOCKED_OFFLINE_DEPENDENCY\nmissing crates/packages:\n- rustfmt'

create_case absent-clippy
create_cargo
create_rustc 1.90.0
create_rustfmt
assert_blocked absent-clippy $'BLOCKED_OFFLINE_DEPENDENCY\nmissing crates/packages:\n- clippy'

create_case missing-cached-crate
create_cargo missing-cache
create_rustc 1.90.0
create_rustfmt
create_clippy
assert_blocked missing-cached-crate $'error: no matching package named `missing-cache` found\nBLOCKED_OFFLINE_DEPENDENCY\nmissing crates/packages:\n- missing-cache'

create_case generic-cargo-failure
create_cargo generic-failure
create_rustc 1.90.0
create_rustfmt
create_clippy
assert_failed generic-cargo-failure 'error: malformed manifest'
