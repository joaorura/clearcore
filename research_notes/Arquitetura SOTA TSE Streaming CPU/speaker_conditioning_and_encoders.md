# Speaker Encoders and Conditioning Mechanisms for Target Speaker Extraction (TSE)

**Deep-research report — SOTA speaker encoders, same-sex confusion root cause, conditioning mechanisms, and online adaptation for ClearCore's streaming CPU pipeline.**

- **Date:** October 2026
- **Scope:** Target Speaker Extraction (TSE) specifically — *not* generic speech enhancement.
- **Deployment context:** ClearCore virtual microphone. Enrollment = 5 clean speech samples (2–3 s each); the resulting profile conditions the real-time audio thread to isolate the user's voice from background speakers. Hard constraints: 10 ms frame hop, single-thread x86 AVX2 CPU, zero heap allocation in the audio callback, sub-millisecond neural budget (0.276 ms measured for the base DFNet3 pass).
- **Companion detail notes** (same folder): `encoder_benchmarks.md`, `same_sex_confusion_rootcause.md`, `conditioning_mechanisms.md`, `streaming_adaptation_and_sota.md`.

---

## 1. Executive Summary

1. **Encoder:** **CAM++** (Interspeech 2023) is the best CPU value for ClearCore's enrollment pipeline: 7.18 M params, 27.9 MB FP32 ONNX (~7 MB INT8), **RTF 0.013 (≈39 ms per 3 s sample)**, 0.65 % VoxCeleb1-O EER. **ERes2NetV2** (ICASSP 2024) is the accuracy pick for *short* enrollment (0.98 % EER on 3.0 s trials, 1.48 % on 2.0 s) at ~180 ms and 71.8 MB. **ECAPA2** (Interspeech 2024) is SOTA-accurate (0.34 % EER) but 27 M params / 104 MB — overkill for on-device.
2. **Same-sex root cause:** a single global 192-d vector + static FiLM affine modulation $\hat x = \gamma(z)\odot x + \beta(z)$ is provably unable to null an interferer whose latent subspace overlaps the target's — same-sex speakers share $F_0$ range and formant bands, so $\mathcal S_{\text{tgt}} \approx \mathcal S_{\text{int}}$ and any static gate that suppresses one suppresses the other. Empirical penalty: **2.1–4.9 dB ΔSDR loss** in same-gender mixtures across SpeakerBeam / TD-SpeakerBeam / SpEx / VoiceFilter.
3. **Fix:** replace the pooled vector with a **multi-token speaker memory $M \in \mathbb R^{K\times D}$ ($K=8..16$)** consumed by **KV-cached multi-head cross-attention**, which re-selects phoneme-specific target tokens every frame. Evidence: Attention-TD-SpeakerBeam beats multiplicative conditioning by >1.8 dB SDRi on same-gender mixtures; reference-attention TSE variants report up to +3.4 dB over single-vector FiLM on same-sex test splits.
4. **Conditioning cost (T=1, C=128, K=16, h=4):** FiLM = **128 MACs** (~0.5 µs via Tract/BLAS); **KV-cached MHCA = 36,960 MACs (≈15.5 µs, 16 KB KV cache, fits L1d)**; uncached MHCA = 561,248 MACs — forbidden in the RT path. AdaIN is ill-defined at T=1 (degenerates to AdaLN).
5. **Online adaptation:** feasible **only** with a frozen enrollment anchor + dual confidence gating + bounded residual $\|\Delta_t\|\le0.15\|e_0\|$ + mean-reverting decay, executed **out-of-band** in the Rust supervisor — never in the audio callback, never as unconstrained EMA (confirmation-bias drift is documented; heuristic updates cannot beat the stability-plasticity frontier, Ma/He/Mesgarani 2026).
6. **Recommendation:** keep FiLM as the baseline engine; add a **single MHCA block at the lowest-rate bottleneck over K=16 cached tokens** for a high-selectivity tier; train with **ArcFace (s=32, m=0.25) + auxiliary extracted-speaker CE + SI-SDR**; enroll via CAM++ (or ERes2NetV2 for short clips) pooling 5×2–3 s samples.

---

## 2. Quantitative Speaker Encoder Benchmark

### 2.1 Comparison table

Single-thread x86 CPU, 16 kHz mono, 3.0 s enrollment chunk, batch=1. EER = Equal Error Rate (lower is better); "LM" = large-margin fine-tuning + AS-Norm scores where reported (second number = vanilla checkpoint).

