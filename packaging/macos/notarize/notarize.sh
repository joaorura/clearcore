#!/usr/bin/env bash
set -uo pipefail

if [[ -z "${APPLE_ID:-}" ]] || [[ -z "${APPLE_APP_SPECIFIC_PASSWORD:-}" ]] || [[ -z "${APPLE_TEAM_ID:-}" ]]; then
    echo "BLOCKED_SIGNING_CREDENTIAL: Apple Notarization credentials (APPLE_ID, APPLE_APP_SPECIFIC_PASSWORD, APPLE_TEAM_ID) missing." >&2
    exit 2
fi

readonly TARGET_PKG="${1:-RealtimeNoise-0.1.0.pkg}"

xcrun notarytool submit "$TARGET_PKG" \
    --apple-id "$APPLE_ID" \
    --password "$APPLE_APP_SPECIFIC_PASSWORD" \
    --team-id "$APPLE_TEAM_ID" \
    --wait

xcrun stapler staple "$TARGET_PKG"
echo "Successfully notarized and stapled $TARGET_PKG"
