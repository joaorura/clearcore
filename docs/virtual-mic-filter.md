# Filtro do microfone virtual (`libclearcore_filter`)

O áudio do microfone virtual é processado pelo `crates/filter-capi` (`libclearcore_filter.so`),
carregado por `dlopen` pelo helper PipeWire (`platform/linux/helper`). Este documento descreve o
modelo que o filtro roda, como escolhe a runtime (ONNX Runtime ou OpenVINO), como consome o perfil de
voz e o que expõe para diagnóstico.

## Modelo (`crates/filter-capi/src/select.rs`)

Resolução fail-closed, sem modelo base:

1. `CLEARCORE_DEV_PDFNET3_V3_ASSET` + `CLEARCORE_DEV_PDFNET3_V3_SHA256` (os dois ou nenhum): arquivo
   pDFNet3 Pro v3 FULL de desenvolvimento;
2. `CLEARCORE_DEV_PDFNET3_STATEFUL_ASSET` + `_SHA256`: arquivo v2 stateful (como antes);
3. `<models>/pdfnet3/pdfnet3-pro-v3-full-stateful-asset-v1.tar.gz`, o único modelo embutido, SHA-256
   do arquivo inteiro fixado em `PDFNET3_PRO_V3_FULL_SHA256` (`crates/model/src/pdfnet3_bundle.rs`,
   reexportado como `V3_BUNDLED_SHA256`). Ausente ou divergente = sem filtro.

O Pro v2 embutido (`pdfnet3-stateful-asset-v1.tar.gz`) e o `CLEARCORE_FILTER_MODEL=v2` foram removidos;
o v2 só entra por override de desenvolvimento (passo 2).

### Contrato do asset v3 (manifest)

`.tar.gz` com `manifest.json`, os grafos listados em `files` (SHA-256 e tamanho de cada um) e
`config.ini`; qualquer outro membro, membro duplicado ou não regular é recusado. O SHA-256 do arquivo
inteiro é conferido antes de descomprimir.

- `graphs.<nome>`: `file`, `inputs`/`outputs` (`{nome: {shape}}`) e `state_pairs`
  (`[{in, out, shape, init: "zeros"}]`), todos com `S = 1` (dimensão dinâmica fixada em 1);
- `routing.<grafo>.<entrada>`: `"<grafo>.<saída>"` ou `"runtime"`; os grafos rodam em ordem de
  dependência (no asset real: `enc`, depois `erb_dec` e `df_dec`);
- entradas de runtime: `feat_erb` `[1,1,1,32]`, `feat_spec` `[1,2,1,96]`, `speaker_embedding`
  `[1,192]` (zeros sem perfil) e `speaker_present` `[1]` (1,0 / 0,0);
- saídas obrigatórias (em qualquer grafo): `lsnr`, `m` (32) e `coefs` (`[1,1,96,10]` lido como 96 bins
  × 5 taps × re/im, o layout de hoje);
- `model == "pdfnet3-pro-v3-full"`, `hop == 480`, `sample_rate == 48000`.

Entrada que não seja de runtime, roteada ou lado `in` de um par de estado faz o carregamento falhar.
Os estados são realimentados por nome e só são confirmados quando todos os grafos do quadro terminaram
sem erro.

## Execução por quadro (`core.rs`, `engine.rs`)

- **Todos os grafos rodam em todo quadro** e todos os estados avançam (decisão de 2026-11-09, fiel ao
  treino). O `lsnr` só seleciona a saída do quadro, exatamente como `emulate_gating_offline`
  (`clearcore-train/src/cctrain/eval/runtime_path.py:342-359`, limiares `(-10, 30, 20)` da l.36):
  `lsnr < -10` → zero; `lsnr > 30` → espectro de entrada sem processar; `20 < lsnr <= 30` → só máscara
  ERB; senão máscara ERB + deep filter. É o `DspPipeline::apply_stages`/`step_synthesis` do
  `crates/model` (mesmo código do daemon); os quatro regimes têm teste em `core.rs`.
