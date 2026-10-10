# MossFormer2 (and Causal/Streaming Variants) for Speaker Separation / Target Speaker Extraction on CPU

## What is MossFormer2? (Identity, Origin, Parameters, FLOPs)

### Takeaway
MossFormer2 is a hybrid Transformer and RNN-free recurrent architecture for speech separation and enhancement published by Alibaba Group at ICASSP 2024, scaling from 37.8M to 55.7M parameters with ~84.2 GMAC/s (168.4 GFLOPs/s) for time-domain speech separation.

### Cited Findings
- **Title and Citation**: "MossFormer2: Combining Transformer and RNN-Free Recurrent Network for Enhanced Time-Domain Monaural Speech Separation" — [IEEE Xplore](https://ieeexplore.ieee.org/document/10445985); [arXiv:2312.11825](https://arxiv.org/abs/2312.11825).
- **Authors and Affiliation**: Shengkui Zhao, Yukun Ma, Chongjia Ni, Chong Zhang, Hao Wang, Trung Hieu Nguyen, Kun Zhou, Jia Qi Yip, Dianwen Ng, Bin Ma (Alibaba Group, Singapore) — [ICASSP 2024](https://cmsworkshops.com/ICASSP2024/view_paper.php?PaperNum=2707).
- **Venue and Timeline**: Published in IEEE ICASSP 2024 (Seoul, Korea, April 14–19, 2024); initial preprint submitted December 2023, revised November 2024 — [arXiv](https://arxiv.org/abs/2312.11825).
- **Predecessor**: Preceded by "MossFormer: Pushing the Performance Limit of Monaural Speech Separation using Gated Single-head Transformer with Convolution-augmented Joint Self-Attentions" by Shengkui Zhao and Bin Ma — [ICASSP 2023 / arXiv:2302.11824](https://arxiv.org/abs/2302.11824).
- **Parameter Count**:
  - `MossFormer2 (S)`: 37.8M parameters ($R=25$ blocks, $N=384$ embedding dim, $N'=256$ bottleneck dim, $L=2$ FSMN depth) — [arXiv:2312.11825 Table 1](https://arxiv.org/html/2312.11825).
  - `MossFormer2 (L)`: 55.7M parameters ($R=24$ blocks, $N=512$ embedding dim, $N'=256$ bottleneck dim, $L=2$ FSMN depth) — [arXiv:2312.11825 Table 1](https://arxiv.org/html/2312.11825).
  - `AV_MossFormer2_TSE_16K` (audio-visual target speaker extraction): ~55.9M to 68.5M parameters including visual embedding fusion layers — [ICLR 2026 AVSS paper](https://proceedings.iclr.cc/paper_files/paper/2026/file/de1739eba209c682a90ec3669229ab2d-Paper-Conference.pdf).
- **Computational Intensity (GMACs / FLOPs)**:
  - On WSJ0-2mix (8 kHz time-domain), MossFormer2 requires 84.2 GMAC/s (equivalent to ~168.4 GFLOPs per second of audio) — [SepMamba Paper / arXiv:2410.20997](https://arxiv.org/html/2410.20997v1).
  - Preceding MossFormer(L) required 70.4 GMAC/s (~140.8 GFLOPs/s) — [SepMamba Paper / arXiv:2410.20997](https://arxiv.org/html/2410.20997v1).

### Inferences
- The parameter scale (37.8M–55.7M) and compute intensity (84.2 GMAC/s on 8 kHz) place MossFormer2 firmly in the heavy cloud/GPU model category, rather than lightweight edge audio models (such as DeepFilterNet3 at ~1.8M–2.3M parameters and < 0.5 GMAC/s).

### Gaps
- Exact FLOP counts for full-band 48 kHz time-domain processing are not explicitly tabulated in the ICASSP 2024 paper because the primary speech separation evaluation was conducted at 8 kHz and 16 kHz.

---

## Architecture: Attention Mechanism, Recurrent FSMN Module, and Sequences

### Takeaway
MossFormer2 combines a Gated Single-Head Transformer (GSHT / FLASH-style) employing joint local full self-attention within non-overlapping chunks and linearized attention across chunks, coupled with an RNN-free Dilated Feedforward Sequential Memory Network (DFSMN) and Gated Convolutional Units (GCU).

### Cited Findings
- **Encoder-Separator-Decoder Framework**: Time-domain implementation uses a 1D Conv encoder (kernel $K_1=16$, stride $K_1/2=8$) and ConvTranspose1D decoder, encoding audio into feature embeddings $X \in \mathbb{R}^{N \times S}$ — [arXiv:2312.11825 Section 2.1](https://arxiv.org/html/2312.11825).
- **Gated Attention (FLASH_ShareA_FFConvM)**: Adapted from FLASH (U-Transformer), replacing multi-head attention with a single-head gated self-attention augmented with rotary positional embeddings (RoPE) — [ClearerVoice-Studio Codebase](https://raw.githubusercontent.com/modelscope/ClearerVoice-Studio/main/clearvoice/clearvoice/models/mossformer2_se/mossformer2_block.py); [arXiv:2302.11824 Section 2](https://arxiv.org/abs/2302.11824).
- **Joint Local and Global Attention**:
  - *Local Attention*: Partitions the sequence into non-overlapping groups/chunks of size $P$ (group size $g=256$). Computes quadratic full attention locally per chunk via $V'_{local, h} = \text{ReLU}^2(\gamma Q_h K_h^T) V_h$ — [arXiv:2302.11824 Eq. 8](https://arxiv.org/abs/2302.11824).
  - *Global Attention*: Computes linearized cross-sequence attention across all chunks via $V'_{global} = Q' (\beta K'^T V)$ where $\beta = 1/S$ — [arXiv:2302.11824 Eq. 7](https://arxiv.org/abs/2302.11824); [ClearerVoice-Studio mossformer2_block.py](https://raw.githubusercontent.com/modelscope/ClearerVoice-Studio/main/clearvoice/clearvoice/models/mossformer2_se/mossformer2_block.py).
- **RNN-Free Recurrent Module**:
  - Employs a Dilated Feedforward Sequential Memory Network (FSMN) with Gated Convolutional Units (GCU), dense connections, and bottleneck projection ($N \to N'$ where $512 \to 256$) — [arXiv:2312.11825 Section 2.3](https://arxiv.org/html/2312.11825).
  - Convolutions use 2D grouping and dilation factors $2^0, 2^1, \dots, 2^{L-1}$ with filter order $l_{order}=20$ — [ClearerVoice-Studio fsmn.py](https://raw.githubusercontent.com/modelscope/ClearerVoice-Studio/main/clearvoice/clearvoice/models/mossformer2_se/fsmn.py).
  - Operates purely feedforward without recurrent hidden states, allowing complete parallelization across all frames during training and batch inference — [arXiv:2312.11825 Section 1 & 2.3](https://arxiv.org/html/2312.11825).

### Inferences
- While standard dual-path models (like DPRNN, SepFormer) iterate alternatively between intra-chunk and inter-chunk processing, MossFormer2 fuses local quadratic chunk attention and global linear attention within each single transformer block.
- The default chunk size $g=256$ in frame embedding units (at stride 8 samples / 0.5 ms per frame at 16 kHz) represents 128 ms of audio per local chunk.

### Gaps
- None regarding the mathematical formulation of the non-causal attention and FSMN layers.

---

## Causal / Streaming MossFormer2 Variants and Lookahead Constraints

### Takeaway
No native frame-by-frame causal (lookahead $\le 20$ ms) checkpoint of MossFormer2 has been released in ClearerVoice-Studio or literature; existing "streaming" or "online" implementations rely strictly on sliding windows with 3 to 4 seconds of buffer latency.

### Cited Findings
- **Official Model Releases**: The official repository ClearerVoice-Studio classifies `MossFormer2_SS_16K`, `MossFormer2_SE_48K`, and `AV_MossFormer2_TSE_16K` as **(Non-causal)** models — [ClearerVoice-Studio TSE Documentation](https://github.com/modelscope/clearervoice-studio/blob/main/train/target_speaker_extraction/README.md).
- **Sliding-Window Pseudo-Streaming**:
  - In `ClearerVoice-Studio/clearvoice/utils/decode.py`, "online decoding" for MossFormer2 does not mean frame-by-frame causal processing. It is implemented as a sliding block window with `decode_window = 3` to `4` seconds and `stride = 0.75 * window` (3.0s window / 2.25s stride, or 4.0s window / 3.0s stride) — [ClearerVoice-Studio decode.py lines 62-115, 237-300](https://raw.githubusercontent.com/modelscope/ClearerVoice-Studio/main/clearvoice/clearvoice/utils/decode.py).
  - An edge discard ("give-up length") of $(window - stride) / 2 = 375$ ms to $500$ ms is discarded at each boundary to mask edge artifacts — [ClearerVoice-Studio decode.py line 253](https://raw.githubusercontent.com/modelscope/ClearerVoice-Studio/main/clearvoice/clearvoice/utils/decode.py); [docs.rs denoize backend/mossformer2.rs](https://docs.rs/denoize/latest/src/denoize/backend/mossformer2.rs.html).
- **Causal Code Hook in Attention**:
  - The internal attention code in `mossformer2_block.py` contains a boolean flag `causal=False`:
    - When `causal=True`, local attention applies an upper-triangular causal mask `torch.ones((g, g), dtype=torch.bool).triu(1)`.
    - Global linear attention replaces full sequence reduction with cumulative sum `lin_kv.cumsum(dim=1)` padded with `(0, 0, 0, 0, 1, -1)` — [ClearerVoice-Studio mossformer2_block.py lines 320-335](https://raw.githubusercontent.com/modelscope/ClearerVoice-Studio/main/clearvoice/clearvoice/models/mossformer2_se/mossformer2_block.py).
- **FSMN Lookahead Barrier**:
  - The Dilated FSMN module in `fsmn.py` pads symmetrically: `pad_length = lorder + (dil - 1) * (lorder - 1) - 1` applied as symmetric padding `nn.ConstantPad2d((0, 0, pad_length, pad_length), value=0.)` with $l_{order}=20$ and dilations up to $2^3=8$ — [ClearerVoice-Studio fsmn.py lines 118-124](https://raw.githubusercontent.com/modelscope/ClearerVoice-Studio/main/clearvoice/clearvoice/models/mossformer2_se/fsmn.py).
  - This symmetric receptive field looks forward into future frames by $(20 - 1) \times 8 \approx 152$ frames (~76 ms to 1.5 s depending on the time scale), making the pre-trained FSMN non-causal.

### Inferences
- To create a truly causal MossFormer2 with $\le 20$ ms lookahead:
  1. The attention module must enable `causal=True` (triangular mask + cumulative sum KV cache).
  2. The Dilated FSMN memory layer must switch from symmetric padding to asymmetric causal padding `(0, 0, 2 * pad_length, 0)` with lookahead order limited to $r_{order} \le 2$ frames.
  3. Conv-TasNet style encoder stride would need to be aligned with the 10 ms hop requirement.
- Such a causal variant would suffer significant degradation in SI-SDRi (typically 2.5–4.5 dB drop from non-causal, consistent with findings in causal Conv-TasNet vs non-causal Conv-TasNet).

### Gaps
- Alibaba has not trained or released weights for the `causal=True` branch of MossFormer2.

---

## Benchmark Performance: Separation & Extraction Quality

### Takeaway
Non-causal MossFormer2 achieves state-of-the-art separation performance on academic benchmarks (24.1 dB SI-SDRi on WSJ0-2mix, 21.7 dB on Libri2Mix), slightly outperforming SepFormer and competitive with TF-GridNet.

### Cited Findings
- **WSJ0-2mix (8 kHz Clean)**:
  - `MossFormer2`: **24.1 dB** SI-SDRi (55.7M parameters) — [arXiv:2312.11825 Table 2](https://arxiv.org/html/2312.11825).
  - `MossFormer2 (S)`: **23.2 dB** SI-SDRi (37.8M parameters) — [arXiv:2312.11825 Table 1](https://arxiv.org/html/2312.11825).
  - `MossFormer (L)`: 22.8 dB SI-SDRi (42.1M parameters) — [arXiv:2302.11824 Table 2](https://arxiv.org/abs/2302.11824).
  - `SepFormer + DM`: 22.3 dB SI-SDRi (25.7M parameters) — [arXiv:2312.11825 Table 2](https://arxiv.org/html/2312.11825).
  - `TF-GridNet`: 23.4 dB to 23.6 dB SI-SDRi (14.4M parameters) — [Wang et al., IEEE/ACM TASLP 2023](https://arxiv.org/abs/2209.03952).
  - `DPMamba-L` (2025): 24.4 dB SI-SNRi (surpassing MossFormer2) — [arXiv:2508.14556](https://arxiv.org/pdf/2508.14556).
- **Libri2Mix (Clean)**:
  - `MossFormer2`: **21.7 dB** SI-SDRi — [arXiv:2312.11825 Table 2](https://arxiv.org/html/2312.11825).
  - `SepFormer + DM`: 19.2 dB SI-SDRi — [arXiv:2312.11825 Table 2](https://arxiv.org/html/2312.11825).
  - `SFSRNet`: 20.4 dB SI-SDRi — [arXiv:2312.11825 Table 2](https://arxiv.org/html/2312.11825).
  - `TF-GridNet`: 20.1 dB SI-SDRi — [SOTA2 Research Libri2Mix Leaderboard](https://www.sota2.com/research/sota/speech-separation-on-libri2mix-test).
- **WHAM! / WHAMR! (Noisy & Reverberant)**:
  - `MossFormer2` on WHAM!: **18.1 dB** SI-SDRi (vs SepFormer 16.4 dB) — [arXiv:2312.11825 Table 3](https://arxiv.org/html/2312.11825).
  - `MossFormer2` on WHAMR!: **17.0 dB** SI-SDRi (vs SepFormer 14.0 dB) — [arXiv:2312.11825 Table 3](https://arxiv.org/html/2312.11825).
- **Target Speaker Extraction (Audio-Visual AV-MossFormer2)**:
  - VoxCeleb2 2-mix: 14.6 dB SI-SDRi / 14.9 dB SDRi — [ClearerVoice-Studio TSE Leaderboard](https://github.com/modelscope/clearervoice-studio/blob/main/train/target_speaker_extraction/README.md).
  - VoxCeleb2 3-mix: 15.5 dB SI-SDRi / 16.0 dB SDRi — [ClearerVoice-Studio TSE Leaderboard](https://github.com/modelscope/clearervoice-studio/blob/main/train/target_speaker_extraction/README.md).
  - LRS2 2-mix: 15.5 dB SI-SDRi (vs AV-TFGridNet 15.1 dB, AV-DPRNN 12.0 dB) — [ClearerVoice-Studio TSE Leaderboard](https://github.com/modelscope/clearervoice-studio/blob/main/train/target_speaker_extraction/README.md).

### Inferences
- In non-causal offline evaluation, MossFormer2 provides outstanding separation fidelity, outperforming SepFormer and matching or slightly exceeding base TF-GridNet on clean benchmarks.

### Gaps
- Published causal SI-SDR numbers on Libri2Mix or WSJ0-2mix do not exist for MossFormer2 because a trained causal checkpoint was never published.

---

## CPU Feasibility Evaluation (< 10 ms Latency per 10 ms Frame on x86_64 AVX2/AVX-512)

### Takeaway
MossFormer2 **CANNOT** run on an x86_64 CPU within a < 10 ms latency budget per 10 ms audio frame (budget violation of approximately 8× to 26× on AVX2/AVX-512); attention is not the primary bottleneck—the overall 55.7M parameter depth and 84–120 GMAC/s workload make real-time CPU execution impossible.

### Cited Findings
- **Empirical CPU Benchmark Evidence**:
  - `Audio-Denoiser-ONNX` benchmark on Intel Core i7-1165G7 laptop CPU: `MossFormer2_SS_16K` in FP32 achieved a Real-Time Factor (RTF) of **2.63** (meaning 1 second of audio takes 2.63 seconds to process on CPU) — [DakeQQ/Audio-Denoiser-ONNX Benchmark Table](https://github.com/DakeQQ/Audio-Denoiser-ONNX).
  - `MossFormerGAN_SE_16K` on desktop Intel Core i3-12300 CPU: RTF of **1.085** (standard CPU) and **0.95** (OpenVINO CPU) — [DakeQQ/Audio-Denoiser-ONNX Benchmark Table](https://github.com/DakeQQ/Audio-Denoiser-ONNX).
  - `MossFormer2_SE_48K` (mel-masking network) on Intel Core i3-12300 CPU: RTF of **0.09** when processing 1000 ms chunks — [DakeQQ/Audio-Denoiser-ONNX Benchmark Table](https://github.com/DakeQQ/Audio-Denoiser-ONNX). However, this is for 1-second bulk inference, not 10 ms streaming frames.
  - Python Audio Super Resolution benchmark on Colab CPU: 0.25 seconds of audio took **4.1 to 5.2 seconds** for initial model inference on CPU (RTF > 16.0) — [Tinnci/python-audio-super-resolution BACKEND_CANDIDATES.md](https://github.com/Tinnci/python-audio-super-resolution/blob/main/docs/BACKEND_CANDIDATES.md).
- **Arithmetic Analysis for 10 ms Audio Frame (48 kHz / 480 samples)**:
  - Time budget: 10 ms real-time per hop; target execution: $< 3$ ms (to leave headroom for system audio pipeline).
  - Compute per second: MossFormer2 requires 84.2 GMAC/s at 8 kHz (~168.4 GFLOPs/s). At 16 kHz or 48 kHz time-domain, compute scales linearly with sequence length unless hop size is drastically increased.
  - If frame step is 10 ms (100 frames/sec), processing 1 frame requires:
    $$\frac{84.2 \text{ GMAC/s}}{100 \text{ frames/s}} = 0.842 \text{ GMAC} = 1.684 \text{ GFLOPs per 10 ms frame}$$
  - A modern x86_64 CPU core (e.g. Zen 4 or Intel Raptor Lake @ 4.0 GHz) with AVX2 executes 16 single-precision FLOPs/cycle (two 256-bit FMAs). Peak theoretical throughput per core = $4.0 \text{ GHz} \times 16 = 64 \text{ GFLOPs/s}$.
  - Real-world sustained efficiency for deep neural networks on CPU with memory traffic is typically 20% to 35% of peak (~15 to 22 GFLOPs/s per single core).
  - Minimum execution time on 1 CPU core for 1.684 GFLOPs:
    $$\text{Time} = \frac{1.684 \text{ GFLOPs}}{20 \text{ GFLOPs/s}} \approx 84.2 \text{ ms}$$
  - 84.2 ms latency for a 10 ms frame represents an RTF of **8.42×**, exceeding the 10 ms deadline by over $800\%$. Even across 4 multi-threaded AVX-512 cores running at 100% saturation, latency would remain around $20\text{--}25$ ms.
- **Attention vs. Feedforward Computational Breakdown**:
  - In MossFormer2, attention within a 10-frame chunk ($g=10$, $d=128$) is trivial: $10 \times 10 \times 128 = 12,800$ FLOPs per layer.
  - The dominant compute in MossFormer2 is **NOT** the attention matrix multiply ($O(n^2)$), but the **dense feedforward projections and convolutions across 24 blocks**:
    - Linear projections ($512 \to 2048 \to 512$): $24 \times 2 \times 512 \times 2048 \times 2 = 100.6 \text{ MFLOPs}$ per frame.
    - Dilated FSMN and GCU convolutions ($24 \times 2$ branches): dense channel weights.
    - Memory footprint: 55.7M weights in FP32 = 222.8 MB; in INT8 = 55.7 MB. Streaming 55.7 MB of weights through CPU L3 cache (typically 16–32 MB) every 10 ms frame requires $5.57 \text{ GB/s} \times 100 = 557 \text{ GB/s}$ of memory bandwidth, exceeding modern DDR5 dual-channel memory bandwidth (~80 GB/s) by 7×!

### Inferences
- The attention mechanism itself easily runs in $< 0.1$ ms for a small chunk. The bottleneck that makes real-time CPU streaming impossible is the **sheer weight volume (55.7M parameters) and feedforward arithmetic/memory bandwidth**.
- For comparison, real-time CPU streaming models like DeepFilterNet3 use 1.8M–2.3M parameters and require $< 0.5$ GMAC/s, achieving an RTF of $\approx 0.05$ on CPU.

### Gaps
- No vendor or research lab has demonstrated a real-time 10 ms frame-synchronous x86 CPU pipeline for MossFormer2.

---

## Comparison: MossFormer2 vs. TF-GridNet vs. BSRNN

### Takeaway
MossFormer2 delivers separation quality comparable to TF-GridNet and BSRNN on clean mixtures, but requires 4× the parameters of TF-GridNet and is completely unsuited for frame-level CPU streaming compared to compact sub-band models.

### Cited Findings
- **Separation Quality (SI-SDRi / SDRi on WSJ0-2mix)**:
  - `MossFormer2`: **24.1 dB** SI-SDRi — [arXiv:2312.11825](https://arxiv.org/abs/2312.11825).
  - `TF-GridNet`: **23.4 dB to 23.6 dB** SI-SDRi (arXiv:2209.03952 / TASLP 2023) — [Wang et al., 2023](https://arxiv.org/abs/2209.03952).
  - `BSRNN` (Band-Split RNN): **21.4 dB to 22.0 dB** SI-SDRi on WSJ0-2mix (Luo & Yu, 2023) — [SonicSim Benchmark / ICLR 2025](https://proceedings.iclr.cc/paper_files/paper/2025/file/a8633d27d782f66fe660c2fb4bae446e-Paper-Conference.pdf).
- **Model Size (Parameter Count)**:
  - `MossFormer2`: **55.7M** parameters — [arXiv:2312.11825](https://arxiv.org/abs/2312.11825).
  - `TF-GridNet`: **14.4M** parameters (~3.8× smaller than MossFormer2) — [Wang et al., 2023](https://arxiv.org/abs/2209.03952); [ResearchGate 2024](https://www.researchgate.net/publication/379818619_MossFormer2_Combining_Transformer_and_RNN-Free_Recurrent_Network_for_Enhanced_Time-Domain_Monaural_Speech_Separation).
  - `BSRNN`: **12.5M to 21.8M** parameters (depending on sub-band configuration) — [Luo & Yu, 2023](https://arxiv.org/abs/2209.03952).
- **Computational Cost & Real-Time Viability**:
  - `TF-GridNet`: Requires 445 GMAC/s (due to bidirectional LSTM scanning across both time and frequency axes) — [ResearchGate 2024](https://www.researchgate.net/publication/379818619_MossFormer2_Combining_Transformer_and_RNN-Free_Recurrent_Network_for_Enhanced_Time-Domain_Monaural_Speech_Separation). It is even heavier computationally per second than MossFormer2 despite having fewer parameters.
  - `BSRNN`: Employs dual-path RNNs (intra-band and inter-band). In its non-causal version, bidirectional LSTMs prevent streaming. In causal variants (unidirectional LSTM), BSRNN has been adapted for real-time operation, but requires significant CPU resources.
  - `MossFormer2`: Requires 84.2 GMAC/s. While lighter in GMAC/s than TF-GridNet on 8 kHz, its 55.7M weights overwhelm CPU cache.
- **Architectural Domain**:
  - `MossFormer2`: Time-domain (or STFT magnitude/FBank masking), 1D convolutional front-end, Transformer + FSMN separator.
  - `TF-GridNet`: Time-frequency domain (complex STFT spectrum), 2D grid processing combining sub-band full-band BLSTMs and self-attention.
  - `BSRNN`: Band-split time-frequency domain, dividing spectrum into non-uniform psychoacoustic sub-bands and processing with interleaved RNNs.

### Inferences
- Neither full MossFormer2 nor full TF-GridNet is viable for real-time $< 10$ ms streaming on CPU. Both were engineered as high-capacity offline/cloud models to push leaderboard benchmarks on benchmark datasets.
- If real-time streaming target speaker extraction / separation is required on CPU, architectures designed from inception with sub-10M (or sub-3M) parameters—such as causal Band-Split RNNs, causal Conformer/FastMN, or DeepFilterNet-style architectures—are the only viable routes.

### Gaps
- A direct head-to-head comparison of causalized MossFormer2 vs. causal BSRNN under identical low-latency constraints ($\le 20$ ms lookahead) does not exist because no causal MossFormer2 checkpoint has been evaluated in the literature.
