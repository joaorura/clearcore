#!/usr/bin/env bash
# Teste de scripts/detect-hardware.sh --json com ferramentas FALSAS.
#
# Nao toca no sistema real: lspci/ldconfig/lscpu/python3 falsos na frente do PATH, HOME
# falso, ambiente limpo (env -i) e CLEARCORE_DETECT_ROOT apontando para uma arvore
# temporaria (prefixo de /dev, /sys, /proc, /lib64, /usr/lib64, /opt, /home).
#
# Cenarios:
#  S0) nada instalado                                   -> cpu_tract, NPU runtime false
#  S1) OpenVINO FORA do ldconfig (caso Fedora com pacote Intel em /opt/intel):
#      a) so o Python do PATH importa openvino          -> openvino_npu
#      b) arquivos em /opt/intel/openvino*/runtime/lib/intel64
#      c) arquivos em ${INTEL_OPENVINO_DIR}
#      d) arquivos num diretorio de LD_LIBRARY_PATH
#      e) Python informado em CLEARCORE_PYTHON (o do PATH nao tem openvino)
#  S2) ldconfig lista libopenvino + Python importa openvino -> openvino_npu
#  S3) plugin NPU em /usr/lib64/openvino-X                  -> openvino_npu
#  S4) ldconfig ok mas Python sem openvino                  -> cpu_tract
#  S5) JSON valido (e so JSON no stdout) mesmo com lixo no stderr do lspci
#  S6) nenhum caminho fixo /home/<autor> no script
#
# Uso: platform/linux/tests/detect-hardware-fakes.test.sh
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
SCRIPT_UNDER_TEST="${REPO_ROOT}/scripts/detect-hardware.sh"
REAL_PY=/usr/bin/python3

TMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/detect-hardware-test.XXXXXX")"
trap 'find "${TMP_ROOT}" -delete' EXIT

FAKEBIN="${TMP_ROOT}/bin"
FAKE_DIR="${TMP_ROOT}/state"
FAKE_ROOT="${TMP_ROOT}/root"
FAKE_HOME="${TMP_ROOT}/home"
mkdir -p "${FAKEBIN}" "${FAKE_DIR}" "${FAKE_ROOT}" "${FAKE_HOME}"

# ---- ferramentas falsas -------------------------------------------------------
cat > "${FAKEBIN}/lspci" <<'EOF'
#!/bin/sh
[ -n "${FAKE_LSPCI_NOISE:-}" ] && echo "lspci: warning: lixo no stderr {not json}" >&2
cat "${FAKE_DIR}/lspci.txt" 2>/dev/null
EOF
cat > "${FAKEBIN}/ldconfig" <<'EOF'
#!/bin/sh
cat "${FAKE_DIR}/ldconfig.txt" 2>/dev/null
EOF
cat > "${FAKEBIN}/lscpu" <<'EOF'
#!/bin/sh
echo "Model name:                              Intel(R) Core(TM) Ultra 7 265H"
EOF
# python3 falso: tensorrt nunca importa; openvino importa se FAKE_OV_DEVICES nao for vazio.
cat > "${FAKEBIN}/python3" <<'EOF'
#!/bin/sh
case "$2" in
  *tensorrt*) exit 1 ;;
  *openvino*) if [ -n "${FAKE_OV_DEVICES:-}" ]; then echo "${FAKE_OV_DEVICES}"; exit 0; fi; exit 1 ;;
esac
exit 1
EOF
mkdir -p "${TMP_ROOT}/venv/bin"
cat > "${TMP_ROOT}/venv/bin/python3" <<'EOF'
#!/bin/sh
case "$2" in
  *openvino*) echo "CPU,GPU,NPU"; exit 0 ;;
