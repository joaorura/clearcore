# Clearcore Studio — Fase 2 (settings.json, IPC de preset e UI) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persistir o preset de acabamento de estúdio em um `settings.json` versionado, aplicá-lo ao `StudioControl` na subida do serviço, expô-lo por um comando IPC `SetPreset` (e no `GetStatus`) e oferecer um seletor Off/Natural/Podcast/Broadcast na UI, em en-US e pt-BR.

**Architecture:** O `ServiceDaemon` passa a ser o dono do `Arc<StudioControl>` (criado a partir do `Settings` carregado na subida) e o expõe por `studio_control()`; o IPC só fala o tipo de fio `StudioPreset` (crate `ipc`, sem dependência do `studio-dsp`) e o serviço converte para `studio_dsp::Preset`. `SetPreset` segue exatamente o padrão de `SetMode` (variante nova de `IpcCommand`, resposta `{preset, success, persisted}`) e é aditivo ao protocolo v1. A UI reaproveita a ponte existente (`invokeBridge` → `window.clearcoreApi` do Electron, ou `invoke` do Tauri) e extrai a lógica testável para um módulo puro, porque o frontend só tem vitest em ambiente node.

**Tech Stack:** Rust 1.90 (edition 2024, workspace lints `unwrap/expect/panic = deny`, clippy pedantic+nursery com `-D warnings`), `serde` 1.0.229 e `serde_json` 1.0.145 já fixados, React 18 + TypeScript 5.6 + vitest 2.1.4, Electron (processo principal em `electron/main.cjs`). Nenhum crate nem pacote npm novo.

**Spec:** `docs/superpowers/specs/2026-10-01-clearcore-studio-pipeline-design.md` (seção 6 "Configuração, enrollment e UI", seção 9 "Ordem de entrega e execução"). Esta é a **Fase 2**; depende da Fase 1 (`studio-dsp` + `StudioBackend`), planejada à parte.

## Global Constraints

Copiados verbatim do spec, mais as restrições impostas pelo usuário para esta fase:

- Spec §6, IPC: "`SetPreset { preset }` e `SetVoiceProfile { profile | clear }`; `GetStatus` informa preset e presença de perfil. Se o protocolo v1 admite variantes novas ou exige v2 (com novos golden em `fixtures/wire/`), decide-se lendo `docs/ipc-v1.md` na fase 2." (Nesta fase: **só** `SetPreset` e o campo `preset` do `GetStatus`; `SetVoiceProfile` e presença de perfil são da fase 4.)
- Spec §6, persistência: "`settings.json` versionado, escrita atômica (temporário + `rename`), carregado na subida; o `ServiceConfig` ganha `settings_path`. Padrão: preset `Off`, sem perfil."
- Spec §6, UI: "`set_preset`, `enroll_voice`, `clear_voice_profile`; cards de preset e de cadastro; strings em `en-US` e `pt-BR`." (Nesta fase: só `set_preset` e o card de preset.)
- Spec §4: "Presets: `Off` (**passagem bit a bit idêntica**), `Natural`, `Podcast`, `Broadcast`, em tabela constante. [...] Padrão do produto: `Off`."
- Spec §4: "em conformidade com a política do workspace (lints `unwrap/expect/panic = deny`)".
- Spec §5: "Troca de preset por valor atômico compartilhado, aplicada no limite de frame com crossfade de ~10 ms." (Implementado na Fase 1 dentro de `StudioControl`; esta fase só chama `set_preset`.)
- Spec §1: "O produto nunca fica pior do que é hoje: sem asset novo aprovado, roda DFNet3 v1 + DSP."
- Spec §9: "Cada fase só fecha com `cargo fmt --check`, `clippy -D warnings`, `cargo test --locked --offline` e `scripts/check-offline.sh` passando, mais revisão de código. Implementação e revisão por agentes em **Sonnet e Haiku** [...]. Execução autônoma: **sem commit e sem push** (não foram pedidos); cada tarefa termina com um checkpoint (`git status --short`), e as fases paralelas trabalham em arquivos disjuntos do mesmo working tree."
- Política do workspace (`Cargo.toml`): `unsafe_code = "forbid"`; `clippy::all = deny`; `pedantic` e `nursery` em `warn` (viram erro com `-D warnings`); `unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented` em `deny`. Testes de integração usam `#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]` como os existentes.
- Dependências: nenhum crate nem pacote npm de terceiros novo (`serde =1.0.229` e `serde_json =1.0.145` já fixados). A única aresta nova é o path dependency `studio-dsp` em `crates/service`.
- Idioma: respostas e documentação em português do Brasil; identificadores e código em inglês; strings pt-BR com acentuação correta.
- Comandos de gate (CI, `.github/workflows/ci.yml`): `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings`; `cargo test --workspace --locked --offline`; `scripts/check-offline.sh`.
- **Sem commit e sem push.** Onde um plano normal comitaria, esta fase faz um "Checkpoint" (`git status --short`).

---

## Decisões de desenho (com a evidência lida no código)

1. **IPC: `SetPreset` é variante aditiva da v1; não exige v2.** `docs/ipc-v1.md` §1, item 2: "**Strict Protocol Versioning:** The protocol is version-tagged (`realtime-noise.v1`). Incompatible clients are rejected fail-closed with `VersionMismatch`." e §4: "`VersionMismatch`: Client sent an incompatible `version` string. Connection fail-closed." O código confirma que a incompatibilidade é decidida só pela igualdade da string de versão (`handle_request` em `crates/ipc/src/protocol.rs`: `if req.version != PROTOCOL_VERSION { return IpcResponse::version_mismatch(...) }`). O documento não proíbe variantes novas nem campos novos de payload. Consequências verificadas por leitura do código: (a) daemon antigo + cliente novo: `serde` falha no `IpcRequest::from_json` e o servidor responde `InvalidCommand`/`JSON_PARSE_ERROR` (falha fechada, já coberta pelo teste `malformed_json_returns_invalid_command`); (b) daemon novo + cliente antigo: nada muda, os comandos existentes continuam com a mesma forma; (c) o campo novo `preset` no `GetStatus` é ignorado por quem não o lê (o Electron só lê `res.mode`; o TS `EngineStatus` ganha o campo como opcional). Como o doc não tem regra de evolução, a Tarefa 2 adiciona a seção "Compatibility & Evolution" a `docs/ipc-v1.md` registrando: variante de comando e campo de payload novos são aditivos na v1; renomear, remover ou mudar a semântica de algo existente exige v2.
2. **Não existem "golden de fio" binários para o IPC.** `fixtures/wire/frame-v1-valid.bin` e `frame-v1-invalid-reserved.bin` são o envelope de **áudio** do crate `contracts` (`crates/contracts/tests/wire_v1.rs`, lidos por `include_bytes!`; não há gerador no repositório, são arquivos estáticos). O IPC é JSON de uma linha terminada em `\n`. Portanto os "golden novos" desta fase são arquivos JSON em `fixtures/wire/` (`ipc-v1-*.json`) lidos por `include_str!` em testes, no mesmo padrão. Não há comando de geração: são escritos à mão e a verificação é o teste.
3. **Quem cria o `Arc<StudioControl>`:** o `ServiceDaemon`. Evidência: hoje o serviço **não instancia nenhum `DenoiseEngine`** (`ServiceDaemon` guarda só `EngineSupervisor` + `IpcServer`; `DenoiseEngine` só aparece no crate `engine` e nos seus testes), e `SetMode` apenas muda `EngineSupervisor::mode`. Logo o único ponto de vida longa e comum a IPC e subida é o `ServiceDaemon`. Ele cria `Arc::new(StudioControl::new(settings.preset))` em `with_settings`, o `SetPreset` chama `studio.set_preset(..)`, o `GetStatus` lê `studio.preset()` (a fonte da verdade é o que o áudio lê) e `studio_control()` entrega um clone do `Arc` para quem montar o `StudioBackend::new(inner, Arc<StudioControl>)` (engine ou `filter-capi`). Ordem no `SetPreset`: aplica ao `StudioControl` primeiro e depois persiste; se a persistência falhar, o preset continua aplicado e a resposta traz `"persisted": false` (a UI avisa).
4. **Tipo de fio separado, sem `serde` no `studio-dsp`.** O crate `studio-dsp` é Rust puro sem dependências; `studio_dsp::Preset` não tem `Serialize`. O `ipc` define `StudioPreset` (PascalCase, igual a `DenoiseMode`) e o `service` converte (`convert_ipc_preset_to_dsp` / `convert_dsp_preset_to_ipc`), o mesmo padrão de `convert_ipc_mode_to_engine`. O `settings.json` reutiliza `StudioPreset` como tipo de arquivo, então há uma única fonte dos nomes (`Off`, `Natural`, `Podcast`, `Broadcast`).
5. **Preset fora do export de diagnósticos.** `DiagnosticPayload` é uma lista fechada de campos (`crates/diagnostics/src/export.rs`) e nem o `mode` entra nela; acrescentar o preset mudaria o schema `realtime-noise.diagnostics.v1`. Decisão: nada muda no crate; a Tarefa 4 acrescenta um teste-guarda de regressão em `crates/diagnostics/tests/privacy.rs`.

## Divergências entre o spec e o código real (levantadas ao planejar)

- O spec diz "novos golden em `fixtures/wire/`" supondo golden binário; o IPC é JSON e os `.bin` existentes são do envelope de áudio (decisão 2).
- O spec §2 diz que o IPC tem `GetStatus, SetMode, RestartGeneration, GetDiagnostics, Shutdown` e que o modo "fica em memória". Confirmado, e há mais: no Linux o modo que **de fato** governa o áudio vive no arquivo `$XDG_RUNTIME_DIR/clearcore_state` (16 bytes: `mode` u32 @0, `target_node_id` u32 @4, `generation` u64 @8, struct `clearcore_shared_state_t` em `platform/linux/helper/src/pipewire_helper.h`), escrito pelo Electron (`writeClearcoreSharedState` em `electron/main.cjs`) e lido pelo helper C. O `SetMode` do serviço só altera o supervisor.
- O "app Tauri" (`crates/app-tauri/src-tauri`) é um CLI fino sobre o IPC (`commands.rs` + `main.rs`, sem `#[tauri::command]`); a UI real que roda é Electron (`electron/main.cjs` + `preload.cjs`) servindo o React de `src/`. Por isso a UI desta fase precisa tocar também `electron/main.cjs` e `electron/preload.cjs`, além do `commands.rs` pedido.
- O frontend não tem jsdom nem testing-library; só `vitest run` em ambiente node, com um único teste (`src/i18n/__tests__/i18n.test.ts`). Não há script de lint no `package.json`; o "lint" disponível é `tsc --noEmit` (primeiro passo do `npm run build`).

## Pendência fora do escopo (não resolvida por este plano)

**Preset até o áudio no caminho do helper C (Linux).** Este plano entrega o valor de preset correto e persistido dentro do serviço (`ServiceDaemon::studio_control()`), mas o serviço hoje não hospeda o engine, e o helper C só recebe do mundo externo o arquivo `clearcore_state`, cuja struct (16 bytes) não tem campo de preset e é criada com `ftruncate(sizeof(...))`. Fazer o preset escolhido na UI chegar a `clearcore_filter_set_preset` (Fase 1) exige estender essa struct no C e no Electron, ou fazer o helper consultar o serviço. Isso toca `platform/linux/helper` e deve ser decidido com o plano da Fase 1 (que é dona de `filter-capi`); não é feito aqui.

## File Structure

