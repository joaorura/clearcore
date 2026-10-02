# Clearcore Studio — Fase 1b (integração do DSP nos dois caminhos de áudio) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ligar a cadeia `studio-dsp` (criada pelo plano da Fase 1a) aos dois caminhos de áudio do Clearcore (engine Rust e helper C do Linux via `filter-capi`) por um decorator `StudioBackend`, com `Off` por padrão, reset do estado do DSP a cada nova geração e o preset chegando ao helper C por uma extensão compatível do arquivo `clearcore_state`.

**Architecture:** `StudioBackend<B: InferenceBackend>` (crate `model`) embrulha o backend neural, roda `StudioChain` sobre o `ProcessedFrame`, lê o `Arc<StudioControl>` a cada frame e soma 96 amostras à latência; a trait `InferenceBackend` não muda. O engine embrulha o backend num único ponto (construtor/`set_backend`) e zera o DSP por um `StudioResetHandle` (flag atômica) sempre que fecha uma geração. O `filter-capi` embrulha o `TractBackend` e ganha `clearcore_filter_set_preset`; o helper C lê o preset de dentro do arquivo `clearcore_state` (sem mudar o tamanho de 16 bytes: o preset ocupa os 4 bytes altos do campo `generation`, hoje morto) e o Electron é o único escritor desse byte.

**Tech Stack:** Rust 1.90 (edition 2024, lints do workspace `unwrap/expect/panic = deny`, clippy pedantic+nursery com `-D warnings`), C (gnu11, meson/ninja, PipeWire) para o helper, Node 24 + Electron para o escritor do arquivo de estado. Nenhum crate nem pacote npm de terceiros novo; a única aresta nova é o path dependency `studio-dsp` em `crates/model`, `crates/engine` e `crates/filter-capi`.

**Spec:** `docs/superpowers/specs/2026-10-01-clearcore-studio-pipeline-design.md` (seção 5 "Integração nos dois caminhos", seção 9 "Ordem de entrega e execução", seção 10 "Riscos e pendências" — lacuna do preset no Linux). Esta é a **Fase 1b**; depende da **Fase 1a** (crate `studio-dsp`, plano à parte) e é pré-requisito do uso de `studio_control()` da Fase 2 (`docs/superpowers/plans/2026-10-01-clearcore-studio-phase2-settings-ui.md`).

## Global Constraints

Copiados verbatim do spec, mais as restrições do repositório lidas ao planejar:

- Spec §1: "O produto nunca fica pior do que é hoje: sem asset novo aprovado, roda DFNet3 v1 + DSP."
- Spec §3: "Em `Bypass` o áudio passa cru; em `Mute` sai zero; todo o resto só roda em `Active`."
- Spec §4: "`crates/studio-dsp`, Rust puro, **sem dependências externas**, `#![forbid(unsafe_code)]`, em conformidade com a política do workspace (lints `unwrap/expect/panic = deny`)."
- Spec §4: "Contrato: `StudioChain::new(Preset)`, `process(&mut self, &mut [f32; 480])` sem alocação e sem panic, `latency_samples()` (só o lookahead do limiter, ~96 amostras / 2 ms)."
- Spec §4: "Presets: `Off` (**passagem bit a bit idêntica**), `Natural`, `Podcast`, `Broadcast`, em tabela constante. [...] Padrão do produto: `Off`."
- Spec §4: "Segurança numérica: nunca produzir NaN/infinito (`ProcessedFrame::checked` rejeita); tratar denormais."
- Spec §5: "**Decorator** `StudioBackend<B: InferenceBackend>` implementa a mesma trait: `process` chama o backend interno e depois a cadeia; `algorithmic_latency_samples` soma a latência da cadeia. O engine e o `filter-capi` apenas embrulham o backend que já criam. A trait não muda."
- Spec §5: "Modos tratados antes do backend (conferir no helper C; ponto a verificar na fase 1)."
- Spec §5: "Reinício de geração do engine zera o estado do DSP."
- Spec §5: "Troca de preset por valor atômico compartilhado, aplicada no limite de frame com crossfade de ~10 ms. `filter-capi` ganha `clearcore_filter_set_preset`."
- Spec §5: "Fallback do helper C (biblioteca neural ausente): **sem estúdio**. Modo degradado documentado."
- Spec §9: "Cada fase só fecha com `cargo fmt --check`, `clippy -D warnings`, `cargo test --locked --offline` e `scripts/check-offline.sh` passando, mais revisão de código. Implementação e revisão por agentes em **Sonnet e Haiku** [...]. Execução autônoma: **sem commit e sem push** (não foram pedidos); cada tarefa termina com um checkpoint (`git status --short`), e as fases paralelas trabalham em arquivos disjuntos do mesmo working tree."
- Spec §10: "**Lacuna do preset no Linux** (achada ao planejar a fase 2): o modo que governa o helper C vive em `$XDG_RUNTIME_DIR/clearcore_state` (struct C de 16 bytes, sem campo de preset). Para o preset chegar ao áudio no Linux é preciso estender a struct, o helper C e o Electron junto com a fase 1."
- Spec §10: "A UI real do produto é **Electron** (o \"app Tauri\" é um CLI)".
- Política do workspace (`Cargo.toml`): `unsafe_code = "forbid"`; `clippy::all = deny`; `pedantic` e `nursery` em `warn` (viram erro com `-D warnings`); `unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented` em `deny`. Testes de integração usam `#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]` como os existentes. `crates/filter-capi` é a única exceção (não herda `[lints] workspace`, tem `unsafe_code = "allow"`).
- Política de dependências (`crates/workspace-policy`): path dependency é permitido; nenhum pacote do workspace pode declarar `default` features não vazias; nenhum crate de terceiros novo.
- Idioma: respostas e documentação em português do Brasil; identificadores e código em inglês; strings com acentuação correta.
- Comandos de gate (CI, `.github/workflows/ci.yml`): `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings`; `cargo build --workspace --locked --offline`; `cargo test --workspace --locked --offline`; `scripts/check-offline.sh`.
- **Sem commit e sem push.** Onde um plano normal comitaria, esta fase faz um "Checkpoint" (`git status --short`).

---

## Interfaces consumidas da Fase 1a (nomes exatos)

Este plano usa **exatamente** estas assinaturas do crate `studio-dsp` e não define nenhuma delas:

```rust
// crate studio-dsp (lib name studio_dsp)
pub const HOP_SAMPLES: usize = 480;
pub const SAMPLE_RATE_HZ: u32 = 48_000;
#[repr(u8)] pub enum Preset { Off = 0, Natural = 1, Podcast = 2, Broadcast = 3 }   // Clone, Copy, Debug, PartialEq, Eq
impl Preset { pub fn from_u8(v: u8) -> Option<Preset>; pub fn as_u8(self) -> u8; }
pub struct StudioChain;
impl StudioChain {
    pub fn new(preset: Preset) -> Self;
    pub fn set_preset(&mut self, preset: Preset);
    pub fn process(&mut self, frame: &mut [f32; 480]);
    pub fn latency_samples(&self) -> u32;          // = 96
    pub fn reset(&mut self);
}
pub struct StudioControl;                           // usado como Arc<StudioControl>
impl StudioControl {
    pub fn new(preset: Preset) -> Self;
    pub fn set_preset(&self, preset: Preset);
    pub fn preset(&self) -> Preset;
}
```

## Decisões de desenho (com a evidência lida no código)

1. **O engine nunca constrói um backend; quem constrói é o chamador.** `DenoiseEngine::new/with_mode/new_standalone` recebem um `Box<dyn InferenceBackend>` (`crates/engine/src/engine.rs:143-181`) e `set_backend` troca esse box (`engine.rs:235`). Não existe chamador de produção do engine no repositório (só testes em `crates/engine/tests` e o `crates/tools`), e o `crates/tools` (benchmark, `generate-golden`) usa `TractBackend` direto para medir e gerar golden, e **deve continuar assim** (o golden é do modelo cru, `ALGORITHM_LATENCY_SAMPLES = 1_440`). Portanto os "dois pontos" do spec (`worker.rs:17`, `engine.rs:272-317`) são dois pontos de *uso* (`process_frame` do worker e `process_frame` síncrono) de um único campo, `EngineSharedState::backend`. O embrulho entra no ponto de **entrada** desse campo: um novo `DenoiseEngine::with_studio(self, Arc<StudioControl>) -> Self` embrulha o backend e o pendente, e `set_backend` embrulha qualquer backend trocado depois. Sem `with_studio` o engine se comporta exatamente como hoje (os testes existentes não mudam).
2. **Reset do DSP numa geração nova: flag atômica, sem mudar a trait.** O engine só enxerga `Box<dyn InferenceBackend>`, então não alcança `StudioChain::reset`. `StudioBackend` expõe um `StudioResetHandle` (newtype sobre `Arc<AtomicBool>`, `request()`); o engine guarda o handle e o aciona nos 5 pontos em que troca `state.generation` (`worker.rs` 3 vezes, `engine.rs` `begin_generation_restart` e o ramo de falha de `process_frame`); o `StudioBackend` consome a flag no começo do próximo `process` e chama `chain.reset()`. O mesmo handle é reutilizado em todo backend que o engine embrulhar, então `set_backend` não perde a ligação.
3. **`impl InferenceBackend for Box<T>`** (T: `InferenceBackend + ?Sized`) em `crates/model/src/lib.rs`: o engine guarda `Box<dyn InferenceBackend>` e precisa passá-lo como `B` do `StudioBackend<B>`. Não altera a trait; só acrescenta uma implementação.
4. **A latência "reportada" é a do backend.** Hoje o engine não lê `algorithmic_latency_samples` em lugar nenhum (`grep` confirma: só o `model`, `accelerators` e testes). O número passa a incluir os 96 porque `StudioBackend::algorithmic_latency_samples()` e `ProcessedFrame::algorithmic_latency_samples` somam `chain.latency_samples()`. Os testes de golden/qualificação (`crates/model/tests`, `crates/accelerators/tests`) usam o backend interno sem embrulho e continuam com 1.440. Observação (confirmada no plano da Fase 1a, decisão 3 dela): `latency_samples()` retorna sempre 96, inclusive em `Off`, e `Off` é identidade **sem atraso** do sinal. Logo, no preset padrão (`Off`) a latência reportada fica 96 amostras (2 ms) **acima** da real. O plano soma incondicionalmente porque é o contrato ("o chamador decide se compensa"); somar só quando o preset não for `Off` é uma decisão de produto, não tomada aqui, e fica registrada como divergência no relatório final. O Passo 5 da Tarefa 2 continua detectando qualquer outra divergência de comportamento da 1a.
5. **Bypass e Mute já ficam fora do DSP no engine.** `worker.rs:75-101` e `engine.rs:285-297` decidem `Mute` (zeros) e `Bypass` (cópia) *antes* de tocar `backend`; o `StudioBackend` só roda em `Active`. A Tarefa 3 acrescenta testes que travam essa propriedade.
6. **Helper C: Bypass/Mute são decididos antes da função neural (confirmado).** `platform/linux/helper/src/pipewire_helper.c:182-205` lê `shared_state->mode`; `MUTE` zera, `BYPASS` sanitiza o frame cru e só o ramo `else` (Active) chama `neural_process_fn`. O fallback (`neural_process_fn` ausente ou `rc != 0`) cai em `noise_suppressor_process`, que **não passa pelo estúdio** (modo degradado do spec §5; um frame com `rc != 0` no meio de uma sessão ativa sai sem estúdio, e a cadeia continua com o estado do frame anterior, sem reset).
7. **Quem escreve o `clearcore_state` hoje.** Dois escritores: o **Electron** (`writeClearcoreSharedState` em `crates/app-tauri/electron/main.cjs:461`, que lê o arquivo, altera os campos e regrava o buffer inteiro de 16 bytes com `fs.writeFileSync`) e o próprio **helper C**, que cria o arquivo (`open(O_CREAT)`, `ftruncate`, `mmap`) e grava `mode` quando recebe `--mode` (`pipewire_helper.c:791-798`). O daemon Rust e o `crates/app-tauri/src-tauri` (CLI) **não** escrevem nada nele. O leitor é só o helper C (`mode` e `target_node_id`; o campo `generation` do arquivo nunca é lido).
8. **Extensão do arquivo `clearcore_state` compatível, sem mudar o tamanho.** A struct tem 16 bytes (`mode` u32 @0, `target_node_id` u32 @4, `generation` u64 @8) e **não tem padding**. Mas o Electron só grava 32 bits em `@8` (`buf.writeUInt32LE(..., 8)`) e o helper nunca lê o campo: os bytes `@12..@15` estão sempre zero e livres. Decisão: reinterpretar `generation` como `_Atomic uint32_t generation` (@8) + `_Atomic uint32_t preset` (@12). Layout em bytes idêntico, `sizeof == 16`. Alternativas descartadas, com o motivo: (a) crescer a struct para 24 bytes — o Electron regrava com `writeFileSync`, que **trunca** o arquivo; um Electron antigo reduziria o arquivo a 16 bytes por baixo de um helper novo com `mmap` de 24 e o acesso a `@16` daria SIGBUS; (b) codificar o preset nos bits altos de `mode` — um helper antigo compararia `mode == MUTE` com o valor poluído e passaria a tratar Mute como Active, vazando áudio do microfone, o pior defeito possível aqui. Matriz de compatibilidade da decisão adotada: helper antigo + Electron novo (preset ignorado, modo intacto); helper novo + Electron antigo (o Electron antigo copia os 16 bytes existentes e regrava, preservando `@12`; arquivo velho tem zeros, preset `Off`); helper novo + arquivo criado por helper antigo (`ftruncate(16)` não muda nada, `@12 = 0 = Off`). Preset fora de `0..=3` no arquivo é **ignorado** pelo helper (mantém o último aplicado).
9. **Quem escreve o quê (preset).** O valor autoritativo do preset é o `settings.json` do serviço (Fase 2: `ServiceDaemon::studio_control()`, `SetPreset`, `GetStatus.preset`). O **Electron** é o único escritor do byte `@12` do `clearcore_state`: ele copia o preset do serviço para o arquivo (a) na sincronização periódica de status (`pollDaemonStatus`, que já consulta `GetStatus` a cada ciclo) e (b) imediatamente depois de um `set_preset` bem-sucedido. O **helper C** é só leitor: lê `@12` a cada hop (um `atomic_load` relaxado, sem syscall), e chama `clearcore_filter_set_preset` apenas quando o valor mudou. O **daemon** continua sem tocar o arquivo. Dependência entre fases: a Fase 2 é que faz o `GetStatus` devolver `preset`; enquanto isso não existir, o campo vem ausente, o Electron não escreve nada e o helper roda em `Off` (comportamento de hoje). Ou seja, a Fase 1b é segura de executar antes da 2.
10. **Este sandbox não consegue compilar o helper C.** `meson setup` falha aqui com `Dependency "libpipewire-0.3" not found` (Fedora sem `pipewire-devel`; o diretório `platform/linux/helper/build` do repo foi gerado em outra máquina). Por isso toda a lógica nova do helper que não precisa de PipeWire (layout da struct, decodificação do preset, "mudou?") vai para um header novo `clearcore_state.h`, sem `#include` de PipeWire, testável com `gcc` puro; o `meson test` completo só roda onde `libpipewire-0.3` existir. A Tarefa 5 deixa os dois comandos e marca o segundo como não verificado neste ambiente.

## Divergências entre o spec e o código real (levantadas ao planejar)

