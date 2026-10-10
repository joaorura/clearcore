# Benchmarks, Metrics, and CPU Computational Budgets for Streaming Target Speaker Extraction (TSE) and Speech Separation (2023–2026)

## 1. Executive Summary & Objective

This document consolidates comprehensive quantitative benchmark data for **streaming and causal Target Speaker Extraction (TSE)** and **speech separation** models from the 2023–2026 literature (ICASSP, Interspeech, IEEE/ACM TASLP, and preprints).

### Target Operational Context
- **Sampling Rate & Domain:** 48 kHz native or 16 kHz with zero-phase / low-latency multi-rate resampling.
- **Hop Size:** 10 ms frames (480 samples at 48 kHz; 160 samples at 16 kHz).
- **Algorithmic Latency & Lookahead:** Strict causal / streaming constraint: $\le 20\text{ ms}$ algorithmic lookahead (target $0\text{ ms}$ lookahead, $10\text{ ms}$ algorithmic delay from frame hop).
- **Target Execution Hardware:** Single-core commodity x86_64 CPU (AVX2 / FMA3 / AVX-512 / AVX-VNNI) running within strict real-time deadlines ($\text{RTF} < 0.3$ under paced audio callback conditions, leaving $\ge 70\%$ headroom for host DSP and OS scheduling).

---

## 2. Comparative Benchmark Table: Streaming TSE & Speech Separation (2023–2026)

The table below contrasts causal and streaming target speaker extraction / separation architectures. Unless explicitly marked otherwise, all evaluations report **causal / streaming** modes with lookahead $\le 20\text{ ms}$.

| Architecture / Model | Venue / Year | Domain / Frame Config | Lookahead / Alg. Latency | Params (M) | MACs / GFLOPs (per sec) | RTF (CPU) | Libri2Mix Clean SI-SDRi (dB) | Libri2Mix Noisy SI-SDRi (dB) | WSJ0-2mix SI-SDRi (dB) | PESQ (WB) | STOI / ESTOI (%) |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **SCI-LFC** (Liang et al.) | SSRN 2026 | T-F / Causal STFT | 0 ms / 16 ms | 3.2M | 14.8 GMAC/s | ~0.24 (x86) | — | — | **14.77** | 3.12 | 87.2% |
| **SpeakerBeam-SS** (Sato et al.) | Interspeech 2024 | Time / S4D + Conv | 0 ms / 20 ms | 4.8M | 12.1 GMAC/s | **0.18** (EPYC) | — | — | 13.90 | 3.05 | — |
| **penta2himajin TSE** (Prod 48k) | HuggingFace 2026 | Time / Causal TCN | 0 ms / 10 ms | 1.4M | 4.8 GMAC/s | **0.12** (x86_64) | — | 8.55 (VCTK+DEM) | — | — | — |
| **TFACM (Large)** (Li et al.) | ICASSP 2025 | T-F / Causal DualPath | 0 ms / 16 ms | 1.0M | 36.5 GMAC/s | ~0.57 (Xeon) | 11.0 | 9.3 | 13.0 | — | — |
| **TFACM (Small)** (Li et al.) | ICASSP 2025 | T-F / Causal DualPath | 0 ms / 16 ms | 0.5M | 19.4 GMAC/s | ~0.45 (Xeon) | 9.3 | 9.0 | 10.9 | — | — |
| **TF-GridNet-Causal** (Baseline) | TASLP 2023 / 2025 | T-F / Causal BLSTM | 0 ms / 16 ms | 11.4M | 179.2 GMAC/s | >1.00 (Xeon) | 10.8 | 9.2 | 13.5 | — | — |
| **FastEnhancer-T** (Kim et al.) | arXiv 2025 / 2026 | T-F / RNNFormer | 0 ms / 10 ms | 0.22M | 5.5 GMAC/s | **0.012** (Xeon) | — | — | — | 3.34 | 94.0% |
| **FastEnhancer-M** (Kim et al.) | arXiv 2025 / 2026 | T-F / RNNFormer | 0 ms / 10 ms | 4.9M | 29.0 GMAC/s | **0.101** (Xeon) | — | — | — | 3.39 | 95.0% |
| **SkiM (Stride 10)** (Li et al.) | ICASSP / Interspeech | Time / Skipping LSTM | 0 ms / 0.625 ms | 6.0M | 3.9 GMAC/s | **0.46** (Intel) | — | — | 17.1 | — | 74.5% |
| **Causal Conv-TasNet** | Baseline Reference | Time / Causal TCN | 0 ms / 1.0 ms | 5.1M | 4.6 GMAC/s | 0.63 (Xeon) | 9.1 | 7.8 | 11.2 | 2.65 | 78.0% |
| **SpEx+ (Tied)** (Ge et al.) | Interspeech / TASLP | Time / Multi-scale TCN | 0 ms / 2.5 ms | 11.1M | 8.9 GMAC/s | ~0.72 (x86) | — | — | 17.4 | 3.49 | 83.0% |
| **DualSep-S** (Wang et al.) | Interspeech 2024 | T-F / Dual Encoder | 0 ms / 16 ms | 0.83M | 6.2 GMAC/s | **0.39** (Core i7) | — | — | 12.08 | 3.35 | 91.5% |
| *MeanFlow-TSE* (1-step Generative) | Interspeech 2026 | T-F / UDiT (Non-streaming) | Utterance (Offline) | 359M | >300 GMAC/s | 0.018 (GPU) | **18.80** | **12.85** | — | **3.26** | **93.0%** |
| *AlphaFlowTSE* (1-step Generative) | alphaXiv 2026 | T-F / AlphaFlow (Offline) | Utterance (Offline) | 360M | >300 GMAC/s | 0.019 (GPU) | **19.17** | **13.16** | — | **3.27** | — |