| Arquivo | Responsabilidade |
|---|---|
| `crates/ipc/src/protocol.rs` (modificar) | Tipo de fio `StudioPreset`; variante `IpcCommand::SetPreset(StudioPreset)`. |
| `crates/ipc/src/lib.rs` (modificar) | Reexporta `StudioPreset`. |
| `crates/ipc/tests/set_preset.rs` (criar) | Golden do request, round-trip, preset inválido, regressão dos comandos v1. |
| `fixtures/wire/ipc-v1-set-preset-request.json` (criar) | Golden do request `SetPreset`. |
| `fixtures/wire/ipc-v1-set-preset-response.json` (criar) | Golden da resposta `SetPreset`. |
| `fixtures/wire/ipc-v1-get-status-response.json` (criar) | Golden da resposta `GetStatus` com `preset`. |
| `docs/ipc-v1.md` (modificar) | Documenta `SetPreset`, `preset` no `GetStatus` e as regras de evolução da v1. |
| `crates/service/Cargo.toml` (modificar) | Path dependency `studio-dsp`. |
| `Cargo.lock` (modificar) | Nova aresta `realtime-noise-service` → `studio-dsp` (atualizada offline). |
| `crates/service/src/settings.rs` (criar) | `Settings`, leitura tolerante, escrita atômica 0600, conversões de preset, caminho padrão. |
| `crates/service/src/lib.rs` (modificar) | `ServiceDaemon` dono do `Arc<StudioControl>`; handlers `SetPreset`/`GetStatus`. |
| `crates/service/src/bootstrap.rs` (modificar) | `ServiceConfig.settings_path`; carrega `Settings` na subida. |
| `crates/service/tests/common/mod.rs` (criar) | `TempDir` e `send` compartilhados pelos testes. |
| `crates/service/tests/settings.rs` (criar) | Testes do módulo `settings`. |
| `crates/service/tests/preset_ipc.rs` (criar) | Testes ponta a ponta do daemon (SetPreset, persistência, golden). |
| `docs/service-lifecycle.md` (modificar) | Seção 6: settings e dono do `StudioControl`. |
| `crates/diagnostics/tests/privacy.rs` (modificar) | Teste-guarda: preset/settings fora do export. |
| `crates/app-tauri/src-tauri/src/commands.rs` (modificar) | `set_preset`, `parse_preset_arg`. |
| `crates/app-tauri/src-tauri/src/main.rs` (modificar) | Opção de CLI `--preset`. |
| `crates/app-tauri/src/types.ts` (modificar) | Tipo `Preset`; `EngineStatus.preset?`. |
| `crates/app-tauri/src/preset.ts` (criar) | Lógica pura: `PRESETS`, `isPreset`, `presetFromStatus`, chaves i18n. |
| `crates/app-tauri/src/bridge.ts` (modificar) | `setPreset` na interface e rota `set_preset`. |
| `crates/app-tauri/electron/preload.cjs`, `electron/main.cjs` (modificar) | Expõe `setPreset` e trata `set_preset` via IPC. |
| `crates/app-tauri/src/i18n/types.ts`, `locales/en-US.ts`, `locales/pt-BR.ts` (modificar) | Bloco `presets`. |
| `crates/app-tauri/src/__tests__/preset.test.ts`, `bridge.test.ts` (criar) | Testes vitest. |
| `crates/app-tauri/src/PresetCard.tsx` (criar) | Card com o seletor. |
| `crates/app-tauri/src/App.tsx`, `src/styles.css` (modificar) | Estado, handler e estilos do card. |

---

### Task 1: Pré-condição (crate `studio-dsp` da Fase 1) e linha de base

**Files:**
- Nenhum arquivo é alterado nesta tarefa.

**Interfaces:**
- Consumes (criadas pelo plano da Fase 1, nomes exatos):
  - `studio_dsp::Preset`: `#[derive(Clone, Copy, Debug, PartialEq, Eq)] #[repr(u8)] enum { Off = 0, Natural = 1, Podcast = 2, Broadcast = 3 }`, `Preset::from_u8(u8) -> Option<Preset>`, `Preset::as_u8(self) -> u8`.
  - `studio_dsp::StudioControl`: `new(Preset)`, `set_preset(&self, Preset)`, `preset(&self) -> Preset`; usado como `Arc<StudioControl>`; engine e `filter-capi` o recebem via `StudioBackend::new(inner, Arc<StudioControl>)` (crate `model`).
- Produces: nada. Confirma que as Tarefas 3 e 4 podem usar `studio-dsp` como dependência de path (`studio-dsp = { path = "../studio-dsp" }`).

Decisões de desenho desta fase: ver a seção "Decisões de desenho" acima (em particular a 3, "Quem cria o `Arc<StudioControl>`"). Esta tarefa não escreve código; ela garante que a premissa vale antes de gastar trabalho.

- [ ] **Step 1: Verificar que o crate existe e está no workspace**

Run (na raiz do worktree):
```bash
ls crates/studio-dsp/Cargo.toml && grep -n '"crates/studio-dsp"' Cargo.toml
```
Expected: o caminho `crates/studio-dsp/Cargo.toml` é listado e a linha `"crates/studio-dsp",` aparece em `members`. **Se qualquer um falhar, PARE e reporte** que a Fase 1 ainda não entregou o crate; não crie o crate aqui.

- [ ] **Step 2: Verificar nome do pacote e da biblioteca**

Run:
```bash
cargo metadata --locked --offline --format-version 1 --no-deps | python3 -c "import json,sys; m=json.load(sys.stdin); [print(p['name'], [t['name'] for t in p['targets'] if 'lib' in t['kind']]) for p in m['packages'] if p['manifest_path'].endswith('crates/studio-dsp/Cargo.toml')]"
```
Expected: `studio-dsp ['studio_dsp']`. Se o nome do pacote for outro, PARE e reporte: as linhas de `Cargo.toml` e os `use studio_dsp::...` deste plano assumem exatamente `studio-dsp` / `studio_dsp`.

- [ ] **Step 3: Verificar as assinaturas consumidas**

Run:
```bash
grep -rn "pub enum Preset\|repr(u8)\|pub const fn from_u8\|pub fn from_u8\|pub const fn as_u8\|pub fn as_u8\|pub struct StudioControl\|pub fn set_preset\|pub fn preset" crates/studio-dsp/src
```
Expected: aparecem `pub enum Preset`, `#[repr(u8)]`, `from_u8`, `as_u8`, `pub struct StudioControl`, `set_preset` e `preset`. Confirme ainda, lendo o arquivo, que `Preset` deriva `Clone, Copy, Debug, PartialEq, Eq` e que `StudioControl::set_preset` e `preset` recebem `&self` (o `Arc` é compartilhado, sem `&mut`). Divergência: PARE e reporte.

- [ ] **Step 4: Linha de base verde antes de qualquer alteração**

Run:
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo test --workspace --locked --offline
(cd crates/app-tauri && npx tsc --noEmit && npm test)
```
Expected: tudo passa (no estado verificado ao planejar: `npm test` = 1 arquivo, 6 testes; `tsc --noEmit` sem saída). Se a linha de base já estiver vermelha, PARE e reporte o que falhou: não é causado por esta fase.

- [ ] **Step 5: Checkpoint**

Run: `git status --short`
Expected: só aparecem arquivos da Fase 1 e, se ainda não comitado, o spec e este plano em `docs/superpowers/`; nenhum arquivo desta fase foi tocado. Não comitar.

---

### Task 2: IPC — `StudioPreset`, `SetPreset`, golden e documentação

**Files:**
- Modify: `crates/ipc/src/protocol.rs`
- Modify: `crates/ipc/src/lib.rs`
- Modify: `crates/service/src/lib.rs` (apenas um braço provisório no `match`, ver Step 5)
- Modify: `docs/ipc-v1.md`
- Create: `crates/ipc/tests/set_preset.rs`
- Create: `fixtures/wire/ipc-v1-set-preset-request.json`

**Interfaces:**
- Consumes: nada de outras tarefas (esta tarefa independe do `studio-dsp`).
- Produces:
  - `realtime_noise_ipc::StudioPreset` — `#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)] #[serde(rename_all = "PascalCase")] enum { #[default] Off, Natural, Podcast, Broadcast }`, reexportado na raiz do crate.
  - `IpcCommand::SetPreset(StudioPreset)`; forma de fio: `"command":{"SetPreset":"Podcast"}`.
  - Preset inválido ou com caixa errada (`"Loud"`, `"podcast"`) falha em `IpcRequest::from_json`, e `IpcServer::handle_line` responde `InvalidCommand` com código `JSON_PARSE_ERROR` sem chamar o handler (mesmo comportamento já existente para `SetMode` inválido).

- [ ] **Step 1: Criar o golden do request**

Criar `fixtures/wire/ipc-v1-set-preset-request.json` com exatamente este conteúdo (uma linha, terminada em `\n`):

```json
{"version":"realtime-noise.v1","request_id":"golden-set-preset","command":{"SetPreset":"Podcast"},"payload":{}}
```

- [ ] **Step 2: Escrever os testes que falham**

Criar `crates/ipc/tests/set_preset.rs`:

```rust
#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use realtime_noise_ipc::{
    DenoiseMode, IpcCommand, IpcRequest, IpcResponse, IpcServer, IpcStatus, PROTOCOL_VERSION,
    StudioPreset,
};
use serde_json::json;

const SET_PRESET_REQUEST: &str =
    include_str!("../../../fixtures/wire/ipc-v1-set-preset-request.json");

#[test]
fn set_preset_request_matches_golden_wire_bytes() {
    let request = IpcRequest {
        version: PROTOCOL_VERSION.to_string(),
        request_id: "golden-set-preset".to_string(),
        command: IpcCommand::SetPreset(StudioPreset::Podcast),
        payload: json!({}),
    };

    assert_eq!(request.to_json().unwrap(), SET_PRESET_REQUEST.trim_end());
    assert_eq!(
        IpcRequest::from_json(SET_PRESET_REQUEST.trim_end()).unwrap(),
        request
    );
}

#[test]
fn every_preset_round_trips_through_the_wire_format() {
    let cases = [
        (StudioPreset::Off, "Off"),
        (StudioPreset::Natural, "Natural"),
        (StudioPreset::Podcast, "Podcast"),
        (StudioPreset::Broadcast, "Broadcast"),
    ];
    for (preset, name) in cases {
        let request = IpcRequest {
            version: PROTOCOL_VERSION.to_string(),
            request_id: "rt".to_string(),
            command: IpcCommand::SetPreset(preset),
            payload: json!({}),
        };
        let wire = request.to_json().unwrap();
        assert!(
            wire.contains(&format!("\"command\":{{\"SetPreset\":\"{name}\"}}")),
            "unexpected wire form: {wire}"
        );
        assert_eq!(IpcRequest::from_json(&wire).unwrap(), request);
    }
}

#[test]
fn preset_defaults_to_off() {
    assert_eq!(StudioPreset::default(), StudioPreset::Off);
}

#[test]
fn invalid_preset_is_rejected_without_reaching_the_handler() {
    let server = IpcServer::new();
    for bad in ["Loud", "podcast", "", "Off "] {
        let raw = format!(
            "{{\"version\":\"realtime-noise.v1\",\"request_id\":\"bad\",\"command\":{{\"SetPreset\":\"{bad}\"}},\"payload\":{{}}}}"
        );
        let mut handler_called = false;
        let response_json = server.handle_line(&raw, |_cmd, _payload| {
            handler_called = true;
            IpcResponse::success("x", json!({}))
        });

        assert!(!handler_called, "handler must not run for preset {bad:?}");
        let response = IpcResponse::from_json(&response_json).unwrap();
        assert_eq!(response.status, IpcStatus::InvalidCommand);
        let error = response.error.unwrap();
        assert_eq!(error.code, "JSON_PARSE_ERROR");
        assert!(
            error.message.contains("unknown variant"),
            "unexpected message: {}",
            error.message
        );
    }
}

#[test]
fn existing_v1_commands_keep_their_wire_form() {
    let cases = [
        r#"{"version":"realtime-noise.v1","request_id":"a","command":"GetStatus","payload":{}}"#,
        r#"{"version":"realtime-noise.v1","request_id":"b","command":{"SetMode":"Bypass"},"payload":{}}"#,
        r#"{"version":"realtime-noise.v1","request_id":"c","command":"RestartGeneration","payload":{}}"#,
        r#"{"version":"realtime-noise.v1","request_id":"d","command":"GetDiagnostics","payload":{}}"#,
        r#"{"version":"realtime-noise.v1","request_id":"e","command":"Shutdown","payload":{}}"#,
    ];
    let expected = [
        IpcCommand::GetStatus,
        IpcCommand::SetMode(DenoiseMode::Bypass),
        IpcCommand::RestartGeneration,
        IpcCommand::GetDiagnostics,
        IpcCommand::Shutdown,
    ];
    for (raw, command) in cases.iter().zip(expected) {
        assert_eq!(IpcRequest::from_json(raw).unwrap().command, command);
    }
}
```

- [ ] **Step 3: Rodar e confirmar a falha**

Run: `cargo test -p realtime-noise-ipc --locked --offline --test set_preset`
Expected: FAIL de compilação, `no variant or associated item named SetPreset found for enum IpcCommand` e `unresolved import realtime_noise_ipc::StudioPreset`.

- [ ] **Step 4: Implementar o tipo e a variante**

Em `crates/ipc/src/protocol.rs`, logo após o `enum DenoiseMode` (antes do `enum IpcStatus`), inserir:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum StudioPreset {
    #[default]
    Off,
    Natural,
    Podcast,
    Broadcast,
}
```

No mesmo arquivo, no `enum IpcCommand`, adicionar a variante após `SetMode(DenoiseMode),`:

```rust
    SetMode(DenoiseMode),
    SetPreset(StudioPreset),
    RestartGeneration,
