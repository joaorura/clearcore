#!/usr/bin/env bash
# Runs the official CPU benchmark (crates/tools/src/bin/benchmark.rs) in a disposable
# copy of HEAD and stores the report under benchmarks/local/ (git-ignored).
#
# Why a copy: the benchmark binary writes benchmarks/cpu-baseline.{json,md} relative to
# the current directory, which would overwrite the committed baseline.
#
# Usage: scripts/run-cpu-baseline-local.sh [--host-evidence <abs json> --host-evidence-provenance <abs record> --run-id <id>]
# Environment: OUT_DIR (default <repo>/benchmarks/local), CARGO_TARGET_DIR (default <repo>/target).
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
OUT_DIR="${OUT_DIR:-$ROOT/benchmarks/local}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/clearcore-bench.XXXXXX")"

echo "work copy: $WORK"
git -C "$ROOT" archive HEAD | tar -x -C "$WORK"

cd "$WORK"
status_line="$(cargo run --release --locked -p realtime-noise-tools --bin benchmark -- \
  --backend tract --profile avx2-minimum --duration 300 "$@" || true)"
echo "benchmark stdout: $status_line"

stamp="$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$OUT_DIR"
cp "$WORK/benchmarks/cpu-baseline.json" "$OUT_DIR/cpu-baseline-$stamp.json"
cp "$WORK/benchmarks/cpu-baseline.md" "$OUT_DIR/cpu-baseline-$stamp.md"
echo "saved: $OUT_DIR/cpu-baseline-$stamp.json"
jq '{status, run_id, cpu_model: .host.cpu_model, reference_class_qualified: .host.reference_class_qualified, rejection_reason: .host.rejection_reason, measurements}' \
  "$OUT_DIR/cpu-baseline-$stamp.json"
