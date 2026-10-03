# Contrato Clearcore para o repositório de treino (pDFNet3 + enrollment + EQ neural)

Pesquisa somente leitura em `/home/joaorura/orca/workspaces/clearcore/hippocamp` (branch `docs-using-superpowers-skill`), 2026-10-03.
Convenção: `arquivo:linha` são linhas 1-based conferidas com `awk`/`sed`. Os caminhos são relativos ao repo, salvo as notas do vault.
Rótulos: **[REG]** = está escrito em doc/código do repo ou do vault; **[INF]** = inferência minha a partir do código, não registrada.

---

## 1. Requisitos do repo de treino (spec seção 7) e o que Handoff/Plano dizem

### 1.1 Seção 7 do spec, literal (`docs/superpowers/specs/2026-10-01-clearcore-studio-pipeline-design.md:89-115`)

Requisitos ao repo de treino (linhas 106-115), copiados:

1. Modo "sem locutor" estável (embedding dropout, alvo = silêncio com locutor trocado).
2. Enrollment denoisado como aumentação (p≈0,5) para evitar descompasso de treino.
3. ONNX de enrollment com saída `gamma_enc, beta_enc, gamma_df, beta_df`.
4. EQ neural com saída em ganhos dB por banda no contrato acima.
5. Contrato do modelo inalterado (960/480/32/96/5/2) e um script que exporta o DFNet3 com FiLM em identidade (para o spike da fase 3).
6. Ablações: enrollment cru vs. denoisado (TAR/TRR por SNR) e A/B de aplicação do EQ.
7. Licenças de dados compatíveis com a distribuição aberta dos pesos (decisão `decisoes/2026-09-24-hippocamp-open-source-gratuito.md` do vault).

"Contrato acima" do EQ (spec:98-102): ganhos em dB por banda, limitados a [−6, +12] dB e suavizados; aplicados no tempo por filtro de fase mínima (sem overlap-add, sem latência extra). "O método de aplicação é detalhe de runtime e não exige retreino"; terceira variante a avaliar: cabeça de EQ dentro do pDFNet3, aplicada no espectro do libDF antes do iSTFT.

Outros itens da seção 7 que tocam o treino:
- Contexto: "Três assets novos (pDFNet3, enrollment, EQ neural); o DFNet3 v1 atual permanece aprovado e é o fallback." (spec:91).
- Assinaturas exigem a chave do usuário: "o agente **não** produz assinaturas" (spec:103-104).
- Enrollment no produto (spec:81-85): ONNX "áudio 16 kHz → gamma/beta do encoder e do DF decoder"; UI grava 6-10 s, passa pelo pDFNet3 em modo "sem locutor", valida, gera o perfil; "Se a ablação do repo de treino mostrar que o denoise do enrollment não ganha, a UI usa o áudio cru".

### 1.2 O que o Handoff e o plano da fase 4 dizem sobre a Fase 5

- Handoff, tabela de fases (`docs/superpowers/HANDOFF-2026-10-02-clearcore-studio.md:29`): "5 | assets reais (pDFNet3, enrollment, EQ) | **bloqueada**: depende de pesos do repo de treino, que não existe aqui".
- Spec, tabela de fases (spec:148): "5 | Assets reais do repo de treino, novos goldens | 4 e repo de treino"; spec:175 "Fase 5 depende de entregáveis que não existem neste repositório (pesos treinados)".
- Handoff, próximos passos, item 5 "Spec do repo de treino (separado)" (HANDOFF:210-214): repete os requisitos acima e acrescenta "EQ neural ... aplicados no domínio do tempo (sem overlap-add)".
- Plano da fase 4 (`docs/superpowers/plans/2026-10-02-clearcore-studio-phase4-enrollment-eq-plan.md`):
  - :77 os descritores `pdfnet3-release-asset-v1`, `voice-enrollment-asset-v1`, `neural-eq-asset-v1` "não têm SHA-256/tamanho nem registros em `governance/model-assets/` porque os pesos vêm do repositório de treino (Fase 5)".
  - :78 "Assets reais ... pesos do repositório de treino, mais registros de governança e preenchimento de SHA-256/tamanho nos descritores." (pendência 2, linha 86).
  - :79 Enrollment "validada com um ONNX sintético de contrato" (o ONNX real não existe).
  - :115 fallback: sem asset pDFNet3 (ou assinatura de dev sem chave de produção) carrega o DFN3 v1; "O produto nunca fica inoperante".
  - :117 (restrição global 5): com `FiLMVectors::identity()` e EQ neutro a saída deve ser idêntica ao DFNet3 upstream (`max_abs_diff = 0.0`) — isso vale para o **fixture derivado do DFNet3 v1**, não para o pDFNet3 treinado (ver seção 6).
  - :61/:88 fixtures de 16 MB em `fixtures/film/` ainda sem decisão sobre entrar no git.

### 1.3 Decisões em aberto (Handoff, seção 9, e spec seção 8/10)

- Handoff:235-236 (item 5): "Common Voice pt: licença **não confirmada** na fonte oficial (MLS Portuguese é CC BY 4.0 mas 16 kHz). TAGARELA (CC BY-NC-SA) liberado só como dado de teste; fora do treino de pesos distribuídos até decisão registrada."
- Handoff:234 (item 4): fork do libDF com `unsafe` (crate fora do workspace ou nova exceção); a fase 4 resolveu como crate vendorizado fora do workspace (plano:80, 110), decisão formal do dono ainda citada como pendente no vault.
- Spec:126-137 (dados):
  - TAGARELA (`freds0/TAGARELA`): CC BY-NC-SA 4.0, 16 kHz, "já passado por vocoder-denoiser e sem speaker id garantido"; usuário confirmou uso **só como dado de teste**, nunca commitar áudio, só via `--with-tagarela`; **fora do treino de pesos distribuídos** até o dono registrar decisão explícita no vault (NC-SA pode exigir que os pesos herdem CC BY-NC-SA, em conflito com `MIT OR Apache-2.0`).
  - "Liberados pelo usuário (2026-10-01) para uso, inclusive treino": Common Voice pt (CC0 "segundo o relato da pesquisa") e MLS Portuguese (CC BY 4.0).
  - VCTK p225/p226: CC BY 4.0 com atribuição (CSTR VCTK Corpus v0.92) e 3 ruídos Wikimedia CC0 — são **amostras de teste**, não de treino (spec:117-125); DEMAND descartado por divergência de licença na fonte.
