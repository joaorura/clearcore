#!/usr/bin/env bash
# Generates native .rpm (Fedora/RHEL) and .deb (Debian/Ubuntu) packages
# from an existing Clearcore Linux bundle directory.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
RELEASE_DIR="${REPO_ROOT}/release"

BUNDLE_NAME="${1:-Clearcore-linux-x64}"
BUNDLE_DIR="${RELEASE_DIR}/${BUNDLE_NAME}"

if [[ ! -d "${BUNDLE_DIR}" ]]; then
    echo "❌ Bundle directory not found: ${BUNDLE_DIR}"
    exit 1
fi

VERSION="0.1.0"
if [[ -f "${REPO_ROOT}/crates/app-tauri/package.json" ]]; then
    VERSION=$(grep '"version"' "${REPO_ROOT}/crates/app-tauri/package.json" | head -1 | awk -F'"' '{print $4}')
fi
CLEAN_VERSION="${VERSION//-/_}" # RPM versions cannot contain hyphens in release version tag

ARCH="x86_64"
DEB_ARCH="amd64"
if [[ "${BUNDLE_NAME}" == *"arm64"* ]] || [[ "${BUNDLE_NAME}" == *"aarch64"* ]]; then
    ARCH="aarch64"
    DEB_ARCH="arm64"
fi

echo "=========================================================="
echo " Packaging Linux Native Installers for Fedora (.rpm) & Debian (.deb)"
echo " Package: clearcore"
echo " Version: ${VERSION} (RPM: ${CLEAN_VERSION})"
echo " Architecture: ${ARCH} (Debian: ${DEB_ARCH})"
echo " Source Bundle: ${BUNDLE_DIR}"
echo "=========================================================="

# ----------------------------------------------------
# 1. BUILD RPM PACKAGE (Fedora / RHEL / openSUSE)
# ----------------------------------------------------
if command -v rpmbuild >/dev/null 2>&1; then
    echo "📦 Building native Fedora / RHEL RPM package..."
    RPM_TOPDIR="${RELEASE_DIR}/rpmbuild_${ARCH}"
    rm -rf "${RPM_TOPDIR}"
    mkdir -p "${RPM_TOPDIR}"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}

    ICON_SOURCE="${REPO_ROOT}/crates/app-tauri/assets/icon.png"
    if [[ ! -f "${ICON_SOURCE}" && -f "${BUNDLE_DIR}/clearcore.png" ]]; then
        ICON_SOURCE="${BUNDLE_DIR}/clearcore.png"
    fi

    cat << 'EOF' > "${RPM_TOPDIR}/SPECS/clearcore.spec"
%define _topdir TARGET_TOPDIR
%define _source_dir TARGET_BUNDLE_DIR
%define _icon_file TARGET_ICON_FILE

%define __strip /bin/true
%define __brp_strip /bin/true
%define __brp_ldconfig /bin/true
%define __brp_strip_comment_note /bin/true
%define __brp_strip_lto /bin/true
%define _build_id_links none
%global _enable_debug_package 0
%global debug_package %{nil}
%global __os_install_post /usr/lib/rpm/brp-compress %{nil}

Name:           clearcore
Version:        TARGET_VERSION
Release:        1%{?dist}
Summary:        Realtime AI Noise Suppression Virtual Microphone (DeepFilterNet3)
License:        PolyForm Noncommercial 1.0.0
URL:            https://github.com/joaorura/clearcore
AutoReqProv:    no
Requires:       pipewire >= 0.3.0

%description
Clearcore is a first-party realtime noise-suppression virtual microphone
powered by DeepFilterNet3 ONNX, PipeWire C bridge, and Rust supervisor daemon.
Provides voice isolation, neural EQ acoustic calibration, and studio DSP.

