# Realtime Noise Suppression Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Entregar, no mesmo GA, um microfone virtual de primeira parte em Windows, macOS e Linux que suprima ruído localmente sem encaminhar microfone cru em falhas.

**Architecture:** Um workspace Rust contém contratos, engine, supervisor, IPC, métricas e adaptadores comuns. Cada sistema tem somente o adaptador nativo indispensável para capturar e publicar o endpoint virtual, com callbacks limitados a cópia bounded e filas pré alocadas. A UI Tauri 2 controla um serviço de usuário separado, nunca possui a sessão de áudio.

**Tech Stack:** Rust edition 2024, Cargo workspace, `libDF` com `tract` CPU, Tauri 2, C++20 e WDK PortCls/WaveRT, Swift e CoreAudio HAL Audio Server Plug in, C17 e PipeWire 1.0+, GitHub Actions, SPDX SBOM.

**Spec:** `docs/superpowers/specs/2026-09-23-realtime-noise-suppression-design.md`

## Global Constraints

- O lançamento é GA simultâneo em Windows 11 23H2+ x64, macOS 13+ Apple Silicon, Ubuntu 24.04 LTS x86_64 e Fedora 42+ x86_64. Uma plataforma reprovada bloqueia todas as plataformas.
- A licença provisória de código próprio é Apache-2.0. Código, pesos, runtime, driver, amostras e ativos têm licenças separadas.
- O único ativo de release é `df-compatible-release-asset-v1`. Sem licença, origem, SHA-256, redistribuição e aprovação jurídica registradas, M0/M1 falha e não existe GA.
- O baseline obrigatório é `libDF + tract` em CPU, com `default-features = false`. Não exigir AVX-512, não tratar Intel 10a geração como NPU e não usar VB-Cable, BlackHole, filter-chain ou LADSPA como dependência final.
- O contrato interno é PCM mono `f32`, 48 kHz, `HOP_SAMPLES = 480`, sem PCM no IPC `realtime-noise.v1`.
- `WireFrameEnvelopeV1` Windows/Linux é little-endian, 1.960 bytes, alinhamento 8, offsets 0/4/8/12/16/24/32/40 e rejeita versão, tamanho, flags ou bytes reservados inválidos.
- Drivers, HAL, host de plugin e callbacks PipeWire não alocam, bloqueiam, fazem I/O, log síncrono, RPC de controle, carregamento de modelo ou inferência.
- Não há bypass raw: `Bypass` preserva framing e endpoint sem inferência, `Mute` publica silêncio, e toda falha publica silêncio até uma geração aquecida válida.
- Filas têm capacidade 24 hops; watermark de idade de 20 ms ou 2 hops, o que ocorrer primeiro, fecha geração. Em geração válida, idade de fila é menor ou igual a 10 ms.
- CPU de qualificação é x86_64 AVX2, quatro núcleos físicos, 2,0 GHz sustentados em AC, 8 GB RAM, classe não mais rápida que Intel Core i5-10210U. Apple Silicon requer orçamento CPU caracterizado separado.
- Orçamento de produto é p95 menor ou igual a 80 ms: 40 ms algoritmo DFN3 derivado, até 10 ms captura, até 5 ms conversão, até 10 ms fila e até 15 ms saída. Inferência CPU tem deadline de 10 ms e p99 menor ou igual a 7 ms.
- Soak controlado de 8 horas exige zero `inference_deadline_miss`, zero callback miss atribuível ao produto, zero restart de geração e zero underrun após warm-up.
- `EngineSupervisor` usa backoff 1/2/4/8/16 s, no máximo cinco crashes em 15 minutos, depois `TerminalSafeState` com silêncio e diagnóstico preservado.
- A semântica de sessão é owner lock em Windows/macOS e uma fonte PipeWire por usuário no Linux. Disputa retorna `UnavailableBusy`, nunca preempta nem cruza áudio.
- Formatos externos normativos: Windows PCM16 ou Float32 mono 48 kHz, macOS Float32 mono 48 kHz, Linux F32LE mono 48 kHz com `pipewire-pulse`.
- Convenção de comandos: salvo `cwd` explícito, cada `Run:` executa na raiz do repositório. Uma task cria todos os arquivos e registra todos os packages exigidos pelo seu primeiro comando antes de executá-lo.

---

## Mapa de Arquivos Proposto

| Área | Caminhos e responsabilidade |
|---|---|
| Workspace | `Cargo.toml`, `rust-toolchain.toml`, `Cargo.lock`, `crates/{workspace-policy,tools,qualification,contracts,engine,supervisor,service,ipc,metrics,model,format-adapter,platform-api,app-tauri,diagnostics,accelerators}`. Cada crate entra em `workspace.members` na task que cria seu `Cargo.toml`, antes de qualquer `cargo -p` que o use. |
| Contratos e testes | `crates/contracts/src/{audio,transport,wire,ipc,endpoint}.rs`, `crates/contracts/tests/{wire_v1,transport_properties,ipc_schema}.rs`, `fixtures/{wire,corpus,golden}/` |
| Engine, serviço e benchmark | `crates/engine/src/{engine,queue,worker,generation}.rs`, `crates/supervisor/src/{lib,server}.rs`, `crates/service/{Cargo.toml,src/{main,bootstrap,install}.rs,tests/service_lifecycle.rs}`, `crates/model/src/{asset_manifest,tract_backend,golden}.rs`, `crates/tools/{Cargo.toml,src/bin/{benchmark,calibrate-backend,validate-asset,generate-golden}.rs}`, `benchmarks/{cpu_baseline,allocation}.md` |
| Windows | `platform/windows/driver/{RealtimeNoise.inf,driver.vcxproj,src/{Driver,WaveRtMiniport,IoctlTransport,WireFrameEnvelopeV1}.cpp}`, `platform/windows/host/{Cargo.toml,src/{wasapi,endpoint}.rs}`, `platform/windows/tests/{driver_verifier.ps1,hlk-checklist.md,endpoint-spike.ps1}` |
| macOS | `platform/macos/HAL/RealtimeNoiseHAL.xcodeproj`, `platform/macos/HAL/Sources/{Plugin.swift,HiddenOutput.swift,VisibleInput.swift,RingBuffer.swift}`, `platform/macos/Bridge/{Package.swift,Sources/RealtimeNoiseBridge/EngineXpc.swift}`, `platform/macos/App/RealtimeNoiseMac.xcodeproj`, `platform/macos/tests/{endpoint-spike.sh,notarization-check.sh}` |
| Linux | `platform/linux/helper/{meson.build,src/{pipewire_helper.c,transport_bridge.c,format_converter.c}}`, `platform/linux/host/{Cargo.toml,src/{pipewire,endpoint}.rs}`, `platform/linux/tests/{endpoint-spike.sh,rebind-check.sh}` |
| UI e diagnóstico | `crates/app-tauri/{package.json,package-lock.json,vite.config.ts,tsconfig.json,index.html,src/{main.tsx,App.tsx,diagnostics.tsx,styles.css},scripts/prepare-sidecar.mjs,src-tauri/{Cargo.toml,build.rs,tauri.conf.json,capabilities/default.json,binaries/}}`, `crates/diagnostics/src/{export,privacy}.rs`, `docs/diagnostics-schema.md` |
| Qualificação e release | `crates/qualification/{Cargo.toml,src/lib.rs,tests/ga_decision.rs}`, `tools/qualification/{collect_metrics.rs,run_matrix.py}`, `platform/windows/service/{register-user-service.ps1,unregister-user-service.ps1}`, `packaging/macos/LaunchAgents/com.clearcore.realtime-noise.plist`, `packaging/linux/systemd/user/realtime-noise.service`, `.github/workflows/{ci,windows-hardware,macos-hardware,linux-hardware,release}.yml`, `packaging/{windows,macos,linux}/`, `scripts/{build-sbom,scan-artifacts,qualify-ga}.sh`, `sbom/`, `THIRD_PARTY_LICENSES`, `docs/{support-matrix,release-gates,uninstall}.md` |