- Taxa de amostragem dos datasets (`docs/testing/voice-samples.md:80-95`, conferido 2026-10-02 sem baixar):
  - MLS Portuguese: CC BY 4.0, **16 kHz** mono FLAC/Opus, confirmado na fonte (artigo arXiv 2012.03411 §3.1 e `ffprobe` de um FLAC: `flac,16000,1`).
  - Common Voice pt (Mozilla Data Collective, "Scripted Speech 23.0 - Portuguese"): CC0-1.0 "segundo indício indireto"; MP3; "16 kHz segundo relato secundário (originais a 48 kHz)"; **não confirmado na fonte oficial** (ficha renderizada no cliente, API deu 404).
  - Consequência registrada (voice-samples.md:92-95): o pipeline é 48 kHz fullband; os dois datasets chegam a 16 kHz, servem para fala/enrollment pt-BR, "não para validar banda acima de 8 kHz (de-esser, EQ de brilho)".
  - [INF] Para treinar o pDFNet3 (48 kHz, fullband) com fala pt-BR 16 kHz seria preciso upsample ou usar só como alvo/limitado em banda; o repo não decide isso. O enrollment é 16 kHz, então os datasets 16 kHz casam melhor com ele do que com o denoiser.
  - VCTK (48 kHz originalmente) é CC BY 4.0 mas está catalogado como amostra de teste; o spec/decisão não o lista entre os "liberados para treino" (spec:135-137). [INF] Se o repo de treino quiser usar VCTK, tratar como decisão a registrar.
- Handoff:219-231 etc. (HELPER_BIN, seletor de acelerador, CI) não afetam o repo de treino.

---

## 2. Contrato do modelo DFNet3 (denoiser)

### 2.1 Parâmetros fixos (devem ficar inalterados)

Validados em `crates/model/src/tract_backend.rs:358-374` (`validate_model_contract`); qualquer divergência → `InferenceError::InputContract("approved model does not match the frozen DSP contract")`:

| Parâmetro | Valor exigido | Fonte |
|---|---|---|
| sr | 48000 (`SAMPLE_RATE_HZ`) | tract_backend.rs:360; `crates/contracts/src/audio.rs:3` |
| canais | 1 (`CHANNELS`) | tract_backend.rs:361; audio.rs:4 |
| hop | 480 (`HOP_SAMPLES`; `AudioFrame = [f32; 480]`) | tract_backend.rs:362; audio.rs:5,7 |
| fft_size | 960 (n_freqs = 481) | tract_backend.rs:363; tract.rs:337 |
| nb_erb | 32 | tract_backend.rs:364 |
| nb_df | 96 | tract_backend.rs:365 |
| df_order | 5 | tract_backend.rs:366 |
| df_lookahead | 2 | tract_backend.rs:367 |
| conv_lookahead | 2 | tract_backend.rs:368 |
| latência algorítmica | (fft−hop) + lookahead·hop = 480+2·480 = **1440** amostras (`ALGORITHM_LATENCY_SAMPLES`) | tract_backend.rs:359,369; `crates/model/src/lib.rs:44` |
| FiLM hidden (se o grafo declara FiLM) | 256 (`FILM_HIDDEN_DIM`) | tract_backend.rs:375-382; `voice_profile.rs:9` |

`lookahead` do runtime = `max(conv_lookahead, df_lookahead)` quando `[train] model = deepfilternet3` (tract.rs:364-371). Qualquer outro `model` é recusado.

### 2.2 `config.ini` exigido (chaves lidas com `unwrap()` — ausência = panic/erro de carga)

Referência: `models/stateful/config.ini` (cópia byte a byte do config do asset aprovado, SHA-256 `415eb925...4290`, 2067 B, `models/stateful/README.md:21`). Chaves que o runtime lê (vendor/crates/deep_filter/src/tract.rs:286-343, 364):
- `[df]`: `sr=48000`, `fft_size=960`, `hop_size=480`, `nb_erb=32`, `nb_df=96`, `min_nb_erb_freqs=2`, `df_order=5` (ou em `[deepfilternet]`), `df_lookahead=2`, `norm_tau=1` (ou `norm_alpha`). Também presentes no config de referência: `lsnr_max=35`, `lsnr_min=-15`, `pad_mode=output`.
- `[deepfilternet]`: `conv_lookahead=2`, `conv_ch=64`, `emb_hidden_dim=256`, `df_hidden_dim=256`, `emb_num_layers=3`, `df_num_layers=2`, etc.
- `[train] model = deepfilternet3`.
- `emb_hidden_dim` define a dimensão FiLM: `film_hidden = emb_hidden_dim` (tract.rs:289), usada para **os dois** grafos (enc e df_dec, tract.rs:291,304); [INF] portanto `df_hidden_dim` também deve ser 256 (o FiLM do df_dec é montado com `emb_hidden_dim`, tract.rs:1119).
- Bandas ERB (larguras fixas, soma 481): `[2×13, 5, 5, 7, 7, 8, 10, 12, 13, 15, 18, 20, 24, 28, 31, 37, 42, 50, 56, 67]` (`crates/model/tests/parity_eq.rs:26-31`); derivam de sr/fft/nb_erb/min_nb_erb_freqs.
- O arquivo é lido como entrada de tar que **termina em** `config.ini` (tract.rs:58).