- Spec §2/§5 falam em dois pontos de embrulho (`worker.rs:17`, `engine.rs:272-317`); no código são dois pontos de uso de um único campo, e quem constrói o backend é o chamador (decisão 1).
- Spec §5 diz "O engine e o `filter-capi` apenas embrulham o backend que já criam". Verdade para o `filter-capi` (`lib.rs:38`); para o engine, ele não cria nada e hoje não tem chamador de produção: o embrulho é uma API nova (`with_studio`) que o primeiro chamador real vai usar.
- Spec §5 "Reinício de geração do engine zera o estado do DSP": a trait não deixa o engine alcançar a cadeia; foi preciso o `StudioResetHandle` (decisão 2).
- Spec §10 fala em "estender a struct"; a extensão adotada **não estende** a struct (16 bytes), reaproveita os 4 bytes mortos de `generation` (decisão 8).
- Não existe header C para o ABI do `filter-capi` (nenhum `clearcore_filter.h`); cada arquivo C declara `typedef` local dos três ponteiros de função (`pipewire_helper.c:464-466`, `tests/test_neural_filter.c`, `tests/denoise_file.c`). Não se cria header novo aqui; o contrato do ABI fica no doc comment de `filter-capi/src/lib.rs` e no typedef do helper.
- O helper só tem **um** alvo de teste no meson (`realtime_callback_contract`); `test_neural_filter.c` e `denoise_file.c` não estão no `meson.build` (são utilitários compilados à mão contra `platform/linux/helper/lib/libclearcore_filter.so`, que é ignorado pelo git).
- O Electron tem um defeito latente na gravação do arquivo de estado: `fs.writeFileSync` trunca para 0 antes de regravar, e um helper com o arquivo mapeado que leia nessa janela leva SIGBUS. A Tarefa 6 troca a gravação por escrita em posição fixa sem truncar (necessária para não encolher o arquivo e barata de fazer junto).

## File Structure

| Arquivo | Responsabilidade |
|---|---|
| `crates/model/Cargo.toml` (modificar) | Path dependency `studio-dsp`. |
| `crates/model/src/studio_backend.rs` (criar) | `StudioBackend<B>`, `StudioResetHandle`, testes unitários com backend fake. |
| `crates/model/src/lib.rs` (modificar) | `mod studio_backend` + reexports; `impl InferenceBackend for Box<T>`. |
| `crates/model/tests/studio_contract.rs` (criar) | Paridade `studio_dsp::{HOP_SAMPLES,SAMPLE_RATE_HZ}` x `contracts::audio`. |
| `crates/engine/Cargo.toml` (modificar) | Path dependency `studio-dsp`. |
| `crates/engine/src/engine.rs` (modificar) | `StudioAttachment`, `with_studio`, embrulho em `set_backend`, `advance_generation` com reset do DSP. |
| `crates/engine/src/worker.rs` (modificar) | Usa `advance_generation` nos 3 fechamentos de geração. |
| `crates/engine/tests/studio.rs` (criar) | Preset no engine, Bypass/Mute fora do DSP, reset por geração, latência. |
| `crates/filter-capi/Cargo.toml` (modificar) | Path dependency `studio-dsp`. |
| `crates/filter-capi/src/lib.rs` (modificar) | `ClearcoreFilter { backend: StudioBackend<TractBackend>, control }`, `clearcore_filter_set_preset`. |
| `crates/filter-capi/tests/set_preset.rs` (criar) | Teste de ABI C: nulo, preset inválido, preset válido, latência com estúdio. |
| `platform/linux/helper/src/clearcore_state.h` (criar) | Layout de 16 bytes do arquivo de estado (sem PipeWire), `clearcore_preset_decode`, `clearcore_preset_changed`. |
| `platform/linux/helper/src/pipewire_helper.h` (modificar) | Usa `clearcore_state.h`; novo campo `neural_set_preset_fn`, `applied_preset`. |
| `platform/linux/helper/src/pipewire_helper.c` (modificar) | `dlsym` opcional de `clearcore_filter_set_preset`; aplica o preset a cada hop em `Active`; documenta a ordem Mute/Bypass/Active. |
| `platform/linux/helper/tests/test_shared_state.c` (criar) | Teste do layout e da compatibilidade (sem PipeWire). |
| `platform/linux/helper/meson.build` (modificar) | Alvo `test_shared_state`. |
| `crates/app-tauri/electron/clearcore-state.cjs` (criar) | Codificação pura do buffer de 16 bytes e escrita em posição fixa. |
| `crates/app-tauri/electron/clearcore-state.selftest.cjs` (criar) | Autoteste em `node` puro (fora do glob do vitest). |
| `crates/app-tauri/electron/main.cjs` (modificar) | `writeClearcoreSharedState` usa o módulo; `preset` sincronizado de `GetStatus`. |
| `crates/app-tauri/package.json` (modificar) | Script `test:electron-state`. |

---

### Task 1: Pré-condição (crate `studio-dsp` da Fase 1a) e linha de base

**Files:**
- Nenhum arquivo do repositório é alterado nesta tarefa.

**Interfaces:**
- Consumes: as assinaturas da seção "Interfaces consumidas da Fase 1a".
- Produces: nada. Confirma que as Tarefas 2 a 4 podem declarar `studio-dsp = { path = "../studio-dsp" }` e registra a linha de base (o que passa antes de qualquer alteração).

- [ ] **Step 1: Carregar o toolchain Rust**

Run:
```bash
source <SCRATCHPAD>/rust-env/env.sh && cargo --version
```
Expected: `cargo 1.90.0 ...`. O script define `CARGO_TARGET_DIR` fora do repositório. Use o mesmo `source` no começo de cada comando `cargo` deste plano (cada chamada de shell é isolada).

- [ ] **Step 2: Verificar que o crate existe e está no workspace**

Run (na raiz do worktree):
```bash
ls crates/studio-dsp/Cargo.toml && grep -n '"crates/studio-dsp"' Cargo.toml
```
Expected: o caminho `crates/studio-dsp/Cargo.toml` é listado e a linha `"crates/studio-dsp",` aparece em `members`. **Se qualquer um falhar, PARE e reporte** que a Fase 1a ainda não entregou o crate; não crie o crate aqui.

- [ ] **Step 3: Verificar nome do pacote e as assinaturas consumidas**

Run:
```bash
grep -n '^name' crates/studio-dsp/Cargo.toml
grep -rnE "pub const HOP_SAMPLES|pub const SAMPLE_RATE_HZ|pub enum Preset|repr\(u8\)|fn from_u8|fn as_u8|pub struct StudioChain|pub struct StudioControl|fn set_preset|fn preset\b|fn latency_samples|fn reset|fn process" crates/studio-dsp/src
```
Expected: `name = "studio-dsp"`; aparecem `HOP_SAMPLES`, `SAMPLE_RATE_HZ`, `pub enum Preset`, `#[repr(u8)]`, `from_u8`, `as_u8`, `StudioChain`, `StudioControl`, `set_preset` (em `StudioChain` com `&mut self` e em `StudioControl` com `&self`), `preset(&self)`, `latency_samples`, `reset` e `process(&mut self, ... &mut [f32; 480])`. Leia o arquivo que define `StudioControl` e confirme que `StudioControl: Send + Sync` (é usado como `Arc<StudioControl>` entre a thread de áudio e a de IPC), que `StudioChain: Send` (a trait `InferenceBackend` exige `Send`, então `StudioBackend` só compila se a cadeia for `Send`) e que `Preset` deriva `Clone, Copy, Debug, PartialEq, Eq`. **Qualquer divergência: PARE e reporte** (os trechos de código das Tarefas 2 a 4 assumem exatamente estes nomes).

- [ ] **Step 4: Linha de base do Rust**

Run:
```bash
source <SCRATCHPAD>/rust-env/env.sh && cargo fmt --all -- --check && cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings && cargo test --workspace --locked --offline
```
Expected: tudo passa. Se a linha de base já estiver vermelha, PARE e reporte o que falhou: não é causado por esta fase.

- [ ] **Step 5: Linha de base do helper C neste ambiente**

Run:
```bash
export PATH=~/miniconda3/bin:$PATH && meson setup <SCRATCHPAD>/helper-build platform/linux/helper
```
Expected num ambiente com PipeWire: configuração concluída; então `ninja -C <esse diretório> && meson test -C <esse diretório>` deve passar o teste `realtime_callback_contract` (é o único alvo do `meson.build`). **Neste sandbox (medido ao planejar):** falha com `ERROR: Dependency "libpipewire-0.3" not found (tried pkg-config)`. Nesse caso registre "helper C não compilável aqui" e siga: as Tarefas 5 e 6 trazem verificações alternativas (`gcc` puro sobre o header sem PipeWire, `node` puro) e marcam o `ninja`/`meson test` completo como **não verificado**. Não toque em `platform/linux/helper/build/` (é o diretório gerado em outra máquina; está no `.gitignore`).

- [ ] **Step 6: Checkpoint**

Run: `git status --short`
Expected: só arquivos da Fase 1a (crate `studio-dsp`, `Cargo.toml`, `Cargo.lock`), da Fase 2 (se já executada) e os documentos em `docs/superpowers/`; nenhum arquivo desta fase foi tocado. Não comitar.

---

### Task 2: `StudioBackend<B>` e `StudioResetHandle` no crate `model`

**Files:**
- Modify: `crates/model/Cargo.toml`
- Modify: `Cargo.lock` (nova aresta `realtime-noise-model` → `studio-dsp`, atualizada offline)
- Create: `crates/model/src/studio_backend.rs`
- Modify: `crates/model/src/lib.rs` (`mod studio_backend;`, reexports, `impl InferenceBackend for Box<T>`)
- Create: `crates/model/tests/studio_contract.rs`

**Interfaces:**
- Consumes: `studio_dsp::{Preset, StudioChain, StudioControl, HOP_SAMPLES, SAMPLE_RATE_HZ}` (Fase 1a); `InferenceBackend`, `BackendDescriptor`, `ProcessedFrame::checked`, `InferenceError` (`crates/model/src/lib.rs:62-100`); `realtime_noise_contracts::{AudioFrame, HOP_SAMPLES, SAMPLE_RATE_HZ}`.
- Produces (usados pelas Tarefas 3 e 4):
  - `pub struct StudioResetHandle` (`Clone + Default + Debug`): `StudioResetHandle::new() -> Self`, `request(&self)`, `is_requested(&self) -> bool` (apenas espia; usado pelos testes do engine).
  - `pub struct StudioBackend<B: InferenceBackend>`: `new(inner: B, control: Arc<StudioControl>) -> Self`, `with_reset_handle(inner: B, control: Arc<StudioControl>, reset: StudioResetHandle) -> Self`, `reset_handle(&self) -> StudioResetHandle`; `impl<B: InferenceBackend> InferenceBackend for StudioBackend<B>`.
  - `impl<T: InferenceBackend + ?Sized> InferenceBackend for Box<T>`.
  - Reexports no `realtime_noise_model`: `StudioBackend`, `StudioResetHandle`.

O código desta tarefa foi compilado e testado (8 testes, `clippy -D warnings` com pedantic+nursery, `fmt`) num crate descartável contra um stub de `studio-dsp` com as assinaturas acima; o que o plano mostra é o texto validado.

- [ ] **Step 1: Declarar a dependência e escrever os testes que falham**

Em `crates/model/Cargo.toml`, na seção `[dependencies]`, logo depois da linha `sha2 = ...` e antes de `tar = ...`, inserir:

```toml
studio-dsp = { path = "../studio-dsp" }
```

Criar `crates/model/tests/studio_contract.rs`:

```rust
#![forbid(unsafe_code)]

//! O `studio-dsp` não depende de `contracts`; esta paridade impede que as duas constantes divirjam.

#[test]
fn studio_dsp_hop_matches_the_audio_contract() {
    assert_eq!(
        studio_dsp::HOP_SAMPLES,
        realtime_noise_contracts::HOP_SAMPLES
    );
}

#[test]
fn studio_dsp_sample_rate_matches_the_audio_contract() {
    assert_eq!(
        studio_dsp::SAMPLE_RATE_HZ,
        realtime_noise_contracts::SAMPLE_RATE_HZ
    );
}
```

Criar `crates/model/src/studio_backend.rs` contendo **somente** o módulo de testes abaixo (a implementação entra no Step 3, acima dele):

```rust
#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use realtime_noise_contracts::HOP_SAMPLES;

    const INNER_LATENCY: u32 = 1_440;

    fn descriptor() -> BackendDescriptor {
        BackendDescriptor {
            backend: "fake",
            backend_version: "1",
            runtime: "test",
            runtime_version: "1",
            asset_id: "fake-asset".to_owned(),
            asset_sha256: "0".repeat(64),
            cpu_profile: "test",
        }
    }

    /// Returns the input unchanged, or fails when `fail` is set.
    struct FakeBackend {
        fail: bool,
    }

    impl InferenceBackend for FakeBackend {
        fn descriptor(&self) -> BackendDescriptor {
            descriptor()
        }

        fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError> {
            if self.fail {
                return Err(InferenceError::UnsupportedCpuProfile(
                    "fake failure".to_owned(),
                ));
            }
            ProcessedFrame::checked(*input, INNER_LATENCY, descriptor())
        }

        fn algorithmic_latency_samples(&self) -> u32 {
            INNER_LATENCY
        }
    }

    fn tone(frame_index: u16) -> AudioFrame {
        let mut frame = [0.0_f32; HOP_SAMPLES];
        for (i, sample) in (0_u16..).zip(frame.iter_mut()) {
            let n = f32::from(frame_index).mul_add(480.0, f32::from(i));
            *sample = 0.9 * (n * 0.13).sin();
        }
        frame
    }

    fn backend_with(preset: Preset) -> (StudioBackend<FakeBackend>, Arc<StudioControl>) {
        let control = Arc::new(StudioControl::new(preset));
        let backend = StudioBackend::new(FakeBackend { fail: false }, Arc::clone(&control));
        (backend, control)
    }

    #[test]
    fn off_preset_returns_the_inner_output_bit_for_bit() {
        let (mut backend, _control) = backend_with(Preset::Off);
        for index in 0..20 {
            let input = tone(index);
            let output = backend.process(&input).map(|frame| frame.samples);
            assert_eq!(output.ok(), Some(input), "frame {index}");
        }
    }

    #[test]
    fn latency_is_inner_plus_chain_in_both_places() {
        let (mut backend, _control) = backend_with(Preset::Off);
        assert_eq!(backend.algorithmic_latency_samples(), INNER_LATENCY + 96);
        let reported = backend
            .process(&tone(0))
            .map(|frame| frame.algorithmic_latency_samples);
        assert_eq!(reported.ok(), Some(INNER_LATENCY + 96));
    }

    #[test]
    fn descriptor_is_delegated_to_the_inner_backend() {
        let (backend, _control) = backend_with(Preset::Off);
        assert_eq!(backend.descriptor(), descriptor());
    }

    #[test]
    fn inner_error_is_propagated() {
        let control = Arc::new(StudioControl::new(Preset::Off));
        let mut backend = StudioBackend::new(FakeBackend { fail: true }, control);
        assert!(matches!(
            backend.process(&tone(0)),
            Err(InferenceError::UnsupportedCpuProfile(_))
        ));
    }

    #[test]
    fn preset_change_on_the_control_reaches_the_chain() {
        let (mut backend, control) = backend_with(Preset::Off);
        let off_output = backend.process(&tone(0)).map(|frame| frame.samples);
        assert_eq!(off_output.ok(), Some(tone(0)));

        control.set_preset(Preset::Broadcast);
        let mut changed = false;
        for index in 1..200 {
            let input = tone(index);
            let output = backend.process(&input).map(|frame| frame.samples);
            if output.ok() != Some(input) {
                changed = true;
                break;
            }
        }
        assert!(changed, "Broadcast never altered the signal");
    }

    #[test]
    fn reset_request_restores_the_initial_chain_state() {
        let (mut warmed, _warmed_control) = backend_with(Preset::Broadcast);
        let (mut fresh, _fresh_control) = backend_with(Preset::Broadcast);
        for index in 0..30 {
            assert!(warmed.process(&tone(index)).is_ok());
        }

        warmed.reset_handle().request();
        let probe = tone(1_000);
        let after_reset = warmed.process(&probe).map(|frame| frame.samples);
        let first_frame = fresh.process(&probe).map(|frame| frame.samples);
        assert!(after_reset.is_ok());
        assert_eq!(after_reset.ok(), first_frame.ok());
    }

    #[test]
    fn reset_handle_is_shared_between_clones_and_consumed_once() {
        let handle = StudioResetHandle::new();
        let clone = handle.clone();
        assert!(!handle.is_requested());
        clone.request();
        assert!(handle.is_requested());
        assert!(handle.take());
        assert!(!handle.is_requested());
        assert!(!handle.take());
    }

    #[test]
    fn boxed_dyn_backend_can_be_wrapped() {
        let control = Arc::new(StudioControl::new(Preset::Off));
        let boxed: Box<dyn InferenceBackend> = Box::new(FakeBackend { fail: false });
        let mut backend = StudioBackend::new(boxed, control);
        assert_eq!(backend.algorithmic_latency_samples(), INNER_LATENCY + 96);
        let output = backend.process(&tone(0)).map(|frame| frame.samples);
        assert_eq!(output.ok(), Some(tone(0)));
    }
}
```

