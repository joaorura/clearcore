# Clearcore Studio — Fase 4 (Isolamento do Locutor pDFNet3 + FiLM, Equalização Neural e Contratos de Enrollment) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implementar o suporte a modelos personalizados no Clearcore Studio:
1. Vendorização do fork do `libDF` (`deep_filter_fork`) com o patch de condicionamento FiLM (+127/−21 linhas) e gancho de EQ espectral (+15/−1 linhas) em crate isolado fora do workspace para acomodar blocos `unsafe` do upstream sem violar `#![forbid(unsafe_code)]` do produto.
2. Evolução do gate de assets para um Registro de Descritores (`ModelAssetRegistry`) capaz de validar múltiplos modelos assinados (pDFNet3 com FiLM, ONNX de enrollment 16 kHz e Neural EQ).
3. Integração do condicionamento por locutor no `TractBackend` (`set_voice_profile` sem reinicialização de GRUs e troca sem jitter).
4. Aplicação de ganhos de Neural EQ no espectro antes do iSTFT com latência algorítmica zero (lag = 0 amostras).
5. Armazenamento seguro de perfis de voz (`0600`), teste de privacidade na exportação de diagnósticos, extensão do IPC e bateria de testes de paridade bit a bit.

**Architecture:**
- **Pipeline de Áudio em Tempo Real:**
  ```
  Entrada (48 kHz, hop 480) 
     │
     ▼
  ┌─────────────────────────────────────────────────────────────┐
  │ TractBackend (Personalizado via FiLM)                       │
  │   - Encoder (feat_erb, feat_spec, gamma_enc, beta_enc)     │
  │   - ERB Decoder (atenuação grosseira)                       │
  │   - DF Decoder (emb, c0, gamma_df, beta_df)                │
  │   - Gancho de EQ Espectral (32 bandas ERB -> 481 bins)     │
  │     [spec_enh *= 10^(gain_db / 20) antes do iSTFT]          │
  │   - iSTFT Synthesis (lag = 0 amostras extras)               │
  └─────────────────────────────────────────────────────────────┘
     │
     ▼
  ┌─────────────────────────────────────────────────────────────┐
  │ StudioBackend<TractBackend> (Fase 1 / DSP)                  │
  │   - High-pass 80 Hz -> EQ fixo -> De-esser                  │
  │   - Compressor -> AGC -> Limiter (-1 dBFS)                  │
  └─────────────────────────────────────────────────────────────┘
     │
     ▼
  Saída de Áudio (Virtual Mic Endpoint)
  ```
- **Pipeline de Enrollment (Fora do Caminho Crítico):**
  ```
  Gravação de Voz (6-10 s @ 16 kHz mono)
     │
     ▼
  Validação de Sinal (VAD, SNR via lsnr, consistência temporal)
     │
     ▼
  ONNX de Enrollment (Extração de Embeddings -> FiLM Projection)
     │
     ▼
  FiLMVectors (gamma_enc, beta_enc, gamma_df, beta_df [256]) + BandGains
     │
     ▼
  VoiceProfile (Hash SHA-256 canônico, salvo em disco modo 0600)
     │ [Áudio bruto original é descartado imediatamente da memória]
  ```

**Tech Stack:**
- Rust 1.90 (edition 2024, workspace lints `unsafe_code = "forbid"`, `unwrap/expect/panic = deny`, clippy pedantic+nursery com `-D warnings`).
- `tract-core` 0.19.16, `tract-onnx` 0.19.16, `tract-pulse` 0.19.16.
- `vendor/crates/deep_filter` (`deep_filter_fork` 0.5.6, código C e Rust com `unsafe` isolado fora dos membros do workspace).
- `ed25519-dalek` 2.2.0, `sha2` 0.10.9, `serde` 1.0.229, `serde_json` 1.0.145.
- IPC `realtime-noise.v1` aditivo (comando `SetVoiceProfile`, query de perfil em `GetStatus`).

**Spec & Documentos de Referência:**
- Spec de Design: `docs/superpowers/specs/2026-10-01-clearcore-studio-pipeline-design.md` (seções 1, 3, 6, 7, 9, 10).
- Relatório do Spike da Fase 3: `docs/superpowers/plans/2026-10-01-clearcore-studio-phase3-libdf-spike-report.md`.
- Handoff Operacional: `docs/superpowers/HANDOFF-2026-10-02-clearcore-studio.md`.

