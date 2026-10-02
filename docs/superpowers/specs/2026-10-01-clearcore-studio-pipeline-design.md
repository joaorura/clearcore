# Clearcore: pipeline de voz personalizada e acabamento "estúdio" — design

Data: 2026-10-01. Status: desenho aprovado pelo usuário em conversa (seções 1 a 5). Este documento é o spec do
repositório Clearcore. O repositório de treino (pDFNet3, enrollment, EQ neural) tem spec próprio; os requisitos que
este spec impõe a ele estão na seção 7.

## 1. Objetivo e escopo

Evoluir o Clearcore de "supressão de ruído" para "isolamento do locutor cadastrado + acabamento de estúdio":

1. **Estágio 1** — pDFNet3: o DeepFilterNet3 condicionado ao locutor por FiLM (treinado fora deste repositório).
2. **EQ neural** — corrige a coloração do microfone com ganhos por banda (treinado fora deste repositório).
3. **Cadeia DSP** — high-pass, EQ fixo, de-esser, compressor, AGC de loudness e limiter, em Rust, neste repositório.

Fora de escopo: BWE / extensão de banda (decidido não fazer agora), qualquer modelo generativo no caminho de tempo
real, e o código de treino.

Princípio de entrega: **o que não depende de modelo treinado vem primeiro**. As fases 1 e 2 entregam valor sem ML.
O produto nunca fica pior do que é hoje: sem asset novo aprovado, roda DFNet3 v1 + DSP.

## 2. Estado atual verificado (levantamento de 2026-10-01)

- Runtime: `libDF` (`deep_filter`) com `tract` 0.19.16, DFN3 v0.5.6, 48 kHz mono, hop 480, latência algorítmica
  1.440 amostras / 30 ms (`crates/model/src/lib.rs:22`).
- Trait `InferenceBackend { descriptor, process(&AudioFrame) -> ProcessedFrame, algorithmic_latency_samples }`
  (`crates/model/src/lib.rs:96-100`). `AudioFrame = [f32; 480]` (`crates/contracts/src/audio.rs`).
- Dois caminhos de áudio: engine Rust (`crates/engine/src/worker.rs:17`, `engine.rs:272-317`) e, no Linux, o helper C
  que chama `filter-capi` (`clearcore_filter_*`) por `dlopen`. O fallback `noise_suppressor.c` não passa pelo modelo.
- Gate de assets: SHA-256, tamanho e assinatura Ed25519 fixos em código (`asset_manifest.rs:15-20,142-149`),
  allowlist de 4 membros (`archive.rs:9-30`), contrato do modelo 960/480/32/96/5/2 (`tract_backend.rs:156-174`).
  O loader upstream não aceita inputs extras.
- IPC `realtime-noise.v1`: `GetStatus, SetMode, RestartGeneration, GetDiagnostics, Shutdown`
  (`crates/ipc/src/protocol.rs:28-34`). `ServiceConfig` só tem `endpoint_path`. O modo fica em memória.
- Não existe: código de treino, DSP pós-denoiser, preset, enrollment, configuração persistida, áudio de teste.
- Latência: o único número medido é 0,276 ms por frame (README; sem máquina registrada). O orçamento de produto soma
  70 ms no pior caso (p95 ≤ 80 ms; `design.md:313`) e **nunca foi medido fim a fim**. Os p95 de 28,4 ms nos docs de
  evidência estão fixos no código e não são medição.

## 3. Pipeline-alvo

```
entrada → [pDFNet3 + FiLM] → [EQ neural: ganhos por banda → filtro de fase mínima] → [DSP] → saída
          (assets assinados)   (asset assinado)                                       (código)
```

Em `Bypass` o áudio passa cru; em `Mute` sai zero; todo o resto só roda em `Active`.

## 4. Crate `studio-dsp`

- `crates/studio-dsp`, Rust puro, **sem dependências externas**, `#![forbid(unsafe_code)]`, em conformidade com a
  política do workspace (lints `unwrap/expect/panic = deny`).