*Note on Generative 1-step TSE (MeanFlow-TSE, AlphaFlowTSE):* Included for upper-bound reference. While achieving single-step inference (NFE=1) and low RTF on high-end GPUs (RTF ~0.018 on NVIDIA L40), their 350M+ parameter footprints and full-utterance cross-attention prevent single-core CPU streaming deployment.

---

## 3. Notable Streaming TSE Architectures (Detailed Analysis)

### 3.1. SCI-LFC (Lightweight Causal TSE via Staged Condition Injection & Learnable Frequency Compression)
- **Authors & Citation:** Ruiyu Liang, Yu Xu, Minjie Xiang, Jiaming Cheng, Björn Schuller, Rui Liu, Xiaoshuai Hao (SSRN Preprint, Sept 2026).
- **Core Innovation:**
  1. *Staged Condition Injection (SCI):* Instead of injecting the target speaker embedding globally from the input layer (which corrupts early representation learning), SCI injects the condition only into the last 3 separator stages. The early layers learn general mixture acoustic representations.
  2. *Learnable Frequency Compression (LFC):* Compresses the frequency dimension through learnable weighted aggregation of adjacent frequency bins, preserving harmonic structure while reducing dual-path complexity by ~75%.
  3. *Simplified Temporal Gating (STG):* Dynamically compensates for inter-frame temporal loss caused by causal masking.
- **Complexity & Metrics:** 3.2M parameters, 14.8 GMAC/s, achieving **14.77 dB SI-SNRi** on WSJ0-2mix and **10.75 dB** on Aishell-2mix under strict causality.

### 3.2. SpeakerBeam-SS (State Space Modeling with S4D for Real-Time TSE)
- **Authors & Citation:** T. Sato, M. Delcroix, T. Moriya, T. Ochiai, K. Matsuura (Interspeech 2024, pp. 2940–2944).
- **Core Innovation:**
  - Replaces dilated 1D convolutional layers in Conv-TasNet with Diagonal Structured State Space Sequence (S4D) layers.
  - S4D maintains an infinite effective receptive field causally via state recurrence with $O(1)$ inference step complexity per audio frame.
  - Employs an over-parameterized frontend encoder with larger frame stride ($L=320$ samples = 20 ms), reducing the number of frames processed per second and dropping RTF by 78% on CPU.
- **Measured CPU RTF:** **0.18** on a single core AMD EPYC 7502P (using native C++ inference).

### 3.3. penta2himajin / mellonella TSE (Native 48 kHz Streaming Causal TSE)
- **Provenance:** HuggingFace `penta2himajin/tse-conv-tasnet-48k` (2024–2026); inference engine in `mellonella-core` (Rust).
- **Core Innovation:**
  - Designed specifically for native 48 kHz VoIP/audio pipelines without sample rate conversion.
  - Fixed chunk processing: 480 samples per chunk (10 ms at 48 kHz).
  - Tripled encoder stride ($N=256$, kernel 96, stride 48) to maintain a 1 kHz internal frame rate, ensuring separator computation matches standard 16 kHz models.
  - Explicit externalization of 89 causal conv state buffers passed between chunk calls, eliminating graph internal reallocation.
- **Conditioning:** Frozen 192-dim ECAPA-TDNN target speaker embedding modulated via a 2-layer MLP into FiLM $(\gamma, \beta)$ parameters.
- **Complexity:** 1.4M parameters, 5.3 MB ONNX weights, ~4.8 GMAC/s, RTF ~0.12 on single x86_64 CPU core.

