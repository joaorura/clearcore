# Technical Note: TF-GridNet Streaming/Causal Variants for Speech Separation & Target Speaker Extraction on CPU

**Target System Context**: Real-time virtual microphone / edge audio pipeline operating at **48 kHz**, **10 ms hop size (480 samples)**, **x86_64 CPU (AVX2 / AVX-512)**, with strict latency requirement **< 10 ms**.

---

## 1. Original TF-GridNet Profile

* **Authors**: Zhong-Qiu Wang, Samuele Cornell, Shukjae Choi, Younglo Lee, Byeong-Yeol Kim, Shinji Watanabe (Carnegie Mellon University, Università Politecnica delle Marche, Hyundai Motor Company).
* **Venues & Years**:
  * *Conference (preliminary)*: **ICASSP 2023** (*"TF-GridNet: Making Time-Frequency Domain Models Great Again for Monaural Speaker Separation"*, arXiv:2209.03952, Sep 2022 / ICASSP May 2023).
  * *Journal (comprehensive)*: **IEEE/ACM TASLP 2023** (*"TF-GridNet: Integrating Full- and Sub-Band Modeling for Speech Separation"*, IEEE/ACM Trans. Audio, Speech, Lang. Process., vol. 31, pp. 3221–3236, Aug 2023).
* **Model Configurations & Parameters**:
  * Small: ~**2.6M - 5.5M** params.
  * Standard / Baseline: **8.0M** params ($B=6, D=24, H=192$).
  * Large: **14.4M - 14.5M** params ($B=6, D=64, H=256$, where $B$ = blocks, $D$ = embedding dimension, $H$ = LSTM hidden units).
* **Computational Cost**:
  * **~128 to 323.8 GMACs/s** (reported up to ~445 GMACs/s in unpruned configurations at 16 kHz).
  * In standard benchmarks (16 kHz, 4-second input): **~128.7 GMACs/s** (S4M paper) to **323.75 GMACs/s** (TIGER ICLR 2025 paper benchmark; TF-MoE 2026).
* **STFT Configuration**:
  * Window size: **32 ms** (512 samples at 16 kHz / 256 samples at 8 kHz; square-root Hann).
  * Hop size: **8 ms** (128 samples at 16 kHz / 64 samples at 8 kHz; 75% overlap).
  * Frequency bins: $F=257$ (16 kHz) or $F=129$ (8 kHz).
* **Algorithmic Latency**:
  * **Offline / Infinite** (non-causal whole-utterance bidirectional processing).
* **Separation Performance (SI-SDRi)**:
  * **WSJ0-2mix**: **23.4 – 23.5 dB** (anechoic SOTA benchmark, overtaking Conv-TasNet [15.3 dB] and SepFormer [22.3 dB]).
  * **Libri2Mix**: **19.2 – 19.8 dB** SI-SDRi.
  * **WHAMR!** (noisy reverberant): **16.9 – 17.2 dB** SI-SDRi.

---

## 2. Causality Breakdown & Non-Causal Components

Original TF-GridNet processes complex STFT representations (concatenated Real & Imaginary components) through $B$ multi-path blocks. It is **inherently non-causal** due to three key structural stages:

| Component | Architecture in Original TF-GridNet | Causality Status | Mechanism / Future Dependency |
| :--- | :--- | :--- | :--- |
| **Encoder / Unfold** | 2D Conv / Unfold across frames ($I=8$ frames) | **Non-causal** | Kernels span symmetric temporal windows (centered around $t$, accessing $t + \Delta t$). |
| **Intra-frame Spectral Module** | Bidirectional LSTM across frequency axis ($F$) | **Causal in Time** (Non-causal in Freq) | Computes across frequency bins within the *same* time frame $t$. Causality violation is along frequency (which is legitimate within a single frame), but non-causal across time if framed with future context. |
| **Sub-band Temporal Module** | Bidirectional LSTM across time axis ($T$) | **Strictly Non-causal** | Full backward LSTM pass requires future frames up to utterance end ($t \to T$). |
| **Cross-Frame Self-Attention** | Multi-Head Self-Attention over sequence time frames $T$ | **Strictly Non-causal** | Global softmax attention matrix $\text{Softmax}(QK^T / \sqrt{d})$ attends to all past and future frames. |
| **Decoder / Deconv1D** | 2D Transposed Conv / Overlap-Add | **Non-causal** | Kernel overlap reconstruction requires subsequent frames. |