### 2.3 Grafos ONNX (nomes, formas, ordem)

O runtime (tract 0.19.16, pulsado em `S`=1) **renomeia as entradas por posição** (`with_input_names`), logo a **ordem** das entradas é o contrato; os nomes abaixo são os que o runtime impõe e que o ONNX base usa.

Dimensões de referência: `ch=1`, `nb_erb=32`, `nb_df=96`, `conv_ch=64`, `n_hidden = conv_ch·nb_erb/4 = 512` (tract.rs:988, 1103).

**enc.onnx** (tract.rs:916-955)
- Entradas: `feat_erb [1,1,S,32]`, `feat_spec [1,2,S,96]` (+ `gamma [1,S,256]`, `beta [1,S,256]` se FiLM).
- Saídas, em ordem (tract.rs:945): `e0, e1, e2, e3, emb, c0, lsnr`. Consumo em `process_raw` (tract.rs:589-593): `lsnr` escalar f32; `c0`; `emb`; depois `e3,e2,e1,e0`.
- Formas das saídas que alimentam o decoder (derivadas das entradas do erb_dec, tract.rs:990-999): `emb [1,S,512]`, `e3 [1,64,S,8]`, `e2 [1,64,S,8]`, `e1 [1,64,S,16]`, `e0 [1,64,S,32]`; `c0 [1,64,S,96]` (tract.rs:1106-1109).

**erb_dec.onnx** (tract.rs:976-1068)
- Entradas, em ordem: `emb [1,S,512]`, `e3`, `e2`, `e1`, `e0` (nomes `emb, e3, e2, e1, e0`; tract.rs:1010-1016, chamada em 604-612).
- Saída: máscara `m` (ganhos ERB, [1,1,S,32] após `remove_axis` ×2 → `[ch, nb_erb]`, tract.rs:612-615). Sem entradas FiLM.

**df_dec.onnx** (tract.rs:1090-1136)
- Entradas: `emb [1,S,512]`, `c0 [1,64,S,96]` (+ `gamma [1,S,256]`, `beta [1,S,256]` se FiLM).
- Saída `coefs` com reshape final `[ch, nb_df=96, df_order=5, 2]` (tract.rs:629-630).

**Detecção de FiLM:** o runtime considera que um grafo tem FiLM **se e somente se tem exatamente 4 entradas** (`input_outlets().len() == 4`, tract.rs:306, 311, 935, 1116). Enc e df_dec são detectados independentemente. `gamma`/`beta` são as entradas de índice 2 e 3, nessa ordem. Em `process`, são anexadas depois das entradas normais (tract.rs:585-588, 625-628).

**Stateful:** o ONNX do runtime CPU (tract) é o export "streaming-agnóstico" do upstream (GRUs com estado interno do tract pulsado). Os grafos `models/stateful/*` (h_in/h_out, `feat_erb_buf`, etc.) são **outro** formato, só para backends de aceleração (OpenVINO), **não passam pelo gate** (`models/stateful/README.md:8-14`); o repo de treino **não** precisa entregá-los para o Clearcore atual. Se quiser aceleração futura, eles saem de `tools/accelerators/make_stateful_onnx.py` (cirurgia de grafo).

### 2.4 Pós-processamento fixo no runtime (não é treinável aqui)

`RuntimeParams::default()` (post-filter desligado no `DspPipeline` default, plano:68; atenuação limite padrão). O produto chama `DfTract::new(parameters, &RuntimeParams::default())` (tract_backend.rs:339).

---

## 3. Contrato FiLM

### 3.1 Onde entram no grafo

- API do runtime: `FilmVectors { gamma_enc, beta_enc, gamma_df, beta_df: Vec<f32> }` (vendor/crates/deep_filter/src/tract.rs:198-206). Cada vetor tem `ch · film_hidden` = **1 × 256 = 256** elementos (tract.rs:199, 216-223; `film_hidden() = emb_hidden_dim`, tract.rs:459-462).
- Tensor entregue ao grafo: `[ch, 1, hidden]` = `[1,1,256]` (tract.rs:210-211, 225-226); o grafo declara `[ch, S, hidden]` com S simbólico (tract.rs:938, 1119). **O mesmo vetor é reenviado em todo quadro** (clone do tensor, tract.rs:586-587, 626-627): FiLM é **constante no tempo** durante a inferência (um vetor por utterance/perfil, não por quadro). [INF] O treino deve condicionar com gamma/beta estáticos por locutor, broadcast no tempo.
- `set_film(&FilmVectors)` troca os vetores sem reiniciar as GRUs (tract.rs:464-478); recusa se o grafo não declara FiLM ou se o tamanho ≠ `ch·hidden`.
- Quem converte: `tract_backend.rs:226-` (`to_film_vectors`) e `condition` (tract_backend.rs:320-329). `TractBackend::set_voice_profile` (tract_backend.rs:150-190): sem perfil → aplica `FiLMVectors::identity()`; perfil não-neutro em modelo sem FiLM → `InferenceError::InputContract("backend model does not support speaker conditioning")` (plano:318-319).

### 3.2 Semântica de identidade

- `gamma = 1.0`, `beta = 0.0` em todos os 256 canais de enc e df_dec (`voice_profile.rs:110-132`, `FiLMVectors::identity`). Na semântica do ONNX atual: `x' = x * gamma + beta` (gen_film_onnx.py:6, 42-43). Identidade é bit-exata no ORT e no tract para o modelo editado (gen_film_onnx.py:104-110; spike report seção 4; `parity_film.rs` casos C/D).
- `FiLMVectors::is_identity()` usa tolerância `f32::EPSILON` (voice_profile.rs:135-145).
- [INF] O pDFNet3 **treinado** deve tratar `gamma=1, beta=0` como o operador "sem locutor" (é o que o runtime aplica quando não há perfil), coerente com o requisito 1 do spec (embedding dropout). Isto não está escrito como requisito literal no repo, é consequência de tract_backend.rs:156-175.

