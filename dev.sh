#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
echo "=========================================================="
echo " Clearcore Desktop - Starting Development Environment"
echo "=========================================================="

cd "${SCRIPT_DIR}/crates/app-tauri"
if [[ ! -f "node_modules/.bin/electron" ]]; then
    echo "📦 Dependencies missing. Running npm install..."
    npm install
fi
npm run dev
