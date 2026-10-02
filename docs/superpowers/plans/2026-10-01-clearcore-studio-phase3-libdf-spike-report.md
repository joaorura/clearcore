# Fase 3: spike do fork do libDF — relatório

Data de execução: 2026-10-01 (a execução cruzou a meia-noite e terminou em 2026-10-02). Executor: agente Claude Sonnet 5.5 (execução autônoma, sem commit e sem push). Plano: `docs/superpowers/plans/2026-10-01-clearcore-studio-phase3-libdf-spike.md`.
Máquina: Intel(R) Core(TM) Ultra 7 265H (16 CPUs, híbrida); governor `performance`; núcleo fixado CPU 2 (núcleo P, MAXMHZ 5300). Build **release** (`opt-level=3`, `lto="fat"`, `codegen-units=1`) em todas as medidas.
libDF: `deep_filter` 0.5.6 @ `978576aa8400552a4ce9730838c635aa30db5e61`; tract `v0.19.16` (saída de `cargo tree -i tract-core`, ver seção 3); rustc 1.90.0 (1159e78c4 2025-09-14).

Desvios do plano, todos registrados aqui: (1) a toolchain não foi baixada em `$SPIKE/rustup|cargo`: usou-se a que o orquestrador deixou em `scratchpad/rust-env` (`CARGO_HOME`/`RUSTUP_HOME` lá, `CARGO_TARGET_DIR=$SPIKE/target`); (2) o `Cargo.lock` do harness é o do repositório, que o cargo podou das crates não usadas (versões mantidas: tract-core 0.19.16, ndarray 0.15.6); (3) `unwrap`/`expect` no harness são descartáveis, como o plano permite; (4) o `compare.py`/`gen_film_onnx.py` foram reescritos a partir do texto do plano (já existia uma cópia de `gen_film_onnx.py` de uma sondagem anterior; foi sobrescrita pelo texto do plano).

## 1. Veredito por pergunta

| # | Pergunta | Veredito | Evidência (arquivo em `$SPIKE/evidence/`) |
|---|---|---|---|
| Q1 | O libDF está disponível offline? | **NO-GO offline; GO com fetch autorizado para o scratchpad** | T1-verdict.txt, T1-inventory.txt, T1-fetch.txt |
| Q2 | A cópia intocada compila e reproduz o golden? | **GO** | T2-build.txt, T2-golden.txt, T2-replay.txt |
| Q3 | Dá para gerar o par ONNX com FiLM em identidade? | **GO** | T3-gen.txt |
| Q4 | O fork tem paridade com gamma=1, beta=0? | **GO forte** (bit-exato, sem tolerância); patch **não é "mínimo"** pelo critério do plano (+127 linhas em `tract.rs`) | T4-parity.txt, patch-libdf-film.diff, T4-red.txt |
| Q5 | O custo por frame é aceitável? | **GO** (p50 +0,1%; p99 0,74 ms contra limite de 7 ms) | T5-summary.txt |
| Q6 | A cabeça de EQ no espectro antes do iSTFT é viável? | **GO** (a, b e c passam; lag 0) | T6-code-points.txt, T6-hook.txt |

Decisão para a fase 4: **rota A (entradas gamma/beta com patch no libDF) é viável tecnicamente**: paridade bit-exata com o upstream em identidade, custo mensurável como zero e troca de perfil sem recarregar o modelo (`set_film` não reinicia as GRUs). O preço é um fork de ~+127/-21 linhas em `tract.rs` (mais o gancho de espectro, +15/-1) que precisa ser vendorizado no repositório e tratado frente à política `unsafe_code = "forbid"`. A **rota B (constantes embutidas, sem patch no libDF)** carrega no libDF intocado (controle F) e dá saída idêntica à rota A com os mesmos valores (G), mas **só serve para o spike**: um ONNX gerado por usuário não tem SHA-256 fixo e não passa no gate de assets (SHA-256, tamanho, assinatura Ed25519, allowlist). Detalhes na seção 9 (recomendação para a fase 4).

## 2. Disponibilidade e licença (Q1)

Resposta offline literal (T1-verdict.txt, primeira versão, antes do fetch):

```
Q1 offline: NO-GO (BLOCKED_OFFLINE_DEPENDENCY)
motivo: ver T1-inventory.txt (cargo/rustc ausentes no PATH limpo; fonte do libDF ausente; onnx/onnxruntime ausentes)
```

Inventário (T1-inventory.txt):

```
## toolchain
NENHUM no PATH
ls: não foi possível acessar '~/.rustup': Arquivo ou diretório inexistente
## cache do cargo
~/.cargo
~/.cargo/.package-cache
~/.cargo/registry
~/.cargo/git
~/.cargo/.global-cache
~/.cargo/.package-cache-mutate
~/.cargo/registry/CACHEDIR.TAG
~/.cargo/git/CACHEDIR.TAG
64K	~/.cargo
## vendor
<REPO>/vendor/approved/df-compatible-release-asset-v1.bin
## busca do fonte do libDF (excluindo o scratchpad desta sessao)
(fim da busca)
## pin
12:    "dep:deep_filter",
23:deep_filter = { git = "https://github.com/Rikorose/DeepFilterNet.git", rev = "978576aa8400552a4ce9730838c635aa30db5e61", package = "deep_filter", default-features = false, features = ["tract"], optional = true }
## python
Python 3.14.7
ModuleNotFoundError: No module named 'onnx'
ModuleNotFoundError: No module named 'onnxruntime'
numpy 2.5.2
torch 2.13.0+cu130
## rede (so cabecalho)
https://github.com HTTP/2 200 
https://pypi.org/simple/onnx/ HTTP/2 200 
https://static.rust-lang.org HTTP/2 200 
https://static.crates.io HTTP/2 403 
## cpu
CPU(s):                                  16
Model name:                              Intel(R) Core(TM) Ultra 7 265H
CPU(s) scaling MHz:                      77%
CPU CORE    MAXMHZ
  0    0 5300,0000
  1    1 5300,0000
  2    2 5300,0000
  3    3 5300,0000
  4    4 5300,0000
  5    5 5300,0000
  6    6 4600,0000
  7    7 4600,0000
  8    8 4600,0000
  9    9 4600,0000
 10   10 4600,0000
 11   11 4600,0000
 12   12 4600,0000
 13   13 4600,0000
 14   14 2500,0000
 15   15 2500,0000
```

