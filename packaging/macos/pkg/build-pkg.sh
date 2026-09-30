#!/usr/bin/env bash
set -uo pipefail

readonly PKG_ID="com.clearcore.realtime-noise"
readonly VERSION="0.1.0"
readonly OUTPUT_PKG="RealtimeNoise-${VERSION}.pkg"

echo "Building macOS component package ${OUTPUT_PKG}..."

if [[ -z "${DEVELOPER_ID_INSTALLER:-}" ]]; then
    echo "BLOCKED_SIGNING_CREDENTIAL: DEVELOPER_ID_INSTALLER environment variable is not set." >&2
    echo "Developer ID signing and notarization can only be performed with authentic Apple Developer credentials." >&2
    exit 2
fi

# In staging rack with developer ID credentials:
pkgbuild --root "staging/root" \
         --identifier "$PKG_ID" \
         --version "$VERSION" \
         --install-location "/" \
         --sign "$DEVELOPER_ID_INSTALLER" \
         "$OUTPUT_PKG"

echo "Package created and signed: $OUTPUT_PKG"