---

## Status de execução (2026-10-02)

Legenda: **Feita** = código e testes no working tree, verificados; **Feita (parcial)** = feita com ressalva descrita; **Bloqueada** = depende de algo que não existe aqui.

| Tarefa | Status | Evidência principal |
|---|---|---|
| 1. Vendorização do `deep_filter_fork` (FiLM + EQ) | **Feita** | `vendor/crates/deep_filter/src/tract.rs` (`FilmVectors`, `set_film`, `set_spectral_eq_*`, gancho de EQ), `crates/model/Cargo.toml:23`; paridade em `crates/model/tests/parity_film.rs` |
| 2. `ModelAssetRegistry` | **Feita (parcial)**: os 3 assets novos seguem **bloqueados** | `crates/model/src/model_registry.rs`: `verify_role`, recusa de descritor sem pin, allowlist exata. Os descritores `pdfnet3-release-asset-v1`, `voice-enrollment-asset-v1` e `neural-eq-asset-v1` não têm SHA-256/tamanho nem registros em `governance/model-assets/` porque os pesos vêm do repositório de treino (Fase 5) |
| 3. FiLM e EQ no `TractBackend` | **Feita (parcial)**: falta a fiação engine/IPC → backend | `crates/model/src/tract_backend.rs` (`set_voice_profile`, `active_voice_profile`, `set_spectral_eq`, `from_verified_asset`), `crates/model/src/spectral_eq.rs` |
| 4. Enrollment | **Feita (parcial)**: sem o ONNX real, validada com um ONNX sintético de contrato | `crates/model/src/enrollment.rs` |
| 5. Persistência 0600, IPC e privacidade | **Feita (parcial)**: o IPC guarda e reporta o perfil, mas **não o aplica ao áudio** | `crates/model/src/profile_store.rs`, `crates/ipc/src/protocol.rs`, `crates/service/src/lib.rs`, `crates/diagnostics/tests/privacy.rs` |
| 6. Paridade e gates | **Feita (parcial)**: `check-offline.sh` falha por crate de outro agente | `crates/model/tests/parity_film.rs`, `parity_eq.rs`, `dsp_pipeline_parity.rs` |

Pendências que bloqueiam o fechamento da fase:

1. **Fiação até o áudio.** `InferenceBackend` (trait) não tem `set_voice_profile`; `Engine`/`Supervisor` não o repassam; `StudioBackend` não o delega. Hoje o serviço persiste o perfil e `GetStatus` devolve `is_voice_profile_active: true`, mas nada condiciona o áudio. Entra na integração da Fase 1b.
2. **Assets reais** (pDFNet3, enrollment, EQ neural): pesos do repositório de treino, mais registros de governança e preenchimento de SHA-256/tamanho nos descritores.
3. **`./scripts/check-offline.sh`** falha porque `crates/workspace-policy` rejeita `crates/runtime-gpu-compute/Cargo.toml` ("inline tables cannot span multiple lines"), fora do escopo desta fase.
4. **Fixtures de 16 MB** em `fixtures/film/` (dois assets FiLM). Decidir se entram no git ou se o CI os regera com `tools/accelerators/gen_film_onnx.py`.

Decisões tomadas na execução (e o porquê):

