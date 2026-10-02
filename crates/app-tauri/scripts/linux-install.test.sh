#!/usr/bin/env bash
# Teste do instalador Linux (scripts/linux-install.sh) com HOME falso e ferramentas falsas.
# Nao instala nada de verdade: roda o instalador contra um bundle de brinquedo em um
# diretorio temporario.
#
# Cobre:
#  B) aviso claro (sem instalar nada) quando o GNOME nao tem a extensao AppIndicator;
#  C) icone instalado em hicolor/<LxA>/apps com a dimensao REAL do PNG, e Icon= consistente.
#
# Uso: crates/app-tauri/scripts/linux-install.test.sh
set -uo pipefail

SCRIPTS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_DIR="$(cd "${SCRIPTS_DIR}/.." && pwd)"
INSTALLER="${SCRIPTS_DIR}/linux-install.sh"
ICON_SRC="${APP_DIR}/assets/icon.png"

TMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/linux-install-test.XXXXXX")"
trap 'find "${TMP_ROOT}" -delete' EXIT

TOOLS="${TMP_ROOT}/tools"   # PATH base: nada do sistema real alem destas ferramentas
FAKES="${TMP_ROOT}/fakes"
mkdir -p "${TOOLS}" "${FAKES}"
for t in bash env mkdir cp chmod find sed ln dirname grep od cat uname awk tr head sort rm id printf mktemp; do
    src="$(command -v "${t}" 2>/dev/null || true)"
    if [[ -n "${src}" && -x "${src}" ]]; then ln -s "${src}" "${TOOLS}/${t}"; fi
done

cat > "${FAKES}/gnome-extensions" <<'EOF'
#!/bin/sh
case "$*" in
  "list --enabled") printf '%s' "${FAKE_EXT_ENABLED:-}" ;;
  "list")           printf '%s' "${FAKE_EXT_ALL:-}" ;;
