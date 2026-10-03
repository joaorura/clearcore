# Fine-tune do DeepFilterNet3 com FiLM por locutor (pDFNet3): pipeline de treino, FiLM, literatura, custo e export

Data: 2026-10-03. Escopo: pesquisa (web + leitura local), nada foi instalado, baixado ou executado de treino.

Convenção: **[FATO]** tem fonte (link ou caminho local). **[INFERÊNCIA]** é raciocínio meu, não verificado por execução. Onde um resumo automático de página divergiu do texto real do PDF, vale o PDF (ver nota em 3.1).

Máquina: RTX PRO 1000 Blackwell 8151 MiB (`nvidia-smi`), 16 CPUs lógicas, 30 GB de RAM com apenas ~4 GB disponíveis no momento da pesquisa (`free -g`), 31 GB livres em `/home` (`df`), host com Python 3.14.7 e torch 2.13.0+cu130 [FATO, medido localmente].

---

## 0. Resumo executivo

1. O upstream **não** tem noção de locutor: o dataloader em Rust sorteia fala, ruído e RIR ao acaso e não devolve nem a chave do arquivo de fala. Para pDFNet3 é preciso (a) um dataloader próprio em Python ou (b) um fork do Rust (o Clearcore já vendoriza `dataloader.rs`/`dataset.rs`, mas **não** o crate `pyDF-data`). Recomendo (a) para o primeiro ciclo.
2. O treino roda em **Python 3.11** (únicas wheels de `deepfilterlib`/`deepfilterdataloader` 0.5.6 vão de cp38 a cp311; `numpy<2`). O Python 3.14 do host não serve para o treino. O **export ONNX** deve ser feito com torch pinado (o asset aprovado foi exportado com **PyTorch 1.13.1**), de preferência em ambiente separado (ver seção 5).
3. FiLM nos dois pontos que o Clearcore já usa: saída do `ReLU` de `emb_gru.linear_in` (encoder) e de `df_gru.linear_in` (df_dec), tensor `[B,T,256]`. Identidade exata por construção (`gamma = 1 + delta`, última camada do gerador com pesos e bias zero) e "dropout de embedding" por máscara que zera o delta.
4. Não achei na literatura um "pDFNet3 com FiLM". O mais próximo é o **pDeepFilterNet2** (ICASSP 2024), que concatena o embedding ECAPA-TDNN de 192 dimensões no encoder. Isso muda a dimensão da entrada da GRU e não preserva os pesos pré-treinados; nosso FiLM preserva.
5. Custo: modelo de ~2,1 M de parâmetros, treina em fp32 no upstream. Lote 16 a 32 de 3 s cabe nos 8 GB (inferência, medir). O gargalo prático é **disco** (HDF5 em PCM 48 kHz custa ~346 MB por hora de fala) e **RAM**, não GPU.

---

## 1. Pipeline de treino do DeepFilterNet (tag v0.5.6)

### 1.1 Estrutura e fluxo