### 3.4. TFACM (Time-Frequency Attention Cache Memory for Causal Speech Separation)
- **Authors & Citation:** C. Li, F. Grondin et al. (ICASSP 2024 / IEEE 2025).
- **Core Innovation:**
  - Directly addresses the computational bottleneck of TF-GridNet-Causal (11.4M params, 179 GMAC/s).
  - Combines an LSTM along the frequency dimension with causal intra-chunk temporal modeling.
  - Uses an explicit Cache Memory (CM) module that caches hidden/cell states across temporal chunks.
  - Causal Attention Refinement (CAR) module applies diagonally masked cross-attention.
- **Efficiency:** Achieves 13.0 dB SI-SNRi on WHAM! with only **8.8% of the parameters (1.0M)** and **20.4% of the compute (36.5 GMAC/s)** of TF-GridNet-Causal.

---

## 4. CPU Computational Budget Analysis for $<10\text{ ms}$ Inference

### 4.1. Hardware Architectural Ceilings (x86_64 AVX2 / AVX-512 / AVX-VNNI)
For modern commodity x86_64 CPUs (Intel Core 12th–14th Gen / AMD Zen 3/4/5) running at typical single-core sustained clocks of 3.2 GHz – 4.0 GHz:

1. **AVX2 + FMA3 (256-bit SIMD):**
   - Two 256-bit FMA execution units per cycle.
   - Vector width: 8 single-precision floats (`f32`).
   - Peak Arithmetic Throughput: $2 \times 8 \times 2 = 32\text{ FLOP/cycle} = 16\text{ MAC/cycle}$.
   - At 3.5 GHz single-core clock: Peak FP32 = **112 GFLOP/s (56 GMAC/s)**.
   - Realistic GEMM / Conv efficiency in optimized C/C++ runtimes: **60%–75% of peak** $\implies \mathbf{33.6\text{ to }42.0\text{ GMAC/s}}$.
   
2. **AVX-512 / AVX-VNNI (Int8 Quantized Acceleration):**
   - VPDPBUSD instructions execute 64 to 128 int8 MACs per cycle per core.
   - Peak Int8 Throughput at 3.5 GHz: **220–440 GMAC/s**.
   - Realistic quantized efficiency: $\sim 180\text{ GMAC/s}$.

### 4.2. Paced Real-Time Audio Callback Budget (The "Pacing vs. Racing" Reality)
As established in *faster-enhancer.c* (Kim et al., 2026):
- In offline benchmarking, the CPU "races" at peak turbo frequency with warmed caches.
- In a live audio callback running every 10 ms (480 samples at 48 kHz), the audio thread sleeps for $\sim 7\text{ ms}$ and wakes up periodically.
- OS CPU governors throttle clock frequency down during the idle window.
- **Pacing Penalty:** Real-time callbacks exhibit an effective latency penalty of **$2.5\times \text{ to } 4.2\times$** compared to saturated racing benchmarks due to clock down-ramping and cold L1/L2 cache lines.

### 4.3. The Hard 10 ms Single-Core Budget Limits
To guarantee **zero audio dropouts (xruns)** without pegging the CPU or inducing thermal throttling:
- Target audio thread processing deadline: **$\le 3.0\text{ ms}$ execution per 10 ms audio frame** ($\text{Duty Cycle} \le 30\%$, corresponding to $\text{RTF} \le 0.30$).
- At 3.0 ms execution window on a 3.5 GHz core delivering 35 GMAC/s effective sustained throughput:
  $$\text{Budget per 10 ms frame} = 35\text{ GMAC/s} \times 0.003\text{ s} = \mathbf{105\text{ MMAC per frame}}.$$
- Scaled to a continuous 1-second stream (100 frames/sec):
  $$\mathbf{\text{Maximum Continuous Budget}} \le \mathbf{10.5\text{ GMAC/s (or } \sim 21.0\text{ GFLOP/s)} }.$$

### 4.4. Model Feasibility Tiers on Commodity x86_64 CPU

| Model Category | GMAC/s Range | 10 ms Frame Execution Time (Single Core) | Feasibility Verdict |
| :--- | :--- | :--- | :--- |
| **Heavy Models** (TF-GridNet, SepFormer, BSRNN) | 80 – 200+ GMAC/s | 15 – 35 ms (Exceeds 10 ms window) | **Infeasible on CPU.** Causes immediate audio buffer underruns / dropouts. |
| **Medium Models** (TFACM-Large, DPTNet-Causal) | 25 – 45 GMAC/s | 4.5 – 8.0 ms (Fits window, but high load) | **Marginal.** RTF 0.5–0.8. Vulnerable to OS jitter; high power/thermal draw. |
| **Lightweight Causal Models** (SCI-LFC, SpeakerBeam-SS) | 10 – 16 GMAC/s | 2.0 – 3.2 ms | **Feasible & Safe.** RTF 0.20–0.32. Suitable for production background daemons. |
| **Ultra-Lightweight Models** (penta2himajin, FastEnhancer-T/B) | 3 – 8 GMAC/s | 0.6 – 1.6 ms | **Optimal.** RTF < 0.15. Leaves >85% CPU headroom for OS, UI, and audio routing. |