- **Silêncio digital**: RMS do quadro `< 1e-7` → saída zero sem avançar STFT nem modelo.
- **EQ do perfil**: 32 ganhos ERB (dB) interpolados por bin (`spectral_eq::bin_factors`) e aplicados ao
  espectro realçado logo antes da iSTFT (`DspPipeline::set_spectral_eq_factors`), sem latência extra.
- O Pro v2 também roda nesse caminho: o filtro tem uma réplica do `OrtSpectralModel` do
  `crates/model` (`legacy.rs`, idêntica bit a bit ao `OrtBackend` sem perfil, testado) para poder
  aplicar a EQ, que o `OrtBackend::set_voice_profile` ignora.

## Runtime (`select.rs`, `engine.rs`)

O `backend` do `settings.json` (o que a UI grava), sobreposto por `CLEARCORE_FILTER_BACKEND`:

| Valor | Dispositivos tentados |
| :--- | :--- |
| `openvino-npu`, `npu`, `intel-npu`, `openvino` | OpenVINO NPU, GPU, CPU |
| `openvino-gpu`, `intel-gpu`, `arc`, `gpu` | OpenVINO GPU, CPU |
| `openvino-cpu`, `intel-cpu` | OpenVINO CPU |
| outro ou ausente (`auto`, `tract`, `cpu`, `ort`) | ONNX Runtime CPU |

O OpenVINO é consumido pelo `crates/runtime-openvino` sem alterá-lo (mesma busca da
`libopenvino_c`, `OPENVINO_LIB_PATH` primeiro; FP16 em NPU/GPU, FP32 na CPU x86; todas as dimensões
estáticas). Cada dispositivo precisa compilar e passar na qualificação (10 quadros de aquecimento,
30 medidos, o mais lento abaixo de 90 % do orçamento do hop de 10 ms). O primeiro que passa roda com o
ONNX Runtime CPU compilado como reserva quente; se nenhum passa, roda o ONNX Runtime e o motivo vai
para o status.

Troca para a reserva durante a sessão:

- erro de inferência: o quadro é refeito na reserva (sem perder áudio);
- latência: um quadro acima de 3× o orçamento (depois dos 20 primeiros) ou 5 quadros acima do
  orçamento numa janela de 500. `CLEARCORE_FILTER_HOP_BUDGET_US` muda o orçamento (testes).

No v3 o estado recorrente passa para a reserva (troca sem descontinuidade, testado); no v2 em
OpenVINO (`Dfn3Session`, que não exporta estado) a reserva começa zerada.

## Perfil de voz

O filtro lê o mesmo `active_profile.json` (`ProfileStore::default_dir()`, modo 0600, arquivo regular)
que o enrollment grava. Formato v2, definido pelo enrollment (`crates/model/src/voice_profile_v2.rs`
na branch `feat/pro-v3-enroll`, `docs/ipc-v1.md` lá):

```json
{ "version": 2, "id": "p-1", "name": "Voz", "created_at_utc": "2026-11-09T12:00:00Z",
  "model_id": "pdfnet3-pro-v3-full", "model_version": 3, "embedding_dim": 192,
  "speaker_embedding_b64": "<192 f32 little-endian em base64 padrão, com padding>",
  "eq": { "gains_db": [32 valores, -6..12 dB] } | null,
  "integrity_hash": "<sha256 hex de serde_json::to_vec dos campos acima, nesta ordem>" }
```

O parser e o payload canônico estão replicados em `crates/filter-capi/src/profile_v2.rs` (este branch
não tem o módulo do enrollment); o teste `the_canonical_payload_is_the_documented_field_order` fixa os
bytes. Validação: versão, `model_id`/`model_version`, `embedding_dim == 192`, embedding finito com
norma L2 = 1 ± 1e-3, EQ na faixa, hash.

| Perfil | Modelo v3 | Modelo v2 (FiLM) |
| :--- | :--- | :--- |
| v2 | `speaker_embedding` + `speaker_present = 1` + EQ | só EQ |
| v1 | desatualizado: voz neutra, sem EQ, filtro segue (`reload` → 0, `set` → -3) | FiLM + EQ (como antes) |
| nenhum / isolamento desligado | zeros + `speaker_present = 0`, sem EQ | identidade |

