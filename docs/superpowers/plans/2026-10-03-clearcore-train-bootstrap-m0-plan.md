# Clearcore Train: bootstrap do repositório + M0 (piloto) — plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Criar o repositório `clearcore-train` (ambientes, toolchain, `tract-check`, licenças/manifests, resampler v1, `gates-v1.yaml`) e executar o marco M0 da spec, que substitui as estimativas `[est.]` por números medidos nesta máquina e aprova I1 e I2.

**Architecture:** Repositório Python 3.11 (uv, pacote `cctrain` em `src/`, sem instalação: `PYTHONPATH=src`) com o DeepFilterNet v0.5.6 vendorizado em `third_party/`, mais um crate Rust isolado (`tools/tract-check`) que carrega os assets com o mesmo `DfTract` do Clearcore (tract 0.19.16). As medições do M0 são scripts finos sobre módulos testados; cada medição grava JSON em `runs/m0/` e um gerador produz `docs/m0-report.md`.

**Tech Stack:** Python 3.11 + uv, PyTorch (índice cu128, sm_120), `deepfilterlib==0.5.6`, onnx/onnxruntime, soundfile/soxr, pyroomacoustics, remotezip, pytest, ruff; Rust 1.90.0 (rustup em `~/.rustup`), `tract-onnx =0.19.16`, `deep_filter_fork` do Clearcore por dependência git pinada.

**Spec:** `docs/superpowers/specs/2026-10-03-clearcore-train-pdfnet3-enrollment-design.md` (rev. 2) no repositório do Clearcore. Pesquisas de apoio: `docs/superpowers/research/2026-10-03-clearcore-train/01..08` (citadas como R01..R08). O executor lê a spec junto com este plano.

## Global Constraints

Copiadas da spec (valores literais); valem para todas as tarefas.

- Contrato fixo: 48 kHz, mono, hop 480, FFT 960, `nb_erb=32`, `nb_df=96`, `df_order=5`, `df_lookahead=2`, `conv_lookahead=2`, latência 1.440 amostras, `[train] model = deepfilternet3` (spec §2.1).
- FiLM é detectado **se e somente se** o grafo tem exatamente 4 entradas; `gamma`/`beta` nas posições 2 e 3; `film_hidden = emb_hidden_dim = 256` (§2.1).
- Semântica: `x' = x * gamma + beta` na saída do `ReLU` de `emb_gru.linear_in` (enc) e de `df_gru.linear_in` (df_dec), sites `/emb_gru/linear_in/1/Relu_output_0` e `/df_gru/linear_in/linear_in.1/Relu_output_0`; FiLM é constante no tempo (§2.1).
- `config.ini`: o do DFNet3 v0.5.6 byte a byte (SHA-256 `415eb925d44990d938fb739f514aa3662c1ec0ea836cff044fa1291b82cb4290`); o teste é igualdade de hash (§2.1).
- Asset aprovado do DFNet3: SHA-256 `c94d91f70911001c946e0fabb4aa9adc37045f45a03b56008cb0c8244cb63616` (§2.4); golden de 1.000 quadros do upstream: SHA-256 `c041ee1fa4b2ce4bdad4f07e28b8fdcb901c56bab20380d17abd254d5c9736aa` (§6.3).
- Enrollment: entrada `audio`, `f32 [1, N]`, 16 kHz mono, **N ≥ 32.000 (2 s)** até 192.000 (12 s); saídas `gamma_enc`, `beta_enc`, `gamma_df`, `beta_df` f32 `[256]` e `embedding` f32 `[192]`; Opset 13, `ir_version` 8 (§2.2).
- Limites, rejeitados e nunca "clampados": `gamma ∈ [0,001; 100]`, `beta ∈ [−50; 50]`, sem NaN/Inf (§2.2).
- `resampler-48k-16k-v1`: FIR de fase linear, 241 taps, janela Kaiser β=8,6, corte 7.600 Hz, decimação por 3, saída `y[n] = Σ_k h[k]·x[3n + 120 − k]` (atraso compensado, zeros nas bordas); publica os coeficientes (f32), seu SHA-256 e vetores dourados (§2.2).
- Pacote: tar com membros planos, ordem fixa, modo 0644, `mtime=0`, `uid=gid=0`, `uname=gname=""`; gzip `mtime=0`. Nome `<asset_id>.tar.gz`, com SHA-256 (hex minúsculo) e tamanho (§2.3).
- Gerador FiLM: `gamma_* = exp(Γ · tanh(dγ*))`, `Γ = ln 10`; `beta_* = B · tanh(dβ*)`; última camada com pesos e bias iniciados em ZERO; `B = min(50, max(4, 2·p99))`, com `p99` = percentil 99 das ativações nos dois sites medido no DFNet3 upstream sobre os dados do M0; `Γ ≤ ln 100`, o validador da config recusa fora disso (§3.2).
- Gating do runtime: padrão −10/30/20 dB e `atten_lim_db` desligado: `lsnr < −10` → máscara zero; `lsnr > 30` → nenhum processamento; `lsnr > 20` → só a máscara ERB; senão, ERB + DF (§3.5).
- Réplica Python (`eval/runtime_path.py`: `apply_stages` + aplicação de máscara/DF) com paridade ≤ 1e-4 contra o `tract-check` em 1.000 quadros; cada cenário reporta a fração de quadros em cada estágio (§3.5).
- Allowlist: o loader recusa qualquer arquivo cujo manifest não tenha licença em `{CC0-1.0, CC-BY-4.0, Apache-2.0, domínio público declarado}`. NC, ND, SA, ODbL/DbCL, GPL, "research only" e licença não confirmada ficam fora do treino. Fontes "só teste" têm marcação própria e o loader de treino as recusa (§4.1).
- Brutos são transitórios (extração em stream onde o formato permitir; um bruto por vez), em disco persistente, nunca em `/tmp` (§4.2). `CARGO_TARGET_DIR` no disco do repo, nunca em `/tmp` (§5.2).
- Common Voice: nenhum áudio nem derivado em forma de áudio sai da máquina (inclusive goldens); nenhum metadado é cruzado com fonte externa, nenhum rótulo é publicado (§4.6).
- Ambiente `train`: Python 3.11 (uv), torch com CUDA ≥ 12.8 (sm_120), `numpy<2`, `deepfilterlib==0.5.6`, DeepFilterNet v0.5.6 vendorizado com patch em `df/io.py` (§5.2).
- Ambiente `tract-check`: Rust 1.90.0 em `~/.rustup` (persistente), `tract-onnx =0.19.16` e o `deep_filter` vendorizado do Clearcore por dependência git pinada num commit (§5.2).
- Pesos passam entre ambientes como `safetensors` de `state_dict`, com `.contiguous()` em cada tensor (§5.2).
- Identidade: I1 `max_abs_diff == 0` em CPU determinística; I2 `max_abs_diff == 0` contra o golden; P1 `atol 1e-5, rtol 1e-6`; P2 `|d| ≤ 1e-4 + 1e-4·|ref|`. Tolerância diferente de 0 em I1-I3 só com causa demonstrada e registrada; nunca se alarga epsilon (§6.3).
- Pré-registro: `configs/eval/gates-v1.yaml` congela **todos** os níveis de exigência (G0-G11, gates do M1, regra da sonda) antes do T1; mudar um nível exige `gates-v2.yaml` com justificativa escrita (§7.1).
- Reprodutibilidade: seed global na config, seeds de worker derivadas; manifests com SHA-256 por arquivo; `uv.lock` e `Cargo.lock` versionados; código sob `MIT OR Apache-2.0` (§8).
- Decisões do dono (2026-10-03): fontes de dados aprovadas; CV `client_id` pode agrupar por falante com o filtro da §4.6; estágio F é o plano (sonda C primeiro); **o dono libera ≥ 20 GB de disco por conta própria: nenhum código ou agente apaga nada fora dos próprios temporários**; governança/assinatura só no M5.
- Nenhum caminho pessoal no código ou nas configs: tudo deriva de `$HOME`, de variáveis `CCTRAIN_*` ou da raiz do repositório. Pouco disco é erro (falha fechada), nunca aviso.

---

## Convenções de execução

**Repositório.** `$HOME/orca/projects/clearcore-train` (git, branch `main`, sem remoto). A Task 1 o cria; nenhuma outra tarefa o recria.

**Checkout do Clearcore.** Várias tarefas leem arquivos do Clearcore (asset aprovado, golden, fixtures FiLM, grafos stateful) e o `tract-check` compila o crate `deep_filter_fork` dele. O orquestrador exporta `CCTRAIN_CLEARCORE_ROOT` apontando para um checkout que contenha o commit `70dc85cf2c767cf2dbde9b569f7b72cb00d8d4aa` (hoje: o worktree `hippocamp` do Clearcore; o `master` local **não** tem `vendor/crates/deep_filter`, `fixtures/film` nem `models/stateful`). Toda leitura confere o SHA-256 do arquivo.

**Ambiente comum.** A Task 1 cria `scripts/env.sh`. Toda etapa `bash` das Tasks 2-18 começa com:

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
```

`env.sh` define `CCTRAIN_ROOT` (checkout principal), `CCTRAIN_WT` (worktree da tarefa; igual a `CCTRAIN_ROOT` quando sequencial), `UV_PROJECT_ENVIRONMENT` (o `.venv` único em `$CCTRAIN_ROOT`), `CCTRAIN_PY` (o Python desse `.venv`), `PYTHONPATH=$CCTRAIN_WT/src`, `CCTRAIN_DATA_DIR`/`CCTRAIN_RUNS_DIR` (sempre `data/` e `runs/` do checkout principal, compartilhados entre worktrees), `CCTRAIN_TRACT_CHECK_BIN`, `RUSTUP_HOME`, `CARGO_HOME`.

**Paralelismo.** Tarefas paralelas tocam arquivos disjuntos (a Task 1 cria todos os `__init__.py`, o `pyproject.toml`, o `uv.lock` e o `.gitignore`; nenhuma outra tarefa os altera). Para rodar em paralelo, o orquestrador cria um worktree por tarefa (`git -C "$CCTRAIN_ROOT" worktree add "$CCTRAIN_ROOT/../clearcore-train-wt/tNN" -b m0/tNN`), exporta `CCTRAIN_WT` para o subagente e, ao fim, faz merge em `main` (sem conflitos esperados). O `.venv` e o `target/` do Rust ficam só em `$CCTRAIN_ROOT` e são compartilhados (o `pyproject` não muda depois da Task 1).

**Marcadores de teste.** `gpu` (CUDA), `tract` (binário `tract-check` compilado), `clearcore` (`CCTRAIN_CLEARCORE_ROOT`), `network` (rede), `data` (dados do piloto baixados). O `addopts` exclui `network` por padrão. Pré-requisito ausente é **falha**, não `skip`. A verificação mínima de cada tarefa é: os testes da própria tarefa, `ruff check` e `pytest -m "not network and not gpu and not tract and not data"`.

## Grafo de dependências e ondas

| Task | Entrega | Depende de |
|---|---|---|
| 1 | Bootstrap: repo, `pyproject`/`uv.lock` com **todas** as dependências do M0, DeepFilterNet vendorizado, `paths`/`disk`/`hashing`, `env.sh` | — |
| 2 | Rust 1.90.0 em `~/.rustup` + crate `tract-check` + subcomando `identity` (I2 contra o fixture do Clearcore) | 1 |
| 3 | `data/licenses.py` + `data/manifest.py` + manifests de fonte do piloto | 1 |
| 4 | `resampler-48k-16k-v1` (coeficientes, SHA-256, vetores dourados) | 1 |
| 5 | `configs/eval/gates-v1.yaml` + `configs/m0/budget-v1.yaml` congelados + carregador | 1 |
| 6 | `export/film.py` (`add_film`) + `export/pack.py` sobre o asset aprovado | 1 |
| 7 | `models/film.py` (gerador FiLM, `FilmConfig`, regra de `B`) | 1 |
| 8 | Patch FiLM no DFNet3 vendorizado + `models/pdfnet3.py` + I1 + hash do `state_dict` | 1, 7 |
| 9 | `models/fbank.py` + `models/ecapa.py` + `export/enrollment.py` (PyTorch/ORT) | 1, 7 |
| 10 | `data/sources.py` (download em stream, hash, HTTP Range, tar em stream) | 3 |
| 11 | `data/rir.py` (RIR sintéticas 48 kHz) + manifest | 3 |
| 12 | Busca do piloto (VCTK por Range, LibriSpeech dev-clean, ruídos CC0, amostras de teste, CV 26.0 condicional) + `data/band.py` + manifests por arquivo | 10 |
| 13 | `data/audio.py`, `data/features.py` (paridade libdf), `data/mix.py`, `data/loader.py` | 3 |
| 14 | `tract-check`: subcomandos `gating` e `enroll` | 2 |
| 15 | `eval/runtime_path.py` (réplica do gating) + paridade com o `tract-check` | 8, 12, 14 |
| 16 | Protótipo do fbank + ECAPA (pooling) no tract, N de 2 a 12 s, −40 dBFS a 0,99 | 9, 14 |
| 17 | Medições do M0 (disco, loader, passo GPU, `B`, desvio offline × runtime) | 8, 11, 12, 13, 15 |
| 18 | I2 sobre o asset do próprio repo + relatório M0 + go/no-go | 2, 4, 5, 6, 15, 16, 17 |

Ondas para despacho paralelo:

- **Onda 0:** 1.
- **Onda 1 (6 em paralelo):** 2, 3, 4, 5, 6, 7.
- **Onda 2 (6 em paralelo):** 8 e 9 (após 7); 10, 11 e 13 (após 3); 14 (após 2).
- **Onda 3 (2 em paralelo):** 12 (após 10); 16 (após 9 e 14).
- **Onda 4:** 15 (após 8, 12, 14).
- **Onda 5:** 17.
- **Onda 6:** 18.

Tarefas com rede: 1 (wheels, clone do DeepFilterNet), 2 (rustup, crates.io), 12 (dados). Tarefas com GPU: 1 (checagem), 17. As demais rodam offline depois da Task 1.

## Decisões de planejamento sobre pontos ambíguos da spec

Registradas aqui para o executor não as reabrir; o relatório M0 as repete.

1. **Estado dos decoders sob gating.** O runtime não executa `erb_dec` nos quadros "zero"/"sem processamento" nem `df_dec` nos quadros sem DF, e pula STFT e encoder nos quadros com média quadrática `< 1e-7` (`tract.rs:663-667`); o estado recorrente dessas redes congela nesses quadros. Uma réplica "máscara/DF sobre um forward de sequência inteira" não reproduz isso. Decisão: a réplica (`eval/runtime_path.py`) espelha `DfTract::process` quadro a quadro e consome um `FrameModel` com estado; no M0 o `FrameModel` é o ONNX stateful do Clearcore (`models/stateful/`, pesos upstream, sem FiLM). A emulação usada no treino (máscara por estágio sobre o forward PyTorch) é medida contra a réplica na Task 17 e vira dado para o M2.
2. **Transitório inicial.** Se a paridade da réplica falhar só nos primeiros quadros (aquecimento do `lookahead`), o executor **não** alarga a tolerância nem exclui quadros: para e reporta a curva de erro por quadro.
3. **Dependência git do `tract-check`.** O crate vendorizado não está em nenhum branch publicado de `github.com/joaorura/clearcore` (o `origin/master` não o tem; o branch local está 11 commits à frente). O `Cargo.toml` pina `rev = "70dc85cf2c767cf2dbde9b569f7b72cb00d8d4aa"` com a URL pública, e o `scripts/tract-check.sh` redireciona essa URL para `CCTRAIN_CLEARCORE_ROOT` por `git -c url.<local>.insteadOf` (via `GIT_CONFIG_*`), com `CARGO_NET_GIT_FETCH_WITH_CLI=true`. O `Cargo.lock` registra a URL pública; quando o dono publicar o commit, nada muda.
4. **"~1 h de dados" × LibriSpeech dev-clean (5,4 h).** O tar.gz (337 MB) é lido inteiro em stream de qualquer forma; o dev-clean fica inteiro em disco (~350 MB FLAC) e o loader do piloto usa um subconjunto de ~1 h (VCTK ~20 min + 20 falantes LibriSpeech).
5. **Common Voice 26.0 pt-BR.** O download exige o aceite dos termos do Mozilla Data Collective pela conta do dono (pendente). A Task 12 só ingere o arquivo se `CCTRAIN_CV26_ARCHIVE` apontar para ele; sem isso, o relatório marca "não executado". O manifest por arquivo do CV (que traz `client_id`) fica em `data/manifests/` (ignorado pelo git); só o SHA-256 dele é versionado, para cumprir "nenhum rótulo é publicado" mesmo se o repo ganhar remoto.
6. **Licença das RIR geradas.** A allowlist da §4.1 não tem um identificador para dados gerados pelo próprio repo. O plano usa `LicenseRef-cctrain-generated`, aceito pelo loader de treino; o dono confirma no M5.
7. **Resampler.** A spec não fixa comprimento de saída, normalização nem precisão de acumulação. Decisão: `h` projetado em float64 (seno cardinal × Kaiser, soma normalizada para 1, idêntico a `scipy.signal.firwin(241, 7600, window=("kaiser", 8.6), fs=48000)`), publicado em f32; saída com `ceil(L/3)` amostras; acumulação em float64 sobre os coeficientes f32, saída arredondada para f32. Registro: a transição vai de ~7,05 kHz a ~8,2 kHz, logo em 8 kHz a atenuação é só ~39 dB (aliasing residual em 7,8-8,0 kHz na saída).
8. **Regra de `B`.** "p99 das ativações nos dois sites" é lido como percentil 99 das ativações dos dois sites **juntas** (um único `B`, como no gerador); os p99 por site também são registrados.
9. **`max_freq`.** "metade da banda efetiva" é ambíguo; o M0 só registra a banda efetiva por clipe (rolloff de 99%, em Hz) no manifest. A definição de `max_freq` fica para o plano do M2.
10. **BatchNorm fundida no `enrollment.onnx`.** No ECAPA a ordem é Conv → ReLU → BN, que não funde no conv anterior. O protótipo do M0 exporta `BatchNormalization`; a estratégia de fusão fica para o M3.
11. **Parâmetros do fbank** (n_fft 512, mel HTK de 20 a 7.600 Hz, 80 bandas) não estão na spec; o M0 os escolhe e o M1 os congela.
12. **G2.** A spec lista "WER ≤ 0,85×DFNet3"; R06 §4 também exige "≤ WER do sinal ruidoso". Como a spec adota R06 "com estes ajustes", `gates-v1.yaml` mantém os dois. G3b e G5b de R06 entram como classe M.
13. **Gzip do pacote.** A spec fixa `mtime=0` mas não o campo FNAME do cabeçalho; o plano grava nome vazio para o SHA-256 não depender do caminho. Como os membros são planos (o fixture do Clearcore usa o prefixo `tmp/export/`), o SHA-256 do tar do repo difere do fixture por construção; os `.onnx` são comparados um a um.
14. **Níveis do go/no-go do M0** não estão na spec; o plano os pré-registra em `configs/m0/budget-v1.yaml` (Task 5) antes de qualquer medição.
15. **RAM.** Hoje há ~2 GB livres (medido no planejamento); as medições da Task 17 registram a RAM livre no momento e o relatório avisa se houve swap. Fechar aplicações é decisão do dono (decisão 7).

---

## File Structure

```
clearcore-train/
  pyproject.toml, uv.lock, .python-version, .gitignore
  LICENSE-MIT, LICENSE-APACHE, NOTICE
  scripts/env.sh                       ambiente comum das tarefas
  scripts/tract-check.sh               build/test/run do tract-check com toolchain persistente
  scripts/fetch_pilot.py               busca dos dados do piloto (Task 12)
  scripts/gen_rir_pilot.py             RIR do piloto (Task 11)
  scripts/gen_resampler_v1.py          coeficientes e vetores dourados (Task 4)
  scripts/m0_measure.py                medições do M0 (Task 17)
  scripts/m0_report.py                 relatório M0 (Task 18)
  third_party/DeepFilterNet/           v0.5.6: DeepFilterNet/df, models/DeepFilterNet3.zip, LICENSE-*, VENDORED.json, PATCHES.md
  configs/eval/gates-v1.yaml (+ .sha256)
  configs/m0/budget-v1.yaml (+ .sha256)
  configs/pdfnet3/film-v1.yaml
  configs/data/minimal-set-v1.yaml
  manifests/sources/*.yaml             uma fonte por arquivo (URL, licença, evidência, papel)
  manifests/sources.lock.json          SHA-256 dos brutos baixados
  manifests/files/*.jsonl              manifest por arquivo (exceto CV)
  manifests/files/cv26-ptbr.sha256     só o hash do manifest do CV
  resampler/v1/coeffs.f32, coeffs.sha256, manifest.json, golden/*.f32
  src/cctrain/
    __init__.py, paths.py, disk.py, hashing.py, dfvendor.py, tractcheck.py
    data/{__init__,licenses,manifest,sources,band,rir,audio,features,mix,loader,resample}.py
    models/{__init__,film,dfnet3_upstream,pdfnet3,state_hash,fbank,ecapa}.py
    export/{__init__,film,pack,enrollment}.py
    eval/{__init__,gates,runtime_path}.py
    losses/__init__.py, train/__init__.py
    m0/{__init__,disk_probe,loader_probe,step_probe,beta_probe,gpu_monitor,offline_probe,report}.py
  tools/tract-check/{Cargo.toml,Cargo.lock,rust-toolchain.toml,src/{main,common,identity,gating,enroll}.rs}
  tests/ (espelha src/cctrain)
  docs/m0-report.md                    relatório M0 (Task 18)
  data/, runs/, delivery/, build/      ignorados pelo git
```

---
### Task 1: Bootstrap do repositório e do ambiente `train`

**Files:**
- Create: `pyproject.toml`, `uv.lock`, `.python-version`, `.gitignore`, `LICENSE-MIT`, `LICENSE-APACHE`, `NOTICE`, `scripts/env.sh`
- Create: `third_party/DeepFilterNet/` (cópia de `DeepFilterNet/` e `models/DeepFilterNet3.zip` da tag v0.5.6, `LICENSE-MIT`, `LICENSE-APACHE`, `VENDORED.json`, `PATCHES.md`)
- Create: `src/cctrain/__init__.py`, `src/cctrain/{data,models,export,eval,losses,train,m0}/__init__.py`, `src/cctrain/paths.py`, `src/cctrain/disk.py`, `src/cctrain/hashing.py`, `src/cctrain/dfvendor.py`
- Test: `tests/conftest.py`, `tests/test_paths.py`, `tests/test_disk.py`, `tests/test_hashing.py`, `tests/test_dfvendor.py`, `tests/test_env.py`

**Interfaces:**
- Consumes: nada.
- Produces:
  - `cctrain.paths`: `ConfigError(RuntimeError)`; `repo_root() -> Path`; `Paths.from_env(env: Mapping[str,str] | None = None) -> Paths` com campos `repo, data, runs, delivery, build: Path`; `clearcore_root(env=None) -> Path`; constantes `ENV_DATA_DIR="CCTRAIN_DATA_DIR"`, `ENV_RUNS_DIR="CCTRAIN_RUNS_DIR"`, `ENV_CLEARCORE_ROOT="CCTRAIN_CLEARCORE_ROOT"`.
  - `cctrain.disk`: `DiskError(RuntimeError)`; `filesystem_type(path, mounts_file=Path("/proc/self/mounts")) -> str`; `assert_persistent(path, mounts_file=...) -> None`; `reserve_bytes(env=None) -> int` (env `CCTRAIN_MIN_FREE_GB`, padrão 10); `require_free(path, need_bytes: int, *, reserve: int | None = None, mounts_file=..., usage=shutil.disk_usage) -> int` (devolve bytes livres); `GIB`.
  - `cctrain.hashing`: `sha256_bytes(data: bytes) -> str`; `sha256_file(path) -> str`; `file_size(path) -> int`.
  - `cctrain.dfvendor`: `VENDORED_ROOT: Path`; `ensure_df_importable() -> ModuleType` (importa `df`); `vendored_info() -> dict`; `ensure_dfnet3_checkpoint(paths: Paths | None = None) -> Path` (diretório com `config.ini` e `checkpoints/`).
  - Ambiente: `$CCTRAIN_ROOT/.venv` com Python 3.11, torch cu128, `deepfilterlib==0.5.6`, numpy < 2 e as demais dependências do M0. **Nenhuma tarefa posterior altera `pyproject.toml` ou `uv.lock`.**

- [ ] **Step 1: Conferir espaço e criar o repositório**

```bash
export CCTRAIN_ROOT="$HOME/orca/projects/clearcore-train"
test ! -e "$CCTRAIN_ROOT" || { echo "ERRO: $CCTRAIN_ROOT já existe; não sobrescrever"; exit 1; }
avail_kb=$(df --output=avail -k "$(dirname "$CCTRAIN_ROOT")" | tail -1)
fstype=$(findmnt -no FSTYPE --target "$(dirname "$CCTRAIN_ROOT")")
echo "livre: $((avail_kb / 1024 / 1024)) GiB; fs: $fstype"
[ "$fstype" != "tmpfs" ] || { echo "ERRO: destino em tmpfs"; exit 1; }
[ "$avail_kb" -ge $((22 * 1024 * 1024)) ] || { echo "ERRO: menos de 22 GiB livres (ambiente ~8 GiB + reserva 10 GiB + margem); liberar disco é decisão do dono"; exit 1; }
mkdir -p "$CCTRAIN_ROOT" && cd "$CCTRAIN_ROOT" && git init -b main
mkdir -p src/cctrain/{data,models,export,eval,losses,train,m0} tests scripts configs manifests third_party
for d in src/cctrain src/cctrain/{data,models,export,eval,losses,train,m0}; do : > "$d/__init__.py"; done
echo "3.11" > .python-version
```

Expected: `Initialized empty Git repository`, sem erro de espaço.

- [ ] **Step 2: Vendorizar o DeepFilterNet v0.5.6**

O clone vai para `build/` (disco persistente do repo, ignorado pelo git), nunca para `/tmp`.

```bash
cd "$HOME/orca/projects/clearcore-train"
mkdir -p build
git clone --depth 1 --branch v0.5.6 https://github.com/Rikorose/DeepFilterNet.git build/DeepFilterNet-v0.5.6
commit=$(git -C build/DeepFilterNet-v0.5.6 rev-parse HEAD); echo "commit: $commit"
case "$commit" in 978576aa*) echo "ok: commit da tag confere com o do spike (978576aa)";; *) echo "PARE: commit $commit diferente de 978576aa; reporte ao orquestrador"; exit 1;; esac
mkdir -p third_party/DeepFilterNet/models
cp -r build/DeepFilterNet-v0.5.6/DeepFilterNet third_party/DeepFilterNet/DeepFilterNet
cp build/DeepFilterNet-v0.5.6/models/DeepFilterNet3.zip third_party/DeepFilterNet/models/
cp build/DeepFilterNet-v0.5.6/LICENSE-MIT build/DeepFilterNet-v0.5.6/LICENSE-APACHE third_party/DeepFilterNet/
find third_party/DeepFilterNet -name '__pycache__' -prune -exec rm -rf {} +
ls -la third_party/DeepFilterNet/models/DeepFilterNet3.zip
grep -n "torchaudio\|_six" third_party/DeepFilterNet/DeepFilterNet/df/*.py | head -40
sed -n '/\[tool.poetry.dependencies\]/,/^\[/p' third_party/DeepFilterNet/DeepFilterNet/pyproject.toml
```

Expected: `DeepFilterNet3.zip` com 7986207 bytes (R02 §1.2). Anote as linhas de `torchaudio` e as dependências do poetry: as que não estiverem na lista do Step 4 entram nela com a mesma restrição de versão (exceto `torch`/`torchaudio`, já presentes).

Grave `third_party/DeepFilterNet/VENDORED.json`:

```bash
cd "$HOME/orca/projects/clearcore-train"
python3 - <<'EOF'
import hashlib, json, pathlib, subprocess
root = pathlib.Path("third_party/DeepFilterNet")
zip_path = root / "models/DeepFilterNet3.zip"
commit = subprocess.check_output(["git", "-C", "build/DeepFilterNet-v0.5.6", "rev-parse", "HEAD"], text=True).strip()
info = {
    "upstream": "https://github.com/Rikorose/DeepFilterNet",
    "tag": "v0.5.6",
    "commit": commit,
    "license": "MIT OR Apache-2.0",
    "copied": ["DeepFilterNet/", "models/DeepFilterNet3.zip", "LICENSE-MIT", "LICENSE-APACHE"],
    "files": {"models/DeepFilterNet3.zip": {
        "sha256": hashlib.sha256(zip_path.read_bytes()).hexdigest(),
        "size": zip_path.stat().st_size}},
    "patches": "PATCHES.md",
}
(root / "VENDORED.json").write_text(json.dumps(info, indent=2) + "\n")
print(json.dumps(info, indent=2))
EOF
```

- [ ] **Step 3: Patch de `df/io.py` (torchaudio novo)**

Em `third_party/DeepFilterNet/DeepFilterNet/df/io.py`, substitua a linha `from torchaudio.backend.common import AudioMetaData` por:

```python
# CCTRAIN-PATCH(io-audiometadata): torchaudio>=2.1 removeu torchaudio.backend.common.AudioMetaData.
try:
    from torchaudio.backend.common import AudioMetaData  # noqa: F401
except ImportError:  # depende da versão do torchaudio
    from dataclasses import dataclass

    @dataclass
    class AudioMetaData:  # mesmos campos do torchaudio 0.13
        sample_rate: int
        num_frames: int
        num_channels: int
        bits_per_sample: int
        encoding: str
```

Crie `third_party/DeepFilterNet/PATCHES.md`:

```markdown
# Patches sobre o DeepFilterNet v0.5.6

Cada trecho alterado é marcado no código com `CCTRAIN-PATCH(<id>)`. Licença do upstream mantida (MIT OR Apache-2.0).

| id | arquivo | motivo |
|---|---|---|
| io-audiometadata | `DeepFilterNet/df/io.py` | `torchaudio.backend.common.AudioMetaData` não existe no torchaudio >= 2.1 (R02 §1.6) |
```

Se o import de `df.enhance`/`df.deepfilternet3` (Step 8) falhar por outro símbolo removido do torch/torchaudio, aplique a menor correção possível, marque com `CCTRAIN-PATCH(<id>)` e acrescente a linha na tabela. Não altere nada além disso nesta tarefa (o patch FiLM é da Task 8).

- [ ] **Step 4: `pyproject.toml`, `.gitignore`, licenças e `env.sh`**

`pyproject.toml` (inclua também as dependências de runtime do DeepFilterNet anotadas no Step 2 que faltarem):

```toml
[project]
name = "cctrain"
version = "0.0.0"
description = "Clearcore Train: pDFNet3 + speaker enrollment (subproject A)"
requires-python = "==3.11.*"
license = "MIT OR Apache-2.0"
dependencies = [
  "numpy>=1.26,<2",
  "scipy>=1.11",
  "torch>=2.7",
  "torchaudio>=2.7",
  "deepfilterlib==0.5.6",
  "onnx>=1.16",
  "onnxruntime>=1.18",
  "soundfile>=0.12.1",
  "soxr>=0.3.7",
  "pyyaml>=6",
  "requests>=2.31",
  "remotezip==0.12.6",
  "pyroomacoustics>=0.7.4",
  "safetensors>=0.4",
  "psutil>=5.9",
  "loguru>=0.7",
  "packaging>=23",
  "appdirs>=1.4",
]

[dependency-groups]
dev = ["pytest>=8", "ruff>=0.6"]

[tool.uv]
package = false

[tool.uv.sources]
torch = { index = "pytorch-cu128" }
torchaudio = { index = "pytorch-cu128" }

[[tool.uv.index]]
name = "pytorch-cu128"
url = "https://download.pytorch.org/whl/cu128"
explicit = true

[tool.pytest.ini_options]
pythonpath = ["src"]
testpaths = ["tests"]
addopts = "-ra --basetemp=build/pytest-tmp -m 'not network'"
markers = [
  "gpu: precisa de CUDA",
  "tract: precisa do binário tract-check compilado",
  "clearcore: precisa de CCTRAIN_CLEARCORE_ROOT",
  "network: acessa a rede",
  "data: precisa dos dados do piloto (Task 12)",
]

[tool.ruff]
line-length = 120
target-version = "py311"
extend-exclude = ["third_party", "build", "data", "runs"]

[tool.ruff.lint]
select = ["E", "F", "I", "B", "UP"]
ignore = ["E501", "B905"]
```

`.gitignore`:

```gitignore
.venv/
__pycache__/
.pytest_cache/
.ruff_cache/
data/
runs/
delivery/
build/
tools/tract-check/target/
```

`LICENSE-APACHE`: copie `third_party/DeepFilterNet/LICENSE-APACHE` (texto Apache-2.0 padrão). `LICENSE-MIT`:

```bash
cd "$HOME/orca/projects/clearcore-train"
cp third_party/DeepFilterNet/LICENSE-APACHE LICENSE-APACHE
holder="$(git config user.name)"
cat > LICENSE-MIT <<EOF
MIT License

Copyright (c) 2026 ${holder}

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
EOF
```

`NOTICE`:

```text
clearcore-train
Licensed under MIT OR Apache-2.0, at your option.

Third-party components:

- DeepFilterNet v0.5.6 (third_party/DeepFilterNet), Copyright (c) Hendrik Schröter,
  MIT OR Apache-2.0. Patches are marked CCTRAIN-PATCH and listed in third_party/DeepFilterNet/PATCHES.md.
- add_film() in src/cctrain/export/film.py is copied from tools/accelerators/gen_film_onnx.py of the
  Clearcore repository (MIT OR Apache-2.0).

Data attributions (CC BY 4.0 sources used for training) are listed per source in manifests/sources/.
```

`scripts/env.sh`:

```bash
# shellcheck shell=bash
# Ambiente comum das tarefas: source "$HOME/orca/projects/clearcore-train/scripts/env.sh"
# Sem caminhos pessoais: tudo deriva de $HOME ou de variáveis exportadas pelo orquestrador.
export CCTRAIN_ROOT="${CCTRAIN_ROOT:-$HOME/orca/projects/clearcore-train}"
export CCTRAIN_WT="${CCTRAIN_WT:-$CCTRAIN_ROOT}"
export UV_PROJECT_ENVIRONMENT="${UV_PROJECT_ENVIRONMENT:-$CCTRAIN_ROOT/.venv}"
export CCTRAIN_PY="$UV_PROJECT_ENVIRONMENT/bin/python"
export CCTRAIN_TRACT_CHECK_BIN="${CCTRAIN_TRACT_CHECK_BIN:-$CCTRAIN_ROOT/tools/tract-check/target/release/tract-check}"
export CCTRAIN_DATA_DIR="${CCTRAIN_DATA_DIR:-$CCTRAIN_ROOT/data}"
export CCTRAIN_RUNS_DIR="${CCTRAIN_RUNS_DIR:-$CCTRAIN_ROOT/runs}"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export PYTHONPATH="$CCTRAIN_WT/src${PYTHONPATH:+:$PYTHONPATH}"
export PYTHONHASHSEED=0
if [ -z "${CCTRAIN_CLEARCORE_ROOT:-}" ]; then
  echo "AVISO: CCTRAIN_CLEARCORE_ROOT não definido (checkout do Clearcore com o commit 70dc85c)" >&2
fi
```

- [ ] **Step 5: Criar o ambiente e o lock**

```bash
cd "$HOME/orca/projects/clearcore-train"
export UV_PROJECT_ENVIRONMENT="$PWD/.venv"
uv python install 3.11
uv lock
uv sync --frozen
.venv/bin/python -c "import torch, numpy, libdf; print(torch.__version__, torch.version.cuda, numpy.__version__, torch.cuda.is_available(), torch.cuda.get_device_capability() if torch.cuda.is_available() else None)"
du -sh .venv
```

Expected: torch com `torch.version.cuda` ≥ 12.8, numpy 1.26.x, `True`, `(12, 0)`. Se o resolvedor recusar alguma dependência por `numpy<2`, fixe a versão anterior compatível dessa dependência (ex.: `onnx<1.18`) em vez de relaxar `numpy<2`.

- [ ] **Step 6: Escrever os testes que falham**

`tests/conftest.py`:

```python
import pytest

from cctrain.paths import Paths


@pytest.fixture(scope="session")
def paths() -> Paths:
    return Paths.from_env()


@pytest.fixture(scope="session")
def m0_runs(paths: Paths):
    out = paths.runs / "m0"
    out.mkdir(parents=True, exist_ok=True)
    return out
```

`tests/test_paths.py`:

```python
from pathlib import Path

import pytest

from cctrain.paths import ConfigError, Paths, clearcore_root, repo_root


def test_repo_root_contains_pyproject():
    assert (repo_root() / "pyproject.toml").is_file()


def test_defaults_live_inside_repo():
    p = Paths.from_env({})
    assert p.data == repo_root() / "data"
    assert p.runs == repo_root() / "runs"
    assert p.delivery == repo_root() / "delivery"


def test_env_overrides_data_and_runs(tmp_path: Path):
    p = Paths.from_env({"CCTRAIN_DATA_DIR": str(tmp_path / "d"), "CCTRAIN_RUNS_DIR": str(tmp_path / "r")})
    assert p.data == (tmp_path / "d").resolve()
    assert p.runs == (tmp_path / "r").resolve()


def test_clearcore_root_is_required():
    with pytest.raises(ConfigError, match="CCTRAIN_CLEARCORE_ROOT"):
        clearcore_root({})


def test_clearcore_root_must_have_approved_asset(tmp_path: Path):
    with pytest.raises(ConfigError, match="df-compatible-release-asset-v1.bin"):
        clearcore_root({"CCTRAIN_CLEARCORE_ROOT": str(tmp_path)})


def test_no_personal_path_in_sources():
    home = str(Path.home())
    for f in (repo_root() / "src").rglob("*.py"):
        assert home not in f.read_text(), f"caminho pessoal em {f}"
```

`tests/test_disk.py`:

```python
from collections import namedtuple
from pathlib import Path

import pytest

from cctrain.disk import GIB, DiskError, assert_persistent, filesystem_type, require_free

Usage = namedtuple("Usage", "total used free")


def _mounts(tmp_path: Path, extra: str = "") -> Path:
    f = tmp_path / "mounts"
    f.write_text(
        "/dev/root / btrfs rw 0 0\n"
        "tmpfs /tmp tmpfs rw 0 0\n"
        "/dev/sdb /media/with\\040space ext4 rw 0 0\n" + extra
    )
    return f


def test_longest_mount_prefix_wins(tmp_path: Path):
    m = _mounts(tmp_path)
    assert filesystem_type(Path("/tmp/x/y"), m) == "tmpfs"
    assert filesystem_type(Path("/home/user"), m) == "btrfs"
    assert filesystem_type(Path("/media/with space/a"), m) == "ext4"


def test_tmpfs_is_refused(tmp_path: Path):
    with pytest.raises(DiskError, match="volátil"):
        assert_persistent(Path("/tmp/data"), _mounts(tmp_path))


def test_require_free_fails_closed(tmp_path: Path):
    m = _mounts(tmp_path)
    usage = lambda _p: Usage(100 * GIB, 95 * GIB, 5 * GIB)  # noqa: E731
    with pytest.raises(DiskError, match="espaço insuficiente"):
        require_free(Path("/home/user/data"), 1 * GIB, reserve=10 * GIB, mounts_file=m, usage=usage)


def test_require_free_passes_with_room(tmp_path: Path):
    m = _mounts(tmp_path)
    usage = lambda _p: Usage(100 * GIB, 50 * GIB, 50 * GIB)  # noqa: E731
    assert require_free(Path("/home/user/data"), 1 * GIB, reserve=10 * GIB, mounts_file=m, usage=usage) == 50 * GIB


def test_real_repo_is_persistent(paths):
    assert_persistent(paths.repo)
```

`tests/test_hashing.py`:

```python
from cctrain.hashing import file_size, sha256_bytes, sha256_file


def test_sha256_known_vector(tmp_path):
    assert sha256_bytes(b"abc") == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    f = tmp_path / "a"
    f.write_bytes(b"abc")
    assert sha256_file(f) == sha256_bytes(b"abc")
    assert file_size(f) == 3
```

`tests/test_dfvendor.py`:

```python
import json

from cctrain.dfvendor import VENDORED_ROOT, ensure_df_importable, ensure_dfnet3_checkpoint, vendored_info
from cctrain.hashing import sha256_file


def test_vendored_metadata_matches_files():
    info = vendored_info()
    assert info["tag"] == "v0.5.6"
    assert info["commit"].startswith("978576aa")
    meta = info["files"]["models/DeepFilterNet3.zip"]
    assert meta["size"] == 7986207
    assert sha256_file(VENDORED_ROOT / "models/DeepFilterNet3.zip") == meta["sha256"]


def test_df_package_imports():
    df = ensure_df_importable()
    import df.deepfilternet3  # noqa: F401
    import df.enhance  # noqa: F401

    assert df.__file__.startswith(str(VENDORED_ROOT))


def test_patches_are_marked():
    io_py = (VENDORED_ROOT / "DeepFilterNet/df/io.py").read_text()
    assert "CCTRAIN-PATCH(io-audiometadata)" in io_py


def test_checkpoint_extraction_is_idempotent(paths):
    d1 = ensure_dfnet3_checkpoint(paths)
    d2 = ensure_dfnet3_checkpoint(paths)
    assert d1 == d2
    assert (d1 / "config.ini").is_file()
    assert any((d1 / "checkpoints").iterdir())
    marker = json.loads((d1 / ".cctrain-extracted.json").read_text())
    assert marker["zip_sha256"] == vendored_info()["files"]["models/DeepFilterNet3.zip"]["sha256"]
```

`tests/test_env.py`:

```python
import numpy as np
import pytest
import torch


def test_numpy_below_2():
    assert int(np.__version__.split(".")[0]) < 2


def test_deepfilterlib_version():
    from importlib.metadata import version

    assert version("deepfilterlib") == "0.5.6"


@pytest.mark.gpu
def test_cuda_blackwell():
    assert torch.cuda.is_available(), "CUDA indisponível"
    assert tuple(int(x) for x in torch.version.cuda.split(".")[:2]) >= (12, 8)
    assert torch.cuda.get_device_capability() == (12, 0)
```

- [ ] **Step 7: Rodar e ver falhar**

```bash
cd "$HOME/orca/projects/clearcore-train" && source scripts/env.sh
"$CCTRAIN_PY" -m pytest tests -v 2>&1 | tail -20
```

Expected: FAIL com `ModuleNotFoundError: No module named 'cctrain.paths'` (e equivalentes).

- [ ] **Step 8: Implementar `paths.py`, `disk.py`, `hashing.py`, `dfvendor.py`**

`src/cctrain/paths.py`:

```python
"""Caminhos do repositório e configuração externa. Nenhum caminho pessoal no código."""

from __future__ import annotations

import os
from collections.abc import Mapping
from dataclasses import dataclass
from pathlib import Path

ENV_DATA_DIR = "CCTRAIN_DATA_DIR"
ENV_RUNS_DIR = "CCTRAIN_RUNS_DIR"
ENV_CLEARCORE_ROOT = "CCTRAIN_CLEARCORE_ROOT"
APPROVED_ASSET_RELPATH = "vendor/approved/df-compatible-release-asset-v1.bin"


class ConfigError(RuntimeError):
    """Configuração ausente ou inválida. Sempre falha fechado."""


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


@dataclass(frozen=True)
class Paths:
    repo: Path
    data: Path
    runs: Path
    delivery: Path
    build: Path

    @classmethod
    def from_env(cls, env: Mapping[str, str] | None = None) -> Paths:
        env = os.environ if env is None else env
        repo = repo_root()
        data = Path(env.get(ENV_DATA_DIR) or repo / "data").expanduser().resolve()
        runs = Path(env.get(ENV_RUNS_DIR) or repo / "runs").expanduser().resolve()
        return cls(repo=repo, data=data, runs=runs, delivery=repo / "delivery", build=repo / "build")


def clearcore_root(env: Mapping[str, str] | None = None) -> Path:
    env = os.environ if env is None else env
    raw = env.get(ENV_CLEARCORE_ROOT)
    if not raw:
        raise ConfigError(
            f"{ENV_CLEARCORE_ROOT} não definido: aponte para um checkout do Clearcore com o commit fixado"
        )
    root = Path(raw).expanduser().resolve()
    if not (root / APPROVED_ASSET_RELPATH).is_file():
        raise ConfigError(f"{root} não contém {APPROVED_ASSET_RELPATH}")
    return root
```

`src/cctrain/hashing.py`:

```python
"""SHA-256 de bytes e arquivos (hex minúsculo)."""

from __future__ import annotations

import hashlib
from pathlib import Path

CHUNK = 1 << 20


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: str | Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(CHUNK), b""):
            h.update(chunk)
    return h.hexdigest()


def file_size(path: str | Path) -> int:
    return Path(path).stat().st_size
```

`src/cctrain/disk.py`:

```python
"""Guarda de disco: só disco persistente e falha fechada quando falta espaço.

Liberar espaço é decisão do dono; este módulo nunca apaga nada.
"""

from __future__ import annotations

import os
import shutil
from collections.abc import Callable, Mapping
from pathlib import Path

ENV_MIN_FREE_GB = "CCTRAIN_MIN_FREE_GB"
DEFAULT_MIN_FREE_GB = 10.0
VOLATILE_FILESYSTEMS = frozenset({"tmpfs", "ramfs", "devtmpfs"})
GIB = 1024**3
MOUNTS = Path("/proc/self/mounts")


class DiskError(RuntimeError):
    """Destino volátil ou espaço insuficiente."""


def _existing_ancestor(path: Path) -> Path:
    p = Path(path).expanduser().resolve()
    while not p.exists():
        p = p.parent
    return p


def _unescape(field: str) -> str:
    return field.replace("\\040", " ").replace("\\011", "\t").replace("\\012", "\n").replace("\\134", "\\")


def filesystem_type(path: Path, mounts_file: Path = MOUNTS) -> str:
    target = Path(path).expanduser().resolve()
    best: Path | None = None
    best_type: str | None = None
    for line in Path(mounts_file).read_text().splitlines():
        parts = line.split()
        if len(parts) < 3:
            continue
        mount_point, fs_type = Path(_unescape(parts[1])), parts[2]
        if target == mount_point or mount_point in target.parents:
            if best is None or len(mount_point.parts) >= len(best.parts):
                best, best_type = mount_point, fs_type
    if best_type is None:
        raise DiskError(f"sem ponto de montagem para {target}")
    return best_type


def assert_persistent(path: Path, mounts_file: Path = MOUNTS) -> None:
    fs = filesystem_type(path, mounts_file)
    if fs in VOLATILE_FILESYSTEMS:
        raise DiskError(
            f"{path} está em {fs} (volátil); dados, checkpoints e builds vão para disco persistente"
        )


def reserve_bytes(env: Mapping[str, str] | None = None) -> int:
    env = os.environ if env is None else env
    raw = env.get(ENV_MIN_FREE_GB)
    value = DEFAULT_MIN_FREE_GB if raw in (None, "") else float(raw)
    if value < 0:
        raise DiskError(f"{ENV_MIN_FREE_GB} negativo: {value}")
    return int(value * GIB)


def require_free(
    path: Path,
    need_bytes: int,
    *,
    reserve: int | None = None,
    mounts_file: Path = MOUNTS,
    usage: Callable = shutil.disk_usage,
) -> int:
    assert_persistent(path, mounts_file)
    reserve = reserve_bytes() if reserve is None else reserve
    free = usage(_existing_ancestor(Path(path))).free
    if free - need_bytes < reserve:
        raise DiskError(
            f"espaço insuficiente em {path}: livre {free / GIB:.2f} GiB, necessário {need_bytes / GIB:.2f} GiB"
            f" + reserva {reserve / GIB:.2f} GiB. Liberar disco é decisão do dono; nada é apagado aqui."
        )
    return free
```

`src/cctrain/dfvendor.py`:

```python
"""Acesso ao DeepFilterNet v0.5.6 vendorizado e ao checkpoint PyTorch do DFNet3."""

from __future__ import annotations

import importlib
import json
import os
import sys
import zipfile
from pathlib import Path
from types import ModuleType

from cctrain.disk import require_free
from cctrain.hashing import sha256_file
from cctrain.paths import ConfigError, Paths, repo_root

VENDORED_ROOT = repo_root() / "third_party" / "DeepFilterNet"
_PACKAGE_DIR = VENDORED_ROOT / "DeepFilterNet"
_ZIP_REL = "models/DeepFilterNet3.zip"
_MARKER = ".cctrain-extracted.json"


def vendored_info() -> dict:
    return json.loads((VENDORED_ROOT / "VENDORED.json").read_text())


def ensure_df_importable() -> ModuleType:
    path = str(_PACKAGE_DIR)
    if path not in sys.path:
        sys.path.insert(0, path)
    module = importlib.import_module("df")
    if not str(Path(module.__file__).resolve()).startswith(str(_PACKAGE_DIR.resolve())):
        raise ConfigError(f"pacote 'df' importado de fora do vendor: {module.__file__}")
    return module


def ensure_dfnet3_checkpoint(paths: Paths | None = None) -> Path:
    paths = paths or Paths.from_env()
    expected = vendored_info()["files"][_ZIP_REL]
    target = paths.data / "models" / "DeepFilterNet3"
    marker = target / _MARKER
    if marker.is_file() and json.loads(marker.read_text()).get("zip_sha256") == expected["sha256"]:
        return _model_dir(target)
    zip_path = VENDORED_ROOT / _ZIP_REL
    if sha256_file(zip_path) != expected["sha256"]:
        raise ConfigError(f"{zip_path}: SHA-256 difere de VENDORED.json")
    require_free(paths.data, 4 * expected["size"])
    staging = target.with_name(target.name + ".staging")
    if staging.exists():
        raise ConfigError(f"{staging} existe de uma extração interrompida; inspecione antes de continuar")
    staging.mkdir(parents=True)
    with zipfile.ZipFile(zip_path) as z:
        for info in z.infolist():
            name = Path(info.filename)
            if name.is_absolute() or ".." in name.parts:
                raise ConfigError(f"membro inseguro no zip: {info.filename}")
        z.extractall(staging)
    (staging / _MARKER).write_text(json.dumps({"zip_sha256": expected["sha256"]}) + "\n")
    if target.exists():
        raise ConfigError(f"{target} existe sem marcador válido; inspecione antes de continuar")
    os.replace(staging, target)
    return _model_dir(target)


def _model_dir(target: Path) -> Path:
    configs = sorted(p for p in target.rglob("config.ini"))
    if len(configs) != 1:
        raise ConfigError(f"esperado 1 config.ini em {target}, achados {len(configs)}")
    model_dir = configs[0].parent
    if not (model_dir / "checkpoints").is_dir():
        raise ConfigError(f"{model_dir} sem checkpoints/")
    return model_dir
```

- [ ] **Step 9: Rodar os testes até passarem**

```bash
cd "$HOME/orca/projects/clearcore-train" && source scripts/env.sh
"$CCTRAIN_PY" -m pytest tests -v -m "not network"
"$CCTRAIN_PY" -m ruff check src tests
```

Expected: todos PASS (inclusive `test_cuda_blackwell` e `test_df_package_imports`); ruff sem erros. Se `test_df_package_imports` falhar por símbolo removido, volte ao Step 3 (patch mínimo marcado).

- [ ] **Step 10: Commit**

```bash
cd "$HOME/orca/projects/clearcore-train"
git add .
git status --short | grep -E "^A  (data|runs|build|\.venv)/" && { echo "ERRO: diretório ignorado no índice"; exit 1; }
git commit -m "chore: bootstrap clearcore-train repo, train env and vendored DeepFilterNet v0.5.6"
```

---

### Task 2: Toolchain Rust persistente e `tract-check identity` (I2)

**Files:**
- Create: `tools/tract-check/Cargo.toml`, `tools/tract-check/Cargo.lock`, `tools/tract-check/rust-toolchain.toml`, `tools/tract-check/src/main.rs`, `tools/tract-check/src/common.rs`, `tools/tract-check/src/identity.rs`
- Create: `scripts/tract-check.sh`, `src/cctrain/tractcheck.py`
- Test: `tests/test_tractcheck_identity.py` (marcadores `tract`, `clearcore`); testes unitários Rust em `common.rs`

**Interfaces:**
- Consumes: `cctrain.paths.clearcore_root`, `cctrain.hashing.sha256_file` (Task 1).
- Produces:
  - Binário `tract-check` com `identity <film_asset.tar.gz> <approved_asset.bin> <golden.f32> <out.json>`; código de saída 0 = aprovado, 1 = reprovado, 2 = erro.
  - Módulo Rust `common` com `HOP`, `synthetic(frames)`, `sha256_hex`, `read_f32_le`, `write_f32_le`, `max_abs_diff`, `load_model(asset, &RuntimeParams)`, `process_frame(&mut DfTract, &[f32]) -> (Vec<f32>, f32)` (usado pela Task 14).
  - `scripts/tract-check.sh build|test|run ...`.
  - `cctrain.tractcheck`: `binary(env=None) -> Path`; `run(*args, timeout=900) -> subprocess.CompletedProcess`; constantes `UPSTREAM_GOLDEN_SHA256`, `APPROVED_ASSET_SHA256`, `CLEARCORE_FILM_IDENTITY_SHA256 = "8783baff55cad837953d5420c0f96edfbe40c01e5e2ac715cb4f3fef2b6cb45d"`.

- [ ] **Step 1: Instalar rustup e o Rust 1.90.0 em `~/.rustup`**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
for d in "$RUSTUP_HOME" "$CARGO_HOME"; do
  case "$d" in /tmp/*) echo "ERRO: $d em /tmp"; exit 1;; esac
done
if [ ! -x "$CARGO_HOME/bin/rustup" ]; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path --default-toolchain none --profile minimal
fi
"$CARGO_HOME/bin/rustup" toolchain install 1.90.0 --profile minimal --component clippy,rustfmt
"$CARGO_HOME/bin/rustup" run 1.90.0 rustc --version
du -sh "$RUSTUP_HOME"
```

Expected: `rustc 1.90.0 (...)`. `--no-modify-path` evita editar os arquivos de shell do usuário.

- [ ] **Step 2: Escrever o script de build**

`scripts/tract-check.sh`:

```bash
#!/usr/bin/env bash
# Build/test/run do tools/tract-check com a toolchain persistente (Rust 1.90.0 em ~/.rustup).
# O target fica no checkout principal (compartilhado entre worktrees), nunca em /tmp.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_ROOT="${CCTRAIN_ROOT:-$HERE}"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export PATH="$CARGO_HOME/bin:$PATH"
export CARGO_TARGET_DIR="$TARGET_ROOT/tools/tract-check/target"
for d in "$RUSTUP_HOME" "$CARGO_HOME" "$CARGO_TARGET_DIR"; do
  case "$d" in /tmp/*) echo "ERRO: $d em /tmp" >&2; exit 1;; esac
done
mkdir -p "$CARGO_TARGET_DIR"
fstype="$(findmnt -no FSTYPE --target "$CARGO_TARGET_DIR")"
[ "$fstype" != "tmpfs" ] || { echo "ERRO: CARGO_TARGET_DIR em tmpfs" >&2; exit 1; }
# O commit fixado do Clearcore ainda não está publicado: a URL pública é redirecionada ao checkout local.
export CARGO_NET_GIT_FETCH_WITH_CLI=true
if [ -n "${CCTRAIN_CLEARCORE_ROOT:-}" ]; then
  export GIT_CONFIG_COUNT=1
  export GIT_CONFIG_KEY_0="url.${CCTRAIN_CLEARCORE_ROOT}.insteadOf"
  export GIT_CONFIG_VALUE_0="https://github.com/joaorura/clearcore.git"
fi
MANIFEST="$HERE/tools/tract-check/Cargo.toml"
cmd="${1:-}"; shift || true
case "$cmd" in
  build|test)
    if [ ! -x "$CARGO_TARGET_DIR/release/tract-check" ]; then
      avail_kb=$(df --output=avail -k "$CARGO_TARGET_DIR" | tail -1)
      [ "$avail_kb" -ge $((14 * 1024 * 1024)) ] || { echo "ERRO: < 14 GiB livres (target ~3-4 GiB + reserva 10 GiB)" >&2; exit 1; }
    fi
    lock_flag="--locked"; [ -f "$HERE/tools/tract-check/Cargo.lock" ] || lock_flag=""
    cargo "$cmd" --release $lock_flag --manifest-path "$MANIFEST" "$@"
    ;;
  run) exec "$CARGO_TARGET_DIR/release/tract-check" "$@" ;;
  *) echo "uso: scripts/tract-check.sh build|test|run [args]" >&2; exit 2 ;;
esac
```

```bash
chmod +x "$CCTRAIN_WT/scripts/tract-check.sh"
```

- [ ] **Step 3: Escrever o crate com os testes unitários (falham sem a implementação)**

`tools/tract-check/rust-toolchain.toml`:

```toml
[toolchain]
channel = "1.90.0"
components = ["clippy", "rustfmt"]
profile = "minimal"
```

`tools/tract-check/Cargo.toml`:

```toml
[package]
name = "tract-check"
version = "0.1.0"
edition = "2021"
rust-version = "1.90"
publish = false
license = "MIT OR Apache-2.0"
description = "Carrega assets do Clearcore com o mesmo DfTract do runtime (tract 0.19.16): identidade, gating, enrollment"

[workspace]
resolver = "3"

[dependencies]
deep_filter = { git = "https://github.com/joaorura/clearcore.git", rev = "70dc85cf2c767cf2dbde9b569f7b72cb00d8d4aa", package = "deep_filter_fork", default-features = false, features = ["tract"] }
tract-core = { version = "=0.19.16", default-features = false }
tract-hir = { version = "=0.19.16", default-features = false }
tract-onnx = { version = "=0.19.16", default-features = false }
tract-pulse = { version = "=0.19.16", default-features = false }
ndarray = { version = "=0.15.6", default-features = false, features = ["std"] }
sha2 = "0.10"
serde_json = "1"
anyhow = "1"

[profile.release]
opt-level = 3
```

`tools/tract-check/src/common.rs`:

```rust
//! Utilidades compartilhadas: sinal sintético do `parity_film.rs` do Clearcore, E/S f32 e carga do DfTract.
use std::{fs, path::Path};

use anyhow::{bail, Context, Result};
use deep_filter::tract::{DfParams, DfTract, RuntimeParams};
use ndarray::{Array2, ArrayView2};
use sha2::{Digest, Sha256};

pub const HOP: usize = 480;

/// Sinal idêntico ao de `crates/model/tests/parity_film.rs` (pilha harmônica de 140 Hz, envelope de
/// 3 Hz, ruído LCG). Precisa reproduzir a aritmética bit a bit: sem `mul_add`.
#[allow(clippy::suboptimal_flops, clippy::cast_possible_truncation)]
pub fn synthetic(frames: usize) -> Vec<f32> {
    let mut state: u32 = 0x1234_5678;
    (0..frames * HOP)
        .map(|index| {
            let t = f64::from(u32::try_from(index).unwrap_or(u32::MAX)) / 48_000.0;
            let envelope = 0.5 * (1.0 + (2.0 * std::f64::consts::PI * 3.0 * t).sin());
            let voiced: f64 = (1..=12)
                .map(|h| (2.0 * std::f64::consts::PI * 140.0 * f64::from(h) * t).sin() / f64::from(h))
                .sum();
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = f64::from(state >> 8) / 8_388_608.0 - 1.0;
            (0.08 * envelope * voiced + 0.02 * noise).clamp(-0.99, 0.99) as f32
        })
        .collect()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn to_le_bytes(samples: &[f32]) -> Vec<u8> {
    samples.iter().flat_map(|s| s.to_le_bytes()).collect()
}

pub fn read_f32_le(path: &Path) -> Result<Vec<f32>> {
    let bytes = fs::read(path).with_context(|| format!("lendo {}", path.display()))?;
    if bytes.len() % 4 != 0 {
        bail!("{}: tamanho {} não é múltiplo de 4", path.display(), bytes.len());
    }
    Ok(bytes.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect())
}

pub fn write_f32_le(path: &Path, data: &[f32]) -> Result<()> {
    fs::write(path, to_le_bytes(data)).with_context(|| format!("gravando {}", path.display()))
}

pub fn max_abs_diff(a: &[f32], b: &[f32]) -> Result<f32> {
    if a.len() != b.len() {
        bail!("comprimentos diferentes: {} e {}", a.len(), b.len());
    }
    Ok(a.iter().zip(b).fold(0.0_f32, |acc, (x, y)| acc.max((x - y).abs())))
}

pub fn load_model(asset: &Path, params: &RuntimeParams) -> Result<DfTract> {
    let dfp = DfParams::new(asset.to_path_buf())?;
    DfTract::new(dfp, params)
}

pub fn process_frame(model: &mut DfTract, frame: &[f32]) -> Result<(Vec<f32>, f32)> {
    let input = ArrayView2::from_shape((1, HOP), frame)?;
    let mut output = Array2::<f32>::zeros((1, HOP));
    let lsnr = model.process(input, output.view_mut())?;
    Ok((output.into_raw_vec(), lsnr))
}

pub fn run_signal(model: &mut DfTract, signal: &[f32]) -> Result<Vec<f32>> {
    let mut out = Vec::with_capacity(signal.len());
    for frame in signal.chunks_exact(HOP) {
        out.extend(process_frame(model, frame)?.0);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn synthetic_is_bounded_and_deterministic() {
        let a = synthetic(10);
        assert_eq!(a.len(), 10 * HOP);
        assert!(a.iter().all(|x| x.abs() <= 0.99));
        assert_eq!(to_le_bytes(&a), to_le_bytes(&synthetic(10)));
    }

    #[test]
    fn max_abs_diff_rejects_length_mismatch() {
        assert!(max_abs_diff(&[0.0], &[0.0, 1.0]).is_err());
        assert_eq!(max_abs_diff(&[1.0, -2.0], &[1.5, -2.0]).unwrap(), 0.5);
    }
}
```

`tools/tract-check/src/identity.rs`:

```rust
//! I2 (spec §6.3): asset com FiLM em identidade × golden de 1.000 quadros do upstream, max_abs_diff == 0.
use std::{fs, path::Path};

use anyhow::{bail, Result};
use deep_filter::tract::{FilmVectors, RuntimeParams};
use serde_json::json;

use crate::common::{load_model, max_abs_diff, read_f32_le, run_signal, sha256_hex, synthetic, to_le_bytes, HOP};

pub const UPSTREAM_GOLDEN_SHA256: &str = "c041ee1fa4b2ce4bdad4f07e28b8fdcb901c56bab20380d17abd254d5c9736aa";

fn identity_vectors(hidden: usize) -> FilmVectors {
    FilmVectors {
        gamma_enc: vec![1.0; hidden],
        beta_enc: vec![0.0; hidden],
        gamma_df: vec![1.0; hidden],
        beta_df: vec![0.0; hidden],
    }
}

pub fn run(args: &[String]) -> Result<bool> {
    let [film_asset, approved_asset, golden_path, out_json] = args else {
        bail!("uso: tract-check identity <film_asset.tar.gz> <approved_asset.bin> <golden.f32> <out.json>");
    };
    let golden_bytes = fs::read(golden_path)?;
    let golden_sha = sha256_hex(&golden_bytes);
    if golden_sha != UPSTREAM_GOLDEN_SHA256 {
        bail!("golden {golden_path}: sha256 {golden_sha} != {UPSTREAM_GOLDEN_SHA256}");
    }
    let golden = read_f32_le(Path::new(golden_path))?;
    let frames = golden.len() / HOP;
    let signal = synthetic(frames);
    let params = RuntimeParams::default();

    let approved = run_signal(&mut load_model(Path::new(approved_asset), &params)?, &signal)?;
    let mut film_default = load_model(Path::new(film_asset), &params)?;
    let film_hidden = film_default.film_hidden();
    let out_default = run_signal(&mut film_default, &signal)?;
    let mut film_explicit = load_model(Path::new(film_asset), &params)?;
    film_explicit.set_film(&identity_vectors(film_hidden.unwrap_or(256)))?;
    let out_explicit = run_signal(&mut film_explicit, &signal)?;

    let sha_default = sha256_hex(&to_le_bytes(&out_default));
    let sha_explicit = sha256_hex(&to_le_bytes(&out_explicit));
    let sha_approved = sha256_hex(&to_le_bytes(&approved));
    let pass = film_hidden == Some(256)
        && sha_default == UPSTREAM_GOLDEN_SHA256
        && sha_explicit == UPSTREAM_GOLDEN_SHA256;
    let report = json!({
        "frames": frames,
        "film_hidden": film_hidden,
        "golden_sha256": golden_sha,
        "approved_out_sha256": sha_approved,
        "film_default_out_sha256": sha_default,
        "film_explicit_out_sha256": sha_explicit,
        "max_abs_diff_default_vs_golden": max_abs_diff(&out_default, &golden)?,
        "max_abs_diff_explicit_vs_golden": max_abs_diff(&out_explicit, &golden)?,
        "max_abs_diff_explicit_vs_approved": max_abs_diff(&out_explicit, &approved)?,
        "approved_bit_exact_vs_golden": sha_approved == UPSTREAM_GOLDEN_SHA256,
        "pass": pass,
    });
    fs::write(out_json, serde_json::to_string_pretty(&report)? + "\n")?;
    println!("{report}");
    Ok(pass)
}
```

`tools/tract-check/src/main.rs`:

```rust
mod common;
mod identity;

use std::process::ExitCode;

const USAGE: &str = "uso: tract-check identity <film_asset.tar.gz> <approved_asset.bin> <golden.f32> <out.json>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("identity") => identity::run(&args[1..]),
        _ => Err(anyhow::anyhow!(USAGE)),
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("erro: {e:#}");
            ExitCode::from(2)
        }
    }
}
```

- [ ] **Step 4: Gerar o lock, compilar e rodar os testes Rust**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
test -n "$CCTRAIN_CLEARCORE_ROOT" && git -C "$CCTRAIN_CLEARCORE_ROOT" cat-file -e 70dc85cf2c767cf2dbde9b569f7b72cb00d8d4aa^{commit}
scripts/tract-check.sh build
grep -A2 'name = "tract-onnx"' tools/tract-check/Cargo.lock
grep -A3 'name = "deep_filter_fork"' tools/tract-check/Cargo.lock
scripts/tract-check.sh test
```

Expected: build sem erro (avisos do código vendorizado são aceitáveis); `tract-onnx` `version = "0.19.16"`; `deep_filter_fork` com `source = "git+https://github.com/joaorura/clearcore.git?rev=70dc85c...#70dc85c..."`; 3 testes Rust PASS. Se o resolvedor puxar crate com `rust-version` > 1.90 mesmo com `resolver = "3"`, fixe a versão com `cargo update -p <crate> --precise <versão>` (rodando pelo script com `CARGO_HOME`/`RUSTUP_HOME` exportados) e registre no commit.

- [ ] **Step 5: Escrever o teste Python de integração (I2 contra o fixture do Clearcore)**

`src/cctrain/tractcheck.py`:

```python
"""Invoca o binário tools/tract-check (Rust, tract 0.19.16)."""

from __future__ import annotations

import os
import subprocess
from collections.abc import Mapping
from pathlib import Path

from cctrain.paths import ConfigError, repo_root

ENV_BIN = "CCTRAIN_TRACT_CHECK_BIN"
UPSTREAM_GOLDEN_SHA256 = "c041ee1fa4b2ce4bdad4f07e28b8fdcb901c56bab20380d17abd254d5c9736aa"
APPROVED_ASSET_SHA256 = "c94d91f70911001c946e0fabb4aa9adc37045f45a03b56008cb0c8244cb63616"
CLEARCORE_FILM_IDENTITY_SHA256 = "8783baff55cad837953d5420c0f96edfbe40c01e5e2ac715cb4f3fef2b6cb45d"


def binary(env: Mapping[str, str] | None = None) -> Path:
    env = os.environ if env is None else env
    raw = env.get(ENV_BIN)
    path = Path(raw) if raw else repo_root() / "tools/tract-check/target/release/tract-check"
    if not path.is_file():
        raise ConfigError(f"tract-check não compilado em {path}: rode scripts/tract-check.sh build")
    return path


def run(*args: object, timeout: float = 900) -> subprocess.CompletedProcess:
    return subprocess.run(
        [str(binary()), *map(str, args)], capture_output=True, text=True, timeout=timeout, check=False
    )
```

`tests/test_tractcheck_identity.py`:

```python
import json

import pytest

from cctrain import tractcheck
from cctrain.hashing import sha256_file
from cctrain.paths import clearcore_root

pytestmark = [pytest.mark.tract, pytest.mark.clearcore]


def test_i2_on_clearcore_film_identity_fixture(m0_runs):
    root = clearcore_root()
    film = root / "fixtures/film/film-identity-asset.tar.gz"
    approved = root / "vendor/approved/df-compatible-release-asset-v1.bin"
    golden = root / "fixtures/film/upstream-golden-1000f.f32"
    assert sha256_file(film) == tractcheck.CLEARCORE_FILM_IDENTITY_SHA256
    assert sha256_file(approved) == tractcheck.APPROVED_ASSET_SHA256
    out = m0_runs / "i2_clearcore_fixture.json"
    proc = tractcheck.run("identity", film, approved, golden, out)
    assert proc.returncode == 0, proc.stdout + proc.stderr
    report = json.loads(out.read_text())
    assert report["pass"] is True
    assert report["film_hidden"] == 256
    assert report["max_abs_diff_explicit_vs_golden"] == 0.0
    assert report["max_abs_diff_explicit_vs_approved"] == 0.0
```

- [ ] **Step 6: Rodar o teste de integração**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" -m pytest tests/test_tractcheck_identity.py -v
"$CCTRAIN_PY" -m ruff check src tests
```

Expected: PASS. Se `pass` for `false` com diferença > 0 (CPU diferente da do golden), **pare** e reporte o JSON: I2 exige `max_abs_diff == 0`, sem tolerância.

- [ ] **Step 7: Commit**

```bash
cd "$CCTRAIN_WT"
git add tools/tract-check/Cargo.toml tools/tract-check/Cargo.lock tools/tract-check/rust-toolchain.toml tools/tract-check/src scripts/tract-check.sh src/cctrain/tractcheck.py tests/test_tractcheck_identity.py
git commit -m "feat(tract-check): persistent Rust 1.90 toolchain and identity (I2) check against tract 0.19.16"
```

---
### Task 3: Allowlist de licenças e manifests

**Files:**
- Create: `src/cctrain/data/licenses.py`, `src/cctrain/data/manifest.py`
- Create: `manifests/sources/vctk-0.92.yaml`, `manifests/sources/librispeech-dev-clean.yaml`, `manifests/sources/noise-wikimedia-cc0.yaml`, `manifests/sources/cv26-ptbr.yaml`, `manifests/sources/rir-synthetic-v1.yaml`, `manifests/sources.lock.json`
- Test: `tests/test_data_licenses.py`, `tests/test_data_manifest.py`

**Interfaces:**
- Consumes: `cctrain.paths.repo_root` (Task 1).
- Produces:
  - `cctrain.data.licenses`: `Role(str, Enum)` com `TRAIN="train"`, `TEST_ONLY="test_only"`, `TOOL_ONLY="tool_only"`; `TRAIN_ALLOWED: frozenset[str]`; `LicenseRefused(PermissionError)`; `check_train_allowed(license_id: str, role: Role | str, *, what: str) -> None`.
  - `cctrain.data.manifest`: `ManifestError(ValueError)`; `SourceManifest` (campos `id, name, url, landing_page, license, license_evidence, role: Role, attribution, native_sr: int | None, access, artifacts: tuple[dict, ...], restrictions: tuple[str, ...]`); `load_sources(directory=SOURCES_DIR) -> dict[str, SourceManifest]`; `FileRecord` (campos `path, sha256, source_id, license, role, speaker, split, sr, num_samples, band_hz, codec`) com `to_dict()`/`from_dict()`; `SPLITS = {"pilot","test","noise","rir","dev"}`; `write_file_manifest(records, path) -> str` (SHA-256 do arquivo); `read_file_manifest(path) -> list[FileRecord]`; `training_records(records, sources) -> list[FileRecord]`; `update_lock(source_id, artifact, sha256, size, *, upstream_checksum=None, lock_path=LOCK_PATH) -> None`; `lock_entry(source_id, artifact, lock_path=LOCK_PATH) -> dict | None`; constantes `SOURCES_DIR`, `FILES_DIR`, `LOCK_PATH`.

- [ ] **Step 1: Escrever os testes que falham**

`tests/test_data_licenses.py`:

```python
import pytest

from cctrain.data.licenses import TRAIN_ALLOWED, LicenseRefused, Role, check_train_allowed


@pytest.mark.parametrize("lic", sorted(TRAIN_ALLOWED))
def test_allowlisted_licenses_pass_for_train(lic):
    check_train_allowed(lic, Role.TRAIN, what="x")


def test_allowlist_is_exactly_the_spec_set_plus_generated():
    assert TRAIN_ALLOWED == {
        "CC0-1.0",
        "CC-BY-4.0",
        "Apache-2.0",
        "LicenseRef-public-domain-declared",
        "LicenseRef-cctrain-generated",
    }


@pytest.mark.parametrize(
    "lic",
    ["CC-BY-NC-4.0", "CC-BY-NC-SA-4.0", "CC-BY-SA-3.0", "CC-BY-ND-4.0", "ODbL-1.0", "DbCL-1.0",
     "GPL-3.0-or-later", "LicenseRef-research-only", "unknown", "", "cc-by-4.0"],
)
def test_everything_else_is_refused(lic):
    with pytest.raises(LicenseRefused):
        check_train_allowed(lic, Role.TRAIN, what="x")


def test_test_only_role_is_refused_even_with_permissive_license():
    with pytest.raises(LicenseRefused, match="test_only"):
        check_train_allowed("CC-BY-4.0", Role.TEST_ONLY, what="demand.wav")


def test_unknown_role_is_refused():
    with pytest.raises(ValueError):
        check_train_allowed("CC0-1.0", "whatever", what="x")
```

`tests/test_data_manifest.py`:

```python
import json
from pathlib import Path

import pytest

from cctrain.data.licenses import LicenseRefused
from cctrain.data.manifest import (
    FileRecord,
    ManifestError,
    load_sources,
    lock_entry,
    read_file_manifest,
    training_records,
    update_lock,
    write_file_manifest,
)

SHA = "a" * 64


def _rec(**kw) -> FileRecord:
    base = dict(path="raw/vctk/p227/p227_001_mic1.flac", sha256=SHA, source_id="vctk-0.92", license="CC-BY-4.0",
                role="train", speaker="p227", split="pilot", sr=48000, num_samples=48000, band_hz=20000.0,
                codec="flac")
    base.update(kw)
    return FileRecord.from_dict(base)


def test_pilot_sources_load_and_ids_match_filenames():
    sources = load_sources()
    assert set(sources) == {"vctk-0.92", "librispeech-dev-clean", "noise-wikimedia-cc0", "cv26-ptbr",
                            "rir-synthetic-v1"}
    assert sources["vctk-0.92"].license == "CC-BY-4.0"
    assert sources["cv26-ptbr"].license == "CC0-1.0"
    assert "no_rehost" in sources["cv26-ptbr"].restrictions
    assert len(sources["noise-wikimedia-cc0"].artifacts) == 3


def test_source_missing_key_is_rejected(tmp_path: Path):
    (tmp_path / "x.yaml").write_text("id: x\nname: X\n")
    with pytest.raises(ManifestError, match="faltam"):
        load_sources(tmp_path)


def test_source_id_must_match_filename(tmp_path: Path):
    src = (Path(__file__).resolve().parents[1] / "manifests/sources/vctk-0.92.yaml").read_text()
    (tmp_path / "other.yaml").write_text(src)
    with pytest.raises(ManifestError, match="id"):
        load_sources(tmp_path)


@pytest.mark.parametrize("bad", [dict(sha256="xyz"), dict(path="../escape.flac"), dict(path="/abs.flac"),
                                 dict(sr=0), dict(split="train"), dict(role="nope")])
def test_file_record_validation(bad):
    with pytest.raises((ManifestError, ValueError)):
        _rec(**bad)


def test_write_read_roundtrip_is_deterministic(tmp_path: Path):
    recs = [_rec(path="b.flac"), _rec(path="a.flac")]
    h1 = write_file_manifest(recs, tmp_path / "m1.jsonl")
    h2 = write_file_manifest(list(reversed(recs)), tmp_path / "m2.jsonl")
    assert h1 == h2
    back = read_file_manifest(tmp_path / "m1.jsonl")
    assert [r.path for r in back] == ["a.flac", "b.flac"]
    assert back[0] == _rec(path="a.flac")


def test_training_records_enforce_allowlist_and_source_license():
    sources = load_sources()
    assert training_records([_rec()], sources) == [_rec()]
    with pytest.raises(ManifestError, match="licença"):
        training_records([_rec(license="CC0-1.0")], sources)
    with pytest.raises(LicenseRefused):
        training_records([_rec(role="test_only")], sources)
    with pytest.raises(ManifestError, match="fonte"):
        training_records([_rec(source_id="voxceleb")], sources)


def test_lock_refuses_hash_change(tmp_path: Path):
    lock = tmp_path / "lock.json"
    update_lock("s", "a.tar.gz", SHA, 10, upstream_checksum="md5:00", lock_path=lock)
    update_lock("s", "a.tar.gz", SHA, 10, upstream_checksum="md5:00", lock_path=lock)
    assert lock_entry("s", "a.tar.gz", lock) == {"sha256": SHA, "size": 10, "upstream_checksum": "md5:00"}
    with pytest.raises(ManifestError, match="mudou"):
        update_lock("s", "a.tar.gz", "b" * 64, 10, lock_path=lock)
    assert json.loads(lock.read_text())["s"]["a.tar.gz"]["sha256"] == SHA
```

- [ ] **Step 2: Rodar e ver falhar**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" -m pytest tests/test_data_licenses.py tests/test_data_manifest.py -v 2>&1 | tail -5
```

Expected: FAIL com `ModuleNotFoundError: No module named 'cctrain.data.licenses'`.

- [ ] **Step 3: Implementar `licenses.py`**

`src/cctrain/data/licenses.py`:

```python
"""Allowlist de licenças do treino (spec §4.1). Tudo o que não está na lista é recusado."""

from __future__ import annotations

from enum import Enum


class Role(str, Enum):
    TRAIN = "train"
    TEST_ONLY = "test_only"
    TOOL_ONLY = "tool_only"


# `LicenseRef-cctrain-generated`: dados gerados pelo próprio repo, sem terceiros (RIR sintéticas).
# Não está na lista da spec §4.1; decisão de planejamento 6, a confirmar pelo dono no M5.
TRAIN_ALLOWED = frozenset(
    {
        "CC0-1.0",
        "CC-BY-4.0",
        "Apache-2.0",
        "LicenseRef-public-domain-declared",
        "LicenseRef-cctrain-generated",
    }
)


class LicenseRefused(PermissionError):
    """Arquivo fora da allowlist ou de fonte só-teste/ferramenta."""


def check_train_allowed(license_id: str, role: Role | str, *, what: str) -> None:
    role = Role(role)
    if role is not Role.TRAIN:
        raise LicenseRefused(f"{what}: papel {role.value} não pode entrar no treino")
    if license_id not in TRAIN_ALLOWED:
        raise LicenseRefused(
            f"{what}: licença {license_id!r} fora da allowlist {sorted(TRAIN_ALLOWED)}"
            " (NC, ND, SA, ODbL/DbCL, GPL, research-only e não confirmadas ficam fora)"
        )
```

- [ ] **Step 4: Implementar `manifest.py`**

`src/cctrain/data/manifest.py`:

```python
"""Manifests de fonte (YAML, um por fonte) e por arquivo (JSONL), e o lock de hashes dos brutos."""

from __future__ import annotations

import json
import os
import re
from collections.abc import Iterable, Mapping
from dataclasses import asdict, dataclass, field
from pathlib import Path, PurePosixPath

import yaml

from cctrain.data.licenses import Role, check_train_allowed
from cctrain.hashing import sha256_file
from cctrain.paths import repo_root

SOURCES_DIR = repo_root() / "manifests" / "sources"
FILES_DIR = repo_root() / "manifests" / "files"
LOCK_PATH = repo_root() / "manifests" / "sources.lock.json"
SPLITS = frozenset({"pilot", "test", "noise", "rir", "dev"})
_SHA_RE = re.compile(r"^[0-9a-f]{64}$")
_REQUIRED = ("id", "name", "url", "landing_page", "license", "license_evidence", "role", "attribution",
             "native_sr", "access")


class ManifestError(ValueError):
    """Manifest inválido ou inconsistente. Falha fechado."""


@dataclass(frozen=True)
class SourceManifest:
    id: str
    name: str
    url: str
    landing_page: str
    license: str
    license_evidence: str
    role: Role
    attribution: str
    native_sr: int | None
    access: str
    artifacts: tuple[dict, ...] = field(default_factory=tuple)
    restrictions: tuple[str, ...] = field(default_factory=tuple)

    @classmethod
    def from_yaml(cls, path: Path) -> SourceManifest:
        data = yaml.safe_load(Path(path).read_text())
        if not isinstance(data, Mapping):
            raise ManifestError(f"{path}: não é um mapeamento YAML")
        missing = [k for k in _REQUIRED if k not in data]
        if missing:
            raise ManifestError(f"{path}: faltam {missing}")
        if data["id"] != Path(path).stem:
            raise ManifestError(f"{path}: id {data['id']!r} difere do nome do arquivo")
        if not str(data["license"]).strip():
            raise ManifestError(f"{path}: licença vazia")
        return cls(
            id=data["id"], name=data["name"], url=data["url"], landing_page=data["landing_page"],
            license=data["license"], license_evidence=data["license_evidence"], role=Role(data["role"]),
            attribution=data["attribution"], native_sr=data["native_sr"], access=data["access"],
            artifacts=tuple(data.get("artifacts") or ()), restrictions=tuple(data.get("restrictions") or ()),
        )


def load_sources(directory: Path = SOURCES_DIR) -> dict[str, SourceManifest]:
    out: dict[str, SourceManifest] = {}
    for path in sorted(Path(directory).glob("*.yaml")):
        src = SourceManifest.from_yaml(path)
        out[src.id] = src
    return out


@dataclass(frozen=True)
class FileRecord:
    path: str
    sha256: str
    source_id: str
    license: str
    role: str
    speaker: str | None
    split: str
    sr: int
    num_samples: int
    band_hz: float | None
    codec: str

    def __post_init__(self) -> None:
        p = PurePosixPath(self.path)
        if p.is_absolute() or ".." in p.parts or not self.path:
            raise ManifestError(f"caminho inválido: {self.path!r}")
        if not _SHA_RE.match(self.sha256):
            raise ManifestError(f"{self.path}: sha256 inválido")
        if self.sr <= 0 or self.num_samples < 0:
            raise ManifestError(f"{self.path}: sr/num_samples inválidos")
        if self.split not in SPLITS:
            raise ManifestError(f"{self.path}: split {self.split!r} fora de {sorted(SPLITS)}")
        Role(self.role)

    def to_dict(self) -> dict:
        return asdict(self)

    @classmethod
    def from_dict(cls, d: Mapping) -> FileRecord:
        return cls(**{k: d[k] for k in cls.__dataclass_fields__})


def write_file_manifest(records: Iterable[FileRecord], path: Path) -> str:
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    lines = [json.dumps(r.to_dict(), sort_keys=True, ensure_ascii=False) for r in sorted(records, key=lambda r: r.path)]
    tmp = path.with_name(path.name + ".tmp")
    tmp.write_text("".join(line + "\n" for line in lines))
    os.replace(tmp, path)
    return sha256_file(path)


def read_file_manifest(path: Path) -> list[FileRecord]:
    return [FileRecord.from_dict(json.loads(line)) for line in Path(path).read_text().splitlines() if line]


def training_records(records: Iterable[FileRecord], sources: Mapping[str, SourceManifest]) -> list[FileRecord]:
    out = []
    for r in records:
        src = sources.get(r.source_id)
        if src is None:
            raise ManifestError(f"{r.path}: fonte {r.source_id!r} sem manifest")
        if r.license != src.license:
            raise ManifestError(f"{r.path}: licença {r.license!r} difere da fonte ({src.license!r})")
        check_train_allowed(r.license, r.role, what=r.path)
        out.append(r)
    return out


def _read_lock(lock_path: Path) -> dict:
    return json.loads(Path(lock_path).read_text()) if Path(lock_path).is_file() else {}


def lock_entry(source_id: str, artifact: str, lock_path: Path = LOCK_PATH) -> dict | None:
    return _read_lock(lock_path).get(source_id, {}).get(artifact)


def update_lock(source_id: str, artifact: str, sha256: str, size: int, *, upstream_checksum: str | None = None,
                lock_path: Path = LOCK_PATH) -> None:
    if not _SHA_RE.match(sha256):
        raise ManifestError(f"sha256 inválido para {source_id}/{artifact}")
    lock = _read_lock(lock_path)
    current = lock.get(source_id, {}).get(artifact)
    entry = {"sha256": sha256, "size": size, "upstream_checksum": upstream_checksum}
    if current is not None and (current["sha256"] != sha256 or current["size"] != size):
        raise ManifestError(
            f"{source_id}/{artifact}: hash mudou ({current['sha256']} -> {sha256}); o bruto upstream mudou,"
            " investigue antes de aceitar"
        )
    lock.setdefault(source_id, {})[artifact] = entry
    tmp = Path(lock_path).with_name(Path(lock_path).name + ".tmp")
    tmp.write_text(json.dumps(lock, indent=2, sort_keys=True) + "\n")
    os.replace(tmp, lock_path)
```

- [ ] **Step 5: Escrever os manifests de fonte do piloto**

`manifests/sources/vctk-0.92.yaml`:

```yaml
id: vctk-0.92
name: "CSTR VCTK Corpus 0.92 (somente mic1)"
url: "https://datashare.ed.ac.uk/server/api/core/bitstreams/535f4286-e54c-4038-838c-a02285e32cb2/content"
landing_page: "https://datashare.ed.ac.uk/handle/10283/3443"
license: CC-BY-4.0
license_evidence: "DataShare ?show=full (R04 §1.1); o arquivo de licença dentro do zip é conferido na Task 12"
role: train
attribution: "Yamagishi, J., Veaux, C., MacDonald, K. CSTR VCTK Corpus 0.92, University of Edinburgh. CC BY 4.0. doi:10.7488/ds/2645"
native_sr: 48000
access: http_range_zip
```

`manifests/sources/librispeech-dev-clean.yaml`:

```yaml
id: librispeech-dev-clean
name: "LibriSpeech dev-clean (OpenSLR 12)"
url: "https://www.openslr.org/resources/12/dev-clean.tar.gz"
landing_page: "https://www.openslr.org/12/"
license: CC-BY-4.0
license_evidence: "OpenSLR 12 (R03 §3, R04 §1.1)"
role: train
attribution: "Panayotov, V., Chen, G., Povey, D., Khudanpur, S. LibriSpeech: an ASR corpus based on public domain audio books. OpenSLR 12. CC BY 4.0."
native_sr: 16000
access: http_tar_stream
```

`manifests/sources/noise-wikimedia-cc0.yaml` (mesmos recortes do `scripts/fetch-voice-samples.sh` do Clearcore; esses ruídos são de **teste** do Clearcore e entram no M0 só para medição, com `split: test`):

```yaml
id: noise-wikimedia-cc0
name: "Ruídos CC0 do Wikimedia Commons usados nos testes do Clearcore"
url: "https://commons.wikimedia.org/"
landing_page: "https://commons.wikimedia.org/"
license: CC0-1.0
license_evidence: "Página de cada arquivo no Wikimedia Commons (CC0); mesma seleção de scripts/fetch-voice-samples.sh do Clearcore"
role: train
attribution: "Wikimedia Commons contributors, CC0 1.0"
native_sr: 48000
access: http_range_ffmpeg_cut
artifacts:
  - name: street_traffic_rain_cc0_10s
    url: "https://upload.wikimedia.org/wikipedia/commons/8/8c/Urban_Street_on_a_Rainy_Afternoon.flac"
    page: "https://commons.wikimedia.org/wiki/File:Urban_Street_on_a_Rainy_Afternoon.flac"
    range_bytes: 6000000
    start_s: 5
    raw_ext: flac
    clearcore_sha256: "53f24f9e9e13b04a3285094f145aac5b0f52dabf4caf85bda4c76d61e9c237b3"
  - name: forest_rain_cc0_10s
    url: "https://upload.wikimedia.org/wikipedia/commons/b/b6/Light_Rain_Distant_Thunder_July_5th_2016.wav"
    page: "https://commons.wikimedia.org/wiki/File:Light_Rain_Distant_Thunder_July_5th_2016.wav"
    range_bytes: 5000000
    start_s: 0
    raw_ext: wav
    clearcore_sha256: "4a02931545d941cccf7526ebd3c892234dee31915882dbb1e2f786ec472da1a1"
  - name: ac_fan_cc0_10s
    url: "https://upload.wikimedia.org/wikipedia/commons/d/df/Resident_air-conditioned_out_door_unit.ogg"
    page: "https://commons.wikimedia.org/wiki/File:Resident_air-conditioned_out_door_unit.ogg"
    range_bytes: 0
    start_s: 5
    raw_ext: ogg
    clearcore_sha256: "3763bff659ff596d5fa70c673b5aa7901abf39696f1a24810dc551a91abb7c8e"
```

`manifests/sources/cv26-ptbr.yaml`:

```yaml
id: cv26-ptbr
name: "Common Voice Scripted Speech 26.0 - Brazilian Portuguese"
url: "https://mozilladatacollective.com/datasets/cmruxo9ew00d3md07veethj6k"
landing_page: "https://mozilladatacollective.com/datasets/cmruxo9ew00d3md07veethj6k"
license: CC0-1.0
license_evidence: "Página oficial do Mozilla Data Collective (R05 §2), lida em 2026-10-03; download exige aceite dos termos pela conta do dono"
role: train
attribution: "Mozilla Common Voice contributors, CC0 1.0"
native_sr: 48000
access: manual_owner_download
restrictions: [no_rehost, no_speaker_identification, no_label_publication]
```

`manifests/sources/rir-synthetic-v1.yaml`:

```yaml
id: rir-synthetic-v1
name: "RIR sintéticas (image source via pyroomacoustics), geradas por cctrain"
url: "local:scripts/gen_rir_pilot.py"
landing_page: "local"
license: LicenseRef-cctrain-generated
license_evidence: "Gerado por este repositório, sem dados de terceiros (spec §4.2; decisão de planejamento 6)"
role: train
attribution: "clearcore-train"
native_sr: 48000
access: generated
```

`manifests/sources.lock.json`:

```json
{}
```

- [ ] **Step 6: Rodar os testes até passarem**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" -m pytest tests/test_data_licenses.py tests/test_data_manifest.py -v
"$CCTRAIN_PY" -m ruff check src tests
```

Expected: todos PASS.

- [ ] **Step 7: Commit**

```bash
cd "$CCTRAIN_WT"
git add src/cctrain/data/licenses.py src/cctrain/data/manifest.py manifests tests/test_data_licenses.py tests/test_data_manifest.py
git commit -m "feat(data): license allowlist, source/file manifests and raw-hash lock"
```

---
### Task 4: `resampler-48k-16k-v1` com vetores dourados

**Files:**
- Create: `src/cctrain/data/resample.py`, `scripts/gen_resampler_v1.py`
- Create (gerados e versionados): `resampler/v1/coeffs.f32`, `resampler/v1/coeffs.sha256`, `resampler/v1/manifest.json`, `resampler/v1/golden/*.in.f32`, `resampler/v1/golden/*.out.f32`
- Test: `tests/test_data_resample.py`

**Interfaces:**
- Consumes: `cctrain.hashing`, `cctrain.paths.repo_root` (Task 1).
- Produces: `cctrain.data.resample`: constantes `NAME="resampler-48k-16k-v1"`, `TAPS=241`, `CENTER=120`, `DECIMATION=3`, `FS_IN=48000`, `FS_OUT=16000`, `CUTOFF_HZ=7600.0`, `KAISER_BETA=8.6`, `V1_DIR`; `design_coefficients() -> np.ndarray` (f32 `[241]`); `load_coefficients(directory=V1_DIR) -> np.ndarray`; `output_length(n: int) -> int`; `resample_48k_to_16k(x: np.ndarray, coeffs: np.ndarray | None = None) -> np.ndarray` (f32 1-D); `golden_inputs() -> dict[str, np.ndarray]`.

- [ ] **Step 1: Escrever os testes que falham**

`tests/test_data_resample.py`:

```python
import json

import numpy as np
import pytest

from cctrain.data import resample as rs
from cctrain.hashing import sha256_bytes

# SHA-256 dos coeficientes f32 calculado no planejamento (numpy do sistema). Se divergir, pare e reporte.
PLANNING_COEFFS_SHA256 = "d230da4dc7dd9ebbdee979dc33a11d3ee61cf2fe29477c768745442865fd7043"


def test_design_matches_published_file_bit_exact():
    assert np.array_equal(rs.design_coefficients(), rs.load_coefficients())


def test_coefficients_hash_is_recorded_and_matches_planning():
    raw = rs.design_coefficients().astype("<f4").tobytes()
    assert sha256_bytes(raw) == (rs.V1_DIR / "coeffs.sha256").read_text().split()[0]
    assert sha256_bytes(raw) == PLANNING_COEFFS_SHA256


def test_linear_phase_and_unit_dc_gain():
    h = rs.load_coefficients().astype(np.float64)
    assert h.shape == (241,)
    assert np.max(np.abs(h - h[::-1])) <= 1e-9
    assert abs(h.sum() - 1.0) < 1e-6


def test_matches_scipy_firwin():
    from scipy.signal import firwin

    ref = firwin(241, 7600.0, window=("kaiser", 8.6), fs=48000.0)
    assert np.max(np.abs(ref.astype(np.float32) - rs.design_coefficients())) <= 1e-8


def test_frequency_response():
    h = rs.load_coefficients().astype(np.float64)
    n = 1 << 18
    mag = 20 * np.log10(np.abs(np.fft.rfft(h, n)) + 1e-300)
    f = np.fft.rfftfreq(n, 1 / 48000)
    assert np.max(np.abs(mag[f <= 7000])) <= 1e-3
    assert np.max(mag[f >= 8200]) <= -85.0
    assert abs(mag[np.argmin(np.abs(f - 7600))] + 6.02) <= 0.1


@pytest.mark.parametrize("n", range(0, 11))
def test_output_length_is_ceil_third(n):
    assert len(rs.resample_48k_to_16k(np.zeros(n, np.float32))) == (n + 2) // 3


def test_impulse_response_is_delay_compensated():
    h64 = rs.load_coefficients().astype(np.float64)
    x = np.zeros(3000, np.float32)
    x[1500] = 1.0
    y = rs.resample_48k_to_16k(x)
    for n in range(len(y)):
        k = 3 * n + 120 - 1500
        want = np.float32(h64[k]) if 0 <= k < 241 else np.float32(0.0)
        assert y[n] == want, n


def test_sine_passes_with_zero_delay():
    t = np.arange(48000) / 48000.0
    y = rs.resample_48k_to_16k((0.5 * np.sin(2 * np.pi * 1000 * t)).astype(np.float32))
    n = np.arange(len(y))
    ref = 0.5 * np.sin(2 * np.pi * 1000 * n / 16000.0)
    assert np.max(np.abs(y[100:-100] - ref[100:-100])) <= 1e-3


def test_goldens_reproduce_bit_exact():
    manifest = json.loads((rs.V1_DIR / "manifest.json").read_text())
    inputs = rs.golden_inputs()
    assert set(manifest["golden"]) == set(inputs)
    for name, x in inputs.items():
        entry = manifest["golden"][name]
        x_file = np.fromfile(rs.V1_DIR / "golden" / f"{name}.in.f32", dtype="<f4")
        y_file = np.fromfile(rs.V1_DIR / "golden" / f"{name}.out.f32", dtype="<f4")
        assert np.array_equal(x_file, x), name
        assert np.array_equal(y_file, rs.resample_48k_to_16k(x)), name
        assert sha256_bytes(x_file.tobytes()) == entry["in_sha256"]
        assert sha256_bytes(y_file.tobytes()) == entry["out_sha256"]
        assert entry["n_out"] == rs.output_length(entry["n_in"])
```

- [ ] **Step 2: Rodar e ver falhar**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" -m pytest tests/test_data_resample.py -v 2>&1 | tail -3
```

Expected: FAIL com `ImportError` (`cctrain.data.resample` não existe).

- [ ] **Step 3: Implementar `resample.py`**

`src/cctrain/data/resample.py`:

```python
"""resampler-48k-16k-v1 (spec §2.2).

FIR de fase linear, 241 taps, janela Kaiser beta=8,6, corte 7.600 Hz, decimação por 3:
y[n] = sum_k h[k] * x[3n + 120 - k], com zeros fora de [0, L). Decisões de planejamento (item 7):
h projetado em float64 com ganho DC 1 e publicado em f32; saída com ceil(L/3) amostras; acumulação em
float64 sobre os coeficientes f32; saída arredondada para f32.
"""

from __future__ import annotations

from pathlib import Path

import numpy as np

from cctrain.hashing import sha256_bytes
from cctrain.paths import repo_root

NAME = "resampler-48k-16k-v1"
TAPS = 241
CENTER = 120
DECIMATION = 3
FS_IN = 48_000
FS_OUT = 16_000
CUTOFF_HZ = 7_600.0
KAISER_BETA = 8.6
V1_DIR = repo_root() / "resampler" / "v1"


def design_coefficients() -> np.ndarray:
    k = np.arange(TAPS, dtype=np.float64) - CENTER
    fc = CUTOFF_HZ / FS_IN
    h = 2.0 * fc * np.sinc(2.0 * fc * k) * np.kaiser(TAPS, KAISER_BETA)
    h = h / h.sum()
    return h.astype(np.float32)


def load_coefficients(directory: Path = V1_DIR) -> np.ndarray:
    raw = (Path(directory) / "coeffs.f32").read_bytes()
    expected = (Path(directory) / "coeffs.sha256").read_text().split()[0]
    if sha256_bytes(raw) != expected:
        raise ValueError(f"{directory}/coeffs.f32: SHA-256 não confere com coeffs.sha256")
    h = np.frombuffer(raw, dtype="<f4").astype(np.float32)
    if h.shape != (TAPS,):
        raise ValueError(f"esperados {TAPS} coeficientes, lidos {h.shape}")
    return h


def output_length(n: int) -> int:
    return (n + DECIMATION - 1) // DECIMATION


def resample_48k_to_16k(x: np.ndarray, coeffs: np.ndarray | None = None) -> np.ndarray:
    x = np.asarray(x, dtype=np.float32)
    if x.ndim != 1:
        raise ValueError("entrada mono 1-D")
    h = (load_coefficients() if coeffs is None else np.asarray(coeffs, dtype=np.float32)).astype(np.float64)
    n_out = output_length(len(x))
    if n_out == 0:
        return np.zeros(0, dtype=np.float32)
    padded = np.zeros(len(x) + 2 * CENTER + DECIMATION, dtype=np.float64)
    padded[CENTER : CENTER + len(x)] = x
    full = np.convolve(padded, h)  # full[m] = sum_k h[k] * padded[m - k]
    idx = DECIMATION * np.arange(n_out) + 2 * CENTER  # x[3n+120-k] = padded[3n+240-k]
    return full[idx].astype(np.float32)


def golden_inputs() -> dict[str, np.ndarray]:
    t = np.arange(48_000, dtype=np.float64) / 48_000.0
    impulses = np.zeros(3000, dtype=np.float32)
    impulses[[0, 1000, 2001]] = [1.0, 0.5, -0.25]
    chirp_phase = 2 * np.pi * (50.0 * t + (23_950.0 - 50.0) * t**2 / 2.0)
    out = {
        "impulses_3000": impulses,
        "dc_4800": np.full(4800, 0.5, dtype=np.float32),
        "sine_1k_1s": (0.5 * np.sin(2 * np.pi * 1000 * t)).astype(np.float32),
        "chirp_50_23950_1s": (0.5 * np.sin(chirp_phase)).astype(np.float32),
        "square_440_099_1s": (0.99 * np.sign(np.sin(2 * np.pi * 440 * t))).astype(np.float32),
        "noise_1s": (0.1 * np.random.default_rng(20261003).standard_normal(48_000)).astype(np.float32),
    }
    for n in (1, 2, 3, 4, 241, 1000):
        out[f"noise_len{n}"] = (0.1 * np.random.default_rng(n).standard_normal(n)).astype(np.float32)
    return out
```

- [ ] **Step 4: Escrever o gerador e gerar os arquivos**

`scripts/gen_resampler_v1.py`:

```python
"""Gera resampler/v1: coeficientes f32, SHA-256 e vetores dourados (determinístico)."""

import json

from cctrain.data import resample as rs
from cctrain.hashing import sha256_bytes


def main() -> None:
    out = rs.V1_DIR
    (out / "golden").mkdir(parents=True, exist_ok=True)
    h = rs.design_coefficients().astype("<f4")
    (out / "coeffs.f32").write_bytes(h.tobytes())
    coeffs_sha = sha256_bytes(h.tobytes())
    (out / "coeffs.sha256").write_text(f"{coeffs_sha}  coeffs.f32\n")
    golden = {}
    for name, x in rs.golden_inputs().items():
        y = rs.resample_48k_to_16k(x, h)
        (out / "golden" / f"{name}.in.f32").write_bytes(x.astype("<f4").tobytes())
        (out / "golden" / f"{name}.out.f32").write_bytes(y.astype("<f4").tobytes())
        golden[name] = {"n_in": int(len(x)), "n_out": int(len(y)),
                        "in_sha256": sha256_bytes(x.astype("<f4").tobytes()),
                        "out_sha256": sha256_bytes(y.astype("<f4").tobytes())}
    manifest = {
        "name": rs.NAME,
        "formula": "y[n] = sum_{k=0}^{240} h[k] * x[3n + 120 - k]; x = 0 fora de [0, L); len(y) = ceil(L/3)",
        "design": "h[k] = 2fc*sinc(2fc*(k-120)) * kaiser(241, 8.6)[k], fc = 7600/48000, normalizado para soma 1 em float64",
        "taps": rs.TAPS, "decimation": rs.DECIMATION, "fs_in": rs.FS_IN, "fs_out": rs.FS_OUT,
        "cutoff_hz": rs.CUTOFF_HZ, "kaiser_beta": rs.KAISER_BETA,
        "coefficients": {"file": "coeffs.f32", "dtype": "float32-le", "sha256": coeffs_sha},
        "arithmetic": "acumulação em float64 sobre os coeficientes f32; saída arredondada para float32",
        "golden_tolerance_abs_proposed": 1.0e-6,
        "golden": golden,
    }
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    print(coeffs_sha)


if __name__ == "__main__":
    main()
```

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" scripts/gen_resampler_v1.py
```

Expected: imprime `d230da4dc7dd9ebbdee979dc33a11d3ee61cf2fe29477c768745442865fd7043`. Se imprimir outro valor, **pare** e reporte a versão do numpy (o hash não é ajustado).

- [ ] **Step 5: Rodar os testes até passarem**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" -m pytest tests/test_data_resample.py -v
"$CCTRAIN_PY" -m ruff check src tests scripts
```

Expected: todos PASS.

- [ ] **Step 6: Commit**

```bash
cd "$CCTRAIN_WT"
git add src/cctrain/data/resample.py scripts/gen_resampler_v1.py resampler tests/test_data_resample.py
git commit -m "feat(data): resampler-48k-16k-v1 with published f32 coefficients and golden vectors"
```

---

### Task 5: `gates-v1.yaml` e `budget-v1.yaml` congelados, com carregador

**Files:**
- Create: `configs/eval/gates-v1.yaml`, `configs/eval/gates-v1.sha256`, `configs/m0/budget-v1.yaml`, `configs/m0/budget-v1.sha256`, `src/cctrain/eval/gates.py`
- Test: `tests/test_eval_gates.py`

**Interfaces:**
- Consumes: `cctrain.hashing.sha256_file`, `cctrain.paths.repo_root` (Task 1).
- Produces: `cctrain.eval.gates`: `GatesError(ValueError)`; `Gate` (`id, cls, criteria: Mapping, notes: str`); `GateSet` (`version, gates: dict[str, Gate], m1, probe_rule, calibrated_in_dev: tuple[str, ...], statistics, runtime_path, informative, sha256`); `load_gates(path=GATES_V1, *, verify=True) -> GateSet`; `Budget` (`reserve_gb, vram_b16_reserved_gb_max, loader_rss_gb_max, t3: Mapping, sha256`); `load_budget(path=BUDGET_V1, *, verify=True) -> Budget`; `verify_frozen(path) -> str`; constantes `GATES_V1`, `BUDGET_V1`, `REQUIRED_GATES`.

- [ ] **Step 1: Escrever `gates-v1.yaml` (níveis a priori, sem olhar dado nenhum)**

`configs/eval/gates-v1.yaml`:

```yaml
# Níveis de exigência pré-registrados (spec §7.1-§7.2, §3.1, §5.3; R06 §4). Congelado pelo SHA-256 em
# gates-v1.sha256. Mudar qualquer nível exige gates-v2.yaml com justificativa escrita no relatório.
# O dev só calibra os parâmetros de medida listados em calibrated_in_dev, nunca um nível.
version: 1
spec: "docs/superpowers/specs/2026-10-03-clearcore-train-pdfnet3-enrollment-design.md (rev. 2)"
statistics:
  bootstrap: {unit: speaker, paired: true, resamples: 10000, ci: 0.95}
calibrated_in_dev: [vad_threshold, evaluator_cosine_threshold_at_eer, tsos_config]
runtime_path:
  thresholds_db: {min_db_thresh: -10.0, max_db_erb_thresh: 30.0, max_db_df_thresh: 20.0}
  atten_lim_db: null
  replica_parity: {abs_tol: 1.0e-4, rel_tol: 1.0e-4, frames: 1000}
gates:
  G0:
    class: B
    notes: "contrato/paridade; inclui I1-I3, P1-P2 e o hash do config.ini"
    criteria:
      contract: {sr: 48000, hop: 480, fft: 960, nb_erb: 32, nb_df: 96, df_order: 5, df_lookahead: 2,
                 conv_lookahead: 2, latency_samples: 1440, film_hidden: 256, film_inputs: 4}
      config_ini_sha256: "415eb925d44990d938fb739f514aa3662c1ec0ea836cff044fa1291b82cb4290"
      i1_max_abs_diff: 0.0
      i2_max_abs_diff: 0.0
      i3_max_abs_diff: 0.0
      p1: {atol: 1.0e-5, rtol: 1.0e-6}
      p2: {abs_tol: 1.0e-4, rel_tol: 1.0e-4, frames: 1000, lsnr_range_db: [-10.0, 20.0]}
      film_limits: {gamma_min: 0.001, gamma_max: 100.0, beta_min: -50.0, beta_max: 50.0, finite: true}
  G1:
    class: B
    notes: "modo sem perfil; estágio C bit-exato (I1+I2), estágio F com margens"
    criteria:
      stage_c: {i1_max_abs_diff: 0.0, i2_max_abs_diff: 0.0}
      stage_f: {delta_stoi_min: -0.003, delta_si_sdr_db_min: -0.5, delta_dnsmos_ovrl_min: -0.05,
                delta_dnsmos_sig_min: -0.05, delta_dnsmos_bak_min: -0.07, delta_lsd_above_8k_db_max: 0.5}
  G2:
    class: B
    notes: "falante concorrente; reportado por corpus do alvo"
    criteria: {conditions: [C3, C4], sir_db_max: 5.0, enrollment_s: [6, 10], delta_si_sdr_db_min: 3.0,
               delta_si_sdr_ci_lower_gt: 0.0, delta_dnsmos_p_ovrl_min: 0.15, wer_ratio_to_dfnet3_max: 0.85,
               wer_not_above_noisy: true}
  G3:
    class: B
    notes: "TSOS; também por estágio de gating"
    criteria: {tsos_max_pct: {C0: 1.0, C1_C2: 2.0, C4: 3.0}, tsos_max_over_dfnet3_pp: 0.5}
  G3b:
    class: M
    criteria: {os_run_p99_ms_max: 300.0}
  G4:
    class: B
    notes: "locutor trocado; atten_lim_db desligado; reportado por corpus"
    criteria: {trr_attenuation_db: 15.0, trr_min_different_sex: 0.90, trr_min_same_sex: 0.70,
               mode_separation_db_min: 15.0}
  G5:
    class: B
    notes: "voz preservada; avaliadores independentes do encoder treinado"
    criteria: {spk_sim_delta_ci_lower_min: -0.02, evaluators: [speechbrain-ecapa, wespeaker-resnet34],
               acceptance_rate_not_below_dfnet3: true}
  G5b:
    class: M
    criteria: {spk_sim_gain_min: 0.05}
  G6:
    class: B
    criteria: {enrollment_snr_db: 5.0, enrollment_s: 6.0, reference_enrollment_s: 30.0,
               delta_si_sdri_db_min: -1.5, tar_min: 0.97}
  G7:
    class: M
    criteria: {wer_max_over_dfnet3_pp: 1.0, bound: ci_upper}
  G8:
    class: M
    notes: "só se houver disco; senão 'não executado'"
    criteria: {dnsmos_p_ovrl_min_over_dfnet3: {pn: 0.0, ps: 0.15, psn: 0.15}, sig_drop_vs_noisy_max: 0.5}
  G9:
    class: B
    notes: "medido no Clearcore; o repo de treino só confirma topologia idêntica"
    criteria: {hop_p99_ms_max: 7.0, e2e_p95_ms_max: 80.0, latency_samples: 1440, film_cost_p50_max_pct: 5.0}
  G10:
    class: M
    notes: "executado pelo tract-check"
    criteria: {duration_min: 30, nan_inf_allowed: false, level_drift_db_max: 0.5,
               switch_discontinuity_max_x_local_std: 2.0}
  G11:
    class: B
    criteria: {input_name: audio, sr: 16000, n_min: 32000, n_max: 192000, test_durations_s: [2, 6, 8, 10, 12],
               outputs: [gamma_enc, beta_enc, gamma_df, beta_df, embedding], film_dim: 256, embedding_dim: 192,
               opset: 13, ir_version: 8, edge_inputs: [silence, white_noise, clipping, music],
               level_range: {rms_dbfs_min: -40.0, peak_max: 0.99}}
m1:
  eer_max: {librispeech_test_clean: 0.03, vctk_heldout: 0.06, cv_pt_heldout: 0.08}
  tract_parity: {abs_tol: 1.0e-4, durations_s: [2, 6, 8, 10, 12]}
  functional: {stage: C, same_sex: true, sir_db: 0.0, delta_si_sdr_ci_lower_gt: 0.0}
probe_rule:
  condition: C3
  sir_db: 0.0
  split: dev
  metric: "delta_si_sdr_db (estágio C - DFNet3)"
  ci: 0.95
  go_ci_lower_min: 1.0
informative:
  saturation: {tanh_abs_threshold: 0.95, flag_fraction: 0.05}
  gating_fractions: true
  verification_eer_from_film_vectors: true
```

- [ ] **Step 2: Escrever `budget-v1.yaml` (níveis do go/no-go do M0, antes de medir)**

`configs/m0/budget-v1.yaml`:

```yaml
# Níveis do go/no-go do orçamento no M0 (decisão de planejamento 14), pré-registrados antes das medições.
version: 1
disk:
  reserve_gb: 10.0
vram:
  batch16_reserved_gb_max: 7.0
ram:
  loader_rss_gb_max: 6.0
t3:
  # R02 §4.2: 15-30 épocas, 10-25 min por época de 100 h; spec §5.5: T3 15-35 h.
  epochs: 20
  hours_audio_per_epoch: 100.0
  segment_s: 3.0
  batch: 16
  go_hours_max: 35.0
  caveat_hours_max: 70.0
```

- [ ] **Step 3: Escrever os testes que falham**

`tests/test_eval_gates.py`:

```python
import shutil
from pathlib import Path

import pytest

from cctrain.eval.gates import BUDGET_V1, GATES_V1, REQUIRED_GATES, GatesError, load_budget, load_gates


def test_gates_v1_is_frozen_and_complete():
    gs = load_gates()
    assert gs.version == 1
    assert set(REQUIRED_GATES) <= set(gs.gates)
    assert {g.cls for g in gs.gates.values()} <= {"B", "M"}
    assert gs.gates["G0"].criteria["config_ini_sha256"].startswith("415eb925")


def test_spec_levels_are_present():
    gs = load_gates()
    assert gs.gates["G1"].criteria["stage_f"]["delta_stoi_min"] == -0.003
    assert gs.gates["G2"].criteria["delta_si_sdr_db_min"] == 3.0
    assert gs.gates["G4"].criteria["trr_min_different_sex"] == 0.90
    assert gs.gates["G4"].criteria["trr_min_same_sex"] == 0.70
    assert gs.gates["G6"].criteria["tar_min"] == 0.97
    assert gs.m1["eer_max"] == {"librispeech_test_clean": 0.03, "vctk_heldout": 0.06, "cv_pt_heldout": 0.08}
    assert gs.probe_rule["go_ci_lower_min"] == 1.0
    assert gs.runtime_path["thresholds_db"] == {"min_db_thresh": -10.0, "max_db_erb_thresh": 30.0,
                                                "max_db_df_thresh": 20.0}
    assert "vad_threshold" in gs.calibrated_in_dev


def _copy(tmp_path: Path, src: Path) -> Path:
    dst = tmp_path / src.name
    shutil.copy(src, dst)
    shutil.copy(src.with_suffix(".sha256"), dst.with_suffix(".sha256"))
    return dst


def test_any_edit_breaks_the_freeze(tmp_path: Path):
    p = _copy(tmp_path, GATES_V1)
    p.write_text(p.read_text().replace("delta_si_sdr_db_min: 3.0", "delta_si_sdr_db_min: 2.0"))
    with pytest.raises(GatesError, match="congelado"):
        load_gates(p)


def test_missing_gate_is_rejected(tmp_path: Path):
    p = _copy(tmp_path, GATES_V1)
    text = p.read_text()
    start = text.index("  G7:")
    end = text.index("  G8:")
    p.write_text(text[:start] + text[end:])
    with pytest.raises(GatesError, match="G7"):
        load_gates(p, verify=False)


def test_bad_class_is_rejected(tmp_path: Path):
    p = _copy(tmp_path, GATES_V1)
    p.write_text(p.read_text().replace("  G7:\n    class: M", "  G7:\n    class: X", 1))
    with pytest.raises(GatesError, match="classe"):
        load_gates(p, verify=False)


def test_budget_is_frozen():
    b = load_budget()
    assert b.reserve_gb == 10.0
    assert b.vram_b16_reserved_gb_max == 7.0
    assert b.loader_rss_gb_max == 6.0
    assert b.t3["go_hours_max"] == 35.0
    p = BUDGET_V1
    assert p.with_suffix(".sha256").is_file()
```

- [ ] **Step 4: Rodar e ver falhar**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" -m pytest tests/test_eval_gates.py -v 2>&1 | tail -3
```

Expected: FAIL com `ImportError` (`cctrain.eval.gates`).

- [ ] **Step 5: Implementar `gates.py`**

`src/cctrain/eval/gates.py`:

```python
"""Carrega e valida os níveis pré-registrados (gates-v1) e o orçamento do M0, conferindo o congelamento."""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass
from pathlib import Path

import yaml

from cctrain.hashing import sha256_file
from cctrain.paths import repo_root

GATES_V1 = repo_root() / "configs" / "eval" / "gates-v1.yaml"
BUDGET_V1 = repo_root() / "configs" / "m0" / "budget-v1.yaml"
REQUIRED_GATES = tuple(f"G{i}" for i in range(12))
OPTIONAL_GATES = ("G3b", "G5b")
CLASSES = frozenset({"B", "M"})
_PROBE_KEYS = {"condition", "sir_db", "split", "metric", "ci", "go_ci_lower_min"}
_M1_KEYS = {"eer_max", "tract_parity", "functional"}
_SCALARS = (int, float, bool, str, type(None))


class GatesError(ValueError):
    """Arquivo de níveis inválido ou alterado depois de congelado."""


@dataclass(frozen=True)
class Gate:
    id: str
    cls: str
    criteria: Mapping
    notes: str


@dataclass(frozen=True)
class GateSet:
    version: int
    gates: dict[str, Gate]
    m1: Mapping
    probe_rule: Mapping
    calibrated_in_dev: tuple[str, ...]
    statistics: Mapping
    runtime_path: Mapping
    informative: Mapping
    sha256: str


@dataclass(frozen=True)
class Budget:
    reserve_gb: float
    vram_b16_reserved_gb_max: float
    loader_rss_gb_max: float
    t3: Mapping
    sha256: str


def verify_frozen(path: Path) -> str:
    path = Path(path)
    lock = path.with_suffix(".sha256")
    if not lock.is_file():
        raise GatesError(f"{lock} ausente: o arquivo não está congelado")
    expected = lock.read_text().split()[0]
    actual = sha256_file(path)
    if actual != expected:
        raise GatesError(f"{path} foi alterado depois de congelado ({actual} != {expected}); crie uma nova versão")
    return actual


def _check_leaves(obj, where: str) -> None:
    if isinstance(obj, Mapping):
        for k, v in obj.items():
            _check_leaves(v, f"{where}.{k}")
    elif isinstance(obj, list):
        for i, v in enumerate(obj):
            _check_leaves(v, f"{where}[{i}]")
    elif not isinstance(obj, _SCALARS):
        raise GatesError(f"{where}: tipo {type(obj).__name__} não permitido")


def load_gates(path: Path = GATES_V1, *, verify: bool = True) -> GateSet:
    path = Path(path)
    digest = verify_frozen(path) if verify else sha256_file(path)
    data = yaml.safe_load(path.read_text())
    if data.get("version") != 1:
        raise GatesError("version deve ser 1")
    raw_gates = data.get("gates") or {}
    missing = [g for g in REQUIRED_GATES if g not in raw_gates]
    if missing:
        raise GatesError(f"gates ausentes: {missing}")
    extra = set(raw_gates) - set(REQUIRED_GATES) - set(OPTIONAL_GATES)
    if extra:
        raise GatesError(f"gates desconhecidos: {sorted(extra)}")
    gates: dict[str, Gate] = {}
    for gid, g in raw_gates.items():
        if g.get("class") not in CLASSES:
            raise GatesError(f"{gid}: classe {g.get('class')!r} fora de {sorted(CLASSES)}")
        criteria = g.get("criteria")
        if not isinstance(criteria, Mapping) or not criteria:
            raise GatesError(f"{gid}: criteria vazio")
        _check_leaves(criteria, gid)
        gates[gid] = Gate(id=gid, cls=g["class"], criteria=criteria, notes=g.get("notes", ""))
    m1 = data.get("m1") or {}
    probe = data.get("probe_rule") or {}
    if set(m1) != _M1_KEYS:
        raise GatesError(f"m1 deve ter {sorted(_M1_KEYS)}")
    if set(probe) != _PROBE_KEYS:
        raise GatesError(f"probe_rule deve ter {sorted(_PROBE_KEYS)}")
    return GateSet(
        version=1, gates=gates, m1=m1, probe_rule=probe,
        calibrated_in_dev=tuple(data.get("calibrated_in_dev") or ()),
        statistics=data.get("statistics") or {}, runtime_path=data.get("runtime_path") or {},
        informative=data.get("informative") or {}, sha256=digest,
    )


def load_budget(path: Path = BUDGET_V1, *, verify: bool = True) -> Budget:
    path = Path(path)
    digest = verify_frozen(path) if verify else sha256_file(path)
    d = yaml.safe_load(path.read_text())
    if d.get("version") != 1:
        raise GatesError("budget: version deve ser 1")
    return Budget(
        reserve_gb=float(d["disk"]["reserve_gb"]),
        vram_b16_reserved_gb_max=float(d["vram"]["batch16_reserved_gb_max"]),
        loader_rss_gb_max=float(d["ram"]["loader_rss_gb_max"]),
        t3=d["t3"], sha256=digest,
    )
```

- [ ] **Step 6: Congelar (gravar os SHA-256) e rodar os testes**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
sha256sum configs/eval/gates-v1.yaml | sed 's#configs/eval/##' > configs/eval/gates-v1.sha256
sha256sum configs/m0/budget-v1.yaml | sed 's#configs/m0/##' > configs/m0/budget-v1.sha256
"$CCTRAIN_PY" -m pytest tests/test_eval_gates.py -v
"$CCTRAIN_PY" -m ruff check src tests
```

Expected: todos PASS.

- [ ] **Step 7: Commit**

```bash
cd "$CCTRAIN_WT"
git add configs/eval configs/m0 src/cctrain/eval/gates.py tests/test_eval_gates.py
git commit -m "feat(eval): freeze gates-v1 and M0 budget levels with a validating loader"
```

---
### Task 6: `add_film` sobre o asset aprovado e empacotamento reprodutível

**Files:**
- Create: `src/cctrain/export/pack.py`, `src/cctrain/export/film.py`
- Test: `tests/test_export_pack.py`, `tests/test_export_film.py` (marcador `clearcore`)

**Interfaces:**
- Consumes: `cctrain.hashing`, `cctrain.disk.require_free`, `cctrain.paths` (Task 1).
- Produces:
  - `cctrain.export.pack`: `PackResult(path: Path, sha256: str, size: int)`; `DFNET_MEMBERS = ("enc.onnx", "erb_dec.onnx", "df_dec.onnx", "config.ini")`; `pack_asset(out_path: Path, members: Sequence[tuple[str, Path]]) -> PackResult`; `read_members(archive: Path) -> dict[str, bytes]` (aceita o prefixo `tmp/export/`, recusa diretórios, links, duplicatas e bit executável).
  - `cctrain.export.film`: `SITES: dict[str, str]`; `HIDDEN = 256`; `CONFIG_INI_SHA256`; `APPROVED_ASSET_SHA256`; `FilmGraphError(RuntimeError)`; `add_film(model: onnx.ModelProto, site: str) -> onnx.ModelProto`; `verify_film_graph(original: bytes, filmed: bytes, site: str) -> None`; `build_film_identity_members(approved_asset: Path, out_dir: Path) -> dict[str, Path]`; `build_film_identity_asset(approved_asset: Path, out_dir: Path, name: str = "film-identity-m0.tar.gz") -> PackResult`.

- [ ] **Step 1: Escrever os testes que falham**

`tests/test_export_pack.py`:

```python
import gzip
import io
import tarfile
from pathlib import Path

import pytest

from cctrain.export.pack import pack_asset, read_members


def _files(tmp_path: Path) -> list[tuple[str, Path]]:
    out = []
    for name, data in (("enc.onnx", b"E" * 10), ("erb_dec.onnx", b"R"), ("df_dec.onnx", b"D" * 3),
                       ("config.ini", b"[df]\n")):
        p = tmp_path / ("src-" + name)
        p.write_bytes(data)
        out.append((name, p))
    return out


def test_pack_is_reproducible_and_path_independent(tmp_path: Path):
    files = _files(tmp_path)
    a = pack_asset(tmp_path / "a.tar.gz", files)
    (tmp_path / "sub").mkdir()
    b = pack_asset(tmp_path / "sub" / "other-name.tar.gz", files)
    assert a.sha256 == b.sha256
    assert a.size == b.size


def test_pack_layout_follows_spec(tmp_path: Path):
    res = pack_asset(tmp_path / "x.tar.gz", _files(tmp_path))
    raw = res.path.read_bytes()
    assert raw[:2] == b"\x1f\x8b"
    assert raw[3] & 0x08 == 0, "gzip sem FNAME"
    assert raw[4:8] == b"\x00\x00\x00\x00", "gzip mtime=0"
    with tarfile.open(fileobj=io.BytesIO(gzip.decompress(raw))) as tar:
        members = tar.getmembers()
    assert [m.name for m in members] == ["enc.onnx", "erb_dec.onnx", "df_dec.onnx", "config.ini"]
    for m in members:
        assert m.isreg() and m.mode == 0o644 and m.mtime == 0
        assert (m.uid, m.gid, m.uname, m.gname) == (0, 0, "", "")


def test_pack_refuses_nested_or_duplicate_names(tmp_path: Path):
    files = _files(tmp_path)
    with pytest.raises(ValueError):
        pack_asset(tmp_path / "x.tar.gz", [("tmp/export/enc.onnx", files[0][1])])
    with pytest.raises(ValueError):
        pack_asset(tmp_path / "x.tar.gz", [files[0], files[0]])


def test_read_members_roundtrip_and_prefix(tmp_path: Path):
    res = pack_asset(tmp_path / "x.tar.gz", _files(tmp_path))
    assert read_members(res.path)["enc.onnx"] == b"E" * 10
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w") as tar:
        info = tarfile.TarInfo("tmp/export/config.ini")
        info.size, info.mode = 3, 0o755
        tar.addfile(info, io.BytesIO(b"abc"))
    bad = tmp_path / "exec.tar.gz"
    bad.write_bytes(gzip.compress(buf.getvalue()))
    with pytest.raises(ValueError, match="executável"):
        read_members(bad)
```

`tests/test_export_film.py`:

```python
import gzip
import io
import json
import tarfile

import numpy as np
import onnx
import onnxruntime as ort
import pytest

from cctrain.export.film import (
    CONFIG_INI_SHA256,
    HIDDEN,
    SITES,
    build_film_identity_asset,
    build_film_identity_members,
    verify_film_graph,
)
from cctrain.export.pack import read_members
from cctrain.hashing import sha256_bytes
from cctrain.paths import clearcore_root

pytestmark = pytest.mark.clearcore


def _approved():
    return clearcore_root() / "vendor/approved/df-compatible-release-asset-v1.bin"


def _feeds(name: str, steps: int, seed: int) -> dict:
    r = np.random.RandomState(seed)
    if name == "enc":
        return {"feat_erb": r.randn(1, 1, steps, 32).astype("f4"), "feat_spec": r.randn(1, 2, steps, 96).astype("f4")}
    return {"emb": r.randn(1, steps, 512).astype("f4"), "c0": r.randn(1, 64, steps, 96).astype("f4")}


def _run(model_bytes: bytes, feeds: dict):
    return ort.InferenceSession(model_bytes, providers=["CPUExecutionProvider"]).run(None, feeds)


def test_film_graphs_keep_initializers_and_add_only_mul_add(tmp_path):
    out = build_film_identity_members(_approved(), tmp_path)
    original = read_members(_approved())
    for name, site in SITES.items():
        verify_film_graph(original[f"{name}.onnx"], out[f"{name}.onnx"].read_bytes(), site)
        m = onnx.load(str(out[f"{name}.onnx"]))
        assert [i.name for i in m.graph.input][-2:] == ["gamma", "beta"]
        assert len(m.graph.input) == 4
    assert out["erb_dec.onnx"].read_bytes() == original["erb_dec.onnx"]
    assert sha256_bytes(out["config.ini"].read_bytes()) == CONFIG_INI_SHA256


def test_identity_is_bit_exact_and_non_identity_changes_output(tmp_path):
    out = build_film_identity_members(_approved(), tmp_path)
    original = read_members(_approved())
    steps = 8
    for name in SITES:
        orig, film = original[f"{name}.onnx"], out[f"{name}.onnx"].read_bytes()
        for seed in range(5):
            f = _feeds(name, steps, seed)
            base = _run(orig, f)
            ident = _run(film, {**f, "gamma": np.ones((1, steps, HIDDEN), "f4"),
                                "beta": np.zeros((1, steps, HIDDEN), "f4")})
            assert all(np.array_equal(a, b) for a, b in zip(base, ident)), (name, seed)
        f = _feeds(name, steps, 99)
        moved = _run(film, {**f, "gamma": np.full((1, steps, HIDDEN), 1.5, "f4"),
                            "beta": np.full((1, steps, HIDDEN), 0.1, "f4")})
        assert not all(np.array_equal(a, b) for a, b in zip(_run(orig, f), moved)), name


def test_asset_pack_is_reproducible_and_compared_with_clearcore_fixture(tmp_path, m0_runs):
    a = build_film_identity_asset(_approved(), tmp_path / "a")
    b = build_film_identity_asset(_approved(), tmp_path / "b")
    assert a.sha256 == b.sha256
    with tarfile.open(fileobj=io.BytesIO(gzip.decompress(a.path.read_bytes()))) as tar:
        assert [m.name for m in tar.getmembers()] == ["enc.onnx", "erb_dec.onnx", "df_dec.onnx", "config.ini"]
    ours = read_members(a.path)
    fixture = read_members(clearcore_root() / "fixtures/film/film-identity-asset.tar.gz")
    same = {k: ours[k] == fixture[k] for k in ours}
    (m0_runs / "film_asset.json").write_text(json.dumps(
        {"sha256": a.sha256, "size": a.size, "members_equal_to_clearcore_fixture": same}, indent=2) + "\n")
```

- [ ] **Step 2: Rodar e ver falhar**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" -m pytest tests/test_export_pack.py tests/test_export_film.py -v 2>&1 | tail -3
```

Expected: FAIL com `ImportError`.

- [ ] **Step 3: Implementar `pack.py`**

`src/cctrain/export/pack.py`:

```python
"""Empacotamento reprodutível dos assets (spec §2.3) e leitura validada (regras de model_registry.rs)."""

from __future__ import annotations

import gzip
import io
import os
import tarfile
from collections.abc import Sequence
from dataclasses import dataclass
from pathlib import Path, PurePosixPath

from cctrain.disk import require_free
from cctrain.hashing import sha256_file

DFNET_MEMBERS = ("enc.onnx", "erb_dec.onnx", "df_dec.onnx", "config.ini")
_ALLOWED_PREFIX = ("tmp", "export")


@dataclass(frozen=True)
class PackResult:
    path: Path
    sha256: str
    size: int


def pack_asset(out_path: Path, members: Sequence[tuple[str, Path]]) -> PackResult:
    out_path = Path(out_path)
    names = [n for n, _ in members]
    if len(set(names)) != len(names):
        raise ValueError(f"membros duplicados: {names}")
    for n in names:
        if "/" in n or n in ("", ".", ".."):
            raise ValueError(f"membro {n!r}: o pacote tem membros planos")
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w", format=tarfile.USTAR_FORMAT) as tar:
        for name, src in members:
            data = Path(src).read_bytes()
            info = tarfile.TarInfo(name)
            info.size, info.mtime, info.mode = len(data), 0, 0o644
            info.uid = info.gid = 0
            info.uname = info.gname = ""
            info.type = tarfile.REGTYPE
            tar.addfile(info, io.BytesIO(data))
    raw = buf.getvalue()
    out_path.parent.mkdir(parents=True, exist_ok=True)
    require_free(out_path.parent, 2 * len(raw))
    tmp = out_path.with_name(out_path.name + ".tmp")
    with open(tmp, "wb") as f, gzip.GzipFile(filename="", mode="wb", fileobj=f, mtime=0) as gz:
        gz.write(raw)
    os.replace(tmp, out_path)
    return PackResult(out_path, sha256_file(out_path), out_path.stat().st_size)


def read_members(archive: Path) -> dict[str, bytes]:
    out: dict[str, bytes] = {}
    with tarfile.open(archive, mode="r:gz") as tar:
        for m in tar.getmembers():
            parts = PurePosixPath(m.name).parts
            if not m.isreg():
                raise ValueError(f"{m.name}: só arquivos regulares")
            if m.mode & 0o111:
                raise ValueError(f"{m.name}: bit executável")
            if len(parts) not in (1, 3) or (len(parts) == 3 and parts[:2] != _ALLOWED_PREFIX):
                raise ValueError(f"{m.name}: caminho fora do formato (plano ou tmp/export/)")
            base = parts[-1]
            if base in out:
                raise ValueError(f"{base}: duplicado")
            f = tar.extractfile(m)
            out[base] = f.read() if f is not None else b""
    return out
```

- [ ] **Step 4: Implementar `film.py`**

`src/cctrain/export/film.py`:

```python
"""FiLM nos grafos do DFNet3 por edição de grafo (spec §6.1, estágio C).

`add_film` copiado de tools/accelerators/gen_film_onnx.py do Clearcore (MIT OR Apache-2.0), trocando
SystemExit por FilmGraphError.
"""

from __future__ import annotations

from pathlib import Path

import numpy as np
import onnx
from onnx import TensorProto, helper, numpy_helper

from cctrain.export.pack import DFNET_MEMBERS, PackResult, pack_asset, read_members
from cctrain.hashing import sha256_bytes, sha256_file

SITES = {
    "enc": "/emb_gru/linear_in/1/Relu_output_0",
    "df_dec": "/df_gru/linear_in/linear_in.1/Relu_output_0",
}
HIDDEN = 256
CONFIG_INI_SHA256 = "415eb925d44990d938fb739f514aa3662c1ec0ea836cff044fa1291b82cb4290"
APPROVED_ASSET_SHA256 = "c94d91f70911001c946e0fabb4aa9adc37045f45a03b56008cb0c8244cb63616"
_FILM_NODES = ("film_mul", "film_add")


class FilmGraphError(RuntimeError):
    """Grafo fora do esperado para a inserção do FiLM."""


def add_film(model: onnx.ModelProto, site: str) -> onnx.ModelProto:
    g = model.graph
    if any(i.name in ("gamma", "beta") for i in g.input):
        raise FilmGraphError("modelo já tem gamma/beta")
    producers = [i for i, nd in enumerate(g.node) if site in nd.output]
    if len(producers) != 1:
        raise FilmGraphError(f"site {site}: {len(producers)} produtores (esperado 1)")
    for name in ("gamma", "beta"):
        g.input.append(helper.make_tensor_value_info(name, TensorProto.FLOAT, [1, "S", HIDDEN]))
    scaled, filmed = site + "_gamma", site + "_film"
    for nd in g.node:
        for k, x in enumerate(nd.input):
            if x == site:
                nd.input[k] = filmed
    at = producers[0] + 1
    g.node.insert(at, helper.make_node("Mul", [site, "gamma"], [scaled], name="film_mul"))
    g.node.insert(at + 1, helper.make_node("Add", [scaled, "beta"], [filmed], name="film_add"))
    onnx.checker.check_model(model)
    return model


def _initializers(model: onnx.ModelProto) -> dict[str, tuple]:
    out = {}
    for t in model.graph.initializer:
        a = numpy_helper.to_array(t)
        out[t.name] = (a.dtype.str, a.shape, np.ascontiguousarray(a).tobytes())
    return out


def verify_film_graph(original: bytes, filmed: bytes, site: str) -> None:
    a, b = onnx.load_from_string(original), onnx.load_from_string(filmed)
    if _initializers(a) != _initializers(b):
        raise FilmGraphError("initializers diferem do asset aprovado")
    new = [n for n in b.graph.node if n.name in _FILM_NODES]
    if [(n.name, n.op_type) for n in new] != [("film_mul", "Mul"), ("film_add", "Add")]:
        raise FilmGraphError(f"nós novos inesperados: {[(n.name, n.op_type) for n in new]}")
    rest = [n for n in b.graph.node if n.name not in _FILM_NODES]
    if len(rest) != len(a.graph.node):
        raise FilmGraphError("número de nós originais mudou")
    for x, y in zip(a.graph.node, rest):
        inputs_y = [site if i == site + "_film" else i for i in y.input]
        if (x.op_type, list(x.output), list(x.input)) != (y.op_type, list(y.output), inputs_y):
            raise FilmGraphError(f"nó {x.name} alterado")


def build_film_identity_members(approved_asset: Path, out_dir: Path) -> dict[str, Path]:
    if sha256_file(approved_asset) != APPROVED_ASSET_SHA256:
        raise FilmGraphError(f"{approved_asset}: não é o asset aprovado")
    members = read_members(approved_asset)
    if sorted(members) != sorted(DFNET_MEMBERS):
        raise FilmGraphError(f"membros inesperados: {sorted(members)}")
    if sha256_bytes(members["config.ini"]) != CONFIG_INI_SHA256:
        raise FilmGraphError("config.ini difere do v0.5.6")
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    paths: dict[str, Path] = {}
    for name in DFNET_MEMBERS:
        data = members[name]
        stem = name.removesuffix(".onnx")
        if stem in SITES:
            filmed = add_film(onnx.load_from_string(data), SITES[stem]).SerializeToString()
            verify_film_graph(data, filmed, SITES[stem])
            data = filmed
        paths[name] = out_dir / name
        paths[name].write_bytes(data)
    return paths


def build_film_identity_asset(approved_asset: Path, out_dir: Path,
                              name: str = "film-identity-m0.tar.gz") -> PackResult:
    members = build_film_identity_members(approved_asset, Path(out_dir) / "members")
    return pack_asset(Path(out_dir) / name, [(m, members[m]) for m in DFNET_MEMBERS])
```

- [ ] **Step 5: Rodar os testes até passarem**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" -m pytest tests/test_export_pack.py tests/test_export_film.py -v
cat runs/m0/film_asset.json
"$CCTRAIN_PY" -m ruff check src tests
```

Expected: todos PASS; `film_asset.json` registra se os `.onnx` são iguais byte a byte aos do fixture do Clearcore (informativo: a versão do `onnx` pode mudar a serialização; não é critério).

- [ ] **Step 6: Commit**

```bash
cd "$CCTRAIN_WT"
git add src/cctrain/export/pack.py src/cctrain/export/film.py tests/test_export_pack.py tests/test_export_film.py
git commit -m "feat(export): add_film on the approved DFNet3 asset and reproducible flat tar packing"
```

---

### Task 7: Gerador FiLM e configuração com validador

**Files:**
- Create: `src/cctrain/models/film.py`, `configs/pdfnet3/film-v1.yaml`
- Test: `tests/test_models_film.py`

**Interfaces:**
- Consumes: `cctrain.paths.repo_root` (Task 1).
- Produces: `cctrain.models.film`: `FILM_CONFIG_V1`; `FilmConfigError(ValueError)`; `FilmConfig(log_gamma_bound: float, beta_bound: float, hidden_dim: int = 256, embedding_dim: int = 192, mid_dim: int = 256)` com `validate()`; `load_film_config(path=FILM_CONFIG_V1) -> FilmConfig`; `beta_bound_rule(p99: float) -> float`; `FilmVectors(gamma_enc, beta_enc, gamma_df, beta_df)` (tensores `[B, 1, 256]`); `FilmGenerator(cfg: FilmConfig)` com `forward(embedding: Tensor[B,192], mask: Tensor[B,1]) -> FilmVectors`, atributos `hidden: nn.Linear(192→256)` e `out: nn.Linear(256→1024)`.

- [ ] **Step 1: Escrever os testes que falham**

`tests/test_models_film.py`:

```python
import math

import pytest
import torch

from cctrain.models.film import (
    FilmConfig,
    FilmConfigError,
    FilmGenerator,
    beta_bound_rule,
    load_film_config,
)

LN10 = math.log(10.0)


def test_rule_for_beta_bound():
    assert beta_bound_rule(0.5) == 4.0
    assert beta_bound_rule(3.0) == 6.0
    assert beta_bound_rule(30.0) == 50.0


@pytest.mark.parametrize("gamma_bound,beta", [(0.0, 4.0), (math.log(100.0) + 1e-6, 4.0), (LN10, 3.9),
                                              (LN10, 50.1)])
def test_validator_refuses_out_of_range(gamma_bound, beta):
    with pytest.raises(FilmConfigError):
        FilmConfig(log_gamma_bound=gamma_bound, beta_bound=beta).validate()


def test_config_v1_has_gamma_ln10_and_beta_pending_until_m0():
    with pytest.raises(FilmConfigError, match="B"):
        load_film_config()


def test_zero_init_gives_exact_identity_for_any_embedding():
    gen = FilmGenerator(FilmConfig(LN10, 4.0))
    v = gen(torch.randn(3, 192), torch.ones(3, 1))
    for t in (v.gamma_enc, v.gamma_df):
        assert t.shape == (3, 1, 256) and torch.equal(t, torch.ones_like(t))
    for t in (v.beta_enc, v.beta_df):
        assert torch.equal(t, torch.zeros_like(t))


def test_mask_zero_gives_identity_even_with_trained_weights():
    gen = FilmGenerator(FilmConfig(LN10, 4.0))
    with torch.no_grad():
        gen.out.weight.normal_(std=1.0)
        gen.out.bias.normal_(std=1.0)
    x = torch.randn(4, 1, 256).abs()
    v = gen(torch.randn(4, 192), torch.zeros(4, 1))
    assert torch.equal(x * v.gamma_enc + v.beta_enc, x)
    assert torch.equal(x * v.gamma_df + v.beta_df, x)


def test_outputs_respect_runtime_limits():
    gen = FilmGenerator(FilmConfig(LN10, 50.0))
    with torch.no_grad():
        gen.out.weight.normal_(std=100.0)
    v = gen(torch.randn(16, 192), torch.ones(16, 1))
    for g in (v.gamma_enc, v.gamma_df):
        assert g.min() >= 0.1 - 1e-6 and g.max() <= 10.0 + 1e-5
    for b in (v.beta_enc, v.beta_df):
        assert b.abs().max() <= 50.0
    assert sum(p.numel() for p in gen.parameters()) == 192 * 256 + 256 + 256 * 1024 + 1024
```

- [ ] **Step 2: Rodar e ver falhar**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" -m pytest tests/test_models_film.py -v 2>&1 | tail -3
```

Expected: FAIL com `ImportError`.

- [ ] **Step 3: Escrever a config e implementar `film.py`**

`configs/pdfnet3/film-v1.yaml`:

```yaml
# Gerador FiLM (spec §3.2). Γ = ln 10; B é fixado no M0 (Task 17) pela regra min(50, max(4, 2·p99)) e
# congelado antes do T2. Enquanto beta_bound for null, load_film_config recusa a config.
version: 1
log_gamma_bound: 2.302585092994046
beta_bound: null
beta_rule:
  sites: [enc.emb_gru.linear_in, df_dec.df_gru.linear_in]
  percentile: 99
  factor: 2.0
  min: 4.0
  max: 50.0
embedding_dim: 192
mid_dim: 256
hidden_dim: 256
p_drop: {stage_c: 0.0, stage_f: 0.2}
```

`src/cctrain/models/film.py`:

```python
"""Gerador FiLM (spec §3.2): identidade exata por construção, faixa limitada e configurável."""

from __future__ import annotations

import math
from dataclasses import dataclass
from pathlib import Path

import torch
import yaml
from torch import Tensor, nn

from cctrain.paths import repo_root

FILM_CONFIG_V1 = repo_root() / "configs" / "pdfnet3" / "film-v1.yaml"
_GAMMA_BOUND_MAX = math.log(100.0)  # [e^-Γ, e^Γ] ⊂ [0,001; 100] exige Γ ≤ ln 100
_BETA_MIN, _BETA_MAX = 4.0, 50.0


class FilmConfigError(ValueError):
    """Configuração do FiLM fora dos limites do runtime."""


@dataclass(frozen=True)
class FilmConfig:
    log_gamma_bound: float
    beta_bound: float | None
    hidden_dim: int = 256
    embedding_dim: int = 192
    mid_dim: int = 256

    def validate(self) -> FilmConfig:
        if not 0.0 < self.log_gamma_bound <= _GAMMA_BOUND_MAX:
            raise FilmConfigError(f"Γ={self.log_gamma_bound} fora de (0, ln 100]")
        if self.beta_bound is None:
            raise FilmConfigError("B ainda não fixado (regra do M0, Task 17)")
        if not _BETA_MIN <= self.beta_bound <= _BETA_MAX:
            raise FilmConfigError(f"B={self.beta_bound} fora de [{_BETA_MIN}, {_BETA_MAX}]")
        if self.hidden_dim != 256:
            raise FilmConfigError("film_hidden do runtime é 256")
        return self


def beta_bound_rule(p99: float) -> float:
    return min(_BETA_MAX, max(_BETA_MIN, 2.0 * float(p99)))


def load_film_config(path: Path = FILM_CONFIG_V1) -> FilmConfig:
    d = yaml.safe_load(Path(path).read_text())
    cfg = FilmConfig(
        log_gamma_bound=float(d["log_gamma_bound"]),
        beta_bound=None if d["beta_bound"] is None else float(d["beta_bound"]),
        hidden_dim=int(d["hidden_dim"]), embedding_dim=int(d["embedding_dim"]), mid_dim=int(d["mid_dim"]),
    )
    return cfg.validate()


@dataclass(frozen=True)
class FilmVectors:
    gamma_enc: Tensor
    beta_enc: Tensor
    gamma_df: Tensor
    beta_df: Tensor


class FilmGenerator(nn.Module):
    def __init__(self, cfg: FilmConfig):
        super().__init__()
        self.cfg = cfg.validate()
        self.hidden = nn.Linear(cfg.embedding_dim, cfg.mid_dim)
        self.out = nn.Linear(cfg.mid_dim, 4 * cfg.hidden_dim)
        nn.init.zeros_(self.out.weight)
        nn.init.zeros_(self.out.bias)

    def forward(self, embedding: Tensor, mask: Tensor) -> FilmVectors:
        d = self.out(torch.relu(self.hidden(embedding))) * mask
        dge, dbe, dgd, dbd = d.chunk(4, dim=-1)
        g_bound, b_bound = self.cfg.log_gamma_bound, self.cfg.beta_bound

        def gamma(x: Tensor) -> Tensor:
            return torch.exp(g_bound * torch.tanh(x)).unsqueeze(1)

        def beta(x: Tensor) -> Tensor:
            return (b_bound * torch.tanh(x)).unsqueeze(1)

        return FilmVectors(gamma(dge), beta(dbe), gamma(dgd), beta(dbd))
```

- [ ] **Step 4: Rodar os testes até passarem**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" -m pytest tests/test_models_film.py -v
"$CCTRAIN_PY" -m ruff check src tests
```

Expected: todos PASS.

- [ ] **Step 5: Commit**

```bash
cd "$CCTRAIN_WT"
git add src/cctrain/models/film.py configs/pdfnet3/film-v1.yaml tests/test_models_film.py
git commit -m "feat(models): FiLM generator with exact identity, bounded range and config validator"
```

---
### Task 8: Patch FiLM no DFNet3 vendorizado, `PDFNet3` e I1

**Files:**
- Modify: `third_party/DeepFilterNet/DeepFilterNet/df/modules.py` (`SqueezedGRU_S.forward`), `third_party/DeepFilterNet/DeepFilterNet/df/deepfilternet3.py` (`Encoder.forward`, `DfDecoder.forward`, `DfNet.forward`), `third_party/DeepFilterNet/PATCHES.md`
- Create: `src/cctrain/models/dfnet3_upstream.py`, `src/cctrain/models/pdfnet3.py`, `src/cctrain/models/state_hash.py`
- Test: `tests/test_models_pdfnet3.py`, `tests/test_models_state_hash.py`

**Interfaces:**
- Consumes: `cctrain.dfvendor.ensure_df_importable`, `ensure_dfnet3_checkpoint` (Task 1); `cctrain.models.film.FilmGenerator`, `FilmConfig`, `FilmVectors` (Task 7).
- Produces:
  - `cctrain.models.dfnet3_upstream`: `load_upstream_dfnet3(model_dir: Path | None = None, device: str = "cpu") -> tuple[nn.Module, DF]` (modelo em `eval()`); `df_features_batch(audios: Sequence[np.ndarray], df_state) -> tuple[Tensor, Tensor, Tensor]` (`spec [B,1,T,481,2]`, `feat_erb [B,1,T,32]`, `feat_spec [B,1,T,96,2]`).
  - `cctrain.models.pdfnet3`: `PDFNet3(base: nn.Module, generator: FilmGenerator)` com `forward(spec, feat_erb, feat_spec, embedding, mask) -> (spec_e, m, lsnr, df_coefs)` e `film_sites() -> dict[str, nn.Module]` (`"enc"` → `base.enc.emb_gru.linear_in`, `"df_dec"` → `base.df_dec.df_gru.linear_in`).
  - DFNet3 vendorizado: `DfNet.forward(spec, feat_erb, feat_spec, film_enc=None, film_df=None)`, com `film_*` = `(gamma [B,1,256], beta [B,1,256])` ou `None` (caminho original).
  - `cctrain.models.state_hash`: `state_dict_sha256(module: nn.Module) -> str`.
  - `runs/m0/i1.json`.

- [ ] **Step 1: Localizar os trechos do upstream**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
D=third_party/DeepFilterNet/DeepFilterNet/df
grep -n "class SqueezedGRU_S" -A45 $D/modules.py | grep -n "def forward\|linear_in\|self.gru(" 
grep -n "def forward\|emb_gru(\|df_gru(\|self.enc(\|self.df_dec(" $D/deepfilternet3.py
```

Expected: `SqueezedGRU_S.forward(self, input, h=None)` com `x = self.linear_in(input)` seguido de `x, h = self.gru(x, h)`; em `deepfilternet3.py`, a chamada `self.emb_gru(emb)` dentro de `Encoder.forward`, `self.df_gru(emb)` dentro de `DfDecoder.forward`, e `self.enc(feat_erb, feat_spec)` / `self.df_dec(emb, c0)` dentro de `DfNet.forward` (R02 §2.2). Se a forma divergir disso, pare e reporte antes de editar.

- [ ] **Step 2: Escrever os testes que falham**

`tests/test_models_state_hash.py`:

```python
import torch
from torch import nn

from cctrain.models.state_hash import state_dict_sha256


def test_hash_is_stable_and_detects_any_change():
    torch.manual_seed(0)
    m = nn.Sequential(nn.Linear(4, 3), nn.BatchNorm1d(3))
    h1 = state_dict_sha256(m)
    m.eval()
    with torch.no_grad():
        m(torch.randn(5, 4))
    assert state_dict_sha256(m) == h1
    with torch.no_grad():
        m[0].weight[0, 0] += 1e-7
    assert state_dict_sha256(m) != h1
```

`tests/test_models_pdfnet3.py`:

```python
import json
import math

import numpy as np
import pytest
import torch

from cctrain.models.dfnet3_upstream import df_features_batch, load_upstream_dfnet3
from cctrain.models.film import FilmConfig, FilmGenerator
from cctrain.models.pdfnet3 import PDFNet3
from cctrain.models.state_hash import state_dict_sha256


@pytest.fixture(scope="module")
def upstream():
    torch.use_deterministic_algorithms(True)
    torch.set_num_threads(1)
    model, df_state = load_upstream_dfnet3(device="cpu")
    yield model, df_state
    torch.use_deterministic_algorithms(False)


def _audio(n: int = 2, seconds: float = 1.0, seed: int = 0) -> list[np.ndarray]:
    rng = np.random.default_rng(seed)
    t = np.arange(int(48_000 * seconds)) / 48_000.0
    return [(0.1 * np.sin(2 * np.pi * (150 + 50 * i) * t) + 0.02 * rng.standard_normal(t.size)).astype(np.float32)
            for i in range(n)]


def _generator(scale: float) -> FilmGenerator:
    gen = FilmGenerator(FilmConfig(math.log(10.0), 4.0))
    if scale:
        g = torch.Generator().manual_seed(1)
        with torch.no_grad():
            gen.out.weight.copy_(torch.randn(gen.out.weight.shape, generator=g) * scale)
            gen.out.bias.copy_(torch.randn(gen.out.bias.shape, generator=g) * scale)
    return gen


def test_i1_mask_zero_is_bit_exact_with_upstream(upstream, m0_runs):
    base, df_state = upstream
    spec, erb, specf = df_features_batch(_audio(), df_state)
    emb = torch.randn(2, 192, generator=torch.Generator().manual_seed(2))
    before = state_dict_sha256(base)
    with torch.no_grad():
        ref = base(spec.clone(), erb.clone(), specf.clone())
        out = PDFNet3(base, _generator(0.5)).eval()(spec.clone(), erb.clone(), specf.clone(), emb, torch.zeros(2, 1))
    diffs = [float((r - o).abs().max()) if r.numel() else 0.0 for r, o in zip(ref, out)]
    (m0_runs / "i1.json").write_text(json.dumps(
        {"test": "I1", "device": "cpu", "deterministic": True, "max_abs_diff_per_output": diffs,
         "pass": all(torch.equal(r, o) for r, o in zip(ref, out))}, indent=2) + "\n")
    assert all(torch.equal(r, o) for r, o in zip(ref, out)), diffs
    assert state_dict_sha256(base) == before


def test_zero_initialized_generator_is_identity_with_mask_one(upstream):
    base, df_state = upstream
    spec, erb, specf = df_features_batch(_audio(seed=3), df_state)
    with torch.no_grad():
        ref = base(spec.clone(), erb.clone(), specf.clone())
        out = PDFNet3(base, _generator(0.0)).eval()(spec.clone(), erb.clone(), specf.clone(),
                                                     torch.randn(2, 192), torch.ones(2, 1))
    assert all(torch.equal(r, o) for r, o in zip(ref, out))


def test_non_identity_film_changes_enhancement(upstream):
    base, df_state = upstream
    spec, erb, specf = df_features_batch(_audio(seed=4), df_state)
    with torch.no_grad():
        ref = base(spec.clone(), erb.clone(), specf.clone())
        out = PDFNet3(base, _generator(0.5)).eval()(spec.clone(), erb.clone(), specf.clone(),
                                                     torch.randn(2, 192), torch.ones(2, 1))
    assert not torch.equal(ref[0], out[0])
    assert torch.isfinite(out[0]).all()


def test_film_sites_are_the_relu_outputs(upstream):
    base, _ = upstream
    sites = PDFNet3(base, _generator(0.0)).film_sites()
    assert set(sites) == {"enc", "df_dec"}
    for module in sites.values():
        assert isinstance(list(module.children())[-1], torch.nn.ReLU)
```

- [ ] **Step 3: Rodar e ver falhar**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" -m pytest tests/test_models_pdfnet3.py tests/test_models_state_hash.py -v 2>&1 | tail -3
```

Expected: FAIL com `ImportError` (`cctrain.models.dfnet3_upstream`).

- [ ] **Step 4: Aplicar o patch FiLM (mínimo, marcado)**

Em `df/modules.py`, `SqueezedGRU_S.forward`: acrescente o parâmetro `film=None` ao fim da assinatura e, logo depois de `x = self.linear_in(input)`, insira:

```python
        # CCTRAIN-PATCH(film): x' = x * gamma + beta na saída do ReLU de linear_in (spec §3.3).
        if film is not None:
            x = x * film[0] + film[1]
```

Em `df/deepfilternet3.py`:

- `Encoder.forward`: acrescente `film=None` ao fim da assinatura e troque `self.emb_gru(emb)` por `self.emb_gru(emb, film=film)  # CCTRAIN-PATCH(film)`.
- `DfDecoder.forward`: acrescente `film=None` ao fim da assinatura e troque `self.df_gru(emb)` por `self.df_gru(emb, film=film)  # CCTRAIN-PATCH(film)`.
- `DfNet.forward`: acrescente `film_enc=None, film_df=None` ao fim da assinatura; troque `self.enc(feat_erb, feat_spec)` por `self.enc(feat_erb, feat_spec, film=film_enc)  # CCTRAIN-PATCH(film)` e `self.df_dec(emb, c0)` por `self.df_dec(emb, c0, film=film_df)  # CCTRAIN-PATCH(film)`.

Com `film=None` o código executado é exatamente o original. Acrescente a `third_party/DeepFilterNet/PATCHES.md`:

```markdown
| film | `DeepFilterNet/df/modules.py` (`SqueezedGRU_S.forward`), `DeepFilterNet/df/deepfilternet3.py` (`Encoder.forward`, `DfDecoder.forward`, `DfNet.forward`) | FiLM nos sites do `gen_film_onnx.py` (spec §3.3); `film=None` executa o caminho original |
```

- [ ] **Step 5: Implementar os módulos**

`src/cctrain/models/state_hash.py`:

```python
"""SHA-256 do state_dict (integridade da base congelada no estágio C, spec §5.3)."""

from __future__ import annotations

import hashlib

from torch import nn


def state_dict_sha256(module: nn.Module) -> str:
    h = hashlib.sha256()
    for name, tensor in sorted(module.state_dict().items()):
        t = tensor.detach().cpu().contiguous()
        h.update(name.encode())
        h.update(str(tuple(t.shape)).encode())
        h.update(str(t.dtype).encode())
        h.update(t.numpy().tobytes())
    return h.hexdigest()
```

`src/cctrain/models/dfnet3_upstream.py`:

```python
"""DFNet3 upstream v0.5.6 (pesos do DeepFilterNet3.zip vendorizado) e features do libdf em lote."""

from __future__ import annotations

from collections.abc import Sequence
from pathlib import Path

import numpy as np
import torch
from torch import Tensor, nn

from cctrain.dfvendor import ensure_df_importable, ensure_dfnet3_checkpoint

NB_DF = 96


def load_upstream_dfnet3(model_dir: Path | None = None, device: str = "cpu") -> tuple[nn.Module, object]:
    ensure_df_importable()
    from df.enhance import init_df

    model_dir = model_dir or ensure_dfnet3_checkpoint()
    model, df_state, *_ = init_df(
        str(model_dir), post_filter=False, log_level="ERROR", log_file=None,
        config_allow_defaults=True, epoch="best",
    )
    return model.to(device).eval(), df_state


def df_features_batch(audios: Sequence[np.ndarray], df_state) -> tuple[Tensor, Tensor, Tensor]:
    ensure_df_importable()
    from df.enhance import df_features

    feats = [df_features(torch.from_numpy(np.asarray(a, np.float32))[None], df_state, NB_DF, device="cpu")
             for a in audios]
    return tuple(torch.cat([f[k] for f in feats], dim=0) for k in range(3))
```

`src/cctrain/models/pdfnet3.py`:

```python
"""pDFNet3 = DFNet3 upstream com FiLM nos dois sites + gerador FiLM (spec §3.2-§3.4).

O gerador não entra nos três grafos exportados; ele vai no enrollment.onnx.
"""

from __future__ import annotations

from torch import Tensor, nn

from cctrain.models.film import FilmGenerator


class PDFNet3(nn.Module):
    def __init__(self, base: nn.Module, generator: FilmGenerator):
        super().__init__()
        self.base = base
        self.film = generator

    def forward(self, spec: Tensor, feat_erb: Tensor, feat_spec: Tensor, embedding: Tensor, mask: Tensor):
        v = self.film(embedding, mask)
        return self.base(spec, feat_erb, feat_spec, film_enc=(v.gamma_enc, v.beta_enc),
                         film_df=(v.gamma_df, v.beta_df))

    def film_sites(self) -> dict[str, nn.Module]:
        return {"enc": self.base.enc.emb_gru.linear_in, "df_dec": self.base.df_dec.df_gru.linear_in}
```

- [ ] **Step 6: Rodar os testes até passarem**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" -m pytest tests/test_models_pdfnet3.py tests/test_models_state_hash.py tests/test_dfvendor.py -v
cat runs/m0/i1.json
"$CCTRAIN_PY" -m ruff check src tests
```

Expected: todos PASS; `i1.json` com `"pass": true` e todas as diferenças `0.0`. Se `init_df` recusar algum argumento (`log_file`), confira a assinatura com `"$CCTRAIN_PY" -c "import sys; sys.path.insert(0,'third_party/DeepFilterNet/DeepFilterNet'); import inspect, df.enhance as e; print(inspect.signature(e.init_df))"` e passe só os argumentos existentes. I1 diferente de zero é falha: pare e reporte (não se alarga tolerância).

- [ ] **Step 7: Commit**

```bash
cd "$CCTRAIN_WT"
git add third_party/DeepFilterNet src/cctrain/models/dfnet3_upstream.py src/cctrain/models/pdfnet3.py src/cctrain/models/state_hash.py tests/test_models_pdfnet3.py tests/test_models_state_hash.py
git commit -m "feat(models): FiLM hook in vendored DFNet3 and pDFNet3 wrapper; I1 bit-exact on CPU"
```

---

### Task 9: fbank tract-safe, ECAPA-TDNN e export do enrollment (PyTorch/ORT)

**Files:**
- Create: `src/cctrain/models/fbank.py`, `src/cctrain/models/ecapa.py`, `src/cctrain/export/enrollment.py`
- Test: `tests/test_models_fbank.py`, `tests/test_models_ecapa.py`, `tests/test_export_enrollment.py`

**Interfaces:**
- Consumes: `cctrain.models.film.FilmGenerator`, `FilmConfig` (Task 7).
- Produces:
  - `cctrain.models.fbank`: `mel_filterbank(n_mels=80, n_fft=512, sr=16000, f_min=20.0, f_max=7600.0) -> np.ndarray`; `num_frames(n_samples: int) -> int` (= `1 + (n - 400) // 160`); `TractSafeFbank(nn.Module)` com `forward(audio [B, N]) -> [B, 80, T]` (log-mel com média no tempo subtraída).
  - `cctrain.models.ecapa`: `ECAPATDNN(n_mels=80, channels=512, mfa_channels=1536, embedding_dim=192, attention=128, context_mode="expand")` com `forward(feats [B,80,T]) -> [B,192]`; `context_mode ∈ {"expand", "broadcast"}`.
  - `cctrain.export.enrollment`: `OUTPUT_NAMES = ("gamma_enc", "beta_enc", "gamma_df", "beta_df", "embedding")`; `EnrollmentModel(fbank, encoder, generator)` com `forward(audio [1,N]) -> 5 tensores 1-D`; `build_prototype(seed: int, context_mode: str, beta_bound: float = 4.0, out_std: float = 0.5) -> EnrollmentModel`; `export_enrollment_onnx(model, path: Path) -> Path` (opset 13, `ir_version` 8, eixo dinâmico `N`); `speechlike_16k(n: int, seed: int) -> np.ndarray`; `scale_to_rms_dbfs(x, dbfs) -> np.ndarray`; `scale_to_peak(x, peak) -> np.ndarray`.

- [ ] **Step 1: Escrever os testes que falham**

`tests/test_models_fbank.py`:

```python
import numpy as np
import torch

from cctrain.models.fbank import TractSafeFbank, mel_filterbank, num_frames


def _reference(x: np.ndarray) -> np.ndarray:
    n = num_frames(len(x))
    frames = np.stack([x[i * 160 : i * 160 + 400] for i in range(n)]) * np.hamming(400)
    power = np.abs(np.fft.rfft(frames, n=512, axis=1)) ** 2
    mel = np.log(power @ mel_filterbank().T + 1e-6).T
    return mel - mel.mean(axis=1, keepdims=True)


def test_frame_count():
    assert num_frames(32_000) == 198
    assert num_frames(192_000) == 1198


def test_matches_numpy_reference():
    x = (0.1 * np.random.default_rng(0).standard_normal(16_000)).astype(np.float32)
    got = TractSafeFbank()(torch.from_numpy(x)[None])[0].numpy()
    assert got.shape == (80, num_frames(16_000))
    assert np.max(np.abs(got - _reference(x.astype(np.float64)))) <= 1e-3


def test_mel_filterbank_is_triangular_and_covers_range():
    fb = mel_filterbank()
    assert fb.shape == (80, 257)
    assert np.all(fb >= 0) and np.all(fb.max(axis=1) > 0.5)
```

`tests/test_models_ecapa.py`:

```python
import torch

from cctrain.models.ecapa import ECAPATDNN


def test_shapes_and_size():
    torch.manual_seed(0)
    m = ECAPATDNN().eval()
    out = m(torch.randn(2, 80, 198))
    assert out.shape == (2, 192)
    assert 5.5e6 <= sum(p.numel() for p in m.parameters()) <= 7.0e6


def test_context_modes_are_numerically_equivalent():
    torch.manual_seed(0)
    a = ECAPATDNN(context_mode="expand").eval()
    b = ECAPATDNN(context_mode="broadcast").eval()
    b.load_state_dict(a.state_dict())
    x = torch.randn(1, 80, 300)
    with torch.no_grad():
        assert torch.allclose(a(x), b(x), atol=1e-6, rtol=0)
```

`tests/test_export_enrollment.py`:

```python
import numpy as np
import onnx
import onnxruntime as ort
import pytest
import torch

from cctrain.export.enrollment import (
    OUTPUT_NAMES,
    build_prototype,
    export_enrollment_onnx,
    scale_to_peak,
    scale_to_rms_dbfs,
    speechlike_16k,
)


@pytest.mark.parametrize("mode", ["expand", "broadcast"])
def test_export_contract_and_p1_parity(tmp_path, mode):
    model = build_prototype(seed=0, context_mode=mode)
    path = export_enrollment_onnx(model, tmp_path / f"enroll-{mode}.onnx")
    proto = onnx.load(str(path))
    assert proto.ir_version == 8
    assert proto.opset_import[0].version == 13
    assert [i.name for i in proto.graph.input] == ["audio"]
    assert [o.name for o in proto.graph.output] == list(OUTPUT_NAMES)
    sess = ort.InferenceSession(str(path), providers=["CPUExecutionProvider"])
    for n in (32_000, 96_000, 192_000):
        x = scale_to_rms_dbfs(speechlike_16k(n, seed=n), -20.0)[None]
        with torch.no_grad():
            ref = [t.numpy() for t in model(torch.from_numpy(x))]
        got = sess.run(None, {"audio": x})
        for name, r, g in zip(OUTPUT_NAMES, ref, got):
            assert g.shape == ((192,) if name == "embedding" else (256,))
            np.testing.assert_allclose(g, r, atol=1e-5, rtol=1e-6, err_msg=f"{mode} {name} N={n}")


def test_outputs_within_runtime_limits(tmp_path):
    model = build_prototype(seed=1, context_mode="expand", out_std=5.0)
    for x in (scale_to_rms_dbfs(speechlike_16k(32_000, 1), -39.9), scale_to_peak(speechlike_16k(192_000, 2), 0.99)):
        with torch.no_grad():
            g_enc, b_enc, g_df, b_df, emb = model(torch.from_numpy(x)[None])
        for g in (g_enc, g_df):
            assert torch.isfinite(g).all() and g.min() >= 0.001 and g.max() <= 100.0
        for b in (b_enc, b_df):
            assert torch.isfinite(b).all() and b.abs().max() <= 50.0
        assert abs(float(emb.norm()) - 1.0) < 1e-4


def test_level_helpers():
    x = speechlike_16k(16_000, 0)
    y = scale_to_rms_dbfs(x, -40.0)
    assert abs(10 * np.log10(np.mean(y.astype(np.float64) ** 2)) + 40.0) < 1e-3
    assert abs(np.max(np.abs(scale_to_peak(x, 0.99))) - 0.99) < 1e-6
```

- [ ] **Step 2: Rodar e ver falhar**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" -m pytest tests/test_models_fbank.py tests/test_models_ecapa.py tests/test_export_enrollment.py -v 2>&1 | tail -3
```

Expected: FAIL com `ImportError`.

- [ ] **Step 3: Implementar `fbank.py`**

`src/cctrain/models/fbank.py`:

```python
"""Front-end do encoder dentro do grafo, só com operadores comuns (spec §3.1).

Hamming de 25 ms, hop de 10 ms, DFT como Conv1d com base de Fourier fixa (sem op STFT), potência,
80 mel (MatMul), Log(x + 1e-6) e subtração da média no tempo. Parâmetros (decisão de planejamento 11):
n_fft 512, mel HTK de 20 a 7.600 Hz, sem padding (T = 1 + (N - 400) // 160).
"""

from __future__ import annotations

import math

import numpy as np
import torch
from torch import Tensor, nn

SR = 16_000
WIN = 400
HOP = 160
N_FFT = 512
N_MELS = 80


def num_frames(n_samples: int) -> int:
    return 1 + (n_samples - WIN) // HOP


def mel_filterbank(n_mels: int = N_MELS, n_fft: int = N_FFT, sr: int = SR, f_min: float = 20.0,
                   f_max: float = 7600.0) -> np.ndarray:
    def hz_to_mel(f):
        return 2595.0 * np.log10(1.0 + np.asarray(f) / 700.0)

    def mel_to_hz(m):
        return 700.0 * (10.0 ** (np.asarray(m) / 2595.0) - 1.0)

    hz = mel_to_hz(np.linspace(hz_to_mel(f_min), hz_to_mel(f_max), n_mels + 2))
    bins = np.linspace(0.0, sr / 2.0, n_fft // 2 + 1)
    fb = np.zeros((n_mels, bins.size))
    for i in range(n_mels):
        lo, center, hi = hz[i], hz[i + 1], hz[i + 2]
        fb[i] = np.maximum(0.0, np.minimum((bins - lo) / (center - lo), (hi - bins) / (hi - center)))
    return fb


class TractSafeFbank(nn.Module):
    def __init__(self) -> None:
        super().__init__()
        n_bins = N_FFT // 2 + 1
        window = np.hamming(WIN)
        k = np.arange(n_bins)[:, None]
        n = np.arange(WIN)[None, :]
        angle = 2.0 * math.pi * k * n / N_FFT
        basis = np.concatenate([np.cos(angle) * window, -np.sin(angle) * window], axis=0)
        self.n_bins = n_bins
        self.register_buffer("basis", torch.from_numpy(basis).float().unsqueeze(1))  # [2F, 1, WIN]
        self.register_buffer("mel", torch.from_numpy(mel_filterbank()).float())  # [80, F]

    def forward(self, audio: Tensor) -> Tensor:
        spec = nn.functional.conv1d(audio.unsqueeze(1), self.basis, stride=HOP)  # [B, 2F, T]
        re, im = spec[:, : self.n_bins], spec[:, self.n_bins :]
        mel = torch.matmul(self.mel, re * re + im * im)  # [B, 80, T]
        logmel = torch.log(mel + 1e-6)
        return logmel - logmel.mean(dim=-1, keepdim=True)
```

- [ ] **Step 4: Implementar `ecapa.py`**

`src/cctrain/models/ecapa.py`:

```python
"""ECAPA-TDNN (C=512, embedding 192-d), com attentive statistics pooling de contexto global (spec §3.1).

context_mode="expand" usa Expand sobre o eixo de tempo (como o SpeechBrain); "broadcast" usa x*0 + stat,
sem operadores de forma. O M0 prototipa os dois no tract (Task 16).
"""

from __future__ import annotations

import torch
from torch import Tensor, nn


class TDNNBlock(nn.Module):
    def __init__(self, cin: int, cout: int, kernel: int, dilation: int = 1):
        super().__init__()
        self.conv = nn.Conv1d(cin, cout, kernel, dilation=dilation, padding=dilation * (kernel - 1) // 2)
        self.bn = nn.BatchNorm1d(cout)

    def forward(self, x: Tensor) -> Tensor:
        return self.bn(torch.relu(self.conv(x)))


class Res2Block(nn.Module):
    def __init__(self, channels: int, kernel: int, dilation: int, scale: int = 8):
        super().__init__()
        if channels % scale:
            raise ValueError("channels deve ser múltiplo de scale")
        self.scale = scale
        width = channels // scale
        self.blocks = nn.ModuleList([TDNNBlock(width, width, kernel, dilation) for _ in range(scale - 1)])

    def forward(self, x: Tensor) -> Tensor:
        chunks = torch.chunk(x, self.scale, dim=1)
        ys = [chunks[0]]
        y = chunks[0]
        for i, block in enumerate(self.blocks, start=1):
            y = block(chunks[i] if i == 1 else chunks[i] + y)
            ys.append(y)
        return torch.cat(ys, dim=1)


class SEBlock(nn.Module):
    def __init__(self, channels: int, bottleneck: int = 128):
        super().__init__()
        self.conv1 = nn.Conv1d(channels, bottleneck, 1)
        self.conv2 = nn.Conv1d(bottleneck, channels, 1)

    def forward(self, x: Tensor) -> Tensor:
        s = x.mean(dim=2, keepdim=True)
        return x * torch.sigmoid(self.conv2(torch.relu(self.conv1(s))))


class SERes2Block(nn.Module):
    def __init__(self, channels: int, kernel: int, dilation: int, scale: int = 8):
        super().__init__()
        self.tdnn1 = TDNNBlock(channels, channels, 1)
        self.res2 = Res2Block(channels, kernel, dilation, scale)
        self.tdnn2 = TDNNBlock(channels, channels, 1)
        self.se = SEBlock(channels)

    def forward(self, x: Tensor) -> Tensor:
        return x + self.se(self.tdnn2(self.res2(self.tdnn1(x))))


class AttentiveStatsPool(nn.Module):
    def __init__(self, channels: int, attention: int = 128, context_mode: str = "expand", eps: float = 1e-12):
        super().__init__()
        if context_mode not in ("expand", "broadcast"):
            raise ValueError(f"context_mode inválido: {context_mode}")
        self.context_mode = context_mode
        self.eps = eps
        self.tdnn = TDNNBlock(3 * channels, attention, 1)
        self.conv = nn.Conv1d(attention, channels, 1)

    def _broadcast(self, stat: Tensor, x: Tensor) -> Tensor:
        if self.context_mode == "expand":
            return stat.expand(-1, -1, x.shape[-1])
        return x * 0.0 + stat

    def forward(self, x: Tensor) -> Tensor:
        mean = x.mean(dim=2, keepdim=True)
        std = torch.sqrt(torch.clamp(((x - mean) ** 2).mean(dim=2, keepdim=True), min=self.eps))
        ctx = torch.cat([x, self._broadcast(mean, x), self._broadcast(std, x)], dim=1)
        w = torch.softmax(self.conv(torch.tanh(self.tdnn(ctx))), dim=2)
        mu = (w * x).sum(dim=2)
        sg = torch.sqrt(torch.clamp((w * x * x).sum(dim=2) - mu * mu, min=self.eps))
        return torch.cat([mu, sg], dim=1)


class ECAPATDNN(nn.Module):
    def __init__(self, n_mels: int = 80, channels: int = 512, mfa_channels: int = 1536, embedding_dim: int = 192,
                 attention: int = 128, context_mode: str = "expand"):
        super().__init__()
        self.layer1 = TDNNBlock(n_mels, channels, 5, 1)
        self.layer2 = SERes2Block(channels, 3, 2)
        self.layer3 = SERes2Block(channels, 3, 3)
        self.layer4 = SERes2Block(channels, 3, 4)
        self.mfa = TDNNBlock(3 * channels, mfa_channels, 1)
        self.pool = AttentiveStatsPool(mfa_channels, attention, context_mode)
        self.pool_bn = nn.BatchNorm1d(2 * mfa_channels)
        self.fc = nn.Linear(2 * mfa_channels, embedding_dim)

    def forward(self, feats: Tensor) -> Tensor:
        x1 = self.layer1(feats)
        x2 = self.layer2(x1)
        x3 = self.layer3(x2)
        x4 = self.layer4(x3)
        x = self.mfa(torch.cat([x2, x3, x4], dim=1))
        return self.fc(self.pool_bn(self.pool(x)))
```

- [ ] **Step 5: Implementar `export/enrollment.py`**

`src/cctrain/export/enrollment.py`:

```python
"""enrollment.onnx: áudio 16 kHz [1, N] -> gamma_enc, beta_enc, gamma_df, beta_df [256] e embedding [192].

Opset 13, ir_version 8, eixo dinâmico N (spec §2.2, §6.2). No M0 os pesos são aleatórios com semente
(protótipo de operadores); o encoder treinado é do M1.
"""

from __future__ import annotations

import math
from pathlib import Path

import numpy as np
import onnx
import torch
from torch import Tensor, nn

from cctrain.models.ecapa import ECAPATDNN
from cctrain.models.fbank import TractSafeFbank
from cctrain.models.film import FilmConfig, FilmGenerator

OUTPUT_NAMES = ("gamma_enc", "beta_enc", "gamma_df", "beta_df", "embedding")


class EnrollmentModel(nn.Module):
    def __init__(self, fbank: TractSafeFbank, encoder: ECAPATDNN, generator: FilmGenerator):
        super().__init__()
        self.fbank = fbank
        self.encoder = encoder
        self.generator = generator
        self.register_buffer("mask", torch.ones(1, 1))

    def forward(self, audio: Tensor):
        emb = self.encoder(self.fbank(audio))
        e = emb / torch.sqrt((emb * emb).sum(dim=-1, keepdim=True) + 1e-12)
        v = self.generator(e, self.mask)
        return (v.gamma_enc.reshape(-1), v.beta_enc.reshape(-1), v.gamma_df.reshape(-1), v.beta_df.reshape(-1),
                e.reshape(-1))


def build_prototype(seed: int, context_mode: str, beta_bound: float = 4.0, out_std: float = 0.5) -> EnrollmentModel:
    torch.manual_seed(seed)
    gen = FilmGenerator(FilmConfig(math.log(10.0), beta_bound))
    with torch.no_grad():
        gen.out.weight.normal_(std=out_std)
        gen.out.bias.normal_(std=out_std)
    return EnrollmentModel(TractSafeFbank(), ECAPATDNN(context_mode=context_mode), gen).eval()


def export_enrollment_onnx(model: EnrollmentModel, path: Path) -> Path:
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    example = torch.from_numpy(scale_to_rms_dbfs(speechlike_16k(96_000, 0), -20.0))[None]
    torch.onnx.export(
        model.eval(), (example,), str(path), input_names=["audio"], output_names=list(OUTPUT_NAMES),
        dynamic_axes={"audio": {1: "N"}}, opset_version=13, do_constant_folding=True, dynamo=False,
    )
    proto = onnx.load(str(path))
    proto.ir_version = 8
    onnx.checker.check_model(proto)
    onnx.save(proto, str(path))
    return path


def speechlike_16k(n: int, seed: int) -> np.ndarray:
    rng = np.random.default_rng(seed)
    t = np.arange(n) / 16_000.0
    f0 = 110.0 + 40.0 * rng.random()
    envelope = 0.5 * (1.0 + np.sin(2 * np.pi * 3.0 * t))
    voiced = sum(np.sin(2 * np.pi * f0 * h * t) / h for h in range(1, 20) if f0 * h < 7_500)
    return (envelope * voiced + 0.05 * rng.standard_normal(n)).astype(np.float32)


def scale_to_rms_dbfs(x: np.ndarray, dbfs: float) -> np.ndarray:
    rms = float(np.sqrt(np.mean(np.asarray(x, np.float64) ** 2)))
    return (np.asarray(x, np.float64) * (10 ** (dbfs / 20.0) / rms)).astype(np.float32)


def scale_to_peak(x: np.ndarray, peak: float) -> np.ndarray:
    return (np.asarray(x, np.float64) * (peak / float(np.max(np.abs(x))))).astype(np.float32)
```

- [ ] **Step 6: Rodar os testes até passarem**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" -m pytest tests/test_models_fbank.py tests/test_models_ecapa.py tests/test_export_enrollment.py -v
"$CCTRAIN_PY" -m ruff check src tests
```

Expected: todos PASS. Se o P1 (`atol 1e-5, rtol 1e-6`) falhar em algum modo, registre a maior diferença e pare: P1 é critério da spec e não se alarga; o ajuste (ex.: `do_constant_folding=False`) é decisão do orquestrador.

- [ ] **Step 7: Commit**

```bash
cd "$CCTRAIN_WT"
git add src/cctrain/models/fbank.py src/cctrain/models/ecapa.py src/cctrain/export/enrollment.py tests/test_models_fbank.py tests/test_models_ecapa.py tests/test_export_enrollment.py
git commit -m "feat(models): tract-safe fbank, ECAPA-TDNN and enrollment ONNX export prototype"
```

---
### Task 10: Download em stream com verificação (`data/sources.py`)

**Files:**
- Create: `src/cctrain/data/sources.py`
- Test: `tests/test_data_sources.py`

**Interfaces:**
- Consumes: `cctrain.disk.require_free`, `DiskError` (Task 1).
- Produces: `cctrain.data.sources`: `USER_AGENT`; `IntegrityError(RuntimeError)`; `StreamDigest(sha256: str, md5: str, size: int)`; `stream_download(source: str | Path, dest: Path, *, expected_sha256=None, expected_md5=None, expected_size=None, need_bytes=None, headers=None, session=None) -> StreamDigest`; `stream_extract_tar(source: str | Path, out_dir: Path, select: Callable[[str], str | None], *, expected_sha256=None, expected_md5=None, expected_size=None, need_bytes=None, session=None) -> tuple[StreamDigest, list[str]]` (extrai só os membros selecionados, num diretório de staging renomeado ao fim; o bruto não fica em disco); `list_zip_members(url: str) -> dict[str, int]`; `fetch_zip_members(url: str, members: Mapping[str, Path]) -> dict[str, StreamDigest]` (HTTP Range via `remotezip`); `cut_clip(raw: Path, dest: Path, start_s: float, duration_s: float = 10.0) -> float` (ffmpeg, mono 48 kHz PCM s16le, devolve a duração medida pelo ffprobe).

- [ ] **Step 1: Escrever os testes que falham**

`tests/test_data_sources.py`:

```python
import functools
import gzip
import hashlib
import http.server
import io
import tarfile
import threading
import zipfile
from pathlib import Path

import numpy as np
import pytest
import soundfile as sf

from cctrain.data.sources import (
    IntegrityError,
    cut_clip,
    fetch_zip_members,
    list_zip_members,
    stream_download,
    stream_extract_tar,
)
from cctrain.disk import DiskError


class _RangeHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args):  # silencioso nos testes
        pass

    def do_GET(self):  # noqa: N802
        path = Path(self.translate_path(self.path))
        if not path.is_file():
            self.send_error(404)
            return
        data = path.read_bytes()
        rng = self.headers.get("Range")
        if rng:
            first, last = rng.split("=", 1)[1].split("-")
            if first == "":
                start, end = max(0, len(data) - int(last)), len(data) - 1
            else:
                start, end = int(first), min(int(last) if last else len(data) - 1, len(data) - 1)
            body = data[start : end + 1]
            self.send_response(206)
            self.send_header("Content-Range", f"bytes {start}-{end}/{len(data)}")
        else:
            body = data
            self.send_response(200)
        self.send_header("Accept-Ranges", "bytes")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


@pytest.fixture()
def www(tmp_path):
    root = tmp_path / "www"
    root.mkdir()
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0),
                                             functools.partial(_RangeHandler, directory=str(root)))
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    yield root, f"http://127.0.0.1:{server.server_address[1]}"
    server.shutdown()


def _targz(entries: dict[str, bytes]) -> bytes:
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w") as tar:
        for name, data in entries.items():
            info = tarfile.TarInfo(name)
            info.size = len(data)
            tar.addfile(info, io.BytesIO(data))
    return gzip.compress(buf.getvalue())


def test_stream_download_verifies_and_is_atomic(www, tmp_path):
    root, base = www
    (root / "a.bin").write_bytes(b"x" * 5000)
    sha = hashlib.sha256(b"x" * 5000).hexdigest()
    d = stream_download(f"{base}/a.bin", tmp_path / "out" / "a.bin", expected_sha256=sha, expected_size=5000)
    assert d.sha256 == sha and d.size == 5000
    assert d.md5 == hashlib.md5(b"x" * 5000).hexdigest()
    with pytest.raises(IntegrityError):
        stream_download(f"{base}/a.bin", tmp_path / "out" / "b.bin", expected_sha256="0" * 64)
    assert not (tmp_path / "out" / "b.bin").exists()
    assert not (tmp_path / "out" / "b.bin.part").exists()


def test_stream_download_fails_closed_on_disk(www, tmp_path):
    root, base = www
    (root / "a.bin").write_bytes(b"x")
    with pytest.raises(DiskError):
        stream_download(f"{base}/a.bin", tmp_path / "a.bin", need_bytes=10**16)


def test_range_header_is_honored(www, tmp_path):
    root, base = www
    (root / "big.bin").write_bytes(bytes(range(256)) * 10)
    d = stream_download(f"{base}/big.bin", tmp_path / "head.bin", headers={"Range": "bytes=0-99"})
    assert d.size == 100 and (tmp_path / "head.bin").read_bytes() == bytes(range(100))


def test_stream_extract_tar_selects_members_and_checks_md5(www, tmp_path):
    root, base = www
    raw = _targz({"L/dev/1/a.flac": b"A", "L/dev/1/a.txt": b"T", "L/dev/2/b.flac": b"BB"})
    (root / "x.tar.gz").write_bytes(raw)
    select = lambda name: ("lib/" + name.split("/", 1)[1]) if name.endswith(".flac") else None  # noqa: E731
    d, files = stream_extract_tar(f"{base}/x.tar.gz", tmp_path / "lib-out", select,
                                  expected_md5=hashlib.md5(raw).hexdigest())
    assert files == ["lib/dev/1/a.flac", "lib/dev/2/b.flac"]
    assert (tmp_path / "lib-out" / "lib/dev/2/b.flac").read_bytes() == b"BB"
    assert d.size == len(raw)
    with pytest.raises(IntegrityError):
        stream_extract_tar(f"{base}/x.tar.gz", tmp_path / "bad-out", select, expected_md5="0" * 32)
    assert not (tmp_path / "bad-out").exists()
    assert not (tmp_path / "bad-out.staging").exists()


def test_stream_extract_tar_from_local_file_and_traversal(tmp_path):
    archive = tmp_path / "local.tar.gz"
    archive.write_bytes(_targz({"a.mp3": b"M"}))
    _, files = stream_extract_tar(archive, tmp_path / "cv", lambda n: "clips/" + n)
    assert files == ["clips/a.mp3"]
    with pytest.raises(IntegrityError):
        stream_extract_tar(archive, tmp_path / "evil", lambda n: "../" + n)


def test_fetch_zip_members_by_range(www, tmp_path):
    root, base = www
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", compression=zipfile.ZIP_STORED) as z:
        z.writestr("wav48/p1/p1_001_mic1.flac", b"F" * 3000)
        z.writestr("wav48/p1/p1_001_mic2.flac", b"G" * 3000)
        z.writestr("license_text", b"Creative Commons Attribution 4.0")
    (root / "c.zip").write_bytes(buf.getvalue())
    names = list_zip_members(f"{base}/c.zip")
    assert names["license_text"] == len(b"Creative Commons Attribution 4.0")
    got = fetch_zip_members(f"{base}/c.zip", {"wav48/p1/p1_001_mic1.flac": tmp_path / "v" / "a.flac",
                                              "license_text": tmp_path / "v" / "license_text"})
    assert (tmp_path / "v" / "a.flac").read_bytes() == b"F" * 3000
    assert got["license_text"].sha256 == hashlib.sha256(b"Creative Commons Attribution 4.0").hexdigest()
    with pytest.raises(IntegrityError):
        fetch_zip_members(f"{base}/c.zip", {"nope": tmp_path / "v" / "nope"})


def test_cut_clip_produces_10s_mono_48k(tmp_path):
    raw = tmp_path / "raw.wav"
    sf.write(raw, (0.1 * np.random.default_rng(0).standard_normal((44_100 * 20, 2))).astype("f4"), 44_100)
    dur = cut_clip(raw, tmp_path / "clip.wav", start_s=5.0)
    info = sf.info(tmp_path / "clip.wav")
    assert abs(dur - 10.0) <= 0.01
    assert (info.samplerate, info.channels, info.subtype) == (48_000, 1, "PCM_16")
```

- [ ] **Step 2: Rodar e ver falhar**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" -m pytest tests/test_data_sources.py -v 2>&1 | tail -3
```

Expected: FAIL com `ImportError`.

- [ ] **Step 3: Implementar `sources.py`**

`src/cctrain/data/sources.py`:

```python
"""Download em stream, com hash e guarda de disco; brutos transitórios (spec §4.2).

O bruto inteiro nunca é gravado quando o formato permite extração em stream (tar). Arquivos parciais
criados por esta função são removidos em caso de falha; nada mais é apagado.
"""

from __future__ import annotations

import hashlib
import io
import os
import shutil
import subprocess
import tarfile
from collections.abc import Callable, Iterator, Mapping
from dataclasses import dataclass
from pathlib import Path, PurePosixPath

import requests

from cctrain.disk import require_free

USER_AGENT = "clearcore-train/0.1 (research data fetch)"
CHUNK = 1 << 20


class IntegrityError(RuntimeError):
    """Hash, tamanho ou caminho fora do esperado. Falha fechado."""


@dataclass(frozen=True)
class StreamDigest:
    sha256: str
    md5: str
    size: int


def _is_url(source: str | Path) -> bool:
    return isinstance(source, str) and source.startswith(("http://", "https://"))


def _chunks(source: str | Path, *, headers: Mapping[str, str] | None = None,
            session: requests.Session | None = None, timeout: float = 60.0) -> Iterator[bytes]:
    if not _is_url(source):
        with open(source, "rb") as f:
            yield from iter(lambda: f.read(CHUNK), b"")
        return
    s = session or requests.Session()
    with s.get(str(source), stream=True, timeout=timeout,
               headers={"User-Agent": USER_AGENT, **(headers or {})}) as r:
        r.raise_for_status()
        for chunk in r.iter_content(CHUNK):
            if chunk:
                yield chunk


class _HashingReader(io.RawIOBase):
    def __init__(self, chunks: Iterator[bytes]):
        self._it = iter(chunks)
        self._buf = bytearray()
        self._sha = hashlib.sha256()
        self._md5 = hashlib.md5()
        self.size = 0

    def readable(self) -> bool:
        return True

    def readinto(self, b) -> int:
        while len(self._buf) < len(b):
            try:
                self._buf += next(self._it)
            except StopIteration:
                break
        n = min(len(b), len(self._buf))
        data = bytes(self._buf[:n])
        del self._buf[:n]
        b[:n] = data
        self._sha.update(data)
        self._md5.update(data)
        self.size += n
        return n

    def drain(self) -> None:
        buf = bytearray(CHUNK)
        while self.readinto(buf):
            pass

    def digest(self) -> StreamDigest:
        return StreamDigest(self._sha.hexdigest(), self._md5.hexdigest(), self.size)


def _verify(d: StreamDigest, what: str, sha256: str | None, md5: str | None, size: int | None) -> None:
    if size is not None and d.size != size:
        raise IntegrityError(f"{what}: tamanho {d.size} != {size}")
    if sha256 and d.sha256 != sha256:
        raise IntegrityError(f"{what}: sha256 {d.sha256} != {sha256}")
    if md5 and d.md5 != md5:
        raise IntegrityError(f"{what}: md5 {d.md5} != {md5}")


def stream_download(source: str | Path, dest: Path, *, expected_sha256: str | None = None,
                    expected_md5: str | None = None, expected_size: int | None = None,
                    need_bytes: int | None = None, headers: Mapping[str, str] | None = None,
                    session: requests.Session | None = None) -> StreamDigest:
    dest = Path(dest)
    dest.parent.mkdir(parents=True, exist_ok=True)
    require_free(dest.parent, need_bytes if need_bytes is not None else (expected_size or 0))
    part = dest.with_name(dest.name + ".part")
    reader = _HashingReader(_chunks(source, headers=headers, session=session))
    try:
        buf = bytearray(CHUNK)
        with open(part, "wb") as f:
            while n := reader.readinto(buf):
                f.write(memoryview(buf)[:n])
        d = reader.digest()
        _verify(d, str(source), expected_sha256, expected_md5, expected_size)
    except BaseException:
        part.unlink(missing_ok=True)
        raise
    os.replace(part, dest)
    return d


def _safe_rel(rel: str) -> PurePosixPath:
    p = PurePosixPath(rel)
    if p.is_absolute() or ".." in p.parts or not p.parts:
        raise IntegrityError(f"caminho de saída inseguro: {rel!r}")
    return p


def stream_extract_tar(source: str | Path, out_dir: Path, select: Callable[[str], str | None], *,
                       expected_sha256: str | None = None, expected_md5: str | None = None,
                       expected_size: int | None = None, need_bytes: int | None = None,
                       session: requests.Session | None = None) -> tuple[StreamDigest, list[str]]:
    out_dir = Path(out_dir)
    if out_dir.exists():
        raise IntegrityError(f"{out_dir} já existe; inspecione antes de reextrair")
    staging = out_dir.with_name(out_dir.name + ".staging")
    if staging.exists():
        raise IntegrityError(f"{staging} existe de uma extração interrompida; inspecione")
    staging.mkdir(parents=True)
    require_free(staging, need_bytes or 0)
    reader = _HashingReader(_chunks(source, session=session))
    extracted: list[str] = []
    try:
        with tarfile.open(fileobj=io.BufferedReader(reader, CHUNK), mode="r|*") as tar:
            for member in tar:
                if not member.isfile():
                    continue
                rel = select(member.name)
                if rel is None:
                    continue
                dest = staging / _safe_rel(rel)
                dest.parent.mkdir(parents=True, exist_ok=True)
                require_free(staging, member.size)
                src = tar.extractfile(member)
                with open(dest, "wb") as f:
                    shutil.copyfileobj(src, f, CHUNK)
                extracted.append(str(_safe_rel(rel)))
        reader.drain()
        d = reader.digest()
        _verify(d, str(source), expected_sha256, expected_md5, expected_size)
    except BaseException:
        shutil.rmtree(staging, ignore_errors=True)
        raise
    os.replace(staging, out_dir)
    return d, sorted(extracted)


def list_zip_members(url: str) -> dict[str, int]:
    from remotezip import RemoteZip

    with RemoteZip(url, headers={"User-Agent": USER_AGENT}) as z:
        return {i.filename: i.file_size for i in z.infolist()}


def fetch_zip_members(url: str, members: Mapping[str, Path]) -> dict[str, StreamDigest]:
    from remotezip import RemoteZip

    out: dict[str, StreamDigest] = {}
    with RemoteZip(url, headers={"User-Agent": USER_AGENT}) as z:
        infos = {i.filename: i for i in z.infolist()}
        missing = [m for m in members if m not in infos]
        if missing:
            raise IntegrityError(f"membros ausentes no zip: {missing}")
        for member, dest in members.items():
            dest = Path(dest)
            dest.parent.mkdir(parents=True, exist_ok=True)
            require_free(dest.parent, infos[member].file_size)
            part = dest.with_name(dest.name + ".part")
            try:
                with z.open(member) as src:
                    reader = _HashingReader(iter(lambda s=src: s.read(CHUNK), b""))
                    buf = bytearray(CHUNK)
                    with open(part, "wb") as f:
                        while n := reader.readinto(buf):
                            f.write(memoryview(buf)[:n])
                d = reader.digest()
                _verify(d, member, None, None, infos[member].file_size)
            except BaseException:
                part.unlink(missing_ok=True)
                raise
            os.replace(part, dest)
            out[member] = d
    return out


def cut_clip(raw: Path, dest: Path, start_s: float, duration_s: float = 10.0) -> float:
    dest = Path(dest)
    dest.parent.mkdir(parents=True, exist_ok=True)
    require_free(dest.parent, int(duration_s * 48_000 * 2) + (1 << 20))
    subprocess.run(["ffmpeg", "-nostdin", "-v", "error", "-y", "-ss", str(start_s), "-t", str(duration_s), "-i",
                    str(raw), "-ac", "1", "-ar", "48000", "-c:a", "pcm_s16le", str(dest)], check=True)
    out = subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0",
                          str(dest)], check=True, capture_output=True, text=True)
    duration = float(out.stdout.strip())
    if abs(duration - duration_s) > 0.01:
        raise IntegrityError(f"{dest}: duração {duration} s, esperado {duration_s} s")
    return duration
```

- [ ] **Step 4: Rodar os testes até passarem**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" -m pytest tests/test_data_sources.py -v
"$CCTRAIN_PY" -m ruff check src tests
```

Expected: todos PASS (o servidor HTTP é local; não há acesso à rede).

- [ ] **Step 5: Commit**

```bash
cd "$CCTRAIN_WT"
git add src/cctrain/data/sources.py tests/test_data_sources.py
git commit -m "feat(data): streaming download with hash checks, HTTP-range zip members and tar stream extraction"
```

---

### Task 11: RIR sintéticas 48 kHz

**Files:**
- Create: `src/cctrain/data/rir.py`, `scripts/gen_rir_pilot.py`
- Create (gerados): `data/rir/synthetic-v1/rir_*.flac` (ignorado), `manifests/files/rir-synthetic-v1-pilot.jsonl`, `manifests/files/rir-synthetic-v1-pilot.params.jsonl`
- Test: `tests/test_data_rir.py`

**Interfaces:**
- Consumes: `cctrain.data.manifest.FileRecord`, `write_file_manifest`, `load_sources`, `training_records` (Task 3); `cctrain.disk.require_free`; `cctrain.hashing.sha256_file`; `cctrain.paths.Paths`.
- Produces: `cctrain.data.rir`: `RirParams` (dataclass: `seed, room_dim, rt60_target, rt60_measured, max_order, source, mic, length, gain`); `generate_rir(seed: int, fs: int = 48000, rt60_range=(0.05, 1.0), max_order_cap: int = 20, max_len_s: float = 1.2) -> tuple[np.ndarray, RirParams]` (f32, pico 0,99 no caminho direto, que fica até 1 ms do início); `write_rir_set(out_dir: Path, seeds: Iterable[int], data_root: Path) -> tuple[list[FileRecord], list[RirParams]]`.

- [ ] **Step 1: Escrever os testes que falham**

`tests/test_data_rir.py`:

```python
import numpy as np

from cctrain.data.manifest import load_sources, training_records
from cctrain.data.rir import generate_rir, write_rir_set


def test_rir_is_deterministic_and_bounded():
    h1, p1 = generate_rir(3)
    h2, p2 = generate_rir(3)
    assert np.array_equal(h1, h2)
    assert (p1.seed, p1.room_dim, p1.source, p1.mic, p1.max_order) == (p2.seed, p2.room_dim, p2.source, p2.mic,
                                                                        p2.max_order)
    assert h1.dtype == np.float32
    assert len(h1) <= int(1.2 * 48_000)
    assert abs(float(np.max(np.abs(h1))) - 0.99) < 1e-6
    assert int(np.argmax(np.abs(h1))) <= 48
    assert 0.05 <= p1.rt60_target <= 1.0
    assert not np.array_equal(generate_rir(4)[0][:100], h1[:100])


def test_written_set_has_valid_training_records(tmp_path):
    records, params = write_rir_set(tmp_path / "rir", seeds=range(3), data_root=tmp_path)
    assert len(records) == len(params) == 3
    assert all(r.split == "rir" and r.sr == 48_000 and r.codec == "flac" for r in records)
    assert training_records(records, load_sources()) == records
```

- [ ] **Step 2: Rodar e ver falhar**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" -m pytest tests/test_data_rir.py -v 2>&1 | tail -3
```

Expected: FAIL com `ImportError`.

- [ ] **Step 3: Implementar `rir.py` e o script**

`src/cctrain/data/rir.py`:

```python
"""RIR sintéticas por image source (pyroomacoustics), 48 kHz, RT60 alvo U[0,05; 1,0] s (spec §4.2).

Cada RIR é cortada para começar 1 ms antes do caminho direto (o alvo seco fica alinhado com a mistura),
limitada a 1,2 s e normalizada para pico 0,99. max_order é limitado a 20 (custo); o RT60 medido é registrado.
"""

from __future__ import annotations

from collections.abc import Iterable
from dataclasses import asdict, dataclass
from pathlib import Path

import numpy as np
import pyroomacoustics as pra
import soundfile as sf

from cctrain.data.manifest import FileRecord
from cctrain.disk import require_free
from cctrain.hashing import sha256_file

SOURCE_ID = "rir-synthetic-v1"
LICENSE = "LicenseRef-cctrain-generated"


@dataclass(frozen=True)
class RirParams:
    seed: int
    room_dim: tuple[float, float, float]
    rt60_target: float
    rt60_measured: float
    max_order: int
    source: tuple[float, float, float]
    mic: tuple[float, float, float]
    length: int
    gain: float


def generate_rir(seed: int, fs: int = 48_000, rt60_range: tuple[float, float] = (0.05, 1.0),
                 max_order_cap: int = 20, max_len_s: float = 1.2) -> tuple[np.ndarray, RirParams]:
    rng = np.random.default_rng(seed)
    for _ in range(200):
        room = np.array([rng.uniform(3.0, 10.0), rng.uniform(3.0, 10.0), rng.uniform(2.5, 4.0)])
        rt60 = float(rng.uniform(*rt60_range))
        src = rng.uniform(0.5, room - 0.5)
        mic = rng.uniform(0.5, room - 0.5)
        if np.linalg.norm(src - mic) < 0.5:
            continue
        try:
            e_abs, max_order = pra.inverse_sabine(rt60, room)
        except ValueError:
            continue
        order = int(min(max_order, max_order_cap))
        r = pra.ShoeBox(room, fs=fs, materials=pra.Material(e_abs), max_order=order, air_absorption=False)
        r.add_source(src)
        r.add_microphone(mic)
        r.compute_rir()
        h = np.asarray(r.rir[0][0], dtype=np.float64)
        start = max(0, int(np.argmax(np.abs(h))) - fs // 1000)
        h = h[start : start + int(max_len_s * fs)]
        gain = 0.99 / float(np.max(np.abs(h)))
        h = (h * gain).astype(np.float32)
        try:
            measured = float(pra.experimental.measure_rt60(h, fs=fs))
        except Exception:  # noqa: BLE001 - medida informativa; RIR curta pode não decair 60 dB
            measured = float("nan")
        params = RirParams(seed, tuple(map(float, room)), rt60, measured, order, tuple(map(float, src)),
                           tuple(map(float, mic)), int(h.size), gain)
        return h, params
    raise RuntimeError(f"seed {seed}: nenhuma sala viável em 200 tentativas")


def write_rir_set(out_dir: Path, seeds: Iterable[int], data_root: Path) -> tuple[list[FileRecord], list[RirParams]]:
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    records, params = [], []
    for seed in seeds:
        h, p = generate_rir(seed)
        path = out_dir / f"rir_{seed:05d}.flac"
        require_free(out_dir, h.size * 4)
        sf.write(path, h, 48_000, subtype="PCM_24")
        records.append(FileRecord(
            path=str(path.relative_to(data_root)), sha256=sha256_file(path), source_id=SOURCE_ID, license=LICENSE,
            role="train", speaker=None, split="rir", sr=48_000, num_samples=int(h.size), band_hz=None,
            codec="flac"))
        params.append(p)
    return records, params


def params_to_jsonl(params: Iterable[RirParams]) -> str:
    import json

    return "".join(json.dumps(asdict(p), sort_keys=True) + "\n" for p in params)
```

`scripts/gen_rir_pilot.py`:

```python
"""Gera as 64 RIR do piloto em data/rir/synthetic-v1 e os manifests versionados."""

from cctrain.data.manifest import FILES_DIR, write_file_manifest
from cctrain.data.rir import params_to_jsonl, write_rir_set
from cctrain.paths import Paths


def main() -> None:
    paths = Paths.from_env()
    records, params = write_rir_set(paths.data / "rir" / "synthetic-v1", seeds=range(64), data_root=paths.data)
    digest = write_file_manifest(records, FILES_DIR / "rir-synthetic-v1-pilot.jsonl")
    (FILES_DIR / "rir-synthetic-v1-pilot.params.jsonl").write_text(params_to_jsonl(params))
    print(f"{len(records)} RIR; manifest sha256 {digest}")


if __name__ == "__main__":
    main()
```

- [ ] **Step 4: Rodar os testes e gerar o conjunto do piloto**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" -m pytest tests/test_data_rir.py -v
"$CCTRAIN_PY" scripts/gen_rir_pilot.py
du -sh data/rir/synthetic-v1
"$CCTRAIN_PY" -m ruff check src tests scripts
```

Expected: PASS; `64 RIR; manifest sha256 ...` (os FLAC vão para `$CCTRAIN_DATA_DIR`, compartilhado entre worktrees).

- [ ] **Step 5: Commit**

```bash
cd "$CCTRAIN_WT"
git add src/cctrain/data/rir.py scripts/gen_rir_pilot.py manifests/files/rir-synthetic-v1-pilot.jsonl manifests/files/rir-synthetic-v1-pilot.params.jsonl tests/test_data_rir.py
git commit -m "feat(data): synthetic 48 kHz image-source RIRs for the pilot with manifests"
```

---
### Task 12: Busca do piloto, banda efetiva e manifests por arquivo

**Files:**
- Create: `src/cctrain/data/band.py`, `src/cctrain/data/pilot.py`, `scripts/fetch_pilot.py`
- Create (gerados): `manifests/files/vctk-0.92-pilot.jsonl`, `manifests/files/librispeech-dev-clean-pilot.jsonl`, `manifests/files/noise-wikimedia-cc0-pilot.jsonl`, `manifests/files/cv26-ptbr.sha256` (só se o CV for ingerido), atualizações em `manifests/sources.lock.json`
- Test: `tests/test_data_band.py`, `tests/test_data_pilot.py`, `tests/test_data_pilot_integrity.py` (marcador `data`)

**Interfaces:**
- Consumes: `cctrain.data.sources.stream_download`, `stream_extract_tar`, `list_zip_members`, `fetch_zip_members`, `cut_clip`, `IntegrityError` (Task 10); `cctrain.data.manifest` (Task 3); `cctrain.paths.Paths`; `cctrain.hashing`.
- Produces:
  - `cctrain.data.band`: `effective_band_hz(x: np.ndarray, sr: int, rolloff: float = 0.99) -> float`.
  - `cctrain.data.pilot`: `VCTK_PILOT_SPEAKERS`, `VCTK_TEST_FILES: dict[str, str]` (nome → SHA-256 do Clearcore), `VCTK_UTTERANCES_PER_SPEAKER = 50`; `select_vctk_members(names) -> dict[str, str]` (membro → caminho relativo ao data root); `check_license_text(text: str, what: str) -> None`; `librispeech_select(name: str) -> str | None`; `record_for_audio(path: Path, data_root: Path, *, source: SourceManifest, speaker: str | None, split: str, codec: str) -> FileRecord`; `fetch_vctk(paths)`, `fetch_librispeech(paths)`, `fetch_noise(paths)`, `ingest_cv26(paths, archive: Path)`, cada um gravando o manifest e devolvendo `list[FileRecord]`; `PILOT_MANIFESTS: dict[str, Path]`.
  - Dados em `$CCTRAIN_DATA_DIR/raw/{vctk-0.92,librispeech-dev-clean,noise-wikimedia-cc0,cv26-ptbr}/`.

- [ ] **Step 1: Escrever os testes que falham**

`tests/test_data_band.py`:

```python
import numpy as np
from scipy.signal import firwin, lfilter

from cctrain.data.band import effective_band_hz


def test_band_of_lowpassed_noise():
    x = np.random.default_rng(0).standard_normal(48_000 * 3)
    y = lfilter(firwin(511, 4000, fs=48_000), [1.0], x)
    assert 3500 <= effective_band_hz(y, 48_000) <= 4300


def test_band_of_white_noise_reaches_nyquist_region():
    x = np.random.default_rng(1).standard_normal(16_000 * 3)
    assert effective_band_hz(x, 16_000) >= 7500


def test_silence_has_zero_band():
    assert effective_band_hz(np.zeros(16_000), 16_000) == 0.0
```

`tests/test_data_pilot.py`:

```python
import pytest

from cctrain.data.manifest import ManifestError
from cctrain.data.pilot import (
    VCTK_PILOT_SPEAKERS,
    VCTK_TEST_FILES,
    check_license_text,
    librispeech_select,
    select_vctk_members,
)


def _names(n_per_speaker: int = 60) -> list[str]:
    names = ["license_text", "README.txt"]
    for spk in VCTK_PILOT_SPEAKERS + ("p240",):
        for i in range(1, n_per_speaker + 1):
            names += [f"wav48_silence_trimmed/{spk}/{spk}_{i:03d}_mic1.flac",
                      f"wav48_silence_trimmed/{spk}/{spk}_{i:03d}_mic2.flac"]
    names += [f"wav48_silence_trimmed/{n.split('_')[0]}/{n}" for n in VCTK_TEST_FILES]
    return names


def test_vctk_selection_is_mic1_first_50_and_test_files():
    sel = select_vctk_members(_names())
    pilot = [v for v in sel.values() if "/pilot/" in v]
    assert len(pilot) == 50 * len(VCTK_PILOT_SPEAKERS)
    assert all(v.endswith("_mic1.flac") for v in pilot)
    assert "raw/vctk-0.92/pilot/p227/p227_050_mic1.flac" in sel.values()
    assert "raw/vctk-0.92/pilot/p227/p227_051_mic1.flac" not in sel.values()
    assert sum("/test/" in v for v in sel.values()) == len(VCTK_TEST_FILES)
    assert sel["license_text"] == "raw/vctk-0.92/license_text"
    assert not any("p240" in v for v in sel.values())


def test_vctk_selection_fails_closed():
    with pytest.raises(ManifestError, match="licença"):
        select_vctk_members([n for n in _names() if n != "license_text"])
    with pytest.raises(ManifestError, match="p227"):
        select_vctk_members(_names(n_per_speaker=10))


def test_license_text_must_be_cc_by_4():
    check_license_text("This corpus is licensed under the Creative Commons License: Attribution 4.0 International", "x")
    with pytest.raises(ManifestError):
        check_license_text("Open Data Commons Attribution License (ODC-By) v1.0", "x")


def test_librispeech_selection():
    assert librispeech_select("LibriSpeech/dev-clean/1272/128104/1272-128104-0000.flac") == \
        "dev-clean/1272/128104/1272-128104-0000.flac"
    assert librispeech_select("LibriSpeech/dev-clean/1272/128104/1272-128104.trans.txt") is not None
    assert librispeech_select("LibriSpeech/LICENSE.TXT") == "LICENSE.TXT"
    assert librispeech_select("LibriSpeech/dev-clean/1272/") is None
```

`tests/test_data_pilot_integrity.py`:

```python
from collections import Counter

import pytest

from cctrain.data.manifest import load_sources, read_file_manifest, training_records
from cctrain.data.pilot import PILOT_MANIFESTS, VCTK_TEST_FILES
from cctrain.hashing import sha256_file

pytestmark = pytest.mark.data


@pytest.mark.parametrize("source_id", sorted(PILOT_MANIFESTS))
def test_every_record_matches_its_file(paths, source_id):
    records = read_file_manifest(PILOT_MANIFESTS[source_id])
    assert records, source_id
    for r in records:
        assert sha256_file(paths.data / r.path) == r.sha256, r.path
    allowed = [r for r in records if r.role == "train"]
    assert training_records(allowed, load_sources()) == allowed


def test_pilot_amounts():
    vctk = read_file_manifest(PILOT_MANIFESTS["vctk-0.92"])
    pilot_s = sum(r.num_samples / r.sr for r in vctk if r.split == "pilot")
    assert pilot_s >= 15 * 60
    assert {r.path.rsplit("/", 1)[1] for r in vctk if r.split == "test"} == set(VCTK_TEST_FILES)
    libri = read_file_manifest(PILOT_MANIFESTS["librispeech-dev-clean"])
    assert len({r.speaker for r in libri}) >= 40
    assert sum(r.num_samples / r.sr for r in libri) >= 5 * 3600
    noise = read_file_manifest(PILOT_MANIFESTS["noise-wikimedia-cc0"])
    assert Counter(r.num_samples for r in noise) == Counter({480_000: 3})
```

- [ ] **Step 2: Rodar e ver falhar**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" -m pytest tests/test_data_band.py tests/test_data_pilot.py -v 2>&1 | tail -3
```

Expected: FAIL com `ImportError`.

- [ ] **Step 3: Implementar `band.py` e `pilot.py`**

`src/cctrain/data/band.py`:

```python
"""Banda efetiva por clipe: frequência em que a potência acumulada atinge 99% (spec §4.3).

A definição de max_freq a partir dela fica para o M2 (decisão de planejamento 9).
"""

from __future__ import annotations

import numpy as np
from scipy.signal import welch


def effective_band_hz(x: np.ndarray, sr: int, rolloff: float = 0.99) -> float:
    x = np.asarray(x, dtype=np.float64)
    if x.size == 0 or not np.any(x):
        return 0.0
    nperseg = min(x.size, 2048 if sr >= 32_000 else 1024)
    f, p = welch(x, fs=sr, nperseg=nperseg)
    c = np.cumsum(p)
    idx = int(np.searchsorted(c, rolloff * c[-1]))
    return float(f[min(idx, f.size - 1)])
```

`src/cctrain/data/pilot.py`:

```python
"""Dados do piloto do M0 (spec §9.2): ~1 h para medir, nunca para treinar.

VCTK por HTTP Range (8 falantes x 50 falas mic1 + as 6 amostras de teste do Clearcore), LibriSpeech
dev-clean inteiro (stream do tar.gz; o loader usa um subconjunto), os 3 ruídos CC0 dos testes do Clearcore
(split test) e, se o dono tiver baixado, o CV 26.0 pt-BR (manifest fora do git).
"""

from __future__ import annotations

import csv
from collections import defaultdict
from collections.abc import Iterable
from pathlib import Path, PurePosixPath

import numpy as np
import requests
import soundfile as sf

from cctrain.data.band import effective_band_hz
from cctrain.data.manifest import (
    FILES_DIR,
    FileRecord,
    ManifestError,
    SourceManifest,
    load_sources,
    update_lock,
    write_file_manifest,
)
from cctrain.data.sources import (
    IntegrityError,
    cut_clip,
    fetch_zip_members,
    list_zip_members,
    stream_download,
    stream_extract_tar,
)
from cctrain.hashing import sha256_file
from cctrain.paths import Paths

VCTK_PREFIX = "wav48_silence_trimmed"
VCTK_PILOT_SPEAKERS = ("p227", "p228", "p229", "p230", "p231", "p233", "p234", "p236")
VCTK_UTTERANCES_PER_SPEAKER = 50
# Amostras de teste do Clearcore (falantes p225/p226, fora do treino); SHA-256 de scripts/voice-samples.sha256.
VCTK_TEST_FILES = {
    "p225_003_mic1.flac": "877a8327608bd37b82d125adca98bdf7c92c904dff8c58fb711d7d9be364dc99",
    "p225_008_mic1.flac": "c4c4d3e1e128821c364db33fcec4b708e36edcea3bbc70f5334d6715aa4e00f9",
    "p225_011_mic1.flac": "8eaa96bb597b8ea88198b35da6f4a4e546125bc92c114e0aaee1cd87e7f08bbb",
    "p225_022_mic1.flac": "a4a88b254b17544e05fb259ded18c294b133abda8ff515ae9c72e842b64ec9d0",
    "p226_008_mic1.flac": "9570b6f075670d731ed0f0e0097db48988972b7a541e36da6455c9f1003ae3c6",
    "p226_016_mic1.flac": "4b3124fbf362cff6f20e557cf18d20f2975f8f76d42a2942612ce67921b408da",
}
LIBRISPEECH_MD5_URL = "https://www.openslr.org/resources/12/md5sum.txt"
PILOT_MANIFESTS = {
    "vctk-0.92": FILES_DIR / "vctk-0.92-pilot.jsonl",
    "librispeech-dev-clean": FILES_DIR / "librispeech-dev-clean-pilot.jsonl",
    "noise-wikimedia-cc0": FILES_DIR / "noise-wikimedia-cc0-pilot.jsonl",
}


def check_license_text(text: str, what: str) -> None:
    if "Attribution 4.0" not in text:
        raise ManifestError(f"{what}: texto de licença não declara CC BY 4.0; pare e consulte o dono")


def select_vctk_members(names: Iterable[str]) -> dict[str, str]:
    names = list(names)
    out: dict[str, str] = {}
    by_speaker: dict[str, list[str]] = defaultdict(list)
    for n in names:
        parts = n.split("/")
        if len(parts) == 3 and parts[0] == VCTK_PREFIX and parts[2].endswith("_mic1.flac"):
            if parts[1] in VCTK_PILOT_SPEAKERS:
                by_speaker[parts[1]].append(n)
            elif parts[2] in VCTK_TEST_FILES:
                out[n] = f"raw/vctk-0.92/test/{parts[2]}"
    licenses = [n for n in names if "licen" in n.lower() and not n.endswith("/")]
    if not licenses:
        raise ManifestError("VCTK: arquivo de licença ausente no zip")
    for n in licenses:
        out[n] = f"raw/vctk-0.92/{PurePosixPath(n).name}"
    for spk in VCTK_PILOT_SPEAKERS:
        members = sorted(by_speaker[spk])
        if len(members) < VCTK_UTTERANCES_PER_SPEAKER:
            raise ManifestError(f"VCTK: {spk} tem só {len(members)} falas mic1")
        for m in members[:VCTK_UTTERANCES_PER_SPEAKER]:
            out[m] = f"raw/vctk-0.92/pilot/{spk}/{PurePosixPath(m).name}"
    missing = set(VCTK_TEST_FILES) - {PurePosixPath(v).name for v in out.values()}
    if missing:
        raise ManifestError(f"VCTK: amostras de teste ausentes: {sorted(missing)}")
    return out


def librispeech_select(name: str) -> str | None:
    parts = PurePosixPath(name).parts
    if len(parts) < 2 or parts[0] != "LibriSpeech":
        return None
    rest = PurePosixPath(*parts[1:])
    if len(rest.parts) == 1 and rest.name.upper().endswith(".TXT"):
        return str(rest)
    if rest.parts[0] == "dev-clean" and (rest.name.endswith(".flac") or rest.name.endswith(".trans.txt")):
        return str(rest)
    return None


def record_for_audio(path: Path, data_root: Path, *, source: SourceManifest, speaker: str | None, split: str,
                     codec: str) -> FileRecord:
    audio, sr = sf.read(str(path), dtype="float32", always_2d=True)
    mono = audio.mean(axis=1)
    return FileRecord(
        path=str(Path(path).relative_to(data_root)), sha256=sha256_file(path), source_id=source.id,
        license=source.license, role=source.role.value, speaker=speaker, split=split, sr=int(sr),
        num_samples=int(mono.size), band_hz=effective_band_hz(mono, int(sr)), codec=codec)


def fetch_vctk(paths: Paths) -> list[FileRecord]:
    src = load_sources()["vctk-0.92"]
    selection = select_vctk_members(list_zip_members(src.url))
    digests = fetch_zip_members(src.url, {m: paths.data / rel for m, rel in selection.items()})
    for member, rel in selection.items():
        name = PurePosixPath(rel).name
        if "licen" in name.lower():
            check_license_text((paths.data / rel).read_text(errors="replace"), f"VCTK {member}")
            update_lock(src.id, member, digests[member].sha256, digests[member].size)
        if name in VCTK_TEST_FILES and digests[member].sha256 != VCTK_TEST_FILES[name]:
            raise IntegrityError(f"{name}: SHA-256 difere do registrado pelo Clearcore")
    records = [
        record_for_audio(paths.data / rel, paths.data, source=src, speaker=PurePosixPath(rel).name.split("_")[0],
                         split="test" if "/test/" in rel else "pilot", codec="flac")
        for rel in selection.values() if rel.endswith(".flac")
    ]
    write_file_manifest(records, PILOT_MANIFESTS["vctk-0.92"])
    return records


def _librispeech_md5() -> str | None:
    try:
        r = requests.get(LIBRISPEECH_MD5_URL, timeout=30)
        r.raise_for_status()
    except requests.RequestException:
        return None
    for line in r.text.splitlines():
        parts = line.split()
        if len(parts) == 2 and parts[1] == "dev-clean.tar.gz":
            return parts[0]
    return None


def fetch_librispeech(paths: Paths) -> list[FileRecord]:
    src = load_sources()["librispeech-dev-clean"]
    md5 = _librispeech_md5()
    out_dir = paths.data / "raw" / "librispeech-dev-clean"
    digest, files = stream_extract_tar(src.url, out_dir, librispeech_select, expected_md5=md5,
                                       need_bytes=600 * 1024**2)
    update_lock(src.id, "dev-clean.tar.gz", digest.sha256, digest.size,
                upstream_checksum=f"md5:{md5}" if md5 else None)
    lic = next((f for f in files if "/" not in f and "LICENSE" in f.upper()), None)
    if lic is None:
        raise ManifestError("LibriSpeech: LICENSE.TXT ausente no tar")
    check_license_text((out_dir / lic).read_text(errors="replace"), "LibriSpeech LICENSE.TXT")
    records = [record_for_audio(out_dir / f, paths.data, source=src, speaker=PurePosixPath(f).parts[1],
                                split="pilot", codec="flac") for f in files if f.endswith(".flac")]
    write_file_manifest(records, PILOT_MANIFESTS["librispeech-dev-clean"])
    return records


def fetch_noise(paths: Paths) -> list[FileRecord]:
    src = load_sources()["noise-wikimedia-cc0"]
    out_dir = paths.data / "raw" / "noise-wikimedia-cc0"
    records = []
    for art in src.artifacts:
        raw = out_dir / "_raw" / f"{art['name']}.{art['raw_ext']}"
        headers = {"Range": f"bytes=0-{art['range_bytes'] - 1}"} if art["range_bytes"] else None
        d = stream_download(art["url"], raw, headers=headers, need_bytes=art["range_bytes"] or 64 * 1024**2)
        clip = out_dir / f"{art['name']}.wav"
        cut_clip(raw, clip, start_s=float(art["start_s"]))
        raw.unlink()  # bruto transitório criado por esta função
        matches = sha256_file(clip) == art["clearcore_sha256"]
        update_lock(src.id, raw.name, d.sha256, d.size,
                    upstream_checksum="clearcore-clip:" + ("igual" if matches else "difere (versão do ffmpeg)"))
        records.append(record_for_audio(clip, paths.data, source=src, speaker=None, split="test", codec="wav"))
    write_file_manifest(records, PILOT_MANIFESTS["noise-wikimedia-cc0"])
    return records


def _cv_select(name: str) -> str | None:
    base = PurePosixPath(name).name
    if base.endswith(".mp3"):
        return f"clips/{base}"
    if base.endswith(".tsv"):
        return f"meta/{base}"
    return None


def ingest_cv26(paths: Paths, archive: Path) -> list[FileRecord]:
    src = load_sources()["cv26-ptbr"]
    archive = Path(archive)
    if not archive.is_file():
        raise ManifestError(f"{archive} não existe (download manual pelo dono, após aceitar os termos)")
    out_dir = paths.data / "raw" / "cv26-ptbr"
    digest, _ = stream_extract_tar(archive, out_dir, _cv_select, need_bytes=3 * archive.stat().st_size)
    update_lock(src.id, archive.name, digest.sha256, digest.size)
    tsvs = sorted((out_dir / "meta").glob("*.tsv"), key=lambda p: (p.name != "validated.tsv", p.name))
    table = None
    for tsv in tsvs:
        with open(tsv, newline="") as f:
            reader = csv.DictReader(f, delimiter="\t")
            if reader.fieldnames and {"client_id", "path"} <= set(reader.fieldnames):
                table = list(reader)
                break
    if table is None:
        raise ManifestError("CV: nenhuma tabela .tsv com client_id e path")
    records = []
    for row in table:
        clip = out_dir / "clips" / PurePosixPath(row["path"]).name
        if clip.is_file():
            records.append(record_for_audio(clip, paths.data, source=src, speaker=row["client_id"], split="pilot",
                                            codec="mp3"))
    private = paths.data / "manifests" / "cv26-ptbr.jsonl"
    digest_manifest = write_file_manifest(records, private)
    (FILES_DIR / "cv26-ptbr.sha256").write_text(f"{digest_manifest}  data/manifests/cv26-ptbr.jsonl\n")
    return records


def duration_hours(records: Iterable[FileRecord]) -> float:
    return float(np.sum([r.num_samples / r.sr for r in records]) / 3600.0)
```

`scripts/fetch_pilot.py`:

```python
"""Busca os dados do piloto do M0. Uso: fetch_pilot.py {vctk,librispeech,noise,cv26,all}."""

import os
import sys
from pathlib import Path

from cctrain.data import pilot
from cctrain.paths import Paths


def main(which: str) -> None:
    paths = Paths.from_env()
    steps = {"vctk": pilot.fetch_vctk, "librispeech": pilot.fetch_librispeech, "noise": pilot.fetch_noise}
    for name, fn in steps.items():
        if which in (name, "all"):
            recs = fn(paths)
            print(f"{name}: {len(recs)} arquivos, {pilot.duration_hours(recs):.2f} h")
    if which in ("cv26", "all"):
        archive = os.environ.get("CCTRAIN_CV26_ARCHIVE")
        if not archive:
            print("cv26: NÃO EXECUTADO (CCTRAIN_CV26_ARCHIVE ausente; aceite dos termos pendente com o dono)")
        else:
            recs = pilot.ingest_cv26(paths, Path(archive))
            print(f"cv26: {len(recs)} clipes, {len({r.speaker for r in recs})} client_id, "
                  f"{pilot.duration_hours(recs):.2f} h")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "all")
```

- [ ] **Step 4: Rodar os testes unitários**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" -m pytest tests/test_data_band.py tests/test_data_pilot.py -v
"$CCTRAIN_PY" -m ruff check src tests scripts
```

Expected: todos PASS.

- [ ] **Step 5: Buscar os dados (rede; um bruto por vez)**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
df -h "$CCTRAIN_DATA_DIR" | tail -1
"$CCTRAIN_PY" scripts/fetch_pilot.py vctk
"$CCTRAIN_PY" scripts/fetch_pilot.py librispeech
"$CCTRAIN_PY" scripts/fetch_pilot.py noise
"$CCTRAIN_PY" scripts/fetch_pilot.py cv26
du -sh "$CCTRAIN_DATA_DIR"/raw/*
```

Expected: `vctk: 406 arquivos, ~0.3 h` (400 do piloto + 6 de teste); `librispeech: ~2703 arquivos, ~5.4 h`; `noise: 3 arquivos, 0.01 h`; `cv26: NÃO EXECUTADO ...` a menos que o dono tenha exportado `CCTRAIN_CV26_ARCHIVE`. Falhas de licença (`check_license_text`) ou de hash param a tarefa: reporte, não contorne.

- [ ] **Step 6: Rodar a verificação de integridade**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" -m pytest tests/test_data_pilot_integrity.py -v -m data
```

Expected: todos PASS.

- [ ] **Step 7: Commit**

```bash
cd "$CCTRAIN_WT"
git add src/cctrain/data/band.py src/cctrain/data/pilot.py scripts/fetch_pilot.py manifests tests/test_data_band.py tests/test_data_pilot.py tests/test_data_pilot_integrity.py
git status --short | grep -q "cv26-ptbr.jsonl" && { echo "ERRO: manifest do CV no índice"; exit 1; }
git commit -m "feat(data): fetch M0 pilot data with license/hash checks and per-file manifests"
```

---
### Task 13: Áudio, features (paridade libdf), misturas e loader do piloto

**Files:**
- Create: `src/cctrain/data/audio.py`, `src/cctrain/data/features.py`, `src/cctrain/data/mix.py`, `src/cctrain/data/loader.py`
- Test: `tests/test_data_features.py`, `tests/test_data_mix.py`, `tests/test_data_loader.py`

**Interfaces:**
- Consumes: `cctrain.data.manifest` (`FileRecord`, `load_sources`, `training_records`, `read_file_manifest`, `FILES_DIR`), `cctrain.data.licenses` (Task 3); `cctrain.dfvendor` (Task 1).
- Produces:
  - `cctrain.data.audio`: `load_audio(path: Path) -> tuple[np.ndarray, int]` (mono f32); `resample(x, sr_in: int, sr_out: int) -> np.ndarray` (soxr HQ).
  - `cctrain.data.features`: constantes `SR=48000, FFT=960, HOP=480, NB_ERB=32, NB_DF=96, MIN_NB_ERB_FREQS=2, NORM_ALPHA=0.99`; `Features(spec [1,T,481,2], feat_erb [1,T,32], feat_spec [1,T,96,2])`; `extract_features(audio_48k: np.ndarray) -> Features`; `analysis(audio_48k) -> np.ndarray` (`[1,T,481,2]`).
  - `cctrain.data.mix`: `MixKind(IntEnum)` = `TARGET_NOISE, TARGET_INTERFERER, TARGET_INTERFERER_NOISE, INTERFERER_ONLY`; `SNRS_DB = (-5, 0, 5, 10, 20, 40)`; `SIR_RANGE_DB = (-5.0, 25.0)`; `mixture_fractions(s: float = 0.05) -> dict[MixKind, float]`; `power(x) -> float`; `scale_for_ratio(ref, other, ratio_db) -> np.ndarray`; `apply_rir(x, h) -> np.ndarray`; `crop_or_pad(x, n, rng)`; `loop_to_length(x, n, rng)`; `MixResult(noisy, clean, kind, snr_db, sir_db)`; `make_mixture(target, interferers, noise, *, kind, snr_db, sir_db, rir_target=None, rir_interferer=None) -> MixResult`.
  - `cctrain.data.loader`: `PilotLoaderConfig(segment_s=3.0, silence_fraction=0.05, p_rir=0.3, p_two_interferers=0.5, seed=20261003)`; `PilotMixDataset(speech, noise, rirs, data_root, cfg: PilotLoaderConfig | None = None, sources=None)` (`IterableDataset`; itens com `noisy [N]`, `clean [N]`, `spec [1,T,481,2]`, `spec_clean [1,T,481,2]`, `feat_erb [1,T,32]`, `feat_spec [1,T,96,2]`, `kind`, `snr_db`, `sir_db`); `pilot_records(data_root: Path, librispeech_speakers: int = 20) -> tuple[list[FileRecord], list[FileRecord], list[FileRecord]]` (fala ~1 h, ruído, RIR).

- [ ] **Step 1: Escrever os testes que falham**

`tests/test_data_features.py`:

```python
import numpy as np
import torch

from cctrain.data.features import NORM_ALPHA, extract_features
from cctrain.dfvendor import ensure_df_importable, ensure_dfnet3_checkpoint


def test_features_are_identical_to_upstream_df_features():
    ensure_df_importable()
    from df.enhance import df_features, init_df
    from df.utils import get_norm_alpha

    _, df_state, *_ = init_df(str(ensure_dfnet3_checkpoint()), post_filter=False, log_level="ERROR",
                              log_file=None, config_allow_defaults=True, epoch="best")
    assert get_norm_alpha(False) == NORM_ALPHA
    x = (0.1 * np.random.default_rng(0).standard_normal(48_000 * 2)).astype(np.float32)
    spec, erb, specf = df_features(torch.from_numpy(x)[None], df_state, 96, device="cpu")
    ours = extract_features(x)
    assert np.array_equal(ours.spec, spec[:, 0].numpy())
    assert np.array_equal(ours.feat_erb, erb[:, 0].numpy())
    assert np.array_equal(ours.feat_spec, specf[:, 0].numpy())
    assert ours.spec.shape == (1, 200, 481, 2)
```

`tests/test_data_mix.py`:

```python
import numpy as np
import pytest

from cctrain.data.mix import MixKind, make_mixture, mixture_fractions, power, scale_for_ratio


@pytest.mark.parametrize("s", [0.0, 0.05, 0.10])
def test_fractions_follow_spec(s):
    f = mixture_fractions(s)
    assert abs(sum(f.values()) - 1.0) < 1e-12
    assert f[MixKind.INTERFERER_ONLY] == s


def test_fractions_with_ten_percent_silence():
    f = mixture_fractions(0.10)
    assert [round(f[k], 10) for k in MixKind] == [0.35, 0.20, 0.35, 0.10]


def test_scale_for_ratio_hits_target():
    rng = np.random.default_rng(0)
    a, b = rng.standard_normal(48_000), 3 * rng.standard_normal(48_000)
    b2 = scale_for_ratio(a, b, 7.5)
    assert abs(10 * np.log10(power(a) / power(b2)) - 7.5) < 1e-9


def test_mixture_kinds_and_targets():
    rng = np.random.default_rng(1)
    t, i, n = (0.1 * rng.standard_normal(48_000) for _ in range(3))
    io = make_mixture(t, [i], n, kind=MixKind.INTERFERER_ONLY, snr_db=10.0, sir_db=float("nan"))
    assert not np.any(io.clean)
    tn = make_mixture(t, [], n, kind=MixKind.TARGET_NOISE, snr_db=5.0, sir_db=float("nan"))
    measured = 10 * np.log10(power(tn.clean) / power(tn.noisy - tn.clean))
    assert abs(measured - 5.0) < 1e-4
    loud = make_mixture(10 * t, [10 * i], 10 * n, kind=MixKind.TARGET_INTERFERER_NOISE, snr_db=0.0, sir_db=0.0)
    assert np.max(np.abs(loud.noisy)) <= 0.99 + 1e-6 and np.max(np.abs(loud.clean)) <= 0.99 + 1e-6
```

`tests/test_data_loader.py`:

```python
import numpy as np
import pytest
import soundfile as sf

from cctrain.data.licenses import LicenseRefused
from cctrain.data.loader import PilotLoaderConfig, PilotMixDataset
from cctrain.data.manifest import FileRecord
from cctrain.hashing import sha256_file


def _write(root, rel, x, sr, source, lic, speaker, split, role="train"):
    p = root / rel
    p.parent.mkdir(parents=True, exist_ok=True)
    sf.write(p, x.astype("f4"), sr, subtype="PCM_16")
    return FileRecord(path=rel, sha256=sha256_file(p), source_id=source, license=lic, role=role, speaker=speaker,
                      split=split, sr=sr, num_samples=x.size, band_hz=None, codec="flac")


@pytest.fixture()
def corpus(tmp_path):
    rng = np.random.default_rng(0)
    speech = [_write(tmp_path, f"s/{spk}_{k}.flac", 0.1 * rng.standard_normal(sr * 2), sr, "vctk-0.92",
                     "CC-BY-4.0", spk, "pilot") for spk, sr in (("a", 48_000), ("b", 16_000), ("c", 48_000))
              for k in range(2)]
    noise = [_write(tmp_path, "n/n.flac", 0.05 * rng.standard_normal(48_000 * 4), 48_000, "noise-wikimedia-cc0",
                    "CC0-1.0", None, "test")]
    h = np.zeros(4800)
    h[0], h[100] = 0.99, 0.3
    rirs = [_write(tmp_path, "r/r.flac", h, 48_000, "rir-synthetic-v1", "LicenseRef-cctrain-generated", None,
                   "rir")]
    return tmp_path, speech, noise, rirs


def test_items_have_dfnet_shapes(corpus):
    root, speech, noise, rirs = corpus
    it = iter(PilotMixDataset(speech, noise, rirs, root, PilotLoaderConfig(segment_s=1.0)))
    item = next(it)
    assert item["noisy"].shape == (48_000,) and item["clean"].shape == (48_000,)
    assert item["spec"].shape == (1, 100, 481, 2)
    assert item["spec_clean"].shape == (1, 100, 481, 2)
    assert item["feat_erb"].shape == (1, 100, 32)
    assert item["feat_spec"].shape == (1, 100, 96, 2)
    assert np.isfinite(item["noisy"]).all()


def test_dataset_is_deterministic(corpus):
    root, speech, noise, rirs = corpus
    cfg = PilotLoaderConfig(segment_s=1.0, seed=7)
    a = [next(iter(PilotMixDataset(speech, noise, rirs, root, cfg)))["noisy"] for _ in range(2)]
    assert np.array_equal(a[0], a[1])


def test_dataset_refuses_test_only_or_single_speaker(corpus):
    root, speech, noise, rirs = corpus
    bad = [FileRecord(**{**speech[0].to_dict(), "role": "test_only"})]
    with pytest.raises(LicenseRefused):
        PilotMixDataset(bad + speech, noise, rirs, root)
    with pytest.raises(ValueError, match="falantes"):
        PilotMixDataset([r for r in speech if r.speaker == "a"], noise, rirs, root)
```

- [ ] **Step 2: Rodar e ver falhar**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" -m pytest tests/test_data_features.py tests/test_data_mix.py tests/test_data_loader.py -v 2>&1 | tail -3
```

Expected: FAIL com `ImportError`.

- [ ] **Step 3: Implementar `audio.py` e `features.py`**

`src/cctrain/data/audio.py`:

```python
"""Leitura de áudio (FLAC/WAV/MP3 via libsndfile) e reamostragem soxr HQ para o loader (spec §4.3)."""

from __future__ import annotations

from pathlib import Path

import numpy as np
import soundfile as sf
import soxr


def load_audio(path: Path) -> tuple[np.ndarray, int]:
    data, sr = sf.read(str(path), dtype="float32", always_2d=True)
    return data.mean(axis=1).astype(np.float32), int(sr)


def resample(x: np.ndarray, sr_in: int, sr_out: int) -> np.ndarray:
    if sr_in == sr_out:
        return np.asarray(x, dtype=np.float32)
    return soxr.resample(np.asarray(x, dtype=np.float32), sr_in, sr_out, quality="HQ").astype(np.float32)
```

`src/cctrain/data/features.py`:

```python
"""Features do DFNet3 pelo libdf (deepfilterlib 0.5.6), idênticas a df.enhance.df_features (spec §5.1)."""

from __future__ import annotations

from dataclasses import dataclass

import numpy as np
from libdf import DF, erb, erb_norm, unit_norm

SR = 48_000
FFT = 960
HOP = 480
NB_ERB = 32
NB_DF = 96
MIN_NB_ERB_FREQS = 2
NORM_ALPHA = 0.99  # calc_norm_alpha(48000, 480, tau=1) arredondado como no libDF


@dataclass(frozen=True)
class Features:
    spec: np.ndarray
    feat_erb: np.ndarray
    feat_spec: np.ndarray


def _as_real(c: np.ndarray) -> np.ndarray:
    return np.stack([c.real, c.imag], axis=-1).astype(np.float32)


def _state() -> DF:
    return DF(SR, FFT, HOP, NB_ERB, MIN_NB_ERB_FREQS)


def analysis(audio_48k: np.ndarray) -> np.ndarray:
    return _as_real(_state().analysis(np.ascontiguousarray(audio_48k, dtype=np.float32)[None]))


def extract_features(audio_48k: np.ndarray) -> Features:
    state = _state()
    spec = state.analysis(np.ascontiguousarray(audio_48k, dtype=np.float32)[None])
    feat_erb = np.asarray(erb_norm(erb(spec, state.erb_widths()), NORM_ALPHA), dtype=np.float32)
    feat_spec = unit_norm(np.ascontiguousarray(spec[..., :NB_DF]), NORM_ALPHA)
    return Features(_as_real(spec), feat_erb, _as_real(np.asarray(feat_spec)))
```

- [ ] **Step 4: Implementar `mix.py` e `loader.py`**

`src/cctrain/data/mix.py`:

```python
"""Receita das misturas (spec §4.4), versão do piloto: tipos, frações, SNR/SIR, RIR e alvo seco."""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass
from enum import IntEnum

import numpy as np
from scipy.signal import fftconvolve

SNRS_DB = (-5, 0, 5, 10, 20, 40)
SIR_RANGE_DB = (-5.0, 25.0)
INTERFERER_ONLY_RMS_DBFS = -25.0
PEAK = 0.99


class MixKind(IntEnum):
    TARGET_NOISE = 0
    TARGET_INTERFERER = 1
    TARGET_INTERFERER_NOISE = 2
    INTERFERER_ONLY = 3


def mixture_fractions(s: float = 0.05) -> dict[MixKind, float]:
    if not 0.0 <= s < 1.0:
        raise ValueError("fração de silêncio fora de [0, 1)")
    k = (1.0 - s) / 0.9
    return {MixKind.TARGET_NOISE: 0.35 * k, MixKind.TARGET_INTERFERER: 0.20 * k,
            MixKind.TARGET_INTERFERER_NOISE: 0.35 * k, MixKind.INTERFERER_ONLY: s}


def power(x: np.ndarray) -> float:
    return float(np.mean(np.asarray(x, dtype=np.float64) ** 2)) + 1e-20


def scale_for_ratio(ref: np.ndarray, other: np.ndarray, ratio_db: float) -> np.ndarray:
    gain = np.sqrt(power(ref) / (power(other) * 10.0 ** (ratio_db / 10.0)))
    return (np.asarray(other, dtype=np.float64) * gain).astype(np.float32)


def apply_rir(x: np.ndarray, h: np.ndarray) -> np.ndarray:
    return fftconvolve(np.asarray(x, np.float64), np.asarray(h, np.float64))[: len(x)].astype(np.float32)


def crop_or_pad(x: np.ndarray, n: int, rng: np.random.Generator) -> np.ndarray:
    if len(x) >= n:
        start = int(rng.integers(0, len(x) - n + 1))
        return np.asarray(x[start : start + n], dtype=np.float32)
    return np.pad(np.asarray(x, np.float32), (0, n - len(x)))


def loop_to_length(x: np.ndarray, n: int, rng: np.random.Generator) -> np.ndarray:
    reps = int(np.ceil(n / len(x))) + 1
    return crop_or_pad(np.tile(np.asarray(x, np.float32), reps), n, rng)


@dataclass(frozen=True)
class MixResult:
    noisy: np.ndarray
    clean: np.ndarray
    kind: MixKind
    snr_db: float
    sir_db: float


def make_mixture(target: np.ndarray, interferers: Sequence[np.ndarray], noise: np.ndarray | None, *,
                 kind: MixKind, snr_db: float, sir_db: float, rir_target: np.ndarray | None = None,
                 rir_interferer: np.ndarray | None = None) -> MixResult:
    n = len(target)
    has_target = kind is not MixKind.INTERFERER_ONLY
    clean = np.asarray(target, np.float32) if has_target else np.zeros(n, np.float32)
    wet = apply_rir(clean, rir_target) if (has_target and rir_target is not None) else clean
    speech = wet.astype(np.float64)
    if kind in (MixKind.TARGET_INTERFERER, MixKind.TARGET_INTERFERER_NOISE, MixKind.INTERFERER_ONLY):
        interf = np.zeros(n)
        for x in interferers:
            interf += apply_rir(x, rir_interferer) if rir_interferer is not None else np.asarray(x, np.float64)
        if has_target:
            interf = scale_for_ratio(wet, interf, sir_db)
        else:
            ref = np.full(n, 10 ** (INTERFERER_ONLY_RMS_DBFS / 20.0))
            interf = scale_for_ratio(ref, interf, 0.0)
        speech = speech + interf
    noisy = speech
    if noise is not None and kind in (MixKind.TARGET_NOISE, MixKind.TARGET_INTERFERER_NOISE,
                                      MixKind.INTERFERER_ONLY):
        noisy = speech + scale_for_ratio(speech, noise, snr_db)
    gain = min(1.0, PEAK / max(float(np.max(np.abs(noisy))), 1e-9), PEAK / max(float(np.max(np.abs(clean))), 1e-9))
    return MixResult((noisy * gain).astype(np.float32), (clean * gain).astype(np.float32), kind, snr_db, sir_db)
```

`src/cctrain/data/loader.py`:

```python
"""Loader do piloto (IterableDataset): mistura em CPU e features libdf por amostra (spec §5.1).

Só aceita registros na allowlist de treino. Usado no M0 para medir amostras/s e RSS; a receita completa
(enrollment, máscara de perfil, banco de embeddings) é do M2.
"""

from __future__ import annotations

from collections import defaultdict
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path

import numpy as np
from torch.utils.data import IterableDataset, get_worker_info

from cctrain.data.audio import load_audio, resample
from cctrain.data.features import SR, analysis, extract_features
from cctrain.data.manifest import FILES_DIR, FileRecord, SourceManifest, load_sources, read_file_manifest, training_records
from cctrain.data.mix import SIR_RANGE_DB, SNRS_DB, MixKind, crop_or_pad, loop_to_length, make_mixture, mixture_fractions


@dataclass(frozen=True)
class PilotLoaderConfig:
    segment_s: float = 3.0
    silence_fraction: float = 0.05
    p_rir: float = 0.3
    p_two_interferers: float = 0.5
    seed: int = 20261003


class PilotMixDataset(IterableDataset):
    def __init__(self, speech: Sequence[FileRecord], noise: Sequence[FileRecord], rirs: Sequence[FileRecord],
                 data_root: Path, cfg: PilotLoaderConfig | None = None,
                 sources: Mapping[str, SourceManifest] | None = None):
        super().__init__()
        sources = sources or load_sources()
        self.speech = training_records(speech, sources)
        self.noise = training_records(noise, sources)
        self.rirs = training_records(rirs, sources)
        self.by_speaker: dict[str, list[FileRecord]] = defaultdict(list)
        for r in self.speech:
            self.by_speaker[r.speaker].append(r)
        if len(self.by_speaker) < 2:
            raise ValueError("o piloto precisa de pelo menos 2 falantes")
        if not self.noise:
            raise ValueError("sem ruído")
        self.speakers = sorted(self.by_speaker)
        self.data_root = Path(data_root)
        self.cfg = cfg = cfg or PilotLoaderConfig()
        fr = mixture_fractions(cfg.silence_fraction)
        self.kinds = list(fr)
        self.kind_p = np.array([fr[k] for k in self.kinds])

    def _load48(self, r: FileRecord) -> np.ndarray:
        x, sr = load_audio(self.data_root / r.path)
        return resample(x, sr, SR)

    def _sample(self, rng: np.random.Generator) -> dict:
        n = int(self.cfg.segment_s * SR)
        kind = self.kinds[int(rng.choice(len(self.kinds), p=self.kind_p))]
        spk = self.speakers[int(rng.integers(len(self.speakers)))]
        target = crop_or_pad(self._load48(self.by_speaker[spk][int(rng.integers(len(self.by_speaker[spk])))]), n, rng)
        others = [s for s in self.speakers if s != spk]
        interferers = []
        if kind is not MixKind.TARGET_NOISE:
            count = 2 if (rng.random() < self.cfg.p_two_interferers and len(others) > 1) else 1
            for s in rng.choice(others, size=count, replace=False):
                recs = self.by_speaker[str(s)]
                interferers.append(crop_or_pad(self._load48(recs[int(rng.integers(len(recs)))]), n, rng))
        noise = loop_to_length(self._load48(self.noise[int(rng.integers(len(self.noise)))]), n, rng)
        if kind is MixKind.INTERFERER_ONLY and rng.random() < 0.5:
            noise = None
        def rir():
            if self.rirs and rng.random() < self.cfg.p_rir:
                return self._load48(self.rirs[int(rng.integers(len(self.rirs)))])
            return None
        snr = float(SNRS_DB[int(rng.integers(len(SNRS_DB)))])
        sir = float(rng.uniform(*SIR_RANGE_DB)) if interferers else float("nan")
        mix = make_mixture(target, interferers, noise, kind=kind, snr_db=snr, sir_db=sir, rir_target=rir(),
                           rir_interferer=rir())
        feats = extract_features(mix.noisy)
        return {"noisy": mix.noisy, "clean": mix.clean, "spec": feats.spec, "spec_clean": analysis(mix.clean),
                "feat_erb": feats.feat_erb, "feat_spec": feats.feat_spec, "kind": int(kind), "snr_db": snr,
                "sir_db": sir}

    def __iter__(self):
        info = get_worker_info()
        rng = np.random.default_rng([self.cfg.seed, info.id if info else 0])
        while True:
            yield self._sample(rng)


def pilot_records(data_root: Path, librispeech_speakers: int = 20) -> tuple[list[FileRecord], list[FileRecord], list[FileRecord]]:
    vctk = [r for r in read_file_manifest(FILES_DIR / "vctk-0.92-pilot.jsonl") if r.split == "pilot"]
    libri = read_file_manifest(FILES_DIR / "librispeech-dev-clean-pilot.jsonl")
    keep = sorted({r.speaker for r in libri})[:librispeech_speakers]
    speech = vctk + [r for r in libri if r.speaker in keep]
    cv_manifest = Path(data_root) / "manifests" / "cv26-ptbr.jsonl"
    if cv_manifest.is_file():
        speech += read_file_manifest(cv_manifest)
    noise = read_file_manifest(FILES_DIR / "noise-wikimedia-cc0-pilot.jsonl")
    rirs = read_file_manifest(FILES_DIR / "rir-synthetic-v1-pilot.jsonl")
    return speech, noise, rirs
```

- [ ] **Step 5: Rodar os testes até passarem**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" -m pytest tests/test_data_features.py tests/test_data_mix.py tests/test_data_loader.py -v
"$CCTRAIN_PY" -m ruff check src tests
```

Expected: todos PASS. Se `test_features_are_identical_to_upstream_df_features` falhar por diferença numérica, compare `df.enhance.df_features` com `extract_features` campo a campo e alinhe a chamada (ex.: o upstream passar fatia não contígua ao `unit_norm`): a exigência é igualdade exata.

- [ ] **Step 6: Commit**

```bash
cd "$CCTRAIN_WT"
git add src/cctrain/data/audio.py src/cctrain/data/features.py src/cctrain/data/mix.py src/cctrain/data/loader.py tests/test_data_features.py tests/test_data_mix.py tests/test_data_loader.py
git commit -m "feat(data): libdf-identical features, pilot mixture recipe and IterableDataset loader"
```

---
### Task 14: `tract-check gating` e `tract-check enroll`

**Files:**
- Create: `tools/tract-check/src/gating.rs`, `tools/tract-check/src/enroll.rs`
- Modify: `tools/tract-check/src/main.rs` (dois braços novos no `match` e no texto de uso)
- Test: `tests/test_tractcheck_gating.py` (marcadores `tract`, `clearcore`), `tests/test_tractcheck_enroll.py` (marcador `tract`); testes unitários Rust em `gating.rs`

**Interfaces:**
- Consumes: módulo Rust `common` (`HOP`, `load_model`, `process_frame`, `read_f32_le`, `write_f32_le`, `sha256_hex`, `to_le_bytes`) e `cctrain.tractcheck.run` (Task 2).
- Produces:
  - `tract-check gating <asset.tar.gz> <in_48k.f32> <out_prefix> [film.f32]`: grava `<prefix>.out.f32` (saída, `quadros × 480`), `<prefix>.lsnr.f32` (um `lsnr` por quadro), `<prefix>.stage.u8` (0 = máscara zero, 1 = só ERB, 2 = ERB+DF, 3 = sem processamento, 4 = silêncio, atalho `média quadrática < 1e-7`) e `<prefix>.json` (frações por estágio). `film.f32` = `gamma_enc, beta_enc, gamma_df, beta_df` concatenados (4 × 256 f32 LE).
  - `tract-check enroll <model.onnx> <in_16k.f32> <out.json>`: carrega como `enrollment.rs:418-435` (entrada 0 fixada em `f32 [1, N]`, `into_optimized`, `into_runnable`), grava `{"n", "ok", "outputs": {nome: [...]}}` ou `{"n", "ok": false, "error"}`; código 0 se rodou, 1 se o tract recusou.

- [ ] **Step 1: Escrever os testes que falham**

`tests/test_tractcheck_gating.py`:

```python
import json

import numpy as np
import pytest

from cctrain import tractcheck
from cctrain.paths import clearcore_root

pytestmark = [pytest.mark.tract, pytest.mark.clearcore]


def _signal(frames: int = 300) -> np.ndarray:
    t = np.arange(frames * 480) / 48_000.0
    rng = np.random.default_rng(0)
    voiced = sum(np.sin(2 * np.pi * 140 * h * t) / h for h in range(1, 12))
    x = 0.08 * (0.5 + 0.5 * np.sin(2 * np.pi * 2 * t)) * voiced + 0.01 * rng.standard_normal(t.size)
    x[: 50 * 480] = 0.0
    return x.astype("<f4")


def _run(tmp_path, name: str):
    x = _signal()
    inp = tmp_path / "in.f32"
    x.tofile(inp)
    asset = clearcore_root() / "vendor/approved/df-compatible-release-asset-v1.bin"
    prefix = tmp_path / name
    proc = tractcheck.run("gating", asset, inp, prefix)
    assert proc.returncode == 0, proc.stdout + proc.stderr
    out = np.fromfile(f"{prefix}.out.f32", dtype="<f4")
    lsnr = np.fromfile(f"{prefix}.lsnr.f32", dtype="<f4")
    stages = np.fromfile(f"{prefix}.stage.u8", dtype=np.uint8)
    return x, out, lsnr, stages, json.loads(open(f"{prefix}.json").read())


def test_gating_outputs_are_consistent(tmp_path):
    x, out, lsnr, stages, report = _run(tmp_path, "a")
    assert out.size == x.size and lsnr.size == stages.size == 300
    assert set(np.unique(stages)) <= {0, 1, 2, 3, 4}
    assert np.all(stages[:50] == 4) and np.all(lsnr[:50] == -15.0) and not np.any(out[:50 * 480])
    assert abs(sum(report["fractions"].values()) - 1.0) < 1e-9
    assert np.isfinite(out).all()


def test_gating_is_deterministic(tmp_path):
    a = _run(tmp_path, "a")
    b = _run(tmp_path, "b")
    assert np.array_equal(a[1], b[1]) and np.array_equal(a[3], b[3])
```

`tests/test_tractcheck_enroll.py`:

```python
import json

import numpy as np
import onnx
import pytest
from onnx import TensorProto, helper

from cctrain import tractcheck

pytestmark = pytest.mark.tract


def _tiny_onnx(path):
    graph = helper.make_graph(
        [helper.make_node("Mul", ["audio", "two"], ["doubled"]),
         helper.make_node("ReduceMean", ["audio"], ["mean"], keepdims=0)],
        "tiny",
        [helper.make_tensor_value_info("audio", TensorProto.FLOAT, [1, "N"])],
        [helper.make_tensor_value_info("doubled", TensorProto.FLOAT, [1, "N"]),
         helper.make_tensor_value_info("mean", TensorProto.FLOAT, [])],
        [helper.make_tensor("two", TensorProto.FLOAT, [], [2.0])],
    )
    model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 13)])
    model.ir_version = 8
    onnx.checker.check_model(model)
    onnx.save(model, str(path))


def test_enroll_runs_a_dynamic_graph_like_the_runtime(tmp_path):
    _tiny_onnx(tmp_path / "tiny.onnx")
    x = np.linspace(-0.5, 0.5, 32_000, dtype="<f4")
    x.tofile(tmp_path / "in.f32")
    proc = tractcheck.run("enroll", tmp_path / "tiny.onnx", tmp_path / "in.f32", tmp_path / "out.json")
    assert proc.returncode == 0, proc.stdout + proc.stderr
    rep = json.loads((tmp_path / "out.json").read_text())
    assert rep["ok"] is True and rep["n"] == 32_000
    np.testing.assert_allclose(rep["outputs"]["doubled"], 2 * x, rtol=0, atol=1e-7)
    assert abs(rep["outputs"]["mean"][0] - float(x.mean())) < 1e-6


def test_enroll_reports_unloadable_model(tmp_path):
    (tmp_path / "bad.onnx").write_bytes(b"not an onnx file")
    np.zeros(32_000, dtype="<f4").tofile(tmp_path / "in.f32")
    proc = tractcheck.run("enroll", tmp_path / "bad.onnx", tmp_path / "in.f32", tmp_path / "out.json")
    assert proc.returncode == 1
    assert json.loads((tmp_path / "out.json").read_text())["ok"] is False
```

- [ ] **Step 2: Rodar e ver falhar**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" -m pytest tests/test_tractcheck_gating.py tests/test_tractcheck_enroll.py -v 2>&1 | tail -3
```

Expected: FAIL (o binário responde `uso: ...` com código 2).

- [ ] **Step 3: Implementar `gating.rs`**

`tools/tract-check/src/gating.rs`:

```rust
//! Caminho do runtime com gating por lsnr: `DfTract::process` quadro a quadro (spec §3.5).
use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{bail, Context, Result};
use deep_filter::tract::{FilmVectors, RuntimeParams};
use serde_json::json;

use crate::common::{load_model, process_frame, read_f32_le, sha256_hex, to_le_bytes, write_f32_le, HOP};

pub const STAGE_ZERO: u8 = 0;
pub const STAGE_ERB_ONLY: u8 = 1;
pub const STAGE_ERB_DF: u8 = 2;
pub const STAGE_NONE: u8 = 3;
pub const STAGE_SILENT: u8 = 4;

pub fn stage_code(stages: (bool, bool, bool)) -> u8 {
    match stages {
        (false, true, false) => STAGE_ZERO,
        (true, false, false) => STAGE_ERB_ONLY,
        (true, false, true) => STAGE_ERB_DF,
        (false, false, false) => STAGE_NONE,
        _ => u8::MAX,
    }
}

/// Mesma conta de `DfTract::process_with_spec_hook`: soma sequencial de x² em f32, dividida pelo comprimento.
#[allow(clippy::cast_precision_loss)]
pub fn mean_square(frame: &[f32]) -> f32 {
    frame.iter().fold(0f32, |acc, x| acc + x.powi(2)) / frame.len() as f32
}

fn read_film(path: &Path, hidden: usize) -> Result<FilmVectors> {
    let v = read_f32_le(path)?;
    if v.len() != 4 * hidden {
        bail!("{}: esperados {} valores f32, lidos {}", path.display(), 4 * hidden, v.len());
    }
    Ok(FilmVectors {
        gamma_enc: v[..hidden].to_vec(),
        beta_enc: v[hidden..2 * hidden].to_vec(),
        gamma_df: v[2 * hidden..3 * hidden].to_vec(),
        beta_df: v[3 * hidden..].to_vec(),
    })
}

fn stage_name(code: u8) -> &'static str {
    match code {
        STAGE_ZERO => "zero",
        STAGE_ERB_ONLY => "erb_only",
        STAGE_ERB_DF => "erb_df",
        STAGE_NONE => "none",
        STAGE_SILENT => "silent",
        _ => "invalid",
    }
}

#[allow(clippy::cast_precision_loss)]
pub fn run(args: &[String]) -> Result<bool> {
    let (asset, input, prefix, film) = match args {
        [a, i, p] => (a, i, p, None),
        [a, i, p, f] => (a, i, p, Some(f)),
        _ => bail!("uso: tract-check gating <asset.tar.gz> <in_48k.f32> <out_prefix> [film.f32]"),
    };
    let signal = read_f32_le(Path::new(input))?;
    let frames = signal.len() / HOP;
    if frames == 0 {
        bail!("entrada com menos de um hop");
    }
    let mut model = load_model(Path::new(asset), &RuntimeParams::default())?;
    if let Some(f) = film {
        let hidden = model.film_hidden().context("asset sem entradas FiLM")?;
        model.set_film(&read_film(Path::new(f), hidden)?)?;
    }
    let mut out = Vec::with_capacity(frames * HOP);
    let mut lsnr = Vec::with_capacity(frames);
    let mut stages = Vec::with_capacity(frames);
    for frame in signal.chunks_exact(HOP) {
        let silent = mean_square(frame) < 1e-7;
        let (y, l) = process_frame(&mut model, frame)?;
        stages.push(if silent { STAGE_SILENT } else { stage_code(model.apply_stages(l)) });
        lsnr.push(l);
        out.extend(y);
    }
    write_f32_le(Path::new(&format!("{prefix}.out.f32")), &out)?;
    write_f32_le(Path::new(&format!("{prefix}.lsnr.f32")), &lsnr)?;
    fs::write(format!("{prefix}.stage.u8"), &stages)?;
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for s in &stages {
        *counts.entry(stage_name(*s)).or_default() += 1;
    }
    let fractions: BTreeMap<&str, f64> =
        counts.iter().map(|(k, v)| (*k, *v as f64 / frames as f64)).collect();
    let report = json!({
        "frames": frames,
        "fractions": fractions,
        "out_sha256": sha256_hex(&to_le_bytes(&out)),
        "film": film.is_some(),
    });
    fs::write(format!("{prefix}.json"), serde_json::to_string_pretty(&report)? + "\n")?;
    println!("{report}");
    Ok(!counts.contains_key("invalid"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_codes_follow_apply_stages() {
        assert_eq!(stage_code((false, true, false)), STAGE_ZERO);
        assert_eq!(stage_code((true, false, false)), STAGE_ERB_ONLY);
        assert_eq!(stage_code((true, false, true)), STAGE_ERB_DF);
        assert_eq!(stage_code((false, false, false)), STAGE_NONE);
        assert_eq!(stage_code((true, true, true)), u8::MAX);
    }

    #[test]
    fn mean_square_matches_definition() {
        assert_eq!(mean_square(&[0.0; 480]), 0.0);
        assert_eq!(mean_square(&[2.0, 0.0]), 2.0);
    }
}
```

- [ ] **Step 4: Implementar `enroll.rs` e ligar no `main.rs`**

`tools/tract-check/src/enroll.rs`:

```rust
//! Carga do enrollment como `crates/model/src/enrollment.rs:418-435`: entrada 0 fixada em f32 [1, N].
use std::{collections::BTreeMap, fs, io::Cursor, path::Path};

use anyhow::{bail, Result};
use serde_json::json;
use tract_onnx::prelude::*;

use crate::common::read_f32_le;

fn extract(bytes: &[u8], audio: &[f32]) -> TractResult<BTreeMap<String, Vec<f32>>> {
    let mut model = onnx().model_for_read(&mut Cursor::new(bytes))?;
    let mut names = Vec::new();
    for outlet in model.output_outlets()?.to_vec() {
        names.push(model.outlet_label(outlet).map(str::to_owned));
    }
    model.set_input_fact(0, InferenceFact::dt_shape(f32::datum_type(), tvec!(1, audio.len())))?;
    let runnable = model.into_optimized()?.into_runnable()?;
    let input = Tensor::from_shape(&[1, audio.len()], audio)?;
    let outputs = runnable.run(tvec!(input.into()))?;
    let mut map = BTreeMap::new();
    for (name, value) in names.iter().zip(outputs.iter()) {
        if let Some(name) = name {
            map.insert(name.clone(), value.as_slice::<f32>()?.to_vec());
        }
    }
    Ok(map)
}

pub fn run(args: &[String]) -> Result<bool> {
    let [onnx_path, input_path, out_json] = args else {
        bail!("uso: tract-check enroll <model.onnx> <in_16k.f32> <out.json>");
    };
    let bytes = fs::read(onnx_path)?;
    let audio = read_f32_le(Path::new(input_path))?;
    let result = extract(&bytes, &audio);
    let report = match &result {
        Ok(outputs) => json!({"n": audio.len(), "ok": true, "outputs": outputs}),
        Err(e) => json!({"n": audio.len(), "ok": false, "error": format!("{e:#}")}),
    };
    fs::write(out_json, serde_json::to_string(&report)? + "\n")?;
    if let Err(e) = &result {
        eprintln!("tract recusou o modelo: {e:#}");
    }
    Ok(result.is_ok())
}
```

Em `tools/tract-check/src/main.rs`, troque o conteúdo por:

```rust
mod common;
mod enroll;
mod gating;
mod identity;

use std::process::ExitCode;

const USAGE: &str = "uso: tract-check identity <film_asset.tar.gz> <approved_asset.bin> <golden.f32> <out.json>
       tract-check gating <asset.tar.gz> <in_48k.f32> <out_prefix> [film.f32]
       tract-check enroll <model.onnx> <in_16k.f32> <out.json>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("identity") => identity::run(&args[1..]),
        Some("gating") => gating::run(&args[1..]),
        Some("enroll") => enroll::run(&args[1..]),
        _ => Err(anyhow::anyhow!(USAGE)),
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("erro: {e:#}");
            ExitCode::from(2)
        }
    }
}
```

- [ ] **Step 5: Compilar e rodar todos os testes**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
scripts/tract-check.sh build
scripts/tract-check.sh test
"$CCTRAIN_PY" -m pytest tests/test_tractcheck_gating.py tests/test_tractcheck_enroll.py tests/test_tractcheck_identity.py -v
```

Expected: 5 testes Rust e todos os testes Python PASS. Em worktree, o `CARGO_TARGET_DIR` é o do checkout principal: o binário usado pelos testes (`CCTRAIN_TRACT_CHECK_BIN`) é o recém-compilado a partir do worktree.

- [ ] **Step 6: Commit**

```bash
cd "$CCTRAIN_WT"
git add tools/tract-check/src tests/test_tractcheck_gating.py tests/test_tractcheck_enroll.py
git commit -m "feat(tract-check): gating trace (lsnr and stage per frame) and runtime-equivalent enrollment loader"
```

---

### Task 15: Réplica Python do caminho do runtime e paridade com o `tract-check`

**Files:**
- Create: `src/cctrain/eval/runtime_path.py`
- Test: `tests/test_eval_runtime_path.py` (unitário), `tests/test_eval_runtime_parity.py` (marcadores `tract`, `clearcore`, `data`)

**Interfaces:**
- Consumes: `cctrain.tractcheck.run`, `APPROVED_ASSET_SHA256` (Task 2); `tract-check gating` (Task 14); `cctrain.data.audio.load_audio`, `resample` (Task 13); dados de teste do piloto (Task 12: `raw/vctk-0.92/test/p225_*.flac`, `raw/noise-wikimedia-cc0/street_traffic_rain_cc0_10s.wav`); `cctrain.hashing`, `cctrain.paths` (Task 1).
- Produces: `cctrain.eval.runtime_path`: `Stage(IntEnum)` (`ZERO=0, ERB_ONLY=1, ERB_DF=2, NONE=3, SILENT=4`); `DEFAULT_THRESHOLDS`; `apply_stages(lsnr, thresholds=DEFAULT_THRESHOLDS) -> tuple[bool, bool, bool]`; `stage_of(flags) -> Stage`; `mean_square_f32(frame) -> np.float32`; `apply_band_gains(spec_frame, gains, widths) -> None`; `deep_filter(frames, coefs) -> np.ndarray`; `EncoderStep(lsnr, tensors)`; `FrameModel` (Protocol: `reset`, `encode`, `erb_decode`, `df_decode`); `StatefulOrtFrameModel(model_dir: Path, verify_sha256: bool = True)`; `ReplicaResult(audio, lsnr, stages)`; `run_replica(audio, model, thresholds=DEFAULT_THRESHOLDS) -> ReplicaResult`; `tract_gating(asset: Path, audio: np.ndarray, out_prefix: Path, film: np.ndarray | None = None) -> ReplicaResult`; `emulate_gating_offline(spec_noisy, spec_masked, spec_enhanced, lsnr, thresholds=DEFAULT_THRESHOLDS) -> tuple[Tensor, Tensor]`; `stage_fractions(stages) -> dict[str, float]`; `parity_mixture(data_root: Path, frames: int = 1000, snr_db: float = 5.0, silent_frames: int = 20) -> np.ndarray`.

- [ ] **Step 1: Conferir os nomes de entrada/saída dos grafos stateful**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
sha256sum "$CCTRAIN_CLEARCORE_ROOT"/models/stateful/*.onnx
"$CCTRAIN_PY" - <<'EOF'
import os, onnxruntime as ort
d = os.environ["CCTRAIN_CLEARCORE_ROOT"] + "/models/stateful"
for n in ("enc", "erb_dec", "df_dec"):
    s = ort.InferenceSession(f"{d}/{n}.onnx", providers=["CPUExecutionProvider"])
    print(n, [(i.name, i.shape) for i in s.get_inputs()], [(o.name, o.shape) for o in s.get_outputs()])
EOF
```

Expected: SHA-256 `c061d8a0…`, `0c84ecc2…`, `a65a3dd9…` (`models/stateful/README.md`); `enc` com entradas `feat_erb, feat_spec, h_in, feat_erb_buf, feat_spec_buf` e saídas `e0, e1, e2, e3, emb, c0, lsnr, h_out, feat_erb_buf_out, feat_spec_buf_out`; `erb_dec` com `emb, e3, e2, e1, e0, h_in` → `m, h_out`; `df_dec` com `emb, c0, h_in, c0_buf` → `coefs, h_out, c0_buf_out`. Se divergir, ajuste só os dicionários `ENC_STATE`/`ERB_STATE`/`DF_STATE` do Step 3 e registre no commit.

- [ ] **Step 2: Escrever os testes que falham**

`tests/test_eval_runtime_path.py`:

```python
import numpy as np
import torch

from cctrain.eval.runtime_path import (
    HOP,
    EncoderStep,
    Stage,
    apply_band_gains,
    apply_stages,
    deep_filter,
    emulate_gating_offline,
    mean_square_f32,
    run_replica,
    stage_of,
)


def test_apply_stages_boundaries_match_rust():
    assert apply_stages(-10.0001) == (False, True, False)
    assert apply_stages(-10.0) == (True, False, True)
    assert apply_stages(20.0) == (True, False, True)
    assert apply_stages(20.0001) == (True, False, False)
    assert apply_stages(30.0) == (True, False, False)
    assert apply_stages(30.0001) == (False, False, False)
    assert stage_of(apply_stages(0.0)) is Stage.ERB_DF


def test_mean_square_is_sequential_f32():
    x = np.random.default_rng(0).standard_normal(480).astype(np.float32)
    acc = np.float32(0.0)
    for v in x:
        acc = np.float32(acc + v * v)
    assert mean_square_f32(x) == np.float32(acc / np.float32(480))


def test_band_gains_and_deep_filter():
    spec = (np.arange(10) + 1j * np.arange(10)).astype(np.complex64)
    apply_band_gains(spec, np.array([0.5, 2.0], np.float32), np.array([4, 6]))
    assert np.array_equal(spec[:4], (np.arange(4) * 0.5 * (1 + 1j)).astype(np.complex64))
    assert np.array_equal(spec[4:], (np.arange(4, 10) * 2.0 * (1 + 1j)).astype(np.complex64))
    frames = [np.full(481, k + 1, np.complex64) for k in range(5)]
    coefs = np.zeros((96, 5), np.complex64)
    coefs[:, 2] = 1.0
    assert np.array_equal(deep_filter(frames, coefs), np.full(96, 3, np.complex64))


class _FakeModel:
    def __init__(self, lsnrs):
        self.lsnrs = lsnrs
        self.calls = []

    def reset(self):
        self.t = 0

    def encode(self, feat_erb, feat_spec):
        step = EncoderStep(np.float32(self.lsnrs[self.t]), {})
        self.t += 1
        return step

    def erb_decode(self, step):
        self.calls.append(("erb", self.t - 1))
        return np.ones(32, np.float32)

    def df_decode(self, step):
        self.calls.append(("df", self.t - 1))
        c = np.zeros((96, 5), np.complex64)
        c[:, 2] = 1.0
        return c


def test_decoders_only_run_on_their_stages_and_silence_skips_everything():
    rng = np.random.default_rng(0)
    x = (0.1 * rng.standard_normal(8 * HOP)).astype(np.float32)
    x[:HOP] = 0.0
    lsnrs = [-20.0, 40.0, 25.0, 0.0, 0.0, 40.0, 25.0]
    model = _FakeModel(lsnrs)
    res = run_replica(x, model)
    assert list(res.stages) == [Stage.SILENT, Stage.ZERO, Stage.NONE, Stage.ERB_ONLY, Stage.ERB_DF, Stage.ERB_DF,
                                Stage.NONE, Stage.ERB_ONLY]
    assert model.calls == [("erb", 2), ("erb", 3), ("df", 3), ("erb", 4), ("df", 4), ("erb", 6)]
    assert res.lsnr[0] == np.float32(-15.0) and not np.any(res.audio[:HOP])


def test_no_processing_path_has_contract_latency_of_1440_samples():
    x = (0.1 * np.random.default_rng(1).standard_normal(200 * HOP)).astype(np.float32)
    res = run_replica(x, _FakeModel([40.0] * 200))
    np.testing.assert_allclose(res.audio[1440:], x[:-1440], atol=1e-5)


def test_offline_emulation_selects_by_stage():
    shape = (1, 1, 4, 3, 2)
    noisy, masked, enhanced = torch.full(shape, 1.0), torch.full(shape, 2.0), torch.full(shape, 3.0)
    lsnr = torch.tensor([[-20.0, 40.0, 25.0, 0.0]])
    out, stages = emulate_gating_offline(noisy, masked, enhanced, lsnr)
    assert out[0, 0, :, 0, 0].tolist() == [0.0, 1.0, 2.0, 3.0]
    assert stages.tolist() == [[0, 3, 1, 2]]
```

`tests/test_eval_runtime_parity.py`:

```python
import json

import numpy as np
import pytest

from cctrain.eval.runtime_path import (
    Stage,
    StatefulOrtFrameModel,
    parity_mixture,
    run_replica,
    stage_fractions,
    tract_gating,
)
from cctrain.paths import clearcore_root

pytestmark = [pytest.mark.tract, pytest.mark.clearcore, pytest.mark.data]
ABS, REL = 1e-4, 1e-4


def test_replica_matches_tract_check_on_1000_real_frames(paths, m0_runs, tmp_path):
    x = parity_mixture(paths.data)
    assert x.size == 1000 * 480
    root = clearcore_root()
    rt = tract_gating(root / "vendor/approved/df-compatible-release-asset-v1.bin", x, tmp_path / "rt")
    py = run_replica(x, StatefulOrtFrameModel(root / "models/stateful"))
    excess = np.abs(py.audio - rt.audio) - (ABS + REL * np.abs(rt.audio))
    per_frame = excess.reshape(1000, 480).max(axis=1)
    lsnr_excess = np.abs(py.lsnr - rt.lsnr) - (ABS + REL * np.abs(rt.lsnr))
    report = {
        "frames": 1000,
        "max_abs_diff": float(np.max(np.abs(py.audio - rt.audio))),
        "worst_excess_over_tol": float(excess.max()),
        "worst_frame": int(per_frame.argmax()),
        "first_frames_excess": [float(v) for v in per_frame[:10]],
        "frames_over_tol": int((per_frame > 0).sum()),
        "lsnr_worst_excess": float(lsnr_excess.max()),
        "stage_mismatches": int((py.stages != rt.stages).sum()),
        "fractions_tract": stage_fractions(rt.stages),
        "fractions_replica": stage_fractions(py.stages),
        "pass": bool(excess.max() <= 0 and lsnr_excess.max() <= 0 and np.array_equal(py.stages, rt.stages)),
    }
    (m0_runs / "gating_parity.json").write_text(json.dumps(report, indent=2) + "\n")
    assert np.count_nonzero(rt.stages == Stage.SILENT) == 20
    assert len({int(s) for s in rt.stages} - {int(Stage.SILENT)}) >= 2, report["fractions_tract"]
    assert report["pass"], json.dumps(report, indent=2)
```

- [ ] **Step 3: Rodar e ver falhar**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" -m pytest tests/test_eval_runtime_path.py -v 2>&1 | tail -3
```

Expected: FAIL com `ImportError`.

- [ ] **Step 4: Implementar `runtime_path.py`**

`src/cctrain/eval/runtime_path.py`:

```python
"""Réplica Python de DfTract::process (vendor/crates/deep_filter/src/tract.rs) com gating por lsnr (spec §3.5).

Espelha, quadro a quadro: atalho de silêncio (média quadrática < 1e-7: saída zero, lsnr -15, sem análise nem
modelo), anéis rolling_spec_buf_x (5) e rolling_spec_buf_y (7), apply_stages, máscara ERB por banda, deep
filtering de ordem 5 nos 96 primeiros bins e síntese. O FrameModel tem estado e só avança o decoder que roda
(decisão de planejamento 1). emulate_gating_offline é a aproximação usada no treino (sem esse estado).
"""

from __future__ import annotations

import enum
from collections import deque
from collections.abc import Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Protocol

import numpy as np
import torch
from torch import Tensor

from cctrain import tractcheck
from cctrain.hashing import sha256_file

HOP = 480
N_FREQS = 481
NB_ERB = 32
NB_DF = 96
DF_ORDER = 5
CONV_LOOKAHEAD = 2
DF_LOOKAHEAD = 2
LOOKAHEAD = max(CONV_LOOKAHEAD, DF_LOOKAHEAD)
NORM_ALPHA = 0.99
SILENCE_MEAN_SQUARE = np.float32(1e-7)
SILENT_LSNR = np.float32(-15.0)
DEFAULT_THRESHOLDS = (np.float32(-10.0), np.float32(30.0), np.float32(20.0))
STATEFUL_SHA256 = {
    "enc.onnx": "c061d8a021d060f00a01a3191bf5a253247f359987b2a13043ef0ad46931191e",
    "erb_dec.onnx": "0c84ecc2841445c0a4eee5486f43212bf41241a4459a887bf067a79cdeb057b4",
    "df_dec.onnx": "a65a3dd9b763add2cfa7e822e7e910d0d70297426dff1b3863336a970b4ea655",
}


class Stage(enum.IntEnum):
    ZERO = 0
    ERB_ONLY = 1
    ERB_DF = 2
    NONE = 3
    SILENT = 4


_STAGE_OF = {(False, True, False): Stage.ZERO, (True, False, False): Stage.ERB_ONLY,
             (True, False, True): Stage.ERB_DF, (False, False, False): Stage.NONE}


def apply_stages(lsnr, thresholds=DEFAULT_THRESHOLDS) -> tuple[bool, bool, bool]:
    lo, hi_erb, hi_df = (np.float32(t) for t in thresholds)
    value = np.float32(lsnr)
    if value < lo:
        return (False, True, False)
    if value > hi_erb:
        return (False, False, False)
    if value > hi_df:
        return (True, False, False)
    return (True, False, True)


def stage_of(flags: tuple[bool, bool, bool]) -> Stage:
    return _STAGE_OF[flags]


def mean_square_f32(frame: np.ndarray) -> np.float32:
    x = np.asarray(frame, dtype=np.float32)
    total = np.add.accumulate(x * x, dtype=np.float32)[-1]
    return np.float32(total / np.float32(x.size))


def apply_band_gains(spec_frame: np.ndarray, gains: np.ndarray, widths: Sequence[int]) -> None:
    view = spec_frame.view(np.float32).reshape(-1, 2)
    start = 0
    for width, gain in zip(widths, gains):
        view[start : start + int(width)] *= np.float32(gain)
        start += int(width)


def deep_filter(frames: Sequence[np.ndarray], coefs: np.ndarray) -> np.ndarray:
    out = np.zeros(NB_DF, dtype=np.complex64)
    for i, frame in enumerate(frames):
        out += frame[:NB_DF] * coefs[:, i]
    return out


@dataclass
class EncoderStep:
    lsnr: np.float32
    tensors: dict[str, np.ndarray]


class FrameModel(Protocol):
    def reset(self) -> None: ...

    def encode(self, feat_erb: np.ndarray, feat_spec: np.ndarray) -> EncoderStep: ...

    def erb_decode(self, step: EncoderStep) -> np.ndarray: ...

    def df_decode(self, step: EncoderStep) -> np.ndarray: ...


class StatefulOrtFrameModel:
    ENC_STATE = {"h_in": ("h_out", (1, 1, 256)), "feat_erb_buf": ("feat_erb_buf_out", (1, 1, 2, 32)),
                 "feat_spec_buf": ("feat_spec_buf_out", (1, 2, 2, 96))}
    ERB_STATE = {"h_in": ("h_out", (2, 1, 256))}
    DF_STATE = {"h_in": ("h_out", (2, 1, 256)), "c0_buf": ("c0_buf_out", (1, 64, 4, 96))}

    def __init__(self, model_dir: Path, verify_sha256: bool = True):
        import onnxruntime as ort

        model_dir = Path(model_dir)
        if verify_sha256:
            for name, sha in STATEFUL_SHA256.items():
                if sha256_file(model_dir / name) != sha:
                    raise ValueError(f"{model_dir / name}: SHA-256 não confere com models/stateful/README.md")
        opts = ort.SessionOptions()
        opts.intra_op_num_threads = 1
        opts.inter_op_num_threads = 1
        self._sessions = {g: ort.InferenceSession(str(model_dir / f"{g}.onnx"), opts,
                                                  providers=["CPUExecutionProvider"])
                          for g in ("enc", "erb_dec", "df_dec")}
        self._names = {g: [o.name for o in s.get_outputs()] for g, s in self._sessions.items()}
        self._specs = {"enc": self.ENC_STATE, "erb_dec": self.ERB_STATE, "df_dec": self.DF_STATE}
        self.reset()

    def reset(self) -> None:
        self._state = {g: {k: np.zeros(shape, np.float32) for k, (_, shape) in spec.items()}
                       for g, spec in self._specs.items()}

    def _run(self, graph: str, feeds: dict[str, np.ndarray]) -> dict[str, np.ndarray]:
        out = dict(zip(self._names[graph], self._sessions[graph].run(None, {**feeds, **self._state[graph]})))
        for inp, (outp, _) in self._specs[graph].items():
            self._state[graph][inp] = out.pop(outp)
        return out

    def encode(self, feat_erb: np.ndarray, feat_spec: np.ndarray) -> EncoderStep:
        out = self._run("enc", {
            "feat_erb": np.asarray(feat_erb, np.float32).reshape(1, 1, 1, NB_ERB),
            "feat_spec": np.ascontiguousarray(np.asarray(feat_spec, np.float32).T).reshape(1, 2, 1, NB_DF),
        })
        lsnr = np.float32(out.pop("lsnr").reshape(-1)[0])
        return EncoderStep(lsnr, out)

    def erb_decode(self, step: EncoderStep) -> np.ndarray:
        out = self._run("erb_dec", {k: step.tensors[k] for k in ("emb", "e3", "e2", "e1", "e0")})
        return out["m"].reshape(-1)[:NB_ERB].astype(np.float32)

    def df_decode(self, step: EncoderStep) -> np.ndarray:
        out = self._run("df_dec", {"emb": step.tensors["emb"], "c0": step.tensors["c0"]})
        c = out["coefs"].reshape(NB_DF, DF_ORDER, 2)  # mesma reinterpretação de tract.rs (set_shape)
        return (c[..., 0] + 1j * c[..., 1]).astype(np.complex64)


@dataclass
class ReplicaResult:
    audio: np.ndarray
    lsnr: np.ndarray
    stages: np.ndarray


def run_replica(audio: np.ndarray, model: FrameModel, thresholds=DEFAULT_THRESHOLDS) -> ReplicaResult:
    from libdf import DF, erb, erb_norm, unit_norm

    frames = len(audio) // HOP
    framed = np.asarray(audio[: frames * HOP], np.float32).reshape(frames, HOP)
    silent = np.array([mean_square_f32(f) < SILENCE_MEAN_SQUARE for f in framed], dtype=bool)
    active = np.flatnonzero(~silent)
    out = np.zeros((frames, HOP), np.float32)
    lsnr = np.full(frames, SILENT_LSNR, np.float32)
    stages = np.full(frames, int(Stage.SILENT), np.uint8)
    model.reset()
    if active.size:
        state = DF(48_000, 960, HOP, NB_ERB, 2)
        spec = state.analysis(np.ascontiguousarray(framed[active].reshape(1, -1)))
        feat_erb = np.asarray(erb_norm(erb(spec, state.erb_widths()), NORM_ALPHA))[0]
        feat_spec = np.asarray(unit_norm(np.ascontiguousarray(spec[..., :NB_DF]), NORM_ALPHA))[0]
        widths = np.asarray(state.erb_widths())
        spec = spec[0]
        rx = deque(np.zeros(N_FREQS, np.complex64) for _ in range(max(DF_ORDER, LOOKAHEAD)))
        ry = deque(np.zeros(N_FREQS, np.complex64) for _ in range(DF_ORDER + CONV_LOOKAHEAD))
        enhanced = np.zeros_like(spec)
        for t in range(spec.shape[0]):
            rx.popleft()
            ry.popleft()
            rx.append(spec[t].copy())
            ry.append(spec[t].copy())
            step = model.encode(feat_erb[t], np.stack([feat_spec[t].real, feat_spec[t].imag], axis=-1))
            flags = apply_stages(step.lsnr, thresholds)
            gains = model.erb_decode(step) if flags[0] else (np.zeros(NB_ERB, np.float32) if flags[1] else None)
            coefs = model.df_decode(step) if flags[2] else None
            target = ry[DF_ORDER - 1]
            if gains is not None:
                apply_band_gains(target, gains, widths)
            frame_out = target.copy()
            if coefs is not None:
                frame_out[:NB_DF] = deep_filter(list(rx), coefs)
            enhanced[t] = frame_out
            lsnr[active[t]] = step.lsnr
            stages[active[t]] = int(stage_of(flags))
        synth = np.asarray(state.synthesis(np.ascontiguousarray(enhanced[None])), np.float32)
        out[active] = synth.reshape(-1, HOP)
    return ReplicaResult(out.reshape(-1), lsnr, stages)


def tract_gating(asset: Path, audio: np.ndarray, out_prefix: Path, film: np.ndarray | None = None) -> ReplicaResult:
    out_prefix = Path(out_prefix)
    out_prefix.parent.mkdir(parents=True, exist_ok=True)
    inp = out_prefix.with_suffix(".in.f32")
    np.asarray(audio, "<f4").tofile(inp)
    args = ["gating", asset, inp, out_prefix]
    if film is not None:
        film_path = out_prefix.with_suffix(".film.f32")
        np.asarray(film, "<f4").tofile(film_path)
        args.append(film_path)
    proc = tractcheck.run(*args)
    if proc.returncode != 0:
        raise RuntimeError(f"tract-check gating falhou: {proc.stdout}{proc.stderr}")
    return ReplicaResult(np.fromfile(f"{out_prefix}.out.f32", "<f4"), np.fromfile(f"{out_prefix}.lsnr.f32", "<f4"),
                         np.fromfile(f"{out_prefix}.stage.u8", np.uint8))


def emulate_gating_offline(spec_noisy: Tensor, spec_masked: Tensor, spec_enhanced: Tensor, lsnr: Tensor,
                           thresholds=DEFAULT_THRESHOLDS) -> tuple[Tensor, Tensor]:
    lo, hi_erb, hi_df = (float(t) for t in thresholds)
    level = lsnr.detach()
    zero = level < lo
    none = (~zero) & (level > hi_erb)
    erb_only = (~zero) & (~none) & (level > hi_df)

    def w(mask: Tensor) -> Tensor:
        return mask[:, None, :, None, None]

    out = torch.where(w(zero), torch.zeros_like(spec_enhanced), spec_enhanced)
    out = torch.where(w(erb_only), spec_masked, out)
    out = torch.where(w(none), spec_noisy, out)
    stages = torch.full_like(level, int(Stage.ERB_DF), dtype=torch.long)
    stages[zero] = int(Stage.ZERO)
    stages[erb_only] = int(Stage.ERB_ONLY)
    stages[none] = int(Stage.NONE)
    return out, stages


def stage_fractions(stages: np.ndarray) -> dict[str, float]:
    s = np.asarray(stages)
    return {st.name.lower(): float(np.mean(s == int(st))) for st in Stage}


def parity_mixture(data_root: Path, frames: int = 1000, snr_db: float = 5.0, silent_frames: int = 20) -> np.ndarray:
    from cctrain.data.audio import load_audio, resample

    data_root = Path(data_root)
    parts = []
    for name in ("p225_003", "p225_008", "p225_011", "p225_022"):
        x, sr = load_audio(data_root / f"raw/vctk-0.92/test/{name}_mic1.flac")
        parts += [resample(x, sr, 48_000), np.zeros(9_600, np.float32)]
    n = frames * HOP
    speech = np.concatenate(parts)
    speech = np.tile(speech, int(np.ceil(n / speech.size)))[:n].astype(np.float64)
    noise, sr = load_audio(data_root / "raw/noise-wikimedia-cc0/street_traffic_rain_cc0_10s.wav")
    noise = np.tile(resample(noise, sr, 48_000), 2)[:n].astype(np.float64)
    noise *= np.sqrt(np.mean(speech**2) / (np.mean(noise**2) * 10 ** (snr_db / 10)))
    x = speech + noise
    x *= min(1.0, 0.9 / np.max(np.abs(x)))
    x[: silent_frames * HOP] = 0.0
    return x.astype(np.float32)
```

- [ ] **Step 5: Rodar os testes unitários e a paridade**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" -m pytest tests/test_eval_runtime_path.py -v
"$CCTRAIN_PY" -m pytest tests/test_eval_runtime_parity.py -v -m "tract and clearcore and data"
cat "$CCTRAIN_RUNS_DIR/m0/gating_parity.json"
"$CCTRAIN_PY" -m ruff check src tests
```

Expected: unitários PASS; paridade PASS com `"pass": true`, `stage_mismatches: 0` e frações de estágio iguais dos dois lados. Se falhar: **não** alargue a tolerância nem exclua quadros (decisão de planejamento 2). Reporte o JSON ao orquestrador; os campos `first_frames_excess` e `worst_frame` mostram se a divergência é transitória de aquecimento ou contínua.

- [ ] **Step 6: Commit**

```bash
cd "$CCTRAIN_WT"
git add src/cctrain/eval/runtime_path.py tests/test_eval_runtime_path.py tests/test_eval_runtime_parity.py
git commit -m "feat(eval): Python replica of the runtime lsnr gating path with tract-check parity"
```

---
### Task 16: Protótipo do fbank + ECAPA (com pooling) no tract

**Files:**
- Create: `tests/test_m0_ecapa_tract.py` (marcador `tract`)
- Create (gerado): `$CCTRAIN_RUNS_DIR/m0/ecapa_tract.json`

**Interfaces:**
- Consumes: `cctrain.export.enrollment.build_prototype`, `export_enrollment_onnx`, `speechlike_16k`, `scale_to_rms_dbfs`, `scale_to_peak`, `OUTPUT_NAMES` (Task 9); `tract-check enroll` e `cctrain.tractcheck.run` (Tasks 2 e 14).
- Produces: `ecapa_tract.json` com, para cada `context_mode` (`expand`, `broadcast`), cada duração (2, 6, 8, 10, 12 s) e cada nível (RMS −39,9 dBFS, RMS −20 dBFS, pico 0,99): carregou no tract (`ok`), erro do tract se houver, pior excesso sobre `1e-4 + 1e-4·|ref|` contra o PyTorch, saídas finitas e dentro dos limites; `all_pass` por modo. É a resposta do M0 ao risco "op do front-end ou do pooling do ECAPA não suportada no tract 0.19.16" (spec §9.1).

- [ ] **Step 1: Escrever o teste**

`tests/test_m0_ecapa_tract.py`:

```python
import json

import numpy as np
import pytest
import torch

from cctrain import tractcheck
from cctrain.export.enrollment import (
    OUTPUT_NAMES,
    build_prototype,
    export_enrollment_onnx,
    scale_to_peak,
    scale_to_rms_dbfs,
    speechlike_16k,
)

pytestmark = pytest.mark.tract
DURATIONS_S = (2, 6, 8, 10, 12)
LEVELS = {
    "rms_-39.9dBFS": lambda x: scale_to_rms_dbfs(x, -39.9),
    "rms_-20dBFS": lambda x: scale_to_rms_dbfs(x, -20.0),
    "peak_0.99": lambda x: scale_to_peak(x, 0.99),
}


def _within_limits(name: str, v: np.ndarray) -> bool:
    if not np.isfinite(v).all():
        return False
    if name.startswith("gamma"):
        return bool(v.min() >= 0.001 and v.max() <= 100.0)
    if name.startswith("beta"):
        return bool(np.abs(v).max() <= 50.0)
    return True


def test_enrollment_prototype_loads_and_matches_in_tract(tmp_path, m0_runs):
    results = {}
    for mode in ("expand", "broadcast"):
        model = build_prototype(seed=0, context_mode=mode)
        onnx_path = export_enrollment_onnx(model, tmp_path / f"enroll-{mode}.onnx")
        cases = []
        for seconds in DURATIONS_S:
            for level, scale in LEVELS.items():
                x = scale(speechlike_16k(16_000 * seconds, seed=seconds)).astype("<f4")
                inp, out = tmp_path / f"{mode}-{seconds}-{level}.f32", tmp_path / f"{mode}-{seconds}-{level}.json"
                x.tofile(inp)
                proc = tractcheck.run("enroll", onnx_path, inp, out)
                rep = json.loads(out.read_text())
                case = {"seconds": seconds, "level": level, "ok": rep["ok"], "error": rep.get("error"),
                        "returncode": proc.returncode}
                if rep["ok"]:
                    with torch.no_grad():
                        ref = [t.numpy() for t in model(torch.from_numpy(x)[None])]
                    excess, limits = [], []
                    for name, r in zip(OUTPUT_NAMES, ref):
                        got = np.asarray(rep["outputs"][name], np.float32)
                        excess.append(float(np.max(np.abs(got - r) - (1e-4 + 1e-4 * np.abs(r)))))
                        limits.append(_within_limits(name, got))
                    case.update(worst_excess=max(excess), within_limits=all(limits),
                                passed=max(excess) <= 0 and all(limits))
                else:
                    case["passed"] = False
                cases.append(case)
        results[mode] = {"all_pass": all(c["passed"] for c in cases), "cases": cases}
    (m0_runs / "ecapa_tract.json").write_text(json.dumps(results, indent=2) + "\n")
    assert any(r["all_pass"] for r in results.values()), json.dumps(
        {m: [c for c in r["cases"] if not c["passed"]][:3] for m, r in results.items()}, indent=2)
```

- [ ] **Step 2: Rodar**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" -m pytest tests/test_m0_ecapa_tract.py -v -m tract
"$CCTRAIN_PY" -c "import json,os; r=json.load(open(os.environ['CCTRAIN_RUNS_DIR']+'/m0/ecapa_tract.json')); print({m: v['all_pass'] for m, v in r.items()})"
```

Expected: PASS com pelo menos um modo `all_pass: true`. Este teste não tem "fase vermelha" própria: ele mede um fato (o tract aceita ou não os operadores). Se **nenhum** modo passar, o teste falha e o JSON traz a mensagem do tract: reporte ao orquestrador (é um achado do M0, não um bug a contornar nesta tarefa).

- [ ] **Step 3: Commit**

```bash
cd "$CCTRAIN_WT"
git add tests/test_m0_ecapa_tract.py
git commit -m "test(m0): prototype fbank + ECAPA attentive pooling in tract at 2-12 s and -40 dBFS..0.99"
```

---

### Task 17: Medições do M0 (disco, loader, passo GPU, `B`, desvio offline × runtime)

**Files:**
- Create: `src/cctrain/m0/gpu_monitor.py`, `src/cctrain/m0/disk_probe.py`, `src/cctrain/m0/loader_probe.py`, `src/cctrain/m0/step_probe.py`, `src/cctrain/m0/beta_probe.py`, `src/cctrain/m0/offline_probe.py`, `scripts/m0_measure.py`
- Modify: `configs/pdfnet3/film-v1.yaml` (linha `beta_bound`)
- Test: `tests/test_m0_probes.py`
- Create (gerados): `$CCTRAIN_RUNS_DIR/m0/{env,disk,loader,step_b16,step_b32,step_b16_teacher,beta_rule,offline_vs_runtime}.json` (o resumo do `nvidia-smi` vai dentro de cada `step_*.json`)

**Interfaces:**
- Consumes: `PilotMixDataset`, `PilotLoaderConfig`, `pilot_records` (Task 13); `extract_features` (Task 13); `load_upstream_dfnet3` (Task 8); `PDFNet3` (Task 8); `FilmGenerator`, `FilmConfig`, `beta_bound_rule`, `FILM_CONFIG_V1` (Task 7); `run_replica`, `StatefulOrtFrameModel`, `emulate_gating_offline`, `parity_mixture` (Task 15); `cctrain.data.manifest` (Task 3); `cctrain.disk`, `cctrain.paths` (Task 1).
- Produces:
  - `cctrain.m0.gpu_monitor`: `GpuSample`; `parse_line(line: str) -> GpuSample`; `GpuMonitor` (context manager, `summary() -> dict`).
  - `cctrain.m0.disk_probe`: `flac_ratio_by_sr(records, data_root) -> dict[str, dict]`; `dir_size(path) -> int`; `disk_report(paths) -> dict`.
  - `cctrain.m0.loader_probe`: `total_rss() -> int`; `probe_loader(dataset, *, batch_size=16, num_workers=4, prefetch_factor=2, warmup_batches=3, batches=30) -> dict`.
  - `cctrain.m0.step_probe`: `vorbis_window(n=960) -> Tensor`; `istft_proxy(spec) -> Tensor`; `MultiResSpecLossProxy`; `probe_step(model, batch, *, device, teacher=None, warmup=5, steps=20) -> dict`.
  - `cctrain.m0.beta_probe`: `collect_site_activations(base, batches, device) -> dict[str, np.ndarray]`; `beta_rule_report(acts: dict[str, np.ndarray]) -> dict`; `write_beta_bound(value: float, path=FILM_CONFIG_V1) -> None`.
  - `cctrain.m0.offline_probe`: `offline_vs_runtime(base, audio, replica: ReplicaResult) -> dict`.
  - `configs/pdfnet3/film-v1.yaml` com `beta_bound` numérico (`B` fixado pela regra da §3.2).

- [ ] **Step 1: Escrever os testes unitários que falham**

`tests/test_m0_probes.py`:

```python
import io

import numpy as np
import pytest
import soundfile as sf
import torch
from torch import nn

from cctrain.data.manifest import FileRecord
from cctrain.hashing import sha256_file
from cctrain.m0.beta_probe import beta_rule_report, write_beta_bound
from cctrain.m0.disk_probe import flac_ratio_by_sr
from cctrain.m0.gpu_monitor import parse_line
from cctrain.m0.step_probe import MultiResSpecLossProxy, istft_proxy


def test_parse_nvidia_smi_line():
    s = parse_line("2026/10/03 10:00:00.000, 71, 2100, 48.5, 3120, 0x0000000000000004")
    assert (s.temperature_c, s.sm_clock_mhz, s.power_w, s.memory_used_mib) == (71.0, 2100.0, 48.5, 3120.0)
    assert s.reasons == "0x0000000000000004"


def test_flac_ratio_by_sr(tmp_path):
    x = (0.1 * np.random.default_rng(0).standard_normal(16_000 * 2)).astype("f4")
    p = tmp_path / "a.wav"
    sf.write(p, x, 16_000, subtype="PCM_16")
    rec = FileRecord(path="a.wav", sha256=sha256_file(p), source_id="s", license="CC0-1.0", role="train",
                     speaker=None, split="noise", sr=16_000, num_samples=x.size, band_hz=None, codec="wav")
    out = flac_ratio_by_sr([rec], tmp_path)["16000"]
    y, _ = sf.read(p, dtype="float32")
    buf = io.BytesIO()
    sf.write(buf, y, 16_000, format="FLAC", subtype="PCM_16")
    assert out["pcm16_bytes"] == x.size * 2
    assert out["flac_bytes"] == len(buf.getvalue())
    assert abs(out["hours"] - 2 / 3600) < 1e-9


def test_beta_rule_report_pools_both_sites():
    acts = {"enc": np.linspace(0, 1, 1001), "df_dec": np.linspace(0, 3, 1001)}
    rep = beta_rule_report(acts)
    pooled = float(np.percentile(np.concatenate(list(acts.values())), 99))
    assert rep["p99_pooled"] == pytest.approx(pooled)
    assert rep["beta_bound"] == min(50.0, max(4.0, 2 * pooled))


def test_write_beta_bound_only_touches_that_line(tmp_path):
    p = tmp_path / "film.yaml"
    p.write_text("version: 1\nlog_gamma_bound: 2.302585092994046\nbeta_bound: null\nmid_dim: 256\n")
    write_beta_bound(5.25, p)
    assert p.read_text().splitlines()[2].startswith("beta_bound: 5.25")
    with pytest.raises(ValueError):
        write_beta_bound(6.0, p)


def test_loss_proxy_is_zero_for_identical_inputs_and_positive_otherwise():
    spec = torch.randn(2, 1, 20, 481, 2)
    loss = MultiResSpecLossProxy()
    y = istft_proxy(spec)
    assert float(loss(y, y)) == 0.0
    assert float(loss(y, istft_proxy(spec * 0.5))) > 0.0
    assert isinstance(loss, nn.Module)
```

- [ ] **Step 2: Rodar e ver falhar**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" -m pytest tests/test_m0_probes.py -v 2>&1 | tail -3
```

Expected: FAIL com `ImportError`.

- [ ] **Step 3: Implementar os módulos de medição**

`src/cctrain/m0/gpu_monitor.py`:

```python
"""Amostragem do nvidia-smi a cada 1 s durante uma medição (temperatura, clock, potência, motivos de throttling)."""

from __future__ import annotations

import subprocess
import threading
from dataclasses import dataclass

FIELDS = ["timestamp", "temperature.gpu", "clocks.sm", "power.draw", "memory.used"]
REASON_FIELDS = ["clocks_event_reasons.active", "clocks_throttle_reasons.active"]


@dataclass(frozen=True)
class GpuSample:
    temperature_c: float
    sm_clock_mhz: float
    power_w: float
    memory_used_mib: float
    reasons: str


def _num(v: str) -> float:
    try:
        return float(v)
    except ValueError:
        return float("nan")


def parse_line(line: str) -> GpuSample:
    parts = [p.strip() for p in line.split(",")]
    return GpuSample(_num(parts[1]), _num(parts[2]), _num(parts[3]), _num(parts[4]), parts[5])


def _query() -> list[str]:
    for reason in REASON_FIELDS:
        fields = FIELDS + [reason]
        probe = subprocess.run(["nvidia-smi", f"--query-gpu={','.join(fields)}", "--format=csv,noheader,nounits"],
                               capture_output=True, text=True, check=False)
        if probe.returncode == 0:
            return fields
    raise RuntimeError("nvidia-smi não aceitou nenhum campo de motivo de throttling")


class GpuMonitor:
    def __init__(self, interval_ms: int = 1000):
        self.interval_ms = interval_ms
        self.samples: list[GpuSample] = []

    def __enter__(self) -> GpuMonitor:
        fields = _query()
        self._proc = subprocess.Popen(
            ["nvidia-smi", f"--query-gpu={','.join(fields)}", "--format=csv,noheader,nounits",
             f"-lms={self.interval_ms}"], stdout=subprocess.PIPE, text=True)
        self._thread = threading.Thread(target=self._read, daemon=True)
        self._thread.start()
        return self

    def _read(self) -> None:
        for line in self._proc.stdout:
            if line.strip():
                self.samples.append(parse_line(line))

    def __exit__(self, *exc) -> None:
        self._proc.terminate()
        self._proc.wait(timeout=10)
        self._thread.join(timeout=10)

    def summary(self) -> dict:
        s = self.samples
        reasons = sorted({x.reasons for x in s if x.reasons and int(x.reasons, 16) != 0}) if s else []
        return {
            "samples": len(s),
            "max_temperature_c": max((x.temperature_c for x in s), default=float("nan")),
            "min_sm_clock_mhz": min((x.sm_clock_mhz for x in s), default=float("nan")),
            "mean_power_w": float(sum(x.power_w for x in s) / len(s)) if s else float("nan"),
            "max_memory_used_mib": max((x.memory_used_mib for x in s), default=float("nan")),
            "nonzero_throttle_reasons": reasons,
        }
```

`src/cctrain/m0/disk_probe.py`:

```python
"""Fator FLAC/PCM por taxa de amostragem e tamanho dos ambientes (substitui os [est.] da spec §4.2)."""

from __future__ import annotations

import io
import os
import shutil
from collections import defaultdict
from collections.abc import Iterable
from pathlib import Path

import soundfile as sf

from cctrain.data.audio import load_audio
from cctrain.data.manifest import FileRecord
from cctrain.paths import Paths


def flac_ratio_by_sr(records: Iterable[FileRecord], data_root: Path) -> dict[str, dict]:
    acc: dict[int, dict] = defaultdict(lambda: {"files": 0, "seconds": 0.0, "pcm16_bytes": 0, "flac_bytes": 0,
                                                "stored_bytes": 0, "codecs": set()})
    for r in records:
        a = acc[r.sr]
        path = Path(data_root) / r.path
        a["files"] += 1
        a["seconds"] += r.num_samples / r.sr
        a["pcm16_bytes"] += r.num_samples * 2
        a["stored_bytes"] += path.stat().st_size
        a["codecs"].add(r.codec)
        if r.codec == "flac":
            a["flac_bytes"] += path.stat().st_size
        else:
            x, sr = load_audio(path)
            buf = io.BytesIO()
            sf.write(buf, x, sr, format="FLAC", subtype="PCM_16")
            a["flac_bytes"] += len(buf.getvalue())
    out = {}
    for sr, a in sorted(acc.items()):
        hours = a["seconds"] / 3600.0
        out[str(sr)] = {
            "files": a["files"], "hours": hours, "pcm16_bytes": a["pcm16_bytes"], "flac_bytes": a["flac_bytes"],
            "stored_bytes": a["stored_bytes"], "codecs": sorted(a["codecs"]),
            "flac_over_pcm16": a["flac_bytes"] / a["pcm16_bytes"],
            "flac_gb_per_hour": a["flac_bytes"] / 1e9 / hours if hours else float("nan"),
            "stored_gb_per_hour": a["stored_bytes"] / 1e9 / hours if hours else float("nan"),
        }
    return out


def dir_size(path: Path) -> int:
    total = 0
    for root, _, files in os.walk(path):
        for f in files:
            p = os.path.join(root, f)
            if not os.path.islink(p):
                total += os.path.getsize(p)
    return total


def disk_report(paths: Paths, records: Iterable[FileRecord]) -> dict:
    records = list(records)
    env_dirs = {
        "venv": Path(os.environ.get("UV_PROJECT_ENVIRONMENT", paths.repo / ".venv")),
        "tract_check_target": paths.repo / "tools/tract-check/target",
        "rustup": Path(os.environ.get("RUSTUP_HOME", Path.home() / ".rustup")),
        "data": paths.data,
        "third_party": paths.repo / "third_party",
    }
    rir = [r for r in records if r.split == "rir"]
    return {
        "by_sr": flac_ratio_by_sr([r for r in records if r.split != "rir"], paths.data),
        "rir": {"count": len(rir), "stored_bytes": sum((paths.data / r.path).stat().st_size for r in rir)},
        "dirs_bytes": {k: dir_size(v) if v.exists() else None for k, v in env_dirs.items()},
        "free_bytes": shutil.disk_usage(paths.data).free,
    }
```

`src/cctrain/m0/loader_probe.py`:

```python
"""Vazão do loader (amostras/s, s por hop produzido) e RSS com 4 workers e prefetch 2 (spec §5.5)."""

from __future__ import annotations

import time

import psutil
from torch.utils.data import DataLoader

HOPS_PER_SECOND = 100


def total_rss() -> int:
    p = psutil.Process()
    procs = [p, *p.children(recursive=True)]
    total = 0
    for proc in procs:
        try:
            total += proc.memory_info().rss
        except psutil.NoSuchProcess:
            pass
    return total


def probe_loader(dataset, *, batch_size: int = 16, num_workers: int = 4, prefetch_factor: int = 2,
                 warmup_batches: int = 3, batches: int = 30) -> dict:
    dl = DataLoader(dataset, batch_size=batch_size, num_workers=num_workers, prefetch_factor=prefetch_factor,
                    persistent_workers=True)
    it = iter(dl)
    for _ in range(warmup_batches):
        next(it)
    rss_max = 0
    t0 = time.perf_counter()
    for _ in range(batches):
        batch = next(it)
        rss_max = max(rss_max, total_rss())
    dt = time.perf_counter() - t0
    samples = batches * batch_size
    seconds_audio = float(batch["noisy"].shape[1]) / 48_000.0
    del it, dl
    return {
        "batch_size": batch_size, "num_workers": num_workers, "prefetch_factor": prefetch_factor,
        "batches": batches, "seconds": dt, "samples_per_s": samples / dt,
        "wall_s_per_hop": dt / (samples * seconds_audio * HOPS_PER_SECOND),
        "rss_max_gb": rss_max / 1024**3, "segment_s": seconds_audio,
    }
```

`src/cctrain/m0/step_probe.py`:

```python
"""Passo de treino do pDFNet3 (forward + backward) para medir s/passo e VRAM (spec §5.5).

A perda é um proxy de custo da multi-resolução do upstream (FFT 256/512/1024/2048, gamma 0,3, fatores 500/500)
sobre uma iSTFT com janela Vorbis; a perda real (paridade com df/loss.py) é do M2.
"""

from __future__ import annotations

import math
import statistics
import time

import torch
from torch import Tensor, nn

from cctrain.m0.loader_probe import total_rss

GIB = 1024**3


def vorbis_window(n: int = 960) -> Tensor:
    k = torch.arange(n, dtype=torch.float64)
    return torch.sin(0.5 * math.pi * torch.sin(math.pi * (k + 0.5) / n) ** 2).float()


def istft_proxy(spec: Tensor) -> Tensor:
    c = torch.view_as_complex(spec[:, 0].permute(0, 2, 1, 3).contiguous())  # [B, F, T]
    return torch.istft(c, n_fft=960, hop_length=480, window=vorbis_window().to(spec.device), center=True)


class MultiResSpecLossProxy(nn.Module):
    def __init__(self, n_ffts=(256, 512, 1024, 2048), gamma: float = 0.3, factor: float = 500.0,
                 factor_complex: float = 500.0):
        super().__init__()
        self.n_ffts, self.gamma, self.factor, self.factor_complex = n_ffts, gamma, factor, factor_complex

    def forward(self, est: Tensor, ref: Tensor) -> Tensor:
        loss = est.new_zeros(())
        for n in self.n_ffts:
            w = torch.hann_window(n, device=est.device)
            y = torch.stft(est, n, n // 4, window=w, return_complex=True)
            s = torch.stft(ref, n, n // 4, window=w, return_complex=True)
            ya, sa = y.abs().clamp_min(1e-12).pow(self.gamma), s.abs().clamp_min(1e-12).pow(self.gamma)
            loss = loss + self.factor * nn.functional.mse_loss(ya, sa)
            yc = torch.view_as_real(y / y.abs().clamp_min(1e-12) * ya)
            sc = torch.view_as_real(s / s.abs().clamp_min(1e-12) * sa)
            loss = loss + self.factor_complex * nn.functional.mse_loss(yc, sc)
        return loss


def probe_step(model: nn.Module, batch: dict, *, device: str, teacher: nn.Module | None = None,
               warmup: int = 5, steps: int = 20) -> dict:
    model.to(device).train()
    for m in model.modules():
        if isinstance(m, nn.modules.batchnorm._BatchNorm):
            m.eval()
    opt = torch.optim.AdamW([p for p in model.parameters() if p.requires_grad], lr=1e-4)
    loss_fn = MultiResSpecLossProxy().to(device)
    spec, erb, specf, clean = (torch.as_tensor(batch[k]).to(device) for k in ("spec", "feat_erb", "feat_spec",
                                                                              "spec_clean"))
    b = spec.shape[0]
    g = torch.Generator(device="cpu").manual_seed(0)
    emb = nn.functional.normalize(torch.randn(b, 192, generator=g), dim=-1).to(device)
    mask = (torch.rand(b, 1, generator=g) > 0.2).float().to(device)
    ref = istft_proxy(clean)
    torch.cuda.reset_peak_memory_stats(device)
    times, finite = [], True
    for i in range(warmup + steps):
        torch.cuda.synchronize(device)
        t0 = time.perf_counter()
        opt.zero_grad(set_to_none=True)
        spec_e = model(spec.clone(), erb, specf, emb, mask)[0]
        est = istft_proxy(spec_e)
        loss = loss_fn(est, ref)
        if teacher is not None:
            with torch.no_grad():
                t_spec = teacher(spec.clone(), erb, specf)[0]
            loss = loss + loss_fn(est, istft_proxy(t_spec))
        loss.backward()
        nn.utils.clip_grad_norm_(model.parameters(), 1.0)
        opt.step()
        torch.cuda.synchronize(device)
        finite = finite and bool(torch.isfinite(loss))
        if i >= warmup:
            times.append(time.perf_counter() - t0)
    return {
        "batch": b, "teacher": teacher is not None, "steps": steps,
        "s_per_step_median": statistics.median(times), "s_per_step_max": max(times),
        "peak_allocated_gb": torch.cuda.max_memory_allocated(device) / GIB,
        "peak_reserved_gb": torch.cuda.max_memory_reserved(device) / GIB,
        "rss_gb": total_rss() / GIB, "loss_finite": finite,
    }
```

`src/cctrain/m0/beta_probe.py`:

```python
"""Regra de B (spec §3.2): p99 das ativações pós-ReLU nos dois sites FiLM do DFNet3 upstream, dados do M0."""

from __future__ import annotations

import re
from collections.abc import Iterable
from pathlib import Path

import numpy as np
import torch
from torch import nn

from cctrain.models.film import FILM_CONFIG_V1, beta_bound_rule


def collect_site_activations(base: nn.Module, batches: Iterable[dict], device: str) -> dict[str, np.ndarray]:
    sites = {"enc": base.enc.emb_gru.linear_in, "df_dec": base.df_dec.df_gru.linear_in}
    store: dict[str, list[np.ndarray]] = {k: [] for k in sites}
    hooks = [m.register_forward_hook(
        lambda _m, _i, out, k=k: store[k].append(out.detach().float().cpu().reshape(-1).numpy()))
        for k, m in sites.items()]
    try:
        with torch.no_grad():
            for b in batches:
                base(*(torch.as_tensor(b[k]).to(device) for k in ("spec", "feat_erb", "feat_spec")))
    finally:
        for h in hooks:
            h.remove()
    return {k: np.concatenate(v) for k, v in store.items()}


def beta_rule_report(acts: dict[str, np.ndarray]) -> dict:
    pooled = np.concatenate(list(acts.values()))
    p99 = float(np.percentile(pooled, 99))
    return {
        "p99_by_site": {k: float(np.percentile(v, 99)) for k, v in acts.items()},
        "max_by_site": {k: float(v.max()) for k, v in acts.items()},
        "values_by_site": {k: int(v.size) for k, v in acts.items()},
        "p99_pooled": p99,
        "rule": "B = min(50, max(4, 2*p99_pooled))",
        "beta_bound": beta_bound_rule(p99),
    }


def write_beta_bound(value: float, path: Path = FILM_CONFIG_V1) -> None:
    path = Path(path)
    text = path.read_text()
    if not re.search(r"^beta_bound: null$", text, flags=re.M):
        raise ValueError(f"{path}: beta_bound já fixado; mudar exige nova versão da config")
    path.write_text(re.sub(r"^beta_bound: null$", f"beta_bound: {value}  # fixado no M0 (regra da spec §3.2)",
                           text, count=1, flags=re.M))
```

`src/cctrain/m0/offline_probe.py`:

```python
"""Desvio entre a emulação de gating do treino (forward PyTorch inteiro + máscara por estágio) e a réplica do runtime."""

from __future__ import annotations

import numpy as np
import torch
from libdf import DF

from cctrain.data.features import extract_features
from cctrain.eval.runtime_path import HOP, LOOKAHEAD, ReplicaResult, Stage, emulate_gating_offline


def offline_vs_runtime(base, audio: np.ndarray, replica: ReplicaResult) -> dict:
    f = extract_features(audio)
    spec, erb, specf = (torch.from_numpy(a)[None] for a in (f.spec, f.feat_erb, f.feat_spec))
    with torch.no_grad():
        spec_e, m, lsnr, _ = base(spec.clone(), erb, specf)
        spec_m = base.mask(spec.clone(), m)
        out, stages = emulate_gating_offline(spec, spec_m, spec_e, lsnr.reshape(1, -1))
    y = np.asarray(DF(48_000, 960, HOP, 32, 2).synthesis(
        np.ascontiguousarray(torch.view_as_complex(out[:, 0].contiguous()).numpy())), np.float32).reshape(-1)
    shift = LOOKAHEAD * HOP
    rt, off = replica.audio[shift:], y[: y.size - shift]
    n = min(rt.size, off.size)
    diff = rt[:n] - off[:n]
    valid = replica.stages[LOOKAHEAD:] != int(Stage.SILENT)
    agree = stages.numpy()[0][: valid.size][valid] == replica.stages[LOOKAHEAD:][valid]
    return {
        "max_abs_diff": float(np.max(np.abs(diff))),
        "error_to_signal_db": float(10 * np.log10(np.mean(diff.astype(np.float64) ** 2) /
                                                   max(np.mean(rt[:n].astype(np.float64) ** 2), 1e-20))),
        "stage_agreement": float(np.mean(agree)),
        "offline_stage_fractions": {s.name.lower(): float(np.mean(stages.numpy() == int(s))) for s in Stage},
        "note": "emulação offline sem o estado congelado dos decoders; usada no treino (spec §3.5)",
    }
```

`scripts/m0_measure.py`:

```python
"""Medições do M0. Uso: m0_measure.py {env,disk,loader,step,beta,offline,all}. Grava em $CCTRAIN_RUNS_DIR/m0/."""

import json
import platform
import subprocess
import sys

import numpy as np
import psutil
import torch

from cctrain.data.loader import PilotLoaderConfig, PilotMixDataset, pilot_records
from cctrain.data.manifest import read_file_manifest
from cctrain.data.pilot import PILOT_MANIFESTS
from cctrain.m0 import beta_probe, disk_probe, loader_probe, offline_probe, step_probe
from cctrain.m0.gpu_monitor import GpuMonitor
from cctrain.models.dfnet3_upstream import load_upstream_dfnet3
from cctrain.models.film import FilmConfig, FilmGenerator
from cctrain.models.pdfnet3 import PDFNet3
from cctrain.paths import Paths, clearcore_root

PATHS = Paths.from_env()
OUT = PATHS.runs / "m0"


def _save(name: str, obj: dict) -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / f"{name}.json").write_text(json.dumps(obj, indent=2, default=str) + "\n")
    print(name, json.dumps(obj, default=str)[:400])


def _dataset() -> PilotMixDataset:
    speech, noise, rirs = pilot_records(PATHS.data)
    return PilotMixDataset(speech, noise, rirs, PATHS.data, PilotLoaderConfig())


def _batches(n_batches: int, batch_size: int) -> list[dict]:
    it = iter(_dataset())
    out = []
    for _ in range(n_batches):
        items = [next(it) for _ in range(batch_size)]
        out.append({k: torch.as_tensor(np.stack([i[k] for i in items])) for k in
                    ("spec", "spec_clean", "feat_erb", "feat_spec")})
    return out


def env() -> None:
    vm = psutil.virtual_memory()
    smi = subprocess.run(["nvidia-smi", "-q"], capture_output=True, text=True, check=False).stdout
    _save("env", {"python": platform.python_version(), "torch": torch.__version__, "cuda": torch.version.cuda,
                  "gpu": torch.cuda.get_device_name(0) if torch.cuda.is_available() else None,
                  "ram_total_gb": vm.total / 1024**3, "ram_available_gb": vm.available / 1024**3,
                  "swap_used_gb": psutil.swap_memory().used / 1024**3, "nvidia_smi_q_head": smi[:4000]})


def disk() -> None:
    records = [r for p in PILOT_MANIFESTS.values() for r in read_file_manifest(p)]
    records += read_file_manifest(PILOT_MANIFESTS["vctk-0.92"].parent / "rir-synthetic-v1-pilot.jsonl")
    cv = PATHS.data / "manifests" / "cv26-ptbr.jsonl"
    if cv.is_file():
        records += read_file_manifest(cv)
    _save("disk", disk_probe.disk_report(PATHS, records))


def loader() -> None:
    _save("loader", loader_probe.probe_loader(_dataset()))


def step() -> None:
    free, total = torch.cuda.mem_get_info()
    print(f"VRAM livre {free / 1024**3:.2f} / {total / 1024**3:.2f} GiB")
    for name, bsz, with_teacher in (("step_b16", 16, False), ("step_b32", 32, False), ("step_b16_teacher", 16, True)):
        base, _ = load_upstream_dfnet3(device="cpu")
        model = PDFNet3(base, FilmGenerator(FilmConfig(2.302585092994046, 4.0)))
        teacher = load_upstream_dfnet3(device="cuda")[0] if with_teacher else None
        batch = _batches(1, bsz)[0]
        try:
            with GpuMonitor() as mon:
                result = step_probe.probe_step(model, batch, device="cuda", teacher=teacher)
            result["gpu"] = mon.summary()
        except torch.cuda.OutOfMemoryError as e:
            result = {"batch": bsz, "teacher": with_teacher, "oom": True, "error": str(e)[:500]}
        _save(name, result)
        del model, teacher, base
        torch.cuda.empty_cache()


def beta() -> None:
    base, _ = load_upstream_dfnet3(device="cuda" if torch.cuda.is_available() else "cpu")
    device = next(base.parameters()).device.type
    acts = beta_probe.collect_site_activations(base, _batches(25, 8), device)
    report = beta_probe.beta_rule_report(acts)
    report["samples"] = 25 * 8
    _save("beta_rule", report)
    beta_probe.write_beta_bound(report["beta_bound"])


def offline() -> None:
    from cctrain.eval.runtime_path import StatefulOrtFrameModel, parity_mixture, run_replica

    x = parity_mixture(PATHS.data)
    replica = run_replica(x, StatefulOrtFrameModel(clearcore_root() / "models/stateful"))
    base, _ = load_upstream_dfnet3(device="cpu")
    _save("offline_vs_runtime", offline_probe.offline_vs_runtime(base, x, replica))


def main(which: str) -> None:
    steps = {"env": env, "disk": disk, "loader": loader, "step": step, "beta": beta, "offline": offline}
    for name, fn in steps.items():
        if which in (name, "all"):
            fn()


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "all")
```

- [ ] **Step 4: Rodar os testes unitários**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" -m pytest tests/test_m0_probes.py -v
"$CCTRAIN_PY" -m ruff check src tests scripts
```

Expected: todos PASS.

- [ ] **Step 5: Executar as medições (GPU livre, aplicações pesadas fechadas pelo dono)**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
nvidia-smi --query-gpu=memory.used,memory.total,temperature.gpu --format=csv
free -g
"$CCTRAIN_PY" scripts/m0_measure.py env
"$CCTRAIN_PY" scripts/m0_measure.py disk
"$CCTRAIN_PY" scripts/m0_measure.py loader
"$CCTRAIN_PY" scripts/m0_measure.py step
"$CCTRAIN_PY" scripts/m0_measure.py beta
"$CCTRAIN_PY" scripts/m0_measure.py offline
ls "$CCTRAIN_RUNS_DIR/m0/"
grep -n "^beta_bound" configs/pdfnet3/film-v1.yaml
"$CCTRAIN_PY" -c "from cctrain.models.film import load_film_config; print(load_film_config())"
```

Expected: os oito JSON em `runs/m0/`; `beta_bound: <valor>` com `4 ≤ B ≤ 50`; `load_film_config()` agora carrega. Um OOM no lote 32 não é erro do script: fica registrado (`"oom": true`) e entra no go/no-go. Não rode `step` com outro treino ou inferência na GPU.

- [ ] **Step 6: Commit**

```bash
cd "$CCTRAIN_WT"
git add src/cctrain/m0 scripts/m0_measure.py tests/test_m0_probes.py configs/pdfnet3/film-v1.yaml
git commit -m "feat(m0): disk, loader, GPU step and FiLM beta-bound measurements; fix B by the spec rule"
```

---

### Task 18: I2 no asset do repositório, relatório M0 e go/no-go do orçamento

**Files:**
- Create: `src/cctrain/m0/report.py`, `scripts/m0_report.py`, `configs/data/minimal-set-v1.yaml`
- Create (gerados): `$CCTRAIN_RUNS_DIR/m0/i2_own.json`, `$CCTRAIN_RUNS_DIR/m0/report.json`, `docs/m0-report.md`
- Test: `tests/test_m0_report.py`

**Interfaces:**
- Consumes: todos os JSON de `runs/m0/` (Tasks 2, 6, 8, 15, 16, 17); `load_gates`, `load_budget` (Task 5); `build_film_identity_asset` (Task 6); `tract-check identity` (Task 2); `cctrain.data.resample.V1_DIR` (Task 4).
- Produces: `cctrain.m0.report`: `project_disk(minimal: dict, disk: dict) -> dict`; `t3_hours(budget_t3: Mapping, s_per_step: float) -> float`; `go_no_go(budget, disk_projection_gb: float, free_gb: float, step16: dict, loader: dict) -> list[dict]`; `render_markdown(context: dict) -> str`; o relatório `docs/m0-report.md`.

- [ ] **Step 1: Escrever `minimal-set-v1.yaml`**

`configs/data/minimal-set-v1.yaml`:

```yaml
# Conjunto mínimo (spec §4.2; R04 §5; R05). Horas e tamanhos fixos vêm das pesquisas; o M0 troca os fatores
# de bytes por hora e os tamanhos de ambiente pelos medidos.
version: 1
speech:
  - {id: vctk-0.92-mic1, hours: 44.0, sr: 48000}
  - {id: tts-portuguese, hours: 10.5, sr: 48000}
  - {id: cml-tts-pt-cap30h, hours: 30.0, sr: 24000}
  - {id: librispeech-cap4min, hours: 155.9, sr: 16000, note: "2338 falantes x 4 min"}
  - {id: cv27-pt-mp3, fixed_gb: 4.85}
noise:
  - {id: dns-noise-fullband-cc0, hours: 10.0, sr: 48000}
  - {id: fsd50k-eval-cc0-ccby, hours: 15.0, sr: 44100}
rir:
  count: 10000
estimated_fixed_gb:
  evaluators_whisper_dnsmos: 3.0
  test_sets: 3.0
  prerendered_dev_test: 3.0
  runs_checkpoints: 2.0
```

- [ ] **Step 2: Escrever os testes que falham**

`tests/test_m0_report.py`:

```python
import pytest

from cctrain.eval.gates import load_budget
from cctrain.m0.report import go_no_go, project_disk, render_markdown, t3_hours

DISK = {"by_sr": {"16000": {"flac_gb_per_hour": 0.06}, "48000": {"flac_gb_per_hour": 0.17}},
        "rir": {"count": 64, "stored_bytes": 64 * 100_000},
        "dirs_bytes": {"venv": 7e9, "tract_check_target": 3e9, "rustup": 1.5e9, "data": 1e9, "third_party": 1e7}}
MINIMAL = {"speech": [{"id": "a", "hours": 10.0, "sr": 48000}, {"id": "b", "hours": 10.0, "sr": 24000},
                      {"id": "c", "fixed_gb": 1.0}],
           "noise": [{"id": "n", "hours": 1.0, "sr": 16000}], "rir": {"count": 1000},
           "estimated_fixed_gb": {"x": 2.0}}


def test_disk_projection_uses_measured_factors_and_flags_interpolation():
    p = project_disk(MINIMAL, DISK)
    rows = {r["id"]: r for r in p["rows"]}
    assert rows["a"]["gb"] == pytest.approx(1.7)
    assert rows["b"]["factor_source"] == "interpolado"
    assert rows["c"]["gb"] == 1.0
    assert rows["rir"]["gb"] == pytest.approx(0.1)
    assert p["measured_gb"]["venv"] == pytest.approx(7.0)
    assert p["total_gb"] == pytest.approx(sum(r["gb"] for r in p["rows"]) + 7.0 + 3.0 + 1.5 + 2.0)


def test_t3_projection_formula():
    t3 = {"epochs": 20, "hours_audio_per_epoch": 100.0, "segment_s": 3.0, "batch": 16}
    assert t3_hours(t3, 0.5) == pytest.approx(20 * (100 * 3600 / 3) / 16 * 0.5 / 3600)


def test_go_no_go_levels_come_from_frozen_budget():
    b = load_budget()
    rows = go_no_go(b, disk_projection_gb=40.0, free_gb=55.0,
                    step16={"peak_reserved_gb": 6.5, "s_per_step_median": 0.2}, loader={"rss_max_gb": 5.0})
    verdict = {r["item"]: r["verdict"] for r in rows}
    assert verdict == {"disco": "go", "vram_b16": "go", "ram_loader": "go", "tempo_t3": "go"}
    rows = go_no_go(b, disk_projection_gb=50.0, free_gb=55.0,
                    step16={"peak_reserved_gb": 7.5, "s_per_step_median": 2.0}, loader={"rss_max_gb": 7.0})
    verdict = {r["item"]: r["verdict"] for r in rows}
    assert verdict["disco"] == "no-go" and verdict["vram_b16"] == "no-go" and verdict["ram_loader"] == "no-go"
    assert verdict["tempo_t3"] == "no-go"


def test_markdown_lists_every_section():
    md = render_markdown({"title": "M0", "sections": [("Identidade", "I1 ok"), ("Disco", "x")],
                          "go_no_go": [{"item": "disco", "value": 1, "limit": 2, "verdict": "go"}]})
    assert "## Identidade" in md and "## Disco" in md and "| disco |" in md
```

- [ ] **Step 3: Rodar e ver falhar**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" -m pytest tests/test_m0_report.py -v 2>&1 | tail -3
```

Expected: FAIL com `ImportError`.

- [ ] **Step 4: Implementar `report.py`**

`src/cctrain/m0/report.py`:

```python
"""Relatório M0: projeções com fatores medidos e go/no-go pelos níveis pré-registrados (budget-v1)."""

from __future__ import annotations

from collections.abc import Mapping

from cctrain.eval.gates import Budget

GB = 1e9


def _factor(disk: Mapping, sr: int) -> tuple[float, str]:
    by_sr = {int(k): v["flac_gb_per_hour"] for k, v in disk["by_sr"].items()}
    if sr in by_sr:
        return by_sr[sr], "medido"
    lower = max((s for s in by_sr if s < sr), default=min(by_sr))
    upper = min((s for s in by_sr if s > sr), default=max(by_sr))
    if lower == upper:
        return by_sr[lower] * sr / lower, "interpolado"
    w = (sr - lower) / (upper - lower)
    return by_sr[lower] + w * (by_sr[upper] - by_sr[lower]), "interpolado"


def project_disk(minimal: Mapping, disk: Mapping) -> dict:
    rows = []
    for group in ("speech", "noise"):
        for item in minimal[group]:
            if "fixed_gb" in item:
                rows.append({"id": item["id"], "gb": float(item["fixed_gb"]), "factor_source": "fixo (pesquisa)"})
                continue
            factor, source = _factor(disk, int(item["sr"]))
            rows.append({"id": item["id"], "gb": item["hours"] * factor, "factor_source": source})
    rir = disk["rir"]
    per_rir = rir["stored_bytes"] / max(rir["count"], 1)
    rows.append({"id": "rir", "gb": minimal["rir"]["count"] * per_rir / GB, "factor_source": "medido"})
    dirs = disk["dirs_bytes"]
    measured = {k: (dirs.get(k) or 0) / GB for k in ("venv", "tract_check_target", "rustup")}
    estimated = dict(minimal["estimated_fixed_gb"])
    total = sum(r["gb"] for r in rows) + sum(measured.values()) + sum(estimated.values())
    return {"rows": rows, "measured_gb": measured, "estimated_gb": estimated, "total_gb": total}


def t3_hours(t3: Mapping, s_per_step: float) -> float:
    steps_per_epoch = t3["hours_audio_per_epoch"] * 3600.0 / t3["segment_s"] / t3["batch"]
    return t3["epochs"] * steps_per_epoch * s_per_step / 3600.0


def go_no_go(budget: Budget, disk_projection_gb: float, free_gb: float, step16: Mapping,
             loader: Mapping) -> list[dict]:
    hours = t3_hours(budget.t3, float(step16["s_per_step_median"])) if "s_per_step_median" in step16 else float("inf")
    time_verdict = ("go" if hours <= budget.t3["go_hours_max"]
                    else "go com ressalva" if hours <= budget.t3["caveat_hours_max"] else "no-go")
    vram = float(step16.get("peak_reserved_gb", float("inf")))
    return [
        {"item": "disco", "value": disk_projection_gb, "limit": free_gb - budget.reserve_gb,
         "verdict": "go" if disk_projection_gb <= free_gb - budget.reserve_gb else "no-go"},
        {"item": "vram_b16", "value": vram, "limit": budget.vram_b16_reserved_gb_max,
         "verdict": "go" if vram <= budget.vram_b16_reserved_gb_max else "no-go"},
        {"item": "ram_loader", "value": float(loader["rss_max_gb"]), "limit": budget.loader_rss_gb_max,
         "verdict": "go" if float(loader["rss_max_gb"]) <= budget.loader_rss_gb_max else "no-go"},
        {"item": "tempo_t3", "value": hours, "limit": budget.t3["go_hours_max"], "verdict": time_verdict},
    ]


def render_markdown(ctx: Mapping) -> str:
    lines = [f"# {ctx['title']}", ""]
    for heading, body in ctx["sections"]:
        lines += [f"## {heading}", "", body.rstrip(), ""]
    lines += ["## Go/no-go do orçamento (níveis de configs/m0/budget-v1.yaml)", "",
              "| item | valor | limite | veredito |", "|---|---:|---:|---|"]
    for r in ctx["go_no_go"]:
        lines.append(f"| {r['item']} | {r['value']:.3f} | {r['limit']:.3f} | {r['verdict']} |")
    return "\n".join(lines) + "\n"
```

- [ ] **Step 5: Implementar `scripts/m0_report.py` (inclui I2 no asset do próprio repo)**

`scripts/m0_report.py`:

```python
"""I2 no asset FiLM gerado pelo repo + relatório M0 (docs/m0-report.md e runs/m0/report.json)."""

import json
import shutil

import yaml

from cctrain import tractcheck
from cctrain.data.resample import V1_DIR
from cctrain.eval.gates import load_budget, load_gates
from cctrain.export.film import build_film_identity_asset
from cctrain.m0.report import go_no_go, project_disk, render_markdown
from cctrain.paths import Paths, clearcore_root

PATHS = Paths.from_env()
RUNS = PATHS.runs / "m0"


def _load(name: str) -> dict:
    p = RUNS / f"{name}.json"
    return json.loads(p.read_text()) if p.is_file() else {"status": "não executado"}


def i2_own() -> dict:
    root = clearcore_root()
    asset = build_film_identity_asset(root / "vendor/approved/df-compatible-release-asset-v1.bin", PATHS.build / "m0")
    out = RUNS / "i2_own.json"
    proc = tractcheck.run("identity", asset.path, root / "vendor/approved/df-compatible-release-asset-v1.bin",
                          root / "fixtures/film/upstream-golden-1000f.f32", out)
    rep = json.loads(out.read_text())
    rep.update(asset_sha256=asset.sha256, asset_size=asset.size, returncode=proc.returncode)
    out.write_text(json.dumps(rep, indent=2) + "\n")
    return rep


def fmt(obj) -> str:
    return "```json\n" + json.dumps(obj, indent=2, ensure_ascii=False, default=str) + "\n```"


def main() -> None:
    i2 = i2_own()
    runs = {n: _load(n) for n in ("env", "i1", "i2_clearcore_fixture", "film_asset", "gating_parity",
                                  "offline_vs_runtime", "ecapa_tract", "disk", "loader", "step_b16", "step_b32",
                                  "step_b16_teacher", "beta_rule")}
    gates, budget = load_gates(), load_budget()
    minimal = yaml.safe_load((PATHS.repo / "configs/data/minimal-set-v1.yaml").read_text())
    proj = project_disk(minimal, runs["disk"])
    already = sum(v or 0 for k, v in runs["disk"]["dirs_bytes"].items() if k in ("venv", "tract_check_target",
                                                                                  "rustup", "data")) / 1e9
    free_gb = shutil.disk_usage(PATHS.data).free / 1e9 + already
    verdicts = go_no_go(budget, proj["total_gb"], free_gb, runs["step_b16_teacher"], runs["loader"])
    resampler = json.loads((V1_DIR / "manifest.json").read_text())["coefficients"]
    sections = [
        ("Resumo", "\n".join(f"- {r['item']}: **{r['verdict']}**" for r in verdicts)
         + f"\n- I1: **{'aprovado' if runs['i1'].get('pass') else 'REPROVADO'}**; "
           f"I2 (asset do repo): **{'aprovado' if i2.get('pass') else 'REPROVADO'}**\n"
           f"- B fixado: {runs['beta_rule'].get('beta_bound')}\n"
           f"- Réplica do gating: {'paridade ok' if runs['gating_parity'].get('pass') else 'FALHOU'}\n"
           f"- ECAPA no tract: { {m: v.get('all_pass') for m, v in runs['ecapa_tract'].items()} }"),
        ("Ambiente", fmt(runs["env"])),
        ("Identidade (I1, I2)", fmt({"i1": runs["i1"], "i2_fixture_clearcore": runs["i2_clearcore_fixture"],
                                     "i2_asset_do_repo": i2, "film_asset": runs["film_asset"]})),
        ("Congelamentos", fmt({"gates_v1_sha256": gates.sha256, "budget_v1_sha256": budget.sha256,
                               "resampler_v1": resampler})),
        ("Réplica do gating × tract-check", fmt(runs["gating_parity"])),
        ("Emulação offline × runtime (dado para o M2)", fmt(runs["offline_vs_runtime"])),
        ("Protótipo fbank + ECAPA no tract", fmt({m: {"all_pass": v.get("all_pass"),
                                                     "falhas": [c for c in v.get("cases", []) if not c["passed"]]}
                                                 for m, v in runs["ecapa_tract"].items()})),
        ("Disco (fatores medidos e projeção do conjunto mínimo)", fmt({"medido": runs["disk"], "projecao": proj,
                                                                       "livre_mais_ja_usado_gb": free_gb})),
        ("Loader", fmt(runs["loader"])),
        ("Passo de treino na GPU", fmt({k: runs[k] for k in ("step_b16", "step_b32", "step_b16_teacher")})),
        ("Regra de B", fmt(runs["beta_rule"])),
        ("Pendências e decisões", "- CV 26.0: " + ("ingerido" if (PATHS.data / "manifests/cv26-ptbr.jsonl").is_file()
                                                   else "não executado (aceite dos termos pendente com o dono)")
         + "\n- Decisões de planejamento 1-15 do plano bootstrap+M0 valem como registradas lá."),
    ]
    md = render_markdown({"title": "Clearcore Train — relatório M0 (piloto)", "sections": sections,
                          "go_no_go": verdicts})
    (PATHS.repo / "docs").mkdir(exist_ok=True)
    (PATHS.repo / "docs/m0-report.md").write_text(md)
    (RUNS / "report.json").write_text(json.dumps({"go_no_go": verdicts, "disk_projection": proj, "i2_own": i2},
                                                 indent=2, default=str) + "\n")
    print(md)


if __name__ == "__main__":
    main()
```

- [ ] **Step 6: Rodar os testes, gerar o relatório e a verificação final**

```bash
source "$HOME/orca/projects/clearcore-train/scripts/env.sh" && cd "$CCTRAIN_WT"
"$CCTRAIN_PY" -m pytest tests/test_m0_report.py -v
"$CCTRAIN_PY" scripts/m0_report.py
"$CCTRAIN_PY" -c "import json,os; print(json.load(open(os.environ['CCTRAIN_RUNS_DIR']+'/m0/i2_own.json'))['pass'])"
"$CCTRAIN_PY" -m pytest tests -v -m "not network"
scripts/tract-check.sh test
"$CCTRAIN_PY" -m ruff check src tests scripts
```

Expected: `True` para I2 no asset do repo; a suíte inteira (exceto `network`) PASS; `docs/m0-report.md` com todas as seções e a tabela de go/no-go. Um "no-go" no relatório **não** é falha da tarefa: é o resultado do M0 para o dono decidir (ex.: liberar mais disco).

- [ ] **Step 7: Commit**

```bash
cd "$CCTRAIN_WT"
git add src/cctrain/m0/report.py scripts/m0_report.py configs/data/minimal-set-v1.yaml tests/test_m0_report.py docs/m0-report.md
git commit -m "docs(m0): I2 on the repo-built FiLM asset, M0 report and budget go/no-go"
```

---

## Próximos planos (M1-M5)

Cada marco terá plano próprio, escrito depois que o relatório M0 existir. O que cada um precisa do M0:

- **M1 encoder (T1).** Precisa: o modo de pooling que passou no tract (`ecapa_tract.json`) e os parâmetros do fbank para congelar; `gates-v1.yaml` já congelado (níveis do M1); fatores FLAC por SR e projeção de disco para dimensionar LibriSpeech (teto 4 min/falante), VCTK, CML-TTS e CV 27.0; vazão do loader (amostras/s) para estimar as 10-20 h do T1; a decisão sobre o CV 26/27 (aceite dos termos pelo dono) e o filtro de `client_id` da §4.6; a validação de enrollment em Python com paridade contra o Rust (`validate_enrollment_audio`), que o M0 não cobriu; splits por falante (`data/splits.py`) e o teste de disjunção.
- **M2 pDFNet3 (T2 sonda, T3, AB2).** Precisa: `B` fixado (`film-v1.yaml`); s/passo e VRAM em lote 16/32, com e sem professor, para escolher o lote e o cronograma; o desvio entre a emulação offline do gating e a réplica (`offline_vs_runtime.json`), que decide se a avaliação de dev pode usar o forward PyTorch ou precisa de grafos stateful por checkpoint; a paridade da réplica (`gating_parity.json`); I1 aprovado; a perda real com paridade contra `df/loss.py` (o M0 só mediu um proxy de custo); a definição de `max_freq` a partir da banda efetiva; RAM do loader para fixar workers/prefetch.
- **M3 export.** Precisa: `add_film` e o empacotamento reprodutível (Task 6) e I2 sobre o asset do repo (Task 18); o resultado do protótipo do enrollment no tract (ops aceitas, `BatchNormalization` não fundida); a decisão sobre a dependência git do `tract-check` (commit publicado pelo dono ou checkout local); para o estágio F, o ambiente `export` (Python 3.10, torch 1.13.1), que o M0 não criou.
- **M4 avaliação.** Precisa: `gates-v1.yaml` (congelado no M0), a réplica do gating e o `tract-check gating` (frações por estágio), o resampler v1 e seus vetores dourados, e a projeção de disco para decidir se o G8 (DNS5 dev) cabe.
- **M5 entrega.** Precisa: o pacote reprodutível (Task 6), os coeficientes e goldens do resampler (Task 4), o rascunho de proveniência com os SHA-256 dos manifests e do relatório, e as decisões do dono sobre governança (itens 5-6) e sobre a licença `LicenseRef-cctrain-generated` das RIR.

---

## Autorrevisão contra a spec

- **Cobertura do M0 (§9.2):** ambientes (Tasks 1-2); ~1 h de dados com VCTK por HTTP Range, LibriSpeech dev-clean, CV 26.0 condicional, os 3 ruídos CC0 do Clearcore e RIR sintéticas (Tasks 11-12); `add_film` sobre o asset aprovado (Task 6); protótipo do fbank + ECAPA com pooling no tract (Tasks 9, 14, 16); réplica do gating com paridade (Tasks 14-15); resampler v1 (Task 4); medidas que substituem os [est.]: fator FLAC por SR, amostras/s, s/passo e VRAM em lote 16/32, RSS, temperatura (Task 17); `B` pela regra da §3.2 (Task 17); I1 (Task 8) e I2 (Tasks 2 e 18); go/no-go do orçamento (Tasks 5 e 18). Pedidos do orquestrador: allowlist e manifests com hash (Tasks 3, 10, 12), `gates-v1.yaml` congelado com carregador (Task 5), disco persistente e falha fechada (Tasks 1, 2, 10), sem caminhos pessoais (Task 1, `test_no_personal_path_in_sources`).
- **Fora do M0 de propósito:** ambiente `export` (só estágio F, M3); `splits.py`, `cv_filter.py`, `enroll.py` (M1); perdas reais, checkpoints e treino (M2); stateful com FiLM (§6.4).
- **Placeholders:** nenhum "TBD"; os únicos pontos adaptativos são verificações de assinatura de API do upstream (`init_df`, trechos do patch FiLM), com o comando exato para conferir e a regra de parada.
- **Consistência de tipos:** `FileRecord` (Task 3) é o mesmo em 11, 12, 13, 17; `FilmConfig`/`FilmGenerator`/`FilmVectors` (Task 7) em 8, 9, 17; `PackResult`/`read_members` (Task 6) em 18; `ReplicaResult`/`run_replica`/`StatefulOrtFrameModel`/`parity_mixture` (Task 15) em 17; `load_budget`/`Budget` (Task 5) em 18; códigos de estágio iguais no Rust (`gating.rs`) e no Python (`Stage`).