- Pacote Python `DeepFilterNet/df/`: `train.py`, `model.py` (registro), `deepfilternet3.py`, `loss.py`, `config.py`, `checkpoint.py`, `enhance.py`, `io.py`, `modules.py`, `multiframe.py`, `lr.py`, etc.; `scripts/` com `prepare_data.py`, `export.py`, `sample_from_hdf5.py`, `split_hdf5.py`, `trim_silence_hdf5.py`, `fix_n_samples_hdf5.py`, `list_attrs_in_hdf5.py`, `model_summary.py` e outros [FATO: [df/](https://github.com/Rikorose/DeepFilterNet/tree/v0.5.6/DeepFilterNet/df), [df/scripts](https://api.github.com/repos/Rikorose/DeepFilterNet/contents/DeepFilterNet/df/scripts?ref=v0.5.6)].
- Uso: `python df/train.py <dataset.cfg.json> <data_dir> <base_dir>`; flags opcionais `--no-resume`, `--host-batchsize-config`, `--log-level`, `--debug`. O `base_dir` contém `config.ini` e `checkpoints/` [FATO: [train.py](https://raw.githubusercontent.com/Rikorose/DeepFilterNet/v0.5.6/DeepFilterNet/df/train.py), [README](https://github.com/Rikorose/DeepFilterNet/tree/v0.5.6/DeepFilterNet)].
- Registro de modelo: `ModelParams` lê `model` em `[train]` (padrão `deepfilternet3`) e importa `df.<nome>`; cada módulo expõe `ModelParams` e `init_model()`. O modelo é convertido para `channels_last` [FATO: [model.py](https://raw.githubusercontent.com/Rikorose/DeepFilterNet/v0.5.6/DeepFilterNet/df/model.py)]. Para um `pdeepfilternet3` basta um módulo novo `df/pdeepfilternet3.py` e `model = pdeepfilternet3` no `config.ini`.
- Passo de treino (`run_epoch`): lê `feat_erb`, `feat_spec`, `noisy`, `speech` (alvo limpo) e `snr` do lote; chama `model.forward(spec=as_real(noisy), feat_erb=..., feat_spec=...)`; `losses.forward(clean, noisy, enh, m, lsnr, snrs=snrs)`; `backward`, `clip_grad_norm_(1.0)`, `opt.step()`. O LR é atualizado **por iteração** (warmup + cosseno), com batch scheduling [FATO: train.py].
- **Sem AMP/autocast e sem `torch.compile`** no `train.py` [FATO: train.py, resumo do código]. Treino em fp32. [INFERÊNCIA] bf16 autocast é possível mas é patch próprio e arriscado para as perdas com STFT; não é necessário para este tamanho de modelo.

### 1.2 Como se faz fine-tune

- `load_model()` carrega o checkpoint com `load_state_dict(..., strict=False)` e só emite warning para chaves ausentes (`Missing key`). Nome: `{name}_{epoch}.ckpt` mais sufixo `.best`; `epoch="best"` escolhe o `.best` de maior época [FATO: [checkpoint.py](https://raw.githubusercontent.com/Rikorose/DeepFilterNet/v0.5.6/DeepFilterNet/df/checkpoint.py)]. Logo, um modelo com módulos novos (gerador FiLM) carrega os pesos do DFNet3 e deixa os novos com a inicialização aleatória. Isso é o que viabiliza o fine-tune.
- Os pesos PyTorch do DFNet3 estão no repo: `models/DeepFilterNet3.zip` (7 986 207 bytes), junto de `DeepFilterNet3_onnx.tar.gz` (7 983 136 bytes; é o asset aprovado do Clearcore, SHA em `models/stateful/README.md`) [FATO: [models/](https://api.github.com/repos/Rikorose/DeepFilterNet/contents/models?ref=v0.5.6)]. [INFERÊNCIA] o zip tem `config.ini` + `checkpoints/model_120.ckpt.best`, o formato que `train.py` espera em `base_dir`; **confirmar abrindo o zip** (não baixei).
- Modos de congelamento existentes: `mask_only` e `df_only` em `[train]` restringem os parâmetros treinados [FATO: train.py]. Para congelar a base e treinar só o FiLM é patch próprio no ponto onde se monta o otimizador.
- Hiperparâmetros do DFNet3 upstream (de `models/stateful/config.ini`, cópia byte a byte do asset) [FATO, leitura local]: `max_epochs=120`, `batch_size_scheduling=0/16,2/24,5/32,10/64,20/128,40/9999`, `max_sample_len_s=3.0`, `lr=1e-3`, `lr_min=1e-6`, `warmup_epochs=3`, AdamW, `weight_decay_end=0.01`, `early_stopping_patience=25`, `p_reverb=0.1`, `p_interfer_sp=0.0`, `dataloader_snrs=-100,-5,0,5,10,20,40`, `[multiresspecloss] factor=500 factor_complex=500 fft_sizes=256,512,1024,2048 gamma=0.3`, `[localsnrloss] factor=1e-3`, `[spectralloss] factor_*=0`, `[maskloss] factor=0`, `[dfalphaloss] factor=0`, `df_order=5`, `nb_df=96`, `nb_erb=32`, `fft_size=960`, `hop_size=480`, `sr=48000`, `emb_hidden_dim=256`, `df_hidden_dim=256`, `conv_ch=64`.
- Perda (`df/loss.py`): soma ponderada de `MaskLoss`, `SpectralLoss`, `MultiResSpecLoss`, `SdrLoss`, `LocalSnrLoss`, `ASRLoss`, `DfAlphaLoss`; assinatura `forward(clean, noisy, enhanced, mask, lsnr, snrs, max_freq=None)`; `LocalSnrLoss = mse(lsnr_pred, lsnr_alvo) * factor` [FATO: [loss.py](https://raw.githubusercontent.com/Rikorose/DeepFilterNet/v0.5.6/DeepFilterNet/df/loss.py)]. No DFNet3 as ativas são a multi-resolução (módulo e complexa) e a de SNR local. No paper do DFNet2, `λ_spec=1e3`, `λ_MR=5e2` [FATO: [arXiv 2205.05474](https://arxiv.org/pdf/2205.05474)].

### 1.3 Formato de dados

- `prepare_data.py <type> <audio_files.txt> <out.hdf5>` com `type` = `speech`, `noise` ou `noisy` (a docstring do upstream diz isso; o dataset usa grupos `speech`, `noise`, `rir`). Flags: `--num_workers 4 --max_freq -1 --sr 48000 --dtype int16 --codec pcm --mono --compression`. Codecs: `pcm` (padrão), `flac`, `vorbis` (vorbis força float32). Reamostra com Kaiser best. Chaves = caminho com `/` trocado por `_` [FATO: [prepare_data.py](https://raw.githubusercontent.com/Rikorose/DeepFilterNet/v0.5.6/DeepFilterNet/df/scripts/prepare_data.py)].
- Atributos por dataset HDF5: `sr`, `dtype`, `codec`, `max_freq`, `n_samples` [FATO: [dataset.rs](https://raw.githubusercontent.com/Rikorose/DeepFilterNet/v0.5.6/libDF/src/dataset.rs)]. O `max_freq` serve para fala de banda limitada (ex.: fonte de 16 kHz) não ser cobrada acima do corte na perda.
- Config de dataset (JSON): `{"train": [[arq.hdf5, fator_amostragem], ...], "valid": [...], "test": [...]}`; cada entrada aceita fator (padrão 1.0), sr e max_freq de fallback, cache de chaves [FATO: dataset.rs]. Para RIR, o `prepare_data` usa o mesmo mecanismo (grupo `rir`) [FATO: dataset.rs].
- Construção da amostra (`Dataset::get_sample`, Rust): 1 fala-alvo (`sp_keys[idx]`) + 2 a 5 ruídos concatenados, RIR aplicada à fala e ao ruído, SNR sorteado de `snrs`, ganho de `gains`; o **alvo limpo é a fala sem RIR**; a mistura usa a fala distorcida (RIR, clipping, air absorption, bandwidth) [FATO: dataset.rs]. Confirmei no código vendorizado (`vendor/crates/deep_filter/src/dataset.rs:1210-1350`).

### 1.4 Dataloader em Rust: o que ele faz e o que NÃO faz

- `pyDF-data` (crate `DeepFilterDataLoader`, lib `libdfdata`) expõe `PyDataLoader(ds_dir, config_path, sr, batch_size, fft_size, ..., p_interfer_sp, snrs, gains, ...)` e `get_batch()` devolve 10 elementos: `speech`, `noisy` (STFT complexa), `feat_erb`, `feat_spec`, `lengths`, `max_freq`, `snr`, `gain`, `timings`, `ids` [FATO: [pyDF-data/src/lib.rs](https://raw.githubusercontent.com/Rikorose/DeepFilterNet/v0.5.6/pyDF-data/src/lib.rs)]. `ids` é o índice da amostra na época, não chave de locutor nem de arquivo [FATO: dataloader.rs, `ids.push(s.idx)`].
- **Interferente (`p_interfer_sp`)**: com essa probabilidade entram 1 a 2 falas sorteadas **de qualquer locutor, inclusive o próprio alvo**, mixadas na parte distorcida com SNR **fixo em {30, 20, 15} dB** em relação à fala-alvo (`let snr_interfer = [30., 20., 15.]`) [FATO: `vendor/crates/deep_filter/src/dataset.rs:1312-1345`]. É interferência fraca (SIR 15 a 30 dB); a literatura de PSE treina com SIR de -5 a 25 dB (seção 3). E o upstream padrão tem `p_interfer_sp=0`.
- Observação sobre o código [INFERÊNCIA, a confirmar lendo/rodando]: dentro do laço do interferente, `self.reverb.transform_single(&mut speech, rir)` aplica a RIR em `speech` (a fala-alvo), não em `sample` (o interferente). Se for isso mesmo, o alvo limpo é alterado quando `p_interfer_sp>0`. Verificar antes de usar esse caminho.
- Conclusão [INFERÊNCIA]: pDFNet3 precisa de controle sobre quem é o alvo, quem é o interferente, a SIR, e de um clipe de enrollment do mesmo locutor. Isso não existe na interface do `libdfdata`.

### 1.5 Dependências e versões

- `DeepFilterNet/pyproject.toml`: `python >=3.8,<4.0`, `deepfilterlib = 0.5.6`, `numpy >=1.22,<2.0`; extra `train` = `deepfilterdataloader 0.5.6` + `icecream`; torch/torchaudio instalados à parte (tarefas do poetry apontam torch 1.13.1) [FATO: [pyproject.toml](https://raw.githubusercontent.com/Rikorose/DeepFilterNet/v0.5.6/DeepFilterNet/pyproject.toml)]. O README acrescenta `libhdf5-dev`, `h5py`, `librosa`, `soundfile`, Rust/maturin/poetry [FATO: [README](https://github.com/Rikorose/DeepFilterNet/tree/v0.5.6/DeepFilterNet)].
- Wheels no PyPI: `DeepFilterLib 0.5.6` = cp38, cp39, cp310, cp311 (manylinux_2_28 x86_64/aarch64, macOS, Windows). `DeepFilterDataLoader 0.5.6` = cp38 a cp311, **só manylinux_2_28 x86_64**. Nada para cp312+ [FATO: [deepfilterlib](https://pypi.org/pypi/deepfilterlib/0.5.6/json), [deepfilterdataloader](https://pypi.org/pypi/deepfilterdataloader/0.5.6/json)].
- Código-fonte Rust: `pyo3 0.19` + `numpy 0.19` (crates), `rust-version 1.60`, `hdf5` crate via git rev `26046fb` (ou feature `hdf5-static`), `maturin>=0.11,<0.12` no `pyDF-data/pyproject.toml` [FATO: [pyDF-data/Cargo.toml](https://raw.githubusercontent.com/Rikorose/DeepFilterNet/v0.5.6/pyDF-data/Cargo.toml), [libDF/Cargo.toml](https://raw.githubusercontent.com/Rikorose/DeepFilterNet/v0.5.6/libDF/Cargo.toml), [pyDF-data/pyproject.toml](https://raw.githubusercontent.com/Rikorose/DeepFilterNet/v0.5.6/pyDF-data/pyproject.toml)].
- Features de áudio para Python: `df_features()` usa `libdf.DF`, `erb`, `erb_norm`, `unit_norm` (do `deepfilterlib`) [FATO: [enhance.py](https://raw.githubusercontent.com/Rikorose/DeepFilterNet/v0.5.6/DeepFilterNet/df/enhance.py)]. Isso permite um dataloader Python que só depende de `deepfilterlib` (wheel pronta), sem compilar nada.

### 1.6 Compatibilidade com Python 3.14 / torch 2.13

- **Python 3.14: não.** Não há wheel de `deepfilterlib`/`deepfilterdataloader` acima de cp311 (fato acima), e `pyo3 0.19` oficialmente não chega ao 3.13/3.14 [INFERÊNCIA sobre a faixa do pyo3 0.19, conhecimento geral, não verifiquei]. `numpy<2` também restringe (rust-numpy 0.19 [INFERÊNCIA: não suporta numpy 2]).
- **Recomendado: Python 3.11** (via `uv python install 3.11`, venv isolada; o `uv` está em `~/.local/bin/uv`) com `numpy<2`, `deepfilterlib==0.5.6`, `deepfilterdataloader==0.5.6` (se usar o loader Rust), torch com CUDA para Blackwell. [INFERÊNCIA] torch 2.13+cu130 deve ter wheel cp311; confirmar no índice cu130 antes (não consultei). Sm_120 exige torch com CUDA >= 12.8.
- **torchaudio novo quebra o upstream.** `df/io.py` faz `from torchaudio.backend.common import AudioMetaData`; esse caminho sumiu a partir do torchaudio 2.1 e `torchaudio.AudioMetaData` também foi removido nas versões mais novas [FATO: [df/io.py](https://raw.githubusercontent.com/Rikorose/DeepFilterNet/v0.5.6/DeepFilterNet/df/io.py); discussões: [PyTorch forum](https://discuss.pytorch.org/t/no-module-named-torchaudio-backend-common/186937), [HF forum](https://discuss.huggingface.co/t/module-torchaudio-has-no-attribute-audiometadata/175647)]. [INFERÊNCIA] com torchaudio >= 2.9 é preciso patchar `df/io.py` (trocar por `soundfile` + dataclass própria). Também há relato de `torch._six` ausente ([resultado de busca HF](https://huggingface.co/spaces/hshr/DeepFilterNet)); não verifiquei se afeta a v0.5.6.
- Plano de ambientes [INFERÊNCIA]: (A) treino em Python 3.11 + torch recente (GPU); (B) export em Python 3.10 + torch 1.13.1 CPU, só para reproduzir nomes de nós (seção 5).

---

## 2. Onde inserir o FiLM no DFNet3

### 2.1 O que o Clearcore já faz (leitura local)

- `tools/accelerators/gen_film_onnx.py`: insere `x' = x * gamma + beta` na saída do **Relu que alimenta Transpose->GRU** em dois grafos. Sítios (nomes de nó do export com torch 1.13.1): `enc`: `/emb_gru/linear_in/1/Relu_output_0`; `df_dec`: `/df_gru/linear_in/linear_in.1/Relu_output_0`. `gamma` e `beta` entram como **entradas extras** `float32 [1, "S", 256]` (HIDDEN=256), na ordem `gamma, beta` depois das entradas originais. O script prova identidade bit-exata no ORT (gamma=1, beta=0) e que valores não triviais mudam a saída. Há também a variante "baked" (initializers).
- `vendor/crates/deep_filter/src/tract.rs`: `FilmVectors {gamma_enc, beta_enc, gamma_df, beta_df}`, cada um com `ch * film_hidden` elementos (`film_hidden = emb_hidden_dim = 256`); `FilmInputs` guarda tensores `[ch, 1, hidden]`; o FiLM é considerado ativo quando o grafo tem **4 entradas** (`input_outlets()?.len() == 4`); no `process_raw` o encoder recebe `[feat_erb, feat_spec, gamma, beta]` e o df_dec `[emb, c0, gamma, beta]`. O erb_dec **não** recebe FiLM; o `emb` que ele consome já vem modulado porque sai da GRU do encoder.
- `tools/accelerators/gen_enrollment_test_onnx.py` (contrato do enrollment): entrada `audio float32 [1, N]` a 16 kHz mono; saídas `gamma_enc, beta_enc, gamma_df, beta_df`, `float32 [256]` cada. **O FiLM é invariante no tempo**: um vetor por locutor, calculado uma vez no enrollment. O pDFNet3 treinado deve respeitar isso: gerador = função só do embedding do locutor.

### 2.2 Onde no código PyTorch do DFNet3

Fato (código [deepfilternet3.py](https://raw.githubusercontent.com/Rikorose/DeepFilterNet/v0.5.6/DeepFilterNet/df/deepfilternet3.py) e [modules.py](https://raw.githubusercontent.com/Rikorose/DeepFilterNet/v0.5.6/DeepFilterNet/df/modules.py)):
- `Encoder.emb_gru` e `DfDecoder.df_gru` são `SqueezedGRU_S`, cuja `forward` é `x = self.linear_in(input); x, h = self.gru(x, h); x = self.linear_out(x)`; `linear_in = Sequential(GroupedLinearEinsum(in, hidden, groups), ReLU)`. É exatamente o Relu dos sítios acima. Dimensões: entrada 512 (`conv_ch*nb_erb/4`), hidden 256.
- `DfNet.forward(spec, feat_erb, feat_spec)` chama `enc`, `erb_dec(emb,e3,e2,e1,e0)` e `df_dec(emb, c0)`.

Patch proposto [INFERÊNCIA, é desenho meu]:

```python
# modules.py: SqueezedGRU_S.forward ganha um argumento opcional
def forward(self, input, h=None, film=None):
    x = self.linear_in(input)              # [B,T,256], ReLU já aplicado
    if film is not None:
        g, b = film                        # [B,1,256] ou [B,T,256]
        x = x * g + b
    x, h = self.gru(x, h)
    ...
# Encoder.forward(feat_erb, feat_spec, film=None) -> self.emb_gru(emb, film=film)
# DfDecoder.forward(emb, c0, film=None)           -> self.df_gru(emb, film=film)
# DfNet.forward(spec, feat_erb, feat_spec, spk_emb=None, spk_mask=None)
```

Gerador FiLM (novo módulo, treinado; entrada = embedding congelado do locutor, ex. 192-d):
```python
class FilmGen(nn.Module):
    def __init__(self, d_emb=192, hidden=256, d_mid=256):
        self.net = nn.Sequential(nn.Linear(d_emb, d_mid), nn.ReLU())   # sem LayerNorm que dependa do vetor zero
        self.out = nn.Linear(d_mid, 4 * hidden)
        nn.init.zeros_(self.out.weight); nn.init.zeros_(self.out.bias)  # identidade exata
    def forward(self, e, mask):                 # mask: [B,1] com 1=personalizado, 0=sem locutor
        d = self.out(self.net(e)) * mask        # delta = 0 quando sem locutor
        dg_enc, db_enc, dg_df, db_df = d.chunk(4, dim=-1)
        return (1 + dg_enc, db_enc), (1 + dg_df, db_df)   # cada um [B,256]; unsqueeze(1) na aplicação
```

### 2.3 Identidade e "embedding dropout"

- **Identidade (fato do desenho, verificável)**: com `out.weight = out.bias = 0`, `gamma=1` e `beta=0` para qualquer embedding; `x*1+0` é bit-exato em fp32 (o mesmo argumento que o `gen_film_onnx.py` verifica no ORT). Teste de aceitação a fazer: carregar `DeepFilterNet3` (`strict=False`), rodar um lote com e sem o ramo FiLM (`film=None` contra `film=identidade`) e exigir `max_abs_diff == 0`. O Clearcore já exige o mesmo no Rust (`parity_film.rs`, `FiLMVectors::identity()`, `max_abs_diff = 0.0`, plano da Fase 4).
- Observação [INFERÊNCIA]: com a última camada zerada, o gradiente para as camadas anteriores do gerador é zero no passo 0 (só `out` aprende primeiro). É benigno; alternativa: std pequeno (1e-3) e relaxar a exigência de bit-exato para o início, mas isso perde a garantia. Prefiro zeros.
- **Embedding dropout / modo sem locutor**: com probabilidade `p` (sugestão 0,2 a 0,35) por amostra, `mask=0`: o delta é zerado, resultando em `gamma=1`, `beta=0` exatamente, o mesmo que o Clearcore alimenta quando não há perfil (`FiLMVectors::identity()`, Fase 4 Step 3.3). Nessas amostras, o **alvo muda**: sem locutor o modelo deve se comportar como DFNet3 (denoise sem separar locutores). Isso copia a ideia do "Unified Personalized/Non-personalized PercepNet": flag de personalização, embedding **zerado** quando a flag é 0, e alvo = fala do locutor-alvo (flag 1) ou fala de **todos** os locutores (flag 0) [FATO: [arXiv 2302.11768](https://arxiv.org/pdf/2302.11768)]. Sugestão extra [INFERÊNCIA]: nas amostras com `mask=0`, regularizar também com a saída de um **professor** (DFNet3 original congelado) para não regredir o modo sem locutor.
- Aumento de enrollment: embeddings de enrollment com ruído/reverb (10 variantes por clipe) reduziram o sobreajuste ao espaço de embeddings no UPN [FATO: 2302.11768, seção 3.2.3 e 4.2].

### 2.4 Consequências para o export

- No export, `gamma/beta` viram **entradas** de `enc` e `df_dec`, nessa ordem, `[1,"S",256]` (alinhado ao `gen_film_onnx.py` e ao `tract.rs`). O gerador FiLM **não** entra nos 3 grafos: ele vira parte do `enrollment.onnx` (encoder de locutor 16 kHz + gerador, saindo já `gamma = 1+delta`), contrato de `gen_enrollment_test_onnx.py`.
- Para o encoder de locutor, o contrato do Clearcore é áudio 16 kHz mono. Opções comuns: ECAPA-TDNN do SpeechBrain, 192-d, **Apache-2.0** na model card, 0,80% EER na VoxCeleb1-test (resultado de busca, [localai.io](https://localai.io/features/voice-recognition/index.html) e outras páginas; **não abri a model card oficial**); WeSpeaker (ResNet34/CAM++), pesos "seguem a licença do dataset" (VoxCeleb = CC BY 4.0) [FATO: [WeSpeaker pretrained.md](https://github.com/wenet-e2e/wespeaker/blob/master/docs/pretrained.md), via resultado de busca]. O processo de governança do Clearcore (`governance/model-assets/`) exige registro de licença por asset; tratar a licença de pesos treinados em VoxCeleb como ponto de revisão [INFERÊNCIA, não é parecer jurídico].

---

## 3. Literatura de personalized speech enhancement

### 3.1 Trabalhos relevantes e leves

| Trabalho | Condicionamento | Dados e perdas relevantes | Fonte |
|---|---|---|---|
| **pDeepFilterNet2** (Serre et al., ICASSP 2024) | Embedding ECAPA-TDNN de 192-d (congelado, pré-treinado na VoxCeleb2), **concatenado** onde os dois ramos do encoder se juntam ("unified encoder"); variante "dual encoder" (+0,40 G MACs). Melhor: unified, **sem custo extra** (2,31 M parâmetros, 0,33 G MACs). Concatenar só no ramo DF rende quase o mesmo que nos dois; só no ERB rende menos | Alvo + ruído: DNS5 personalizado (3230 locutores, 750 h; após limpeza 2448 locutores no treino); interferente: Mozilla Common Voice (261 h, 7069 vozes). 950 h geradas: **20% alvo+ruído, 30% alvo+interferente, 50% os três**; SNR em [-5,35] dB, SIR em [-5,25] dB. Perda: `λ_spec=1e3`, `λ_MR=5e2`, `λ_OS=5e2` (over-suppression), janelas {5,10,20,40} ms, batch scheduling 8->128, early stopping paciência 15. PESQ 2,10 -> 2,36 no teste sintético | [arXiv 2404.08022](https://arxiv.org/pdf/2404.08022), li o texto do PDF |
| **Personalized PercepNet** (Giri et al., Interspeech 2021, AWS) | Embedding do locutor (rede embedder própria, perda GE2E, entrada de 68 features, último frame da GRU normalizado) **anexado a cada frame antes das GRUs** | Mistura alvo + interferente + ruído, SNR -5 a 35 dB, SIR -5 a 10 dB; perdas do PercepNet; 4500 locutores | [arXiv 2106.04129](https://arxiv.org/pdf/2106.04129), li o texto do PDF |
| **pDCCRN / pDCATTUNET** (Eskimez et al., ICASSP 2022, Microsoft) | d-vector (Res2Net) **concatenado na entrada da LSTM complexa** do DCCRN (aumenta só essa entrada) | 2000 h treino (544 h de fala DNS/LibriVox); **60% alvo+interferente+ruído, 40% alvo+ruído** (a parcela sem interferente mantém o desempenho do SE comum); alvo a 0 a 1,3 m, interferente a mais de 2 m; enrollment de ~60 s ou 30 amostras VCTK. Perdas: PLCPA (power-law compressed phase-aware, p=0,3) e **assimétrica** `L_OS` para sub-supressão do locutor-alvo; multi-task com ASR; métrica TSOS (target speaker over-suppression) | [arXiv 2110.09625](https://arxiv.org/pdf/2110.09625), li o texto do PDF |
| **Unified Personalized/Non-personalized PercepNet** (Wang et al., ICASSP 2023) | Embedding ECAPA-TDNN 192-d congelado + **flag de personalização por frame**; embedding zerado quando flag=0 | Alvo da flag=1: fala só do alvo; flag=0: fala de todos. Perda ponderada por VAD (`mu`, 0,9 mitiga over-suppression); aumento de enrollment (ruído+reverb); SIR de -2 a 10 dB; interferentes só com similaridade < 0,5 ao alvo; flag sorteada 1/3 personalizado, 1/3 não, 1/3 alternando; mistura 50% sobreposta, 25% alternada, 25% um locutor; 8 V100, lote 256 | [arXiv 2302.11768](https://arxiv.org/pdf/2302.11768), li o texto do PDF |
| **TEA-PSE 3.0** (Tencent, ICASSP 2023 DNS, 1º lugar nas duas trilhas) | Estrutura LGR (local-global representation) para extrair informação do locutor; perda multi-STFT; retreino com freeze | Modelo grande: **22,24 M parâmetros, 19,66 G MACs** (segundo o paper pDeepFilterNet2). Não serve de baseline de custo | [arXiv 2303.07704](https://arxiv.org/abs/2303.07704), abstract; números de custo via 2404.08022 |
| NPU-Elevoc (ICASSP 2023 DNS, empate no 1º lugar na trilha 1) | Base TEA-PSE 2.0, estratégia de fusão do embedding, treino adversarial, perda multi-escala | | [arXiv 2303.06811](https://arxiv.org/pdf/2303.06811), apenas abstract |
| **DNS Challenge ICASSP 2022/2023** (trilha personalizada) | Baseline com embedding RawNet2 (VoxCeleb2, banda larga) | Fala de treino por locutor com **2,5 min de enrollment**; 750 h, 3230 locutores (DNS5 PDNS); métrica: DNSMOS P.835 + WAcc | [arXiv 2202.13288](https://arxiv.org/pdf/2202.13288) (edição 2022); o dataset de 2023 é descrito em 2404.08022/2302.11768 |
| **FiLM** (Perez et al., AAAI 2018) | `x' = gamma(z) * x + beta(z)` por feature, gamma/beta funções do condicionamento | Origem geral do mecanismo; não é paper de áudio | [arXiv 1709.07871](https://arxiv.org/pdf/1709.07871) (só abstract via busca; **não li a seção de inicialização**) |

Notas:
- **"pDeepFilterNet3"/FiLM em DFNet**: não encontrei publicação. O único DeepFilterNet personalizado publicado é o pDeepFilterNet2, que usa **concatenação** [FATO]. Logo, o pDFNet3 com FiLM do Clearcore é desenho próprio sem referência direta; a evidência de que condicionamento multiplicativo funciona vem de SpeakerBeam (camada de adaptação multiplicativa, citada em [2110.09625]) e do uso geral de FiLM [INFERÊNCIA].
- Resultado do pDeepFilterNet2 aplicado ao nosso caso [INFERÊNCIA]: o ramo DF é o que mais se beneficia do embedding. Isso justifica ter FiLM no `df_dec` além do encoder, como o Clearcore já tem.
- Aviso de qualidade das minhas fontes: um resumo automático de página trouxe números errados para o paper pDCCRN (disse "500 h" e "SIR -5 a 15 dB"); descartei e usei o texto real do PDF (2000 h; 60%/40%; PLCPA). Só uso fatos dos PDFs lidos.

### 3.2 Perdas e dados a adotar (derivado da literatura) [INFERÊNCIA]

- Base: a perda do DFNet3 (multi-resolução módulo+complexa `500/500`, `gamma=0.3`, lsnr `1e-3`).
- Supressão de interferente: o alvo é só a fala do locutor-alvo; o interferente entra **só na mistura** (mesmo mecanismo do loader: alvo limpo exclui o interferente). Acrescentar penalização assimétrica de over-suppression do alvo (`L_OS` de pDCCRN/pDFNet2). O `config.ini` do upstream já tem `factor_under` e `f_under`; a semântica exata está em `loss.py` e deve ser lida antes de reaproveitar.
- Sem locutor (`mask=0`): alvo = soma da fala de todos os locutores da mistura (UPN) e/ou saída do DFNet3 professor.
- Mistura sugerida: 30% alvo+ruído, 30% alvo+interferente, 30% alvo+interferente+ruído (SIR U[-5, 15] dB, SNR do DFNet3), 10% interferente sem alvo (alvo ausente, saída quase silêncio; só aplicar se o produto quiser esse comportamento, pDCCRN faz). Interferente com similaridade de locutor < 0,5 ao alvo (UPN).
- Enrollment: clipe de **outra** gravação do mesmo locutor, 3 a 30 s sorteados, com ruído/reverb aleatórios (aumento de enrollment).

---

## 4. Custo: GPU, lote, dataset, disco

### 4.1 Fatos de partida

- DFNet2 (referência próxima do DFNet3): 2,31 M parâmetros, 0,33 G MACs por segundo de áudio [FATO: tabela 1 de [2404.08022](https://arxiv.org/pdf/2404.08022)]; os três ONNX do DFNet3 somam ~8,6 MB em fp32 (`models/stateful/README.md`: 1 954 042 + 3 292 397 + 3 340 803 bytes), ou seja ~2,1 M de parâmetros [INFERÊNCIA a partir do tamanho].
- Upstream treinou DFNet2 em DNS4 inglês: mais de 500 h de fala full-band, ~150 h de ruído, 150 RIR reais e 60 000 simuladas; 100 épocas com AdamW; split 70/15/15 [FATO: [2205.05474](https://arxiv.org/pdf/2205.05474)]. O DFNet3 foi treinado no DNS4 multilíngue completo, com PTDB e VCTK superamostrados 10x [FATO: [2305.08227](https://arxiv.org/pdf/2305.08227)].
- Config de produção do DFNet3: 120 épocas, lote até 128 (batch scheduling), `max_sample_len_s=3` [FATO: `models/stateful/config.ini`].

### 4.2 Estimativas [INFERÊNCIA, NÃO medidas; medir 200 passos antes de comprometer]

- **FLOPs**: 100 h de fala = 360 000 s x ~0,35 G MACs x 3 (fwd+bwd) x 2 = ~7,6e14 FLOP por época, ~4 a 5 min de GPU puro a 3 TFLOPS efetivos. O custo real fica por conta da GRU sequencial (300 passos de 10 ms em 3 s), das STFT das perdas (até FFT 2048 a 48 kHz) e do loader. Ordem de grandeza: **10 a 25 min por época de 100 h**.
- **Fine-tune típico**: Estágio A (base congelada, só gerador FiLM): 2 a 4 épocas. Estágio B (tudo descongelado, LR 1e-4 a 2e-4 com cosseno, em vez do 1e-3 do treino do zero): 15 a 30 épocas, early stopping. Total por rodada: **~4 a 12 h de GPU**; contando 2 a 3 ablações (FiLM só enc / enc+df, p de dropout): **~15 a 35 h**.
- **Lote em 8 GB**: sem AMP, ~60 a 120 MB por amostra de 3 s com autograd e perdas [INFERÊNCIA]; lote 32 deve caber (2 a 4 GB), 64 é limítrofe; usar **16 a 32** com LR proporcional e checar `torch.cuda.max_memory_allocated()`. O batch scheduling do upstream (até 128) não cabe.
- **Dataset mínimo útil** [INFERÊNCIA]: 50 a 100 h de fala limpa, mas o que importa para o gerador FiLM é **diversidade de locutores** (centenas a milhares). Referências: pDFNet2 usou 2448 locutores; UPN, mais de 7000; PPN, 4500. Abaixo de ~500 locutores o risco é decorar o espaço de embeddings (daí o aumento de enrollment). Com fonte de 16 kHz, a banda alta não é aprendida; para uso a 48 kHz incluir fala full-band (DNS5 PDNS é 48 kHz).
- **Disco do HDF5**: PCM int16 a 48 kHz = 96 000 B/s = **345,6 MB/h**. 50 h = 17 GB; 100 h = 35 GB (**não cabe nos 31 GB livres**); 200 h = 69 GB. FLAC (`--codec flac`, suportado pelo loader) tipicamente 50 a 65% do PCM [INFERÊNCIA]: 100 h = 17 a 22 GB. Vorbis ~43 MB/h a 96 kbps (lossy, artefatos no alvo limpo; aceitável para ruído). Para fonte nativa a 16 kHz, guardar a 16 kHz (115 MB/h) e deixar o loader reamostrar (atributo `sr` do HDF5) com `max_freq=8000`. Acrescentar ruído (20 a 40 h em FLAC ≈ 4 a 8 GB) e RIRs (pequeno). **Orçamento recomendado: ~20 GB de dados + ~6 GB de ambiente Python/torch CUDA (inferência) = no limite**; liberar espaço ou usar disco externo antes de gerar HDF5.
- **RAM**: o host está com ~4 GB disponíveis. O loader Rust bufferiza `NUM_PREFETCH_BATCHES` lotes (config: `num_prefetch_batches=8`, `num_workers=16`); um lote de 32 amostras de 3 s com `speech`, `noisy`, features é da ordem de 100 MB [INFERÊNCIA]. Reduzir `num_workers` (4 a 8) e `num_prefetch_batches` (2 a 4) e fechar outros processos.

---

## 5. Export ONNX (enc / erb_dec / df_dec e versão stateful)

### 5.1 Script upstream

- `DeepFilterNet/df/scripts/export.py` [FATO: [export.py](https://raw.githubusercontent.com/Rikorose/DeepFilterNet/v0.5.6/DeepFilterNet/df/scripts/export.py)]: `python df/scripts/export.py <export_dir> [--model-base-dir DIR] [--no-check] [--simplify] [--opset 12]`. Carrega com `init_df(model_base_dir, ..., epoch=...)`; exporta três arquivos com `torch.onnx.export(..., keep_initializers_as_inputs=False, dynamic_axes={...: "S"})`:
  - `enc.onnx`: entradas `feat_erb`, `feat_spec`; saídas `e0,e1,e2,e3,emb,c0,lsnr`.
  - `erb_dec.onnx`: entradas `emb,e3,e2,e1,e0`; saída `m`.
  - `df_dec.onnx`: entradas `emb,c0`; saída `coefs`.
  - Valida contra o ORT com `rtol=1e-6, atol=1e-5`; copia `config.ini`; empacota `{model}_onnx.tar.gz` (membros em `tmp/export/`, igual ao asset do Clearcore).
- O asset aprovado foi exportado com **PyTorch 1.13.1** [FATO: string `pytorch 1.13.1` no cabeçalho dos `.onnx` em `models/stateful/`], de acordo com a tarefa poetry de torch 1.13.1 no `pyproject.toml`.

### 5.2 FiLM no export

- Estender o `export.py` (cópia própria) para que `Encoder` e `DfDecoder` recebam `gamma, beta` como entradas extras na ordem do contrato e `dynamic_axes` com `S` no eixo 1: exemplo de entrada `gamma = ones(1, S, 256)`, `beta = zeros(1, S, 256)`; nomes de entrada exatos `gamma`, `beta` (o `gen_film_onnx.py` e o `tract.rs` assumem 4 entradas). A validação ORT deve comparar contra o PyTorch com FiLM identidade **e** com valores não triviais.
- Alternativa de menor risco [INFERÊNCIA]: treinar em PyTorch, exportar o modelo **sem** FiLM no ambiente fiel (torch 1.13.1) e aplicar o FiLM com `gen_film_onnx.py`, já que o treino com FiLM no PyTorch usa exatamente `x*gamma+beta` no mesmo ponto. Isso reaproveita os sítios `/emb_gru/linear_in/1/Relu_output_0` e `/df_gru/linear_in/linear_in.1/Relu_output_0` e a prova bit-exata. A condição é que a **topologia** do modelo fine-tunado seja a original (só os pesos mudam), o que é o caso.
- Risco do exporter [INFERÊNCIA]: com torch >= 2.9 o padrão passou a ser o exporter via dynamo, que gera nomes de nó diferentes; o `make_stateful_onnx.py` casa nós **por nome** (`/erb_conv0/0/Pad`, `/df_convp/df_convp.0/Pad`, `/emb_gru/Slice`, `/emb_gru/Slice_1`, `/df_gru/gru/Slice`, `/df_gru/gru/Slice_1`, `/emb_gru/GRU_output_1`, `/emb_gru/GRU_1_output_1`, `/df_gru/gru/GRU_output_1`, `/df_gru/gru/GRU_1_output_1`). Exportar com o mesmo torch do asset (1.13.1, Python 3.10, CPU) evita reescrever o script.

### 5.3 Versão stateful (Clearcore)

`tools/accelerators/make_stateful_onnx.py` (leitura local) faz cirurgia de grafo nos 3 ONNX, nesta ordem:
1. `enc`: expõe `h_in [1,1,256]`/`h_out` da GRU (`input[5]`, `output[1]`); anéis de atraso de 2 quadros (`feat_erb_buf [1,1,2,32]`, `feat_spec_buf [1,2,2,96]`) trocando os nós `Pad` por `Concat`+`Slice`.
2. `erb_dec`: `h_in [2,1,256]`/`h_out` religando 2 `Slice` e concatenando as duas GRU.
3. `df_dec`: `h_in [2,1,256]`/`h_out` e anel `c0_buf [1,64,4,96]`.
4. Copia `config.ini`; verificação de paridade S=1 contínuo vs S=8 em lote, tolerância 1e-4 (OpenVINO).
Observação [INFERÊNCIA]: o script **não trata** entradas `gamma/beta`; ele só faz `append` de novas entradas, logo funciona sobre grafos com FiLM (as entradas extras ficam depois de `h_in` etc. na ordem de declaração); a ordem das entradas e a verificação de paridade precisam ser revistas (a verificação passa `h_in` etc. por nome; teria de passar também `gamma`/`beta`). O `README.md` de `models/stateful/` e o plano da Fase 4 já registram que stateful e FiLM são assets distintos a registrar no `ModelAssetRegistry` com governança própria.

---

## 6. Plano mínimo sugerido (para decisão, não executado)

1. Ambiente: `uv` com Python 3.11, torch cu130, `numpy<2`, `deepfilterlib==0.5.6`; patch de `df/io.py` para torchaudio novo.
2. Teste de aceitação 0: carregar `DeepFilterNet3.zip` + `pdeepfilternet3` com FiLM zerado; `max_abs_diff == 0` contra o DFNet3 original em lote fixo.
3. Dataloader próprio em Python (mistura com controle de locutor, SIR, enrollment), features via `libdf` (`df_features`), HDF5 ou arquivos FLAC por locutor; medir 200 passos (tempo, VRAM, RAM).
4. Estágio A (só gerador) -> Estágio B (descongelar, LR baixo); métricas: PESQ/DNSMOS e TSOS nas três condições (alvo+ruído, alvo+interferente, os três) e regressão no modo sem locutor contra o DFNet3.
5. Export: ambiente torch 1.13.1; `gen_film_onnx.py` + `make_stateful_onnx.py` (revisando a verificação para `gamma/beta`); `enrollment.onnx` = encoder de locutor + gerador FiLM com saída `gamma_enc, beta_enc, gamma_df, beta_df` (256 cada).

## 7. Lacunas desta pesquisa

- Não abri `DeepFilterNet3.zip` nem verifiquei wheels de torch 2.13 para cp311 no índice cu130.
- Não li `loss.py` por completo (semântica de `factor_under`) nem o código do `Loss` de SNR local; usei resumos.
- Não verifiquei a licença dos pesos de encoder de locutor (SpeechBrain ECAPA, WeSpeaker) nas páginas oficiais, só em resultados de busca.
- Custos (horas, VRAM, tamanho FLAC) são estimativas sem benchmark.
- A hipótese de bug de RIR no laço do interferente (seção 1.4) não foi testada.
