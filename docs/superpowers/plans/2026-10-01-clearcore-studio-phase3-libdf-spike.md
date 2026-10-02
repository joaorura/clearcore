# Fase 3: spike do fork do libDF com paridade — plano

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Responder, com evidência literal, se um fork do libDF (`deep_filter` v0.5.6, commit já pinado) consegue alimentar entradas extras constantes `gamma`/`beta` (FiLM) nos grafos `enc` e `df_dec` com paridade contra o upstream quando `gamma=1, beta=0`, a que custo, e se uma cabeça de EQ pode ser aplicada no espectro antes do iSTFT.

**Architecture:** Tudo roda numa pasta de spike fora do repositório (`$SPIKE`, no scratchpad da sessão): uma cópia intocada do libDF (`libdf-pristine`), uma cópia com o patch (`libdf-fork`), um harness Rust que carrega as duas por feature e um script Python que edita os ONNX do asset aprovado. O repositório e o cache do cargo não são tocados; a única escrita no repositório é o relatório (T7). Cada tarefa é uma PERGUNTA com veredito GO / NO-GO e a saída literal dos comandos como evidência.

**Tech Stack:** Rust 1.90.0 (toolchain fixado em `rust-toolchain.toml`), `deep_filter` 0.5.6 @ `978576aa8400552a4ce9730838c635aa30db5e61`, `tract-*` 0.19.16, `ndarray` 0.15.6, Python 3.14 + `onnx` + `onnxruntime` + `numpy` num venv do spike.

**Spec:** `docs/superpowers/specs/2026-10-01-clearcore-studio-pipeline-design.md` (seções 2, 7 e 10; fase 3 da seção 9).

## Global Constraints

- Latência algorítmica do modelo: 1.440 amostras / 30 ms; contrato do modelo `960/480/32/96/5/2` (fft/hop/nb_erb/nb_df/df_order/lookahead), 48 kHz mono, hop 480 (`crates/model/src/tract_backend.rs:156-174`). O patch não pode alterar esse contrato.
- Runtime: `tract` 0.19.16 (fixado com `=0.19.16` em `crates/model/Cargo.toml`).
- Fork "no commit já pinado, com patch mínimo para alimentar `gamma/beta` quando o grafo declarar esses inputs"; "Spike (fase 3) com teste de paridade (gamma=1, beta=0 contra o upstream) decide a viabilidade. Conferir a licença do código do libDF." (spec seção 7).
- Terceira variante a avaliar no spike: "cabeça de EQ dentro do pDFNet3, aplicada no espectro do libDF antes do iSTFT" (spec seção 7).
- O gate de assets existente (SHA-256, tamanho, assinatura Ed25519, allowlist de 4 membros) não é tocado. O agente não produz assinaturas de manifest.
- Execução autônoma: **sem commit e sem push**. Cada tarefa termina com um Checkpoint (`git status --short`), nunca com `git commit`.
- Não tocar o repositório (exceto o arquivo de relatório da T7) nem o cache do cargo: `CARGO_HOME` e `RUSTUP_HOME` ficam dentro de `$SPIKE`. `~/.cargo` não pode mudar.
- Áudio nunca vai para o git. O spike usa só sinal sintético gerado em código e o `fixtures/golden/frozen-reference.json` já versionado (somente leitura).
- Política do workspace Clearcore: `unsafe_code = "forbid"`, `unwrap/expect/panic = deny`. O libDF contém `unsafe` (`Tensor::uninitialized_dt`, `tract.rs:295-299`); o fork nunca pode virar membro do workspace sem tratar isso (risco registrado na T7, não resolvido no spike).
- Implementação e revisão por agentes em Sonnet e Haiku (Opus só em caso raro).
- Respostas e texto em português do Brasil; identificadores e código em inglês.

## Estado do ambiente verificado em 2026-10-01 (ao escrever este plano)

Fatos medidos por comandos de leitura, para a T1 reconferir (o ambiente pode ter mudado):

- `cargo`, `rustc` e `rustup` **não estão no PATH** e não há `~/.rustup`; `~/.cargo` tem 8 KB (só `CACHEDIR.TAG` em `git/` e `registry/`); não há `~/.cargo/git/checkouts`.
- `vendor/` só contém `vendor/approved/df-compatible-release-asset-v1.bin` (tar.gz com `tmp/export/{enc,erb_dec,df_dec}.onnx` + `config.ini`; SHA-256 `c94d91f70911001c946e0fabb4aa9adc37045f45a03b56008cb0c8244cb63616`).
- `find / -xdev` por `tract.rs`, `libDF`, `deep_filter*`, `DeepFilterNet*`, `*.crate` e `tract-*`: **nenhum resultado**. O código-fonte do libDF **não existe localmente**. O `target/` do repositório (3,8 GB) tem artefatos compilados e `.fingerprint/deep_filter-*`, mas nenhum código-fonte.
- `python3` 3.14.7 tem `torch` 2.13.0+cu130 e `numpy` 2.5.2; **não tem** `onnx`, `onnxruntime`, `onnxscript`, `onnx_graphsurgeon`.
- A rede responde (GitHub, PyPI, static.rust-lang.org, static.crates.io: HTTP 200), mas a política do projeto é offline (`scripts/check-offline.sh`). Baixar qualquer coisa exige autorização do usuário.
- `ffmpeg`, `gcc`, `cc`, `ninja`, `uv` 0.12.7 existem. CPU: Intel Core Ultra 7 265H, 16 CPUs (híbrida: núcleos P e E).
- Licença do libDF (lida de `LICENSE` e `libDF/Cargo.toml` no commit pinado, baixados só para leitura): `license = "MIT/Apache-2.0"`; `LICENSE` diz "Licensed under either of Apache License, Version 2.0 ... or MIT license ... at your option."; `LICENSE-MIT` começa com "Copyright (c) 2021 Hendrik Schröter". `THIRD_PARTY_LICENSES` do repositório registra a decisão do dono de tratar pesos e código como `MIT OR Apache-2.0`.
- Pontos de código no commit pinado (`libDF/src/tract.rs`, 1065 linhas, SHA-256 `ad809ddb10e489990e572a8e97d79394df762c6f9c16a8defc9484d03c94426b`): `DfParams::from_targz` 37-67; `DfTract::new` 237; `into_runnable` + `SimpleState::new` 255-257; `init_encoder_impl` 754 (facts nas linhas 764-775); `init_df_decoder_impl` 915 (facts 929-943); `PulsedModel::new(..., &1.to_dim())` 782/849/950; `process_raw` 435 com `enc.run` 454, `erb_dec.run` 480, `df_dec.run` 492; `process` 503; post-filter 601-608; `atten_lim` 610-614; loop de `synthesis` 616-625; `get_spec_enh`/`get_mut_spec_enh` 682-695.
- O estado das GRUs vive dentro dos três `TractModel = TypedSimpleState<...>` (campos `enc`, `erb_dec`, `df_dec` de `DfTract`): o `Scan` pulsado guarda `hidden_state` no `OpState` (`tract-core-0.19.16/src/ops/scan/lir.rs:83,197`), inicializado na primeira execução. `DfTract::init()` não o reinicia.
- Os grafos ONNX (opset 12, IR 7): `enc` tem entradas `feat_erb [1,1,S,32]` e `feat_spec [1,2,S,96]`; `df_dec` tem `emb [1,S,512]` e `c0 [1,64,S,96]`. Cada um tem uma GRU alimentada por `Relu -> Transpose`; o ponto de FiLM natural é a saída desse `Relu`, de forma `[1,1,256]` em `S=1` (`/emb_gru/linear_in/1/Relu_output_0` no `enc`; `/df_gru/linear_in/linear_in.1/Relu_output_0` no `df_dec`). Um protótipo em `onnx`+`onnxruntime` inserindo `Mul(gamma)`+`Add(beta)` com `gamma=1, beta=0` deu saída **bit-exata** nos dois grafos e saída diferente com `gamma=1.5`.
- Achados do `tract` 0.19.16 que condicionam a T4: (1) `with_input_names([...])` **substitui** a lista de entradas (`tract-core .../graph.rs:162-174`), então um grafo com 4 entradas carregado pelo `init_*_impl` atual perde `gamma/beta`; (2) `PulsedFact::from_tensor_fact_pulse` falha com "Can not pulse a tensor with no streaming dim" para qualquer `Source` sem o símbolo `S` (`tract-pulse .../fact.rs:39-56`), então `gamma/beta` precisam de fact `[n_ch, S, 256]`; (3) o pulsifier alinha atrasos de entradas de operações binárias com `sync_inputs` (`tract-pulse .../model.rs:140-170`): no `enc` o `Relu` do ponto de FiLM vem depois de convoluções com lookahead (atraso > 0) e `gamma/beta` têm atraso 0, então pode entrar um `Delay` com buffer inicial zero nas primeiras `conv_lookahead` quadros, e a paridade pode falhar só no início. No `df_dec` o ponto vem de `emb` (atraso 0) e não deve sofrer isso. **Nada disso foi executado em Rust** (não há toolchain); a T4 mede.
- `fixtures/golden/frozen-reference.json` (versionado): 6 casos x 50 frames de 480 amostras, gerado por `TractBackend` com `deep_filter-v0.5.6` + `tract 0.19.16`, tolerância `absolute 0.0001, relative 0.0001`. O gerador (`crates/tools/src/bin/generate-golden.rs:52,106-130`) cria **um único** backend e processa os casos em ordem, sem reinício: o replay da T2 deve fazer o mesmo.

## Estrutura de arquivos

Fora do repositório (`SPIKE=<SCRATCHPAD>/spike-libdf`):

- `$SPIKE/upstream/` clone esparso do upstream no commit pinado (somente leitura depois da T1).
- `$SPIKE/libdf-pristine/` cópia de `libDF/` + `LICENSE*`, sem alteração (T2).
- `$SPIKE/libdf-fork/` cópia de `libDF/` + `LICENSE*` com o patch (T4).
- `$SPIKE/libdf-eqhook/` cópia de `libDF/` + o gancho de espectro (T6).
- `$SPIKE/harness/` crate Rust com os subcomandos `golden`, `replay`, `film`, `bench` (T2, T4, T5, T6).
- `$SPIKE/scripts/gen_film_onnx.py` e `$SPIKE/scripts/compare.py` (T3, T4).
- `$SPIKE/asset/` extração do asset aprovado; `$SPIKE/asset-stock.tar.gz` cópia dele; `$SPIKE/out/` ONNX e tar.gz com FiLM.
- `$SPIKE/evidence/` saídas literais (`T1-*.txt`, `T2-*.txt`, ...), citadas no relatório.
- `$SPIKE/cargo/`, `$SPIKE/rustup/` toolchain isolada.

No repositório: apenas `docs/superpowers/plans/2026-10-01-clearcore-studio-phase3-libdf-spike-report.md` (T7).

Cada bloco `bash` abaixo começa com as variáveis, porque o shell não persiste entre chamadas.

---

### Task 1: Inventário do ambiente — o libDF está disponível offline?

**Pergunta Q1:** o código-fonte do libDF (`deep_filter` @ `978576aa...`), a toolchain Rust 1.90.0 e as ferramentas Python/ONNX estão disponíveis **offline** nesta máquina? Se não, existe um caminho autorizado para obtê-los só dentro de `$SPIKE`?

**Files:**
- Create: `$SPIKE/evidence/T1-inventory.txt`, `$SPIKE/evidence/T1-license.txt`, `$SPIKE/evidence/T1-fetch.txt`, `$SPIKE/evidence/T1-python.txt`
- Create (cópias): `$SPIKE/libdf-pristine/`, `$SPIKE/asset-stock.tar.gz`, `$SPIKE/asset/`

