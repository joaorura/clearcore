# Speaker encoder para o enrollment de voz do Clearcore

Data: 2026-10-03. Pesquisa web apenas (nada instalado/baixado). Convenção: **[FATO]** = verificado em fonte linkada nesta pesquisa; **[INFERÊNCIA]** = raciocínio nosso, não registrado em fonte; **[NÃO VERIFICADO]** = lembrança que não pude confirmar.

## 1. Contrato do modelo de enrollment (do código)

Fonte: `crates/model/src/enrollment.rs`, `crates/model/src/voice_profile.rs`.

- **[FATO]** Entrada: 1 tensor `audio`, `f32`, `[1, N]`, mono, **16 kHz** (`ENROLLMENT_SAMPLE_RATE_HZ = 16_000`). Duração aceita: 6 s a 12 s (96.000 a 192.000 amostras). Pico máximo 0,99, RMS mínimo -40 dBFS, fração mínima de fala 60% (VAD de energia).
- **[FATO]** Saídas: `gamma_enc`, `beta_enc`, `gamma_df`, `beta_df`, cada uma com 256 floats (`FILM_HIDDEN_DIM`; qualquer shape que achate para 256).
- **[FATO]** Limites físicos (rejeitados, nunca clampados): gamma em [0.001, 100], beta em [-50, 50]. Identidade: gamma=1, beta=0.
- **[FATO]** Asset: `voice-enrollment-asset-v1` (tar.gz com `enrollment.onnx`), carregado via `tract-onnx`, assinado; roda fora do caminho de tempo real.
- **[INFERÊNCIA]** Consequência: o ONNX recebe áudio cru, então o front-end (fbank/log-mel) precisa estar dentro do grafo ou ser trazido para o Rust antes do encoder. A latência do encoder não é restrição (roda uma vez, offline, sobre no máximo 12 s); o que pesa é tamanho do asset, simplicidade de operadores no tract e qualidade.

## 2. Situação de licença do VoxCeleb (ponto crítico)

**[FATO]** O que as fontes primárias dizem, e elas **não são consistentes entre si**:

- Página Oxford VGG (vox1 e vox2): "The provided VoxCeleb metadata is licensed under a Creative Commons Attribution-ShareAlike 4.0 International License" (CC BY-SA 4.0, e só para os **metadados**). A mesma página diz que URLs, timestamps, áudios e metadados "are no longer available from this website". https://www.robots.ox.ac.uk/~vgg/data/voxceleb/vox1.html e https://www.robots.ox.ac.uk/~vgg/data/voxceleb/vox2.html
- Página espelho do dataset (KAIST/mm): "available to download **for research purposes** under a Creative Commons Attribution 4.0 International License. The copyright remains with the original owners of the video." https://mm.kaist.ac.kr/datasets/voxceleb/
- `license.txt` do mesmo espelho: CC BY 4.0; "The copyright of both the original and cropped versions of the videos remains with the original owners"; baixar implica seguir as mesmas condições para modificação/redistribuição. https://mm.kaist.ac.kr/datasets/voxceleb/files/license.txt
- O áudio vem de vídeos do YouTube. A CC BY aplicada pelos autores cobre o trabalho deles (anotações/curadoria); **não** concede direitos sobre o áudio de terceiros.