### Causal Conversion Requirements:
1. **Time-axis BLSTM $\to$ Unidirectional LSTM** (or causal linear RNN / Mamba / GRU) with state caching.
2. **Global Attention $\to$ Causal Masked Attention / Cache Memory** (e.g. TFACM diagonal mask) or complete removal.
3. **Convolutional Layers $\to$ Causal padding** (left padding only, zero lookahead) or controlled lookahead buffer (e.g., $K-1$ lookahead frames).

---

## 3. Streaming & Causal Variants (2024–2026 Literature)

Several recent studies adapted or evaluated causal/streaming derivatives of TF-GridNet:

### A. TF-GridNet-Causal / TFACM Baseline (2024–2025)
* **Modifications**: Replaced time-BLSTM with unidirectional LSTM; applied causal lower-triangular attention mask; causal 2D conv left-padding.
* **Separation Performance**:
  * WSJ0-2mix SI-SDRi drops from **23.5 dB (offline)** to **~13.5 – 13.8 dB** (a massive **~9.7 dB penalty**).
  * WHAMR! SI-SDRi drops from **17.2 dB** to **~9.9 dB**.
* **Complexity**: Retains ~**11.4M – 13.8M** params and **~179 GMACs/s**, making it computationally intractable for real-time edge devices despite being mathematically causal.

### B. CHiME-9 Task 2 (ECHI) Official Baseline & Low-Latency TSE (2025–2026)
* **Model**: Causal multi-channel TF-GridNet with FiLM speaker embedding conditioning.
* **Algorithmic Latency**: Fixed at **20 ms total lookahead** at 16 kHz:
  * Centered STFT ($N_{\text{FFT}}=128$, hop $H=64$ / 4 ms): **4 ms** lookahead.
  * $N=3$ causal GridNet blocks with temporal kernel `emb_ks=2`: **12 ms** lookahead.
  * Conv/Deconv layers (kernel 3): **4 ms** lookahead.
  * Total = 320 samples / **20 ms** latency.
* **Observation**: Complex spectrogram regression produced artifacts under low-latency streaming; bounded magnitude masking with mixture phase was required for perceptual stability.

### C. TF-MLPNet (Clarity Workshop / Interspeech 2025)
* **Innovation**: Deconstructed causal TF-GridNet by replacing sequential frequency BLSTMs with an **all-MLP-Mixer** and time LSTMs with **conv-batched LSTMs**.
* **Chunk size / Hop**: **6 ms** chunk, **10 ms** window (algorithmic latency = **10 ms**).
* **Performance**: Achieved **12.37 dB** SI-SDR (floating-point) on real-time hearables, drastically reducing GAP9 runtime from **25.8 ms** (causal TF-GridNet) down to **3.6 ms** (TF-MLPNet) for a 6 ms chunk.

### D. Summary of Offline vs Causal Degradation
Across literature, constraining TF-GridNet to strict causality ($0 - 20\text{ ms}$ lookahead) incurs:
* **~8 to 10 dB SI-SDRi drop** on anechoic speech (from ~23.5 dB down to ~13.5–14.5 dB).
* **~5 to 7 dB drop** on noisy-reverberant speech.
* Full-band cross-frame attention and backward temporal LSTMs account for over 50% of TF-GridNet's separating power.

---

## 4. CPU Feasibility Analysis (< 10 ms Latency, AVX2 / AVX-512)