%install
rm -rf %{buildroot}
mkdir -p %{buildroot}/opt/clearcore
cp -a %{_source_dir}/* %{buildroot}/opt/clearcore/

mkdir -p %{buildroot}/usr/bin
cat << 'WRAPPER' > %{buildroot}/usr/bin/clearcore
#!/usr/bin/env bash
exec /opt/clearcore/clearcore "$@"
WRAPPER
chmod 0755 %{buildroot}/usr/bin/clearcore

mkdir -p %{buildroot}/usr/share/applications
if [ -f "%{_source_dir}/clearcore.desktop" ]; then
    cp "%{_source_dir}/clearcore.desktop" %{buildroot}/usr/share/applications/clearcore.desktop
else
    cat << 'DESKTOP' > %{buildroot}/usr/share/applications/clearcore.desktop
[Desktop Entry]
Type=Application
Name=Clearcore
GenericName=Noise Suppression Virtual Microphone
Comment=Realtime AI Noise Suppression Virtual Microphone (DeepFilterNet3)
Exec=/opt/clearcore/clearcore
Icon=clearcore
Terminal=false
Categories=AudioVideo;Audio;
Keywords=audio;microphone;noise;filter;clearcore;pipewire;
StartupWMClass=clearcore
DESKTOP
fi

mkdir -p %{buildroot}/usr/share/icons/hicolor/512x512/apps
if [ -f "%{_icon_file}" ]; then
    cp "%{_icon_file}" %{buildroot}/usr/share/icons/hicolor/512x512/apps/clearcore.png
fi

%files
/opt/clearcore
/usr/bin/clearcore
/usr/share/applications/clearcore.desktop
/usr/share/icons/hicolor/512x512/apps/clearcore.png

%post
gtk-update-icon-cache -f -t /usr/share/icons/hicolor 2>/dev/null || true
update-desktop-database /usr/share/applications 2>/dev/null || true

%preun
if [ "$1" -eq 0 ]; then
    # Complete uninstallation of Clearcore package
    pkill -x "clearcore" >/dev/null 2>&1 || true
    pkill -x "pipewire_helper" >/dev/null 2>&1 || true
    pkill -x "realtime-noise-service" >/dev/null 2>&1 || true
    pkill -f "^pw-loopback.*realtime-noise" >/dev/null 2>&1 || true

    # Destroy PipeWire virtual nodes (source, capture, sink)
    if command -v pw-cli >/dev/null 2>&1; then
        for nid in $(pw-cli list-objects Node 2>/dev/null | awk '
            $1 == "id" { cur_id = $2; sub(/,/, "", cur_id) }
            $0 ~ "node.name = \"realtime-noise" || $0 ~ "node.description = \".*Realtime Noise" {
                if (cur_id != "") { print cur_id }
            }
        ' | sort -u); do
            pw-cli destroy "$nid" >/dev/null 2>&1 || true
        done
    fi

    # Clean runtime locks and sockets
    rm -f /tmp/hippocamp_pipewire_helper*.lock /tmp/clearcore*.lock /tmp/clearcore_state* /tmp/realtime-noise*.sock 2>/dev/null || true
    rm -f /run/user/*/hippocamp_pipewire_helper*.lock /run/user/*/clearcore_state* /run/user/*/realtime-noise*.sock 2>/dev/null || true
fi

%postun
gtk-update-icon-cache -f -t /usr/share/icons/hicolor 2>/dev/null || true
update-desktop-database /usr/share/applications 2>/dev/null || true

%changelog
* Mon Oct 05 2026 João Rura <joaorura@users.noreply.github.com> - TARGET_VERSION-1
- Official Clearcore multi-platform beta release.
EOF

    # Replace placeholders
    sed -i "s|TARGET_TOPDIR|${RPM_TOPDIR}|g" "${RPM_TOPDIR}/SPECS/clearcore.spec"
    sed -i "s|TARGET_BUNDLE_DIR|${BUNDLE_DIR}|g" "${RPM_TOPDIR}/SPECS/clearcore.spec"
    sed -i "s|TARGET_ICON_FILE|${ICON_SOURCE}|g" "${RPM_TOPDIR}/SPECS/clearcore.spec"
    sed -i "s|TARGET_VERSION|${CLEAN_VERSION}|g" "${RPM_TOPDIR}/SPECS/clearcore.spec"

    rpmbuild --define "_topdir ${RPM_TOPDIR}" --target "${ARCH}" -bb "${RPM_TOPDIR}/SPECS/clearcore.spec"
    
    RPM_FILE="$(find "${RPM_TOPDIR}/RPMS" -name "*.rpm" | head -n1)"
    if [[ -n "${RPM_FILE}" && -f "${RPM_FILE}" ]]; then
        FINAL_RPM="${RELEASE_DIR}/Clearcore-${VERSION}-1.${ARCH}.rpm"
        cp "${RPM_FILE}" "${FINAL_RPM}"
        echo "✅ Fedora / RHEL RPM created successfully: ${FINAL_RPM}"
    fi
    rm -rf "${RPM_TOPDIR}"
else
    echo "⚠️ rpmbuild not found. Skipping RPM build. (Install 'rpm-build' or run on Fedora to generate .rpm)"
fi

