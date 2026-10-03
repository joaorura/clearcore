# Datasets de fala em português (PT-BR) para o fine-tune do DeepFilterNet3

Data da pesquisa: 2026-10-03. Pesquisa só na web, nenhum dado foi baixado.
Escopo: apenas português (de preferência PT-BR). Datasets gerais (VCTK, DNS, LibriTTS, ruído, RIR) estão em outro relatório.

Critério de licença (do pedido): os pesos saem como MIT OR Apache-2.0 com uso comercial. CC0 e CC BY servem; CC BY-SA é discutível; qualquer NC (e ND) é proibido para treino.

Convenção: **[F]** = fato lido na fonte citada; **[I]** = inferência minha, sem fonte direta.

---

## 1. Resposta curta

**Utilizáveis para TREINO de pesos abertos (licença compatível):**

| Dataset | Licença | Observação principal |
|---|---|---|
| Common Voice pt 27.0 (e o recorte pt-BR 26.0) | CC0-1.0 | 48 kHz nominal em MP3, crowd, ~624 falantes pt-BR; licença confirmada |
| CML-TTS pt (OpenSLR 146) | CC BY 4.0 | 24 kHz, ~69 h, 48 falantes, leitura de audiolivro |
| MLS Portuguese (OpenSLR 94) | CC BY 4.0 | só 16 kHz distribuído; serve para o lado "fala limpa" apenas se se aceitar banda de 8 kHz |
| TTS-Portuguese Corpus (Edresson) | CC BY 4.0 | 48 kHz, mas 1 falante e já passado por RNNoise |
| Open Home Foundation (Jeff, Cadu, Faber; Tugão é pt-PT) | CC0-1.0 | 3 vozes pt-BR de ~1,5 h cada, volume pequeno, taxa não declarada |
| LibriVox PT (fonte bruta) | domínio público (declaração do LibriVox) | exige pipeline próprio; ver ressalvas |

**Só para TESTE / avaliação (ou proibidos para treino):**

| Dataset | Motivo |
|---|---|
| TAGARELA | CC BY-NC-SA 4.0 (confirmado no cartão do HF) e proíbe voice cloning/speaker ID |
| CORAA ASR 1.1 | CC BY-NC-ND 4.0 |
| CORAA-MUPE / MuPe Life Stories | CC BY-NC-ND 4.0 (segundo tabela do paper Certas Palavras) |
| NURC-SP Audio Corpus | CC BY-NC-ND 4.0 |
| Certas Palavras | CC BY-NC 4.0 |
| CoVoST 2 pt-en | CC BY-NC 4.0 |
| Spoltech, West Point (LDC) | licença LDC/CSLU, só pesquisa não comercial, pago |
| VoxForge pt | GPL-3.0 (incompatível com MIT OR Apache-2.0) e só 4,5 h |
| CETUC | "para fins de pesquisa exclusivamente"; o MIT no HF contradiz a fonte, tratar como não utilizável |

**Melhor conjunto PT-BR com speaker ID e >= 24 kHz [I]:**
1. **CML-TTS pt** como alvo "mais limpo": 24 kHz, CC BY, 48 falantes, IDs de falante herdados do MLS.
2. **Common Voice pt-BR** (CV 27.0, ou o recorte 26.0 já filtrado por votos) como volume e diversidade: CC0, 48 kHz nominal, `client_id` por falante, ~624 falantes pt-BR; precisa de filtro de qualidade (DNSMOS ou similar) porque é crowd.
3. Opcional, para o espectro alto: TTS-Portuguese Corpus (48 kHz, 1 falante) e as 3 vozes pt-BR da Open Home Foundation (CC0).

Nenhum corpus PT-BR aberto compatível com ambas as condições (estúdio de verdade, multi-falante, >= 44,1 kHz) foi encontrado. O corpus "97-speaker" a 48 kHz em estúdio (DataoceanAI) aparece em resultado de busca, mas é comercial e não foi verificado.

---

## 2. Common Voice pt (a pergunta sobre CC0)

**Licença confirmada: CC0-1.0.** [F] Página oficial do Mozilla Data Collective, "Common Voice Scripted Speech 27.0 - Portuguese": campo Licenciamento = "Creative Commons Zero v1.0 Universal (CC0-1.0)", link SPDX https://spdx.org/licenses/CC0-1.0.html. Fonte: https://mozilladatacollective.com/datasets/cmu5wui2w00e8nq07idithpm6 (lida em 2026-10-03, via navegador, porque a página é renderizada por JS).

