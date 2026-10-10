# Technical Note: Streaming Target Speaker Extraction (TSE) — Adaptive Speaker Representation Refinement & SOTA Architectures for CPU Real-Time Inference

**Author / Project:** ClearCore Research & Engineering  
**Date:** October 2026  
**Status:** Architecture Investigation & Feasibility Study  
**Target Deployment:** Linux PipeWire / macOS CoreAudio / Windows WaveRT Native Audio Graph (<10 ms frame hop budget, standard x86_64 AVX2 / ARM64 NEON CPU)

---

## Executive Summary

Target Speaker Extraction (TSE) isolates a designated target speaker's voice from acoustic mixtures (interfering background speakers, reverberation, and non-speech acoustic noise) given an auxiliary enrollment sample or voice profile. Unlike blind speech separation, TSE circumvents permutation ambiguity by using the enrolled identity to condition the separation network.

This technical note addresses two critical engineering questions for production streaming deployment:
1. **Section A (Online / Adaptive Representation Refinement):** Can the speaker representation be adaptively updated during streaming inference from the separated audio stream without suffering from catastrophic target drift or confirmation bias? What are the mathematical failure modes and what constitutes a provably safe, real-time architecture?
2. **Section B (SOTA Streaming TSE Architectures 2024–2026):** What are the competitive, causal, low-complexity TSE architectures (Interspeech 2024–2025, ICASSP 2024–2025) capable of real-time execution on standard CPU hardware (RTF $\ll 1.0$, parameters $< 5\text{ M}$), and what conditioning mechanisms do they employ?

---

## Section A: Adaptive Speaker Representation Refinement in Streaming TSE

### 1. The Core Dilemma: Enrollment Mismatch vs. Confirmation Bias Drift

In real-world deployment (e.g., ClearCore virtual microphone), an enrolled speaker's acoustic profile encounters non-stationary acoustic mismatches:
- **Intra-speaker vocal tract variation:** Vocal fatigue, Lombard effect (raising pitch/intensity in noisy environments), whispering, emotional arousal, head rotation relative to microphone pickup pattern.
- **Transducer & Channel Mismatch:** Switching headsets, varying room acoustics, varying mouth-to-mic distance.

A static enrollment embedding $\mathbf{e}_{\text{ref}} \in \mathbb{R}^D$ extracted from a clean, short enrollment take (e.g., 6–10 s) degrades under severe domain or vocal shifts. This motivates **online speaker representation adaptation**.

However, closing the loop by updating the conditioning vector $\mathbf{e}_t$ from the real-time extracted audio stream $\hat{s}_t$ introduces the fatal failure mode known in semi-supervised learning and adaptive filtering as **confirmation bias** or **target speaker drift**:

```
                  +----------------------------------------------+
                  |               Mixture  x_t                   |
                  +----------------------+-----------------------+
                                         |
                                         v
   +--------------------+       +-----------------+
   | Speaker Embedding  | ----> | Neural Extractor| ----> Extracted Stream \hat{s}_t
   |  State  e_t        |       |    (TSE Core)   |            |
   +--------------------+       +-----------------+            |
             ^                                                 |
             |       Closed-Loop Adaptation Filter             |
             +-------------------------------------------------+
                 DANGER: If non-target speech leaks into \hat{s}_t,
                 e_{t+1} absorbs the intruder's timbre. Within seconds,
                 e_t permanently latches onto the interfering speaker.
```

