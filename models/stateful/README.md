# Grafos ONNX stateful do DeepFilterNet3

Entrada dos backends de aceleração (OpenVINO hoje; TensorRT, CoreML, DirectML e Vulkan planejados).
Os três grafos são o DeepFilterNet3 do asset aprovado com os estados recorrentes e os anéis de atraso
das convoluções temporais expostos como entradas e saídas do grafo (`h_in`/`h_out`, `feat_erb_buf`,
`feat_spec_buf`, `c0_buf`), para que cada quadro possa ser processado com S=1 sem reprocessar o passado.

**Não são assets aprovados.** Eles não passam pelo gate de assinatura de `crates/model`
(`ModelAssetRegistry`) e não devem ser distribuídos nem carregados em produto até existir o registro
descrito em "Governança" abaixo. Quem consome (`crates/runtime-openvino`, `crates/accelerators`) lê a
pasta diretamente, mas confere o SHA-256 dos três `.onnx` contra `APPROVED_STATEFUL_DIGESTS`
(`crates/accelerators/src/openvino.rs`) e recusa o carregamento se algum não bater (o `config.ini` não entra nessa conferência). Isso protege contra
adulteração dos arquivos, não substitui a aprovação: o conjunto de digests é fixado no código, sem
assinatura nem registro de governança.

| Arquivo | Bytes | SHA-256 |
|---|---:|---|
| `enc.onnx` | 1956817 | `aead48ee9a7995785780841a1346fe3e43404d55ce1e30417966be192eb5841e` |
| `erb_dec.onnx` | 3292490 | `346266ad7ff31adfe472c669558ffd8ca4bd362c6514405c429d790bb77c46d1` |
| `df_dec.onnx` | 3343468 | `1d95ed8864eed3c1d5870f7602881bb51a2a43a75abb34342ff6806197e09209` |
| `config.ini` | 2067 | `415eb925d44990d938fb739f514aa3662c1ec0ea836cff044fa1291b82cb4290` (cópia byte a byte do `config.ini` do asset aprovado) |

Conferir: `sha256sum models/stateful/*` (os três `.onnx` e o `config.ini` devem bater com a tabela; a dos `.onnx` espelha `APPROVED_STATEFUL_DIGESTS`; ao regenerar os arquivos, atualize as duas).

## Origem dos pesos

Os pesos são os do DeepFilterNet3 (`DeepFilterNet3_onnx.tar.gz`, tag `v0.5.6` do repositório
`Rikorose/DeepFilterNet`), exatamente o asset base `df-compatible-release-asset-v1`, em
`vendor/approved/df-compatible-release-asset-v1.bin` (SHA-256
`c94d91f70911001c946e0fabb4aa9adc37045f45a03b56008cb0c8244cb63616`, 7 983 136 bytes). Os originais,
sem alteração, são:

| Arquivo original (`tmp/export/`) | Bytes | SHA-256 |
|---|---:|---|
| `enc.onnx` | 1954042 | `7c5399d3da8a50ebef1c1a0ae421b33376aa5e45d0e92df16da7e83c9c131916` |
| `erb_dec.onnx` | 3292397 | `ab669a1d10afe20911728b33053a452071042317a90581092b325da7b2f9d895` |
| `df_dec.onnx` | 3340803 | `23114ce3b0f6464b763ee62f7bb8aab6b2a129a21eabd5bcfe59413db05f278a` |

Registro de origem e licença: `governance/model-assets/df-compatible-release-asset-v1/` e o relatório
`docs/superpowers/plans/2026-10-01-clearcore-studio-phase3-libdf-spike-report.md` (seção que cita o registro
de terceiros).

## Como foram gerados

Cirurgia de grafo em `tools/accelerators/make_stateful_onnx.py` (usa `onnx` e `numpy`; a verificação de
paridade usa também `openvino`). A partir da raiz do repositório:

```
mkdir -p /tmp/clearcore-dfnet3-export
tar -xzf vendor/approved/df-compatible-release-asset-v1.bin -C /tmp/clearcore-dfnet3-export
python3 tools/accelerators/make_stateful_onnx.py \
    --input-dir /tmp/clearcore-dfnet3-export/tmp/export \
    --output-dir models/stateful
```

O script reescreve os 5 nós GRU (1 em `enc`, 2 em `erb_dec`, 2 em `df_dec`) para expor `h_in`/`h_out`,
troca o zero-padding causal das convoluções temporais por anéis de atraso (2 quadros para `feat_erb` e
`feat_spec` no `enc`, 4 quadros para `c0` no `df_dec`) e copia o `config.ini`. Em seguida roda a
verificação de paridade S=1 contínuo contra S=8 em lote (erro máximo < 1e-4 por saída); o teste
automatizado equivalente é `python3 tools/accelerators/test_make_stateful_onnx.py`.