- **Descritor sem pin falha fechado.** O código anterior pulava a conferência de SHA-256 e tamanho quando o descritor tinha `sha256` vazio; a assinatura era conferida contra registros que não eram ligados ao arquivo. Agora `verify_descriptor` recusa antes de qualquer E/S.
- **Allowlist vazia não aceita nada** (antes aceitava tudo), e `enc.onnx` / `tmp/export/enc.onnx` contam como o mesmo membro (duplicata).
- **`DspPipelineConfig::default()` desliga o post-filter** (era `0.02`): o `RuntimeParams::default()` do libDF não o liga, e o teste de paridade contra `DfTract::process` divergia em `4e-3`.
- **`DspPipeline` usava `feat_cplx_t`, que só divide a saída no lugar e não copia a entrada**: o modelo receberia um espectro todo zero. Trocado por `feat_cplx` (o caminho do `DfTract`).
- **`DspPipeline` falha fechado** se o modelo não devolver ganhos ERB ou coeficientes DF que o estágio exige (antes o espectro ruidoso passava sem tratamento).
- **EQ por interpolação em dB entre os centros das bandas** (plano, Step 3.2: "suavização"), não degrau por banda. EQ plano continua exatamente igual em todos os bins, e EQ neutro desliga o gancho (bit-exato com "sem EQ").
- **`set_voice_profile` aplica FiLM e o EQ do perfil juntos**; `set_spectral_eq` é um override direto que não altera o perfil ativo.
- **O arquivo do perfil nasce com 0600** (`OpenOptions::mode`), em vez de escrever e depois `chmod`, e a pasta com 0700.
- **Enrollment valida e rejeita, não corta (clamp)** vetores fora dos limites físicos. O áudio é zerado ao fim de `enroll` em qualquer caminho e no `Drop`. A inferência roda numa passada só (o contrato do ONNX real ainda não existe).
- **Referência upstream da paridade:** hash SHA-256 e amostras do `golden-pristine.f32` do relatório do spike (libDF sem patch). Se a CPU produzir bits diferentes, o teste cai para a tolerância do gate (`1e-4`), com aviso; nesta máquina o hash confere.

## Global Constraints

1. **Memória e Segurança de Código:**
   - O workspace Clearcore mantém estritamente `#![forbid(unsafe_code)]` em todas as crates do workspace (`crates/*`).
   - O código do fork do libDF contém blocos `unsafe` herdados do upstream (`Tensor::uninitialized_dt`, operações de buffer cru). Por isso, o fork deve ser **vendorizado como crate isolado** em `vendor/crates/deep_filter`, **fora** do array `members` de `Cargo.toml`.
   - `crates/model` consome `deep_filter_fork` via path dependency e mantém `#![forbid(unsafe_code)]`.
2. **Garantia de Silêncio Digital (Fail-Closed):**
   - Sob nenhuma hipótese áudio corrompido ou áudio bruto não processado pode vazar se a inferência falhar ou se os vetores FiLM forem rejeitados.
   - Qualquer vetor com `NaN`, `Inf` ou fora dos limites físicos é rejeitado antes de atingir o grafo neural.
3. **Zero Alocações no Callback de Tempo Real:**
   - A chamada `set_voice_profile` prepara tensores `FilmInputs` no carregador/supervisor. O método `process()` apenas consome referências clonadas de tensores `TValue` pré-alocados sem alocação dinâmica no heap de áudio.
4. **Fallback Suave sem Regressão:**
   - Se o asset pDFNet3 não estiver presente ou a assinatura for de desenvolvimento sem chave correspondente em produção, o sistema carrega o asset aprovado DFN3 v1 padrão (`df-compatible-release-asset-v1`). O produto nunca fica inoperante.
5. **Identidade Bit a Bit (Paridade Rigorosa):**
   - Quando condicionado com vetores neutros (`FiLMVectors::identity()`: `gamma = 1.0`, `beta = 0.0`) e equalização neutra (`BandGains::neutral()`: `0.0 dB`), a saída deve ser matematicamente idêntica (`max_abs_diff = 0.0`) ao DeepFilterNet3 upstream sem patch, reproduzindo os resultados GO do Spike da Fase 3.
6. **Privacidade e Proteção de Dados Biométricos:**
   - Embeddings de voz e perfis (`VoiceProfile`) são dados biométricos do usuário. Devem ser gravados com permissão estrita `0600` (POSIX) e **jamais** devem ser incluídos em pacotes de exportação de diagnósticos (`crates/diagnostics`).
   - O áudio bruto gravado na etapa de enrollment é descartado da memória RAM imediatamente após a extração dos vetores FiLM.
7. **Regras de Execução:**
   - NENHUM commit e NENHUM push.
   - Todo comando de compilação ou teste Rust DEVE carregar o ambiente:
     `source <scratchpad>/rust-env/env.sh && cargo ...` (script de ambiente Rust local, fora do repositório)

---

## Decisões de Arquitetura e Engenharia