Em `crates/model/src/lib.rs`, declarar o módulo junto dos outros `mod` (depois de `mod m0_records;`):

```rust
mod studio_backend;
```

- [ ] **Step 2: Rodar os testes para ver falhar**

Run:
```bash
source <SCRATCHPAD>/rust-env/env.sh && cargo test -p realtime-noise-model --offline --lib studio_backend
```
(sem `--locked` de propósito: o `Cargo.lock` ainda não tem a aresta nova e o cargo a grava aqui; depois deste passo todos os comandos voltam a usar `--locked`.)
Expected: FAIL de compilação com `cannot find type StudioBackend in this scope` / `unresolved import super::*` itens (`Arc`, `Preset`, `StudioControl`, `StudioResetHandle`, `AudioFrame`, `InferenceError` ainda não importados no módulo).

- [ ] **Step 3: Escrever a implementação**

Colocar no **topo** de `crates/model/src/studio_backend.rs`, acima do módulo `tests` já criado:

```rust
//! Decorator that runs the `studio-dsp` finishing chain after a neural backend.
//!
//! `StudioBackend` implements the same [`InferenceBackend`] trait as the backend it wraps, so the
//! engine and the C ABI only wrap the backend they already build. The trait does not change.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use realtime_noise_contracts::AudioFrame;
use studio_dsp::{Preset, StudioChain, StudioControl};

use crate::{BackendDescriptor, InferenceBackend, InferenceError, ProcessedFrame};

/// Cloneable handle that asks a [`StudioBackend`] to reset its DSP state before the next frame.
///
/// The engine only sees a `Box<dyn InferenceBackend>`, so it cannot reach the chain directly.
/// It keeps this handle and calls [`StudioResetHandle::request`] whenever it opens a new
/// generation; the backend consumes the request at the start of the next `process` call.
#[derive(Debug, Clone, Default)]
pub struct StudioResetHandle(Arc<AtomicBool>);

impl StudioResetHandle {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests a DSP state reset before the next processed frame.
    pub fn request(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Reports whether a reset is pending, without consuming it.
    #[must_use]
    pub fn is_requested(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    fn take(&self) -> bool {
        self.0.swap(false, Ordering::AcqRel)
    }
}

/// Runs `inner`, then applies the studio chain to the processed frame.
pub struct StudioBackend<B: InferenceBackend> {
    inner: B,
    chain: StudioChain,
    control: Arc<StudioControl>,
    applied: Preset,
    reset: StudioResetHandle,
}

impl<B: InferenceBackend> StudioBackend<B> {
    /// Wraps `inner`; the chain starts on the preset currently held by `control`.
    pub fn new(inner: B, control: Arc<StudioControl>) -> Self {
        Self::with_reset_handle(inner, control, StudioResetHandle::new())
    }

    /// Like [`StudioBackend::new`], but shares an existing reset handle (used by the engine so the
    /// handle survives a backend swap).
    pub fn with_reset_handle(
        inner: B,
        control: Arc<StudioControl>,
        reset: StudioResetHandle,
    ) -> Self {
        let applied = control.preset();
        Self {
            inner,
            chain: StudioChain::new(applied),
            control,
            applied,
            reset,
        }
    }

    /// Returns a handle that resets the DSP state before the next frame.
    pub fn reset_handle(&self) -> StudioResetHandle {
        self.reset.clone()
    }
}

impl<B: InferenceBackend> InferenceBackend for StudioBackend<B> {
    fn descriptor(&self) -> BackendDescriptor {
        self.inner.descriptor()
    }

    fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError> {
        let mut processed = self.inner.process(input)?;

        if self.reset.take() {
            self.chain.reset();
        }
        let wanted = self.control.preset();
        if wanted != self.applied {
            self.chain.set_preset(wanted);
            self.applied = wanted;
        }

        self.chain.process(&mut processed.samples);
        ProcessedFrame::checked(
            processed.samples,
            processed
                .algorithmic_latency_samples
                .saturating_add(self.chain.latency_samples()),
            processed.provenance,
        )
    }

    fn algorithmic_latency_samples(&self) -> u32 {
        self.inner
            .algorithmic_latency_samples()
            .saturating_add(self.chain.latency_samples())
    }
}
```

Em `crates/model/src/lib.rs`, ao lado dos outros `pub use` (depois de `pub use golden::{...};`), acrescentar:

```rust
pub use studio_backend::{StudioBackend, StudioResetHandle};
```

e, logo **depois** da definição de `pub trait InferenceBackend: Send { ... }` (e antes de `#[cfg(test)] mod tests`), acrescentar a implementação para `Box` (não altera a trait; o engine guarda `Box<dyn InferenceBackend>` e precisa dele como `B`):

```rust
impl<T: InferenceBackend + ?Sized> InferenceBackend for Box<T> {
    fn descriptor(&self) -> BackendDescriptor {
        (**self).descriptor()
    }

    fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError> {
        (**self).process(input)
    }

    fn algorithmic_latency_samples(&self) -> u32 {
        (**self).algorithmic_latency_samples()
    }
}
```

- [ ] **Step 4: Rodar os testes para ver passar**

Run:
```bash
source <SCRATCHPAD>/rust-env/env.sh && cargo test -p realtime-noise-model --locked --offline --lib studio_backend && cargo test -p realtime-noise-model --locked --offline --test studio_contract
```
Expected: PASS (8 testes em `studio_backend::tests`, 2 em `studio_contract`). Se `--locked` reclamar do `Cargo.lock`, o Step 2 não gravou a aresta: rode `cargo metadata --offline --format-version 1 > /dev/null` uma vez e repita.

- [ ] **Step 5: Verificar o contrato da Fase 1a que os testes pressupõem**

Dois testes dependem do comportamento prometido pela 1a, não do código desta tarefa: `off_preset_returns_the_inner_output_bit_for_bit` (spec §4: `Off` é "passagem bit a bit idêntica", sem atraso do sinal) e `reset_request_restores_the_initial_chain_state` (`reset()` volta ao estado inicial). Se algum falhar com a cadeia real, **não mude o teste para passar**: PARE e reporte o texto do assert e o comportamento observado (por exemplo "o `Off` atrasa o sinal em 96 amostras", caso em que a latência reportada em `Off` também precisa ser decidida com a 1a).

- [ ] **Step 6: Lint e formatação do crate**

Run:
```bash
source <SCRATCHPAD>/rust-env/env.sh && cargo fmt -p realtime-noise-model -- --check && cargo clippy -p realtime-noise-model --all-targets --all-features --locked --offline -- -D warnings
```
Expected: sem saída do `fmt` e `Finished` do clippy sem warnings. (`--all-features` compila o backend `tract`; precisa dos crates offline já presentes, como no CI.)

- [ ] **Step 7: Checkpoint**

Run: `git status --short`
Expected (além do que veio antes): `M crates/model/Cargo.toml`, `M crates/model/src/lib.rs`, `M Cargo.lock`, `?? crates/model/src/studio_backend.rs`, `?? crates/model/tests/studio_contract.rs`. Não comitar.

---

### Task 3: Engine — `with_studio`, embrulho em `set_backend`, reset do DSP por geração

**Files:**
- Modify: `crates/engine/Cargo.toml`
- Modify: `Cargo.lock` (aresta `realtime-noise-engine` → `studio-dsp`)
- Modify: `crates/engine/src/engine.rs`
- Modify: `crates/engine/src/worker.rs`
- Create: `crates/engine/tests/studio.rs`

**Interfaces:**
- Consumes (Tarefa 2 e Fase 1a): `realtime_noise_model::{StudioBackend, StudioResetHandle}` com `StudioBackend::with_reset_handle(inner, control, reset)`, `StudioResetHandle::{new, request, is_requested}`, `impl InferenceBackend for Box<T>`; `studio_dsp::{StudioControl, Preset}`.
- Produces:
  - `DenoiseEngine::with_studio(self, control: Arc<StudioControl>) -> Self` (`#[must_use]`; chamar uma vez, antes de `start`; segunda chamada é ignorada).
  - `pub(crate) struct StudioAttachment` com `new(control: Arc<StudioControl>, reset: StudioResetHandle) -> Self`; campo novo `EngineSharedState::studio: Option<StudioAttachment>`.
  - `EngineSharedState::advance_generation(&mut self, reason: ResetReason) -> (u64, GenerationId)` (fecha a geração, abre a próxima e pede o reset do DSP); é o único lugar que troca `generation`.
  - `set_backend` embrulha o backend recebido quando há estúdio anexado.
- A API pública existente (`new`, `with_mode`, `new_standalone`, `start`, ...) não muda de assinatura; sem `with_studio` o comportamento é idêntico ao atual.

Contexto lido no código (ver Decisões 1, 2 e 5): o engine nunca constrói backend; `Bypass`/`Mute` são decididos antes de `state.backend` ser tocado (`worker.rs:92-101`, `engine.rs:280-287`), então o DSP só roda em `Active`; a geração é trocada em cinco lugares (`worker.rs` três vezes, `engine.rs` `begin_generation_restart` e o ramo de falha do `process_frame` síncrono). Os trechos abaixo foram compilados e testados numa cópia descartável do workspace (15 testes unitários, os 4 de integração existentes e 8 novos, `clippy --all-features -D warnings` sem avisos).

- [ ] **Step 1: Declarar a dependência e escrever os testes que falham**

Em `crates/engine/Cargo.toml`, em `[dependencies]`, depois de `realtime-noise-model = { path = "../model" }`:

```toml
studio-dsp = { path = "../studio-dsp" }
```

Criar `crates/engine/tests/studio.rs`:

```rust
#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use std::sync::Arc;

use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES};
use realtime_noise_engine::{DenoiseEngine, DenoiseMode, ResetReason};
use realtime_noise_model::{BackendDescriptor, InferenceBackend, InferenceError, ProcessedFrame};
use studio_dsp::{Preset, StudioControl};

struct Passthrough;

impl Passthrough {
    fn descriptor() -> BackendDescriptor {
        BackendDescriptor {
            backend: "passthrough",
            backend_version: "1",
            runtime: "test",
            runtime_version: "1",
            asset_id: "test".to_owned(),
            asset_sha256: "0".repeat(64),
            cpu_profile: "test",
        }
    }
}

impl InferenceBackend for Passthrough {
    fn descriptor(&self) -> BackendDescriptor {
        Self::descriptor()
    }

    fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError> {
        ProcessedFrame::checked(*input, 1_440, Self::descriptor())
    }

    fn algorithmic_latency_samples(&self) -> u32 {
        1_440
    }
}

fn tone(frame_index: u16) -> AudioFrame {
    let mut frame = [0.0_f32; HOP_SAMPLES];
    for (i, sample) in (0_u16..).zip(frame.iter_mut()) {
        let n = f32::from(frame_index).mul_add(480.0, f32::from(i));
        *sample = 0.9 * (n * 0.13).sin();
    }
    frame
}

fn started_engine(control: &Arc<StudioControl>, mode: DenoiseMode) -> DenoiseEngine {
    let mut engine =
        DenoiseEngine::new_standalone(Box::new(Passthrough), mode).with_studio(Arc::clone(control));
    engine.start().unwrap();
    engine
}

/// Feeds tones until the engine output differs from the input; returns whether it ever did.
fn output_ever_differs(engine: &mut DenoiseEngine) -> bool {
    (0..200).any(|index| {
        let input = tone(index);
        engine.process_frame(&input).unwrap() != input
    })
}

#[test]
fn off_preset_leaves_the_active_output_identical() {
    let control = Arc::new(StudioControl::new(Preset::Off));
    let mut engine = started_engine(&control, DenoiseMode::Active);
    for index in 0..20 {
        let input = tone(index);
        assert_eq!(
            engine.process_frame(&input).unwrap(),
            input,
            "frame {index}"
        );
    }
}

#[test]
fn a_studio_preset_changes_the_active_output() {
    let control = Arc::new(StudioControl::new(Preset::Broadcast));
    let mut engine = started_engine(&control, DenoiseMode::Active);
    assert!(output_ever_differs(&mut engine));
}

#[test]
fn preset_switched_while_running_reaches_the_audio() {
    let control = Arc::new(StudioControl::new(Preset::Off));
    let mut engine = started_engine(&control, DenoiseMode::Active);
    let first = tone(0);
    assert_eq!(engine.process_frame(&first).unwrap(), first);

    control.set_preset(Preset::Broadcast);
    assert!(output_ever_differs(&mut engine));
}

#[test]
fn bypass_returns_the_raw_input_even_with_a_studio_preset() {
    let control = Arc::new(StudioControl::new(Preset::Broadcast));
    let mut engine = started_engine(&control, DenoiseMode::Bypass);
    for index in 0..50 {
        let input = tone(index);
        assert_eq!(
            engine.process_frame(&input).unwrap(),
            input,
            "frame {index}"
        );
    }
}

#[test]
fn mute_returns_silence_even_with_a_studio_preset() {
    let control = Arc::new(StudioControl::new(Preset::Broadcast));
    let mut engine = started_engine(&control, DenoiseMode::Mute);
    for index in 0..50 {
        let output = engine.process_frame(&tone(index)).unwrap();
        assert_eq!(output, [0.0; HOP_SAMPLES], "frame {index}");
    }
}

#[test]
fn generation_restart_resets_the_dsp_state() {
    let control = Arc::new(StudioControl::new(Preset::Broadcast));
    let mut warmed = started_engine(&control, DenoiseMode::Active);
    let mut fresh = started_engine(&control, DenoiseMode::Active);
    for index in 0..30 {
        warmed.process_frame(&tone(index)).unwrap();
    }

    warmed
        .begin_generation_restart(ResetReason::UserRequested)
        .unwrap();
    let probe = tone(1_000);
    assert_eq!(
        warmed.process_frame(&probe).unwrap(),
        fresh.process_frame(&probe).unwrap()
    );
}

#[test]
fn a_backend_set_later_keeps_the_studio_chain() {
    let control = Arc::new(StudioControl::new(Preset::Broadcast));
    let mut engine = DenoiseEngine::new_standalone(Box::new(Passthrough), DenoiseMode::Active)
        .with_studio(Arc::clone(&control));
    engine.set_backend(Box::new(Passthrough)).unwrap();
    engine.start().unwrap();
    assert!(output_ever_differs(&mut engine));
}

#[test]
fn an_engine_without_studio_behaves_as_before() {
    let mut engine = DenoiseEngine::new_standalone(Box::new(Passthrough), DenoiseMode::Active);
    engine.start().unwrap();
    for index in 0..20 {
        let input = tone(index);
        assert_eq!(
            engine.process_frame(&input).unwrap(),
            input,
            "frame {index}"
        );
    }
}
```