## Ondas e Dependências

| Onda | Trabalhos | Dependência para iniciar | Regra de conclusão |
|---|---|---|---|
| 0 | Governança, toolchain e licença | Nenhuma | M0 aprovado, ou trabalho de produto bloqueado |
| 1 | Golden harness, benchmark e contratos | M0 aprovado | M1 aprovado |
| 2 | Engine, filas, resampling, IPC, supervisão e serviço de usuário | Contratos M1 | M3 funcional sem endpoint final e serviço independente da UI |
| 3 | Spikes Windows, macOS e Linux, em paralelo | Contratos M1 e transporte de referência | Três relatórios de evidência, não aprovação de GA |
| 4 | Adaptadores de produção e microfones virtuais | Spike correspondente aprovado | Três endpoints integrados |
| 5 | UI, tray e diagnóstico | Engine supervisor e IPC | UI pode fechar sem afetar áudio |
| 6 | Aceleradores e AUTO, em paralelo após M1 | Baseline CPU aprovado | Plugins apenas promovidos por qualificação; não são requisito nem dependência do GA CPU |
| 7 | Empacotamento, assinatura, atualização e remoção | Adaptadores de produção | Artefatos assinados e removíveis |
| 8 | Qualificação de GA simultâneo | Tasks 1 a 14, Task 16 e baseline CPU M1 aprovados; Task 15 é excluída | Uma reprovação de requisito GA bloqueia a tag em todos os SOs; plugin opcional não bloqueia GA |

### Task 1: Onda 0, governança e gates de ativo

**Files:**
- Create: `LICENSE`, `README.md`, `THIRD_PARTY_LICENSES`, `docs/{release-gates.md,model-asset-manifest.schema.json,evidence/.gitkeep}`, `governance/model-assets/{schemas/{candidate-provenance,legal-review,approval-manifest}.schema.json,templates/{candidate-provenance,legal-review,approval-manifest}.template.json}`, `fixtures/m0/{candidate-incomplete.json,legal-review-incomplete.json,approval-manifest-unsigned.json,approver-public-key.pem}`, `fixtures/corpus/README.md`, `scripts/verify-m0-asset.py`

**Interfaces:**
- Consumes: a especificação aprovada e a identificação `df-compatible-release-asset-v1`.
- Produces: schemas e verificador M0. O candidato exige proveniência, URL ou origem imutável, `asset_id = df-compatible-release-asset-v1`, SHA-256 e termos de conversão. A revisão legal exige `reviewer_identity`, `review_date`, licença de código e peso, e termos explícitos de redistribuição e conversão. A aprovação exige identidade e data do aprovador, hash imutável do candidato, IDs de revisão, `approved_for_release: true`, assinatura Ed25519 e `key_id`.

- [ ] **Step 1: Criar verificador M0, templates pendentes e fixtures fail-closed, sem crate Cargo**

```python
def verify_m0(candidate, legal_review, approval, asset, public_key):
    require_fields(candidate, "asset_id", "source", "sha256", "weight_license", "code_license", "conversion_terms")
    require_fields(legal_review, "reviewer_identity", "review_date", "redistribution_terms", "conversion_terms", "legal_approval_id")
    require_fields(approval, "approved_for_release", "candidate_sha256", "legal_approval_id", "approver_identity", "approval_date", "key_id", "signature")
    verify_sha256(asset, candidate["sha256"])
    verify_ed25519(public_key, canonical_json(approval, without="signature"), approval["signature"])
```

- [ ] **Step 2: Rodar o teste RED**

Run: `python3 scripts/verify-m0-asset.py --candidate fixtures/m0/candidate-incomplete.json --legal-review fixtures/m0/legal-review-incomplete.json --approval fixtures/m0/approval-manifest-unsigned.json --asset /nonexistent/df-compatible-release-asset-v1.bin --approver-key fixtures/m0/approver-public-key.pem --status-out docs/evidence/m0-asset-gate.json`

Expected: sai com código 2 e grava `BLOCKED_NO_APPROVED_ASSET`; templates pendentes, assinatura ausente, arquivo inexistente ou hash não conferente jamais retornam aprovação. O comando é executável porque verificador e fixtures foram criados no Step 1. Não criar binário de release.

- [ ] **Step 3: Definir schemas e registros de aprovação sem fabricar um ativo aprovado**

```json
{
  "approval_manifest": {
    "required": ["asset_id", "candidate_sha256", "legal_approval_id", "approver_identity", "approval_date", "redistribution_terms", "conversion_terms", "approved_for_release", "key_id", "signature"],
    "approved_for_release": true
  }
}
```

Os templates devem usar `status: PENDING`, valores vazios e sem assinatura. Documentar que o padrão é bloqueio M0/M1, não ativo alternativo, DeepFilterNet ambíguo nem peso baixado automaticamente.

- [ ] **Step 4: Confirmar caminho fail-closed após os schemas existirem**

Run: `python3 scripts/verify-m0-asset.py --candidate fixtures/m0/candidate-incomplete.json --legal-review fixtures/m0/legal-review-incomplete.json --approval fixtures/m0/approval-manifest-unsigned.json --asset /nonexistent/df-compatible-release-asset-v1.bin --approver-key fixtures/m0/approver-public-key.pem --status-out docs/evidence/m0-asset-gate.json`

Expected: sai com código 2 e `docs/evidence/m0-asset-gate.json` contém `BLOCKED_NO_APPROVED_ASSET`; esse é o GREEN do comportamento fail-closed.

- [ ] **Step 5: Executar o único caminho de aprovação M0 após revisão legal real**

Após uma revisão legal externa, criar somente então os arquivos reais `governance/model-assets/df-compatible-release-asset-v1/{candidate-provenance.json,legal-review.json,approval-manifest.json,approver-public-key.pem}` e disponibilizar o blob aprovado localmente em `vendor/approved/df-compatible-release-asset-v1.bin`. A aprovação assina, com a chave privada fora do repositório, o JSON canônico de `approval-manifest.json` sem o campo `signature`.

Run: `python3 scripts/verify-m0-asset.py --candidate governance/model-assets/df-compatible-release-asset-v1/candidate-provenance.json --legal-review governance/model-assets/df-compatible-release-asset-v1/legal-review.json --approval governance/model-assets/df-compatible-release-asset-v1/approval-manifest.json --asset vendor/approved/df-compatible-release-asset-v1.bin --approver-key governance/model-assets/df-compatible-release-asset-v1/approver-public-key.pem --status-out docs/evidence/m0-asset-gate.json`

Expected: somente se todos os campos requeridos, `asset_id`, origem, licenças, termos de redistribuição/conversão, identidade/data de revisão legal, `approved_for_release`, SHA-256 do blob e assinatura Ed25519 passarem, sai com código 0 e grava `M0_APPROVED`. Não inserir no plano valores, hash, identidade, data, assinatura ou ativo fictícios.

- [ ] **Step 6: Registrar gate e commit**

Run: `git add LICENSE THIRD_PARTY_LICENSES docs README.md governance fixtures scripts && git commit -m "docs: define release asset and license gates"`

Expected: commit contém somente governança e templates pendentes. Se não houver ativo aprovado, M0 fica `BLOCKED_NO_APPROVED_ASSET`; implementação de produto e GA permanecem bloqueados, e não iniciar Tasks 2 a 17.

### Task 2: Onda 0, workspace, toolchain e CI offline