### 1. Vendorização do `deep_filter_fork`
- **Problema:** O `libDF` upstream (`deep_filter`) não aceita entradas extras `gamma`/`beta` nos grafos ONNX e depende de repositório git externo no `crates/model/Cargo.toml` (`git = ...`, `rev = ...`), quebrando builds offline (`--offline`) e o script `check-offline.sh`. Além disso, possui `unsafe`.
- **Decisão:** Criar `vendor/crates/deep_filter` contendo o código fonte completo do `deep_filter` 0.5.6 (commit `978576aa8400552a4ce9730838c635aa30db5e61`), aplicando o patch validado no Spike da Fase 3:
  - Adição de `FilmVectors`, `FilmInputs` e métodos `film_hidden()`, `set_film()` em `src/tract.rs`.
  - Suporte a 4 entradas nos grafos `enc` (`feat_erb`, `feat_spec`, `gamma`, `beta`) e `df_dec` (`emb`, `c0`, `gamma`, `beta`).
  - Gancho de EQ no espectro complexo (`spec_enh`) imediatamente antes de `synthesis` (iSTFT) em `src/tract.rs:process`.
  - Inclusão dos cabeçalhos de licença `MIT / Apache-2.0` com nota explícita de modificação (Apache-2.0 seção 4(b)) e aviso de copyright de Hendrik Schröter mantido.
  - O crate `vendor/crates/deep_filter` NÃO é membro do workspace no root `Cargo.toml`. O `crates/model` referencia-o por:
    `deep_filter = { path = "../../vendor/crates/deep_filter", package = "deep_filter_fork", default-features = false, features = ["tract"] }`.

### 2. Registro de Descritores de Assets (`ModelAssetRegistry`)
- **Problema:** O `ApprovedAssetManifest` atual possui hashes fixos no código apenas para o asset legado `df-compatible-release-asset-v1`. A Fase 4 introduz pDFNet3, modelo de enrollment e EQ neural.
- **Decisão:** Substituir a validação pontual por uma estrutura modular `ModelAssetRegistry`:
  - `ModelRole`: `DenoisingBase`, `DenoisingPersonalized`, `SpeakerEnrollment`, `NeuralEq`.
  - `AssetDescriptor`: metadados estáticos do modelo (ID, hash SHA-256 esperado, tamanho exato, lista branca de membros tar.gz, dimensões esperadas).
  - Verificação criptográfica padronizada: cada asset é acompanhado de `candidate-provenance.json`, `legal-review.json`, `approval-manifest.json` e assinado pela chave Ed25519 autorizada em `trust-policy.json`.
  - Ambiente de desenvolvimento / CI aceita chaves de teste marcadas como `test-author-key` através de flag de configuração explícita sem comprometer a política restrita de produção.

### 3. Equalização Neural com Latência Zero (Pre-iSTFT ERB Hook)
- **Problema:** Um equalizador pós-iSTFT baseado em STFT próprio adicionaria 10 ms de latência algorítmica (480 amostras). Filtros IIR no domínio do tempo exigem biquads de alta ordem para aproximar curvas de 32 bandas.
- **Decisão:** Utilizar a descoberta comprovada no Spike da Fase 3 (Q6):
  - O DeepFilterNet3 já realiza a transformação STFT na entrada e o iSTFT na saída.
  - Entre o bloco de atenuação residual (`atten_lim`) e a chamada `state.synthesis()`, o espectro complexo realimentado `spec_enh [ch, 1, 1, n_freqs, 2]` está totalmente desdobrado na memória.
  - O contrato `BandGains` define 32 ganhos em dB (limitados a `[-6.0, +12.0]` dB).
  - O gancho interpola os ganhos das 32 bandas ERB para os 481 bins de frequência (`n_freqs = 481` para 48 kHz / N=960) e multiplica `spec_enh` diretamente.
  - **Latência algorítmica adicionada = 0 amostras.**

### 4. Gestão de `VoiceProfile` e Segurança
- **Problema:** Perfis de voz contêm representações biométricas do usuário. O vazamento acidental em arquivos de log ou exportações de suporte viola a privacidade do usuário.
- **Decisão:**
  - O tipo `VoiceProfile` armazena `FiLMVectors` (dimensão 256) e `BandGains` opcionais.
  - Possui `integrity_hash` SHA-256 canônico gerado a partir do payload.
  - Gravação atômica em disco via arquivo temporário irmão + `rename`.
  - Permissões POSIX estritas `0600` (`S_IRUSR | S_IWUSR`), recusando abertura se permissões de grupo/outros forem detectadas.
  - O crate `crates/diagnostics` possui teste explícito de guarda (`privacy.rs`) garantindo que diretórios de perfis de voz NUNCA sejam incluídos no zip de diagnósticos.

