# Handoff — Clearcore Studio (2026-10-02)

Para quem continuar em outro harness. Tudo está no **working tree** do worktree
`<REPO>`, branch `docs-using-superpowers-skill`.
Os commits foram feitos em grupos lógicos nessa branch (veja `git log --oneline -n 10`) e **já foram enviados e
mergeados**: PR #1 (https://github.com/joaorura/clearcore/pull/1) na `master`, merge commit `103b187`, e a **release
`v0.1.0-beta.2` foi publicada** (prerelease): https://github.com/joaorura/clearcore/releases/tag/v0.1.0-beta.2
(seção 5c). O `schedule` dos workflows de hardware já não existe na `master`. O app se chama **Clearcore**; "hippocamp" é só o nome do
worktree (memória do projeto: `hippocamp-e-nome-de-worktree`; as notas antigas do vault ainda dizem `project: hippocamp`).

Idioma: português do Brasil. Modelos: **Sonnet e Haiku** para quase tudo, Opus só em caso raro (decisão do usuário).

## 1. Objetivo

Evoluir o Clearcore de "supressão de ruído" para "isolamento do locutor cadastrado + acabamento de estúdio":
pDFNet3 (DeepFilterNet3 + FiLM, **treinado em repo separado**), EQ neural (idem) e cadeia DSP em Rust neste repo.
Spec: `docs/superpowers/specs/2026-10-01-clearcore-studio-pipeline-design.md` (desenho aprovado seção a seção).

## 2. Estado por fase

| Fase | Conteúdo | Estado |
|---|---|---|
| 0 | benchmark local, `tech-stack.md`, script de amostras | **feita e verificada** (benchmark termina `BLOCKED_UNSUPPORTED_CPU_PROFILE` neste host; nenhum número inventado) |
| 1a | crate `crates/studio-dsp` (DSP puro) | **feita, revisada por agente independente, achados 1/2/4 corrigidos** |
| 1b | integração (`StudioBackend`, engine, `filter-capi`, helper C, Electron) | **plano pronto, NÃO executada** |
| 2 | `settings.json`, IPC `SetPreset`, UI de preset | **plano pronto, NÃO executada** |
| 3 | spike do fork do libDF | **feito: GO** (relatório abaixo) |
| 4 | registro de assets, backend personalizado, enrollment | **sem plano** (escrever com o resultado do spike) |
| 5 | assets reais (pDFNet3, enrollment, EQ) | **bloqueada**: depende de pesos do repo de treino, que não existe aqui |

Fora desse roteiro, também foram feitos: 4 correções de áudio/instalação (seção 5) e o `schedule` removido dos
3 workflows de hardware. As correções foram **publicadas na v0.1.0-beta.2** (seção 5c).

### Evidência final (rodada nesta sessão, do zero)

