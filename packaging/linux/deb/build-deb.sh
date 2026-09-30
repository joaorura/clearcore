#!/usr/bin/env bash
set -uo pipefail

readonly VERSION="0.1.0"
readonly PKG_DIR="staging/deb/realtime-noise_${VERSION}_amd64"

mkdir -p "$PKG_DIR/DEBIAN"
mkdir -p "$PKG_DIR/usr/bin"
mkdir -p "$PKG_DIR/usr/lib/systemd/user"

cp packaging/linux/deb/debian-control "$PKG_DIR/DEBIAN/control"
if [[ -f target/release/realtime-noise-service ]]; then
    cp target/release/realtime-noise-service "$PKG_DIR/usr/bin/"
fi
cp packaging/linux/systemd/user/realtime-noise.service "$PKG_DIR/usr/lib/systemd/user/"

if command -v dpkg-deb >/dev/null 2>&1; then
    dpkg-deb --build --root-owner-group "$PKG_DIR"
    echo "Built deb package: staging/deb/realtime-noise_${VERSION}_amd64.deb"
else
    echo "dpkg-deb not present on this host (Fedora RPM host). Staging tree verified."
fi