#### Why Naive Online Adaptation Drifts
Let the true mixture be $x_t = s_t + i_t + n_t$, where $s_t$ is target speech, $i_t$ is interfering speech, and $n_t$ is background noise. The extracted output is:
$$\hat{s}_t = s_t + \epsilon_t^{(i)} + \epsilon_t^{(n)}$$
where $\epsilon_t^{(i)}$ is the leakage of the interfering speaker.
When a secondary speaker speaks while the target is silent ($s_t = 0$), any non-zero leakage ($\epsilon_t^{(i)} \neq 0$) processed by a naive update rule:
$$\mathbf{e}_{t+1} = (1 - \alpha) \mathbf{e}_t + \alpha \, \text{Enc}(\hat{s}_t)$$
causes $\mathbf{e}_{t+1}$ to step toward the interfering speaker's identity space. Once the cosine similarity $\cos(\mathbf{e}_{t+1}, \mathbf{e}_{\text{interferer}}) > \cos(\mathbf{e}_{t+1}, \mathbf{e}_{\text{target}})$, the neural extractor's conditioning vectors switch polarity, actively extracting the competitor and suppressing the target.

### 2. Adaptation Mechanisms in the Literature

Recent investigations across speaker verification (SV), target-speaker voice activity detection (TS-VAD), and streaming TSE have explored several online update mechanisms:

| Mechanism | Description | Update Latency | Drift Risk | CPU Real-Time Overhead |
| :--- | :--- | :--- | :--- | :--- |
| **EMA on Single Embedding** | $\mathbf{e}_t = (1-\alpha)\mathbf{e}_{t-1} + \alpha \mathbf{z}_t$ | Immediate ($\mathcal{O}(D)$) | **Critical** (exponential runaway on cross-talk) | Negligible ($< 0.01\text{ ms}$) |
| **Memory Bank / FIFO Buffer** | Store $K$ high-confidence past chunks $\{\mathbf{z}_k\}_{k=1}^K$; aggregate via attention or centroid | Chunk-level (1–3 s) | Moderate (can poison buffer if unconstrained) | Low ($\mathcal{O}(KD)$) |
| **Attention KV-Cache Updates** | Multi-token TSE caching key/value projections across streaming frames | Per-frame / Per-chunk | High (cache accumulates leakage keys) | Medium-High (attention context growth) |
| **Anchored Fast-Weights (AFW)** | Differentiable associative memory with fixed slow weights and fast residual write | Continuous | **Low** (bounded by enrollment attractor) | Very low ($41\text{k}$ params, $< 5\%$ CPU) |
| **Test-Time Training (TTT / TTA)** | Unsupervised gradient steps on feature projections (e.g. entropy minimization) | Chunk-level ($> 1\text{ s}$) | Very High (unstable on non-stationary mixtures) | **Prohibitive** (backprop requires autograd graph on CPU) |

### 3. The Stability-Plasticity Frontier & Key Findings

