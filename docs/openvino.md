# pDFNet3 Pro no OpenVINO (NPU, GPU, CPU)

O daemon (`realtime-noise-service`) roda o pDFNet3 Pro no OpenVINO quando um backend OpenVINO é
escolhido nas configurações. Este documento registra como ele é carregado, como a runtime é achada e
empacotada, a evidência de ABI que permite aceitar as séries 2025 e 2026, os números de paridade e
latência medidos e a decisão sobre o backend padrão.

## Modelo e alimentação

- **Export usado**: `models/pdfnet3/pdfnet3-stateful-asset-v1.tar.gz` (SHA-256 do arquivo inteiro
  fixado em `crates/model/src/pdfnet3_bundle.rs`, o mesmo do filtro do microfone virtual). O export
  do tract (`pdfnet3-release-asset-v1`) não serve: ele não expõe o estado recorrente como porta e o
  tract o "pulsa" internamente.
- **Grafos**: `enc` (entradas `feat_erb`, `feat_spec`, `gamma`, `beta`, `h_in`, `feat_erb_buf`,
  `feat_spec_buf`), `erb_dec` (sem FiLM), `df_dec` (`emb`, `c0`, `gamma`, `beta`, `h_in`, `c0_buf`).
  As entradas FiLM são detectadas no carregamento (`ov_model_const_input_by_index` +
  `ov_port_get_any_name`); sem elas no `enc` e no `df_dec`, o carregamento falha.
- **Shapes estáticos**: toda dimensão dinâmica é fixada em `S = 1` (um hop de 10 ms) com
  `ov_model_reshape_input_by_name`, inclusive `gamma`/`beta` em `[1, 1, 256]`. É o que a NPU exige.
- **FiLM**: identidade (`gamma = 1`, `beta = 0`) sem perfil, como o `FiLMVectors::identity()` que o
  tract recebe; `set_voice_profile` reescreve os quatro tensores no lugar entre dois hops, sem tocar
  no estado recorrente. A EQ do perfil é aplicada por bin logo antes da iSTFT
  (`DspPipeline::set_spectral_eq_factors`), no mesmo ponto do `DfTract::set_spectral_eq_factors`.
- **Estado**: as saídas `*_out` (`h_out`, `feat_*_buf_out`, `c0_buf_out`) voltam para as entradas
  correspondentes depois de cada hop, só quando os três estágios terminaram sem erro.
- **Precisão**: FP32 na CPU x86, FP16 (`INFERENCE_PRECISION_HINT=f16`) na GPU e na NPU.
- **Coerência**: `supports_voice_profile()` é `true` exatamente quando os grafos têm FiLM; com o Pro,
  `set_voice_profile` aplica e confirma o perfil (testado em `crates/supervisor/tests/isolation_model.rs`).

## Seleção no daemon (`crates/supervisor/src/supervisor.rs`)

| Pedido | Resultado |
| :--- | :--- |
| `auto`, `tract`, `cpu` | tract (CPU) |
| `openvino-npu`, `npu`, `intel-npu` | OpenVINO NPU |
| `openvino-gpu`, `intel-gpu`, `arc`, `gpu` | OpenVINO GPU |
| `openvino-cpu`, `intel-cpu` | OpenVINO CPU |
| `openvino` | NPU, depois GPU, depois CPU (o primeiro que compila e passa no prazo) |
| `tensorrt` e outros aceleradores | tract, `fallback: true` (não alimentam FiLM) |

Antes de instalar, o backend passa pela qualificação de tempo real (5 hops de aquecimento, 3 medidos,
cada um abaixo de 9 ms no build release) e o estado é zerado. Qualquer falha instala o tract e o
motivo aparece em `SetBackend.fallback_reason` e em `GetStatus.backend_fallback_reason`.

**Decisão: o `auto` continua no tract.** O backend do daemon não processa o áudio do microfone
virtual: esse caminho é o `libclearcore_filter` (ONNX Runtime na CPU). Pôr a NPU como padrão do daemon
não tiraria carga da CPU, só acrescentaria compilação na partida (100 ms a 1 s), um dispositivo aberto
e o desvio numérico do FP16; e o tract é a referência de paridade. O OpenVINO fica opcional nas
configurações até o filtro também rodar nele.