---

## Estrutura de Arquivos da Fase 4

| Arquivo | Ação | Responsabilidade |
|---|---|---|
| `vendor/crates/deep_filter/` | Criar | Código do `deep_filter_fork` 0.5.6 vendorizado offline com patch FiLM + EQ hook. |
| `vendor/crates/deep_filter/Cargo.toml` | Criar | Manifesto do fork renomeado para `deep_filter_fork`. |
| `vendor/crates/deep_filter/src/tract.rs` | Criar | Implementação do patch FiLM e gancho espectral. |
| `crates/model/Cargo.toml` | Modificar | Apontar `deep_filter` para `path = "../../vendor/crates/deep_filter"` em vez de `git = ...`. |
| `crates/model/src/voice_profile.rs` | Existente | Contratos `VoiceProfile`, `FiLMVectors`, `BandGains`, validação e serialização 0600. |
| `crates/model/src/model_registry.rs` | Criar | `ModelAssetRegistry`, descritores de múltiplos assets, verificação de assinaturas Ed25519. |
| `crates/model/src/tract_backend.rs` | Modificar | Suporte a carregamento de pDFNet3, bind dinâmico de `gamma`/`beta` e gancho de EQ. |
| `crates/model/src/spectral_eq.rs` | Criar | Mapeia os 32 ganhos ERB em 481 fatores por bin (interpolação em dB). |
| `crates/model/src/profile_store.rs` | Criar | Armazenamento do perfil ativo (0600/0700, atômico, fail-closed na leitura). |
| `crates/model/src/enrollment.rs` | Criar | Pipeline de enrollment: decodificação 16 kHz mono -> inferência de embedding -> `FiLMVectors`. |
| `crates/model/src/lib.rs` | Modificar | Expor `model_registry`, `voice_profile`, `enrollment`. |
| `crates/model/tests/parity_film.rs` | Criar | Bateria de testes de paridade bit a bit (Casos B, C, D, E, G do relatório do Spike). |
| `crates/model/tests/parity_eq.rs` | Criar | Testes de verificação do gancho de EQ (flat6, top12, latência 0). |
| `crates/model/tests/dsp_pipeline_parity.rs` | Criar | `DspPipeline` desacoplado contra `DfTract::process` (padrão, post-filter, limite de atenuação). |
| `fixtures/film/` | Criar | Assets FiLM de teste, golden upstream e README com SHA-256 e como regerar. |
| `tools/accelerators/gen_film_onnx.py`, `gen_enrollment_test_onnx.py` | Criar | Geradores dos fixtures (FiLM; ONNX sintético de contrato do enrollment). |
| `crates/ipc/src/protocol.rs` | Modificar | Comandos IPC `SetVoiceProfile`, `ClearVoiceProfile`, resposta com status do perfil. |
| `crates/diagnostics/tests/privacy.rs` | Modificar | Teste de guarda: garantia de exclusão de `VoiceProfile` e arquivos 0600 da telemetria. |

---

## Tarefas de Implementação

### Task 1: Vendorização do `deep_filter_fork` com Patches FiLM e EQ

**Files:**
- `vendor/crates/deep_filter/Cargo.toml`
- `vendor/crates/deep_filter/src/tract.rs`
- `vendor/crates/deep_filter/src/lib.rs`
- `vendor/crates/deep_filter/LICENSE-MIT`
- `vendor/crates/deep_filter/LICENSE-APACHE`
- `crates/model/Cargo.toml`

- [x] **Step 1.1: Criar a árvore vendorizada de `deep_filter_fork`**
  Copiar os arquivos do libDF upstream (commit `978576aa8400552a4ce9730838c635aa30db5e61`) para `vendor/crates/deep_filter`.
  Manter licenças `LICENSE-MIT` e `LICENSE-APACHE` com o aviso de copyright de Hendrik Schröter.

