# 04 - Datasets para o fine-tune do pDFNet3 (DeepFilterNet3, 48 kHz)

Data da pesquisa: 2026-10-03. Pesquisa web apenas, nada foi baixado.
Contexto: pesos distribuídos como MIT OR Apache-2.0, uso comercial possível. Alvo: falante enrolado preservado; ruído e falantes concorrentes suprimidos; foco pt-BR. Decisões vigentes (`Obsidian Vault/decisoes/2026-10-02-clearcore-pipeline-studio-arquitetura.md`): Common Voice pt e MLS pt liberados para treino; TAGARELA (CC BY-NC-SA 4.0) só para teste. A nota mantém a licença do Common Voice pt como "não confirmada"; ver seção 2.

Convenção: **[F]** = fato lido numa fonte (link na linha ou na seção Fontes); **[I]** = inferência minha, não registrada em fonte. Onde uma fonte era secundária (resumo de busca, não a página oficial), está dito.

---

## 0. Resumo executivo

1. **Dois candidatos pedidos estão eliminados por licença NC [F]:** EARS (CC BY-NC 4.0) e Expresso (CC BY-NC 4.0). WHAM! noise também é CC BY-NC 4.0 (fonte secundária, ver 1.3). Nenhum entra em treino de pesos distribuídos. CETUC não tem licença explícita e é descrito como "só pesquisa" (fonte secundária): fora.
2. **Fala limpa full-band (48 kHz) com licença livre é escassa [F]:** VCTK 0.92 (CC BY 4.0, 44 h, 110 falantes, inglês) e TTS-Portuguese Corpus (CC BY 4.0, 10,5 h, **1 falante**, pt-BR). Todo o resto em pt-BR é 16 kHz (MLS) ou 24 kHz (CML-TTS) ou MP3 de microfone variado (Common Voice).
3. **O pt-BR de licença livre tem poucos falantes, exceto o Common Voice [F]:** MLS pt tem 42 falantes de treino + 20 de dev/test; CML-TTS pt tem 48 no total (é derivado do MLS: não somar os dois). Common Voice pt 23.0 tem 3.801 falantes. Para treinar o condicionamento por falante (FiLM/enrollment), o Common Voice é a única fonte pt com diversidade de falantes.
4. **Dados de 16 kHz servem, com ressalvas [F+I]:** o próprio DeepFilterNet tem suporte (`--max_freq` no `prepare_data.py` e low-pass do ruído quando a fala tem taxa menor). Não podem ser a única fonte do fine-tune; misturar com 48 kHz (seção 4).
5. **O gargalo de disco é o HDF5 processado, não os downloads [I, aritmética sobre fato]:** PCM int16 48 kHz mono = 345,6 MB/hora. 25 GB = 72 h em PCM 48 kHz. Usar `--codec flac` (existe no script [F]) e armazenar fontes de 16/24 kHz na taxa nativa (se o libDF reamostrar na leitura, a verificar) para caber mais horas.
6. **Conjunto mínimo (~24 GB finais, ~154 h de fala, ~31 h de ruído):** seção 5. **Completo (~90 GB finais, ~700 h de fala, ~180 h de ruído):** seção 6.

---

## 1. Tabelas de candidatos

Legenda "Spk ID": se o corpus traz identificador de falante utilizável para enrollment (par enrollment/alvo do mesmo falante).
Legenda "Adequação": FL = fala limpa; RU = ruído; RIR = resposta ao impulso; INT = útil como falante concorrente.

### 1.1 Fala limpa