Dados da mesma página [F]:
- Versão mais recente: **CV 27.0** (cv-corpus-27.0-2026-09-11), lançada em 17/09/2026. Formato MP3, 4,85 GB. Administrador: Common Voice (contato commonvoice@mozilla.com).
- 198.059 clipes, 230,38 h gravadas (188,84 h validadas), **3.860 falantes**, 43.840 frases. Clipe médio 4,187 s.
- Variantes: pt-BR 86.209 clipes (43,5%) e 624 falantes; pt-PT 3.428 clipes (1,7%) e 76 falantes. O resto não tem variante declarada.
- Gênero declarado só para 20,4% dos falantes (789 de 3.860); 944 masculino, 137 feminino. Idade declarada para 22,1%.
- Splits de modelagem cobrem só 26,3% dos validados (23.300 train, 9.705 dev, 9.707 test).
- Há também o recorte "Common Voice Scripted Speech 26.0 - Brazilian Portuguese" (CC0-1.0, 122 MB): clipes com up_votes > 0 e down_votes = 0 e sotaque auto-declarado brasileiro. https://mozilladatacollective.com/datasets/cmruxo9ew00d3md07veethj6k
- Não existe "Spontaneous Speech" em português na listagem (só Galician, Puno Quechua etc.). [F] listagem https://mozilladatacollective.com/datasets?q=Portuguese

