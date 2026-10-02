# Technology Stack: Hippocamp

## 1. Core & Architecture
- **Language:** Rust (Edition 2024, toolchain pinned at `1.90.0`).
- **Workspace Policy:** Strict compile-time checks (`unsafe_code = "forbid"`, Clippy pedantic/nursery warnings, zero unwrap/panic in core libraries).
- **Core Crates:**
  - `crates/contracts`: Audio framing, ring buffers, wire ABI (`WireFrameEnvelopeV1`).
  - `crates/model`: Tract backend abstraction, asset manifests, golden fixture verification.
  - `crates/tools`: Offline benchmark, hardware observation, golden generator, asset validation.
  - `crates/engine`: Low-latency audio processing loop, silence-on-failure state machine.
  - `crates/supervisor`: Process watchdog, backoff crash recovery, health telemetry.
  - `crates/service`: Background OS user session service.
  - `crates/studio-dsp` *(planejado, fase 1)*: Cadeia DSP de acabamento "estúdio" (Rust puro, sem dependências externas).
  - `crates/libdf-fork` *(planejado, fase 4; nome provisório do crate isolado)*: Fork vendorizado do libDF com entradas FiLM `gamma/beta` (ver seção 6).

## 2. Neural Audio & DSP
- **Default Reference Model:** DeepFilterNet3 (DFN3 `v0.5.6`) full-band (48 kHz mono, `HOP_SAMPLES = 480`).
- **Inference Runtime:** `tract` CPU (AVX2 on x86_64, NEON on ARM64/Apple Silicon) with `default-features = false`.
- **Planned Accelerators:** NVIDIA TensorRT, AMD MIGraphX, Apple CoreML, Intel OpenVINO.

## 3. Platform Native Virtual Drivers
- **Windows 11:** C++20 with Windows Driver Kit (WDK) PortCls / WaveRT virtual miniport driver.
- **macOS 13+:** Swift + C with CoreAudio HAL Audio Server Plug-in.
- **Linux (Ubuntu 24.04+, Fedora 42+):** C17 with native PipeWire 1.0+ filter/node integration.

## 4. User Interface & Presentation
- **Framework:** Tauri 2 desktop shell.
- **Frontend:** TypeScript + React + Vite + Tailwind CSS.
- **Decoupled Architecture:** GUI runs as a client communicating via local IPC (`realtime-noise.v1`) with the background daemon. Closing or crashing the UI never drops or halts audio streaming.

## 5. Quality, Verification & Tooling
- **CI/CD:** Multi-stage GitHub Actions, Docker-isolated offline execution (`hippocamp-task4-rust:local`).
- **Verification Harness:** Golden reference comparator (`generate-golden`), sustained frequency measurement, allocation tracking via `stats_alloc`.
- **Supply Chain:** SPDX Software Bill of Materials (SBOM), pinned dependencies with locked revisions.

## 6. Studio Pipeline (planejado)
Decisões de stack registradas antes da implementação (`conductor/workflow.md`, princípio 2). Fonte: `docs/superpowers/specs/2026-10-01-clearcore-studio-pipeline-design.md`. Nada desta seção está implementado até a fase correspondente fechar.

- **`studio-dsp` (fase 1):** crate Rust puro, **sem dependências externas**, com `#![forbid(unsafe_code)]` e os lints do workspace (`unwrap`/`expect`/`panic` em `deny`). Cadeia, nesta ordem: high-pass 80 Hz, EQ fixo (2 biquads), de-esser (passa-banda 5-9 kHz com envelope), compressor, AGC de loudness (ITU-R BS.1770, alvo -16 LUFS) e limiter com lookahead e teto de -1 dBFS. Contrato: `StudioChain::new(Preset)`, `process(&mut self, &mut [f32; 480])` sem alocação e sem panic, e `latency_samples()` (só o lookahead do limiter, cerca de 96 amostras ou 2 ms). Nunca produz NaN nem infinito.
- **`Preset`:** `Off`, `Natural`, `Podcast`, `Broadcast`, em tabela constante. `Off` é **passagem bit a bit idêntica** e é o padrão do produto. A troca de preset usa valor atômico compartilhado, aplicado no limite de frame com crossfade de cerca de 10 ms. Persistência em `settings.json` versionado (fase 2).
- **`StudioBackend<B: InferenceBackend>` (fase 1):** decorator que implementa a mesma trait `InferenceBackend`: `process` chama o backend interno e depois a cadeia, e `algorithmic_latency_samples` soma a latência da cadeia. O engine e o `filter-capi` apenas embrulham o backend que já criam; **a trait não muda**. Em `Bypass` o áudio passa cru e em `Mute` sai zero; o estúdio só roda em `Active`.
- **Fork do libDF vendorizado (fase 4):** hoje a dependência `deep_filter` v0.5.6 é por `git` + `rev`, o que exige rede e conflita com o gate offline. O spike da fase 3 (GO, paridade bit-exata em identidade, custo +0,1%) mostrou que o backend personalizado precisa de um fork com entradas `gamma/beta` (patch de +127/-21 linhas, ou seja, não "mínimo"), que passa a ser **vendorizado dentro do repositório**.
  - **Ressalva de `unsafe`:** o código do fork contém `unsafe`, em conflito com `unsafe_code = "forbid"` do workspace (`forbid` não pode ser relaxado por `allow` dentro de um crate que herda `[lints] workspace = true`). A exceção só é aceita num **crate isolado**, que não herda os lints do workspace, com a exceção escrita e justificada no próprio `Cargo.toml` e neste arquivo. Todo o restante do workspace continua em `forbid`.
  - **Licença:** o libDF é `MIT OR Apache-2.0`, compatível com a política do projeto. É obrigatório manter `LICENSE-MIT` e `LICENSE-APACHE` e o aviso de copyright do upstream dentro do diretório vendorizado e **marcar os arquivos modificados**.
  - Os assets neurais novos (pDFNet3, enrollment, EQ neural) entram por um registro de descritores com a mesma verificação de assinatura; o DFNet3 v1 atual segue aprovado e é o fallback.