O que foi preciso baixar, autorizado pelo usuário (autorização repassada pelo orquestrador), **somente para dentro do scratchpad da sessão**: toolchain Rust 1.90.0 (já instalada pelo orquestrador em `scratchpad/rust-env`), o upstream no commit pinado (clone esparso do GitHub), as crates do `Cargo.lock` (cache em `scratchpad/rust-env/cargo`) e `onnx`/`onnxruntime`/`numpy` (PyPI, venv em `$SPIKE/venv`).

Integridade e verdade final de Q1 (T1-fetch.txt e T1-verdict.txt):

```
toolchain (pre-instalada pelo orquestrador em scratchpad/rust-env; RUSTUP_HOME/CARGO_HOME la, em vez de $SPIKE/rustup)
rustc 1.90.0 (1159e78c4 2025-09-14)
cargo 1.90.0 (840b83a10 2025-07-30)
<SCRATCHPAD>/rust-env/cargo
dda7234360b7f578ca8b0ddcb80145646fa61a67c1720a5abc7051b35c9fcb71 *./rustup-init
HEAD=978576aa8400552a4ce9730838c635aa30db5e61
ESPERADO=978576aa8400552a4ce9730838c635aa30db5e61
ad809ddb10e489990e572a8e97d79394df762c6f9c16a8defc9484d03c94426b  $SPIKE/upstream/libDF/src/tract.rs
180df9cc7e7129d4372f09345846b17709a296bd51e3f8af073e1333ed1aab48  $SPIKE/upstream/libDF/src/lib.rs
ESPERADO tract.rs=ad809ddb10e489990e572a8e97d79394df762c6f9c16a8defc9484d03c94426b
1065 $SPIKE/upstream/libDF/src/tract.rs
pristine identico ao upstream
```

```
Q1 offline: NO-GO (BLOCKED_OFFLINE_DEPENDENCY: libDF, toolchain no PATH e onnx ausentes)
Q1 com fetch autorizado para $SPIKE/scratchpad: GO (HEAD=978576aa... e SHA-256 de tract.rs conferem; rustc 1.90.0 e cargo 1.90.0 em rust-env; onnx 1.23.1, onnxruntime 1.30.0 importam)
consequencia fase 4: build offline do produto exige vendorizar o fork dentro do repositorio
```

Python (T1-python.txt):

```
         If this is intentional, set `export UV_LINK_MODE=copy` or use `--link-mode=copy` to suppress this warning.
Installed 8 packages in 142ms
 + flatbuffers==25.12.19
 + ml-dtypes==0.6.0
 + numpy==2.5.3
 + onnx==1.23.1
 + onnxruntime==1.30.0
 + packaging==26.3
 + protobuf==7.36.2
 + typing-extensions==4.16.0
onnx 1.23.1 ort 1.30.0 numpy 2.5.3
c94d91f70911001c946e0fabb4aa9adc37045f45a03b56008cb0c8244cb63616  $SPIKE/asset-stock.tar.gz
$SPIKE/asset/tmp/export/config.ini
$SPIKE/asset/tmp/export/df_dec.onnx
$SPIKE/asset/tmp/export/enc.onnx
$SPIKE/asset/tmp/export/erb_dec.onnx
```

Licença, texto literal do `LICENSE` do upstream no commit pinado (T1-license.txt):

```
### LICENSE (raiz do upstream no commit pinado)
## License

Licensed under either of
 * Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)
at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.

### libDF/Cargo.toml
2:name = "deep_filter"
3:version = "0.5.6"
4:authors = ["Hendrik Schröter"]
5:edition = "2021"
8:license = "MIT/Apache-2.0"
13:name = "df"
17:name = "sample-hdf5"
22:name = "sample-dataset"
27:name = "deep-filter"
130:name = "deep_filter"
133:name = "libdeepfilter"
136:name = "deepfilter"

### LICENSE-MIT (cabecalho)
The MIT License (MIT)
Copyright (c) 2021 Hendrik Schröter


### LICENSE-APACHE (cabecalho)
                              Apache License
                        Version 2.0, January 2004
                     http://www.apache.org/licenses/


### repositorio Clearcore (decisao do dono)
THIRD-PARTY LICENSE REGISTER

The exact approved M0 release asset is `vendor/approved/df-compatible-release-asset-v1.bin`. It is the byte-for-byte upstream `DeepFilterNet3_onnx.tar.gz` from `Rikorose/DeepFilterNet` tag `v0.5.6`, SHA-256 `c94d91f70911001c946e0fabb4aa9adc37045f45a03b56008cb0c8244cb63616`.

The repository's Apache-2.0 license applies only to original repository work. It does not grant rights to DeepFilterNet code or weights, libDF, tract, platform SDKs, drivers, samples, corpora, or any other third-party material.

Before a third-party component or asset may be distributed, this register must identify its exact version or immutable source, copyright holder, license, redistribution terms, conversion or derivative-work terms where applicable, and corresponding legal approval evidence.

Copyright (c) 2021 Hendrik Schröter. João Messias Lima Pereira recorded the project owner's explicit directive for this free and open-source application to treat the selected model weights and code as `MIT OR Apache-2.0`. This is an owner decision, not external legal counsel. Attribution and license references: [upstream repository](https://github.com/Rikorose/DeepFilterNet/tree/v0.5.6), [MIT](https://github.com/Rikorose/DeepFilterNet/blob/v0.5.6/LICENSE-MIT), and [Apache-2.0](https://github.com/Rikorose/DeepFilterNet/blob/v0.5.6/LICENSE-APACHE).
```

`libDF/Cargo.toml`: `license = "MIT/Apache-2.0"`. Obrigações para o fork: manter `LICENSE-MIT` e `LICENSE-APACHE` e o aviso "Copyright (c) 2021 Hendrik Schröter", e, pela Apache-2.0 (seção 4(b)), marcar os arquivos modificados (o `tract.rs` do fork e o `Cargo.toml`). Decisão registrada do dono em `THIRD_PARTY_LICENSES`: "João Messias Lima Pereira recorded the project owner's explicit directive for this free and open-source application to treat the selected model weights and code as `MIT OR Apache-2.0`. This is an owner decision, not external legal counsel." Isto é leitura de licença, não parecer jurídico.
Consequência para o build offline do produto: o fork precisa ser **vendorizado dentro do repositório** (hoje o `crates/model/Cargo.toml` depende do libDF por `git = ...` + `rev`, que exige rede); decisão fora do spike.

## 3. Golden local e reprodução do upstream (Q2)

Build (T2-build.txt, final):