```

Em `crates/ipc/src/lib.rs`, trocar o bloco de reexport de `protocol` por:

```rust
pub use protocol::{
    DenoiseMode, IpcCommand, IpcErrorDetail, IpcRequest, IpcResponse, IpcStatus, PROTOCOL_VERSION,
    StudioPreset, handle_request,
};
```

- [ ] **Step 5: Manter o workspace compilando (braço provisório no serviço)**

O `match cmd` do `ServiceDaemon` é exaustivo; sem tratar a variante nova o workspace não compila. Em `crates/service/src/lib.rs`, adicionar este braço logo após o braço `IpcCommand::SetMode(ipc_mode) => { ... }` (a Tarefa 4 substitui o arquivo inteiro e implementa o comportamento real):

```rust
                    IpcCommand::SetPreset(_) => IpcResponse::error(
                        "set-preset-resp",
                        IpcStatus::InternalError,
                        "NOT_IMPLEMENTED",
                        "SetPreset is not wired to the service yet",
                    ),
```

- [ ] **Step 6: Rodar os testes do crate `ipc`**

Run: `cargo test -p realtime-noise-ipc --locked --offline`
Expected: PASS (5 testes novos em `set_preset` + os 4 existentes em `protocol`).

- [ ] **Step 7: Documentar em `docs/ipc-v1.md`**

(a) Na descrição de `### GetStatus`, trocar a frase "Retrieves current supervisor lifecycle state, operational mode, and crash telemetry." por:

```markdown
Retrieves current supervisor lifecycle state, operational mode, studio preset, and crash telemetry.
```

(b) No payload de resposta do `GetStatus` (o bloco que contém `"can_restart": true,`), acrescentar a linha `"preset": "Off",` logo após `"mode": "Active",`:

```json
  {
    "state": "Running",
    "is_terminal": false,
    "can_restart": true,
    "mode": "Active",
    "preset": "Off",
    "crash_count_15m": 0,
    "total_crashes": 0
  }
```

(c) Inserir, entre o fim da seção `### SetMode` e `### RestartGeneration`, a nova seção:

````markdown
### `SetPreset`
Changes the studio finish preset: `"Off"`, `"Natural"`, `"Podcast"` or `"Broadcast"` (case-sensitive). `Off` is the default and leaves the audio bit-for-bit untouched. The preset is applied immediately and persisted to `settings.json`.

- **Request:**
  ```json
  {"version":"realtime-noise.v1","request_id":"c2b","command":{"SetPreset":"Podcast"},"payload":{}}
  ```
- **Response Payload:**
  ```json
  {
    "preset": "Podcast",
    "success": true,
    "persisted": true
  }
  ```
  `persisted` is `false` when the preset was applied but `settings.json` could not be written (or the service runs without a settings path); the preset then reverts to the stored value on the next service start.
- **Invalid preset:** an unknown or wrongly-cased name fails request parsing and is answered with `InvalidCommand` / `JSON_PARSE_ERROR`, the same as an invalid `SetMode` value. The active preset is not changed.
````

(d) Acrescentar ao fim do arquivo, depois da seção "5. Security & Isolation Boundary":

```markdown

---

## 6. Compatibility & Evolution

The version string (`realtime-noise.v1`) identifies the wire contract. Within `v1`, only **additive** changes are allowed:

- a new `IpcCommand` variant (a daemon that does not know it answers `InvalidCommand` / `JSON_PARSE_ERROR`, i.e. it fails closed);
- a new field in a response `payload` (clients MUST ignore fields they do not know).

Renaming or removing a command, a status code or a payload field, or changing the meaning of an existing one, requires a new version (`realtime-noise.v2`) and new golden fixtures. `SetPreset` and the `preset` field of `GetStatus` were added to `v1` under this rule. Golden wire fixtures live in `fixtures/wire/ipc-v1-*.json`.
```

- [ ] **Step 8: Gates da tarefa**

Run:
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo test --workspace --locked --offline
```
Expected: tudo passa. Se `fmt --check` falhar, rode `cargo fmt --all` e repita. (Esta tarefa não toca a UI; sem comandos de frontend.)

- [ ] **Step 9: Checkpoint**

Run: `git status --short`
Expected (além dos arquivos da Fase 1): `M crates/ipc/src/protocol.rs`, `M crates/ipc/src/lib.rs`, `M crates/service/src/lib.rs`, `M docs/ipc-v1.md`, `?? crates/ipc/tests/set_preset.rs`, `?? fixtures/wire/ipc-v1-set-preset-request.json`. Não comitar.

---

### Task 3: Módulo `settings` do serviço (leitura tolerante, escrita atômica 0600)

**Files:**
- Modify: `crates/service/Cargo.toml`
- Modify: `Cargo.lock` (atualizado offline, Step 1)
- Modify: `crates/service/src/lib.rs` (apenas `pub mod settings;`)
- Create: `crates/service/src/settings.rs`
- Create: `crates/service/tests/common/mod.rs`
- Create: `crates/service/tests/settings.rs`

**Interfaces:**
- Consumes: `studio_dsp::Preset` (ver Tarefa 1); `realtime_noise_ipc::StudioPreset` (Tarefa 2).
- Produces (módulo `realtime_noise_service::settings`):
  - `pub const SETTINGS_VERSION: u32 = 1;` e `pub const SETTINGS_FILE_NAME: &str = "settings.json";`
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct Settings { pub version: u32, pub preset: Preset }`; `Default` = `{ version: SETTINGS_VERSION, preset: Preset::Off }`.
  - `Settings::parse_or_default(text: &str) -> Settings`, `Settings::load(path: &Path) -> Settings`, `Settings::to_json(self) -> Result<String, serde_json::Error>`, `Settings::save(self, path: &Path) -> io::Result<()>`.
  - `pub fn persist_settings(settings: Settings, path: Option<&Path>) -> bool`.
  - `pub const fn convert_ipc_preset_to_dsp(StudioPreset) -> Preset` e `pub const fn convert_dsp_preset_to_ipc(Preset) -> StudioPreset`.
  - `pub fn settings_path_from(config_base: Option<PathBuf>, temp_dir: &Path) -> PathBuf` e `pub fn default_settings_path() -> PathBuf`.
- Política de leitura: arquivo ausente, ilegível, vazio, JSON inválido, `version` ausente, `0` ou maior que `SETTINGS_VERSION`, ou `preset` desconhecido → `Settings::default()`; nunca falha nem entra em pânico. `version: 1` sem o campo `preset` → `Off`.
- Política de escrita: cria diretórios pais; escreve em `<arquivo>.tmp` criado com `create_new` (modo `0o600` em unix), `sync_all`, depois `rename` sobre o destino; em erro, remove o temporário e devolve o erro sem tocar o arquivo anterior.

- [ ] **Step 1: Adicionar a dependência de path e atualizar o `Cargo.lock` offline**

Em `crates/service/Cargo.toml`, na seção `[dependencies]`, adicionar após a linha `realtime-noise-supervisor`:

```toml
studio-dsp = { path = "../studio-dsp" }
```

Run: `cargo check -p realtime-noise-service --offline`
(sem `--locked`, de propósito: é isto que grava a aresta nova no `Cargo.lock`; path dependency não precisa de rede.)

Run: `git diff Cargo.lock`
Expected: o diff só acrescenta a linha `"studio-dsp",` na lista de dependências do pacote `realtime-noise-service`. Qualquer outra mudança (versões de terceiros) é inesperada: PARE e reporte.

Run: `cargo check -p realtime-noise-service --locked --offline`
Expected: compila (agora com `--locked`).

- [ ] **Step 2: Criar o auxiliar de testes**

Criar `crates/service/tests/common/mod.rs`:

```rust
#![allow(dead_code)]

use realtime_noise_ipc::{IpcCommand, IpcRequest, IpcResponse};
use realtime_noise_service::ServiceDaemon;
use serde_json::json;
use std::io::Cursor;
use std::path::{Path, PathBuf};

/// Diretório temporário removido ao sair do escopo (sem crates de terceiros).
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "clearcore-service-test-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Envia um comando ao daemon por uma sessão em memória e devolve a resposta.
pub fn send(daemon: &mut ServiceDaemon, command: IpcCommand) -> IpcResponse {
    let request = IpcRequest::new(command, json!({}));
    let mut reader = Cursor::new(format!("{}\n", request.to_json().expect("serialize")).into_bytes());
    let mut writer = Cursor::new(Vec::new());
    daemon
        .serve_client(&mut reader, &mut writer)
        .expect("serve client");
    IpcResponse::from_json(
        String::from_utf8(writer.into_inner())
            .expect("utf8")
            .trim(),
    )
    .expect("parse response")
}
```

- [ ] **Step 3: Escrever os testes que falham**

Criar `crates/service/tests/settings.rs`:

```rust
#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::TempDir;
use realtime_noise_ipc::StudioPreset;
use realtime_noise_service::settings::{
    SETTINGS_VERSION, Settings, convert_dsp_preset_to_ipc, convert_ipc_preset_to_dsp,
    persist_settings, settings_path_from,
};
use std::path::{Path, PathBuf};
use studio_dsp::Preset;

const ALL_PRESETS: [Preset; 4] = [
    Preset::Off,
    Preset::Natural,
    Preset::Podcast,
    Preset::Broadcast,
];

fn write_settings(dir: &TempDir, text: &str) -> PathBuf {
    let path = dir.path().join("settings.json");
    std::fs::write(&path, text).unwrap();
    path
}

#[test]
fn defaults_are_version_one_with_preset_off() {
    let settings = Settings::default();
    assert_eq!(settings.version, SETTINGS_VERSION);
    assert_eq!(SETTINGS_VERSION, 1);
    assert_eq!(settings.preset, Preset::Off);
}

#[test]
fn missing_file_yields_defaults() {
    let dir = TempDir::new("settings-missing");
    let settings = Settings::load(&dir.path().join("settings.json"));
    assert_eq!(settings, Settings::default());
}

#[test]
fn corrupted_or_unusable_files_yield_defaults() {
    let dir = TempDir::new("settings-corrupt");
    let cases = [
        "",
        "not json",
        "{\"version\":1,\"preset\":",
        "{\"version\":1,\"preset\":\"Loud\"}",
        "{\"version\":0,\"preset\":\"Podcast\"}",
        "{\"version\":99,\"preset\":\"Podcast\"}",
        "{\"preset\":\"Podcast\"}",
        "[]",
        "null",
    ];
    for text in cases {
        let path = write_settings(&dir, text);
        assert_eq!(
            Settings::load(&path),
            Settings::default(),
            "case: {text:?}"
        );
    }
}

#[test]
fn version_one_without_preset_field_uses_off() {
    let dir = TempDir::new("settings-no-preset");
    let path = write_settings(&dir, "{\"version\":1}");
    assert_eq!(Settings::load(&path).preset, Preset::Off);
}

#[test]
fn valid_file_is_read() {
    let dir = TempDir::new("settings-valid");
    let path = write_settings(&dir, "{\"version\":1,\"preset\":\"Broadcast\"}");
    let settings = Settings::load(&path);
    assert_eq!(settings.preset, Preset::Broadcast);
    assert_eq!(settings.version, SETTINGS_VERSION);
}

#[test]
fn save_then_load_round_trips_every_preset() {
    let dir = TempDir::new("settings-roundtrip");
    let path = dir.path().join("settings.json");
    for preset in ALL_PRESETS {
        let settings = Settings {
            version: SETTINGS_VERSION,
            preset,
        };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path), settings);
    }
}

#[test]
fn save_writes_versioned_json_creates_parents_and_leaves_no_temporary_file() {
    let dir = TempDir::new("settings-layout");
    let path = dir.path().join("nested").join("clearcore").join("settings.json");
    Settings {
        version: SETTINGS_VERSION,
        preset: Preset::Podcast,
    }
    .save(&path)
    .unwrap();

    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(value["version"], 1);
    assert_eq!(value["preset"], "Podcast");
    assert!(!path.with_file_name("settings.json.tmp").exists());
}

#[cfg(unix)]
#[test]
fn save_produces_a_private_file_even_over_a_permissive_one() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new("settings-perms");
    let path = write_settings(&dir, "{}");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

    Settings::default().save(&path).unwrap();

    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn save_replaces_a_stale_temporary_file() {
    let dir = TempDir::new("settings-stale-tmp");
    let path = dir.path().join("settings.json");
    let stale = dir.path().join("settings.json.tmp");
    std::fs::write(&stale, "garbage left by a crashed run").unwrap();

    Settings {
        version: SETTINGS_VERSION,
        preset: Preset::Natural,
    }
    .save(&path)
    .unwrap();

    assert!(!stale.exists());
    assert_eq!(Settings::load(&path).preset, Preset::Natural);
}

#[test]
fn failed_save_keeps_the_previous_file_intact() {
    let dir = TempDir::new("settings-failed-save");
    let path = dir.path().join("settings.json");
    Settings {
        version: SETTINGS_VERSION,
        preset: Preset::Natural,
    }
    .save(&path)
    .unwrap();

    // Um diretório no lugar do temporário faz o `create_new` falhar.
    std::fs::create_dir(dir.path().join("settings.json.tmp")).unwrap();
    let result = Settings {
        version: SETTINGS_VERSION,
        preset: Preset::Broadcast,
    }
    .save(&path);

    assert!(result.is_err());
    assert_eq!(Settings::load(&path).preset, Preset::Natural);
}

#[test]
fn persist_settings_reports_success_failure_and_absence_of_a_path() {
    let dir = TempDir::new("settings-persist");
    let settings = Settings {
        version: SETTINGS_VERSION,
        preset: Preset::Podcast,
    };

    assert!(persist_settings(settings, Some(&dir.path().join("settings.json"))));
    assert!(!persist_settings(settings, None));

    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, "a file, not a directory").unwrap();
    assert!(!persist_settings(settings, Some(&blocker.join("settings.json"))));
}

#[test]
fn preset_conversions_are_inverse_of_each_other() {
    let pairs = [
        (StudioPreset::Off, Preset::Off),
        (StudioPreset::Natural, Preset::Natural),
        (StudioPreset::Podcast, Preset::Podcast),
        (StudioPreset::Broadcast, Preset::Broadcast),
    ];
    for (wire, dsp) in pairs {
        assert_eq!(convert_ipc_preset_to_dsp(wire), dsp);
        assert_eq!(convert_dsp_preset_to_ipc(dsp), wire);
    }
}

#[test]
fn settings_path_uses_the_clearcore_subdirectory_of_the_config_base() {
    let base = PathBuf::from("/home/user/.config");
    let path = settings_path_from(Some(base.clone()), Path::new("/tmp"));
    assert_eq!(path, base.join("clearcore").join("settings.json"));
}

#[test]
fn settings_path_falls_back_to_the_temp_dir_without_a_config_base() {
    let path = settings_path_from(None, Path::new("/tmp"));
    assert_eq!(path, PathBuf::from("/tmp").join("clearcore-settings.json"));
}
```