## Runtime: descoberta e versões (`crates/runtime-openvino/src/ffi.rs`)

Ordem de busca de `libopenvino_c` (`libopenvino_c.so*` por glob no Linux, `openvino_c.dll` no
Windows), maior sufixo numérico primeiro dentro de cada diretório:

1. `OPENVINO_LIB_PATH` (o arquivo, ou um diretório);
2. runtime embutido: `resources/openvino` e `openvino` sob o diretório do executável e seus três
   primeiros ancestrais (o daemon fica em `<app>/resources/bin`), depois
   `/opt/clearcore/resources/openvino`, `/usr/lib/clearcore/...`, `/usr/share/clearcore/...`;
3. sistema: `/usr/lib64`, `/lib64`, `/usr/local/lib64`, `/usr/lib/x86_64-linux-gnu`, `/usr/lib`,
   `/usr/local/lib`;
4. último recurso: o soname puro no `dlopen`.

A biblioteca é aberta pelo caminho absoluto (`RTLD_NOW | RTLD_LOCAL`); as dependências e os plugins
saem do mesmo diretório (RPATH `$ORIGIN` em todas as `.so` do wheel; o OpenVINO procura os plugins ao
lado do `libopenvino` quando não há `plugins.xml`). No Windows o `LoadLibraryExA` usa
`LOAD_WITH_ALTERED_SEARCH_PATH` para o mesmo efeito.

O estado da runtime é publicado em `GetStatus.openvino_runtime.status`: `ok`, `not_found`,
`incompatible_version`, `load_failed`, `npu_plugin_missing` (há NPU no host e a runtime não tem o
plugin, caso do pacote `openvino` 2026.0 do Fedora 45, só CPU/GPU) ou `npu_device_unavailable`
(plugin presente, NPU não listada: driver, firmware ou permissão em `/dev/accel`).

### Evidência de ABI 2025 × 2026

Séries aceitas: 2025 e 2026 (`SUPPORTED_MAJOR_VERSIONS`). Conferido em 2026-11-09:

- Cabeçalhos C dos wheels `openvino` 2025.4.0 e 2026.4.1 (`openvino/include/openvino/c/`):
  `diff -r -I Copyright` mostra só (a) o `ov_element_type_e` de 2026 sem o alias `UNDEFINED`
  (que valia `DYNAMIC = 0`; todos os outros valores iguais, `F32 = 4`), (b) funções novas não variádicas
  (`ov_core_compile_model_props`, `ov_core_set_properties`, ...) e (c) comentários. `ov_status_e`,
  `ov_shape_t`, `ov_partial_shape_t`, `ov_dimension_t`, `ov_version_t`, `ov_available_devices_t`
  idênticos. Os valores 2025.1 espelhados em `ov_element_type_e` batem com o cabeçalho 2025.4.
- Símbolos `ov_*` exportados (`nm -D --defined-only`) de `libopenvino_c.so.2540` (pip 2025.4),
  `.2600` (Fedora 2026.0) e `.2641` (pip 2026.4): iguais entre 2540 e 2600; o 2641 só acrescenta as
  funções novas acima. Todo símbolo que o crate liga existe nos três.

## Empacotamento

- **Fonte**: wheel oficial `openvino` 2026.4.1 da PyPI, fixado (URL, tamanho, SHA-256) em
  `packaging/openvino/openvino-runtime.lock`. `crates/app-tauri/scripts/openvino-runtime.mjs fetch`
  baixa, confere tamanho e SHA-256 **antes** de extrair, e monta `third_party/openvino-runtime/<plat>`
  com um manifesto (`openvino-runtime.json`, SHA-256 por arquivo); o `package-app.mjs` confere o
  manifesto de novo e copia para `<app>/resources/openvino`. No release, `CLEARCORE_REQUIRE_OPENVINO=1`
  faz o build falhar sem a runtime.
- **Arquivos** (Linux x64): `libopenvino_c`, `libopenvino`, frontends ONNX e IR, plugins CPU, GPU e
  NPU, `libtbb`, `libtbbmalloc`, `libtbbbind_2_5`, `libhwloc`, `cache.json` (kernels da GPU) e as
  licenças do wheel (Apache-2.0 + third-party). Sem código Python.