### 3.3 Como `gen_film_onnx.py` insere as entradas (`tools/accelerators/gen_film_onnx.py`)

- Sites (gen_film_onnx.py:19-22): saída do `Relu` que alimenta `Transpose → GRU`:
  - enc: `/emb_gru/linear_in/1/Relu_output_0`
  - df_dec: `/df_gru/linear_in/linear_in.1/Relu_output_0`
- Constrói duas entradas `gamma`, `beta` (`FLOAT`, forma `[1, "S", 256]`, :35), insere `Mul(site, gamma)` → `Add(…, beta)` após o produtor e religa todos os consumidores para a saída filmada (:36-43). Exige exatamente 1 produtor do site (:31-33).
- Gera `enc_film.onnx`, `df_dec_film.onnx`, e empacota `film_identity_asset.tar.gz` com `tmp/export/{enc,erb_dec,df_dec}.onnx` + `config.ini` (pack(), :77-87: modo 0644, mtime 0, gzip mtime 0). Também gera a versão "baked" (gamma=1.5, beta=0.1 como initializers, :48-57) para o caso G.
- Hidden fixo `HIDDEN = 256` (:23); embedding de teste usa `emb [1,S,512]`, `c0 [1,64,S,96]`, `feat_erb [1,1,S,32]`, `feat_spec [1,2,S,96]` (:60-66).
- Risco registrado (spike report:747): o ponto `Relu` pós-exportação "pode não existir no modelo treinado de verdade"; mitigação = requisito 5 (o repo de treino exporta o DFNet3 com FiLM em identidade). Se o site de FiLM mudar, "recontar nós `Delay`" do grafo pulsado (spike report:741). O runtime **não** exige um site específico: exige 4 entradas (com `gamma`/`beta` `[ch,S,256]` nas posições 2,3) e comportamento correto pulsado em S=1.
- O ONNX exportado deve ser carregável pelo **tract 0.19.16** (pinado `=0.19.16`, `crates/model/Cargo.toml:34-36`) com `with_ignore_output_shapes(true)` e `PulsedModel::new(&m, S, 1)` (tract.rs:962, 951). [INF] Evitar ops que o tract 0.19.16 não suporta/pulsa.

### 3.4 Limites físicos validados (`voice_profile.rs:14-17, 164-200`)

Qualquer vetor FiLM é **rejeitado** (nunca "clampado") antes de atingir o grafo se: tamanho ≠ 256, NaN/Inf, ou fora de
- `gamma ∈ [0.001, 100.0]`
- `beta ∈ [−50.0, 50.0]`
(`gamma_enc, gamma_df` usam o intervalo de gamma; `beta_enc, beta_df` o de beta.) O treino deve garantir que o ONNX de enrollment nunca emita fora disso (enrollment.rs:298-299 rejeita via `FiLMVectors::new`).

---

## 4. Contrato de enrollment e `VoiceProfile`

### 4.1 Áudio de entrada (`crates/model/src/enrollment.rs`)

- Taxa: **16 000 Hz** mono (`ENROLLMENT_SAMPLE_RATE_HZ`, :22); outras taxas → `UnsupportedSampleRate` (:147-151).
- Duração: **6,0 s a 12,0 s** inclusive = 96 000 a 192 000 amostras (`ENROLLMENT_MIN/MAX_SAMPLES`, :23-28, 153-162). Observação: o spec fala em gravar "6-10 s" (spec:83) e o plano em 6-12 s (plano:344); o código aceita 6-12 s.
- Validações (puras, antes do modelo): sem NaN/Inf (:163); pico ≤ 0,99 (`ENROLLMENT_MAX_PEAK`, :30, 167); RMS > −40 dBFS (:32, 171); VAD de energia por quadros de 20 ms (320 amostras): quadro é fala se ≥ max(−50 dBFS, pico−30 dB) (:36-41, 175-187); fração ativa ≥ 0,60 (:34, 188). SNR via `lsnr` e consistência por janela de ~2 s do spec **não** estão implementadas aqui (comentário :142-143).
- O áudio é zerado após `enroll` em qualquer caminho (:289-297) e no `Drop` (:238-242).

### 4.2 ONNX de enrollment (`OnnxEnrollmentModel`, enrollment.rs:343-465)

- Membro do tar: `enrollment.onnx` (`MEMBER`, :357; procura entrada cujo path **termina** em `enrollment.onnx`, :387-390).
- Papel exigido: `ModelRole::SpeakerEnrollment` (:373).
- **Entrada única** (posição 0; o nome documentado é `audio`): `f32`, forma `[1, N]`, 16 kHz mono (docs :362-363; `set_input_fact(0, f32 [1, N])` com N = nº de amostras reais da gravação, :426-430). O código usa o índice 0, não o nome. O fixture sintético declara `audio [1,"N"]` (gen_enrollment_test_onnx.py:43), então N deve poder variar entre 96 000 e 192 000.
- **Quatro saídas** com esses nomes exatos (`OUTPUTS`, :358; encontradas por `outlet_label`, :440-456): `gamma_enc`, `beta_enc`, `gamma_df`, `beta_df`; tipo `f32`; "256 valores cada, qualquer forma que achate para 256" (:363-364). A leitura é por `as_slice::<f32>` (:452) e o tamanho final é checado = 256 (:323-337).
- O modelo é reconstruído e otimizado **a cada** `extract` (:418-435): custo de carga fora do caminho crítico.
- [INF] Tem de ser carregável por tract 0.19.16 sem recorrer a ops que ele não suporta (o repo de treino deve incluir um teste de carga com tract).
- O ONNX sintético de teste (`crates/model/tests/fixtures/enrollment-contract-test.onnx`; `tools/accelerators/gen_enrollment_test_onnx.py`) define o contrato de I/O: saídas `[256]` float32; opset 13, ir_version 8 (gen_enrollment_test_onnx.py:48-49).