**Files:**
- Create: `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `.cargo/config.toml`, `.github/workflows/ci.yml`, `scripts/check-offline.sh`, `docs/development-environment.md`, `crates/workspace-policy/{Cargo.toml,src/lib.rs,tests/policy.rs}`, `crates/tools/{Cargo.toml,src/lib.rs}`, `crates/qualification/{Cargo.toml,src/lib.rs}`

**Interfaces:**
- Consumes: gate M0 aprovado e política `default-features = false`.
- Produces: workspace inicial com `workspace-policy`, `realtime-noise-tools` e `realtime-noise-qualification` registrados; os dois últimos são packages vazios até suas tasks de implementação.

- [ ] **Step 1: Criar workspace inicial, manifests registrados e teste RED**

```rust
#[test]
fn workspace_policy_rejects_nonempty_default_features() {
    assert!(!cargo_metadata().package("workspace-policy").default_features_enabled());
}
```

Criar os manifests concretos antes de rodar qualquer comando Cargo:

```toml
# crates/workspace-policy/Cargo.toml
[package]
name = "workspace-policy"
version = "0.1.0"
edition = "2024"

# crates/tools/Cargo.toml
[package]
name = "realtime-noise-tools"
version = "0.1.0"
edition = "2024"

# crates/qualification/Cargo.toml
[package]
name = "realtime-noise-qualification"
version = "0.1.0"
edition = "2024"
```

Criar `src/lib.rs` vazio nos dois packages e executar `cargo generate-lockfile --offline` após os três manifests existirem, produzindo o `Cargo.lock` usado pela CI.

- [ ] **Step 2: Rodar RED**

Run: `cargo test -p workspace-policy workspace_policy_rejects_nonempty_default_features`

Expected: falha porque o manifest inicial de `workspace-policy` declara uma feature default propositalmente. O package e seu registro no workspace já existem.

- [ ] **Step 3: Criar workspace explícito**

```toml
[workspace]
members = ["crates/workspace-policy", "crates/tools", "crates/qualification"]
resolver = "3"
```

Remover a feature default proposital de `workspace-policy`, configurar CI com `cargo build --workspace --locked --offline`, format, clippy e testes. Cada task posterior acrescenta seu crate em `workspace.members` no mesmo Step 1 que cria o manifest. Falha de cache offline é bloqueio de toolchain, não autorização para baixar dependência em job de release.

- [ ] **Step 4: Rodar GREEN**

Run: `cargo test -p workspace-policy workspace_policy_rejects_nonempty_default_features && scripts/check-offline.sh`

Expected: ambos passam localmente, ou o executor registra `BLOCKED_OFFLINE_DEPENDENCY` com a lista exata de crates ausentes.

- [ ] **Step 5: Commit**

Run: `git add Cargo.toml rust-toolchain.toml .cargo .github scripts docs && git commit -m "build: establish offline Rust workspace"`

### Task 3: Onda 1, contratos de áudio, transporte e Wire ABI

**Files:**
- Create: `crates/contracts/{Cargo.toml,src/{lib,audio,transport,wire}.rs,tests/{wire_v1,transport_properties}.rs}`, `fixtures/wire/frame-v1-valid.bin`, `fixtures/wire/frame-v1-invalid-reserved.bin`
- Modify: `Cargo.toml`

**Interfaces:**
- Produces: `AudioFrame = [f32; 480]`, `FrameEnvelope`, `Discontinuity`, `RealtimeTransport`, `WireFrameEnvelopeV1::encode/decode`.
- Consumes: Windows/Linux adapters must use `encode` and `decode`, never transmutar layout Rust.

- [ ] **Step 1: Criar e registrar `realtime-noise-contracts`, depois escrever testes RED**

```rust
#[test]
fn wire_v1_rejects_nonzero_reserved_bytes() {
    assert!(WireFrameEnvelopeV1::decode(include_bytes!("../../../fixtures/wire/frame-v1-invalid-reserved.bin")).is_err());
}