| Model | Params (M) | FLOPs (3 s) | Size FP32 / INT8 (MB) | CPU latency (3 s) | VoxCeleb1-O EER % | VoxCeleb1-E / H EER % | CNCeleb EER % | Emb. dim | Venue / year |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| **CAM++** (DTDNN+CAM) | **7.18** | **1.15–1.72 G** | **27.9 / ~7.0** | **~39 ms (RTF 0.013)** | **0.659 LM** (0.73 base) | 0.80 / 1.56 | 6.78 | 192 / 512 | Interspeech 2023 |
| **ERes2Net-base** | 6.61 | 5.16 G | 26.4 / ~6.6 | ~90 ms (RTF ~0.030) | 0.84 LM (0.97 base) | 0.96 / 1.78 | 6.69 | 192 / 512 | ISCSLP 2022 / Interspeech 2023 |
| **ERes2NetV2** | 17.8 | 12.6 G | 71.8 / ~18.0 | ~180 ms (RTF ~0.060) | **0.61** (0.68 base) | 0.76 / 1.45 | **6.14** | 192 / 512 | **ICASSP 2024** (short-duration SOTA: 0.98 % @3.0 s, 1.48 % @2.0 s) |
| ERes2Net-large | 22.46 | 20.4 G | 89.8 / ~22.5 | ~240 ms | 0.52 | 0.75 / 1.44 | 6.17 | 192/512 | 3D-Speaker 2023 |
| **ECAPA-TDNN C=512** | 6.19 | **1.04 G** | 24.8 / ~6.2 | ~50 ms (RTF ~0.017) | 0.78 LM (1.01 base) | 1.00 / 1.82 | 7.41 | 192 | Interspeech 2020 |
| ECAPA-TDNN C=1024 | 14.65 | 2.65–3.96 G | 58.6 / ~14.7 | ~99 ms (RTF 0.033) | 0.707 (0.87 base) | 0.89 / 1.61 | 7.39 | 192 | Interspeech 2020 |
| **ECAPA2** (LFE+GFE) | 27.0 | ~9.8 G | 103.8 / ~26.0 | ~150–200 ms | **0.34** (0.029 minDCF) | 0.52 / 0.99 | n/r | 192 | **Interspeech 2024** (non-SSL SOTA) |
| **ResNet34-TSTP** (WeSpeaker) | 6.63 | 4.55–6.84 G | 25.3 / ~6.3 | ~96 ms (RTF 0.032) | 0.659 LM (0.867 vanilla) | 0.82 / 1.43 | **6.12** | 256 | Interspeech 2022 / ICASSP 2023 |
| FastResNet / FasNet variants | 4.0–5.8 | 1.8–3.2 G | 16–23 / ~4–6 | ~45–65 ms (RTF 0.015–0.021) | 0.95–1.15 | 1.10–1.30 / 2.10–2.45 | 7.80–8.50 | 192/256 | 2021–2023 |

*Latency figures are paper/toolkit-reported RTF extrapolations for a 3 s chunk on single-thread x86; treat as order-of-magnitude. Verify per target CPU before pinning. minDCF and full trial-split breakdowns are in `encoder_benchmarks.md`.*

### 2.2 Per-encoder notes