- [ ] **Step 4: Rodar e confirmar a falha**

Run: `cargo test -p realtime-noise-service --locked --offline --test settings`
Expected: FAIL de compilação, `unresolved import realtime_noise_service::settings` (módulo inexistente).

- [ ] **Step 5: Implementar o módulo**

Em `crates/service/src/lib.rs`, trocar `pub mod install;` por:

```rust
pub mod install;
pub mod settings;
```

Criar `crates/service/src/settings.rs`:

```rust
#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_const_for_fn)]

//! Configuração persistida do serviço (`settings.json`).
//!
//! Leitura tolerante (qualquer problema vira os padrões) e escrita atômica
//! (arquivo temporário + `rename`), com permissão 0600 em unix.

use realtime_noise_ipc::StudioPreset;
use serde::{Deserialize, Serialize};
use std::ffi::{OsStr, OsString};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use studio_dsp::Preset;

pub const SETTINGS_VERSION: u32 = 1;
pub const SETTINGS_FILE_NAME: &str = "settings.json";

const SETTINGS_DIR_NAME: &str = "clearcore";
const FALLBACK_FILE_NAME: &str = "clearcore-settings.json";
const TEMP_SUFFIX: &str = ".tmp";

/// Configuração persistida. Hoje só o preset de acabamento de estúdio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    pub version: u32,
    pub preset: Preset,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            preset: Preset::Off,
        }
    }
}

/// Forma em disco. Usa o tipo de fio `StudioPreset` como única fonte dos nomes.
#[derive(Serialize, Deserialize)]
struct SettingsFile {
    version: u32,
    #[serde(default)]
    preset: StudioPreset,
}

#[must_use]
pub const fn convert_ipc_preset_to_dsp(preset: StudioPreset) -> Preset {
    match preset {
        StudioPreset::Off => Preset::Off,
        StudioPreset::Natural => Preset::Natural,
        StudioPreset::Podcast => Preset::Podcast,
        StudioPreset::Broadcast => Preset::Broadcast,
    }
}

#[must_use]
pub const fn convert_dsp_preset_to_ipc(preset: Preset) -> StudioPreset {
    match preset {
        Preset::Off => StudioPreset::Off,
        Preset::Natural => StudioPreset::Natural,
        Preset::Podcast => StudioPreset::Podcast,
        Preset::Broadcast => StudioPreset::Broadcast,
    }
}

impl Settings {
    /// Interpreta o texto de um `settings.json`; qualquer problema vira os padrões.
    #[must_use]
    pub fn parse_or_default(text: &str) -> Self {
        match serde_json::from_str::<SettingsFile>(text) {
            Ok(file) if (1..=SETTINGS_VERSION).contains(&file.version) => Self {
                version: SETTINGS_VERSION,
                preset: convert_ipc_preset_to_dsp(file.preset),
            },
            _ => Self::default(),
        }
    }

    /// Lê o arquivo; ausente, ilegível ou inválido resulta nos padrões.
    #[must_use]
    pub fn load(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(text) => Self::parse_or_default(&text),
            Err(err) => {
                if err.kind() != io::ErrorKind::NotFound {
                    eprintln!(
                        "Could not read settings file {}: {err}; using defaults",
                        path.display()
                    );
                }
                Self::default()
            }
        }
    }

    pub fn to_json(self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&SettingsFile {
            version: SETTINGS_VERSION,
            preset: convert_dsp_preset_to_ipc(self.preset),
        })
    }

    /// Escrita atômica: temporário no mesmo diretório + `rename`.
    pub fn save(self, path: &Path) -> io::Result<()> {
        let json = self
            .to_json()
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;

        match path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => fs::create_dir_all(parent)?,
            _ => {}
        }

        let temp_path = temp_path_for(path);
        let _ = fs::remove_file(&temp_path);
        let result = write_private_file(&temp_path, json.as_bytes())
            .and_then(|()| fs::rename(&temp_path, path));
        if result.is_err() {
            let _ = fs::remove_file(&temp_path);
        }
        result
    }
}

/// Persiste sem propagar erro: devolve `true` se gravou. `None` desliga a persistência.
#[must_use]
pub fn persist_settings(settings: Settings, path: Option<&Path>) -> bool {
    let Some(path) = path else {
        return false;
    };
    match settings.save(path) {
        Ok(()) => true,
        Err(err) => {
            eprintln!("Could not persist settings to {}: {err}", path.display());
            false
        }
    }
}

fn temp_path_for(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map_or_else(|| OsString::from(SETTINGS_FILE_NAME), OsStr::to_os_string);
    name.push(TEMP_SUFFIX);
    path.with_file_name(name)
}

fn write_private_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Caminho do `settings.json` a partir da base de configuração do usuário.
#[must_use]
pub fn settings_path_from(config_base: Option<PathBuf>, temp_dir: &Path) -> PathBuf {
    config_base.map_or_else(
        || temp_dir.join(FALLBACK_FILE_NAME),
        |base| base.join(SETTINGS_DIR_NAME).join(SETTINGS_FILE_NAME),
    )
}

#[cfg(windows)]
fn config_base() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(PathBuf::from)
}

#[cfg(target_os = "macos")]
fn config_base() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("Library").join("Application Support"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn config_base() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
}

/// `$XDG_CONFIG_HOME/clearcore/settings.json` (ou `~/.config/...`), `~/Library/Application Support/...`
/// no macOS, `%APPDATA%\clearcore\settings.json` no Windows; sem base, um arquivo no diretório
/// temporário (mesma estratégia de reserva do socket em `default_endpoint_path`).
#[must_use]
pub fn default_settings_path() -> PathBuf {
    settings_path_from(config_base(), &std::env::temp_dir())
}
```

- [ ] **Step 6: Rodar os testes do módulo**

Run: `cargo test -p realtime-noise-service --locked --offline --test settings`
Expected: PASS (13 testes; em plataformas não unix, 12).

- [ ] **Step 7: Gates da tarefa**

Run:
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo test --workspace --locked --offline
```
Expected: tudo passa. Se `fmt --check` falhar, rode `cargo fmt --all` e repita. Se o clippy pedantic/nursery apontar algo em `settings.rs` (por exemplo `doc_markdown` nos comentários), corrija o apontamento exato no arquivo; não use `#[allow]` novo. (Sem UI nesta tarefa.)

- [ ] **Step 8: Checkpoint**

Run: `git status --short`
Expected (além da Fase 1 e das tarefas anteriores): `M Cargo.lock`, `M crates/service/Cargo.toml`, `M crates/service/src/lib.rs`, `?? crates/service/src/settings.rs`, `?? crates/service/tests/common/mod.rs`, `?? crates/service/tests/settings.rs`. Não comitar.

---

### Task 4: `ServiceDaemon` dono do `StudioControl`, `ServiceConfig.settings_path`, handlers e golden

**Files:**
- Modify (reescrita completa): `crates/service/src/lib.rs`
- Modify: `crates/service/src/bootstrap.rs`
- Modify: `docs/service-lifecycle.md`
- Modify: `crates/diagnostics/tests/privacy.rs`
- Create: `crates/service/tests/preset_ipc.rs`
- Create: `fixtures/wire/ipc-v1-set-preset-response.json`
- Create: `fixtures/wire/ipc-v1-get-status-response.json`

**Interfaces:**
- Consumes: `Settings`, `persist_settings`, `convert_ipc_preset_to_dsp`, `convert_dsp_preset_to_ipc`, `default_settings_path` (Tarefa 3); `IpcCommand::SetPreset`, `StudioPreset` (Tarefa 2); `studio_dsp::{Preset, StudioControl}` (Tarefa 1).
- Produces:
  - `ServiceConfig { pub endpoint_path: PathBuf, pub settings_path: PathBuf }` (o `Default` usa `default_settings_path()`).
  - `ServiceDaemon::with_settings(settings: Settings, settings_path: Option<PathBuf>) -> ServiceDaemon`; `ServiceDaemon::new()` continua existindo (sem persistência, preset `Off`).
  - `ServiceDaemon::studio_control(&self) -> Arc<StudioControl>` (clone do `Arc` compartilhado) e `ServiceDaemon::settings(&self) -> Settings`.
  - `ServiceBootstrap::new(config)` carrega `Settings::load(&config.settings_path)` e cria o daemon com ele.
  - IPC `SetPreset(p)`: resposta `{"preset": p, "success": true, "persisted": bool}` (`request_id` `"set-preset-resp"`); `GetStatus` ganha `"preset"` (lido de `studio.preset()`).

- [ ] **Step 1: Criar os golden**

`fixtures/wire/ipc-v1-set-preset-response.json` (uma linha + `\n`):

```json
{"version":"realtime-noise.v1","request_id":"set-preset-resp","status":"Ok","payload":{"persisted":true,"preset":"Podcast","success":true},"error":null}
```

`fixtures/wire/ipc-v1-get-status-response.json` (uma linha + `\n`):

```json
{"version":"realtime-noise.v1","request_id":"status-resp","status":"Ok","payload":{"can_restart":true,"crash_count_15m":0,"is_terminal":false,"mode":"Active","preset":"Off","state":"Running","total_crashes":0},"error":null}
```

- [ ] **Step 2: Escrever os testes que falham**

Criar `crates/service/tests/preset_ipc.rs`:

```rust
#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{TempDir, send};
use realtime_noise_ipc::{IpcCommand, IpcResponse, IpcStatus, StudioPreset};
use realtime_noise_service::ServiceDaemon;
use realtime_noise_service::bootstrap::{ServiceBootstrap, ServiceConfig};
use realtime_noise_service::settings::Settings;
use studio_dsp::Preset;

const GET_STATUS_GOLDEN: &str =
    include_str!("../../../fixtures/wire/ipc-v1-get-status-response.json");
const SET_PRESET_GOLDEN: &str =
    include_str!("../../../fixtures/wire/ipc-v1-set-preset-response.json");

fn config_in(dir: &TempDir) -> ServiceConfig {
    ServiceConfig {
        endpoint_path: dir.path().join("realtime-noise.sock"),
        settings_path: dir.path().join("settings.json"),
    }
}

#[test]
fn fresh_daemon_reports_off_and_studio_control_is_off() {
    let mut daemon = ServiceDaemon::new();
    let response = send(&mut daemon, IpcCommand::GetStatus);

    assert_eq!(response.status, IpcStatus::Ok);
    assert_eq!(response.payload["preset"], "Off");
    assert_eq!(daemon.studio_control().preset(), Preset::Off);
    assert_eq!(daemon.settings(), Settings::default());
}

#[test]
fn set_preset_updates_studio_control_status_and_response() {
    let mut daemon = ServiceDaemon::new();

    let response = send(&mut daemon, IpcCommand::SetPreset(StudioPreset::Podcast));

    assert_eq!(response.status, IpcStatus::Ok);
    assert_eq!(response.request_id, "set-preset-resp");
    assert_eq!(response.payload["preset"], "Podcast");
    assert_eq!(response.payload["success"], true);
    assert_eq!(response.payload["persisted"], false);
    assert_eq!(daemon.studio_control().preset(), Preset::Podcast);

    let status = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(status.payload["preset"], "Podcast");
}

#[test]
fn studio_control_handle_is_shared_with_the_daemon() {
    let mut daemon = ServiceDaemon::new();
    let handle = daemon.studio_control();

    send(&mut daemon, IpcCommand::SetPreset(StudioPreset::Broadcast));

    assert_eq!(handle.preset(), Preset::Broadcast);
}

#[test]
fn set_preset_does_not_touch_the_mode() {
    let mut daemon = ServiceDaemon::new();
    send(&mut daemon, IpcCommand::SetPreset(StudioPreset::Natural));
    let status = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(status.payload["mode"], "Active");
}

#[test]
fn set_preset_persists_and_the_next_boot_applies_it() {
    let dir = TempDir::new("preset-persist");
    let mut first = ServiceBootstrap::new(config_in(&dir));
    assert_eq!(first.daemon().studio_control().preset(), Preset::Off);

    let response = send(
        first.daemon_mut(),
        IpcCommand::SetPreset(StudioPreset::Natural),
    );
    assert_eq!(response.payload["persisted"], true);
    assert!(dir.path().join("settings.json").exists());

    let mut second = ServiceBootstrap::new(config_in(&dir));
    assert_eq!(second.daemon().studio_control().preset(), Preset::Natural);
    let status = send(second.daemon_mut(), IpcCommand::GetStatus);
    assert_eq!(status.payload["preset"], "Natural");
}

#[test]
fn boot_with_corrupted_settings_falls_back_to_off() {
    let dir = TempDir::new("preset-corrupt");
    std::fs::write(dir.path().join("settings.json"), "{ definitely not json").unwrap();

    let bootstrap = ServiceBootstrap::new(config_in(&dir));

    assert_eq!(bootstrap.daemon().studio_control().preset(), Preset::Off);
}

#[test]
fn failed_persistence_still_applies_the_preset_and_says_so() {
    let dir = TempDir::new("preset-unwritable");
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, "a file, not a directory").unwrap();
    let config = ServiceConfig {
        endpoint_path: dir.path().join("realtime-noise.sock"),
        settings_path: blocker.join("settings.json"),
    };
    let mut bootstrap = ServiceBootstrap::new(config);

    let response = send(
        bootstrap.daemon_mut(),
        IpcCommand::SetPreset(StudioPreset::Broadcast),
    );

    assert_eq!(response.status, IpcStatus::Ok);
    assert_eq!(response.payload["persisted"], false);
    assert_eq!(response.payload["success"], true);
    assert_eq!(
        bootstrap.daemon().studio_control().preset(),
        Preset::Broadcast
    );
}

#[test]
fn default_service_config_points_to_a_settings_json() {
    let config = ServiceConfig::default();
    let name = config.settings_path.file_name().unwrap().to_string_lossy();
    assert!(name.ends_with("settings.json"), "unexpected name: {name}");
}

#[test]
fn fresh_status_response_matches_the_golden_wire_fixture() {
    let mut daemon = ServiceDaemon::new();
    let actual = send(&mut daemon, IpcCommand::GetStatus);
    let golden = IpcResponse::from_json(GET_STATUS_GOLDEN.trim()).unwrap();
    assert_eq!(actual, golden);
}

#[test]
fn set_preset_response_matches_the_golden_wire_fixture() {
    let dir = TempDir::new("preset-golden");
    let mut bootstrap = ServiceBootstrap::new(config_in(&dir));
    let actual = send(
        bootstrap.daemon_mut(),
        IpcCommand::SetPreset(StudioPreset::Podcast),
    );
    let golden = IpcResponse::from_json(SET_PRESET_GOLDEN.trim()).unwrap();
    assert_eq!(actual, golden);
}
```

- [ ] **Step 3: Rodar e confirmar a falha**

Run: `cargo test -p realtime-noise-service --locked --offline --test preset_ipc`
Expected: FAIL de compilação, `no field settings_path on type ServiceConfig` / `no method named studio_control found for struct ServiceDaemon`.

- [ ] **Step 4: Reescrever `crates/service/src/lib.rs`**

Substituir o conteúdo inteiro do arquivo por (preserva todo o comportamento existente; acrescenta estado de estúdio, `SetPreset` e `preset` no `GetStatus`; os braços usam acesso direto a campos de `self` para que o closure capture campos disjuntos, como o código atual):

```rust
#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_const_for_fn)]

pub mod bootstrap;
pub mod install;
pub mod settings;

use realtime_noise_ipc::{IpcCommand, IpcResponse, IpcServer, IpcStatus};
use realtime_noise_supervisor::{
    EngineSupervisor, convert_engine_mode_to_ipc, convert_ipc_mode_to_engine,
};
use serde_json::json;
use settings::{
    Settings, convert_dsp_preset_to_ipc, convert_ipc_preset_to_dsp, persist_settings,
};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;
use studio_dsp::StudioControl;

pub struct ServiceDaemon {
    supervisor: EngineSupervisor,
    server: IpcServer,
    shutdown: bool,
    served_client_count: usize,
    studio: Arc<StudioControl>,
    settings: Settings,
    settings_path: Option<PathBuf>,
}

impl Default for ServiceDaemon {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceDaemon {
    /// Daemon sem persistência e com os padrões (preset `Off`).
    #[must_use]
    pub fn new() -> Self {
        Self::with_settings(Settings::default(), None)
    }

    /// Daemon com as configurações carregadas; o preset inicial é aplicado ao `StudioControl`.
    /// `settings_path = None` desliga a persistência.
    #[must_use]
    pub fn with_settings(settings: Settings, settings_path: Option<PathBuf>) -> Self {
        Self {
            supervisor: EngineSupervisor::default(),
            server: IpcServer::new(),
            shutdown: false,
            served_client_count: 0,
            studio: Arc::new(StudioControl::new(settings.preset)),
            settings,
            settings_path,
        }
    }

    #[must_use]
    pub const fn supervisor(&self) -> &EngineSupervisor {
        &self.supervisor
    }

    pub fn supervisor_mut(&mut self) -> &mut EngineSupervisor {
        &mut self.supervisor
    }

    /// Clone do `Arc<StudioControl>` compartilhado: quem monta o `StudioBackend`
    /// (engine ou `filter-capi`) deve receber este handle.
    #[must_use]
    pub fn studio_control(&self) -> Arc<StudioControl> {
        Arc::clone(&self.studio)
    }

    #[must_use]
    pub const fn settings(&self) -> Settings {
        self.settings
    }

    #[must_use]
    pub const fn is_shutdown(&self) -> bool {
        self.shutdown
    }

    #[must_use]
    pub const fn served_client_count(&self) -> usize {
        self.served_client_count
    }

    /// Serves a control client connection stream until the client disconnects (EOF).
    /// Safe client disconnect: Daemon outlives UI disconnects and accepts subsequent client connections.
    pub fn serve_client<R: BufRead, W: Write>(
        &mut self,
        mut reader: R,
        mut writer: W,
    ) -> io::Result<()> {
        self.served_client_count += 1;

        let mut line = String::new();
        while reader.read_line(&mut line)? > 0 {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                let resp_str = self.server.handle_line(trimmed, |cmd, _payload| match cmd {
                    IpcCommand::GetStatus => {
                        let status = self.supervisor.status();
                        let mode_ipc = convert_engine_mode_to_ipc(status.active_mode);
                        let preset_ipc = convert_dsp_preset_to_ipc(self.studio.preset());
                        IpcResponse::success(
                            "status-resp",
                            json!({
                                "state": format!("{:?}", status.state),
                                "is_terminal": self.supervisor.is_terminal(),
                                "can_restart": self.supervisor.can_restart(),
                                "mode": mode_ipc,
                                "preset": preset_ipc,
                                "crash_count_15m": status.crash_count_15m,
                                "total_crashes": status.total_crashes,
                            }),
                        )
                    }
                    IpcCommand::SetMode(ipc_mode) => {
                        let engine_mode = convert_ipc_mode_to_engine(*ipc_mode);
                        self.supervisor.set_mode(engine_mode);
                        IpcResponse::success(
                            "set-mode-resp",
                            json!({
                                "mode": ipc_mode,
                                "success": true,
                            }),
                        )
                    }
                    IpcCommand::SetPreset(ipc_preset) => {
                        let preset = convert_ipc_preset_to_dsp(*ipc_preset);
                        self.studio.set_preset(preset);
                        self.settings.preset = preset;
                        let persisted =
                            persist_settings(self.settings, self.settings_path.as_deref());
                        IpcResponse::success(
                            "set-preset-resp",
                            json!({
                                "preset": ipc_preset,
                                "success": true,
                                "persisted": persisted,
                            }),
                        )
                    }
                    IpcCommand::RestartGeneration => match self.supervisor.reset() {
                        Ok(()) => IpcResponse::success(
                            "restart-resp",
                            json!({"restarted": true, "state": "Running"}),
                        ),
                        Err(err) => IpcResponse::error(
                            "restart-resp",
                            IpcStatus::InternalError,
                            "RESTART_FAILED",
                            err.to_string(),
                        ),
                    },
                    IpcCommand::GetDiagnostics => {
                        let diag = self.supervisor.diagnostics();
                        IpcResponse::success(
                            "diagnostics-resp",
                            json!({
                                "diagnostics": diag,
                                "total_crashes": self.supervisor.status().total_crashes,
                            }),
                        )
                    }
                    IpcCommand::Shutdown => {
                        self.shutdown = true;
                        IpcResponse::success("shutdown-resp", json!({"shutdown": true}))
                    }
                });
                writer.write_all(resp_str.as_bytes())?;
                writer.write_all(b"\n")?;
                writer.flush()?;
            }
            line.clear();
        }
        Ok(())
    }
}
```

- [ ] **Step 5: Ajustar `crates/service/src/bootstrap.rs`**

Substituir o bloco do topo, de `use crate::ServiceDaemon;` até o fim de `ServiceBootstrap::new` (as linhas 1 a 41 do arquivo atual; `config()`, `daemon()`, `daemon_mut()` e `run()` ficam como estão), por:

```rust
#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_const_for_fn)]

use crate::ServiceDaemon;
use crate::settings::{Settings, default_settings_path};
use realtime_noise_ipc::default_endpoint_path;
use std::io;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ServiceConfig {
    pub endpoint_path: PathBuf,
    pub settings_path: PathBuf,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            endpoint_path: PathBuf::from(default_endpoint_path()),
            settings_path: default_settings_path(),
        }
    }
}

pub struct ServiceBootstrap {
    config: ServiceConfig,
    daemon: ServiceDaemon,
}

impl Default for ServiceBootstrap {
    fn default() -> Self {
        Self::new(ServiceConfig::default())
    }
}

impl ServiceBootstrap {
    /// Carrega o `settings.json` (tolerante: ausente/corrompido vira os padrões) e
    /// aplica o preset ao `StudioControl` do daemon.
    #[must_use]
    pub fn new(config: ServiceConfig) -> Self {
        let settings = Settings::load(&config.settings_path);
        let daemon = ServiceDaemon::with_settings(settings, Some(config.settings_path.clone()));
        Self { config, daemon }
    }
```

(O `use std::io;` e o `use std::path::PathBuf;` já existiam; o resultado não deve duplicá-los. Mantenha o restante do arquivo, a partir de `#[must_use] pub fn config(&self) -> &ServiceConfig`, intacto.)

- [ ] **Step 6: Rodar os testes do serviço**

Run: `cargo test -p realtime-noise-service --locked --offline`
Expected: PASS (testes novos de `preset_ipc` e `settings`, mais os 5 de `service_lifecycle`, que continuam usando `ServiceDaemon::new()`).

- [ ] **Step 7: Teste-guarda de privacidade do export de diagnósticos**

Acrescentar ao fim de `crates/diagnostics/tests/privacy.rs`:

```rust
#[test]
fn diagnostic_export_does_not_include_studio_preset_or_settings() {
    let exporter = DiagnosticExporter::new("test-installation-salt-12345");
    let input = RawDiagnosticInput {
        device_id: "hw-mic-usb-046d-0825-device-serial-999".to_string(),
        generation: 1,
        causes: vec!["inference timeout".to_string()],
        last_attempt: Some(1),
        pcm_samples: vec![],
        embeddings: vec![],
        meeting_title: None,
        transcript_snippet: None,
        latency_p50_us: 1200,
        latency_p95_us: 2800,
        latency_p99_us: 4500,
        budget_measured_us: 1500,
        budget_configured_us: 3000,
        budget_derived_us: 800,
        budget_unobservable_us: 200,
    };

    let json_archive = exporter
        .export_json(&input)
        .expect("serialization should succeed");

    for forbidden in ["preset", "Preset", "settings", "voice_profile"] {
        assert!(
            !json_archive.contains(forbidden),
            "diagnostics export must not carry `{forbidden}`"
        );
    }
}
```

Run: `cargo test -p realtime-noise-diagnostics --locked --offline`
Expected: PASS de imediato (é um teste-guarda de regressão: documenta e trava a decisão 5; não há implementação a escrever).

- [ ] **Step 8: Documentar em `docs/service-lifecycle.md`**

Acrescentar ao fim do arquivo:

````markdown

---

## 6. Persisted Settings & Studio Preset

`ServiceBootstrap::new` loads `settings.json` (`ServiceConfig::settings_path`) before the daemon accepts clients:

- Default location: `$XDG_CONFIG_HOME/clearcore/settings.json` (or `~/.config/clearcore/settings.json`) on Linux, `~/Library/Application Support/clearcore/settings.json` on macOS, `%APPDATA%\clearcore\settings.json` on Windows; without a config base it falls back to `clearcore-settings.json` in the temporary directory.
- Format: `{"version": 1, "preset": "Off"}`. A missing, unreadable or corrupted file, an unknown `version` (0 or newer than the daemon understands) or an unknown preset name falls back to the defaults (preset `Off`); the daemon never fails to start because of it.
- Writes are atomic (`settings.json.tmp` created with `create_new`, `sync_all`, then `rename`) and the file is `0600` on Unix.

`ServiceDaemon` owns the `Arc<StudioControl>` (built from the loaded preset) and hands a clone to whoever assembles the `StudioBackend` through `ServiceDaemon::studio_control()`. `SetPreset` applies the preset to that `StudioControl` first and then persists it; if persistence fails the preset stays applied and the response carries `"persisted": false`. `GetStatus` reports the preset read from the `StudioControl`.

The studio preset is **not** part of the diagnostics export (`DiagnosticPayload` is a closed field list; see `crates/diagnostics/tests/privacy.rs`).
````

- [ ] **Step 9: Gates da tarefa**

Run:
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo test --workspace --locked --offline
```
Expected: tudo passa. Se `fmt --check` falhar, rode `cargo fmt --all` e repita (os imports em `lib.rs` e o corpo do `SetPreset` são os candidatos a reformatação). Se o clippy acusar `significant_drop_tightening` ou similar no closure, corrija o ponto apontado sem `#[allow]` novo. (Sem UI nesta tarefa.)

- [ ] **Step 10: Checkpoint**

Run: `git status --short`
Expected (além da Fase 1 e das tarefas anteriores): `M crates/service/src/lib.rs`, `M crates/service/src/bootstrap.rs`, `M docs/service-lifecycle.md`, `M crates/diagnostics/tests/privacy.rs`, `?? crates/service/tests/preset_ipc.rs`, `?? fixtures/wire/ipc-v1-set-preset-response.json`, `?? fixtures/wire/ipc-v1-get-status-response.json`. Não comitar.

---

### Task 5: Cliente Rust do app — comando `set_preset` e opção de CLI `--preset`

**Files:**
- Modify: `crates/app-tauri/src-tauri/src/commands.rs`
- Modify: `crates/app-tauri/src-tauri/src/main.rs`

**Interfaces:**
- Consumes: `realtime_noise_ipc::{IpcCommand::SetPreset, StudioPreset}` (Tarefa 2).
- Produces:
  - `commands::set_preset(preset: StudioPreset) -> Result<Value, CommandError>` (devolve o payload `{preset, success, persisted}`).
  - `commands::parse_preset_arg(value: &str) -> Option<StudioPreset>`: aceita `off|natural|podcast|broadcast` sem distinguir caixa.
  - CLI: `realtime-noise-app-tauri --preset <off|natural|podcast|broadcast>`; `--status` já imprime o `preset` porque repassa o payload do `GetStatus`.

- [ ] **Step 1: Escrever os testes que falham**

Acrescentar ao **fim** de `crates/app-tauri/src-tauri/src/commands.rs` (o módulo de teste deve ser o último item do arquivo):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_preset_arg_accepts_every_preset_case_insensitively() {
        assert_eq!(parse_preset_arg("off"), Some(StudioPreset::Off));
        assert_eq!(parse_preset_arg("Natural"), Some(StudioPreset::Natural));
        assert_eq!(parse_preset_arg("PODCAST"), Some(StudioPreset::Podcast));
        assert_eq!(parse_preset_arg("broadcast"), Some(StudioPreset::Broadcast));
    }

    #[test]
    fn parse_preset_arg_rejects_unknown_names() {
        assert_eq!(parse_preset_arg("loud"), None);
        assert_eq!(parse_preset_arg(""), None);
        assert_eq!(parse_preset_arg("off "), None);
    }
}
```

- [ ] **Step 2: Rodar e confirmar a falha**

Run: `cargo test -p realtime-noise-app-tauri --locked --offline`
Expected: FAIL de compilação, `cannot find function parse_preset_arg in this scope` e `cannot find type StudioPreset`.

- [ ] **Step 3: Implementar**

Em `commands.rs`, trocar o `use realtime_noise_ipc::{...}` do topo por:

```rust
use realtime_noise_ipc::{
    DenoiseMode, IpcClient, IpcCommand, IpcRequest, IpcResponse, IpcStatus, StudioPreset,
    default_endpoint_path,
};
```

Logo após a função `set_mode`, inserir:

```rust
pub fn set_preset(preset: StudioPreset) -> Result<Value, CommandError> {
    let resp = execute_ipc_command(IpcCommand::SetPreset(preset), json!({}))?;
    Ok(resp.payload)
}

/// Converte o argumento de CLI (`off|natural|podcast|broadcast`, sem distinguir caixa).
#[must_use]
pub fn parse_preset_arg(value: &str) -> Option<StudioPreset> {
    match value.to_lowercase().as_str() {
        "off" => Some(StudioPreset::Off),
        "natural" => Some(StudioPreset::Natural),
        "podcast" => Some(StudioPreset::Podcast),
        "broadcast" => Some(StudioPreset::Broadcast),
        _ => None,
    }
}
```

Em `main.rs`, na `print_usage`, após a linha de `--mode`, acrescentar:

```rust
    println!("  --preset <off|natural|podcast|broadcast> Set studio finish preset");
```

e, no `match args[1].as_str()`, logo após o braço `"--mode" => { ... }` e antes de `"--restart"`, inserir:

```rust
            "--preset" => {
                if args.len() < 3 {
                    eprintln!(
                        "Missing preset argument. Expected: off, natural, podcast, or broadcast"
                    );
                    return ExitCode::from(1);
                }
                let Some(preset) = commands::parse_preset_arg(&args[2]) else {
                    eprintln!(
                        "Unknown preset: {}. Expected off, natural, podcast, or broadcast",
                        args[2]
                    );
                    return ExitCode::from(1);
                };
                match commands::set_preset(preset) {
                    Ok(resp) => {
                        println!("Preset updated successfully: {resp}");
                        return ExitCode::SUCCESS;
                    }
                    Err(e) => {
                        eprintln!("Error setting preset: {e}");
                        return ExitCode::from(1);
                    }
                }
            }
```

- [ ] **Step 4: Rodar os testes do crate**

Run: `cargo test -p realtime-noise-app-tauri --locked --offline`
Expected: PASS (2 testes novos).

- [ ] **Step 5: Gates da tarefa**

Run:
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo test --workspace --locked --offline
```
Expected: tudo passa (se `fmt --check` falhar, `cargo fmt --all` e repetir). Esta tarefa não altera o frontend, mas confirme que ele segue verde: `(cd crates/app-tauri && npx tsc --noEmit && npm test)`; Expected: sem erros e 6 testes passando.

- [ ] **Step 6: Checkpoint**

Run: `git status --short`
Expected (além do que veio antes): `M crates/app-tauri/src-tauri/src/commands.rs`, `M crates/app-tauri/src-tauri/src/main.rs`. Não comitar.

---

### Task 6: Frontend — tipos, lógica pura, ponte (bridge/preload/Electron) e strings i18n

**Files:**
- Modify: `crates/app-tauri/src/types.ts`
- Create: `crates/app-tauri/src/preset.ts`
- Modify: `crates/app-tauri/src/bridge.ts`
- Modify: `crates/app-tauri/electron/preload.cjs`
- Modify: `crates/app-tauri/electron/main.cjs`
- Modify: `crates/app-tauri/src/i18n/types.ts`
- Modify: `crates/app-tauri/src/i18n/locales/en-US.ts`
- Modify: `crates/app-tauri/src/i18n/locales/pt-BR.ts`
- Create: `crates/app-tauri/src/__tests__/preset.test.ts`
- Create: `crates/app-tauri/src/__tests__/bridge.test.ts`

**Interfaces:**
- Consumes: contrato IPC da Tarefa 4: `set_preset` devolve `{preset, success, persisted}`; `get_status` devolve `preset` (campo opcional para daemons antigos).
- Produces:
  - `export type Preset = 'Off' | 'Natural' | 'Podcast' | 'Broadcast'` e `EngineStatus.preset?: Preset` (`src/types.ts`).
  - `src/preset.ts`: `PRESETS: readonly Preset[]`, `DEFAULT_PRESET: Preset`, `isPreset(value: unknown): value is Preset`, `presetFromStatus(status: { preset?: unknown } | null | undefined): Preset`, `presetLabelKey(preset: Preset): string` (`presets.<nome em minúsculas>`), `presetHintKey(preset: Preset): string` (`presets.<nome>Hint`).
  - `ClearcoreApi.setPreset: (preset: string) => Promise<unknown>`; `invokeBridge('set_preset', { preset })` roteia para ela (padrão `'Off'`); no Electron, `ipcMain.handle('set_preset')` envia `{ SetPreset: preset }` ao daemon.
  - Bloco i18n `presets` com as chaves: `title`, `description`, `ariaLabel`, `off`, `natural`, `podcast`, `broadcast`, `offHint`, `naturalHint`, `podcastHint`, `broadcastHint`, `inactiveHint`, `switchFailed` (com `{error}`), `notPersisted`.

- [ ] **Step 1: Escrever os testes que falham**

Criar `crates/app-tauri/src/__tests__/preset.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import {
  DEFAULT_PRESET,
  PRESETS,
  isPreset,
  presetFromStatus,
  presetHintKey,
  presetLabelKey,
} from '../preset';
import { ptBR } from '../i18n/locales/pt-BR';
import { enUS } from '../i18n/locales/en-US';

function lookup(obj: unknown, path: string): unknown {
  return path
    .split('.')
    .reduce<unknown>((acc, key) => (acc as Record<string, unknown> | undefined)?.[key], obj);
}

describe('preset logic', () => {
  it('lists the four presets in selector order with Off as default', () => {
    expect(PRESETS).toEqual(['Off', 'Natural', 'Podcast', 'Broadcast']);
    expect(DEFAULT_PRESET).toBe('Off');
  });

  it('isPreset accepts only the exact wire names', () => {
    for (const preset of PRESETS) expect(isPreset(preset)).toBe(true);
    for (const bad of ['off', 'Loud', '', null, undefined, 1, {}]) {
      expect(isPreset(bad)).toBe(false);
    }
  });

  it('presetFromStatus reads the daemon preset and falls back to Off', () => {
    expect(presetFromStatus({ preset: 'Podcast' })).toBe('Podcast');
    expect(presetFromStatus({})).toBe('Off');
    expect(presetFromStatus({ preset: 'Loud' })).toBe('Off');
    expect(presetFromStatus(null)).toBe('Off');
    expect(presetFromStatus(undefined)).toBe('Off');
  });

  it('builds the i18n keys from the preset name', () => {
    expect(presetLabelKey('Broadcast')).toBe('presets.broadcast');
    expect(presetHintKey('Broadcast')).toBe('presets.broadcastHint');
  });
});

describe('preset strings', () => {
  it('has a label and a hint for every preset in both locales', () => {
    for (const preset of PRESETS) {
      for (const locale of [ptBR, enUS]) {
        for (const key of [presetLabelKey(preset), presetHintKey(preset)]) {
          const value = lookup(locale, key);
          expect(typeof value, key).toBe('string');
          expect((value as string).length, key).toBeGreaterThan(0);
        }
      }
    }
  });

  it('keeps proper accents in pt-BR', () => {
    expect(ptBR.presets.title).toBe('Acabamento de Estúdio');
    expect(ptBR.presets.inactiveHint).toBe('O acabamento de estúdio só atua no modo Ativo.');
    expect(ptBR.presets.notPersisted).toContain('não');
    expect(ptBR.presets.switchFailed).toContain('{error}');
  });

  it('provides the English copy', () => {
    expect(enUS.presets.title).toBe('Studio Finish');
    expect(enUS.presets.switchFailed).toContain('{error}');
  });
});
```

