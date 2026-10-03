# 06 - Protocolo de avaliação do pDFNet3 e do enrollment

Data: 2026-10-03. Escopo: pesquisa web + leitura local, sem instalar nem baixar nada (os PDFs foram lidos pelo
`WebFetch`, que os guarda no scratchpad da sessão). Nada aqui foi executado: não há modelo treinado, e nenhum número
deste documento foi medido por mim.

Legenda de confiança (a mesma dos outros documentos desta pasta):

- **[F]** fato de fonte externa, com link.
- **[L]** fato lido no repositório (caminho citado).
- **[I]** inferência ou proposta minha. Todo limiar numérico marcado [I] é **ponto de partida a calibrar**, não valor
  derivado de norma. Onde o limiar é derivado de números publicados, a aritmética está à vista.

## 0. Resumo executivo

1. **Gates bloqueantes devem usar só ferramentas de licença aberta**: SI-SDR, STOI/ESTOI, DNSMOS P.835 (com a variante
   personalizada `-p`), TSOS (definida abaixo), similaridade de locutor com encoder independente, WER com Whisper (MIT)
   e RTF/latência pelo próprio benchmark do Clearcore. **PESQ e POLQA ficam fora dos gates**: a ITU **retirou** a
   P.862 (PESQ) em 5/01/2024 e a recomenda substituir pela P.863 (POLQA), ambas sujeitas a licença comercial (OPTICOM)
   [F]. PESQ entra só como "referência de literatura" (o paper do DFN3 reporta PESQ 3,17 no VCTK-DEMAND), com a
   licença resolvida antes de qualquer publicação.
2. **Métricas não intrusivas sozinhas enganam em modelo personalizado**: no paper que define o TSOS, o DCCRN sem
   personalização tem DNSMOS 3,50 e WER 54,64% no cenário alvo+interferente+ruído (VCTK), e o pDCCRN tem DNSMOS 3,37 com
   WER 35,75% [F]. Por isso cada gate de qualidade é emparelhado com uma métrica com referência ou com WER.
3. **TSOS** (target speaker over-suppression) vem de Eskimez et al. (Microsoft, 2021): fração de quadros em que o
   erro assimétrico, que só conta o que o modelo removeu do alvo, supera 10% da energia comprimida do alvo (fórmula na
   seção 1.3). Os melhores modelos publicados ficam em 0,3% a 1,2% dos quadros; os sem tratamento específico passam de 5% nos cenários com ruído.
4. **Conjuntos**: DNS Challenge 5 (dev, com enrollment de 30 s, só não intrusivo), VCTK-DEMAND (824 enunciados, 2
   falantes: serve para reproduzir os números do DFN3, **não** para avaliar personalização), misturas sintéticas feitas
   por nós com falantes e ruídos disjuntos do treino (é onde os gates de personalização medem de verdade), e pt-BR com
   Common Voice pt / MLS pt. O TAGARELA (CC BY-NC-SA) entra só em teste local não intrusivo: não tem id de falante e já
   passou por vocoder-denoiser.
5. **Gates principais** (todos relativos ao DFNet3 upstream medido no **mesmo** pipeline): não regressão no modo sem
   locutor (margens da seção 4); ganho com falante concorrente; TSOS máximo de 1% / 2% / 3% (alvo só / alvo+ruído /
   alvo+interferente+ruído); rejeição de locutor trocado; p99 por hop dentro do limite de 7 ms que o repositório já
   usa.
6. **Integração**: o modelo treinado **não** passa nos testes bit-exatos de `parity_film.rs`, que fixam a saída do
   upstream (os pesos mudam por definição). Esses testes continuam valendo para o **runtime** e para o export
   "FiLM identidade sobre pesos upstream". Para o pDFNet3 treinado propõe-se um conjunto paralelo (seção 6): contrato,
   paridade tract versus PyTorch com a mesma tolerância 1e-4, golden de qualidade em `GoldenFixture` e verificação do
   relatório de avaliação do repo de treino.

## 1. Métricas e ferramentas

### 1.1 Tabela de métricas, licenças e papel no protocolo

