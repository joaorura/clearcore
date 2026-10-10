# Subband, Filter-and-Sum, and Hybrid Time–Frequency Architectures for Real-Time Streaming Target Speaker Extraction on CPU @ 48 kHz

> **Scope.** Architectural analysis, CPU compute budget, and a concrete optimal hybrid subband TSE specification for
> 48 kHz / 16–48 kHz streaming audio, **10 ms hop (480 samples @ 48 kHz)**, single-core x86_64 CPU, hard
> **≤ 5 ms execution per 10 ms frame** (ClearCore deadline context: Tract baseline = 0.28 ms, migration to
> ONNX Runtime, 100% of budget available for quality gains).
>
> **Companion documents in this folder:** `benchmarks_metrics.md` (model tables + CPU budgets), `bsrnn.md`,
> `conditioning_mechanisms.md` (FiLM/MHCA math), `spex_usef_tse.md`, `tf_gridnet.md`, `voicefilter_lite.md`,
> `streaming_adaptation_and_sota.md`, `mossformer2.md`, `encoder_benchmarks.md`, `open_source_frameworks_and_onnx_runtime.md`,
> `same_sex_confusion_rootcause.md`.

---

## 1. Executive Summary

1. **Pure time-domain separator backbones (Conv-TasNet, DPRNN, SepFormer) are a poor fit for 48 kHz CPU streaming.**
   Their learned encoder emits a latent sequence at `fs/stride` Hz (typically **1000 Hz** at 48 kHz with stride 48) versus
   **100 frames/s** for a 10 ms-hop STFT — a 10× sequence-rate penalty that compounds into quadratic attention cost
   (a fixed 0.5 s chunk contains ~500 frames at 48 kHz vs ~31 at 16 kHz/stride 256 → **~260× more intra-chunk attention**),
   10× longer recurrence chains, and activation tensors ~40× larger than an ERB bottleneck — all of which are
   memory-bandwidth-bound, not FLOP-bound, on a CPU.
2. **DeepFilterNet's hybrid TF design wins because it attacks all three axes of the budget simultaneously:**
   sequence rate (100 fps), channel width (481 bins → **32 ERB bands**, a 15× bottleneck), and operator sparsity
   (separable convs + P=8 grouped GRUs). Result: **0.348 GMAC/s = 3.48 MMAC/frame**, measured **0.28 ms/frame**
   on ClearCore's Tract pipeline and **RTF 0.19 on an old i5-8250U** — i.e. 10–35× inside a 5 ms cap.
3. **DFNet is *not* a TSE architecture.** Its three TSE gaps: (a) 32-band ERB resolution (≥100 Hz min bandwidth) cannot
   resolve speaker-discriminative fine structure and its deep-filter stage only taps *time* within each bin below 5 kHz —
   no cross-frequency, no frame-adaptive speaker selectivity; (b) **no cross-attention** — enrollment enters only as a
   single global embedding via FiLM (static γ, β, ~128 MACs/frame); (c) FiLM-style conditioning is channel-gating only,
   which the USEF-TSE line shows is dominated by **frame-level** speaker features at low SIR.
4. **The recommended upgrade is cheap.** Injecting a **MHCA (h=4, D=128, K=16 enrollment tokens, KV-cached)** into the
   DFNet stage-1 bottleneck costs **36,960 MACs/frame (0.037 MMAC)** — vs 3.48 MMAC for the base model, **+1.1%** —
   and measures well under 0.5 ms (arithmetic ~2–24 µs at 24 GMAC/s measured for D=128 attention kernels; the
   remainder is graph-dispatch overhead). A causal **subband Conformer block** (d=128, h=4, rolling window 16,
   KV-cached) adds only **~0.2 MMAC/frame**. Full recommended pipeline: **≈ 4.0 MMAC/frame ≈ 0.4 GMAC/s →
   ~0.8–1.3 ms paced per 10 ms hop**, 4–6× headroom under the 5 ms cap.
5. **BSRNN-family subband RNN is feasible but expensive**: published online pBSRNN = **14.7 GMAC/s, RTF 0.42
   (4.2 ms/hop)** — inside 5 ms only on paper, before the **2.5–4.2× paced-callback penalty**; verdict: only
   small/compact configs (N≤64–96, 4–6 layers, 33 bands) or it is rejected.

---

## 2. Why Pure Time-Domain Models Struggle at 48 kHz on CPU

### 2.1 The sequence-rate mismatch (48,000 steps/s vs 100 STFT frames)

A learned analysis filterbank (Conv-TasNet style) with kernel `K_e` and stride `S` produces a latent frame rate

```
f_lat = fs / S        (e.g. fs=48 kHz, S=48 → f_lat = 1000 Hz)
```

while the STFT front-end of a 10 ms-hop system produces

```
f_stft = fs / hop = 48000 / 480 = 100 Hz
```

**Exact dimensions at 48 kHz / 10 ms hop:**

| Front-end | Sequence rate | Steps per 10 ms hop | Context for 1 s of audio |
|---|---|---|---|
| Raw samples | 48,000/s | 480 | 48,000 |
| Conv encoder, S=48 (e.g. `penta2himajin/tse-conv-tasnet-48k`: kernel 96, stride 48, 256 ch) | 1,000/s | 10 | 1,000 |
| STFT, hop 480 (DeepFilterNet: N_FFT=960) | 100/s | 1 | 100 |