esac
exit 1
EOF
chmod +x "${FAKEBIN}"/* "${TMP_ROOT}/venv/bin/python3"

PASS=0
FAIL=0
check() { # check "descricao" condicao...
    local desc="$1"; shift
    if "$@"; then echo "ok   - ${desc}"; PASS=$((PASS + 1)); else echo "FAIL - ${desc}"; FAIL=$((FAIL + 1)); fi
}

reset_state() {
    find "${FAKE_DIR}" -mindepth 1 -delete
    find "${FAKE_ROOT}" -mindepth 1 -delete
    find "${TMP_ROOT}/custom" -mindepth 1 -delete 2>/dev/null || true
    # Hardware: NPU Intel (Core Ultra) presente no barramento PCI.
    printf '00:0b.0 Processing accelerators [1200]: Intel Corporation Meteor Lake NPU [8086:7d1d] (rev 04)\n' > "${FAKE_DIR}/lspci.txt"
    : > "${FAKE_DIR}/ldconfig.txt"
}

# run_detect [VAR=valor ...]: roda o script em ambiente limpo; stdout -> out.json, stderr -> err.txt
run_detect() {
    env -i PATH="${FAKEBIN}:/usr/bin:/bin" HOME="${FAKE_HOME}" FAKE_DIR="${FAKE_DIR}" \
        CLEARCORE_DETECT_ROOT="${FAKE_ROOT}" "$@" \
        bash "${SCRIPT_UNDER_TEST}" --json >"${FAKE_DIR}/out.json" 2>"${FAKE_DIR}/err.txt"
}

jq_py() { "${REAL_PY}" -c 'import json,sys; d=json.load(open(sys.argv[1])); print(eval(sys.argv[2]))' "${FAKE_DIR}/out.json" "$1" 2>/dev/null; }
auto_id()     { jq_py 'd["auto_resolved_backend"]["id"]'; }
npu_runtime() { jq_py '[b for b in d["backends"] if b["id"]=="openvino_npu"][0]["runtime_installed"]'; }
valid_json()  { "${REAL_PY}" -c 'import json,sys; json.load(sys.stdin)' < "${FAKE_DIR}/out.json" 2>/dev/null; }

make_plugin() { # make_plugin <dir>: libopenvino.so + plugin NPU
    mkdir -p "$1"
    : > "$1/libopenvino.so.2510"
    : > "$1/libopenvino_intel_npu_plugin.so"
}

# ---- S0: nada instalado ------------------------------------------------------
reset_state
run_detect
check "S0: JSON valido" valid_json
check "S0: sem nada instalado, auto=cpu_tract" test "$(auto_id)" = "cpu_tract"
check "S0: sem nada instalado, NPU runtime=False" test "$(npu_runtime)" = "False"

# ---- S1a: so o Python do PATH importa openvino (fora do ldconfig) -------------
reset_state
run_detect FAKE_OV_DEVICES="CPU,GPU,NPU"
check "S1a: JSON valido" valid_json
check "S1a: Python do PATH prova o runtime, NPU runtime=True" test "$(npu_runtime)" = "True"
check "S1a: auto=openvino_npu" test "$(auto_id)" = "openvino_npu"

# ---- S1b: instalacao manual em /opt/intel/openvino_X ---------------------------
reset_state
make_plugin "${FAKE_ROOT}/opt/intel/openvino_2025.1.0/runtime/lib/intel64"
run_detect
check "S1b: /opt/intel/openvino*, NPU runtime=True" test "$(npu_runtime)" = "True"
check "S1b: auto=openvino_npu" test "$(auto_id)" = "openvino_npu"

# ---- S1c: INTEL_OPENVINO_DIR ----------------------------------------------------
reset_state
make_plugin "${TMP_ROOT}/custom/runtime/lib/intel64"
run_detect INTEL_OPENVINO_DIR="${TMP_ROOT}/custom"
check "S1c: INTEL_OPENVINO_DIR, NPU runtime=True" test "$(npu_runtime)" = "True"
check "S1c: auto=openvino_npu" test "$(auto_id)" = "openvino_npu"

# ---- S1d: LD_LIBRARY_PATH ---------------------------------------------------------
reset_state
make_plugin "${TMP_ROOT}/custom/ldlib"
run_detect LD_LIBRARY_PATH="/nao/existe:${TMP_ROOT}/custom/ldlib"
check "S1d: LD_LIBRARY_PATH, NPU runtime=True" test "$(npu_runtime)" = "True"
check "S1d: auto=openvino_npu" test "$(auto_id)" = "openvino_npu"

# ---- S1e: CLEARCORE_PYTHON ----------------------------------------------------------
reset_state
run_detect CLEARCORE_PYTHON="/nao/existe/python3:${TMP_ROOT}/venv/bin/python3"
check "S1e: CLEARCORE_PYTHON, NPU runtime=True" test "$(npu_runtime)" = "True"
check "S1e: auto=openvino_npu" test "$(auto_id)" = "openvino_npu"

# ---- S2: ldconfig + Python ------------------------------------------------------------
reset_state
echo "	libopenvino.so.2510 (libc6,x86-64) => /usr/lib64/libopenvino.so.2510" > "${FAKE_DIR}/ldconfig.txt"
run_detect FAKE_OV_DEVICES="CPU,GPU,NPU"
check "S2: ldconfig + python ok, auto=openvino_npu" test "$(auto_id)" = "openvino_npu"
check "S2: NPU runtime=True" test "$(npu_runtime)" = "True"

# ---- S3: plugin em /usr/lib64/openvino-X -------------------------------------------------
reset_state
make_plugin "${FAKE_ROOT}/usr/lib64/openvino-2025.1.0"
run_detect
check "S3: plugin em /usr/lib64/openvino-X, auto=openvino_npu" test "$(auto_id)" = "openvino_npu"
check "S3: NPU runtime=True" test "$(npu_runtime)" = "True"

# ---- S4: ldconfig ok mas Python sem openvino -----------------------------------------------
reset_state
echo "	libopenvino.so.2510 (libc6,x86-64) => /usr/lib64/libopenvino.so.2510" > "${FAKE_DIR}/ldconfig.txt"
run_detect
check "S4: ldconfig ok sem plugin e sem python, auto=cpu_tract" test "$(auto_id)" = "cpu_tract"
check "S4: NPU runtime=False" test "$(npu_runtime)" = "False"

# ---- S5: JSON valido mesmo com lixo no stderr do lspci ---------------------------------------
reset_state
run_detect FAKE_LSPCI_NOISE=1 FAKE_OV_DEVICES="CPU,GPU,NPU"
check "S5: stdout e JSON valido apesar do lixo no stderr do lspci" valid_json
check "S5: stdout nao contem o lixo" bash -c "! grep -q 'lixo no stderr' '${FAKE_DIR}/out.json'"

# ---- S6: sem caminhos fixos da maquina do autor ----------------------------------------------
check "S6: script sem '/home/joaorura'" bash -c "! grep -q '/home/joaorura' '${SCRIPT_UNDER_TEST}'"

echo
echo "Resultado: ${PASS} ok, ${FAIL} falhas"
[[ "${FAIL}" -eq 0 ]]
