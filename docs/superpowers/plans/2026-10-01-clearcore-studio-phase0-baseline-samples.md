# Clearcore Studio — Fase 0: baseline de CPU, tech-stack e amostras de teste — Plano de Implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Registrar o resultado real do benchmark oficial de CPU sem tocar no baseline commitado, documentar no `conductor/tech-stack.md` a stack que as fases 1 a 4 vão introduzir, e entregar o script e o tutorial que baixam amostras de voz e ruído (com licença e SHA-256 conferidos) para `fixtures/voice-samples/`.

**Architecture:** Nenhum código Rust muda nesta fase. São três entregas independentes: (a) um wrapper de shell que roda o `benchmark` numa cópia descartável de `HEAD` e guarda o relatório em `benchmarks/local/` (ignorado pelo git); (b) texto novo em `conductor/tech-stack.md`; (c) `scripts/fetch-voice-samples.sh` + lista de hashes + tutorial, com áudio sempre fora do git. Uma tarefa de pesquisa fecha a fase confirmando licença e taxa de amostragem de Common Voice pt e MLS Portuguese na fonte oficial.

**Tech Stack:** Bash (`set -euo pipefail`), `curl`, `ffmpeg`/`ffprobe`, `jq`, `sha256sum`, Python 3 `venv` + `remotezip` (só dentro de `fixtures/voice-samples/.venv`), cargo 1.90.0 (somente para rodar o benchmark existente).

**Spec:** `docs/superpowers/specs/2026-10-01-clearcore-studio-pipeline-design.md` (seções 8 e 9; a seção 10 traz as ressalvas do fork do libDF usadas na Tarefa 2).

## Global Constraints

- Sem commit e sem push: cada tarefa termina com um **Checkpoint** (`git status --short`), nunca com `git commit` (spec seção 9).
- "Áudio nunca vai para o git" (spec seção 8): `fixtures/voice-samples/` fica no `.gitignore`; só o script, a lista de hashes e o tutorial são versionados.
- "Testes que usam áudio **pulam** quando os arquivos faltam (CI offline segue verde)" (spec seção 8).
- TAGARELA é **CC BY-NC-SA 4.0**: só dado de teste local, baixado apenas com a flag explícita `--with-tagarela`, fora do conjunto padrão, nunca commitado, sem extração de áudio, fora do treino de pesos distribuídos (spec seção 8).
- VCTK Corpus 0.92 é **CC BY 4.0**, com atribuição obrigatória (CSTR VCTK Corpus v0.92, University of Edinburgh); ruídos do Wikimedia Commons são **CC0** (spec seção 8).
- Política do projeto: `MIT OR Apache-2.0`; workspace com `unsafe_code = "forbid"` e toolchain `1.90.0` (`conductor/tech-stack.md`).
- `conductor/workflow.md`, princípio 2: "Changes to the tech stack must be documented in `tech-stack.md` *before* implementation".
- Nunca inventar número de latência: se o benchmark termina `BLOCKED_*`, o status é registrado literalmente.
- A única escrita no repositório que não esteja listada nos `Files` de uma tarefa é proibida; **não** sobrescrever `benchmarks/cpu-baseline.json` nem `benchmarks/cpu-baseline.md` commitados.
- Cada tarefa fecha com `shellcheck` quando existir; senão com `bash -n` (nesta máquina de planejamento o `shellcheck` não está instalado: use `bash -n`).
- Texto de docs em português do Brasil; código e identificadores em inglês.
- Antes de qualquer fase fechar (spec seção 9): `cargo fmt --check`, `clippy -D warnings`, `cargo test --locked --offline`, `scripts/check-offline.sh` e revisão de código. Esta fase não altera Rust, mas o gate final (Tarefa 4, Step 5) roda `scripts/check-offline.sh` para provar que nada quebrou.

---

### Task 1: Benchmark oficial de CPU, registrado sem sobrescrever o baseline commitado

**Contexto verificado (2026-10-02):**

- O binário `crates/tools/src/bin/benchmark.rs` usa `env::current_dir()` como raiz (linha 88) e **grava** `benchmarks/cpu-baseline.json` e `benchmarks/cpu-baseline.md` relativos a ela (linhas 454-461). Rodá-lo na raiz do repositório sobrescreve os arquivos commitados. Por isso o wrapper desta tarefa roda numa cópia de `HEAD` (`git archive`) e só então copia o relatório para `benchmarks/local/`.
- Argumentos aceitos (`validate_arguments`, linhas 471-512): exatamente `--backend tract --profile avx2-minimum --duration 300`, nessa ordem; opcionalmente, **os três juntos**, `--host-evidence <json> --host-evidence-provenance <registro> --run-id <id>`. Passar só `--run-id` falha com a mensagem de uso. Caminhos relativos de evidência são resolvidos a partir da raiz da cópia, então use caminhos absolutos.
- A duração mínima aceita pelo código é 300 s, mas esses 300 s só são gastos se o host for qualificado. Sem evidência o binário termina em ~0,05 s com status `BLOCKED_UNSUPPORTED_CPU_PROFILE` e **não mede latência alguma** (`measurements` traz só `reason` e `allocation` com `null`).
- **Forma legítima de fornecer `--host-evidence`** (`crates/tools/src/host_evidence.rs`): o documento (`schema_version` 2) e o registro de proveniência só são aceitos se: o host real for `x86_64`, `cpu_model` contendo `i5-10210U`, 4 núcleos físicos, AVX2, RAM >= 8 GiB e **não virtualizado**; o `run_id` do documento for igual ao `--run-id`; a observação durar exatamente 300 s e terminar até 300 s antes do início do benchmark; AC ligado, `governor` uniforme, `affinity_cpus` canônico e idêntico ao observado localmente; 301 snapshots de frequência (1 Hz, todas as CPUs lógicas, >= 2000 MHz); e `provenance.source_sha256` igual ao SHA-256 do registro. O coletor citado nos testes (`qualification-host-observer observe --seconds 300`) **não existe neste repositório** (só aparece em `crates/tools/tests/support/mod.rs`). Logo, o benchmark qualificado só roda numa máquina de referência i5-10210U, com o coletor externo, e nunca se fabrica o arquivo de evidência. Este host (Intel Core Ultra 7 265H, 16 núcleos) nunca será "qualificado", e o resultado correto aqui é o `BLOCKED` registrado literalmente.

**Files:**
- Create: `scripts/run-cpu-baseline-local.sh`
- Create: `benchmarks/local/README.md`
- Modify: `.gitignore` (acrescentar ao final)
- Generated, não versionado: `benchmarks/local/cpu-baseline-<UTC>.json` e `.md`

**Interfaces:**
- Consumes: o binário existente `benchmark` (pacote `realtime-noise-tools`); `git`, `tar`, `jq`, `cargo` no PATH.
- Produces: `scripts/run-cpu-baseline-local.sh [--host-evidence <abs> --host-evidence-provenance <abs> --run-id <id>]`, com variáveis opcionais `OUT_DIR` (padrão `<repo>/benchmarks/local`) e `CARGO_TARGET_DIR`. Escreve `cpu-baseline-<YYYYMMDDTHHMMSSZ>.json|md` em `OUT_DIR`.

- [ ] **Step 1: Criar o wrapper**

Crie `scripts/run-cpu-baseline-local.sh` com exatamente este conteúdo e dê permissão de execução (`chmod +x scripts/run-cpu-baseline-local.sh`):