Every recurrent layer, attention layer, and dual-path chunking scheme scales with `f_lat`. Concrete consequences:

1. **Recurrent chains.** A GRU/LSTM runs `f_lat` sequential steps per second. Sequential steps cannot be vectorized
   across time without chunking, so the wall-clock is bounded by single-step latency × 1000. At 100 fps the same
   layer runs 10× fewer steps.
2. **Quadratic attention.** SepFormer-style intra-chunk self-attention costs `O(L_c²·d)` per chunk for chunk length `L_c`.
   For a *fixed physical chunk duration* of 0.5 s:
   - 16 kHz, stride 256 → 62.5 Hz → `L_c ≈ 31` frames → `L_c² ≈ 961`
   - 48 kHz, stride 48 → 1000 Hz → `L_c ≈ 500` frames → `L_c² = 250,000`

   Ratio ≈ **260×** per layer for the same temporal receptive field. Mitigations (smaller physical chunks, lower
   `d`, chunk stacking) all trade away the very context that made dual-path models strong.
3. **Activation bandwidth.** Per hop, a time-domain separator materializes `[C=256 × T=10]` latent plus per-block
   residuals ≈ tens of KB of *live* activations flowing through every block, versus `[32]` ERB features + `[512]`
   GRU hidden for DFNet's stage 1. On CPU, small sequential GEMMs/elementwise ops are **latency- and
   bandwidth-bound**: the arithmetic intensity stays below the L1/L2 ridge point, so extra FLOPs are "free" but
   extra *bytes* are not (see §7).
4. **Filterbank redundancy.** The learned encoder must re-derive a 96-tap (~2 ms) analysis bank per forward pass;
   at 48 kHz a 2 ms kernel gives 500 Hz native resolution before striding — the model starts with *worse* frequency
   localization than a 20 ms STFT and must spend capacity recovering it.

### 2.2 Measured evidence

| Model | Domain | MACs/s | CPU RTF | ≈ ms / 10 ms hop | Source (this folder) |
|---|---|---|---|---|---|
| SepFormer-class dual-path (typical, 48 kHz port) | Time | 10–50 (est.) | often >0.5 | 5–50 | `benchmarks_metrics.md` §4 |
| `penta2himajin` causal TCN TSE @48 kHz | Time | 4.8 | **0.12** | 1.2 | `benchmarks_metrics.md` |
| Waveformer (streaming TSE, consumer CPU) | Time-hybrid | — | 0.66–0.94 | 6.6–9.4 | arXiv:2211.02250 |
| SpeakerBeam-SS (S4D state-space) | Time | 12.1 | **0.18** | 1.8 | `benchmarks_metrics.md` |
| **DeepFilterNet (hybrid TF)** | **T-F** | **0.348** | **0.19** (i5-8250U) / 0.028 (ClearCore) | **0.28** | §3, `benchmarks_metrics.md` |

**Verdict:** time-domain separation at 48 kHz on CPU is possible only with small causal-TCN designs (~5 GMAC/s,
RTF ≈ 0.12); full SepFormer-class models do not fit. The hybrid TF path delivers **10–40× lower compute** at
comparable or better quality.

---

## 3. Why DeepFilterNet's Hybrid Achieves ~0.28 ms CPU

### 3.1 Signal path and exact dimensions (48 kHz profile)

```
x[n], 480 samples (10 ms)
  → window (960, 50% overlap, Vorbis) → rFFT → X(k,f) ∈ ℂ^481      (bin = 50 Hz)
  ├─ Stage 1 (envelope):  log-power → EMN (τ=1 s) → ERB FB W∈ℝ^(32×481)
  │     → UNet separable-conv encoder/decoder (3×2, C=64) + grouped GRU
  │       (P=8 groups, h=512 total → 64/group)  → gains G(k,b) ∈ ℝ^32
  │     → inverse ERB (Wᵀ) → mask over 481 bins → Y^G = G↑ ⊙ X
  └─ Stage 2 (periodicity): complex X[0:96] (≤4.8 kHz), unit-norm
        → conv/GRU decoder → taps C(k,i,f), i=0..5, f<96 → deep filter on Y^G
        → blend Y = α·Y_DF + (1−α)·Y^G        (α learned, LSNR-gated)
  → iSTFT → ŷ[n], 480 samples
```

**Deep filtering (per bin, per frame):**

```
Y_DF(k,f) = Σ_{i=0}^{N} C(k,i,f) · Y^G(k − i + l, f),    N = 5,  l = l_DF (lookahead frame)
```

Complex multiply-add = 4 mults + 2 adds = 6 FLOPs → application cost:
`96 bins × 6 taps × 6 FLOPs ≈ 3.5 kFLOPs ≈ 0.0006 MMAC/frame` — numerically free; the cost is the *network that
predicts* the taps.

**ERB decomposition:**

```
X_ERB(k,b) = log( ε + Σ_f W[b,f] · |X(k,f)|² ),   b = 0..31
W ∈ ℝ^(32×481),  rows = rectangular ERB filters,  Σ_f W[b,f] = 1
band b covers bins [f_b, f_{b+1}) → min bandwidth = 2 bins × 50 Hz = 100 Hz (low bands 100–250 Hz)
```

### 3.2 The four levers that make it 10–40× cheaper