```
warning: `deep_filter` (lib) generated 5 warnings
   Compiling spike-harness v0.0.0 ($SPIKE/harness)
    Finished `release` profile [optimized] target(s) in 2m 28s
## versoes
tract-core v0.19.16
├── deep_filter v0.5.6 ($SPIKE/libdf-pristine)
│   └── spike-harness v0.0.0 ($SPIKE/harness)
ndarray v0.15.6
├── deep_filter v0.5.6 ($SPIKE/libdf-pristine)
```

O build compilou **sem nenhuma correção** do harness do plano. Os cinco avisos do rustc são do código do libDF (lifetimes elididas).

```
golden frames=1000 out=$SPIKE/golden-pristine.f32
golden frames=1000 out=$SPIKE/golden-pristine-2.f32
c041ee1fa4b2ce4bdad4f07e28b8fdcb901c56bab20380d17abd254d5c9736aa  $SPIKE/golden-pristine.f32
c041ee1fa4b2ce4bdad4f07e28b8fdcb901c56bab20380d17abd254d5c9736aa  $SPIKE/golden-pristine-2.f32
samples=480000 bit_exact=True max_abs_diff=0.000e+00 frames_with_diff=0 first_diff_frame=-1 last_diff_frame=-1
max_abs_diff_first_12_frames= ['0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00']
err_rms_dB_re_signal=-569.3
replay frames=300 max_abs_diff=1.3456809996870156e-7 max_excess_over_tol=-9.995888396517185e-5 within_tol=true
```

Os dois SHA-256 são idênticos (`bit_exact=True`, determinístico). O replay do golden congelado do repositório (6 casos x 50 frames, um único backend, na ordem do gerador) dá `max_abs_diff=1,35e-7` e `within_tol=true` contra a tolerância `1e-4` do gate. A saída sintética não é silêncio (RMS 0,0291; pico 0,1434).

## 4. ONNX com FiLM (Q3)

```
enc seed=0 identidade max_abs_diff=0.0 bit_exato=True
enc seed=1 identidade max_abs_diff=0.0 bit_exato=True
enc seed=2 identidade max_abs_diff=0.0 bit_exato=True
enc seed=3 identidade max_abs_diff=0.0 bit_exato=True
enc seed=4 identidade max_abs_diff=0.0 bit_exato=True
enc gamma=1.5 beta=0.1 altera a saida: True
enc baked == entradas com mesmos valores: True
df_dec seed=0 identidade max_abs_diff=0.0 bit_exato=True
df_dec seed=1 identidade max_abs_diff=0.0 bit_exato=True
df_dec seed=2 identidade max_abs_diff=0.0 bit_exato=True
df_dec seed=3 identidade max_abs_diff=0.0 bit_exato=True
df_dec seed=4 identidade max_abs_diff=0.0 bit_exato=True
df_dec gamma=1.5 beta=0.1 altera a saida: True
df_dec baked == entradas com mesmos valores: True
ok $SPIKE/out/film_identity_asset.tar.gz $SPIKE/out/film_baked_asset.tar.gz
cde085a00f74e6ec7f0fac76414cca9abd3cd4beb9f52eec08eb716085493aee  $SPIKE/out/df_dec_film_baked.onnx
6fb094ac493cba8aecd54ea9d5dff7c0d9e23e214ddf306a387b550eda6d100a  $SPIKE/out/df_dec_film.onnx
a86614cc58bfef3e6d7891276dff904ea684095f8613677fc82214effe7f565c  $SPIKE/out/enc_film_baked.onnx
d8f6847e0b626ed2eeb05756c167348aa6598467947afd20f2e9c4b301a325f0  $SPIKE/out/enc_film.onnx
aa977fafbd81bf6df729661d76ef41e49ae5e6d5ea225d1d8f89f7bca6f18342  $SPIKE/out/film_baked_asset.tar.gz
8783baff55cad837953d5420c0f96edfbe40c01e5e2ac715cb4f3fef2b6cb45d  $SPIKE/out/film_identity_asset.tar.gz
-rw-r--r-- 0/0         1954321 1969-12-31 21:00 tmp/export/enc.onnx
-rw-r--r-- 0/0         3292397 1969-12-31 21:00 tmp/export/erb_dec.onnx
-rw-r--r-- 0/0         3341118 1969-12-31 21:00 tmp/export/df_dec.onnx
-rw-r--r-- 0/0            2067 1969-12-31 21:00 tmp/export/config.ini
enc_film ['feat_erb', 'feat_spec', 'gamma', 'beta']
df_dec_film ['emb', 'c0', 'gamma', 'beta']
enc_film_baked ['feat_erb', 'feat_spec']
df_dec_film_baked ['emb', 'c0']
```

Pontos de inserção: enc `/emb_gru/linear_in/1/Relu_output_0`, df_dec `/df_gru/linear_in/linear_in.1/Relu_output_0`; forma `[1, S, 256]`. Identidade bit-exata no onnxruntime (10 de 10 execuções) e `gamma=1.5, beta=0.1` altera a saída nos dois grafos.

## 5. Patch e paridade (Q4)

Tamanho do patch (`patch-libdf-film.diff`, 293 linhas no total): `src/tract.rs` **+127 / -21** linhas; `Cargo.toml` +1 / -1 (só a renomeação para `deep_filter_fork`). É "mínimo"? **Não pelo critério do plano** (~100 linhas adicionadas): cerca de 36 linhas são os tipos novos (`FilmVectors`, `FilmInputs`), ~21 a API pública (`film_hidden`, `set_film`) e o restante é plumbing de parâmetro (`film_hidden`) por 4 wrappers + 2 `_impl` e a montagem das 4 entradas em `process_raw`. É mecânico e localizado em um arquivo, mas é um registro do risco da seção 10 do spec ("patch pode não ser pequeno").