| Gate | Resultado |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings` | exit 0 |
| `cargo test --workspace --locked --offline` | **265 passando, 0 falhando** (baseline antes: 189) |
| `cargo test -p studio-dsp` | 63 + 12 + 1 passando |
| `scripts/check-offline.sh` | **exit 0** *com a toolchain carregada* (sem ela dá `BLOCKED_OFFLINE_DEPENDENCY`: não é falha real) |
| `platform/linux/tests/check-virtual-mic-fakes.test.sh` | 25 passaram, 0 falharam |
| `crates/app-tauri/scripts/linux-install.test.sh` | 21 passaram, 0 falharam |
| `crates/app-tauri/scripts/capture-link-plan.selftest.cjs` | 8/8 |
| teste C `test_target_resolver.c` (gcc direto) | passou |
| `bash -n` / `node --check` | ok (7 .sh, 4 arquivos node) |
| `platform/linux/tests/detect-hardware-fakes.test.sh` | 23 ok, 0 falhas (vermelho antes: 10 ok / 13 falhas) |
| `platform/linux/tests/check-virtual-mic-fakes.test.sh` (atual) | agora **39/39** (eram 25; o teste do layout de pacote falhava antes: 31 ok / 8 falhas) |
| `crates/app-tauri/scripts/hardware-json.selftest.cjs` | 9/9 |
| `crates/app-tauri/scripts/backend-selection.selftest.cjs` | 4/4 |
| vitest (`crates/app-tauri`) e `tsc --noEmit` | 18/18 e exit 0 |
| `release.yml` e os 3 workflows de hardware | YAML válido |

**Não verificado em nenhum momento:** áudio real (nada foi ouvido), o helper rodando, o Electron com GNOME real,
`meson`/`ninja` do helper (falta `libpipewire-0.3` aqui; só `gcc -c` contra headers de `~/.local/usr/include`),
Fedora 41 / PipeWire 1.2, macOS e Windows, o PC dos colegas.

## 3. Como retomar (ambiente)

- **A toolchain que usei é isolada e vive em `/tmp` (tmpfs) da sessão antiga**:
  `<SCRATCHPAD>/rust-env/`
  (`env.sh`). Pode não existir mais. Para recriar: `rustup toolchain install 1.90.0 --profile minimal -c clippy -c rustfmt`
  (o `rust-toolchain.toml` do repo fixa 1.90.0), depois `cargo fetch --locked` **uma vez com rede**; só então
  `--locked --offline` funciona. A máquina original não tinha Rust no PATH.
- `CARGO_TARGET_DIR` foi movido para o `target/` do repo (no disco, ignorado pelo git; 13 GB). **Não ponha o target em
  `/tmp`**: é RAM; com ele cheio o linker morreu com `Bus error` (`ld ... signal 7`).
- Cada chamada de shell é isolada: **carregue o ambiente (`source env.sh`) na mesma linha do cargo**.
- CI (`.github/workflows/ci.yml`): `fmt --check`, `clippy --workspace --all-targets --all-features --locked --offline -D warnings`,
  build, test, `scripts/check-offline.sh`. Política do workspace: `unsafe_code = "forbid"`, `unwrap/expect/panic = deny`,
  deps pinadas com `=` e `default-features = false`.
- **Amostras de voz** (nunca no git): `scripts/fetch-voice-samples.sh` (VCTK p225/p226 CC BY 4.0 + 3 ruídos CC0), tutorial em
  `docs/testing/voice-samples.md`. `--with-tagarela` baixa um shard de teste (CC BY-NC-SA: só teste local).

## 4. O que mudou no working tree

**Modificados:** `.gitignore`, `Cargo.toml`, `Cargo.lock`, `INSTALL.md`, `conductor/tech-stack.md`,
`crates/app-tauri/electron/main.cjs`, `crates/app-tauri/scripts/package-app.mjs`, `platform/linux/helper/meson.build`,
`platform/linux/helper/src/pipewire_helper.{c,h}`, `scripts/check-virtual-mic.sh`, `scripts/uninstall-linux.sh`,
`.github/workflows/{linux,macos,windows}-hardware.yml` (schedule removido).

**Novos (antes não rastreados):** `crates/studio-dsp/`, `crates/app-tauri/scripts/linux-install.sh`,
`crates/app-tauri/scripts/linux-install.test.sh`, `crates/app-tauri/scripts/capture-link-plan.selftest.cjs`,
`crates/app-tauri/electron/capture-link-plan.cjs`, `platform/linux/helper/src/target_resolver.h`,
`platform/linux/helper/tests/test_target_resolver.c`, `platform/linux/tests/check-virtual-mic-fakes.test.sh`,
`scripts/fetch-voice-samples.sh`, `scripts/run-cpu-baseline-local.sh`, `scripts/voice-samples.sha256`, `benchmarks/local/`,
`docs/testing/`, `docs/superpowers/{specs,plans}/2026-10-01-*`.

**Tudo está commitado e já foi mergeado na `master`** (ver `git log` e a seção 5c). Arquivos que passaram a existir **desde o handoff original**:
`crates/app-tauri/electron/{hardware-json,backend-selection}.cjs`,
`crates/app-tauri/scripts/{hardware-json,backend-selection}.selftest.cjs`, `crates/app-tauri/src/hardwareBackend.ts`,
`crates/app-tauri/src/__tests__/hardwareBackend.test.ts`, `platform/linux/tests/detect-hardware-fakes.test.sh`.
Modificados desde então: `scripts/detect-hardware.sh`, `scripts/install-openvino.sh`,
`crates/app-tauri/src/{HardwareAcceleratorCard.tsx,bridge.ts,i18n/locales/{pt-BR,en-US}.ts,i18n/types.ts}`,
`docs/accelerator-qualification.md`, `docs/ga-decision.md`, `.github/workflows/release.yml`.

**Risco:** `package-app.mjs` depende de `linux-install.sh` (ele lê esse arquivo); hoje ambos estão no **mesmo commit**,
então não separe os dois ao reordenar, cherry-pickar ou reverter.

## 5. Correções de áudio/instalação (a pedido, de relatos de usuários)

Relato original: um amigo no **Fedora 41**; esta máquina é **Fedora 44** (PipeWire 1.6.9, WirePlumber 0.5.18, KDE).

| | Causa raiz (confirmada no código) | Correção |
|---|---|---|
| A | o helper passava o **id** do nó em `PW_KEY_TARGET_OBJECT`; o WirePlumber lê número como `object.serial` (`find-defined-target.lua:45-78`), então nenhum mic ligava e saía silêncio. **Existe também no Fedora 44**, não é só do 41 | resolve id→`node.name` (`target_resolver.h`, `pipewire_helper.c`, `check-virtual-mic.sh`) |
| B | GNOME sem extensão appindicator não mostra bandeja; o instalador não avisava | `linux-install.sh` avisa (não instala nada); `createTray` em try/catch; `INSTALL.md` |
| C | `icon.png` é 64x64, mas era instalado em `hicolor/256x256` | instala no `hicolor/<LxA>` real (lê IHDR); uninstall remove só `*/apps/clearcore.png` |
| D | só portas `_FL/_FR/_1/_2` eram ligadas; mic mono (`capture_MONO`, ex. Bluetooth) nunca ligava | `capture-link-plan.cjs`; `ensure_capture_link` reescrita |

Revisão independente: sem bloqueios. Corrigidos depois dela: fallback que ligava o *primeiro* nó que casava (podia somar
webcam ao mic escolhido), env numérico sobrepondo `--target NOME`, `strncpy`→`snprintf`. O `find -delete` dos
desinstaladores foi testado e está bem escopado.

**Achados adiados (nenhum bloqueia):** (2) não há teste de integração da fiação em `pipewire_helper.c`
(registry→tabela→resolve→propriedade), do uso de `planCaptureLinks` em `main.cjs` nem dos dois `find -delete`;
(4) portas multicanal `AUX0/AUX1` com 2+ portas não geram link; (5) `ensure_capture_link` roda uma vez, sem retry se as
portas ainda não existem; (7) `png_size` não valida assinatura PNG; (8) o aviso do GNOME sob `sudo` consulta o
`gnome-extensions` do root. Também: `physical_source_name` escolhe o primeiro mic do `wpctl`; se o helper subiu com
outro `--target`, `--set-default` pode ligar um mic diferente do que o helper usa. Preexistente em `main.cjs:~799`:
`execSync` com `"${srcPort}"` interpolado no shell.

## 5b. Correções do HELPER_BIN, do seletor de aceleradores e da detecção (feitas depois, a pedido)

1. **HELPER_BIN.** `scripts/check-virtual-mic.sh:38-56` agora resolve o helper por candidatos: override
   `CLEARCORE_HELPER_BIN`, `${SCRIPT_DIR}/bin/pipewire_helper` (layout do pacote) e o caminho de dev. Sem helper, emite
   AVISO no stderr ("usando loopback SEM supressao de ruido") **sem mudar o JSON**. A seção F do teste cobre pacote,
   override, dev, precedência e ausência. O `release.yml` (job `build-linux`) ganhou o passo "Verify Linux Package
   Contents", que confere o tarball — **rodou pela primeira vez no CI e passou** (run da v0.1.0-beta.2, seção 5c).
2. **Seletor de acelerador.** `set_hardware_backend` (`main.cjs` ~1303) usa `electron/backend-selection.cjs` e só aceita
   `auto` e `cpu_tract`; os demais devolvem `{success:false, reason:'not_implemented', active_backend:'cpu_tract'}`. O
   card mostra o selo "Prévia — ainda não processa áudio", não permite selecioná-los, perdeu o toast falso, mostra
   "CPU (Tract)" como mecanismo ativo e o acelerador em "Detectado (ainda não utilizado)". Também foram reescritos
   `autoRecommend`/`autoResolvedCurrent` nos locales. Os docs `accelerator-qualification.md` e `ga-decision.md` foram
   corrigidos (a seleção não está ligada ao motor; os plugins são stubs em staging).
3. **Detecção.** `detect-hardware.sh` reconhece OpenVINO em `/opt/intel/openvino*/runtime/lib/intel64`,
   `INTEL_OPENVINO_DIR` e `LD_LIBRARY_PATH`, roda a sonda `import openvino` do PATH (timeout de 4 s) mesmo sem a base por
   arquivo, e não tem mais caminhos específicos da máquina do desenvolvedor. Variáveis novas: `CLEARCORE_PYTHON`, `CLEARCORE_PROBE_TIMEOUT` e
   `CLEARCORE_DETECT_ROOT` (seam de teste). O `main.cjs` extrai o JSON de forma tolerante (`hardware-json.cjs`), com
   timeout de 15 s e `detection_error` visível. `install-openvino.sh` para Fedora lista pacotes que existem
   (`openvino`, `openvino-plugins` — onde está o plugin NPU —, `intel-npu-driver`, `intel-npu-compiler`,
   `oneapi-level-zero`); os antigos `intel-driver-compiler-npu` e `intel-level-zero-npu` **não existem** no Fedora 44.

**Ressalvas:** a NPU real, o app rodando e o PC do usuário nunca foram verificados; o backend OpenVINO continua stub
passthrough (o áudio é sempre Tract na CPU).

## 5c. Release v0.1.0-beta.2 (publicado)

- **PR #1** (https://github.com/joaorura/clearcore/pull/1) **mergeada** na `master` em 2026-10-02T12:32:14Z, merge commit
  `103b187` (= topo de `origin/master` e = alvo da tag anotada `v0.1.0-beta.2`). Antes do push a `master` foi trazida para
  a branch e todos os gates passaram **no resultado mesclado** (fmt, clippy `-D warnings`, 265 testes, `check-offline`,
  vitest 18, `tsc`, selftests, testes shell).
- **Release** publicada (prerelease) em 2026-10-02T12:43:53Z:
  https://github.com/joaorura/clearcore/releases/tag/v0.1.0-beta.2. Assets: `Clearcore-darwin-arm64.tar.gz`
  (130214214 B), `Clearcore-linux-x64.tar.gz` (142223703 B), `Clearcore-win32-x64.zip` (158085987 B).
- **Run do release** 37007293383 (https://github.com/joaorura/clearcore/actions/runs/37007293383): os 4 jobs `success`,
  incluindo os passos novos "Verify Linux Package Contents" e "Verify release notes exist" (primeira vez no CI; passaram).
  O tarball Linux publicado foi baixado e conferido: contém `resources/bin/pipewire_helper` e o `check-virtual-mic.sh`
  com a correção (`CLEARCORE_HELPER_BIN`); o tamanho baixado é igual ao do asset.
- O `release.yml` agora deriva `body_path` da tag (`release/RELEASE-NOTES-${{ github.ref_name }}.md`) e **exige o
  arquivo**; publicar só é suportado por tag `v*`.
- **Versão:** o bump foi só nos pontos do app (`package.json`, `package-lock.json` 2x, `tauri.conf.json`, `main.cjs`
  `APP_VERSION`, `Clearcore-Setup.nsi`/`.iss`); os `Cargo.toml` continuam `0.1.0`.
- **Ressalva — o `ci.yml` NÃO validou a PR:** usa runner self-hosted (0 registrados) e `push: branches: [main]` (a branch
  padrão é `master`), então o job ficou QUEUED; nenhum check automatizado rodou na PR (os gates foram locais).
- Os workflows `*-hardware.yml` já não têm `cron` na `master` (confirmado); as 3 execuções presas foram canceladas e o
  cron **não volta** a enfileirar.
- **Tag local `v0.1.0-beta.1` DESATUALIZADA** em relação ao remoto de propósito (a remota foi movida para `b34766c`):
  não use `git push --tags` nem `git fetch --tags --force` sem decidir isso.
- A branch remota `docs-using-superpowers-skill` **ainda existe** (não foi apagada).

## 6. `studio-dsp` — achados adiados da revisão

- **AGC amplifica ruído de fundo até +12 dB** nas pausas (congela só abaixo de −50 LUFS; o ruído residual do denoiser
  fica entre −50 e −28). Decidir por **escuta com fala real**, que nunca foi feita (só sinal sintético).
- Cosméticos: dois valores para o teto do limiter (`CEILING_DBFS` vs `CEILING_LINEAR_F32`; os testes unitários comparam
  com a constante errada); custo O(97) por amostra; crossfade Off↔ativo mistura seco sem atraso com ativo atrasado 96
  amostras (filtro de pente de ~8 ms, e o teto de −1 dBFS não vale nesse hop); `Release/Acquire` mais forte que o necessário.
- Decisões aceitas: `latency_samples()` fixo em 96 também em `Off` (superestima 96 amostras; nada no código real afirma
  1.440 sobre o backend decorado); dev-dependency `stats_alloc =0.1.10` (justificada: alocador contador escrito à mão exige `unsafe`).
- Valores dos presets (`Natural/Podcast/Broadcast`) são **pontos de partida**, não ajustados por escuta.

## 7. Spike do libDF (fase 3): GO

Relatório: `docs/superpowers/plans/2026-10-01-clearcore-studio-phase3-libdf-spike-report.md` (traz o diff completo do patch).
Fork com entradas `gamma/beta`: paridade **bit-exata** com o upstream em identidade; custo +0,1% (p50 656 µs, p99 0,74 ms
contra o limite de 7 ms; Core Ultra 7 265H); cabeça de EQ antes do iSTFT viável com lag 0; a hipótese do atraso do
pulsifier não apareceu. Ressalvas para a fase 4: patch de +127/−21 linhas (não "mínimo"); o fork precisa ser
**vendorizado** (hoje é `git`+`rev`, exige rede) e **contém `unsafe`**, em conflito com `unsafe_code = "forbid"`
(`CONTRIBUTING.md:39-46`: a única exceção é `filter-capi`) → crate isolado fora do workspace ou segunda exceção explícita
(decisão do dono). A rota de constantes embutidas só vale para o spike (um ONNX por usuário não tem SHA-256 fixo no gate
de assets). Licença do libDF: `MIT OR Apache-2.0` (manter LICENSE-MIT/APACHE e copyright; marcar arquivos modificados).

## 8. Próximos passos, em ordem

1. **Instalar a v0.1.0-beta.2 num Linux** (Fedora 41/44) e confirmar que a supressão de ruído funciona (o helper de
   `resources/bin`) e que o seletor, o ícone e a bandeja se comportam.
2. **Fase 1b** (`...phase1b-integration.md`): **revalidar âncoras primeiro**. As correções de áudio mexeram em
   `pipewire_helper.c`, `main.cjs` e `check-virtual-mic.sh`, então números de linha do plano podem ter deslocado.
3. **Fase 2** (`...phase2-settings-ui.md`), depois da 1b (compartilham `Cargo.lock`, `engine`, `main.cjs`).
   Ajustes decididos na revisão cruzada: no handler `set_preset` do Electron incluir `syncPresetToHelper` protegido por
   `typeof` (senão o preset só chega ao helper pelo poll de 2,5 s); corrigir textos desatualizados do plano (pendência do
   helper C já resolvida pela 1b); conferir gates de git/`Cargo.lock` por **caminho**; 1b não tem passo de revisão de
   código (usar um revisor separado); card de preset deve avisar fora do Linux e `docs/support-matrix.md` deve dizer que
   Windows/macOS ficam sem preset no áudio; a UI real é **Electron** e os golden de IPC são `ipc-v1-*.json`.
4. **Plano da fase 4** (a escrever): registro de descritores de assets com a mesma verificação de assinatura (ler a regra
   `policy_object.len() != 1` do `trust-policy.json`), fork vendorizado do libDF, enrollment (ONNX áudio 16 kHz →
   `gamma_enc, beta_enc, gamma_df, beta_df`; só os vetores são salvos, arquivo 0600, fora dos diagnósticos).
   O agente **não produz assinaturas**: testes com chave e trust-policy de desenvolvimento.
5. **Spec do repo de treino** (separado). Requisitos já fixados: modo "sem locutor" estável (embedding dropout);
   enrollment denoisado como aumentação (p≈0,5); ONNX de enrollment; EQ neural com saída em **ganhos dB por banda**
   limitados a [−6,+12] e suavizados, aplicados no domínio do tempo (sem overlap-add); contrato do modelo inalterado
   (960/480/32/96/5/2); script que exporta o DFNet3 com FiLM em identidade; ablações (enrollment cru vs denoisado por SNR;
   A/B de aplicação do EQ); licenças de dados compatíveis com pesos abertos.
6. Medir a **latência fim a fim real** antes de gastar margem (hoje só há orçamento: 70 ms no pior caso, p95 ≤ 80 ms).

## 9. Decisões em aberto (são do usuário)

1. **Bug do `HELPER_BIN`**: **corrigido, commitado, mergeado e PUBLICADO na v0.1.0-beta.2 (verificado no tarball);
   pendentes: tornar o fallback visível na UI (JSON + `main.cjs`, decisão do dono; vale para o próximo release) e
   confirmar o comportamento real em um PC Linux (Fedora 41/44) instalando a v0.1.0-beta.2.** (Causa original: o script
   procurava `resources/platform/linux/helper/build/pipewire_helper`, mas o pacote põe o helper em
   `resources/bin/pipewire_helper`; ver 5b.)
2. **Seletor de acelerador (OpenVINO)**: **CORRIGIDO (opção "prévia/desabilitar"); ligar a seleção de ponta a ponta
   continua fora de escopo (backend OpenVINO real com `unsafe`, comando IPC, gate de qualificação).** (Causa original:
   `set_hardware_backend` só guardava uma variável e devolvia `success:true`; o `filter-capi` constrói sempre
   `TractBackend`; `openvino.rs` é stub passthrough; ver 5b.)
3. **Push/PR/merge/release**: **FEITOS** (PR #1, `103b187`, release v0.1.0-beta.2; seção 5c). As 3 execuções presas
   (36980913239, 36981750310, 36982741766) foram **CANCELADAS** e o cron não volta a enfileirar (removido na `master`).
   Causa original da fila: **0 runners self-hosted** registrados. **Pendentes (decisão do dono):** apagar a branch remota
   `docs-using-superpowers-skill`, se quiser; alinhar a tag local `v0.1.0-beta.1` (ver 5c); e corrigir o `ci.yml`
   (runner self-hosted inexistente e `branches: [main]` — hoje **nenhuma PR é validada automaticamente**): registrar um
   runner, trocar para `ubuntu-latest` ou mudar o gatilho para `master`.
4. Fork do libDF com `unsafe`: crate fora do workspace ou nova exceção à política.
5. Common Voice pt: licença **não confirmada** na fonte oficial (MLS Portuguese é CC BY 4.0 mas 16 kHz).
   TAGARELA (CC BY-NC-SA) liberado só como dado de teste; fora do treino de pesos distribuídos até decisão registrada.
6. Vault: **FEITO** — 3 notas criadas no vault geral (`decisoes/2026-10-02-clearcore-pipeline-studio-arquitetura.md`,
   `decisoes/2026-10-02-clearcore-bugs-pacote-linux-e-aceleradores.md`,
   `padroes/orquestracao-agentes-aprovacao-travada.md`) e appends de backlink em `projetos/hippocamp.md` e
   `projetos/ambiente-claude-code.md`; as notas antigas ainda dizem `project: hippocamp`.

## 10. Armadilhas e lições operacionais

- **Agentes travam em pedidos de aprovação** quando o usuário está ausente: um agente ficou **8 horas** parado num
  comando composto (`rm -rf crates/*` + `cd` + heredocs). Proibir `rm -rf`, globs com `rm` e comandos longos; gravar
  progresso em arquivo a cada tarefa; checar `ListAgents` e o mtime dos arquivos em vez de só esperar.
- **Não tocar o PipeWire real** da máquina em testes (`pw-loopback`, `pw-link`, `wpctl set-*`, subir o helper): os testes
  do repo usam `pw-*` falsos e HOME falso.
- Relatos de agentes baratos foram **conferidos antes de aceitar** (ex.: um disse que `check-offline.sh` bloqueado era
  "esperado"; era só a toolchain fora do PATH).
- `scripts/check-offline.sh` retornando `BLOCKED_OFFLINE_DEPENDENCY` e listando `cargo`/`rustc` ausentes significa
  toolchain fora do PATH naquele shell, não falha real.
- O relato de um agente em segundo plano pode ser **provisório** ("ainda sem saída"): consulte o estado real (`gh`,
  `git ls-remote`) antes de concluir.
- `grep -c` devolve código 1 quando a contagem é 0: não encadeie com `&&`.
- O `/tmp` é tmpfs e some no reinício; os logs e artefatos de verificação do scratchpad são voláteis.
- `unsafe_code = "forbid"` + `filter-capi` como única exceção; `ProcessedFrame::checked` rejeita saída não finita.
- Os números de latência/CPU "oficiais" nos docs de evidência (`28,4 ms`, `4,8 ms`, "8h soak") estão **fixos no código**
  (`tools/qualification/collect_metrics.rs:35-37`, `run_matrix.py`) ou vêm de backends mock; não são medição.
- `README.md:42` cita 0,276 ms/frame sem registrar a máquina (benchmark em C que repete o mesmo frame).

## 11. Mapa de documentos

- Spec: `docs/superpowers/specs/2026-10-01-clearcore-studio-pipeline-design.md`
- Planos: `docs/superpowers/plans/2026-10-01-clearcore-studio-phase{0-baseline-samples,1a-studio-dsp-crate,1b-integration,2-settings-ui,3-libdf-spike}.md`
  e o relatório do spike (`...phase3-libdf-spike-report.md`).
- Vault: `decisoes/2026-09-24-hippocamp-open-source-gratuito.md` (app gratuito e open source; pesos do DFNet3 tratados como
  `MIT OR Apache-2.0` por decisão do dono), `decisoes/2026-09-25-hippocamp-backends-gpu-por-fornecedor.md`
  (TensorRT/NVIDIA, MIGraphX/AMD; não cita OpenVINO).