`voice_isolation_intensity = 0` desliga o locutor (`speaker_present = 0`); qualquer outro valor usa o
embedding como gravado (o modelo foi treinado com `speaker_present` em {0, 1}; não há interpolação).

## C ABI

Inalterada para o helper (`create`, `process`, `free`, `set_preset`, `set_leveler_intensity`,
`set_voice_profile`, `clear_voice_profile`, `reload_active_profile`, `supports_voice_profile`,
`is_voice_profile_active`). Nova e opcional: `clearcore_filter_status_json(filter, buf, len)`, JSON com
`model`, `backend` (`ort:CPU`, `openvino:NPU`...), `fallback_reason`, `standby`, `profile_id`,
`speaker_conditioning`, `eq`, `silent_hops` e `latency_us` (`hops`, `p50`, `p99`, `max`). O helper a
resolve por `dlsym` e registra o status na partida e a cada recarga de perfil.

## Medições (2026-11-09)

Intel Core Ultra 7 265H, OpenVINO 2026.4.1 do pip, asset real v3 (`c1bee559…56ee`), quadro completo
(STFT + 3 grafos + iSTFT + cadeia de estúdio), 1000 quadros após 50 de aquecimento, máquina com outros
builds rodando:

| Runtime | p50 | p99 | max |
| :--- | :--- | :--- | :--- |
| ORT CPU (`dlopen`, máquina quieta, 500 quadros) | 0,40 ms | 0,47 ms | 1,2 ms |
| ORT CPU (bench, máquina carregada) | 0,86–0,90 ms | 4,1–24 ms | 8–48 ms |
| OpenVINO CPU | 0,91–0,99 ms | 1,4–1,8 ms | 3,1–4,1 ms |
| OpenVINO GPU | 1,0 ms | 2,8–3,2 ms | 4,3–4,8 ms |
| OpenVINO NPU | 2,0–2,2 ms | 3,2–3,3 ms | 3,7–4,4 ms |

Concordância com o ONNX Runtime no mesmo asset (saídas brutas `lsnr`/`m`/`coefs`, 400 quadros):
OpenVINO CPU 132–138 dB, GPU 126–135 dB, NPU 56–69 dB (FP16). No áudio, sem perfil, a NPU fica em
26 dB: o FP16 desloca o `lsnr` perto dos limiares e alguns quadros mudam de regime.

## Testes

```text
OPENVINO_LIB_PATH=<.../openvino/libs> cargo test --release -p realtime-noise-filter-capi
# latência e concordância entre runtimes (asset real opcional):
CLEARCORE_V3_BENCH_ASSET=<tar.gz> CLEARCORE_V3_BENCH_SHA256=<sha> OPENVINO_LIB_PATH=... \
  cargo test --release -p realtime-noise-filter-capi -- --ignored --nocapture bench runtimes_agree
# paridade contra o daemon (manifest.json, input.f32, expected_daemon_{neutral,profile}.f32,
# conditioning.json; asset em ../pro-v3-full/ ou CLEARCORE_V3_BENCH_ASSET/_SHA256):
CLEARCORE_FILTER_PARITY_DIR=<dir> OPENVINO_LIB_PATH=... \
  cargo test --release -p realtime-noise-filter-capi -- --ignored --nocapture parity
```

Paridade contra o daemon (tract, executor stateful genérico), 600 hops de `mix_0002`, hops 0–20 e
300–320 zerados, embedding ECAPA real + EQ em rampa −3..+3 dB, arquivo inteiro:

| Runtime do filtro | neutro | com perfil |
| :--- | :--- | :--- |
| ORT CPU | 131,4 dB (max \|erro\| 2,5e-7) | 129,8 dB (3,3e-7) |
| OpenVINO CPU | 130,4 dB (2,7e-7) | 125,5 dB (4,8e-7) |

O fixture `crates/filter-capi/tests/fixtures/pdfnet3-pro-v3-full-synthetic.tar.gz` (158 KB, gerado por
`make_synth_v3.py` no mesmo diretório) tem o formato e os nomes do asset real com pesos aleatórios.
