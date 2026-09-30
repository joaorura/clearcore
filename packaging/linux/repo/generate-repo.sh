#!/usr/bin/env bash
set -uo pipefail

readonly REPO_DIR="${1:-dist/repo}"

echo "Generating Linux repository metadata at $REPO_DIR..."
mkdir -p "$REPO_DIR/rpm" "$REPO_DIR/deb"

if command -v createrepo_c >/dev/null 2>&1; then
    createrepo_c "$REPO_DIR/rpm"
    echo "RPM repo metadata generated with createrepo_c"
fi

if command -v apt-ftparchive >/dev/null 2>&1; then
    apt-ftparchive packages "$REPO_DIR/deb" > "$REPO_DIR/deb/Packages"
    gzip -k -f "$REPO_DIR/deb/Packages"
    echo "APT repo metadata generated with apt-ftparchive"
fi

echo "Repository generation completed."
