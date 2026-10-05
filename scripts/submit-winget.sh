#!/usr/bin/env bash
# Helper script to submit Clearcore WinGet manifests to microsoft/winget-pkgs
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
VERSION="${1:-0.1.0-beta.3}"
MANIFEST_SRC="${REPO_ROOT}/packaging/winget/manifests/j/joaorura/Clearcore/${VERSION}"

echo "=========================================================="
echo " Submitting Clearcore v${VERSION} to microsoft/winget-pkgs"
echo " Manifest Directory: ${MANIFEST_SRC}"
echo "=========================================================="

if [ ! -d "${MANIFEST_SRC}" ]; then
    echo "❌ Manifest directory not found: ${MANIFEST_SRC}"
    exit 1
fi

WORK_DIR="${HOME}/.cache/winget-work"
mkdir -p "${WORK_DIR}"
TEMP_FORK="${WORK_DIR}/winget-pkgs-${VERSION}"
rm -rf "${TEMP_FORK}"

echo "1. Checking GitHub authentication..."
gh auth status

echo "2. Forking or cloning microsoft/winget-pkgs..."
gh repo fork microsoft/winget-pkgs --clone=false 2>/dev/null || true

MY_USER="$(gh api user --jq '.login')"
git clone --filter=blob:none --no-checkout --depth 1 "https://github.com/${MY_USER}/winget-pkgs.git" "${TEMP_FORK}"

BRANCH_NAME="clearcore-${VERSION}"
cd "${TEMP_FORK}"
git sparse-checkout init --cone
git sparse-checkout set manifests/j/joaorura
git checkout -b "${BRANCH_NAME}"

TARGET_DIR="manifests/j/joaorura/Clearcore/${VERSION}"
mkdir -p "${TARGET_DIR}"
cp -a "${MANIFEST_SRC}/"* "${TARGET_DIR}/"

git add "${TARGET_DIR}"
git commit -m "New version: joaorura.Clearcore version ${VERSION}"
git push -u origin "${BRANCH_NAME}" --force

echo "3. Opening Pull Request to microsoft/winget-pkgs..."
gh pr create \
    --repo microsoft/winget-pkgs \
    --head "${MY_USER}:${BRANCH_NAME}" \
    --base master \
    --title "New version: joaorura.Clearcore version ${VERSION}" \
    --body "### Package Submission
- **Package Identifier:** joaorura.Clearcore
- **Package Version:** ${VERSION}
- **Installer URL:** https://github.com/joaorura/clearcore/releases/download/v${VERSION}/Clearcore-Setup.exe
- **License:** PolyForm Noncommercial 1.0.0
- **Automated Submission:** Clearcore Release Pipeline"

echo "✅ Pull request successfully opened on microsoft/winget-pkgs!"