```bash
#!/usr/bin/env bash
# Runs the official CPU benchmark (crates/tools/src/bin/benchmark.rs) in a disposable
# copy of HEAD and stores the report under benchmarks/local/ (git-ignored).
#
# Why a copy: the benchmark binary writes benchmarks/cpu-baseline.{json,md} relative to
# the current directory, which would overwrite the committed baseline.
#
# Usage: scripts/run-cpu-baseline-local.sh [--host-evidence <abs json> --host-evidence-provenance <abs record> --run-id <id>]
# Environment: OUT_DIR (default <repo>/benchmarks/local), CARGO_TARGET_DIR (default <repo>/target).
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
OUT_DIR="${OUT_DIR:-$ROOT/benchmarks/local}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/clearcore-bench.XXXXXX")"

echo "work copy: $WORK"
git -C "$ROOT" archive HEAD | tar -x -C "$WORK"

cd "$WORK"
status_line="$(cargo run --release --locked -p realtime-noise-tools --bin benchmark -- \
  --backend tract --profile avx2-minimum --duration 300 "$@" || true)"
echo "benchmark stdout: $status_line"

stamp="$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$OUT_DIR"
cp "$WORK/benchmarks/cpu-baseline.json" "$OUT_DIR/cpu-baseline-$stamp.json"
cp "$WORK/benchmarks/cpu-baseline.md" "$OUT_DIR/cpu-baseline-$stamp.md"
echo "saved: $OUT_DIR/cpu-baseline-$stamp.json"
jq '{status, run_id, cpu_model: .host.cpu_model, reference_class_qualified: .host.reference_class_qualified, rejection_reason: .host.rejection_reason, measurements}' \
  "$OUT_DIR/cpu-baseline-$stamp.json"
```

- [ ] **Step 2: Validar a sintaxe**

Run: `command -v shellcheck && shellcheck scripts/run-cpu-baseline-local.sh || bash -n scripts/run-cpu-baseline-local.sh`
Expected: sem saída de erro (sem `shellcheck` instalado, só o `bash -n` roda e não imprime nada).

- [ ] **Step 3: Ignorar a saída local e documentá-la**

Acrescente ao final de `.gitignore`:

```gitignore
# Saída local do benchmark de CPU (específica da máquina; ver benchmarks/local/README.md)
benchmarks/local/*
!benchmarks/local/README.md
```

Crie `benchmarks/local/README.md`:

```markdown
# Resultados locais do benchmark de CPU

Esta pasta guarda relatórios de `scripts/run-cpu-baseline-local.sh`. Os arquivos
`cpu-baseline-*.json|md` são específicos da máquina e ficam fora do git (ver `.gitignore`).
O baseline commitado continua sendo `benchmarks/cpu-baseline.json`; ele nunca é sobrescrito
por este fluxo.

## Como rodar

    scripts/run-cpu-baseline-local.sh

O script roda o benchmark oficial numa cópia descartável de `HEAD` (o binário grava
`benchmarks/cpu-baseline.*` relativo ao diretório atual).

## Status possíveis

- `BLOCKED_UNSUPPORTED_CPU_PROFILE`: o host não apresentou evidência qualificada. Nenhuma latência
  é medida e nenhum número deve ser inferido deste relatório. Evidência qualificada exige uma
  máquina de referência (x86_64, `i5-10210U`, 4 núcleos físicos, sem virtualização), um registro
  de observação de 300 s gerado por coletor externo e os três argumentos
  `--host-evidence`, `--host-evidence-provenance` e `--run-id` (caminhos absolutos).
- `BLOCKED_NO_APPROVED_ASSET`, `BLOCKED_PENDING_GOLDEN`: o asset aprovado ou o golden não foi
  verificado; o campo `measurements.reason` traz o motivo.
- `M1_APPROVED`: único status com latência medida.

O relatório mede só a latência do worker de inferência, não a latência fim a fim do produto.
```

- [ ] **Step 4: Rodar o benchmark de verdade (uma vez)**

Run: `scripts/run-cpu-baseline-local.sh`
(A primeira compilação leva cerca de 2,5 minutos; o benchmark em si termina em menos de 1 s quando bloqueado. Use `timeout 900` se rodar sob automação.)

Saída esperada de referência (observada em 2026-10-02 neste host; `run_id` e carimbos variam):

```text
benchmark stdout: {"report":"benchmarks/cpu-baseline.json","status":"BLOCKED_UNSUPPORTED_CPU_PROFILE"}
saved: .../benchmarks/local/cpu-baseline-20261002T111201Z.json
{
  "status": "BLOCKED_UNSUPPORTED_CPU_PROFILE",
  "run_id": "task4-1790939520",
  "cpu_model": "Intel(R) Core(TM) Ultra 7 265H",
  "reference_class_qualified": false,
  "rejection_reason": "host evidence was not supplied",
  "measurements": {
    "allocation": {
      "available": true,
      "initialization_bytes": null,
      "initialization_count": null,
      "measurement_completed": false,
      "mechanism": "stats_alloc-0.1.10",
      "per_hop_bytes": null,
      "per_hop_count": null,
      "total_bytes": null,
      "total_count": null,
      "verification_bytes": null,
      "verification_count": null,
      "warm_up_bytes": null,
      "warm_up_count": null
    },
    "reason": "host evidence was not supplied"
  }
}
```

O código de saída do binário é 2 para qualquer status diferente de `M1_APPROVED`; o wrapper ignora esse código de propósito (`|| true`) porque `BLOCKED_*` é resultado válido. Se o status impresso for diferente do esperado, **registre o status real literalmente** e não "corrija" nada.

- [ ] **Step 5: Provar que o baseline commitado não foi tocado**

Run: `git diff --stat -- benchmarks/cpu-baseline.json benchmarks/cpu-baseline.md && git status --short benchmarks`
Expected: `git diff --stat` sem saída; `git status --short benchmarks` lista só `?? ` ou nada para `benchmarks/local/README.md` (o JSON/MD gerados não aparecem, estão ignorados).

- [ ] **Step 6: Registrar o resultado**

Anote, no relatório final da tarefa, o caminho do `cpu-baseline-<UTC>.json` gerado e o `status` literal. Como o benchmark não mediu latência, a única referência de custo por frame vigente continua sendo a do spike da fase 3 (p50 656 µs e p99 0,74 ms em `taskset -c 2`, governor `performance`; válida só naquela máquina e **não** fim a fim; ver `docs/superpowers/plans/2026-10-01-clearcore-studio-phase3-libdf-spike-report.md`). Não escreva nenhum outro número de latência.

- [ ] **Step 7: Checkpoint**

Run: `git status --short`
Expected: aparecem `?? benchmarks/local/`, `?? scripts/run-cpu-baseline-local.sh` e ` M .gitignore` (além de arquivos de outras fases, se houver). **Não commitar.**

---

### Task 2: Documentar a stack do pipeline "estúdio" em `conductor/tech-stack.md`

`conductor/workflow.md`, princípio 2, exige registrar mudança de stack em `tech-stack.md` **antes** da implementação. O arquivo existente está em inglês; o texto novo segue a regra de idioma do usuário (português do Brasil, termos técnicos em inglês). Esta tarefa não toca em código.

**Files:**
- Modify: `conductor/tech-stack.md` (duas inserções: lista de crates na seção 1 e nova seção 6 ao final)

**Interfaces:**
- Consumes: nomes e números do spec (`StudioChain`, `Preset`, `StudioBackend<B: InferenceBackend>`, patch do libDF +127/-21).
- Produces: a seção `## 6. Studio Pipeline (planejado)`, citada pelas fases 1 a 4 como registro da decisão de stack.