Criar `crates/app-tauri/src/__tests__/bridge.test.ts`:

```ts
import { describe, it, expect, vi, afterEach } from 'vitest';
import { invokeBridge } from '../bridge';

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('invokeBridge set_preset', () => {
  it('routes set_preset to clearcoreApi.setPreset with the chosen preset', async () => {
    const reply = { preset: 'Podcast', success: true, persisted: true };
    const setPreset = vi.fn().mockResolvedValue(reply);
    vi.stubGlobal('window', { clearcoreApi: { setPreset } });

    const result = await invokeBridge('set_preset', { preset: 'Podcast' });

    expect(setPreset).toHaveBeenCalledTimes(1);
    expect(setPreset).toHaveBeenCalledWith('Podcast');
    expect(result).toEqual(reply);
  });

  it('defaults to Off when no preset argument is given', async () => {
    const setPreset = vi.fn().mockResolvedValue({});
    vi.stubGlobal('window', { clearcoreApi: { setPreset } });

    await invokeBridge('set_preset');

    expect(setPreset).toHaveBeenCalledWith('Off');
  });

  it('does not disturb the existing set_mode route', async () => {
    const setMode = vi.fn().mockResolvedValue({});
    vi.stubGlobal('window', { clearcoreApi: { setMode } });

    await invokeBridge('set_mode', { mode: 'Bypass' });

    expect(setMode).toHaveBeenCalledWith('Bypass');
  });
});
```

- [ ] **Step 2: Rodar e confirmar a falha**

Run: `(cd crates/app-tauri && npx vitest run)`
Expected: FAIL: `Failed to resolve import "../preset"`, `Cannot read properties of undefined (reading 'title')` (bloco `presets` inexistente) e o teste de `set_preset` falhando (a rota não existe, `setPreset` não é chamada).

- [ ] **Step 3: Tipos e lógica pura**

Em `crates/app-tauri/src/types.ts`, após a linha `export type DenoiseMode = 'Active' | 'Bypass' | 'Mute';`, inserir:

```ts
export type Preset = 'Off' | 'Natural' | 'Podcast' | 'Broadcast';
```

e, na `interface EngineStatus`, após `mode: DenoiseMode;`, inserir:

```ts
  preset?: Preset;
```

Criar `crates/app-tauri/src/preset.ts`:

```ts
import type { Preset } from './types';

/** Ordem do seletor; os nomes são exatamente os do protocolo IPC (`StudioPreset`). */
export const PRESETS: readonly Preset[] = ['Off', 'Natural', 'Podcast', 'Broadcast'];

export const DEFAULT_PRESET: Preset = 'Off';

export function isPreset(value: unknown): value is Preset {
  return typeof value === 'string' && (PRESETS as readonly string[]).includes(value);
}

/** Daemons anteriores a esta versão não enviam `preset`: cai em `Off`. */
export function presetFromStatus(status: { preset?: unknown } | null | undefined): Preset {
  return status && isPreset(status.preset) ? status.preset : DEFAULT_PRESET;
}

export function presetLabelKey(preset: Preset): string {
  return `presets.${preset.toLowerCase()}`;
}

export function presetHintKey(preset: Preset): string {
  return `presets.${preset.toLowerCase()}Hint`;
}
```

- [ ] **Step 4: Ponte (bridge, preload, processo principal do Electron)**

Em `crates/app-tauri/src/bridge.ts`, na `interface ClearcoreApi`, após a linha `setMode: (mode: string) => Promise<unknown>;`:

```ts
  setPreset: (preset: string) => Promise<unknown>;
```

e em `invokeBridge`, após a linha do `set_mode`:

```ts
    if (cmd === 'set_preset') return (await api.setPreset(String(args?.preset ?? 'Off'))) as unknown as T;
```

Em `crates/app-tauri/electron/preload.cjs`, após a linha `setMode: (mode) => ipcRenderer.invoke('set_mode', { mode }),`:

```js
  setPreset: (preset) => ipcRenderer.invoke('set_preset', { preset }),
```

Em `crates/app-tauri/electron/main.cjs`, imediatamente antes de `ipcMain.handle('restart_generation', async () => {`, inserir:

```js
ipcMain.handle('set_preset', async (_event, args) => {
  const preset = args && args.preset ? args.preset : 'Off';
  return await sendIpcRequest({ SetPreset: preset });
});

```

- [ ] **Step 5: Strings i18n**

Em `crates/app-tauri/src/i18n/types.ts`, na `interface Translations`, logo após o bloco `modes: { ... };` (que termina com `switchFailed: string;` e `};`), inserir:

```ts
  presets: {
    title: string;
    description: string;
    ariaLabel: string;
    off: string;
    natural: string;
    podcast: string;
    broadcast: string;
    offHint: string;
    naturalHint: string;
    podcastHint: string;
    broadcastHint: string;
    inactiveHint: string;
    switchFailed: string;
    notPersisted: string;
  };
```

Em `crates/app-tauri/src/i18n/locales/en-US.ts`, logo após o bloco `modes: { ... },`, inserir:

```ts
  presets: {
    title: 'Studio Finish',
    description: 'Polishes the cleaned voice with high-pass, EQ, de-esser, compressor, loudness control and limiter.',
    ariaLabel: 'Studio finish preset',
    off: 'Off',
    natural: 'Natural',
    podcast: 'Podcast',
    broadcast: 'Broadcast',
    offHint: 'Off: the audio leaves exactly as the denoiser delivers it.',
    naturalHint: 'Light polish that keeps the voice natural.',
    podcastHint: 'Present, level voice for recording and meetings.',
    broadcastHint: 'Stronger polish with steady loudness and a peak ceiling.',
    inactiveHint: 'The studio finish only runs in Active mode.',
    switchFailed: 'Failed to switch preset: {error}',
    notPersisted: 'Preset applied, but it could not be saved and will revert when the service restarts.',
  },
```

Em `crates/app-tauri/src/i18n/locales/pt-BR.ts`, logo após o bloco `modes: { ... },`, inserir:

```ts
  presets: {
    title: 'Acabamento de Estúdio',
    description: 'Refina a voz já limpa com filtro passa-altas, equalização, de-esser, compressor, controle de loudness e limitador.',
    ariaLabel: 'Preset de acabamento de estúdio',
    off: 'Desligado',
    natural: 'Natural',
    podcast: 'Podcast',
    broadcast: 'Broadcast',
    offHint: 'Desligado: o áudio sai exatamente como o denoiser entrega.',
    naturalHint: 'Acabamento leve, que mantém a voz natural.',
    podcastHint: 'Voz presente e nivelada, para gravação e reuniões.',
    broadcastHint: 'Acabamento mais forte, com loudness constante e teto de pico.',
    inactiveHint: 'O acabamento de estúdio só atua no modo Ativo.',
    switchFailed: 'Falha ao trocar de preset: {error}',
    notPersisted: 'Preset aplicado, mas não foi possível salvá-lo; ele voltará ao valor anterior quando o serviço reiniciar.',
  },
```

- [ ] **Step 6: Rodar os testes e a checagem de tipos**

Run: `(cd crates/app-tauri && npx vitest run)`
Expected: PASS (testes novos de `preset.test.ts` e `bridge.test.ts` + os 6 de `i18n.test.ts`, incluindo o de paridade de chaves entre pt-BR e en-US).

Run: `(cd crates/app-tauri && npx tsc --noEmit && node --check electron/main.cjs && node --check electron/preload.cjs)`
Expected: sem saída (sucesso).

- [ ] **Step 7: Gates da tarefa**

Run:
```bash
(cd crates/app-tauri && npx tsc --noEmit && npm test && node --check electron/main.cjs && node --check electron/preload.cjs)
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo test --workspace --locked --offline
```
Expected: tudo passa. Não existe script de lint no `package.json`; `tsc --noEmit` (o primeiro passo de `npm run build`) é a checagem estática disponível. Não rodar `npm run build` aqui: ele regrava `crates/app-tauri/dist` (ignorado pelo git).

- [ ] **Step 8: Checkpoint**

Run: `git status --short`
Expected (além do que veio antes): `M crates/app-tauri/src/types.ts`, `M crates/app-tauri/src/bridge.ts`, `M crates/app-tauri/electron/preload.cjs`, `M crates/app-tauri/electron/main.cjs`, `M crates/app-tauri/src/i18n/types.ts`, `M crates/app-tauri/src/i18n/locales/en-US.ts`, `M crates/app-tauri/src/i18n/locales/pt-BR.ts`, `?? crates/app-tauri/src/preset.ts`, `?? crates/app-tauri/src/__tests__/preset.test.ts`, `?? crates/app-tauri/src/__tests__/bridge.test.ts`. Não comitar.

---

### Task 7: Frontend — card de preset e integração no `App.tsx`

**Files:**
- Create: `crates/app-tauri/src/PresetCard.tsx`
- Modify: `crates/app-tauri/src/App.tsx`
- Modify: `crates/app-tauri/src/styles.css`

**Interfaces:**
- Consumes: `Preset`, `DenoiseMode` (`src/types.ts`); `PRESETS`, `presetFromStatus`, `presetLabelKey`, `presetHintKey` (`src/preset.ts`); `invokeBridge('set_preset', { preset })` e `invokeBridge('get_status')` (Tarefa 6); strings `presets.*`; `useI18n` de `./i18n`.
- Produces: `PresetCard: React.FC<{ preset: Preset; mode: DenoiseMode; onChange: (preset: Preset) => void }>`: grupo de rádio com os quatro presets, dica do preset escolhido e aviso quando o modo não é `Active`.

Observação de teste: o padrão do frontend é vitest em ambiente node, sem jsdom nem testing-library, e não se adicionam pacotes. Por isso o comportamento testável (nomes, fallback, rota da ponte, chaves i18n) já foi coberto na Tarefa 6; este componente é só apresentação e é verificado por `tsc --noEmit` (tipos e chaves) e pela verificação manual do Step 5.

- [ ] **Step 1: Criar o componente**

Criar `crates/app-tauri/src/PresetCard.tsx`:

```tsx
import React from 'react';
import { useI18n } from './i18n';
import { PRESETS, presetHintKey, presetLabelKey } from './preset';
import type { DenoiseMode, Preset } from './types';

interface PresetCardProps {
  preset: Preset;
  mode: DenoiseMode;
  onChange: (preset: Preset) => void;
}

export const PresetCard: React.FC<PresetCardProps> = ({ preset, mode, onChange }) => {
  const { t } = useI18n();

  return (
    <div className="card preset-card">
      <h2 className="card-title">{t('presets.title')}</h2>
      <p className="preset-description">{t('presets.description')}</p>
      <div className="mode-group" role="radiogroup" aria-label={t('presets.ariaLabel')}>
        {PRESETS.map((item) => (
          <button
            key={item}
            type="button"
            role="radio"
            aria-checked={preset === item}
            className={`mode-btn ${preset === item ? 'active-mode' : ''}`}
            onClick={() => onChange(item)}
          >
            {t(presetLabelKey(item))}
          </button>
        ))}
      </div>
      <p className="preset-hint">{t(presetHintKey(preset))}</p>
      {mode !== 'Active' && (
        <p className="preset-hint preset-inactive">{t('presets.inactiveHint')}</p>
      )}
    </div>
  );
};
```

- [ ] **Step 2: Estilos**

Acrescentar ao fim de `crates/app-tauri/src/styles.css`:

```css
.preset-description {
  color: var(--text-muted);
  font-size: 0.9rem;
  margin-bottom: 12px;
}

.preset-hint {
  color: var(--text-muted);
  font-size: 0.85rem;
  margin-top: 12px;
}

.preset-inactive {
  color: var(--accent-bypass);
}
```

- [ ] **Step 3: Integrar no `App.tsx`**

(a) Imports. Trocar:

```tsx
import { AudioTestCard } from './AudioTestCard';
```
por:
```tsx
import { AudioTestCard } from './AudioTestCard';
import { PresetCard } from './PresetCard';
import { presetFromStatus } from './preset';
```

