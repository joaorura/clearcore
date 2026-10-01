#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
echo "=========================================================="
echo " Clearcore Desktop Application - Build & Package"
echo "=========================================================="

cd "${SCRIPT_DIR}/crates/app-tauri"
npm run package

echo ""
echo "Packaging complete! Standalone packages located in: ${SCRIPT_DIR}/release"