**[INFERÊNCIA]** Leitura prática:
1. Não existe cláusula explícita que permita uso comercial do **áudio**; existe, em ao menos uma página oficial-espelho, a expressão "research purposes". Três textos diferentes (CC BY, CC BY-SA, "research purposes") para o mesmo dataset são, por si, um sinal de risco de procedência.
2. Se os **pesos** treinados em VoxCeleb são "obra derivada" do áudio é questão jurídica **em aberto** (sem precedente claro; análises recentes tratam como incerta e aconselham postura conservadora com datasets NC/SA: https://www.emergentmind.com/open-problems/derivative-work-status-of-llms, fonte secundária e fraca). Não afirmo que contamina, nem que não contamina.
3. Na prática de mercado o risco é tolerado: SpeechBrain publica ECAPA-VoxCeleb como Apache-2.0 e o Praat (GPL) embute um modelo treinado em VoxCeleb citando a CC BY 4.0 (https://www.fon.hum.uva.nl/praat/manual/VoxCeleb_CC-BY-4_0_license.html). Isso mostra que a licença **do peso** é permissiva; **não** mostra que o licenciante tinha direito de concedê-la sobre os dados.
4. Para um projeto que promete "pesos sob MIT OR Apache-2.0", o enunciado limpo é "treinado só em dados CC BY/CC0". Com VoxCeleb no encoder enviado, o enunciado passa a ser "licença permissiva do peso, procedência dos dados cinzenta".
5. O encoder congelado **vai dentro** do `enrollment.onnx` distribuído, então a pergunta de contaminação se aplica ao asset inteiro, não só a um artefato intermediário. Usar um encoder VoxCeleb como **professor** (distilação) ou como juiz de avaliação é mais fraco como risco, mas não elimina a dependência de procedência se o aluno aprende a reproduzir o espaço do professor.

## 3. Comparativo dos candidatos

| Candidato | Licença dos pesos | Dados de treino | Tamanho | Qualidade | ONNX / tract |
|---|---|---|---|---|---|
| **SpeechBrain ECAPA (spkrec-ecapa-voxceleb)** | Apache-2.0 (metadado HF) | VoxCeleb1+2 (train) | **[NÃO VERIFICADO]** ~6M (C=512) a ~15M (C=1024) no paper; 3D-Speaker lista ECAPA-TDNN com 20,8 M | EER 0,80% Vox1-test (cleaned); 16 kHz mono; embedding 192-d **[NÃO VERIFICADO na página]** | Sem ONNX oficial na página; front-end (Fbank, norm) é Python. Arquitetura (Conv1d dilatada, Res2Net split/concat, SE, atenção+Softmax, BatchNorm) = operadores comuns |
| **WeSpeaker ResNet34** | Repositório Apache-2.0; HF `Wespeaker/wespeaker-voxceleb-resnet34` marcado Apache-2.0; o `pretrained.md` diz que o modelo "follows the license of its corresponding dataset" (CC BY 4.0); `pyannote/wespeaker-voxceleb-resnet34-LM` marcado CC-BY-4.0 | VoxCeleb | 6,63 M params; ONNX de 26,5 MB | EER Vox1-O 0,723% (baseline VoxSRC2023) | ONNX oficial (recebe fbank já calculado). ResNet 2D: operadores comuns, mais pesado em CPU |
| **WeSpeaker CAM++** | idem (HF `wespeaker-voxceleb-CAM++` retornou 401 na checagem) | VoxCeleb | 7,18 M params | EER Vox1-O 0,654% | ONNX oficial. Pooling de segmento do CAM (avg-pool com ceil_mode + expand) é o ponto exótico para tract **[INFERÊNCIA]** |
| **3D-Speaker (ERes2Net, CAM++, ERes2NetV2)** | Código Apache-2.0; **licença dos pesos no ModelScope e do dataset 3D-Speaker não confirmada** (página não carregou) | Modelos principais: 200k falantes de mandarim (rotulado); recipes para Vox/CN-Celeb | CAM++ 7,2 M; ERes2Net-base 6,61 M; ERes2NetV2 17,8 M | Vox1-O: CAM++ 0,65%, ERes2NetV2 0,61%, ERes2Net-large 0,52% (README) | Há ONNX Runtime no repo (2024-04). Modelos mandarim: viés de idioma para o nosso uso **[INFERÊNCIA]** |
| **NVIDIA TitaNet-L (NeMo)** | CC-BY-4.0 (https://huggingface.co/nvidia/speakerverification_en_titanet_large) | VoxCeleb1+2, **Fisher, Switchboard, LibriSpeech, SRE 2004-2010** | 23 M params | EER 0,66% Vox1 (cleaned) | Existe ONNX comunitário (Recogment/titanet-large-onnx). **[INFERÊNCIA]** Fisher/Switchboard/SRE são corpora LDC (licença própria, paga): procedência **pior** que a do VoxCeleb |
| **Resemblyzer / GE2E** | Código Apache-2.0 (https://github.com/resemble-ai/Resemblyzer) | O encoder do Real-Time-Voice-Cloning: VoxCeleb1, VoxCeleb2 e LibriSpeech-other (8,4k falantes, segundo a literatura citada; não está no README) | LSTM de 3 camadas, 256-d | Fraco frente a ECAPA/ResNet (GE2E é de 2018) **[INFERÊNCIA]** | LSTM é suportado pelo tract; mel em Python (librosa) |
| **pyannote/embedding** | MIT, acesso condicionado (formulário) (https://huggingface.co/pyannote/embedding) | VoxCeleb | n/d | EER 2,8% Vox1-test (sem VAD/PLDA), muito pior que os acima | Ruim para ONNX/tract (SincNet + x-vector) **[INFERÊNCIA]** |
| **Treinado por nós em dados permissivos** | Nossa (MIT/Apache) | LibriSpeech (CC BY 4.0), MLS (CC BY 4.0), Common Voice (CC0) | Ajustável (ECAPA C=512 ~6M) | A medir; sem benchmark publicado de ECAPA só com esses dados que eu tenha encontrado | Controlamos o grafo (front-end in-graph, só operadores suportados) |

Licenças dos datasets permissivos: LibriSpeech CC BY 4.0 (https://openslr.org/12/), MLS CC BY 4.0 (https://openslr.org/94/), Common Voice CC0 (https://blog.mozilla.org/en/?p=62523). CN-Celeb é CC BY-SA 4.0 (https://openslr.org/82/); VoxBlink2 é CC BY-NC-SA 4.0 (**[FATO]** via arXiv 2407.11510, só anotações; vídeo por conta do usuário), então ambos ficam fora.

Falantes rotulados disponíveis (para dimensionar o treino):
- LibriSpeech: 960 h, 2.338 falantes **[FATO]** (via resultado de busca; ~1000 h no OpenSLR).
- MLS inglês: ~44,5 mil horas **[FATO]** (arXiv 2012.03411); número de falantes **[NÃO VERIFICADO]** (lembro de ~5,5 mil, não confirmei).
- Common Voice: `client_id` serve de proxy de falante, mas "multiple speakers can contribute under the same ID" **[FATO]** (https://discourse.mozilla.org/t/speaker-ids-for-speaker-identification-model/114358): rótulos ruidosos, exigem filtragem. **[INFERÊNCIA]** Em compensação é gravação de microfone de usuário, bem próxima do cenário "notebook".
- Não encontrei nenhum encoder pré-treinado publicado só nesses datasets (busca ampla). **[INFERÊNCIA]** Teremos de treinar.

## 4. Compatibilidade com tract

- **[FATO]** O crate `tract-onnx` registra módulos `array, cast, cumsum, fft, logic, math, ml, nn, quant, rec, ...`, mais `Einsum`, `Resize`, `GridSample` (https://raw.githubusercontent.com/sonos/tract/main/onnx/src/ops/mod.rs). `fft.rs` registra `DFT`, `STFT`, `MelWeightMatrix` e janelas Hann/Hamming/Blackman, com restrições: `STFT` exige `frame_step` constante e janela constante (ou frame length constante), entrada 3-D (https://raw.githubusercontent.com/sonos/tract/main/onnx/src/ops/fft.rs).
- **[INFERÊNCIA]** Isso torna viável embutir o front-end no ONNX (STFT + MelWeightMatrix, ou DFT como Conv1d com base de Fourier, que é o truque comum porque `torch.stft` exporta mal). Alternativa mais segura: calcular log-mel no Rust (já existe lado DSP no projeto) e fazer o ONNX receber features; mas isso muda o contrato `audio [1,N]` descrito em `enrollment.rs`. Decisão a tomar.
- **[INFERÊNCIA]** ECAPA-TDNN é o mais seguro para tract (só Conv1d, BatchNorm, ReduceMean/Sum, Softmax, Sigmoid, Concat, Slice). CAM++ traz o pooling de segmento (ponto de atenção). ResNet34 funciona, mas é o mais pesado em CPU. Validar com `tract` real (dummy export) antes de congelar a arquitetura: não testei, porque não instalei nada.

## 5. Arquitetura do enrollment: encoder congelado + projeção vs. treinar do zero

- **[INFERÊNCIA]** **Encoder pré-treinado congelado + projeção pequena treinada junto com o pDFNet3** é a opção correta. Motivos: (a) o encoder aprende discriminação de falante com rótulos de milhares de falantes; treinar do zero só com a perda de enhancement tende a decorar os falantes do conjunto do pDFNet3 e a generalizar mal para vozes novas; (b) o gradiente do pDFNet3 só precisa ajustar uma projeção de ~100k parâmetros; (c) o encoder pode ser validado isoladamente por EER.
- **[FATO]** O caminho já tem precedente publicado: pDFNet2 personaliza o DeepFilterNet2 com embeddings de **ECAPA-TDNN** (https://arxiv.org/abs/2404.08022; detalhes de dados e ponto de injeção não pude extrair do PDF).
- **[INFERÊNCIA]** Projeção sugerida: L2-norm do embedding (192/256-d), MLP de uma camada oculta, quatro cabeças lineares de 256. Inicialização das cabeças em zero para começar na identidade (gamma=1, beta=0), com gamma = `1 + tanh`/`exp` limitado e beta limitado de forma a ficar sempre dentro de [0.001,100] e [-50,50] (hoje valores fora disso são rejeitados). Treinar com enrollment e alvo de **trechos diferentes** do mesmo falante, ruído/reverb no enrollment e crop aleatório de 6-12 s, para o perfil resistir ao desencontro de condições. Opcional: descongelar o encoder com LR muito baixo numa última fase.
- **Sobre contaminação por VoxCeleb:** ver seção 2. Se o encoder congelado é treinado em VoxCeleb, o `enrollment.onnx` redistribuído carrega pesos cuja procedência de dados é cinzenta; se a projeção é treinada só com dados limpos, isso não limpa o encoder. Treinar do zero **o encoder** em dados permissivos evita o problema; treinar do zero **toda a cadeia só com a perda de enhancement** é o que eu desaconselho (acima).

## 6. Recomendação

**Principal: ECAPA-TDNN (C=512, ~6M params), treinado por nós com AAM-softmax em LibriSpeech + MLS (+ Common Voice filtrado) [todos CC BY/CC0], front-end log-mel dentro do ONNX ou no Rust, congelado, com projeção MLP para os 4 vetores FiLM treinada junto com o pDFNet3.**
- Por quê: único caminho que sustenta o enunciado "pesos MIT/Apache, dados CC BY/CC0" sem asterisco **[INFERÊNCIA]**; receita pública e Apache-2.0 (SpeechBrain/WeSpeaker têm recipes de ECAPA); operadores seguros no tract; mesma família já usada em personalização do DeepFilterNet2.
- Custo e risco: é trabalho de treino na Fase 5 (o repositório de treino existe); a qualidade (EER) é desconhecida e pode ficar abaixo dos 0,8% do VoxCeleb por menos falantes e por domínio de leitura (audiolivros). Mitigar com Common Voice (voz de microfone de usuário, domínio mais próximo) e medir EER em um conjunto de validação limpo (LibriSpeech dev/test). Para o nosso uso (condicionamento, não verificação), um EER na faixa de poucos por cento provavelmente basta, mas é hipótese a validar no resultado final de enhancement **[INFERÊNCIA]**.
- Obrigação: atribuição CC BY (LibriSpeech, MLS) no model card/NOTICE.

**Alternativa: SpeechBrain `spkrec-ecapa-voxceleb` (Apache-2.0, EER 0,80% Vox1-test) como encoder congelado, exportado para ONNX por nós, com a mesma projeção.**
- Por quê: qualidade imediata, zero treino de encoder, arquitetura idêntica à recomendada (trocar depois é só substituir o encoder e re-treinar a projeção).
- Custo: risco de procedência do VoxCeleb (seção 2) para uma distribuição que promete MIT/Apache. Se for usada, documentar abertamente a origem dos dados e a incerteza no model card, e não afirmar "treinado só em dados abertos".
- Uso recomendado mesmo se escolhermos o principal: rodar esse modelo **apenas em desenvolvimento** como baseline de EER e de qualidade de enhancement (comparação A/B), sem enviá-lo.
- Em segundo lugar nessa categoria: WeSpeaker CAM++ (0,654% EER, ONNX oficial, 7,18 M), menos seguro no tract e com a mesma questão de dados.

**Evitar:** TitaNet (dados LDC no treino: Fisher/Switchboard/SRE), pyannote/embedding (EER pior, gating, difícil no tract), Resemblyzer (qualidade inferior, mesma procedência Vox), 3D-Speaker (licença dos pesos e do dataset não confirmada; modelos principais em mandarim).

## 7. Pontos em aberto

- Confirmar a licença dos pesos do ModelScope (3D-Speaker) e do dataset 3D-Speaker (não carregou).
- Confirmar contagem de falantes do MLS inglês e parâmetros exatos do ECAPA C=512 do SpeechBrain (192-d).
- Prototipar o export ONNX do ECAPA (com front-end STFT in-graph) e abrir no tract para checar operadores e o shape dinâmico `[1,N]`.
- Decisão de contrato: front-end dentro do ONNX (mantém `audio [1,N]`) ou features calculadas em Rust.
- Se o jurídico do projeto exigir certeza sobre VoxCeleb, essa é uma questão para advogado, não para este relatório.
