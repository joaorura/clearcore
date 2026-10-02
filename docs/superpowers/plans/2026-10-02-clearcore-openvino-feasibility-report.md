# Relatório de Viabilidade e Medições: Execução Real de Runtimes de Aceleração (OpenVINO / NPU / GPU)

**Data:** 2026-10-02  
**Autor:** Antigravity / Pair Programming  
**Status:** Concluído / Decisão de Implementação Registrada pelo Usuário  
**Investigação de Origem:** Sessão Claude `463e3f9e-5f34-4ae2-a2be-58d4067aa248`

---

## 1. Resumo Executivo e Contexto

No Clearcore, a interface de usuário exibia os aceleradores de hardware (OpenVINO, NVIDIA TensorRT, Apple CoreML, AMD RyzenAI) com o selo **"Prévia — ainda não processa áudio"**, limitando o processamento real à CPU via Tract (`cpu_tract`).

Ao auditar o histórico de desenvolvimento e o código-fonte:
1. **Por que era "Prévia":** A Task 15 do Conductor foi encerrada como `completed` porque implementou a infraestrutura de política de seleção (`auto.rs`), as assinaturas do trait `InferenceBackend` e mocks de teste. No entanto, os backends de hardware em `crates/accelerators/` (`openvino.rs`, `cuda.rs`, `coreml.rs`, `ryzenai.rs`) são stubs passthrough (`let output = *input;`). O `filter-capi` instancia fixamente o `TractBackend`.
2. **Decisão do Usuário:** O usuário determinou que o **foco primário do projeto é implementar a execução real desses runtimes de aceleração no aplicativo**.

Este relatório consolida a investigação de viabilidade técnica, as medições em hardware real na máquina local, o principal desafio arquitetural identificado (estado temporal e pulsificação) e o roteiro para execução.

---

## 2. Hardware e Ambiente Medido

Os testes foram executados na máquina local do desenvolvedor:
- **Processador (CPU):** Intel Core Ultra 7 265H (x86_64, arquitetura híbrida de alto desempenho)
- **Placa de Vídeo Dedicada (GPU):** NVIDIA RTX PRO 1000 Blackwell (dGPU)
- **Acelerador de IA (NPU):** Intel AI Boost
- **Runtime Utilizado:** OpenVINO 2026.4.1 (`pylib`)
- **Asset Avaliado:** DeepFilterNet3 oficial aprovado (`vendor/approved/df-compatible-release-asset-v1.bin`), contendo `enc.onnx`, `erb_dec.onnx` e `df_dec.onnx` (opset 12).

---

## 3. Inspeção Estrutural dos Grafos ONNX