# ----------------------------------------------------
# 2. BUILD DEB PACKAGE (Debian / Ubuntu / Mint)
# ----------------------------------------------------
if command -v dpkg-deb >/dev/null 2>&1; then
    echo "📦 Building native Debian / Ubuntu DEB package..."
    DEB_ROOT="${RELEASE_DIR}/debbuild_${DEB_ARCH}"
    rm -rf "${DEB_ROOT}"
    mkdir -p "${DEB_ROOT}/opt/clearcore"
    mkdir -p "${DEB_ROOT}/usr/bin"
    mkdir -p "${DEB_ROOT}/usr/share/applications"
    mkdir -p "${DEB_ROOT}/usr/share/icons/hicolor/512x512/apps"
    mkdir -p "${DEB_ROOT}/DEBIAN"

    cp -a "${BUNDLE_DIR}"/* "${DEB_ROOT}/opt/clearcore/"
    ln -sf /opt/clearcore/clearcore "${DEB_ROOT}/usr/bin/clearcore"

    ICON_SOURCE="${REPO_ROOT}/crates/app-tauri/assets/icon.png"
    if [[ ! -f "${ICON_SOURCE}" && -f "${BUNDLE_DIR}/clearcore.png" ]]; then
        ICON_SOURCE="${BUNDLE_DIR}/clearcore.png"
    fi
    if [[ -f "${ICON_SOURCE}" ]]; then
        cp "${ICON_SOURCE}" "${DEB_ROOT}/usr/share/icons/hicolor/512x512/apps/clearcore.png"
    fi

    if [[ -f "${BUNDLE_DIR}/clearcore.desktop" ]]; then
        cp "${BUNDLE_DIR}/clearcore.desktop" "${DEB_ROOT}/usr/share/applications/clearcore.desktop"
    fi

    cat << EOF > "${DEB_ROOT}/DEBIAN/control"
Package: clearcore
Version: ${VERSION}
Architecture: ${DEB_ARCH}
Maintainer: João Messias Lima Pereira <jmessiaslp856@gmail.com>
Depends: pipewire (>= 0.3.0)
Section: sound
Priority: optional
Homepage: https://github.com/joaorura/clearcore
Description: Realtime AI Noise Suppression Virtual Microphone (DeepFilterNet3)
 Clearcore provides realtime voice isolation, neural EQ acoustic calibration,
 and studio DSP for Linux desktops via PipeWire.
EOF

    cat << 'EOF' > "${DEB_ROOT}/DEBIAN/prerm"
#!/bin/sh
set -e

case "$1" in
    remove|purge|deconfigure)
        # Encerrar processos ativos do Clearcore
        killall -q clearcore 2>/dev/null || pkill -x clearcore 2>/dev/null || true
        killall -q pipewire_helper 2>/dev/null || pkill -x pipewire_helper 2>/dev/null || true
        killall -q realtime-noise-service 2>/dev/null || pkill -x realtime-noise-service 2>/dev/null || true
        pkill -f "^pw-loopback.*realtime-noise" 2>/dev/null || true

        # Destruir nós virtuais do PipeWire (source, capture e sink)
        if command -v pw-cli >/dev/null 2>&1; then
            for nid in $(pw-cli list-objects Node 2>/dev/null | awk '
                $1 == "id" { cur_id = $2; sub(/,/, "", cur_id) }
                $0 ~ "node.name = \"realtime-noise" || $0 ~ "node.description = \".*Realtime Noise" {
                    if (cur_id != "") { print cur_id }
                }
            ' | sort -u); do
                pw-cli destroy "$nid" >/dev/null 2>&1 || true
            done
        fi

        # Limpar travas, sockets e arquivos de estado
        rm -f /tmp/hippocamp_pipewire_helper*.lock /tmp/clearcore*.lock /tmp/clearcore_state* /tmp/realtime-noise*.sock 2>/dev/null || true
        rm -f /run/user/*/hippocamp_pipewire_helper*.lock /run/user/*/clearcore_state* /run/user/*/realtime-noise*.sock 2>/dev/null || true
        ;;
    failed-upgrade|upgrade)
        # Ao atualizar, interromper processos anteriores
        pkill -x "clearcore" >/dev/null 2>&1 || true
        pkill -x "pipewire_helper" >/dev/null 2>&1 || true
        pkill -x "realtime-noise-service" >/dev/null 2>&1 || true
        ;;
    *)
        ;;
esac

exit 0
EOF
    chmod 755 "${DEB_ROOT}/DEBIAN/prerm"

    cat << 'EOF' > "${DEB_ROOT}/DEBIAN/postinst"
#!/bin/sh
set -e
gtk-update-icon-cache -f -t /usr/share/icons/hicolor 2>/dev/null || true
update-desktop-database /usr/share/applications 2>/dev/null || true
EOF
    chmod 755 "${DEB_ROOT}/DEBIAN/postinst"

    cat << 'EOF' > "${DEB_ROOT}/DEBIAN/postrm"
#!/bin/sh
set -e
gtk-update-icon-cache -f -t /usr/share/icons/hicolor 2>/dev/null || true
update-desktop-database /usr/share/applications 2>/dev/null || true
EOF
    chmod 755 "${DEB_ROOT}/DEBIAN/postrm"

    FINAL_DEB="${RELEASE_DIR}/Clearcore-${VERSION}_${DEB_ARCH}.deb"
    dpkg-deb --build "${DEB_ROOT}" "${FINAL_DEB}"
    echo "✅ Debian / Ubuntu DEB created successfully: ${FINAL_DEB}"
    rm -rf "${DEB_ROOT}"
else
    echo "⚠️ dpkg-deb not found. Skipping DEB build."
fi

echo "=========================================================="
echo " Native Linux packaging complete!"
echo " Output files in ${RELEASE_DIR}:"
ls -lh "${RELEASE_DIR}"/Clearcore-*.* 2>/dev/null || true
echo "=========================================================="