- [x] **Step 1.2: Aplicar o patch FiLM em `vendor/crates/deep_filter/src/tract.rs`**
  Incorporar a struct `FilmVectors`, a struct interna `FilmInputs`, os métodos `film_hidden(&self)` e `set_film(&mut self, v: &FilmVectors)`.
  Ajustar `init_encoder` e `init_df_decoder` para inspecionar `m.input_outlets()?.len() == 4` e configurar entradas de facts para `gamma` e `beta` com formato `[n_ch, S, film_hidden]`.

- [x] **Step 1.3: Aplicar o gancho de EQ Espectral em `vendor/crates/deep_filter/src/tract.rs`**
  Em `DfTract::process()`, entre o bloco `atten_lim` e a chamada `state.synthesis()`, introduzir a aplicação de ganhos espectrais:
  ```rust
  if let Some(eq_factors) = &self.spectral_eq_factors {
      // eq_factors: ArrayView1<f32> de 481 bins interpolados
      for mut ch_spec in spec_enh.axis_iter_mut(Axis(0)) {
          for (mut bin, &gain) in izip!(ch_spec.axis_iter_mut(Axis(2)), eq_factors.iter()) {
              bin.map_inplace(|c| *c *= gain);
          }
      }
  }
  ```

- [x] **Step 1.4: Atualizar `crates/model/Cargo.toml` para usar dependência local offline**
  Substituir a dependência `git` por:
  ```toml
  deep_filter = { path = "../../vendor/crates/deep_filter", package = "deep_filter_fork", default-features = false, features = ["tract"], optional = true }
  ```
  Verificar que `vendor/crates/deep_filter` NÃO está em `members` do workspace no root `Cargo.toml`.

- [x] **Step 1.5: Validar compilação isolada**
  Executar:
  `source scratchpad/rust-env/env.sh && cargo check -p realtime-noise-model --offline`
  Confirmar que compila sem erros de rede.

---

### Task 2: Registro de Descritores de Assets (`ModelAssetRegistry`)

**Files:**
- `crates/model/src/model_registry.rs`
- `crates/model/src/asset_manifest.rs`
- `crates/model/src/lib.rs`
- `governance/model-assets/`

