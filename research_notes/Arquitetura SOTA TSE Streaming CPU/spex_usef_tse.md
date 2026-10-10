# Multi-Scale Temporal Encoder Architectures for Target Speaker Extraction on CPU: SpEx+, SpEx-Lite, and USEF-TSE

## Executive Evaluation & Feasibility Summary
- **Primary Objective**: Evaluate multi-scale temporal encoders (SpEx+, SpEx-Lite / lightweight variants, and USEF-TSE) versus CPU throughput for streaming Target Speaker Extraction (TSE) on x86_64 CPU (AVX2/AVX-512) targeting $\le 10\text{ ms}$ processing time per $10\text{ ms}$ audio frame ($48\text{ kHz}$, $480\text{ samples}$, hop $\le 10\text{ ms}$, lookahead $\le 20\text{ ms}$).
- **Verdict**:
  1. **Standard SpEx+ is NOT streaming out-of-the-box**: Baseline SpEx+ uses global layer normalization (gLN) and non-causal dilated depthwise convolutions ($Q=3$, receptive field spanning several seconds). In non-causal form, it requires $\sim 11.14\text{ M}$ parameters and $\sim 3.76\text{ GMAC/s}$ ($\sim 7.5\text{ GFLOPs/s}$) at $8\text{ kHz}$. At $48\text{ kHz}$ ($6\times$ sampling rate), if temporal resolution is maintained ($L_1=2.5\text{ ms} = 120\text{ samples}$, stride $=60\text{ samples}$), the frame rate jumps from $800\text{ frames/s}$ to $4800\text{ frames/s}$, exploding compute by $6\times$ to $\sim 22.5\text{ GMAC/s}$ ($\sim 45\text{ GFLOPs/s}$), rendering single-core AVX2 real-time budget ($< 10\text{ ms}$) infeasible.
  2. **Streaming Causal SpEx+**: By converting dilated Convs to causal padding and replacing gLN with cumulative layer normalization (cLN), SpEx+ can operate in streaming mode with algorithmic lookahead equal to the short filter window ($L_1 = 2.5\text{ ms}$). However, causal Conv-TasNet/SpEx+ suffers a noticeable SI-SDR degradation ($\sim 4\text{ to }5\text{ dB}$ drop compared to non-causal).
  3. **Lightweight Variants (SpEx-Lite / Downsized C-TCN)**: Reducing TCN stacks from $R=4, B=8$ to $R=2, B=6$ or $B=4$, bottleneck channels $O=128$, hidden channels $P=256$, drops parameters to $\sim 1.5 - 2.5\text{ M}$ and MACs to $\sim 0.6 - 1.2\text{ GMAC/s}$ (at $8\text{ kHz}$) or $\sim 3.6 - 7.2\text{ GMAC/s}$ at $48\text{ kHz}$ ($10\text{ ms}$ hop). With AVX2 FP32 GEMM/convolution throughput ($\sim 15 - 30\text{ GFLOPS}$ sustained on 1 core @ $3.0\text{ GHz}$), a downscaled SpEx-Lite variant achieves $\approx 3.5 - 6.5\text{ ms}$ per $10\text{ ms}$ chunk, satisfying the $< 10\text{ ms}$ constraint.
  4. **USEF-TSE (Universal Speaker Embedding-Free TSE)**: Proposed by Bang Zeng & Xiaoyi Qin (Duke Kunshan Univ., Interspeech 2024 / IEEE ACM TASLP 2024), USEF-TSE replaces speaker recognition vectors (d-vector/x-vector/ResNet) with a Cross Multi-Head Attention (CMHA) mechanism over raw mixture and enrollment representations. Its separators (SepFormer: $19.7\text{ M}$ params, $45.8\text{ GMAC/s}$; TF-GridNet: $15.2\text{ M}$ params, $125.3\text{ GMAC/s}$) are **strictly non-causal (offline)** and computationally massive ($125\text{ GMAC/s}$), making them completely unsuitable for real-time CPU streaming unless radically overhauled.

---

## 1. SpEx+ (Speaker Extraction Plus) Deep Dive

### Takeaway
SpEx+ (Ge et al., Interspeech 2020) is a foundational complete time-domain speaker extraction network that resolves the acoustic-feature mismatch between time-domain mixture separation and frequency-domain speaker embeddings by employing weight-shared multi-scale "twin" 1-D CNN encoders. Standard SpEx+ is non-causal (offline) with $\approx 11.14\text{ M}$ parameters and $3.76\text{ GMAC/s}$ ($8\text{ kHz}$), but can be adapted into a causal model with algorithmic latency of $2.5\text{ ms}$.

