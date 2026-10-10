# Migração 100% Tract para ONNX Runtime (`ort`) - Plano de Implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Substituir 100% o runtime Tract pelo ONNX Runtime (crate `ort` v2) em todo o workspace Rust do ClearCore (`crates/model`, `crates/filter-capi`, `crates/supervisor`, `crates/service`), garantindo inferência streaming de alta performance com zero alocação de heap por quadro de áudio.

**Architecture:** A camada de inferência neural em `crates/model` adotará o `OrtBackend` implementando `InferenceBackend` e `SpectralModelBackend`. A inferência operará sobre o pipeline DSP agnóstico existente (`DspPipeline`), utilizando `ort::session::Session` e `IoBinding` com tensores pré-alocados para realimentação de estados recorrentes (GRU) e buffers de atraso causal. O modelo de perfil de voz injetará tensores de condicionamento (FiLM e futura memória multi-token) diretamente nas entradas vinculadas do grafo ONNX sem custo dinâmico de alocação.

**Tech Stack:** Rust 1.90+ (`#![forbid(unsafe_code)]`), ONNX Runtime v2 (`ort = "2.0.0-rc.13"`), `ndarray = "=0.15.6"`, `deep_filter_fork` (com feature `transforms`, sem `tract`).

**Spec:** `docs/superpowers/specs/2026-10-09-stage2-virtual-mic-pdfnet3-pro-design.md`, `reports/Arquitetura SOTA TSE Streaming CPU.md`

## Global Constraints

- **Zero Alocação na Thread de Áudio**: Proibido `malloc`, `Vec::push`, `Box::new` ou qualquer alocação dinâmica durante o callback de áudio em tempo real (`process`).
- **Segurança de Memória**: `#![forbid(unsafe_code)]` obrigatório em todos os crates seguros (`crates/model`, `crates/engine`, `crates/supervisor`, `crates/service`).
- **Latência Estrita**: Deadline do quadro de áudio <= 10.0 ms (480 amostras a 48 kHz). A inferência na CPU deve executar em < 2.0 ms.
- **Fail-Closed**: Em caso de falha de carregamento do modelo, emitir silêncio digital absoluto em vez de vazar áudio cru desprotegido.

---

### Task 1: Configuração de Dependências e Desacoplamento do Tract

**Files:**
- Modify: `crates/model/Cargo.toml`
- Modify: `crates/filter-capi/Cargo.toml`
- Modify: `crates/supervisor/Cargo.toml`
- Modify: `crates/service/Cargo.toml`
- Modify: `crates/engine/Cargo.toml`
- Modify: `crates/accelerators/Cargo.toml`

**Interfaces:**
- Consumes: Crate `ort = "=2.0.0-rc.13"` com features `["std", "ndarray", "download-binaries"]`.
- Produces: Features do workspace limpas sem dependências de `tract-*` e `deep_filter/tract`.

- [ ] **Passo 1: Atualizar `crates/model/Cargo.toml`**
  Remover dependências `tract-core`, `tract-hir`, `tract-onnx`, `tract-pulse` e substituir a feature `tract` por `ort`.
  Configurar `deep_filter` para usar apenas `transforms`.

- [ ] **Passo 2: Atualizar dependências nos crates consumidores**
  Substituir `features = ["tract"]` por `features = ["ort"]` em `crates/filter-capi/Cargo.toml`, `crates/supervisor/Cargo.toml`, `crates/service/Cargo.toml`, `crates/engine/Cargo.toml` e `crates/accelerators/Cargo.toml`.

- [ ] **Passo 3: Verificar resolução do Cargo**
  Executar: `cargo check -p realtime-noise-contracts`
  Garantir que a resolução do workspace não quebra por conflitos de features.

---

### Task 2: Implementação do `OrtBackend` em `crates/model`

**Files:**
- Create: `crates/model/src/ort_backend.rs`
- Modify: `crates/model/src/lib.rs`
- Modify: `crates/model/src/dsp_pipeline.rs`

**Interfaces:**
- Consumes: `ApprovedAssetManifest`, `VerifiedAsset`, `CpuProfile`, `ExtractedFeatures`, `ModelFrameOutput`.
- Produces: `pub struct OrtBackend` implementando `InferenceBackend` e `SpectralModelBackend`.

- [ ] **Passo 1: Escrever teste de fumaça de inferência ORT**
  Criar `crates/model/tests/ort_smoke.rs` carregando um modelo ONNX e validando execução de quadro com verificação de zero alocação e formato de saída.

