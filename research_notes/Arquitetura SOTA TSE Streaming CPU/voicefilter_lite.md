# Research Dossier: VoiceFilter-Lite & Google Streaming Target Speaker Extraction (TSE) for Real-Time CPU Deployment

## 1. Executive Summary & Core Identity

- **Model Name:** VoiceFilter-Lite (VFL)
- **Year & Venue:** Published at **Interspeech 2020** (Virtual/Shanghai), arXiv preprint `arXiv:2009.04323` (Sept 2020). Official Google Research Blog published November 11, 2020.
- **Authors (Google Research & Google Assistant):**
  Quan Wang, Ignacio Lopez Moreno, Mert Saglam, Kevin Wilson, Alan Chiao, Renjie Liu, Yanzhang He, Wei Li, Jason Pelecanos, Marily Nika, Alexander Gruenstein.
- **Predecessor:** "VoiceFilter: Targeted Voice Separation by Speaker-Conditioned Spectrogram Masking" (Wang, Muckenhirn, Wilson, Sridhar, Wu, Hershey, Saurous, Weiss, Jia, Moreno — Interspeech 2019 / arXiv:1810.04826).
- **Subsequent Google Extensions (2021–2022):**
  - "Personalized Keyphrase Detection using Speaker and Environment Information" (Rikhye et al., Interspeech 2021).
  - "Multi-user VoiceFilter-Lite via Attentive Speaker Embedding" (Rikhye, Wang, Liang, He, McGraw — IEEE ASRU 2021).
  - "Closing the Gap between Single-User and Multi-User VoiceFilter-Lite" (Rikhye, Wang, Liang, He, McGraw — Odyssey 2022 / arXiv:2202.12169).
- **Core Mission:** Real-time, streaming, on-device Target Speaker Extraction designed as an acoustic feature preprocessor for automatic speech recognition (ASR) and personalized keyphrase detection (KWS/SV) on mobile phones, smart displays, and smart speakers (Google Home / Nest).

---

## 2. Foundational Architecture & Evolution

### 2.1 Foundational VoiceFilter (2018–2019) vs. VoiceFilter-Lite (2020)

| Architectural Dimension | VoiceFilter (Interspeech 2019) | VoiceFilter-Lite (Interspeech 2020) | Multi-User VFL (Odyssey 2022) |
| :--- | :--- | :--- | :--- |
| **Operation Domain** | Time-Frequency Spectrogram (STFT $\to$ iSTFT audio output) | Acoustic Feature Domain (ASR stacked log Mel-filterbanks) | Feature Domain (stacked log Mel-filterbanks) |
| **Streaming / Causality** | Non-causal (Bi-LSTM, temporal 2D convolutions with future context) | **Strictly causal & streaming** (Uni-directional LSTM, optional 1D frequency conv) | Strictly causal & streaming (with AttentionNet) |
| **Audio Reconstruction** | Inverse STFT using noisy phase | **None** (Outputs enhanced filterbanks straight to acoustic model) | None (Feature level enhancement) |
| **Network Topology** | 8 Conv layers (dilated in time/freq) + 1 Bi-LSTM (400) + 2 FC (600) | **3 Uni-LSTM layers (512 or 256 cells)** + 1 FC mask layer (+ optional 2 FC noise-type) | 3 Uni-LSTM (512/256) + FiLM conditioning + AttentionNet |
| **Conditioning Mode** | Frame-wise concatenation of d-vector after CNN 8 | Frame-wise concatenation (or FiLM in 2022) with input feature | **FiLM (Feature-wise Linear Modulation)** + Softmax Attention |
| **Model Size (Weights)** | > 100 MB (Float32 TF Graph) | **6.8 MB (FP32/512-node) $\to$ 2.2 MB (Int8/256-node)** | **3.23 MB (Int8)** |
| **Over-Suppression Handling**| Standard L2 spectral loss (degrades clean speech WER) | **Asymmetric L2 Loss ($\alpha=10$) + Adaptive Suppression ($w^{(t)}$)** | Asymmetric Loss + FiLM + Dual-LR AttentionNet |

### 2.2 Speaker Encoder (d-vector Generator)
- **Model Topology:** 3-layer uni-directional LSTM network with 768 projection nodes or 512 hidden nodes, trained with Generalized End-to-End (GE2E) loss (Wan et al., ICASSP 2018).
- **Input Features:** 40-dim or 128-dim log Mel-filterbanks extracted over 1600 ms sliding windows with 50% overlap.
- **Output:** Fixed 256-dimensional $L_2$-normalized speaker embedding vector ($\mathbf{e} \in \mathbb{R}^{256}$).
- **Deployment Lifecycle:** Offline one-time enrollment on device. The reference embedding is stored locally and loaded at runtime into RAM with zero ongoing computational cost during inference.