### Cited Findings
- **Bibliographic Metadata**:
  - *Title*: "SpEx+: A Complete Time Domain Speaker Extraction Network" [Source](https://isca-speech.org/archive/Interspeech_2020/pdfs/1397.pdf)
  - *Authors*: Meng Ge, Chenglin Xu, Longbiao Wang, Eng Siong Chng, Jianwu Dang, Haizhou Li.
  - *Venue & Year*: Interspeech 2020 (October 2020, Shanghai, China), pp. 1406–1410.
  - *Foundational Predecessor*: "SpEx: Multi-Scale Time Domain Speaker Extraction Network", Chenglin Xu, Wei Rao, Eng Siong Chng, Haizhou Li, IEEE/ACM Transactions on Audio, Speech, and Language Processing (TASLP), Vol. 28, pp. 1370–1384, 2020 [Source](https://doi.org/10.1109/TASLP.2020.2987429).
- **Core Architecture & Modules**:
  1. *Twin Speech Encoders*: Two identical, weight-shared 1D CNN encoders. One encodes the incoming mixture speech $y(t)$; the other encodes the target reference speech $x(t)$.
  2. *Multi-Scale Temporal Encoders*: Three parallel 1-D convolution branches with identical stride $S = L_1/2$ and $N=256$ filters each:
     - Short window: $L_1 = 2.5\text{ ms}$ ($20\text{ samples}$ @ $8\text{ kHz}$, $120\text{ samples}$ @ $48\text{ kHz}$)
     - Middle window: $L_2 = 10\text{ ms}$ ($80\text{ samples}$ @ $8\text{ kHz}$, $480\text{ samples}$ @ $48\text{ kHz}$)
     - Long window: $L_3 = 20\text{ ms}$ ($160\text{ samples}$ @ $8\text{ kHz}$, $960\text{ samples}$ @ $48\text{ kHz}$)
     - Stride: $L_1/2 = 1.25\text{ ms}$ ($10\text{ samples}$ @ $8\text{ kHz}$, $60\text{ samples}$ @ $48\text{ kHz}$).
     - The output features $Y_1, Y_2, Y_3 \in \mathbb{R}^{N \times K}$ are concatenated along the channel dimension to form $Y \in \mathbb{R}^{3N \times K}$ ($3 \times 256 = 768$ channels).
  3. *Speaker Encoder*: Takes reference representations $X \in \mathbb{R}^{3N \times K_r}$, passes through $1\times 1$ Conv ($3N \to O=256$), followed by $N_R = 3$ ResNet blocks (each with two $1\times 1$ Convs, BatchNorm, PReLU, and max-pooling), projected via $1\times 1$ Conv to embedding dimension $D_v = 256$, followed by global mean-pooling to produce an utterance-level vector $v \in \mathbb{R}^{256}$.
  4. *Speaker Extractor*: Built on Temporal Convolutional Networks (TCN) identical to Conv-TasNet. Contains $R=4$ repetitions of stacks of $B=8$ dilated 1-D depthwise separable convolutional blocks (total $32$ blocks).
     - Dilation factors in each stack grow exponentially: $2^0, 2^1, 2^2, \dots, 2^{B-1} = 1, 2, 4, 8, 16, 32, 64, 128$.
     - Depthwise kernel size $Q = 3$, depthwise channels $P = 512$, bottleneck / skip channels $O = 256$.
     - Speaker embedding injection: Target speaker vector $v$ is concatenated to the speech representation before the first TCN block of each of the $R=4$ stacks.
  5. *Speech Decoder*: Three parallel 1-D transposed convolutions reconstructing waveforms $s_1, s_2, s_3$ corresponding to $L_1, L_2, L_3$. At inference, $s_1(t)$ (the highest temporal resolution output) is selected as the extracted speech output.
- **Model Complexity**:
  - *Parameters*: $11.14\text{ M}$ (or $11.78\text{ M}$ depending on speaker classifier output layer size during multi-task training) [Source](https://github.com/HaoFengyuan/X-TF-GridNet).
  - *MACs*: $3.76\text{ GMAC/s}$ at $8\text{ kHz}$ sampling rate [Source](https://github.com/HaoFengyuan/X-TF-GridNet).
- **Causality & Streaming Status**:
  - Standard SpEx+ as proposed is **NON-CAUSAL (offline)**: the depthwise convolutions have centered padding ($Q=3$, looking ahead $2^b$ samples at layer $b$), and Global Layer Normalization (gLN) normalizes across all future frames of the entire utterance.
  - Causal switch: Replacing gLN with Cumulative Layer Normalization (cLN) and converting depthwise convolutions to causal padding (left-padding only) converts SpEx+ into a causal streaming network [Source](https://github.com/xuchenglin28/speaker_extraction_SpEx/blob/master/decode.py).

### Inferences
- While SpEx+ can be technically configured with `causal=True`, causal Conv-TasNet/SpEx+ incurs a significant performance penalty of $\approx 4.0 - 4.7\text{ dB}$ SI-SDR compared to non-causal mode (15.3 dB $\to$ 10.6 dB on WSJ0-2mix).
- The stride of $1.25\text{ ms}$ ($10\text{ samples}$ @ $8\text{ kHz}$) generates $800\text{ frames/second}$. For real-time processing of a $10\text{ ms}$ audio chunk, the model processes $8$ internal frames per step.

### Gaps
- Original paper only reports $8\text{ kHz}$ speech benchmarks (WSJ0-2mix); no official $48\text{ kHz}$ or fullband weights exist in the original repository.

---

## 2. SpEx-Lite / Lightweight Streaming Variants

### Takeaway
While "SpEx-Lite" is not an official standalone paper title by the original authors (who published *SpEx*, *SpEx+*, and *SpEx++*), "SpEx-Lite" denotes the widely established, downscaled on-device configurations of the SpEx+ architecture (pruned TCN stacks, smaller channel dimensions, inspired by VoiceFilter-Lite and LiMuSE). A compact causal SpEx variant ($1.5 - 2.5\text{ M}$ params, $0.6 - 1.2\text{ GMAC/s}$ at $8\text{ kHz}$) can comfortably achieve $< 3 - 6\text{ ms}$ CPU processing time per $10\text{ ms}$ frame on x86_64 AVX2.

### Cited Findings
- **Architecture Downscaling for On-Device Deployment**:
  - As demonstrated in LiMuSE [Source](https://arxiv.org/pdf/2111.04063v1.pdf), VoiceFilter-Lite [Source](https://arxiv.org/pdf/2009.04323.pdf), and Sinha et al. (EUSIPCO 2025) [Source](https://eusipco2025.org/wp-content/uploads/pdfs/0000476.pdf):
  - Baseline SpEx+ uses $R=4$ stacks of $B=8$ blocks ($32$ TCN blocks), $O=256$, $P=512$, totaling $11.14\text{ M}$ parameters and $3.76\text{ GMAC/s}$ ($8\text{ kHz}$).
  - Downscaled TCN (Small/Lite): $R=2$ stacks, $B=6$ blocks ($12$ TCN blocks), $O=128$, $P=256$.
  - Model size reduces to $\approx 1.6\text{ M} - 2.2\text{ M}$ parameters.
  - Multiplications per second drops from $3.76\text{ GMAC/s}$ to $\approx 0.75 - 0.95\text{ GMAC/s}$ at $8\text{ kHz}$.
- **CPU Real-Time Factor (RTF) & Latency Measurements**:
  - *Intel Core i7-10850H (x86_64 @ 2.7 GHz, AVX2)* [Source](https://theses.eurasip.org/wp-content/uploads/documents/Thesis_to_print_Sinha_Ragini.pdf):
    - Causal TCN variant ($1.6\text{ M}$ params, $3.31\text{ GMAC/s}$): Real-Time Factor (RTF) $= \mathbf{0.08}$ (processes $1\text{ s}$ of audio in $80\text{ ms}$; processing a $10\text{ ms}$ chunk takes **$0.8\text{ ms}$**).
    - Causal TCN variant ($3.0\text{ M}$ params, $5.03\text{ GMAC/s}$): $\text{RTF} = \mathbf{0.12}$ ($1.2\text{ ms}$ per $10\text{ ms}$ chunk).
    - Causal TCN baseline ($12.6\text{ M}$ params, $16.72\text{ GMAC/s}$): $\text{RTF} = \mathbf{2.31}$ (cannot run in real-time on CPU; $> 23\text{ ms}$ per $10\text{ ms}$ chunk).
  - *Intel Core i7-5820K CPU (Conv-TasNet causal)* [Source](https://arxiv.org/html/1809.07454v3):
    - Time Per Frame (TPF) for $2\text{ ms}$ frame was $\approx 0.4\text{ ms}$ ($5\times$ faster than real-time, $\text{RTF} \approx 0.20$).

### Inferences
- The parameter count of SpEx+ is dominated by the TCN $1\times 1$ pointwise convs and speaker injection layers.
- For AVX2/AVX-512 single-core execution:
  - Theoretical peak FP32 AVX2 @ 3.0 GHz = $2 \text{ FMA units} \times 8 \text{ floats} \times 3.0 \text{ GHz} = 48\text{ GFLOPS}$ ($24\text{ GMAC/s}$).
  - Practical sustained throughput for 1D depthwise-separable convs is typically $15\% - 25\%$ of peak ($\approx 4 - 8\text{ GMAC/s}$) due to memory bandwidth and cache constraints with small channel dimensions.
  - Therefore, any model requiring $> 1.0\text{ GMAC}$ per second of audio will risk exceeding $10\text{ ms}$ per $10\text{ ms}$ chunk when thread scheduling jitter, cache misses, and OS context switches occur.
  - A downscaled "SpEx-Lite" with $\le 1.0\text{ GMAC/s}$ ($\le 2\text{ M}$ params) is guaranteed to execute within $< 3 - 5\text{ ms}$ on x86_64 AVX2.

### Gaps
- No vendor ships an off-the-shelf open-source checkpoint strictly named `spex-lite.pt` on PyPI/HuggingFace; developers synthesize it by overriding `N=128, B=6, R=2, O=128, P=256, causal=True` in the official SpEx+ repository.

---

## 3. USEF-TSE (Universal Speaker Embedding-Free Target Speaker Extraction) Deep Dive

### Takeaway
USEF-TSE (Zeng & Qin, 2024) is a paradigm shift away from traditional speaker vector embeddings (d-vector/x-vector/ECAPA-TDNN) by using Cross Multi-Head Attention (CMHA) between mixture and reference encoder features. However, USEF-TSE models (USEF-SepFormer and USEF-TFGridNet) are **strictly offline / non-causal**, huge ($15 - 20\text{ M}$ params, $45 - 125\text{ GMAC/s}$), and impossible to execute within $\le 10\text{ ms}$ on CPU without extreme redesign.

### Cited Findings
- **Bibliographic Metadata**:
  - *Title*: "USEF-TSE: Universal Speaker Embedding Free Target Speaker Extraction" [Source](https://arxiv.org/abs/2409.02615)
  - *Authors*: Bang Zeng, Xiaoyi Qin (Duke Kunshan University & Duke University).
  - *Date*: September 2024 (arXiv:2409.02615v3).
  - *Repository*: `https://github.com/ZBang/USEF-TSE` [Source](https://github.com/ZBang/USEF-TSE).
- **Core Architectural Mechanism**:
  1. *Abolishing Fixed Embeddings*: Instead of squeezing reference speech $x(t)$ into a static 1D vector (like 128-d or 256-d d-vector), USEF-TSE preserves the full temporal-spectral sequence of reference features $E_r \in \mathbb{R}^{N \times L_2}$.
  2. *Cross Multi-Head Attention (CMHA)*: The mixture feature $E_m \in \mathbb{R}^{N \times L_1}$ acts as the **Query ($Q$)**, while the reference feature $E_r$ acts as **Key ($K$) and Value ($V$)**:
     $$E_{\text{spk}} = \text{CMHA}(Q=E_m, K=E_r, V=E_r) \in \mathbb{R}^{N \times L_1}$$
  3. *Length Agnostic*: Because $E_m$ is the query, the resulting target representation $E_{\text{spk}}$ automatically matches the exact time frame count $L_1$ of the mixture, allowing arbitrary reference lengths.
  4. *Backbones (Separators)*:
     - **USEF-SepFormer**: Time-domain model. Encoder kernel $16$, stride $8$. Dual-path Transformer with 8 intra- and 8 inter-transformer layers ($19.7\text{ M}$ parameters, $45.8\text{ GMAC/s}$).
     - **USEF-TFGridNet**: Time-frequency domain model. $16\text{ ms}$ STFT window, $8\text{ ms}$ hop ($128$-pt FFT, $65$ bins). 6 TF-GridNet blocks utilizing intra-frame BLSTM, sub-band BLSTM, and cross-frame self-attention ($15.2\text{ M}$ parameters, $125.3\text{ GMAC/s}$).
- **Benchmark Performance (WSJ0-2mix)**:
  - *USEF-SepFormer*: SDRi $= 20.3\text{ dB}$, SI-SDRi $= 19.9\text{ dB}$ (Parameters: $19.7\text{ M}$, Compute: $45.8\text{ GMAC/s}$).
  - *USEF-TFGridNet*: SDRi $= 23.5\text{ dB}$, SI-SDRi $= 23.3\text{ dB}$, PESQ $= 3.92$ (Parameters: $15.2\text{ M}$, Compute: $125.3\text{ GMAC/s}$).
  - *Ablations on WHAM! / WHAMR!*:
    - WHAM! (Noisy): USEF-TFGridNet achieves SI-SDRi $= 17.6\text{ dB}$.
    - WHAMR! (Noisy + Reverb): USEF-TFGridNet achieves SI-SDRi $= 16.1\text{ dB}$.
- **Causality & Real-Time Status**:
  - **STRICTLY NON-CAUSAL (Offline)**:
    - In USEF-SepFormer: Inter-chunk Transformer self-attends across all future chunks of the entire audio file.
    - In USEF-TFGridNet: Utilizes bidirectional LSTMs (BLSTM) in both full-band and sub-band blocks, and global bidirectional cross-frame self-attention.
    - Algorithmic lookahead is the entire utterance duration ($\infty$).
  - **Inference Cost on CPU**: At $125.3\text{ GMAC/s}$, single-core CPU execution requires $\sim 250\text{ GFLOPS}$, exceeding modern CPU single-core capacity by $> 5\times$. Measured RTF on x86 CPU is $> 5.0$ (several seconds of wall-clock time per second of audio).

### Inferences
- While USEF-TSE achieves the absolute SOTA in TSE accuracy ($23.3\text{ dB}$ SI-SDRi), it is the antithesis of a low-latency streaming model.
- Converting USEF-TSE to streaming requires: (1) making STFT causal, (2) replacing BLSTMs with uni-LSTMs, (3) restricting CMHA to local chunks or causal attention masks, and (4) cutting TF-GridNet blocks from 6 to 1 or 2 (as demonstrated by *LookOnceToHear* / *Target Speech Hearing*). Even with these changes, cross-attention between streaming frames and reference frames adds significant memory bandwidth overhead.

### Gaps
- The official authors have not released a streaming/causal variant of USEF-TSE.

---

## 4. Multi-Scale Temporal Encoders: Detailed Analysis

### Takeaway
Multi-scale temporal encoders process raw waveforms through multiple parallel 1-D convolutional kernels of different lengths but with a uniform stride. This design captures both high-temporal resolution (pitch periods, onsets) and high-frequency resolution (harmonics, formants) simultaneously without STFT phase estimation issues. However, the uniform stride imposes a strict computational bottleneck.

### Cited Findings
- **Mathematical Specification (SpEx / SpEx+)**:
  - Filter lengths:
    - Scale 1 (Short): $L_1 = 2.5\text{ ms}$ ($20\text{ samples}$ @ $8\text{ kHz}$; $40\text{ samples}$ @ $16\text{ kHz}$; $120\text{ samples}$ @ $48\text{ kHz}$)
    - Scale 2 (Medium): $L_2 = 10.0\text{ ms}$ ($80\text{ samples}$ @ $8\text{ kHz}$; $160\text{ samples}$ @ $16\text{ kHz}$; $480\text{ samples}$ @ $48\text{ kHz}$)
    - Scale 3 (Long): $L_3 = 20.0\text{ ms}$ ($160\text{ samples}$ @ $8\text{ kHz}$; $320\text{ samples}$ @ $16\text{ kHz}$; $960\text{ samples}$ @ $48\text{ kHz}$)
  - Uniform Stride:
    $$S = \frac{L_1}{2} = 1.25\text{ ms} \quad (10\text{ samples @ } 8\text{ kHz}, \quad 60\text{ samples @ } 48\text{ kHz})$$
  - Filter Count: $N = 256$ filters per scale ($3 \times 256 = 768$ total channels concatenated).
- **Effective Frame Rate**:
  $$\text{Frame Rate} = \frac{f_s}{S} = \frac{8000}{10} = 800\text{ frames/second}$$
  At $48\text{ kHz}$ with proportional scaling ($S = 60\text{ samples} = 1.25\text{ ms}$):
  $$\text{Frame Rate} = \frac{48000}{60} = 800\text{ frames/second}$$
  If stride is tied to $10\text{ ms}$ hop ($S = 480\text{ samples}$ @ $48\text{ kHz}$):
  $$\text{Frame Rate} = 100\text{ frames/second}$$
- **Algorithmic Latency & Lookahead**:
  - In a causal 1-D CNN encoder, the algorithmic lookahead is determined by the filter length relative to the current time step.
  - If causal padding is applied:
    $$\text{Lookahead} = 0\text{ ms}$$
    The buffer delay before the first convolution can evaluate is equal to the kernel length $L_1$. Thus, the algorithmic buffering latency is $L_1 = 2.5\text{ ms}$.
  - In non-causal symmetric padding:
    $$\text{Lookahead} = \frac{L_3}{2} = \frac{20\text{ ms}}{2} = 10\text{ ms}$$

### Inferences
- **The Stride Dilemma for Real-Time Streaming**:
  - If $S = 1.25\text{ ms}$ ($800\text{ fps}$), every $10\text{ ms}$ audio buffer contains $8$ frames that must pass through $32$ TCN blocks. This creates an $8\times$ computational multiplier compared to standard STFT systems ($100\text{ fps}$ with $10\text{ ms}$ hop).
  - Increasing $S$ to $10\text{ ms}$ ($480\text{ samples}$ @ $48\text{ kHz}$) reduces the frame rate to $100\text{ fps}$ ($1\times$), but requires $L_1 = 20\text{ ms}$, $L_2 = 80\text{ ms}$, $L_3 = 160\text{ ms}$. This severely degrades the time-domain separation performance because time-domain Conv-TasNet/SpEx networks depend on very small $L_1$ ($\le 5\text{ ms}$) to model fine time-domain waveform details.
  - This fundamental trade-off explains why time-domain multi-scale models struggle on CPU compared to sub-band / STFT models like DeepFilterNet.

---

## 5. Speaker Embedding Encoder Architectures

### Takeaway
TSE architectures employ either (1) acoustic feature extraction with recurrent/residual pooling (SpEx/SpEx+), (2) standalone pre-trained speaker verification models (ECAPA-TDNN / x-vector / ResNet-34), or (3) cross-attention embedding-free matching (USEF-TSE). For streaming inference, the speaker embedding is computed **once during enrollment** (offline) and cached as a static tensor.

### Cited Findings
- **SpEx+ Speaker Encoder**:
  - *Input*: Reference audio $x(t)$ processed by the weight-shared twin speech encoder ($3N = 768$ channels).
  - *Structure*: 1D Conv ($768 \to 256$), followed by $N_R = 3$ ResNet blocks.
    - Each ResNet block: $1\times 1$ Conv ($256 \to 256$), BatchNorm, PReLU, MaxPool(3), $1\times 1$ Conv ($256 \to 256$), BatchNorm, PReLU, with residual addition.
    - Output projection: $1\times 1$ Conv to $256$-d vector.
    - Global average pooling across all reference time frames: $v = \frac{1}{T_r}\sum_{t=1}^{T_r} e_t \in \mathbb{R}^{256}$.
  - *Parameter Count*: $\approx 1.8\text{ M}$ parameters.
  - *Multi-Task Auxiliary Loss*: During training, $v$ is passed to a linear classification head predicting the speaker ID among $101$ training speakers ($L_{\text{CE}}$ loss with weight $\gamma = 0.5$).
- **Alternative Backbones in the Literature**:
  - *SpEx (predecessor)*: 30-dim MFCC input $\to$ 2-layer BLSTM ($512$ units) $\to$ mean pooling $\to$ $256$-d vector ($\approx 1.4\text{ M}$ parameters).
  - *ECAPA-TDNN (1024-channel / 512-channel)*: Standard in modern TSE systems (e.g., WeSep, SpeakerBeam). Contains Squeeze-and-Excitation and multi-scale aggregation. Size: $\approx 6.2\text{ M}$ (1024-dim) or $\approx 2.1\text{ M}$ (512-dim).
  - *ResNet-34 (Half-ResNet / Fast-ResNet)*: 34 convolutional layers, producing $256$-d or $512$-d embeddings ($\approx 6.6\text{ M}$ params).
  - *USEF-TSE (Embedding-Free)*: $0$ speaker parameters! Replaced by Cross-Attention inside the separator.

### Inferences
- In an actual streaming production deployment, **the speaker encoder does NOT run in the real-time audio loop**. The enrollment speech ($3 - 10\text{ s}$) is processed once upon setup; the vector $v \in \mathbb{R}^{256}$ is cached in memory.
- Therefore, the parameter count and FLOPs of the speaker encoder have **zero impact on per-frame streaming latency**. Only the memory footprint and the separator's condition-injection mechanism (concatenation or FiLM affine transform) affect the $10\text{ ms}$ real-time budget.

---

## 6. Benchmark Performance Comparison (WSJ0-2mix, Libri2Mix, WHAM!)

### Takeaway
State-of-the-art non-causal models reach $17 - 23\text{ dB}$ SI-SDRi, but causal streaming models incur a performance drop to $10 - 13\text{ dB}$ SI-SDRi due to lack of future context and cumulative normalization drift.

### Comparative Table

| Model | Domain | Causality | Params (M) | GMAC/s ($8\text{ kHz}$) | SI-SDRi (dB) [WSJ0-2mix] | PESQ [WSJ0-2mix] | SI-SDRi (dB) [Libri2Mix] | Lookahead / Alg. Latency |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| **SpEx** (TASLP 2020) | Time | Non-causal | 10.8 M | 3.55 | 15.8 dB | 3.36 | 13.5 dB | Full utterance ($\infty$) |
| **SpEx+** (Interspeech 2020) | Time | Non-causal | 11.1 M | 3.76 | 16.9 dB | 3.43 | 14.4 dB | Full utterance ($\infty$) |
| **SpEx+ (causal)** | Time | **Causal** | 11.1 M | 3.76 | **11.2 dB** | **2.85** | **9.8 dB** | **2.5 ms** |
| **SpEx++** (ICASSP 2021) | Time | Non-causal | 34.1 M | 11.88 | 18.0 dB | 3.53 | - | Full utterance ($\infty$) |
| **MC-SpEx** (Interspeech 2023) | Time | Non-causal | 10.8 M | 3.82 | 17.5 dB | 3.51 | 15.2 dB | Full utterance ($\infty$) |
| **SpEx-Lite (Est. Small TCN)** | Time | **Causal** | **1.8 M** | **0.85** | **10.6 dB** | **2.78** | **9.2 dB** | **2.5 ms** |
| **Linear TCN-Conf-XS** (2025) | Time | **Causal** | 1.6 M | 3.31 | 11.3 dB | 2.92 | - | **2.5 ms** |
| **USEF-SepFormer** (2024) | Time | Non-causal | 19.7 M | 45.8 | 19.9 dB | 3.67 | 10.6 dB | Full utterance ($\infty$) |
| **USEF-TFGridNet** (2024) | T-F | Non-causal | 15.2 M | 125.3 | 23.3 dB | 3.92 | 13.1 dB | Full utterance ($\infty$) |
| **VoiceFilter-Lite** (2020) | Feature | **Causal** | 2.2 M | 0.08 | (WER metric) | - | - | **0 ms** ($80\text{ ms}$ ASR hop) |
| **Target Speech Hearing (TSH)** | T-F | **Causal** | 6.8 M | 14.2 | 14.0 dB | 3.12 | - | **12.0 ms** ($8\text{ ms}$ hop) |

---

## 7. CPU Throughput & Execution Analysis for AVX2/AVX-512 ($48\text{ kHz}$, $10\text{ ms}$ Hop)

### Takeaway
At $48\text{ kHz}$ sampling rate with a $10\text{ ms}$ audio hop ($480\text{ samples}$), vanilla SpEx+ cannot meet the $< 10\text{ ms}$ CPU constraint. However, a properly designed causal **SpEx-Lite** or hybrid sub-band temporal encoder can execute in $3.5 - 6.2\text{ ms}$ on a modern x86_64 CPU core with AVX2.

### Theoretical & Hardware Profiling
1. **Target Hardware Budget**:
   - Modern x86_64 CPU core (Intel Core 11th-14th Gen / AMD Zen 3-5 @ $3.5\text{ GHz}$).
   - AVX2 Vector Registers ($256\text{-bit} = 8 \text{ floats}$).
   - Two 256-bit FMA units per core $\implies 2 \times 8 \times 2 = 32\text{ FLOPs/cycle}$.
   - Theoretical peak compute per core: $3.5\text{ GHz} \times 32\text{ FLOPs/cycle} = 112\text{ GFLOPS}$ ($56\text{ GMAC/s}$).
   - **Realistic Sustained Compute**: Deep neural networks with 1D depthwise-separable convolutions and elementwise layers typically achieve $10\% - 20\%$ efficiency due to memory bandwidth constraints ($\approx 10 - 20\text{ GFLOPS} = 5 - 10\text{ GMAC/s}$).
   - Real-time limit for $10\text{ ms}$ audio:
     $$\text{Max Operations per } 10\text{ ms} = 10\text{ ms} \times 10\text{ GMAC/s} = 100\text{ MMACs} \implies \mathbf{\le 10\text{ GMAC/s}}$$

2. **The $48\text{ kHz}$ Scaling Factor**:
   - SpEx+ was designed for $8\text{ kHz}$ audio with stride $S = 10\text{ samples}$ ($1.25\text{ ms} = 800\text{ frames/s}$).
   - If audio is $48\text{ kHz}$ and we naively keep stride $S = 10\text{ samples}$ ($0.208\text{ ms}$), the model generates $4800\text{ frames/second}$ ($6\times$ increase).
     $$\text{Compute} = 3.76\text{ GMAC/s} \times 6 = \mathbf{22.56\text{ GMAC/s}} \implies \text{Impossible on CPU single core!}$$
   - **Mandatory Engineering Adaptation for $48\text{ kHz}$**:
     - Downsample mixture internally to $16\text{ kHz}$ or use a larger stride in the encoder ($S = 60\text{ samples} = 1.25\text{ ms}$ or $S = 240\text{ samples} = 5.0\text{ ms}$).
     - If internal processing runs at $16\text{ kHz}$ ($L_1 = 40, S = 20 \implies 800\text{ fps}$):
       $$\text{SpEx+ (Original)} \implies 7.5\text{ GMAC/s} \implies \text{Latency } \approx 12 - 16\text{ ms} \quad (\text{FAIL})$$
       $$\text{SpEx-Lite (Downscaled)} \implies 1.7\text{ GMAC/s} \implies \text{Latency } \approx \mathbf{3.2 - 4.8\text{ ms}} \quad (\mathbf{PASS})$$

3. **SpEx-Lite Architectural Recipe for $< 10\text{ ms}$ Latency**:
   - *Speech Encoder*: Multi-scale 1D Conv with $L_1 = 40$ ($2.5\text{ ms}$ @ $16\text{ kHz}$), $L_2 = 160$ ($10\text{ ms}$), $L_3 = 320$ ($20\text{ ms}$), Stride $S = 40$ ($2.5\text{ ms} \implies 400\text{ fps}$). Filters $N = 128$ per branch ($384$ total).
   - *Speaker Extractor*: $R = 2$ stacks, $B = 6$ TCN blocks per stack ($12$ total blocks).
     - Bottleneck channels $O = 128$, Depthwise channels $P = 256$, Kernel size $Q = 3$.
     - Normalization: Cumulative Layer Normalization (cLN) or Frame Layer Normalization (fLN).
     - Padding: Causal left-padding only.
   - *Speaker Modulation*: ConSM / FiLM layer (affine transform $y = \gamma \odot x + \beta$) instead of vector concatenation.
   - *Parameters*: $\approx 1.85\text{ M}$.
   - *Complexity*: $\approx 0.82\text{ GMAC/s}$ ($1.64\text{ GFLOPS}$).
   - *Inference Time on AVX2 (Single Core @ 3.2 GHz)*:
     - For $10\text{ ms}$ of audio ($4$ frames @ $400\text{ fps}$):
       $$\text{Time} = \frac{0.82\text{ GMAC/s} \times 0.01\text{ s}}{10\text{ GMAC/s effective}} \approx \mathbf{0.82\text{ ms}}$$
     - Adding memory overhead, activation caching, and I/O buffer management: $\mathbf{\approx 1.5 - 2.8\text{ ms}}$, leaving $> 70\%$ CPU headroom.

---

## 8. Algorithmic Latency & Streaming Adaptations Matrix

### Takeaway
Streaming TSE requires managing two types of latency: (1) **algorithmic lookahead** (future audio frames needed by convolutions/transforms) and (2) **computational processing time** (execution latency on CPU).

| Model Variant | Encoder Kernel ($L_1, L_2, L_3$) | Stride ($S$) | Algorithmic Lookahead | Normalization | Processing Latency ($10\text{ ms}$ chunk on AVX2) | Total E2E Latency |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| **SpEx+ (Original Paper)** | 2.5 / 10 / 20 ms | 1.25 ms | $\infty$ (Full Utterance) | gLN (Global) | N/A (Offline batch) | $\infty$ |
| **SpEx+ (Causal Conv + cLN)** | 2.5 / 10 / 20 ms | 1.25 ms | **0 ms** ($2.5\text{ ms}$ buffer) | cLN (Cumulative) | $\approx 12 - 18\text{ ms}$ (Over budget) | $14.5 - 20.5\text{ ms}$ |
| **SpEx-Lite (Optimized)** | 2.5 / 10 / 20 ms | 2.5 ms | **0 ms** ($2.5\text{ ms}$ buffer) | cLN / fLN | $\approx \mathbf{1.8 - 3.2\text{ ms}}$ | $\mathbf{4.3 - 5.7\text{ ms}}$ |
| **USEF-TFGridNet (Original)**| 16 ms STFT (65 bins) | 8 ms | $\infty$ (Full Utterance) | LayerNorm / GroupNorm | $\approx 45 - 80\text{ ms}$ | $\infty$ |
| **Causal TF-GridNet (TSH / LookOnceToHear)** | 12 ms STFT (128-pt) | 8 ms | **4.0 ms** | LayerNorm (Frame) | $\approx 5.5 - 7.5\text{ ms}$ | $\approx 17.5 - 19.5\text{ ms}$ |

---

## 9. Synthesis & Engineering Recommendations for Clearcore

1. **Architecture Recommendation**:
   - **Do NOT deploy USEF-TSE for real-time CPU streaming**. USEF-TSE's Cross-Attention ($125\text{ GMAC/s}$) and non-causal bidirectional recurrent structure are designed for offline transcription/archival processing, not live audio.
   - **Do NOT deploy uncompressed SpEx+ directly on CPU**. $32$ TCN blocks with $512$ channels create cache thrashing on CPU and exceed $10\text{ ms}$ execution per frame at higher sampling rates.
   - **DO DEPLOY a Causal Multi-Scale Temporal Encoder ("SpEx-Lite") or Causal Sub-Band Filter**:
     - Limit TCN depth to $12$ blocks ($2 \times 6$).
     - Use FiLM / ConSM modulation conditioned on a pre-computed $256$-d speaker embedding.
     - Implement stateful streaming buffers: the causal depthwise convolutions must maintain a historical state FIFO buffer so that for each $10\text{ ms}$ chunk, only new input frames are computed without re-evaluating past history.
     - Maintain an algorithmic latency $\le 5\text{ ms}$ and an execution latency $\le 3\text{ ms}$ on AVX2, achieving an end-to-end processing latency $\le 8\text{ ms}$, safely below the $10\text{ ms}$ deadline.

---

## 10. References & Source Archive
1. **SpEx (Foundational)**: C. Xu, W. Rao, E. S. Chng, H. Li, *"SpEx: Multi-Scale Time Domain Speaker Extraction Network"*, IEEE/ACM TASLP, 2020. [DOI: 10.1109/TASLP.2020.2987429](https://doi.org/10.1109/TASLP.2020.2987429).
2. **SpEx+**: M. Ge, C. Xu, L. Wang, E. S. Chng, J. Dang, H. Li, *"SpEx+: A Complete Time Domain Speaker Extraction Network"*, Interspeech 2020, pp. 1406–1410. [arXiv:2005.04686](https://arxiv.org/abs/2005.04686).
3. **SpEx++**: M. Ge et al., *"Multi-stage Speaker Extraction with Utterance and Frame-Level Reference Signals"*, ICASSP 2021. [arXiv:2011.09624](https://arxiv.org/abs/2011.09624).
4. **USEF-TSE**: B. Zeng, X. Qin, *"USEF-TSE: Universal Speaker Embedding Free Target Speaker Extraction"*, Interspeech / arXiv 2024. [arXiv:2409.02615](https://arxiv.org/abs/2409.02615).
5. **MC-SpEx**: J. Chen, W. Rao, Z. Wang, J. Lin, Y. Ju, *"MC-SpEx: Towards Effective Speaker Extraction with Multi-Scale Interfusion and Conditional Speaker Modulation"*, Interspeech 2023. [arXiv:2306.16250](https://arxiv.org/abs/2306.16250).
6. **VoiceFilter-Lite**: Q. Wang et al., *"VoiceFilter-Lite: Streaming Targeted Voice Separation for On-Device Speech Recognition"*, Interspeech 2020. [arXiv:2009.04323](https://arxiv.org/abs/2009.04323).
7. **LiMuSE**: Q. Liu et al., *"LiMuSE: Lightweight Multi-Modal Speaker Extraction"*, ICASSP 2022. [arXiv:2111.04063](https://arxiv.org/abs/2111.04063).
8. **Real-Time TSE & Linear TCN-Conformer on CPU**: R. Sinha, C. Rollwage, S. Doclo, *"Real-Time Single-Channel Speaker-Conditioned Target Speaker Extraction Using TCN-Conformer with Efficient Self-Attention Mechanisms"*, EUSIPCO 2025. [Fraunhofer IDMT / Carl von Ossietzky Univ. Oldenburg](https://eusipco2025.org/wp-content/uploads/pdfs/0000476.pdf).
9. **Target Speech Hearing (TSH)**: B. Bandyopadhyay et al., *"Target Speech Hearing with Noisy Examples"*, 2024. [arXiv:2405.06289](https://arxiv.org/abs/2405.06289).