No fim do módulo `tests` de `crates/engine/src/engine.rs` (antes da última `}` do arquivo), acrescentar:

```rust
    #[test]
    fn restart_requests_a_studio_reset_only_when_a_studio_is_attached() {
        use studio_dsp::Preset;

        let control = Arc::new(StudioControl::new(Preset::Off));
        let mut engine =
            DenoiseEngine::new_standalone(Box::new(PassthroughBackend::new()), DenoiseMode::Active)
                .with_studio(control);
        let handle = {
            let shared = engine.shared.lock().unwrap_or_else(PoisonError::into_inner);
            shared.studio.as_ref().map(|studio| studio.reset.clone())
        };
        assert!(handle.is_some());
        let handle = handle.unwrap_or_default();
        assert!(!handle.is_requested());

        assert_eq!(
            engine.begin_generation_restart(ResetReason::UserRequested),
            Ok(2)
        );
        assert!(handle.is_requested());

        let mut plain =
            DenoiseEngine::new_standalone(Box::new(PassthroughBackend::new()), DenoiseMode::Active);
        assert_eq!(
            plain.begin_generation_restart(ResetReason::UserRequested),
            Ok(2)
        );
        let shared = plain.shared.lock().unwrap_or_else(PoisonError::into_inner);
        assert!(shared.studio.is_none());
    }
```

No fim de `crates/engine/src/worker.rs`, acrescentar o módulo (prova que os três fechamentos de geração do worker pedem o reset do DSP):

```rust

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{EngineState, StudioAttachment};
    use crate::generation::{Generation, GenerationId};
    use crate::queue::BoundedQueueTransport;
    use realtime_noise_model::{
        BackendDescriptor, InferenceBackend, ProcessedFrame, StudioResetHandle,
    };
    use studio_dsp::{Preset, StudioControl};

    /// Echoes the input, or fails every call when `fail` is set.
    struct ScriptedBackend {
        fail: bool,
    }

    impl ScriptedBackend {
        fn descriptor() -> BackendDescriptor {
            BackendDescriptor {
                backend: "scripted",
                backend_version: "1",
                runtime: "test",
                runtime_version: "1",
                asset_id: "test".to_owned(),
                asset_sha256: "0".repeat(64),
                cpu_profile: "test",
            }
        }
    }

    impl InferenceBackend for ScriptedBackend {
        fn descriptor(&self) -> BackendDescriptor {
            Self::descriptor()
        }

        fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError> {
            if self.fail {
                return Err(InferenceError::InferenceExecution("scripted".to_owned()));
            }
            ProcessedFrame::checked(*input, 0, Self::descriptor())
        }

        fn algorithmic_latency_samples(&self) -> u32 {
            0
        }
    }

    fn shared_state(fail: bool, reset: &StudioResetHandle) -> Arc<Mutex<EngineSharedState>> {
        let control = Arc::new(StudioControl::new(Preset::Off));
        Arc::new(Mutex::new(EngineSharedState {
            state: EngineState::Running,
            mode: DenoiseMode::Active,
            backend: Some(Box::new(ScriptedBackend { fail })),
            pending_backend: None,
            generation: Generation::active(GenerationId::new(1)),
            deadline_miss_count: 0,
            is_running: true,
            studio: Some(StudioAttachment::new(control, reset.clone())),
        }))
    }

    fn input_frame() -> FrameEnvelope {
        FrameEnvelope {
            samples: [0.25; HOP_SAMPLES],
            sequence: 7,
            capture_monotonic_ns: 1,
            generation: 1,
            discontinuity: Discontinuity::NONE,
        }
    }

    fn run_one_frame(shared: &Arc<Mutex<EngineSharedState>>, backlog: usize) {
        let input: Arc<dyn RealtimeTransport> = Arc::new(BoundedQueueTransport::new());
        let output: Arc<dyn RealtimeTransport> = Arc::new(BoundedQueueTransport::new());
        DenoiseWorker::process_frame(shared, &input, &output, &input_frame(), backlog);
    }

    fn generation_id(shared: &Arc<Mutex<EngineSharedState>>) -> u64 {
        let state = shared.lock().unwrap_or_else(PoisonError::into_inner);
        state.generation.id().get()
    }

    #[test]
    fn healthy_frame_does_not_request_a_studio_reset() {
        let reset = StudioResetHandle::new();
        let shared = shared_state(false, &reset);
        run_one_frame(&shared, 0);
        assert!(!reset.is_requested());
        assert_eq!(generation_id(&shared), 1);
    }

    #[test]
    fn queue_watermark_closure_requests_a_studio_reset() {
        let reset = StudioResetHandle::new();
        let shared = shared_state(false, &reset);
        run_one_frame(&shared, DEFAULT_WATERMARK_HOPS + 1);
        assert!(reset.is_requested());
        assert_eq!(generation_id(&shared), 2);
    }

    #[test]
    fn backend_error_closure_requests_a_studio_reset() {
        let reset = StudioResetHandle::new();
        let shared = shared_state(true, &reset);
        run_one_frame(&shared, 0);
        assert!(reset.is_requested());
        assert_eq!(generation_id(&shared), 2);
    }
}
```

- [ ] **Step 2: Rodar os testes para ver falhar**

Run (sem `--locked` neste passo, que grava a aresta nova no `Cargo.lock`):
```bash
source <SCRATCHPAD>/rust-env/env.sh && cargo test -p realtime-noise-engine --offline --no-run
```
Expected: FAIL de compilação: `no method named with_studio found for struct DenoiseEngine`, `unresolved import crate::engine::StudioAttachment`, `no field studio on type EngineSharedState`.

- [ ] **Step 3: Implementar em `crates/engine/src/engine.rs`**

(a) Trocar a linha de import do `model` e acrescentar o do `studio-dsp`:

```rust
use realtime_noise_model::{InferenceBackend, InferenceError, StudioBackend, StudioResetHandle};
use studio_dsp::StudioControl;
```

(b) Em `EngineSharedState`, acrescentar o último campo e, logo depois da `struct`, o anexo e o método único que troca a geração:

```rust
pub(crate) struct EngineSharedState {
    pub(crate) state: EngineState,
    pub(crate) mode: DenoiseMode,
    pub(crate) backend: Option<Box<dyn InferenceBackend>>,
    pub(crate) pending_backend: Option<Box<dyn InferenceBackend>>,
    pub(crate) generation: Generation,
    pub(crate) deadline_miss_count: u64,
    pub(crate) is_running: bool,
    pub(crate) studio: Option<StudioAttachment>,
}

/// Studio finishing chain attached to the engine by [`DenoiseEngine::with_studio`].
///
/// Every backend the engine holds is wrapped in a [`StudioBackend`] sharing the same control and
/// the same reset handle, so a backend swap keeps both.
pub(crate) struct StudioAttachment {
    control: Arc<StudioControl>,
    reset: StudioResetHandle,
}

impl StudioAttachment {
    pub(crate) fn new(control: Arc<StudioControl>, reset: StudioResetHandle) -> Self {
        Self { control, reset }
    }

    fn wrap(&self, inner: Box<dyn InferenceBackend>) -> Box<dyn InferenceBackend> {
        Box::new(StudioBackend::with_reset_handle(
            inner,
            Arc::clone(&self.control),
            self.reset.clone(),
        ))
    }
}

impl EngineSharedState {
    /// Closes the active generation, opens the next one and asks the studio chain (if attached)
    /// to reset its state before the next frame. Returns `(closed_id, next_id)`.
    pub(crate) fn advance_generation(&mut self, reason: ResetReason) -> (u64, GenerationId) {
        let old_id = self.generation.id().get();
        self.generation.close(reason);
        let next_id = self.generation.id().next();
        self.generation = Generation::active(next_id);
        if let Some(studio) = &self.studio {
            studio.reset.request();
        }
        (old_id, next_id)
    }
}
```

(c) Em `DenoiseEngine::with_mode`, no literal de `EngineSharedState`, depois de `is_running: false,` acrescentar `studio: None,`.

(d) Inserir `with_studio` logo antes de `/// Starts the engine and background worker thread.`:

```rust
    /// Attaches the studio finishing chain: the current backend (and a pending one, if any) is
    /// wrapped in a [`StudioBackend`] driven by `control`, and so is every backend set later.
    ///
    /// Call it once, before [`DenoiseEngine::start`]; a second call is ignored because wrapping
    /// twice would run the chain twice. `Bypass` and `Mute` never reach the backend, so they never
    /// reach the chain either.
    #[must_use]
    pub fn with_studio(self, control: Arc<StudioControl>) -> Self {
        {
            let mut shared = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
            if shared.studio.is_none() {
                let attachment = StudioAttachment::new(control, StudioResetHandle::new());
                shared.backend = shared.backend.take().map(|inner| attachment.wrap(inner));
                shared.pending_backend = shared
                    .pending_backend
                    .take()
                    .map(|inner| attachment.wrap(inner));
                shared.studio = Some(attachment);
            }
        }
        self
    }
```

(e) Em `set_backend`, logo depois do `let mut shared = ...lock()...;` e antes de `if shared.is_running {`:

```rust
        let backend = match &shared.studio {
            Some(studio) => studio.wrap(backend),
            None => backend,
        };
```

(f) Em `begin_generation_restart`, trocar as quatro linhas que fecham e reabrem a geração:

```rust
        let old_id = shared.generation.id().get();
        shared.generation.close(reason);
        let next_id = shared.generation.id().next();
        shared.generation = Generation::active(next_id);
```
por:
```rust
        let (old_id, next_id) = shared.advance_generation(reason);
```

(g) No ramo `if elapsed > INFERENCE_HARD_DEADLINE || process_result.is_err() {` do `process_frame` síncrono, trocar as três linhas
```rust
                    shared.generation.close(ResetReason::InferenceDeadlineMiss);
                    let next_id = shared.generation.id().next();
                    shared.generation = Generation::active(next_id);
```
por:
```rust
                    shared.advance_generation(ResetReason::InferenceDeadlineMiss);
```

- [ ] **Step 4: Implementar em `crates/engine/src/worker.rs`**

(a) Remover a linha `use crate::generation::Generation;` (deixa de ser usada).

(b) No bloco do watermark, trocar
```rust
            let old_id = state.generation.id().get();
            state
                .generation
                .close(ResetReason::QueueAgeWatermarkExceeded);
            let next_id = state.generation.id().next();
            state.generation = Generation::active(next_id);
```
por:
```rust
            let (old_id, next_id) =
                state.advance_generation(ResetReason::QueueAgeWatermarkExceeded);
```

(c) Nos **dois** blocos que fecham a geração por `InferenceDeadlineMiss` (o do `if elapsed > INFERENCE_HARD_DEADLINE` e o do `let Ok(processed_frame) = process_result else`), trocar, em cada um,
```rust
                    let old_id = state.generation.id().get();
                    state.generation.close(ResetReason::InferenceDeadlineMiss);
                    let next_id = state.generation.id().next();
                    state.generation = Generation::active(next_id);
```
por:
```rust
                    let (old_id, next_id) =
                        state.advance_generation(ResetReason::InferenceDeadlineMiss);
```

Depois confira que não sobrou nenhuma troca manual de geração fora do helper:
```bash
grep -n "generation = Generation::active" crates/engine/src/engine.rs crates/engine/src/worker.rs
```
Expected: exatamente duas linhas em `engine.rs` e nenhuma em `worker.rs`: `self.generation = Generation::active(next_id);` (dentro de `advance_generation`) e `let initial_generation = Generation::active(GenerationId::new(1));` (a geração inicial de `with_mode`).

- [ ] **Step 5: Rodar os testes para ver passar**

Run:
```bash
source <SCRATCHPAD>/rust-env/env.sh && cargo test -p realtime-noise-engine --locked --offline
```
Expected: PASS: 15 testes unitários (eram 11; 4 novos: 1 em `engine`, 3 em `worker`), `backpressure` 2, `deadline` 1, `generation` 1 e `studio` 8. Se `generation_restart_resets_the_dsp_state` falhar com a cadeia real, o `reset()` da 1a não restaura o estado inicial: PARE e reporte (não relaxe o teste).

- [ ] **Step 6: Lint e formatação**

Run:
```bash
source <SCRATCHPAD>/rust-env/env.sh && cargo fmt -p realtime-noise-engine -- --check && cargo clippy -p realtime-noise-engine --all-targets --all-features --locked --offline -- -D warnings
```
Expected: sem saída do `fmt`; clippy `Finished` sem avisos.

- [ ] **Step 7: Checkpoint**

Run: `git status --short`
Expected (além do que veio antes): `M crates/engine/Cargo.toml`, `M crates/engine/src/engine.rs`, `M crates/engine/src/worker.rs`, `M Cargo.lock`, `?? crates/engine/tests/studio.rs`. Não comitar.

---

### Task 4: `filter-capi` — embrulhar o `TractBackend` e expor `clearcore_filter_set_preset`

**Files:**
- Modify: `crates/filter-capi/Cargo.toml`
- Modify: `Cargo.lock` (aresta `realtime-noise-filter-capi` → `studio-dsp`)
- Modify: `crates/filter-capi/src/lib.rs`

**Interfaces:**
- Consumes (Tarefa 2 e Fase 1a): `realtime_noise_model::StudioBackend::new(inner, control)`, `impl InferenceBackend for StudioBackend<B>`; `studio_dsp::{Preset, StudioControl}` (`Preset::from_u8`, `StudioControl::new/set_preset`).
- Produces (ABI C usado pela Tarefa 5):
  - `int32_t clearcore_filter_set_preset(ClearcoreFilter *filter, uint8_t preset)`: `preset` 0=Off, 1=Natural, 2=Podcast, 3=Broadcast; retorna `0` ok, `-1` ponteiro nulo, `-3` preset fora de 0..=3 (preset atual preservado; `-2` continua sendo o erro de `clearcore_filter_process`). Mesmas convenções das funções existentes: `#[unsafe(no_mangle)] pub unsafe extern "C" fn`, ponteiro nulo → `-1`, doc `# Safety`.
  - `ClearcoreFilter { backend: StudioBackend<TractBackend>, control: Arc<StudioControl> }` (campos privados); o preset inicial de todo filtro novo é `Off`.
  - `clearcore_filter_create/process/free` mantêm assinatura e códigos de retorno.
- Observações verificadas ao planejar: (1) o crate é `cdylib` + `staticlib` sem `rlib`, então um diretório `tests/` **não consegue** linkar contra ele; os testes ficam num `#[cfg(test)] mod tests` dentro de `src/lib.rs`. (2) O DFNet3 devolve **exatamente zero** para ruído branco sintético (medido: `ref_max = 0.0` em 100 frames), então um teste "o preset altera a saída" com ruído passaria por engano ou falharia por engano; o teste usa um sinal vozeado sintético (120 Hz com 20 harmônicos e envelope de 4 Hz), cuja saída do modelo não é nula. (3) Não existe header C do ABI; ver "Divergências". (4) Este crate não herda `[lints] workspace` (tem `unsafe_code = "allow"`), então só os lints padrão do clippy valem aqui. Todo o código desta tarefa foi compilado e testado (5 testes, `clippy --all-features -D warnings`) numa cópia descartável do workspace com um stub de `studio-dsp`.

- [ ] **Step 1: Declarar a dependência e escrever os testes que falham**