- Contrato: `StudioChain::new(Preset)`, `process(&mut self, &mut [f32; 480])` sem alocação e sem panic,
  `latency_samples()` (só o lookahead do limiter, ~96 amostras / 2 ms).
- Ordem: high-pass 80 Hz → EQ fixo (2 biquads) → de-esser (passa-banda 5–9 kHz com envelope) → compressor
  (attack/release, razão, limiar, makeup) → AGC (ITU-R BS.1770, ganho lento, alvo −16 LUFS) → limiter com lookahead
  e teto de −1 dBFS.
- Presets: `Off` (**passagem bit a bit idêntica**), `Natural`, `Podcast`, `Broadcast`, em tabela constante. Os valores
  iniciais são ponto de partida e se ajustam por escuta. Padrão do produto: `Off`.
- Segurança numérica: nunca produzir NaN/infinito (`ProcessedFrame::checked` rejeita); tratar denormais.
- Testes (sem áudio commitado): sinais sintéticos. `Off` idêntico; limiter respeita o teto; ganho do compressor bate com
  a fórmula; tom calibrado mede a loudness esperada (±0,1 LU); zero alocações por hop (`stats_alloc`, como o benchmark).

## 5. Integração nos dois caminhos

- **Decorator** `StudioBackend<B: InferenceBackend>` implementa a mesma trait: `process` chama o backend interno e
  depois a cadeia; `algorithmic_latency_samples` soma a latência da cadeia. O engine e o `filter-capi` apenas embrulham
  o backend que já criam. A trait não muda.
- Modos tratados antes do backend (conferir no helper C; ponto a verificar na fase 1).
- Reinício de geração do engine zera o estado do DSP.
- Troca de preset por valor atômico compartilhado, aplicada no limite de frame com crossfade de ~10 ms.
  `filter-capi` ganha `clearcore_filter_set_preset`.
- Fallback do helper C (biblioteca neural ausente): **sem estúdio**. Modo degradado documentado.

## 6. Configuração, enrollment e UI

- **IPC**: `SetPreset { preset }` e `SetVoiceProfile { profile | clear }`; `GetStatus` informa preset e presença de
  perfil. Se o protocolo v1 admite variantes novas ou exige v2 (com novos golden em `fixtures/wire/`), decide-se lendo
  `docs/ipc-v1.md` na fase 2.
- **Persistência**: `settings.json` versionado, escrita atômica (temporário + `rename`), carregado na subida; o
  `ServiceConfig` ganha `settings_path`. Padrão: preset `Off`, sem perfil.
- **Enrollment**: um ONNX de enrollment (áudio 16 kHz → `gamma/beta` do encoder e do DF decoder). Fluxo na UI: gravar
  6–10 s → passar pelo pDFNet3 em modo "sem locutor" → validar (fala efetiva, nível, SNR via `lsnr`, consistência entre
  janelas de ~2 s por similaridade de embedding) → A/B original vs. limpo → gerar o perfil. **A gravação é descartada**;
  só os vetores ficam (arquivo 0600, fora do export de diagnósticos, com teste de privacidade novo). Se a ablação do
  repo de treino mostrar que o denoise do enrollment não ganha, a UI usa o áudio cru e mantém a validação.
- **UI (Tauri)**: `set_preset`, `enroll_voice`, `clear_voice_profile`; cards de preset e de cadastro; strings em
  `en-US` e `pt-BR`.

## 7. Assets neurais e requisitos ao repo de treino

Três assets novos (pDFNet3, enrollment, EQ neural); o DFNet3 v1 atual permanece aprovado e é o fallback.

- O gate vira um **registro de descritores** (id, SHA-256, tamanho, allowlist de membros, contrato), todos passando pela
  mesma verificação de assinatura. A regra `policy_object.len() != 1` do `trust-policy.json` deve ser lida na fase 4.