Limite de reprodutibilidade: a saída de `onnx.save` pode variar de byte com a versão do `onnx`, e a
versão usada para gerar estes arquivos não ficou registrada no repositório. Um novo `make_stateful_onnx.py`
pode portanto produzir SHA-256 diferentes com o mesmo grafo. O que vale como identidade dos arquivos
commitados é a tabela acima, não a regeneração. Não foi possível regenerar e comparar aqui (o `onnx` não
está instalado neste ambiente).

## Licença

Pesos e código de origem: `MIT OR Apache-2.0`, copyright (c) 2021 Hendrik Schröter, por decisão do dono do
projeto registrada em `governance/model-assets/df-compatible-release-asset-v1/` (decisão do dono, não
parecer jurídico externo). A cirurgia de grafo (`tools/accelerators/make_stateful_onnx.py`) é código
original deste repositório, sob a licença de código do repositório (PolyForm Noncommercial 1.0.0; até `v0.1.0-beta.2` era Apache-2.0). Os grafos stateful são obra
derivada dos pesos e herdam os termos dos pesos; a atribuição ao DeepFilterNet continua obrigatória.

## Governança: o que falta para virar asset de produto

`governance/model-assets/` só tem registro para `df-compatible-release-asset-v1`, e os termos dele dizem
"Redistributed byte-for-byte ... no model conversion or quantization has occurred" (`conversion_terms`).
Estes grafos **são** uma conversão, então a aprovação existente não os cobre. Para distribuí-los ou
passá-los por `ModelAssetRegistry` faltaria:

1. Um novo `asset_id` (por exemplo `df-stateful-release-asset-v1`) registrado em
   `crates/model/src/model_registry.rs` com `role`, `sha256` e `size_bytes` do arquivo `.tar.gz` empacotado
   e `allowed_members` (hoje as entradas de papéis não-base têm `sha256` vazio e `size_bytes: 0`).
2. Um pacote do asset (tar.gz com os quatro membros), com o SHA-256 próprio.
3. Uma pasta `governance/model-assets/<asset_id>/` com os quatro arquivos que a do asset base tem:
   `candidate-provenance.json`, `legal-review.json`, `approval-manifest.json` e `approver-public-key.pem`
   (formatos em `governance/model-assets/schemas/` e `templates/`).
4. `conversion_terms` novos nesses registros, descrevendo a cirurgia de grafo (ferramenta, versão do
   `onnx`, comando acima) e o SHA-256 do asset base de origem, em vez da frase "byte-for-byte".
5. Nova revisão do dono e novo `legal_approval_id`, e a assinatura Ed25519 do `approval-manifest.json` por
   uma chave listada em `governance/model-assets/trust-policy.json`. Essa assinatura não pode ser
   gerada ou simulada fora do fluxo de aprovação; o que existe hoje assina apenas o asset base.
6. Registrar a versão do `onnx` (e do Python) usada na geração, para tornar a saída reproduzível.

## Engines compilados TensorRT (`models/stateful/tensorrt/`)

Para execução acelerada em GPUs NVIDIA via runtime TensorRT (`crates/runtime-tensorrt`), a pasta `models/stateful/tensorrt/` contém os planos de inferência compilados a partir dos grafos ONNX stateful (`enc.onnx`, `erb_dec.onnx`, `df_dec.onnx`):

| Arquivo | Bytes | SHA-256 |
|---|---:|---|
| `tensorrt/enc.engine` | 2232860 | `fa57ea4ac0f97743b6219f31d941c0863d51683454960a004cb8b3c98cd13364` |
| `tensorrt/erb_dec.engine` | 3574596 | `8d40934f996293212df4f49b834c50b9697f840dd57be5da9ccfca9c05bda13a` |
| `tensorrt/df_dec.engine` | 3522388 | `63fdaf590fa619f399f953aa83779644c368f038a66be8d8c74263a2cae1361c` |

## Empacotamento e Distribuição Multi-Runtime

Nas pipelines de release (`.github/workflows/release.yml`) e empacotamento desktop (`crates/app-tauri/scripts/package-app.mjs`):
1. **Pacotes desktop standalone (Linux, Windows, macOS)**:
   - A pasta `models/stateful/` (ONNX e engines TensorRT) é empacotada em `resources/models/stateful/` e na raiz do pacote `models/stateful/`.
   - O modelo aprovado Tract é empacotado em `resources/vendor/approved/` e `vendor/approved/`.
2. **Pacote universal de modelos**:
   - `Clearcore-Models-All-Runtimes.tar.gz` e `Clearcore-Models-All-Runtimes.zip` são publicados nos assets oficiais da release no GitHub contendo todos os modelos de todos os runtimes (`models/stateful/`, `models/stateful/tensorrt/`, `vendor/approved/`).