- **Tamanho** (Linux x64, medido com os arquivos do 2026.4.1): 154,5 MB descompactados (147 MiB);
  +49 MB no `tar.gz`, cerca de +32 MB com xz (RPM).
- **Plataformas**: Linux x64 (tar.gz, AppImage, RPM, DEB, spec do COPR — todos reaproveitam o mesmo
  bundle) e Windows x64 (zip e instaladores NSIS/Inno copiam o diretório inteiro; o wheel `win_amd64`
  traz o plugin NPU). ARM64 e macOS não embutem: o Pro roda no tract.
- **RPM/DEB**: `AutoReqProv: no` continua (as `.so` embutidas não podem ser providas ao sistema); nada
  vai para o `ld.so.conf`. Dependências externas da runtime: glibc, libstdc++, libgcc (já exigidas) e
  `libOpenCL.so.1` para o plugin GPU. Os drivers da NPU/GPU entram como `Recommends`
  (`OpenCL-ICD-Loader`, `intel-npu-driver`, `oneapi-level-zero`, `intel-level-zero` no Fedora;
  `ocl-icd-libopencl1` e `Suggests: intel-opencl-icd, libze1` no Debian/Ubuntu).
- `scripts/install-openvino.sh` deixou de sugerir pacotes inexistentes: agora só diagnostica (mesma
  busca do daemon, plugin NPU, driver user space, acesso a `/dev/accel`) e mostra os comandos dos
  drivers por distribuição.

## Paridade e latência medidas

> **Histórico (Pro v2, FiLM).** Esta seção mediu os exports v2 (`pdfnet3-release-asset-v1` e
> `pdfnet3-stateful-asset-v1`), que não são mais distribuídos; o teste `openvino_pdfnet3` foi removido
> junto com eles. A paridade do Pro v3 completo no OpenVINO está na seção do executor genérico abaixo
> (`crates/runtime-openvino/tests/stateful_graph.rs`).

Comando usado na época (teste `#[ignore]`, ligado por variável):

```text
CLEARCORE_OV_PARITY=1 OPENVINO_LIB_PATH=<.../openvino/libs> \
CLEARCORE_PARITY_WAV=<fala 48 kHz mono 16 bits> \
cargo test --release -p realtime-noise-accelerators --test openvino_pdfnet3 -- --ignored --nocapture
```

Máquina: Intel Core Ultra (NPU Intel, `intel-npu-driver` 1.32), OpenVINO 2026.4.1 do pip; 600 hops de
`mix_0002_snr-01.5db_noisy.wav`; SNR da saída contra a referência, depois de 50 hops de aquecimento;
perfil = FiLM do `models/enrollment/enrollment.onnx` + EQ em rampa de -3 a +3 dB.

| Caso | Dispositivo | vs tract (SNR / max\|erro\|) | vs ORT, mesmo export (SNR) | p50 / p99 / max por hop |
| :--- | :--- | :--- | :--- | :--- |
| sem perfil | tract (referência) | — | — | 0,73 / 1,60 / 1,80 ms |
| sem perfil | CPU fp32 | 29,7 dB / 0,086 | 133,7 dB | 0,38 / 0,99 / 3,07 ms |
| sem perfil | GPU fp16 | 29,7 dB / 0,086 | 131,2 dB | 0,72 / 1,24 / 2,48 ms |
| sem perfil | NPU fp16 | 29,6 dB / 0,086 | 58,8 dB | 1,74 / 3,56 / 21,2 ms |
| só EQ | CPU / GPU / NPU | 29,3 / 29,3 / 29,2 dB | (ORT ignora a EQ) | — |
| só FiLM | CPU / GPU / NPU | 6,8 dB | 132,9 / 129,5 / 31,8 dB | NPU 1,17 / 2,54 / 15,5 ms |
| FiLM + EQ | CPU / GPU / NPU | 6,9 dB | (ORT ignora a EQ) | NPU 1,17 / 2,49 / 13,9 ms |

Leitura:

- O OpenVINO reproduz o ONNX Runtime no mesmo export (130 dB na CPU/GPU, 59 dB na NPU em FP16 sem
  perfil, 32 dB com perfil). A alimentação dos estados e do FiLM está certa.
- Os ~30 dB contra o tract sem perfil são a diferença entre os dois exports (tract pulsado × stateful),
  a mesma do ONNX Runtime e do DFNet3 base.