- **Backend personalizado**: fork do libDF no commit já pinado, com patch mínimo para alimentar `gamma/beta` quando o
  grafo declarar esses inputs. **Spike (fase 3)** com teste de paridade (gamma=1, beta=0 contra o upstream) decide a
  viabilidade. Conferir a licença do código do libDF.
- **EQ neural**: contrato = ganhos em dB por banda, limitados a [−6, +12] dB e suavizados; aplicados no domínio do
  tempo por filtro de fase mínima (sem overlap-add, sem latência extra). O método de aplicação é detalhe de runtime e
  não exige retreino; o STFT próprio (+10 ms) só entra se ganhar de forma clara no A/B cego e **depois de medida a
  latência fim a fim**. Terceira variante a avaliar no spike: cabeça de EQ dentro do pDFNet3, aplicada no espectro do
  libDF antes do iSTFT.
- Assinaturas de manifest exigem a chave do usuário: o agente **não** produz assinaturas; testes usam chave e
  trust-policy de desenvolvimento, marcadas como não-release.

Requisitos ao repo de treino:
1. Modo "sem locutor" estável (embedding dropout, alvo = silêncio com locutor trocado).
2. Enrollment denoisado como aumentação (p≈0,5) para evitar descompasso de treino.
3. ONNX de enrollment com saída `gamma_enc, beta_enc, gamma_df, beta_df`.
4. EQ neural com saída em ganhos dB por banda no contrato acima.
5. Contrato do modelo inalterado (960/480/32/96/5/2) e um script que exporta o DFNet3 com FiLM em identidade
   (para o spike da fase 3).
6. Ablações: enrollment cru vs. denoisado (TAR/TRR por SNR) e A/B de aplicação do EQ.
7. Licenças de dados compatíveis com a distribuição aberta dos pesos (decisão
   `decisoes/2026-09-24-hippocamp-open-source-gratuito.md` do vault).

## 8. Amostras de teste

Áudio nunca vai para o git. `scripts/fetch-voice-samples.sh` baixa o conjunto, confere SHA-256 (originais e derivados)
contra uma lista commitada e escreve em `fixtures/voice-samples/` (no `.gitignore`); `docs/testing/voice-samples.md`
traz o tutorial e a atribuição. Testes que usam áudio **pulam** quando os arquivos faltam (CI offline segue verde).
Conjunto inicial (pasta temporária; licenças lidas pelo agente, a conferir antes de virarem texto de tutorial):
VCTK p225 (4 falas, a `p225_011` de 6,8 s serve de enrollment) e p226 (2 falas) — CC BY 4.0, atribuição obrigatória
(CSTR VCTK Corpus v0.92, Univ. de Edimburgo); 3 clipes de ruído de 10 s do Wikimedia Commons — CC0. Os extratos usam
HTTP Range (o VCTK completo tem 11,7 GB); precisam de `ffmpeg`/`ffprobe`. Lacunas: tudo em inglês; DEMAND descartado
por divergência de licença na fonte. Dataset de podcast em português do Brasil: o usado no fine-tune do Parakeet é o TAGARELA
(`freds0/TAGARELA`), **CC BY-NC-SA 4.0**, derivado dos "Cem Mil Podcasts" (pesquisa não comercial, direitos dos
podcasters não esclarecidos), 16 kHz, já passado por vocoder-denoiser e sem speaker id garantido. O usuário informou
(2026-10-01) que o uso é de pesquisa e não comercial, o que permite **uso local para avaliação em pt-BR**: o usuário
confirmou que o TAGARELA **pode ser usado como dado de teste**: baixar um shard sob demanda, nunca commitar áudio, e só
por opção explícita no `fetch-voice-samples.sh` (flag `--with-tagarela`), fora do conjunto padrão. **Fica fora do treino de
pesos que serão distribuídos** até o dono do projeto registrar no vault uma decisão explícita, porque o NC-SA pode
exigir que os pesos herdem CC BY-NC-SA, em conflito com a política `MIT OR Apache-2.0` e com a decisão vigente de
app gratuito e open source (se pesos contam como adaptação é questão jurídica não resolvida). Tecnicamente também é
fraco: 16 kHz, já denoisado por vocoder, sem speaker id garantido. **Liberados pelo usuário (2026-10-01) para uso, inclusive treino**: Common Voice pt (CC0 segundo o relato
da pesquisa) e MLS Portuguese (CC BY 4.0 segundo o relato da pesquisa). A licença exata e a taxa de amostragem
(a suspeita é de que o MLS seja 16 kHz) são confirmadas na fonte oficial ao escrever o passo de download.

