# Clearcore Train, subprojeto A: pDFNet3 + enrollment de falante — design

**Data:** 2026-10-03 (rev. 2, após revisão adversarial)
**Status:** Aprovada pelo dono em 2026-10-03 (decisões 2-4); 1, 3-execução, 5-7 pendentes. Nada foi baixado, instalado ou treinado.
**Repositório:** `~/orca/projects/clearcore-train` (separado do Clearcore; ainda não existe).
**Escopo:** subprojeto A (pDFNet3 e `enrollment.onnx`). O subprojeto B (EQ neural) terá spec própria (seção 10).

> **Emenda 2026-10-03 (tarde, decisões do dono).**
> **D1 (encoder):** o encoder de falante do enrollment é o SpeechBrain ECAPA pré-treinado
> (`speechbrain/spkrec-ecapa-voxceleb`, pesos Apache-2.0, treinado em VoxCeleb), portado para o grafo tract-safe do
> projeto; substitui o ECAPA próprio treinado em dados CC BY/CC0 (§3 e a justificativa de §3.1 não valem para o
> enrollment). O avaliador independente dos gates passa a ser só o WeSpeaker ResNet34; `gates-v2.yaml` troca apenas a
> lista de avaliadores, com os mesmos níveis (aprovado pelo orquestrador).
> **D2 (licença):** somente o asset de enrollment (`enrollment.onnx`, `voice-enrollment-asset-v1`) é licenciado como
> **CC BY-NC 4.0**, com NOTICE declarando os pesos SpeechBrain (Apache-2.0) e a proveniência VoxCeleb ("research
> purposes"; copyright do áudio com os donos dos vídeos). Código do Clearcore, pDFNet3 e EQ neural continuam
> `MIT OR Apache-2.0`. O perfil de voz do Clearcore é, portanto, de uso não comercial; releases já publicadas não mudam.
> A decisão 5 de §11 foi atualizada de acordo; o restante do texto abaixo não foi reescrito.

Convenção: **[F: arquivo]** é fato com fonte (pesquisas em `docs/superpowers/research/2026-10-03-clearcore-train/`,
abreviadas `R01`..`R08`, ou código do Clearcore, conferido nesta data). **Decisão** é a recomendação deste spec.
**[est.]** marca número de custo estimado, que só vale depois de medido no M0 (seção 9). Para não confundir com os
subprojetos A e B, os dois estágios de treino do pDFNet3 se chamam **estágio C** (base congelada) e **estágio F**
(fine-tune completo).

Decisões do dono já tomadas e não reabertas aqui: repo separado; fine-tune do DFNet3 upstream v0.5.6 (não do zero);
subprojeto A antes do B; nada é baixado antes do plano aprovado; pesos `MIT OR Apache-2.0` com uso comercial, logo só
dados com licença compatível (NC proibido; TAGARELA só teste); Common Voice pt e MLS pt liberados.

## 1. Objetivo, escopo e critérios de sucesso

**Objetivo.** Entregar ao Clearcore dois assets que passem pelo gate assinado existente:

1. `pdfnet3-release-asset-v1`: os três grafos do DFNet3 com entradas FiLM (`gamma`, `beta`) no encoder e no DF decoder,
   que preservam o falante cadastrado e suprimem ruído e falantes concorrentes, e que se comportam como o DFNet3
   upstream (dentro da tolerância do G1) quando recebem FiLM identidade, ou seja, sem perfil.
2. `voice-enrollment-asset-v1`: `enrollment.onnx`, áudio 16 kHz → `gamma_enc, beta_enc, gamma_df, beta_df` (+ `embedding`).

**Escopo.** Encoder de falante, gerador FiLM, fine-tune do DFNet3, preparo de dados, export ONNX, verificação de
paridade com o tract 0.19.16, avaliação no caminho do runtime (gates G0..G11, ablações AB1 e AB2), pacote de entrega
com proveniência.

**Fora de escopo.** EQ neural e AB3 (subprojeto B); variante stateful/OpenVINO (seção 6.4); treino do zero; BWE e
modelos generativos; mudanças no Clearcore (seção 2.5 lista o que o Clearcore precisa fazer); assinaturas (só o dono);
PESQ/POLQA como gate (licença, R06 §1.1).

**Critérios de sucesso** (verificáveis por artefato):

- S1. Os dois `.tar.gz` carregam no tract 0.19.16 com o mesmo código de carga do Clearcore e passam G0 e G11.
- S2. Modo sem perfil: G1 aprovado (bit-exato se o estágio C for o entregue; com as margens do G1 se for o F).
- S3. Com perfil: G2, G3, G4, G5 e G6 aprovados no teste final, medidos no caminho do runtime (seção 3.5), com os
  níveis de exigência congelados em `configs/eval/gates-v1.yaml` antes do T1 (inclusive a regra da sonda do T2).
- S4. `report.json` + proveniência completa (manifests com SHA-256, commit, configs, seeds, versões) e rascunhos dos
  registros de governança para o dono assinar.
- S5. Todos os dados de treino têm licença na allowlist (seção 4.1), comprovada pelo manifest.

## 2. Contrato de entrega para o Clearcore

Fatos do código, salvo onde marcado **Decisão**.

### 2.1 `pdfnet3-release-asset-v1`

- **Membros** (allowlist, `crates/model/src/model_registry.rs:202-208`): `enc.onnx`, `erb_dec.onnx`, `df_dec.onnx`,
  `config.ini`. Arquivos regulares, sem bit executável, sem diretórios/links, sem duplicatas, todos presentes; o
  prefixo `tmp/export/` é tolerado (`model_registry.rs:409-459`, R01 §5.2).
- **A ordem é o contrato**: o runtime renomeia entradas e saídas por posição
  (`vendor/crates/deep_filter/src/tract.rs:941-945, 1016, 1121-1125`).

| Grafo | Entradas (ordem, forma) | Saídas (ordem) |
|---|---|---|
| `enc.onnx` | `feat_erb [1,1,S,32]`, `feat_spec [1,2,S,96]`, `gamma [1,S,256]`, `beta [1,S,256]` | `e0, e1, e2, e3, emb, c0, lsnr` |
| `erb_dec.onnx` | `emb [1,S,512]`, `e3`, `e2`, `e1`, `e0` (sem FiLM) | `m` |
| `df_dec.onnx` | `emb [1,S,512]`, `c0 [1,64,S,96]`, `gamma [1,S,256]`, `beta [1,S,256]` | `coefs` |

- FiLM é detectado **se e somente se** o grafo tem exatamente 4 entradas (`tract.rs:306, 311, 935, 1116`);
  `gamma`/`beta` nas posições 2 e 3; `film_hidden = emb_hidden_dim = 256` (`tract.rs:289`).
- Semântica: `x' = x * gamma + beta` na saída do `ReLU` de `emb_gru.linear_in` (enc) e de `df_gru.linear_in`
  (df_dec), sites `/emb_gru/linear_in/1/Relu_output_0` e `/df_gru/linear_in/linear_in.1/Relu_output_0`
  (`tools/accelerators/gen_film_onnx.py:19-22, 36-43`). O runtime reenvia o mesmo vetor `[1,1,256]` em todo quadro:
  FiLM é constante no tempo (`tract.rs:585-588, 625-628`).
- Contrato fixo: 48 kHz, mono, hop 480, FFT 960, `nb_erb=32`, `nb_df=96`, `df_order=5`, `df_lookahead=2`,
  `conv_lookahead=2`, latência 1.440 amostras, `[train] model = deepfilternet3` (R01 §2.1-2.2).
- No produto, "sem perfil" é `FiLMVectors::identity()` (gamma=1, beta=0) aplicado pelo runtime
  (`tract_backend.rs:150-190`, `voice_profile.rs:127-130`); o `enrollment.onnx` nunca produz a identidade.
- **Decisão — `config.ini`**: o do DFNet3 v0.5.6 byte a byte (SHA-256
  `415eb925d44990d938fb739f514aa3662c1ec0ea836cff044fa1291b82cb4290`, `models/stateful/README.md:21`). A topologia
  não muda; parâmetros de treino ficam no repo de treino. O teste é igualdade de hash.
- Saída finita em todo quadro (`ProcessedFrame::checked`), carregável pulsado em `S=1` (`tract.rs:951`).

### 2.2 `voice-enrollment-asset-v1`

- Membro único `enrollment.onnx` (`model_registry.rs:210-216`; `crates/model/src/enrollment.rs:357`).
- **Entrada**: posição 0, `f32 [1, N]`, 16 kHz mono; o runtime fixa `N` = tamanho real do áudio a cada extração
  (`enrollment.rs:426-430`). **Decisão**: nome `audio`; o ONNX aceita **N ≥ 32.000 (2 s)** até 192.000 (12 s), para
  permitir a consistência entre janelas de ~2 s pedida no spec do Clearcore (2026-10-01 §6). A validação da gravação
  inteira no Clearcore continua 6-12 s.
- **Saídas** localizadas por nome (`enrollment.rs:358, 421-456`; saídas extras são ignoradas): `gamma_enc`,
  `beta_enc`, `gamma_df`, `beta_df`, f32 `[256]`. **Decisão**: saída aditiva `embedding` f32 `[192]` (embedding do
  ECAPA após L2norm), para o Clearcore medir a consistência entre janelas por cosseno sem um segundo modelo. Opset 13,
  `ir_version` 8, como o fixture `gen_enrollment_test_onnx.py`.
- Limites, rejeitados e nunca "clampados": `gamma ∈ [0,001; 100]`, `beta ∈ [−50; 50]`, sem NaN/Inf
  (`voice_profile.rs:14-17, 164-200`).
- **Duração (6-10 s × 6-12 s), resolvida**: o código aceita 6 a 12 s (`enrollment.rs:23-28`); o spec do Clearcore
  manda a UI **gravar** 6-10 s. Não há conflito: 6-12 s é o contrato, 6-10 s é a escolha de UI. **Decisão**: o treino
  sorteia enrollments em U[6, 12] s; o ONNX é testado em 2, 6, 8, 10 e 12 s; a avaliação reporta 6-10 s e 30 s.
  Enrollments de treino e teste passam pela mesma validação de `validate_enrollment_audio` (pico ≤ 0,99,
  RMS > −40 dBFS, fala ativa ≥ 60%, `enrollment.rs:30-41, 147-188`), reimplementada em Python com teste de paridade.
- **Reamostragem 48 → 16 kHz** [F]: o único resampler do Clearcore, `LinearResampler`
  (`crates/format-adapter/src/resampler.rs:65`), é interpolação linear sem anti-aliasing. **Decisão**: o filtro é
  parte do contrato. O repo de treino define `resampler-48k-16k-v1`: FIR de fase linear, 241 taps, janela Kaiser
  β=8,6, corte 7.600 Hz, decimação por 3, saída `y[n] = Σ_k h[k]·x[3n + 120 − k]` (atraso compensado, zeros nas
  bordas). Publica os coeficientes (f32), seu SHA-256 e vetores dourados. Treino, banco de enrollment e avaliação usam
  exatamente esse filtro; o Clearcore precisa implementá-lo (seção 2.5).

### 2.3 Pacote

**Decisão** (segue `gen_film_onnx.py:pack`): tar com membros planos, ordem fixa, modo 0644, `mtime=0`, `uid=gid=0`,
`uname=gname=""`; gzip `mtime=0`. Nome `<asset_id>.tar.gz`, com SHA-256 (hex minúsculo) e tamanho.

### 2.4 Governança: quem produz o quê

| Item | Quem produz |
|---|---|
| `<asset_id>.tar.gz`, SHA-256, tamanho | repo de treino |
| `candidate-provenance.draft.json` com os 7 campos exatos (`asset_id, code_license, conversion_terms, sha256, source, status, weight_license`; R01 §5.1), `source = urn:sha256:<sha>`, `status = "PENDING_OWNER"` | repo de treino (rascunho) |
| `conversion_terms` proposto: estágio C, "pesos do DeepFilterNet3 v0.5.6 (asset `c94d91f70911001c946e0fabb4aa9adc37045f45a03b56008cb0c8244cb63616`) inalterados, com entradas FiLM inseridas por edição de grafo"; estágio F, "fine-tune desses pesos com condicionamento FiLM"; ambos citam repo, commit e manifests | repo de treino (rascunho) |
| `PROVENANCE.md`, `NOTICE` (atribuições CC BY), model card, `report.json` + SHA-256, goldens de paridade, coeficientes do resampler | repo de treino |
| `candidate-provenance.json` final, `legal-review.json`, `approval-manifest.json`, assinatura Ed25519, `approver-public-key.pem`, eventual `trust-policy.json` | **dono** |
| SHA-256/tamanho em `model_registry.rs`, cópia para `vendor/approved/<asset_id>.bin`, testes e goldens | Clearcore fase 5, com o dono |

### 2.5 Requisitos que este spec impõe ao Clearcore (fase 5)

1. Reamostrar o enrollment 48 → 16 kHz com `resampler-48k-16k-v1` (coeficientes do repo de treino), com teste de
   paridade contra os vetores dourados; não usar o `LinearResampler` para enrollment.
2. Consistência entre janelas de ~2 s via saída `embedding` (o limiar de cosseno vem do relatório, calibrado no EER).
3. Avaliar e operar com `atten_lim_db` desligado no modo personalizado, ou aceitar que um limite de `L` dB limita a
   rejeição do G4 a `L` dB (seção 3.5).
4. Os demais testes (contrato, paridade tract com os goldens, subconjunto Rust, golden de qualidade, G9) da seção 7.4.

## 3. Arquitetura

### 3.1 Speaker encoder

**Decisão: ECAPA-TDNN próprio (C=512, embedding 192-d, ~6 M parâmetros [est.]), treinado com AAM-softmax só em
dados CC BY/CC0, congelado depois do M1.**

Justificativa [F: R03 §2-§6]: o VoxCeleb tem três textos de licença incompatíveis entre si e o áudio é de terceiros
(YouTube); o encoder vai **dentro** do `enrollment.onnx` distribuído, então SpeechBrain/WeSpeaker tornariam falso o
enunciado "pesos MIT OR Apache-2.0, dados CC BY/CC0". Não existe encoder publicado só com dados permissivos. O ECAPA usa
operadores comuns e é a família do pDeepFilterNet2 (R02 §3.1). SpeechBrain ECAPA-VoxCeleb e WeSpeaker ResNet34 entram
só como ferramentas locais (avaliação G5 e filtro do CV, seção 4.6), nunca no asset.

- Front-end **dentro do grafo** (mantém `audio [1,N]`): Hamming 25 ms, hop 10 ms, DFT como `Conv1d` com base de
  Fourier fixa (sem op `STFT`, não verificado no tract 0.19.16, R03 §4), potência, 80 mel (`MatMul`), `Log(x+1e-6)`,
  normalização de média no tempo.
- Dados (seção 4.2): LibriSpeech train-clean-100/360 **e train-other-500** (teto 4 min/falante), VCTK, CML-TTS pt e,
  se aprovado, Common Voice pt. Aumentação de canal: ruído, RIR, curvas de microfone aleatórias e codecs (MP3/Opus).
  VoxPopuli fica fora (licença ambígua).
- **Gates do M1** (níveis fixos em `gates-v1.yaml`): EER ≤ 3% em LibriSpeech test-clean; **fora do domínio** EER ≤ 6%
  em VCTK retido e ≤ 8% em CV pt retido (se o CV for aprovado); paridade PyTorch × tract ≤ 1e-4 em 2, 6, 8, 10 e 12 s;
  **gate funcional**: uma rodada curta do estágio C (~1 h de GPU [est.]) em misturas de falantes do **mesmo sexo**
  com SIR 0 dB, ΔSI-SDR vs DFNet3 com limite inferior do IC 95% > 0.

### 3.2 Gerador FiLM

**Decisão:**

```
e   = L2norm(ecapa(fbank(audio)))                    # [192], também exposto como saída `embedding`
h   = ReLU(Linear(192→256)(e))
d   = Linear(256→1024)(h) · mask                     # pesos e bias da última camada iniciados em ZERO
dγe, dβe, dγd, dβd = split(d, 4)                     # [256] cada
gamma_* = exp(Γ · tanh(dγ*)),  Γ = ln 10             # ∈ [0,1; 10]
beta_*  = B · tanh(dβ*)                              # ∈ [−B; B]
```

- **Identidade exata por construção** com a última camada zerada (`tanh(0)=0`, `exp(0)=1`; `x·1+0` é exato e os
  valores pós-ReLU não são `−0`). Gradiente zero nas camadas internas no passo 0 é benigno (R02 §2.3).
- **Faixa configurável e limitada**: `Γ` e `B` vêm da config. Regra determinística para `B`, aplicada no M0 e congelada
  antes do T2: `B = min(50, max(4, 2·p99))`, com `p99` = percentil 99 das ativações nos dois sites medido no DFNet3
  upstream sobre os dados do M0. `[e^−Γ; e^Γ] ⊂ [0,001; 100]` exige `Γ ≤ ln 100`; o validador da config recusa fora
  disso. O ONNX nunca emite valor que o runtime rejeite.
- **Saturação**: cada avaliação loga, por site e por vetor, a fração de componentes com `|tanh| > 0,95`; acima de 5% no
  fim de um run, o relatório marca e a faixa é revista no run seguinte (nunca no meio de um run).
- **Modo sem perfil**: `mask=0` reproduz exatamente a identidade. Embedding dropout: `p_drop = 0` no estágio C (base
  congelada: o modo sem perfil já é exato e amostras `mask=0` não dão gradiente ao gerador); `p_drop = 0,2` no estágio F
  (AB2 varre).
- FiLM no tempo: um vetor por amostra, broadcast `[B,1,256]` em todos os quadros, como no runtime.

### 3.3 Onde o FiLM entra no DFNet3

**Decisão**: patch mínimo em `SqueezedGRU_S.forward(input, h=None, film=None)`: `x = linear_in(input)` (ReLU incluso);
`x = x*gamma + beta` se `film` existir; GRU. `Encoder.forward` repassa a `emb_gru`, `DfDecoder.forward` a `df_gru`
(R02 §2.2). São os sites do `gen_film_onnx.py`. FiLM nos dois pontos (o ramo DF é o que mais ganha com o embedding no
pDFNet2, R02 §3.1). O `lsnr` do encoder sai do `emb` pós-GRU, logo também é condicionado pelo FiLM.

### 3.4 Divisão entre os ONNX

| `enrollment.onnx` | 3 grafos do pDFNet3 |
|---|---|
| fbank in-graph + ECAPA + L2norm + gerador + `exp/tanh`; saídas FiLM e `embedding` | topologia DFNet3 + `Mul`/`Add` nos 2 sites; nenhum parâmetro do gerador |

### 3.5 Gating por `lsnr` no runtime

[F] O runtime decide por quadro a partir do `lsnr` previsto (`vendor/crates/deep_filter/src/tract.rs:805-819`;
`crates/model/src/dsp_pipeline.rs:63-90, 205`), com padrão −10/30/20 dB e `atten_lim_db` desligado:
`lsnr < −10` → máscara zero; `lsnr > 30` → **nenhum processamento** (o interferente passa cru); `lsnr > 20` → só a
máscara ERB, DF desligado; senão, ERB + DF. O forward PyTorch do upstream não aplica esse gating.

**Decisões:**

- **Avaliação no caminho do runtime**: G1-G6 e as ablações são medidos com o gating. No teste final, pelo
  `tract-check` (Rust, o próprio runtime). No dev, por uma réplica Python (`eval/runtime_path.py`: `apply_stages` +
  aplicação de máscara/DF) com paridade ≤ 1e-4 contra o `tract-check` em 1.000 quadros. Cada cenário reporta a fração
  de quadros em cada estágio (zero / só ERB / ERB+DF / sem processamento).
- **Treino com gating emulado no forward** desde o T2: a decisão de estágio usa o `lsnr` previsto (destacado do grafo
  de gradiente) e a saída usada na perda é a que o runtime produziria. Quadros "sem processamento" passam o ruidoso.
- **Alvo de `lsnr`**: com perfil, fala-alvo contra tudo o mais (ruído + interferentes), o que leva o `lsnr` abaixo de
  30 dB quando há interferente; sem perfil, toda a fala contra o ruído. Penalidade adicional
  `L_gate = λ_g · média(relu(lsnr_prev − 30)²)` nos quadros com interferente ativo e perfil presente, `λ_g = 1e-3`.
  Na classe "sem alvo" (silêncio), a `LocalSnrLoss` é **mascarada** (não se força −15 dB, para não ensinar o modelo a
  zerar quando incerto).
- **`atten_lim_db`**: os gates valem com ele desligado (padrão). Um limite de `L` dB limita a atenuação do interferente
  a `L` dB e torna o G4 inatingível para `L < 15`; registrado como requisito ao Clearcore (seção 2.5).

## 4. Dados

### 4.1 Allowlist de licenças

**Decisão**: o loader recusa qualquer arquivo cujo manifest não tenha licença em
`{CC0-1.0, CC-BY-4.0, Apache-2.0, domínio público declarado}`. NC, ND, SA, ODbL/DbCL, GPL, "research only" e
licença não confirmada ficam fora do treino [F: R04 §3, R05 §1]. Fontes "só teste" têm marcação própria e o loader de
treino as recusa.

### 4.2 Fontes

| Fonte | Licença [F] | SR | Papel | Conj. |
|---|---|---|---|---|
| VCTK 0.92, só `mic1` | CC BY 4.0 (R04 §1.1) | 48 kHz | alvo/interferente full-band, encoder | mín. |
| TTS-Portuguese Corpus | CC BY 4.0 (R05 §3) | 48 kHz | alvo full-band pt-BR (1 falante, peso ≤ 5%) | mín. |
| CML-TTS pt (cap 30 h no mín.) | CC BY 4.0 (R05 §3) | 24 kHz | alvo pt, encoder | mín. |
| Common Voice pt 27.0 (MP3 original) | CC0-1.0, confirmada na fonte (R05 §2) | 48 kHz nominal, banda medida | interferente; alvo e encoder **se** o `client_id` for aprovado | mín. |
| LibriSpeech train-clean-100/360 e train-other-500, teto 4 min/falante | CC BY 4.0 (R03 §3, R04 §1.1) | 16 kHz | encoder; no estágio C também alvo/interferente | mín. |
| DNS `noise_fullband`, só arquivos Freesound CC0 identificáveis | CC0 (R04 §1.2) | 48 kHz | ruído | mín. |
| FSD50K eval, só CC0/CC BY e sem classes de voz | por clipe (R04 §1.2) | 44,1 kHz | ruído | mín. |
| RIR sintéticas (image source, RT60 0,05-1,0 s, 48 kHz), geradas por nós | sem terceiros | 48 kHz | reverberação | mín. |
| MLS pt (dedup. contra CML-TTS), LibriTTS-R clean-100/360, HiFi-TTS (subset), DNS noise maior, FSD50K dev, SLR28 | CC BY 4.0 / Apache-2.0 (R04) | 16-48 kHz | escala | completo |

Só teste: DEMAND (CC BY-SA), TAGARELA (via `--with-tagarela` do Clearcore), VCTK-DEMAND, DNS5 dev (se houver disco).
Excluídos: EARS, Expresso, WHAM!, CETUC, CORAA, VoxCeleb, VoxPopuli, AudioSet e DEMAND dentro do DNS (R04, R05).
Arquivo do DNS cuja origem Freesound/CC0 não seja determinável sai do manifest. CV: **26.0 pt-BR** (122 MB) no M0;
**27.0** no treino e na avaliação finais.

**Orçamento de disco [est.]**: dados mínimos ~30 GB em FLAC na taxa nativa (VCTK mic1 5,5-7,5; TTS-PT 1,8; CML-TTS
2,6; CV 4,9 em MP3; LibriSpeech com teto 9,5; ruído 4,3; RIR 0,5); ambientes Python ~8; `target/` do `tract-check`
~3; Whisper, avaliadores e DNSMOS ~3; conjuntos de teste ~3; misturas de dev/teste pré-renderizadas ~3; runs e
checkpoints ~2. **Total ~45-52 GB contra 31 GB livres**. Completo: ~110 GB. Brutos são transitórios (extração em
stream onde o formato permitir; um bruto por vez), em disco persistente, nunca em `/tmp` (tmpfs de 16 GB, R07 §3).

### 4.3 Taxas de amostragem

**Decisão** (o loader é nosso, seção 5.1):

- Cada fonte na **taxa nativa** em FLAC (CV em MP3 original), reamostrada para 48 kHz no loader (soxr HQ).
- `max_freq` por amostra = metade da banda **efetiva**, medida por clipe (rolloff de 99%), não pela taxa do arquivo.
- Ruído e RIR com passa-baixa em `max_freq` da fala quando ela é limitada (prática do DFN, R04 §4.1); a perda ignora
  bins acima de `max_freq` (`loss.forward(..., max_freq)`, R02 §1.2).
- **Estágio C**: a base congelada não aprende banda, então a amostragem prioriza **diversidade de falantes** (todas as
  fontes de fala, inclusive LibriSpeech e CV), sem cota de banda.
- **Estágio F**: cota por batch da fala-alvo **≥ 50% full-band, ~30% 24 kHz, ≤ 20% 16 kHz** (R04 §4.3), com
  destilação (seção 5.4) e o critério de LSD > 8 kHz do G1.
- Sem BWE nos dados. G2 e G4 são reportados também **por corpus** do alvo.

### 4.4 Montagem das misturas

**Decisão** (R02 §3.2, R06 §2.3), com perfil (`mask=1`):

| Tipo | Fração | Alvo da perda |
|---|---|---|
| alvo + ruído | 35% · (1 − s) / 0,9 | fala-alvo seca |
| alvo + interferente | 20% · (1 − s) / 0,9 | fala-alvo seca |
| alvo + interferente + ruído | 35% · (1 − s) / 0,9 | fala-alvo seca |
| só interferente (± ruído), perfil de quem não fala ("locutor trocado") | s | silêncio |

- `s` (fração de silêncio) é hiperparâmetro: `s = 5%` por padrão, varrido em {0; 5; 10}% na AB2. Com `s = 10%`, as
  frações ficam 35/20/35/10.
- Sem perfil (`mask=0`, só no estágio F): alvo = soma das falas secas de **todos** os falantes presentes (padrão do
  UPN, R02 §2.3).
- SNR: grade do upstream `{−5, 0, 5, 10, 20, 40}` dB (R02 §1.2); SIR U[−5, 25] dB; 1 ou 2 interferentes; RIR com
  `p = 0,3`, aplicada a alvo e interferente separadamente (alvo da perda sem RIR, como o upstream). Trechos de 3 s.
- Interferente de outro falante do mesmo split; sem filtro de similaridade (reavaliar se G4 falhar em mesmo sexo).
- **Enrollment**: outro trecho do mesmo falante (nunca o da mistura), U[6, 12] s, concatenando falas curtas com
  pausas de 150-400 ms; aumentação: ruído com SNR U[0, 30] dB (p=0,7), RIR (p=0,3) e, com **p=0,5, denoise pelo
  DFNet3 upstream sem perfil** (requisito 2); reamostragem por `resampler-48k-16k-v1`; recusa se falhar a validação.
- **Divergência registrada**: o treino denoisa o enrollment com o DFNet3 upstream; o produto usaria o pDFNet3 sem
  perfil. No estágio C são o mesmo modelo. No estágio F diferem dentro da tolerância do G1; a AB1 mede no teste com o
  caminho do produto (pDFNet3 F sem perfil), então o efeito da divergência aparece no resultado e não é suposto nulo.
- **Banco de embeddings pré-computado**: com o encoder congelado, cada falante de treino recebe ~20 variantes de
  enrollment, guardadas só como embedding de 192-d. O encoder não roda no treino do pDFNet3.

### 4.5 Splits disjuntos por falante

**Decisão**: split por falante, nunca por clipe, fixado no manifest com seed, **comum ao T1 e ao T2/T3**: nenhum
falante de dev/teste aparece no treino do encoder, no do pDFNet3, como enrollment, como interferente ou no ajuste de
hiperparâmetros (R06 §2.1). Teste fixo: VCTK p225, p226 (amostras do Clearcore), p232, p257 (VCTK-DEMAND) e mais 10
falantes VCTK sorteados; dev/test oficiais do LibriSpeech e do CML-TTS; ≥ 40 falantes pt retidos do CV (se aprovado).
O MLS só entra deduplicado por falante contra o CML-TTS. Ruídos de teste disjuntos dos de treino (DEMAND só teste).
Teste automatizado de disjunção sobre os manifests.

### 4.6 Common Voice: conformidade

[F: R05 §2] A página oficial declara CC0-1.0 e proíbe (a) tentar determinar a identidade de falantes e (b) re-hospedar
o dataset. Um `client_id` pode cobrir várias pessoas e uma pessoa pode ter vários `client_id` (R03 §3, R05 §2).

**Decisão**: (b) nenhum áudio nem derivado em forma de áudio sai da máquina (inclusive goldens: o perfil do golden P2
vem de falante VCTK, nunca do CV); só pesos. (a) o `client_id` só agrupa clipes do mesmo gravador para formar pares
enrollment/alvo e para a perda do encoder; nenhum metadado é cruzado com fonte externa, nenhum rótulo é publicado.
Filtro de rótulo: mínimo de 20 clipes por `client_id` e descarte de clipes cujo cosseno ao centróide do próprio
`client_id`, medido por um avaliador independente (SpeechBrain ECAPA, local), fique abaixo do limiar de EER desse
avaliador. Como treinar um encoder de falante com esse agrupamento toca a cláusula (a), **o uso depende do dono**
(seção 11), e se recomenda consultar a Mozilla (commonvoice@mozilla.com). Sem aprovação: o CV entra só como
interferente com alvo de outro corpus, e o encoder treina com LibriSpeech + VCTK + CML-TTS.

## 5. Pipeline de treino

### 5.1 Dataloader

**Decisão: dataloader próprio em Python** (`IterableDataset`), features pelo `deepfilterlib` 0.5.6 (`df_features`,
wheel pronta). Motivo [F: R02 §1.4]: o loader Rust upstream não devolve chave de arquivo nem de falante, seu
interferente tem SIR fixo {15, 20, 30} dB e pode ser o próprio alvo, e há suspeita de RIR aplicada ao alvo no laço do
interferente; forkar o `pyDF-data` exige pyo3 0.19 e `maturin<0.12`. Teste obrigatório: features do loader idênticas
às do `libdf` para o mesmo áudio.

### 5.2 Ambientes

**Decisão** (R02 §1.5-1.6):

| Ambiente | Conteúdo | Uso |
|---|---|---|
| `train` | Python 3.11 (uv), torch com CUDA ≥ 12.8 (sm_120), `numpy<2`, `deepfilterlib==0.5.6`, DeepFilterNet v0.5.6 vendorizado com patch em `df/io.py` | treino, avaliação, export do enrollment |
| `export` (só no estágio F) | Python 3.10, torch 1.13.1 CPU, torchaudio 0.13.1, `onnx`/`onnxruntime` pinados | reexport dos 3 grafos com os nomes de nó do asset aprovado |
| `tract-check` | Rust 1.90.0 em `~/.rustup` (persistente; o 1.90 atual está em `/tmp`, volátil), `tract-onnx =0.19.16` e o `deep_filter` vendorizado do Clearcore por dependência git pinada num commit | paridade, carga, gating, G10 |

Pesos passam entre ambientes como `safetensors` de `state_dict`, com `.contiguous()` em cada tensor antes de salvar (o
modelo usa `channels_last`, R02 §1.1). `CARGO_TARGET_DIR` no disco do repo, nunca em `/tmp` (handoff do Clearcore §3).

### 5.3 Fases

| Fase | Treina | Congela | Config principal |
|---|---|---|---|
| T1 encoder | ECAPA + AAM-softmax (m=0,2, s=30) | — | crops 2-3 s, 16 kHz, aumentação de canal |
| T2 **sonda**, estágio C | só o gerador FiLM (~310 k parâmetros) | DFNet3 inteiro em `eval()` e encoder | AdamW, LR 1e-3, warmup; 1 rodada |
| T3 **caminho planejado**, estágio F | DFNet3 + gerador (iniciado do T2) | encoder; BatchNorm em `eval()` com parâmetros congelados | LR 1e-4 cosseno, early stopping no dev, destilação |

**Decisão**: o estágio F é o caminho planejado (orçamento, disco, janela de máquina e cronograma assumem o T3). Motivo:
o pDeepFilterNet2 treina a rede de realce inteira com o ECAPA congelado (arXiv 2404.08022 §3.2.2), e não há publicação
de FiLM com base congelada; o estágio C provavelmente não passa G2/G4. O T2 é uma **sonda** de 3-6 h [est.] que valida
encoder, gerador, sites e dados antes de gastar o orçamento do T3, com regra pré-registrada em `gates-v1.yaml`:

- **no-go**: ΔSI-SDR (C − DFNet3) em C3 a SIR 0 dB, dev, com limite inferior do IC 95% < +1 dB → para e diagnostica
  (encoder, banco de embeddings, dados, sites) antes do T3.
- **go**: ≥ +1 dB → segue para o T3 com o gerador iniciado do T2.
- Se o estágio C já passar **todos** os gates bloqueantes no dev, ele vira candidato à entrega (G1 bit-exato); a
  escolha final entre C e F segue a decisão do dono da seção 11 (item 4).

Integridade da base no estágio C: hash SHA-256 do `state_dict` da base antes e depois do T2 deve ser idêntico (teste
automatizado). No estágio F, BatchNorm congelado evita deriva das estatísticas com lotes de 16-32.

### 5.4 Perdas

- Base: perda do DFNet3 upstream (multi-resolução de módulo e complexa, `factor=500`, `factor_complex=500`,
  `gamma=0,3`, FFT 256/512/1024/2048; `LocalSnrLoss` 1e-3) (R02 §1.2), calculada sobre a saída com gating emulado
  (seção 3.5).
- **Over-suppression assimétrica**, implementada por nós (sem reaproveitar `factor_under` sem ler a semântica):
  `L_OS = λ_OS · média_res Σ h(|S|^0,3 − |Ŝ|^0,3)²`, `h(x)=max(x,0)`, `λ_OS = 500` (pDFNet2, R02 §3.1).
- `L_gate` da seção 3.5.
- Estágio F: destilação nas amostras `mask=0`, `L_T = λ_T · MR(Ŝ_aluno, Ŝ_DFNet3 congelado)`, `λ_T = 500`.
- Bins acima de `max_freq` excluídos de todas as perdas.

### 5.5 Orçamento nesta máquina

[F: R02 abertura, R07]: RTX PRO 1000 com 8 GB e limite de 50 W; 16 CPUs; 30 GB de RAM com ~4 GB livres e swap 85%
ocupado; 31 GB livres em disco. (O "~100 M parâmetros" e o "batch 1-4" de R07 estão errados: o DFNet3 tem ~2,1 M,
R02 §4.1.)

- GPU [est.]: batch 16-32 de 3 s em fp32 (sem AMP, como o upstream). T1 10-20 h; T2 sonda 3-6 h; gate funcional do M1
  ~1 h; T3 15-35 h (inclui o forward do professor DFNet3 na destilação, +~30%); AB1 no estágio F: uma rodada extra
  (`p=0`), 15-35 h; AB2: 5 ramos de 3-8 h cada a partir do checkpoint do T3 a 70% do cronograma.
- RAM: loader com 4 workers e prefetch 2, alvo ≤ 6 GB RSS [est.]; exige fechar aplicações (seção 11).
- Térmica: log de `nvidia-smi` (temperatura, clock, `clocks_throttle_reasons`) por minuto no diretório do run.

### 5.6 Checkpoints e retomada

**Decisão**: `runs/<run_id>/checkpoints/` no disco do repo (ignorado pelo git), nunca em `/tmp`. Gravação atômica
(temporário no mesmo diretório + `rename`) a cada 30 min e no fim de época; mantém os 3 últimos + o melhor no dev. Cada
checkpoint guarda modelo, otimizador, scheduler, passo, estados de RNG (Python, NumPy, torch, CUDA), posição do
sampler, hash da config e commit. `train --resume` retoma do último checkpoint válido e reproduz a sequência de amostras.

## 6. Export e verificação

### 6.1 Export do pDFNet3

**Decisão:**

- **Estágio C: não reexporta.** `add_film` (lógica de `gen_film_onnx.py:26-43`, copiada com atribuição) é aplicado aos
  três ONNX do asset aprovado (`c94d91f7…`). Teste: todos os initializers do resultado são iguais, byte a byte, aos do
  asset aprovado; os únicos nós novos são os 2 `Mul` e 2 `Add`.
- **Estágio F**: no ambiente `export`, `df/scripts/export.py` do upstream (cópia própria) exporta os 3 grafos dos pesos
  treinados com os nomes de nó do asset aprovado (R02 §5.1); depois `add_film` nos dois sites (falha se o site não
  tiver exatamente 1 produtor).
- O script de export do DFNet3 com FiLM identidade (requisito 5 do spec do Clearcore) é o caminho do estágio C.

### 6.2 Export do enrollment

ONNX no ambiente `train`, com front-end in-graph, eixo dinâmico `N`, opset 13, `ir_version` 8, BatchNorm fundida, sem
controle de fluxo. O attentive statistics pooling do ECAPA (`Expand`/`Tile` sobre o eixo de tempo dinâmico) é
prototipado no tract já no M0. Testes: carga no tract com o procedimento de `enrollment.rs:418-435` em 2, 6, 8, 10 e
12 s; entradas de −40 dBFS RMS a pico 0,99; áudio de borda (silêncio, ruído branco, clipping, música) nunca gera
NaN/Inf nem valor fora dos limites.

### 6.3 Paridade e identidade

| Teste | Comparação | Critério |
|---|---|---|
| I1 identidade PyTorch | pDFNet3 com `mask=0` × DFNet3 upstream, mesmo lote, **CPU determinística** (`torch.use_deterministic_algorithms`) | `max_abs_diff == 0` |
| I2 identidade tract, pesos upstream (estágio C; = G1b) | `tract-check` com FiLM identidade × golden de 1.000 quadros do upstream (SHA-256 `c041ee1fa4b2ce4bdad4f07e28b8fdcb901c56bab20380d17abd254d5c9736aa`, `crates/model/tests/parity_film.rs:28`) | `max_abs_diff == 0`; é o caso C/D já bit-exato |
| I3 identidade tract, pesos do estágio F | grafo com FiLM identidade × mesmo grafo sem FiLM | `max_abs_diff == 0` |
| P1 ORT × PyTorch | 3 grafos e enrollment | `atol 1e-5, rtol 1e-6` (critério do `export.py` upstream) |
| P2 tract × PyTorch | 1.000 quadros de uma **mistura real** (falante VCTK de teste + ruído CC0) com `lsnr` entre −10 e 20 dB, de modo que ERB e DF sejam exercitados × 3 vetores FiLM (identidade, perfil de um falante VCTK de teste, extremos `γ = e^±Γ`, `β = ±B`) | `|d| ≤ 1e-4 + 1e-4·|ref|` (`GATE_ABS_TOL/GATE_REL_TOL`, `parity_film.rs:37-38`) |

Pesos treinados não são comparados bit a bit com o upstream (R06 §6.1). Tolerância diferente de 0 em I1-I3 só com
causa demonstrada e registrada; nunca se alarga epsilon (`fixtures/golden/README.md`). O repo entrega os goldens de
P2 (entradas, vetores, saídas, SHA-256) para o Clearcore repetir a comparação.

### 6.4 Variante stateful

Fora da entrega v1: o runtime CPU não a usa e ela não passa pelo gate (R01 §2.3). Registro para depois: o
`make_stateful_onnx.py` só faz `append` de entradas, então funciona sobre grafos com FiLM, mas
`verify_stateful_models` precisa passar `gamma`/`beta` por nome e a ordem final das entradas precisa ser fixada.

## 7. Avaliação

### 7.1 Métricas e protocolo

Só ferramentas de licença aberta como gate: SI-SDR, STOI/ESTOI (`pystoi`, MIT), DNSMOS P.835 e `-p` (repo MIT;
`.onnx` só locais), TSOS (Eskimez 2021, `p=0,3`, `γ=0,1`, STFT 20 ms/10 ms; conferir a equação 3 no PDF antes de
implementar), similaridade de locutor com SpeechBrain ECAPA e WeSpeaker ResNet34 (locais), WER com Whisper (MIT), LSD
acima de 8 kHz. PESQ/POLQA fora; NISQA (pesos NC) só diagnóstico local [F: R06 §1.1].

- Tudo no caminho do runtime (seção 3.5), inclusive o baseline: o DFNet3 upstream rodado com o mesmo gating. A
  calibração do pipeline (STOI 0,944 ± 0,003 no VCTK-DEMAND, R06 §3) também passa por esse caminho.
- Bootstrap emparelhado por falante (10.000 reamostragens), IC 95%.
- **Pré-registro**: `configs/eval/gates-v1.yaml` congela **todos** os níveis de exigência (G0-G11, gates do M1, regra
  da sonda) antes do T1. O dev só calibra **parâmetros de medida** (limiar de VAD, limiar de cosseno no EER dos
  avaliadores, configuração do TSOS), nunca o nível exigido. Mudar um nível exige nova versão do arquivo
  (`gates-v2.yaml`) com justificativa escrita, registrada no relatório.

### 7.2 Gates

**Decisão: adotar G0..G11 de R06 §4**, com estes ajustes (níveis fixos):

| Gate | Classe | Ajuste |
|---|---|---|
| G0 contrato/paridade | B | inclui I1-I3, P1-P2 e o hash do `config.ini` |
| G1 sem perfil | B | estágio C: I1 + I2 (bit-exato; I2 é o antigo G1b, fundido). Estágio F: margens de R06 (ΔSTOI ≥ −0,003; ΔSI-SDR ≥ −0,5 dB; ΔDNSMOS OVRL/SIG ≥ −0,05, BAK ≥ −0,07) e ΔLSD > 8 kHz ≤ +0,5 dB; ΔPESQ removido |
| G2 concorrente | B | ΔSI-SDR ≥ +3 dB com IC > 0 em C3/C4, SIR ≤ 5 dB; ΔDNSMOS-p OVRL ≥ +0,15; WER ≤ 0,85×DFNet3; ΔPESQ removido; reportado por corpus |
| G3 TSOS | B | ≤ 1% / 2% / 3% (C0 / C1-C2 / C4) e ≤ DFNet3 + 0,5 p.p.; também por estágio de gating |
| G4 locutor trocado | B | TRR(15 dB) ≥ 0,90 sexos diferentes, ≥ 0,70 mesmo sexo; separação de modos ≥ 15 dB; com `atten_lim_db` desligado; reportado por corpus |
| G5 voz preservada | B | sem mudança; avaliadores independentes do encoder treinado |
| G6 enrollment robusto | B | sem mudança (5 dB, 6 s: ΔSI-SDRi ≥ −1,5 dB; TAR ≥ 97%) |
| G7 WER sem interferente | M | sem mudança |
| G8 DNS5 dev | M | só se houver disco; senão "não executado" no relatório |
| G9 CPU/latência | B | medido **no Clearcore**; o repo de treino só confirma topologia idêntica |
| G10 estabilidade 30 min | M | executado pelo `tract-check` |
| G11 contrato do enrollment | B | inclui limites em todas as gravações de teste e de borda (seção 6.2), saída `embedding` e N de 2 a 12 s |

Informativo (sem gate): fração de quadros por estágio de gating por cenário; saturação do `tanh` por site; EER de
verificação usando só os 4 vetores FiLM (o perfil é dado biométrico; vai para o model card).

### 7.3 Ablações

- **AB1 (enrollment cru × denoisado)**, R06 §5, roda **depois** da escolha entre C e F, no estágio escolhido. Fator A
  `p ∈ {0; 0,5}` no treino (no F: o T3 principal é `p=0,5`, a rodada extra é `p=0`, 1 seed cada, resultado rotulado
  "indicativo" com margens de decisão dobradas, R06 §3; no C: 3 seeds por nível). Fator B no teste: cru, denoisado pelo
  modelo entregue sem perfil (caminho do produto), limpo. SNR do enrollment {limpo, 20, 10, 5, 0, −5}; duração {3, 6,
  10, 30} s marginal. Regra de decisão de R06 §5. O indicador barato (cosseno do embedding cru/denoisado com o limpo)
  roda logo após o T1.
- **AB2 (sem perfil estável)**, só no estágio F: ramos a partir do checkpoint do T3 a 70% do cronograma; um fator por
  vez: `p_drop ∈ {0,1; 0,2; 0,3}` com `s = 5%`, depois `s ∈ {0; 5; 10}%` com o melhor `p_drop`. Escolhe o menor
  `p_drop` que passa G1, G4 e C8 (enrollment inválido não zera o alvo: TSOS ≤ modo sem perfil + 1 p.p.); se nenhum
  passa G1 e G4 juntos, G1 não é relaxado (R06 §5).
- Ablação de banda 48+24 × +16 kHz (R04 §4.4): só se o estágio F falhar o critério de LSD do G1.

### 7.4 Entrada no Clearcore

O repo entrega `report.json` (gates, ICs, frações de gating, configs, SHA-256 de manifests e assets) e seu SHA-256,
os goldens de P2 e os coeficientes e vetores dourados do resampler. O Clearcore, na fase 5, **verifica** (não
recalcula): contrato do pDFNet3, paridade tract com os goldens (1e-4), subconjunto Rust com as amostras do
`fetch-voice-samples.sh` (SI-SDR, TSOS, identidade; p225/p226 fora do treino), golden de qualidade em `GoldenFixture`
com o escalar "pior margem normalizada ≥ 0", G9 e os requisitos da seção 2.5 (R06 §6.2). Onde guardar o SHA-256 do
relatório no esquema do golden é decisão do plano da fase 5 do Clearcore.

## 8. Estrutura do repositório

```
clearcore-train/
  pyproject.toml, uv.lock               # ambiente train (Python 3.11)
  envs/export/pyproject.toml, uv.lock   # ambiente export (Python 3.10, torch 1.13.1), só estágio F
  third_party/DeepFilterNet/            # v0.5.6 fixado, LICENSE-MIT/APACHE, patches marcados
  configs/{data,encoder,pdfnet3,eval}/  # YAML; eval/gates-v1.yaml congela níveis antes do T1
  manifests/                            # versionado: fontes (URL, licença, SHA-256) e por arquivo
                                        # (caminho, SHA-256, falante, split, SR, banda efetiva)
  resampler/                            # coeficientes 48k→16k v1, SHA-256, vetores dourados
  src/cctrain/
    data/licenses.py   allowlist e recusa
    data/sources.py    download e verificação por fonte
    data/manifest.py   construção e leitura de manifests
    data/splits.py     splits por falante (T1 e T2/T3)
    data/band.py       banda efetiva e max_freq
    data/resample.py   resampler-48k-16k-v1
    data/enroll.py     validação de enrollment (paridade com o Rust) e banco de embeddings
    data/cv_filter.py  filtro de rótulos do client_id
    data/mix.py        receita das misturas
    data/loader.py     IterableDataset + features libdf
    models/fbank.py    front-end tract-safe
    models/ecapa.py    encoder
    models/film.py     gerador
    models/pdfnet3.py  DFNet3 + hook FiLM + gating emulado
    losses/            perda DFN, L_OS, L_gate, destilação
    train/ckpt.py      checkpoints atômicos e retomada
    train/encoder.py, train/pdfnet3.py
    export/dfnet.py, export/film.py, export/enrollment.py, export/pack.py
    eval/runtime_path.py  réplica de apply_stages (paridade com tract-check)
    eval/metrics.py, eval/scenarios.py, eval/stats.py, eval/report.py
  tools/tract-check/                    # Rust, Cargo.lock versionado
  tests/                                # identidade, hash da base, limites, licenças, splits, mix, features,
                                        # resampler, gating, pack
  scripts/                              # CLIs finas
  data/, runs/, delivery/               # ignorados pelo git
  LICENSE-MIT, LICENSE-APACHE, NOTICE
```

Reprodutibilidade (**decisão**): seed global na config, seeds de worker derivadas; misturas de dev/teste
pré-renderizadas de uma lista de seeds e verificadas por SHA-256; manifests com SHA-256 por arquivo; `uv.lock` dos
ambientes e `Cargo.lock`; cada run grava commit, hash da config, versões de torch/onnx/onnxruntime/tract e o
`nvidia-smi -q` inicial. Código sob `MIT OR Apache-2.0`, como o DeepFilterNet.

## 9. Riscos e marcos

### 9.1 Riscos

| Risco | Mitigação |
|---|---|
| Disco insuficiente (~45-52 GB [est.] contra 31 GB) | M0 mede; liberar espaço é decisão do dono; um bruto por vez; FLAC na taxa nativa |
| RAM livre ~4 GB, swap cheio | 4 workers, prefetch 2; janela de treino com apps fechados (dono) |
| Estágio C não separa falantes | é sonda; o F é o caminho planejado |
| Estágio F regride o modo sem perfil | destilação, BatchNorm congelado, G1 com margens fixas e LSD |
| Gating do runtime deixa o interferente passar cru (`lsnr > 30`) ou zera o alvo (`lsnr < −10`) | gating emulado no treino, `L_gate`, alvo de `lsnr` com interferente como ruído, avaliação no caminho do runtime com frações por estágio |
| Encoder fraco com dados só permissivos | train-other-500, aumentação de canal, gates fora do domínio e funcional no M1 |
| Resampler do produto diferente do treino | filtro fixado no contrato, vetores dourados, requisito ao Clearcore |
| Op do front-end ou do pooling do ECAPA não suportada no tract 0.19.16 | protótipo no `tract-check` já no M0 |
| Exporter gera nós com nomes diferentes | estágio C sem reexport; no F, torch 1.13.1 e `add_film` exige 1 produtor por site |
| Rótulos ruidosos do `client_id` | mínimo de 20 clipes e filtro de consistência |
| Vazamento treino/teste | splits por falante comuns a T1-T3, teste de disjunção |
| Ajuste de exigência depois de ver o resultado | `gates-v1.yaml` congelado antes do T1; mudança só por nova versão justificada |
| Throttling térmico (50 W) | log de throttling; checkpoints a cada 30 min |

### 9.2 Marcos

| Marco | Entrega | Saída verificável |
|---|---|---|
| **M0 piloto** | ambientes; ~1 h de dados (VCTK por HTTP Range, LibriSpeech dev-clean, CV 26.0 pt-BR se a decisão 3 existir, os 3 ruídos CC0 do Clearcore, RIR sintéticas); `add_film` sobre o asset aprovado; protótipo do fbank + ECAPA (com pooling) no tract; réplica do gating com paridade; resampler v1 | medidas que substituem os [est.] (fator FLAC por SR, amostras/s, s/passo e VRAM em batch 16/32, RSS, temperatura); `B` fixado pela regra da seção 3.2; I1 e I2 aprovados; go/no-go do orçamento |
| **M1 encoder** | `gates-v1.yaml` congelado; dados do encoder; T1 | gates do M1 (seção 3.1), banco de embeddings, indicador da AB1 |
| **M2 pDFNet3** | T2 sonda (regra pré-registrada); T3; AB2 | decisão de go/no-go registrada; gates no dev; escolha C ou F registrada (com a decisão 4 do dono) |
| **M3 export** | tars reprodutíveis | G0, G11, I1-I3, P1-P2; dois exports seguidos com o mesmo SHA-256 |
| **M4 avaliação** | rodada única no teste; AB1 | `report.json` com G0..G11 |
| **M5 entrega** | `delivery/<asset_id>/`: tar, SHA-256, tamanho, rascunho de proveniência, PROVENANCE, NOTICE, model card, goldens, resampler, relatório | pacote completo para o dono assinar |

## 10. Interface com o subprojeto B (EQ neural)

- O A **não** produz EQ: o `enrollment.onnx` devolve os 4 vetores FiLM e o `embedding`; o EQ é asset separado
  (`neural-eq-asset-v1`, `neural_eq.onnx`) e entra no perfil por fora (`enrollment.rs:293`, R08 §1).
- O B herda do A: allowlist de licenças, manifests, splits por falante, `data/band.py`, `data/resample.py`, as
  features STFT/ERB com paridade libdf, a réplica do gating, o resultado da AB1 e o checkpoint escolhido do pDFNet3
  para fechar a avaliação (até lá, o DFNet3 v1 serve de denoiser, R08 §4).
- Restrição para o B: enrollment a 16 kHz deixa as bandas ERB 25-31 (> 8,5 kHz) inobserváveis (R08 §3.6).
- O B mora no mesmo repo como subpacote `cctrain/eq/` com configs próprias; o contrato do `neural_eq.onnx` é da spec do B.

## 11. Decisões que precisam do dono

Respostas do dono em 2026-10-03 registradas abaixo de cada item (o texto original das opções foi mantido).

1. **Liberar disco**: ≥ ~20 GB além dos 31 GB livres para o conjunto mínimo (~45-52 GB [est.], refeito no M0) ou
   ~80 GB para o completo. O que apagar (ex.: volumes/caches do Docker, `~/.cache/huggingface`) é escolha do dono; o
   repo não roda `docker prune` nem limpezas.
   **Resposta (2026-10-03): PENDENTE de execução pelo dono.** O dono libera ≥ 20 GB por conta própria; nenhum agente apaga nada.
2. **Lista de fontes**: aprovar o manifest de fontes da seção 4.2 além de CV pt e MLS pt (VCTK, TTS-Portuguese,
   CML-TTS, LibriSpeech incluindo train-other-500, DNS noise Freesound CC0, FSD50K CC0/CC BY, RIR sintéticas; no
   completo, LibriTTS-R, HiFi-TTS, SLR28) e o uso local de DEMAND, TAGARELA, DNS5 dev, SpeechBrain ECAPA, WeSpeaker,
   DNSMOS e Whisper só para avaliação e filtragem (nunca no asset).
   **Resposta (2026-10-03): DECIDIDO (APROVADA a lista toda).** Treino: VCTK, TTS-Portuguese, CML-TTS, LibriSpeech incluindo train-other-500, ruído Freesound CC0 do DNS, FSD50K CC0/CC BY, RIR sintéticas. Avaliação local: DEMAND, TAGARELA, DNS5 dev, SpeechBrain, WeSpeaker, DNSMOS, Whisper.
3. **Common Voice**: aceitar os termos do Mozilla Data Collective com a conta do dono; decidir se o `client_id` pode
   agrupar clipes para treinar o encoder e formar pares enrollment/alvo (com o filtro da seção 4.6), registrar a
   interpretação no vault e, se quiser, consultar a Mozilla antes.
   **Resposta (2026-10-03): DECIDIDO (aprovado agrupar por `client_id`) com o filtro da seção 4.6** (≥ 20 clipes + consistência por avaliador independente), sem identificar ninguém. **PENDENTE de execução:** o aceite dos termos do Mozilla Data Collective fica com o dono.
4. **Estágio F × só estágio C**: no F, qualquer peso da base treinado também altera o modo sem perfil; o G1 deixa de
   ser bit-exato e passa a valer a tolerância do G1 (ΔSTOI ≥ −0,003 etc.). O dono aceita isso (caminho planejado) ou
   prefere ficar só no estágio C, com modo sem perfil bit-exato e ganho menor? Nesse caso, entregar o C com níveis de
   G2/G4 abaixo dos de `gates-v1.yaml` exige uma `gates-v2.yaml` aprovada explicitamente pelo dono.
   **Resposta (2026-10-03): DECIDIDO (APROVADO F como caminho planejado, sonda C primeiro).** O modo sem perfil passa a valer a tolerância de G1.
5. **Governança dos pesos retreinados**: `weight_license` e `code_license` dos dois assets (proposta
   original `MIT OR Apache-2.0`; **emenda 2026-10-03: enrollment = CC BY-NC 4.0 + NOTICE, demais `MIT OR Apache-2.0`**), texto final de `conversion_terms`, `legal-review.json` e `approval-manifest.json`; a decisão
   de 2026-09-24 cobre o DFNet3 oficial, não pesos derivados nem o encoder próprio.
   **Resposta (2026-10-03): PENDENTE.** Necessária só no M5.
6. **Assinatura Ed25519** dos manifests (chave atual
   `sha256:cfa7e10c021031f5481775cf32093d41e5aaaad454653f3ed0022769cbbfd3f2` ou nova, com edição da
   `trust-policy.json`), SHA-256/tamanho em `model_registry.rs` e cópia para `vendor/approved/`.
   **Resposta (2026-10-03): PENDENTE.** Necessária só no M5.
7. **Janela de uso da máquina**: com o F planejado, ~60-140 h de GPU no total [est.] (T1, sonda, T3, AB1, AB2) em
   rodadas de até ~35 h no notebook, com Docker, IDE e navegadores fechados para liberar RAM; quando rodar é do dono.
   **Resposta (2026-10-03): PENDENTE.** Necessária durante os treinos.