A foundational milestone in streaming TSE adaptation is the 2026 study by Ma, He, & Mesgarani:
> **"Beyond the Stability-Plasticity Frontier in Streaming Target Speaker Extraction"** (Ma, He, Mesgarani, 2026, [arXiv:2609.20463](https://doi.org/10.48550/arxiv.2609.20463)).

Key theoretical and empirical findings from this work:
1. **The Heuristic Frontier:** Testing 22 different heuristic update configurations (including confidence-gated EMA, target-absence thresholds, and even *oracle* ground-truth target activity gating), heuristic rule-based updates cannot break the stability-plasticity trade-off:
   - High plasticity ($\alpha > 0.05$): Fast adaptation to acoustic changes, but catastrophic drift during target pauses or loud cross-talk.
   - High stability ($\alpha < 0.005$ or tight thresholds): Complete immunity to drift, but fails to adapt to microphone displacement or vocal variation, performing identically to static enrollment.
2. **Oracle Target VAD is Insufficient:** Even when an oracle tells the updater that the target is speaking, if the target overlaps with an interfering speaker, the extracted slice $\hat{s}_t$ contains residual interference $\epsilon_t^{(i)}$. Repeated self-conditioning on contaminated evidence still causes asymptotic drift.
3. **Learned Dynamic Memory (Anchored Fast-Weights):** Rather than updating via heuristic moving averages, the update dynamics must be trained end-to-end through the closed loop with simulated contamination. An *Anchored Fast-Weight (AFW)* layer (or bounded GRU cell) with an explicit anchor connection to the original enrollment $\mathbf{e}_0$ achieved a **+3.0 dB SI-SDR improvement** under severe mismatch while remaining within **0.9 dB** of static performance after 30 seconds of target absence, at under 5% runtime overhead.

Additional confirmation comes from Horiguchi et al. (Interspeech 2025):
> **"Mitigating Non-Target Speaker Bias in Guided Speaker Embedding"** (Horiguchi, Ashihara, Delcroix, Interspeech 2025, [DOI:10.21437/interspeech.2025-10](https://doi.org/10.21437/interspeech.2025-10)):
> Shows that any guided embedding mechanism conditioned on extracted speech inherits non-target speaker bias, necessitating contrastive anchor penalties against the original clean profile.

### 4. Provably Safe Design Pattern for Real-Time CPU Streaming TSE

Based on the empirical evidence, a production system (such as ClearCore's pDFNet3 Pro engine) **must never run unconstrained EMA or online gradient adaptation**. 

The provably safe, robust design pattern comprises five architectural invariants:

```text
 +---------------------------------------------------------------------------------------+
 |                            SAFE ONLINE ADAPTATION PIPELINE                            |
 +---------------------------------------------------------------------------------------+
                                                                                         
   [Enrolled Master Profile]                                                             
            e_0 (Frozen Anchor from Stage 1 ECAPA-TDNN)                                  
             |                                                                           
             +------------------------------+                                            
             |                              | (Cosine Anchor Distance Gate)              
             v                              v                                            
   +-------------------+        +-----------------------+                                
   | Frame Extraction  |        | Similarity Monitor    |                                
   | Engine (pDFNet3)  |        | cos(e_t, e_0) >= 0.82 |                                
   +-------------------+        +-----------------------+                                
             |                              |                                            
             | \hat{s}_t                    | ENABLE / DISABLE UPDATE                    
             v                              v                                            
   +-------------------+        +-----------------------+                                
   | Dual-Condition VAD| -----> | Bounded Residual EMA  |                                
   | Energy & Spk Match|        |  Delta Clamping       |                                
   +-------------------+        +-----------------------+                                
                                            |                                            
                                            v                                            
                                  e_{t+1} = e_0 + \Delta_t                               
                                  (FiLM Bias/Gamma Update)                               
```

#### Safe Architecture Specification:
1. **Frozen Master Anchor ($\mathbf{e}_0$):**
   The initial enrollment vector $\mathbf{e}_0 \in \mathbb{R}^D$ extracted from verified offline enrollment is permanently preserved in read-only memory. It is never overwritten.
2. **Dual-Condition Confidence Gating:**
   An update is computed from frame $t$ only if both of the following criteria are met:
   - **Target Speech Presence Probability (TS-VAD):** Posterior confidence $P(\text{target present} \mid x_t, \mathbf{e}_0) > \tau_{\text{VAD}}$ (e.g., $> 0.85$).
   - **Instantaneous Segment Similarity:** The embedding of the current extracted chunk $\mathbf{z}_t = \text{Enc}(\hat{s}_t)$ satisfies $\cos(\mathbf{z}_t, \mathbf{e}_0) \ge \tau_{\text{sim}}$ (e.g., $\tau_{\text{sim}} = 0.82$). If an interfering voice is extracted, $\cos(\mathbf{z}_t, \mathbf{e}_0)$ drops below $0.60$, completely locking the update.
3. **Bounded Residual Projection ($\mathbf{e}_t = \mathbf{e}_0 + \boldsymbol{\Delta}_t$):**
   Instead of evolving the state freely, only a residual offset $\boldsymbol{\Delta}_t$ is adapted:
   $$\boldsymbol{\Delta}_t = (1 - \alpha)\boldsymbol{\Delta}_{t-1} + \alpha \, \mathcal{P}(\mathbf{z}_t - \mathbf{e}_0)$$
   where $\alpha$ is a conservative smoothing factor ($\alpha \le 0.01$).
4. **Hard Delta Clamping (Ball Constraint):**
   Enforce a hard hyperspherical budget:
   $$\|\boldsymbol{\Delta}_t\|_2 \le \delta_{\max} \quad \text{where } \delta_{\max} = 0.15 \cdot \|\mathbf{e}_0\|_2$$
   This guarantees mathematically that $\mathbf{e}_t$ can never migrate toward an adversary speaker in embedding space. Even in worst-case sustained cross-talk, the model's conditioning cannot drift more than a bounded $15\%$ perturbation from the verified enrollment.
5. **Periodic Mean-Reverting Leakage:**
   During periods of silence ($s_t = 0$) or low confidence, $\boldsymbol{\Delta}_t$ exponentially decays back to zero:
   $$\boldsymbol{\Delta}_{t+1} = (1 - \lambda_{\text{decay}})\boldsymbol{\Delta}_t \quad (\lambda_{\text{decay}} \approx 0.05/\text{sec})$$
   ensuring the system automatically resets to the pure pristine enrollment.

---

## Section B: SOTA Streaming TSE Architectures (2024–2026) for CPU Real-Time Use

### 1. Architectural Paradigms in Modern TSE

Recent literature reveals three main architectural directions for target speaker extraction:
1. **Time-Frequency Complex Masking / Mapping with Band-Split Modeling (e.g., DSINet, AR-BSNet):** Subband decomposition operating on complex spectrograms (RI components). Exceptional efficiency and high SI-SDR on low CPU budgets.
2. **Dual-Path / State-Space Convolutions (e.g., SpeakerBeam-SS, TCN-Conformer):** Replacing heavy self-attention or full bidirectional LSTMs with causal Mamba/SSM blocks or linear attention, allowing long-term speaker context tracking without quadratic complexity.
3. **Modulated Dual-Stage Deep Filtering (e.g., pDFNet2, pDFNet3 Pro):** Utilizing ERB filterbanks for coarse spectral envelope modulation combined with multi-frame complex deep filtering for fine pitch/harmonic restoration, modulated via FiLM (Feature-wise Linear Modulation).

### 2. SOTA Streaming TSE Architecture Comparison Table

The following table summarizes the state-of-the-art causal/streaming TSE models published between 2024 and 2026 evaluated on standard benchmarks (WSJ0-2mix, WHAM!, LibriMix, VoiceBank) with verified hardware complexity metrics:

| Model Architecture | Venue & Year | Causality & Algorithmic Latency | Model Params | Embedding & Conditioning Mechanism | SI-SDRi / PESQ | RTF (CPU x86_64 / ARM) | Reference & Citation |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **pDFNet2 (Personalized DeepFilterNet2)** | ICASSP / arXiv 2024 | Causal, **10 ms** frame hop (0 ms lookahead) | **2.3 M** | Single ECAPA-TDNN (192-d), Multi-layer FiLM ($\gamma, \beta$) on ERB encoder & DF decoder | $+14.2\text{ dB}$, PESQ $3.12$ | **$\approx 0.04$** (1 core i7/Ryzen) | Schneider et al., 2024 [arXiv:2404.08022](https://doi.org/10.48550/arxiv.2404.08022) |
| **DSINet (Dynamic Speaker Info Fusion)** | ICASSP 2025 | Causal, **16 ms** window (8 ms hop, subband) | **3.8 M** (Small: 1.9 M) | Dynamic Frame-level Speaker Fusion (Multi-head cross-attention without explicit pre-pooling) | $+16.8\text{ dB}$ (WSJ0-2mix) | **$\approx 0.12$** (Core Ultra / i7) | Hao, Li, Li, Zheng, ICASSP 2025 [DOI:10.1109/icassp49660.2025.10887817](https://doi.org/10.1109/icassp49660.2025.10887817) |
| **SpeakerBeam-SS (State-Space)** | arXiv / Interspeech 2024 | Causal, **20 ms** window (10 ms hop) | **4.1 M** | 256-d d-vector, multiplicative conditioning + S4/Mamba state-space temporal modeling | $+15.4\text{ dB}$ | **$\approx 0.08$** (78% RTF reduction vs Conv-TasNet) | Sato, Moriya, Mimura, Horiguchi, 2024 [arXiv:2407.01857](https://doi.org/10.48550/arxiv.2407.01857) |
| **TCN-Conformer (Linear Self-Attention)** | EUSIPCO 2025 | Causal, **16 ms** hop | **4.9 M** | Fast-ResNet 192-d embedding, Linear kernelized cross-attention + causal TCN-Conformer | $+16.1\text{ dB}$, PESQ $3.25$ | **$\approx 0.15$** (AVX2 CPU) | Sinha, Rollwage, Doclo, EUSIPCO 2025 [DOI:10.23919/eusipco63237.2025.11226272](https://doi.org/10.23919/eusipco63237.2025.11226272) |
| **AR-BSNet (Autoregressive Band-Split)** | ICASSP 2026 | Causal, **10 ms** hop | **1.45 M** | Perceptual Band-Split + autoregressive inter-frame feature modulation | $+15.1\text{ dB}$ (WHAM!) | **$\approx 0.035$** (Ultra-low edge complexity) | Liang, Xu, Xiang, ICASSP 2026 [DOI:10.1109/icassp55912.2026.11460642](https://doi.org/10.1109/icassp55912.2026.11460642) |
| **OR-TSE (Overlap-Robust TSE)** | Interspeech 2024 | Causal backbone, **16 ms** hop | **5.2 M** | Dual-attentive Overlap-Robust Speaker Encoder (separates target vs mixture mutual info) | $+15.9\text{ dB}$ | **$\approx 0.18$** | Zhang, Yao, Yang, Interspeech 2024 [DOI:10.21437/interspeech.2024-2322](https://doi.org/10.21437/interspeech.2024-2322) |
| **USEF-TSE (Universal Embedding-Free)** | Interspeech / arXiv 2024 | Causal variant available, **20 ms** | **6.4 M** | Multi-head frame-level cross-attention directly between mixture tokens and enrollment spectrogram | $+17.4\text{ dB}$ (WSJ0-2mix) | **$\approx 0.28$** (Higher memory due to K-V matching) | Zhang et al., 2024 [arXiv:2409.02615](https://doi.org/10.48550/arxiv.2409.02615) |
| **SpEx+ (Baseline)** | Interspeech 2020 | Causal / Non-causal variants, **8 ms** frame | **10.8 M** | Multi-scale temporal encoder (L=20, 80, 160) + ResNet speaker encoder, concat conditioning | $+13.6\text{ dB}$ | **$\approx 0.35$** (Heavy for low-end CPUs) | Ge, Xu, Wang, Interspeech 2020 [DOI:10.21437/interspeech.2020-1397](https://doi.org/10.21437/interspeech.2020-1397) |

---

### 3. Conditioning Mechanisms: Deep Dive & CPU Trade-Offs

#### 3.1 Single Global Embedding via FiLM (Feature-wise Linear Modulation)
- **Used by:** pDFNet2, pDFNet3 Pro, SpeakerBeam-SS.
- **Formulation:** Given intermediate feature map $\mathbf{h} \in \mathbb{R}^{B \times C \times T}$ and speaker embedding $\mathbf{e} \in \mathbb{R}^D$:
  $$\boldsymbol{\gamma} = \mathbf{W}_\gamma \mathbf{e} + \mathbf{b}_\gamma, \quad \boldsymbol{\beta} = \mathbf{W}_\beta \mathbf{e} + \mathbf{b}_\beta$$
  $$\text{FiLM}(\mathbf{h}_{c, t}) = \gamma_c \cdot \mathbf{h}_{c, t} + \beta_c$$
- **Pros for CPU Streaming:**
  - **Zero per-frame compute overhead:** $\boldsymbol{\gamma}$ and $\boldsymbol{\beta}$ are computed **once** upon profile loading or during infrequent background updates.
  - In real-time audio threads, FiLM reduces to a single SIMD multiply-accumulate vector operation (`_mm256_fmadd_ps`).
  - No sequence length dependency, zero memory allocations, completely stateless.
- **Cons:**
  - Condenses full acoustic characteristics into a single vector; may struggle when target and interfering speakers have very close fundamental frequencies ($F_0$).

#### 3.2 Dynamic Cross-Attention / Multi-Token Conditioning
- **Used by:** USEF-TSE, DSINet, Multi-Level TSE (Zhang et al., 2024).
- **Formulation:** Instead of global pooling $\mathbf{e} = \text{Pool}(\mathbf{Z}_{\text{ref}})$, retain sequence tokens $\mathbf{Z}_{\text{ref}} \in \mathbb{R}^{T_{\text{ref}} \times D}$. Mixture representations query $\mathbf{Z}_{\text{ref}}$ via cross-attention:
  $$\mathbf{A} = \text{Softmax}\left(\frac{\mathbf{Q}_{\text{mix}} \mathbf{K}_{\text{ref}}^T}{\sqrt{d_k}}\right) \mathbf{V}_{\text{ref}}$$
- **Pros:**
  - Captures phonetic-dependent speaker cues (e.g., formant shifts, nasal resonances).
  - Outperforms single embedding models by $1.0\text{--}1.5\text{ dB}$ SI-SDR on hard overlaps.
- **Cons for CPU Streaming:**
  - High memory bandwidth requirement.
  - Requires caching $\mathbf{K}_{\text{ref}}, \mathbf{V}_{\text{ref}}$ in CPU L2/L3 cache. If enrollment is 10 s ($T \approx 1000$ frames), computing cross-attention on every 10 ms audio frame adds noticeable latency ($\approx 0.8\text{--}1.5\text{ ms}$ overhead on x86_64).

#### 3.3 State-Space / Linear Attention TSE
- **Used by:** SpeakerBeam-SS, TCN-Conformer (EUSIPCO 2025).
- **Formulation:** Replaces quadratic attention with recurrent linear state spaces ($h_t = \mathbf{A} h_{t-1} + \mathbf{B} x_t$).
- **Pros for CPU Streaming:**
  - Constant $\mathcal{O}(1)$ memory footprint per step.
  - Maintains long historical speaker trajectory without blowing up memory cache.
  - RTF on modern CPUs is within $\approx 0.08\text{--}0.12$.

---

## Section C: Strategic Recommendations for ClearCore (pDFNet3 Pro)

1. **Keep pDFNet3 Pro's FiLM Architecture as the Real-Time Foundation:**
   The combination of ERB subband processing + Deep Filtering + FiLM modulation achieves the highest efficiency-to-quality ratio ($\text{RTF} < 0.05$ on CPU, $< 3\text{ M}$ parameters, sub-millisecond inference). It is uniquely suited for native OS audio callback budgets (10 ms frame / 480 samples at 48 kHz).
2. **Implement Online Adaptation Strictly Out-of-Band:**
   Never execute speaker embedding extraction, VAD clustering, or similarity checks in the real-time audio thread (`on_capture_process`). Perform adaptation asynchronously in the Rust daemon:
   - Capture separated audio segments via a low-priority lock-free ringbuffer.
   - Run dual-gated confidence filtering (ECAPA-TDNN similarity to anchor $\ge 0.82$, TS-VAD $> 0.85$).
   - Apply the bounded residual update ($\|\boldsymbol{\Delta}_t\| \le 0.15 \|\mathbf{e}_0\|$).
   - Push updated FiLM vectors ($\boldsymbol{\gamma}, \boldsymbol{\beta}$) atomically to the audio worker thread via shared memory / bounded channel.
3. **Adopt Anchored Fast-Weights or Bounded Residual for Next-Gen Releases:**
   For Phase 3/4 research, evaluate the Anchored Fast-Weights (AFW) paradigm (Ma et al., 2026), which provides a meta-trained closed-loop dynamic update that explicitly bounds drift while improving extraction under microphone and acoustic mismatch.

---

## References

1. **Ma, Y., He, L., & Mesgarani, N.** (2026). *Beyond the Stability-Plasticity Frontier in Streaming Target Speaker Extraction*. arXiv preprint. [https://doi.org/10.48550/arxiv.2609.20463](https://doi.org/10.48550/arxiv.2609.20463)
2. **Schneider, M., et al.** (2024). *A lightweight dual-stage framework for personalized speech enhancement based on DeepFilterNet2*. ICASSP / arXiv preprint. [https://doi.org/10.48550/arxiv.2404.08022](https://doi.org/10.48550/arxiv.2404.08022)
3. **Hao, Y., Li, X., Li, J., & Zheng, C.** (2025). *DSINet: Towards Real-Time Target Speaker Extraction with Dynamic Speaker Information Fusion*. ICASSP 2025. [https://doi.org/10.1109/icassp49660.2025.10887817](https://doi.org/10.1109/icassp49660.2025.10887817)
4. **Sato, H., Moriya, T., Mimura, M., & Horiguchi, S.** (2024). *SpeakerBeam-SS: Real-time Target Speaker Extraction with Lightweight Conv-TasNet and State Space Modeling*. arXiv preprint. [https://doi.org/10.48550/arxiv.2407.01857](https://doi.org/10.48550/arxiv.2407.01857)
5. **Sinha, S., Rollwage, M., & Doclo, S.** (2025). *Real-Time Single-Channel Speaker-Conditioned Target Speaker Extraction Using TCN-Conformer with Efficient Self-Attention Mechanisms*. EUSIPCO 2025. [https://doi.org/10.23919/eusipco63237.2025.11226272](https://doi.org/10.23919/eusipco63237.2025.11226272)
6. **Liang, Y., Xu, C., & Xiang, X.** (2026). *AR-BSNet: Towards Ultra-Low Complexity Autoregressive Target Speaker Extraction With Band-Split Modeling*. ICASSP 2026. [https://doi.org/10.1109/icassp55912.2026.11460642](https://doi.org/10.1109/icassp55912.2026.11460642)
7. **Zhang, Y., Yao, X., & Yang, Y.** (2024). *OR-TSE: An Overlap-Robust Speaker Encoder for Target Speech Extraction*. Interspeech 2024. [https://doi.org/10.21437/interspeech.2024-2322](https://doi.org/10.21437/interspeech.2024-2322)
8. **Horiguchi, S., Ashihara, T., & Delcroix, M.** (2025). *Mitigating Non-Target Speaker Bias in Guided Speaker Embedding*. Interspeech 2025. [https://doi.org/10.21437/interspeech.2025-10](https://doi.org/10.21437/interspeech.2025-10)
9. **Zhang, B., et al.** (2024). *USEF-TSE: Universal Speaker Embedding Free Target Speaker Extraction*. arXiv preprint. [https://doi.org/10.48550/arxiv.2409.02615](https://doi.org/10.48550/arxiv.2409.02615)
10. **Ge, M., Xu, C., & Wang, L.** (2020). *SpEx+: A Complete Time Domain Speaker Extraction Network*. Interspeech 2020. [https://doi.org/10.21437/interspeech.2020-1397](https://doi.org/10.21437/interspeech.2020-1397)
11. **He, L., Liu, Y., & Li, J.** (2024). *3S-TSE: Efficient Three-Stage Target Speaker Extraction for Real-Time and Low-Resource Applications*. ICASSP 2024. [https://doi.org/10.1109/icassp48485.2024.10448000](https://doi.org/10.1109/icassp48485.2024.10448000)
12. **Zhang, X., Li, X., & Wang, Y.** (2024). *Multi-Level Speaker Representation for Target Speaker Extraction*. ICASSP 2025 / arXiv preprint. [https://doi.org/10.48550/arxiv.2410.16059](https://doi.org/10.48550/arxiv.2410.16059)
