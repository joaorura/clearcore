#!/usr/bin/env bash
# Teste de scripts/check-virtual-mic.sh com ferramentas PipeWire FALSAS.
#
# Nao toca no grafo de audio real: copia o script para uma arvore temporaria, poe
# wpctl/pw-cli/pw-link/pw-loopback/systemd-run/systemctl/pkill falsos na frente do
# PATH e confere o que o script PEDIRIA ao PipeWire.
#
# Cobre:
#  A) o microfone fisico vai ao helper por node.name (nunca por numero de no);
#  D) fonte MONO (ex.: headset Bluetooth) e ligada; estereo continua igual;
#  F) o helper e localizado no layout do PACOTE (resources/bin), por override
#     (CLEARCORE_HELPER_BIN) e no layout de desenvolvimento; sem helper, avisa no stderr.
#
# Uso: platform/linux/tests/check-virtual-mic-fakes.test.sh
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
SCRIPT_UNDER_TEST="${REPO_ROOT}/scripts/check-virtual-mic.sh"

TMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/check-virtual-mic-test.XXXXXX")"
trap 'find "${TMP_ROOT}" -delete' EXIT

TREE="${TMP_ROOT}/repo"
FAKEBIN="${TMP_ROOT}/bin"
FAKE_DIR="${TMP_ROOT}/state"
mkdir -p "${TREE}/scripts" "${TREE}/platform/linux/helper/build" "${FAKEBIN}" "${FAKE_DIR}" "${TMP_ROOT}/run"
cp "${SCRIPT_UNDER_TEST}" "${TREE}/scripts/check-virtual-mic.sh"
printf '#!/bin/sh\nexit 0\n' > "${TREE}/platform/linux/helper/build/pipewire_helper"
chmod +x "${TREE}/platform/linux/helper/build/pipewire_helper"

# ---- ferramentas falsas -------------------------------------------------------
cat > "${FAKEBIN}/wpctl" <<'EOF'
#!/bin/sh
case "$1" in
  status)  cat "${FAKE_DIR}/wpctl_status.txt" 2>/dev/null ;;
  inspect) cat "${FAKE_DIR}/inspect_$2.txt" 2>/dev/null || exit 1 ;;
  *)       echo "wpctl $*" >> "${FAKE_DIR}/calls.log" ;;
esac
EOF
cat > "${FAKEBIN}/pw-cli" <<'EOF'
#!/bin/sh
case "$1 $2" in
  "list-objects Node") cat "${FAKE_DIR}/pw_cli_nodes.txt" 2>/dev/null ;;
  info*)               cat "${FAKE_DIR}/pw_cli_info_$2.txt" 2>/dev/null || exit 1 ;;
esac
EOF
cat > "${FAKEBIN}/pw-link" <<'EOF'
#!/bin/sh
case "$1" in
  -o) cat "${FAKE_DIR}/pw_link_o.txt" 2>/dev/null ;;
  -i) cat "${FAKE_DIR}/pw_link_i.txt" 2>/dev/null ;;
  -l) cat "${FAKE_DIR}/pw_link_l.txt" 2>/dev/null ;;
  *)  echo "pw-link $*" >> "${FAKE_DIR}/calls.log" ;;
esac
EOF
cat > "${FAKEBIN}/systemd-run" <<'EOF'
#!/bin/sh
: > "${FAKE_DIR}/helper_args.txt"
for a in "$@"; do printf '%s\n' "$a" >> "${FAKE_DIR}/helper_args.txt"; done
EOF
# Nada abaixo pode chegar ao sistema real: registram e saem.
for tool in systemctl pkill pw-loopback pw-dump pw-metadata nohup; do
  printf '#!/bin/sh\necho "%s $*" >> "${FAKE_DIR}/calls.log"\nexit 0\n' "${tool}" > "${FAKEBIN}/${tool}"