| Dataset | Licença exata | SR nativa | Horas | Falantes | Idioma | Disco (bruto) | Spk ID | Adequação / veredito |
|---|---|---|---|---|---|---|---|---|
| **VCTK 0.92** ([Edinburgh DataShare](https://datashare.ed.ac.uk/handle/10283/3443), DOI 10.7488/ds/2645) | CC BY 4.0 [F, resultado de busca sobre a página da DataShare, `?show=full`; o `license_text.txt` do zip não foi aberto]. Atenção: o README do DNS-Challenge lista o VCTK como "ODC-By v1.0", a versão antiga; a 0.92 declara CC BY 4.0 | 48 kHz (gravado a 96 kHz, distribuído a 48 kHz/16 bit) [F] | ~44 h (110 x ~400 frases) [F] | 110 (109 com transcrição; p315 perdido) [F] | en (sotaques variados) | zip 10,94 GB [F]; o zip traz mic1+mic2 (os mesmos 44 h gravados por dois microfones) [I] | Sim (pXXX) [F] | **FL, full-band, câmara hemi-anecoica. Núcleo do conjunto 48 kHz.** Cuidado: é a base do benchmark VCTK-DEMAND, então os falantes de teste desse benchmark devem ficar fora do treino se for reportar métricas nele [I] |
| **DNS Challenge 5 - fala** ([GitHub microsoft/DNS-Challenge](https://github.com/microsoft/DNS-Challenge)) | Por fonte, no README [F]: LibriVox "public domain"; VCTK ODC-By v1.0; PTDB-TUG ODbL 1.0 (o README também a cita como ODC-By em outro trecho); VocalSet CC BY 4.0; CREMA-D ODC DbCL; VoxCeleb2 "CC BY 4.0"; Edinburgh 56 speaker "licença no link do dataset" | 48 kHz (full-band) [F] | DFN3 treinou com "mais de 750 h" de fala full-band do DNS [F, [arXiv 2110.05588](https://arxiv.org/abs/2110.05588) §3.1] | milhares | en, de, es, fr, it, ru (sem pt) [F] | read_speech 299 GB, German 319 GB, French 62 GB, Spanish 65 GB, Italian 42 GB, Russian 12 GB, emotional 2,4 GB, VCTK 27 GB, VocalSet 974 MB; clean total ~827 GB [F] | Parcial (depende da subfonte) [I] | **Útil só como complemento em inglês e só a parte LibriVox/VCTK/VocalSet.** Excluir VoxCeleb2 (áudio vem de YouTube, a licença CC BY 4.0 do README cobre anotações; áudio sem garantia [I]); excluir PTDB-TUG (ODbL é share-alike sobre a base [I]) e CREMA-D (DbCL) por incerteza de efeito sobre pesos. Sem português |
| **LibriTTS-R** ([OpenSLR 141](https://www.openslr.org/141/)) | CC BY 4.0 [F] | 24 kHz [F] | ~585 h [F] | 2.456 [F, citado em [arXiv 2306.10097](https://arxiv.org/abs/2306.10097)] | en | train-clean-100 8,1 GB; train-clean-360 28 GB; train-other-500 46 GB; total treino 82,1 GB; dev/test ~1-1,3 GB cada [F] | Sim (pasta por falante) [I, estrutura LibriTTS] | **FL (restaurado por modelo neural, vem limpo, mas com artefatos de síntese [I]). Banda acima de 12 kHz vazia.** Boa diversidade de falantes em inglês. clean-100 ≈ 54 h / 247 falantes [I, números do LibriTTS original, de memória] |
| **EARS** ([GitHub](https://github.com/facebookresearch/ears_dataset)) | **CC BY-NC 4.0** [F] (o README escreve "CC-NC 4.0 International") | 48 kHz [F] | 100 h [F] | 107 [F] | en | n/d | Sim (p001-p107) | **PROIBIDO (NC).** Só para teste local, como o TAGARELA |
| **Expresso** ([HF ylacombe/expresso](https://huggingface.co/datasets/ylacombe/expresso); [repo textlesslib](https://github.com/facebookresearch/textlesslib/tree/main/examples/expresso), arquivado em 2024-11-01) | **CC BY-NC 4.0** [F, página do HF; o README do repo não declara] | 48 kHz/24 bit [F] | 40 h (11 h lido, 30 h improvisado) [F] | 4 [F] | en | 5,76 GB (versão HF) [F] | Sim, mas só 4 | **PROIBIDO (NC).** Além disso só 4 falantes |
| **Common Voice pt 23.0** ([Mozilla Data Collective](https://mozilladatacollective.com/datasets/cmflnn483xz7xpuiogors5llv)) | **CC0 1.0** [F: o Mozilla declara áudio e transcrições dedicados ao domínio público sob CC0 ([blog Mozilla 2019](https://blog.mozilla.org/blog/2019/02/28/sharing-our-common-voices-mozilla-releases-the-largest-to-date-public-domain-transcribed-voice-dataset/); [discourse](https://discourse.mozilla.org/t/licensing-and-contribution-to-common-voice/31746)). O campo "license" da página 23.0 pt em si não pôde ser extraído (página renderizada em JS): abrir no navegador para fechar a confirmação]. Termos de acesso (fonte secundária, resumo de busca): proibido tentar identificar falantes; proibido re-hospedar o dataset | MP3; a página 23.0 pt não informa a SR na minha extração. Os clips do Common Voice são MP3 a 48 kHz [I, de memória], mas a **banda efetiva** costuma ser bem menor (microfone de notebook/celular e MP3 lossy) [I] | 230 h gravadas, 181 h validadas [F] | 3.801 [F] | pt (misto pt-BR/pt-PT, sotaques variados [I]) | 4,79 GB (MP3) [F] | Sim: `client_id` (hash) no TSV [I, de memória]; é pseudônimo, serve para agrupar e não para identificar | **Fala ruidosa/variável, não é "limpa".** Usar como alvo sem filtrar ensina o modelo a preservar ruído. Filtrar por SNR/DNSMOS, ver 4.3. É a única fonte pt com milhares de falantes |
| **MLS Portuguese** ([OpenSLR 94](https://www.openslr.org/94/); [HF](https://huggingface.co/datasets/facebook/multilingual_librispeech)) | CC BY 4.0 [F] (áudio original: LibriVox, domínio público) | **16 kHz** [F, `sampling_rate: 16000` no card do HF] | 160,96 h treino + 3,64 dev + 3,74 test = **168,3 h** [F] | 42 treino (26 H/16 M) + 10 dev + 10 test = 62 [F] | pt (LibriVox; pt-BR e pt-PT misturados [I, inferido do paper CML-TTS, que cita diferenças ortográficas entre países]) | `mls_portuguese.tar.gz` 9,3 GB (FLAC) ou 2,5 GB (Opus) [F] | Sim (nome do arquivo) [I] | **FL razoável (voluntários em casa, microfones variados), 16 kHz. Poucos falantes.** Preferir FLAC; o Opus é lossy [I] |
| **CML-TTS pt** ([OpenSLR 146](https://www.openslr.org/146/); [paper](https://arxiv.org/abs/2306.10097)) | CC BY 4.0 [F] | **24 kHz** [F] | ~69 h (soma da Tabela 1 do paper: 23,14 + 44,81 + 0,28 + 0,24 + 0,68 + 0,20) [F, interpretação da tabela minha] | ~48 (20+10+5+4+6+3) [F, idem] | pt | `cml_tts_dataset_portuguese_v0.1.tar.bz` 9,7 GB [F] | Sim | **FL, 24 kHz. Derivado do MLS (mesmas gravações LibriVox, resegmentado). Não somar com o MLS sem desduplicar por falante** [F+I] |
| **TTS-Portuguese Corpus** ([arXiv 2005.05144](https://arxiv.org/abs/2005.05144)) | CC BY 4.0 [F, resumo de busca sobre o paper; confirmar no repositório do corpus] | **48 kHz**, 32 bit [F] | 10 h 28 min [F] | **1** (homem, pt-BR, não profissional, sem isolamento acústico) [F] | pt-BR | 3.632 arquivos; ~3,6 GB (16 bit) a ~7,2 GB (32 bit) [I, aritmética] | n/a (1 falante) | **FL full-band em pt-BR. Único, mas sem diversidade de falantes: serve para a banda alta e a fonética pt-BR, não para o enrollment** |
| **CETUC** (Alcaim et al.; [CORAA paper](https://arxiv.org/pdf/2110.15731)) | **Sem licença explícita** ("publicamente disponível, sem licença explícita"; a página do Alcaim diz "para fins de pesquisa exclusivamente") [F, fontes secundárias] | 16 kHz | 145 h | 100 (50 H/50 M) | pt-BR | n/d | Sim | **Excluir do treino** (licença não confirmada, aparente restrição a pesquisa). Candidato a teste local. Mesmo vale para Constituição/LapsBM/VoxForge-pt: licença não verificada [I] |
| **HiFi-TTS** ([OpenSLR 109](https://www.openslr.org/109/)) | CC BY 4.0 [F] | 44,1 kHz [F] | 291,6 h (≥17 h/falante) [F] | 10 [F] | en | **41 GB** num único arquivo [F] | Sim (10) | FL, banda larga real. Só 10 falantes; o arquivo único de 41 GB não cabe no disco sem extração em stream (`curl | tar -x` filtrado) [I] |
| **LibriSpeech** ([OpenSLR 12](https://www.openslr.org/12/)) | CC BY 4.0 [F] | **16 kHz** [F] | ~1.000 h [F] | ~2.484 [I, número conhecido; a página não informa] | en | clean-100 6,3 GB; clean-360 23 GB; other-500 30 GB [F] | Sim | FL com banda limitada (origem MP3 do LibriVox [I]). **Redundante com LibriTTS-R (mesma fonte, 24 kHz, melhor): preferir o LibriTTS-R** |

### 1.2 Ruído

| Dataset | Licença exata | SR nativa | Horas | Disco | Adequação / veredito |
|---|---|---|---|---|---|
| **DNS Challenge - noise_fullband** ([repo](https://github.com/microsoft/DNS-Challenge)) | Freesound: "Only files with CC0 licenses were selected" [F]; AudioSet: README declara CC BY 4.0 [F] (o áudio vem de YouTube, a licença CC BY do README é a do conjunto/anotações [I]: risco residual menor, mas não zero); DEMAND dentro do pacote: CC BY-SA 3.0 [F] | 48 kHz [F] | DFN: "180 h de vários tipos de ruído" [F, [arXiv 2110.05588](https://arxiv.org/abs/2110.05588)] | `Noise_fullband` 58 GB [F] | **Melhor fonte de ruído full-band com licença declarada.** Pegar fatia (~10 h = ~3 GB no mínimo). Remover o que for DEMAND (CC BY-SA, reservar para teste) |
| **MUSAN** ([OpenSLR 17](https://www.openslr.org/17/); [paper](https://arxiv.org/abs/1510.08484)) | OpenSLR: CC BY 4.0 [F]. Paper: ~109 h "em domínio público ou Creative Commons", "todo o conteúdo permite uso comercial"; cada subdiretório tem arquivo `LICENSE` por WAV [F, [ar5iv](https://ar5iv.labs.arxiv.org/html/1510.08484)] | **16 kHz** WAV [F] | fala 60 h, música 42,5 h, **ruído 6 h** [F] | 11 GB (tar.gz) [F] | Ruído (6 h, só 16 kHz) e música (como interferente) úteis, **mas limitados a 8 kHz de banda**: usar com `max_freq` 8000, nunca como única fonte de ruído. Os 60 h de fala servem como falante concorrente (en, LibriVox e governo EUA) |
| **DEMAND** ([Zenodo 1227121](https://zenodo.org/records/1227121)) | **CC BY-SA 3.0** no README do dataset e no DNS [F]; o metadado do Zenodo mostra CC BY 4.0 [F, a mesma página mostra os dois]. Tratar como o mais restritivo (BY-SA) [I] | 48 kHz e 16 kHz [F] | 15 ambientes, 16 canais (5 cm a 21,8 cm), ~5 min/canal [I, duração de memória] | zips de 78-349 MB por ambiente [F] | **Reservar para teste/validação** (é o ruído do benchmark VCTK-DEMAND). Share-alike sobre pesos treinados é questão jurídica aberta: não arriscar no treino |
| **WHAM! / WHAM!48kHz** ([anúncio](https://www.auditory.org/mhonarc/2020/msg00544.html)) | **CC BY-NC 4.0** [F, fonte secundária: LibriMix, [arXiv 2005.11262](https://arxiv.org/abs/2005.11262), e resumo de busca; a página oficial não foi acessível (certificado inválido)] | 48 kHz / 24 bit, estéreo binaural [F] | ~78 h [F] | n/d (~80 GB estimados em 24 bit estéreo [I]) | **PROIBIDO (NC).** Só teste local |
| **FSD50K** ([Zenodo 4060432](https://zenodo.org/records/4060432)) | CC por clipe [F]. Dev (40.966 clipes): CC0 14.959, CC BY 20.017, CC BY-NC 4.616, CC Sampling+ 1.374. Eval (10.231): CC0 4.914, CC BY 3.489, CC BY-NC 1.425, CC Sampling+ 403 [F]. O dataset em si é "CC BY", nota própria dizendo que a escolha "não é direta" [F] | 44,1 kHz mono 16 bit [F] | 108,3 h (dev 80,4 + eval 27,9) [F] | dev ~24,7 GB (6 partes), eval ~6,2 GB (2 partes) [F] | **Filtrar CC0 + CC BY pelo JSON de metadados** (excluir NC e Sampling+ [I]). Sobra ~34.976 clipes no dev e 8.403 no eval (~85% do total; ~69 h dev + ~23 h eval se proporcional [I]). **Remover classes de voz humana** (fala, riso, canto etc.) senão o modelo aprende a preservá-las [I]. O dev é zip partido (precisa baixar as 6 partes); o eval (6,2 GB) é o corte mais barato [I] |

### 1.3 RIR

| Dataset | Licença exata | SR nativa | Tamanho | Observação |
|---|---|---|---|---|
| **SLR26 (RIR simuladas)** ([OpenSLR 26](https://www.openslr.org/26/)) | Apache 2.0 [F] | 8 kHz e 16 kHz (versões 128 MB e 178 MB) [F] | 178 MB | Só 16 kHz: banda limitada. Com `max_freq`/low-pass, ou preferir RIR sintéticas 48 kHz |
| **SLR28 RIRS_NOISES** ([OpenSLR 28](https://www.openslr.org/28/)) | Apache 2.0 [F] (as RIRs reais vêm de RWCP, REVERB 2014 e Aachen AIR; a licença Apache declarada no pacote pode não refletir a de cada origem [I]) | 16 kHz, 16 bit [F] | 1,3 GB | RIRs reais + simuladas + ruídos pontuais do MUSAN. Usado pelo DNS (README do DNS cita SLR26/28 como Apache 2.0) [F] |
| **MIT IR Survey** ([página](https://mcdermottlab.mit.edu/Reverb/IR_Survey.html)) | CC BY 4.0 [F, fonte secundária: cópia no [HF](https://huggingface.co/datasets/hashdoa/MIT_environmental_impulse_responses) e [DSpace@MIT](https://dspace.mit.edu/handle/1721.1/112183); a página do laboratório não declara licença] | n/d (32 kHz [I, de memória]) | pequeno (centenas de MB no máximo [I]) | 271 IRs de 301 locais reais, distância fixa 1,5 m [F]. Boa para realismo; confirmar a licença no DSpace |
| **BUT Speech@FIT ReverbDB** ([página](https://speech.fit.vut.cz/software/but-speech-fit-reverb-database)) | CC BY 4.0 [F] | n/d na página (16 kHz [I, de memória]) | RIR-only 8,7 GB; versão com LibriSpeech retransmitido 117 GB [F] | 9 salas, 31 microfones, 961 RIRs [F]. Baixar só o RIR-only; o de 117 GB está fora de cogitação |
| **OpenAIR** ([site](https://www.openair.hosted.york.ac.uk/?page_id=2)) | **Varia por item**: "a maior parte do conteúdo sob Creative Commons", cada contribuidor escolhe a licença (inclui CC BY-SA e CC BY-NC) [F, resumo de busca; a página do site tem certificado expirado e não abriu] | tipicamente 44,1-96 kHz [I] | pequeno | **Só com triagem item a item** (aceitar CC0/CC BY, rejeitar NC e, se cauteloso, SA). Trabalho manual de curadoria; valor marginal baixo no mínimo |
| **RIR sintéticas 48 kHz (geração própria)** | Sem licença (gerado por nós) | 48 kHz | ~0,5 GB para 10.000 [I, aritmética] | O DFN fez exatamente isso: além das RIRs do DNS a 16 kHz, simulou "10.000 RIRs a 48 kHz com image source model, RT60 0,05 a 1,00 s" [F, arXiv 2110.05588 §3.1]. **Recomendado como base do mínimo** (pyroomacoustics/gpuRIR [I]) |

---

## 2. Sobre a licença do Common Voice pt (decisão registrada como "não confirmada")

- [F] O Mozilla dedica áudio e transcrições do Common Voice ao domínio público sob CC0 1.0 ([blog 2019](https://blog.mozilla.org/blog/2019/02/28/sharing-our-common-voices-mozilla-releases-the-largest-to-date-public-domain-transcribed-voice-dataset/), [discourse](https://discourse.mozilla.org/t/licensing-and-contribution-to-common-voice/31746)).
- [F] Desde outubro de 2025 os datasets só saem pelo Mozilla Data Collective ([nota no HF](https://huggingface.co/datasets/mozilla-foundation/common_voice_17_0)); a versão atual do pt é "Scripted Speech 23.0 - Portuguese" (195.889 clipes, 230 h, 181 h validadas, 3.801 falantes, 4,79 GB MP3 - resumo de busca da página).
- **Não confirmado por mim:** o campo "license" da própria página 23.0 pt (renderizada em JS, o fetch não trouxe o texto) e os termos de acesso do Data Collective. Resumo de busca (fonte secundária) aponta duas condições: não tentar identificar falantes; não re-hospedar. **Ação sugerida ao dono: abrir a página no navegador e anotar a licença e os termos literais na nota do vault.** Se os termos do Data Collective valerem em cima do CC0, "não re-hospedar" não afeta distribuir pesos (não é o dataset), e "não identificar falantes" exige cuidado com o treino do enrollment: usar o `client_id` só para agrupar clipes por falante, nunca para reidentificar [I, risco a validar].
- Release atual no GitHub: [common-voice/cv-dataset](https://github.com/common-voice/cv-dataset) mostra Scripted Speech v27.0 (2026-09) como versão mais recente; a v23.0 é a que a busca devolveu para pt. Conferir se há versão do pt mais nova [F+I].

---

## 3. Restrições de licença por tipo (para o model card)

| Licença | Uso no conjunto | Observação [I salvo indicação] |
|---|---|---|
| CC0 | Common Voice, Freesound (DNS) | Sem exigências |
| CC BY 4.0 | VCTK, LibriTTS-R, MLS, CML-TTS, TTS-Portuguese, HiFi-TTS, MUSAN, BUT ReverbDB, MIT IR, FSD50K (subset) | Exige atribuição: listar todos no model card/NOTICE |
| Domínio público | LibriVox (nas fontes) | |
| Apache 2.0 | SLR26, SLR28 | |
| CC BY-SA 3.0 | DEMAND | Share-alike: efeito sobre pesos é incerto. Manter fora do treino |
| ODbL / DbCL | PTDB-TUG, CREMA-D (DNS) | Idem: manter fora |
| CC BY-NC | EARS, Expresso, WHAM!, FSD50K (parte), OpenAIR (parte) | **Proibido** |
| Sem licença / só pesquisa | CETUC (e outros corpora pt-BR acadêmicos) | Fora do treino |

---

## 4. Questão crítica: dados de 16 kHz servem para um modelo 48 kHz?

### 4.1 O que é fato

- [F] O DeepFilterNet foi desenhado para 48 kHz; o README diz que só aceita wavs a 48 kHz, e o `prepare_data.py` tem `--sr` (padrão 48000), reamostra com `kaiser_best`, e tem `--max_freq`, documentado como útil "para sinais com upsampling, que não têm informação em frequências altas" ([fonte: prepare_data.py](https://raw.githubusercontent.com/Rikorose/DeepFilterNet/main/DeepFilterNet/df/scripts/prepare_data.py)). Também tem `--codec` (pcm/flac/vorbis), `--dtype` (int16/float32), `--mono`.
- [F] O paper do DFN descreve: "aplicamos um filtro passa-baixa ao ruído antes de misturar se a taxa de amostragem do sinal de fala é menor que a do modelo; isso permite que modelos treinados em full-band funcionem igualmente bem em sinais com taxas menores" ([arXiv 2110.05588, §2.5](https://arxiv.org/abs/2110.05588)). O DFN3 foi treinado no DNS4 multilíngue com VCTK e PTDB sobreamostrados 10x ([arXiv 2305.08227](https://arxiv.org/abs/2305.08227)), ou seja, o próprio autor trata os corpora full-band de alta qualidade como o "ouro" do treino.
- [F] O DFN aplica deep filtering só até `f_DF` = 5 kHz; acima disso o modelo faz apenas ganhos por banda ERB (32 bandas) [F, arXiv 2110.05588 §3.1 "NERB = 32, fDF = 5 kHz"].

### 4.2 O que é inferência minha

1. **O que o upsampling faz.** Fala a 16 kHz reamostrada para 48 kHz tem energia zero acima de 8 kHz. Como **alvo**, ela diz ao modelo "fala limpa não tem nada acima de 8 kHz". Se o ruído misturado tem conteúdo acima de 8 kHz (ruído full-band) e o alvo não, o modelo aprende a zerar tudo acima de 8 kHz, e em fala real full-band isso abafa fricativas (/s/, /ʃ/) e o "ar" da voz. É o risco principal.
2. **Por que o low-pass do ruído resolve parcialmente.** Com o ruído também limitado a 8 kHz, as bandas altas ficam vazias em entrada e alvo: o ganho ali é indeterminado, o modelo não é punido nem treinado. O dado de 16 kHz ensina bem a faixa até 8 kHz (inclui toda a faixa do deep filtering, 0-5 kHz, e a parte inteligibilidade), mas **não ensina nada** sobre 8-24 kHz. Esquecimento catastrófico das bandas altas é possível se o fine-tune for *só* com 16 kHz.
3. **Impacto previsto.** O núcleo do aprendizado personalizado (separar o falante enrolado do concorrente) vive na faixa baixa e média: dados 16 kHz bastam para isso. A qualidade percebida em 8-24 kHz depende de manter dados full-band no mix. Efeito exato: não medido, precisa de ablação A/B (ver 4.4).
4. **A decisão do vault já aponta na mesma direção:** o enrollment é áudio 16 kHz (nota de arquitetura), então a condição por falante não usa a banda alta de qualquer forma. BWE está explicitamente fora de escopo; portanto **não** fabricar banda alta por extensão neural nos dados de treino [decisão registrada].

### 4.3 Receita proposta (inferência, com fatos de suporte)

- **Separar os datasets por faixa de banda real**, cada um em seu HDF5, com `--max_freq` = metade da SR nativa (8000 para 16 kHz, 12000 para 24 kHz, `-1` para 44,1/48 kHz): `speech_48k` (VCTK, TTS-Portuguese, HiFi-TTS), `speech_24k` (CML-TTS, LibriTTS-R), `speech_16k` (MLS, Common Voice, MUSAN-fala). O arquivo `config.cfg` do DFN aceita vários datasets por categoria com fator de amostragem [F, README].
- **Pesar a mistura por banda**: sugestão de partida **≥ 50% das amostras de cada batch com fala full-band (48 kHz)**, 30% 24 kHz, ≤ 20% 16 kHz. É heurística minha, não registrada em fonte. Como só há ~55 h de fala full-band livre (VCTK + TTS-Portuguese), isso implica sobreamostrar (o próprio DFN sobreamostrou VCTK/PTDB 10x [F]).
- **Medir a banda efetiva de cada clipe** (rolloff de 99% da energia ou razão de energia acima de 8 kHz) em especial no Common Voice, onde "MP3 a 48 kHz" não garante banda real: clipes com rolloff < 8 kHz entram como 16 kHz (`max_freq` 8000); os raros com banda real larga entram como 48 kHz [I].
- **Filtro de qualidade no Common Voice**: descartar clipes com SNR baixo ou DNSMOS (ou equivalente) abaixo de um limiar, senão o alvo "limpo" carrega ruído; priorizar falantes com muitos clipes [I].
- **Não usar RIR de 16 kHz sem cuidado**: RIR a 16 kHz aplicadas a fala 48 kHz deixam a reverberação sem energia acima de 8 kHz. Usar RIR sintéticas a 48 kHz como base (o DFN fez isso [F]) e as reais de 16 kHz como complemento [I].
- **Teste de verificação barato antes de investir** [I]: gerar 1 h de cada dataset em HDF5, confirmar que o libDF aplica o low-pass do ruído com `max_freq` (o paper diz que sim; não li o código Rust) e confirmar se o libDF reamostra HDF5 de SR diferente na leitura (se sim, dá para guardar 16/24 kHz na taxa nativa e economizar 2-3x de disco; se não, guardar a 48 kHz em FLAC, onde a banda vazia comprime bem).

### 4.4 Ablação recomendada (inferência)

Treinar duas variantes curtas do fine-tune: (A) só fala 48 kHz+24 kHz; (B) A + 16 kHz. Comparar em teste full-band (VCTK-DEMAND com falantes fora do treino, e um conjunto pt-BR local, o TAGARELA) usando métricas de banda alta (LSD acima de 8 kHz, ou espectro médio) além de PESQ/DNSMOS e do TAR/TRR da decisão de enrollment.

---

## 5. Conjunto MÍNIMO viável (alvo: caber em ~25 GB finais)

Premissas de espaço **[I, aritmética, a validar com 1 h de amostra]**: HDF5 PCM int16 a 48 kHz = 0,346 GB/h; FLAC ≈ 0,5x disso (≈ 0,17 GB/h a 48 kHz, ≈ 0,086 GB/h a 24 kHz, ≈ 0,058 GB/h a 16 kHz, se o libDF aceitar SR nativa por dataset). Todos os brutos são **transitórios**: converter e apagar na ordem abaixo, mantendo o pico (HDF5 acumulado + um bruto + extração) < 31 GB.

| # | Fonte | Horas | Falantes | SR armazenada | HDF5 final (est.) | Download bruto transitório |
|---|---|---|---|---|---|---|
| 1 | VCTK 0.92 (só mic1) | 44 | 110 | 48 kHz | 7,5 GB | 10,94 GB (zip) |
| 2 | TTS-Portuguese Corpus | 10,5 | 1 | 48 kHz | 1,8 GB | 3,6-7,2 GB |
| 3 | CML-TTS pt (já cobre o MLS) | ~69 | ~48 | 24 kHz | 6,0 GB | 9,7 GB (tar.bz; extrair em stream) |
| 4 | Common Voice pt 23.0, subset filtrado, maximizando falantes (~30 h) | 30 | centenas | 16 kHz | 1,7 GB | 4,79 GB (zip inteiro; filtrar depois) |
| | **Subtotal fala** | **~153,5 h** | | | **17,0 GB** | |
| 5 | DNS noise_fullband, fatia CC0/AudioSet (~10 h) | 10 | n/a | 48 kHz | 3,2 GB | ~3 GB (por blocos) |
| 6 | FSD50K eval filtrado CC0+CC BY, sem classes de voz (~15 h de 23 h) | ~15 | n/a | 48 kHz | ~2,6 GB | 6,2 GB (2 arquivos) |
| 7 | MUSAN, só ruído (6 h) | 6 | n/a | 16 kHz | 0,4 GB | 11 GB (tar.gz inteiro) |
| | **Subtotal ruído (DEMAND fica de fora, reservado para teste)** | **~31 h** | | | **6,2 GB** | |
| 8 | RIR: 10.000 sintéticas 48 kHz (geradas) + SLR28 (reais, 16 kHz) | n/a | n/a | 48 / 16 kHz | ~0,6 GB | 1,3 GB |
| | **TOTAL MÍNIMO** | **~154 h fala + ~31 h ruído + RIR** | | | **~24 GB** | pico de bruto: ~11 GB (MUSAN, VCTK) |

Notas:
- Falantes concorrentes: sorteados dos próprios corpora de fala (outro `spk_id`), sem custo extra de disco [I].
- Falantes pt: CML-TTS (~48) + Common Voice subset + 1 (TTS-Portuguese); a diversidade pt para enrollment **depende do Common Voice**. Se couber, subir o CV para ~60 h (3,4 GB) trocando 30 h de FSD50K.
- MUSAN custa 11 GB de download para 6 h de ruído: se a banda for apertada, cortar o MUSAN (a perda é pequena, o ruído 16 kHz é o de menor valor aqui) e ganhar margem.
- Teste (fora do treino, não contam no orçamento): TAGARELA, DEMAND, CETUC (se o dono quiser), mais um corte de VCTK reservado por falante.

---

## 6. Conjunto COMPLETO (se o usuário liberar disco: ~100 GB livres)

Mesmas premissas de espaço da seção 5 **[I]**.

| # | Fonte | Horas | Falantes | SR armazenada | HDF5 final (est.) |
|---|---|---|---|---|---|
| 1 | VCTK 0.92 | 44 | 110 | 48 | 7,5 GB |
| 2 | TTS-Portuguese Corpus | 10,5 | 1 | 48 | 1,8 GB |
| 3 | CML-TTS pt | ~69 | ~48 | 24 | 6,0 GB |
| 4 | MLS pt (se incluído: desduplicar contra o CML-TTS por falante; hours marginais desconhecidas) | até 168 | até 62 | 16 | até 9,8 GB |
| 5 | Common Voice pt 23.0, filtrado | ~120 | até 3.801 | 16 (ou 48 para os clipes de banda real) | 7,0 GB |
| 6 | LibriTTS-R train-clean-100 + clean-360 | ~245 | ~1.150 | 24 | 21,1 GB |
| 7 | HiFi-TTS, subset (~40 h; extração em stream de 41 GB) | ~40 | 10 | 48 (44,1 reamostrado) | 6,8 GB |
| | **Subtotal fala** | **~700 h** | | | **~60 GB** |
| 8 | DNS noise_fullband (~60 h) | 60 | n/a | 48 | ~12 GB |
| 9 | FSD50K dev+eval CC0+CC BY, sem voz (~70 h) | ~70 | n/a | 48 | ~12 GB |
| 10 | MUSAN ruído + música (48,5 h) | 48,5 | n/a | 16 | 2,8 GB |
| | **Subtotal ruído** | **~180 h** | | | **~27 GB** |
| 11 | RIR: 10.000 sintéticas 48 kHz + SLR26/28 + MIT IR (271) + BUT ReverbDB (RIR-only, 961) + OpenAIR filtrado CC0/CC BY | n/a | n/a | 48/16/32/? | ~2 GB |
| | **TOTAL COMPLETO** | **~700 h fala + ~180 h ruído** | | | **~90 GB** |

Pico de bruto: ~41 GB (HiFi-TTS, só viável com extração em stream) ou 31 GB (FSD50K dev, 6 partes) somado ao HDF5 já gerado.
Sem o LibriTTS-R clean-360 (-16 GB) e o HiFi-TTS (-6,8 GB) o completo cai para ~67 GB e ~460 h de fala.

---

## 7. Lacunas e itens a conferir

1. **Licença do Common Voice pt 23.0 na página do Data Collective** e termos de uso literais (não extraídos). Atualizar a nota do vault depois.
2. **`license_text.txt` do zip do VCTK** (confirma CC BY 4.0 vs ODC-By; a página da DataShare, via busca, diz CC BY 4.0).
3. **WHAM! CC BY-NC 4.0**: só fonte secundária (LibriMix). A página oficial não abriu. Não muda a decisão (excluir).
4. **Licença do TTS-Portuguese Corpus** (CC BY 4.0 por resumo do paper): confirmar no repositório do corpus.
5. **SR e tamanho de MIT IR, BUT ReverbDB e OpenAIR**: não confirmados.
6. **Comportamento do libDF** com `max_freq` e com HDF5 em SR diferente de 48 kHz: confirmado só o script de preparo e o paper; falta ler o dataloader Rust ou testar com 1 h de dados.
7. **Fator FLAC (~0,5) e tamanhos de HDF5**: estimativas, medir antes de planejar disco.
8. **CETUC/Constituição/LapsBM/VoxForge-pt**: licença não verificada na fonte original; tratados como fora do treino.
9. **Parte pt-BR vs pt-PT** do MLS/CML-TTS/Common Voice: não medida (nenhum rótulo de variante nas fontes lidas). Se o produto for só pt-BR, filtrar por sotaque/metadado onde existir.
10. **Contaminação de teste:** VCTK-DEMAND usa falantes VCTK + DEMAND; MLS e CML-TTS compartilham falantes; evitar vazamento entre treino e teste.

---

## Fontes

- VCTK 0.92: https://datashare.ed.ac.uk/handle/10283/3443 ; https://datashare.ed.ac.uk/handle/10283/3443?show=full
- DNS Challenge: https://github.com/microsoft/DNS-Challenge
- LibriTTS-R: https://www.openslr.org/141/
- EARS: https://github.com/facebookresearch/ears_dataset
- Expresso: https://huggingface.co/datasets/ylacombe/expresso ; https://github.com/facebookresearch/textlesslib/tree/main/examples/expresso
- Common Voice: https://mozilladatacollective.com/datasets/cmflnn483xz7xpuiogors5llv ; https://blog.mozilla.org/blog/2019/02/28/sharing-our-common-voices-mozilla-releases-the-largest-to-date-public-domain-transcribed-voice-dataset/ ; https://discourse.mozilla.org/t/licensing-and-contribution-to-common-voice/31746 ; https://github.com/common-voice/cv-dataset ; https://huggingface.co/datasets/mozilla-foundation/common_voice_17_0
- MLS: https://www.openslr.org/94/ ; https://huggingface.co/datasets/facebook/multilingual_librispeech ; https://arxiv.org/abs/2012.03411
- CML-TTS: https://www.openslr.org/146/ ; https://arxiv.org/abs/2306.10097 ; https://huggingface.co/datasets/ylacombe/cml-tts
- TTS-Portuguese Corpus: https://arxiv.org/abs/2005.05144
- CETUC/CORAA: https://arxiv.org/pdf/2110.15731 ; https://app.alphaneural.io/datasets/Racoci/alcaim
- HiFi-TTS: https://www.openslr.org/109/
- LibriSpeech: https://www.openslr.org/12/
- MUSAN: https://www.openslr.org/17/ ; https://arxiv.org/abs/1510.08484 ; https://ar5iv.labs.arxiv.org/html/1510.08484
- DEMAND: https://zenodo.org/records/1227121
- WHAM!: https://www.auditory.org/mhonarc/2020/msg00544.html ; https://arxiv.org/abs/2005.11262
- FSD50K: https://zenodo.org/records/4060432
- RIR: https://www.openslr.org/26/ ; https://www.openslr.org/28/ ; https://mcdermottlab.mit.edu/Reverb/IR_Survey.html ; https://dspace.mit.edu/handle/1721.1/112183 ; https://speech.fit.vut.cz/software/but-speech-fit-reverb-database ; https://www.openair.hosted.york.ac.uk/?page_id=2
- DeepFilterNet: https://arxiv.org/abs/2110.05588 ; https://arxiv.org/abs/2305.08227 ; https://github.com/Rikorose/DeepFilterNet ; https://raw.githubusercontent.com/Rikorose/DeepFilterNet/main/DeepFilterNet/df/scripts/prepare_data.py