esac
EOF
cat > "${FAKES}/gtk-update-icon-cache" <<'EOF'
#!/bin/sh
echo "gtk-update-icon-cache $*" >> "${FAKE_LOG}"
EOF
cat > "${FAKES}/update-desktop-database" <<'EOF'
#!/bin/sh
exit 0
EOF
chmod +x "${FAKES}"/*

PASS=0
FAIL=0
check() {
    local desc="$1"; shift
    if "$@"; then echo "ok   - ${desc}"; PASS=$((PASS + 1)); else echo "FAIL - ${desc}"; FAIL=$((FAIL + 1)); fi
}

UUID='appindicatorsupport@rgcjonas.gmail.com'

new_bundle() { # cria um bundle de brinquedo em ${TMP_ROOT}/bundle
    find "${TMP_ROOT}/bundle" -delete 2>/dev/null || true
    mkdir -p "${TMP_ROOT}/bundle/resources/app/assets" "${TMP_ROOT}/bundle/resources/bin" "${TMP_ROOT}/bundle/resources/scripts"
    cp "${INSTALLER}" "${TMP_ROOT}/bundle/install.sh"
    printf '#!/bin/sh\nexit 0\n' > "${TMP_ROOT}/bundle/clearcore"
    chmod +x "${TMP_ROOT}/bundle/clearcore"
    cp "${ICON_SRC}" "${TMP_ROOT}/bundle/resources/app/assets/icon.png"
    printf '[Desktop Entry]\nType=Application\nName=Clearcore\nExec=clearcore\nIcon=clearcore\n' > "${TMP_ROOT}/bundle/clearcore.desktop"
}

fresh_home() {
    find "${TMP_ROOT}/home" -delete 2>/dev/null || true
    mkdir -p "${TMP_ROOT}/home"
    : > "${TMP_ROOT}/log.txt"
}

run_install() { # run_install [VAR=valor ...]; usa HOME falso e USE_PATH
    env -i HOME="${TMP_ROOT}/home" PATH="${USE_PATH}" FAKE_LOG="${TMP_ROOT}/log.txt" "$@" \
        bash "${TMP_ROOT}/bundle/install.sh" > "${TMP_ROOT}/out.txt" 2>&1
    echo $? > "${TMP_ROOT}/rc.txt"
}
rc_is_zero() { [[ "$(cat "${TMP_ROOT}/rc.txt")" == "0" ]]; }
out_has() { grep -qF -- "$1" "${TMP_ROOT}/out.txt"; }
out_lacks() { ! grep -qF -- "$1" "${TMP_ROOT}/out.txt"; }

ICON_ROOT="${TMP_ROOT}/home/.local/share/icons/hicolor"
DESKTOP_FILE="${TMP_ROOT}/home/.local/share/applications/clearcore.desktop"

# ---- C: icone -----------------------------------------------------------------
# Dimensao real do PNG (IHDR, bytes 16..23)
read -r w1 w2 w3 w4 h1 h2 h3 h4 < <(od -An -tu1 -j16 -N8 "${ICON_SRC}")
W=$(( (w1 << 24) | (w2 << 16) | (w3 << 8) | w4 ))
H=$(( (h1 << 24) | (h2 << 16) | (h3 << 8) | h4 ))
SIZE_DIR="${W}x${H}"
echo "# icone do repo: ${SIZE_DIR}"

USE_PATH="${TOOLS}:${FAKES}"
new_bundle
fresh_home
run_install XDG_CURRENT_DESKTOP=KDE
check "C0: instalador termina com sucesso" rc_is_zero
check "C1: icone em hicolor/${SIZE_DIR}/apps (dimensao real do PNG)" test -f "${ICON_ROOT}/${SIZE_DIR}/apps/clearcore.png"
check "C2: nao instala em 256x256 quando o PNG nao tem 256" bash -c "[[ '${SIZE_DIR}' == '256x256' ]] || [[ ! -e '${ICON_ROOT}/256x256/apps/clearcore.png' ]]"
check "C3: Icon= do atalho aponta para o arquivo instalado" grep -qx "Icon=${ICON_ROOT}/${SIZE_DIR}/apps/clearcore.png" "${DESKTOP_FILE}"
check "C4: gtk-update-icon-cache roda sobre o hicolor do usuario" grep -q "gtk-update-icon-cache .*${ICON_ROOT}" "${TMP_ROOT}/log.txt"

# C5: reinstalacao remove o icone antigo no diretorio de tamanho errado
if [[ "${SIZE_DIR}" != "256x256" ]]; then
    fresh_home
    mkdir -p "${TMP_ROOT}/home/.local/share/icons/hicolor/256x256/apps"
    cp "${ICON_SRC}" "${TMP_ROOT}/home/.local/share/icons/hicolor/256x256/apps/clearcore.png"
    run_install XDG_CURRENT_DESKTOP=KDE
    check "C5: reinstalar apaga o icone legado de 256x256" test ! -e "${ICON_ROOT}/256x256/apps/clearcore.png"
    check "C5: ... e instala o novo no tamanho certo" test -f "${ICON_ROOT}/${SIZE_DIR}/apps/clearcore.png"
fi

# C6: sem gtk-update-icon-cache no PATH nao ha erro
USE_PATH="${TOOLS}"
new_bundle
fresh_home
run_install XDG_CURRENT_DESKTOP=KDE
check "C6: sem gtk-update-icon-cache o instalador conclui sem erro" rc_is_zero
check "C6: icone continua instalado" test -f "${ICON_ROOT}/${SIZE_DIR}/apps/clearcore.png"

# ---- B: aviso de bandeja no GNOME ---------------------------------------------
USE_PATH="${TOOLS}:${FAKES}"
new_bundle

fresh_home
run_install XDG_CURRENT_DESKTOP=GNOME FAKE_EXT_ALL='' FAKE_EXT_ENABLED=''
check "B1: GNOME sem extensao: instalador termina com sucesso" rc_is_zero
check "B1: warns about the system tray" out_has "system tray"
check "B1: mostra o comando de instalacao do Fedora" out_has "sudo dnf install gnome-shell-extension-appindicator"
check "B1: mostra o comando para habilitar" out_has "gnome-extensions enable ${UUID}"
check "B1: tells the user to log out and back in" out_has "log out and back in"

fresh_home
run_install XDG_CURRENT_DESKTOP=ubuntu:GNOME FAKE_EXT_ALL="${UUID}"$'\n' FAKE_EXT_ENABLED="${UUID}"$'\n'
check "B2: extensao instalada e habilitada: sem aviso" out_lacks "system tray"

fresh_home
run_install XDG_CURRENT_DESKTOP=GNOME FAKE_EXT_ALL="${UUID}"$'\n' FAKE_EXT_ENABLED=''
check "B3: instalada mas desabilitada: manda habilitar" out_has "gnome-extensions enable ${UUID}"
check "B3: ... e nao manda instalar de novo" out_lacks "dnf install"

fresh_home
run_install XDG_CURRENT_DESKTOP=GNOME FAKE_EXT_ALL=$'ubuntu-appindicators@ubuntu.com\n' FAKE_EXT_ENABLED=$'ubuntu-appindicators@ubuntu.com\n'
check "B4: ubuntu-appindicators habilitada: sem aviso" out_lacks "system tray"

fresh_home
run_install XDG_CURRENT_DESKTOP=KDE
check "B5: KDE (bandeja nativa): sem aviso" out_lacks "system tray"

fresh_home
run_install
check "B6: sem XDG_CURRENT_DESKTOP: sem aviso" out_lacks "system tray"
check "B6: ... e termina com sucesso" rc_is_zero

echo "----"
echo "${PASS} passaram, ${FAIL} falharam"
[[ "${FAIL}" -eq 0 ]]