Em `crates/filter-capi/Cargo.toml`, em `[dependencies]`, depois de `realtime-noise-model = { path = "../model", features = ["tract"] }`:

```toml
studio-dsp = { path = "../studio-dsp" }
```

No fim de `crates/filter-capi/src/lib.rs`, acrescentar:

```rust

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    fn repo_root() -> CString {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        CString::new(root.to_string_lossy().as_bytes()).unwrap_or_default()
    }

    fn create_filter() -> *mut ClearcoreFilter {
        let root = repo_root();
        unsafe { clearcore_filter_create(root.as_ptr()) }
    }

    /// Voiced-speech-like frame: 120 Hz fundamental with 20 harmonics and a 4 Hz syllabic envelope.
    fn voiced_frame(index: u32) -> [f32; 480] {
        let mut frame = [0.0_f32; 480];
        for (offset, sample) in (0_u32..).zip(frame.iter_mut()) {
            let t = (f64::from(index) * 480.0 + f64::from(offset)) / 48_000.0;
            let envelope = 0.5 + 0.5 * (std::f64::consts::TAU * 4.0 * t).sin();
            let voiced: f64 = (1..=20_u32)
                .map(|h| {
                    let h = f64::from(h);
                    (std::f64::consts::TAU * 120.0 * h * t).sin() / h
                })
                .sum();
            *sample = (0.25 * envelope * voiced) as f32;
        }
        frame
    }

    fn process(filter: *mut ClearcoreFilter, input: &[f32; 480]) -> ([f32; 480], i32) {
        let mut output = [0.0_f32; 480];
        let rc = unsafe { clearcore_filter_process(filter, input.as_ptr(), output.as_mut_ptr()) };
        (output, rc)
    }

    #[test]
    fn set_preset_rejects_a_null_handle() {
        assert_eq!(
            unsafe { clearcore_filter_set_preset(std::ptr::null_mut(), 0) },
            -1
        );
    }

    #[test]
    fn set_preset_rejects_unknown_values_and_keeps_the_filter_usable() {
        let filter = create_filter();
        assert!(!filter.is_null());
        assert_eq!(unsafe { clearcore_filter_set_preset(filter, 4) }, -3);
        assert_eq!(unsafe { clearcore_filter_set_preset(filter, 255) }, -3);
        assert_eq!(process(filter, &voiced_frame(0)).1, 0);
        unsafe { clearcore_filter_free(filter) };
    }

    #[test]
    fn set_preset_accepts_every_preset() {
        let filter = create_filter();
        assert!(!filter.is_null());
        for value in 0..=3_u8 {
            assert_eq!(unsafe { clearcore_filter_set_preset(filter, value) }, 0);
        }
        unsafe { clearcore_filter_free(filter) };
    }

    #[test]
    fn new_filter_reports_the_model_latency_plus_the_chain() {
        let filter = create_filter();
        assert!(!filter.is_null());
        let latency = unsafe { (*filter).backend.algorithmic_latency_samples() };
        assert_eq!(latency, 1_440 + 96);
        unsafe { clearcore_filter_free(filter) };
    }

    #[test]
    fn off_is_bit_exact_with_the_raw_neural_backend_and_a_preset_changes_it() {
        let root = repo_root();
        let root_path = Path::new(root.to_str().unwrap_or("."));
        let manifest = ApprovedAssetManifest::verify(root_path).ok();
        assert!(manifest.is_some());
        let Some(manifest) = manifest else { return };
        let reference = TractBackend::new(&manifest, CpuProfile::Avx2Minimum).ok();
        assert!(reference.is_some());
        let Some(mut reference) = reference else {
            return;
        };

        let filter = create_filter();
        assert!(!filter.is_null());

        for index in 0..30 {
            let input = voiced_frame(index);
            let expected = reference.process(&input).map(|frame| frame.samples).ok();
            let (output, rc) = process(filter, &input);
            assert_eq!(rc, 0);
            assert_eq!(Some(output), expected, "Off must not alter frame {index}");
        }

        assert_eq!(unsafe { clearcore_filter_set_preset(filter, 3) }, 0);
        let mut changed = false;
        for index in 30..130 {
            let input = voiced_frame(index);
            let expected = reference.process(&input).map(|frame| frame.samples).ok();
            let (output, rc) = process(filter, &input);
            assert_eq!(rc, 0);
            if Some(output) != expected {
                changed = true;
                break;
            }
        }
        assert!(changed, "Broadcast never altered the neural output");
        unsafe { clearcore_filter_free(filter) };
    }
}
```

- [ ] **Step 2: Rodar os testes para ver falhar**

Run (sem `--locked`: grava a aresta nova no `Cargo.lock`):
```bash
source <SCRATCHPAD>/rust-env/env.sh && cargo test -p realtime-noise-filter-capi --offline --no-run
```
Expected: FAIL de compilação com `cannot find function clearcore_filter_set_preset in this scope`.

- [ ] **Step 3: Implementar em `crates/filter-capi/src/lib.rs`**

(a) Trocar os imports e a `struct` do topo (linhas `use std::path::Path;` até o fim de `pub struct ClearcoreFilter`) por:

```rust
use std::path::Path;
use std::sync::Arc;

use realtime_noise_contracts::AudioFrame;
use realtime_noise_model::{
    ApprovedAssetManifest, CpuProfile, InferenceBackend, StudioBackend, TractBackend,
};
use studio_dsp::{Preset, StudioControl};

pub struct ClearcoreFilter {
    backend: StudioBackend<TractBackend>,
    control: Arc<StudioControl>,
}
```

(b) Trocar o doc comment de `clearcore_filter_create` (a linha `/// Create a new neural filter instance backed by Tract and the approved DeepFilterNet3 model asset.`) por:

```rust
/// Create a new neural filter instance backed by Tract and the approved DeepFilterNet3 model asset,
/// followed by the studio finishing chain. The studio preset starts at `Off` (bit-exact passthrough
/// of the neural output); select another one with `clearcore_filter_set_preset`.
```

(c) Em `clearcore_filter_create`, trocar as duas linhas finais
```rust
    let filter = Box::new(ClearcoreFilter { backend });
    Box::into_raw(filter)
```
por:
```rust
    let control = Arc::new(StudioControl::new(Preset::Off));
    let filter = Box::new(ClearcoreFilter {
        backend: StudioBackend::new(backend, Arc::clone(&control)),
        control,
    });
    Box::into_raw(filter)
```

(d) Em `clearcore_filter_process`, trocar a primeira linha do doc comment por `/// Process a single 480-sample (10 ms @ 48 kHz) audio frame through the DeepFilterNet3 neural network and the studio chain.`

(e) Inserir, entre `clearcore_filter_process` e `clearcore_filter_free`:

```rust
/// Select the studio finishing preset applied after the neural network.
///
/// `preset` is `0` = Off, `1` = Natural, `2` = Podcast, `3` = Broadcast. The change takes effect at
/// the next 480-sample frame boundary.
///
/// Returns `0` on success, `-1` if `filter` is NULL and `-3` if `preset` is not one of the values
/// above (the current preset is left untouched). `-2` is reserved for `clearcore_filter_process`.
///
/// # Safety
/// `filter` must be a valid pointer returned by `clearcore_filter_create`, or NULL. It must not be
/// called concurrently with `clearcore_filter_process` or `clearcore_filter_free` on the same
/// handle: call it from the thread that calls `clearcore_filter_process` (the C helper does).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clearcore_filter_set_preset(
    filter: *mut ClearcoreFilter,
    preset: u8,
) -> i32 {
    if filter.is_null() {
        return -1;
    }
    let Some(preset) = Preset::from_u8(preset) else {
        return -3;
    };
    let filter_ref = unsafe { &*filter };
    filter_ref.control.set_preset(preset);
    0
}
```

- [ ] **Step 4: Rodar os testes para ver passar**

Run:
```bash
source <SCRATCHPAD>/rust-env/env.sh && cargo test -p realtime-noise-filter-capi --locked --offline
```
Expected: PASS, 5 testes (o último carrega o modelo duas vezes e leva alguns segundos em build debug). Se `off_is_bit_exact_...` falhar no trecho `Off must not alter frame`, a cadeia da 1a não é passagem idêntica em `Off`: PARE e reporte.

- [ ] **Step 5: Conferir os símbolos exportados**

Run:
```bash
source <SCRATCHPAD>/rust-env/env.sh && cargo build -p realtime-noise-filter-capi --locked --offline --release && nm -D "$CARGO_TARGET_DIR/release/libclearcore_filter.so" | grep clearcore_filter_
```
Expected: quatro símbolos `T`: `clearcore_filter_create`, `clearcore_filter_free`, `clearcore_filter_process`, `clearcore_filter_set_preset`. (O build release do workspace tem `lto = "fat"`; pode levar alguns minutos.)

- [ ] **Step 6: Lint e formatação**

Run:
```bash
source <SCRATCHPAD>/rust-env/env.sh && cargo fmt -p realtime-noise-filter-capi -- --check && cargo clippy -p realtime-noise-filter-capi --all-targets --all-features --locked --offline -- -D warnings
```
Expected: sem saída do `fmt`; clippy sem avisos.

- [ ] **Step 7: Checkpoint**

Run: `git status --short`
Expected (além do que veio antes): `M crates/filter-capi/Cargo.toml`, `M crates/filter-capi/src/lib.rs`, `M Cargo.lock`. Não comitar.

---

### Task 5: Helper C Linux — arquivo de estado com `preset` e aplicação do preset em `Active`

**Files:**
- Create: `platform/linux/helper/src/clearcore_state.h`
- Create: `platform/linux/helper/tests/test_shared_state.c`
- Modify: `platform/linux/helper/src/pipewire_helper.h`
- Modify: `platform/linux/helper/src/pipewire_helper.c`
- Modify: `platform/linux/helper/meson.build`

**Interfaces:**
- Consumes (Tarefa 4): `int clearcore_filter_set_preset(void *filter, uint8_t preset)` exportado por `libclearcore_filter.so` (0 ok, -1 nulo, -3 preset inválido); carregado por `dlsym` e **opcional** (biblioteca antiga sem o símbolo → o helper segue sem preset, em `Off`).
- Produces (usados pela Tarefa 6 e pelo plano da Fase 2):
  - Contrato do arquivo `$XDG_RUNTIME_DIR/clearcore_state` (16 bytes, little-endian, **tamanho inalterado**): `mode` u32 @0, `target_node_id` u32 @4, `generation` u32 @8, **`preset` u32 @12** (`0`=Off, `1`=Natural, `2`=Podcast, `3`=Broadcast; fora disso é ignorado pelo helper). Só o Electron escreve `@12`.
  - `clearcore_state.h` (sem `#include` de PipeWire): `CLEARCORE_MODE_*`, `CLEARCORE_STATE_FILE`, `CLEARCORE_STATE_SIZE`, `CLEARCORE_PRESET_{OFF,NATURAL,PODCAST,BROADCAST,INVALID}`, `clearcore_shared_state_t` (agora `generation` u32 + `preset` u32), `clearcore_preset_decode(uint32_t) -> int`, `clearcore_preset_to_apply(uint32_t raw, int applied) -> int`, `clearcore_set_preset_fn_t`, `clearcore_state_sync_preset(state, filter, set_fn, &applied)`.
  - `pipewire_helper_context_t` ganha `int (*neural_set_preset_fn)(void *filter, uint8_t preset)` e `int applied_preset`.
- Confirmado e documentado nesta tarefa (Decisão 6): em `on_capture_process`, `Mute` e `Bypass` são decididos antes da função neural; o preset só é aplicado e o estúdio só roda dentro do ramo `Active` que chama `neural_process_fn`. O fallback `noise_suppressor_process` (biblioteca ausente ou `rc != 0`) **não passa pelo estúdio**.
- Limitação deste ambiente: sem `libpipewire-0.3` o `meson`/`ninja` não configuram o helper (Tarefa 1, Passo 5). O que dá para rodar aqui é o teste de `clearcore_state.h` com `gcc` puro (compilado e aprovado ao planejar com `-Wall -Wextra -Werror`, o mesmo nível de aviso do `meson.build`, `warning_level=2`). As edições em `pipewire_helper.[ch]` são pequenas e mecânicas, mas **não foram compiladas aqui**: a verificação delas é o `ninja` + `meson test` numa máquina com PipeWire (Passo 6).

- [ ] **Step 1: Escrever o teste que falha**

Criar `platform/linux/helper/tests/test_shared_state.c`:

```c
#define _GNU_SOURCE
#include "clearcore_state.h"

#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>

/* Bytes exactly as the Electron app writes them: mode, targetNodeId and generation as 32-bit LE. */
static void put_u32(unsigned char *buf, size_t offset, uint32_t value) {
    buf[offset + 0] = (unsigned char)(value & 0xFF);
    buf[offset + 1] = (unsigned char)((value >> 8) & 0xFF);
    buf[offset + 2] = (unsigned char)((value >> 16) & 0xFF);
    buf[offset + 3] = (unsigned char)((value >> 24) & 0xFF);
}

static void test_layout(void) {
    printf("[TEST] test_layout\n");
    assert(sizeof(clearcore_shared_state_t) == 16);
    assert(offsetof(clearcore_shared_state_t, mode) == 0);
    assert(offsetof(clearcore_shared_state_t, target_node_id) == 4);
    assert(offsetof(clearcore_shared_state_t, generation) == 8);
    assert(offsetof(clearcore_shared_state_t, preset) == 12);
    /* Mute must keep its value: an old helper compares `mode == 2` to silence the microphone. */
    assert(CLEARCORE_MODE_ACTIVE == 0);
    assert(CLEARCORE_MODE_BYPASS == 1);
    assert(CLEARCORE_MODE_MUTE == 2);
}

static void test_old_writer_leaves_preset_off(void) {
    printf("[TEST] test_old_writer_leaves_preset_off\n");
    unsigned char file[16];
    memset(file, 0, sizeof(file));
    put_u32(file, 0, CLEARCORE_MODE_MUTE);
    put_u32(file, 4, 77);
    put_u32(file, 8, 5);

    clearcore_shared_state_t state;
    memcpy(&state, file, sizeof(state));
    assert(atomic_load(&state.mode) == CLEARCORE_MODE_MUTE);
    assert(atomic_load(&state.target_node_id) == 77);
    assert(atomic_load(&state.generation) == 5);
    assert(clearcore_preset_decode(atomic_load(&state.preset)) == CLEARCORE_PRESET_OFF);
}

static void test_old_64bit_generation_leaves_preset_off(void) {
    printf("[TEST] test_old_64bit_generation_leaves_preset_off\n");
    /* A writer that stored the old u64 generation = 5 little-endian: bytes 12..15 are zero. */
    unsigned char file[16];
    memset(file, 0, sizeof(file));
    uint64_t generation = 5;
    memcpy(file + 8, &generation, sizeof(generation));

    clearcore_shared_state_t state;
    memcpy(&state, file, sizeof(state));
    assert(atomic_load(&state.generation) == 5);
    assert(clearcore_preset_decode(atomic_load(&state.preset)) == CLEARCORE_PRESET_OFF);
}

static void test_new_writer_preset_does_not_touch_the_mode(void) {
    printf("[TEST] test_new_writer_preset_does_not_touch_the_mode\n");
    unsigned char file[16];
    memset(file, 0, sizeof(file));
    put_u32(file, 0, CLEARCORE_MODE_BYPASS);
    put_u32(file, 12, CLEARCORE_PRESET_PODCAST);

    clearcore_shared_state_t state;
    memcpy(&state, file, sizeof(state));
    assert(atomic_load(&state.mode) == CLEARCORE_MODE_BYPASS);
    assert(clearcore_preset_decode(atomic_load(&state.preset)) == CLEARCORE_PRESET_PODCAST);
}

static void test_decode_rejects_out_of_range(void) {
    printf("[TEST] test_decode_rejects_out_of_range\n");
    for (uint32_t raw = 0; raw <= 3; ++raw) {
        assert(clearcore_preset_decode(raw) == (int)raw);
    }
    assert(clearcore_preset_decode(4) == CLEARCORE_PRESET_INVALID);
    assert(clearcore_preset_decode(255) == CLEARCORE_PRESET_INVALID);
    assert(clearcore_preset_decode(0xFFFFFFFFu) == CLEARCORE_PRESET_INVALID);
}

static void test_to_apply_only_on_change(void) {
    printf("[TEST] test_to_apply_only_on_change\n");
    assert(clearcore_preset_to_apply(2, CLEARCORE_PRESET_OFF) == 2);
    assert(clearcore_preset_to_apply(2, 2) == CLEARCORE_PRESET_INVALID);
    assert(clearcore_preset_to_apply(0, 2) == 0);
    assert(clearcore_preset_to_apply(9, 2) == CLEARCORE_PRESET_INVALID);
    assert(clearcore_preset_to_apply(0, CLEARCORE_PRESET_OFF) == CLEARCORE_PRESET_INVALID);
}

static int g_set_calls = 0;
static int g_last_preset = -1;
static int g_set_result = 0;

static int fake_set_preset(void *filter, uint8_t preset) {
    (void)filter;
    g_set_calls++;
    g_last_preset = preset;
    return g_set_result;
}

static void test_sync_applies_only_on_change(void) {
    printf("[TEST] test_sync_applies_only_on_change\n");
    clearcore_shared_state_t state;
    memset(&state, 0, sizeof(state));
    int filter_handle = 0;
    int applied = CLEARCORE_PRESET_OFF;
    g_set_calls = 0;
    g_set_result = 0;

    clearcore_state_sync_preset(&state, &filter_handle, fake_set_preset, &applied);
    assert(g_set_calls == 0); /* file says Off and the filter starts Off */

    atomic_store(&state.preset, CLEARCORE_PRESET_PODCAST);
    clearcore_state_sync_preset(&state, &filter_handle, fake_set_preset, &applied);
    assert(g_set_calls == 1 && g_last_preset == CLEARCORE_PRESET_PODCAST);
    assert(applied == CLEARCORE_PRESET_PODCAST);

    clearcore_state_sync_preset(&state, &filter_handle, fake_set_preset, &applied);
    assert(g_set_calls == 1); /* unchanged: no second call */

    atomic_store(&state.preset, 9); /* garbage in the file is ignored */
    clearcore_state_sync_preset(&state, &filter_handle, fake_set_preset, &applied);
    assert(g_set_calls == 1 && applied == CLEARCORE_PRESET_PODCAST);

    atomic_store(&state.preset, CLEARCORE_PRESET_OFF);
    clearcore_state_sync_preset(&state, &filter_handle, fake_set_preset, &applied);
    assert(g_set_calls == 2 && g_last_preset == CLEARCORE_PRESET_OFF);
}

static void test_sync_retries_after_a_failed_call(void) {
    printf("[TEST] test_sync_retries_after_a_failed_call\n");
    clearcore_shared_state_t state;
    memset(&state, 0, sizeof(state));
    atomic_store(&state.preset, CLEARCORE_PRESET_BROADCAST);
    int filter_handle = 0;
    int applied = CLEARCORE_PRESET_OFF;
    g_set_calls = 0;
    g_set_result = -3;

    clearcore_state_sync_preset(&state, &filter_handle, fake_set_preset, &applied);
    assert(g_set_calls == 1 && applied == CLEARCORE_PRESET_OFF);
    g_set_result = 0;
    clearcore_state_sync_preset(&state, &filter_handle, fake_set_preset, &applied);
    assert(g_set_calls == 2 && applied == CLEARCORE_PRESET_BROADCAST);
}

static void test_sync_tolerates_a_library_without_set_preset(void) {
    printf("[TEST] test_sync_tolerates_a_library_without_set_preset\n");
    clearcore_shared_state_t state;
    memset(&state, 0, sizeof(state));
    atomic_store(&state.preset, CLEARCORE_PRESET_NATURAL);
    int filter_handle = 0;
    int applied = CLEARCORE_PRESET_OFF;

    clearcore_state_sync_preset(&state, &filter_handle, NULL, &applied);
    clearcore_state_sync_preset(NULL, &filter_handle, fake_set_preset, &applied);
    clearcore_state_sync_preset(&state, NULL, fake_set_preset, &applied);
    assert(applied == CLEARCORE_PRESET_OFF);
}

static void test_mapped_file_round_trip(void) {
    printf("[TEST] test_mapped_file_round_trip\n");
    const char *tmp_dir = getenv("TMPDIR");
    char path[512];
    snprintf(path, sizeof(path), "%s/clearcore_state_test_XXXXXX", tmp_dir ? tmp_dir : "/tmp");
    int fd = mkstemp(path);
    assert(fd >= 0);
    assert(ftruncate(fd, CLEARCORE_STATE_SIZE) == 0);

    clearcore_shared_state_t *mapped = mmap(NULL, sizeof(clearcore_shared_state_t),
                                            PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    assert(mapped != MAP_FAILED);

    /* A writer outside the mapping (as the Electron app is) changes only the preset bytes. */
    unsigned char preset_bytes[4];
    put_u32(preset_bytes, 0, CLEARCORE_PRESET_BROADCAST);
    assert(pwrite(fd, preset_bytes, sizeof(preset_bytes), 12) == (ssize_t)sizeof(preset_bytes));
    assert(atomic_load(&mapped->preset) == CLEARCORE_PRESET_BROADCAST);
    assert(atomic_load(&mapped->mode) == CLEARCORE_MODE_ACTIVE);

    munmap(mapped, sizeof(clearcore_shared_state_t));
    close(fd);
    unlink(path);
}

int main(void) {
    test_layout();
    test_old_writer_leaves_preset_off();
    test_old_64bit_generation_leaves_preset_off();
    test_new_writer_preset_does_not_touch_the_mode();
    test_decode_rejects_out_of_range();
    test_to_apply_only_on_change();
    test_sync_applies_only_on_change();
    test_sync_retries_after_a_failed_call();
    test_sync_tolerates_a_library_without_set_preset();
    test_mapped_file_round_trip();
    printf("[TEST] all shared state tests passed\n");
    return 0;
}
```

- [ ] **Step 2: Compilar o teste para ver falhar**

Run (o `gcc` do sistema basta; o diretório de saída é o scratchpad):
```bash
S=<SCRATCHPAD> && gcc -std=gnu11 -Wall -Wextra -Werror -Iplatform/linux/helper/src platform/linux/helper/tests/test_shared_state.c -o $S/test_shared_state
```
Expected: FAIL: `fatal error: clearcore_state.h: No such file or directory`.

- [ ] **Step 3: Criar `clearcore_state.h`**

Criar `platform/linux/helper/src/clearcore_state.h`:

```c
#ifndef CLEARCORE_STATE_H
#define CLEARCORE_STATE_H

/*
 * Layout of the control file shared between the Electron app (writer) and the PipeWire helper
 * (reader): $XDG_RUNTIME_DIR/clearcore_state, mapped MAP_SHARED by the helper.
 *
 * This header deliberately includes nothing from PipeWire so the layout and the preset decoding
 * can be tested without the PipeWire development files.
 *
 * Byte layout (16 bytes, little-endian, never changes size):
 *
 *   offset  size  field           written by
 *   0       4     mode            Electron (and `pipewire_helper --mode`)
 *   4       4     target_node_id  Electron
 *   8       4     generation      Electron (unused by the helper)
 *   12      4     preset          Electron (studio finishing preset, see CLEARCORE_PRESET_*)
 *
 * History: bytes 8..15 used to be a single `_Atomic uint64_t generation`. The Electron app only ever
 * wrote 32 bits at offset 8 and the helper never read the field, so bytes 12..15 were always zero.
 * `preset` reuses them. Compatibility:
 *   - old helper + new Electron: the helper ignores offset 12; `mode` is untouched (the preset is
 *     NOT packed into `mode`, because an old helper compares `mode == MUTE` and would leak audio);
 *   - new helper + old Electron or an older file: offset 12 is zero, which is `Off`;
 *   - the file never grows, so a writer that rewrites 16 bytes cannot shrink the helper's mapping.
 * A `preset` outside CLEARCORE_PRESET_OFF..CLEARCORE_PRESET_BROADCAST is ignored by the helper.
 *
 * Degraded mode: the studio chain runs inside libclearcore_filter.so (StudioBackend). When that
 * library is missing, fails to load, or a frame returns an error, the helper falls back to
 * noise_suppressor.c, which does NOT apply any studio preset. The preset in this file is then
 * simply not used.
 */

#include <stdatomic.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#if !defined(__BYTE_ORDER__) || __BYTE_ORDER__ != __ORDER_LITTLE_ENDIAN__
#error "clearcore_state.h: the shared state file is little-endian; this host is not"
#endif

/* ClearCore Operating Modes */
#define CLEARCORE_MODE_ACTIVE   0
#define CLEARCORE_MODE_BYPASS   1
#define CLEARCORE_MODE_MUTE     2
#define CLEARCORE_STATE_FILE    "clearcore_state"
#define CLEARCORE_STATE_SIZE    16

/* Studio finishing presets (mirror studio_dsp::Preset::as_u8 and clearcore_filter_set_preset) */
#define CLEARCORE_PRESET_OFF        0
#define CLEARCORE_PRESET_NATURAL    1
#define CLEARCORE_PRESET_PODCAST    2
#define CLEARCORE_PRESET_BROADCAST  3
#define CLEARCORE_PRESET_INVALID    (-1)

/**
 * Shared memory / control state for mode, physical mic binding and studio preset.
 */
typedef struct clearcore_shared_state {
    _Atomic uint32_t mode;             /* 0=Active, 1=Bypass, 2=Mute */
    _Atomic uint32_t target_node_id;   /* Physical microphone node ID (0=auto) */
    _Atomic uint32_t generation;       /* Engine generation counter (unused by the helper) */
    _Atomic uint32_t preset;           /* 0=Off, 1=Natural, 2=Podcast, 3=Broadcast */
} clearcore_shared_state_t;

_Static_assert(sizeof(clearcore_shared_state_t) == CLEARCORE_STATE_SIZE,
               "clearcore_state layout must stay 16 bytes");
_Static_assert(offsetof(clearcore_shared_state_t, mode) == 0, "mode must be at offset 0");
_Static_assert(offsetof(clearcore_shared_state_t, target_node_id) == 4,
               "target_node_id must be at offset 4");
_Static_assert(offsetof(clearcore_shared_state_t, generation) == 8,
               "generation must be at offset 8");
_Static_assert(offsetof(clearcore_shared_state_t, preset) == 12, "preset must be at offset 12");

/** Returns `raw` as a preset in 0..=3, or CLEARCORE_PRESET_INVALID when it is out of range. */
static inline int clearcore_preset_decode(uint32_t raw) {
    return raw <= CLEARCORE_PRESET_BROADCAST ? (int)raw : CLEARCORE_PRESET_INVALID;
}

/**
 * Decides what the helper must do about the preset written in the state file.
 * Returns the preset to apply, or CLEARCORE_PRESET_INVALID when nothing must change (the stored
 * value is invalid or equals the preset already applied to the filter).
 */
static inline int clearcore_preset_to_apply(uint32_t raw, int applied) {
    int wanted = clearcore_preset_decode(raw);
    if (wanted == CLEARCORE_PRESET_INVALID || wanted == applied) {
        return CLEARCORE_PRESET_INVALID;
    }
    return wanted;
}

/** Signature of `clearcore_filter_set_preset` in libclearcore_filter.so (0 on success). */
typedef int (*clearcore_set_preset_fn_t)(void *filter, uint8_t preset);

/**
 * Called once per hop from the realtime callback, in Active mode only. Reads the preset from the
 * shared state (one relaxed atomic load, no syscall) and calls `set_fn` only when it differs from
 * `*applied`. `*applied` is updated only when `set_fn` reports success, so a failed call is retried
 * on the next hop. Does nothing if the library has no `set_fn` (NULL) or there is no filter/state.
 */
static inline void clearcore_state_sync_preset(const clearcore_shared_state_t *state, void *filter,
                                               clearcore_set_preset_fn_t set_fn, int *applied) {
    if (!state || !filter || !set_fn || !applied) {
        return;
    }
    int wanted = clearcore_preset_to_apply(
        atomic_load_explicit(&state->preset, memory_order_relaxed), *applied);
    if (wanted == CLEARCORE_PRESET_INVALID) {
        return;
    }
    if (set_fn(filter, (uint8_t)wanted) == 0) {
        *applied = wanted;
    }
}

#endif /* CLEARCORE_STATE_H */
```

- [ ] **Step 4: Rodar o teste para ver passar**

Run:
```bash
S=<SCRATCHPAD> && gcc -std=gnu11 -Wall -Wextra -Werror -Iplatform/linux/helper/src platform/linux/helper/tests/test_shared_state.c -o $S/test_shared_state && TMPDIR=$S $S/test_shared_state
```
Expected: 10 linhas `[TEST] ...` e `[TEST] all shared state tests passed`, código de saída 0 (medido ao planejar).

- [ ] **Step 5: Ligar o header ao helper**

Em `platform/linux/helper/src/pipewire_helper.h`:

(a) Depois de `#include "noise_suppressor.h"` acrescentar:
```c
#include "clearcore_state.h"
```

(b) **Remover** o bloco que hoje define os modos e a struct (agora vivem em `clearcore_state.h`), isto é, apagar exatamente estas linhas:
```c
/* ClearCore Operating Modes */
#define CLEARCORE_MODE_ACTIVE   0
#define CLEARCORE_MODE_BYPASS   1
#define CLEARCORE_MODE_MUTE     2
#define CLEARCORE_STATE_FILE    "clearcore_state"

/**
 * Shared memory / control state for mode and physical mic binding.
 */
typedef struct clearcore_shared_state {
    _Atomic uint32_t mode;             /* 0=Active, 1=Bypass, 2=Mute */
    _Atomic uint32_t target_node_id;   /* Physical microphone node ID (0=auto) */
    _Atomic uint64_t generation;       /* Engine generation counter */
} clearcore_shared_state_t;
```
e no lugar deixar só o comentário:
```c
/* ClearCore operating modes, the shared state file layout and the studio preset helpers live in
 * clearcore_state.h (no PipeWire dependency, so they can be unit tested without it). */
```

(c) Em `pipewire_helper_context_t`, logo depois de `void (*neural_free_fn)(void *filter);`, acrescentar:
```c
    int (*neural_set_preset_fn)(void *filter, uint8_t preset); /* optional: NULL on an old library */
    int applied_preset;                                        /* CLEARCORE_PRESET_* last applied */
```
(`pipewire_helper_init` já faz `memset(ctx, 0, ...)`, então `applied_preset` começa em `CLEARCORE_PRESET_OFF`, que é o preset inicial de todo filtro novo.)

Em `platform/linux/helper/src/pipewire_helper.c`:

(d) Logo depois da função `init_shared_state` (antes de `/* Core Events Listener for Synchronous Discovery */`), acrescentar:
```c
/* Studio finishing preset: copy the value written by the Electron app into the filter. Runs in the
 * realtime callback, in Active mode only; costs one relaxed atomic load per hop. */
static void apply_studio_preset(pipewire_helper_context_t *ctx) {
    clearcore_state_sync_preset(ctx->shared_state, ctx->neural_filter, ctx->neural_set_preset_fn,
                                &ctx->applied_preset);
}
```