---

## 5. Architectural Recommendations for 48 kHz Streaming TSE

1. **Hybrid Sub-Band / Multirate Approach:**
   - Raw time-domain convolutions at 48 kHz with small strides ($L \le 32$ samples) incur extreme frame rates ($>1500\text{ frames/sec}$), exhausting the CPU.
   - Recommended: Use a causal STFT with 10 ms hop (100 frames/sec) or a causal 1D learned encoder with stride 48 (1000 frames/sec) and channel dimension $D \le 128$.
2. **Eliminate Full-Matrix Cross-Frame Attention:**
   - Global multi-head self-attention over historical frames requires dynamically expanding KV-caches, causing allocation overhead and quadratic scaling.
   - Recommended: Replace temporal attention with **S4D (State Space Models)** or **Causal Depthwise Dilated TCNs** with fixed state buffers.
3. **Conditioning Injection Point:**
   - Following the SCI-LFC paradigm, decouple early acoustic feature extraction from speaker modulation. Inject target speaker embeddings (from pre-computed 192-dim ECAPA-TDNN vectors) into the intermediate/late layers using FiLM $(\gamma, \beta)$ modulation.
4. **Zero Allocation Contract (`no_alloc`):**
   - Pre-allocate all state tensors (recurrent hidden states, causal conv delay lines) during engine initialization. Pass explicit in/out state tensors across audio callback invocations.

---

## 6. Citations & Primary Literature References

1. **SCI-LFC (2026):** R. Liang, Y. Xu, M. Xiang, J. Cheng, B. Schuller, R. Liu, X. Hao, *"Lightweight Causal Target Speaker Extraction via Staged Condition Injection and Learnable Frequency Compression,"* SSRN Preprint, Sept 2026. DOI: 10.2139/ssrn.7478491.
2. **SpeakerBeam-SS (2024):** T. Sato, M. Delcroix, T. Moriya, T. Ochiai, K. Matsuura, *"SpeakerBeam-SS: Real-time Target Speaker Extraction with Lightweight Conv-TasNet and State Space Modeling,"* Interspeech 2024, pp. 2940–2944.
3. **TFACM (2024/2025):** C. Li, F. Grondin, *"Time-Frequency Attention Cache Memory for Causal Speech Separation,"* IEEE ICASSP 2024 / arXiv:2505.13094.
4. **TF-GridNet (2023):** Z.-Q. Wang, S. Cornell, S. Choi, Y. Lee, B.-Y. Kim, S. Watanabe, *"TF-GridNet: Integrating Full- and Sub-Band Modeling for Speech Separation,"* IEEE/ACM Transactions on Audio, Speech, and Language Processing (TASLP), vol. 31, pp. 3221–3236, 2023.
5. **faster-enhancer.c (2026):** G. Kim, *"faster-enhancer.c: A Dependency-Free int8 Runtime for Streaming Speech Enhancement on Commodity CPUs,"* arXiv:2607.25350, 2026.
6. **FastEnhancer (2025):** S. Kim et al., *"FastEnhancer: Speed-Optimized Streaming Neural Speech Enhancement,"* arXiv:2509.21867, 2025.
7. **DualSep (2024):** Z. Wang, J. Sun, Z. Zhang, X. Li, J. Liu, L. Xie, *"DualSep: A Light-weight dual-encoder convolutional recurrent network for real-time in-car speech separation,"* Interspeech 2024.
8. **SkiM (2022):** C. Li, L. Yang, W. Wang, Y. Qian, *"SkiM: Skipping Memory LSTM for Low-Latency Real-Time Continuous Speech Separation,"* ICASSP 2022.
9. **SpEx+ (2020):** M. Ge, C. Xu, L. Wang, E. S. Chng, J. Dang, H. Li, *"SpEx+: A Complete Time-Domain Speaker Extraction Network,"* Interspeech 2020 / IEEE/ACM TASLP 2021.
10. **MeanFlow-TSE (2026):** R. Shimizu, X. Jiang, N. Mesgarani, *"MeanFlow-TSE: One-Step Generative Target Speaker Extraction with Mean Flow,"* Interspeech 2026 / arXiv:2512.18572.