### Target Specifications:
* **Audio Sampling Rate ($f_s$)**: 48,000 Hz.
* **Hop Size ($H$)**: 10 ms = **480 samples**.
* **Max Execution Time Budget per Hop**: **< 10 ms** (to prevent buffer underrun; practically **< 5–6 ms** for safety headroom, i.e., Real-Time Factor $\text{RTF} \le 0.5$).

### A. FLOPs Budget & Scaling to 48 kHz
In the original 16 kHz configuration:
* Original TF-GridNet: **~128 – 320 GMACs/s** = **256 – 640 GFLOP/s** (1 MAC = 2 FLOPs).
* At 16 kHz with an 8 ms hop (125 hops/s): each hop requires $\frac{320\text{ GMACs}}{125} = \mathbf{2.56\text{ GMACs / hop}}$ (**5.12 GFLOPs / hop**).

When operating at **48 kHz** with STFT:
* To maintain equivalent spectral resolution, $N_{\text{FFT}}$ must triple: $F = 257 \to \mathbf{769\text{ bins}}$ (or 513 bins for 1024-point FFT).
* In TF-GridNet, the intra-frame spectral BLSTMs and unfold operations scale **linearly to quadratically** with $F$.
* Computational demand at 48 kHz jumps by **$2.5\times - 3\times$**, scaling baseline TF-GridNet to **> 600 – 900 GMACs/s** (**1.2 – 1.8 TFLOP/s**).

### B. Hardware Capabilities: Modern x86_64 CPU (AVX2 vs AVX-512)
* **Single Core AVX2 (e.g. Intel Core i7 / AMD Zen 3/4 @ 4.0 GHz)**:
  * 2 FMA units $\times$ 8 FP32 lanes $\times$ 4.0 GHz = **64 GFLOP/s peak theoretical**.
  * Sustained GEMM efficiency in optimized inference engines (ONNX Runtime, OpenVINO, LibTorch): ~**60–70% of peak** $\approx$ **40 GFLOP/s sustained**.
  * Within a **10 ms window**, a single AVX2 core can deliver:
    $$\text{Max compute per 10 ms} = 40\text{ GFLOP/s} \times 0.01\text{ s} = \mathbf{0.40\text{ GFLOPs}} \quad (\mathbf{0.20\text{ GMACs}}).$$
* **Single Core AVX-512 / AVX10 (e.g. Intel Xeon / Skylake-X / Zen 4 @ 4.0 GHz)**:
  * 2 FMA units $\times$ 16 FP32 lanes $\times$ 4.0 GHz = **128 GFLOP/s peak**.
  * Sustained FP32 GEMM: ~**80 GFLOP/s sustained** $\implies \mathbf{0.80\text{ GFLOPs per 10 ms}}$ (**0.40 GMACs**).
* **Multi-core (4 threads parallelized across sub-bands)**:
  * Sustained: ~**1.5 – 2.5 GFLOPs per 10 ms window**.

### C. Gap Analysis & Feasibility Verdict

$$\frac{\text{Required Compute per 10 ms (Causal TF-GridNet)}}{\text{Available Single-Core AVX2 Compute in 10 ms}} = \frac{\approx 3.0\text{ to }5.0\text{ GFLOPs}}{0.4\text{ GFLOPs}} \approx \mathbf{7.5\times - 12.5\times\text{ OVER BUDGET}}.$$

* Even empirical measurements on Xeon 2.2 GHz (Heitkaemper et al., 2024; S4M 2023) show:
  * TF-GridNet single-core CPU-RTF is **86.4** (meaning 1 second of audio takes **86.4 seconds** to process on CPU).
  * TIGER (which cut MACs by 95% down to 15 GMACs/s) still required **765 ms CPU time** for 1 second of audio ($\text{RTF} \approx 0.76$).
* **Sequential RNN bottleneck**: In addition to pure FLOP counts, intra-frame BLSTMs require sequential step evaluation across 769 frequency bins. This cannot be effectively vectorized with wide AVX-512 SIMD registers due to low loop trip counts and pointer chasing, creating massive instruction cache stalls.