- **Com o FiLM do perfil cadastrado, os dois exports divergem (6,8 dB) também no ONNX Runtime**, ou
  seja, o daemon (tract) e o microfone virtual (ORT) já não concordam hoje com esse perfil. Com um FiLM
  sintético moderado (`gamma` 1 ± 0,15) concordam (32,3 dB). Não é deste backend; fica registrado para
  o repositório de treino (o gate G0 diz que os perfis extremos passam).
- O filtro do microfone virtual (ORT) não aplica a EQ do perfil; o tract e o OpenVINO aplicam.
- Latência: CPU e GPU folgadas; a NPU tem p99 de 2,5 a 3,6 ms e picos isolados de 14 a 21 ms (acima
  do hop de 10 ms). A qualificação de 9 ms por hop roda na seleção, mas não pega picos raros.

Na runtime 2026.0 do Fedora (`/usr/lib64`, sem plugin NPU) CPU e GPU dão os mesmos números (29,7 dB
contra o tract) e a NPU falha com `OpenVINO device not found: NPU`, como esperado.

## pDFNet3 Pro v3 completo: executor genérico (um export para todos os runtimes)

- **Asset**: `models/pdfnet3/pdfnet3-pro-v3-full-stateful-asset-v1.tar.gz` (SHA-256 do arquivo inteiro e
  dos três grafos fixados em `crates/model/src/pdfnet3_bundle.rs`), `manifest.json` + `enc`/`erb_dec`/`df_dec`
  (opset 12, `S = 1`). Condicionamento: `speaker_embedding` [1,192] (ECAPA L2) e `speaker_present` [1]
  (zeros/0 sem perfil), dentro do `enc` e do `df_dec`.
- **Executor** (`crates/model/src/stateful_graph.rs`): resolve tudo por nome — features, condicionamento,
  pares de estado `<x>_in`/`<x>_out` (ou `<x>`/`<x>_out`, legado), valores entre grafos (ordem topológica,
  conferida contra `manifest.routing`) e as saídas `m`/`coefs`/`lsnr`. Roda os três grafos em **todo** hop e
  avança todos os estados sempre; a seleção de estágio pelo `lsnr` (`DspPipeline::apply_stages`) replica
  `emulate_gating_offline` (`clearcore-train/src/cctrain/eval/runtime_path.py:342-360`). Silêncio digital
  (RMS < 1e-7) dá saída zero sem avançar STFT nem modelo; a EQ do perfil vai antes da iSTFT.
- **Runtimes** (`GraphSession`): tract (`tract_graph.rs`), ONNX Runtime (`ort_graph.rs`) e OpenVINO
  (`crates/runtime-openvino/src/graph.rs`, portas descobertas com `ov_model_const_{input,output}_by_index` +
  `ov_port_get_partial_shape`, toda dimensão dinâmica fixada em 1 com `reshape` antes de compilar).
- **Ops no tract 0.19.16**: GRU, Einsum, MatMul/Gemm, Softmax, Erf, Exp, Conv/ConvTranspose registrados;
  `LayerNormalization` (opset 17) **não** — exportar em opset ≤ 16 (o Pro v3 usa opset 12).
- **Paridade medida** (300 hops sintéticos de fala + ruído, perfil = embedding + EQ em rampa):
  tract × ORT 128,8 / 125,0 dB (neutro / perfil); OpenVINO × tract: CPU 130,1 / 117,4 dB, GPU 130,9 / 119,5 dB,
  NPU 41,2 / 48,4 dB (FP16). Latência por hop (p50 / máx): CPU 0,66 / 3,96 ms, GPU 0,85 / 4,01 ms,
  NPU 2,03 / 21,85 ms.
- **Seleção**: com o arquivo de manifesto como modelo de isolamento, `auto`/`tract` usam o executor no tract e
  os pedidos OpenVINO usam o executor no OpenVINO com o MESMO arquivo (não há mais export separado).

## Filtro do microfone virtual

Desde a branch `feat/pro-v3-filter` o `libclearcore_filter` também roda no OpenVINO (NPU → GPU → CPU
conforme o `backend` do `settings.json`, com o ONNX Runtime CPU de reserva quente) e aplica a EQ do
perfil. Detalhes, política de troca e medições em `docs/virtual-mic-filter.md`.
