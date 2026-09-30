# Specification: Task 16 Packaging, Signing, Updates & SBOM

## Overview
Implement multi-platform packaging recipes, artifact scanning, SBOM generation, uninstallation verification, and CI/CD workflows for release gates.

## Key Deliverables
1. **Packaging structures:**
   - Windows: MSIX / installer definitions (`packaging/windows/{msix,installer}/`)
   - macOS: PKG installer, notarization configs (`packaging/macos/{pkg,notarize}/`)
   - Linux: Debian/Ubuntu `.deb`, Fedora/RHEL `.rpm`, local repository configs (`packaging/linux/{deb,rpm,repo}/`)
2. **Release Scripts:**
   - `scripts/build-sbom.sh`: generates SPDX/CycloneDX SBOM from manifest
   - `scripts/scan-artifacts.sh`: scans packages for unapproved assets (fail-closed with `BLOCKED_UNLISTED_ASSET`)
   - `scripts/uninstall-smoke.sh`: verifies clean uninstallation without touching third-party files
   - `scripts/qualify-ga.sh`: evaluates final release criteria
3. **CI Workflows:**
   - `.github/workflows/{release,windows-hardware,macos-hardware,linux-hardware}.yml`
4. **Documentation:**
   - `docs/support-matrix.md`
   - `docs/uninstall.md`
   - `docs/release-gates.md`
   - `sbom/.gitkeep`