- [x] **Step 2.1: Definir os tipos `ModelRole` e `AssetDescriptor`**
  Em `crates/model/src/model_registry.rs`:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
  pub enum ModelRole {
      DenoisingBase,
      DenoisingPersonalized,
      SpeakerEnrollment,
      NeuralEq,
  }

  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct AssetDescriptor {
      pub asset_id: String,
      pub role: ModelRole,
      pub sha256: String,
      pub size_bytes: u64,
      pub allowed_members: &'static [&'static str],
  }
  ```

- [x] **Step 2.2: Implementar catálogo padrão de descritores autorizados** _(catálogo cadastrado; os itens 2 a 4 estão sem SHA-256/tamanho: bloqueados pelos pesos)_
  Cadastrar:
  1. `df-compatible-release-asset-v1` (Base DFN3 legado, SHA-256 `c94d91f7...`, 7.983.136 bytes).
  2. `pdfnet3-release-asset-v1` (pDFNet3 com FiLM, membros: `enc.onnx`, `erb_dec.onnx`, `df_dec.onnx`, `config.ini`).
  3. `voice-enrollment-asset-v1` (Modelo de extração de embeddings 16 kHz, membro: `enrollment.onnx`).
  4. `neural-eq-asset-v1` (Modelo de equalização neural, membro: `neural_eq.onnx`).

- [x] **Step 2.3: Generalizar a validação de assinaturas Ed25519 e proveniência** _(feita; faltam os registros de governança dos 3 assets novos)_
  Evoluir o algoritmo de `verify` de `asset_manifest.rs` para receber qualquer `AssetDescriptor` e verificar:
  - `candidate-provenance.json` canônico.
  - `legal-review.json` canônico.
  - `approval-manifest.json` com assinatura válida Ed25519 pela chave do aprovador autorizada em `trust-policy.json`.
  - Integridade do arquivo tarball `.bin`.

- [x] **Step 2.4: Testes unitários do registro de assets**
  Escrever testes cobrindo:
  - Carregamento de asset base com sucesso.
  - Rejeição de asset com role incorreta.
  - Rejeição de arquivo com hash adulterado.
  - Rejeição de arquivo com membro fora da allowlist.

---

### Task 3: Suporte a FiLM e EQ Espectral no `TractBackend`

**Files:**
- `crates/model/src/tract_backend.rs`
- `crates/model/src/lib.rs`

- [x] **Step 3.1: Adicionar métodos de condicionamento ao `TractBackend`**
  ```rust
  impl TractBackend {
      pub fn set_voice_profile(&mut self, profile: Option<&VoiceProfile>) -> Result<(), InferenceError>;
      pub fn active_voice_profile(&self) -> Option<&VoiceProfile>;
      pub fn set_spectral_eq(&mut self, eq: Option<&BandGains>) -> Result<(), InferenceError>;
  }
  ```

- [x] **Step 3.2: Implementar interpolação ERB -> bins de FFT (481 bins)**
  Utilizar as larguras de banda ERB do modelo (32 bandas) para mapear o vetor de 32 ganhos lineares em um array contínuo de 481 fatores multiplicativos:
  - Suavização harmônica para evitar degraus entre bins adjacentes.
  - Modo neutro (`BandGains::neutral()`): array com todos os valores `1.0`.

- [x] **Step 3.3: Integrar `set_film` com `DfTract` sem reiniciar o estado das GRUs**
  Quando `set_voice_profile` for invocado:
  - Se o modelo subjacente declara entradas FiLM, converte os vetores para `FilmVectors` do libDF e chama `df.set_film(&film_vectors)`.
  - O estado temporal das GRUs (`df_states`, `rolling_spec_buf_y`, `rolling_spec_buf_x`) é preservado integralmente, garantindo transição sem clique audível.
  - Se o perfil for `None`, aplica `FiLMVectors::identity()`.

- [x] **Step 3.4: Tratamento para modelo sem FiLM (Base DFN3)**
  Se um `VoiceProfile` não neutro for carregado sobre um backend sem suporte a FiLM, retornar erro tipado `InferenceError::InputContract("backend model does not support speaker conditioning")`.

---

### Task 4: Pipeline e Contrato de Enrollment de Locutor

**Files:**
- `crates/model/src/enrollment.rs`
- `crates/model/src/voice_profile.rs`
- `crates/model/src/lib.rs`

- [x] **Step 4.1: Definir o contrato de entrada de áudio para enrollment**
  ```rust
  pub const ENROLLMENT_SAMPLE_RATE_HZ: u32 = 16_000;
  pub const ENROLLMENT_MIN_DURATION_SECS: f32 = 6.0;
  pub const ENROLLMENT_MAX_DURATION_SECS: f32 = 12.0;

  pub struct EnrollmentAudio<'a> {
      pub samples: &'a [f32],
      pub sample_rate: u32,
  }
  ```

- [x] **Step 4.2: Validação prévia de qualidade de áudio**
  Implementar função pura `validate_enrollment_audio`:
  - Duração entre 6 e 12 segundos (96.000 a 192.000 amostras a 16 kHz).
  - Ausência de clipping excessivo (pico <= 0.99).
  - Nível mínimo de energia (RMS > -40 dBFS) para evitar gravação em silêncio.
  - Fração de fala ativa (VAD simples baseado em energia por frame) >= 60%.

- [x] **Step 4.3: Extração de FiLMVectors pelo grafo ONNX de enrollment** _(feita contra um ONNX sintético de contrato; o asset real não existe)_
  Criar `SpeakerEnrollmentEngine`:
  - Carrega o asset assinado `voice-enrollment-asset-v1` via `tract-onnx`.
  - Executa inferência sobre as janelas de 16 kHz.
  - Extrai 4 tensores de dimensão 256: `gamma_enc`, `beta_enc`, `gamma_df`, `beta_df`.
  - Normaliza e valida os vetores contra `FiLMVectors::validate()`.

- [x] **Step 4.4: Descarte seguro de áudio da memória RAM**
  Garantir que os buffers de áudio PCM da gravação sejam sobrescritos com zeros ou descartados imediatamente após a geração do `VoiceProfile`.

---

### Task 5: Persistência Segura (0600), IPC e Privacidade

**Files:**
- `crates/ipc/src/protocol.rs`
- `crates/service/src/settings.rs`
- `crates/service/src/lib.rs`
- `crates/diagnostics/tests/privacy.rs`

- [x] **Step 5.1: Extensão do protocolo IPC v1 (Aditiva)**
  Adicionar comandos ao `IpcCommand`:
  - `SetVoiceProfile { profile_json: String }`
  - `ClearVoiceProfile`
  Em `GetStatusResponse`, adicionar campo opcional:
  - `active_voice_profile_id: Option<String>`
  - `is_voice_profile_active: bool`

- [x] **Step 5.2: Persistência segura em diretório dedicado** _(`ProfileStore` em `crates/model`; `crates/service/src/settings.rs` não existe)_
  Armazenar o perfil ativo em:
  `$XDG_DATA_HOME/clearcore/profiles/active_profile.json` (ou equivalente Windows/macOS).
  Garantir permissão `0600` na criação do arquivo e validação do modo na leitura.

- [x] **Step 5.3: Teste de guarda de privacidade em `crates/diagnostics`** _(adaptado: a API real é `DiagnosticExporter::export_json`/`export_archive`; achou e corrigiu vazamento em `sanitize_text`)_
  Em `crates/diagnostics/tests/privacy.rs`, criar teste automatizado:
  - Cria um perfil de teste com string sentinela `"BIOMETRIC_SENTINEL_TOKEN_SECRET"`.
  - Executa a rotina completa de exportação de diagnósticos (`DiagnosticCollector::export_archive`).
  - Descompacta o arquivo tar/zip gerado e verifica recursivamente que o token de sentinela NÃO aparece em nenhum arquivo de log ou dump JSON.

---

### Task 6: Bateria de Testes de Paridade Bit a Bit e Offline Gate

**Files:**
- `crates/model/tests/parity_film.rs`
- `crates/model/tests/parity_eq.rs`
- `scripts/check-offline.sh`

- [x] **Step 6.1: Teste de regressão Caso B (Fork com asset aprovado sem FiLM)**
  Executar 1000 frames de sinal sintético através de `deep_filter_fork` com asset padrão.
  Comparar contra o golden unconditioned do upstream:
  `assert_eq!(max_abs_diff, 0.0);` (Bit-exato).

- [x] **Step 6.2: Teste de paridade Caso C/D (Fork com FiLM em identidade)**
  Executar 1000 frames alimentando `FiLMVectors::identity()` (`gamma=1.0, beta=0.0`).
  Comparar contra o golden unconditioned:
  `assert_eq!(max_abs_diff, 0.0);` (Bit-exato).

- [x] **Step 6.3: Teste de sensibilidade Caso E (FiLM com valores não triviais)**
  Alimentar `gamma = 1.5, beta = 0.1`.
  Verificar que `max_abs_diff > 0.05` e que nenhum valor não-finito ou NaN é gerado.

- [x] **Step 6.4: Teste de latência zero do Neural EQ**
  Executar teste com impulsos e tons senoidais comparando `eq_none` vs `flat6` (+6 dB em todas as bandas):
  - Verificar que o ganho é exatamente +6.00 dB.
  - Verificar que o atraso de grupo medido é rigorosamente de 0 amostras (`lag = 0`).

- [x] **Step 6.5: Execução completa dos gates de validação offline** _(fmt, clippy e testes dos pacotes da fase passam; `check-offline.sh` bloqueado por `runtime-gpu-compute`)_
  Executar sequencialmente:
  1. `cargo fmt --all -- --check`
  2. `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings`
  3. `cargo test --workspace --locked --offline`
  4. `./scripts/check-offline.sh`

---

## Verificação e Critérios de Aceite

1. **Compilação e Lints:**
   - O workspace compila com zero avisos no Clippy (`cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings`).
   - `#![forbid(unsafe_code)]` permanece intocado em todas as crates do workspace principal.
2. **Determinismo e Paridade:**
   - Os testes de paridade comprovam que `FiLMVectors::identity()` produz exatamente a mesma saída que o modelo upstream DFN3 (`max_abs_diff = 0.0`).
3. **Privacidade Comprovada:**
   - O teste em `crates/diagnostics/tests/privacy.rs` comprova matematicamente que nenhum perfil ou vetor de voz vaza nas exportações de telemetria.
4. **Offline Completo:**
   - Nenhum download ou acesso à internet é realizado durante o build ou suíte de testes.