done
chmod +x "${FAKEBIN}"/*

PASS=0
FAIL=0
check() { # check "descricao" condicao...
    local desc="$1"; shift
    if "$@"; then echo "ok   - ${desc}"; PASS=$((PASS + 1)); else echo "FAIL - ${desc}"; FAIL=$((FAIL + 1)); fi
}

reset_state() {
    find "${FAKE_DIR}" -mindepth 1 -delete
    : > "${FAKE_DIR}/calls.log"
    cat > "${FAKE_DIR}/pw_cli_nodes.txt" <<'EOF'
	id 135, type PipeWire:Interface:Node/3
 		media.class = "Audio/Source"
 		node.name = "realtime-noise-source"
EOF
}

run_script() { # run_script args...
    env PATH="${FAKEBIN}:/usr/bin:/bin" FAKE_DIR="${FAKE_DIR}" XDG_RUNTIME_DIR="${TMP_ROOT}/run" \
        bash "${TREE}/scripts/check-virtual-mic.sh" "$@" >"${FAKE_DIR}/stdout.txt" 2>&1
}

helper_target() { # imprime o valor que seguiu --target na chamada do helper
    awk 'prev == "--target" { print; exit } { prev = $0 }' "${FAKE_DIR}/helper_args.txt" 2>/dev/null
}
has_call() { grep -qxF -- "$1" "${FAKE_DIR}/calls.log"; }
count_links() { grep -c '^pw-link [^-]' "${FAKE_DIR}/calls.log" || true; }

WPCTL_STATUS_ALSA='Audio
 ├─ Devices:
 ├─ Sinks:
 │  *   45. Speaker [vol: 0.40]
 │
 ├─ Sources:
 │      67. Core Ultra Microphone                [vol: 0.85]
 │      68. Core Ultra Headset Microphone        [vol: 0.94]
 │  *  135. Realtime Noise Virtual Microphone    [vol: 1.00]
 │
 ├─ Filters:
 ├─ Streams:'

ALSA_NAME='alsa_input.pci-0000_00_1f.3-platform-sof_sdw.HiFi__Mic__source'
BT_NAME='bluez_input.38:FB:A0:38:F7:EB'

# ---- A1: wpctl lista um microfone ALSA -> helper recebe o NOME ---------------
reset_state
printf '%s\n' "${WPCTL_STATUS_ALSA}" > "${FAKE_DIR}/wpctl_status.txt"
printf 'id 67, type PipeWire:Interface:Node\n  * node.name = "%s"\n  * object.serial = "67"\n' "${ALSA_NAME}" > "${FAKE_DIR}/inspect_67.txt"
run_script --recreate
check "A1: helper recebe --target <node.name> (e nao '67')" test "$(helper_target)" = "${ALSA_NAME}"

# ---- A2: primeiro microfone e Bluetooth, nome com ':' -------------------------
reset_state
cat > "${FAKE_DIR}/wpctl_status.txt" <<'EOF'
Audio
 ├─ Sources:
 │      143. Baseus Bass BC1                      [vol: 1.00]
 │  *   135. Realtime Noise Virtual Microphone    [vol: 1.00]
 ├─ Filters:
EOF
printf 'id 143, type PipeWire:Interface:Node\n    audio.position = "[MONO]"\n  * node.name = "%s"\n  * object.serial = "19537"\n' "${BT_NAME}" > "${FAKE_DIR}/inspect_143.txt"
run_script --recreate
check "A2: nome Bluetooth com ':' chega inteiro ao helper" test "$(helper_target)" = "${BT_NAME}"

# ---- A3: sem wpctl util, cai no pw-cli e tambem entrega o NOME ---------------
reset_state
: > "${FAKE_DIR}/wpctl_status.txt"
cat > "${FAKE_DIR}/pw_cli_nodes.txt" <<EOF
	id 67, type PipeWire:Interface:Node/3
 		media.class = "Audio/Source"
 		node.name = "${ALSA_NAME}"
	id 135, type PipeWire:Interface:Node/3
 		media.class = "Audio/Source"
 		node.name = "realtime-noise-source"
EOF
run_script --recreate
check "A3: fallback pw-cli entrega o node.name (e nao '67')" test "$(helper_target)" = "${ALSA_NAME}"

# ---- A4: nenhum microfone fisico -> helper sem --target ----------------------
reset_state
: > "${FAKE_DIR}/wpctl_status.txt"
run_script --recreate
check "A4: sem microfone fisico, o helper nao recebe --target" test -z "$(helper_target)"

# ---- D: ligacao da captura (via --set-default -> ensure_capture_link) --------
prep_link_state() { # prep_link_state "<portas -o>" "<portas -i>" "<saida -l>" ["<node.name do mic escolhido>"]
    reset_state
    # O mic escolhido (o que o helper recebe em --target) vem do fallback pw-cli;
    # sem 4o argumento nenhum mic fisico e conhecido.
    if [[ -n "${4:-}" ]]; then
        cat >> "${FAKE_DIR}/pw_cli_nodes.txt" <<EOF2
	id 67, type PipeWire:Interface:Node/3
 		media.class = "Audio/Source"
 		node.name = "$4"
EOF2
    fi
    printf '%s\n' "$1" > "${FAKE_DIR}/pw_link_o.txt"
    printf '%s\n' "$2" > "${FAKE_DIR}/pw_link_i.txt"
    printf '%s\n' "$3" > "${FAKE_DIR}/pw_link_l.txt"
}
IN_FLFR=$'realtime-noise-capture:input_FL\nrealtime-noise-capture:input_FR'
IN_MONO='realtime-noise-capture:input_MONO'

# D1: fonte MONO (Bluetooth) com entradas FL/FR -> a unica porta alimenta as duas
prep_link_state "${BT_NAME}:capture_MONO
realtime-noise-source:capture_MONO" "${IN_FLFR}" "" "${BT_NAME}"
run_script --set-default
check "D1: mono -> input_FL" has_call "pw-link ${BT_NAME}:capture_MONO realtime-noise-capture:input_FL"
check "D1: mono -> input_FR" has_call "pw-link ${BT_NAME}:capture_MONO realtime-noise-capture:input_FR"

# D2: fonte MONO com entrada unica input_MONO
prep_link_state "${BT_NAME}:capture_MONO" "${IN_MONO}" "" "${BT_NAME}"
run_script --set-default
check "D2: mono -> input_MONO" has_call "pw-link ${BT_NAME}:capture_MONO realtime-noise-capture:input_MONO"

# D3: estereo ALSA com entradas FL/FR -> FL->FL, FR->FR
prep_link_state "${ALSA_NAME}:capture_FL
${ALSA_NAME}:capture_FR" "${IN_FLFR}" "" "${ALSA_NAME}"
run_script --set-default
check "D3: estereo FL -> input_FL" has_call "pw-link ${ALSA_NAME}:capture_FL realtime-noise-capture:input_FL"
check "D3: estereo FR -> input_FR" has_call "pw-link ${ALSA_NAME}:capture_FR realtime-noise-capture:input_FR"
check "D3: estereo faz exatamente 2 ligacoes" test "$(count_links)" = "2"

# D4: estereo ALSA com entrada unica input_MONO -> igual ao comportamento antigo (so FL)
prep_link_state "${ALSA_NAME}:capture_FL
${ALSA_NAME}:capture_FR" "${IN_MONO}" "" "${ALSA_NAME}"
run_script --set-default
check "D4: estereo + input_MONO liga so FL -> input_MONO" has_call "pw-link ${ALSA_NAME}:capture_FL realtime-noise-capture:input_MONO"
check "D4: ... e nada mais" test "$(count_links)" = "1"

# D5: ja ligado a uma fonte fisica -> nenhuma ligacao nova (idempotente)
prep_link_state "${ALSA_NAME}:capture_FL
${ALSA_NAME}:capture_FR" "${IN_FLFR}" "${ALSA_NAME}:capture_FL
  |-> realtime-noise-capture:input_FL
realtime-noise-capture:input_FL
  |<- ${ALSA_NAME}:capture_FL" "${ALSA_NAME}"
run_script --set-default
check "D5: ja ligado, nao refaz ligacoes" test "$(count_links)" = "0"

# D6: o self-loop (virtual -> captura) e sempre desfeito
prep_link_state "${BT_NAME}:capture_MONO" "${IN_FLFR}" "" "${BT_NAME}"
run_script --set-default
check "D6: desfaz self-loop em input_FL" has_call "pw-link -d realtime-noise-source:capture_MONO realtime-noise-capture:input_FL"

# ---- E: so o mic ESCOLHIDO e ligado, nunca "o primeiro nó cuja porta casa" -----
V4L2_NAME='v4l2_input.pci-0000_00_14.0-usb-0_5_1.0'
E_PORTS_O="${V4L2_NAME}:capture_1
${BT_NAME}:capture_MONO
${ALSA_NAME}:capture_FL
${ALSA_NAME}:capture_FR
realtime-noise-source:capture_MONO"

# E1: 3 fontes casam o padrao antigo (webcam v4l2 capture_1, bluez mono, alsa estereo);
# o mic escolhido e o ALSA -> so ele e ligado, a webcam e o bluez nao
prep_link_state "${E_PORTS_O}" "${IN_FLFR}" "" "${ALSA_NAME}"
run_script --set-default
check "E1: liga o mic escolhido FL -> input_FL" has_call "pw-link ${ALSA_NAME}:capture_FL realtime-noise-capture:input_FL"
check "E1: liga o mic escolhido FR -> input_FR" has_call "pw-link ${ALSA_NAME}:capture_FR realtime-noise-capture:input_FR"
check "E1: NAO liga a webcam (v4l2)" test "$(grep -c "pw-link ${V4L2_NAME}" "${FAKE_DIR}/calls.log")" = "0"
check "E1: NAO liga o bluez" test "$(grep -c "pw-link ${BT_NAME}" "${FAKE_DIR}/calls.log")" = "0"
check "E1: exatamente 2 ligacoes" test "$(count_links)" = "2"

# E2: o escolhido e o bluez (mono), com a webcam listada antes -> so o bluez
prep_link_state "${E_PORTS_O}" "${IN_FLFR}" "" "${BT_NAME}"
run_script --set-default
check "E2: bluez escolhido -> input_FL" has_call "pw-link ${BT_NAME}:capture_MONO realtime-noise-capture:input_FL"
check "E2: NAO liga a webcam (v4l2)" test "$(grep -c "pw-link ${V4L2_NAME}" "${FAKE_DIR}/calls.log")" = "0"
check "E2: NAO liga o alsa" test "$(grep -c "pw-link ${ALSA_NAME}" "${FAKE_DIR}/calls.log")" = "0"

# E3: nenhum mic escolhido conhecido -> nao liga nada as cegas (WirePlumber decide)
prep_link_state "${E_PORTS_O}" "${IN_FLFR}" ""
run_script --set-default
check "E3: sem nome conhecido, nenhuma ligacao" test "$(count_links)" = "0"

# E4: mesmo caminho via --recreate: o nome passado ao helper e o ligado
prep_link_state "${E_PORTS_O}" "${IN_FLFR}" "" "${ALSA_NAME}"
run_script --recreate
check "E4: recreate liga so o mic do --target (FL)" has_call "pw-link ${ALSA_NAME}:capture_FL realtime-noise-capture:input_FL"
check "E4: recreate NAO liga a webcam" test "$(grep -c "pw-link ${V4L2_NAME}" "${FAKE_DIR}/calls.log")" = "0"

# ---- F: localizacao do helper (layout do PACOTE x desenvolvimento x override) -----
# No pacote o script mora em <raiz>/resources/scripts/ e o helper em <raiz>/resources/bin/.
# O systemd-run falso executa o "helper" que recebe (o fake sai na hora), para que o no
# apareca no grafo como o helper real faria; assim o fallback pw-loopback so e chamado
# se o helper NAO tiver sido encontrado.
cat > "${FAKEBIN}/systemd-run" <<'EOF'
#!/bin/sh
: > "${FAKE_DIR}/helper_args.txt"
for a in "$@"; do printf '%s\n' "$a" >> "${FAKE_DIR}/helper_args.txt"; done
for a in "$@"; do
  case "$a" in
    --*) ;;
    *) if [ -x "$a" ]; then "$a"; fi; break ;;
  esac
done
exit 0
EOF
chmod +x "${FAKEBIN}/systemd-run"

make_fake_helper() { # make_fake_helper <caminho>: registra a invocacao e faz o no aparecer
    mkdir -p "$(dirname "$1")"
    cat > "$1" <<'EOF'
#!/bin/sh
echo "invoked $0" >> "${FAKE_DIR}/helper_invoked.txt"
printf '\tid 135, type PipeWire:Interface:Node/3\n \t\tmedia.class = "Audio/Source"\n \t\tnode.name = "realtime-noise-source"\n' >> "${FAKE_DIR}/pw_cli_nodes.txt"
exit 0
EOF
    chmod +x "$1"
}

# prepara um layout isolado e limpo; imprime a raiz
new_layout() { # new_layout <nome> <pacote|dev>
    local root="${TMP_ROOT}/layout-$1"
    if [[ "$2" == "pacote" ]]; then
        mkdir -p "${root}/resources/scripts"
        cp "${SCRIPT_UNDER_TEST}" "${root}/resources/scripts/check-virtual-mic.sh"
    else
        mkdir -p "${root}/scripts"
        cp "${SCRIPT_UNDER_TEST}" "${root}/scripts/check-virtual-mic.sh"
    fi
    printf '%s\n' "${root}"
}

# run_layout <script> [VAR=valor ...] -- args...
run_layout() {
    local script="$1"; shift
    local -a extra=()
    while [[ $# -gt 0 && "$1" != "--" ]]; do extra+=("$1"); shift; done
    shift
    env PATH="${FAKEBIN}:/usr/bin:/bin" FAKE_DIR="${FAKE_DIR}" XDG_RUNTIME_DIR="${TMP_ROOT}/run" \
        ${extra[@]+"${extra[@]}"} bash "${script}" "$@" >"${FAKE_DIR}/stdout.txt" 2>&1
}
reset_state_empty() { # estado limpo e SEM o no virtual no grafo
    reset_state
    : > "${FAKE_DIR}/pw_cli_nodes.txt"
}
helper_ran() { grep -qxF "invoked $1" "${FAKE_DIR}/helper_invoked.txt" 2>/dev/null; }
helper_arg_is() { grep -qxF -- "$1" "${FAKE_DIR}/helper_args.txt" 2>/dev/null; }
# o script lanca o loopback via "nohup pw-loopback ..." (nohup falso registra "nohup pw-loopback ...")
# (o pkill falso tambem registra "pw-loopback" no padrao; so conta a LANCAMENTO via nohup)
no_loopback() { ! grep -q '^nohup pw-loopback' "${FAKE_DIR}/calls.log"; }
has_loopback() { grep -q '^nohup pw-loopback' "${FAKE_DIR}/calls.log"; }
out_has() { grep -qF -- "$1" "${FAKE_DIR}/stdout.txt"; }
WARN_TEXT='pipewire_helper nao encontrado; usando loopback SEM supressao de ruido'

# F1: layout de PACOTE (resources/scripts + resources/bin) -> usa resources/bin/pipewire_helper
PKG="$(new_layout pacote pacote)"
make_fake_helper "${PKG}/resources/bin/pipewire_helper"
reset_state_empty
run_layout "${PKG}/resources/scripts/check-virtual-mic.sh" -- --recreate
check "F1: pacote: helper de resources/bin foi invocado" helper_ran "${PKG}/resources/bin/pipewire_helper"
check "F1: pacote: systemd-run recebeu o helper de resources/bin" helper_arg_is "${PKG}/resources/bin/pipewire_helper"
check "F1: pacote: pw-loopback NAO foi chamado" no_loopback
check "F1: pacote: sem aviso de loopback" bash -c '! grep -qF -- "$1" "$2"' _ "${WARN_TEXT}" "${FAKE_DIR}/stdout.txt"

# F2: override CLEARCORE_HELPER_BIN tem precedencia sobre resources/bin
OVR_HELPER="${TMP_ROOT}/override/meu_helper"
make_fake_helper "${OVR_HELPER}"
reset_state_empty
run_layout "${PKG}/resources/scripts/check-virtual-mic.sh" "CLEARCORE_HELPER_BIN=${OVR_HELPER}" -- --recreate
check "F2: override: helper do override foi invocado" helper_ran "${OVR_HELPER}"
check "F2: override: resources/bin NAO foi usado" bash -c '! grep -qxF -- "invoked $1" "$2" 2>/dev/null' _ "${PKG}/resources/bin/pipewire_helper" "${FAKE_DIR}/helper_invoked.txt"
check "F2: override: pw-loopback NAO foi chamado" no_loopback

# F2b: override apontando para algo inexistente e ignorado (cai no pacote)
reset_state_empty
run_layout "${PKG}/resources/scripts/check-virtual-mic.sh" "CLEARCORE_HELPER_BIN=${TMP_ROOT}/nao/existe" -- --recreate
check "F2b: override invalido cai no helper do pacote" helper_ran "${PKG}/resources/bin/pipewire_helper"

# F3: layout de DESENVOLVIMENTO (scripts/ + platform/linux/helper/build/) continua funcionando
DEV="$(new_layout dev dev)"
make_fake_helper "${DEV}/platform/linux/helper/build/pipewire_helper"
reset_state_empty
run_layout "${DEV}/scripts/check-virtual-mic.sh" -- --recreate
check "F3: dev: helper de platform/linux/helper/build foi invocado" helper_ran "${DEV}/platform/linux/helper/build/pipewire_helper"
check "F3: dev: pw-loopback NAO foi chamado" no_loopback

# F4: pacote com os DOIS layouts presentes -> prefere resources/bin (ordem dos candidatos)
BOTH="$(new_layout ambos pacote)"
make_fake_helper "${BOTH}/resources/bin/pipewire_helper"
make_fake_helper "${BOTH}/resources/platform/linux/helper/build/pipewire_helper"
reset_state_empty
run_layout "${BOTH}/resources/scripts/check-virtual-mic.sh" -- --recreate
check "F4: ordem: resources/bin vence o layout de desenvolvimento" helper_ran "${BOTH}/resources/bin/pipewire_helper"

# F5: helper em lugar nenhum -> aviso no stderr, fallback pw-loopback continua, nao quebra
NONE="$(new_layout nenhum pacote)"
reset_state_empty
run_layout "${NONE}/resources/scripts/check-virtual-mic.sh" -- --recreate
check "F5: sem helper: aviso 'SEM supressao' no stderr" out_has "${WARN_TEXT}"
check "F5: sem helper: o fallback pw-loopback continua" has_loopback
check "F5: sem helper: o script nao quebra (sem erro de sintaxe/bash)" bash -c '! grep -qE "syntax error|command not found|unbound variable" "$1"' _ "${FAKE_DIR}/stdout.txt"

echo "----"
echo "${PASS} passaram, ${FAIL} falharam"
[[ "${FAIL}" -eq 0 ]]