(e) Em `on_capture_process`, trocar o comentário de documentação da função e o bloco de decisão por modo. Substituir:
```c
/**
 * Realtime callback for physical microphone capture.
 * Dequeues captured samples, filters through noise suppressor according to mode,
 * and pushes envelopes into bounded transport ring.
 */
```
por:
```c
/**
 * Realtime callback for physical microphone capture.
 * Dequeues captured samples, filters through noise suppressor according to mode,
 * and pushes envelopes into bounded transport ring.
 *
 * Order of decisions for every hop (the mode is read from the shared state first):
 *   1. Mute   -> digital silence; neither the neural model nor the studio chain runs.
 *   2. Bypass -> sanitized raw frame; neither the neural model nor the studio chain runs.
 *   3. Active -> the studio preset is synchronized, then neural_process_fn runs the model and the
 *                studio chain together (StudioBackend inside libclearcore_filter.so).
 *      Fallback (no library, or rc != 0): noise_suppressor_process, which has NO studio chain
 *      (documented degraded mode, see clearcore_state.h).
 */
```
e, no ramo `Active`, trocar
```c
                if (ctx->neural_filter && ctx->neural_process_fn) {
                    int rc = ctx->neural_process_fn(ctx->neural_filter, raw_frame, processed_frame);
```
por
```c
                if (ctx->neural_filter && ctx->neural_process_fn) {
                    apply_studio_preset(ctx);
                    int rc = ctx->neural_process_fn(ctx->neural_filter, raw_frame, processed_frame);
```

(f) Em `neural_filter_init`, depois de `ctx->neural_free_fn = NULL;` do bloco inicial, acrescentar:
```c
    ctx->neural_set_preset_fn = NULL;
    ctx->applied_preset = CLEARCORE_PRESET_OFF;
```
Depois da linha `free_fn_t free_fn = (free_fn_t)dlsym(lib, "clearcore_filter_free");` acrescentar o símbolo **opcional** (não entra no `if (!create_fn || !process_fn || !free_fn)`):
```c
    /* Optional: libraries built before the studio chain have no preset symbol. */
    clearcore_set_preset_fn_t set_preset_fn =
        (clearcore_set_preset_fn_t)dlsym(lib, "clearcore_filter_set_preset");
    if (!set_preset_fn) {
        fprintf(stderr, "[pipewire_helper] clearcore_filter_set_preset not found; studio presets disabled (Off).\n");
    }
```
E, no final da função, depois de `ctx->neural_free_fn = free_fn;`:
```c
    ctx->neural_set_preset_fn = set_preset_fn;
```

(g) Em `neural_filter_free`, depois de `ctx->neural_free_fn = NULL;` (o do fim da função), acrescentar:
```c
    ctx->neural_set_preset_fn = NULL;
    ctx->applied_preset = CLEARCORE_PRESET_OFF;
```

(h) Em `init_shared_state`, acima de `if (ftruncate(fd, sizeof(clearcore_shared_state_t)) != 0) {`, acrescentar o comentário (o tamanho continua 16 bytes, então nada mais muda):
```c
    /* The file is exactly CLEARCORE_STATE_SIZE (16) bytes, as it has always been. An older file
     * keeps working: offset 12 (preset) is zero there, which means Off. See clearcore_state.h. */
```

- [ ] **Step 6: Alvo de teste no `meson.build` e verificação onde há PipeWire**

Em `platform/linux/helper/meson.build`, no fim do arquivo, acrescentar (sem dependência de PipeWire):
```meson

test_shared_state = executable(
    'test_shared_state',
    'tests/test_shared_state.c',
    include_directories: inc_dir
)

test('shared_state_layout', test_shared_state)
```
Run, **somente numa máquina com `libpipewire-0.3`** (`export PATH=~/miniconda3/bin:$PATH` quando meson/ninja estiverem lá), com diretório de build fora do repositório:
```bash
B=/tmp/clearcore-helper-build && meson setup $B platform/linux/helper && ninja -C $B && meson test -C $B --print-errorlogs
```
Expected: compila sem avisos novos e `meson test` passa 2 testes: `realtime_callback_contract` (já existia) e `shared_state_layout`. Para o `ninja` do CLAUDE.md (`ninja -C platform/linux/helper/build && cargo build --release`) vale o mesmo, usando o diretório de build do repositório. **Neste sandbox este passo não roda** (Decisão 10): registre como "não verificado" no relatório final. Verificação estática que roda aqui:
```bash
grep -n "apply_studio_preset\|neural_set_preset_fn\|applied_preset\|clearcore_filter_set_preset" platform/linux/helper/src/pipewire_helper.c platform/linux/helper/src/pipewire_helper.h
grep -n "uint64_t generation" platform/linux/helper/src/pipewire_helper.h
```
Expected: o primeiro mostra `apply_studio_preset` definida e chamada uma vez, `neural_set_preset_fn` e `applied_preset` no header e nos 3 pontos do `.c` (init, atribuição final, free), e o `dlsym` do símbolo; o segundo **não** mostra a linha `_Atomic uint64_t generation` da struct do estado (ela só existe agora, `uint32_t`, em `clearcore_state.h`; as linhas `uint64_t generation` restantes são as do envelope de transporte).

- [ ] **Step 7: Checkpoint**

Run: `git status --short`
Expected (além do que veio antes): `M platform/linux/helper/meson.build`, `M platform/linux/helper/src/pipewire_helper.c`, `M platform/linux/helper/src/pipewire_helper.h`, `?? platform/linux/helper/src/clearcore_state.h`, `?? platform/linux/helper/tests/test_shared_state.c`. Não comitar.

---

### Task 6: Electron — único escritor do `preset` no `clearcore_state`

**Files:**
- Create: `crates/app-tauri/electron/clearcore-state.cjs`
- Create: `crates/app-tauri/scripts/clearcore-state.selftest.cjs`
- Modify: `crates/app-tauri/electron/main.cjs` (`require`, `writeClearcoreSharedState`, `syncPresetToHelper`, `pollDaemonStatus`)
- Modify: `crates/app-tauri/package.json` (script `test:electron-state`)

**Interfaces:**
- Consumes: o contrato do arquivo da Tarefa 5 (16 bytes, `preset` u32 LE @12); o campo `preset` (string `Off|Natural|Podcast|Broadcast`) que a **Fase 2** põe na resposta de `GetStatus` e o comando `SetPreset` dela. Enquanto a Fase 2 não existe, `GetStatus` não traz `preset`, `syncPresetToHelper(undefined)` não faz nada e o helper roda em `Off`.
- Produces:
  - `electron/clearcore-state.cjs`: `STATE_SIZE = 16`, `PRESET_NAMES`, `isPreset(value) -> boolean`, `encodeState(existing: Buffer|undefined, updates) -> Buffer(16)`, `readPreset(statePath) -> 'Off'|'Natural'|'Podcast'|'Broadcast'|null`, `updateStateFile(statePath, updates) -> Buffer`. `updates` aceita `mode` ('Active'|'Bypass'|'Mute'), `targetNodeId`, `generation`, `preset`.
  - Em `main.cjs`: `clearcoreStatePath()`, `writeClearcoreSharedState(updates)` (mesma assinatura de antes, agora aceita `preset`) e `syncPresetToHelper(preset)`.
- **Quem escreve o quê** (Decisão 9): o Electron copia o preset do serviço (fonte da verdade, `settings.json`) para o byte `@12` na sincronização de status que já roda a cada 2,5 s (`setInterval(pollDaemonStatus, 2500)`) e logo após um `set_preset` bem-sucedido; o helper C só lê. O daemon e o CLI Tauri não escrevem o arquivo. Mudança vinda de fora da UI (por exemplo `--preset` do CLI da Fase 2) chega ao áudio em até 2,5 s.
- A gravação passa a ser **em posição fixa, sem truncar** (`open 'r+'` + `write` na posição 0), no lugar de `fs.writeFileSync`, que trunca para zero antes de regravar: um helper com o arquivo mapeado leva SIGBUS se ler nessa janela (defeito existente) e, com o arquivo encurtado por um escritor antigo, também no byte novo.
- A lógica pura vive em módulo próprio porque `main.cjs` faz `require('electron')` e não é testável fora do Electron; o autoteste roda em `node` puro e fica em `scripts/` (não é copiado para o pacote: `package-app.mjs` copia só `electron/`) e fora do glob do vitest.

Os trechos abaixo foram executados ao planejar (9/9 do autoteste em `node` 24; `node --check` sobre o `main.cjs` modificado).

- [ ] **Step 1: Escrever o autoteste que falha**

Criar `crates/app-tauri/scripts/clearcore-state.selftest.cjs`:

```js
'use strict';

// Self-test for electron/clearcore-state.cjs. Plain node, no test runner:
//   node scripts/clearcore-state.selftest.cjs
const assert = require('node:assert/strict');
const fs = require('fs');
const os = require('os');
const path = require('path');

const state = require('../electron/clearcore-state.cjs');

function bytes(...values) {
  const buf = Buffer.alloc(16, 0);
  values.forEach(([offset, value]) => buf.writeUInt32LE(value, offset));
  return buf;
}

const tests = [];
function test(name, fn) {
  tests.push([name, fn]);
}

test('encodes the mode at offset 0 and keeps the rest zero', () => {
  assert.deepEqual(state.encodeState(undefined, { mode: 'Bypass' }), bytes([0, 1]));
  assert.deepEqual(state.encodeState(undefined, { mode: 'Mute' }), bytes([0, 2]));
  assert.deepEqual(state.encodeState(undefined, { mode: 'Active' }), bytes());
});

test('updating the mode preserves target node, generation and preset', () => {
  const existing = bytes([4, 7], [8, 5], [12, 2]);
  assert.deepEqual(
    state.encodeState(existing, { mode: 'Mute' }),
    bytes([0, 2], [4, 7], [8, 5], [12, 2])
  );
});

test('updating the preset preserves the mode', () => {
  const existing = bytes([0, 1], [4, 7]);
  assert.deepEqual(
    state.encodeState(existing, { preset: 'Broadcast' }),
    bytes([0, 1], [4, 7], [12, 3])
  );
});

test('an unknown preset name is ignored, not turned into Off', () => {
  const existing = bytes([12, 2]);
  assert.deepEqual(state.encodeState(existing, { preset: 'Loud' }), existing);
  assert.deepEqual(state.encodeState(existing, { preset: 2 }), existing);
});

test('target node and generation are written as 32-bit values and never reach the preset', () => {
  const existing = bytes([12, 3]);
  const out = state.encodeState(existing, { targetNodeId: '42', generation: 0xffffffff });
  assert.deepEqual(out, bytes([4, 42], [8, 0xffffffff], [12, 3]));
});

test('only the first 16 bytes of a longer file are used; a shorter one is ignored', () => {
  const longer = Buffer.concat([bytes([12, 1]), Buffer.alloc(8, 0xaa)]);
  assert.deepEqual(state.encodeState(longer, { mode: 'Bypass' }), bytes([0, 1], [12, 1]));
  assert.deepEqual(state.encodeState(Buffer.alloc(8, 0xff), { mode: 'Bypass' }), bytes([0, 1]));
});

test('readPreset returns the name, null for a missing file and null for an invalid value', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'clearcore-state-'));
  try {
    const file = path.join(dir, 'clearcore_state');
    assert.equal(state.readPreset(file), null);
    fs.writeFileSync(file, bytes([12, 2]));
    assert.equal(state.readPreset(file), 'Podcast');
    fs.writeFileSync(file, bytes([12, 9]));
    assert.equal(state.readPreset(file), null);
    fs.writeFileSync(file, Buffer.alloc(8));
    assert.equal(state.readPreset(file), null);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('updateStateFile creates a 16-byte file and never truncates an existing one', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'clearcore-state-'));
  try {
    const file = path.join(dir, 'clearcore_state');
    state.updateStateFile(file, { mode: 'Bypass', preset: 'Natural' });
    assert.equal(fs.statSync(file).size, 16);
    assert.deepEqual(fs.readFileSync(file), bytes([0, 1], [12, 1]));

    // A file that is longer than the contract keeps its size; an in-place write never shrinks it.
    fs.writeFileSync(file, Buffer.concat([bytes([0, 2]), Buffer.alloc(8, 0xaa)]));
    state.updateStateFile(file, { preset: 'Broadcast' });
    const after = fs.readFileSync(file);
    assert.equal(after.length, 24);
    assert.deepEqual(after.subarray(0, 16), bytes([0, 2], [12, 3]));

    // A short file grows to 16 bytes (never the other way around).
    fs.writeFileSync(file, Buffer.alloc(4, 0));
    state.updateStateFile(file, { mode: 'Mute' });
    assert.equal(fs.statSync(file).size, 16);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('the mapped size seen by a reader is stable across writes', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'clearcore-state-'));
  try {
    const file = path.join(dir, 'clearcore_state');
    state.updateStateFile(file, { mode: 'Active' });
    const fd = fs.openSync(file, 'r');
    try {
      for (const preset of state.PRESET_NAMES) {
        state.updateStateFile(file, { preset });
        assert.equal(fs.fstatSync(fd).size, 16, `size changed while writing ${preset}`);
      }
    } finally {
      fs.closeSync(fd);
    }
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

let failed = 0;
for (const [name, fn] of tests) {
  try {
    fn();
    console.log(`ok - ${name}`);
  } catch (error) {
    failed += 1;
    console.error(`not ok - ${name}\n${error.stack}`);
  }
}
console.log(`${tests.length - failed}/${tests.length} passed`);
process.exit(failed === 0 ? 0 : 1);
```

- [ ] **Step 2: Rodar para ver falhar**

Run (a partir de `crates/app-tauri`; `TMPDIR` aponta para o scratchpad para os arquivos temporários do teste):
```bash
cd crates/app-tauri && TMPDIR=<SCRATCHPAD> node scripts/clearcore-state.selftest.cjs
```
Expected: FAIL com `Cannot find module '../electron/clearcore-state.cjs'`.

- [ ] **Step 3: Criar o módulo**

Criar `crates/app-tauri/electron/clearcore-state.cjs`:

```js
'use strict';

// Layout of $XDG_RUNTIME_DIR/clearcore_state, shared with the native PipeWire helper
// (platform/linux/helper/src/clearcore_state.h). 16 bytes, little-endian, never changes size:
//   0  u32 mode            0=Active, 1=Bypass, 2=Mute
//   4  u32 target_node_id  physical microphone node (0=auto)
//   8  u32 generation      unused by the helper
//   12 u32 preset          0=Off, 1=Natural, 2=Podcast, 3=Broadcast (studio finishing)
// The Electron app is the only writer of `preset`.

const fs = require('fs');

const STATE_SIZE = 16;
const OFFSET_MODE = 0;
const OFFSET_TARGET_NODE_ID = 4;
const OFFSET_GENERATION = 8;
const OFFSET_PRESET = 12;

const PRESET_VALUES = Object.freeze({ Off: 0, Natural: 1, Podcast: 2, Broadcast: 3 });
const PRESET_NAMES = Object.freeze(['Off', 'Natural', 'Podcast', 'Broadcast']);

function isPreset(value) {
  return typeof value === 'string' && Object.prototype.hasOwnProperty.call(PRESET_VALUES, value);
}

// Builds the 16-byte buffer: starts from the first 16 bytes of `existing` (when it has at least
// 16) and applies only the fields present in `updates`, so everything else is preserved.
function encodeState(existing, updates = {}) {
  const buf = Buffer.alloc(STATE_SIZE, 0);
  if (Buffer.isBuffer(existing) && existing.length >= STATE_SIZE) {
    existing.copy(buf, 0, 0, STATE_SIZE);
  }
  if (updates.mode !== undefined) {
    const modeVal = updates.mode === 'Bypass' ? 1 : updates.mode === 'Mute' ? 2 : 0;
    buf.writeUInt32LE(modeVal, OFFSET_MODE);
  }
  if (updates.targetNodeId !== undefined) {
    buf.writeUInt32LE(Number(updates.targetNodeId) || 0, OFFSET_TARGET_NODE_ID);
  }
  if (updates.generation !== undefined) {
    buf.writeUInt32LE(Number(updates.generation) || 0, OFFSET_GENERATION);
  }
  if (updates.preset !== undefined && isPreset(updates.preset)) {
    buf.writeUInt32LE(PRESET_VALUES[updates.preset], OFFSET_PRESET);
  }
  return buf;
}

// Preset stored in the file: a name, or null if the file is missing, short or holds a value
// outside 0..3 (the helper ignores such values too).
function readPreset(statePath) {
  let existing;
  try {
    existing = fs.readFileSync(statePath);
  } catch {
    return null;
  }
  if (existing.length < STATE_SIZE) return null;
  const raw = existing.readUInt32LE(OFFSET_PRESET);
  return raw < PRESET_NAMES.length ? PRESET_NAMES[raw] : null;
}

// Rewrites the file IN PLACE: never truncates it. fs.writeFileSync truncates to zero first, and a
// helper that has the file mapped (mmap, MAP_SHARED) faults with SIGBUS when it reads past the end
// of a shortened file. The position-0 write leaves the size untouched (and creates the file with
// the same size the helper expects when it does not exist yet).
function updateStateFile(statePath, updates = {}) {
  let existing;
  try {
    existing = fs.readFileSync(statePath);
  } catch {
    existing = undefined;
  }
  const buf = encodeState(existing, updates);
  const fd = fs.openSync(statePath, existing === undefined ? 'w' : 'r+', 0o666);
  try {
    fs.writeSync(fd, buf, 0, buf.length, 0);
  } finally {
    fs.closeSync(fd);
  }
  return buf;
}

module.exports = {
  STATE_SIZE,
  PRESET_NAMES,
  isPreset,
  encodeState,
  readPreset,
  updateStateFile,
};
```

- [ ] **Step 4: Rodar para ver passar**

Run:
```bash
cd crates/app-tauri && TMPDIR=<SCRATCHPAD> node scripts/clearcore-state.selftest.cjs
```
Expected: 9 linhas `ok - ...` e `9/9 passed`, código de saída 0.

- [ ] **Step 5: Ligar o módulo em `electron/main.cjs`**

(a) Depois de `const { execFile, spawn } = require('child_process');` (linha 6), acrescentar:
```js
const clearcoreState = require('./clearcore-state.cjs');
```

(b) Substituir a função inteira `writeClearcoreSharedState` (do comentário `// Shared Memory state synchronization with native helper` até a `}` que a fecha, imediatamente antes de `function updateTrayMenu() {`) por:
```js
// Shared Memory state synchronization with native helper
function clearcoreStatePath() {
  const runtimeDir = process.env.XDG_RUNTIME_DIR || '/tmp';
  return path.join(runtimeDir, 'clearcore_state');
}

function writeClearcoreSharedState(updates = {}) {
  try {
    clearcoreState.updateStateFile(clearcoreStatePath(), updates);
  } catch (err) {
    console.warn('[Clearcore] Could not update shared state:', err.message);
  }
}

// The service owns the studio preset (settings.json); the native helper only reads the copy kept in
// the shared state file. Call it with the preset reported by GetStatus and after SetPreset.
// `preset` is undefined while the service predates the preset field: then nothing is written.
function syncPresetToHelper(preset) {
  if (process.platform !== 'linux' || !clearcoreState.isPreset(preset)) return;
  if (clearcoreState.readPreset(clearcoreStatePath()) !== preset) {
    writeClearcoreSharedState({ preset });
  }
}

```
(As chamadas existentes `writeClearcoreSharedState({ mode })`, `({ mode: currentMode })`, `({ mode: 'Active' | 'Bypass' | 'Mute' })` e `({ targetNodeId: deviceId })` continuam como estão: mesma assinatura, mesmos bytes.)

(c) Em `pollDaemonStatus`, dentro de `if (res && res.mode) { ... }`, logo depois do bloco `if (res.mode !== currentMode) { ... }`, acrescentar:
```js
      syncPresetToHelper(res.preset);
```
ficando:
```js
    if (res && res.mode) {
      currentStatus = res;
      if (res.mode !== currentMode) {
        currentMode = res.mode;
        updateTrayMenu();
      }
      syncPresetToHelper(res.preset);
    }
```

- [ ] **Step 6: Script npm e verificação de sintaxe**

Em `crates/app-tauri/package.json`, em `"scripts"`, logo depois da linha `"test": "vitest run",`:
```json
    "test:electron-state": "node scripts/clearcore-state.selftest.cjs",
```
Run:
```bash
cd crates/app-tauri && node --check electron/main.cjs && node --check electron/clearcore-state.cjs && TMPDIR=<SCRATCHPAD> npm run test:electron-state --silent
```
Expected: sem erro de sintaxe e `9/9 passed`. Confirme também que o resto do frontend continua verde: `cd crates/app-tauri && npx tsc --noEmit && npm test` (nada em `src/` foi alterado; se a linha de base já estava vermelha, ver Tarefa 1).

- [ ] **Step 7: Ligação com o `set_preset` da Fase 2 (condicional)**

O plano da Fase 2 (Tarefa 6) adiciona em `main.cjs`, antes de `ipcMain.handle('restart_generation', ...)`, o handler:
```js
ipcMain.handle('set_preset', async (_event, args) => {
  const preset = args && args.preset ? args.preset : 'Off';
  return await sendIpcRequest({ SetPreset: preset });
});
```
Run: `grep -n "ipcMain.handle('set_preset'" crates/app-tauri/electron/main.cjs`.
- Se **não** houver ocorrência (Fase 2 ainda não executada): nada a fazer aqui; a sincronização periódica do Passo 5(c) cobre o caso e a Fase 2 deve, ao criar o handler, incluir a chamada abaixo (este plano a registra como pendência de integração no relatório final).
- Se houver (Fase 2 já executada): trocar o corpo do handler por
```js
ipcMain.handle('set_preset', async (_event, args) => {
  const preset = args && args.preset ? args.preset : 'Off';
  const res = await sendIpcRequest({ SetPreset: preset });
  syncPresetToHelper(res && res.preset ? res.preset : preset);
  return res;
});
```
(a resposta do serviço traz `preset`; o fallback para o valor pedido só vale se a resposta vier sem o campo). Rode de novo `node --check electron/main.cjs`.

- [ ] **Step 8: Checkpoint**

Run: `git status --short`
Expected (além do que veio antes): `M crates/app-tauri/electron/main.cjs`, `M crates/app-tauri/package.json`, `?? crates/app-tauri/electron/clearcore-state.cjs`, `?? crates/app-tauri/scripts/clearcore-state.selftest.cjs`. Não comitar.

---

### Task 7: Documentação da latência/modo degradado e gate de fechamento da Fase 1b

**Files:**
- Modify: `docs/support-matrix.md` (seção "Audio Pipeline Specifications")
- Nenhum outro arquivo; esta tarefa só documenta e roda o gate.

**Interfaces:**
- Consumes: tudo das Tarefas 2 a 6 (latência 1.440 + 96 = 1.536; `Off` por padrão; preset no byte 12 do `clearcore_state`; modo degradado do helper).
- Produces: nada que outra tarefa use; fecha a fase.

- [ ] **Step 1: Atualizar a documentação de pipeline**

Em `docs/support-matrix.md` (arquivo em inglês, mantenha o idioma do arquivo), trocar a linha
```markdown
- **Algorithmic Latency:** 30.0 ms (1,440 samples @ 48 kHz).
```
por:
```markdown
- **Algorithmic Latency:** 30.0 ms (1,440 samples @ 48 kHz) for the neural model. When the studio finishing chain is attached it adds 2.0 ms (96 samples, limiter lookahead), reported as 1,536 samples regardless of preset (the chain reports its lookahead even in `Off`, where the signal is not actually delayed).
- **Studio Finishing:** optional DSP chain (`studio-dsp`) applied after the neural model and selected by preset: `Off` (default, bit-exact passthrough), `Natural`, `Podcast`, `Broadcast`. It runs only in `Active` mode; `Bypass` and `Mute` never reach it. On Linux the preset reaches the PipeWire helper through byte offset 12 of `$XDG_RUNTIME_DIR/clearcore_state` (layout in `platform/linux/helper/src/clearcore_state.h`, written only by the Electron app). Degraded mode: if the helper cannot load `libclearcore_filter.so`, or a frame fails inference, it falls back to the built-in DSP suppressor, which applies no studio preset.
```

- [ ] **Step 2: Formatação**

Run:
```bash
source <SCRATCHPAD>/rust-env/env.sh && cargo fmt --all -- --check
```
Expected: sem saída.

- [ ] **Step 3: Clippy (comando do CI)**

Run:
```bash
source <SCRATCHPAD>/rust-env/env.sh && cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
```
Expected: `Finished` sem avisos nem erros.

- [ ] **Step 4: Build e testes do workspace (comandos do CI)**

Run:
```bash
source <SCRATCHPAD>/rust-env/env.sh && cargo build --workspace --locked --offline && cargo test --workspace --locked --offline
```
Expected: todos os testes passam. Contagens esperadas das partes desta fase: `realtime-noise-model` +8 unitários (`studio_backend`) e +2 (`studio_contract`); `realtime-noise-engine` +4 unitários e +8 em `tests/studio.rs`; `realtime-noise-filter-capi` 5 testes (antes 0). Qualquer teste pré-existente que falhe (`crates/model/tests`, `crates/accelerators/tests`, `crates/tools`) é regressão desta fase: eles usam o backend interno sem estúdio e devem continuar com 1.440 de latência.

- [ ] **Step 5: Gate offline**

Run: `scripts/check-offline.sh`
Expected: termina com status 0 (se imprimir `BLOCKED_OFFLINE_DEPENDENCY`, o gate acusou um crate fora do cache offline: reporte o nome; esta fase não adiciona nenhum crate de terceiros).

- [ ] **Step 6: Lockfile e escopo**

Run:
```bash
git diff Cargo.lock | grep '^[+-]' | grep -v '^+++\|^---'
git status --short
```
Expected: o `Cargo.lock` só ganha linhas `+ "studio-dsp",` nas listas de dependências de `realtime-noise-model`, `realtime-noise-engine` e `realtime-noise-filter-capi` (e o pacote `studio-dsp` em si, vindo da Fase 1a); nenhuma versão de terceiros muda. O `git status` lista apenas os arquivos dos Checkpoints das Tarefas 2 a 7, mais os da Fase 1a/2 e os documentos de `docs/superpowers/`; nada em `target/`, nada fora do escopo, e `platform/linux/helper/build/` intacto.

- [ ] **Step 7: Os dois testes que não dependem de Rust**

Run:
```bash
S=<SCRATCHPAD> && gcc -std=gnu11 -Wall -Wextra -Werror -Iplatform/linux/helper/src platform/linux/helper/tests/test_shared_state.c -o $S/test_shared_state && TMPDIR=$S $S/test_shared_state && (cd crates/app-tauri && TMPDIR=$S npm run test:electron-state --silent && npx tsc --noEmit && npm test)
```
Expected: `all shared state tests passed`, `9/9 passed`, `tsc` sem saída e o vitest verde.

- [ ] **Step 8: O que NÃO foi verificado (registrar no relatório de fechamento)**

Reporte explicitamente, sem afirmar sucesso:
1. `ninja -C platform/linux/helper/build` e `meson test`: só rodam onde há `libpipewire-0.3`; neste ambiente falha na configuração (Tarefa 1, Passo 5). As edições de `pipewire_helper.[ch]` (Tarefa 5, Passo 5) ficam sem compilação até lá.
2. Teste ponta a ponta no áudio real do Linux (preset escolhido na UI chegando ao helper e mudando o som): exige PipeWire, microfone e a Fase 2; não existe verificação automatizada aqui.
3. Qualquer medição de latência fim a fim ou de CPU com a cadeia ligada (spec §10: nunca medido); os 96 samples são o contrato da 1a, não uma medição.

- [ ] **Step 9: Checkpoint final**

Run: `git status --short`
Expected: a lista da Fase 1b completa (Tarefas 2 a 7) e nada mais. **Não comitar e não fazer push.**

---

## Self-Review

**1. Spec coverage (seção 5 e pendências da seção 10):**
- Decorator `StudioBackend<B>` com `process` = interno + cadeia e latência somada, trait inalterada → Tarefa 2 (`StudioBackend`, `impl ... for Box<T>` só acrescenta implementação).
- "O engine e o `filter-capi` apenas embrulham o backend" → Tarefa 3 (`with_studio`, `set_backend`) e Tarefa 4 (`create`).
- "Modos tratados antes do backend (conferir no helper C)" → conferido e documentado na Tarefa 5 (comentário de ordem em `on_capture_process`), travado por testes no engine (Tarefa 3: `bypass_...`, `mute_...`).
- "Reinício de geração do engine zera o estado do DSP" → Tarefa 3 (`advance_generation` + `StudioResetHandle`; testes unitários nos três fechamentos do worker, no `begin_generation_restart` e o ponta a ponta).
- "Troca de preset por valor atômico compartilhado ... `filter-capi` ganha `clearcore_filter_set_preset`" → Tarefa 4 (a atomicidade e o crossfade são da 1a, em `StudioControl`/`StudioChain`; aqui só se lê `control.preset()` por frame).
- "Fallback do helper C: sem estúdio. Modo degradado documentado" → Tarefa 5 (`clearcore_state.h` e comentário do callback) e Tarefa 7 (`docs/support-matrix.md`).
- Pendência §10 "Lacuna do preset no Linux" → Tarefas 5 e 6 (struct de 16 bytes com `preset` @12, compatível nos dois sentidos; Electron único escritor; helper lê e chama `clearcore_filter_set_preset`).
- Spec §9 gate → Tarefa 7. Sem commit/push → todos os checkpoints.
- Lacuna conhecida e declarada: o preset só chega ao Linux quando a Fase 2 expõe `preset` em `GetStatus`; antes disso o helper roda em `Off` (comportamento de hoje). O caminho do engine Rust não tem chamador de produção, então `with_studio` fica disponível mas sem uso até alguém montar o engine real.

**2. Placeholder scan:** sem "TBD/TODO/implementar depois"; todo passo de código traz o código; os dois pontos condicionais (Tarefa 6, Passo 7; Tarefa 1, Passo 5) trazem as duas alternativas com comandos e texto exatos.

**3. Type consistency:** `StudioResetHandle::{new, request, is_requested}` e `StudioBackend::{new, with_reset_handle, reset_handle}` (Tarefa 2) são os usados na Tarefa 3 (`StudioAttachment`, testes do worker) e na Tarefa 4 (`StudioBackend::new`). `Preset::from_u8`/`StudioControl::{new, set_preset, preset}` aparecem com a mesma assinatura da Fase 1a em todas as tarefas. O ABI `clearcore_filter_set_preset(filter, preset: u8) -> i32` (0/-1/-3) da Tarefa 4 coincide com `clearcore_set_preset_fn_t` (`int (*)(void*, uint8_t)`) e com `neural_set_preset_fn` da Tarefa 5. Os valores de preset 0..3 e os nomes `Off|Natural|Podcast|Broadcast` coincidem entre `Preset::as_u8` (1a), `CLEARCORE_PRESET_*` (Tarefa 5), `PRESET_VALUES`/`PRESET_NAMES` (Tarefa 6) e `StudioPreset` da Fase 2. O offset do preset (12) e o tamanho (16) coincidem entre `clearcore_state.h`, o teste C e `clearcore-state.cjs`.