| Lever | Effect |
|---|---|
| **Sequence rate 100 fps** | 10× fewer recurrent steps / frame boundaries than latent-1 kHz models |
| **ERB bottleneck 481 → 32** | 15× width compression of the expensive encoder/decoder path; features = 128 B/frame |
| **Separable convs (3×2) + P=8 grouped GRU** | depthwise+pointwise and 8× smaller recurrent GEMMs; MACs concentrated in vectorizable 1×1 ops |
| **Stage gating (local SNR ξ)** | ξ < −10 dB → silent output (decoders off); ξ > 20 dB → DF stage off; average-case compute < peak |

**Budget arithmetic:**

```
0.348 GMAC/s ÷ 100 frames/s        = 3.48 MMAC per 10 ms frame
3.48 MMAC ÷ 12.4 GMAC/s (measured) = 0.28 ms          ← matches ClearCore Tract measurement
Stage-1 only: 0.2506 GMAC/s        = 2.51 MMAC/frame  (w/o stage 2, PESQ 2.57 vs 2.81)
Stage-2 (DF): 0.0976 GMAC/s        = 0.98 MMAC/frame  (buys +0.24 PESQ, +2.8 dB SI-SDR)
```

Effective throughput of 12.4 GMAC/s is *below* peak dense-GEMM CPU capability — typical for GRU/elementwise-heavy
graphs — but at that rate the model sits **18× under the 5 ms cap** (0.28 ms), and ~4–7× under it after applying
the 2.5–4.2× paced-callback penalty (§7.2).

---

## 4. DeepFilterNet's Limitations for Target Speaker Extraction

| # | Limitation | Technical detail | Consequence for TSE |
|---|---|---|---|
| L1 | **ERB resolution** | 32 bands; min bandwidth 100 Hz at 50 Hz bin spacing. Male F0 harmonics ~80–150 Hz apart → 1–2 harmonics per low band; formant transitions inside a band are invisible to stage 1. | Cannot resolve speaker-discriminative fine structure; envelope gains act on *both* speakers' energy jointly. |
| L2 | **Deep filter is time-only & band-limited** | Taps `C(k,i,f)` vary over frames but never mix frequencies; applied only to f < 96 (4.8 kHz). | No cross-frequency harmonic grouping (a key speaker cue); nothing above 5 kHz is anything but a per-bin mask. |
| L3 | **No cross-attention / frame-adaptive conditioning** | pDFNet3 injects one enrollment embedding via FiLM: `h' = γ(e)⊙h + β(e)` — **~128 MACs/frame**, constant over time. | Static conditioning cannot attend to *which* enrollment token matches the current phoneme; degrades at low SIR / same-sex pairs (see `same_sex_confusion_rootcause.md`). |
| L4 | **FiLM is channel-gating only** | γ, β ∈ ℝ^C broadcast over T (and F). No content routing between enrollment and mixture. | USEF-TSE (§6.2) demonstrates frame-level speaker features beat global embeddings on WSJ0-2mix/WHAM!/WHAMR!. |
| L5 | **Noise-suppression objective shape** | Bounded gains + α-gating tuned for noise attenuation, not interferer rejection. | Interferer's harmonics pass wherever the ERB gain is open; TSE needs *selective* suppression, not just denoising. |

---

## 5. Subband Decomposition: Mathematical Formulations

### 5.1 ERB filterbank (DeepFilterNet stage 1) — *perceptual, non-uniform*

```
Forward:  X_ERB(k,b) = log( ε + Σ_{f=0}^{480} W[b,f]·|X(k,f)|² ),  W ∈ ℝ^(32×481)
Inverse:  G↑(k,f)    = Σ_{b=0}^{31} W[b,f]·G(k,b)      (transpose, renormalized)
Cost:     2 × 32 × 481 = 30,784 MACs ≈ 0.031 MMAC/frame   (both directions)
Latency:  0 memory (stateless matrix); W is 61 KB fp32 → L2-resident
```

### 5.2 Band-split module (BSRNN family) — *learned, arbitrary partitions*

```
Given partition 0 = G₀ < G₁ < … < G_K = F (F = 481 for 960-pt FFT @48 kHz):
  B_i = X[G_{i-1} : G_i, :]              ∈ ℂ^(w_i × T),   w_i = G_i − G_{i-1}
  Z_i = FC_i( LN( [Re B_i ; Im B_i] ) )  ∈ ℝ^(N × T)       (per-band FC, shared temporal RNN)
Speech (pBSRNN, 48 kHz, 20 ms/10 ms STFT): K = 33 bands
  20 bands × 200 Hz (0–4 kHz) + 6 × 500 Hz (4–7 kHz) + 7 × 2 kHz (7–21 kHz)
Feature dim N = 96;  band/sequence stack: 6 layers, LSTM H = 192
Cost: Σ_i (2·w_i·N) = 2·481·96 ≈ 92 kMACs for split+project (trivial);
      recurrent stack dominates: pBSRNN total = 14.7 GMAC/s = 147 MMAC/frame ← budget problem
```

Key finding (BSRNN paper): **fine-grained splitting below 1 kHz is where the quality lives** (V4–V7 ablation:
100 Hz bands under 1 kHz → large uSDR gains); coarse high bands are nearly free. This transfers directly to TSE:
resolve F0/harmonics below ~4 kHz, compress everything above.

### 5.3 Subband / narrow-band Conformer (NBC family) — *shared per-band backbone*