- [ ] **Step 1: Registrar os crates planejados na seção 1**

Em `conductor/tech-stack.md`, localize a linha:

```markdown
  - `crates/service`: Background OS user session service.
```

e acrescente logo abaixo (mesma indentação):

```markdown
  - `crates/studio-dsp` *(planejado, fase 1)*: Cadeia DSP de acabamento "estúdio" (Rust puro, sem dependências externas).
  - `crates/libdf-fork` *(planejado, fase 4; nome provisório do crate isolado)*: Fork vendorizado do libDF com entradas FiLM `gamma/beta` (ver seção 6).
```

- [ ] **Step 2: Acrescentar a seção 6 ao final do arquivo**

Acrescente exatamente este bloco depois da última linha do arquivo (a de `Supply Chain`), deixando uma linha em branco antes:

```markdown

## 6. Studio Pipeline (planejado)
Decisões de stack registradas antes da implementação (`conductor/workflow.md`, princípio 2). Fonte: `docs/superpowers/specs/2026-10-01-clearcore-studio-pipeline-design.md`. Nada desta seção está implementado até a fase correspondente fechar.

- **`studio-dsp` (fase 1):** crate Rust puro, **sem dependências externas**, com `#![forbid(unsafe_code)]` e os lints do workspace (`unwrap`/`expect`/`panic` em `deny`). Cadeia, nesta ordem: high-pass 80 Hz, EQ fixo (2 biquads), de-esser (passa-banda 5-9 kHz com envelope), compressor, AGC de loudness (ITU-R BS.1770, alvo -16 LUFS) e limiter com lookahead e teto de -1 dBFS. Contrato: `StudioChain::new(Preset)`, `process(&mut self, &mut [f32; 480])` sem alocação e sem panic, e `latency_samples()` (só o lookahead do limiter, cerca de 96 amostras ou 2 ms). Nunca produz NaN nem infinito.
- **`Preset`:** `Off`, `Natural`, `Podcast`, `Broadcast`, em tabela constante. `Off` é **passagem bit a bit idêntica** e é o padrão do produto. A troca de preset usa valor atômico compartilhado, aplicado no limite de frame com crossfade de cerca de 10 ms. Persistência em `settings.json` versionado (fase 2).
- **`StudioBackend<B: InferenceBackend>` (fase 1):** decorator que implementa a mesma trait `InferenceBackend`: `process` chama o backend interno e depois a cadeia, e `algorithmic_latency_samples` soma a latência da cadeia. O engine e o `filter-capi` apenas embrulham o backend que já criam; **a trait não muda**. Em `Bypass` o áudio passa cru e em `Mute` sai zero; o estúdio só roda em `Active`.
- **Fork do libDF vendorizado (fase 4):** hoje a dependência `deep_filter` v0.5.6 é por `git` + `rev`, o que exige rede e conflita com o gate offline. O spike da fase 3 (GO, paridade bit-exata em identidade, custo +0,1%) mostrou que o backend personalizado precisa de um fork com entradas `gamma/beta` (patch de +127/-21 linhas, ou seja, não "mínimo"), que passa a ser **vendorizado dentro do repositório**.
  - **Ressalva de `unsafe`:** o código do fork contém `unsafe`, em conflito com `unsafe_code = "forbid"` do workspace (`forbid` não pode ser relaxado por `allow` dentro de um crate que herda `[lints] workspace = true`). A exceção só é aceita num **crate isolado**, que não herda os lints do workspace, com a exceção escrita e justificada no próprio `Cargo.toml` e neste arquivo. Todo o restante do workspace continua em `forbid`.
  - **Licença:** o libDF é `MIT OR Apache-2.0`, compatível com a política do projeto. É obrigatório manter `LICENSE-MIT` e `LICENSE-APACHE` e o aviso de copyright do upstream dentro do diretório vendorizado e **marcar os arquivos modificados**.
  - Os assets neurais novos (pDFNet3, enrollment, EQ neural) entram por um registro de descritores com a mesma verificação de assinatura; o DFNet3 v1 atual segue aprovado e é o fallback.
