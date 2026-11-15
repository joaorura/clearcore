# pDFNet3 Pro v3 completo e Pro v7 (modelos de isolamento embutidos)

Sem perfil de voz (ou com intensidade 0) roda o Pro v3 completo em modo neutro; com um perfil v7
(embedding + tokens do `enroll_tokens`) roda o Pro v7. Um perfil antigo, sem os tokens do v7, é
recusado com erro no log (voz neutra) e precisa de um novo cadastro.

O modelo de isolamento sem perfil é o pDFNet3 Pro v3 **completo** (memória de
locutor + cross-attention), condicionado por `speaker_embedding` [1,192] (ECAPA, L2) e `speaker_present` [1]
(binário: 0 com intensidade 0 ou sem perfil, 1 caso contrário). O DFNet3 base (`models/stateful/`,
`vendor/approved/`) não é distribuído e não há fallback para ele: se o arquivo faltar ou o SHA-256 não bater,
o daemon não carrega modelo nenhum e o `GetStatus` informa `isolation_model: "none"` com
`isolation_model_error` (`MODEL_*`), e o filtro do microfone virtual não é criado.

| Arquivo | Bytes | SHA-256 (arquivo inteiro) | Quem usa |
|---|---:|---|---|
| `pdfnet3-pro-v3-full-stateful-asset-v1.tar.gz` | 12375900 | `c1bee5593870bc326913e805803dbcfb19632236f0f42163635a5cc502d256ee` | UM export stateful com `manifest.json` (`enc`/`erb_dec`/`df_dec`, opset 12, `S = 1`) para todos os runtimes: daemon (tract ou OpenVINO, executor genérico, `pdfnet3_bundle::load_isolation_archive`) e `libclearcore_filter` (ONNX Runtime ou OpenVINO) |
| `pdfnet3-pro-v7-a2-300k-stateful-asset-v1.tar.gz` | 14175123 | `b0f125d71d8764e3b73e91b667b2cb7aba1ef1e4b6987c53e9019958f5369608` | pDFNet3 Pro **v7** A2-300k com gate de silêncio C: o modelo do **modo isolamento** quando há perfil de voz v7 (intensidade > 0). Seis grafos (`enroll_tokens`, `enc_conv`, `detector`, `enc_post`, `erb_dec`, `df_dec`) + FIR 48→16 kHz (`.f32`); contrato em `docs/pdfnet3-pro-v7-stream.md`. Daemon: tract (`pdfnet3_v7::V7IsolationBackend`); filtro: ONNX Runtime, 1 thread. Latência total 240 ms |

O hash do arquivo e os de cada grafo estão fixados em `crates/model/src/pdfnet3_bundle.rs` e conferidos também
por `crates/app-tauri/scripts/package-app.mjs` (o build do pacote falha se não baterem) e pelo workflow de
release. Os exports antigos do Pro v2 com FiLM (`pdfnet3-release-asset-v1.tar.gz`, `1cfa86c7…`, e
`pdfnet3-stateful-asset-v1.tar.gz`, `d1b9c2fd…`) foram removidos.

## Origem

- Run `clearcore-train/runs/m3_deploy_pro_v3_full` (entregue em 2026-11-09); paridade ORT/OpenVINO/tract
  ≥ 107,6 dB contra o PyTorch.
- O modelo de cadastro `models/enrollment/enrollment.onnx` (enrollment v3, SHA-256 `82a292b9…c1`) é o par
  obrigatório: o embedding que ele produz é o que este modelo espera.

## Busca em tempo de execução

`<dir>/pdfnet3/<arquivo>` sobre: `CLEARCORE_MODELS_DIR`; `models/` e `resources/models/` no diretório atual,
no diretório do executável e nos ancestrais; `$XDG_DATA_HOME/clearcore/models` (ou `~/.local/share/...`);
`/opt/clearcore/models`, `/opt/clearcore/resources/models`, `/usr/share/clearcore/models`,
`/usr/local/share/clearcore/models`. O primeiro arquivo encontrado é o usado (um arquivo adulterado não faz
a busca continuar).

Overrides de desenvolvimento (opcionais, os dois juntos): `CLEARCORE_DEV_PDFNET3_ASSET`/`_SHA256` (daemon)
e `CLEARCORE_DEV_PDFNET3_V3_ASSET`/`_SHA256` ou `CLEARCORE_DEV_PDFNET3_STATEFUL_ASSET`/`_SHA256` (filtro).

## Aceleradores

O daemon roda o mesmo arquivo no **OpenVINO** (NPU, GPU ou CPU Intel) quando um backend OpenVINO é pedido
(`openvino-npu`, `openvino-gpu`, `openvino-cpu`, `openvino`), com as portas descobertas por nome e as
dimensões fixadas em 1; qualquer falha volta para o tract com o motivo em `GetStatus.backend_fallback_reason`.
O `auto` fica no tract (ver `docs/openvino.md`). O filtro do microfone virtual segue o `backend` do
`settings.json` (ver `docs/virtual-mic-filter.md`).

## Licença

Pesos derivados do DeepFilterNet3 (`MIT OR Apache-2.0`, copyright (c) 2021 Hendrik Schröter); a atribuição ao
DeepFilterNet continua obrigatória.
