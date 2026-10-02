#!/usr/bin/env bash
# Clearcore Linux installer. Shipped as install.sh inside the release bundle
# (copied verbatim by crates/app-tauri/scripts/package-app.mjs).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_NAME="clearcore"
APPINDICATOR_UUID="appindicatorsupport@rgcjonas.gmail.com"

echo "=== Clearcore Desktop Application Installer ==="

if [[ $EUID -eq 0 ]]; then
    # System-wide installation
    INSTALL_DIR="/opt/clearcore"
    BIN_LINK="/usr/local/bin/clearcore"
    DESKTOP_DIR="/usr/share/applications"
    ICON_ROOT="/usr/share/icons/hicolor"
else
    # User-local installation (no root required)
    INSTALL_DIR="${HOME}/.local/share/clearcore"
    BIN_LINK="${HOME}/.local/bin/clearcore"
    DESKTOP_DIR="${HOME}/.local/share/applications"
    ICON_ROOT="${HOME}/.local/share/icons/hicolor"
    mkdir -p "${HOME}/.local/bin"
fi

# Prints "<width>x<height>" read from the PNG IHDR chunk (bytes 16..23), or nothing.
png_size() {
    local w1 w2 w3 w4 h1 h2 h3 h4
    read -r w1 w2 w3 w4 h1 h2 h3 h4 < <(od -An -tu1 -j16 -N8 "$1" 2>/dev/null) || return 0
    [[ -n "${h4:-}" ]] || return 0
    echo "$(( (w1 << 24) | (w2 << 16) | (w3 << 8) | w4 ))x$(( (h1 << 24) | (h2 << 16) | (h3 << 8) | h4 ))"
}

# GNOME has no tray of its own: without the AppIndicator extension the tray icon is
# invisible. Never installs anything; only tells the user what to do.
warn_if_gnome_without_tray() {
    case "${XDG_CURRENT_DESKTOP:-}" in
        *GNOME*|*gnome*) ;;
        *) return 0 ;;
    esac

    local installed="" enabled="" found=""
    if command -v gnome-extensions >/dev/null 2>&1; then
        enabled="$(gnome-extensions list --enabled 2>/dev/null | grep -i appindicator || true)"
        [[ -n "${enabled}" ]] && return 0
        installed="$(gnome-extensions list 2>/dev/null | grep -i appindicator || true)"
        found="$(printf '%s\n' "${installed}" | head -n1)"
    fi

    echo ""
    echo "⚠️  GNOME detected without the AppIndicator extension enabled."
    echo "   GNOME does not show a system tray by default, so the Clearcore tray icon will not"
    echo "   be visible. Clearcore still works: open it from the application menu (launching it"
    echo "   again brings the window back)."
    if [[ -n "${found}" ]]; then
        echo "   The extension '${found}' is installed but not enabled. Enable it with:"
        echo "     gnome-extensions enable ${found}"
    else
        echo "   To get the tray icon, install the extension (this installer does not do it for you):"
        echo "     Fedora:  sudo dnf install gnome-shell-extension-appindicator"
        echo "     Other distributions: install your distribution's AppIndicator extension package"
        echo "       (usually 'gnome-shell-extension-appindicator') or get it from"
        echo "       https://extensions.gnome.org/extension/615/appindicator-support/"
        echo "     Then enable it:"
        echo "     gnome-extensions enable ${APPINDICATOR_UUID}"
    fi
    echo "   Then log out and back in so GNOME Shell loads it."
}

echo "Installing Clearcore to ${INSTALL_DIR}..."
mkdir -p "${INSTALL_DIR}"
cp -r "${SCRIPT_DIR}"/* "${INSTALL_DIR}/"
chmod +x "${INSTALL_DIR}/clearcore"
find "${INSTALL_DIR}/resources/bin" -type f -exec chmod +x {} + 2>/dev/null || true
find "${INSTALL_DIR}/resources/scripts" -name "*.sh" -exec chmod +x {} + 2>/dev/null || true

echo "Creating launcher symlink at ${BIN_LINK}..."
mkdir -p "$(dirname "${BIN_LINK}")"
ln -sf "${INSTALL_DIR}/clearcore" "${BIN_LINK}"

echo "Installing icon and desktop entry..."
mkdir -p "${DESKTOP_DIR}"
ICON_SRC="${INSTALL_DIR}/resources/app/assets/icon.png"
ICON_FILE=""
if [[ -f "${ICON_SRC}" ]]; then
    # The hicolor directory must match the real pixel size of the PNG, otherwise the
    # shell/menu ignores or mis-scales the icon (a 64x64 PNG used to land in 256x256).
    ICON_SIZE="$(png_size "${ICON_SRC}")"
    if [[ -z "${ICON_SIZE}" ]]; then
        ICON_SIZE="256x256"
    fi
    ICON_DIR="${ICON_ROOT}/${ICON_SIZE}/apps"
    mkdir -p "${ICON_DIR}"
    cp "${ICON_SRC}" "${ICON_DIR}/clearcore.png"
    ICON_FILE="${ICON_DIR}/clearcore.png"
    # Drop copies left by older installers in other size directories
    find "${ICON_ROOT}" -path '*/apps/clearcore.png' ! -path "${ICON_FILE}" -delete 2>/dev/null || true
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -q -f -t "${ICON_ROOT}" >/dev/null 2>&1 || true
    fi
fi

if [[ -n "${ICON_FILE}" ]]; then
    sed -e "s|^Exec=.*|Exec=${INSTALL_DIR}/clearcore|" -e "s|^Icon=.*|Icon=${ICON_FILE}|" \
        "${INSTALL_DIR}/clearcore.desktop" > "${DESKTOP_DIR}/clearcore.desktop"
else
    sed "s|^Exec=.*|Exec=${INSTALL_DIR}/clearcore|" "${INSTALL_DIR}/clearcore.desktop" > "${DESKTOP_DIR}/clearcore.desktop"
fi
chmod +x "${DESKTOP_DIR}/clearcore.desktop"

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "${DESKTOP_DIR}" 2>/dev/null || true
fi

echo ""
echo "✅ Clearcore installed successfully!"
echo "You can launch Clearcore directly from your application menu or run 'clearcore' in your terminal."

warn_if_gnome_without_tray