```

- [ ] **Step 3: Conferir o texto inserido**

Run: `grep -c -E "studio-dsp|StudioBackend|Preset|libDF|forbid|MIT OR Apache-2.0" conductor/tech-stack.md`
Expected: um número maior ou igual a 8.

Run: `git diff --stat -- conductor/tech-stack.md`
Expected: apenas `conductor/tech-stack.md` com linhas adicionadas e nenhuma removida (sem a palavra `deletion` na saída).

- [ ] **Step 4: Checkpoint**

Run: `git status --short`
Expected: ` M conductor/tech-stack.md` aparece (além dos itens da Tarefa 1). **Não commitar.**

---

### Task 3: Script de amostras de voz, lista de hashes, tutorial e `.gitignore`

**Decisões (verificadas em 2026-10-02 contra a rede e os dados já baixados):**

- **Ferramenta de download parcial:** `remotezip` 0.12.6 (MIT), instalada num **venv local** `fixtures/voice-samples/.venv` (ignorado pelo git, porque a pasta inteira está no `.gitignore`). Nada é instalado no sistema nem com `pip install --user`. O script cria o venv sob demanda, só quando faltam arquivos do VCTK.
- **Modo offline:** `--from-dir DIR` aceita arquivos já baixados (`DIR/<nome>`, `DIR/speech/<nome>` ou `DIR/noise/<nome>`), confere os hashes e dispensa rede e venv. Serve para CI local e para o executor reaproveitar os dados da sessão de pesquisa.
- **Originais vs. derivados:** os FLAC do VCTK são originais (hash divergente **aborta** e remove o arquivo). Os clipes de ruído `noise/*.wav` são recortes feitos com `ffmpeg` (8.1.3 reproduziu os três hashes bit a bit); hash divergente por versão de `ffmpeg` só **avisa** (`WARNING`) e o script termina com código 0.
- **TAGARELA:** o caminho real do shard de teste é `data/test-00000-of-00001.parquet` (e não a raiz do repositório): `https://huggingface.co/datasets/freds0/TAGARELA/resolve/main/data/test-00000-of-00001.parquet`. Conferido por `curl -sIL` e pela API do Hugging Face: `content-length`/`x-linked-size` = 78277756 bytes, licença `cc-by-nc-sa-4.0`, `gated: false`, SHA-256 LFS `0b104a616d3f4f7c5d645f998e46aa4f14f9b0b5525f942a2aef4d716f8709ef` (confirmado ao baixar o arquivo inteiro). O script confere tamanho e SHA-256 e **não extrai áudio**.
- **Ruído CC0:** `street_traffic_rain` usa os primeiros 6 000 000 bytes (HTTP Range) e corta de 5 s a 15 s; `forest_rain`, os primeiros 5 000 000 bytes, de 0 a 10 s; `ac_fan`, o arquivo `.ogg` inteiro (225 434 bytes), de 5 s a 15 s. Saída: 10 s, mono, PCM s16le, 48 kHz.
- `shellcheck` não está instalado nesta máquina; o script passou em `shellcheck 0.11.0` (instalado num venv descartável) e em `bash -n`. O executor usa `shellcheck` se existir e `bash -n` caso contrário.

**Files:**
- Create: `scripts/fetch-voice-samples.sh`
- Create: `scripts/voice-samples.sha256`
- Create: `docs/testing/voice-samples.md`
- Modify: `.gitignore` (acrescentar `fixtures/voice-samples/`)
- Generated, não versionado: `fixtures/voice-samples/{speech,noise,tagarela}/`, `fixtures/voice-samples/ATTRIBUTION.txt`, `fixtures/voice-samples/.venv/`

**Interfaces:**
- Consumes: `curl`, `ffmpeg`, `ffprobe`, `sha256sum` (ou `shasum -a 256`), `python3` com `venv`, `remotezip==0.12.6` (instalado no venv).
- Produces: `scripts/fetch-voice-samples.sh [--with-tagarela] [--from-dir DIR] [--verify-only]` (variáveis `FIXTURES_DIR` e `HASH_FILE`); layout `fixtures/voice-samples/speech/p225_{003,008,011,022}_mic1.flac`, `speech/p226_{008,016}_mic1.flac`, `noise/{street_traffic_rain,forest_rain,ac_fan}_cc0_10s.wav`, `tagarela/test-00000-of-00001.parquet`. Testes de fases futuras localizam as amostras por esses caminhos relativos e **pulam** se faltarem. A `p225_011` (6,8 s) é a fala de enrollment.

- [ ] **Step 1: Criar o script**

Crie `scripts/fetch-voice-samples.sh` com exatamente este conteúdo e `chmod +x scripts/fetch-voice-samples.sh`:

```bash
#!/usr/bin/env bash
# Downloads the voice/noise test samples into fixtures/voice-samples/ (git-ignored) and
# verifies SHA-256 against scripts/voice-samples.sha256. Tutorial: docs/testing/voice-samples.md
#
# Usage: scripts/fetch-voice-samples.sh [--with-tagarela] [--from-dir DIR] [--verify-only]
#   --with-tagarela  also fetch the TAGARELA test shard (CC BY-NC-SA 4.0, local test data only)
#   --from-dir DIR   take already-downloaded files from DIR instead of the network
#                    (DIR/<name> or DIR/speech/<name> or DIR/noise/<name>)
#   --verify-only    download nothing; only check the hashes of the files present
# Environment: FIXTURES_DIR (default <repo>/fixtures/voice-samples), HASH_FILE.
# Needs: curl, sha256sum (or shasum), ffmpeg/ffprobe (noise clips), python3 with venv (VCTK range download).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FIXTURES_DIR="${FIXTURES_DIR:-$ROOT/fixtures/voice-samples}"
HASH_FILE="${HASH_FILE:-$ROOT/scripts/voice-samples.sha256}"
VENV_DIR="$FIXTURES_DIR/.venv"
REMOTEZIP_VERSION="0.12.6"
USER_AGENT="clearcore-voice-samples/1.0"

VCTK_ZIP_URL="https://datashare.ed.ac.uk/server/api/core/bitstreams/535f4286-e54c-4038-838c-a02285e32cb2/content"
VCTK_PREFIX="wav48_silence_trimmed"
SPEECH_FILES=(p225_003 p225_008 p225_011 p225_022 p226_008 p226_016)

# name|url|range_bytes (0 = whole file)|start_seconds|raw_extension
NOISE_CLIPS=(
  "street_traffic_rain_cc0_10s|https://upload.wikimedia.org/wikipedia/commons/8/8c/Urban_Street_on_a_Rainy_Afternoon.flac|6000000|5|flac"
  "forest_rain_cc0_10s|https://upload.wikimedia.org/wikipedia/commons/b/b6/Light_Rain_Distant_Thunder_July_5th_2016.wav|5000000|0|wav"
  "ac_fan_cc0_10s|https://upload.wikimedia.org/wikipedia/commons/d/df/Resident_air-conditioned_out_door_unit.ogg|0|5|ogg"
)

TAGARELA_URL="https://huggingface.co/datasets/freds0/TAGARELA/resolve/main/data/test-00000-of-00001.parquet"
TAGARELA_REL="tagarela/test-00000-of-00001.parquet"
TAGARELA_BYTES=78277756

WITH_TAGARELA=0
FROM_DIR=""
VERIFY_ONLY=0
WARNINGS=0

die() { echo "ERROR: $*" >&2; exit 1; }
warn() { echo "WARNING: $*" >&2; WARNINGS=$((WARNINGS + 1)); }

usage() { sed -n '2,11p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; }

while [ $# -gt 0 ]; do
  case "$1" in
    --with-tagarela) WITH_TAGARELA=1 ;;
    --verify-only) VERIFY_ONLY=1 ;;
    --from-dir)
      [ $# -ge 2 ] || die "--from-dir needs a directory"
      FROM_DIR="$2"
      shift
      ;;
    -h | --help) usage; exit 0 ;;
    *) usage >&2; die "unknown argument: $1" ;;
  esac
  shift
done

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

expected_hash() { # $1 = path relative to FIXTURES_DIR
  [ -f "$HASH_FILE" ] || die "hash list not found: $HASH_FILE"
  awk -v p="$1" '$0 !~ /^#/ && $2 == p { print $1 }' "$HASH_FILE"
}

# Returns 0 = hash matches, 1 = mismatch, 2 = file missing or no expected hash.
check_file() {
  local rel="$1" want got
  [ -f "$FIXTURES_DIR/$rel" ] || return 2
  want="$(expected_hash "$rel")"
  [ -n "$want" ] || return 2
  got="$(sha256_of "$FIXTURES_DIR/$rel")"
  [ "$got" = "$want" ]
}

find_in_from_dir() { # $1 = file name, $2 = subfolder
  local candidate
  [ -n "$FROM_DIR" ] || return 1
  for candidate in "$FROM_DIR/$1" "$FROM_DIR/$2/$1"; do
    if [ -f "$candidate" ]; then echo "$candidate"; return 0; fi
  done
  return 1
}

# Originals must match exactly: a mismatch aborts.
require_original() {
  local rel="$1"
  check_file "$rel" || {
    rm -f -- "${FIXTURES_DIR:?}/$rel"
    die "SHA-256 mismatch for $rel (file removed)"
  }
  echo "ok   $rel"
}

# Derived clips depend on the ffmpeg version: a mismatch only warns.
check_derived() {
  local rel="$1" status=0
  check_file "$rel" || status=$?
  if [ "$status" -eq 0 ]; then
    echo "ok   $rel"
  else
    warn "derived clip $rel differs from the recorded SHA-256 (depends on the ffmpeg version); keeping it"
  fi
}

write_attribution() {
  cat >"$FIXTURES_DIR/ATTRIBUTION.txt" <<'EOF'
Speech (speech/*.flac): CSTR VCTK Corpus v0.92, University of Edinburgh, The Centre for Speech
Technology Research (CSTR); Yamagishi et al. License CC BY 4.0 (attribution required).
https://datashare.ed.ac.uk/handle/10283/3443

Noise (noise/*.wav): 10 s mono 48 kHz PCM s16le excerpts of Wikimedia Commons recordings, CC0:
- street_traffic_rain_cc0_10s: https://commons.wikimedia.org/wiki/File:Urban_Street_on_a_Rainy_Afternoon.flac (5 s to 15 s)
- forest_rain_cc0_10s: https://commons.wikimedia.org/wiki/File:Light_Rain_Distant_Thunder_July_5th_2016.wav (0 s to 10 s)
- ac_fan_cc0_10s: https://commons.wikimedia.org/wiki/File:Resident_air-conditioned_out_door_unit.ogg (5 s to 15 s)

tagarela/ (only with --with-tagarela): freds0/TAGARELA test shard, CC BY-NC-SA 4.0. Local test
data only: never commit, never redistribute, never use to train distributed weights.
EOF
}

mkdir -p "$FIXTURES_DIR/speech" "$FIXTURES_DIR/noise"
TMP_DIR=""
cleanup() { if [ -n "$TMP_DIR" ] && [ -d "$TMP_DIR" ]; then rm -rf -- "$TMP_DIR"; fi; }
trap cleanup EXIT

if [ "$VERIFY_ONLY" -eq 0 ]; then
  TMP_DIR="$(mktemp -d "$FIXTURES_DIR/.tmp.XXXXXX")"
fi

# ---------------------------------------------------------------- speech (VCTK)
missing_members=()
for id in "${SPEECH_FILES[@]}"; do
  name="${id}_mic1.flac"
  rel="speech/$name"
  if [ "$VERIFY_ONLY" -eq 1 ]; then
    status=0; check_file "$rel" || status=$?
    [ "$status" -eq 0 ] || die "$rel missing or with wrong SHA-256"
    echo "ok   $rel"
    continue
  fi
  status=0; check_file "$rel" || status=$?
  if [ "$status" -eq 0 ]; then echo "ok   $rel (already present)"; continue; fi
  if src="$(find_in_from_dir "$name" speech)"; then
    cp -- "$src" "$FIXTURES_DIR/$rel"
    require_original "$rel"
  else
    missing_members+=("$VCTK_PREFIX/${id%%_*}/$name")
  fi
done

if [ "${#missing_members[@]}" -gt 0 ]; then
  command -v python3 >/dev/null 2>&1 || die "python3 is required for the VCTK range download"
  if [ ! -x "$VENV_DIR/bin/remotezip" ]; then
    echo "creating venv $VENV_DIR (remotezip $REMOTEZIP_VERSION)"
    python3 -m venv "$VENV_DIR" || die "python3 -m venv failed (on Debian/Ubuntu install python3-venv)"
    "$VENV_DIR/bin/pip" install --quiet "remotezip==$REMOTEZIP_VERSION"
  fi
  echo "fetching ${#missing_members[@]} VCTK files by HTTP Range (the full zip is 11.7 GB; it is NOT downloaded)"
  "$VENV_DIR/bin/remotezip" -d "$TMP_DIR/vctk" "$VCTK_ZIP_URL" "${missing_members[@]}"
  for member in "${missing_members[@]}"; do
    name="$(basename "$member")"
    [ -f "$TMP_DIR/vctk/$member" ] || die "remotezip did not extract $member"
    mv -- "$TMP_DIR/vctk/$member" "$FIXTURES_DIR/speech/$name"
    require_original "speech/$name"
  done
fi

# ---------------------------------------------------------------- noise clips
for spec in "${NOISE_CLIPS[@]}"; do
  IFS='|' read -r name url range_bytes start_seconds raw_ext <<<"$spec"
  rel="noise/$name.wav"
  if [ "$VERIFY_ONLY" -eq 1 ]; then
    [ -f "$FIXTURES_DIR/$rel" ] || die "$rel missing"
    status=0; check_file "$rel" || status=$?
    if [ "$status" -eq 0 ]; then echo "ok   $rel"; else warn "derived clip $rel differs from the recorded SHA-256"; fi
    continue
  fi
  status=0; check_file "$rel" || status=$?
  if [ "$status" -eq 0 ]; then echo "ok   $rel (already present)"; continue; fi
  if src="$(find_in_from_dir "$name.wav" noise)"; then
    cp -- "$src" "$FIXTURES_DIR/$rel"
    check_derived "$rel"
    continue
  fi
  for tool in ffmpeg ffprobe; do
    command -v "$tool" >/dev/null 2>&1 || die "$tool is required to cut the noise clips"
  done
  raw="$TMP_DIR/$name.$raw_ext"
  if [ "$range_bytes" -gt 0 ]; then
    curl -fsSL --retry 3 -A "$USER_AGENT" -r "0-$((range_bytes - 1))" -o "$raw" "$url"
  else
    curl -fsSL --retry 3 -A "$USER_AGENT" -o "$raw" "$url"
  fi
  ffmpeg -nostdin -v error -y -ss "$start_seconds" -t 10 -i "$raw" -ac 1 -ar 48000 -c:a pcm_s16le "$FIXTURES_DIR/$rel"
  duration="$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$FIXTURES_DIR/$rel")"
  awk -v d="$duration" 'BEGIN { exit !(d >= 9.99 && d <= 10.01) }' || die "clip $rel has duration $duration s, expected 10 s"
  check_derived "$rel"
done

# ---------------------------------------------------------------- TAGARELA (opt-in)
if [ "$WITH_TAGARELA" -eq 1 ]; then
  echo "TAGARELA is CC BY-NC-SA 4.0: local test data only, never commit it (no audio is extracted)."
  dest="$FIXTURES_DIR/$TAGARELA_REL"
  if [ "$VERIFY_ONLY" -eq 0 ]; then
    mkdir -p "$FIXTURES_DIR/tagarela"
    if [ ! -f "$dest" ] || [ "$(wc -c <"$dest" | tr -d ' ')" != "$TAGARELA_BYTES" ]; then
      curl -fSL --progress-bar --retry 3 -A "$USER_AGENT" -C - -o "$dest.part" "$TAGARELA_URL"
      mv -- "$dest.part" "$dest"
    fi
  fi
  [ -f "$dest" ] || die "$TAGARELA_REL missing"
  [ "$(wc -c <"$dest" | tr -d ' ')" = "$TAGARELA_BYTES" ] || die "$TAGARELA_REL has the wrong size (expected $TAGARELA_BYTES bytes)"
  check_file "$TAGARELA_REL" || die "SHA-256 mismatch for $TAGARELA_REL"
  echo "ok   $TAGARELA_REL"
fi

write_attribution
if [ "$WARNINGS" -gt 0 ]; then
  echo "done with $WARNINGS warning(s)"
else
  echo "done"
fi
```

- [ ] **Step 2: Criar a lista de hashes**

Crie `scripts/voice-samples.sha256` com exatamente este conteúdo (formato do `sha256sum`; linhas iniciadas por `#` são comentários):

```text
# SHA-256 of the files fetched by scripts/fetch-voice-samples.sh (paths relative to fixtures/voice-samples/).
# Format: "<sha256>  <path>" (same as sha256sum). Verified 2026-10-02.
# speech/*: originals from VCTK Corpus 0.92 (CC BY 4.0); a mismatch aborts the script.
# noise/*: 10 s clips cut with ffmpeg 8.1.3 from CC0 Wikimedia Commons files; a mismatch only warns
#          (the bytes depend on the ffmpeg version).
# tagarela/*: only checked with --with-tagarela (CC BY-NC-SA 4.0, local test data); the hash is the
#             Hugging Face LFS oid of the file.
877a8327608bd37b82d125adca98bdf7c92c904dff8c58fb711d7d9be364dc99  speech/p225_003_mic1.flac
c4c4d3e1e128821c364db33fcec4b708e36edcea3bbc70f5334d6715aa4e00f9  speech/p225_008_mic1.flac
8eaa96bb597b8ea88198b35da6f4a4e546125bc92c114e0aaee1cd87e7f08bbb  speech/p225_011_mic1.flac
a4a88b254b17544e05fb259ded18c294b133abda8ff515ae9c72e842b64ec9d0  speech/p225_022_mic1.flac
9570b6f075670d731ed0f0e0097db48988972b7a541e36da6455c9f1003ae3c6  speech/p226_008_mic1.flac
4b3124fbf362cff6f20e557cf18d20f2975f8f76d42a2942612ce67921b408da  speech/p226_016_mic1.flac
53f24f9e9e13b04a3285094f145aac5b0f52dabf4caf85bda4c76d61e9c237b3  noise/street_traffic_rain_cc0_10s.wav
4a02931545d941cccf7526ebd3c892234dee31915882dbb1e2f786ec472da1a1  noise/forest_rain_cc0_10s.wav
3763bff659ff596d5fa70c673b5aa7901abf39696f1a24810dc551a91abb7c8e  noise/ac_fan_cc0_10s.wav
0b104a616d3f4f7c5d645f998e46aa4f14f9b0b5525f942a2aef4d716f8709ef  tagarela/test-00000-of-00001.parquet
```

- [ ] **Step 3: Validar o script estaticamente**

Run: `command -v shellcheck >/dev/null && shellcheck scripts/fetch-voice-samples.sh || bash -n scripts/fetch-voice-samples.sh`
Expected: sem saída (código 0).

Run: `scripts/fetch-voice-samples.sh -h`
Expected: imprime o bloco de uso (de "Downloads the voice/noise test samples..." até "Needs: ...") e sai com 0.

- [ ] **Step 4: Ignorar a pasta de áudio**

Acrescente ao final de `.gitignore`:

```gitignore
# Amostras de voz e ruído baixadas por scripts/fetch-voice-samples.sh (licenças próprias; nunca commitar)
fixtures/voice-samples/
```

Run: `git check-ignore -v fixtures/voice-samples/speech/p225_011_mic1.flac fixtures/voice-samples/.venv/bin/python fixtures/voice-samples/tagarela/test-00000-of-00001.parquet`
Expected: as três linhas apontam para a regra `fixtures/voice-samples/` do `.gitignore`.

- [ ] **Step 5: Reconferir os hashes dos dados da sessão de pesquisa**

Os arquivos abaixo foram baixados e verificados antes deste plano; **não confie, reconfira**. Se o diretório não existir na sua máquina, pule para o Step 7.

```bash
SAMPLES_FINAL=<SCRATCHPAD>/voice-samples/final
(cd "$SAMPLES_FINAL" && sha256sum ./*) | sed 's# \./# #' | sort -k2
```

Expected: 9 linhas; os hashes têm de coincidir com os de `scripts/voice-samples.sha256` (os nomes de `noise/` e `speech/` perdem o prefixo da pasta). O Step 6 repete essa conferência de forma automática, e é ele que decide: se algum hash divergir, **pare e reporte**; não edite o hash para fazer o teste passar.

- [ ] **Step 6: Testar o modo `--from-dir` num diretório temporário**

```bash
export FIXTURES_DIR="$(mktemp -d "${TMPDIR:-/tmp}/clearcore-fx.XXXXXX")"
scripts/fetch-voice-samples.sh --from-dir "$SAMPLES_FINAL"
```

Expected (nove linhas `ok` e `done`; sem criar venv e sem rede):

```text
ok   speech/p225_003_mic1.flac
ok   speech/p225_008_mic1.flac
ok   speech/p225_011_mic1.flac
ok   speech/p225_022_mic1.flac
ok   speech/p226_008_mic1.flac
ok   speech/p226_016_mic1.flac
ok   noise/street_traffic_rain_cc0_10s.wav
ok   noise/forest_rain_cc0_10s.wav
ok   noise/ac_fan_cc0_10s.wav
done
```

Run: `scripts/fetch-voice-samples.sh --verify-only`
Expected: as mesmas linhas `ok` e `done`.

- [ ] **Step 7: Testar o download real (HTTP Range) num diretório temporário**

```bash
export FIXTURES_DIR="$(mktemp -d "${TMPDIR:-/tmp}/clearcore-fx-net.XXXXXX")"
time scripts/fetch-voice-samples.sh --with-tagarela
```

Expected (levou 34 s e cerca de 90 MB, dos quais 75 MB são o shard TAGARELA; sem `--with-tagarela` baixa só cerca de 15 MB): a mensagem `creating venv ... (remotezip 0.12.6)`, `fetching 6 VCTK files by HTTP Range (the full zip is 11.7 GB; it is NOT downloaded)`, seis linhas `Extracting wav48_silence_trimmed/p22x/...`, nove linhas `ok` (speech e noise), a nota de licença do TAGARELA, `ok   tagarela/test-00000-of-00001.parquet` e `done`. Rodar de novo deve imprimir `ok ... (already present)` e não baixar nada.

- [ ] **Step 8: Testar os caminhos de falha**

1. Arquivo original corrompido deve abortar:

```bash
SRC="$(mktemp -d "${TMPDIR:-/tmp}/clearcore-bad.XXXXXX")"
cp "$SAMPLES_FINAL"/* "$SRC"/
echo x >> "$SRC/p225_003_mic1.flac"
FIXTURES_DIR="$(mktemp -d "${TMPDIR:-/tmp}/clearcore-fx-bad.XXXXXX")" scripts/fetch-voice-samples.sh --from-dir "$SRC"; echo "rc=$?"
```

Expected: `ERROR: SHA-256 mismatch for speech/p225_003_mic1.flac (file removed)` e `rc=1`.

2. Hash derivado divergente só avisa: copie `scripts/voice-samples.sha256` para um arquivo temporário trocando o hash da linha `noise/street_traffic_rain_cc0_10s.wav` por 64 zeros e rode `HASH_FILE=<copia> FIXTURES_DIR=$FIXTURES_DIR scripts/fetch-voice-samples.sh --from-dir "$SAMPLES_FINAL"`.
Expected: `WARNING: derived clip noise/street_traffic_rain_cc0_10s.wav differs from the recorded SHA-256 (depends on the ffmpeg version); keeping it`, `done with 1 warning(s)` e código 0.

- [ ] **Step 9: Criar o tutorial**

Crie `docs/testing/voice-samples.md` (a seção "Outros conjuntos em pt-BR" é preenchida na Tarefa 4):

````markdown
# Amostras de voz e ruído para testes

Áudio **nunca** vai para o git. O script `scripts/fetch-voice-samples.sh` baixa um conjunto pequeno
(cerca de 15 MB) para `fixtures/voice-samples/` (pasta no `.gitignore`), confere o SHA-256 contra
`scripts/voice-samples.sha256` e grava `fixtures/voice-samples/ATTRIBUTION.txt`. Testes que usam áudio
**pulam** quando os arquivos não existem, então o CI offline continua verde.

## Como usar

```bash
scripts/fetch-voice-samples.sh                  # conjunto padrão (VCTK + 3 ruídos CC0)
scripts/fetch-voice-samples.sh --with-tagarela  # inclui o shard de teste do TAGARELA (opcional)
scripts/fetch-voice-samples.sh --from-dir DIR   # usa arquivos já baixados, sem rede
scripts/fetch-voice-samples.sh --verify-only    # só confere os hashes dos arquivos presentes
```

Requisitos: `curl`, `sha256sum` (ou `shasum`), `ffmpeg` e `ffprobe`, e `python3` com o módulo `venv`
(no Debian/Ubuntu, o pacote `python3-venv`). O script cria `fixtures/voice-samples/.venv` e instala
`remotezip==0.12.6` ali; nada é instalado no sistema. O zip completo do VCTK tem 11,7 GB e **não** é
baixado: o `remotezip` pede por HTTP Range só os seis arquivos necessários.

Para mudar o destino: `FIXTURES_DIR=/outro/caminho scripts/fetch-voice-samples.sh`.

## O que é baixado

| Arquivo | Origem | Licença |
|---|---|---|
| `speech/p225_{003,008,011,022}_mic1.flac`, `speech/p226_{008,016}_mic1.flac` | VCTK Corpus 0.92 (`wav48_silence_trimmed`), 48 kHz | CC BY 4.0 |
| `noise/street_traffic_rain_cc0_10s.wav` | Wikimedia Commons, "Urban Street on a Rainy Afternoon" (5 s a 15 s) | CC0 |
| `noise/forest_rain_cc0_10s.wav` | Wikimedia Commons, "Light Rain Distant Thunder July 5th 2016" (0 s a 10 s) | CC0 |
| `noise/ac_fan_cc0_10s.wav` | Wikimedia Commons, "Resident air-conditioned out door unit" (5 s a 15 s) | CC0 |
| `tagarela/test-00000-of-00001.parquet` (só com `--with-tagarela`) | Hugging Face `freds0/TAGARELA`, shard de teste, 78 277 756 bytes | CC BY-NC-SA 4.0 |

Os clipes de ruído são recortes de 10 s, mono, PCM s16le, 48 kHz, feitos pelo `ffmpeg`. A `p225_011`
(6,8 s) serve de fala de enrollment. As falas do VCTK são em inglês: não há fala em pt-BR no
conjunto padrão (ver o TAGARELA abaixo e a seção "Outros conjuntos em pt-BR").

## Atribuição (obrigatória)

VCTK: *CSTR VCTK Corpus v0.92*, University of Edinburgh, The Centre for Speech Technology Research
(CSTR); Yamagishi et al. Licença CC BY 4.0: https://datashare.ed.ac.uk/handle/10283/3443

Os ruídos são CC0 (sem exigência de atribuição; as fontes estão em `ATTRIBUTION.txt`).

## TAGARELA: só dado de teste local

O TAGARELA é **CC BY-NC-SA 4.0**, derivado dos "Cem Mil Podcasts" (direitos dos podcasters não
esclarecidos), a 16 kHz, já passado por vocoder-denoiser e sem speaker id garantido. Regras:

- só com a flag explícita `--with-tagarela`, nunca no conjunto padrão;
- **nunca commitar** o arquivo nem qualquer áudio derivado dele;
- o script não extrai áudio: o `.parquet` fica como baixado;
- **fora do treino de pesos que serão distribuídos**, até o dono do projeto registrar decisão
  explícita (o NC-SA pode exigir que os pesos herdem CC BY-NC-SA, em conflito com `MIT OR Apache-2.0`).

## Derivados e versão do ffmpeg

Os hashes dos `noise/*.wav` valem para `ffmpeg 8.1.3`. Com outra versão o script emite `WARNING` e
mantém o arquivo; os originais (`speech/*`, `tagarela/*`) divergentes abortam.

## Convenção para testes que usam áudio

Procure o arquivo e **pule** o teste se ele faltar (mensagem em `stderr`, `return` sem falhar):

```rust
fn voice_sample(relative: &str) -> Option<std::path::PathBuf> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/voice-samples")
        .join(relative);
    if path.is_file() {
        Some(path)
    } else {
        eprintln!("skip: {relative} ausente; rode scripts/fetch-voice-samples.sh");
        None
    }
}
```
````

- [ ] **Step 10: Conferir o tutorial e fechar**

Run: `grep -c -E "CC BY 4.0|CC0|CC BY-NC-SA 4.0|remotezip|--with-tagarela" docs/testing/voice-samples.md`
Expected: número maior ou igual a 5.

Os diretórios temporários dos Steps 6 a 8 ficam em `$TMPDIR`, fora do repositório; podem ficar lá (não apague com `rm -rf` nem globs).

- [ ] **Step 11: Checkpoint**

Run: `git status --short`
Expected: aparecem ` M .gitignore`, `?? scripts/fetch-voice-samples.sh`, `?? scripts/voice-samples.sha256`, `?? docs/testing/` e `?? benchmarks/local/`/`?? scripts/run-cpu-baseline-local.sh` da Tarefa 1; **nenhum** arquivo sob `fixtures/voice-samples/`. **Não commitar.**

---

### Task 4: Pesquisa de licença e taxa de amostragem (Common Voice pt e MLS Portuguese) e gate da fase

Tarefa de pesquisa: **não baixa nenhum conjunto** (a sonda de 4 MB abaixo já foi feita no planejamento). Os fatos desta tarefa foram levantados em 2026-10-02; o executor reconfere o que puder, mas **não promove "relato" a "confirmado"**: cada afirmação traz a fonte, e o que não foi lido na fonte oficial fica marcado como não confirmado.

**Files:**
- Modify: `docs/testing/voice-samples.md` (acrescentar a seção "Outros conjuntos em pt-BR" ao final)

**Interfaces:**
- Consumes: `docs/testing/voice-samples.md` criado na Tarefa 3.
- Produces: seção `## Outros conjuntos em pt-BR` com licença, taxa de amostragem, links e condições de acesso; as fases de treino e de avaliação em pt-BR a citam.

- [ ] **Step 1: Reconferir o MLS Portuguese (fonte oficial, sem login)**

Run: `curl -sL https://www.openslr.org/94/ | sed 's/<[^>]*>/ /g' | tr -s ' \n' | grep -E "Identifier|License: |mls_portuguese"`
Expected: `Identifier: SLR94`, `License: CC BY 4.0`, `mls_portuguese.tar.gz (9.3G)` e `mls_portuguese_opus.tar.gz (2.5G)`.

Run: `curl -sIL https://dl.fbaipublicfiles.com/mls/mls_portuguese_opus.tar.gz | grep -i -E "^HTTP|content-length|accept-ranges"`
Expected: `HTTP/2 200`, `content-length: 2597205013`, `accept-ranges: bytes` (acesso anônimo, sem login).

Taxa de amostragem, lida na fonte: o artigo do MLS (arXiv 2012.03411, seção 3.1) diz "All the audio data is downsampled from 48kHz to 16kHz". Conferência independente feita no planejamento, sem baixar o conjunto: os primeiros 4 MB de `mls_portuguese.tar.gz` por HTTP Range, extraindo um FLAC, e `ffprobe` devolveu `flac,16000,1` (FLAC, 16 000 Hz, mono). Para repetir:

```bash
PROBE="$(mktemp -d "${TMPDIR:-/tmp}/clearcore-mls.XXXXXX")" && cd "$PROBE"
curl -s -r 0-4000000 https://dl.fbaipublicfiles.com/mls/mls_portuguese.tar.gz | tar -xzf - 2>/dev/null
ffprobe -v error -show_entries stream=codec_name,sample_rate,channels -of csv=p=0 "$(find . -name '*.flac' | head -1)"
```

Expected: `flac,16000,1`. (O `tar` reclama do fim truncado; o `2>/dev/null` é intencional.)

- [ ] **Step 2: Reconferir o Common Voice pt (fonte oficial)**

O Common Voice migrou para o **Mozilla Data Collective** (`https://commonvoice.mozilla.org/en/datasets` leva até lá; o domínio ativo é `https://mozilladatacollective.com`). Estado em 2026-10-02, **sem confirmar na fonte oficial**:

- As páginas de dataset são renderizadas no cliente (a resposta HTTP traz só um "loading shell"), e a API pública devolveu `404 {"error":"Dataset with id cmflnn483xz7xpuiogors5llv not found"}` para o id encontrado por busca (`Common Voice Scripted Speech 23.0 - Portuguese`). Portanto **licença e taxa de amostragem do pt não puderam ser lidas programaticamente**.
- Indício indireto (resultado de busca citando a página oficial, não lido na própria página): licença CC0-1.0, 195 889 clipes, cerca de 230 h (181 h validadas), 3 801 locutores, arquivos MP3. Relato secundário (não oficial): originais a 48 kHz, distribuídos a 16 kHz. **Trate como não confirmado.**
- A lista oficial de datasets do Data Collective mostra o campo de licença por dataset (a maioria dos Common Voice aparece como CC0-1.0) e o formato `mp3`, mas não foi lida a ficha do pt.
- Baixar exige conta (Sign up / Log in) e aceitar os termos do Data Collective: **não foi testado** e não deve ser feito por agente.

Para fechar a lacuna sem baixar nada: abra a ficha do dataset pt no navegador (ex.: `chrome-devtools-mcp`, `new_page` em `https://mozilladatacollective.com/datasets/cmflnn483xz7xpuiogors5llv`, depois `take_snapshot`) e copie os campos "License", "Format" e "Sampling rate". Se a página não renderizar ou pedir login, **mantenha "não confirmado"** na seção abaixo.

- [ ] **Step 3: Acrescentar a seção ao tutorial**

Acrescente ao final de `docs/testing/voice-samples.md` (se o Step 2 conseguiu ler a ficha, troque a linha do Common Voice pelos valores lidos e cite a data):

````markdown

## Outros conjuntos em pt-BR

Conferido em 2026-10-02, **sem baixar nenhum conjunto** (liberados para uso, inclusive treino, pelo
usuário em 2026-10-01; a decisão sobre pesos distribuídos continua com o dono do projeto).

| Conjunto | Licença | Taxa de amostragem | Acesso | Status da conferência |
|---|---|---|---|---|
| **MLS Portuguese** (Multilingual LibriSpeech, OpenSLR SLR94) | **CC BY 4.0** (https://www.openslr.org/94/) | **16 kHz**, mono, FLAC (ou Opus) | Sem login: https://dl.fbaipublicfiles.com/mls/mls_portuguese.tar.gz (9,3 GB) e `mls_portuguese_opus.tar.gz` (2,5 GB) | Confirmado na fonte: licença na página do OpenSLR; 16 kHz no artigo (arXiv 2012.03411, seção 3.1) e em um FLAC real lido por HTTP Range (`ffprobe`: `flac,16000,1`) |
| **Common Voice pt** (Mozilla Data Collective, "Common Voice Scripted Speech 23.0 - Portuguese") | CC0-1.0 segundo indício indireto | MP3; 16 kHz segundo relato secundário (originais a 48 kHz) | Exige conta e aceite de termos em https://mozilladatacollective.com (ficha: `/datasets/cmflnn483xz7xpuiogors5llv`) | **Não confirmado na fonte oficial**: a ficha é renderizada no cliente e a API devolveu 404 |

Atribuição do MLS (CC BY 4.0): Pratap, Xu, Sriram, Synnaeve, Collobert. *MLS: A Large-Scale
Multilingual Dataset for Speech Research*, arXiv:2012.03411. O áudio vem de audiolivros do LibriVox.

Consequência técnica: o pipeline do Clearcore é de 48 kHz fullband. Os dois conjuntos chegam a
16 kHz, então servem para avaliar fala e enrollment em pt-BR, não para validar banda acima de 8 kHz
(de-esser, EQ de brilho). Nenhum dos dois é baixado por `fetch-voice-samples.sh`.
````

- [ ] **Step 4: Gate da fase**

Run: `command -v shellcheck >/dev/null && shellcheck scripts/fetch-voice-samples.sh scripts/run-cpu-baseline-local.sh || (bash -n scripts/fetch-voice-samples.sh && bash -n scripts/run-cpu-baseline-local.sh)`
Expected: sem saída.

Run: `grep -n "Outros conjuntos em pt-BR" docs/testing/voice-samples.md && grep -c -E "CC BY 4.0|CC0|16 kHz" docs/testing/voice-samples.md`
Expected: uma linha com o título e um número maior ou igual a 4.

- [ ] **Step 5: Gate do spec (seção 9) e prova de que nada de Rust mudou**

Run: `git status --short -- crates Cargo.toml Cargo.lock`
Expected: sem saída (nenhum arquivo Rust ou manifesto alterado).

Run: `source <SCRATCHPAD>/rust-env/env.sh && scripts/check-offline.sh` (ou qualquer shell com o cargo 1.90.0 no PATH; `rust-toolchain.toml` fixa a versão)

Run (equivalente, se o env.sh acima não existir): `scripts/check-offline.sh`
Expected: termina com código 0. Se retornar `BLOCKED_OFFLINE_DEPENDENCY` (crates ausentes no cache offline), registre o texto literal e siga: esta fase não altera dependências, então o bloqueio é do ambiente e não da fase. `cargo fmt --check`, `clippy -D warnings` e `cargo test --locked --offline` não se aplicam a esta fase, porque nada em `crates/` mudou.

- [ ] **Step 6: Revisão de código e Checkpoint**

Peça a revisão (agente Sonnet ou Haiku, spec seção 9) dos entregáveis: `scripts/run-cpu-baseline-local.sh`, `scripts/fetch-voice-samples.sh`, `scripts/voice-samples.sha256`, `docs/testing/voice-samples.md` e `conductor/tech-stack.md`. O revisor confere especialmente: nenhuma escrita fora de `benchmarks/local/` e `fixtures/voice-samples/`; nenhum áudio rastreado.

Run: `git status --short`
Expected: ` M .gitignore`, ` M conductor/tech-stack.md`, `?? benchmarks/local/`, `?? docs/testing/`, `?? scripts/fetch-voice-samples.sh`, `?? scripts/run-cpu-baseline-local.sh`, `?? scripts/voice-samples.sha256` (mais arquivos de outras fases). **Não commitar.**

---

## Auto-revisão

1. **Cobertura do spec (seções 8 e 9, fase 0):** "Medir latência/CPU reais" -> Tarefa 1 (benchmark oficial rodado; resultado real é `BLOCKED_UNSUPPORTED_CPU_PROFILE`, registrado sem inventar número, e a única forma legítima de evidência está descrita); "atualizar `conductor/tech-stack.md`" -> Tarefa 2; "script e tutorial de amostras" com hashes de originais e derivados, `.gitignore`, testes que pulam, VCTK, ruídos CC0, `--with-tagarela`, atribuição e uso de Range -> Tarefa 3; "licenças e taxa de amostragem de Common Voice pt e MLS conferidas na fonte oficial" -> Tarefa 4 (MLS confirmado; Common Voice pt **não confirmado**, lacuna declarada).
2. **Placeholders:** nenhum "TBD"/"TODO"; os dois scripts e a lista de hashes estão por inteiro. O único trecho dependente do executor é o Step 2 da Tarefa 4 (ler uma ficha renderizada no navegador), com o fallback escrito.
3. **Consistência de nomes:** `scripts/run-cpu-baseline-local.sh`, `scripts/fetch-voice-samples.sh`, `scripts/voice-samples.sha256`, variáveis `OUT_DIR`, `FIXTURES_DIR`, `HASH_FILE` e o layout `speech/`, `noise/`, `tagarela/` são os mesmos em todas as tarefas.