A ferramenta [`tools/feasibility/openvino/inspect_onnx.py`](file:///home/joaorura/orca/workspaces/clearcore/hippocamp/tools/feasibility/openvino/inspect_onnx.py) inspecionou as 3 redes que compõem o pipeline do DeepFilterNet3:

| Grafo | Entradas | Saídas | Nós GRU | Conv / Pad |
|---|---|---|---|---|
| `enc` | `feat_erb [1, 1, 'S', 32]`, `feat_spec [1, 2, 'S', 96]` | `e0..e3`, `emb [1, 'S', 512]`, `c0`, `lsnr [1, 'S', 1]` | 1 GRU (`h=256`) | 11 Conv, 2 Pad |
| `erb_dec` | `emb [1, 'S', 512]`, `e3`, `e2`, `e1`, `e0` | `m [1, 1, 'S', 32]` | 2 GRUs (`h=256`) | 9 Conv, 2 ConvTranspose |
| `df_dec` | `emb [1, 'S', 512]`, `c0 [1, 64, 'S', 96]` | `coefs`, `235` | 2 GRUs (`h=256`) | 2 Conv, 1 Pad |

### Achado Crítico: Ocultação de Estados Temporais
- **5 nós GRU no total:** Todos configurados com `hidden_size=256, linear_before_reset=1`.
- A 6ª entrada (`initial_h`) de cada GRU é conectada a nós de constante (`ConstantOfShape` gerando zeros ou `Slice` estático).
- A saída oculta `Y_h` dos GRUs **não é exposta como saída dos grafos**.
- As camadas de convolução temporal (`Conv`) e `Pad` causal também possuem buffers de atraso histórico não expostos nas portas de entrada/saída.
- **Como o Tract funciona hoje:** A biblioteca `deep_filter` utiliza o recurso de **pulsificação do Tract** (`PulsedModel::new(&m, s, ...)`), onde o motor do Tract intercepta os nós recorrentes e convolucionais e gerencia o estado e anéis de atraso em memória internamente a cada hop de áudio.
- **O problema no OpenVINO bruto:** Se os grafos ONNX forem executados no OpenVINO com dimensão de tempo $S=1$ (um frame a cada 10 ms), cada inferência reinicia os GRUs com zeros e perde o histórico das convoluções.

---

## 4. Prova Empírica de Divergência Temporal

O script [`tools/feasibility/openvino/state_check.py`](file:///home/joaorura/orca/workspaces/clearcore/hippocamp/tools/feasibility/openvino/state_check.py) comparou a execução contínua de 8 frames em lote ($S=8$) contra 8 chamadas sucessivas isoladas de 1 frame ($S=1$) na CPU via OpenVINO:

```text
emb:  max|S=1 por frame - S=8| = [0.0000, 0.9888, 1.1083, 1.0814, 1.4701, 1.5862, 0.9572, 0.9750]
lsnr: max|S=1 por frame - S=8| = [0.0000, 4.2953, 4.5297, 3.8794, 24.2626, 27.1918, 0.1400, 18.2689]
```

**Resultado:**
- No frame $t=0$, o erro é **exatamente 0.0000** (pois o estado inicial de ambos começa em zero).
- A partir do frame $t=1$, a divergência dispara imediatamente, atingindo mais de **25 dB** de erro no cálculo de LSNR.
- **Conclusão:** É impossível processar áudio real em tempo real frame a frame com os grafos ONNX brutos sem reexportá-los com portas de estado I/O explícitas ou sem injetar nós `ReadValue`/`Assign` da API de estados do OpenVINO.

---

## 5. Medições de Latência nos Dispositivos Locais

A ferramenta [`tools/feasibility/openvino/bench_ov.py`](file:///home/joaorura/orca/workspaces/clearcore/hippocamp/tools/feasibility/openvino/bench_ov.py) compilou e mediu os 3 grafos ($S=1$) em cada dispositivo disponível no host (20 aquecimentos, 200 repetições):

| Dispositivo | Grafo `enc` (p50 / p99) | Grafo `erb_dec` (p50 / p99) | Grafo `df_dec` (p50 / p99) | **Soma p50** | **Soma p99 (est.)** |
|---|---|---|---|---|---|
| **CPU (OpenVINO)** | 157 µs / 463 µs | 198 µs / 332 µs | 98 µs / 163 µs | **~453 µs** | **~958 µs** |
| **GPU (NVIDIA RTX 1000)** | 295 µs / 817 µs | 219 µs / 653 µs | 172 µs / 539 µs | **~685 µs** | **~2.010 µs** |
| **NPU (Intel AI Boost)** | 770 µs / 2.444 µs | 742 µs / 1.555 µs | 400 µs / 1.084 µs | **~1.911 µs** | **~5.083 µs** |

*(Para referência: o pipeline completo do Tract em CPU leva ~656 µs p50 por frame de 10 ms).*

### Interpretação dos Resultados
1. **Sem ganho de latência:** A CPU no OpenVINO fica na mesma ordem de grandeza do Tract. A GPU é ligeiramente mais lenta devido ao overhead de submissão de kernels para tamanhos de lote pequenos ($S=1$). A NPU é substancialmente mais lenta (~1,9 ms p50, p99 de ~5 ms).
2. **Deadline de 10 ms:** A NPU consome entre 20% e 50% de todo o orçamento de tempo do frame apenas na inferência dos 3 grafos (sem contar STFT, ERB e pós-filtro).
3. **Ganho real esperado:** O benefício primário de offloading para GPU ou NPU não é redução de latência, mas sim **desonerar a CPU principal** (economizando ~6% a 8% de um núcleo de CPU) e potencialmente reduzir o consumo de energia em notebooks/dispositivos móveis.

---

## 6. Requisitos Técnicos para Implementação

Para viabilizar a execução real dos runtimes no app, os seguintes blocos devem ser construídos:

### Bloco A: Cirurgia / Reexportação do Modelo ONNX
- Expor os estados `h` dos 5 nós GRU como entradas e saídas do grafo:
  - Entradas: `h_in_enc [1, 1, 256]`, `h_in_erb0 [1, 1, 256]`, `h_in_erb1 [1, 1, 256]`, `h_in_df0 [1, 1, 256]`, `h_in_df1 [1, 1, 256]`.
  - Saídas: `h_out_enc`, `h_out_erb0`, `h_out_erb1`, `h_out_df0`, `h_out_df1`.
- Expor ou embutir anel de atraso para as convoluções causais.
- Alternativa elegante no OpenVINO: Utilizar a API de `StatefulModel` do OpenVINO (transformação via `ov::pass::MakeStateful` ou nós `ReadValue`/`Assign`), permitindo que o runtime retenha a memória internamente entre inferências consecutivas sem tráfego de buffers no Rust.

### Bloco B: Crate Isolado de FFI OpenVINO (`unsafe`)
- A política global do workspace fixa `#![forbid(unsafe_code)]` (`Cargo.toml:28`). Apenas o `crates/filter-capi` possui exceção.
- Runtimes como OpenVINO exigem FFI C / C++ ou carregamento dinâmico via `libopenvino_c.so`.
- Solução: Criar um crate dedicado (`crates/runtime-openvino` ou similar) com autorização auditada para `unsafe` restrito às fronteiras de FFI do OpenVINO.

### Bloco C: Desacoplamento do Pipeline DSP
- Hoje o pré-processamento (STFT, banco de filtros ERB) e pós-processamento (filtro DF, iSTFT) estão fortemente acoplados ao Tract através da `libdf`.
- Para o OpenVINO, o pipeline precisa receber o espectrograma STFT, alimentar a inferência e aplicar os coeficientes no domínio da frequência.

### Bloco D: IPC e Seleção Dinâmica
- O comando IPC do Clearcore (`realtime-noise.v1`) e o daemon Rust precisam receber a instrução de troca de backend.
- Carregamento assíncrono (off-thread) e aquecimento de inferência antes de ativar a troca no fluxo de áudio, com fallback garantido para a CPU Tract em caso de falha de dispositivo.

---

## 7. Próximos Passos (Plano de Ação)

Seguindo a diretriz de implementar as runtimes, a sequência recomendada é:

1. **Spike 1 — Stateful ONNX & Paridade Numérica:**
   - Construir script que modifica os grafos ONNX existentes (ou reexporta) adicionando estados (`ReadValue`/`Assign` ou portas de entrada/saída de `h`).
   - Validar que o erro contra o golden M1 / Tract cai para zero em todos os frames $t > 0$.
2. **Spike 2 — Runtime FFI em Rust:**
   - Criar protótipo de runner OpenVINO C-API em Rust puro chamando o grafo stateful compilado para CPU e NPU.
3. **Integração no Daemon:**
   - Implementar o método `process()` real em `crates/accelerators/src/openvino.rs`.
   - Adicionar comando de seleção no IPC e remover a trava de "Prévia" na UI do Electron.