**Taxa de amostragem nativa: 48 kHz, MP3 mono 16 bit** [F] segundo o paper do Common Voice (https://arxiv.org/pdf/1912.06670v1, via resumo de busca); o CORAA paper também diz "originalmente coletados a 48 kHz, mas reduzidos a 16 kHz" para a versão que eles usaram. O MP3 impõe corte de banda e artefatos de codec [I].

**Ressalvas relevantes ao projeto:**
- **Uso proibido [F]** (mesma página): "It is forbidden to attempt to determine the identity of speakers in the Common Voice datasets. It is forbidden to re-host or re-share this dataset." Tem duas consequências [I]: (a) não redistribuir o corpus nem derivados em forma de áudio; só os pesos saem; (b) usar `client_id` para agrupar falantes é prática padrão, mas treinar um embedding de falante a partir deles toca a cláusula "determinar identidade". Vale registrar a interpretação (embedding para separar falantes, não para identificar pessoas) e, se possível, consultar o contato da Mozilla.
- A licença é CC0, mas os termos do portal acrescentam as proibições acima [F]; isso não impede treinar e publicar pesos [I].
- **Qualidade:** gravação crowd por navegador, ruído e reverberação variáveis, falantes majoritariamente masculinos e jovens [F, dados demográficos acima]. Útil como fala-alvo apenas após filtro de qualidade [I].
- **Speaker ID:** `client_id` (hash) por gravador [F, via https://arxiv.org/pdf/1912.06670v1 resumo de busca]. Um `client_id` pode ser mais de uma pessoa e uma pessoa pode ter vários `client_id` [I].
- A decisão anterior marcava "licença não confirmada": **agora confirmada** na fonte oficial.

---

## 3. Tabela por dataset

Legenda de uso: TREINO = licença compatível; TESTE = só avaliação; NÃO = não usar.

| Dataset | Licença (fonte) | Taxa nativa | Horas | Falantes | Speaker ID | Qualidade | Tamanho | Uso |
|---|---|---|---|---|---|---|---|---|
| **Common Voice pt 27.0** | CC0-1.0 [F] [MDC](https://mozilladatacollective.com/datasets/cmu5wui2w00e8nq07idithpm6) | 48 kHz MP3 [F] | 230,4 (188,8 validadas); pt-BR ~100 h [I: 86.209 clipes x 4,187 s] | 3.860 (pt-BR 624) | sim, `client_id` | crowd, ruidoso | 4,85 GB | TREINO (com filtro) |
| **MLS Portuguese** (OpenSLR 94) | CC BY 4.0 [F] [OpenSLR](https://www.openslr.org/94/) | 16 kHz (reduzido de 48 kHz do LibriVox) [F] [paper](https://arxiv.org/pdf/2012.03411) | 160,96 train + 3,64 dev + 3,74 test = ~168,3 [F] | 42 train (26 M, 16 F) + 10 dev + 10 test = 62 [F, soma minha]; Certas Palavras cita 54 e CML-TTS cita 48 | sim (arquivos por falante/livro) [I] | audiolivro amador, limpo-ish | 9,3 GB FLAC, 2,5 GB Opus [F] | TREINO, mas limitado a 16 kHz |
| **CORAA ASR 1.1** | CC BY-NC-ND 4.0 [F] [paper](https://arxiv.org/pdf/2110.15731), [GitHub](https://github.com/nilc-nlp/CORAA) | heterogênea (fontes de 8 kHz a 96 kHz) [F, paper] | 290,77 | ~1.689 [F, tabela do Certas Palavras] | parcial | espontânea, ruidosa | n/d | NÃO |
| **CORAA-MUPE-ASR** (MuPe Life Stories) | CC BY-NC-ND 4.0 [F, tabela 1 do [Certas Palavras](https://aclanthology.org/2026.propor-1.81.pdf)]; o cartão do HF não declara licença | 16 kHz [F, [HF](https://huggingface.co/datasets/nilc-nlp/CORAA-MUPE-ASR)] | ~365 | 289 entrevistas | metadados por entrevistado | entrevista espontânea | n/d | NÃO |
| **NURC-SP Audio Corpus** | CC BY-NC-ND 4.0 [F] [arXiv 2409.15350](https://arxiv.org/abs/2409.15350); o cartão do HF `NURC-SP_ENTOA_TTS` diz MIT [F] (conflito, ver nota) | 16 kHz no HF [F] | 239,3 | 401 | diarização automática (WhisperX) | gravações dos anos 1970, qualidade variável | n/d | NÃO |
| **CETUC** | origem: "cedido ao LaPS, para fins de pesquisa exclusivamente" [F, [falabrasil/speech-datasets](https://github.com/falabrasil/speech-datasets)]; cartão do HF diz MIT [F, [HF](https://huggingface.co/datasets/falabrasil/cetuc)] | 16 kHz [F] | 144h39m | 101 (92 no HF) | sim | leitura em ambiente controlado | 17 GB | NÃO (conflito de licença) |
| **LapsBM** | "Public" na tabela do GitHub; MIT no HF [F] | 22,05 kHz [F] | 0h54m | 35 (10 F) | sim | ambiente não controlado, ruído | 141 MB | TESTE |
| **Constituição** (FalaBrasil) | "Controlled" no GitHub; MIT no HF [F] | 22,05 kHz [F] | 8h58m | 1 | n/a | ambiente controlado | 1,4 GB | irrelevante (1 falante) |
| **CoddEF / MF** (FalaBrasil) | CoddEF: "TBD"; MF: não declarada [F] | 16 kHz / n/d | 1h25m / 15 min | 1 / 2 | n/a | n/d | n/d | NÃO |
| **TTS-Portuguese Corpus** (Edresson) | CC BY 4.0 [F] [GitHub](https://github.com/Edresson/TTS-Portuguese-Corpus) | 48 kHz, 16 bit [F] | ~10h28 | 1 (masculino) | n/a | fora de estúdio, com RNNoise aplicado [F] | n/d | TREINO (pouco valor para enrollment) |
| **Open Home Foundation: Jeff, Cadu, Faber** | CC0-1.0 [F] [Jeff](https://mozilladatacollective.com/datasets/cmiupaf3e01i0mf07qkqxbzzx), [Cadu](https://mozilladatacollective.com/datasets/cmiuimd7e01fimf072njn3mbb), [Faber](https://mozilladatacollective.com/datasets/cmiupazq801ftnv079bo1zu4h) | não declarada; formato WEBM [F] | ~1,5 h cada | 1 cada (pt-BR) | n/a | gravação online (Piper Recording Studio), sem pós-processamento [F] | 31 a 91 MB | TREINO (volume irrelevante) |
| **Open Home Foundation: Tugão** | CC0-1.0 [F] | idem | ~1,5 h | 1 (pt-PT) | n/a | idem | 62 MB | fora do escopo (pt-PT) |
| **VoxForge pt** | GPL-3.0-or-later [F] [MDC](https://mozilladatacollective.com/datasets/cmp2gp36800m9mp072hodigy6) | WAV, taxa n/d | 4,5 | >= 116 | sim (pasta por usuário) | crowd, heterogêneo | 1,06 GB | NÃO (GPL) |
| **M-AILABS** | BSD-like [F] [GitHub](https://github.com/imdatceleste/m-ailabs-dataset) | 16 kHz | ~999 total | n/d | sim | audiolivro | n/d | **não tem português** (idiomas: de, en, es, it, uk, ru, fr, pl) [F] |
| **BRSD v2** (Quintanilha) | composto: CETUC + Common Voice + outros pequenos [F, [JCIS](https://jcis.sbrt.org.br/jcis/article/view/721)]; licença do pacote n/d | n/d | 158 | 775 [F, tabela Certas Palavras] | n/d | n/d | n/d | NÃO (herda a licença do CETUC) |
| **BRSpeech-DF** | n/d (não consegui ler a seção de licença/fonte) | n/d | 458 mil enunciados, fala real de 62 falantes [F, [ACL](https://preview.aclanthology.org/setup/2025.emnlp-main.1780)] | 62 reais | sim | majoritariamente fala sintética (deepfake) | n/d | NÃO (é dataset de deepfake) |
| **Spoltech** (LDC2006S16) | CSLU Agreement, só pesquisa não comercial [F, [LDC](https://catalog.ldc.upenn.edu/LDC2006S16)] | 44,1 kHz mono 16 bit [F] | ~5 | 480 | sim | microfone, leitura | n/d | NÃO |
| **West Point Brazilian Portuguese** (LDC2008S04) | LDC [F, listagem do falabrasil] | 16 kHz | 5h22 | 70 | sim | n/d | n/d | NÃO |
| **CML-TTS pt** (OpenSLR 146) | CC BY 4.0 [F] [OpenSLR](https://openslr.org/146/), [HF ylacombe/cml-tts](https://huggingface.co/datasets/ylacombe/cml-tts) | **24 kHz** [F] | ~69,4 (train 67,95 + dev 0,88 + test 0,52) [F, tabela 1 do [paper](https://arxiv.org/html/2306.10097v1)] | 48 (train 20 M + 10 F; dev 6 M + 3 F; test 5 M + 4 F) [F, soma minha] | sim, herdado do MLS [I] | leitura de audiolivro (LibriVox), filtrado por WER via wav2vec2 (>= 0,9 de similaridade) [F] | 9,7 GB (pt) [F] | TREINO |
| **TAGARELA** | **CC BY-NC-SA 4.0** [F, [cartão HF](https://huggingface.co/datasets/freds0/TAGARELA)]; além disso proíbe speaker ID, voice cloning, impersonação [F] | 16 kHz FLAC [F] | 8.972 (subset "limpo" ~2.800) | ~13.368 rótulos por clustering | sim, mas automático | podcasts, denoised com Vocos | n/d | TESTE |
| **Certas Palavras** (nilc-nlp) | CC BY-NC 4.0 [F, [paper PROPOR 2026](https://aclanthology.org/2026.propor-1.81.pdf)] | 16 kHz (48 kHz prometido em versão futura) [F] | 70 | 190 | sim | rádio anos 1980-90, hiss | n/d | NÃO |
| **LibriVox PT** (fonte bruta) | domínio público, "people can do anything they like with them" [F, [LibriVox](https://librivox.org/pages/public-domain/)] | MP3 (os originais são 48 kHz segundo o MLS paper, ver ressalva) | ~284,6 h no LibriVox em 2020 [F, tabela 1 do MLS paper] | ~31 locutores nessa contagem | sim (leitor por projeto) | amador, qualidade desigual | n/d | TREINO, com ressalvas |
| **Bracis/NURC-SP** | ver NURC-SP acima | | | | | | | NÃO |
| **Fala Brasil (UFPA)** | ver CETUC, LapsBM, Constituição, CoddEF, MF acima | | | | | | | ver acima |

### Notas e conflitos de fonte

- **CETUC: conflito entre HF e origem.** O cartão `falabrasil/cetuc` traz `license: MIT` [F], mas o README original do grupo diz que o corpus foi "cedido ao LaPS, para fins de pesquisa exclusivamente" [F], e o paper do CORAA diz "publicamente disponível, sem licença explícita" [F]. Inferência: o rótulo MIT do HF é do empacotamento do grupo e não pode substituir a restrição da PUC-Rio; tratar como não utilizável para pesos comerciais.
- **NURC-SP: conflito entre HF e paper.** Cartão HF `NURC-SP_ENTOA_TTS` diz MIT [F]; o paper diz CC BY-NC-ND 4.0 [F]; o corpus mínimo no Portulan Clarin é CC BY-NC-ND 4.0 [F, [ar5iv](https://ar5iv.labs.arxiv.org/html/2210.07852)]. Prevalece o mais restritivo.
- **Contagem de falantes do MLS pt:** 62 (soma da tabela 2 do paper original), 54 (tabela do Certas Palavras) e 48 (CML-TTS) divergem entre fontes. Não resolvi; a fonte primária é o README dentro dos tarballs do OpenSLR 94, que não abri (a página só aponta para o README e para o paper).
- **Taxa do MLS:** o paper diz que o áudio foi reduzido de 48 kHz para 16 kHz [F]. O LibriVox publica MP3; a taxa real do original pode ser menor que 48 kHz, o que a frase do paper não esclarece [I].
- **CML-TTS 24 kHz:** o paper converte os MP3 originais do LibriVox para WAV de 24 kHz e descarta os que tinham taxa menor [F]. A banda efetiva depende do MP3 de origem [I]; vale medir o espectro de uma amostra antes de contar com conteúdo acima de 8 kHz.
- **TAGARELA:** o arXiv HTML mostra "License: CC BY 4.0", mas essa é a licença do paper; o dataset é CC BY-NC-SA 4.0 (cartão e tags do HF) [F]. Confirmado: só teste, e mesmo para teste a cláusula contra voice cloning e speaker ID merece leitura.
- **Bases com "MIT" no HF (falabrasil/*)**: a coleção do HF se descreve como "Data with permissive licenses for research and development" [F]; isso contrasta com o README de origem do GitHub. Tratar o rótulo como não verificado.

---

## 4. Ressalvas de LibriVox PT (domínio público -> derivados)

- LibriVox declara as gravações como domínio público e sem exigência de crédito [F] https://librivox.org/pages/public-domain/. O mesmo texto não menciona IA nem machine learning [F].
- O paper HiFiTTS-2 (citado em [Certas Palavras](https://aclanthology.org/2026.propor-1.81.pdf)) relata que removeu locutores do LibriVox que pediram expressamente para não ter suas gravações usadas em machine learning [F, via esse paper]. Se um pipeline próprio de LibriVox PT for construído, convém checar essa lista de opt-out [I].
- CML-TTS e MLS já são derivados do LibriVox e foram publicados em CC BY 4.0 [F]. Para treino, usar essas versões é mais barato do que montar a partir do LibriVox bruto [I]; o único ganho do bruto seria taxa de amostragem maior, e isso depende do MP3 original [I].
- O volume de PT no LibriVox é pequeno (~284,6 h e ~31 leitores em 2020, segundo a tabela 1 do paper do MLS) [F]. Não há como ter muito mais do que o que MLS/CML-TTS já cobrem [I].

---

## 5. Recomendação para o fine-tune

1. **Fala limpa (alvo), PT-BR, com speaker ID:** CML-TTS pt (24 kHz) como núcleo; MLS pt só se o treino aceitar 16 kHz [I]. Total ~69 h, 48 falantes; é pouco para generalizar, mas serve como complemento do corpus principal em inglês [I].
2. **Variedade e acústica real:** Common Voice pt-BR (CC0, 48 kHz nominal). Filtrar por DNSMOS/SNR, descartar clipes com ruído de fundo, e manter só os de `client_id` com muitos clipes se o enrollment exigir; ~100 h pt-BR no total [I]. Atenção à cláusula de "não determinar identidade".
3. **Altas frequências:** TTS-Portuguese Corpus (CC BY, 48 kHz, 1 falante) e as vozes pt-BR da Open Home Foundation (CC0); pequenos, mas são as únicas fontes abertas conhecidas aqui com banda cheia potencial [I; taxa das OHF não confirmada].
4. **Teste/avaliação apenas:** LapsBM (54 min, 35 falantes, 22,05 kHz), TAGARELA, CORAA, MuPe, NURC-SP, Certas Palavras. Os de licença NC/ND não devem alimentar treino nem gerar derivados distribuídos.
5. **Para personalização por falante,** o dado de enrollment vem do próprio usuário; os corpora acima servem só para o treino base de speaker-conditioning [I].
6. **Atribuição:** CC BY (MLS, CML-TTS, TTS-Portuguese Corpus) exige crédito; incluir no NOTICE/model card dos pesos [I].

## 6. O que ficou sem confirmar

- Taxa de amostragem das vozes da Open Home Foundation (WEBM; página não declara).
- Licença e fonte do real speech do BRSpeech-DF; licença exata do BRSD v2 como pacote.
- Número definitivo de falantes do MLS pt (README dos tarballs não aberto).
- Páginas de licença do CORAA ASR no HF (HTTP 401); usei o paper e a tabela do Certas Palavras.
- Corpus comercial de 97 falantes a 48 kHz (DataoceanAI): só visto em resultado de busca, não verificado.