## 9. Ordem de entrega e execução

| Fase | Conteúdo | Depende de |
|---|---|---|
| 0 | Medir latência/CPU reais; atualizar `conductor/tech-stack.md`; script e tutorial de amostras | — |
| 1 | `studio-dsp` + `StudioBackend` nos dois caminhos; `Off` por padrão | 0 |
| 2 | `settings.json`, IPC de preset, UI de preset | 1 |
| 3 | Spike do fork do libDF com paridade (em paralelo com 1 e 2) | 0 |
| 4 | Registro de assets, backend personalizado, enrollment | 2 e 3 |
| 5 | Assets reais do repo de treino, novos goldens | 4 e repo de treino |

Cada fase só fecha com `cargo fmt --check`, `clippy -D warnings`, `cargo test --locked --offline` e
`scripts/check-offline.sh` passando, mais revisão de código. Implementação e revisão por agentes em **Sonnet e Haiku**
(Opus só em caso raro, por decisão do usuário em 2026-10-01). Execução autônoma: **sem commit e sem push** (não foram
pedidos); cada tarefa termina com um checkpoint (`git status --short`), e as fases paralelas trabalham em arquivos
disjuntos do mesmo working tree. O plano da fase 4 só é escrito depois do resultado do spike da fase 3.

## 10. Riscos e pendências

- **Spike da fase 3 executado (2026-10-02): GO** (relatório em
  `docs/superpowers/plans/2026-10-01-clearcore-studio-phase3-libdf-spike-report.md`). O fork do libDF com entradas
  `gamma/beta` tem paridade **bit-exata** com o upstream em identidade (inclusive com `gamma=1,5` mudando a saída),
  custo +0,1% (p50 656 µs, p99 0,74 ms contra o limite de 7 ms) e a cabeça de EQ no espectro antes do iSTFT é viável
  com lag 0. A hipótese do atraso do pulsifier não apareceu. Ressalvas para a fase 4: o patch tem +127/-21 linhas
  (não é "mínimo"); o fork **precisa ser vendorizado no repositório** (hoje a dependência é por `git` + `rev`, que
  exige rede) e contém `unsafe`, em conflito com `unsafe_code = "forbid"` do workspace, o que exige crate isolado com
  exceção explícita; a rota de constantes embutidas só vale para o spike, porque um ONNX por usuário não teria
  SHA-256 fixo no gate de assets. Obrigações da licença do fork: manter `LICENSE-MIT`/`LICENSE-APACHE` e o aviso de
  copyright, e marcar os arquivos modificados.
- **Lacuna do preset no Linux** (achada ao planejar a fase 2): o modo que governa o helper C vive em
  `$XDG_RUNTIME_DIR/clearcore_state` (struct C de 16 bytes, sem campo de preset). Para o preset chegar ao áudio no
  Linux é preciso estender a struct, o helper C e o Electron junto com a fase 1.
- A UI real do produto é **Electron** (o "app Tauri" é um CLI); os golden de IPC são JSON, não `.bin`.
- Licença do código do libDF e dos datasets de treino (impacta a distribuição dos pesos).
- Versionamento do IPC (v1 aditivo ou v2).
- Medir a latência fim a fim antes de gastar qualquer margem; hoje não há medição que sustente "folga".
- Fase 5 depende de entregáveis que não existem neste repositório (pesos treinados).
- Compatibilidade de `SetVoiceProfile`/perfil com gerações do engine (reinício) a definir na fase 4.