```
## B: fork + asset aprovado, sem set_film (regressao do patch)
film mode=none frames=1000 out=$SPIKE/golden-B.f32
samples=480000 bit_exact=True max_abs_diff=0.000e+00 frames_with_diff=0 first_diff_frame=-1 last_diff_frame=-1
max_abs_diff_first_12_frames= ['0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00']
err_rms_dB_re_signal=-569.3
## C: fork + asset FiLM identidade, sem set_film (identidade padrao)
film mode=none frames=1000 out=$SPIKE/golden-C.f32
samples=480000 bit_exact=True max_abs_diff=0.000e+00 frames_with_diff=0 first_diff_frame=-1 last_diff_frame=-1
max_abs_diff_first_12_frames= ['0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00']
err_rms_dB_re_signal=-569.3
## D: fork + asset FiLM, set_film(gamma=1, beta=0) explicito
film mode=identity frames=1000 out=$SPIKE/golden-D.f32
samples=480000 bit_exact=True max_abs_diff=0.000e+00 frames_with_diff=0 first_diff_frame=-1 last_diff_frame=-1
max_abs_diff_first_12_frames= ['0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00']
err_rms_dB_re_signal=-569.3
## E: fork + asset FiLM, set_film(gamma=1.5, beta=0.1) (sensibilidade: deve DIFERIR)
film mode=scaled frames=1000 out=$SPIKE/golden-E.f32
samples=480000 bit_exact=False max_abs_diff=1.030e-01 frames_with_diff=895 first_diff_frame=1 last_diff_frame=999
max_abs_diff_first_12_frames= ['0.00e+00', '1.62e-03', '9.64e-03', '3.24e-02', '3.02e-02', '3.32e-02', '3.67e-02', '3.98e-02', '7.81e-02', '8.89e-02', '9.67e-02', '4.51e-02']
err_rms_dB_re_signal=-4.5
## F: controle sem patch: carregador INTOCADO + asset com constantes embutidas (gamma=1.5, beta=0.1)
film mode=none frames=1000 out=$SPIKE/golden-F.f32
samples=480000 bit_exact=False max_abs_diff=1.030e-01 frames_with_diff=895 first_diff_frame=1 last_diff_frame=999
max_abs_diff_first_12_frames= ['0.00e+00', '1.62e-03', '9.64e-03', '3.24e-02', '3.02e-02', '3.32e-02', '3.67e-02', '3.98e-02', '7.81e-02', '8.89e-02', '9.67e-02', '4.51e-02']
err_rms_dB_re_signal=-4.5
## G: rota com entradas (E) vs rota com constantes embutidas (F), mesmos valores
samples=480000 bit_exact=True max_abs_diff=0.000e+00 frames_with_diff=0 first_diff_frame=-1 last_diff_frame=-1
max_abs_diff_first_12_frames= ['0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00']
err_rms_dB_re_signal=-571.4
```

| Caso | O que compara | bit_exact | max_abs_diff | primeiro/último quadro com diferença |
|---|---|---|---|---|
| B | fork + asset aprovado vs pristine | True | 0 | nenhum |
| C | fork + FiLM identidade (padrão, sem set_film) vs pristine | True | 0 | nenhum |
| D | fork + FiLM, set_film(1, 0) vs pristine | True | 0 | nenhum |
| E | fork + FiLM, set_film(1.5, 0.1) vs pristine (deve diferir) | False | 1,030e-1 | 1 / 999 (895 quadros) |
| F | pristine + constantes embutidas (1.5, 0.1) vs pristine | False | 1,030e-1 | 1 / 999 (895 quadros) |
| G | rota entradas (E) vs rota constantes (F) | True | 0 | nenhum |

Replay do golden congelado com o **build fork** (T4-replay-fork.txt), extra ao plano:

```
## replay do golden congelado com o build FORK (asset aprovado)
replay frames=300 max_abs_diff=1.3456809996870156e-7 max_excess_over_tol=-9.995888396517185e-5 within_tol=true
## replay com o build FORK + asset FiLM em identidade (sem set_film)
replay frames=300 max_abs_diff=1.3456809996870156e-7 max_excess_over_tol=-9.995888396517185e-5 within_tol=true
```

Tolerância documentada: **nenhuma necessária** (bit-exato nos casos B, C, D e G; o gate do produto é 1e-4).
Atraso do pulsifier: **hipótese não observada, contagem de nós `Delay` NÃO medida.** O Step 7 do plano só é executado se C/D/G não fossem bit-exatos; todos foram, inclusive G com `gamma=1.5, beta=0.1` (que exercita a soma/multiplicação com valores não triviais) e portanto o alinhamento de atraso do `Mul`/`Add` do `enc` não alterou nenhuma amostra nos 1000 frames. Não foi feita instrumentação do grafo pulsado; a conclusão é por equivalência de saída, não por inspeção do grafo.
Mensagem literal do teste vermelho (carregador intocado + asset FiLM, T4-red.txt):

```

thread 'main' panicked at src/main.rs:37:45:
process: Evaluating #49 "gamma" Source

Caused by:
    Input for node 49 is missing
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

Observação: a mensagem difere do achado (1) do plano. O `with_input_names` não fez o grafo carregar "sem" `gamma/beta` silenciosamente: o nó `Source` `gamma` permanece no grafo e a execução falha porque a entrada não é alimentada. O resultado prático é o mesmo (o carregador intocado não aceita o asset com FiLM), mas por erro explícito em tempo de execução.

### Diff completo do patch (`$SPIKE/patch-libdf-film.diff`, também em `$SPIKE/evidence/T4-patch.diff` com cabeçalhos de caminho)

```diff
--- a/src/tract.rs
+++ b/src/tract.rs
@@ -190,6 +190,42 @@
 
 pub type TractModel = TypedSimpleState<TypedModel, TypedSimplePlan<TypedModel>>;
 
+/// FiLM conditioning vectors fed to the extra `gamma`/`beta` graph inputs.
+/// Every vector has `ch * DfTract::film_hidden()` elements.
+#[derive(Clone, Debug)]
+pub struct FilmVectors {
+    pub gamma_enc: Vec<f32>,
+    pub beta_enc: Vec<f32>,
+    pub gamma_df: Vec<f32>,
+    pub beta_df: Vec<f32>,
+}
+
+#[derive(Clone)]
+struct FilmInputs {
+    gamma: TValue, // [ch, 1, hidden]
+    beta: TValue,  // [ch, 1, hidden]
+}
+
+impl FilmInputs {
+    fn from_slices(ch: usize, hidden: usize, gamma: &[f32], beta: &[f32]) -> Result<Self> {
+        if gamma.len() != ch * hidden || beta.len() != ch * hidden {
+            bail!(
+                "FiLM vectors must have {} elements (got {} and {})",
+                ch * hidden,
+                gamma.len(),
+                beta.len()
+            );
+        }
+        Ok(Self {
+            gamma: TValue::from(Tensor::from_shape(&[ch, 1, hidden], gamma)?),
+            beta: TValue::from(Tensor::from_shape(&[ch, 1, hidden], beta)?),
+        })
+    }
+    fn identity(ch: usize, hidden: usize) -> Result<Self> {
+        Self::from_slices(ch, hidden, &vec![1f32; ch * hidden], &vec![0f32; ch * hidden])
+    }
+}
+
 #[derive(Clone)]
 pub struct DfTract {
     enc: TractModel,
@@ -222,6 +258,9 @@
     m_zeros: Vec<f32>,    // Preallocated buffer for applying a zero mask
     rolling_spec_buf_y: VecDeque<Tensor>, // Enhanced stage 1 spec buf
     rolling_spec_buf_x: VecDeque<Tensor>, // Noisy spec buf
+    film_hidden: usize,
+    film_enc: Option<FilmInputs>, // Some only when enc.onnx declares gamma/beta
+    film_df: Option<FilmInputs>,  // Some only when df_dec.onnx declares gamma/beta
 }
 
 #[cfg(all(not(feature = "capi"), feature = "default-model"))]