```
For each band i (or each STFT bin f in NBC):   u_i ∈ ℝ^(T × d_in)
  u_i' = Conformer_shared(u_i)                 (weights shared across all bands → no ×K param blow-up)
  cross-band: optional band-mixing module (1×1 across i, or band-RNN, or cross-attn over K tokens)
NBC (multichannel, Interspeech 2022): per-frequency Conformer H₁=192, H₂=384, 2.0 M params total,
  RTF 1.32 (runs F≈241–481 times per frame) → NOT feasible as-is; feasibility comes from
  running on K=32 bands instead of F bins (15× fewer executions) + causal + KV-cache.
```

**Causal subband Conformer block (streaming, per new frame, KV-cached):**

```
Q = h_t W_Q                     128×128 = 16,384 MACs
Scores = Q·K_winᵀ/√32           16×32 = 512 × 4 heads = 2,048 MACs   (window 16, cached K,V)
Ctx = Scores·V_win              2,048 MACs
Out = ctx W_O                   16,384 MACs
FFN(128→512→128)                2 × 128 × 512 = 131,072 MACs
Depthwise conv (k=15) + PW(128→128)  ~35,000 MACs
─────────────────────────────────────────────────────
≈ 203,000 MACs ≈ 0.20 MMAC per block per frame
KV-cache: 16 × 128 × 4 B × 2 = 16 KB → fits L1d (32–48 KB)
```

Two blocks ⇒ +0.40 MMAC/frame ⇒ +8% over DFNet base. **This is the affordable replacement for the grouped GRU
if frame-adaptive spectral context is wanted** — or run alongside it (stage-1 GRU keeps long memory, Conformer
block adds local spectral modeling).

---

## 6. Conditioning Upgrades for TSE in the Hybrid Pipeline

### 6.1 FiLM (DFNet/pDFNet3 status quo)

```
γ, β = Linear(e);   h' = γ ⊙ h + β,   e ∈ ℝ^192 (ECAPA) or ℝ^128
MACs: 2 × 192 × C ≈ 128–250/frame;  static across time;  memory: none
```

### 6.2 MHCA / CMHA (USEF-TSE formulation) — **recommended**

```
Speaker tokens:  S ∈ ℝ^(K×C), K=16, C=128   (from enrollment encoder, computed ONCE, KV-projected offline)
Mixture feats:   E_m ∈ ℝ^(T×C); streaming query = current frame h_t ∈ ℝ^(1×128)

Q = h_t W_Q,   K_m = S W_K,   V_m = S W_V        (W_Q,W_K,W_O: 128×128; W_V: 128×128)
per head i (h=4, d_k=32):  s_i = Q_i K_iᵀ / √32 ∈ ℝ^(1×16)
                           a_i = softmax(s_i),   o_i = a_i V_i ∈ ℝ^(1×32)
E_spk = [o₁‖…‖o₄] W_O ∈ ℝ^(1×128)
Fusion (either):  concat → FFN   OR   FiLM: h' = γ(E_spk)⊙h + β(E_spk)

Per-frame MACs (KV-cached):
  Q proj 16,384 + scores 2,048 + softmax ~96 + ctx 2,048 + W_O 16,384 = 36,960 MACs  (0.037 MMAC)
Without KV-cache: + 2×16×128×128 = 524,288 → 561,248 MACs (0.561 MMAC)
KV-cache size: 16 KB (L1d-resident)
```

**Why frame-level beats global (literature):** USEF-TSE computes `E_spk = CMHA(q=E_m; k,v=E_r)` with
`E_m ∈ ℝ^(N×L₁)` (mixture) querying `E_r ∈ ℝ^(N×L₂)` (reference) — output aligned to mixture length, so speaker
evidence is *frame-resolved*; SOTA SI-SDR on WSJ0-2mix / WHAM! / WHAMR!. USEF-SepFormer config: CMHA = 4 layers,
8 heads, FFN 1024; USEF-TFGridNet config: CMHA = 1 layer, **4 heads, FFN 512**. The CHiME-9 ECHI low-latency TSE
system replaces global FiLM with exactly this token cross-attention (`Q = X_tok W_Q, K = S W_K, V = S W_V`,
`S ∈ ℝ^(B×k×C)`) and reports improved Csig at unchanged 20 ms latency.

**Placement inside DFNet:** project the stage-1 GRU hidden (512) → 128, run MHCA there, then
(a) FiLM the stage-1 decoder features and (b) FiLM the stage-2 DF-tap head — both heads become
speaker-selective; the ERB gain path suppresses the interferer's envelope, the DF path keeps only
target-periodic components.

### 6.3 MP-SENet / DAME-family and subband ports — *quality ceiling, needs causalization*

- **MP-SENet** (Interspeech 2023 / TASLP): encoder-decoder + 4 TS-Conformer blocks; parallel magnitude-mask
  decoder + wrapped-phase decoder (arctan2 parallel estimation); PESQ **3.50** (VB+D) / 3.60 (DNS). TF-domain,
  25 ms/6.25 ms @16 kHz, **non-causal**. Streaming port requires causalization — use **LaCo-SENet-style
  asymmetric temporal padding** (fixed 1.37 M backbone spans 12.5–75 ms, causal PESQ 3.35 @12.5 ms) rather than
  naive truncation. Its parallel mag/phase head is an attractive *decoder* to graft onto a subband bottleneck,
  but at 4 TS-Conformers it will not land under 5 ms unless the conformers run on the 32-band sequence
  (0.2 MMAC/block × 4 = 0.8 MMAC — actually affordable; the risk is training, not FLOPs).