| Métrica | O que mede | Ferramenta e licença | Papel | Fonte |
|---|---|---|---|---|
| SI-SDR / SI-SDRi | distorção do sinal em relação à referência, invariante a escala | definição pública (Le Roux et al., ICASSP 2019); implementável em poucas linhas (também em Rust puro, compatível com a política do workspace) | **gate** (com referência) | [F] [arXiv 1811.02508](https://arxiv.org/abs/1811.02508) |
| STOI / ESTOI | inteligibilidade prevista | `pystoi`, MIT [F]; STOI de Taal et al. 2011 | **gate** (com referência) | [F] [pystoi](https://github.com/mpariente/pystoi) |
| DNSMOS P.835 (SIG, BAK, OVRL) | qualidade percebida sem referência, treinada em P.835 | `DNSMOS/dnsmos_local.py` do repositório DNS-Challenge; repositório declara **código MIT e dados CC BY 4.0** [F]. A licença específica dos `.onnx` não foi localizada [I]: tratar como a do repositório e confirmar antes de redistribuí-los | **gate** + diagnóstico; correlação com P.835 humano: PCC 0,94 (SIG), 0,98 (BAK), 0,98 (OVRL) no teste do paper [F] | [F] [paper](https://arxiv.org/abs/2110.01763), [repo](https://github.com/microsoft/DNS-Challenge) |
| DNSMOS personalizado (`-p`) | o mesmo, penalizando falante interferente | mesma ferramenta, flag `-p` [F, README] | **gate** nos cenários com interferente | [F] [README DNSMOS](https://github.com/microsoft/DNS-Challenge/tree/master/DNSMOS) |
| DNSMOS P.808 (original, 2021) | MOS geral sem referência | mesmo repositório | diagnóstico (superado pelo P.835) | [F] [arXiv 2010.15258](https://arxiv.org/abs/2010.15258) |
| PESQ (ITU-T P.862 / .2) | qualidade intrusiva de banda estreita/larga | **retirada pela ITU em 5/01/2024**, substituída pela P.863 [F]. Tecnologia protegida por copyright e patentes, licenciada para uso comercial pela OPTICOM [F, fonte secundária 2013]. O pacote Python `pesq` é MIT e só aceita 8 kHz/16 kHz [F], mas a licença MIT do wrapper **não** elimina as patentes do algoritmo [I] | **fora dos gates**; só referência de literatura, após resolver a licença | [F] [ITU P.862](https://www.itu.int/rec/T-REC-P.862), [thread 2013](https://auditory.org/postings/2013/505.html), [ludlows/PESQ](https://github.com/ludlows/PESQ) |
| POLQA (ITU-T P.863) | idem, banda estreita a fullband (20 Hz a 20 kHz, edição 2018) | proprietário (OPTICOM); preço de 2013 citado em lista de e-mail: EUR 8.900 [F, fonte fraca] | **fora**, salvo se o dono do projeto comprar licença | [F] [ITU P.863](https://www.itu.int/ITU-T/Recommendations/rec.aspx?rec=P.863) |
| NISQA | qualidade/naturalidade sem referência | **código MIT, pesos CC BY-NC-SA 4.0** [F] | só diagnóstico local (os pesos são não comerciais; não redistribuir; resultado não vira gate) | [F] [README](https://github.com/gabrielmittag/NISQA) |
| ViSQOL | qualidade intrusiva, modo áudio 48 kHz e modo fala 16 kHz | **Apache-2.0** [F] | diagnóstico alternativo ao PESQ; **não achei validação publicada em speech enhancement personalizado** [I], por isso não é gate | [F] [google/visqol](https://github.com/google/visqol) |
| Similaridade de locutor | cosseno entre embedding da saída e do alvo limpo/enrollment | ECAPA-TDNN do SpeechBrain: Apache-2.0, EER 0,80% no Vox1-test [F]; WeSpeaker ResNet34: pesos "seguem a licença do dataset", CC BY 4.0 para VoxCeleb [F]. O doc `03-speaker-encoder.md` recomenda treinar o encoder do enrollment com dados permissivos: então o avaliador deve ser **outro** encoder [I] | **gate** (relativo) | [F] [pretrained.md WeSpeaker](https://github.com/wenet-e2e/wespeaker/blob/master/docs/pretrained.md) |
| WER / DEL / INS (ASR) | impacto na transcrição; DEL é correlato de sobre-supressão | Whisper, código e pesos MIT [F]; escolher modelo, decodificador e normalizador fixos | **gate secundário** onde há transcrição; opcional | [F] (resultado de busca) |
| WAcc do DNS (1 - WER) | idem, usado no ranking do desafio | API da Azure no desafio; não reproduzível localmente [I] | só referência de literatura | [F] [DNS22](https://arxiv.org/abs/2202.13288) |
| TSOS | sobre-supressão do locutor-alvo | definição pública, implementável (seção 1.3) | **gate** | [F] [Eskimez 2021](https://arxiv.org/abs/2110.09625) |
| RTF, p50/p95/p99 por hop, latência algorítmica | custo em CPU | benchmark do repositório (`run-cpu-baseline-local.sh`, trait `InferenceBackend`) [L] | **gate** | [L] |
| Teste de escuta (P.808/P.835 crowdsourced ou MUSHRA interno) | verdade perceptual | `open-source implementation of P.808` (Naderi e Cutler) [F]; licença do toolkit não conferida | árbitro final antes de release; não gate numérico | [F] [arXiv 2005.08138](https://arxiv.org/abs/2005.08138) |

Duas observações de método com fonte:

- O próprio DNS Challenge afirma que PESQ, SDR e POLQA "não correlacionam bem" com a qualidade subjetiva e por isso o
  ranking é subjetivo (P.835) mais WAcc [F, [DNS22 §4.3](https://arxiv.org/abs/2202.13288)]. Daí a regra: nenhum gate
  de qualidade se apoia numa métrica só.
- O DNSMOS reamostra para 16 kHz [F, resultado de busca sobre o paper P.808]: conteúdo acima de 8 kHz não é avaliado
  [I]. Avaliar a banda alta (modelo a 48 kHz) exige uma métrica espectral à parte, como o LSD acima de 8 kHz que o
  documento `04-datasets.md` já sugere.

### 1.2 Pré-processamento comum (inferência)

- Saída alinhada pelo atraso algorítmico conhecido (1.440 amostras [L, `crates/model/src/lib.rs:22` via spec §2]);
  confirmar por correlação cruzada, como `parity_eq.rs` já faz (`lag_of_max_correlation`) [L].
- Nenhuma normalização de ganho na saída (o DNS proibiu AGC nos clipes avaliados [F, DNS22 §4.4]); o preset de estúdio
  fica em `Off`, que é bit-idêntico [L, spec §4].
- SI-SDR a 48 kHz; STOI e DNSMOS a 16 kHz (reamostragem fixa e registrada).
- Alvo de referência = o mesmo alvo do treino (reverberante ou seco/early). No cenário de reverberação reportar os dois
  [I].

### 1.3 TSOS: definição operacional

**Fato** (Eskimez et al., 2021, seção 4.3, equações 2 e 3, [arXiv 2110.09625](https://arxiv.org/abs/2110.09625)). Com
`S` o espectro do alvo limpo, `Ŝ` o da saída, `p = 0,3` (compressão), `γ = 0,1`:

```
h(x)      = x se x > 0, senão 0
L_OS(t,f) = h( |S(t,f)|^p - |Ŝ(t,f)|^p )^2            (só penaliza o que foi removido do alvo)
TSOS(t)   = 1 se  Σ_f L_OS(t,f) > γ · Σ_f |S(t,f)|^p, senão 0
TSOS(%)   = 100 · média de TSOS(t) sobre os quadros
```

O paper também menciona a duração total e a duração máxima contínua de sobre-supressão, mas só reporta o percentual
médio. No paper seguinte (E3Net, Interspeech 2022), o TSOS é "normalizado" em **segundos de alvo suprimido por meia
hora** [F, [arXiv 2204.00771](https://arxiv.org/abs/2204.00771)].

Extração por `pdftotext`: a leitura do somatório da equação 3 acima vem do texto extraído do PDF e deve ser conferida
contra o paper antes de implementar [I].

**Escolhas de implementação** (inferência, a fixar no código de avaliação):

- STFT de 20 ms com hop de 10 ms (a configuração do DeepFilterNet [F, paper DFN3]); os papers da Microsoft usam outra
  (E3Net: janela 32 ms, hop 16 ms [F]). Reportar a configuração usada; números não são comparáveis entre
  configurações.
- Duas variantes: todos os quadros (como no paper) e só quadros em que o alvo limpo está ativo (VAD de energia no sinal
  de referência). A segunda evita contar quadros quase silenciosos.
- Reportar também `TSOS_s30min = (nº de quadros OS × hop) / duração × 1800 s` e a **maior corrida contínua** de quadros
  OS (informativo: uma queda de 150 ms é audível; o paper não reporta isso).
- **Atenção a unidades**: o paper de 2021 usa percentual de quadros; o de 2022 usa segundos por meia hora. Os
  números da tabela do E3Net (0,13 a 3,75) e os da tabela de 2021 (0,30 a 16,89) **não** são comparáveis entre si [I].
  Os gates abaixo usam só o percentual de 2021.

### 1.4 Vocabulário TAR/TRR do spec (§7, requisito 6)

O spec usa "TAR/TRR por SNR" sem defini-los. Definição proposta [I]:

- **TAR** (target acceptance rate) = `1 - TSOS_quadros` no alvo presente: fração de quadros de alvo preservados.
- **TRR** (target/non-target rejection rate, aqui: *rejection* de falante não cadastrado) = fração de quadros "só
  interferente" (alvo limpo inativo, interferente ativo, sem ruído de fundo) em que a energia de saída fica ao menos
  15 dB abaixo da energia de entrada. O limiar de 15 dB é arbitrário e deve ser varrido (6, 10, 15, 20 dB) no relatório.
- Curva TAR x TRR **sem limiar**: varrer um fator de intensidade `α` do perfil (`γ(α) = 1 + α(γ_perfil − 1)`,
  `β(α) = α β_perfil`, `α ∈ [0; 1,5]`) produz uma curva de operação por condição e permite comparar variantes de treino
  pela área, sem escolher um ponto de operação [I]. Também dá ao produto um candidato a controle de "intensidade".

## 2. Conjuntos de teste

### 2.1 Fontes

| Conjunto | Licença / acesso | Conteúdo | Uso no protocolo | Limitações |
|---|---|---|---|---|
| **DNS Challenge 5 (ICASSP 2023), dev test set** | repositório: código MIT, dados CC BY 4.0, com ressalva de que "os datasets são fornecidos sob os termos originais" de cada fonte [F]. Diretório `V5_dev_testset` com clipes de 10 s e enrollment de 30 s [F, README] | PDNS dev: 1.443 gravações reais (737 fala lida com os ruídos do track 1; 166 com falante vizinho + ruído; 347 com falante vizinho sem ruído; 193 emocionais), 48 kHz, 10 a 20 s [F, DNS22/DNS23 §3.2] | principal teste **não intrusivo e real** de personalização: DNSMOS `-p`, similaridade de locutor, controle de TSOS por proxy | **sem referência limpa e sem transcrição** ("Dev testset was not transcribed" [F, DNS23 §4.3]); o blind set aparece como `<TBD>` no README do master, então não conta com ele [F]; os números do pDFNet2 no blind set não são comparáveis com o dev set |
| **VCTK-DEMAND (Valentini)** | DataShare lista "End-user Licence" (texto no bitstream, **não lido**) [F]; DEMAND: Zenodo traz CC BY 4.0 no campo de licença e CC BY-SA 3.0 na descrição [F] (a mesma divergência que o spec registra) | 824 enunciados, 2 falantes (1 homem, 1 mulher), SNR 2,5/7,5/12,5/17,5 dB, 5 ruídos DEMAND [F, [referência agregada](https://arxiv.org/abs/2012.10732)]; 48 kHz [F] | **reprodução dos números do DFN3** (PESQ 3,17, STOI 0,944) e teste do modo sem locutor | 2 falantes: **não serve para medir personalização** [I]; nenhum falante interferente |
| **Misturas sintéticas próprias** | VCTK 0.92 (CC BY 4.0 [F, doc 04]) para alvo e interferente, falantes **fora do treino**; ruído: DEMAND (reservado para teste, doc 04), os 3 clipes CC0 do `fetch-voice-samples.sh` [L], e a parte de `noise_fullband` não usada no treino | alvo + interferente + ruído + reverberação, com SNR/SIR controlados e referências limpas | **onde os gates de personalização medem de fato** (receita na seção 2.3) | qualidade depende de o split de falantes/ruídos ser disjunto do treino; o doc 04 já alerta para vazamento (item 10) |
| **pt-BR: Common Voice pt e MLS pt** | CC0 (Common Voice) e CC BY 4.0 (MLS); o spec registra ambos como liberados pelo usuário e com confirmação na fonte pendente [L, spec §8] | falas com id de falante (`client_id`; locutor do MLS) e transcrição | misturas sintéticas em pt-BR com falantes **retidos do treino** (split por falante); WER com transcrição | 16 kHz nativos no MLS; Common Voice é microfone variável e ruidoso, não "limpo" (doc 04); o MLS em português mistura variantes [I] |
| **TAGARELA (pt-BR, teste)** | CC BY-NC-SA 4.0 [F, [card HF](https://huggingface.co/datasets/freds0/TAGARELA)] | split de teste com 356 linhas; 16 kHz mono; clipes de 5 a 20 s; ~91% pt-BR e ~9% pt-PT; já passou por **Vocos como denoiser**; diarização e remoção de fala sobreposta [F, card] | **só teste local não intrusivo**, conforme a decisão do usuário registrada no spec §8: sem treino, sem commit, `--with-tagarela` | **sem id de falante utilizável** (o card diz ~13 mil falantes distintos, mas sem rótulo por clipe [F]): não dá para montar pares enrollment/teste do mesmo falante sem clusterizar por embedding [I]; fala já vocoderizada, sem banda acima de 8 kHz; transcrição foi gerada por ASR, então o WER é relativo, não absoluto [I]. Usar para: identidade versus DFNet3 (DNSMOS, não regressão) e como fonte de fala pseudo-limpa em misturas **apenas locais** |

Regras de contaminação (todas [I], mas decorrem do doc 04): os falantes do VCTK-DEMAND (p232, p257), p225 e p226 (os
do script de amostras [L]) ficam **fora** do treino e do ajuste de hiperparâmetros; DEMAND só no teste; nenhum falante
de teste aparece como enrollment de treino; os mesmos falantes de teste usam enrollments sorteados (≥ 3 por falante)
e nunca o mesmo trecho da fala de teste.

### 2.2 Matriz de cenários

Nomes dos cenários seguem a nomenclatura dos papers (pn, ps, psn; TS1/TS2/TS3 [F]).

| Id | Cenário | Perfil | Contra o quê | Métricas principais |
|---|---|---|---|---|
| C0 | alvo limpo sozinho (TS3) | cadastrado | DFNet3 | TSOS, SI-SDR, spk-sim |
| C1 | **ruído estacionário** (ventilador/AC, ruído branco/rosa, chuva) SNR {-5, 0, 5, 10, 15, 20} | cadastrado e sem perfil | DFNet3 | SI-SDR, STOI, DNSMOS, TSOS |
| C2 | **ruído não estacionário** (rua, teclado, cachorro, cozinha, música de fundo) | idem | DFNet3 | idem |
| C3 | **falante concorrente sem ruído** (ps), SIR {-5, 0, 5, 10}, estratos mesmo sexo / sexos diferentes | cadastrado | DFNet3 | SI-SDRi, TRR, spk-sim, WER/DEL |
| C4 | **falante concorrente + ruído** (psn) | cadastrado | DFNet3 | idem, DNSMOS `-p` |
| C5 | **reverberação** (RIR com T60 de 0,1 a 1,2 s, faixa usada no NAPSE [F, arXiv 2303.06811]) sozinha e com C1/C4 | cadastrado e sem perfil | DFNet3 | SI-SDR com os dois alvos, STOI, DNSMOS |
| C6 | **enrollment ruidoso**: SNR do enrollment {limpo, 20, 10, 5, 0, -5}, duração {3, 6, 10, 30} s, com reverberação, com fala de terceiro ao fundo | cadastrado | perfil de enrollment limpo | ΔTAR, ΔTRR, Δ SI-SDRi (ablação AB1) |
| C7 | **locutor trocado** (só o falante B, perfil do falante A) | A | modo sem locutor | TRR, atenuação mediana (gate G4) |
| C8 | **enrollment inválido** (silêncio, ruído puro, música, fala de outro; amostra curta) | "cadastrado" com esse enrollment | modo sem locutor | TSOS do alvo real: o modelo não pode zerar a saída |
| C9 | **troca de falante no meio do clipe** (usuário e outra pessoa alternando) | cadastrado | DFNet3 | TSOS por trecho, tempo de recuperação |
| C10 | **pt-BR** (Common Voice pt / MLS pt) em C1, C3, C4; **enrollment em uma língua e teste em outra** (en e pt-BR) | cadastrado | DFNet3 | idem, WER |
| C11 | **real, não intrusivo**: DNS5 dev (1.443 clipes com enrollment de 30 s); TAGARELA teste (sem perfil) | cadastrado / sem perfil | DFNet3 | DNSMOS, DNSMOS `-p`, spk-sim com o enrollment |
| C12 | **estabilidade de longa duração**: 30 min contínuos, com troca de perfil/sem perfil a cada 60 s | alterna | DFNet3 | sem NaN/inf, deriva de nível < 0,5 dB, ausência de cliques na troca (o runtime já mantém o estado GRU na troca [L, `parity_film.rs`]) |

### 2.3 Receita das misturas sintéticas (inferência com base nos papers)

- Distribuição de SNR de -5 a 35 dB e SIR de -5 a 25 dB, como no teste do pDFNet2 [F, arXiv 2404.08022 §3.2]; **grade
  fixa** (SNR em passos de 5 dB) em vez de sorteio, para poder reportar curvas por SNR.
- Proporção igual de pn, ps e psn (o teste do pDFNet2 gera os três "igualmente" [F]).
- Alvo a 0 a 1,3 m do microfone e interferente a mais de 2 m, como no treino de Eskimez [F, §4.1]: o alvo é mais
  próximo. Para stress, incluir o caso "interferente mais alto que o alvo" (SIR negativo).
- Enrollment do teste: ≥ 30 s por falante retido, sorteado em trechos de 6 a 10 s (o formato que o produto aceita [L,
  `ENROLLMENT_MIN/MAX_DURATION_SECS` = 6/12 s]) **depois de passar por `validate_enrollment_audio`** (pico ≤ 0,99, RMS
  > -40 dBFS, fala ativa ≥ 60% [L, `crates/model/src/enrollment.rs`]), para avaliar o que o produto realmente aceita.
  Os papers usam enrollments bem mais longos (30 amostras por falante no VCTK e ~60 s no ICR [F, Eskimez §4.2]; 30 s no
  DNS5 e 2,5 min no DNS4 [F, DNS22/23]): reportar o enrollment de 6 a 10 s e o de 30 s.
- Tamanho mínimo: ≥ 40 falantes-alvo de teste, ≥ 300 misturas por célula (cenário x SNR), com intervalo de confiança
  por **bootstrap sobre falantes** (clipes do mesmo falante são correlacionados) [I].

## 3. Protocolo estatístico e baseline

1. **Baseline único e interno**: o DFNet3 upstream (o asset aprovado `df-compatible-release-asset-v1`, DFN3 v0.5.6
   [L]) é rodado pelo **mesmo** pipeline, nos mesmos arquivos e sementes. Os números publicados do DFN3 (PESQ 3,17,
   CSIG 4,34, CBAK 3,61, COVL 3,77, STOI 0,944 no VCTK-DEMAND [F, [arXiv 2305.08227](https://arxiv.org/abs/2305.08227)])
   só servem para **calibrar o pipeline**: se a nossa medição do upstream não reproduzir os valores publicados em
   STOI (±0,003) e em DNSMOS, o pipeline é corrigido antes de qualquer comparação [I]. O paper do DFN3 não publica SI-SDR
   nem DNSMOS para o VCTK-DEMAND, então esses dois só podem ser medidos, não comparados com a literatura [F].
2. **Não inferioridade e superioridade**: cada gate é um teste de hipótese emparelhado (mesma mistura passa pelos dois
   modelos). Não inferioridade: o limite inferior do IC 95% (bootstrap emparelhado, 10.000 reamostragens por falante)
   da diferença ≥ -margem. Superioridade: limite inferior ≥ limiar [I].
3. **Pré-registro**: limiares são congelados antes da primeira rodada no conjunto de teste final; ajustes só no dev.
   O README dos goldens do repositório já exige isso na prática: o golden "nunca é reparado alargando um epsilon de
   runtime" [L, `fixtures/golden/README.md`].
4. **Variância de treino**: as comparações entre variantes treinadas (ablações) usam ≥ 3 sementes de treino. Se só
   couber uma, o resultado é rotulado "indicativo" e a margem de decisão dobra [I].
5. **Pipeline Rust como segunda fonte**: um subconjunto fixo (as amostras do `fetch-voice-samples.sh` + misturas
   determinísticas) é avaliado pela saída do **tract no Clearcore**, não só pelo PyTorch/ONNX Runtime do repo de treino,
   e as métricas devem coincidir (seção 6) [I].

## 4. Critérios de aceite propostos

Classe **B** = bloqueante para a fase 5 (assets reais). Classe **M** = meta: reporta e, se falhar, exige justificativa
escrita. Todos os limiares marcados [I] são propostas a calibrar no dev (seção 3, item 3); a coluna "Base" mostra de
onde cada número sai.

| Id | Cl. | Condição | Critério | Base |
|---|---|---|---|---|
| **G0** | B | export e runtime | contrato 960/480/32/96/5/2 inalterado; entradas `gamma`/`beta` com `FILM_HIDDEN_DIM = 256`; saída finita e com estado em 1.000 quadros; tract versus PyTorch/ORT com tolerância absoluta 1e-4 e relativa 1e-4 (a do gate do repositório); vetores extremos dentro de [0,001; 100] sem NaN/inf | [L] `GATE_ABS_TOL/GATE_REL_TOL` em `parity_film.rs`; `MIN_GAMMA/MAX_GAMMA` em `voice_profile.rs`; contrato no spec §2 |
| **G1** | B | **modo sem locutor** (FiLM neutro γ=1, β=0) versus DFNet3 upstream em VCTK-DEMAND + C1, C2, C5 sem perfil + C11/TAGARELA | não inferioridade com margens: **ΔSTOI ≥ -0,003**; **ΔSI-SDR ≥ -0,5 dB**; **ΔDNSMOS OVRL ≥ -0,05**, **SIG ≥ -0,05**, **BAK ≥ -0,07**; Δ(PESQ de referência, se licenciado) ≥ -0,05 | STOI: o DFN, o DFN2 e o DFN3 vão de 0,942 a 0,944 [F], então qualquer queda maior que 0,003 apaga toda a progressão [I]. PESQ: o DFN2 → DFN3 foi +0,09 (3,08 → 3,17), a margem é ~metade [F, I]. CBAK 3,40 → 3,61 e CSIG 4,30 → 4,34 no mesmo salto [F]. SI-SDR e DNSMOS: **sem número publicado do DFN3**; margens por julgamento [I]; no E3Net os DNSMOS dos modelos comparados em TS1 vão de 3,22 a 3,48 [F, tabela 1], uma dispersão de 0,26, e a margem de 0,05 é uma fração dela |
| **G1b** | B | **identidade bit-exata do upstream**, só no export "FiLM em identidade sobre pesos upstream" (requisito 5 do spec) | `max_abs_diff == 0` e SHA-256 do golden de 1.000 quadros (`c041ee1f...`), ou dentro de 1e-4 | [L] casos B/C/D de `parity_film.rs` já fazem isso; **não se aplica aos pesos fine-tunados** |
| **G2** | B | **ganho com falante concorrente**, perfil correto, C3 e C4 em SIR ≤ 5 dB, enrollment de 6 a 10 s | **ΔSI-SDR (pDFNet3 - DFNet3) ≥ +3 dB** (provisório) e **limite inferior do IC > 0**; **ΔDNSMOS `-p` OVRL ≥ +0,15** (e ΔPESQ ≥ +0,25, só se a licença do PESQ for resolvida); WER ≤ 0,85 × WER do DFNet3 **e** ≤ WER do sinal ruidoso | pDFNet2 sobre DFN2 no teste sintético com pn/ps/psn em partes iguais: PESQ 2,10 → 2,36 (+0,26), STOI 0,75 → 0,78, CSIG 3,11 → 3,66 (+0,55) [F, arXiv 2404.08022 tabela 1]. Na subcategoria pn o PESQ é igual [F, fig. 2]; então o ganho nas duas categorias com interferente é ≈ 0,26 × 3/2 ≈ **+0,39** [I, aritmética]; o limiar +0,25 fica abaixo disso. Sobre WER: o pDCCRN reduz 35% relativo ao DCCRN no cenário completo (54,64 → 35,75) [F, Eskimez tabela 1], e o E3Net 12% relativo ao ruidoso (43,03 → 37,94) [F]. SI-SDR +3 dB: **sem referência publicada**, a calibrar no dev |
| **G3** | B | **TSOS**, perfil correto, enrollment válido, limiar γ=0,1/p=0,3 | **TS3 (C0) ≤ 1,0%; TS2 (C1/C2 com alvo) ≤ 2,0%; TS1 (C4) ≤ 3,0%**, e cada um ≤ TSOS do DFNet3 upstream + 0,5 p.p. | melhores modelos publicados: pDCATTUNET com perda assimétrica + multi-tarefa 1,20/0,90/0,69% (ICR) e 1,07/0,88/0,30% (VCTK); pDCCRN equivalente 3,13/2,48/1,64% e 2,93/2,69/1,12%; pDCCRN sem tratamento 14,51/13,55/4,54% (ICR) e 16,89/9,80/4,86% (VCTK); DCCRN sem personalização 9,01/7,32/1,47% e 5,13/5,33/2,42% [F, Eskimez tabela 1]. Os limiares 1/2/3% ficam a 1,5x a 3x do melhor resultado publicado (pDCATTUNET, 0,3% a 1,2%); o melhor pDCCRN falharia em 5 das 6 células, e os modelos sem perda assimétrica ficam bem acima [I]. **Risco específico do DFN3** [I]: o decodificador de ganho é desligado quando o SNR local previsto é menor que -10 dB e o DF quando é maior que 20 dB [F, paper DFN3]: logo, mede-se TSOS também por faixa de `lsnr` |
| **G3b** | M | maior corrida contínua de quadros OS | informativo; sugestão: p99 ≤ 300 ms | [I] |
| **G4** | B | **rejeição de locutor trocado** (C7): só o falante B, perfil de A | TRR(15 dB) ≥ 0,90 com sexos diferentes e ≥ 0,70 com mesmo sexo; **separação de modos**: atenuação mediana de B sob o perfil de A menos a atenuação sob o modo sem locutor ≥ 15 dB | o spec manda treinar "alvo = silêncio com locutor trocado" (§7, req. 1) [L]. Nenhum número publicado: **DNS5 relata que vizinhos de mesmo sexo/sotaque/volume foram os clipes mais difíceis para transcrever** [F, DNS23 §4.3]; limiares por julgamento [I] |
| **G5** | B | **preservação da voz** (sem interferente) | spk-sim(saída, alvo limpo) ≥ spk-sim do DFNet3 - 0,02 (limite inferior do IC), com **dois encoders independentes** do encoder do treino; taxa de aceitação na verificação de locutor, com limiar fixado no EER dos dados limpos, não pior que a do DFNet3 | [I]; os encoders: SpeechBrain ECAPA (Apache-2.0) e WeSpeaker ResNet34 [F]. Limiar de cosseno **não** fixado a priori: calibrar no EER local (fonte agregadora cita ~0,25 para o ECAPA do SpeechBrain, **não verificado**) |
| **G5b** | M | com interferente: ganho de spk-sim | Δ spk-sim (pDFNet3 - DFNet3) ≥ +0,05 | [I] |
| **G6** | B | **robustez ao enrollment**, tratamento escolhido na AB1 | com enrollment a 5 dB de SNR e 6 s: **ΔSI-SDRi ≥ -1,5 dB** versus o enrollment limpo de 30 s, e TAR ≥ 97% | [I]; não há referência publicada para enrollment de 6 s com ruído. Contexto: o DNS5 limpou o enrollment (5 s) com modelo não causal apenas para os **avaliadores humanos** [F, DNS23 §4.2.1], o que mostra que enrollment sujo é um problema reconhecido, mas não evidência a favor do denoise para o modelo |
| **G7** | M | **WER** (Whisper, normalizador fixo), sem interferente | WER(pDFNet3) ≤ WER(DFNet3) + 1,0 p.p. (limite superior do IC) | o desafio reportou que **nenhum time melhorou SIG** e que o WAcc ficou **2% pior que o ruidoso** no track personalizado do ICASSP 2022 [F, DNS22 §4.4]; no ICASSP 2023, a maioria perdeu SIG [F, DNS23 §6] e o vencedor teve WAcc 0,758 contra 0,843 do ruidoso no headset [F, arXiv 2303.06811 tabela 3, valores como extraídos do PDF]. Então o gate é "não piorar versus DFNet3", não "melhorar o ruidoso" |
| **G8** | M | **DNS5 dev, não intrusivo** (C11) | DNSMOS `-p` OVRL ≥ DFNet3 (pn) e ≥ DFNet3 + 0,15 (ps/psn); queda de SIG versus o ruidoso ≤ 0,5 | o pDFNet2 no blind set do DNS5, headset: SIG 4,15 → 3,69 (-0,46), BAK 2,37 → 3,51, OVRL 2,71 → 3,04 (+0,33); TEA-PSE 3.0: 4,11/4,05/3,65 [F, arXiv 2404.08022 tabela 2]. **Conjuntos diferentes (blind versus dev): só indicativo** |
| **G9** | B | **CPU/latência**, perfil de CPU mínimo do produto | p99 por hop ≤ 7 ms; p95 de ponta a ponta ≤ 80 ms; latência algorítmica igual a 1.440 amostras; custo do FiLM vs. DFNet3 ≤ +5% no p50 | os limites p99 ≤ 7 ms e p95 ≤ 80 ms são os do GA do repositório [L, `docs/release-gates.md`]. O spike mediu p50 656 µs, p99 0,74 ms e +0,1% de custo do FiLM [L, spec §10]. O DFN3 original reporta RTF 0,19 em thread única no i5-8250U [F, paper DFN3]; o pDFNet2 com ECAPA externo tem custo de inferência igual ao DFN2 (RTF 0,03, i7-10870H) quando o embedding é concatenado na entrada do encoder unificado [F]. O modelo de enrollment roda fora do caminho em tempo real: proposta de teto de 1 s para 10 s de áudio [I] |
| **G10** | M | **estabilidade** (C12) | 30 min sem NaN/inf; deriva de nível < 0,5 dB; troca de perfil sem descontinuidade acima de 2× o desvio local | [I]; o runtime já testa a troca com GRU preservado [L] |
| **G11** | B | **enrollment: contrato** | `audio [1,N]` f32 16 kHz → 4 saídas f32 `[256]`; mesma recusa de áudio fora do contrato do `SpeakerEnrollmentEngine`; gravação apagada após extração | [L] `enrollment.rs`, `tools/accelerators/gen_enrollment_test_onnx.py` |

Notas de leitura dos gates:

- **G1 e G2 são complementares**: G1 impede que a personalização estrague o uso sem perfil (que é o caminho do
  produto sem cadastro e o fallback do enrollment inválido); G2 prova que o perfil serve para alguma coisa.
- **G3 é o gate que o produto mais sente**: o usuário percebe um corte de voz própria como falha grave (o paper
  chama TSOS de "crítico" para reuniões online [F]).
- Os limiares de G2, G4, G6 e G10 não têm base publicada direta: são calibrados no dev e congelados. A tabela existe
  para que haja número antes da primeira rodada, não para parecer norma.

## 5. Ablações exigidas na seção 7 do spec

### AB1. Enrollment cru versus denoisado, por SNR (requisitos 2 e 6)

**Pergunta**: o enrollment passado pelo próprio pDFNet3 em modo sem locutor (spec §6) produz perfil melhor que o
enrollment cru, e a aumentação com p≈0,5 evita o descompasso de treino?

**Fatores**:

| Fator | Níveis |
|---|---|
| A. aumentação no treino (probabilidade de usar enrollment denoisado) | p = 0 (só cru) e p = 0,5 (spec). Sensibilidade opcional: p = 1 |
| B. tratamento do enrollment no teste | limpo (limite superior), cru, denoisado pelo **pDFNet3 sem locutor** (caminho do produto), denoisado pelo DFNet3 upstream (alternativa barata), denoisado por modelo maior não causal (limite do "denoise perfeito") |
| C. SNR do enrollment | limpo, 20, 10, 5, 0, -5 dB; ruído estacionário e não estacionário; com e sem reverberação |
| D. duração | 3, 6, 10, 30 s (varredura marginal em SNR = 10 dB) |

Delineamento: A x B x C completo com duração fixa de 8 s; D como varredura marginal. A mesma lista de falantes,
enrollments e misturas de teste em todas as células (comparação emparelhada); ≥ 3 sementes por nível de A.

**Medidas por célula** (todas por falante, depois agregadas): TAR, TRR (limiares 6/10/15/20 dB), curva TAR x TRR por
varredura de α (seção 1.4), ΔSI-SDRi, spk-sim e, **sem treino nenhum**, um indicador antecipado de fidelidade do
enrollment: cosseno entre o embedding do enrollment (cru ou denoisado) e o do enrollment limpo do mesmo falante, com
os dois encoders independentes. Esse indicador prevê o resultado e é barato, então roda antes do treino do pDFNet3.

**Hipóteses a refutar** (do spec): (H1) com p = 0 o enrollment denoisado **piora** por descompasso; (H2) com p = 0,5 o
denoisado ganha em SNR baixo; (H3) em enrollment limpo o denoise não prejudica.

**Regra de decisão para a UI** [I]:

- usar enrollment **denoisado** se, para SNR de enrollment ≤ 10 dB, ΔTRR ≥ +3 p.p. e ΔTAR ≥ -0,5 p.p. (limite
  inferior do IC 95% de cada diferença emparelhada), **e** em SNR ≥ 20 dB ou limpo ΔTAR ≥ -0,5 p.p. e ΔTRR ≥ -1 p.p.;
- caso contrário, o spec já prevê o fallback: áudio cru mais a validação (§6);
- se o ganho aparece só abaixo de um SNR de enrollment, a UI pode condicionar o denoise ao SNR estimado pelo `lsnr`
  (que o fluxo de enrollment já calcula [L, spec §6]).

### AB2. Modo "sem locutor" estável (requisito 1)

**Pergunta**: a taxa de *embedding dropout* e o alvo "silêncio com locutor trocado" mantêm um modo sem locutor
equivalente ao DFNet3 **sem** perder a rejeição quando há perfil?

- Variáveis: taxa de dropout do embedding no treino (por exemplo 0,1, 0,2, 0,3); forma do "sem locutor" na
  inferência: FiLM neutro puro (γ=1, β=0, o que `FiLMVectors::identity()` entrega [L]) versus vetor nulo aprendido.
  O que o runtime tem hoje é a primeira; se o treino usa a segunda, o treino precisa mapear a identidade do runtime
  para o "sem locutor", ponto a confirmar no repo de treino [I].
- Medidas: G1 (não regressão) para cada dropout; G4 (rejeição); **separação de modos** (atenuação de B com perfil de A
  menos a atenuação de B sem perfil; deve ficar ≥ 15 dB); e C8 (enrollments inválidos: o modelo **não** pode zerar o
  alvo real; critério: TSOS com enrollment inválido ≤ TSOS no modo sem locutor + 1 p.p.).
- Decisão: menor dropout que satisfaz G1 e G4 simultaneamente; se nenhuma satisfaz os dois, **não** relaxar G1 (o
  produto "nunca fica pior do que é hoje" [L, spec §1]) e investir no mecanismo de modo (por exemplo, duas cabeças).

### AB3. Aplicação do EQ neural (requisito 6, "A/B de aplicação do EQ")

**Variantes** (spec §7): V0 sem EQ (ganhos 0 dB); V1 filtro de fase mínima no domínio do tempo (padrão do runtime, sem
latência); V2 STFT próprio com overlap-add (+10 ms); V3 cabeça de EQ dentro do pDFNet3, aplicada ao espectro do libDF
antes do iSTFT (lag 0).

**Material**: fala limpa de 48 kHz (VCTK, falantes de teste) colorida por curvas de microfone sintéticas **dentro do
contrato** [-6, +12] dB por banda; o alvo é o original. Isso dá a resposta correta conhecida [I].

**Objetivo**:

- erro de ganho por banda ERB e distância espectral logarítmica contra o alvo (a banda acima de 8 kHz incluída);
- artefatos: ondulação em banda passante, desvio de atraso de grupo, pré-eco/espalhamento de transientes (impulso e
  fala com impulso, como `parity_eq.rs` já faz [L]), descontinuidade na troca de ganhos;
- latência algorítmica por correlação cruzada: os testes `parity_eq.rs` provam lag 0 e exatidão em dB para V3 [L];
  V1 e V2 precisam do mesmo teste;
- DNSMOS SIG/OVRL: não pior que V0 - 0,03.

**Subjetivo** (o árbitro): teste cego com ≥ 12 ouvintes e ≥ 20 itens (ABX ou preferência pareada para V1 versus V2 e
V1 versus V3; MUSHRA se forem comparadas mais de duas variantes). A regra do spec ("o STFT próprio só entra se ganhar
de forma clara no A/B cego e depois de medida a latência fim a fim" [L]) vira: V2 só vence se a preferência ≥ 65% com o
limite inferior do IC de Wilson a 95% > 50% **e** a latência fim a fim medida mostra que os +10 ms cabem no orçamento
de 70 ms do pior caso [L, spec §2]. O limiar de 65% é meu [I].

### AB4. (sugestão minha) Origem do alvo e "enrollment em língua diferente"

Duas ablações baratas que decorrem de C10 e que o spec não pede, mas que o produto vai encontrar: (a) enrollment em
inglês, teste em português e o inverso; (b) enrollment com duração de 3 s (o usuário que interrompe a gravação). Medem
G2/G3 por célula; sem gate novo [I].

## 6. Integração com os testes do Clearcore

### 6.1 O que cada teste existente verifica (leitura de `crates/model/tests/`)

| Teste | O que verifica | Vale para o pDFNet3 treinado? |
|---|---|---|
| `parity_film.rs` (casos B, C, D, E, G; `set_film` recusado em modelo sem FiLM; dimensões erradas recusadas com perfil anterior mantido; troca de FiLM no meio do fluxo preserva o GRU) | o fork vendorizado do libDF contra a saída congelada do upstream sem patch: SHA-256 `c041ee1f...` de 1.000 quadros de um sinal sintético, ou, se a CPU diferir, tolerância 1e-4 absoluta + 1e-4 relativa. Casos C e D exigem bit-exatidão com FiLM em identidade; caso E exige sensibilidade ≥ 0,05 com γ=1,5; caso G compara FiLM em runtime com constantes embutidas no grafo | **Parcialmente.** Os casos de bit-exatidão provam o **runtime**, com assets gerados por `tools/accelerators/gen_film_onnx.py` **sobre os pesos upstream**. Os pesos fine-tunados **nunca** baterão o golden do upstream, e isso é correto. Os testes de recusa de dimensão e de troca no meio do fluxo valem para qualquer asset com FiLM |
| `golden_reference.rs` + `src/golden.rs` | esquema `GoldenFixture`/`GoldenProvenance`: ordem de casos, SHA de entrada/saída, descritor do backend, **`quality_metric`, `quality_metric_version`, `quality_threshold`, `quality_observed_value` (≥ limiar), tolerância absoluta/relativa**; ausência do arquivo = `GoldenPending` | **Sim, é o ponto de encaixe do golden do pDFNet3** (6.2). Hoje `fixtures/golden/frozen-reference.json` não existe e o estado é `BLOCKED_PENDING_GOLDEN` [L] |
| `tract_smoke.rs` | asset aprovado processa quadro finito, sem ser silencioso e com estado entre quadros; latência algorítmica constante | **Sim**, repetir com o pDFNet3 com e sem perfil |
| `asset_gate.rs`, `m0_adversarial.rs` | verificação do manifest, assinatura Ed25519, política de confiança, JSON malformado, symlinks, substituição do arquivo após a verificação | **Sim**, via `ModelAssetRegistry` quando houver descritores com SHA-256, tamanho e assinatura. Hoje `pdfnet3-release-asset-v1`, `voice-enrollment-asset-v1` e `neural-eq-asset-v1` **não têm** pin nem registro em `governance/` [L, plano da fase 4]; e as assinaturas dependem da chave do usuário (o agente não assina [L, spec §7]) |
| `dsp_pipeline_parity.rs` | o `DspPipeline` desacoplado (STFT, ERB, DF, iSTFT sem rede) contra o `DfTract` monolítico | **Condicional** [I]: só se algum backend (por exemplo, OpenVINO [L, log do git: "run DeepFilterNet3 on OpenVINO"]) consumir o pDFNet3 pelo `DspPipeline`; nesse caso, estender `raw_prediction` com `gamma/beta` e repetir a paridade |
| `parity_eq.rs` | gancho de EQ espectral: vazio é bit-exato; +6 dB plano é exato em todos os estímulos; +12 dB só nos agudos; lag 0; comprimento errado recusado | **Sim para a variante V3** da AB3. Também define o contrato dB que a saída do EQ neural precisa respeitar ([-6, +12] dB [L]) |
| `studio_contract.rs` | só consistência de constantes (hop e taxa) entre `studio-dsp` e `contracts` | irrelevante |
| `crates/model/src/enrollment.rs` + `fixtures/enrollment-contract-test.onnx` (sintético) | contrato do enrollment: 16 kHz, 6 a 12 s, pico, nível, fração de fala; 4 vetores de 256; gravação apagada | **Sim**: o ONNX de enrollment real precisa passar o mesmo contrato |

### 6.2 O que adicionar (proposta)

**Pergunta direta: o modelo exportado passa pelos testes do repositório?** Passa pelos de contrato, fumaça e
verificação de asset (se o export obedecer ao contrato). **Não** passa, nem deve passar, pelos casos de bit-exatidão de
`parity_film.rs`, porque eles medem a identidade de **pesos upstream**. A garantia equivalente para os pesos
treinados tem outra forma:

1. **`pdfnet3_contract.rs`** (novo): carrega o asset pelo backend personalizado; confere 4 entradas em `enc` e `df_dec`
   (`gamma`, `beta` `[n_ch, S, 256]`), `film_hidden() == Some(256)`, 960/480/32/96/5/2, recusa de FiLM errado, troca
   no meio do fluxo, saída finita, com estado e não silenciosa com e sem perfil.
2. **Paridade de export (G0)**: o repo de treino publica, junto com o asset, um **golden de referência gerado pelo
   modelo PyTorch** (entrada sintética determinística de 1.000 quadros mais 3 vetores FiLM: identidade, perfil de
   exemplo e extremos), com SHA-256; o teste do Clearcore compara a saída do tract com tolerância 1e-4/1e-4. É o análogo
   direto de `assert_matches_upstream`, mas contra o PyTorch do treino. O mesmo vale para o ONNX de enrollment e para o
   EQ neural. Em qualquer backend alternativo (OpenVINO, TensorRT), repetir contra o tract [I].
3. **Golden de qualidade em `GoldenFixture`**: um `frozen-reference` por descritor, com `quality_metric` =
   "pdfnet3-gate-v1" (versão deste protocolo) e **um escalar** para `quality_threshold`/`quality_observed_value`: a
   **pior margem normalizada entre os gates bloqueantes** (≥ 0 aprova). A validação existente (`observed ≥ threshold`) já
   recusa quando falha [L]. O relatório completo (JSON) fica no repo de treino e seu SHA-256 entra na proveniência do
   golden. O esquema atual não tem campo para esse hash: usar `candidate_record_sha256` ou subir `schema_version`; **decisão
   de design pendente** [I].
4. **Subconjunto de qualidade medido pelo tract** (`pdfnet3_quality.rs`, **pula** sem os arquivos, como manda o spec §8
   [L]): com as amostras do `scripts/fetch-voice-samples.sh` (VCTK p225/p226, 3 ruídos CC0), misturas determinísticas
   e métricas puras em Rust (SI-SDR, TSOS, razão de energia, identidade): confere G1b, G2 (SI-SDRi vs. DFNet3), G3 e G4
   no caminho de produção. DNSMOS, encoder de locutor e Whisper **não entram** no Cargo (política: sem dependências
   externas, offline); ficam no relatório do repo de treino. Para isso, p225 e p226 precisam ficar fora do treino [I].
5. **Verificação do relatório, não recálculo**: o CI do Clearcore (`cargo test --locked --offline`,
   `scripts/check-offline.sh`) não roda ASR nem DNSMOS. Ele **confere** que o relatório assinado referenciado pelo
   golden tem todos os gates bloqueantes ≥ limiar e que o SHA-256 bate.
6. **`docs/release-gates.md`**: acrescentar uma linha (M-gate) "pDFNet3: G0 a G11 aprovados, relatório X". A linha M1
   atual (golden congelado + manifest do corpus) é o modelo.
7. **Privacidade** (já prevista no spec §6): teste de que os 4 vetores e o perfil não vazam no export de diagnósticos e
   de que a gravação é apagada. Avaliação adicional sugerida: **vínculo de identidade**, ou seja, se os 4 vetores
   de 256 permitem verificar o locutor (são dado biométrico) [I]; reportar o EER de verificação a partir só dos vetores.

### 6.3 Divisão de trabalho entre os repositórios (proposta)

| Onde | O quê |
|---|---|
| repo de treino | implementação das métricas (SI-SDR, STOI, DNSMOS, TSOS, spk-sim, WER), geração das misturas, ablações AB1 a AB3, relatório JSON assinado, golden de paridade |
| Clearcore | contrato, paridade tract versus PyTorch, subconjunto Rust (SI-SDR/TSOS/identidade), verificação do relatório, benchmark de CPU (G9) |

## 7. Referências numéricas publicadas (tabela de apoio)

| Fato | Valor | Fonte |
|---|---|---|
| DFN3 no VCTK-DEMAND | PESQ 3,17; CSIG 4,34; CBAK 3,61; COVL 3,77; STOI 0,944 (DFN2: 3,08/4,30/3,40/3,70/0,943; DFN: 2,81/4,14/3,31/3,46/0,942) | [arXiv 2305.08227](https://arxiv.org/abs/2305.08227) |
| DFN3 treino e custo | DNS4 multilíngue completo, PTDB e VCTK sobreamostrados 10x; RTF 0,19 em thread única no i5-8250U; latência de 40 ms no paper (o Clearcore usa 30 ms [L]); desativa decodificadores por SNR local (< -10 dB: silêncio; > 20 dB: sem DF) | [arXiv 2305.08227](https://arxiv.org/abs/2305.08227) |
| TSOS (2021) | pDCCRN sem tratamento 14,51%/13,55%/4,54% (ICR TS1/TS2/TS3); com perda assimétrica + multi-tarefa 3,13/2,48/1,64%; pDCATTUNET assimétrico + MT 1,20/0,90/0,69%; VCTK longo: 1,07/0,88/0,30% | [arXiv 2110.09625](https://arxiv.org/abs/2110.09625) |
| WER versus DNSMOS | VCTK TS1: ruidoso WER 43,03 / DNSMOS 2,92; DCCRN 54,64 / 3,50; pDCCRN 35,75 / 3,37 | idem |
| E3Net (TSOS em s/30 min, TS1/TS2/TS3) | pDCCRN-baseline 14,61/10,46/4,20; E3Net-baseline 3,75/1,83/3,32; E3Net-student com KD e multi-tarefa 0,50/0,27/0,13 | [arXiv 2204.00771](https://arxiv.org/abs/2204.00771) |
| pDeepFilterNet2 (ECAPA, VoxCeleb2) | teste sintético: PESQ 2,10 → 2,36; STOI 0,75 → 0,78; CSIG 3,11 → 3,66; RTF e MACs iguais ao DFN2 com embedding concatenado no encoder unificado; no DNS5 blind (headset) PDNSMOS SIG/BAK/OVRL 3,69/3,51/3,04 contra o ruidoso 4,15/2,37/2,71 | [arXiv 2404.08022](https://arxiv.org/abs/2404.08022) |
| DNS Challenge ICASSP 2022 (PDNS) | métrica final = 0,5 [WAcc + 0,25 (OVRL - 1)]; ninguém melhorou SIG e o WAcc ficou 2% pior que o ruidoso; PCC DNSMOS versus P.835 humano 0,92 a 0,96 por modelo | [arXiv 2202.13288](https://arxiv.org/abs/2202.13288) |
| DNS Challenge ICASSP 2023 (PDNS) | headset e speakerphone; enrollment de 10 a 30 s; 5 s de enrollment limpo (por modelo não causal) para os avaliadores; degradação de SIG na maioria dos modelos | [arXiv 2303.11510](https://arxiv.org/abs/2303.11510) |
| NAPSE (TEA-PSE 2.0 e ResNet34) | headset: SIG 3,58, BAK 2,87, OVRL 2,69, WAcc 0,758 (ruidoso: SIG 3,76, WAcc 0,843); 12,49 M parâmetros | [arXiv 2303.06811](https://arxiv.org/abs/2303.06811) |

## 8. O que não consegui verificar, e o que fica para depois

- **Fórmula exata do TSOS**: li o texto extraído do PDF; antes de implementar, conferir a equação 3 na fonte
  original (em particular se o lado direito leva potência `p` e o somatório sobre `f`).
- **Licença do PESQ/POLQA**: a fonte para licenciamento comercial é uma thread de e-mails de 2013 (e a página ITU não
  traz declaração de patente na página principal da P.862). A retirada da P.862 em 5/01/2024 vem da página oficial da ITU
  [F]. Quem for decidir usar PESQ em número publicado deve consultar a OPTICOM/ITU; **não** tratar a licença MIT do
  pacote Python como autorização.
- **Licença dos `.onnx` do DNSMOS**: o repositório do DNS-Challenge declara MIT para código e CC BY 4.0 para dados; a
  licença específica dos modelos ficou sem localizar.
- **Licença do VCTK-DEMAND**: a DataShare mostra "End-user Licence"; o texto não foi aberto. O doc 04 trata o VCTK 0.92
  como CC BY 4.0 e o DEMAND como CC BY-SA 3.0, e recomenda reservar o DEMAND para teste: esta avaliação segue isso.
- **Blind set do DNS5** não está no `README` do master (`<TBD>`); só o dev set tem enrollment e está disponível.
- **Valores de DNSMOS e SI-SDR do DFNet3**: não há publicados; a calibração é medir o upstream (seção 3, item 1).
- **Valor do limiar de cosseno** do ECAPA para verificação: só fonte agregadora; calibrar no EER local.
- **Execução**: nada foi rodado. Os limiares marcados [I] precisam de uma rodada no dev antes do congelamento.

## 9. Fontes

- Eskimez et al., Personalized Speech Enhancement: New Models and Comprehensive Evaluation (TSOS, pDCCRN, pDCATTUNET):
  https://arxiv.org/abs/2110.09625
- Thakker et al., Fast real-time personalized speech enhancement: E3Net: https://arxiv.org/abs/2204.00771
- Schröter et al., DeepFilterNet (DFN3, demo): https://arxiv.org/abs/2305.08227
- Serre et al., pDeepFilterNet2: https://arxiv.org/abs/2404.08022 e (versão com destilação)
  https://arxiv.org/abs/2601.16235
- Dubey et al., ICASSP 2022 Deep Noise Suppression Challenge: https://arxiv.org/abs/2202.13288
- ICASSP 2023 Deep Noise Suppression Challenge: https://arxiv.org/abs/2303.11510
- NPU-Elevoc (NAPSE): https://arxiv.org/abs/2303.06811
- Reddy et al., DNSMOS P.835: https://arxiv.org/abs/2110.01763; DNSMOS: https://arxiv.org/abs/2010.15258
- Repositório DNS-Challenge (licenças, DNSMOS, PDNS): https://github.com/microsoft/DNS-Challenge
- Le Roux et al., SI-SDR: https://arxiv.org/abs/1811.02508
- ITU-T P.862 (retirada em 2024): https://www.itu.int/rec/T-REC-P.862 ; P.863:
  https://www.itu.int/ITU-T/Recommendations/rec.aspx?rec=P.863
- Thread sobre licenciamento do PESQ (2013): https://auditory.org/postings/2013/505.html
- NISQA (código MIT, pesos CC BY-NC-SA): https://github.com/gabrielmittag/NISQA
- ViSQOL (Apache-2.0): https://github.com/google/visqol
- pystoi (MIT): https://github.com/mpariente/pystoi
- WeSpeaker, modelos pré-treinados: https://github.com/wenet-e2e/wespeaker/blob/master/docs/pretrained.md
- TAGARELA (card, CC BY-NC-SA 4.0): https://huggingface.co/datasets/freds0/TAGARELA
- DEMAND (Zenodo): https://zenodo.org/records/1227121 ; Valentini et al. (DataShare):
  https://datashare.ed.ac.uk/handle/10283/2791
- Locais lidos: `docs/superpowers/specs/2026-10-01-clearcore-studio-pipeline-design.md`;
  `crates/model/tests/{parity_film,golden_reference,tract_smoke,asset_gate,m0_adversarial,dsp_pipeline_parity,parity_eq,studio_contract}.rs`;
  `crates/model/src/{enrollment,golden,voice_profile}.rs`; `fixtures/film/README.md`; `fixtures/golden/README.md`;
  `docs/testing/voice-samples.md`; `docs/release-gates.md`;
  `docs/superpowers/plans/2026-10-02-clearcore-studio-phase4-enrollment-eq-plan.md`; documentos 03 e 04 desta pasta.
