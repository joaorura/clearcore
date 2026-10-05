#!/usr/bin/env bash
# Helper script to submit Clearcore WinGet manifests to microsoft/winget-pkgs via GitHub API
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
VERSION="${1:-0.1.0-beta.1}"
MANIFEST_SRC="${REPO_ROOT}/packaging/winget/manifests/j/joaorura/Clearcore/${VERSION}"

echo "=========================================================="
echo " Submitting Clearcore v${VERSION} to microsoft/winget-pkgs"
echo " Manifest Directory: ${MANIFEST_SRC}"
echo "=========================================================="

if [ ! -d "${MANIFEST_SRC}" ]; then
    echo "❌ Manifest directory not found: ${MANIFEST_SRC}"
    exit 1
fi

echo "1. Checking GitHub authentication..."
gh auth status

echo "2. Ensuring fork of microsoft/winget-pkgs..."
gh repo fork microsoft/winget-pkgs --clone=false 2>/dev/null || true
MY_USER="$(gh api user --jq '.login')"

echo "3. Syncing fork master with upstream..."
gh api -X POST "/repos/${MY_USER}/winget-pkgs/merge-upstream" -f branch=master 2>/dev/null || true
MASTER_SHA="$(gh api "/repos/${MY_USER}/winget-pkgs/git/ref/heads/master" --jq '.object.sha')"

BRANCH_NAME="clearcore-${VERSION}"
echo "4. Creating branch ${BRANCH_NAME} on fork..."
gh api -X POST "/repos/${MY_USER}/winget-pkgs/git/refs" \
    -f ref="refs/heads/${BRANCH_NAME}" \
    -f sha="${MASTER_SHA}" 2>/dev/null || true

TARGET_DIR="manifests/j/joaorura/Clearcore/${VERSION}"
echo "5. Uploading manifests via GitHub API..."
for FILE in joaorura.Clearcore.yaml joaorura.Clearcore.installer.yaml joaorura.Clearcore.locale.en-US.yaml; do
    if [ -f "${MANIFEST_SRC}/${FILE}" ]; then
        echo "   -> Uploading ${FILE}..."
        CONTENT_B64="$(base64 -w 0 "${MANIFEST_SRC}/${FILE}")"
        gh api -X PUT "/repos/${MY_USER}/winget-pkgs/contents/${TARGET_DIR}/${FILE}" \
            -f message="Add ${FILE} for joaorura.Clearcore ${VERSION}" \
            -f content="${CONTENT_B64}" \
            -f branch="${BRANCH_NAME}" > /dev/null
    fi
done

echo "6. Opening Pull Request to microsoft/winget-pkgs..."
PR_URL="$(gh pr create \
    --repo microsoft/winget-pkgs \
    --head "${MY_USER}:${BRANCH_NAME}" \
    --base master \
    --title "New version: joaorura.Clearcore version ${VERSION}" \
    --body "### Package Submission
- **Package Identifier:** joaorura.Clearcore
- **Package Version:** ${VERSION}
- **Installer URL:** https://github.com/joaorura/clearcore/releases/download/v${VERSION}/Clearcore-Setup.exe
- **License:** PolyForm Noncommercial 1.0.0
- **Automated Submission:** Clearcore Release Pipeline")"

echo "✅ Pull request successfully opened on microsoft/winget-pkgs: ${PR_URL}"