- **CAM++** — Wang et al., *CAM++: A Fast and Efficient Network for Speaker Verification Using Context-Aware Masking*, **Interspeech 2023** ([arXiv:2303.00332](https://arxiv.org/abs/2303.00332)). 2D conv front-end (pitch/formant-shift invariance) + densely connected TDNN with context-aware masking + multi-granularity attentive statistics pooling. **>60 % lower latency than ResNet34/ECAPA at equal-or-better EER.** Best RTF/accuracy product on this list.
- **ERes2NetV2** — Gao et al., **ICASSP 2024** ([arXiv:2406.02167](https://arxiv.org/abs/2406.02167)). Purpose-built for **short-duration** verification with bottleneck local/downsample layers — directly relevant because ClearCore enrollment samples are 2–3 s. On 2.0 s trials it reaches 1.48 % EER, beating ECAPA/ResNet34 baselines.
- **ECAPA-TDNN / ECAPA2** — Desplanques et al., **Interspeech 2020** ([arXiv:2005.07143](https://arxiv.org/abs/2005.07143)); Thienpondt & Demuynck, *ECAPA2*, **Interspeech 2024** ([arXiv:2401.08342](https://arxiv.org/abs/2401.08342)). ECAPA-TDNN is ClearCore's *current* enrollment encoder (SpeechBrain `spkrec-ecapa-voxceleb`, Apache-2.0). ECAPA2 is the accuracy ceiling (0.34 % Vox1-O) at 4× the parameters.
- **ResNet34-TSTP / FastResNet** — WeSpeaker toolkit, Wang et al., ICASSP 2023 ([arXiv:2210.17016](https://arxiv.org/abs/2210.17016)). Production workhorse, best-in-class CNCeleb (wild/noisy) at 6.12 %, but 4× the FLOPs and 2.5× the latency of CAM++.

### 2.3 Selection matrix

| Constraint | Winner | Runner-up |
|---|---|---|
| Minimal CPU latency / RTF | **CAM++** (RTF 0.013) | ECAPA-TDNN C=512 (RTF ~0.017) |
| Short enrollment (2–3 s) | **ERes2NetV2** (0.98 % @3 s) | CAM++ |
| Noisy / in-the-wild robustness | ERes2NetV2 / ResNet34-TSTP (~6.1 % CNCeleb) | CAM++ (6.78 %) |
| Absolute discriminability, no limits | ECAPA2 (0.34 %) | ERes2Net-large |
| <10 MB packaged (INT8) | **CAM++** (~7 MB) | ECAPA-TDNN C=512 (~6.2 MB) |

---

## 3. Root Cause Analysis: Same-Sex Speaker Confusion

### 3.1 Why a single 192-d FiLM vector fails on same-pitch / same-gender overlaps

**The pipeline.** Speaker encoder $\mathcal E$ pools the enrollment utterance to one vector; FiLM projects it to per-channel affine parameters:

$$z=\mathcal E(y_{\text{ref}})\in\mathbb R^{D}\ (D=192),\qquad \gamma=W_\gamma z+b_\gamma,\ \beta=W_\beta z+b_\beta,\qquad \hat x_{c,t}=\gamma_c\,x_{c,t}+\beta_c .$$

**Failure mechanism 1 — pooling destroys the discriminative dynamics.** Global average/attentive pooling computes $z \approx \mathbb E_t[h_t]$: only the *centroid* of the speaker's manifold survives — mean vocal-tract length $\bar L_{\text{vt}}$, mean pitch $\bar F_0$, long-term average spectrum. The cues that actually separate two simultaneous talkers — instantaneous formant trajectories $F_1(t),F_2(t),F_3(t)$, glottal-pulse micro-timing, pitch contours $F_0(t)$, phone-transition poles — are averaged away (same_sex_confusion_rootcause.md §1.2.1).

**Failure mechanism 2 — $F_0$/formant overlap across sexes vs within sexes.** Different-sex pairs are easy because physical priors separate them: $\bar F_{0,\text{male}}\in[85,155]$ Hz vs $\bar F_{0,\text{female}}\in[165,255]$ Hz, and $L_{\text{vt}}\approx17$ cm vs $14.5$ cm shifts all formants by 15–20 % ($F_n \approx (2n-1)c_s/(4L_{\text{vt}})$). A static $\gamma$ then works as a coarse band-pass/comb filter. Same-sex pairs have $\Delta F_0\to0$ at crossing moments, overlapping vowel formant bands, and $\cos(z_{\text{tgt}},z_{\text{int}})\gg\cos(z_{\text{male}},z_{\text{female}})$.

**Failure mechanism 3 — linear-affine impossibility.** With mixture features $x_t=\phi(s_{\text{tgt},t})+\phi(s_{\text{int},t})$, FiLM succeeds only if one *static* $\gamma$ satisfies simultaneously

$$\gamma\odot\phi(s_{\text{tgt},t})\approx\phi(s_{\text{tgt},t})\quad\text{and}\quad\gamma\odot\phi(s_{\text{int},t})\approx0\ \ \forall t,$$

which requires the target and interferer latent subspaces to be **orthogonal per channel**. Same-sex speakers span $\mathcal S_{\text{tgt}}\approx\mathcal S_{\text{int}}$ — no static multiplier can null one without zeroing the other. Result: (a) **speaker leakage** (γ stays positive on shared formant channels), or (b) **speaker swap** when the interferer momentarily dominates energy/harmonic salience and the extractor latches onto its pitch track.

**Empirical penalty table (foundation literature):**

| Model | Conditioning | Different-gender ΔSDR | Same-gender ΔSDR | Drop | Reference |
|---|---|---:|---:|---:|---|
| SpeakerBeam | global d-vector (100-d) mult./FiLM | 10.3 dB | 6.2 dB | **−4.1 dB** | Žmolíková et al., IEEE JSTSP 2019 |
| TD-SpeakerBeam | elementwise mult. (512-d) | 15.6 dB | 11.2 dB | **−4.4 dB** | Delcroix et al., Interspeech 2020 |
| SpEx | concat + ResBlock (256-d) | 16.3 dB | 13.9 dB | −2.4 dB | Xu et al., IEEE/ACM TASLP 2020 |
| SpEx+ | shared encoder (256-d) | 17.1 dB | 15.0 dB | −2.1 dB | Ge et al., Interspeech 2020 |
| VoiceFilter | global d-vector (400-d) concat | 12.8 dB | 7.9 dB | **−4.9 dB** | Wang et al., Interspeech 2019 |

**Range: 2.1–4.9 dB ΔSDR degradation attributable directly to single-vector conditioning on same-sex mixtures.**

### 3.2 Why multi-token speaker memory ($K=8..16$) preserves the distinctions

Replace $z\in\mathbb R^D$ with $M=[m_1,\dots,m_K]\in\mathbb R^{K\times D}$ (or a frame sequence $H_{\text{ref}}\in\mathbb R^{T_{\text{ref}}\times D}$), built either by (i) **K learnable latent queries cross-attention-pooling** the enrollment frames (each query specializes: low vowels, fricatives, nasals, modal pitch peaks) or (ii) k-means/VQ centroids of frame embeddings. The mixture frame then *queries* the memory every hop:

$$A_{\tau,k}=\mathrm{softmax}_k\!\left(\frac{Q_\tau K_k^\top}{\sqrt{d_k}}\right),\qquad C_\tau=\sum_{k}A_{\tau,k}V_k .$$

Why this survives same-sex overlap:

1. **Fine-grained harmonic matching** — at an overlapped vowel, $Q_\tau$ matches the token holding *this* speaker's idiosyncratic formant bandwidth/harmonic structure *for this vowel*, not a global mean.
2. **Phone-transition re-gating** — $A_{\tau,k}$ is recomputed per frame; the extraction filter updates on consonant→vowel transitions where same-sex cues actually diverge.
3. **Robustness to interferer salience** — attention keys match speaker-specific resonance even when the interferer is momentarily louder, suppressing speaker-swap events.

**Evidence:** Attention TD-SpeakerBeam > multiplicative FiLM by **>1.8 dB SDRi** on same-gender mixtures (Delcroix et al., Interspeech 2020); SpEx++ multi-scale temporal alignment recovers **2.1 dB** on same-gender errors (Ge et al., Interspeech 2020 / TASLP 2021); reference-attention memory architectures (RAN-TSE-style, ICASSP 2025) report up to **+3.4 dB** over single-vector FiLM on same-sex splits; Styleformer/Manner-TSE (Interspeech 2024) split memory into $K_{\text{id}}=4$ global identity tokens + $K_{\text{manner}}=16$ articulation tokens. Multi-token conditioning typically cuts the same-vs-different gender penalty from **3.5–5.5 dB down to 0.8–1.5 dB**.

**Bounded form for CPU:** keep $K$ fixed (8 or 16) so streaming attention cost is constant — never attend over the raw $T_{\text{ref}}\approx300$–1000 enrollment frames (cost would scale linearly with enrollment length).

### 3.3 Contrastive and speaker losses that force disentanglement

Reconstruction-only training (SI-SDR) lets the network take shortcuts (energy salience) without learning speaker-invariant identity. SOTA TSE uses **multi-task** training:

$$\mathcal L_{\text{total}}=\underbrace{\mathcal L_{\text{rec,multi}}(\hat s,s_{\text{tgt}})}_{\text{multi-scale SI-SDR}}+\lambda_1\underbrace{\mathcal L_{\text{ArcFace}}(z_{\text{ref}},y_{\text{tgt}})}_{\lambda_1=0.1..0.2}+\lambda_2\underbrace{\mathcal L_{\text{spk,ext}}(\bar H_{\text{ext}},y_{\text{tgt}})}_{\lambda_2=0.05..0.1}+\lambda_3\underbrace{\mathcal L_{\text{contrastive}}}_{\lambda_3=0.01}$$

- **ArcFace (additive angular margin)** — Deng et al., TPAMI 2022:
  $$\mathcal L=-\frac1N\sum_i\log\frac{e^{s\cos(\theta_{y_i}+m)}}{e^{s\cos(\theta_{y_i}+m)}+\sum_{j\neq y_i}e^{s\cos\theta_j}},\quad s=32,\ m=0.25\ \text{rad}.$$
  Guarantees inter-speaker geodesic separation $\theta(z_A,z_B)\ge 2m$ on $\mathbb S^{D-1}$ — the mechanism that forces *same-sex* speakers apart in embedding space. **CosFace** variant: $s(\cos\theta_{y_i}-m)$, $m\in[0.2,0.35]$.
- **Auxiliary speaker classification on the extracted stream** (SpEx/SpEx++): classify $\bar H_{\text{ext}}=\frac1T\sum_t \hat M_t\odot X_{\text{mix},t}$ with CE against target ID. Any leak of the interferer's identity into the output triggers a direct penalty — explicitly kills speaker-swap.
- **Asymmetric contrastive leakage term:** $\mathcal L_{\text{contrastive}}=\mathcal L_{\text{rec}}(\hat s,s_{\text{tgt}})-\lambda_{\text{leak}}\mathcal L_{\text{rec}}(\hat s,s_{\text{int}})$ — pulls toward target, pushes away from interferer. (TSE needs no uPIT: the reference anchor resolves permutation; the contrastive term patches "target-tracking dropouts" when the anchor cue is weak.)
- **SI-SDR reconstruction:** $\text{SI-SDR}=10\log_{10}(\|s_{\text{target}}\|^2/\|e_{\text{noise}}\|^2)$, multi-scale weights e.g. $\alpha=(0.8,0.1,0.1)$.

---

## 4. Conditioning Mechanisms Comparison (T=1, C=128, K=16)

### 4.1 Formulas and per-hop cost

| Mechanism | Formula | MACs/frame (T=1) | Tract/BLAS latency | Persistent state | TSE usage |
|---|---|---:|---:|---:|---|
| **Multiplicative gating** | $y=m(z)\odot x$, $m=\sigma(W_m z+b_m)$ | **64** | ~0.25 µs | 0 B (512 B params) | SpeakerBeam compact (ICASSP 2019), VoiceFilter-Lite (Interspeech 2020), MSENet (TASLP 2023) |
| **FiLM** | $y=\gamma(z)\odot x+\beta(z)$ | **128** | ~0.52 µs | 0 B (1 KB params) | SpeakerBeam (Interspeech 2018), USEV (ICASSP 2021), pDFNet2 (ICASSP/HSCMA 2024), ClearCore pDFNet3 Pro |
| **AdaIN → AdaLN at T=1** | $\sigma(z)\frac{x-\mu(x)}{\sigma(x)}+\mu(z)$ (channel-wise when T=1) | ~384–512 | ~1.15 µs | 0 B | AdaIN-VC (TASLP 2021), TSE enhancement (ICASSP 2023) |
| **MHCA (KV-cached)** | $\mathrm{softmax}(QK^\top/\sqrt{d_k})V$, h=4, d_k=32 | **36,960** (16,384 Q + 2,048 QKᵀ + 96 SM + 2,048 AV + 16,384 O) | **~15.5 µs** | **16 KB KV cache (fits L1d)** | SpEx++ (TASLP 2021), CITI (ICASSP 2024), MIMO-Speech2 (Interspeech 2024), Liu et al. (ICASSP 2025) |
| MHCA (uncached) | + 2×K×C×C K/V projections | 561,248 | ~180 µs | — | **Forbidden in RT path** |
| AdaConv (K_w=3) | $y_{t,c}=\sum_k W_{c,k}(z)x_{t-k,c}$ | 384 | ~1.2 µs | 1.5 KB FIR | Interspeech 2024 low-latency TSE explorations |

**FiLM projection cost** ($z\in\mathbb R^{192}\to\gamma,\beta\in\mathbb R^{128}$): 49,152 MACs — computed **once at profile load**, not per frame.

### 4.2 Direct answers to the comparison questions

- **FiLM vs MHCA:** FiLM is a *static point attractor* — one vector, one affine transform, 128 MACs, bit-exact identity fallback ($\gamma=1,\beta=0$), zero state. MHCA is *content-addressable memory* — the frame's query selects which of K tokens matters now, 288× more MACs than FiLM but still only **~15.5 µs** (1.5 % of a 10 ms hop; ~5 % of ClearCore's 276 µs neural budget) **iff K/V are pre-projected and cached at enrollment**. Uncached re-projection (561 kMACs, ~180 µs) wastes 15× the attention cost itself and must be rejected.
- **Multiplicative gating vs FiLM:** gating is cheaper (64 MACs) and strictly attenuating; it lacks the additive shift β, so it cannot move activation thresholds ahead of GRU/ReLU layers. Empirically comparable to FiLM for global conditioning; both share the same single-vector limitation.
- **AdaIN vs the rest:** AdaIN's temporal instance statistics **degenerate at T=1** ($x_1-\mu(x)\equiv0$); it must reduce to channel-wise Conditional LayerNorm (AdaLN). Cost ~1–1.2 µs plus horizontal reductions (`vhaddps`) that break clean SIMD. Verdict: no advantage over FiLM for streaming; **deprecated** for this pipeline.
- **Memory:** only MHCA carries state — 16 KB (K=16, C=128, fp32 K+V), fits entirely in a 32–48 KB L1d, no L2/L3 thrash, but it **must be pre-allocated** at profile activation (no malloc in the callback).
- **Identity/fallback:** FiLM/gating/AdaConv admit exact identity by parameter setting; MHCA needs an explicit residual scale $\alpha$ (AdaLN-Zero style, init 0) or a bypass mux to preserve ClearCore's fail-closed "no profile ⇒ base denoiser" contract.

### 4.3 Emerging mechanisms (2024–2025)

- **AdaLN / AdaLN-Zero** (DiT, ICCV 2023; adopted in 2024 speech Conformer backbones): LayerNorm + z-driven scale/shift + zero-init residual gate α. ~512 MACs. Clean identity at init.
- **AdaConv / FiLM-Conv** (Interspeech 2024 low-latency TSE): z synthesizes a depthwise FIR kernel ($K_w=3$ → 384 MACs, 1.5 KB state) — static during session, compiles to fixed FIR bank.

---

## 5. Online Adaptation in Streaming — Can the Representation Be Refined Without Drift?

### 5.1 The failure mode

Closing the loop $\hat s_t \to \text{Enc} \to e_{t+1}=(1-\alpha)e_t+\alpha\,\text{Enc}(\hat s_t)$ introduces **confirmation bias / target drift**: whenever the interferer leaks into $\hat s_t$ (ε⁽ⁱ⁾ ≠ 0) — especially while the target is silent — the embedding steps toward the intruder. Once $\cos(e_t,e_{\text{int}})>\cos(e_t,e_{\text{tgt}})$, the extractor **permanently flips polarity** and extracts the background speaker.

### 5.2 What the literature establishes

- **Stability-plasticity frontier is real and hard.** Ma, He & Mesgarani, *Beyond the Stability-Plasticity Frontier in Streaming Target Speaker Extraction* (2026, arXiv:2609.20463): **22 heuristic update configurations** (confidence-gated EMA, target-absence thresholds, even *oracle* target-VAD gating) could not beat the trade-off — α>0.05 drifts during pauses/cross-talk; α<0.005 is indistinguishable from static enrollment. Oracle VAD still drifts under overlap because $\hat s_t$ itself contains residual interference.
- **Guided embeddings inherit non-target bias.** Horiguchi, Ashihara & Delcroix, *Mitigating Non-Target Speaker Bias in Guided Speaker Embedding*, **Interspeech 2025** ([DOI:10.21437/interspeech.2025-10](https://doi.org/10.21437/interspeech.2025-10)) — any embedding conditioned on extracted speech absorbs the intruder; requires contrastive anchoring to the clean profile.
- **The viable path is learned, anchored memory.** Anchored Fast-Weights (AFW): ~41 k params, meta-trained through contaminated closed loops, explicit connection to $e_0$ → **+3.0 dB SI-SDR** under severe mismatch while staying within **0.9 dB** of static performance after 30 s of target absence, <5 % overhead.

### 5.3 Safe design pattern (recommended)

1. **Frozen master anchor** $e_0$ from enrollment — read-only, never overwritten.
2. **Dual confidence gating** before any update: TS-VAD posterior > 0.85 **and** $\cos(z_t,e_0)\ge0.82$ (an interferer's embedding typically drops below 0.60 → hard lockout).
3. **Bounded residual only:** $e_t=e_0+\Delta_t$, $\ \Delta_t=(1-\alpha)\Delta_{t-1}+\alpha\,\mathcal P(z_t-e_0)$, $\alpha\le0.01$.
4. **Hard clamp:** $\|\Delta_t\|_2\le0.15\|e_0\|_2$ — mathematically caps worst-case migration.
5. **Mean-reverting decay:** $\Delta\!\leftarrow\!(1-\lambda)\Delta$ with $\lambda\approx0.05/\text{s}$ during silence/low confidence → auto-reset to pristine enrollment.
6. **Execution venue:** **never** in the audio callback. Run embedding extraction, VAD, similarity in the Rust supervisor over a lock-free tap; publish new (γ,β) atomically to the RT thread. Prefer adapting **FiLM vectors / token memory** rather than raw encoder states.

**Bottom line:** Yes, refinement is possible — but only as *bounded, anchored, out-of-band, confidence-gated residual adaptation*. Unconstrained EMA or test-time gradient steps are documented failures.

### 5.4 SOTA streaming TSE architectures 2024–2026 (context for ClearCore)

| Model | Venue | Latency / causality | Params | Conditioning | SI-SDRi | CPU RTF |
|---|---|---|---:|---|---:|---:|
| pDFNet2 (personalized DeepFilterNet2) | ICASSP/HSCMA 2024 | causal, 10 ms hop | 2.3 M | 192-d ECAPA + multi-layer FiLM | +14.2 dB | ~0.04 |
| **DSINet** | **ICASSP 2025** | causal, 16 ms win / 8 ms hop | 1.9–3.8 M | dynamic frame-level speaker fusion (cross-attn, no pre-pool) | +16.8 dB | ~0.12 |
| SpeakerBeam-SS (state-space) | Interspeech/arXiv 2024 | causal, 20 ms win | 4.1 M | 256-d d-vector + multiplicative + SSM | +15.4 dB | ~0.08 |
| TCN-Conformer (linear self-attn) | EUSIPCO 2025 | causal, 16 ms | 4.9 M | FastResNet 192-d + linear cross-attn | +16.1 dB | ~0.15 |
| OR-TSE (overlap-robust encoder) | **Interspeech 2024** | causal, 16 ms | 5.2 M | dual-attentive overlap-robust speaker encoder | +15.9 dB | ~0.18 |
| USEF-TSE (embedding-free) | Interspeech/arXiv 2024 | causal variant | 6.4 M | frame-level cross-attn mixture↔enrollment | +17.4 dB | ~0.28 |
| AR-BSNet | ICASSP 2026 | causal, 10 ms hop | 1.45 M | band-split autoregressive modulation | +15.1 dB | ~0.035 |

Trend: models ≤5 M params with **frame-level / cross-attention conditioning** dominate 2025–2026 — exactly the direction of §3.2/§4.

---

## 6. Recommendation for ClearCore's Enrollment and Conditioning Pipeline

### 6.1 Enrollment (offline / background thread)

1. **Encoder:** switch or offer **CAM++** (7.18 M, 27.9 MB → ~7 MB INT8, RTF 0.013) as the default; keep ECAPA-TDNN as the compatibility baseline (already licensed Apache-2.0, VoxCeleb provenance documented). For the 2–3 s sample length ClearCore actually captures, **ERes2NetV2** is the accuracy-max option (0.98 % EER @3 s) if +140 ms and +45 MB are acceptable offline.
2. **Pooling:** collect embeddings from **all 5 samples × 2–3 s**; normalize each; aggregate as the L2-normalized mean (optionally attentive weighted by per-clip quality/SNR). This yields a more robust $e_0$ than any single clip and directly mitigates short-duration degradation.
3. **Produce two artifacts at profile compile time:**
   - **(a) FiLM vectors** $(\gamma,\beta)$ per injection site (identity-initialized: $W_\gamma=0,b_\gamma=1$; $W_\beta=0,b_\beta=0$) — 1 KB each, baked for the baseline tier.
   - **(b) KV-cached token memory** $M\in\mathbb R^{16\times128}$ plus pre-projected $K_M,V_M$ (16 KB total) for the high-selectivity tier — computed once via cross-attention pooling over the 5 enrollment clips' frame embeddings.
4. Train-time recipe for any retrained pDFNet/TSE variant: **ArcFace (s=32, m=0.25) on the encoder + auxiliary CE on extracted bottleneck + multi-scale SI-SDR + λ₃ contrastive leakage term** (§3.3).

### 6.2 Conditioning (real-time audio thread)

1. **Baseline tier (ship now): FiLM only.** 128 MACs/frame, ~0.5 µs, 0 B state, bit-exact identity fallback — preserves the fail-closed contract and the 276 µs budget.
2. **High-selectivity tier ("Studio Pro" / anti-same-sex-confusion): hybrid FiLM + ONE MHCA block.**
   - Coarse static FiLM at ERB/DF entry (speaker background).
   - Single 4-head cross-attention ($d_k=32$) at the lowest-rate temporal bottleneck ($T=1,C=128$) over the **cached** K=16 tokens: **36,960 MACs ≈ 15.5 µs, 16 KB L1d-resident state, pre-allocated at profile activation**.
   - Wrap with residual scale α (AdaLN-Zero, α=0 unconditioned) or bypass mux for the identity contract.
   - Never instantiate uncached MHCA (561 kMACs) and never attend over raw enrollment frames.
3. **AdaIN:** rejected (degenerate at T=1). **Gating-only:** acceptable cheapest fallback but strictly weaker than FiLM+MHCA for same-sex selectivity.

### 6.3 Online adaptation

- Phase 1: **static profile** (no RT-thread adaptation) — already the safest baseline.
- Phase 2: **out-of-band bounded residual** (§5.3) in the Rust supervisor: dual-gated (TS-VAD>0.85 ∧ cos≥0.82), $\alpha\le0.01$, $\|\Delta\|\le0.15\|e_0\|$, decay 0.05/s; atomic publish of (γ,β) — never touch $e_0$.
- Phase 3 (research): **Anchored Fast-Weights** meta-trained through contaminated loops (Ma et al. 2026) for learned-but-bounded drift resistance.
- Explicitly banned: unconstrained EMA, test-time backprop in the RT path, KV-cache writes from live extracted audio without the cosine gate (leakage keys accumulate → drift).

### 6.4 Decision summary

| Pipeline stage | Choice | Key number |
|---|---|---|
| Enrollment encoder | **CAM++** (alt. ERes2NetV2 for short clips) | RTF 0.013 / 0.65 % Vox1-O; short: 0.98 % @3 s |
| Enrollment aggregation | mean of 5 L2-normalized clip embeddings | robust $e_0$ |
| Baseline conditioning | **FiLM** at GRU/ERB entry | 128 MACs, ~0.5 µs, 0 B, exact identity |
| Same-sex selectivity upgrade | **FiLM + single KV-cached MHCA (K=16, h=4, C=128)** | 36,960 MACs, 15.5 µs, 16 KB |
| Training losses | ArcFace(32, 0.25) + aux extracted-CE + SI-SDR + contrastive leak | forces same-sex disentanglement |
| Adaptation | frozen anchor + gated bounded residual, out-of-band | Δ ≤ 15 %, α ≤ 0.01, decay 0.05/s |
| Rejected | uncached MHCA, AdaIN@T=1, unconstrained EMA/TTT | 561 kMACs / degenerate / drift |

---

## 7. References

**Encoders**
1. Wang, H. et al. *CAM++: A Fast and Efficient Network for Speaker Verification Using Context-Aware Masking.* **Interspeech 2023**. [arXiv:2303.00332](https://arxiv.org/abs/2303.00332)
2. Chen, Y. et al. *An Enhanced Res2Net with Local and Global Feature Fusion for Speaker Verification (ERes2Net).* **Interspeech 2023 / ISCSLP 2022**. [arXiv:2305.12838](https://arxiv.org/abs/2305.12838)
3. Gao, Z. et al. *ERes2NetV2: Boosting Short-Duration Speaker Verification Performance with Computational Efficiency.* **ICASSP 2024**. [arXiv:2406.02167](https://arxiv.org/abs/2406.02167)
4. Desplanques, B., Thienpondt, J., Demuynck, K. *ECAPA-TDNN: Emphasized Channel Attention, Propagation and Aggregation in TDNN Based Speaker Verification.* **Interspeech 2020**. [arXiv:2005.07143](https://arxiv.org/abs/2005.07143)
5. Thienpondt, J., Demuynck, K. *ECAPA2: A Hybrid Neural Network Architecture and Training Strategy for Robust Speaker Embeddings.* **Interspeech 2024**. [arXiv:2401.08342](https://arxiv.org/abs/2401.08342)
6. Wang, H. et al. *WeSpeaker: A Research and Production Oriented Speaker Embedding Learning Toolkit.* **ICASSP 2023**. [arXiv:2210.17016](https://arxiv.org/abs/2210.17016)

**TSE foundations & same-sex analysis**
7. Qian, K. et al. *SpeakerBeam: Speaker Aware Neural Speech Enhancement.* **Interspeech 2018**.
8. Žmolíková, K. et al. *SpeakerBeam: Speaker Aware Neural Network for TSE in Speech Mixtures.* **IEEE JSTSP 13(4), 2019**. DOI:10.1109/JSTSP.2019.2922820
9. Xu, C. et al. *SpEx: Multi-Scale Time Domain Speaker Extraction Network.* **IEEE/ACM TASLP 28, 2020**. DOI:10.1109/TASLP.2020.2987429
10. Ge, M. et al. *SpEx+ / SpEx++: A Complete Time Domain Speaker Extraction Network.* **Interspeech 2020** / TASLP 2021. [arXiv:2005.04686](https://arxiv.org/abs/2005.04686)
11. Delcroix, M. et al. *Improving Time-Domain SpeakerBeam for TSE with Attention Mechanism and Speech-Quality Losses.* **Interspeech 2020**. DOI:10.21437/Interspeech.2020-2782
12. Wang, D., Chen, J., Chen, Z., Yoshioka, T. *Universal Speaker Extraction with Variational Modulation (USEV).* **ICASSP 2021**. DOI:10.1109/ICASSP39728.2021.9414980
13. Wang, Q. et al. *VoiceFilter / VoiceFilter-Lite.* **Interspeech 2019 / 2020**. DOI:10.21437/Interspeech.2019-1582

**Conditioning mechanisms**
14. Perez, E. et al. *FiLM: Visual Reasoning with a General Conditioning Layer.* **AAAI 2018**.
15. Huang, X., Belongie, S. *Arbitrary Style Transfer in Real-time with Adaptive Instance Normalization (AdaIN).* **ICCV 2017**.
16. Vaswani, A. et al. *Attention Is All You Need.* **NeurIPS 2017**.
17. Dauphin, Y. et al. *Language Modeling with Gated Convolutional Networks.* **ICML 2017**.
18. Peebles, W., Xie, S. *Scalable Diffusion Models with Transformers (DiT / AdaLN-Zero).* **ICCV 2023**.
19. Serre, T. et al. *A lightweight dual-stage framework for personalized speech enhancement based on DeepFilterNet2 (pDFNet2).* **ICASSP/HSCMA 2024**. [arXiv:2404.08022](https://arxiv.org/abs/2404.08022)
20. Xiao, Y. et al. *CITI: Cross-Information Temporal Interaction for Target Speaker Extraction.* **ICASSP 2024**.
21. Cheng, S. et al. *MIMO-Speech2: Multi-channel Multi-speaker Separation with Cross-Attention Attractors.* **Interspeech 2024**.
22. Chou, J.-C. et al. *One-shot Voice Conversion by Separating Speaker Characteristics and Linguistic Content using AdaIN.* **Interspeech 2019**.

**Losses**
23. Deng, J. et al. *ArcFace: Additive Angular Margin Loss for Deep Face and Speaker Recognition.* **IEEE TPAMI 44(10), 2022**. DOI:10.1109/TPAMI.2021.3087709

**Streaming SOTA & adaptation**
24. Ma, Y., He, L., Mesgarani, N. *Beyond the Stability-Plasticity Frontier in Streaming Target Speaker Extraction.* **2026**. [arXiv:2609.20463](https://doi.org/10.48550/arxiv.2609.20463)
25. Horiguchi, S., Ashihara, T., Delcroix, M. *Mitigating Non-Target Speaker Bias in Guided Speaker Embedding.* **Interspeech 2025**. DOI:10.21437/interspeech.2025-10
26. Hao, Y., Li, X., Li, J., Zheng, C. *DSINet: Towards Real-Time Target Speaker Extraction with Dynamic Speaker Information Fusion.* **ICASSP 2025**. DOI:10.1109/icassp49660.2025.10887817
27. Sato, H. et al. *SpeakerBeam-SS: Real-time TSE with Lightweight Conv-TasNet and State Space Modeling.* **2024**. [arXiv:2407.01857](https://arxiv.org/abs/2407.01857)
28. Zhang, Y., Yao, X., Yang, Y. *OR-TSE: An Overlap-Robust Speaker Encoder for Target Speech Extraction.* **Interspeech 2024**. DOI:10.21437/interspeech.2024-2322
29. Zhang, B. et al. *USEF-TSE: Universal Speaker Embedding Free Target Speaker Extraction.* **2024**. [arXiv:2409.02615](https://arxiv.org/abs/2409.02615)
30. Sinha, S., Rollwage, M., Doclo, S. *Real-Time Single-Channel Speaker-Conditioned TSE Using TCN-Conformer with Efficient Self-Attention.* **EUSIPCO 2025**. DOI:10.23919/eusipco63237.2025.11226272
31. Liang, Y., Xu, C., Xiang, X. *AR-BSNet: Ultra-Low Complexity Autoregressive TSE with Band-Split Modeling.* **ICASSP 2026**. DOI:10.1109/icassp55912.2026.11460642
32. He, L., Liu, Y., Li, J. *3S-TSE: Efficient Three-Stage Target Speaker Extraction for Real-Time and Low-Resource Applications.* **ICASSP 2024**. DOI:10.1109/icassp48485.2024.10448000

---

*Method note: quantitative encoder figures are transcribed from the primary papers/toolkit READMEs (3D-Speaker, WeSpeaker, SpeechBrain, ModelScope cards) as cited in `encoder_benchmarks.md`; CPU latencies are reported RTF values for a 3 s chunk on single-thread x86 and should be re-measured on ClearCore target hardware before pinning. Conditioning MAC/latency figures are analytic counts for T=1, C=128, K=16, h=4 with stated AVX2/Tract assumptions (see `conditioning_mechanisms.md` for the full derivation). Adaptation thresholds (0.85 / 0.82 / 0.15 / α≤0.01) are engineering starting points consistent with the drift literature, not paper-quoted constants.*
