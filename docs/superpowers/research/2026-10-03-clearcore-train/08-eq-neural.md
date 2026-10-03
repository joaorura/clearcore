# Subprojeto B: EQ neural (pesquisa para o repositório de treino do Clearcore)

Data: 2026-10-03. Escopo: pesquisa web + leitura local, sem instalar nem baixar nada. Nada aqui foi treinado nem medido.

Convenção: **[FATO]** vem do repositório (com caminho) ou de fonte web (com link) que eu li; **[INFERÊNCIA]** é raciocínio meu a partir dos fatos; **[NÃO VERIFICADO]** é o que não consegui confirmar.

## 0. Resumo executivo

1. O contrato atual trata o EQ neural como **estático por perfil**: os 32 ganhos moram em `VoiceProfile.eq`, são selados com o hash de integridade e entram no gancho pré-iSTFT por um comando de controle (não por quadro). A variante dinâmica (cabeça com `emb` por quadro) só existe como opção no relatório do spike e exigiria mudar o runtime.
2. A literatura de EQ neural (Nercessian 2020, Steinmetz/DeepAFx-ST, Mockenhaupt 2024) prediz parâmetros de um EQ paramétrico. Aqui a saída **já é a resposta em frequência** (32 ganhos em dB em bandas fixas) e a interpolação do runtime é linear em dB, logo não precisa de biquad diferenciável: uma matriz constante 481x32 basta para treinar com perda no domínio da resposta.
3. Dados de treino sintéticos (EQ aleatório sobre fala limpa, aprendendo a inversa) é o caminho padrão (HiFi-GAN, HiFi-GAN-2, Nercessian). **EARS e DAPS são CC BY-NC**, incompatíveis com a distribuição aberta dos pesos; VCTK, Hi-Fi TTS e LibriTTS-R são CC BY 4.0.
4. Achado de contrato que precisa de decisão: o enrollment é **16 kHz** (`ENROLLMENT_SAMPLE_RATE_HZ`), então as **bandas 25 a 31 (acima de 8,5 kHz) não são observáveis** a partir da gravação de cadastro.
5. Dependência de A: o EQ estático **não precisa** do pDFNet3 nem do embedding do locutor para começar; precisa só de áudio de saída do denoiser (o DFNet3 v1 aprovado serve de substituto) e, no fim, de validação com o pDFNet3.

## 1. O EQ neural é estático por perfil ou dinâmico por quadro? O que o contrato implica

### Evidência no repositório

- **[FATO]** `crates/model/src/voice_profile.rs:296-305`: `VoiceProfile { film: FiLMVectors, eq: Option<BandGains>, integrity_hash, .. }`. O `eq` entra no payload canônico do hash (`CanonicalProfilePayload`, linhas 311-318), é persistido com o perfil em arquivo 0600 (plano da fase 4, Task 5). É um atributo do **perfil**, não do quadro.
- **[FATO]** `crates/model/src/enrollment.rs:293`: `SpeakerEnrollmentEngine::enroll(&mut self, recording, metadata, eq: Option<BandGains>)`. O modelo de enrollment só devolve `RawFilmVectors` (gamma/beta enc e df); o `eq` chega **por fora**, de outro componente. Coerente com o descritor separado `neural-eq-asset-v1` (membro único `neural_eq.onnx`, `crates/model/src/model_registry.rs:218-224`).
- **[FATO]** `crates/model/src/tract_backend.rs:187-214`: `set_spectral_eq` e `set_voice_profile` mandam o `Conditioning` por um canal `WorkerCommand::Condition` com resposta síncrona. É plano de controle: aloca `Vec<f32>` de 481 fatores a cada chamada. Gain neutro vira `None` e desliga o gancho (bit-exato com "sem EQ", plano da fase 4, linha 97).
- **[FATO]** `crates/model/src/spectral_eq.rs`: `bin_factors(&BandGains, band_widths) -> Vec<f32>`; interpolação linear **em dB entre centros de banda**, só em frequência, sem nenhuma suavização temporal. `vendor/crates/deep_filter/src/tract.rs:480-518` e `767-775`: os fatores ficam em `spectral_eq_factors` e multiplicam `spec_enh` a cada quadro antes de `state.synthesis()`.
- **[FATO]** Spec (`docs/superpowers/specs/2026-10-01-clearcore-studio-pipeline-design.md` seção 3 e seção 7): "EQ neural — corrige a coloração do microfone com ganhos por banda"; "ganhos em dB por banda, limitados a [-6, +12] dB e suavizados". Requisito 4 ao repo de treino: "EQ neural com saída em ganhos dB por banda no contrato acima".
- **[FATO]** Relatório do spike (`...phase3-libdf-spike-report.md` ~l.732-734 e 754): a "cabeça de EQ dentro do pDFNet3" receberia `emb [ch,1,512]` por quadro, via uma terceira sessão `eq_head.run(emb)`, e o espectro do gancho está atrasado por `lookahead` em relação ao `emb`; "o treino precisa definir em qual atraso o `emb` condiciona o ganho". O spike só provou **ponto de aplicação e latência (lag 0)**, com ganhos constantes por banda; custo da terceira sessão **não foi medido**. A recomendação 4 diz para incorporar o gancho "só se a variante cabeça de EQ dentro do pDFNet3 for a escolhida".
- **[FATO]** O ONNX `neural_eq.onnx` **não tem contrato de entrada/saída escrito em lugar nenhum** (busca por `neural_eq`/`NeuralEq` só acha o descritor, a allowlist e testes de papel).