e trocar as duas linhas
```tsx
import type { DenoiseMode, EngineStatus, VirtualMicStatus, InputDeviceInfo } from './types';

export type { DenoiseMode, EngineStatus, VirtualMicStatus, InputDeviceInfo };
```
por:
```tsx
import type { DenoiseMode, EngineStatus, Preset, VirtualMicStatus, InputDeviceInfo } from './types';

export type { DenoiseMode, EngineStatus, Preset, VirtualMicStatus, InputDeviceInfo };
```

(b) Estado. Após `const [mode, setMode] = useState<DenoiseMode>('Active');`:

```tsx
  const [preset, setPreset] = useState<Preset>('Off');
```

(c) `fetchStatus`. Dentro de `if (res && res.mode) {`, após `setMode(res.mode);`:

```tsx
        setPreset(presetFromStatus(res));
```

(d) Handler. Logo após a função `handleModeChange` (antes de `handleRestartGeneration`):

```tsx
  const handlePresetChange = async (newPreset: Preset) => {
    try {
      const res = await invokeBridge<{ persisted?: boolean }>('set_preset', { preset: newPreset });
      setPreset(newPreset);
      if (res && res.persisted === false) {
        setErrorMessage(t('presets.notPersisted'));
      }
    } catch (err) {
      setErrorMessage(t('presets.switchFailed', { error: String(err) }));
    }
  };
```

(e) Render. Imediatamente após o `</div>` que fecha o card "Modo de Operação" (o bloco que contém `t('modes.mute')`) e antes do comentário `{/* Aceleração de Hardware & Runtimes de IA */}`, inserir:

```tsx
      {/* Acabamento de Estúdio (preset) */}
      <PresetCard preset={preset} mode={mode} onChange={handlePresetChange} />

```

- [ ] **Step 4: Checagem estática e testes**

Run: `(cd crates/app-tauri && npx tsc --noEmit && npm test)`
Expected: `tsc` sem saída; vitest passa todos os testes (nenhum teste novo nesta tarefa).

- [ ] **Step 5: Verificação manual (registrar o resultado no relatório; não automatizável aqui)**

Com o serviço rodando (ver Tarefa 8, Step 3) e `cd crates/app-tauri && npm run dev:ui`, abrir `http://127.0.0.1:5173`: fora do Electron e do Tauri a ponte devolve `{}` (sem daemon), então o card renderiza com `Off` selecionado e a seleção não persiste; isso confirma só a renderização e as strings (alternar o idioma pt-BR/en-US). O fluxo completo com o daemon (selecionar `Podcast`, reabrir, ver `Podcast` marcado) exige o Electron (`npm run start`) e fica como verificação manual do usuário.

- [ ] **Step 6: Gates da tarefa**

Run:
```bash
(cd crates/app-tauri && npx tsc --noEmit && npm test && node --check electron/main.cjs && node --check electron/preload.cjs)
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo test --workspace --locked --offline
```
Expected: tudo passa.

- [ ] **Step 7: Checkpoint**

Run: `git status --short`
Expected (além do que veio antes): `M crates/app-tauri/src/App.tsx`, `M crates/app-tauri/src/styles.css`, `?? crates/app-tauri/src/PresetCard.tsx`. Não comitar.

---

### Task 8: Gate de fechamento da fase e fumaça ponta a ponta (CLI + daemon)

**Files:**
- Nenhum arquivo do repositório é alterado (a fumaça usa diretórios temporários).

**Interfaces:**
- Consumes: tudo das Tarefas 2 a 7; binários `realtime-noise-service` e `realtime-noise-app-tauri`.
- Produces: evidência de que a fase fecha (spec §9) e de que o preset sobrevive a um reinício do serviço.

- [ ] **Step 1: Gates completos da fase (spec §9)**

Run:
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo build --workspace --locked --offline
cargo test --workspace --locked --offline
scripts/check-offline.sh
(cd crates/app-tauri && npx tsc --noEmit && npm test && node --check electron/main.cjs && node --check electron/preload.cjs)
```
Expected: tudo passa. `scripts/check-offline.sh` termina com status 0; se terminar com `BLOCKED_OFFLINE_DEPENDENCY`, registre o crate faltante e reporte (não é causado por esta fase se for um crate de terceiros que já era necessário; se for algo ligado a `studio-dsp`, é bloqueante).

- [ ] **Step 2: Compilar os dois binários da fumaça**

Run: `cargo build -p realtime-noise-service -p realtime-noise-app-tauri --locked --offline`
Expected: sucesso.

- [ ] **Step 3: Subir o daemon isolado, trocar o preset e conferir o arquivo**

Run (um único bloco de shell, a partir da raiz do worktree):
```bash
export XDG_RUNTIME_DIR="$(mktemp -d)" XDG_CONFIG_HOME="$(mktemp -d)"
./target/debug/realtime-noise-service --run > "$XDG_RUNTIME_DIR/daemon.log" 2>&1 &
echo $! > "$XDG_RUNTIME_DIR/daemon.pid"
sleep 1
./target/debug/realtime-noise-app-tauri --status
./target/debug/realtime-noise-app-tauri --preset podcast
./target/debug/realtime-noise-app-tauri --preset loud; echo "exit=$?"
./target/debug/realtime-noise-app-tauri --status
cat "$XDG_CONFIG_HOME/clearcore/settings.json"
stat -c '%a' "$XDG_CONFIG_HOME/clearcore/settings.json"
ls "$XDG_CONFIG_HOME/clearcore"
```
Expected:
- o primeiro `--status` imprime um JSON com `"preset":"Off"`;
- `--preset podcast` imprime `Preset updated successfully: {"persisted":true,"preset":"Podcast","success":true}`;
- `--preset loud` imprime `Unknown preset: loud. Expected off, natural, podcast, or broadcast` e `exit=1`;
- o segundo `--status` imprime `"preset":"Podcast"`;
- `settings.json` contém `"version": 1` e `"preset": "Podcast"`;
- `stat` imprime `600`;
- `ls` lista só `settings.json` (nenhum `.tmp`).

- [ ] **Step 4: Reiniciar o daemon e confirmar a persistência**

Run (mesmo shell, com as variáveis do Step 3 ainda definidas):
```bash
kill "$(cat "$XDG_RUNTIME_DIR/daemon.pid")"
sleep 1
./target/debug/realtime-noise-service --run > "$XDG_RUNTIME_DIR/daemon2.log" 2>&1 &
echo $! > "$XDG_RUNTIME_DIR/daemon.pid"
sleep 1
./target/debug/realtime-noise-app-tauri --status
echo '{ corrupted' > "$XDG_CONFIG_HOME/clearcore/settings.json"
kill "$(cat "$XDG_RUNTIME_DIR/daemon.pid")"
sleep 1
./target/debug/realtime-noise-service --run > "$XDG_RUNTIME_DIR/daemon3.log" 2>&1 &
echo $! > "$XDG_RUNTIME_DIR/daemon.pid"
sleep 1
./target/debug/realtime-noise-app-tauri --status
kill "$(cat "$XDG_RUNTIME_DIR/daemon.pid")"
```
Expected: o `--status` após o primeiro reinício mostra `"preset":"Podcast"`; depois de corromper o arquivo e reiniciar, o daemon sobe normalmente e o `--status` mostra `"preset":"Off"`; o `daemon3.log` contém `Service listening on unix domain socket:` e nenhum pânico.

- [ ] **Step 5: Checkpoint**

Run: `git status --short`
Expected (conjunto total desta fase, além dos arquivos da Fase 1):
```
 M Cargo.lock
 M crates/app-tauri/electron/main.cjs
 M crates/app-tauri/electron/preload.cjs
 M crates/app-tauri/src-tauri/src/commands.rs
 M crates/app-tauri/src-tauri/src/main.rs
 M crates/app-tauri/src/App.tsx
 M crates/app-tauri/src/bridge.ts
 M crates/app-tauri/src/i18n/locales/en-US.ts
 M crates/app-tauri/src/i18n/locales/pt-BR.ts
 M crates/app-tauri/src/i18n/types.ts
 M crates/app-tauri/src/styles.css
 M crates/app-tauri/src/types.ts
 M crates/diagnostics/tests/privacy.rs
 M crates/ipc/src/lib.rs
 M crates/ipc/src/protocol.rs
 M crates/service/Cargo.toml
 M crates/service/src/bootstrap.rs
 M crates/service/src/lib.rs
 M docs/ipc-v1.md
 M docs/service-lifecycle.md
?? crates/app-tauri/src/PresetCard.tsx
?? crates/app-tauri/src/__tests__/bridge.test.ts
?? crates/app-tauri/src/__tests__/preset.test.ts
?? crates/app-tauri/src/preset.ts
?? crates/ipc/tests/set_preset.rs
?? crates/service/src/settings.rs
?? crates/service/tests/common/mod.rs
?? crates/service/tests/preset_ipc.rs
?? crates/service/tests/settings.rs
?? fixtures/wire/ipc-v1-get-status-response.json
?? fixtures/wire/ipc-v1-set-preset-request.json
?? fixtures/wire/ipc-v1-set-preset-response.json
```
Não comitar nem dar push. Solicitar a revisão de código da fase (spec §9) antes de considerá-la fechada.

---

## Self-Review

**1. Spec coverage (seção 6 + escopo pedido):**
- `settings.json` versionado em `crates/service`, `Settings { version, preset }`, leitura tolerante, escrita atômica, 0600 → Tarefa 3 (testes: ausente, corrompido, versão futura, preset desconhecido, round-trip, 0600 sobre arquivo permissivo, temporário obsoleto, falha preserva o anterior).
- `ServiceConfig.settings_path`, carga na subida, aplicação ao `StudioControl` → Tarefa 4 (`ServiceBootstrap::new`, `with_settings`, testes de reboot e de arquivo corrompido).
- IPC `SetPreset`, `GetStatus` com `preset`, erro para preset inválido, golden novos, padrão `SetMode`, persistência ao receber → Tarefas 2 e 4; decisão v1 aditiva, com citação, nas "Decisões" (item 1) e documentada em `docs/ipc-v1.md` §6.
- Comando `set_preset` (CLI/`commands.rs`), tipo TS `Preset`, card de preset, strings en-US e pt-BR com acentuação, testes do frontend no padrão existente → Tarefas 5, 6 e 7 (também Electron, que é a UI real).
- Preset fora do export de diagnósticos → decisão 5 e teste-guarda (Tarefa 4).
- Fora de escopo respeitado: sem enrollment, sem `SetVoiceProfile`, sem presença de perfil no `GetStatus`.
- Lacuna conhecida, declarada: preset até o áudio no helper C (seção "Pendência fora do escopo").

**2. Placeholder scan:** sem "TBD/TODO/implementar depois"; todos os passos de código trazem o código. O único código provisório é o braço `NOT_IMPLEMENTED` da Tarefa 2 (Step 5), explicitamente substituído na Tarefa 4 (reescrita completa de `lib.rs`).

**3. Type consistency:** `StudioPreset` (ipc) ↔ `Preset` (studio_dsp) convertidos só por `convert_ipc_preset_to_dsp` / `convert_dsp_preset_to_ipc` (definidos na Tarefa 3, usados na 4). `Settings { version: u32, preset: Preset }`, `Settings::load/save/to_json/parse_or_default`, `persist_settings(Settings, Option<&Path>) -> bool`, `settings_path_from(Option<PathBuf>, &Path)`, `default_settings_path()` aparecem com a mesma assinatura na definição (Tarefa 3), no uso (Tarefa 4) e nos testes. `ServiceDaemon::{with_settings, studio_control, settings}` e `ServiceConfig { endpoint_path, settings_path }` idem. Payload `{preset, success, persisted}` e `request_id` `"set-preset-resp"` coincidem entre `lib.rs`, golden e testes; `"status-resp"` e as chaves do `GetStatus` coincidem com o golden. No frontend: `Preset`, `PRESETS`, `presetFromStatus`, `presetLabelKey`, `presetHintKey`, `invokeBridge('set_preset', { preset })`, `ClearcoreApi.setPreset`, preload `setPreset`, `ipcMain.handle('set_preset')` e as chaves `presets.*` (14) são consistentes entre tipos, os dois locales, testes e `PresetCard`.

**Não verificado ao planejar:** este ambiente não tem toolchain Rust (`cargo`/`rustc` ausentes do PATH), então nenhum código Rust do plano foi compilado, formatado nem passou por clippy; o código foi escrito contra as lints do workspace, mas o primeiro `cargo clippy` pode apontar ajustes (cada tarefa diz como tratá-los sem `#[allow]` novo). O frontend teve apenas a linha de base conferida (`npx vitest run` com 6 testes e `npx tsc --noEmit` sem erros). O crate `studio-dsp` da Fase 1 não existe ainda; suas assinaturas vêm do contrato informado e são verificadas na Tarefa 1. O comportamento do Electron e o fluxo gráfico completo não são automatizáveis aqui.