- [ ] **Passo 2: Implementar estrutura `OrtBackend` e inicialização de sessão**
  Implementar configuração da sessão `ort::session::Session` com nível de otimização `GraphOptimizationLevel::Level3`, `with_intra_threads(2)` e `with_inter_threads(1)`.

- [ ] **Passo 3: Implementar IO Binding com tensores pré-alocados**
  Pré-alocar tensores para espectro de entrada, estados ocultos do GRU (`h_in`/`h_out`) e vetores de condicionamento de voz. Configurar `session.create_binding()` reutilizável.

- [ ] **Passo 4: Implementar `process_spectrum` e `process` em `OrtBackend`**
  Executar inferência com `session.run_binding()`, copiar saídas para buffers pré-alocados sem alocação dinâmica e retornar `ProcessedFrame`.

- [ ] **Passo 5: Conectar suporte a Voice Profile no `OrtBackend`**
  Implementar `set_voice_profile` atualizando os tensores vinculados de condicionamento (FiLM ou tokens de falante).

- [ ] **Passo 6: Exportar `OrtBackend` em `crates/model/src/lib.rs`**
  Exportar `pub use ort_backend::OrtBackend;` condicionado à feature `ort` ou padrão.

---

### Task 3: Atualização do `filter-capi` (Ponte Virtual Mic / PipeWire)

**Files:**
- Modify: `crates/filter-capi/src/lib.rs`

**Interfaces:**
- Consumes: `realtime-noise-model::OrtBackend`.
- Produces: C-ABI exports `clearcore_filter_init`, `clearcore_filter_process`, `clearcore_filter_set_profile`.

- [ ] **Passo 1: Atualizar instanciação do motor para `OrtBackend`**
  Em `crates/filter-capi/src/lib.rs`, substituir a inicialização de `TractBackend` por `OrtBackend::new(&manifest, CpuProfile::Avx2Minimum)`.

- [ ] **Passo 2: Validar compilação do C-ABI**
  Executar: `cargo build -p realtime-noise-filter-capi`
  Garantir compilação limpa do `libclearcore_filter.so`.

- [ ] **Passo 3: Executar suíte de testes do C-ABI**
  Executar: `cargo test -p realtime-noise-filter-capi`
  Verificar 12/12 testes passando.

- [ ] **Passo 4: Executar testes da ponte PipeWire**
  Executar: `meson test -C platform/linux/helper/build`
  Verificar 3/3 testes da integração PipeWire passando.

---

### Task 4: Atualização do `realtime-noise-supervisor` e `realtime-noise-service`

**Files:**
- Modify: `crates/supervisor/src/backend.rs`
- Modify: `crates/supervisor/src/supervisor.rs`
- Modify: `crates/service/src/main.rs`
- Modify: `crates/service/src/ipc_voice.rs`

**Interfaces:**
- Consumes: `OrtBackend` como motor CPU primário.
- Produces: Daemon do supervisor e serviço IPC operando 100% sobre ONNX Runtime.

- [ ] **Passo 1: Atualizar seleção de backend CPU no supervisor**
  Em `crates/supervisor/src/backend.rs`, alterar o fallback/default CPU de `Tract` para `Ort`.

- [ ] **Passo 2: Atualizar testes do supervisor**
  Rodar e ajustar testes em `crates/supervisor/tests/`.

- [ ] **Passo 3: Atualizar serviço e rotas IPC de perfil**
  Em `crates/service/src/ipc_voice.rs` e `crates/service/src/enrollment_ingest.rs`, certificar que a injeção do perfil interage com `OrtBackend`.

- [ ] **Passo 4: Executar testes de integração do serviço**
  Executar: `cargo test -p realtime-noise-service`

---

### Task 5: Limpeza Final de Código Morto e Validação Completa

**Files:**
- Remove/Deprecate: `crates/model/src/tract_backend.rs`
- Modify: Arquivos de documentação e manifestos

- [ ] **Passo 1: Remover arquivos exclusivos do Tract**
  Remover `crates/model/src/tract_backend.rs` e testes depreciados do Tract.

- [ ] **Passo 2: Rodar checagem global do workspace**
  Executar: `cargo check --workspace`

- [ ] **Passo 3: Rodar suíte de testes completa do workspace**
  Executar: `cargo test --workspace`

- [ ] **Passo 4: Benchmark e medição de latência**
  Verificar latência de inferência <= 2.0 ms e ausência de regressões de áudio.