### Leitura

- **[INFERÊNCIA]** O contrato publicado implica **estático por perfil, calculado no cadastro**: o lugar do dado (perfil selado), o ponto de produção (parâmetro `eq` de `enroll`), o ponto de consumo (comando de controle, `Vec` alocado) e a ausência de qualquer suavização temporal convergem. Fisicamente também combina com "corrigir a coloração do microfone": resposta do microfone, posição e sala mudam devagar; ganho por quadro seguiria o conteúdo fonético e viraria processador dinâmico (bombeamento), não correção de canal.
- **[INFERÊNCIA]** A variante dinâmica não cabe no contrato atual sem mudar: (a) ONNX por quadro no caminho de tempo real (3ª sessão tract); (b) atualização de fatores por quadro sem alocar (hoje há `Vec` + ida e volta por canal); (c) suavização temporal (slew/EMA) que hoje não existe; (d) acoplamento ao grafo do pDFNet3 (qual `emb`, com que atraso). Fica como **extensão opcional (v2)**, só se o A/B cego mostrar ganho claro sobre o estático.
- **Recomendação:** v1 estático. Documentar no repo de treino que a saída é "um vetor de 32 ganhos por perfil".

## 2. Abordagens publicadas

### 2.1 EQ automático/neural

| Trabalho | O que faz | Relevância para o Clearcore |
|---|---|---|
| Nercessian, "Neural Parametric Equalizer Matching Using Differentiable Biquads", DAFx 2020 ([PDF](http://dafx2020.mdw.ac.at/proceedings/papers/DAFx2020_paper_7.pdf)) | **[FATO]** Entrada: resposta em magnitude desejada em dB; 3 blocos (dense 256 + LayerNorm + ReLU) -> projeção linear -> frequência/ganho/Q de K biquads (K=12, 48 kHz, N=4096); camada de `freqz` diferenciável calcula a resposta estimada. Perda = α·MSE(resposta) + β·MSE(parâmetros) + γ·L1(ganhos). Ganhos limitados a ±10 dB na inferência. Dados: parâmetros amostrados (Beta/Bernoulli; bandas ligadas/desligadas com p=0,5 shelves e 0,333 peaks). Resultado (val. set 1, MSE da resposta): proposto 0,0782 vs baseline convexo 0,1679 vs só perda de parâmetros 1,495; no set 2 (distribuição deslocada): 7,02 vs 10,03 vs 25,29. | Lição principal: **perda no domínio da resposta, não só nos parâmetros**; validar também com distribuição de EQ deslocada (OOD). |
| Mockenhaupt et al., "Automatic Equalization for Individual Instrument Tracks Using CNNs", DAFx 2024 ([arXiv](https://arxiv.org/html/2407.16691v1)) | **[FATO, lido via resumo do WebFetch]** CNN recebe a curva de diferença espectral (256 pontos log, 20 Hz a 22 kHz) e prevê parâmetros de EQ paramétrico (shelves + peaks, ±12 dB). 156.730 curvas sintéticas por amostragem aleatória de parâmetros (distribuição logarítmica em frequência, ganho com peso cúbico a favor de ganhos pequenos); ajuste fino com dados reais e perda L1 espectral: MAE de 1,35 dB para 1,02 dB (-24%); A/B subjetivo: 65% preferem o equalizado. | Mostra o ganho de **misturar dados sintéticos com dados reais** e de ajuste fino com perda espectral. Domínio é instrumento, não fala. K exato **[NÃO VERIFICADO]**. |
| Steinmetz, Bryan, Reiss, "Style transfer of audio effects with differentiable signal processing" (DeepAFx-ST, JAES 2022) ([arXiv](https://arxiv.org/abs/2207.08759), [código](https://github.com/adobe-research/DeepAFx-ST)) | **[FATO]** Rede analisa entrada + referência de estilo e prevê parâmetros de efeitos (EQ paramétrico multibanda e compressor); efeitos como operadores diferenciáveis; treino **auto-supervisionado, sem pares rotulados**; usa LibriTTS e VCTK (fala) e DAPS; generaliza para outras taxas de amostragem. Código e modelos sob "Adobe Research License". | Referência de **treino sem pares**. A licença do código/modelos **não é aberta como MIT/Apache** [NÃO VERIFICADO o texto; tratar como não reutilizável]. Detalhes da perda: [NÃO VERIFICADO] (o PDF não foi legível). |
| Steinmetz et al., "Automatic multitrack mixing with a differentiable mixing console of neural audio effects", ICASSP 2021 ([arXiv](https://arxiv.org/pdf/2010.10291)) | **[FATO]** Mixagem multipista aprendida no nível da forma de onda, sub-redes pré-treinadas, parâmetros legíveis. | **[INFERÊNCIA]** Pouco aplicável: é mixagem, não normalização de canal; só confirma o padrão "saída interpretável". |
| Germain, Mysore, Fujioka, "Equalization matching of speech recordings in real-world environments", ICASSP 2016 ([Adobe](https://research.adobe.com/publication/equalization-matching-of-speech-recordings-in-real-world-environments)) | **[FATO]** Casa o EQ de gravações de fala de ambientes diferentes, separando fala e ruído por enhancement, equalizando cada fonte e recombinando; supera EQ matching simples. | Baseline **clássico** (razão de espectros médios) que o neural precisa bater. |
| Sarkar e Lindborg, NDMP (DAFx 2025) ([PDF](https://dafx25.dii.univpm.it/wp-content/uploads/2025/07/DAFx25_paper_81.pdf)) e DiffVox ([arXiv](https://arxiv.org/html/2504.14735v1)) | **[FATO]** Apareceram na busca (EQ estático de 6 bandas + compressão por banda; modelo diferenciável de cadeia vocal). **Não li.** | Possíveis leituras de acompanhamento. |

### 2.2 Fala amadora para "som de estúdio"

- **HiFi-GAN** (Su, Jin, Finkelstein, Interspeech 2020, [arXiv](https://arxiv.org/pdf/2006.05694)). **[FATO]** Alvo: voz de estúdio a partir de ruído, reverberação **e distorção de equalização**. Dados por simulação: fala limpa do DAPS convolvida com 270 respostas ao impulso do MIT IR Survey, mais ruído; o ruído passa por **filtro multibanda aleatório**, e **"filtros multibanda aleatórios são aplicados às respostas ao impulso para distorção de equalização"**. Perdas: L1 na forma de onda + L1 de espectrogramas multi-resolução (FFT 2048/hop 512 e 512/hop 128 a 16 kHz) + perdas adversariais e de feature matching.
- **HiFi-GAN-2** (Su, Jin, Finkelstein, WASPAA 2021, [PDF](https://gfx.cs.princeton.edu/pubs/Su_2021_HSS/Su-HiFi-GAN-2-WASPAA-2021.pdf)). **[FATO]** Waveform-to-waveform, "ruído, reverb e distorção de EQ moderados"; três etapas (rede recorrente prevê MFCC de 18 coeficientes do sinal limpo; WaveNet condicionada; extensão de banda para 48 kHz). Treino por simulação sobre o conjunto limpo do DAPS + 270 IRs + ruído (REVERB Challenge, ACE). Os autores citam a **ambiguidade entre identidade do locutor e efeitos do ambiente (EQ e reverb)** como causa de inconsistência de timbre. **É um modelo generativo/pesado, fora do caminho de tempo real do Clearcore** (spec seção 1 exclui modelo generativo e BWE). Serve como evidência de **protocolo de dados**, não de arquitetura.
- **Adobe Enhance Speech / Podcast** ([guia](https://podcast.adobe.com/en/guides/what-is-enhance-speech)). **[FATO]** Material de produto: "remove ruído, corrige reverb, equaliza características do microfone". **[NÃO VERIFICADO]** Não achei descrição técnica pública; não afirmo que seja HiFi-GAN-2. **[INFERÊNCIA]** É a motivação de produto, não uma referência de método.

### 2.3 Alvo de treino: pares ou perfil de referência?

- **Pares amador -> estúdio reais:** o único conjunto assim que achei é o DAPS (mesmo texto, estúdio vs. tablet/celular em salas reais; 20 locutores, ~14 min cada, 15 versões) ([Zenodo](https://zenodo.org/records/4660670), [página](https://ccrma.stanford.edu/~gautham/Site/daps.html)), **CC BY-NC 4.0**. Pequeno e não comercial.
- **Pares sintéticos (recomendado):** aplicar EQ conhecido a fala limpa; o alvo é a inversa exata. É o que HiFi-GAN faz (para a distorção de EQ) e o que Nercessian/Mockenhaupt fazem com amostragem de parâmetros. **[INFERÊNCIA]** No Clearcore o alvo é conhecido por construção: `g_alvo = -d` (limitado ao intervalo permitido).
- **Matching de perfil espectral de referência:** é o EQ matching clássico (razão entre espectros médios suavizados na escala crítica, como no exemplo real de Nercessian com dois microfones). **[INFERÊNCIA]** Serve de **baseline**, e de definição do alvo "estúdio neutro" (curva média de fala de estúdio) quando a gravação de referência não tem par.
- **[INFERÊNCIA]** O problema mal posto central: com **uma** gravação, separar timbre do locutor de coloração do canal. A rede só consegue pelo **prior** (população de locutores de estúdio com espectro médio de longo prazo dentro de uma faixa). O erro mínimo alcançável é o desvio de LTAS entre locutores; medir esse piso antes de fixar metas (ver seção 3.5).

### 2.4 Dados com licença aberta

| Conjunto | Taxa | Licença | Nota |
|---|---|---|---|
| EARS ([GitHub](https://github.com/facebookresearch/ears_dataset), [arXiv](https://arxiv.org/pdf/2406.06185)) | 48 kHz, 100 h, 107 locutores, câmara anecoica | **[FATO] "CC-NC 4.0"** (texto do README) | Não comercial. **Fora do treino de pesos distribuídos**; no máximo avaliação local. Microfone usado **[NÃO VERIFICADO]**. |
| DAPS ([Zenodo](https://zenodo.org/records/4660670)) | taxa [NÃO VERIFICADO na página lida] | **[FATO] CC BY-NC 4.0** | Único par estúdio/dispositivo. Mesmo tratamento do EARS: só avaliação local. |
| VCTK 0.92 ([DataShare](https://datashare.ed.ac.uk/handle/10283/3443)) | 48 kHz (gravado a 96 kHz/24 bit) | **[FATO] CC BY 4.0** | 110 locutores; todos com o mesmo microfone omni de cabeça DPA 4035 em câmara hemi-anecoica. **[INFERÊNCIA]** Todos compartilham a mesma coloração de microfone: uma coloração residual constante que a rede pode aprender como "neutro". Já previsto no spec (VCTK p225/p226). |
| Hi-Fi TTS ([arXiv](https://arxiv.org/pdf/2104.01497), [OpenSLR 109](https://openslr.org/109)) | 44,1 kHz, 291,6 h, 10 locutores | **[FATO] CC BY 4.0** | Audiolivros (microfones heterogêneos, por isso variedade de coloração real). Faixa útil até ~22 kHz. |
| LibriTTS-R ([OpenSLR 141](https://us.openslr.org/141)) | 24 kHz, 585 h, 2.456 locutores | **[FATO] CC BY 4.0** | Restaurado por modelo de restauração de fala. Limitado a 12 kHz: bandas 28 a 31 não observáveis. |
| TTS-Portuguese Corpus ([GitHub](https://github.com/Edresson/TTS-Portuguese-Corpus)) | 48 kHz, ~10,5 h, 1 locutor | **[FATO] CC BY 4.0** | **Não é estúdio** (os autores aplicaram RNNoise). Uso: só como conjunto OOD pt-BR. |
| CML-TTS ([resumo](https://paperswithcode.com/paper/cml-tts-a-multilingual-dataset-for-speech)) | [NÃO VERIFICADO] | **[FATO] CC BY 4.0** | Derivado do MLS, inclui português. Taxa não verificada. |
| Common Voice pt / MLS pt | 16 kHz suspeito | CC0 / CC BY 4.0 (segundo o spec, seção 8, ainda a confirmar na fonte) | Microfones de usuário, não estúdio. Útil como **entrada realista** de teste, não como alvo. |

- **[FATO]** O spec (seção 7, requisito 7, e seção 8) exige licenças compatíveis com a distribuição aberta dos pesos e já tratou o TAGARELA (CC BY-NC-SA) como "fora do treino" por essa razão. **[INFERÊNCIA]** O mesmo critério exclui EARS e DAPS (NC) do treino de pesos distribuídos. Para decidir "pesos contam como adaptação?" falta decisão do dono no vault; até lá, tratar NC como avaliação local.
- **Respostas de microfone reais** abertas: não achei conjunto de respostas em frequência de microfones com licença clara. Os conjuntos achados (BIRD, BUT ReverbDB) são de **sala** ([BIRD](https://ar5iv.labs.arxiv.org/html/2010.09930), [BUT ReverbDB](https://www.fit.vut.cz/research/publication-file/c159973/280107/IEEE_Final_Published_08717722-1.pdf)). **[INFERÊNCIA]** Por isso o canal será sintetizado por famílias de curvas (seção 3.3), com validação em gravações reais de microfones comuns.

## 3. Proposta concreta

### 3.1 Contrato proposto para `neural_eq.onnx` (v1 estático)

**[INFERÊNCIA / proposta]** O contrato do ONNX não está escrito; sugiro:

- Entradas: `feat [1, 32, 3]` (por banda ERB: média, e percentis P10 e P90 da energia log em dB sobre quadros com fala, depois de normalização de nível global) e `valid [1, 32]` (1,0 onde a banda é observável; 0,0 nas bandas acima de Nyquist da gravação). Saída: `gains_db [1, 32]` com o clamp [-6, +12] **dentro do grafo** (o Rust revalida com `BandGains::validate`).
- As features são calculadas **em Rust** no cadastro (a gravação é apagada logo depois; o ONNX nunca vê áudio). Isso evita depender de STFT dentro do tract (suporte do tract a operadores de STFT em ONNX: **[NÃO VERIFICADO]**) e põe a paridade em um ponto testável (vetores dourados Python vs Rust).
- Executa **uma vez por cadastro**; custo de tempo real = zero (o gancho já multiplica 481 bins).

### 3.2 Arquitetura leve

1. **Cabeça de ganhos suave:** MLP `96 -> 128 -> 128 -> K` com LayerNorm e ReLU (~30k parâmetros, ordem de 100 KB em float32), onde `K = 8` coeficientes de uma base suave (cossenos/B-splines no eixo de banda), expandidos por matriz constante em 32 ganhos. **[INFERÊNCIA]** Impõe suavidade em frequência por construção e reduz o risco de ganho "serrilhado".
2. **Saída com zona neutra:** `g = clamp(g_raw · c, -6, +12)` com `c ∈ [0,1]` uma confiança aprendida; `c -> 0` devolve ganho plano (EQ neutro desliga o gancho, bit-exato, como já está em `eq_factors`). Garante o princípio do spec "o produto nunca fica pior do que é hoje".
3. **Perda no domínio da resposta, não nos parâmetros** (lição de Nercessian). A interpolação do runtime é linear em dB, portanto exatamente uma matriz `M` de 481x32 (centros de banda a partir de `band_widths`; bins fora dos extremos mantêm o ganho da banda). Resposta realizada: `r = M·g`. Isso reproduz `spectral_eq::bin_factors` e deve ter teste de paridade contra a função Rust (tolerância 1e-5 em dB).
4. **Baseline obrigatório (sem rede):** EQ matching clássico (razão LTAS da gravação / LTAS-alvo de estúdio, suavizada em ERB, com clamp). O neural só entra se bater o baseline (seção 3.5).

**Variante dinâmica (v2 opcional):** GRU pequena por quadro com entradas `emb [512]` (do `enc`, via o gancho do spike) + log-energia ERB (32), saída 32 ganhos, com suavização temporal (EMA/slew ~100 a 300 ms) e `lookahead` alinhado ao atraso do espectro do gancho. **[INFERÊNCIA]** Ordem de ~60k parâmetros; custo não medido; depende do grafo do pDFNet3 (seção 4). Só se o estático falhar o A/B cego.

### 3.3 Geração sintética de dados (aplicar EQ aleatório e aprender a inversa)

Pipeline por exemplo:

1. Escolher um clipe de fala limpa de estúdio (VCTK, Hi-Fi TTS, LibriTTS-R), rebaixar/reamostrar para 48 kHz, cortar 6 a 12 s (a janela do cadastro: `ENROLLMENT_MIN/MAX_DURATION_SECS` = 6/12 s).
2. Amostrar a distorção de canal `d(f)` em dB, mistura de famílias:
   - paramétrica: 1 low-shelf + 1 high-shelf + 2 a 4 peaks, parâmetros por Beta/Bernoulli como em Nercessian (frequências log-distribuídas; ganhos concentrados em torno de 0 com bandas desligadas com p~0,5; Q baixo para "forma de tom"); **[FATO]** (Nercessian, Figura 3 e eqs. 11 a 14);
   - roll-off de microfone (passa-alta 80 a 300 Hz; queda em AF a partir de 6 a 16 kHz), proximidade (+ graves), pico de presença 2 a 6 kHz, "boxy" 300 a 600 Hz;
   - curvas suaves aleatórias (série de cossenos no eixo ERB);
   - fração **identidade** `d = 0` (~25%) para ensinar "não mexa quando já está bom".
   O intervalo de `d` é escolhido para que a inversa caiba em [-6, +12]: `d ∈ [-12, +6]` na maioria; uma cauda fora desse intervalo existe de propósito, e o alvo é `clamp(-d, -6, +12)` (a rede aprende a corrigir o que o contrato permite).
3. Aplicar `d` ao áudio no tempo (FIR de fase linear a partir da curva em 481 bins, só para o dado; é "o canal real", não o EQ do produto).
4. **Realismo do estágio anterior:** em um segundo estágio, adicionar ruído/reverb (conjuntos abertos) e passar pelo denoiser (DFNet3 v1 agora; pDFNet3 quando existir) antes de extrair as features. A distribuição de entrada do EQ é a **saída do denoiser**, não a fala limpa.
5. Calcular `feat` e `valid` com o mesmo código que o runtime Rust usará (módulo compartilhado e vetores dourados).
6. Divisão por **locutor** (não por clipe) em treino/validação/teste; uma família de curvas inteira reservada só para o teste (OOD, como o "validation set 2" de Nercessian).

**[INFERÊNCIA]** Usar vários locutores é essencial: é isso que ensina a separar timbre de coloração (seção 2.3).

### 3.4 Perdas

- **Principal:** L1 (Huber) em dB entre `(d + r)` e 0, sobre os 481 bins com peso ERB (mais peso onde a fala tem energia), com `r = M·g`.
- **Assimetria "não machuque":** peso maior quando `d + r` sobrecorrige (ganho no sentido errado) do que quando subcorrige.
- **Regularizadores:** suavidade (segunda diferença entre bandas), L1 pequeno em `g` (viés para neutro, análogo ao termo γ de Nercessian), média de `g` perto de 0 (o nível é problema do AGC da cadeia DSP).
- **No áudio (auxiliar):** perda espectral multi-resolução (L1 em log-espectrogramas, como em HiFi-GAN) entre `EQ(degradado)` e a referência limpa, com normalização de nível. **[INFERÊNCIA]** Opcional; a perda no domínio da resposta já é o alvo exato.
- **Não usar** perda só nos parâmetros (Nercessian: mais fiel nos parâmetros e pior na resposta, MSE 1,495 vs 0,0782). Aqui não há parâmetros intermediários, o que evita o problema.

### 3.5 Métricas e critérios de aceite (propostos; os limiares são pontos de partida a calibrar)

Calibrar primeiro **três referências** no conjunto de teste: (a) bypass (sem EQ); (b) baseline clássico (LTAS); (c) **oráculo de LTAS do próprio locutor** (piso de erro por ambiguidade de timbre).

| # | Métrica | Aceite proposto |
|---|---|---|
| M1 | Erro residual da resposta `|d + r|` (dB), sobre bins observáveis, locutores e famílias de EQ retidos | mediana <= 1,5 dB e P90 <= 3 dB, **e** melhor que o baseline clássico por margem mínima definida após medir o oráculo |
| M2 | OOD: família de curvas nunca vista (como validation set 2) | piora relativa ao in-distribution <= 50%; nunca pior que bypass |
| M3 | "Não machuque": entradas com `d = 0` | `mean(|g|)` <= 0,75 dB; P95 de `|g|` <= 2 dB |
| M4 | Consistência de cadastro: mesmos locutor e canal, clipes diferentes de 6 a 12 s, SNRs variados | desvio padrão dos ganhos entre clipes <= 1 dB |
| M5 | Distância log-espectral por banda ERB do áudio equalizado à referência limpa (após normalização de nível) | melhora >= X% sobre bypass (X após medir o oráculo) |
| M6 | Amplificação de resíduo: ruído/artefato do denoiser em bandas com ganho positivo | sem piora de DNSMOS P.835 (SIG/OVRL); queda <= 0,05 |
| M7 | Subjetivo: A/B cego pequeno (o spec exige A/B cego) | preferência significativa pelo EQ vs. bypass sem queda de naturalidade; métrica auxiliar NISQA "coloration" |
| M8 | Contrato e robustez | saída sempre finita e em [-6, +12]; `BandGains::validate` passa; paridade `M·g` vs `bin_factors` <= 1e-5 dB; EQ neutro desliga o gancho (bit-exato) |
| M9 | Custo | v1: execução no cadastro em ms, zero em tempo real. v2: se adotada, <= 5% do orçamento restante do quadro (o spike mediu p99 0,74 ms contra limite de 7 ms, **sem** o EQ) |

Notas: **[FATO]** NISQA reporta qualidade global, ruído, **coloração**, descontinuidade e loudness ([GitHub](https://github.com/gabrielmittag/NISQA)); código MIT, mas os **pesos pré-treinados são CC BY-NC-SA 4.0**: usar só em avaliação, nunca distribuir. DNSMOS P.835 ([GitHub](https://github.com/microsoft/DNS-Challenge/tree/master/DNSMOS)): licença **[NÃO VERIFICADO]**.

### 3.6 Risco das bandas altas (decisão necessária)

- **[FATO]** Faixas das bandas (48 kHz, N=960, 50 Hz por bin; calculado a partir de `widths()` em `spectral_eq.rs`): bandas 0 a 12 têm 100 Hz de largura; banda 23 vai de 6,3 a 7,3 kHz; banda 24, 7,3 a 8,5 kHz; **bandas 25 a 31: 8,5 a 24 kHz**.
- **[FATO]** O cadastro é 16 kHz (`enrollment.rs:ENROLLMENT_SAMPLE_RATE_HZ`).
- **[INFERÊNCIA]** Opções: (a) EQ só estima as bandas 0 a 24 e extrapola as 7 de cima por prior aprendido + `valid`, limitando o ganho nelas; (b) o cadastro passa a capturar 48 kHz para o EQ (muda o contrato do enrollment); (c) calibração com o fluxo vivo a 48 kHz nos primeiros segundos de fala, depois congelando (continua "estático"). Recomendo começar por (a) com meta de ganho conservador acima de 8,5 kHz e decidir (b)/(c) pelo A/B cego. Isso é decisão do dono.

## 4. Dependências com o subprojeto A

(Assumo que A = pDFNet3 + enrollment, como no spec seção 7.)

- **Embedding do locutor compartilhado?** **[FATO]** Não no contrato atual: o ONNX de enrollment devolve só `gamma/beta` e o `eq` entra por fora (`enrollment.rs:293`); asset separado `neural-eq-asset-v1`. **[INFERÊNCIA]** O EQ estático **não precisa** do embedding do locutor; ele deve **preservar** o timbre e corrigir só o canal, então condicionar ao locutor poderia até piorar a separação. Compartilhar embedding só faria sentido na variante dinâmica (v2), via `emb` do `enc` (512 por quadro), o que **acopla B ao grafo final de A**.
- **Precisa do pDFNet3 pronto?** Para v1 **não**: o DFNet3 v1 aprovado (spec seção 2 e 10) produz a saída do denoiser para montar entradas realistas. **Sim para o fechamento**: o gate final (M5, M6, M7) deve rodar com a saída do pDFNet3, porque o EQ vem depois dele na cadeia e o espectro do pDFNet3 pode diferir. **[INFERÊNCIA]** A máscara ERB do DeepFilterNet usa sigmoide ([resumo da busca sobre o DFNet3](https://soniqo.audio/guides/denoise)), logo só atenua; o denoiser não corrige microfone "abafado". O EQ é, portanto, ortogonal ao denoiser, mas as bandas com ganho positivo podem amplificar artefatos dele (M6).
- **Decisões de A que B herda:** (i) se o cadastro usa áudio **cru ou denoisado** (requisito 2 e ablação 6 do spec): B treina com a mesma pré-processamento; (ii) modo "sem locutor" estável: sem relação direta; (iii) qualquer mudança no front-end STFT/ERB (contrato 960/480/32/96/5/2 fica inalterado, requisito 5): B reutiliza o mesmo módulo de ERB.
- **Código/dados compartilhados:** módulo único de STFT/ERB (com vetores dourados do Rust), lista de licenças permitidas e protocolo de divisão por locutor.
- **Ordem sugerida:** B-v1 pode andar em paralelo a A com o DFNet3 v1; reavaliar com o pDFNet3 quando A entregar o primeiro checkpoint.

## 5. Perguntas abertas para o dono

1. Confirmar **estático por perfil** como contrato da v1 e a variante dinâmica como v2 opcional.
2. Bandas acima de 8,5 kHz: extrapolar, capturar cadastro a 48 kHz, ou calibrar ao vivo?
3. Entrada do `neural_eq.onnx`: features calculadas em Rust (proposto) ou áudio?
4. EARS/DAPS (CC BY-NC): excluir do treino distribuído (proposto) e usar só em avaliação local?
5. O "som de estúdio/broadcast" fica nos presets da cadeia DSP (proposto: EQ neural só normaliza o canal) ou o EQ neural também deve empurrar para uma curva-alvo de broadcast?

## 6. O que não foi verificado

- Perdas e detalhes do treino auto-supervisionado do DeepAFx-ST (PDF ilegível pela ferramenta; só abstract e README).
- Microfone e taxa do DAPS/EARS; licença do DNSMOS; taxa do CML-TTS; texto da "Adobe Research License".
- Qualquer número de custo, qualidade ou tolerância proposto (todos são metas a calibrar, nenhum foi medido).
- Suporte do tract 0.19.16 a todos os operadores do ONNX proposto (MLP simples deve estar coberto, mas não testei).