- **DAME:** no distinct architecture under this name surfaced in the 2023–2026 TSE literature searched here;
  the closest analogues are the dual-path attention mask estimators (TFACM, USEF-TSE's masking network).
  Treat "DAME" as covered by §6.2 and `streaming_adaptation_and_sota.md`.
- **USEF-TSE in subband domain:** run the CMHA over the ERB/band sequence instead of full 481-bin TF:
  `E_m, E_r ∈ ℝ^(Bands×T×C)` with queries per band-frame; cost scales ×(bands/frames) only for the *score*
  term — with K=16 tokens and per-frame queries it stays at 0.037–0.1 MMAC.

---

## 7. FLOPs & Latency Breakdown per 10 ms Hop (48 kHz)

### 7.1 Arithmetic table (MACs per frame, 100 fps)

| # | Component | Operation / dims | MACs/frame | Share of 4 MMAC budget |
|---|---|---|---|---|
| 1 | Window + rFFT(960) + OLA | ~5·N·log₂N FLOPs ≈ 47.5 kFLOPs | 0.024 MMAC | 0.6% |
| 2 | ERB analysis + inverse | 2 × 32 × 481 | 0.031 MMAC | 0.8% |
| 3 | Exp-mean-norm / unit-norm state | elementwise, τ=1 s | ~0.0002 MMAC | ~0% |
| 4 | **Stage 1** UNet + grouped GRU (sepconv 3×2, C=64; P=8, h=512) | published 0.2506 GMAC/s | **2.506 MMAC** | 63% |
| 5 | **Stage 2** DF-tap decoder | published (0.3482−0.2506) | **0.976 MMAC** | 24% |
| 6 | Deep-filter application | 96 × 6 taps × 6 FLOPs | 0.0006 MMAC | ~0% |
| 7 | **MHCA (KV-cached, h=4, D=128, K=16)** | §6.2 | **0.037 MMAC** | 0.9% |
| 8 | FiLM readout heads (2× 128→C) | ~2×128×64 | 0.016 MMAC | 0.4% |
| 9 | *(optional)* causal subband Conformer ×1 | d=128, h=4, win=16, FFN 512 | 0.20 MMAC | 5% |
| 10 | *(optional)* MHCA without KV-cache | §6.2 | 0.561 MMAC | (worst case +14%) |
| | **Total: DFNet3 base + MHCA** | | **≈ 3.57 MMAC** | |
| | **Total: + 1 Conformer block (recommended)** | | **≈ 3.97 MMAC** | |
| | **Total: BSRNN pBSRNN (reference)** | published 14.7 GMAC/s | **147 MMAC** | 37× the recommended build |

### 7.2 From MACs to milliseconds (paced real-time)

```
Measured racing throughput (DFNet-derived): 3.48 MMAC / 0.28 ms = 12.4 GMAC/s
Small-attention kernels (D=128):            2.11 MMAC / 89 µs   = 23.6 GMAC/s   (Kristiansen 2025)
Pacing penalty (audio callback, 10 ms duty): ×2.5 – ×4.2  (faster-enhencer.c study, Kim et al. 2026)
Conservative paced throughput:              12.4 / 2.5 ≈ 5.0 GMAC/s  (to 12.4/4.2 ≈ 3.0)
```

| Configuration | MACs/frame | Paced est. (3–5 GMAC/s) | vs 5 ms cap |
|---|---|---|---|
| ClearCore baseline (DFNet3, Tract, measured) | 3.48 | **0.28 ms** measured racing → 0.7–1.2 ms paced | **4–7× headroom** |
| + MHCA (KV-cached) | 3.57 | 0.72–1.19 ms | 4.2× |
| + MHCA + 1 subband Conformer block | 3.97 | 0.79–1.32 ms | 3.8× |
| + MHCA + 2 Conformer blocks | 4.17 | 0.83–1.39 ms | 3.6× |
| pBSRNN online (published) | 147 | 29–49 ms?? → measured RTF 0.42 on i5 = **4.2 ms/hop racing** | ⚠ 1.2× racing only; **fails paced** |
| TF-GridNet-causal | 1,792 | RTF > 1.00 | ✗ |

**Hard limits:** target **≤ 3.0 ms paced execution per 10 ms frame** (RTF ≤ 0.30, 70% headroom for host DSP/OS);
the task's absolute cap of 5 ms leaves no headroom for pacing — treat 3 ms as the design target, 5 ms as the
never-exceed line.

### 7.3 Answer to "Can MHCA (4 heads, D=128, K=16) run < 0.5 ms on a modern CPU?"

**Yes — with ≥2× margin, comfortably with KV-cache.**

- Arithmetic: **36,960 MACs** (cached) → at 23.6 GMAC/s (measured D=128 attention kernel) = **1.6 µs**;
  even at a pessimistic 1 GMAC/s = 37 µs. Uncached: 561,248 MACs → 24 µs @23.6 GMAC/s, 0.56 ms @1 GMAC/s
  (worst case crosses 0.5 ms only at <1.1 GMAC/s effective — not observed on any modern x86 core).
- Memory: KV-cache **16 KB** + W_Q/W_K/W_V/W_O **256 KB** (fp32) — KV in **L1d**, weights streaming from L2/L3;
  per-frame bytes touched ≪ 32 KB working set.
- Overhead floor: an ORT subgraph of ~8–10 nodes (MatMul×4, Softmax, Add, Mul, Transpose-free with pre-transposed
  KV) dispatches in ~10–50 µs single-threaded; **total node time ≈ 0.02–0.1 ms**, i.e. inside budget even with
  framework overhead included.
- Caveats measured in the wild: ORT's fused `com.microsoft::MultiHeadAttention` kernel was found *slower* than the
  unfused MatMul path for small shapes (ORT issue #19924) — **benchmark both; keep the unfused path available.**
  Keep `intra_op_num_threads = 1` for the audio thread (ORT scales poorly to 8 cores for tiny graphs; the 8-core
  gain on BERT-base was 8.9× only because of batching).

---

## 8. CPU Memory Bandwidth & Cache Efficiency (L1/L2 vs Tensor Shapes)

### 8.1 Working-set audit of the recommended pipeline (per frame, fp32)

| Tensor / state | Shape | Bytes | Cache tier |
|---|---|---|---|
| Input hop + window | 480–960 | 2–4 KB | L1 |
| Complex spectrum | 481 × 2 | 3.8 KB | L1 |
| ERB features / gains | 32 | 128 B | L1 (register) |
| Stage-1 UNet activations (C=64) | 64 × F' (≤481) | ≤ 61 KB | **L1 edge / L2** — keep channels ≤64 at full F |
| GRU hidden + state (P=8 × 64) | 512 × 2 | 4 KB | L1 |
| Stage-2 DF features (96 bins) | 96 × 2 × C | ≤ 24 KB | L1 |
| MHCA KV-cache | 16 × 128 × 2 | **16 KB** | **L1d (32–48 KB)** ✓ |
| MHCA weights (W_Q,K,V,O) | 4 × 128² | 256 KB | L2 |
| Model weights total (~2.5 M params) | — | 10 MB | **L3** (8–32 MB typical) |

**Weight-streaming bound:** at 100 fps, re-reading 10 MB of weights each frame would demand 1 GB/s — fine for
L3 (~50–100 GB/s) but it means **cold-cache first frames after a pacing sleep pay DRAM latency**; this is exactly
the pacing penalty (×2.5–4.2). Mitigations: (1) keep total params ≤ ~3 M (12 MB < common L3 slices);
(2) fold BN into convs (already done in ONNX exports) to shrink bytes; (3) **INT8 weight-only quantization**
halves streaming bytes → ~2× on bandwidth-bound frames; note micro-benchmarks show FP32 can beat INT8 for
*sub-1M* graphs where dequant overhead dominates — **profile before committing**.

### 8.2 Shape rules for the hybrid design (invariants to preserve)

1. **No full 481-bin attention.** Attention only over K=16 speaker tokens or rolling windows ≤16 frames.
   A `T × 481` score map (e.g. 100 × 481 × 4 B = 192 KB per layer per frame) evicts L1 and breaks the pacing budget.
2. **Bottleneck before depth.** Compress F→32 (ERB) or F→bands (≤33) *first*; deep layers must see ≤96 channels-wide
   tensors, not 481.
3. **Cache the static.** KV-projections of enrollment and ERB matrix W are frame-invariant → precompute, pin, never
   re-materialize.
4. **One frame in, one frame out.** All state updates are O(state) bytes; no history tensors longer than the
   attention window (16 frames × 128 × 4 B = 8 KB).
5. **Vectorization-friendly ops only:** 1×1 convs / MatMul (GEMM), depthwise 3×2 (small im2col), elementwise;
   avoid per-bin Python-level loops — per-band loops over K=33 are acceptable if fused into one batched GEMM
   (`[K, w_i, 2] → [K, N]` via banded block-diagonal multiply).

---

## 9. BSRNN vs Subband GRU vs Subband Conformer — Trade-Off Matrix

| Criterion | BSRNN / pBSRNN | Subband GRU (DFNet-style) | Subband Conformer (causal, KV-cached) |
|---|---|---|---|
| MACs/s (48 kHz streaming) | **14.7 GMAC/s** (pBSRNN published) | 0.25–0.35 | +0.2 MMAC/block → +20 GMAC/s?? **No:** 0.2 MMAC × 100 = 0.02 GMAC/s/block |
| ms / 10 ms hop (racing) | 4.2 (i5 2.5 GHz, ONNX RT) | 0.20–0.28 | +0.02–0.04 per block |
| Paced (×2.5–4.2) | **10.5–17.6 ms ✗** | 0.5–1.2 ✓ | +0.05–0.17 per block ✓ |
| Cross-band modeling | explicit band-RNN + seq-RNN | implicit in UNet pathway convs | explicit via windowed attention across band-stacked tokens |
| Frame-adaptive (TSE) | needs external conditioning | FiLM only (static) | **native cross-attn to speaker tokens** |
| Streaming state | 2 LSTM states per layer (small) | GRU state 4 KB | KV-cache 16 KB/layer |
| Verdict | **Reject at 48 kHz/10 ms** unless N≤64, 4 layers, ≤33 bands re-measured to <1.5 GMAC/s | **Baseline (proven)** | **Recommended upgrade** |

---

## 10. Recommended Optimal Hybrid Subband TSE Specification

> Working name: **DFNet-TSE-MHCA** — DeepFilterNet two-stage hybrid + subband bottleneck + cached MHCA
> conditioning + optional causal subband Conformer block. Target: **48 kHz, 10 ms hop, ≤3 ms paced /
> never >5 ms**, ONNX Runtime, single thread, fp32 (INT8 weights optional).

### 10.1 Front-end

| Item | Value |
|---|---|
| Sample rate | 48,000 Hz (accept 16–48 kHz; bandplan scales) |
| Frame / hop | 960-sample analysis window, **hop = 480 (10 ms)**, 50% overlap, Vorbis (or √-Hann) window |
| Spectrum | rFFT 960 → **481 complex bins** (50 Hz spacing) |
| Algorithmic latency | window 20 ms + lookahead `l_DF = 1` frame (10 ms) + conv lookahead ≤2 frames → **≤ 40 ms total, tunable to 30 ms** (l_DNN=1) |

### 10.2 Subband front (stage 1 — envelope, speaker-selective gains)

```
Features:   log-power ERB, EMN (τ = 1 s)                 [1×32] per frame
Bands:      32 rectangular ERB bands (upgrade option: 40 bands = fine 100 Hz split below 4 kHz,
            BSRNN-inspired, +8 bands → +0.008 MMAC)
Encoder:    2 × separable conv (3×2), C = 64, causal after frame 2
Bottleneck: grouped GRU, P = 8, total h = 512 (64/group), 2 layers        ← keep
NEW:        hidden h₅₁₂ → Linear → z ∈ ℝ^128   (MHCA query tap)
MHCA:       h=4, d=32, K=16 speaker tokens (KV-cached) → e ∈ ℝ^128
Decoder:    FiLM(z = γ(e)⊙h + β(e)) into decoder + stage-2 head
Output:     gains G ∈ [0,1]^32 → inverse ERB → mask over 481 bins → Y^G
```

### 10.3 Periodicity stage (stage 2 — deep filtering, target-selective)

```
Input:      Y^G spectrum, bins 0..95 (≤ 4.8 kHz), exponential unit norm
Head:       conv + grouped GRU → taps Ĉ ∈ ℂ^(5×96) + α ∈ (0,1), FiLM-modulated by e
Filter:     Y_DF(k,f) = Σ_{i=0}^{5} C(k,i,f)·Y^G(k−i+1, f)     (causal taps + 1-frame lookahead)
Blend:      Y = α·Y_DF + (1−α)·Y^G   for f < 96;   Y = Y^G  for f ≥ 96
Gate:       local-SNR ξ: ξ<−10 dB → silence; ξ>20 dB → skip DF decoder   (compute scaling, keep)
```

### 10.4 Speaker path (offline / enrollment)

```
Enrollment encoder:   5–10 s → STFT(960/480) → conv stack → frame feats F_r ∈ ℝ^(L_r×128)
                      → attention-pool / strided pool → S ∈ ℝ^(16×128)   (16 tokens)
KV projection:        K = S W_K,  V = S W_V   → stored 16 KB, pinned (recompute only on re-enrollment)
No per-frame cost from enrollment path (0 MACs).
```

### 10.5 Optional add-ons (budget-permitting, in priority order)

1. **Causal subband Conformer block** ×1 on the 32-band sequence (window 16, FFN 512): +0.20 MMAC, +0.05–0.07 ms.
2. **Second MHCA** feeding only the stage-2 DF head (separate 16-token KV): +0.037 MMAC.
3. **Band-fine split below 4 kHz** (32→40 bands): +~0.01 MMAC.
4. **INT8 weight-only quant** on stage-1 convs if profiling shows DRAM-bound pacing frames.
5. *Reject:* full pBSRNN stack, TF-GridNet blocks (179 GMAC/s), dual-path chunk attention, per-bin (481) Conformer.

### 10.6 Final budget ledger

```
Front-end FFT + ERB          0.055 MMAC
Stage 1 (UNet + GRU)         2.506 MMAC
Stage 2 (DF head + apply)    0.977 MMAC
MHCA (KV-cached) + FiLM      0.053 MMAC
+ 1 causal Subband Conformer 0.200 MMAC
────────────────────────────────────────
TOTAL                        ≈ 3.79 MMAC/frame  ≈ 0.379 GMAC/s
Racing  (12.4 GMAC/s)        ≈ 0.31 ms
Paced   (3.0–5.0 GMAC/s)     ≈ 0.76–1.26 ms
Hard cap                     5.0 ms            → 4.0–6.6× headroom ✓
Design target (≤3 ms)        ✓
Params                       ≈ 2.4 M (9.6 MB fp32) → L3-resident
Live working set             ≈ 130 KB → L1/L2-resident hot path
```

### 10.7 Verification plan (before training)

1. **Graph-shape check:** export MHCA + Conformer block to ONNX (opset ≥17), confirm unfused MatMul path is
   selectable (ORT #19924), `intra_op_num_threads=1`.
2. **Microbenchmark:** MHCA-only session, `T=1`, KV pre-pinned → assert p99 < 0.5 ms; full graph → assert
   paced p99 < 3 ms using a 10 ms sleep-wake loop (not a saturated race loop).
3. **Ablation gates:** base (0.28 ms) → +MHCA → +Conformer → +40 bands; each step must stay ≤3 ms paced and
   must show SI-SDRi gains on same-sex / low-SIR sets (the failure modes in `same_sex_confusion_rootcause.md`).

---

## 11. Direct Answers to the Prompt Questions

| Question | Answer |
|---|---|
| Why do pure time-domain models struggle at 48 kHz on CPU? | Latent rate 1000 Hz vs 100 fps STFT → 10× recurrence steps, ~260× intra-chunk attention for fixed physical context, 40× wider activation traffic; measured RTF 0.66–0.94 (Waveformer) vs 0.028 (DFNet). §2 |
| Why does DeepFilterNet hit ~0.28 ms? | 3.48 MMAC/frame: 100 fps × 32-band ERB bottleneck × separable/grouped ops × stage gating; 12.4 GMAC/s effective ⇒ 0.28 ms. §3 |
| DFNet limits for TSE? | ERB ≥100 Hz resolution; time-only, <5 kHz deep filter; no cross-attention; static FiLM (~128 MACs); NS objective. §4 |
| Subband Conformer / Causal Subband Transformer? | Shared per-band backbone on ≤33 bands; KV-cached window-16 block = 0.20 MMAC/frame — affordable. §5.3, §10.5 |
| Deep Filtering + MHCA? | Exactly the recommended build: MHCA at stage-1 bottleneck → FiLM both heads; 36,960 MACs/frame (+1.1%). §6.2, §10 |
| MP-SENet / DAME / USEF-TSE in subband domain? | MP-SENet = quality ceiling, causalize via asymmetric padding; DAME not found as distinct model; USEF-TSE CMHA ports to band sequence at ≤0.1 MMAC. §6.3 |
| BSRNN vs Subband GRU/Conformer? | pBSRNN 14.7 GMAC/s / RTF 0.42 fails paced 10 ms; subband GRU is the proven baseline; causal subband Conformer is the cheap upgrade. §7, §9 |
| FLOPs per 10 ms hop (ERB, complex filtering, attention)? | ERB 0.031, DF apply 0.0006, MHCA 0.037 (cached), Conformer block 0.20 MMAC; base model 3.48 MMAC dominates. §7.1 |
| MHCA (h=4, D=128, K=16) < 0.5 ms on CPU? | **Yes:** 36,960 MACs → 1.6–37 µs arithmetic; 16 KB KV in L1d; total with ORT dispatch 0.02–0.1 ms. §7.3 |
| Memory bandwidth / cache efficiency? | Weight-streaming bound (10 MB → L3), pacing penalty ×2.5–4.2; shape invariants: no T×481 attention, bottleneck ≤32 bands first, pin statics, state-only updates. §8 |
| Optimal hybrid spec? | §10 — DFNet-TSE-MHCA ≈ 3.79 MMAC/frame, ~0.8–1.3 ms paced, ≤3 ms design target. |

---

## 12. References (sources used for this document)

1. Schröter et al., *DeepFilterNet: A Low Complexity Speech Enhancement Framework for Full-Band Audio based on Deep Filtering*, arXiv:2110.05588 (architecture, 0.348 GMAC/s, ERB/DF math, Table 1).
2. Schröter et al., *DeepFilterNet: Perceptually Motivated Real-Time Speech Enhancement*, Interspeech 2023 (RTF 0.19 i5-8250U, stage gating, 96 bins/4.8 kHz, 2-frame lookahead).
3. `soniqo/DeepFilterNet3-ONNX` & `aufklarer/DeepFilterNet3-MLX` model cards (960/480/Vorbis, 481 bins, ERB 32, taps [1,5,T,96,2], 2,131,824 params).
4. Luo et al., *Music Source Separation with Band-split RNN*, arXiv:2209.15174 (band-split math, ablation V1–V7).
5. Yu et al., *High Fidelity Speech Enhancement with Band-split RNN*, Interspeech 2023 (pBSRNN: 33 bands, N=96, H=192, 14.7 GMAC/s, RTF 0.42 ONNX/i5).
6. Zeng & Li, *USEF-TSE: Universal Speaker Embedding Free Target Speaker Extraction*, arXiv:2409.02615 (CMHA formulation, 4h/8h configs, SOTA SI-SDR).
7. CHiME-9 ECHI low-latency TSE approaches, arXiv (cross-attention FiLM over speaker tokens, 20 ms, bounded masking).
8. Quan & Li, *Multichannel Speech Separation with Narrow-band Conformer*, Interspeech 2022 (per-frequency shared Conformer, H=192/384, RTF 1.32).
9. Lu et al., *MP-SENet*, Interspeech 2023 / arXiv:2308.08926 (parallel mag/phase, PESQ 3.50/3.60); LaCo-SENet arXiv:2606.19688 (causalization, 1.37 M, PESQ 3.35 @12.5 ms).
10. Kim et al., *faster-enhancer.c* / FasterEnhancer (paced vs racing penalty ×2.5–4.2, RTF 0.012–0.101) — see `benchmarks_metrics.md` §4.2.
11. Kristiansen, *Optimizing a Tiny Transformer on CPU* (D=128 attention kernels: 2.11 MMAC / 89 µs ≈ 23.6 GMAC/s; L2-resident fusion findings).
12. Microsoft ONNX Runtime issues #19924 (fused MHA slower than unfused), #6639 (thread scaling), MLAS GQA benchmarks — see `open_source_frameworks_and_onnx_runtime.md`.
13. `penta2himajin/tse-conv-tasnet-48k` model card (kernel 96/stride 48/256 ch, 1.45 M, streaming ONNX); Waveformer arXiv:2211.02250 (RTF 0.66–0.94).
14. Companion dossiers in this folder (benchmarks, BSRNN, conditioning, SpEx/USEF, TF-GridNet, VoiceFilter-Lite, streaming SOTA, same-sex confusion) — full citations therein.