### 4.3 `VoiceProfile` (`crates/model/src/voice_profile.rs`)

- `VoiceProfile { version: u32 (=1, `PROFILE_SCHEMA_VERSION`), id, name, created_at_utc, film: FiLMVectors, eq: Option<BandGains>, integrity_hash }` (:296-306). `integrity_hash` = SHA-256 hex do JSON canônico (serde_json de `{version,id,name,created_at_utc,film,eq}`) (:308-316, 366-381); `verify_integrity` valida versão, limites e hash (:384-399).
- `FiLMVectors { gamma_enc, beta_enc, gamma_df, beta_df: Vec<f32> }`, cada um com **256** (`FILM_HIDDEN_DIM`, :9, 164-201). Limites: ver 3.4.
- `BandGains { gains_db: [f32; 32] }` (`NUM_ERB_BANDS = 32`, :10, 203-294). Limites físicos: **[−6,0 dB, +12,0 dB]** (`MIN/MAX_EQ_GAIN_DB`, :11-12); NaN/Inf rejeitados (:275-293). `BandGains::clamped` existe (:244-263) mas `from_array/from_slice/new` validam e rejeitam. `neutral()` = 0 dB nas 32 bandas.
- Persistência: arquivo 0600, pasta 0700, fora do zip de diagnósticos (plano:119, 134-137; `crates/model/src/profile_store.rs`).

### 4.4 EQ neural (asset `neural-eq-asset-v1`)

- No repo existe **só o descritor** (membro `neural_eq.onnx`, papel `NeuralEq`; model_registry.rs:218-224). **Nenhum código carrega ou define E/S** de `neural_eq.onnx` (busca por `neural_eq`/`NeuralEq` só acha o registro e testes de papel). O contrato de E/S do modelo de EQ **não está definido no repo**: só a saída semântica (ganhos dB por banda em [−6,+12], 32 bandas ERB, suavizados; spec:98-102; plano:123-129; `BandGains`).
- O destino dos ganhos no runtime: `bin_factors` interpola **em dB entre centros de banda** as 32 bandas para 481 bins (`crates/model/src/spectral_eq.rs:1-60`), e `DfTract::set_spectral_eq_factors` multiplica o espectro realçado logo antes do iSTFT, lag 0 (vendor tract.rs:480-494, 768; `parity_eq.rs`). Nota de handoff: o spec original dizia filtro de fase mínima no tempo; a fase 4 implementou o gancho espectral.
- [INF/REG] Se a variante "cabeça de EQ dentro do pDFNet3" for escolhida, "o treino precisa definir em qual atraso o `emb` condiciona o ganho" (spike report:732, 754): o espectro do gancho corresponde ao quadro atrasado por `lookahead`.

---

## 5. Governança de assets e o que o repo de treino precisa entregar

### 5.1 Estrutura de um registro existente (`governance/model-assets/df-compatible-release-asset-v1/`)

Quatro arquivos (cópia do padrão a replicar por `asset_id`):
- `candidate-provenance.json` — campos **exatos** (m0_records.rs:5-13): `asset_id, code_license, conversion_terms, sha256, source, status, weight_license`. `status="APPROVED"`; `source` = `urn:sha256:<sha256 do arquivo>` (m0_records.rs:108-110); licenças atuais `"MIT OR Apache-2.0"`.
- `legal-review.json` — campos exatos (m0_records.rs:14-23): `code_license, conversion_terms, legal_approval_id, redistribution_terms, review_date, reviewer_identity, status, weight_license`; datas `YYYY-MM-DD`.
- `approval-manifest.json` — campos exatos (m0_records.rs:24-38): `approval_date, approved_for_release (true), approver_identity, asset_id, candidate_record_sha256, candidate_sha256, conversion_terms, key_id, legal_approval_id, legal_review_record_sha256, redistribution_terms, signature, status`.
- `approver-public-key.pem` — Ed25519 SPKI (44 bytes DER; model_registry.rs:495-511).
- Modelos em branco: `governance/model-assets/templates/*.template.json`; schemas em `governance/model-assets/schemas/*.schema.json` (o schema do approval fixa `asset_id` const `df-compatible-release-asset-v1` — [INF] precisará ser generalizado para os novos ids; o gate Rust não usa o schema, usa `m0_records.rs`).
- Igualdades cruzadas exigidas (m0_records.rs:111-130; model_registry.rs:330-339): `asset_id` igual nos três; `candidate.sha256 == approval.candidate_sha256 == descriptor.sha256`; `legal_approval_id` igual; `conversion_terms` e licenças coerentes; `redistribution_terms` igual (legal↔approval); `approval.candidate_record_sha256 == SHA-256(canônico(candidate))` e `approval.legal_review_record_sha256 == SHA-256(canônico(legal))`.
- Canônico = JSON compacto com **chaves ordenadas**, UTF-8 (`scripts/verify-m0-asset.py:114-115`: `sort_keys=True, separators=(",",":"), ensure_ascii=False`; Rust: `serde_json::to_vec(&Value)`, model_registry.rs:465-470).
- `conversion_terms` atual do base: "Redistributed byte-for-byte ... no model conversion or quantization has occurred." Para o pDFNet3 (fine-tune/derivado) o texto precisa ser novo (ver `models/stateful/README.md:90-95` para o precedente de itens exigidos numa conversão: ferramenta, versão, SHA-256 de origem).
- `governance/model-assets/trust-policy.json`: `{"authorized_key_ids": ["sha256:cfa7e10c021031f5481775cf32093d41e5aaaad454653f3ed0022769cbbfd3f2"]}` — **um** campo (model_registry.rs:341-346 recusa se `len() != 1` ou sem `authorized_key_ids`). `key_id` = `"sha256:" + SHA-256(DER SPKI)` da chave pública do registro (:348-352) e precisa estar na lista (:354-356). Uma chave nova ⇒ editar a trust-policy (decisão do dono).