### Verdict on Pure TF-GridNet:
**NO.** Neither original TF-GridNet nor direct causal adaptations (TF-GridNet-Causal) can run on an x86_64 CPU within a 10 ms latency budget at 48 kHz. They exceed real-time CPU capacity by at least **an order of magnitude**.

### Feasible Alternatives on CPU (< 10 ms at 48 kHz):
1. **Sub-band / Band-Split Hybrids (e.g., DeepFilterNet3 / BSRNN-tiny / TIGER-tiny)**:
   * Constrain neural inference to equivalent ERB bands (e.g., 32–64 frequency bands instead of 769 linear bins).
   * Budget: **< 1.0 – 2.5 GMACs/s** ($\le 0.02$ GMACs / 10 ms hop), comfortably running at RTF $\approx 0.15$ on AVX2.
2. **TF-MLPNet / Feed-Forward Architectures**:
   * Replace recurrent frequency traversal with static linear projections (MLP-Mixer).
   * Quantize to INT8 via VNNI / AVX-512_VNNI instructions.

---

## 5. Key Citation List

1. **Original TF-GridNet (TASLP 2023 / ICASSP 2023)**:
   * Z.-Q. Wang, S. Cornell, S. Choi, Y. Lee, B.-Y. Kim, and S. Watanabe, *"TF-GridNet: Integrating Full- and Sub-Band Modeling for Speech Separation,"* **IEEE/ACM Transactions on Audio, Speech, and Language Processing (TASLP)**, vol. 31, pp. 3221–3236, Aug 2023. DOI: [10.1109/TASLP.2023.3304482](https://doi.org/10.1109/TASLP.2023.3304482).
2. **Causal Target Speaker Extraction Baseline (CHiME-9 2026)**:
   * S. Sharma, et al., *"Perceptually-Motivated Low-Latency Multi-Channel Target Speaker Extraction for Hearing Aids and Smart Glasses,"* **Proc. CHiME-9 Workshop (ICASSP 2026 satellite)**, 2026. (Evaluates official causal multi-channel TF-GridNet baseline with 20 ms algorithmic latency).
3. **Hardware-Efficient Time-Frequency Architecture (TF-MLPNet 2025)**:
   * M. Itani, et al., *"TF-MLPNet: Tiny Real-Time Neural Speech Separation,"* **Proc. Interspeech / Clarity Workshop 2025**, arXiv:2508.03047, 2025. (Replaces BLSTMs with MLP-Mixer to achieve 3.6 ms runtime on 6 ms chunks).
4. **Causal Attention & Cache Memory Adaptation (TFACM 2025)**:
   * Y. Zhang, et al., *"A Causal Speech Separation Model Combining Time-Frequency Attention and Cache Memory,"* **IEEE Signal Processing Letters / arXiv:2505.13094**, May 2025. (Directly evaluates TF-GridNet-Causal vs cached causal variants).
5. **Target Speaker Extraction with TF-GridNet Backbone (X-TF-GridNet 2024)**:
   * F. Hao, X. Li, and C. Zheng, *"X-TF-GridNet: A Time–Frequency Domain Target Speaker Extraction Network with Adaptive Speaker Embedding Fusion,"* **Information Fusion**, vol. 112, p. 102550, 2024. DOI: [10.1016/j.inffus.2024.102550](https://doi.org/10.1016/j.inffus.2024.102550).
6. **Efficiency & Computational Mismatch Analysis (TF-MoE 2026 / TIGER ICLR 2025)**:
   * H. Xu et al., *"TIGER: Time-Frequency Interleaved Gain Extraction and Reconstruction Network,"* **ICLR 2025**, 2025.
   * J. Kim et al., *"TF-MoE: Time-Frequency Mixture-of-Experts for Efficient Speech Separation,"* **arXiv:2606.29575**, 2026.