#[test]
fn full_transport_drops_new_frame_without_removing_consumer_item() { /* assert TransportFull */ }
```

- [ ] **Step 2: Rodar RED**

Run: `cargo test -p realtime-noise-contracts`

Expected: falha pelos tipos e encoder ainda ausentes. `crates/contracts/Cargo.toml` e a entrada `crates/contracts` em `workspace.members` já existem.

- [ ] **Step 3: Implementar contratos mínimos e invariantes estáticos**

```rust
pub trait RealtimeTransport: Send + Sync {
    fn try_push(&self, frame: FrameEnvelope) -> Result<(), TransportFull>;
    fn try_pop(&self) -> Option<FrameEnvelope>;
    fn capacity_hops(&self) -> usize;
    fn backlog_hops(&self) -> usize;
    fn close_generation(&self, generation: u64);
}
```

Inserir assertions de 1.960 bytes, alinhamento 8 e offsets normativos. Decoder falha fechado para versão não 1, payload diferente de 1.920, flags desconhecidas e reservado não zero.

- [ ] **Step 4: Rodar GREEN e propriedades**

Run: `cargo test -p realtime-noise-contracts && cargo test -p realtime-noise-contracts --test transport_properties`

Expected: fixtures válidos fazem round trip; testes demonstram que produtor nunca remove item do consumidor.

- [ ] **Step 5: Commit**

Run: `git add crates/contracts fixtures/wire && git commit -m "feat: add realtime audio and wire contracts"`

### Task 4: Onda 1, modelo de referência, golden harness e benchmark CPU

**Files:**
- Create: `crates/model/{Cargo.toml,src/{lib,asset_manifest,tract_backend,golden}.rs,tests/{asset_gate,golden_reference}.rs}`, `crates/tools/src/bin/{generate-golden,benchmark,validate-asset}.rs`, `fixtures/{golden,corpus}/README.md`, `benchmarks/cpu-baseline.md`
- Modify: `Cargo.toml`

**Interfaces:**
- Produces: `InferenceBackend`, `BackendDescriptor`, `ProcessedFrame`, `ApprovedAssetManifest`, corpus M1 e tolerâncias congeladas.
- Consumes: `AudioFrame`; nenhum backend pode ser construído sem manifesto válido.

- [ ] **Step 1: Criar e registrar `realtime-noise-model`, depois escrever RED de ativo e equivalência**

```rust
#[test]
fn tract_backend_refuses_unapproved_asset() { /* expect AssetNotApproved */ }
#[test]
fn approved_cpu_output_matches_frozen_golden_within_m1_tolerance() { /* compare samples */ }
```

- [ ] **Step 2: Rodar RED**

Run: `cargo test -p realtime-noise-model`

Expected: falha pela ausência de backend e golden fixture. `crates/model/Cargo.toml` e a entrada `crates/model` em `workspace.members` já existem; `realtime-noise-tools` foi registrado na Task 2. Não substituir por mock de pesos.

- [ ] **Step 3: Implementar baseline condicionado ao ativo aprovado**

```rust
pub trait InferenceBackend: Send {
    fn descriptor(&self) -> BackendDescriptor;
    fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError>;
    fn algorithmic_latency_samples(&self) -> u32;
}
```

Gerar golden somente em máquina isolada com ativo aprovado. Registrar corpus, checksum, métrica, tolerância numérica e tolerância de qualidade. Sem ativo, o resultado correto é `BLOCKED_NO_APPROVED_ASSET`.

- [ ] **Step 4: Rodar GREEN e benchmark de qualificação**

Run: `cargo test -p realtime-noise-model && cargo run -p realtime-noise-tools --bin benchmark -- --backend tract --profile avx2-minimum --duration 300`

Expected: testes passam; benchmark produz `benchmarks/cpu-baseline.json`. Aprovar somente se p99 menor ou igual a 7 ms, deadline menor ou igual a 10 ms e alocações por hop medidas. Caso contrário, M1 falha.

- [ ] **Step 5: Congelar evidência e commit**

Run: `git add crates/model crates/tools fixtures/golden benchmarks Cargo.toml && git commit -m "feat: add gated CPU reference backend and golden harness"`

### Task 5: Onda 2, engine, filas, gerações e política sem raw bypass

**Files:**
- Create: `crates/engine/{Cargo.toml,src/{lib,engine,queue,worker,generation}.rs,tests/{deadline,backpressure,generation}.rs}`
- Modify: `Cargo.toml`

**Interfaces:**
- Produces: `DenoiseEngine::{start,stop,set_mode,set_backend,begin_generation_restart,status}`, `EngineStatus`, `EngineError`, `DenoiseMode`.
- Consumes: `RealtimeTransport`, `InferenceBackend`; `Bypass` não invoca `process`, `Mute` e qualquer falha emitem silêncio.

- [ ] **Step 1: Criar e registrar `realtime-noise-engine`, depois escrever RED por comportamento de falha**

```rust
#[test]
fn inference_deadline_miss_closes_generation_and_outputs_silence() { /* assert flag and silence */ }
#[test]
fn bypass_keeps_framing_without_calling_inference_or_raw_fallback() { /* assert endpoint frame contract */ }
```

- [ ] **Step 2: Rodar RED**

Run: `cargo test -p realtime-noise-engine`

Expected: falha porque worker e estados não existem. `crates/engine/Cargo.toml` e a entrada `crates/engine` em `workspace.members` já existem.

- [ ] **Step 3: Implementar worker fora de callback**

```rust
fn begin_generation_restart(&mut self, reason: ResetReason) -> Result<u64, EngineError> {
    self.schedule_warmed_replacement(reason)
}
```

Usar capacidade 24, watermark 20 ms ou 2 hops, descartar somente o novo frame cheio ou lado consumidor permitido, e trocar backend em fronteira de hop. Reset padrão sempre reconstrói e aquece off thread.

- [ ] **Step 4: Rodar GREEN e stress**

Run: `cargo test -p realtime-noise-engine && cargo test -p realtime-noise-engine --test backpressure -- --nocapture`

Expected: flags, contadores, silêncio e ownership corretos. Não declarar zero alocação sem relatório de profiling da Task 4.

- [ ] **Step 5: Commit**

Run: `git add crates/engine && git commit -m "feat: implement engine generation and backpressure policy"`

### Task 6: Onda 2, framing, resampling e formatos externos

**Files:**
- Create: `crates/format-adapter/{Cargo.toml,src/{lib,input_accumulator,output_deframer,resampler,endpoint_converter}.rs,tests/{framing,resampling,formats}.rs}`
- Modify: `Cargo.toml`

**Interfaces:**
- Produces: `InputAccumulator`, `OutputDeframer`, `FormatAdapterWorker`, `EndpointFormatConverter`.
- Consumes: frames nativos em fila bounded e produz `AudioFrame` 48 kHz mono; callbacks apenas copiam buffers nativos.

- [ ] **Step 1: Criar e registrar `realtime-noise-format-adapter`, depois escrever RED de tamanhos arbitrários e formato inválido**

```rust
#[test]
fn accumulator_emits_one_480_sample_frame_from_irregular_callbacks() { /* 127 + 353 */ }
#[test]
fn unsupported_endpoint_format_fails_instead_of_converting_implicitly() { /* expect error */ }
```

- [ ] **Step 2: Rodar RED**

Run: `cargo test -p realtime-noise-format-adapter`

Expected: falha pela ausência dos adaptadores. `crates/format-adapter/Cargo.toml` e a entrada `crates/format-adapter` em `workspace.members` já existem.

- [ ] **Step 3: Implementar cópia bounded fora do callback**

```rust
pub struct EndpointFormatConverter { /* records input format and group delay */ }
```

Conversão para PCM16 apenas no endpoint Windows e conversão CoreAudio/PipeWire apenas quando o servidor exigir. Aceitar taxa externa só após preservar golden M1 e backlog, senão retornar dispositivo incompatível.

- [ ] **Step 4: Rodar GREEN**

Run: `cargo test -p realtime-noise-format-adapter`

Expected: framing, silence deframing e falha fechada de formato passam.

- [ ] **Step 5: Commit**

Run: `git add crates/format-adapter && git commit -m "feat: add bounded framing and format adapters"`

### Task 7: Onda 2, IPC local e EngineSupervisor

**Files:**
- Create: `crates/{ipc,supervisor}/{Cargo.toml,src/{lib,protocol,server}.rs,tests/{protocol,supervision}.rs}`, `crates/service/{Cargo.toml,src/{main,bootstrap,install}.rs,tests/service_lifecycle.rs}`, `platform/windows/service/{register-user-service.ps1,unregister-user-service.ps1}`, `packaging/macos/LaunchAgents/com.clearcore.realtime-noise.plist`, `packaging/linux/systemd/user/realtime-noise.service`, `docs/{ipc-v1.md,service-lifecycle.md}`
- Modify: `Cargo.toml`

**Interfaces:**
- Produces: `realtime-noise.v1`, mensagens `{version, request_id, command, payload}`, `EngineUnavailable`, `Restarting`, `TerminalSafeState` e o executável `realtime-noise-service`.
- Consumes: `realtime-noise-engine`, supervisor e IPC. `src/main.rs` é o único host contínuo do supervisor/control plane: expõe IPC versionado, inicia e monitora engine, aplica backoff 1/2/4/8/16 s e preserva silêncio em estado terminal. A UI nunca o hospeda.
- Instalação: Windows registra tarefa por usuário no logon com `register-user-service.ps1`, não um serviço global, preservando owner lock por sessão; macOS instala LaunchAgent por usuário; Linux instala `systemd --user`. Cada unidade executa `realtime-noise-service --run`, recebe apenas IPC do mesmo usuário e não compartilha PCM entre sessões.

- [ ] **Step 1: Criar e registrar `realtime-noise-ipc`, `realtime-noise-supervisor` e o binário `realtime-noise-service`, depois escrever RED**

```rust
#[test]
fn incompatible_ipc_version_is_rejected_closed() { /* expect VersionMismatch */ }
#[test]
fn sixth_crash_in_fifteen_minutes_enters_terminal_safe_state() { /* assert silence state */ }

