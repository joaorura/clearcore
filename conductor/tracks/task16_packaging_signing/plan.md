# Implementation Plan: Task 16 Packaging, Signing, Updates & SBOM

## Phase 1: Scripts & Scanner Verification (RED)
- [x] Task: Create `scripts/scan-artifacts.sh`, `scripts/build-sbom.sh`, `scripts/uninstall-smoke.sh`, `scripts/qualify-ga.sh`
- [x] Task: Verify failing tests (RED) on dummy unlisted assets (`BLOCKED_UNLISTED_ASSET`)

## Phase 2: Platform Packaging Recipes (GREEN)
- [x] Task: Create Windows packaging (`packaging/windows/msix/`, `packaging/windows/installer/`)
- [x] Task: Create macOS packaging (`packaging/macos/pkg/`, `packaging/macos/notarize/`)
- [x] Task: Create Linux packaging (`packaging/linux/deb/`, `packaging/linux/rpm/`, `packaging/linux/repo/`)
- [x] Task: Create GitHub Actions workflows (`.github/workflows/`)

## Phase 3: Documentation & Release Gates
- [x] Task: Create `docs/support-matrix.md`, `docs/uninstall.md`, `docs/release-gates.md`
- [x] Task: Create `sbom/.gitkeep` and test SBOM generation
- [x] Task: Verify clean uninstall smoke test
- [x] Task: Mark track complete