@@ -241,8 +280,9 @@
         let model_cfg = config.section(Some("deepfilternet")).unwrap();
         let df_cfg = config.section(Some("df")).unwrap();
         let ch = rp.n_ch;
+        let film_hidden = model_cfg.get("emb_hidden_dim").unwrap().parse::<usize>()?;
 
-        let enc = init_encoder_from_read(&mut Cursor::new(dfp.enc), df_cfg, ch)?;
+        let enc = init_encoder_from_read(&mut Cursor::new(dfp.enc), df_cfg, ch, film_hidden)?;
         let erb_dec = init_erb_decoder_from_read(
             &mut Cursor::new(dfp.erb_dec),
             model_cfg,
@@ -250,8 +290,23 @@
             ch,
             Some(rp.reduce_mask.clone()),
         )?;
-        let df_dec =
-            init_df_decoder_from_read(&mut Cursor::new(dfp.df_dec), model_cfg, df_cfg, ch)?;
+        let df_dec = init_df_decoder_from_read(
+            &mut Cursor::new(dfp.df_dec),
+            model_cfg,
+            df_cfg,
+            ch,
+            film_hidden,
+        )?;
+        let film_enc = if enc.input_outlets()?.len() == 4 {
+            Some(FilmInputs::identity(ch, film_hidden)?)
+        } else {
+            None
+        };
+        let film_df = if df_dec.input_outlets()?.len() == 4 {
+            Some(FilmInputs::identity(ch, film_hidden)?)
+        } else {
+            None
+        };
         let enc = SimpleState::new(enc.into_runnable()?)?;
         let erb_dec = SimpleState::new(erb_dec.into_runnable()?)?;
         let df_dec = SimpleState::new(df_dec.into_runnable()?)?;
@@ -350,6 +405,9 @@
             rolling_spec_buf_y,
             rolling_spec_buf_x,
             df_states,
+            film_hidden,
+            film_enc,
+            film_df,
             post_filter: rp.post_filter,
             post_filter_beta: rp.post_filter_beta,
         };
@@ -391,6 +449,27 @@
         };
     }
 
+    /// Hidden size of the FiLM vectors, or `None` when no graph declares `gamma`/`beta`.
+    pub fn film_hidden(&self) -> Option<usize> {
+        (self.film_enc.is_some() || self.film_df.is_some()).then_some(self.film_hidden)
+    }
+
+    /// Replace the FiLM vectors fed on every frame. Does not reset the GRU states.
+    pub fn set_film(&mut self, v: &FilmVectors) -> Result<()> {
+        if self.film_enc.is_none() && self.film_df.is_none() {
+            bail!("model graphs do not declare gamma/beta inputs");
+        }
+        if self.film_enc.is_some() {
+            self.film_enc =
+                Some(FilmInputs::from_slices(self.ch, self.film_hidden, &v.gamma_enc, &v.beta_enc)?);
+        }
+        if self.film_df.is_some() {
+            self.film_df =
+                Some(FilmInputs::from_slices(self.ch, self.film_hidden, &v.gamma_df, &v.beta_df)?);
+        }
+        Ok(())
+    }
+
     pub fn init(&mut self) -> Result<()> {
         let ch = self.ch;
         let spec_shape = [ch, 1, 1, self.n_freqs, 2];
@@ -451,10 +530,15 @@
             );
         }
         // Run encoder
-        let mut enc_emb = self.enc.run(tvec!(
+        let mut enc_in = tvec!(
             self.erb_buf.clone(),
             TValue::from(self.cplx_buf.clone().into_tensor().permute_axes(&[0, 3, 1, 2])?)
-        ))?;
+        );
+        if let Some(film) = &self.film_enc {
+            enc_in.push(film.gamma.clone());
+            enc_in.push(film.beta.clone());
+        }
+        let mut enc_emb = self.enc.run(enc_in)?;
 
         let &lsnr = enc_emb.pop().unwrap().to_scalar::<f32>()?;
         let c0 = enc_emb.pop().unwrap();
@@ -489,7 +573,12 @@
         };
 
         let coefs = if apply_df {
-            let mut coefs = self.df_dec.run(tvec!(emb, c0))?.pop().unwrap().into_tensor();
+            let mut df_in = tvec!(emb, c0);
+            if let Some(film) = &self.film_df {
+                df_in.push(film.gamma.clone());
+                df_in.push(film.beta.clone());
+            }
+            let mut coefs = self.df_dec.run(df_in)?.pop().unwrap().into_tensor();
             coefs.set_shape(&[ch, self.nb_df, self.df_order, 2])?;
             Some(coefs)
         } else {
@@ -755,6 +844,7 @@
     mut m: InferenceModel,
     df_cfg: &ini::Properties,
     n_ch: usize,
+    film_hidden: usize,
 ) -> Result<TypedModel> {
     log::debug!("Start init encoder.");
     let s = m.symbol_table.sym("S");
@@ -769,11 +859,17 @@
         feat_erb.shape,
         feat_spec.shape,
     );
-    m = m
-        .with_input_fact(0, feat_erb)?
-        .with_input_fact(1, feat_spec)?
-        .with_input_names(["feat_erb", "feat_spec"])?
-        .with_output_names(["e0", "e1", "e2", "e3", "emb", "c0", "lsnr"])?;
+    let film = m.input_outlets()?.len() == 4;
+    m = m.with_input_fact(0, feat_erb)?.with_input_fact(1, feat_spec)?;
+    m = if film {
+        let g = InferenceFact::dt_shape(f32::datum_type(), shapefactoid!(n_ch, s, film_hidden));
+        m.with_input_fact(2, g.clone())?
+            .with_input_fact(3, g)?
+            .with_input_names(["feat_erb", "feat_spec", "gamma", "beta"])?
+    } else {
+        m.with_input_names(["feat_erb", "feat_spec"])?
+    };
+    m = m.with_output_names(["e0", "e1", "e2", "e3", "emb", "c0", "lsnr"])?;
 
     m.analyse(true)?;
     let mut m = m.into_typed()?;
