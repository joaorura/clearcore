# pDFNet3 Pro (modelo de isolamento embutido)

Único modelo de isolamento distribuído nos pacotes do Clearcore. É o DeepFilterNet3 com condicionamento
FiLM (`gamma`/`beta`) por perfil de voz, treinado no repositório `clearcore-train`. O DFNet3 base
(`models/stateful/`, `vendor/approved/`) não é mais distribuído e não há fallback para ele: se um destes
arquivos faltar ou o SHA-256 não bater, o daemon não carrega modelo nenhum e o `GetStatus` informa
`isolation_model: "none"` com `isolation_model_error` (`MODEL_*`), e o filtro do microfone virtual não é criado.

| Arquivo | Bytes | SHA-256 (arquivo inteiro) | Quem usa |
|---|---:|---|---|
| `pdfnet3-release-asset-v1.tar.gz` | 7978093 | `1cfa86c75e44e8ac45ace8a905bb3368c143d72e0d6b963fb8c2a3a3a1e782c8` | daemon (`realtime-noise-service`, tract): backend que recebe o perfil e denoiser neutro das amostras de cadastro |
| `pdfnet3-stateful-asset-v1.tar.gz` | 7974311 | `d1b9c2fd49ba602ed48cd503d256885c5e174a869d23b62204e8d668de95d6a5` | `libclearcore_filter` (microfone virtual, ONNX Runtime, estados recorrentes como portas do grafo) |

Os dois hashes estão fixados em `crates/model/src/pdfnet3_bundle.rs` e conferidos também por
`crates/app-tauri/scripts/package-app.mjs` (o build do pacote falha se não baterem) e pelo workflow de release.
Cada arquivo contém exatamente `enc.onnx`, `erb_dec.onnx`, `df_dec.onnx` e `config.ini` na raiz.

## Origem

- Run `clearcore-train/runs/m3_deploy_pro_v3_20261013` (2026-10-13), checkpoint
  `m2/pro-v2-mhca-k16-tse/checkpoints/ckpt-00002000.pt` (SHA-256 `fb59c785…6cb1`), derivado do asset
  aprovado `df-compatible-release-asset-v1` (pesos DeepFilterNet3; registro em
  `governance/model-assets/df-compatible-release-asset-v1/`).
- Gates do `m3_report.json` da run: G0 PASS (paridade tract × stateful, perfis neutro/aleatório/extremos),
  G11 PASS; exportação reprodutível (duas exportações com o mesmo SHA-256).
- O modelo de cadastro `models/enrollment/enrollment.onnx` (SHA-256 `2e825fec…40f2`) é o da mesma run, par
  obrigatório deste modelo: os vetores FiLM que ele produz são os que este pDFNet3 espera.

## Busca em tempo de execução

`<dir>/pdfnet3/<arquivo>` sobre: `CLEARCORE_MODELS_DIR`; `models/` e `resources/models/` no diretório atual,
no diretório do executável e nos ancestrais; `$XDG_DATA_HOME/clearcore/models` (ou `~/.local/share/...`);
`/opt/clearcore/models`, `/opt/clearcore/resources/models`, `/usr/share/clearcore/models`,
`/usr/local/share/clearcore/models`. O primeiro arquivo encontrado é o usado (um arquivo adulterado não faz
a busca continuar).

Overrides de desenvolvimento (opcionais, os dois juntos): `CLEARCORE_DEV_PDFNET3_ASSET`/`_SHA256` (daemon,
export tract) e `CLEARCORE_DEV_PDFNET3_STATEFUL_ASSET`/`_SHA256` (filtro, export stateful).

## Aceleradores

O daemon roda o pDFNet3 Pro no **OpenVINO** (NPU, GPU ou CPU Intel) quando um backend OpenVINO é
pedido nas configurações (`openvino-npu`, `openvino-gpu`, `openvino-cpu`, `openvino`): ele compila o
export **stateful** (o mesmo do filtro), fixa as dimensões em `S = 1` (a NPU exige shapes estáticos),
alimenta `gamma`/`beta` (identidade `1`/`0` sem perfil, os vetores do perfil quando há um) e aplica a
EQ do perfil antes da iSTFT, como o tract. Qualquer falha (runtime ausente, plugin ou dispositivo
faltando, compilação, prazo de tempo real) volta para o tract com o motivo em
`GetStatus.backend_fallback_reason`. O `auto` continua no tract (ver `docs/openvino.md`). O microfone
virtual (`libclearcore_filter`) continua no ONNX Runtime na CPU.

## Licença

Pesos derivados do DeepFilterNet3 (`MIT OR Apache-2.0`, copyright (c) 2021 Hendrik Schröter); a atribuição ao
DeepFilterNet continua obrigatória.