**Interfaces:**
- Consumes: nada.
- Produces: `$SPIKE/libdf-pristine/` (libDF sem alteração, com `LICENSE`, `LICENSE-MIT`, `LICENSE-APACHE` ao lado de `libDF/`'s `Cargo.toml`), `$SPIKE/asset-stock.tar.gz` (cópia byte a byte do asset aprovado), `$SPIKE/asset/tmp/export/{enc,erb_dec,df_dec}.onnx` e `config.ini`, toolchain em `$SPIKE/cargo/bin`, venv em `$SPIKE/venv`, veredito Q1.

- [ ] **Step 1: Reconferir o inventário offline (somente leitura)**

```bash
export REPO=<REPO>
export SPIKE=<SCRATCHPAD>/spike-libdf
mkdir -p $SPIKE/evidence
{
  echo "## toolchain"; command -v cargo rustc rustup || echo "NENHUM no PATH"; ls -A ~/.rustup 2>&1 | head -2
  echo "## cache do cargo"; find ~/.cargo -maxdepth 3 | head -20; du -sh ~/.cargo
  echo "## vendor"; find $REPO/vendor -type f | head
  echo "## busca do fonte do libDF"
  find / -xdev \( -name tract.rs -o -iname libDF -o -iname 'deep_filter*' -o -iname 'DeepFilterNet*' -o -name '*.crate' \) -not -path '/proc/*' 2>/dev/null | head -20
  echo "(fim da busca)"
  echo "## pin"; grep -n 'deep_filter' $REPO/crates/model/Cargo.toml
  echo "## python"; python3 --version
  for m in onnx onnxruntime numpy torch; do python3 -c "import $m; print('$m', $m.__version__)" 2>&1 | tail -1; done
  echo "## rede (so cabecalho)"; for u in https://github.com https://pypi.org/simple/onnx/ https://static.rust-lang.org https://static.crates.io; do echo -n "$u "; timeout 8 curl -sI "$u" | head -1; done
  echo "## cpu"; lscpu | grep -E 'Model name|^CPU\(s\)'; lscpu -e=CPU,CORE,MAXMHZ | head -20
} 2>&1 | tee $SPIKE/evidence/T1-inventory.txt
```

Expected (estado de 2026-10-01): "NENHUM no PATH" para cargo/rustc/rustup; `~/.cargo` com 8,0K; a busca do fonte sem linhas entre os dois marcadores; `onnx`/`onnxruntime` com `ModuleNotFoundError`. Se alguma dessas respostas mudou (por exemplo, o fonte do libDF aparecer), use o que existir em vez de baixar e anote no relatório.

- [ ] **Step 2: Registrar o veredito offline de Q1**

Se a busca do fonte do libDF ficou vazia **ou** não há `cargo`, o veredito offline é **NO-GO** ("`BLOCKED_OFFLINE_DEPENDENCY`: libDF, toolchain 1.90.0 e `onnx` ausentes"). Escreva-o em `$SPIKE/evidence/T1-verdict.txt` com a linha literal de cada ausência (cole a saída do Step 1).

```bash
export SPIKE=<SCRATCHPAD>/spike-libdf
printf 'Q1 offline: NO-GO (BLOCKED_OFFLINE_DEPENDENCY)\nmotivo: ver T1-inventory.txt (cargo/rustc ausentes; fonte do libDF ausente; onnx ausente)\n' | tee $SPIKE/evidence/T1-verdict.txt
```

- [ ] **Step 3: Portão de autorização (decisão do orquestrador, não do executor)**

O restante do spike (T2 a T6) exige baixar, **apenas para dentro de `$SPIKE`**: a toolchain 1.90.0 (static.rust-lang.org), o upstream no commit pinado (GitHub), as crates do `Cargo.lock` (static.crates.io) e `onnx`/`onnxruntime`/`numpy` (PyPI). O executor deve pedir essa autorização ao orquestrador, que a pede ao usuário, e **parar aqui** se ela não vier: nesse caso o relatório da T7 registra Q1 = NO-GO, T2 a T6 = "não executadas (sem autorização de rede)" e recomenda que o usuário forneça o fonte do libDF e a toolchain por outro meio. Só prossiga com resposta afirmativa explícita.

- [ ] **Step 4: Obter a toolchain isolada (após autorização)**

```bash
export SPIKE=<SCRATCHPAD>/spike-libdf
export RUSTUP_HOME=$SPIKE/rustup CARGO_HOME=$SPIKE/cargo
mkdir -p $SPIKE/dl && cd $SPIKE/dl
curl --proto '=https' --tlsv1.2 -sSf -o rustup-init https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init
chmod +x rustup-init
./rustup-init -y --no-modify-path --profile minimal --default-toolchain 1.90.0 2>&1 | tail -5
$CARGO_HOME/bin/rustc --version | tee $SPIKE/evidence/T1-fetch.txt
ls -A ~/.cargo   # deve continuar so com .package-cache git registry (nada novo)
```

Expected: `rustc 1.90.0 (...)`. Se o `rustup-init` falhar, registre a saída literal em `T1-fetch.txt` e vá ao Step 7 com Q1 = NO-GO.

- [ ] **Step 5: Obter o libDF no commit pinado e conferir integridade**

```bash
export SPIKE=<SCRATCHPAD>/spike-libdf
REV=978576aa8400552a4ce9730838c635aa30db5e61
git clone --filter=blob:none --no-checkout https://github.com/Rikorose/DeepFilterNet.git $SPIKE/upstream
git -C $SPIKE/upstream sparse-checkout set --no-cone /libDF /LICENSE /LICENSE-MIT /LICENSE-APACHE
git -C $SPIKE/upstream checkout $REV
{
  echo "HEAD=$(git -C $SPIKE/upstream rev-parse HEAD)"; echo "ESPERADO=$REV"
  sha256sum $SPIKE/upstream/libDF/src/tract.rs $SPIKE/upstream/libDF/src/lib.rs
  echo "ESPERADO tract.rs=ad809ddb10e489990e572a8e97d79394df762c6f9c16a8defc9484d03c94426b"
  wc -l $SPIKE/upstream/libDF/src/tract.rs
} 2>&1 | tee -a $SPIKE/evidence/T1-fetch.txt
mkdir -p $SPIKE/libdf-pristine
cp -r $SPIKE/upstream/libDF/. $SPIKE/libdf-pristine/
cp $SPIKE/upstream/LICENSE $SPIKE/upstream/LICENSE-MIT $SPIKE/upstream/LICENSE-APACHE $SPIKE/libdf-pristine/
diff -r $SPIKE/upstream/libDF $SPIKE/libdf-pristine --exclude='LICENSE*' && echo "pristine identico ao upstream"
```

Expected: `HEAD` igual a `ESPERADO`; SHA-256 de `tract.rs` igual ao esperado e 1065 linhas; `pristine identico ao upstream`. Se o SHA-256 ou o `HEAD` divergir, **pare** (NO-GO de integridade): o fonte obtido não é o que o repositório pina.

- [ ] **Step 6: Ler e registrar a licença literalmente**

```bash
export SPIKE=<SCRATCHPAD>/spike-libdf
{
  echo "### LICENSE (raiz do upstream no commit pinado)"; cat $SPIKE/upstream/LICENSE
  echo; echo "### libDF/Cargo.toml"; grep -n -E '^(name|version|license|authors|edition)' $SPIKE/upstream/libDF/Cargo.toml
  echo; echo "### LICENSE-MIT (cabecalho)"; head -3 $SPIKE/upstream/LICENSE-MIT
  echo; echo "### LICENSE-APACHE (cabecalho)"; head -4 $SPIKE/upstream/LICENSE-APACHE
  echo; echo "### repositorio Clearcore (decisao do dono)"; sed -n 1,10p <REPO>/THIRD_PARTY_LICENSES
} 2>&1 | tee $SPIKE/evidence/T1-license.txt
```

Expected: `license = "MIT/Apache-2.0"` e o texto "Licensed under either of ... Apache License, Version 2.0 ... or MIT license ... at your option." O relatório deve citar esse texto **literalmente** e dizer que o fork precisa manter `LICENSE-MIT` e `LICENSE-APACHE`, o aviso "Copyright (c) 2021 Hendrik Schröter" e, por exigência da Apache-2.0 (seção 4(b)), marcar os arquivos modificados. Isto é leitura de licença, não parecer jurídico; a decisão do dono já está em `THIRD_PARTY_LICENSES`.

- [ ] **Step 7: Preparar Python (após autorização) e o asset**

```bash
export REPO=<REPO>
export SPIKE=<SCRATCHPAD>/spike-libdf
cd $SPIKE
uv venv --python 3.14 venv
VIRTUAL_ENV=$SPIKE/venv uv pip install onnx onnxruntime numpy 2>&1 | tail -10 | tee $SPIKE/evidence/T1-python.txt
$SPIKE/venv/bin/python -c "import onnx, onnxruntime, numpy; print('onnx', onnx.__version__, 'ort', onnxruntime.__version__, 'numpy', numpy.__version__)" | tee -a $SPIKE/evidence/T1-python.txt
cp $REPO/vendor/approved/df-compatible-release-asset-v1.bin $SPIKE/asset-stock.tar.gz
sha256sum $SPIKE/asset-stock.tar.gz | tee -a $SPIKE/evidence/T1-python.txt
mkdir -p $SPIKE/asset && tar xzf $SPIKE/asset-stock.tar.gz -C $SPIKE/asset && find $SPIKE/asset -type f | sort | tee -a $SPIKE/evidence/T1-python.txt
```

Expected: linha `onnx 1.x ort 1.x numpy 2.x`; SHA-256 `c94d91f70911001c946e0fabb4aa9adc37045f45a03b56008cb0c8244cb63616`; quatro arquivos em `$SPIKE/asset/tmp/export/` (`config.ini`, `df_dec.onnx`, `enc.onnx`, `erb_dec.onnx`).

- [ ] **Step 8: Veredito final de Q1**

Escreva em `$SPIKE/evidence/T1-verdict.txt` (sobrescrevendo) duas linhas: `Q1 offline: NO-GO` (com a causa) e `Q1 com fetch autorizado para $SPIKE: GO` se os Steps 4 a 7 passaram (HEAD e SHA-256 conferem, `rustc 1.90.0`, `onnx` importa), ou `NO-GO` com o ponto de falha. A consequência para a fase 4 vai para o relatório: **o build offline do produto exige vendorizar o fork dentro do repositório** (decisão fora do spike).

- [ ] **Step 9: Checkpoint**

```bash
cd <REPO> && git status --short
```

Expected: só `?? docs/superpowers/plans/2026-10-01-clearcore-studio-phase3-libdf-spike.md` e o spec já pendente (`?? docs/superpowers/specs/2026-10-01-clearcore-studio-pipeline-design.md`). Liste os arquivos alterados na saída da tarefa. **Não commitar.**

---

### Task 2: Cópia intocada do libDF compila e reproduz o upstream (golden local)

**Pergunta Q2:** a cópia intocada do libDF compila fora do repositório, é determinística no sinal sintético e reproduz, dentro da tolerância do próprio repositório (`1e-4` absoluto e relativo), a saída do `fixtures/golden/frozen-reference.json`?

**Files:**
- Create: `$SPIKE/harness/Cargo.toml`, `$SPIKE/harness/src/main.rs`, `$SPIKE/scripts/compare.py`
- Create: `$SPIKE/evidence/T2-build.txt`, `$SPIKE/evidence/T2-golden.txt`, `$SPIKE/evidence/T2-replay.txt`, `$SPIKE/golden-pristine.f32`
- Read-only: `$REPO/fixtures/golden/frozen-reference.json`, `$REPO/Cargo.lock`

**Interfaces:**
- Consumes: `$SPIKE/libdf-pristine/`, `$SPIKE/asset-stock.tar.gz`, toolchain em `$SPIKE/cargo/bin` (T1).
- Produces: harness com CLI `harness golden <asset.tar.gz> <out.f32> [frames]`, `harness replay <asset.tar.gz> <frozen-reference.json>`, `harness film <asset.tar.gz> <out.f32> <none|identity|scaled> [frames]` (modos diferentes de `none` só no build `fork`), `harness bench <asset.tar.gz> <frames> <none|identity|scaled>`; arquivo `$SPIKE/golden-pristine.f32` (saída da cópia intocada com o asset aprovado, 1000 frames, f32 little-endian) e `$SPIKE/scripts/compare.py <a.f32> <b.f32>`.

- [ ] **Step 1: Escrever o `Cargo.toml` do harness**

Cria `$SPIKE/harness/Cargo.toml`:

```toml
[package]
name = "spike-harness"
version = "0.0.0"
edition = "2021"
publish = false

[workspace]

[features]
default = ["pristine"]
pristine = ["dep:df_pristine"]
fork = ["dep:df_fork"]

[dependencies]
df_pristine = { path = "../libdf-pristine", package = "deep_filter", default-features = false, features = ["tract"], optional = true }
df_fork = { path = "../libdf-fork", package = "deep_filter_fork", default-features = false, features = ["tract"], optional = true }
ndarray = { version = "=0.15.6", default-features = false, features = ["std"] }
serde_json = "=1.0.145"

[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
```

O fork é renomeado `deep_filter_fork` porque dois pacotes de caminho com o mesmo nome e versão colidem no `Cargo.lock`. O Cargo precisa resolver as duas dependências opcionais mesmo no build `pristine`, então o Step 4 abaixo cria `libdf-fork` (cópia do pristine só com o nome trocado, sem patch) **antes** do primeiro `cargo build`; a T4 aplica o patch nela.

- [ ] **Step 2: Escrever o harness**

Cria `$SPIKE/harness/src/main.rs`:

```rust
use std::{env, fs, path::PathBuf, process::exit, time::Instant};

use ndarray::{Array2, ArrayView2};

#[cfg(feature = "fork")]
use df_fork::tract::{DfParams, DfTract, FilmVectors, RuntimeParams};
#[cfg(not(feature = "fork"))]
use df_pristine::tract::{DfParams, DfTract, RuntimeParams};

const HOP: usize = 480;

/// Deterministic speech-like signal: 140 Hz harmonic stack with a 3 Hz envelope plus LCG noise.
fn synthetic(frames: usize) -> Vec<f32> {
    let mut state: u32 = 0x1234_5678;
    (0..frames * HOP)
        .map(|i| {
            let t = i as f64 / 48_000.0;
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

fn load(asset: &str) -> DfTract {
    let params = DfParams::new(PathBuf::from(asset)).expect("DfParams::new");
    DfTract::new(params, &RuntimeParams::default()).expect("DfTract::new")
}

fn process_one(model: &mut DfTract, frame: &[f32]) -> Vec<f32> {
    let input = ArrayView2::from_shape((1, HOP), frame).expect("input shape");
    let mut output = Array2::<f32>::zeros((1, HOP));
    model.process(input, output.view_mut()).expect("process");
    output.into_raw_vec()
}

fn run(model: &mut DfTract, signal: &[f32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(signal.len());
    for frame in signal.chunks_exact(HOP) {
        out.extend(process_one(model, frame));
    }
    out
}

fn write_f32(path: &str, data: &[f32]) {
    let bytes: Vec<u8> = data.iter().flat_map(|x| x.to_le_bytes()).collect();
    fs::write(path, bytes).expect("write output");
}

#[cfg(feature = "fork")]
fn apply_film(model: &mut DfTract, mode: &str) {
    if mode == "none" {
        return;
    }
    let hidden = model.film_hidden().expect("model graphs do not declare gamma/beta");
    let (gamma, beta) = if mode == "scaled" { (1.5f32, 0.1f32) } else { (1.0, 0.0) };
    let vectors = FilmVectors {
        gamma_enc: vec![gamma; hidden],
        beta_enc: vec![beta; hidden],
        gamma_df: vec![gamma; hidden],
        beta_df: vec![beta; hidden],
    };
    model.set_film(&vectors).expect("set_film");
}

#[cfg(not(feature = "fork"))]
fn apply_film(_model: &mut DfTract, mode: &str) {
    if mode != "none" {
        eprintln!("film mode '{mode}' requires the fork build (--no-default-features --features fork)");
        exit(2);
    }
}

fn replay(asset: &str, json_path: &str) {
    let doc: serde_json::Value =
        serde_json::from_slice(&fs::read(json_path).expect("read json")).expect("parse json");
    let mut model = load(asset); // one backend for all cases, like generate-golden
    let (mut worst_abs, mut worst_excess, mut frames) = (0f64, f64::MIN, 0usize);
    for case in doc["cases"].as_array().expect("cases") {
        let inputs = case["input_frames"].as_array().expect("input_frames");
        let outputs = case["output_frames"].as_array().expect("output_frames");
        for (fin, fout) in inputs.iter().zip(outputs) {
            let x: Vec<f32> =
                fin.as_array().expect("frame").iter().map(|v| v.as_f64().expect("f64") as f32).collect();
            let y = process_one(&mut model, &x);
            for (got, want) in y.iter().zip(fout.as_array().expect("frame")) {
                let want = want.as_f64().expect("f64");
                let diff = (f64::from(*got) - want).abs();
                worst_abs = worst_abs.max(diff);
                worst_excess = worst_excess.max(diff - (1e-4 + 1e-4 * want.abs()));
            }
            frames += 1;
        }
    }
    println!(
        "replay frames={frames} max_abs_diff={worst_abs:e} max_excess_over_tol={worst_excess:e} within_tol={}",
        worst_excess <= 0.0
    );
}

fn bench(asset: &str, frames: usize, mode: &str) {
    let mut model = load(asset);
    apply_film(&mut model, mode);
    let signal = synthetic(frames + 500);
    let mut chunks = signal.chunks_exact(HOP);
    for frame in chunks.by_ref().take(500) {
        process_one(&mut model, frame); // warm-up
    }
    let mut ns: Vec<u64> = Vec::with_capacity(frames);
    for frame in chunks {
        let start = Instant::now();
        let out = process_one(&mut model, frame);
        ns.push(start.elapsed().as_nanos() as u64);
        std::hint::black_box(out);
    }
    ns.sort_unstable();
    let pick = |q: f64| ns[((ns.len() - 1) as f64 * q) as usize];
    let mean = ns.iter().sum::<u64>() as f64 / ns.len() as f64;
    println!(
        "bench mode={mode} frames={} mean_ns={mean:.0} p50_ns={} p95_ns={} p99_ns={} max_ns={}",
        ns.len(),
        pick(0.50),
        pick(0.95),
        pick(0.99),
        ns[ns.len() - 1]
    );
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["golden", asset, out] | ["golden", asset, out, _] => {
            let frames = args.get(3).map_or(1000, |f| f.parse().expect("frames"));
            let mut model = load(asset);
            write_f32(out, &run(&mut model, &synthetic(frames)));
            println!("golden frames={frames} out={out}");
        }
        ["film", asset, out, mode] | ["film", asset, out, mode, _] => {
            let frames = args.get(4).map_or(1000, |f| f.parse().expect("frames"));
            let mut model = load(asset);
            apply_film(&mut model, mode);
            write_f32(out, &run(&mut model, &synthetic(frames)));
            println!("film mode={mode} frames={frames} out={out}");
        }
        ["replay", asset, json] => replay(asset, json),
        ["bench", asset, frames, mode] => bench(asset, frames.parse().expect("frames"), mode),
        _ => {
            eprintln!("usage: harness golden|film|replay|bench ... (see plan T2)");
            exit(2);
        }
    }
}
```

O harness é código descartável do spike (fora do workspace do Clearcore): `unwrap/expect` são aceitáveis aqui. O autor do plano **não compilou** este código (não havia toolchain); corrija erros de compilação sem mudar o comportamento descrito.

- [ ] **Step 3: Escrever `compare.py`**

Cria `$SPIKE/scripts/compare.py`:

```python
#!/usr/bin/env python3
"""Compara duas saidas f32 little-endian do harness (480 amostras por frame)."""
import sys
import numpy as np

a = np.fromfile(sys.argv[1], "<f4")
b = np.fromfile(sys.argv[2], "<f4")
if a.shape != b.shape:
    raise SystemExit(f"tamanhos diferentes: {a.shape} vs {b.shape}")
d = np.abs(a - b)
per_frame = d.reshape(-1, 480).max(axis=1)
nz = np.nonzero(per_frame)[0]
rms = float(np.sqrt(np.mean(a.astype("f8") ** 2)))
err = float(np.sqrt(np.mean(d.astype("f8") ** 2)))
print(
    f"samples={a.size} bit_exact={bool(np.array_equal(a, b))} max_abs_diff={d.max():.3e} "
    f"frames_with_diff={len(nz)} first_diff_frame={int(nz[0]) if len(nz) else -1} "
    f"last_diff_frame={int(nz[-1]) if len(nz) else -1}"
)
print("max_abs_diff_first_12_frames=", [f"{x:.2e}" for x in per_frame[:12]])
print(f"err_rms_dB_re_signal={20 * np.log10(max(err, 1e-30) / max(rms, 1e-30)):.1f}")
```

- [ ] **Step 4: Criar `libdf-fork` (cópia sem patch, só renomeada) e usar o `Cargo.lock` do repositório**

```bash
export REPO=<REPO>
export SPIKE=<SCRATCHPAD>/spike-libdf
export RUSTUP_HOME=$SPIKE/rustup CARGO_HOME=$SPIKE/cargo PATH=$SPIKE/cargo/bin:$PATH
cp -r $SPIKE/libdf-pristine $SPIKE/libdf-fork
sed -i 's/^name = "deep_filter"$/name = "deep_filter_fork"/' $SPIKE/libdf-fork/Cargo.toml
grep -n '^name' $SPIKE/libdf-fork/Cargo.toml | head -2
cp $REPO/Cargo.lock $SPIKE/harness/Cargo.lock   # fixa tract 0.19.16 e demais versoes do produto
```

Expected: `name = "deep_filter_fork"`.

- [ ] **Step 5: Compilar o build pristine e conferir versões**

```bash
export SPIKE=<SCRATCHPAD>/spike-libdf
export RUSTUP_HOME=$SPIKE/rustup CARGO_HOME=$SPIKE/cargo PATH=$SPIKE/cargo/bin:$PATH
cd $SPIKE/harness
{ cargo build --release 2>&1 | tail -15; echo "## versoes"; cargo tree -i tract-core 2>&1 | head -3; cargo tree -i ndarray 2>&1 | head -2; } | tee $SPIKE/evidence/T2-build.txt
mkdir -p $SPIKE/bin && cp target/release/spike-harness $SPIKE/bin/harness-pristine
```

Expected: `Finished release`; `tract-core v0.19.16`; `ndarray v0.15.6`. Se o `cargo tree` mostrar outra versão do `tract-core`, **pare**: o `Cargo.lock` não foi respeitado e a paridade não vale para o produto. Se houver erro de colisão de nomes no lockfile ou de `[lib] name`, o fallback é duas crates de harness separadas (`harness-pristine`, `harness-fork`) incluindo o mesmo `main.rs` por `include!`; registre a mudança.

- [ ] **Step 6: Golden local sintético, duas execuções (determinismo)**

```bash
export SPIKE=<SCRATCHPAD>/spike-libdf
cd $SPIKE
{
  $SPIKE/bin/harness-pristine golden $SPIKE/asset-stock.tar.gz $SPIKE/golden-pristine.f32 1000
  $SPIKE/bin/harness-pristine golden $SPIKE/asset-stock.tar.gz $SPIKE/golden-pristine-2.f32 1000
  sha256sum $SPIKE/golden-pristine.f32 $SPIKE/golden-pristine-2.f32
  $SPIKE/venv/bin/python $SPIKE/scripts/compare.py $SPIKE/golden-pristine.f32 $SPIKE/golden-pristine-2.f32
} 2>&1 | tee $SPIKE/evidence/T2-golden.txt
```

Expected: dois SHA-256 idênticos e `bit_exact=True`. Se não for determinístico, a T4 não pode exigir bit-exatidão: registre o `max_abs_diff` entre execuções como o piso de ruído.

- [ ] **Step 7: Replay do golden congelado do repositório**

```bash
export REPO=<REPO>
export SPIKE=<SCRATCHPAD>/spike-libdf
$SPIKE/bin/harness-pristine replay $SPIKE/asset-stock.tar.gz $REPO/fixtures/golden/frozen-reference.json 2>&1 | tee $SPIKE/evidence/T2-replay.txt
```

Expected: `replay frames=300 ... within_tol=true` (6 casos x 50 frames). `max_abs_diff` pode ser 0 ou da ordem de 1e-7 (seleção de kernels do `tract` por CPU). Se `within_tol=false`, verifique antes se o número de frames e a ordem dos casos estão certos (um único modelo, sem reinício); se persistir, Q2 = NO-GO e o spike para (a cópia não reproduz o upstream-como-compilado-pelo-produto).

- [ ] **Step 8: Veredito Q2**

GO se: compila (Step 5), `bit_exact=True` entre execuções (Step 6) e `within_tol=true` (Step 7). Registre os três trechos literais no relatório e o `max_abs_diff` do replay.

- [ ] **Step 9: Checkpoint**

```bash
cd <REPO> && git status --short
ls -A ~/.cargo
```

Expected: `git status` só com os dois `??` de docs (plano e spec); `~/.cargo` inalterado (só `.package-cache`, `git`, `registry`). **Não commitar.**

---

### Task 3: Gerar o par de ONNX com FiLM em identidade

**Pergunta Q3:** dá para gerar, com as ferramentas desta máquina, `enc.onnx` e `df_dec.onnx` com `Mul(gamma)`+`Add(beta)` antes das GRUs, com `gamma/beta` como entradas extras, bit-exatos contra o original quando `gamma=1, beta=0`?

**Files:**
- Create: `$SPIKE/scripts/gen_film_onnx.py`
- Create: `$SPIKE/out/enc_film.onnx`, `$SPIKE/out/df_dec_film.onnx`, `$SPIKE/out/film_identity_asset.tar.gz`, `$SPIKE/out/enc_film_baked.onnx`, `$SPIKE/out/df_dec_film_baked.onnx`, `$SPIKE/out/film_baked_asset.tar.gz`, `$SPIKE/evidence/T3-gen.txt`

**Interfaces:**
- Consumes: `$SPIKE/asset/tmp/export/*` e `$SPIKE/venv` (T1).
- Produces: `film_identity_asset.tar.gz` (membros `tmp/export/{enc,erb_dec,df_dec}.onnx` + `config.ini`; `enc` e `df_dec` com entradas extras `gamma`, `beta` de forma `[1, S, 256]` anexadas **depois** das entradas originais); `film_baked_asset.tar.gz` (mesmo layout, mas `gamma=1.5`/`beta=0.1` como initializers constantes `[1,1,256]` e sem entradas extras: carrega no libDF **sem patch**).

- [ ] **Step 1: Escrever o script gerador**

Cria `$SPIKE/scripts/gen_film_onnx.py` (este texto foi executado pelo autor do plano em venv com `onnx 1.23.1` e `onnxruntime 1.30.0` sobre o asset aprovado, com a saída esperada do Step 3):

```python
#!/usr/bin/env python3
"""Gera os ONNX do DFNet3 com FiLM a partir do asset aprovado (tarefa T3 do spike).

Uso: gen_film_onnx.py <dir com enc.onnx/erb_dec.onnx/df_dec.onnx/config.ini> <dir de saida>

Insere x' = x * gamma + beta na saida do Relu que alimenta Transpose->GRU de enc e df_dec.
Saidas em <dir de saida>:
  enc_film.onnx, df_dec_film.onnx           gamma/beta como ENTRADAS extras [1, S, 256]
  film_identity_asset.tar.gz                 tar.gz com os dois acima + erb_dec.onnx + config.ini
  enc_film_baked.onnx, df_dec_film_baked.onnx  gamma=1.5, beta=0.1 como INITIALIZERS (sem entradas extras)
  film_baked_asset.tar.gz                    tar.gz equivalente com os baked
"""
import gzip, io, os, sys, tarfile
import numpy as np
import onnx
import onnxruntime as ort
from onnx import TensorProto, helper, numpy_helper

SITES = {
    "enc": "/emb_gru/linear_in/1/Relu_output_0",
    "df_dec": "/df_gru/linear_in/linear_in.1/Relu_output_0",
}
HIDDEN = 256
BAKED_GAMMA, BAKED_BETA = 1.5, 0.1


def add_film(model: onnx.ModelProto, site: str) -> onnx.ModelProto:
    g = model.graph
    if any(i.name in ("gamma", "beta") for i in g.input):
        raise SystemExit("modelo ja tem gamma/beta")
    producers = [i for i, nd in enumerate(g.node) if site in nd.output]
    if len(producers) != 1:
        raise SystemExit(f"site {site}: {len(producers)} produtores (esperado 1)")
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


def bake(model: onnx.ModelProto, gamma: float, beta: float) -> onnx.ModelProto:
    """Troca as entradas gamma/beta por initializers constantes [1, 1, 256]."""
    g = model.graph
    keep = [i for i in g.input if i.name not in ("gamma", "beta")]
    del g.input[:]
    g.input.extend(keep)
    g.initializer.append(numpy_helper.from_array(np.full((1, 1, HIDDEN), gamma, "f4"), "gamma"))
    g.initializer.append(numpy_helper.from_array(np.full((1, 1, HIDDEN), beta, "f4"), "beta"))
    onnx.checker.check_model(model)
    return model


def feeds(name: str, steps: int, seed: int) -> dict:
    r = np.random.RandomState(seed)
    if name == "enc":
        return {"feat_erb": r.randn(1, 1, steps, 32).astype("f4"),
                "feat_spec": r.randn(1, 2, steps, 96).astype("f4")}
    return {"emb": r.randn(1, steps, 512).astype("f4"),
            "c0": r.randn(1, 64, steps, 96).astype("f4")}


def run(model_bytes: bytes, f: dict):
    return ort.InferenceSession(model_bytes, providers=["CPUExecutionProvider"]).run(None, f)


def same(xs, ys) -> bool:
    return all(np.array_equal(a, b) for a, b in zip(xs, ys))


def pack(path: str, enc: str, erb_dec: str, df_dec: str, config: str) -> None:
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w") as tar:
        for member, src in (("enc.onnx", enc), ("erb_dec.onnx", erb_dec),
                            ("df_dec.onnx", df_dec), ("config.ini", config)):
            data = open(src, "rb").read()
            info = tarfile.TarInfo(f"tmp/export/{member}")
            info.size, info.mtime, info.mode = len(data), 0, 0o644
            tar.addfile(info, io.BytesIO(data))
    with gzip.GzipFile(path, "wb", mtime=0) as gz:
        gz.write(buf.getvalue())


def main(src: str, dst: str) -> None:
    os.makedirs(dst, exist_ok=True)
    steps = 8
    for name, site in SITES.items():
        orig = open(f"{src}/{name}.onnx", "rb").read()
        film = add_film(onnx.load_from_string(orig), site)
        film_bytes = film.SerializeToString()
        open(f"{dst}/{name}_film.onnx", "wb").write(film_bytes)
        baked = bake(onnx.load_from_string(film_bytes), BAKED_GAMMA, BAKED_BETA)
        baked_bytes = baked.SerializeToString()
        open(f"{dst}/{name}_film_baked.onnx", "wb").write(baked_bytes)
        for seed in range(5):
            f = feeds(name, steps, seed)
            base = run(orig, f)
            ident = run(film_bytes, {**f, "gamma": np.ones((1, steps, HIDDEN), "f4"),
                                     "beta": np.zeros((1, steps, HIDDEN), "f4")})
            diff = max(float(np.abs(a - b).max()) for a, b in zip(base, ident))
            exact = same(base, ident)
            print(f"{name} seed={seed} identidade max_abs_diff={diff!r} bit_exato={exact}")
            if not exact:
                raise SystemExit(f"{name}: FiLM em identidade NAO e bit-exato no ORT")
        f = feeds(name, steps, 99)
        base = run(orig, f)
        moved = run(film_bytes, {**f, "gamma": np.full((1, steps, HIDDEN), BAKED_GAMMA, "f4"),
                                 "beta": np.full((1, steps, HIDDEN), BAKED_BETA, "f4")})
        changed = not same(base, moved)
        print(f"{name} gamma={BAKED_GAMMA} beta={BAKED_BETA} altera a saida: {changed}")
        if not changed:
            raise SystemExit(f"{name}: FiLM nao-identidade nao altera a saida (grafo nao esta ligado)")
        baked_out = run(baked_bytes, f)
        eq = same(moved, baked_out)
        print(f"{name} baked == entradas com mesmos valores: {eq}")
        if not eq:
            raise SystemExit(f"{name}: versao baked difere da versao com entradas")
    pack(f"{dst}/film_identity_asset.tar.gz", f"{dst}/enc_film.onnx", f"{src}/erb_dec.onnx",
         f"{dst}/df_dec_film.onnx", f"{src}/config.ini")
    pack(f"{dst}/film_baked_asset.tar.gz", f"{dst}/enc_film_baked.onnx", f"{src}/erb_dec.onnx",
         f"{dst}/df_dec_film_baked.onnx", f"{src}/config.ini")
    print("ok", f"{dst}/film_identity_asset.tar.gz", f"{dst}/film_baked_asset.tar.gz")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
```

- [ ] **Step 2: Rodar**

```bash
export SPIKE=<SCRATCHPAD>/spike-libdf
cd $SPIKE && rm -rf out
$SPIKE/venv/bin/python $SPIKE/scripts/gen_film_onnx.py $SPIKE/asset/tmp/export $SPIKE/out 2>&1 | tee $SPIKE/evidence/T3-gen.txt
sha256sum $SPIKE/out/* | tee -a $SPIKE/evidence/T3-gen.txt
tar tzvf $SPIKE/out/film_identity_asset.tar.gz | tee -a $SPIKE/evidence/T3-gen.txt
```

Expected: dez linhas `... identidade max_abs_diff=0.0 bit_exato=True` (5 sementes x `enc`, `df_dec`); `gamma=1.5 beta=0.1 altera a saida: True` e `baked == entradas com mesmos valores: True` para os dois grafos; `ok ...film_identity_asset.tar.gz ...film_baked_asset.tar.gz`; o `tar tzvf` lista os quatro membros `tmp/export/{enc,erb_dec,df_dec}.onnx` e `config.ini` (a mesma allowlist do asset aprovado).

- [ ] **Step 3: Conferir as entradas declaradas**

```bash
export SPIKE=<SCRATCHPAD>/spike-libdf
$SPIKE/venv/bin/python - <<'E' | tee -a $SPIKE/evidence/T3-gen.txt
import os, onnx
d = os.environ["SPIKE"] + "/out/"
for n in ["enc_film", "df_dec_film", "enc_film_baked", "df_dec_film_baked"]:
    m = onnx.load(d + n + ".onnx")
    init = {t.name for t in m.graph.initializer}
    print(n, [i.name for i in m.graph.input if i.name not in init])
E
```

Expected: `enc_film ['feat_erb', 'feat_spec', 'gamma', 'beta']`, `df_dec_film ['emb', 'c0', 'gamma', 'beta']`; os `_baked` só com as duas entradas originais.

- [ ] **Step 4: Veredito Q3 e alternativa**

GO se o Step 2 terminou em `ok` (identidade bit-exata e não-identidade efetiva nos dois grafos) e o Step 3 confere. Se `onnx`/`onnxruntime` não puderam ser instalados (sem rede) ou o script falhou: **NO-GO** registrado com a saída literal do erro, mais a alternativa: (a) construir o FiLM dentro do grafo do `tract` em Rust (inserir `Mul`/`Add` no `TypedModel` depois do `into_typed()` e antes da pulsificação), sem editar ONNX; ou (b) exigir do repo de treino o script de exportação com FiLM em identidade (requisito 5 da seção 7 do spec). O `torch` presente (2.13.0) não substitui a biblioteca `onnx` para editar grafos já exportados.

- [ ] **Step 5: Checkpoint**

```bash
cd <REPO> && git status --short
```

Expected: só os dois `??` de docs. **Não commitar.**

---

### Task 4: Patch mínimo no fork para `gamma/beta` e teste de paridade

**Pergunta Q4:** com `gamma=1, beta=0`, o fork (a) mantém o upstream bit-exato com o asset aprovado e (b) reproduz o upstream com o asset de FiLM em identidade, bit-exato ou dentro de uma tolerância numérica documentada? Quanto o patch cresce?

**Files:**
- Modify: `$SPIKE/libdf-fork/src/tract.rs` (patch), `$SPIKE/libdf-fork/Cargo.toml` (já renomeado na T2)
- Create: `$SPIKE/evidence/T4-red.txt`, `$SPIKE/evidence/T4-parity.txt`, `$SPIKE/evidence/T4-delay.txt`, `$SPIKE/evidence/T4-patch.diff`, `$SPIKE/bin/harness-fork`, `$SPIKE/golden-*.f32`

**Interfaces:**
- Consumes: `$SPIKE/libdf-pristine/`, `$SPIKE/libdf-fork/` (cópia renomeada), `$SPIKE/harness/` (T2), `$SPIKE/out/film_identity_asset.tar.gz` e `film_baked_asset.tar.gz` (T3), `$SPIKE/golden-pristine.f32` (T2).
- Produces: API do fork `pub struct FilmVectors { pub gamma_enc: Vec<f32>, pub beta_enc: Vec<f32>, pub gamma_df: Vec<f32>, pub beta_df: Vec<f32> }` (cada vetor com `ch * film_hidden()` elementos), `DfTract::film_hidden(&self) -> Option<usize>` (`Some(256)` só se algum grafo declara `gamma/beta`), `DfTract::set_film(&mut self, v: &FilmVectors) -> Result<()>`; contagem de linhas do patch; veredito Q4 e rota recomendada.

- [ ] **Step 1: Teste vermelho: o carregador intocado não aceita o asset com FiLM**

```bash
export SPIKE=<SCRATCHPAD>/spike-libdf
$SPIKE/bin/harness-pristine film $SPIKE/out/film_identity_asset.tar.gz $SPIKE/golden-pristine-film.f32 none 50 2>&1 | tail -15 | tee $SPIKE/evidence/T4-red.txt
```

Expected: **falha** (erro de carga ou de execução, por `with_input_names` descartar `gamma/beta` ou por entradas não alimentadas). Registre a mensagem literal: ela confirma o achado (1) do tract. Se o carregador intocado **rodar sem erro**, anote isso (muda a premissa) e prossiga mesmo assim, comparando saídas no Step 6.

- [ ] **Step 2: Patch (a): tipos novos e campos**

Em `$SPIKE/libdf-fork/src/tract.rs`, logo depois de `pub type TractModel = TypedSimpleState<TypedModel, TypedSimplePlan<TypedModel>>;`, adicione:

```rust
/// FiLM conditioning vectors fed to the extra `gamma`/`beta` graph inputs.
/// Every vector has `ch * DfTract::film_hidden()` elements.
#[derive(Clone, Debug)]
pub struct FilmVectors {
    pub gamma_enc: Vec<f32>,
    pub beta_enc: Vec<f32>,
    pub gamma_df: Vec<f32>,
    pub beta_df: Vec<f32>,
}

#[derive(Clone)]
struct FilmInputs {
    gamma: TValue, // [ch, 1, hidden]
    beta: TValue,  // [ch, 1, hidden]
}

impl FilmInputs {
    fn from_slices(ch: usize, hidden: usize, gamma: &[f32], beta: &[f32]) -> Result<Self> {
        if gamma.len() != ch * hidden || beta.len() != ch * hidden {
            bail!(
                "FiLM vectors must have {} elements (got {} and {})",
                ch * hidden,
                gamma.len(),
                beta.len()
            );
        }
        Ok(Self {
            gamma: TValue::from(Tensor::from_shape(&[ch, 1, hidden], gamma)?),
            beta: TValue::from(Tensor::from_shape(&[ch, 1, hidden], beta)?),
        })
    }
    fn identity(ch: usize, hidden: usize) -> Result<Self> {
        Self::from_slices(ch, hidden, &vec![1f32; ch * hidden], &vec![0f32; ch * hidden])
    }
}
```

No `pub struct DfTract`, depois do campo `rolling_spec_buf_x: VecDeque<Tensor>, // Noisy spec buf`, adicione:

```rust
    film_hidden: usize,
    film_enc: Option<FilmInputs>, // Some only when enc.onnx declares gamma/beta
    film_df: Option<FilmInputs>,  // Some only when df_dec.onnx declares gamma/beta
```

- [ ] **Step 3: Patch (b): construção em `DfTract::new`**

Em `DfTract::new`, depois de `let ch = rp.n_ch;` adicione `let film_hidden = model_cfg.get("emb_hidden_dim").unwrap().parse::<usize>()?;`. Troque as três chamadas de init e as três linhas de `SimpleState::new` (linhas 247-257 do original) por:

```rust
        let enc = init_encoder_from_read(&mut Cursor::new(dfp.enc), df_cfg, ch, film_hidden)?;
        let erb_dec = init_erb_decoder_from_read(
            &mut Cursor::new(dfp.erb_dec),
            model_cfg,
            df_cfg,
            ch,
            Some(rp.reduce_mask.clone()),
        )?;
        let df_dec = init_df_decoder_from_read(
            &mut Cursor::new(dfp.df_dec),
            model_cfg,
            df_cfg,
            ch,
            film_hidden,
        )?;
        let film_enc = if enc.input_outlets()?.len() == 4 {
            Some(FilmInputs::identity(ch, film_hidden)?)
        } else {
            None
        };
        let film_df = if df_dec.input_outlets()?.len() == 4 {
            Some(FilmInputs::identity(ch, film_hidden)?)
        } else {
            None
        };
        let enc = SimpleState::new(enc.into_runnable()?)?;
        let erb_dec = SimpleState::new(erb_dec.into_runnable()?)?;
        let df_dec = SimpleState::new(df_dec.into_runnable()?)?;
```

No literal `let mut m = Self { ... }`, depois de `df_states,` adicione `film_hidden, film_enc, film_df,`.

- [ ] **Step 4: Patch (c): execução em `process_raw` e API pública**

Em `process_raw`, troque o bloco `let mut enc_emb = self.enc.run(tvec!(...))?;` (linhas 454-457) por:

```rust
        let mut enc_in = tvec!(
            self.erb_buf.clone(),
            TValue::from(self.cplx_buf.clone().into_tensor().permute_axes(&[0, 3, 1, 2])?)
        );
        if let Some(film) = &self.film_enc {
            enc_in.push(film.gamma.clone());
            enc_in.push(film.beta.clone());
        }
        let mut enc_emb = self.enc.run(enc_in)?;
```

e a linha `let mut coefs = self.df_dec.run(tvec!(emb, c0))?.pop().unwrap().into_tensor();` (linha 492) por:

```rust
            let mut df_in = tvec!(emb, c0);
            if let Some(film) = &self.film_df {
                df_in.push(film.gamma.clone());
                df_in.push(film.beta.clone());
            }
            let mut coefs = self.df_dec.run(df_in)?.pop().unwrap().into_tensor();
```

Dentro de `impl DfTract`, antes de `pub fn init`, adicione:

```rust
    /// Hidden size of the FiLM vectors, or `None` when no graph declares `gamma`/`beta`.
    pub fn film_hidden(&self) -> Option<usize> {
        (self.film_enc.is_some() || self.film_df.is_some()).then_some(self.film_hidden)
    }

    /// Replace the FiLM vectors fed on every frame. Does not reset the GRU states.
    pub fn set_film(&mut self, v: &FilmVectors) -> Result<()> {
        if self.film_enc.is_none() && self.film_df.is_none() {
            bail!("model graphs do not declare gamma/beta inputs");
        }
        if self.film_enc.is_some() {
            self.film_enc =
                Some(FilmInputs::from_slices(self.ch, self.film_hidden, &v.gamma_enc, &v.beta_enc)?);
        }
        if self.film_df.is_some() {
            self.film_df =
                Some(FilmInputs::from_slices(self.ch, self.film_hidden, &v.gamma_df, &v.beta_df)?);
        }
        Ok(())
    }
```

- [ ] **Step 5: Patch (d): `init_encoder_impl` e `init_df_decoder_impl` declaram as 4 entradas**

Em `init_encoder_impl`, troque a assinatura por `fn init_encoder_impl(mut m: InferenceModel, df_cfg: &ini::Properties, n_ch: usize, film_hidden: usize) -> Result<TypedModel>` e o bloco `m = m.with_input_fact(0, feat_erb)?...with_output_names([...])?;` (linhas 772-777) por:

```rust
    let film = m.input_outlets()?.len() == 4;
    m = m.with_input_fact(0, feat_erb)?.with_input_fact(1, feat_spec)?;
    m = if film {
        let g = InferenceFact::dt_shape(f32::datum_type(), shapefactoid!(n_ch, s, film_hidden));
        m.with_input_fact(2, g.clone())?
            .with_input_fact(3, g)?
            .with_input_names(["feat_erb", "feat_spec", "gamma", "beta"])?
    } else {
        m.with_input_names(["feat_erb", "feat_spec"])?
    };
    m = m.with_output_names(["e0", "e1", "e2", "e3", "emb", "c0", "lsnr"])?;
```

Em `init_df_decoder_impl`, adicione o parâmetro `film_hidden: usize` ao final da assinatura e troque o bloco `m = m.with_input_fact(0, emb)?.with_input_fact(1, c0)?.with_input_names(["emb", "c0"])?.with_output_names(["coefs"])?;` (linhas 940-944) por:

```rust
    let film = m.input_outlets()?.len() == 4;
    m = m.with_input_fact(0, emb)?.with_input_fact(1, c0)?;
    m = if film {
        let g = InferenceFact::dt_shape(f32::datum_type(), shapefactoid!(n_ch, s, film_hidden));
        m.with_input_fact(2, g.clone())?
            .with_input_fact(3, g)?
            .with_input_names(["emb", "c0", "gamma", "beta"])?
    } else {
        m.with_input_names(["emb", "c0"])?
    };
    m = m.with_output_names(["coefs"])?;
```

Propague o parâmetro `film_hidden: usize` (último argumento) pelos quatro wrappers `init_encoder`, `init_encoder_from_read`, `init_df_decoder`, `init_df_decoder_from_read` e pelas chamadas ao `_impl`. O `erb_dec` não muda (FiLM só no `enc` e no `df_dec`, conforme os vetores `gamma_enc, beta_enc, gamma_df, beta_df` do spec).

- [ ] **Step 6: Compilar o fork, medir a paridade e registrar o diff**

```bash
export SPIKE=<SCRATCHPAD>/spike-libdf
export RUSTUP_HOME=$SPIKE/rustup CARGO_HOME=$SPIKE/cargo PATH=$SPIKE/cargo/bin:$PATH
cd $SPIKE/harness && cargo build --release --no-default-features --features fork 2>&1 | tail -15
cp target/release/spike-harness $SPIKE/bin/harness-fork
cd $SPIKE
H=$SPIKE/bin/harness-fork; PY=$SPIKE/venv/bin/python; C=$SPIKE/scripts/compare.py
{
  echo "## B: fork + asset aprovado, sem set_film (regressao do patch)"
  $H film $SPIKE/asset-stock.tar.gz $SPIKE/golden-B.f32 none 1000; $PY $C $SPIKE/golden-pristine.f32 $SPIKE/golden-B.f32
  echo "## C: fork + asset FiLM identidade, sem set_film (identidade padrao)"
  $H film $SPIKE/out/film_identity_asset.tar.gz $SPIKE/golden-C.f32 none 1000; $PY $C $SPIKE/golden-pristine.f32 $SPIKE/golden-C.f32
  echo "## D: fork + asset FiLM, set_film(gamma=1, beta=0) explicito"
  $H film $SPIKE/out/film_identity_asset.tar.gz $SPIKE/golden-D.f32 identity 1000; $PY $C $SPIKE/golden-pristine.f32 $SPIKE/golden-D.f32
  echo "## E: fork + asset FiLM, set_film(gamma=1.5, beta=0.1) (sensibilidade: deve DIFERIR)"
  $H film $SPIKE/out/film_identity_asset.tar.gz $SPIKE/golden-E.f32 scaled 1000; $PY $C $SPIKE/golden-pristine.f32 $SPIKE/golden-E.f32
  echo "## F: controle sem patch: carregador INTOCADO + asset com constantes embutidas (gamma=1.5, beta=0.1)"
  $SPIKE/bin/harness-pristine film $SPIKE/out/film_baked_asset.tar.gz $SPIKE/golden-F.f32 none 1000; $PY $C $SPIKE/golden-pristine.f32 $SPIKE/golden-F.f32
  echo "## G: rota com entradas (E) vs rota com constantes embutidas (F), mesmos valores: diferenca = efeito do alinhamento de atraso do pulsifier"
  $PY $C $SPIKE/golden-F.f32 $SPIKE/golden-E.f32
} 2>&1 | tee $SPIKE/evidence/T4-parity.txt
diff -u $SPIKE/libdf-pristine/src/tract.rs $SPIKE/libdf-fork/src/tract.rs > $SPIKE/evidence/T4-patch.diff
diff -u $SPIKE/libdf-pristine/Cargo.toml $SPIKE/libdf-fork/Cargo.toml >> $SPIKE/evidence/T4-patch.diff
echo "linhas do diff (total, +, -):"; wc -l < $SPIKE/evidence/T4-patch.diff
grep -c '^+[^+]' $SPIKE/evidence/T4-patch.diff; grep -c '^-[^-]' $SPIKE/evidence/T4-patch.diff
```

Leitura esperada e critérios de decisão (escritos antes de medir):

- **B** deve ser `bit_exact=True`. Se não for, o patch regrediu o caminho existente: corrija o patch (nenhuma desculpa de tolerância).
- **C** e **D** (FiLM em identidade) são o teste de paridade pedido. **GO forte**: `bit_exact=True`. **GO com tolerância documentada**: `max_abs_diff <= 1e-6` (cerca de -120 dBFS) **e** `last_diff_frame` pequeno (diferença só nos primeiros quadros, por exemplo `<= 20`) **e** limite muito abaixo da tolerância do gate do produto (`1e-4`); registre o valor medido como a tolerância do teste. **NO-GO desta rota**: acima disso, ou diferença que não decai.
- **E** deve **diferir** (`bit_exact=False`, `max_abs_diff` bem acima de zero): prova que `gamma/beta` realmente chegam ao grafo. Se `E` for idêntico a `golden-pristine`, o FiLM não está ligado: bug do patch.
- **F** (carregador intocado, constantes embutidas) deve rodar **sem patch** e diferir do pristine (efeito real de `gamma=1.5, beta=0.1`).
- **G** compara as duas rotas com os mesmos valores. Esperado: iguais, ou diferentes só nos primeiros quadros. Uma diferença confirma o risco (3) do tract (atraso zero nas primeiras `conv_lookahead` quadros do `enc`).

- [ ] **Step 7: Se aparecer diferença nos primeiros quadros, localizar a causa (instrumentação temporária)**

Só se C/D/G não forem bit-exatos. Em `libdf-fork/src/tract.rs`, dentro de `init_encoder_impl`, logo depois de `let m = pulsed.into_typed()?.into_optimized()?;`, adicione temporariamente:

```rust
    eprintln!(
        "ENC delay nodes: {} / total {}",
        m.nodes.iter().filter(|n| n.op.name().contains("Delay")).count(),
        m.nodes.len()
    );
```

Rode `harness-fork film ... none 5` com o asset aprovado e com o asset FiLM e registre as duas contagens em `$SPIKE/evidence/T4-delay.txt`. Se o asset FiLM tiver mais nós `Delay` que o aprovado, a hipótese do pulsifier está confirmada. **Remova a instrumentação** depois e refaça o Step 6 (o diff final não pode conter o `eprintln!`).

Mitigações, na ordem de preferência: (i) rota com **constantes embutidas** (asset `film_baked_asset.tar.gz`, controle F): sem `Delay`, sem patch no libDF, mas cada troca de perfil exige gerar novos bytes de ONNX e recarregar o modelo (reinício de geração); (ii) manter as entradas e aceitar a tolerância medida nos primeiros quadros; (iii) pedir ao repo de treino que o ponto de FiLM do `enc` fique depois de um caminho de atraso zero. Registre qual foi escolhida e por quê.

- [ ] **Step 8: Veredito Q4**

GO se B é bit-exato e C/D passam o critério acima (forte ou com tolerância documentada) e E difere. O relatório informa: rota recomendada (entradas com patch / constantes embutidas sem patch), tolerância, e o número de linhas do patch em `src/tract.rs` separado do `Cargo.toml` (`T4-patch.diff`). Se as linhas adicionadas em `tract.rs` passarem de ~100, registre que o patch deixou de ser "mínimo" (risco do spec, seção 10).

- [ ] **Step 9: Checkpoint**

```bash
cd <REPO> && git status --short
```

Expected: só os dois `??` de docs. **Não commitar.**

---

### Task 5: Custo do patch (ns/frame com e sem entradas extras)

**Pergunta Q5:** o custo por frame com `gamma/beta` fica dentro de um limite aceitável frente ao upstream? Limite escolhido antes de medir: **GO** se a mediana de `p50_ns` do fork com FiLM (modo `identity`) for no máximo 10% acima da mediana do pristine **e** o `p99_ns` do fork com FiLM ficar abaixo de 7 ms (o `P99_LIMIT_MS = 7.0` de `crates/tools/src/bin/benchmark.rs`); o quadro de 10 ms é o orçamento duro.

**Files:**
- Create: `$SPIKE/evidence/T5-bench.txt`, `$SPIKE/evidence/T5-summary.txt`, `$SPIKE/scripts/summarize_bench.py`

**Interfaces:**
- Consumes: `$SPIKE/bin/harness-pristine`, `$SPIKE/bin/harness-fork` (T2/T4), assets `asset-stock`, `out/film_identity_asset` (T3).
- Produces: tabela de custo e veredito Q5 para o relatório.

- [ ] **Step 1: Escolher um núcleo P e fixar a medição**

```bash
lscpu -e=CPU,CORE,MAXMHZ | sort -k3 -n -r | head -4
cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>&1
```

Anote o `CPU` com maior `MAXMHZ` (núcleo P) e o governor. A máquina é híbrida (Core Ultra 7 265H): sem `taskset` o resultado oscila entre núcleos P e E. Substitua `<CPU_P>` abaixo por esse número (ele é saída do comando, não suposição).

- [ ] **Step 2: Escrever o resumo estatístico**

Cria `$SPIKE/scripts/summarize_bench.py`:

```python
#!/usr/bin/env python3
"""Le linhas 'VARIANTE rep bench mode=... p50_ns=... p99_ns=...' e imprime a mediana entre repeticoes."""
import re, statistics, sys
rows = {}
for line in open(sys.argv[1]):
    m = re.match(r"(\S+) rep=(\d+) bench .*mean_ns=(\d+) p50_ns=(\d+) p95_ns=(\d+) p99_ns=(\d+) max_ns=(\d+)", line)
    if m:
        rows.setdefault(m.group(1), []).append([int(x) for x in m.groups()[2:]])
base = None
for name in ("pristine", "fork_stock", "fork_film_none", "fork_film_identity"):
    if name not in rows:
        continue
    cols = list(zip(*rows[name]))
    med = [int(statistics.median(c)) for c in cols]
    base = base or med[1]
    print(f"{name:20s} reps={len(rows[name])} mean={med[0]} p50={med[1]} p95={med[2]} p99={med[3]} max={med[4]} ns"
          f"  p50_vs_pristine={100.0 * (med[1] - base) / base:+.1f}%")
```

- [ ] **Step 3: Rodar 5 repetições intercaladas de 4 variantes**

```bash
export SPIKE=<SCRATCHPAD>/spike-libdf
CPU_P=<CPU_P>   # substitua pelo numero lido no Step 1
N=20000
: > $SPIKE/evidence/T5-bench.txt
for rep in 1 2 3 4 5; do
  echo -n "pristine rep=$rep "            >> $SPIKE/evidence/T5-bench.txt; taskset -c $CPU_P $SPIKE/bin/harness-pristine bench $SPIKE/asset-stock.tar.gz $N none >> $SPIKE/evidence/T5-bench.txt
  echo -n "fork_stock rep=$rep "          >> $SPIKE/evidence/T5-bench.txt; taskset -c $CPU_P $SPIKE/bin/harness-fork bench $SPIKE/asset-stock.tar.gz $N none >> $SPIKE/evidence/T5-bench.txt
  echo -n "fork_film_none rep=$rep "      >> $SPIKE/evidence/T5-bench.txt; taskset -c $CPU_P $SPIKE/bin/harness-fork bench $SPIKE/out/film_identity_asset.tar.gz $N none >> $SPIKE/evidence/T5-bench.txt
  echo -n "fork_film_identity rep=$rep "  >> $SPIKE/evidence/T5-bench.txt; taskset -c $CPU_P $SPIKE/bin/harness-fork bench $SPIKE/out/film_identity_asset.tar.gz $N identity >> $SPIKE/evidence/T5-bench.txt
done
$SPIKE/venv/bin/python $SPIKE/scripts/summarize_bench.py $SPIKE/evidence/T5-bench.txt | tee $SPIKE/evidence/T5-summary.txt
lscpu | grep 'Model name' | tee -a $SPIKE/evidence/T5-summary.txt
```

Cada variante processa 20.000 frames (200 s de áudio) após 500 de aquecimento; são 20 execuções (cerca de 1 a 2 minutos no total se o custo for de ~0,3 ms/frame). O nome da variante é o primeiro campo da linha do `bench-*.txt`.

- [ ] **Step 4: Veredito Q5**

Compare `fork_film_identity` e `fork_film_none` com `pristine` (coluna `p50_vs_pristine`) e o `p99` do `fork_film_identity`. Registre: GO se `p50_vs_pristine <= +10%` e `p99 < 7_000_000 ns`; caso contrário NO-GO com os números. Registre também a diferença `fork_stock` vs `pristine` (custo do patch quando o grafo não declara FiLM: deveria ser ~0). Diga explicitamente que o número vale **só nesta máquina** (CPU, governor, `taskset`) e **não** é medição fim a fim do produto (nada de áudio por dispositivo, resampler ou engine): o spec (seção 10) proíbe tratar essa medida como folga de latência.

- [ ] **Step 5: Checkpoint**

```bash
cd <REPO> && git status --short
```

Expected: só os dois `??` de docs. **Não commitar.**

---

### Task 6: Viabilidade da cabeça de EQ aplicada no espectro antes do iSTFT

**Pergunta Q6:** em que ponto do pipeline do libDF o espectro final está acessível, dá para aplicar ganhos por banda ali sem latência extra e sem tocar o contrato do modelo, e o que uma cabeça de EQ precisaria receber?

**Files:**
- Modify (cópia própria): `$SPIKE/libdf-eqhook/src/tract.rs` (gancho de espectro) e `$SPIKE/libdf-eqhook/Cargo.toml` (nome `deep_filter_eqhook`)
- Create: `$SPIKE/harness-eq/` (segunda crate, copia do `harness` apontando para `libdf-eqhook`), `$SPIKE/evidence/T6-code-points.txt`, `$SPIKE/evidence/T6-hook.txt`, `$SPIKE/scripts/eq_check.py`

**Interfaces:**
- Consumes: `$SPIKE/libdf-pristine/` (T1/T2), `$SPIKE/asset-stock.tar.gz`, `$SPIKE/golden-pristine.f32` (T2), `$SPIKE/venv`.
- Produces: `DfTract::process_with_spec_hook<F: FnMut(&mut ArrayViewMut2<Complex32>)>(&mut self, noisy: ArrayView2<f32>, enh: ArrayViewMut2<f32>, hook: F) -> Result<f32>` e `process` como invólucro com gancho vazio; veredito Q6.

Esta tarefa é só de viabilidade: o gancho vive numa cópia à parte, **não** no `libdf-fork` da T4, e nenhum EQ real é treinado ou calculado.

- [ ] **Step 1: Levantar os pontos de código com evidência literal**

```bash
export SPIKE=<SCRATCHPAD>/spike-libdf
T=$SPIKE/libdf-pristine/src/tract.rs
{
  grep -n 'pub fn process\b\|Run post filter\|Limit noise attenuation\|state.synthesis\|self.enc.run\|self.erb_dec.run\|self.df_dec.run\|pub fn get_mut_spec_enh\|pub fn get_spec_enh\|let mut spec_enh' $T
  echo "## trecho final de process() (apos DF/post-filter/atten_lim, antes do iSTFT)"; sed -n 609,626p $T
  echo "## DFState: erb e apply_mask"; grep -n 'pub erb\|pub fn apply_mask\|pub fn apply_interp_band_gain' $SPIKE/libdf-pristine/src/lib.rs
} 2>&1 | tee $SPIKE/evidence/T6-code-points.txt
```

Expected (commit pinado): `process` na 503; "Run post filter" na 601; "Limit noise attenuation" na 610; o laço de `synthesis` entre 616 e 625; `pub erb: Vec<usize>` no `DFState`; `pub fn apply_mask` e `pub fn apply_interp_band_gain`. Conclusão a escrever: o espectro final (`spec_enh`, uma vista mutável sobre `self.spec_buf`) existe entre a linha 614 e a 616, **dentro** de `process`; `get_mut_spec_enh` (682-695) existe mas só pode ser chamado **depois** de `process`, que já fez o iSTFT, então um gancho exige alterar `process`.

- [ ] **Step 2: Criar a cópia com o gancho**

```bash
export SPIKE=<SCRATCHPAD>/spike-libdf
cp -r $SPIKE/libdf-pristine $SPIKE/libdf-eqhook
sed -i 's/^name = "deep_filter"$/name = "deep_filter_eqhook"/' $SPIKE/libdf-eqhook/Cargo.toml
```

Em `$SPIKE/libdf-eqhook/src/tract.rs`, troque a assinatura `pub fn process(&mut self, noisy: ArrayView2<f32>, mut enh: ArrayViewMut2<f32>) -> Result<f32> {` por:

```rust
    pub fn process(&mut self, noisy: ArrayView2<f32>, enh: ArrayViewMut2<f32>) -> Result<f32> {
        self.process_with_spec_hook(noisy, enh, |_| {})
    }

    /// Like `process`, but calls `hook` on the final enhanced spectrum `[ch, n_freqs]`
    /// right before the inverse STFT.
    pub fn process_with_spec_hook<F>(
        &mut self,
        noisy: ArrayView2<f32>,
        mut enh: ArrayViewMut2<f32>,
        mut hook: F,
    ) -> Result<f32>
    where
        F: FnMut(&mut ArrayViewMut2<Complex32>),
    {
```

(o corpo original de `process` continua logo abaixo, inalterado) e, entre o bloco `if let Some(lim) = self.atten_lim { ... }` (termina na linha 614 do original) e o `for (state, spec_ch, mut enh_out_ch) in izip!(`, insira a linha:

```rust
        hook(&mut spec_enh);
```

- [ ] **Step 3: Harness do gancho**

```bash
export SPIKE=<SCRATCHPAD>/spike-libdf
mkdir -p $SPIKE/harness-eq/src
cp $SPIKE/harness/Cargo.lock $SPIKE/harness-eq/Cargo.lock
```

Cria `$SPIKE/harness-eq/Cargo.toml`:

```toml
[package]
name = "spike-harness-eq"
version = "0.0.0"
edition = "2021"
publish = false

[workspace]

[dependencies]
df_eqhook = { path = "../libdf-eqhook", package = "deep_filter_eqhook", default-features = false, features = ["tract"] }
ndarray = { version = "=0.15.6", default-features = false, features = ["std"] }

[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
```

Cria `$SPIKE/harness-eq/src/main.rs`:

```rust
use std::{env, fs, path::PathBuf};

use df_eqhook::tract::{DfParams, DfTract, RuntimeParams};
use df_eqhook::{apply_interp_band_gain, Complex32};
use ndarray::{Array2, ArrayView2};

const HOP: usize = 480;

fn synthetic(frames: usize) -> Vec<f32> {
    let mut state: u32 = 0x1234_5678;
    (0..frames * HOP)
        .map(|i| {
            let t = i as f64 / 48_000.0;
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

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let (asset, out, mode) = (&args[0], &args[1], args[2].as_str());
    let mut model = DfTract::new(
        DfParams::new(PathBuf::from(asset)).expect("params"),
        &RuntimeParams::default(),
    )
    .expect("model");
    let erb = model.df_states[0].erb.clone(); // 32 bands, bins per band
    let mut gains = vec![1.0f32; erb.len()];
    match mode {
        "none" => {}
        "flat6" => gains.iter_mut().for_each(|g| *g = 10f32.powf(6.0 / 20.0)),
        "top12" => {
            // +12 dB on the last 8 ERB bands only
            let n = gains.len();
            gains[n - 8..].iter_mut().for_each(|g| *g = 10f32.powf(12.0 / 20.0));
        }
        other => panic!("unknown mode {other}"),
    }
    let mut out_samples: Vec<f32> = Vec::new();
    for frame in synthetic(1000).chunks_exact(HOP) {
        let input = ArrayView2::from_shape((1, HOP), frame).expect("shape");
        let mut output = Array2::<f32>::zeros((1, HOP));
        model
            .process_with_spec_hook(input, output.view_mut(), |spec| {
                apply_interp_band_gain::<Complex32>(spec.as_slice_mut().expect("contiguous"), &gains, &erb);
            })
            .expect("process");
        out_samples.extend_from_slice(output.as_slice().expect("contiguous"));
    }
    let bytes: Vec<u8> = out_samples.iter().flat_map(|x| x.to_le_bytes()).collect();
    fs::write(out, bytes).expect("write");
    println!("eqhook mode={mode} bands={} out={out}", erb.len());
}
```

O gancho multiplica o espectro de cada quadro por ganhos lineares por banda ERB usando a rotina do próprio libDF (`apply_interp_band_gain`, a mesma que o `apply_mask` usa para a máscara de estágio 1). O autor do plano **não compilou** este código; se o genérico de `apply_interp_band_gain` não aceitar `Complex32` como escrito, use `df_states[0].apply_mask(spec_slice, &gains)` (mesmo efeito, já público).

- [ ] **Step 4: Escrever o verificador de EQ**

Cria `$SPIKE/scripts/eq_check.py`:

```python
#!/usr/bin/env python3
"""eq_check.py <baseline.f32> <hooked.f32> <modo>: razao de energia por faixa e atraso entre as saidas."""
import sys
import numpy as np

a = np.fromfile(sys.argv[1], "<f4").astype("f8")
b = np.fromfile(sys.argv[2], "<f4").astype("f8")
mode = sys.argv[3]
skip = 480 * 50  # ignora aquecimento
a, b = a[skip:], b[skip:]


def band_db(x, lo, hi):
    spec = np.abs(np.fft.rfft(x * np.hanning(len(x)))) ** 2
    f = np.fft.rfftfreq(len(x), 1 / 48000)
    return 10 * np.log10(spec[(f >= lo) & (f < hi)].sum() + 1e-30)


rms_db = 20 * np.log10(np.sqrt(np.mean(b**2)) / np.sqrt(np.mean(a**2)))
# lag por correlacao cruzada em +-64 amostras (0 = sem latencia extra)
n = min(len(a), 480 * 400)
lags = range(-64, 65)
corr = [np.dot(a[64:n - 64], b[64 + k:n - 64 + k]) for k in lags]
lag = list(lags)[int(np.argmax(corr))]
print(f"modo={mode} rms_ratio_dB={rms_db:+.2f} lag_amostras={lag}")
for lo, hi in ((100, 1000), (1000, 4000), (4000, 12000), (12000, 20000), (20000, 24000)):
    print(f"  faixa {lo}-{hi} Hz: delta_dB={band_db(b, lo, hi) - band_db(a, lo, hi):+.2f}")
```

- [ ] **Step 5: Compilar e rodar os três modos**

```bash
export SPIKE=<SCRATCHPAD>/spike-libdf
export RUSTUP_HOME=$SPIKE/rustup CARGO_HOME=$SPIKE/cargo PATH=$SPIKE/cargo/bin:$PATH
cd $SPIKE/harness-eq && cargo build --release 2>&1 | tail -10
B=$SPIKE/harness-eq/target/release/spike-harness-eq
{
  $B $SPIKE/asset-stock.tar.gz $SPIKE/eq-none.f32 none
  $B $SPIKE/asset-stock.tar.gz $SPIKE/eq-flat6.f32 flat6
  $B $SPIKE/asset-stock.tar.gz $SPIKE/eq-top12.f32 top12
  echo "## gancho vazio vs golden-pristine (a cópia com o gancho nao pode mudar o upstream)"
  $SPIKE/venv/bin/python $SPIKE/scripts/compare.py $SPIKE/golden-pristine.f32 $SPIKE/eq-none.f32
  $SPIKE/venv/bin/python $SPIKE/scripts/eq_check.py $SPIKE/eq-none.f32 $SPIKE/eq-flat6.f32 flat6
  $SPIKE/venv/bin/python $SPIKE/scripts/eq_check.py $SPIKE/eq-none.f32 $SPIKE/eq-top12.f32 top12
} 2>&1 | tee $SPIKE/evidence/T6-hook.txt
```

Critérios, escritos antes de medir: (a) `eq-none` vs `golden-pristine` com `bit_exact=True` (o invólucro não altera o upstream); (b) `flat6`: `rms_ratio_dB` entre `+5.6` e `+6.4` e `lag_amostras=0` (ganho aplicado no ponto certo, **sem latência extra**); (c) `top12`: faixas até 4 kHz com `|delta_dB| <= 1,0` e a faixa 12-20 kHz com `delta_dB` entre `+10` e `+13`, `lag_amostras=0`. A banda do `top12` é "as 8 últimas bandas ERB"; se o sinal sintético tiver pouca energia lá, aumente o ruído do sinal (não o critério) e registre.

- [ ] **Step 6: Registrar o que a cabeça de EQ precisaria receber (leitura de código)**

No relatório, responda com os números de linha do commit pinado: (1) a saída do `enc` já entrega `emb [ch, 1, 512]` por quadro (`process_raw`, linhas 454-461: `emb` é o terceiro valor retirado de `enc_emb`), disponível no mesmo `process_raw` onde uma terceira sessão `eq_head.run(tvec!(emb))` caberia; (2) o espectro do gancho corresponde ao quadro atrasado por `lookahead` (o espectro vem de `rolling_spec_buf_y.get_mut(df_order - 1)`, linhas 541-543 e 571): o treino precisa definir em qual atraso o `emb` condiciona o ganho (risco para o repo de treino); (3) o gancho roda por quadro de 480 amostras já em `process`, então o EQ por banda **não adiciona latência**; ele só custa a multiplicação por 481 bins e, se houver uma terceira sessão ONNX, o custo dela (medir na fase 4); (4) o fork de EQ-no-espectro soma ao patch da T4 cerca de 12 linhas (`diff` do `libdf-eqhook`, registre a contagem real). Veredito Q6: **GO (viável)** se (a), (b) e (c) passam, **NO-GO** caso contrário, com a evidência.

- [ ] **Step 7: Checkpoint**

```bash
cd <REPO> && git status --short
```

Expected: só os dois `??` de docs. **Não commitar.**

---

### Task 7: Relatório do spike

**Files:**
- Create: `docs/superpowers/plans/2026-10-01-clearcore-studio-phase3-libdf-spike-report.md`

**Interfaces:**
- Consumes: todo `$SPIKE/evidence/` e os vereditos Q1 a Q6.
- Produces: o relatório que desbloqueia (ou não) o plano da fase 4 (spec, seção 9: "O plano da fase 4 só é escrito depois do resultado do spike da fase 3").

- [ ] **Step 1: Criar o relatório a partir do modelo abaixo**

Crie o arquivo com este conteúdo e substitua **todo** campo `{{...}}` por texto ou saída literal (cole saídas de comando entre blocos de código; nada de paráfrase no lugar de evidência):

````markdown
# Fase 3: spike do fork do libDF — relatório

Data de execução: {{AAAA-MM-DD}}. Executor: {{agente/modelo}}. Plano: `docs/superpowers/plans/2026-10-01-clearcore-studio-phase3-libdf-spike.md`.
Máquina: {{linha "Model name" do lscpu}}; governor {{valor}}; núcleo fixado {{CPU_P}}.
libDF: `deep_filter` 0.5.6 @ `978576aa8400552a4ce9730838c635aa30db5e61`; tract {{versão de `cargo tree -i tract-core`}}; rustc {{versão}}.

## 1. Veredito por pergunta

| # | Pergunta | Veredito | Evidência (arquivo em `$SPIKE/evidence/`) |
|---|---|---|---|
| Q1 | O libDF está disponível offline? | {{GO / NO-GO}} | {{T1-verdict.txt, T1-inventory.txt}} |
| Q2 | A cópia intocada compila e reproduz o golden? | {{GO / NO-GO}} | {{T2-build.txt, T2-golden.txt, T2-replay.txt}} |
| Q3 | Dá para gerar o par ONNX com FiLM em identidade? | {{GO / NO-GO}} | {{T3-gen.txt}} |
| Q4 | O fork tem paridade com gamma=1, beta=0? | {{GO forte / GO com tolerância / NO-GO}} | {{T4-parity.txt, T4-patch.diff}} |
| Q5 | O custo por frame é aceitável? | {{GO / NO-GO}} | {{T5-summary.txt}} |
| Q6 | A cabeça de EQ no espectro antes do iSTFT é viável? | {{GO / NO-GO}} | {{T6-code-points.txt, T6-hook.txt}} |

Decisão para a fase 4: {{rota A (entradas gamma/beta com patch no libDF) / rota B (constantes embutidas, sem patch) / nenhuma (fases 1 e 2 seguem valendo)}} e por quê, em 2 a 4 frases.

## 2. Disponibilidade e licença (Q1)

Resposta offline literal: {{cole T1-verdict.txt}}.
O que foi preciso baixar, autorizado por {{quem}} em {{quando}}: {{lista}}.
Integridade: HEAD = {{sha}}; SHA-256 de `tract.rs` = {{sha}} (esperado `ad809ddb10e489990e572a8e97d79394df762c6f9c16a8defc9484d03c94426b`).
Licença, texto literal do `LICENSE` do upstream no commit pinado:

```
{{cole o conteúdo de LICENSE}}
```

`libDF/Cargo.toml`: {{linha license}}. Obrigações para o fork: {{manter LICENSE-MIT e LICENSE-APACHE, aviso de copyright, marcar arquivos modificados (Apache-2.0, 4(b))}}. Decisão registrada do dono em `THIRD_PARTY_LICENSES`: {{trecho}}. Isto é leitura de licença, não parecer jurídico.
Consequência para o build offline do produto: {{vendorizar o fork no repositório / outra}}.

## 3. Golden local e reprodução do upstream (Q2)

```
{{T2-golden.txt}}
{{T2-replay.txt}}
```

## 4. ONNX com FiLM (Q3)

```
{{T3-gen.txt, incluindo SHA-256 dos arquivos gerados}}
```

Pontos de inserção: enc `/emb_gru/linear_in/1/Relu_output_0`, df_dec `/df_gru/linear_in/linear_in.1/Relu_output_0`; forma `[1, S, 256]`.

## 5. Patch e paridade (Q4)

Tamanho do patch (de `T4-patch.diff`): `src/tract.rs` +{{n}} / -{{n}} linhas; `Cargo.toml` +{{n}} / -{{n}}; total do diff {{n}} linhas. É "mínimo"? {{sim/não e por quê}}.

| Caso | O que compara | bit_exact | max_abs_diff | primeiro/último quadro com diferença |
|---|---|---|---|---|
| B | fork + asset aprovado vs pristine | {{}} | {{}} | {{}} |
| C | fork + FiLM identidade (padrão) vs pristine | {{}} | {{}} | {{}} |
| D | fork + FiLM, set_film(1, 0) vs pristine | {{}} | {{}} | {{}} |
| E | fork + FiLM, set_film(1.5, 0.1) vs pristine (deve diferir) | {{}} | {{}} | {{}} |
| F | pristine + constantes embutidas (1.5, 0.1) vs pristine | {{}} | {{}} | {{}} |
| G | rota entradas (E) vs rota constantes (F) | {{}} | {{}} | {{}} |

Tolerância documentada (se não bit-exato): {{valor e justificativa; deve ficar abaixo do 1e-4 do gate}}.
Atraso do pulsifier: {{contagem de nós Delay com e sem FiLM, se medida; senão "não medido: Step 7 não foi necessário"}}.
Mensagem literal do teste vermelho (carregador intocado + asset FiLM): `{{T4-red.txt}}`.

## 6. Custo (Q5)

```
{{T5-summary.txt}}
```

Limite adotado antes de medir: p50 do fork com FiLM até +10% sobre o pristine e p99 < 7 ms. Resultado: {{GO/NO-GO}}. Válido só nesta máquina; não é medição fim a fim do produto.

## 7. Cabeça de EQ no espectro (Q6)

Pontos de código (commit pinado): {{cole T6-code-points.txt}}.
Resultado do gancho: {{cole T6-hook.txt}}. Latência extra: {{lag_amostras}} amostras.
O que a cabeça precisaria receber: {{emb [ch,1,512] por quadro; alinhamento de atraso a definir com o repo de treino}}.
Linhas adicionais do gancho: {{n}}.

## 8. Riscos

| Risco | Probabilidade | Impacto | Mitigação / quem decide |
|---|---|---|---|
| Fork contém `unsafe` e o workspace tem `unsafe_code = "forbid"` | {{}} | {{}} | {{vendorizar fora do workspace / decidir}} |
| Patch do libDF deixa de ser pequeno | {{}} | {{}} | {{}} |
| Atraso do pulsifier muda os primeiros quadros do `enc` | {{}} | {{}} | {{rota B ou ponto de FiLM no treino}} |
| Troca de perfil exige recarregar modelo (rota B) | {{}} | {{}} | {{reinício de geração do engine}} |
| Asset de FiLM precisa de descritor no registro e assinatura do usuário | {{}} | {{}} | {{fase 4}} |
| Custo medido só numa máquina | {{}} | {{}} | {{medir fim a fim na fase 0}} |
| {{outro risco encontrado}} | {{}} | {{}} | {{}} |

## 9. Não verificado

{{liste cada coisa que o spike não pôde ou não quis verificar, uma por linha}}

## 10. Checkpoint

```
{{saída de git status --short}}
```
````

- [ ] **Step 2: Verificar que não sobrou campo por preencher**

```bash
cd <REPO>
grep -n '{{' docs/superpowers/plans/2026-10-01-clearcore-studio-phase3-libdf-spike-report.md || echo "sem campos pendentes"
grep -c 'GO' docs/superpowers/plans/2026-10-01-clearcore-studio-phase3-libdf-spike-report.md
```

Expected: `sem campos pendentes`. Qualquer `{{` restante é falha do passo. Se alguma tarefa não rodou (por exemplo, sem autorização de rede), preencha o campo com "NÃO EXECUTADO: motivo", nunca com uma suposição.

- [ ] **Step 3: Conferir que o repositório e o cache do cargo estão intactos**

```bash
cd <REPO>
git status --short
git diff --stat
ls -A ~/.cargo
```

Expected: `git status --short` lista só `?? docs/superpowers/plans/2026-10-01-clearcore-studio-phase3-libdf-spike.md`, `?? docs/superpowers/plans/2026-10-01-clearcore-studio-phase3-libdf-spike-report.md` e `?? docs/superpowers/specs/2026-10-01-clearcore-studio-pipeline-design.md`; `git diff --stat` vazio; `~/.cargo` sem entradas novas.

- [ ] **Step 4: Checkpoint**

Cole a saída do `git status --short` na seção 10 do relatório. **Não commitar, não fazer push.** Entregue ao orquestrador: caminho do relatório, a tabela de vereditos e a decisão para a fase 4.

---

## Auto-revisão

**Cobertura do spec.** Seção 2 (estado atual verificado): o plano reconfere o contrato `960/480/32/96/5/2` e a latência de 1.440 amostras só como restrição; não altera nada do estado atual. Seção 7: "fork do libDF no commit já pinado, patch mínimo para `gamma/beta`" é a T4; "teste de paridade (gamma=1, beta=0) contra o upstream" é a T4 (casos B, C, D); "conferir a licença do código do libDF" é o Step 6 da T1; "terceira variante: cabeça de EQ no espectro do libDF antes do iSTFT" é a T6; "o agente não produz assinaturas" está nas restrições e nenhum passo toca manifest. Requisito 5 ao repo de treino (script que exporta DFNet3 com FiLM em identidade): a T3 gera o par por edição do ONNX e registra, como alternativa de NO-GO, pedir esse script. Seção 10: "patch pode não ser pequeno" é medido em linhas na T4/T7; "licença do libDF" idem; custo ("medir a latência fim a fim antes de gastar margem") está tratado na T5, que declara que o número não é fim a fim. Seção 9 (fase 3 em paralelo, sem commit, cada tarefa termina com checkpoint, plano da fase 4 só depois do spike): coberto. T1 a T7 seguem a ordem pedida.

**Placeholders.** Os únicos campos abertos são os `{{...}}` do modelo de relatório (T7), que o executor preenche por design e que o Step 2 da T7 verifica. O único valor lido em tempo de execução dentro de um comando é `<CPU_P>` (T5), obtido do `lscpu` no passo anterior. Não há "TBD/TODO"; o código dos passos está completo, com a ressalva explícita de que o harness, o patch Rust e o gancho de EQ **não foram compilados** pelo autor do plano (sem toolchain nesta máquina), enquanto `gen_film_onnx.py` **foi executado** e passou.

**Consistência de tipos e nomes.** `FilmVectors { gamma_enc, beta_enc, gamma_df, beta_df }`, `film_hidden() -> Option<usize>` e `set_film(&mut self, &FilmVectors) -> Result<()>` são definidos na T4 (Steps 2 e 4) e usados com os mesmos nomes no harness da T2 (`apply_film`). O modo `none|identity|scaled` do harness coincide com os rótulos B a G da T4 e com os comandos da T5. O pacote do fork é `deep_filter_fork` na T2 (Step 4) e no `Cargo.toml` do harness; o da T6 é `deep_filter_eqhook`. Os arquivos de evidência citados no relatório (`T1-*` a `T6-*`) são os criados em cada tarefa. A lista de entradas `["feat_erb","feat_spec","gamma","beta"]` e `["emb","c0","gamma","beta"]` bate com a saída do Step 3 da T3.