### 5.2 Registro de descritores (`crates/model/src/model_registry.rs:189-224`)

| asset_id | Papel | allowlist de membros | SHA-256/tamanho hoje |
|---|---|---|---|
| `df-compatible-release-asset-v1` | `DenoisingBase` | `tmp/export/{enc,erb_dec,df_dec}.onnx`, `tmp/export/config.ini` | pinado (`c94d91f7...`, 7 983 136 B) |
| `pdfnet3-release-asset-v1` | `DenoisingPersonalized` | `enc.onnx`, `erb_dec.onnx`, `df_dec.onnx`, `config.ini` | **vazio / 0** (bloqueado) |
| `voice-enrollment-asset-v1` | `SpeakerEnrollment` | `enrollment.onnx` | **vazio / 0** |
| `neural-eq-asset-v1` | `NeuralEq` | `neural_eq.onnx` | **vazio / 0** |

O que o gate verifica, na ordem (`verify_descriptor`, model_registry.rs:307-406):
1. Descritor fixado: `sha256.len()==64 && size_bytes != 0`, senão falha **fechado** "not pinned" (:312-319; teste :644-658). **Os SHA-256/tamanho são constantes em código Rust** — preencher exige edição do `model_registry.rs` + build (dono/Clearcore).
2. Lê `candidate-provenance.json`, `legal-review.json`, `approval-manifest.json`, `approver-public-key.pem` em `governance/model-assets/<asset_id>/` e `trust-policy.json` (:320-326); arquivos devem ser regulares, não symlink, não mudar durante a leitura (:480-493).
3. Valida campos exatos/cruzados (m0_records), chave autorizada, e a **assinatura Ed25519** sobre `JSON canônico(approval sem "signature")` em base64 (64 bytes) (:358-378).
4. Lê `vendor/approved/<asset_id>.bin` (:380-384): **o tar.gz é gravado com extensão `.bin`** com o nome do `asset_id`. Confere tamanho exato e SHA-256 do arquivo inteiro (:386-393).
5. `validate_archive_members` (:415-459): gzip+tar; todo membro deve ser **arquivo regular** (diretórios, symlinks, hardlinks → recusa), sem bit executável no `mode` (`mode & 0o111 == 0`), caminho UTF-8, sem duplicatas, só nomes da allowlist, e **todos** presentes. O prefixo `tmp/export/` é aceito/ignorado, ou seja `enc.onnx` e `tmp/export/enc.onnx` valem como o mesmo membro (:409-411, 425, 441).
6. `verify_role` exige o papel certo (:288-305); `TractBackend::from_verified_asset` só aceita `DenoisingBase`/`DenoisingPersonalized` (tract_backend.rs:86-95); `OnnxEnrollmentModel::from_verified_asset` só `SpeakerEnrollment` (enrollment.rs:373).
7. Chaves de desenvolvimento (`test-author-key`, `DEV_KEY_ID`) só valem com `allow_dev_keys`, que **não** é habilitável fora de `cfg(test)` (:19, 242-259).

Compatibilidade extra de leitura (DfParams): as entradas `enc.onnx`, `erb_dec.onnx`, `df_dec.onnx`, `config.ini` são casadas por `path.ends_with(...)` (vendor tract.rs:52-58) — nome com ou sem `tmp/export/` funciona; nomes extras geram só warning no libDF, mas o gate **recusa** membros fora da allowlist. O `gen_film_onnx.py` empacota com prefixo `tmp/export/`; a allowlist do pdfnet3 é plana, mas o prefixo é tolerado. Há também o validador legado `crates/model/src/archive.rs:9-30`, que só vale para o asset base (4 membros com tamanho/hash fixos).

### 5.3 Entregáveis exatos do repo de treino

Por asset (`pdfnet3-release-asset-v1`, `voice-enrollment-asset-v1`, `neural-eq-asset-v1`):

A) **pDFNet3** — um `tar.gz` (gzip de tar) com exatamente 4 membros regulares, modo 0644, sem diretórios:
   - `enc.onnx` (4 entradas `feat_erb, feat_spec, gamma, beta`; saídas `e0,e1,e2,e3,emb,c0,lsnr`),
   - `erb_dec.onnx` (5 entradas `emb,e3,e2,e1,e0`; saída máscara),
   - `df_dec.onnx` (4 entradas `emb, c0, gamma, beta`; saída `coefs`),
   - `config.ini` com `model = deepfilternet3`, `sr=48000, fft_size=960, hop_size=480, nb_erb=32, nb_df=96, df_order=5, df_lookahead=2, conv_lookahead=2, emb_hidden_dim=256, df_hidden_dim=256, conv_ch=64, …`.
   Para reprodutibilidade recomendável (como `gen_film_onnx.py:pack`): mtime 0, ordem fixa de membros, gzip mtime 0, versão de `onnx`/`torch` registrada (o README de `models/stateful` registra que a versão do `onnx` não ficou registrada e a saída não é byte-reproduzível, `models/stateful/README.md:62-66`).
B) **Enrollment** — `tar.gz` com 1 membro `enrollment.onnx` (entrada `audio [1,N]` f32 16 kHz, 4 saídas f32 de 256 valores nomeadas `gamma_enc, beta_enc, gamma_df, beta_df`, dentro de gamma∈[0.001,100], beta∈[−50,50]).
C) **EQ neural** — `tar.gz` com 1 membro `neural_eq.onnx`; E/S **a definir** (ver 4.4). Hoje nada o consome.
D) Informações para os registros de governança (o dono monta/assina): SHA-256 (hex minúsculo, 64) e tamanho em bytes de cada `.tar.gz`; origem/proveniência (dataset, commit do código de treino, hiperparâmetros, seeds, versões de torch/onnx); licença dos pesos e do código; `conversion_terms` descrevendo treino/fine-tune a partir do DFNet3 v0.5.6 (base `c94d91f7...`); lista de datasets usados com licenças.
E) Relatórios de ablação (req. 6): enrollment cru vs. denoisado (TAR/TRR por SNR) e A/B do EQ.
F) Script que exporta o DFNet3 com FiLM em identidade (req. 5) e as saídas de referência para o teste de paridade.

