#!/usr/bin/env bash
# ==============================================================================
# Deploy Script: Clean Release v0.1.0-beta.1 for ClearCore
#
# Actions performed:
# 1. Removes legacy GitHub Releases (v0.1.0-beta.1, v0.1.0-beta.2, v0.1.0-beta.3)
# 2. Deletes legacy remote and local tags
# 3. Synchronizes master branch with the standardized release commit
# 4. Creates and pushes the clean annotated tag v0.1.0-beta.1
#    (dispatches the multi-platform GitHub Actions release.yml workflow)
# ==============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "${REPO_ROOT}"

echo "=========================================================="
echo " ClearCore Clean Release v0.1.0-beta.1 Deployment"
echo "=========================================================="

# Check GitHub CLI authentication
if ! gh auth status >/dev/null 2>&1; then
    echo "❌ GitHub CLI (gh) is not authenticated. Run 'gh auth login' first."
    exit 1
fi

CURRENT_BRANCH="$(git rev-parse --abbrev-ref HEAD)"
echo "Current branch: ${CURRENT_BRANCH}"
echo "Current commit: $(git rev-parse --short HEAD)"

CONFIRM="${1:-}"
if [[ "${CONFIRM}" != "--yes" && "${CONFIRM}" != "-y" ]]; then
    read -p "Are you sure you want to clean old releases and deploy clean v0.1.0-beta.1? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "Aborted by user."
        exit 0
    fi
fi

# 1. Delete legacy GitHub Releases
echo "🗑️  1. Deleting legacy GitHub Releases..."
for rel in v0.1.0-beta.3 v0.1.0-beta.2 v0.1.0-beta.1; do
    if gh release view "${rel}" >/dev/null 2>&1; then
        echo "   Deleting GitHub release ${rel}..."
        gh release delete "${rel}" --yes || true
    fi
done

# 2. Delete legacy remote tags on origin
echo "🗑️  2. Deleting legacy remote tags on origin..."
for tag in v0.1.0-beta.3 v0.1.0-beta.2 v0.1.0-beta.1; do
    if git ls-remote --tags origin "refs/tags/${tag}" | grep -q "${tag}"; then
        echo "   Deleting remote tag ${tag}..."
        git push origin --delete "${tag}" || true
    fi
done

# 3. Delete legacy local tags
echo "🗑️  3. Deleting legacy local tags..."
for tag in v0.1.0-beta.3 v0.1.0-beta.2 v0.1.0-beta.1; do
    if git tag -l "${tag}" | grep -q "${tag}"; then
        echo "   Deleting local tag ${tag}..."
        git tag -d "${tag}" || true
    fi
done

# 4. Synchronize master branch
echo "🔄 4. Synchronizing master branch on origin..."
git push origin "${CURRENT_BRANCH}:master"

# Also sync local master worktree if present
if [[ -d "/home/joaorura/orca/projects/clearcore" ]]; then
    echo "   Syncing local master worktree at /home/joaorura/orca/projects/clearcore..."
    git -C /home/joaorura/orca/projects/clearcore pull origin master || true
fi

# 5. Create and push clean annotated tag v0.1.0-beta.1
echo "🏷️  5. Creating clean annotated tag v0.1.0-beta.1..."
git tag -a v0.1.0-beta.1 -m "Release v0.1.0-beta.1: Realtime AI noise-suppression virtual microphone"

echo "🚀 6. Pushing tag v0.1.0-beta.1 to origin (triggering GitHub Actions release.yml)..."
git push origin v0.1.0-beta.1

echo "=========================================================="
echo "✅ Clean Release v0.1.0-beta.1 Triggered Successfully!"
echo "   Monitor CI pipeline: https://github.com/joaorura/clearcore/actions"
echo "=========================================================="