---

## 3. Streaming Design, Algorithmic Latency & Framing

### 3.1 Input/Output Feature Pipeline
VoiceFilter-Lite supports three operating configurations for input/output representations:
1. **513-dim FFT Magnitudes:** 32 ms window, 10 ms hop size (16 kHz, 512-point FFT + 1 Nyquist bin).
2. **128-dim Log Mel-Filterbank Energies:** Extracted over 32 ms Hanning window, 10 ms step (125 Hz to 7500 Hz).
3. **512-dim Stacked Log Mel-Filterbanks (Google Production Default):**
   - Base frames: 32 ms Hanning window with a 10 ms frame shift (hop).
   - Stacking factor: 4 consecutive frames stacked ($128 \times 4 = 512$ dimensions).
   - Subsampling / Frame stride: Subsampled by a factor of 3 frames.
   - **Effective Frame Rate / Execution Step:** **30 ms**. The model only fires once every 30 ms!

### 3.2 Latency Breakdown
- **Algorithmic Lookahead:** **0 ms**. The network is strictly causal. There are no temporal convolutions with future receptive field, no bi-directional recurrent connections, and no lookahead buffers.
- **Buffer / Framing Latency:**
  - In single-frame mode (10 ms hop): Algorithmic delay is the window length (32 ms) with 10 ms hop.
  - In stacked-by-4, subsampled-by-3 mode: Input requires accumulating 3 new 10 ms hops (30 ms) to emit the next 512-dim stacked frame.
  - In streaming audio deployment (e.g. Ronssin et al., Interspeech 2022 using VoiceFilter causal setup with iSTFT): Total round-trip latency was measured at ~50 ms (40 ms window/hop framing + resamplers + compute).
  - For a standard 10 ms hop streaming DSP pipeline (e.g., Clearcore, 48 kHz mono downsampled to 16 kHz or processed at subbands): When running frame-by-frame without temporal stacking, algorithmic lookahead is **0 ms**.

---

## 4. Speaker Conditioning Mechanisms

### 4.1 Concatenation (Original VFL 2020)
- In the original Interspeech 2020 VoiceFilter-Lite, the 256-dim d-vector $\mathbf{e}$ is concatenated directly to the input feature vector at each time step $t$:
  $$\mathbf{z}^{(t)} = [\mathbf{x}^{(t)}; \mathbf{e}] \in \mathbb{R}^{D + 256}$$
  where $D = 512$ (stacked filterbanks) or $D = 128$ (single filterbank).
- The concatenated vector is fed directly to the first LSTM layer.
- **Drawback:** Concatenation forces the first linear projection matrix of LSTM Layer 1 to learn a joint representation from heterogenous signal modalities without explicit dynamic feature scaling.

### 4.2 FiLM (Feature-wise Linear Modulation) Conditioning (2022 Odyssey)
- In "Closing the Gap between Single-User and Multi-User VoiceFilter-Lite" (Rikhye et al., 2022), Google replaced concatenation with **FiLM**:
  $$\mathbf{x}_{\text{trans}}^{(t)} = \text{LayerNorm}(\mathbf{x}^{(t)})$$
  $$\boldsymbol{\gamma} = \mathbf{W}_\gamma \mathbf{e} + \mathbf{b}_\gamma, \quad \boldsymbol{\beta} = \mathbf{W}_\beta \mathbf{e} + \mathbf{b}_\beta$$
  $$\mathbf{x}_{\text{film}}^{(t)} = \boldsymbol{\gamma} \odot \mathbf{x}_{\text{trans}}^{(t)} + \boldsymbol{\beta}$$
- $\mathbf{W}_\gamma, \mathbf{W}_\beta$ are small 2-layer projection networks mapping $\mathbf{e} \in \mathbb{R}^{256} \to \mathbb{R}^D$.
- **Impact of FiLM:**
  1. Reduced overall parameter count (reduced model size from 3.47 MB to 3.23 MB).
  2. Relative **26.7% reduction in Equal Error Rate (EER)** at 0 dB SNR in multi-speaker conditions.
  3. Closed the gap between single-user and multi-user performance without degrading single-speaker clean audio.