#[test]
fn service_survives_control_client_disconnect_and_serves_next_client() { /* spawn --run, disconnect, reconnect */ }
```

- [ ] **Step 2: Rodar RED**

Run: `cargo test -p realtime-noise-ipc -p realtime-noise-supervisor -p realtime-noise-service`

Expected: falha porque protocolo, supervisor e lifecycle do serviço não existem. Os manifests e registros `crates/ipc`, `crates/supervisor` e `crates/service` em `workspace.members` já existem; `crates/service/Cargo.toml` declara `name = "realtime-noise-service"` e `src/main.rs` como binário.

- [ ] **Step 3: Implementar protocolo e backoff normativo**

```rust
const BACKOFF_SECONDS: [u64; 5] = [1, 2, 4, 8, 16];
const MAX_CRASHES_PER_15_MINUTES: usize = 5;
```

Implementar `src/main.rs` com comandos `--run`, `--install-user-service` e `--uninstall-user-service`. `--run` é o loop contínuo do `EngineSupervisor`; a UI somente conecta ao socket/XPC existente e envia comandos tipados. `--install-user-service` instala a unidade apropriada e a inicia, sem fazer a UI responsável pelo processo. Validar schema e proprietário antes de executar comando. Em estado terminal, preservar diagnóstico, manter endpoint silencioso e exigir ação explícita do usuário.

- [ ] **Step 4: Rodar GREEN**

Run: `cargo test -p realtime-noise-ipc -p realtime-noise-supervisor -p realtime-noise-service && cargo build -p realtime-noise-service --release`

Expected: versão inválida falha, sequência de crashes produz estados, o binário `target/release/realtime-noise-service` é produzido e o teste demonstra que desconectar a UI não encerra o serviço. Nenhum teste contém PCM no payload.

- [ ] **Step 5: Commit**

Run: `git add Cargo.toml crates/ipc crates/supervisor crates/service platform/windows/service packaging/macos/LaunchAgents packaging/linux/systemd/user docs/ipc-v1.md docs/service-lifecycle.md && git commit -m "feat: add supervised user service and local control IPC"`

### Task 8: Onda 3, spike Windows PortCls/WaveRT

**Parallel:** Sim, com Tasks 9 e 10. **Depends on:** Tasks 3, 4 e 7.

**Files:**
- Create: `platform/windows/driver/{RealtimeNoise.inf,driver.vcxproj,src/{Driver,WaveRtMiniport,IoctlTransport,WireFrameEnvelopeV1}.cpp}`, `platform/windows/tests/{endpoint-spike.ps1,driver_verifier.ps1,hlk-checklist.md}`, `docs/evidence/windows-spike.md`

**Interfaces:**
- Produces: IOCTL sobreposto com direct I/O, validação de owner, geração e `WireFrameEnvelopeV1`, endpoint WaveRT silencioso sem engine.
- Consumes: wire fixture e contrato de sessão; ACX não é usado sem gate aprovado.

- [ ] **Step 1: Escrever teste RED de validação de envelope em host WDK**

```cpp
TEST(WireIoctl, RejectsOtherSessionAndInvalidEnvelope) { /* expect STATUS_ACCESS_DENIED or STATUS_INVALID_BUFFER_SIZE */ }
```

- [ ] **Step 2: Rodar RED em host Windows real**

Run: `powershell -File platform/windows/tests/endpoint-spike.ps1 -Phase Red`

Expected: falha por driver não instalado. Registrar versão WDK, Windows, hardware e log no relatório, nunca simular este resultado no Linux.

- [ ] **Step 3: Implementar spike mínimo derivado de SysVAD**

```cpp
// DPC/driver validates and moves buffers only; it never invokes Rust or inference.
NTSTATUS IoctlTransport::SubmitEnvelope(const WireFrameEnvelopeV1* envelope, size_t bytes);
```

Negociar exclusivamente mono 48 kHz PCM16/Float32. Segunda sessão recebe silêncio ou falha, conforme resultado medido, e UI deve representar `UnavailableBusy`.

- [ ] **Step 4: Rodar GREEN e gates físicos**

Run: `powershell -File platform/windows/tests/endpoint-spike.ps1 -Phase Green; powershell -File platform/windows/tests/driver_verifier.ps1`

Expected: endpoint enumerável, IOCTL cancelável, silêncio sem engine, formato e fast user switching observados. HLK, suspensão, assinatura e desinstalação ainda ficam `BLOCKED_PENDING_HLK` até executados em hardware Windows.

- [ ] **Step 5: Anexar evidência e commit**

Run: `git add platform/windows docs/evidence/windows-spike.md && git commit -m "spike: validate Windows WaveRT transport"`

### Task 9: Onda 3, spike macOS HAL Audio Server Plug in

**Parallel:** Sim, com Tasks 8 e 10. **Depends on:** Tasks 3, 4 e 7.

**Files:**
- Create: `platform/macos/HAL/{RealtimeNoiseHAL.xcodeproj, Sources/{Plugin.swift,HiddenOutput.swift,VisibleInput.swift,RingBuffer.swift}}`, `platform/macos/Bridge/{Package.swift,Sources/RealtimeNoiseBridge/EngineXpc.swift}`, `platform/macos/tests/endpoint-spike.sh`, `docs/evidence/macos-spike.md`

**Interfaces:**
- Produces: endpoint input visível Float32 48 kHz, endpoint output oculto, ring local de sequência/geração e XPC fora de callback.
- Consumes: engine escreve PCM no output oculto; HAL nunca transporta envelopes nem faz RPC síncrono.

- [ ] **Step 1: Escrever RED no host macOS**

```swift
func testVisibleInputReadsSilenceUntilWarmedGeneration() throws { /* assert zeroes */ }
```

- [ ] **Step 2: Rodar RED em Apple Silicon real**

Run: `platform/macos/tests/endpoint-spike.sh red`

Expected: falha sem plugin instalado. Registrar macOS, Xcode e hardware em `docs/evidence/macos-spike.md`.

- [ ] **Step 3: Implementar plugin mínimo**

```swift
final class RingBuffer { /* atomic clear on control update; callback only reads/writes bounded samples */ }
```

Implementar owner lock, endpoint visível à máquina e `UnavailableBusy` para sessão não proprietária. Não substituir pelo AudioDriverKit sem prova formal de paridade.

- [ ] **Step 4: Rodar GREEN físico**

Run: `platform/macos/tests/endpoint-spike.sh green`

Expected: captura, input visível, hidden output, silêncio em falha, fast user switching e suspensão têm resultados documentados. Notarização fica `BLOCKED_PENDING_DEVELOPER_ID` até credencial e submissão reais.

- [ ] **Step 5: Anexar evidência e commit**

Run: `git add platform/macos docs/evidence/macos-spike.md && git commit -m "spike: validate macOS HAL endpoint"`

### Task 10: Onda 3, spike Linux PipeWire nativo

**Parallel:** Sim, com Tasks 8 e 9. **Depends on:** Tasks 3, 4 e 7.

**Files:**
- Create: `platform/linux/helper/{meson.build,src/{pipewire_helper.c,transport_bridge.c,format_converter.c}}`, `platform/linux/tests/{endpoint-spike.sh,rebind-check.sh}`, `docs/evidence/linux-spike.md`

**Interfaces:**
- Produces: fonte PipeWire por usuário, F32LE mono 48 kHz via `pw_filter` ou `pw_stream`, compatível com `pipewire-pulse`.
- Consumes: callback somente transfere buffers para/de transport bounded; DSP permanece no worker Rust.

- [ ] **Step 1: Escrever RED do contrato callback**

```c
void test_process_callback_does_not_call_inference_or_allocate(void) { /* instrumentation asserts transfer only */ }
```

- [ ] **Step 2: Rodar RED em Ubuntu 24.04 e Fedora 42 reais**

Run: `platform/linux/tests/endpoint-spike.sh red`

Expected: falha antes de criar o helper. Registrar PipeWire, WirePlumber, kernel e logs por distribuição.

- [ ] **Step 3: Implementar helper nativo mínimo**

```c
static void on_process(void *userdata) { transfer_bounded_buffers(userdata); }
```

Não incluir filter-chain, LADSPA, host de plugin ou fallback para microfone cru. Disputa pelo mesmo dispositivo retorna `UnavailableBusy`; helper continua supervisionável enquanto engine reinicia.

- [ ] **Step 4: Rodar GREEN e rebind real**

Run: `platform/linux/tests/endpoint-spike.sh green && platform/linux/tests/rebind-check.sh`

Expected: consumidor `pipewire-pulse` observa fonte; após recriação o consumidor precisa rebind e recebe áudio somente após geração válida. Falha em qualquer distribuição bloqueia M2 Linux.

- [ ] **Step 5: Anexar evidência e commit**

Run: `git add platform/linux docs/evidence/linux-spike.md && git commit -m "spike: validate native PipeWire endpoint"`

### Task 11: Onda 4, adaptador Windows de produção

**Depends on:** Task 8 aprovada. **Files:**
- Modify: `Cargo.toml`, `platform/windows/driver/src/{WaveRtMiniport,IoctlTransport}.cpp`
- Create: `platform/windows/host/{Cargo.toml,src/{lib,wasapi,endpoint}.rs,tests/{hotplug,session,endpoint_formats}.rs}`, `docs/evidence/windows-integration.md`

**Interfaces:**
- Produces: `AudioBackend` WASAPI e `VirtualMicrophone` WaveRT com IOCTL direct I/O.
- Consumes: engine output e `WireFrameEnvelopeV1`; driver publica silêncio para `EngineUnavailable`, `Restarting` e `TerminalSafeState`.

- [ ] **Step 1: Criar e registrar `realtime-noise-windows-host`, depois escrever RED de hot plug e sessão**

```rust
#[test]
fn selected_device_loss_enters_waiting_without_selecting_another_mic() { /* assert WaitingForDevice */ }
```
- [ ] **Step 2: Rodar RED no host Windows**

Run: `cargo test -p realtime-noise-windows-host --test hotplug`

Expected: falha antes da integração WASAPI. `platform/windows/host/Cargo.toml` e a entrada `platform/windows/host` em `workspace.members` já existem.
- [ ] **Step 3: Implementar adaptador e conversor de endpoint**

```rust
impl VirtualMicrophone for WindowsVirtualMicrophone { /* start consumes output transport only */ }
```
- [ ] **Step 4: Rodar GREEN e compatibilidade de aplicações reais**

Run: `cargo test -p realtime-noise-windows-host && powershell -File platform/windows/tests/endpoint-spike.ps1 -Phase Integration`

Expected: Teams, Zoom, Discord, OBS Studio e cliente WebRTC selecionam o endpoint e gravam a saída processada. Mocks não contam como evidência; cada app gera log, versão e captura de seleção.
- [ ] **Step 5: Commit**

Run: `git add platform/windows docs/evidence/windows-integration.md && git commit -m "feat: integrate Windows virtual microphone"`

### Task 12: Onda 4, adaptador macOS de produção

**Depends on:** Task 9 aprovada. **Files:**
- Modify: `platform/macos/HAL/Sources/{HiddenOutput,VisibleInput,RingBuffer}.swift`, `platform/macos/Bridge/Sources/RealtimeNoiseBridge/EngineXpc.swift`
- Create: `platform/macos/tests/{hotplug,session,endpoint_formats}.swift`, `docs/evidence/macos-integration.md`

**Interfaces:**
- Produces: `AudioBackend` CoreAudio e `VirtualMicrophone` HAL integrados ao bridge XPC.
- Consumes: somente PCM processado no output oculto, geração e owner lock.

- [ ] **Step 1: Escrever RED de atualização atômica do ring**

```swift
func testControlGenerationChangeClearsRingAndVisibleInputOutputsSilence() throws { /* assert zeros */ }
```
- [ ] **Step 2: Rodar RED no host macOS**

Run: `xcodebuild test -project platform/macos/HAL/RealtimeNoiseHAL.xcodeproj -scheme RealtimeNoiseHALTests`

Expected: falha antes da integração.
- [ ] **Step 3: Implementar integração sem RPC em callback**

```swift
func readInput(_ frameCount: Int) -> UnsafeBufferPointer<Float> { ring.readOrSilence(frameCount) }
```
- [ ] **Step 4: Rodar GREEN e aplicações reais**

Run: `xcodebuild test -project platform/macos/HAL/RealtimeNoiseHAL.xcodeproj -scheme RealtimeNoiseHALTests && platform/macos/tests/endpoint-spike.sh integration`

Expected: Teams, Zoom, Discord, OBS Studio e WebRTC selecionam o input HAL e só recebem saída processada. Evidência inclui versões, screenshots e logs locais.
- [ ] **Step 5: Commit**

Run: `git add platform/macos docs/evidence/macos-integration.md && git commit -m "feat: integrate macOS virtual microphone"`

### Task 13: Onda 4, adaptador Linux de produção

**Depends on:** Task 10 aprovada. **Files:**
- Modify: `Cargo.toml`, `platform/linux/helper/src/{pipewire_helper,transport_bridge,format_converter}.c`
- Create: `platform/linux/host/{Cargo.toml,src/{lib,pipewire,endpoint}.rs,tests/{hotplug,session,rebind}.rs}`, `docs/evidence/linux-integration.md`

**Interfaces:**
- Produces: `AudioBackend` e `VirtualMicrophone` PipeWire por usuário.
- Consumes: transport output e estados do supervisor; quando helper recria nó, exige rebind.

- [ ] **Step 1: Criar e registrar `realtime-noise-linux-host`, depois escrever RED de rebind e disputa de dispositivo**

```rust
#[test]
fn recreated_source_requires_rebind_before_audio_is_observed() { /* assert silence then valid generation */ }
```
- [ ] **Step 2: Rodar RED em Ubuntu e Fedora**

Run: `cargo test -p realtime-noise-linux-host --test rebind`

Expected: falha antes da integração do host PipeWire. `platform/linux/host/Cargo.toml` e a entrada `platform/linux/host` em `workspace.members` já existem.
- [ ] **Step 3: Implementar adaptador de produção**

```rust
impl VirtualMicrophone for LinuxVirtualMicrophone { /* supervises per-user helper, never DSP */ }
```
- [ ] **Step 4: Rodar GREEN e aplicações reais nas duas distribuições**

Run: `cargo test -p realtime-noise-linux-host && platform/linux/tests/rebind-check.sh --integration`

Expected: Teams, Zoom, Discord, OBS Studio, WebRTC e um consumidor `pipewire-pulse` selecionam a fonte. Rebind após recriação é observado em cada aplicação; mocks não aprovam endpoint.
- [ ] **Step 5: Commit**

Run: `git add platform/linux docs/evidence/linux-integration.md && git commit -m "feat: integrate Linux virtual microphone"`

### Task 14: Onda 5, UI Tauri, tray e diagnósticos consentidos

**Depends on:** Tasks 5 e 7. **Files:**
- Create: `crates/app-tauri/{package.json,package-lock.json,vite.config.ts,tsconfig.json,index.html,src/{main.tsx,App.tsx,diagnostics.tsx,styles.css},scripts/prepare-sidecar.mjs,src-tauri/{Cargo.toml,build.rs,tauri.conf.json,src/{main.rs,commands.rs},capabilities/default.json,binaries/README.md}}`, `crates/diagnostics/{Cargo.toml,src/{lib,export,privacy}.rs,tests/privacy.rs}`, `docs/diagnostics-schema.md`
- Modify: `Cargo.toml`

**Interfaces:**
- Consumes: IPC versionado e `EngineStatus`.
- Produces: controle de modo/backend/dispositivo, tray, exportação explícita de diagnóstico e nenhuma sessão de áudio na UI. O frontend usa npm com `package-lock.json`; o backend Tauri é o package `realtime-noise-app-tauri` em `src-tauri/Cargo.toml`, registrado em `workspace.members` como `crates/app-tauri/src-tauri` antes de qualquer build Rust.

- [ ] **Step 1: Criar manifests, lockfiles e layout completo antes de testes ou comandos Tauri**

Criar `crates/diagnostics/Cargo.toml`, registrar `crates/diagnostics`, criar `crates/app-tauri/src-tauri/Cargo.toml`, registrar `crates/app-tauri/src-tauri`, e criar todos os arquivos frontend, backend e scripts listados acima. Não criar `crates/app-tauri/Cargo.toml`: o único manifest Rust Tauri é `crates/app-tauri/src-tauri/Cargo.toml`.

```json
// crates/app-tauri/package.json
{
  "packageManager": "npm@10.9.2",
  "scripts": {
    "dev": "vite --host 127.0.0.1",
    "build": "tsc --noEmit && vite build",
    "test": "vitest run",
    "prepare:sidecar": "node scripts/prepare-sidecar.mjs",
    "test:smoke": "npm run build && npm run prepare:sidecar && npm run tauri build -- --no-bundle",
    "tauri": "tauri"
  },
  "dependencies": { "@tauri-apps/api": "2.0.0", "@tauri-apps/plugin-shell": "2.0.0", "react": "18.3.1", "react-dom": "18.3.1" },
  "devDependencies": { "@tauri-apps/cli": "2.0.0", "@types/react": "18.3.12", "@types/react-dom": "18.3.1", "@vitejs/plugin-react": "4.3.4", "typescript": "5.6.3", "vite": "5.4.10", "vitest": "2.1.4" }
}
```

```toml
# crates/app-tauri/src-tauri/Cargo.toml
[package]
name = "realtime-noise-app-tauri"
version = "0.1.0"
edition = "2024"
build = "build.rs"