**Só o dono pode fazer** (o agente nunca produz assinaturas; spec:103-104, plano:209, vault "Assets neurais"):
- Gerar/guardar a chave privada Ed25519 (a atual é `sha256:cfa7e10c...`, privada fora do repo, 0600) e **assinar** o `approval-manifest.json` de cada asset (:358-378).
- Preencher `legal-review.json` e `approval-manifest.json` (identidade de revisor/aprovador, `legal_approval_id`, termos de redistribuição) — decisão do proprietário, "não parecer jurídico externo" (vault 2026-09-24).
- Decidir licença dos pesos (`MIT OR Apache-2.0`) e a **decisão explícita sobre TAGARELA** se quiser usá-lo no treino (spec:131-135).
- Editar `crates/model/src/model_registry.rs` com SHA-256/tamanho finais e, se for o caso, `trust-policy.json` (um único campo).
- Copiar os `.tar.gz` para `vendor/approved/<asset_id>.bin`.
- Fechar a decisão sobre o fork com `unsafe` e o tamanho de fixtures (pendências do plano).

---

## 6. Testes existentes e o que um modelo novo precisa passar

Inventário (`crates/model/tests/`):

- `parity_film.rs`: paridade do fork libDF contra o **DFNet3 v1 upstream**. Casos B (fork + asset aprovado = golden upstream), C (asset FiLM-identidade sem `set_film` bit-exato com o base), D (`set_film` identidade explícito bit-exato), E (gamma=1,5/beta=0,1 muda a saída em > 0,05 e é finito), G (entradas = constantes baked, bit-exato), recusa de `set_film` em modelo sem FiLM e com dimensão errada, e troca de FiLM no meio do fluxo sem perder estado das GRUs (parity_film.rs:180-300). Os fixtures são **derivados do DFNet3 v1** (SHA-256 fixados em `fixtures/film/README.md` e parity_film.rs:27-32); tolerância de fallback 1e-4 abs/rel (:37-38). **Não testam o pDFNet3 treinado**: pesos diferentes não darão bit-exatidão contra `upstream-golden-1000f.f32`.
- `parity_eq.rs`: gancho de EQ espectral: bit-exato sem EQ, +6 dB plano exato com lag 0, bandas altas +12 dB só nos agudos, tamanho errado recusado; usa as larguras ERB congeladas (parity_eq.rs:26-31) e o asset base.
- `dsp_pipeline_parity.rs`: `DspPipeline` desacoplado (STFT/ERB/DF/iSTFT) vs `DfTract::process`, post-filter e limite de atenuação.
- `golden_reference.rs`: valida o **esquema/gate** do golden congelado (`fixtures/golden/frozen-reference.json`: 6 casos × 50 frames, tolerância abs/rel 1e-4, PESQ limiar 2,5 / observado 3,0, ligado a `asset_id = df-compatible-release-asset-v1` e ao descritor do backend) — e testa rejeição de reordenação, drift de descritor, limiar de qualidade. Regras (crates/model/src/golden.rs:119-135, 175-190): tolerância ≤ 1e-3; `quality_observed >= quality_threshold`; sem "alargar epsilon" para consertar divergência (`fixtures/golden/README.md`). Um modelo novo exige **novo golden** gerado em host isolado/offline após verificação M0 fresca e corpus aprovado (spec:148 "novos goldens"; `fixtures/corpus/README.md`: corpus 48 kHz mono, cobertura de vozes masculinas/femininas, ruídos estacionários, teclado, tráfego, ventilador, música, fala concorrente, reverberação, SNRs declarados, silêncio).
- `m0_adversarial.rs` / `asset_gate.rs`: adulteração de hashes, assinatura, chave não autorizada, JSON duplicado/malformado, policy com campos extras, symlinks, troca de arquivo após verificação — **só** para `df-compatible-release-asset-v1` (m0_adversarial.rs:11). Para novos assets o mesmo rigor vale por construção do gate (testes em `model_registry.rs:533-` cobrem papel errado, descritor sem pin, allowlist).
- `tract_smoke.rs`: asset aprovado processa um frame finito, não silencioso e com estado.
- `studio_contract.rs`: hop/sample rate do `studio-dsp` iguais ao contrato de áudio.
- Testes em `src/tract_backend.rs:386-` (FiLM em identidade bit-exato com a saída sem condicionamento; perfil não-neutro recusado em modelo sem FiLM; papéis enrollment/EQ recusados no backend de denoising) e `src/enrollment.rs:467-` (validação de áudio, descarte, contrato de saídas do ONNX).
- Saída deve ser finita: `ProcessedFrame::checked` rejeita NaN/Inf (HANDOFF:257).

Critérios implícitos que o pDFNet3 treinado deve cumprir para passar no gate de carga (execução real, não só teste): contrato 960/480/32/96/5/2 (2.1), 4 entradas nos grafos FiLM, `film_hidden = 256`, saída finita por quadro, carregável em tract 0.19.16 pulsado, assinatura/hash/allowlist válidos.

[INF] Sugestão de gate do lado do treino (não exigida pelo repo): reproduzir os casos C/D/E de `parity_film.rs` com o pDFNet3 (comparar com a saída do mesmo grafo exportado em identidade, bit-exatidão ORT vs tract) e a **estabilidade do modo "sem locutor"** (saída com `gamma=1,beta=0` vs. DFNet3 v1 em PESQ/DNSMOS).