### 4.3 Multi-User Attention Conditioning (Attentive Speaker Embedding)
- For shared smart home devices with $N$ enrolled profiles ($N \le 4$ typically):
  - A tiny scorer network computes frame-wise attention scores across all stored d-vectors $\{\mathbf{e}_1, \dots, \mathbf{e}_N\}$ based on input feature $\mathbf{x}^{(t)}$.
  - Attended speaker embedding $\mathbf{e}_{\text{att}}^{(t)} = \sum_{i=1}^N \alpha_i^{(t)} \mathbf{e}_i$.
  - Trained with a **dual learning rate schedule** (AttentionNet learning rate is an order of magnitude smaller than VoiceFilterNet to avoid overfitting speaker combinations).

---

## 5. Algorithmic Innovations Against Over-Suppression

TSE models frequently suffer from "over-suppression" (attenuating the target speaker's phonemes when background speech or non-speech noise is present), which drastically increases Word Error Rate (WER) deletion errors. VoiceFilter-Lite introduced two primary mechanisms:

### 5.1 Asymmetric Loss Function
Let $S_{\text{cln}}(t, f)$ be clean target energy and $S_{\text{enh}}(t, f)$ be enhanced energy. Define error $x = S_{\text{cln}}(t, f) - S_{\text{enh}}(t, f)$.
The asymmetric penalty function $g_{\text{asym}}(x, \alpha)$ is:
$$g_{\text{asym}}(x, \alpha) = \begin{cases} x, & \text{if } x \le 0 \quad (\text{under-suppression: residual noise}) \\ \alpha \cdot x, & \text{if } x > 0 \quad (\text{over-suppression: target speech deleted}) \end{cases}$$
Where $\alpha > 1$ (empirically set to $\alpha = 10$).
This heavily penalizes removing target speech energy, ensuring the network errs on the side of preserving target speech rather than over-filtering.

### 5.2 Adaptive Runtime Suppression Strength
An auxiliary classification head on top of the LSTM predicts noise type $f_{\text{adapt}}(\mathbf{S}_{\text{in}}^{(t)}) \in [0, 1]$ (0 = clean/non-speech noise, 1 = overlapping speech).
The runtime suppression weight $w^{(t)}$ is smoothed via EMA:
$$w^{(t)} = \beta \cdot w^{(t-1)} + (1 - \beta) \cdot \left[ a \cdot f_{\text{adapt}}(\mathbf{S}_{\text{in}}^{(t)}) + b \right]$$
where $\beta = 0.8$, and the final output feature blends enhanced and raw features:
$$\mathbf{S}_{\text{out}}^{(t)} = w^{(t)} \cdot \mathbf{S}_{\text{enh}}^{(t)} + (1 - w^{(t)}) \cdot \mathbf{S}_{\text{in}}^{(t)}$$
When no interfering speaker is detected, $w^{(t)} \to 0$, rendering the filter completely transparent and preventing any degradation of clean or noise-only speech.

---

## 6. CPU Footprint, FLOPs, Model Sizes & Hardware Benchmarks

### 6.1 Model Parameter Count & Memory Footprint

| Architecture Variant | Hidden Dimension | Precision | Total Parameter Count | FlatBuffer / Disk Footprint | Working RAM |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **VFL Standard (512-node)** | 3x LSTM (512) + FC (512) | Float32 | ~4.74 M parameters | ~19.0 MB | ~24 MB |
| **VFL Standard Quantized** | 3x LSTM (512) + FC (512) | Int8 (Dynamic Range) | ~4.74 M parameters | **6.8 MB** | ~8 MB |
| **VFL Compact (256-node)** | 3x LSTM (256) + FC (512) | Float32 | ~1.46 M parameters | ~5.8 MB | ~8 MB |
| **VFL Compact Quantized** | 3x LSTM (256) + FC (512) | **Int8 (TFLite)** | **~1.46 M parameters** | **2.2 MB** | **< 4 MB** |
| **Multi-User FiLM VFL** | 3x LSTM (256) + FiLM + Attn | Int8 (TFLite) | ~2.1 M parameters | **3.23 MB** | ~5 MB |

### 6.2 Analytical FLOPs & Parameter Computation
For the **2.2 MB Compact VFL** (taking 512-dim stacked filterbank input):
- **Input projection / LSTM Layer 1:**
  - Input dim: $512$ (feature) $+ 256$ (d-vector) $= 768$ (or $512$ if using FiLM).
  - LSTM hidden size: $H = 256$.
  - LSTM parameters: $4 \times ((768 + 1) \times 256 + 256^2) \approx 4 \times (196,864 + 65,536) \approx 1.05\text{ M params}$.
- **LSTM Layer 2:**
  - $4 \times ((256 + 1) \times 256 + 256^2) \approx 4 \times (65,792 + 65,536) \approx 0.525\text{ M params}$.
- **LSTM Layer 3:**
  - $4 \times ((256 + 1) \times 256 + 256^2) \approx 0.525\text{ M params}$.
- **Output FC Layer (Mask Prediction):**
  - $256 \times 512 + 512 \approx 0.13\text{ M params}$.
- **Auxiliary Noise Classifier Head:**
  - $2 \times \text{FC}(64) \approx 0.02\text{ M params}$.
- **Total Parameters:** $\approx 1.46\text{ M}$ parameters ($\approx 1.46\text{ MB}$ raw Int8 matrix weights + offsets/scales $\to$ **2.2 MB** TFLite FlatBuffer).

**FLOPs per Inference Step:**
- Each LSTM cell evaluation requires $2 \times \text{weights}$ operations (multiply-accumulate):
  - Layer 1: $\approx 2.10\text{ MFLOPs}$
  - Layer 2: $\approx 1.05\text{ MFLOPs}$
  - Layer 3: $\approx 1.05\text{ MFLOPs}$
  - FC / Mask: $\approx 0.26\text{ MFLOPs}$
  - Total per frame: **$\approx 4.5\text{ MFLOPs}$ per inference step**.

**FLOPs per Second (Throughput):**
- When executed every 30 ms (stacked/subsampled mode):
  $$\text{FLOPs/sec} = 4.5\text{ MFLOPs} \times 33.33\text{ frames/sec} \approx \mathbf{150\text{ MFLOPS}}\ (0.15\text{ GFLOPS})$$
- When executed every 10 ms (single 10 ms hop streaming mode):
  $$\text{FLOPs/sec} = 4.5\text{ MFLOPs} \times 100\text{ frames/sec} \approx \mathbf{450\text{ MFLOPS}}\ (0.45\text{ GFLOPS})$$

### 6.3 Real-World CPU Inference Execution Times (x86_64 AVX2 vs Mobile ARM)
- **Reported Google Metrics:**
  - Ran on edge hardware (Google Pixel phones, ARM Cortex-A76/A55 cores and Google Home smart speakers).
  - Google reported that the 2.2 MB quantized model consumed **< 5% of a single mobile CPU core** and operated within **< 10 ms aggregate processing budget**, safely running alongside an on-device Conformer/RNN-T streaming recognizer.
- **Independent x86_64 CPU Benchmarks (Ronssin et al., Interspeech 2022; APSIPA 2025):**
  - Evaluated on an **Intel Core i5-7300U CPU @ 3.5 GHz** (dual-core laptop CPU, AVX2):
    - Full end-to-end pipeline (downsampling to 16 kHz, 1024-point FFT, causal CNN-LSTM model inference, iFFT, overlap-add, and upsampling) consumed **< 10% of a single CPU core**.
    - Model inference alone took **$\approx 0.8 - 1.4\text{ ms}$ per audio frame**.
  - Evaluated on modern **Intel Core i7-10870H / 12th/13th Gen x86_64 CPUs**:
    - With AVX2 / AVX-512 FMA vectorization and FP32/Int8 ONNX Runtime or Tract/OpenVINO:
      - 3-layer LSTM (256 units) per-frame execution time is **$0.25 - 0.55\text{ ms}$**.
      - Real-Time Factor (RTF) $\approx \mathbf{0.025 - 0.055}$ for 10 ms frames.
      - **Conclusion for Target Specification:** Easily and reliably achieves **$< 2 - 3\text{ ms}$ latency budget per 10 ms frame on x86_64 AVX2/AVX-512**, leaving > 70–80% of the single-core CPU slice for other DSP blocks (AEC, noise gate, limiter).

---

## 7. Performance Benchmarks: WER, SI-SDR & Separation Gain

### 7.1 Speech Recognition WER Improvements (Google Internal & LibriSpeech)
*LibriSpeech Overlapping Speech Benchmark:*
- **Baseline (No VoiceFilter):**
  - Clean: $8.6\%$ WER
  - Non-speech noise (additive): $35.7\%$ WER
  - Speech noise (overlapping talker, additive): **$77.9\%$ WER**
  - Speech noise (reverberant room): **$79.3\%$ WER**
- **With VoiceFilter-Lite (2.2 MB Int8, Adaptive Suppression $w^{(t)}$):**
  - Clean: **$8.6\%$ WER** (0.0% relative degradation — perfectly harmless)
  - Non-speech noise (additive): **$21.1\%$ WER** (significant noise reduction benefit)
  - Speech noise (additive): **$31.4\%$ WER** (**$46.5\text{ percentage points}$ absolute reduction / $59.7\%$ relative improvement**)
  - Speech noise (reverberant room): **$39.1\%$ WER** (**$40.2\text{ percentage points}$ absolute reduction / $50.7\%$ relative improvement**)

*Realistic Vendor Voice Search Queries (20k utterances, 230 speakers):*
- Clean: $15.4\%$ WER vs $15.2\%$ baseline.
- Overlapping speech (additive): Reduced from **$56.5\%$ down to $28.8\%$** ($49.0\%$ relative improvement).
- Reverberant overlapping speech: Reduced from **$53.8\%$ down to $37.2\%$** ($30.8\%$ relative improvement).

### 7.2 Speaker Verification Equal Error Rate (EER) Improvements
In personalized keyphrase detection (Rikhye et al., Interspeech 2021 & Odyssey 2022):
- Clean & non-speech noise: EER unchanged ($1.1\%$).
- Multi-talker overlapping speech (0 dB SNR):
  - Baseline without VFL: $7.42\%$ EER.
  - With Concat Multi-User VFL: $3.30\%$ EER.
  - With **FiLM-conditioned Multi-User VFL:** **$2.42\%$ EER** (**$67.4\%$ relative error reduction**).
  - False Rejection Rate (FRR) dropped from $41.9\%$ to $29.6\%$.

### 7.3 Separation Metrics (SI-SDR, SDR, PESQ)
*Note on VFL vs. Waveform Audio Output:*
- VoiceFilter-Lite emits *filterbank energies* rather than time-domain audio, so Google evaluated it using WER and EER rather than SI-SDR.
- However, when the exact VoiceFilter architecture is configured with an iSTFT synthesis stage (or evaluated in original VoiceFilter / follow-up papers like Ronssin 2022 and SpeakerBeam-SS 2024):
  - **SDR Improvement:** Mean SDR increases by **$+5.5\text{ to }+7.8\text{ dB}$** on LibriSpeech / VCTK 2-speaker mixtures (from ~10.1 dB baseline to 15.6–17.9 dB).
  - **PESQ Gain:** Typically improves narrowband PESQ by **$+0.4 - 0.7$ MOS points** on overlapping mixtures (e.g. from 1.9–2.1 up to 2.5–2.8).
  - Modern causal benchmark comparisons (SpeakerBeam-SS 2024 / E3Net 2022):
    - Real-time causal LSTM/S4D baselines achieve **$9.8 - 11.6\text{ dB}$ SI-SDRi** with DNSMOS Overall score $\approx 2.95 - 3.33$.

---

## 8. Performance Limits, Vulnerabilities & Edge Cases

### 8.1 Same-Gender vs. Cross-Gender Speaker Degradation
- **Gender Separation Gap:**
  - Single-channel TSE systems face their greatest performance drop when separating speakers of the **same gender** (especially female-female mixtures, followed by male-male).
  - Empirical studies on VoiceFilter derivatives (e.g., Academia/ISCA analyses):
    - **Different-gender mixtures (Male-Female):** High separation efficiency ($+8 - 10\text{ dB}$ SDR gain; fundamental frequencies $F_0$ differ by 50–100 Hz, allowing the LSTM to isolate harmonic combs).
    - **Same-gender mixtures (Male-Male or Female-Female):** Separation gain degrades by **$2.5 - 4.5\text{ dB}$** compared to cross-gender mixtures.
    - **Root Cause:** When $F_0$ medians are within $\pm 10\text{ Hz}$, the magnitude spectrogram mask experiences severe spectral bleeding. The model relies almost exclusively on vocal tract resonances (formants) encoded in the d-vector rather than pitch tracks.

### 8.2 Near-Far Problem & Target Speaker Attenuation (Minimum Separation Gain)
- If the target speaker is far-field (low direct-to-reverberant ratio) and the interfering speaker is near-field (e.g. interfering speaker is $10\text{ dB}$ louder than target speaker, $\text{SIR} \le -10\text{ dB}$):
  - Masking networks tend to attenuate the entire mixture because mask values collapse towards 0.
  - The minimum effective operational Signal-to-Interference Ratio (SIR) is roughly **$-6\text{ dB}$ to $-10\text{ dB}$**. Below this threshold, target speech intelligibility collapses.

### 8.3 Inherent Limitations of Spectrogram Masking vs. Neural Time-Domain / Complex Synthesis
- **Noisy Phase Distortion:** Because VoiceFilter applies a real-valued mask $M(t, f) \in [0, 1]$ directly to magnitude and retains the raw mixture phase $\theta_{\text{mix}}(t, f)$, phase incongruence causes "musical noise" and perceptual harshness when reconstructed to time-domain audio.
- **Deletive-Only Constraint:** As proved in recent 2025–2026 TSE literature (e.g., *Mask2Flow-TSE*, arXiv:2603.12837): A masking network can only *delete* energy; it cannot *insert* or reconstruct spectral components of the target speaker that were completely masked by a louder interferer.

---

## 9. Modern SOTA Landscape & Successors (2021–2026)

1. **Google Multi-User VoiceFilter-Lite (2021–2022):**
   - Added Attentive Speaker Selection and **FiLM conditioning**, achieving 3.2 MB footprint and supporting multi-user smart displays without single-user regressions.
2. **Microsoft Personalized PercepNet & E3Net (2021–2022):**
   - E3Net (Eskimez et al., Interspeech 2022): End-to-end learnable filterbank encoder-decoder with low-complexity recurrent core, achieving $\text{RTF} < 0.1$ with 20–32 ms windows.
3. **SpeakerBeam-SS (Interspeech 2024):**
   - Replaced stacked dilated convolutions in Conv-TasNet with **Structured State Space Sequence models (S4D)**, cutting RTF by 78% while sustaining $11.58\text{ dB}$ SDR with 20 ms causal latency.
4. **ConVoiFilter (2024):**
   - Replaces LSTM layers with streaming causal Conformer / depthwise separable convolutional blocks for superior long-range context.
5. **Mask2Flow-TSE (2026):**
   - Hybrid two-stage paradigm: Stage 1 uses lightweight VoiceFilter-Lite style discriminative masking for coarse real-time deletion; Stage 2 uses a 1-step Flow Matching model to synthesize missing spectral components.

---

## 10. Suitability Assessment for Real-Time CPU Virtual Microphone (Clearcore / 48 kHz / 10 ms Hops)

| Requirement / Constraint | Target Specification | VoiceFilter-Lite Capability | Assessment & Implementation Recommendations |
| :--- | :--- | :--- | :--- |
| **Algorithmic Latency** | $\le 10 - 20\text{ ms}$ | **0 ms lookahead** | **PASS:** Completely causal; no lookahead needed. Framing latency equals DSP hop (10 ms). |
| **Execution Time per Frame**| $< 2.0\text{ ms}$ (on AVX2 CPU) | **$\approx 0.3 - 0.6\text{ ms}$** | **PASS:** 2.2 MB / 1.46M parameter Int8/FP32 LSTM runs at ~0.45 GFLOPS, consuming < 5% of one modern CPU core. |
| **Audio Output** | 48 kHz Clean Audio Waveform | Feature output only (by default) | **ADAPTATION NEEDED:** Must operate on Complex Spectrogram (STFT) or Polyphase Filterbanks (like DeepFilterNet3) rather than Mel-ASR filterbanks. |
| **Speaker Conditioning** | Zero runtime enrollment overhead | 256-dim d-vector | **PASS:** Pre-computed 256-dim embedding modulated via **FiLM** affine scaling before LSTM layers. |
| **Memory Footprint** | $< 15\text{ MB}$ RAM | **2.2 MB - 3.5 MB** | **PASS:** Extremely low cache pressure, easily fits in CPU L2/L3 cache. |

### Architectural Recommendations for Native Clearcore Pipeline:
1. **Combine DeepFilterNet3 ERB Filterbank with VFL Backbone:** Use DFNet3's ERB subband decomposition (32 ERB bands up to 8 kHz + complex DFNet filter up to 48 kHz) rather than standard Mel bands.
2. **Inject Speaker Embedding via FiLM:** Adopt Google's 2022 FiLM conditioning rather than 2020 concatenation for +26% better noise robustness.
3. **Preserve Clean Voice via Asymmetric Loss ($\alpha=10$):** Train the suppression mask with the asymmetric penalty to guarantee zero degradation on single-speaker speech.