[build-dependencies]
tauri-build = "2.0.0"

[dependencies]
realtime-noise-ipc = { path = "../../ipc" }
tauri = "2.0.0"
tauri-plugin-shell = "2.0.0"
```

```json
// crates/app-tauri/src-tauri/tauri.conf.json
{
  "productName": "Realtime Noise",
  "version": "0.1.0",
  "identifier": "com.clearcore.realtime-noise",
  "build": { "beforeDevCommand": "npm run dev", "beforeBuildCommand": "npm run build", "devUrl": "http://127.0.0.1:5173", "frontendDist": "../dist" },
  "app": { "windows": [{ "title": "Realtime Noise", "width": 960, "height": 640 }] },
  "bundle": { "externalBin": ["binaries/realtime-noise-service"] }
}
```

`prepare-sidecar.mjs` deve executar `cargo build -p realtime-noise-service --release` a partir da raiz e copiar o binário para `src-tauri/binaries/realtime-noise-service-<target-triple>` antes de Tauri iniciar ou empacotar. `binaries/README.md` fixa a convenção de sufixo target triple. A configuração `externalBin` apenas empacota o executável de serviço; a janela UI não inicia nem mantém seu processo contínuo. No primeiro uso e no instalador, chamar `realtime-noise-service --install-user-service`; após o registro, a UI conecta ao IPC do serviço já iniciado e só pode solicitar operações de controle. Executar, nesta ordem e com `cwd` explícito: `cd crates/app-tauri && npm install --package-lock-only --offline --ignore-scripts`, depois `cd crates/app-tauri && npm ci --offline --ignore-scripts`; o segundo comando usa o `package-lock.json` criado pelo primeiro e não é interativo. Cache npm ausente gera `BLOCKED_OFFLINE_DEPENDENCY`, sem download alternativo no fluxo de release.

**Interfaces de layout:** `src/main.tsx` monta `App.tsx`; `src/diagnostics.tsx` chama somente comandos Tauri; `src-tauri/src/commands.rs` chama `realtime-noise-ipc` e testa disponibilidade do serviço; `src-tauri/src/main.rs` registra tray e commands; `externalBin` empacota `realtime-noise-service`, cuja unidade registrada possui ciclo de vida próprio. Nunca fazer processamento, loop de supervisor ou ownership de serviço na UI.

- [ ] **Step 2: Escrever RED de privacidade e sobrevivência da UI**

```rust
#[test]
fn diagnostic_export_excludes_pcm_embeddings_and_meeting_names() { /* inspect archive */ }
```
- [ ] **Step 3: Rodar RED**

Run: `cargo test -p realtime-noise-diagnostics`

Expected: falha antes do exportador existir. `crates/diagnostics/Cargo.toml`, `crates/app-tauri/src-tauri/Cargo.toml`, seus registros no workspace, `package.json` e `package-lock.json` já existem.
- [ ] **Step 4: Implementar UI como cliente de controle**

```rust
#[tauri::command]
fn set_mode(request: SetModeRequest) -> Result<EngineStatus, CommandError> { ipc_client().set_mode(request) }
```

Exibir causas, última tentativa, métricas p50/p95/p99, geração e parcelas medida/configurada/derivada/não observável. IDs de dispositivo usam hash com salt por instalação.
- [ ] **Step 5: Rodar GREEN automatizado e QA manual interativo**

Run (automação, cwd: raiz do repositório): `cargo test -p realtime-noise-diagnostics && cd crates/app-tauri && npm run test && npm run test:smoke`

Run (QA manual, cwd: `crates/app-tauri`): `npm run tauri dev`

Expected: a automação termina sem UI interativa e valida bundle sem instalador; o comando manual abre a UI. Fechar a janela e reabrir mostra engine ainda em execução. Se o processo de áudio morrer junto, registrar `BLOCKED_UI_OWNS_AUDIO` e corrigir antes de seguir.
- [ ] **Step 6: Commit**

Run: `git add Cargo.toml Cargo.lock crates/app-tauri crates/diagnostics docs/diagnostics-schema.md && git commit -m "feat: add control UI and private diagnostics"`

### Task 15: Onda 6, plugins de acelerador e política AUTO

**Parallel:** plugins OpenVINO, CUDA e CoreML podem ser implementados em paralelo. **Depends on:** Task 4 M1 aprovado e Task 5.

**Files:**
- Create: `crates/accelerators/{Cargo.toml,src/{lib,auto,openvino,cuda,coreml}.rs,tests/{auto,qualification}.rs}`, `crates/tools/src/bin/calibrate-backend.rs`, `docs/accelerator-qualification.md`
- Modify: `Cargo.toml`

**Interfaces:**
- Produces: `OpenVINOBackend`, `CudaBackend`, `CoreMlBackend`, todos `InferenceBackend`; `BackendRequest::Auto`.
- Consumes: golden M1 congelado, CPU baseline aquecido e thresholds de p99, qualidade, estabilidade e consumo.

- [ ] **Step 1: Criar e registrar `realtime-noise-accelerators`, depois escrever RED de AUTO conservador**

```rust
#[test]
fn auto_uses_warmed_tract_when_plugin_fails_quality_gate() { /* assert tract descriptor */ }
```
- [ ] **Step 2: Rodar RED**

Run: `cargo test -p realtime-noise-accelerators`

Expected: falha porque política e plugins não existem. `crates/accelerators/Cargo.toml` e a entrada `crates/accelerators` em `workspace.members` já existem; `realtime-noise-tools` já está registrado desde a Task 2.
- [ ] **Step 3: Implementar carga off-thread e seleção após calibração**

```rust
fn select_auto(report: CalibrationReport) -> BackendSelection { /* only frozen thresholds */ }
```

Calibração local dura 5 minutos fora da cadeia ativa. Falha seleciona `tract`; falha em runtime usa CPU apenas se já aquecida, senão silêncio e nova geração.
- [ ] **Step 4: Rodar GREEN e gate de hardware aplicável**

Run: `cargo test -p realtime-noise-accelerators && cargo run -p realtime-noise-tools --bin calibrate-backend -- --duration-minutes 5`

Expected: testes passam. Sem dispositivo compatível ou com métricas reprovadas, relatório declara `NOT_PROMOTED` e o plugin não entra no pacote GA.
- [ ] **Step 5: Commit**

Run: `git add crates/accelerators tools docs/accelerator-qualification.md && git commit -m "feat: add gated accelerator plugins and auto policy"`

### Task 16: Onda 7, pacotes, assinatura, update, uninstall, SBOM

**Depends on:** Tasks 7, 11, 12 e 13. **Files:**
- Create: `packaging/{windows/{msix,installer},macos/{pkg,notarize},linux/{deb,rpm,repo}}/`, `scripts/{build-sbom,scan-artifacts,qualify-ga,uninstall-smoke}.sh`, `sbom/.gitkeep`, `.github/workflows/{release,windows-hardware,macos-hardware,linux-hardware}.yml`, `docs/{support-matrix,uninstall,release-gates}.md`

**Interfaces:**
- Produces: artefatos assinados, SBOM SPDX ou CycloneDX, update transacional e remoção que toca somente componentes próprios.
- Consumes: manifesto de ativo aprovado, `THIRD_PARTY_LICENSES`, o binário `realtime-noise-service`, binários de endpoint e evidência por plataforma.

- [ ] **Step 1: Escrever RED de scanner e uninstall**

```bash
scripts/scan-artifacts.sh --artifact test-package
# expected: BLOCKED_UNLISTED_ASSET
```

- [ ] **Step 2: Rodar RED**

Run: `scripts/build-sbom.sh --check && scripts/uninstall-smoke.sh --dry-run`

Expected: falha enquanto não há pacote e lista explícita de ownership.

- [ ] **Step 3: Implementar receitas de pacote sem credenciais embutidas**

Windows atualiza driver e componente de usuário transacionalmente, registrando `realtime-noise-service --install-user-service` no logon do proprietário. macOS monta `.pkg` Developer ID, notarização e stapling, instalando somente o LaunchAgent do usuário. Linux cria pacotes e repositório assinado, instalando `systemd --user` para `realtime-noise-service --run`. A remoção para helper, unidade e regras próprias sem alterar PipeWire/WirePlumber e sem encerrar sessões de outros usuários.

- [ ] **Step 4: Rodar GREEN ou registrar bloqueio de credencial externa**

Run: `scripts/build-sbom.sh --artifact-dir dist && scripts/scan-artifacts.sh --artifact-dir dist && scripts/uninstall-smoke.sh --artifact-dir dist`

Expected: SBOM e scanner limpos com ativo listado. Assinatura Windows, notarização Apple e repositório Linux devem ser executados nos hosts e contas reais. Sem credencial, registrar `BLOCKED_SIGNING_CREDENTIAL` e bloquear M4/M6.

- [ ] **Step 5: Commit**

Run: `git add packaging scripts sbom .github docs && git commit -m "build: add signed packaging and release evidence gates"`

### Task 17: Onda 8, matriz de qualificação simultânea

**Depends on:** Tasks 1 a 14, Task 16 e baseline CPU M1 aprovado. Task 15 é opcional, não é pré-requisito e não pode bloquear esta task. **Files:**
- Create: `crates/qualification/tests/ga_decision.rs`, `tools/qualification/{collect_metrics.rs,run_matrix.py}`, `docs/evidence/{ga-matrix.md,windows.md,macos.md,ubuntu.md,fedora.md}`, `docs/ga-decision.md`
- Modify: `crates/qualification/src/lib.rs`

**Interfaces:**
- Consumes: endpoints reais, métricas locais, golden M1, artefatos assinados e relatórios Driver Verifier/HLK/notarização.
- Produces: decisão única `GA_APPROVED` ou `GA_BLOCKED`, nunca aprovação parcial.

- [ ] **Step 1: Escrever RED no package `realtime-noise-qualification` já registrado**

```rust
#[test]
fn one_platform_failure_blocks_simultaneous_ga() {
    assert_eq!(aggregate([Pass, Pass, Fail, Pass]), GaDecision::Blocked);
}
```
- [ ] **Step 2: Rodar RED**

Run: `cargo test -p realtime-noise-qualification one_platform_failure_blocks_simultaneous_ga`

Expected: falha porque agregador ainda não existe. `crates/qualification/Cargo.toml`, `src/lib.rs` e o registro no workspace existem desde a Task 2.
- [ ] **Step 3: Implementar coletor e matriz obrigatória**

```text
Windows 11: Driver Verifier, HLK aplicável, install/update/remove, suspend, hot-plug, fast user switching, 8h soak, Teams/Zoom/Discord/OBS/WebRTC
macOS 13+: signed/notarized/stapled HAL, install/update/remove, suspend, fast user switching, 8h soak, mesmas aplicações
Ubuntu 24.04 e Fedora 42: PipeWire/WirePlumber, rebind, install/update/remove, 8h soak, mesmas aplicações e pipewire-pulse
```

Cada linha requer p95 produto menor ou igual a 80 ms, p99 CPU menor ou igual a 7 ms quando baseline aplicável, zero eventos de soak e resultado de seleção de endpoint por aplicação real.
- [ ] **Step 4: Rodar GREEN nos quatro ambientes físicos**

Run: `python3 tools/qualification/run_matrix.py --hosts windows11,macos13,ubuntu2404,fedora42 --soak-hours 8`

Expected: produz quatro relatórios assináveis e `docs/ga-decision.md`. Qualquer host indisponível, app não selecionando endpoint, falha de rebind, assinatura ausente ou métrica reprovada produz `GA_BLOCKED` para todos os SOs.
- [ ] **Step 5: Gate final, commit de evidência e proibição de tag se bloqueado**

Run: `cargo test -p realtime-noise-qualification && scripts/qualify-ga.sh --evidence docs/evidence/ga-matrix.md`

Expected: `GA_APPROVED` somente com todos os gates verdes. Se `GA_BLOCKED`, não criar tag, não publicar pacote e abrir remediação pela plataforma reprovada.

## Gates de Dependência Obrigatórios

1. M0: Task 1 deve produzir `M0_APPROVED` para ativo aprovado. Sem isso, Tasks 2 a 17 não começam.
2. M1: Tasks 3 e 4 devem congelar wire ABI, golden e baseline. Sem isso, engine, AUTO e adaptadores não usam modelo nem tolerâncias.
3. M2: Tasks 8, 9 e 10 executam em paralelo, mas a produção de cada plataforma depende de seu próprio spike aprovado.
4. M3: Tasks 5, 6, 7 e 14 integram controle, recuperação e diagnóstico. UI não pode ser promovida se fechar encerrar áudio.
5. M4: Task 16 depende do serviço da Task 7, dos três adaptadores de produção e das credenciais reais de assinatura. Credencial ausente bloqueia release, não habilita pacote sem assinatura.
6. M5: Task 15 é uma linha opcional independente. Plugin reprovado fica fora de AUTO e do release, sem atrasar M6 nem o GA CPU.
7. M6: Task 17 exige baseline CPU M1, Tasks 1 a 14 e Task 16, com aprovação de todos os quatro ambientes normativos. Task 15 não entra na matriz de bloqueio. Não há GA Windows-only, macOS-only, Linux-only, beta promovida silenciosamente nem waiver de endpoint.

## Revisão Final do Plano

- Cobertura da especificação: as ondas 0 a 8 cobrem ativo e licenças, baseline CPU, wire ABI, filas, engine, supervisor, três endpoints nativos, UI, aceleradores, pacote e qualificação simultânea.
- Consistência: contratos são definidos antes de consumidores; spikes dependem de M1; adaptadores dependem do spike respectivo; GA depende de todos os sistemas e evidências físicas.
- Sem placeholders: todo gate tem padrão conservador e consequência explícita. Nenhum teste de seleção de endpoint pode ser satisfeito por mocks.

## Handoff de Execução

Plano completo e salvo em `docs/superpowers/plans/2026-09-23-realtime-noise-suppression-plan.md`. A execução recomendada é Subagent-Driven, com um implementador por task, revisão por task e revisão final da onda 8. A alternativa é execução inline com checkpoints entre ondas. Não iniciar implementação enquanto M0 não estiver aprovado.