---

## 7. Decisões registradas no vault

Busca prévia feita com `mcp__enquire__obsidian_search` (uma chamada): achou as duas notas abaixo e `projetos/hippocamp.md`; **não há** nota do vault sobre o repo de treino em si nem sobre a taxa de amostragem dos datasets.

`decisoes/2026-10-02-clearcore-pipeline-studio-arquitetura.md` (status vigente, 2026-10-02):
- Pipeline: pDFNet3 (DFNet3 + FiLM, **treinado em repo separado**) → EQ neural → cadeia DSP em Rust.
- EQ neural: contrato "ganhos em dB por banda", limitados a [−6, +12], suavizados, aplicados no domínio do tempo por filtro de fase mínima, sem overlap-add; STFT próprio (+10 ms) só se ganhar no A/B cego e depois de medir a latência fim a fim.
- Enrollment: ONNX áudio 16 kHz → gamma/beta do FiLM; só os vetores são salvos (0600, fora dos diagnósticos); denoisar o enrollment só se a ablação (TAR/TRR por SNR) mostrar ganho.
- Assets neurais: passam pelo gate assinado existente (SHA-256, tamanho, Ed25519, allowlist), generalizado para registro de descritores; DFNet3 v1 continua fallback; "O agente nunca produz assinaturas".
- Dados: "Common Voice pt e MLS Portuguese liberados para treino. TAGARELA (CC BY-NC-SA 4.0) só como dado de **teste** local, fora do treino de pesos distribuídos (app open source: o NC-SA poderia contaminar a licença dos pesos). Amostras de teste nunca vão para o git (script + SHA-256)."
- Pendente do dono: fork do libDF com `unsafe` (crate fora do workspace ou segunda exceção).
- Nota: o vault diz `project: clearcore`; as notas antigas usam `hippocamp` (nome de worktree).

`decisoes/2026-09-24-hippocamp-open-source-gratuito.md` (status vigente, 2026-09-24):
- App **gratuito e open source**; sem monetização nem distribuição proprietária; a gratuidade "não substitui a autorização explícita para copiar, converter e redistribuir os pesos".
- Aprovação do ativo M0: João Messias Lima Pereira decidiu tratar pesos e código do `DeepFilterNet3_onnx.tar.gz` v0.5.6 como **dual-licensed `MIT OR Apache-2.0`**; mesma pessoa como revisor de licenciamento e aprovador; SHA-256 `c94d91f7...`; chave Ed25519 autorizada `sha256:cfa7e10c...` com privada fora do repo (0600). É "decisão do proprietário do projeto, não um parecer de consultoria jurídica externa".
- A nota **não lista datasets**; a lista de datasets permitidos vem da nota de 2026-10-02 e do spec (seção 8). Os pesos novos (pDFNet3) precisarão de licença própria registrada: a decisão `MIT OR Apache-2.0` cobre o DFNet3 v0.5.6 oficial, não pesos retreinados [INF]; o `candidate-provenance.json` exige `weight_license` e `code_license`.

Licença do código do libDF: `MIT OR Apache-2.0`, copyright Hendrik Schröter; manter `LICENSE-MIT/APACHE` e marcar arquivos modificados (spec:166-167; plano:136; spike report). Para o repo de treino [INF]: o DeepFilterNet é do mesmo autor/licença — herdar os termos ao derivar pesos.

---

## 8. Lacunas e pontos ambíguos a resolver antes de escrever o spec do repo de treino

1. **E/S do `neural_eq.onnx` indefinida** (entrada? `emb` do pDFNet3? espectro? saída 32 ganhos dB?). Só a semântica de saída está fixada. O código do runtime do EQ neural não existe.
2. **Duração do enrollment**: spec "6-10 s" (spec:83) vs código 6-12 s (enrollment.rs:23-24) vs spec mencionando consistência entre janelas de ~2 s (não implementada). O ONNX deve aceitar N entre 96 000 e 192 000.
3. **Entrada do enrollment por índice 0**, não por nome; manter `audio [1,N]` para compatibilidade com a documentação.
4. **Ponto de inserção do FiLM** no grafo treinado: o runtime só exige 4 entradas; o site `Relu`→GRU do `gen_film_onnx.py` é o único testado. Mudar o site exige repetir a paridade e checar atraso do pulsifier (spike report:741).
5. **`df_hidden_dim` vs `emb_hidden_dim`**: o runtime usa `emb_hidden_dim` (256) para ambos (tract.rs:289-305, 1119); manter ambos 256.
6. **Taxa de amostragem dos datasets**: MLS é 16 kHz confirmado; Common Voice pt é "16 kHz segundo relato secundário" (originais 48 kHz, MP3) e licença CC0 não confirmada na fonte oficial; o pipeline do produto é 48 kHz fullband (consequência registrada em voice-samples.md:92-95). VCTK/ruídos CC0 estão catalogados como teste, não como treino.
7. **Licença dos pesos novos**: a decisão de 2026-09-24 cobre o DFNet3 oficial; o pDFNet3 retreinado exige nova entrada de governança (`weight_license` e `legal_approval_id`), com datasets compatíveis com `MIT OR Apache-2.0` (TAGARELA excluído até decisão explícita).
8. **Fallback sem FiLM em runtime**: sem asset pDFNet3, o produto roda DFNet3 v1 + DSP (spec:19, plano:115); o pDFNet3 só é usado se o gate o aceitar.
9. **Descritores sem hash**: o hash/tamanho de cada asset fica em constante Rust em `model_registry.rs`; cada nova versão do asset = novo commit de código + novo registro assinado (ou novo `asset_id`).
10. A fiação `set_voice_profile` → engine/IPC → áudio ainda **não** existe (plano:85, pendência 1); não afeta o contrato dos artefatos, mas o teste fim a fim do pDFNet3 só será possível depois da fase 1b.