@@ -784,18 +880,24 @@
     let m = pulsed.into_typed()?.into_optimized()?;
     Ok(m)
 }
-fn init_encoder(m: &Path, df_cfg: &ini::Properties, n_ch: usize) -> Result<TypedModel> {
+fn init_encoder(
+    m: &Path,
+    df_cfg: &ini::Properties,
+    n_ch: usize,
+    film_hidden: usize,
+) -> Result<TypedModel> {
     let m = tract_onnx::onnx().with_ignore_output_shapes(true).model_for_path(m)?;
-    init_encoder_impl(m, df_cfg, n_ch)
+    init_encoder_impl(m, df_cfg, n_ch, film_hidden)
 }
 
 fn init_encoder_from_read(
     m: &mut dyn Read,
     df_cfg: &ini::Properties,
     n_ch: usize,
+    film_hidden: usize,
 ) -> Result<TypedModel> {
     let m = tract_onnx::onnx().with_ignore_output_shapes(true).model_for_read(m)?;
-    init_encoder_impl(m, df_cfg, n_ch)
+    init_encoder_impl(m, df_cfg, n_ch, film_hidden)
 }
 
 fn init_erb_decoder_impl(
@@ -917,6 +1019,7 @@
     net_cfg: &ini::Properties,
     df_cfg: &ini::Properties,
     n_ch: usize,
+    film_hidden: usize,
 ) -> Result<TypedModel> {
     log::debug!("Start init DF decoder.");
     let s = m.symbol_table.sym("S");
@@ -937,11 +1040,17 @@
         emb.shape,
         c0.shape,
     );
-    m = m
-        .with_input_fact(0, emb)?
-        .with_input_fact(1, c0)?
-        .with_input_names(["emb", "c0"])?
-        .with_output_names(["coefs"])?;
+    let film = m.input_outlets()?.len() == 4;
+    m = m.with_input_fact(0, emb)?.with_input_fact(1, c0)?;
+    m = if film {
+        let g = InferenceFact::dt_shape(f32::datum_type(), shapefactoid!(n_ch, s, film_hidden));
+        m.with_input_fact(2, g.clone())?
+            .with_input_fact(3, g)?
+            .with_input_names(["emb", "c0", "gamma", "beta"])?
+    } else {
+        m.with_input_names(["emb", "c0"])?
+    };
+    m = m.with_output_names(["coefs"])?;
 
     m.analyse(true)?;
     let mut m = m.into_typed()?;
@@ -957,18 +1066,20 @@
     net_cfg: &ini::Properties,
     df_cfg: &ini::Properties,
     n_ch: usize,
+    film_hidden: usize,
 ) -> Result<TypedModel> {
     let m = tract_onnx::onnx().with_ignore_output_shapes(true).model_for_path(m)?;
-    init_df_decoder_impl(m, net_cfg, df_cfg, n_ch)
+    init_df_decoder_impl(m, net_cfg, df_cfg, n_ch, film_hidden)
 }
 fn init_df_decoder_from_read(
     m: &mut dyn Read,
     net_cfg: &ini::Properties,
     df_cfg: &ini::Properties,
     n_ch: usize,
+    film_hidden: usize,
 ) -> Result<TypedModel> {
     let m = tract_onnx::onnx().with_ignore_output_shapes(true).model_for_read(m)?;
-    init_df_decoder_impl(m, net_cfg, df_cfg, n_ch)
+    init_df_decoder_impl(m, net_cfg, df_cfg, n_ch, film_hidden)
 }
 
 fn calc_norm_alpha(sr: usize, hop_size: usize, tau: f32) -> f32 {
--- a/Cargo.toml
+++ b/Cargo.toml
@@ -1,5 +1,5 @@
 [package]
-name = "deep_filter"
+name = "deep_filter_fork"
 version = "0.5.6"
 authors = ["Hendrik Schröter"]
 edition = "2021"
@@ -127,7 +127,7 @@
 log = { version = "0.4", features = ["std"] }
 
 [package.metadata.capi.header]
-name = "deep_filter"
+name = "deep_filter_fork"
 subdirectory = "deep_filter"
 [package.metadata.capi.pkg_config]
 name = "libdeepfilter"
```

## 6. Custo (Q5)

Medida: harness `bench`, 20.000 frames de 480 amostras (200 s de áudio sintético) após 500 de aquecimento, 5 repetições intercaladas por variante, mediana entre repetições, fixado na CPU 2 (`taskset -c 2`), governor `performance`, build release, mono, 48 kHz, tract 0.19.16.

```
pristine             reps=5 mean=550654 p50=656086 p95=699163 p99=734860 max=3410764 ns  p50_vs_pristine=+0.0%
fork_stock           reps=5 mean=549845 p50=655009 p95=696467 p99=733085 max=3663933 ns  p50_vs_pristine=-0.2%
fork_film_none       reps=5 mean=551087 p50=657250 p95=697793 p99=738520 max=3020392 ns  p50_vs_pristine=+0.2%
fork_film_identity   reps=5 mean=550854 p50=656775 p95=698669 p99=739051 max=2755557 ns  p50_vs_pristine=+0.1%
Model name:                              Intel(R) Core(TM) Ultra 7 265H
```

Limite adotado antes de medir: p50 do fork com FiLM até +10% sobre o pristine e p99 < 7 ms. Resultado: **GO** (`fork_film_identity` p50 +0,1%; p99 739.051 ns = 0,74 ms; custo do patch sem FiLM no grafo: -0,2%, ou seja, ruído). O quadro tem 10 ms; o p50 de ~0,66 ms usa ~6,6% dele. Válido **só nesta máquina** (CPU, governor, `taskset`); **não** é medição fim a fim do produto (sem resampler, dispositivo de áudio ou engine), e o spec (seção 10) proíbe tratar isso como folga de latência. A máquina é híbrida e o pico (`max_ns`, 2,7 a 3,7 ms) mostra eventos esporádicos mesmo fixando o núcleo.

`lscpu | head -20` (T5, `evidence/lscpu.txt`):

```
Architecture:                            x86_64
CPU op-mode(s):                          32-bit, 64-bit
Address sizes:                           46 bits physical, 48 bits virtual
Byte Order:                              Little Endian
CPU(s):                                  16
On-line CPU(s) list:                     0-15
Vendor ID:                               GenuineIntel
Model name:                              Intel(R) Core(TM) Ultra 7 265H
CPU family:                              6
Model:                                   197
Thread(s) per core:                      1
Core(s) per socket:                      16
Socket(s):                               1
Stepping:                                2
CPU(s) scaling MHz:                      77%
CPU max MHz:                             5300,0000
CPU min MHz:                             400,0000
BogoMIPS:                                7372,80
Flags:                                   fpu vme de pse tsc msr pae mce cx8 apic sep mtrr pge mca cmov pat pse36 clflush dts acpi mmx fxsr sse sse2 ss ht tm pbe syscall nx pdpe1gb rdtscp lm constant_tsc art arch_perfmon pebs bts rep_good nopl xtopology nonstop_tsc cpuid aperfmperf tsc_known_freq pni pclmulqdq dtes64 monitor ds_cpl vmx smx est tm2 ssse3 sdbg fma cx16 xtpr pdcm pcid sse4_1 sse4_2 x2apic movbe popcnt tsc_deadline_timer aes xsave avx f16c rdrand lahf_lm abm 3dnowprefetch cpuid_fault epb ssbd ibrs ibpb stibp ibrs_enhanced tpr_shadow flexpriority ept vpid ept_ad fsgsbase tsc_adjust bmi1 avx2 smep bmi2 erms invpcid rdseed adx smap clflushopt clwb intel_pt sha_ni xsaveopt xsavec xgetbv1 xsaves split_lock_detect user_shstk avx_vnni lass lam wbnoinvd dtherm ida arat pln pts hwp hwp_notify hwp_act_window hwp_epp hwp_pkg_req hfi vnmi umip pku ospke waitpkg gfni vaes vpclmulqdq rdpid bus_lock_detect movdiri movdir64b fsrm md_clear serialize pconfig arch_lbr ibt flush_l1d arch_capabilities
Virtualization:                          VT-x
```

## 7. Cabeça de EQ no espectro (Q6)

Pontos de código (commit pinado, T6-code-points.txt):

```
454:        let mut enc_emb = self.enc.run(tvec!(
480:            let mut m = self.erb_dec.run(dec_input)?;
492:            let mut coefs = self.df_dec.run(tvec!(emb, c0))?.pop().unwrap().into_tensor();
503:    pub fn process(&mut self, noisy: ArrayView2<f32>, mut enh: ArrayViewMut2<f32>) -> Result<f32> {
594:        let mut spec_enh = as_arrayview_mut_complex(
601:        // Run post filter
610:        // Limit noise attenuation by mixing back some of the noisy signal
621:            state.synthesis(
682:    pub fn get_spec_enh(&self) -> ArrayView2<Complex32> {
690:    pub fn get_mut_spec_enh(&mut self) -> ArrayViewMut2<Complex32> {
## trecho final de process() (apos DF/post-filter/atten_lim, antes do iSTFT)

        // Limit noise attenuation by mixing back some of the noisy signal
        if let Some(lim) = self.atten_lim {
            spec_enh.map_inplace(|x| *x *= 1. - lim);
            spec_enh.scaled_add(lim.into(), &spec_noisy);
        }

        for (state, spec_ch, mut enh_out_ch) in izip!(
            self.df_states.iter_mut(),
            spec_enh.axis_iter(Axis(0)),
            enh.axis_iter_mut(Axis(0)),
        ) {
            state.synthesis(
                spec_ch.to_owned().as_slice_mut().unwrap(),
                enh_out_ch.as_slice_mut().unwrap(),
            );
        }
        Ok(lsnr)
## DFState: erb e apply_mask
53:    pub erb: Vec<usize>, // frequencies bandwidth (in bands) per ERB band
217:    pub fn apply_mask(&self, output: &mut [Complex32], gains: &[f32]) {
308:pub fn apply_interp_band_gain<T>(out: &mut [T], band_e: &[f32], erb_fb: &[usize])
```

O espectro final (`spec_enh`, declarado na linha 594) existe entre a linha 614 (fim do bloco `atten_lim`) e a 616 (`izip!` do `synthesis`), **dentro** de `process`; `get_mut_spec_enh` (690) só pode ser chamado depois de `process`, que já fez o iSTFT, então um gancho exige alterar `process`.

Resultado do gancho (T6-hook.txt):

```
eqhook mode=none bands=32 out=$SPIKE/eq-none.f32
eqhook mode=flat6 bands=32 out=$SPIKE/eq-flat6.f32
eqhook mode=top12 bands=32 out=$SPIKE/eq-top12.f32
## gancho vazio vs golden-pristine (a copia com o gancho nao pode mudar o upstream)
samples=480000 bit_exact=True max_abs_diff=0.000e+00 frames_with_diff=0 first_diff_frame=-1 last_diff_frame=-1
max_abs_diff_first_12_frames= ['0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00', '0.00e+00']
err_rms_dB_re_signal=-569.3
modo=flat6 rms_ratio_dB=+6.00 lag_amostras=0
  faixa 100-1000 Hz: delta_dB=+6.00
  faixa 1000-4000 Hz: delta_dB=+6.00
  faixa 4000-12000 Hz: delta_dB=+6.00
  faixa 12000-20000 Hz: delta_dB=+6.00
  faixa 20000-24000 Hz: delta_dB=+6.00
modo=top12 rms_ratio_dB=+0.00 lag_amostras=0
  faixa 100-1000 Hz: delta_dB=-0.00
  faixa 1000-4000 Hz: delta_dB=-0.00
  faixa 4000-12000 Hz: delta_dB=+2.72
  faixa 12000-20000 Hz: delta_dB=+12.00
  faixa 20000-24000 Hz: delta_dB=+12.00
```

Critérios do plano, escritos antes de medir: (a) `eq-none` vs `golden-pristine` bit-exato: **passou**; (b) `flat6` com `rms_ratio_dB` entre +5,6 e +6,4 e `lag_amostras=0`: **passou** (+6,00; 0); (c) `top12` com faixas até 4 kHz com |delta| <= 1,0, faixa 12-20 kHz entre +10 e +13 e lag 0: **passou** (0,00; 0,00; +12,00; lag 0). A faixa 4-12 kHz subiu +2,72 dB porque as 8 últimas bandas ERB começam dentro dela. Latência extra: **0 amostras**.
O que a cabeça precisaria receber (leitura de código, não medido): (1) `emb [ch, 1, 512]` por quadro sai do `enc` em `process_raw` (linhas 454-461 do original; `emb` é o terceiro valor retirado de `enc_emb`), disponível no mesmo `process_raw` onde uma terceira sessão `eq_head.run(tvec!(emb))` caberia; (2) o espectro do gancho corresponde ao quadro atrasado por `lookahead` (vem de `rolling_spec_buf_y.get_mut(df_order - 1)`, linhas 541-543 e 571 do original), então o treino precisa definir em qual atraso o `emb` condiciona o ganho (risco para o repo de treino); (3) o gancho roda por quadro de 480 amostras já em `process`: não adiciona latência, custa a multiplicação por 481 bins e, se houver terceira sessão ONNX, o custo dela (a medir na fase 4; não medido aqui).
Linhas adicionais do gancho: **+15 / -1** em `tract.rs` (`$SPIKE/evidence/T6-hook.diff`; o plano estimava ~12), em cópia separada (`libdf-eqhook`), não somada ao `libdf-fork`. Nenhum EQ real foi treinado ou calculado: os ganhos de teste são constantes por banda ERB. A prova é de **ponto de aplicação e latência**, não de qualidade.

## 8. Riscos

| Risco | Probabilidade | Impacto | Mitigação / quem decide |
|---|---|---|---|
| Fork contém `unsafe` (`tract.rs:294-298` `Tensor::uninitialized_dt`, `tract.rs:987-1049`, `capi.rs`, `augmentations.rs:71`) e o workspace tem `unsafe_code = "forbid"` | alta (já existe) | médio | vendorizar fora do workspace como crate com lints próprios, ou isolar atrás de uma crate-fronteira; decisão do dono, fase 4 |
| Patch do libDF deixa de ser pequeno (+127/-21 só para FiLM; +15 para o gancho) | média | médio | manter patch em arquivo `.diff` revisável e teste de paridade B/C/D como gate de CI; upstream do libDF pinado em 0.5.6 |
| Atraso do pulsifier muda os primeiros quadros do `enc` | baixa (não observado) | baixo | só medida por saída (G/C/D bit-exatos); recontar nós `Delay` se o ponto de FiLM mudar no treino |
| Troca de perfil exige recarregar modelo (rota B) | certa na rota B | alto na rota B | rota A evita (`set_film` sem reiniciar GRUs); testar o reinício de geração só se a rota B for escolhida |
| Rota B não passa no gate de assets (ONNX por usuário sem SHA-256 fixo) | certa | alto | rota B só vale para o spike |
| Asset de FiLM precisa de descritor no registro e assinatura do usuário | certa | médio | fase 4; o agente não produz assinaturas |
| Custo medido só numa máquina, mono, sem engine | certa | médio | medir fim a fim na fase 0 |
| Equivalência bit-exata depende da versão do tract/CPU (kernels) | baixa | médio | manter tract fixado em `=0.19.16`; o replay do golden deu 1,35e-7 (não zero) por seleção de kernels |
| FiLM por ponto de inserção editado pós-exportação (`Relu`) pode não existir no modelo treinado de verdade | média | alto | requisito 5 ao repo de treino: exportar o DFNet3 com FiLM em identidade |

## 9. Recomendação para a fase 4

1. **Patch no libDF é viável**: paridade bit-exata em identidade, custo ~0, `set_film` troca perfil sem recarregar nem reiniciar o estado das GRUs, contrato `960/480/32/96/5/2` inalterado. Recomenda-se seguir pela **rota A**.
2. O **contorno de constantes embutidas (rota B) só vale para o spike**, para provar a paridade sem patch e isolar o efeito do alinhamento de atraso. Não serve para produção: um ONNX por usuário não teria SHA-256 fixo e não passaria no gate de assets (SHA-256, tamanho, assinatura Ed25519, allowlist de membros). Na rota A, os ONNX de `enc`/`df_dec` com FiLM são **assets fixos** (um único SHA-256 aprovado) e a variação por usuário vai em `gamma/beta` de **runtime**, saídas do ONNX de enrollment (requisito 3 do spec).
3. O fork deve ser **vendorizado no repositório** (build offline) com os arquivos modificados marcados, `LICENSE-MIT`/`LICENSE-APACHE` e o aviso de copyright mantidos, e com `unsafe` tratado conforme a política (risco da seção 8).
4. Incorporar o gancho de espectro (Q6) ao mesmo fork só se a variante "cabeça de EQ dentro do pDFNet3" for a escolhida; ela precisa que o repo de treino defina o atraso de `emb`.
5. Gate de CI sugerido: replay do `frozen-reference.json` no build fork (stock e FiLM identidade) e os casos B, C, D deste spike.

## 10. Não verificado

- Contagem de nós `Delay` do grafo pulsado (Step 7 do plano não foi necessário); a hipótese do atraso foi avaliada só por equivalência de saída.
- Qualquer coisa fora do sinal sintético mono de 1000 frames e do `frozen-reference.json`: nenhum áudio real, nenhum multicanal (`ch > 1`), nenhuma taxa diferente de 48 kHz.
- Custo fim a fim do produto (engine, dispositivo, resampler), outras máquinas, outros núcleos, carga do sistema; o custo de uma terceira sessão ONNX (cabeça de EQ neural) e do ONNX de enrollment.
- Qualidade perceptiva do FiLM ou do EQ (nenhum modelo treinado existe; os ONNX com FiLM são editados do asset aprovado; o ponto de FiLM no modelo treinado de verdade pode ser outro).
- Se o fork passa nas lints do workspace Clearcore (`unwrap/expect/panic = deny`, `unsafe_code = "forbid"`): o fork nunca foi inserido no workspace.
- `~/.cargo`: ao final, tem `.global-cache` e `.package-cache-mutate` com data de 2026-10-01 23:56:03, **antes** do primeiro comando cargo desta execução (o plano esperava só `.package-cache`, `git`, `registry`). Não foi feito por este spike (todos os comandos cargo usaram `CARGO_HOME` em `scratchpad/rust-env`); provavelmente veio da instalação da toolchain pelo orquestrador. Não foi alterado nem removido.
- A licença foi lida, não passou por parecer jurídico.

## 11. Checkpoint

```
?? docs/superpowers/plans/2026-10-01-clearcore-studio-phase2-settings-ui.md
?? docs/superpowers/plans/2026-10-01-clearcore-studio-phase3-libdf-spike-report.md
?? docs/superpowers/plans/2026-10-01-clearcore-studio-phase3-libdf-spike.md
?? docs/superpowers/specs/2026-10-01-clearcore-studio-pipeline-design.md

$ git diff --stat
$ ls -A ~/.cargo
git
.global-cache
.package-cache
.package-cache-mutate
registry
```

Limpeza: `cargo clean` em `$SPIKE/target` removeu 1451 arquivos, 643,9 MiB (`du` mostrava 591M) do tmpfs; `/tmp` passou de 8,4G para 7,8G usados. O restante do spike (fonte, venv, assets, binários em `$SPIKE/bin`, evidências) ocupa 402M e foi mantido.
