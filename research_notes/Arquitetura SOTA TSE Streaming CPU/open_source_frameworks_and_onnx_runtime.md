# Open-Source TSE Frameworks, Pre-trained Weights & ONNX Runtime Streaming Patterns

> **Research date:** 2026-10-09  
> **Scope:** Target Speaker Extraction (TSE) architectures for ClearCore's streaming CPU deployment  
> **Context:** ClearCore uses `#![forbid(unsafe_code)]`, zero-allocation realtime callbacks, replacing Tract with `ort` (ONNX Runtime Rust wrapper)

---

## Table of Contents

1. [Open-Source Repositories & Implementations](#1-open-source-repositories--implementations)
2. [Licensing Comparison Matrix](#2-licensing-comparison-matrix)
3. [Pre-trained Weights & Sample Rates](#3-pre-trained-weights--sample-rates)
4. [ONNX Export & Streaming State Management](#4-onnx-export--streaming-state-management)
5. [ONNX Runtime Rust (`ort`) Streaming Execution](#5-onnx-runtime-rust-ort-streaming-execution)
6. [Zero-Allocation Streaming Pattern in Rust](#6-zero-allocation-streaming-pattern-in-rust)
7. [Quantization & CPU Acceleration](#7-quantization--cpu-acceleration)
8. [Recommended Starting Point for ClearCore](#8-recommended-starting-point-for-clearcore)

---

## 1. Open-Source Repositories & Implementations

### 1.1 ClearerVoice-Studio / MossFormer / FRCRN / DFX

**Repository:** https://github.com/modelscope/ClearerVoice-Studio  
**License:** Apache 2.0  
**Stars:** ~2.8K | **Forks:** 200+  

ClearerVoice-Studio is Alibaba's unified speech processing toolkit covering speech enhancement, separation, super-resolution, and target speaker extraction. It is the most directly relevant open-source TSE framework for ClearCore's use case.

**Key TSE Models:**

| Model | Task | Sample Rate | Architecture | Causal? |
|-------|------|-------------|--------------|---------|
| `AV_MossFormer2_TSE_16K` | Audio-visual TSE | 16 kHz | MossFormer2 + visual encoder | **No** (non-causal) |
| SpEx+ (replicated) | Audio-only TSE | 8 kHz | Shared encoder + ResBlock | Non-causal |
| AV-TFGridNet | Face-conditioned TSE | 16 kHz | TF-GridNet + visual | Non-causal |
| AV-ConvTasNet | Face-conditioned TSE | 16 kHz | Conv-TasNet + visual | **Causal variant available** |
| AV-DPRNN (USEV) | Face-conditioned TSE | 16 kHz | Dual-path RNN + visual | Non-causal |
| NeuroHeed | EEG-conditioned TSE | 16 kHz | EEG encoder | Non-causal |

**Training scripts** available for: AV-ConvTasNet (Causal/Non-causal), AV-DPRNN, AV-TFGridNet, AV-TFGridNet-ISAM, AV-MossFormer2.

**Key observations:**
- Most TSE models are **non-causal** (full-sequence attention). Only AV-ConvTasNet has a causal variant.
- MossFormer2 uses joint local-global self-attention + FSMN recurrent module (RNN-free recurrence).
- Pre-trained weights auto-fetched from HuggingFace or ModelScope.
- FRCRN (16 kHz) and MossFormer2 (48 kHz) are speech enhancement models with 3M+ and 2.5M+ uses respectively.
- The `AV_MossFormer2_TSE_16K` model requires video input (face/lip), making it unsuitable for audio-only ClearCore.

**Relevant for ClearCore:** The framework provides training scripts and the MossFormer2 backbone, but the TSE models are audio-visual, not audio-only. The SpEx+ replication (audio-only, 8 kHz) is the closest match but is low sample rate.

### 1.2 ModelScope 3D-Speaker / FunASR (CAM++, ERes2NetV2)

**Repository:** https://github.com/modelscope/3D-Speaker  
**License:** Apache 2.0 (toolkit); model weights have separate licenses  
**Repository:** https://github.com/modelscope/FunASR  
**License:** MIT (toolkit); model weights licensed separately

**Speaker Embedding Models (for enrollment in TSE):**

| Model | Params | VoxCeleb1-O EER | Sample Rate | Use Case |
|-------|--------|-----------------|-------------|----------|
| CAM++ | 7.2 M | 0.65% | 16 kHz | Speaker embedding extraction |
| ERes2NetV2 | 17.8 M | 0.61% | 16 kHz | Speaker embedding extraction |
| ERes2Net-base | 6.61 M | 0.84% | 16 kHz | Speaker embedding extraction |
| ECAPA-TDNN | 20.8 M | 0.86% | 16 kHz | Speaker embedding extraction |

**Key observations:**
- 3D-Speaker and FunASR provide **speaker embedding models**, not full TSE pipelines. They would be used in the *enrollment* branch of a TSE system.
- CAM++ (7.2M params) is the most compact and widely used — embedded in FunASR pipeline as `spk_model`.
- FunASR's `AutoModel` supports a full pipeline: VAD (FSMN-VAD) + speaker embedding (CAM++) + ASR (Paraformer).
- Model weights on ModelScope/HuggingFace have **separate licenses** from the MIT toolkit. Check each model card.
- All models operate at **16 kHz**.

**Relevant for ClearCore:** CAM++ or ERes2NetV2 as the enrollment speaker encoder. Both are small enough (7–18M params) for CPU realtime use, and export cleanly to ONNX (pure conv/dense layers, no recurrence).

### 1.3 SpeechBrain (SepFormer, ECAPA, dynamic TSE recipes)

**Repository:** https://github.com/speechbrain/speechbrain  
**License:** **Apache 2.0**  
**Stars:** ~11.7K | **Forks:** ~1.7K

**TSE-Relevant Models:**

| Model | Task | Sample Rate | Pre-trained? | Architecture |
|-------|------|-------------|--------------|--------------|
| SepFormer (WSJ0-2Mix) | Speech separation | 8 kHz | ✅ HuggingFace | Dual-path Transformer |
| SepFormer (Libri2Mix) | Speech separation | 8 kHz | ✅ HuggingFace | Dual-path Transformer |
| SepFormer (WHAMR!) | Separation + noise + reverb | 8 kHz | ✅ HuggingFace | Dual-path Transformer |
| ECAPA-TDNN | Speaker embedding | 16 kHz | ✅ HuggingFace | TDNN + attentive pooling |
| RE-SepFormer | Speech separation | 8 kHz | ✅ HuggingFace | Dual-path Transformer |

**Key observations:**
- SpeechBrain provides **separation** models, not true TSE. To build TSE: extract speaker embedding (ECAPA) → separate (SepFormer) → select by cosine similarity.
- This "separate-then-select" approach is inherently **offline** (process full mixture) or requires chunked separation.
- **Dynamic mixing** support for training data augmentation (LibriMix, WSJ0Mix).
- No native streaming/causal SepFormer variant in the main repo.
- Apache 2.0 — fully permissive for commercial use.

**TSE composition example** (from SpeechBrain community):
```python
# 1. Enrollment: extract speaker embedding
classifier = EncoderClassifier.from_hparams(
    source="speechbrain/spkrec-ecapa-voxceleb")
enroll_emb = classifier.encode_batch(enroll_audio)

# 2. Separate mixture into streams
separator = SepformerSeparation.from_hparams(
    source="speechbrain/sepformer-whamr")
streams = separator.separate_batch(mixture)  # [batch, samples, 2]

# 3. Select target stream by cosine similarity
score1 = F.cosine_similarity(enroll_emb, emb1)
score2 = F.cosine_similarity(enroll_emb, emb2)
target = streams[0] if score1 > score2 else streams[1]
```

**Relevant for ClearCore:** ECAPA-TDNN is an excellent enrollment encoder (192-dim, well-tested, ONNX-exportable). SepFormer separation + selection is a fallback approach but not ideal for streaming TSE. SpeechBrain's training recipes are a good reference for data pipeline design.

### 1.4 Asteroid / ESPnet (SpEx+, BSRNN, TF-GridNet recipes)

**Asteroid:** https://github.com/asteroid-team/asteroid  
**License:** MIT  
**Stars:** ~4K

**ESPnet:** https://github.com/espnet/espnet  
**License:** Apache 2.0  
**Stars:** ~7.5K

**TSE-Relevant Models:**

| Model | Toolkit | Sample Rate | Architecture | Streaming? |
|-------|---------|-------------|--------------|------------|
| SpEx+ | Original: GPL-3.0; ESPnet: Apache 2.0 | 8 kHz | Shared encoder + ResBlock | No |
| SpeakerBeam | Asteroid-based | 8 kHz | TDNN + mask estimation | No |
| TF-GridNet | ESPnet | 8 kHz | Full/sub-band LSTM + attention | No |
| BSRNN | ESPnet | 8 kHz | Band-split RNN | No |
| TDSpeakerBeam | ESPnet | 8 kHz | Time-domain SpeakerBeam | No |

**SpEx+ Original:** https://github.com/xuchenglin28/speaker_extraction_SpEx  
**License:** **GPL-3.0** (restrictive — copyleft!)

**ESPnet TSE architecture:**
- `ESPnetExtractionModel`: Encoder → Extractor (TDSpeakerBeam) → Decoder
- `TSEPreprocessor`: handles enrollment audio, dynamic mixing
- Supports `load_spk_embedding: bool` — can use pre-computed embeddings
- `sample_rate` default: **8000 Hz**
- `enh_tse.py` task: full training pipeline with enrollment references

**Key observations:**
- **Asteroid** is MIT-licensed but primarily for source separation (not TSE). SpeakerBeam recipe exists but is based on the Asteroid toolkit.
- **ESPnet** has the most complete TSE training pipeline (`espnet_model_tse.py`) with enrollment handling, but models are non-causal and 8 kHz.
- **TF-GridNet** achieves SOTA separation quality but uses full-sequence bidirectional LSTM — fundamentally non-streaming.
- SpEx+ original is GPL-3.0 (problematic for commercial use), but ESPnet's Apache-2.0 reimplementation avoids this.
- X-TF-GridNet (Information Fusion 2024): 7.79M params, PESQ 3.70, SI-SDR 19.7 dB on TSE benchmark.

**Relevant for ClearCore:** ESPnet's TSE framework design (encoder-extractor-decoder with enrollment) is a good architectural reference. The TDSpeakerBeam extractor pattern (time-domain speaker-conditioned mask estimation) could inform ClearCore's design. GPL-3.0 SpEx+ should be avoided for the training code.

---

## 2. Licensing Comparison Matrix

| Framework | Toolkit License | Model Weights License | Commercial Use | GPL Risk |
|-----------|----------------|----------------------|----------------|----------|
| **ClearerVoice-Studio** | Apache 2.0 | Apache 2.0 (via ModelScope/HF) | ✅ Yes | None |
| **3D-Speaker** | Apache 2.0 | Check model card (usually Apache 2.0) | ✅ Yes (verify weights) | None |
| **FunASR** | MIT | Separate per model (check card) | ✅ Yes (verify weights) | None |
| **SpeechBrain** | Apache 2.0 | Apache 2.0 | ✅ Yes | None |
| **Asteroid** | MIT | MIT (where published) | ✅ Yes | None |
| **ESPnet** | Apache 2.0 | Apache 2.0 | ✅ Yes | None |
| **SpEx+ (original)** | **GPL-3.0** | GPL-3.0 | ⚠️ Copyleft | **HIGH** |
| **SpeakerBeam (BUT)** | Other (non-standard) | Other | ⚠️ Verify | Medium |
| **DeepFilterNet** | MIT / Apache 2.0 | MIT / Apache 2.0 | ✅ Yes | None |
| **deepfilter-rt** | MIT / Apache 2.0 | MIT / Apache 2.0 | ✅ Yes | None |

**Recommendation:** ClearerVoice-Studio, SpeechBrain, ESPnet, and FunASR are all permissively licensed. **Avoid SpEx+ original code (GPL-3.0)** — use ESPnet's implementation instead if needed.

---

## 3. Pre-trained Weights & Sample Rates

| Model | Sample Rate | Weights Available | Source | Params | Task |
|-------|-------------|-------------------|--------|--------|------|
| MossFormer2_SS_16K | 16 kHz | ✅ | HuggingFace + ModelScope | ~50M+ | Separation |
| FRCRN_SE_16K | 16 kHz | ✅ | HuggingFace + ModelScope | ~5M | Enhancement |
| MossFormer2_SE_48K | 48 kHz | ✅ | HuggingFace + ModelScope | ~50M+ | Enhancement |
| AV_MossFormer2_TSE_16K | 16 kHz | ✅ | HuggingFace + ModelScope | ~50M+ | Audio-visual TSE |
| CAM++ | 16 kHz | ✅ | ModelScope + HuggingFace | 7.2M | Speaker embedding |
| ERes2NetV2 | 16 kHz | ✅ | ModelScope | 17.8M | Speaker embedding |
| ECAPA-TDNN | 16 kHz | ✅ | HuggingFace | 20.8M | Speaker embedding |
| SepFormer (WSJ0-2Mix) | 8 kHz | ✅ | HuggingFace | ~17M | Separation |
| SepFormer (WHAMR!) | 8 kHz | ✅ | HuggingFace | ~17M | Separation |
| DeepFilterNet3 | 48 kHz | ✅ | GitHub releases | ~1M | Enhancement (streaming) |

**Sample rate implications for ClearCore:**
- ClearCore currently operates at **48 kHz** (DeepFilterNet3 pipeline).
- Most TSE research models are **8 kHz or 16 kHz**.
- **16 kHz** is the sweet spot for TSE enrollment + extraction (all speaker embedding models are 16 kHz).
- **48 kHz** TSE models are extremely rare — almost no open-source options.
- Strategy: process TSE at 16 kHz internally, upsample output to 48 kHz for ClearCore's pipeline.

---

## 4. ONNX Export & Streaming State Management

### 4.1 The Fundamental Problem: ONNX Runtime is Stateless

ONNX Runtime is **intentionally stateless** to support concurrent inference. RNN/LSTM/GRU hidden states cannot be cached inside the model. The state must be:

1. **Exported as a graph output** (`Y_h`, `Y_c`) — retrieved after each run
2. **Re-imported as a graph input** (`initial_h`, `initial_c`) — fed to the next run

**From ONNX Runtime issue #11085 (maintainer response):**
> "ORT is stateless so that concurrent inferencing is supported. For ORT you will need to make the hidden state a graph output to capture it, and a graph input so it can be passed back in."

### 4.2 ONNX Operator Specification for RNN States

From the ONNX LSTM spec (`onnx__LSTM.html`):

**Inputs:**
- `initial_h` (optional): Initial hidden state, shape `[num_directions, batch_size, hidden_size]`
- `initial_c` (optional): Initial cell state (LSTM only), same shape

**Outputs:**
- `Y_h` (optional): Last hidden state, same shape as `initial_h`
- `Y_c` (optional): Last cell state (LSTM only), same shape as `initial_c`

**For streaming, the pattern is:**
```
Frame N:   inputs = [x_t, h_{t-1}]  →  outputs = [y_t, h_t]
Frame N+1: inputs = [x_{t+1}, h_t]  →  outputs = [y_{t+1}, h_{t+1}]
```

### 4.3 PyTorch Export Patterns for Stateful Streaming

#### Pattern A: Explicit State Inputs/Outputs (Recommended)

```python
import torch
import torch.nn as nn

class StreamingGRU(nn.Module):
    def __init__(self, input_size, hidden_size, num_layers):
        super().__init__()
        self.gru = nn.GRU(input_size, hidden_size, num_layers,
                          batch_first=True)
    
    def forward(self, x, h_in):
        # x: [batch, seq_len=1, features]
        # h_in: [num_layers, batch, hidden_size]
        out, h_out = self.gru(x, h_in)
        return out, h_out

model = StreamingGRU(input_size=481, hidden_size=256, num_layers=2)
model.eval()

# Example inputs
x = torch.randn(1, 1, 481)        # 1 frame, 481 features
h0 = torch.zeros(2, 1, 256)       # 2 layers, batch 1, hidden 256

torch.onnx.export(
    model,
    (x, h0),
    "streaming_gru.onnx",
    input_names=["feat", "h_in"],
    output_names=["output", "h_out"],
    dynamic_axes={
        "feat": {1: "seq_len"},   # variable sequence length
        "h_in": {0: "num_layers"},
        "h_out": {0: "num_layers"},
    },
    opset_version=17,
    dynamo=True,
)
```

**Exported ONNX graph I/O:**
- Inputs: `feat` [1, 1, 481], `h_in` [2, 1, 256]
- Outputs: `output` [1, 1, 256], `h_out` [2, 1, 256]

#### Pattern B: Split Model for Streaming (DeepFilterNet approach)

For models with both convolutions (which need temporal context) and GRUs (which need persistent state), split the model:

```python
# Split encoder into conv part (stateless, takes T frames) 
# and GRU part (stateful, takes T=1 frame)
class EncConv(nn.Module):
    def forward(self, feat):
        # Causal convolutions with T=3 frame context
        return conv_features  # [1, 3, D]

class EncGRU(nn.Module):
    def forward(self, x, h_in):
        # Take only last frame: T[-1:]
        x_last = x[:, -1:, :]  # [1, 1, D]
        out, h_out = self.gru(x_last, h_in)
        return out, h_out
```

**Critical insight from deepfilter-rt:**
> "Time-slicing encoder conv outputs to T[-1:] before the GRU/decoder path is critical: without it, feeding T=3 frames to a GRU with persistent state causes double-processing — the GRU re-processes frames it already saw in the previous call, corrupting its hidden state."

#### Pattern C: ONNX Graph Surgery (Post-export Patching)

Microsoft provides a utility to add state inputs/outputs to an already-exported ONNX model:
https://gist.github.com/skottmckay/ff23c03752dfb9873eb15888e5892c78

This script:
1. Finds all `RNN`, `LSTM`, `GRU` nodes in the graph
2. Adds `initial_h` / `initial_c` as graph inputs (if not already present)
3. Adds `Y_h` / `Y_c` as graph outputs (if not already present)
4. Makes the initial state optional by providing an initializer with zeros as default

### 4.4 Causal Convolution Buffer Management

Causal convolutions need to maintain a ring buffer of past frames. Two approaches:

**Approach 1: External buffer (Rust side)**
```rust
// Rust maintains a ring buffer of past frames
// Only the current frame goes into ONNX
struct CausalConvBuffer {
    buffer: [[f32; FEATURE_DIM]; KERNEL_SIZE],
    position: usize,
}
// ONNX model receives only the current frame
// Rust prepends the buffer history before feeding
```

**Approach 2: Stateful ONNX (all state inside model)**
```python
# Export with explicit conv buffer as input/output
class CausalConvStream(nn.Module):
    def forward(self, x_t, conv_buf):
        # x_t: current frame [1, 1, C]
        # conv_buf: past frames [1, K-1, C] where K = kernel size
        full = torch.cat([conv_buf, x_t], dim=1)  # [1, K, C]
        out = self.conv(full)
        new_buf = full[:, 1:, :]  # shift buffer
        return out, new_buf
```

### 4.5 Cross-Attention Key-Value Cache

For Transformer-based TSE models with cross-attention (e.g., attention between mixture features and speaker embedding), the K/V cache pattern:

```python
class StreamingCrossAttention(nn.Module):
    def forward(self, query_frame, key_cache, value_cache):
        # query_frame: [1, 1, D] — current frame
        # key_cache: [1, T_past, D] — cached keys from previous frames
        # value_cache: [1, T_past, D] — cached values
        
        # Compute K,V for current frame
        k_new = self.w_k(query_frame)
        v_new = self.w_v(query_frame)
        
        # Append to cache
        key_cache = torch.cat([key_cache, k_new], dim=1)
        value_cache = torch.cat([value_cache, v_new], dim=1)
        
        # Attend over full cache
        attn_out = F.scaled_dot_product_attention(
            query_frame, key_cache, value_cache)
        
        return attn_out, key_cache, value_cache
```

**ONNX export:** `key_cache` and `value_cache` become explicit input/output tensors, just like RNN states.

---

## 5. ONNX Runtime Rust (`ort`) Streaming Execution

### 5.1 The `ort` Crate

**Repository:** https://github.com/pykeio/ort  
**Documentation:** https://ort.pyke.io  
**Crates.io:** `ort = "2.0.0-rc.*"` (latest: 2.0.0-rc.10+)

The `ort` crate is the primary Rust wrapper for ONNX Runtime. Key characteristics:
- **`Session::run` takes `&mut self`** (not thread-safe, by design — one session per thread)
- Supports **IO Binding** for pre-allocated input/output tensors
- **`Session::run` can be zero-alloc on the Rust side** (v2.0.0-rc.10+)
- Pre-allocated outputs via `OutputSelector::preallocate`

### 5.2 Session Setup for Streaming

```rust
use ort::session::{Session, builder::GraphOptimizationLevel};
use ort::execution_providers::CPUExecutionProvider;

fn create_streaming_session(model_path: &str) -> Result<Session, ort::Error> {
    let session = Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_intra_threads(2)?  // 1-2 threads for realtime audio
        .with_inter_threads(1)?
        .commit_from_file(model_path)?;
    Ok(session)
}
```

**Thread configuration for realtime audio:**
- **1-2 intra-op threads** minimizes latency jitter (recommended by deepfilter-rt)
- **1 inter-op thread** avoids scheduling overhead
- Bind to physical cores for consistent timing

### 5.3 Stateful Frame-by-Frame Inference

The core streaming pattern — feed state back between frames:

```rust
use ort::session::Session;
use ort::value::Tensor;
use ort::inputs;

pub struct StreamingTSE {
    session: Session,
    // Pre-allocated state buffers — reused across frames
    gru_state: Tensor<f32>,         // [num_layers, 1, hidden_dim]
    conv_buffer: Tensor<f32>,       // [1, kernel_size-1, feature_dim]
    // Input/output buffers
    frame_buffer: Tensor<f32>,      // [1, 1, feature_dim]
    output_buffer: Tensor<f32>,     // [1, 1, output_dim]
}

impl StreamingTSE {
    pub fn new(model_path: &str) -> Result<Self, ort::Error> {
        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(2)?
            .commit_from_file(model_path)?;
        
        // Pre-allocate all tensors ONCE at initialization
        // These are reused across every 10ms frame
        let gru_state = Tensor::zeros(
            &[2usize, 1, 256],  // 2 layers, batch=1, hidden=256
        )?;
        let conv_buffer = Tensor::zeros(
            &[1usize, 7, 481],  // kernel_size-1=7 frames, 481 features
        )?;
        let frame_buffer = Tensor::zeros(&[1usize, 1, 481])?;
        let output_buffer = Tensor::zeros(&[1usize, 1, 1])?;
        
        Ok(Self {
            session,
            gru_state,
            conv_buffer,
            frame_buffer,
            output_buffer,
        })
    }
    
    pub fn process_frame(
        &mut self,
        features: &[f32],  // feature_dim elements
    ) -> Result<(), ort::Error> {
        // Copy input features into pre-allocated buffer (no allocation)
        {
            let buf = self.frame_buffer.extract_array_mut();
            buf.slice_mut(..).iter_mut().zip(features.iter())
                .for_each(|(dst, src)| *dst = *src);
        }
        
        // Run inference with state tensors
        let outputs = self.session.run(
            inputs![
                "feat" => &self.frame_buffer,
                "h_in" => &self.gru_state,
                "conv_buf" => &self.conv_buffer,
            ]
        )?;
        
        // Extract updated state (read back into pre-allocated buffers)
        // Option A: Use IO Binding to write directly into pre-allocated tensors
        // Option B: Copy from output tensors into pre-allocated state buffers
        
        // For zero-alloc: use IO Binding (see section 5.4)
        
        Ok(())
    }
    
    pub fn reset(&mut self) -> Result<(), ort::Error> {
        // Zero out state buffers between streams
        let zeros = Tensor::zeros(&[2usize, 1, 256])?;
        zeros.copy_into(&mut self.gru_state)?;
        Ok(())
    }
}
```

### 5.4 IO Binding for Zero-Allocation Streaming

IO Binding allows binding inputs/outputs to **pre-allocated tensors**, avoiding per-frame allocation:

```rust
use ort::session::{Session, IoBinding};
use ort::value::Tensor;

pub struct ZeroAllocStreamingTSE {
    session: Session,
    binding: IoBinding,
    // All tensors pre-allocated once
    feat_in: Tensor<f32>,
    h_in: Tensor<f32>,
    conv_buf_in: Tensor<f32>,
    feat_out: Tensor<f32>,
    h_out: Tensor<f32>,
    conv_buf_out: Tensor<f32>,
}

impl ZeroAllocStreamingTSE {
    pub fn new(model_path: &str) -> Result<Self, ort::Error> {
        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(2)?
            .commit_from_file(model_path)?;
        
        // Create IO binding for this session
        let mut binding = session.create_binding()?;
        
        // Pre-allocate input tensors
        let feat_in = Tensor::<f32>::zeros(&[1usize, 1, 481])?;
        let h_in = Tensor::<f32>::zeros(&[2usize, 1, 256])?;
        let conv_buf_in = Tensor::<f32>::zeros(&[1usize, 7, 481])?;
        
        // Pre-allocate output tensors — will be REUSED across runs
        let feat_out = Tensor::<f32>::zeros(&[1usize, 1, 1])?;
        let h_out = Tensor::<f32>::zeros(&[2usize, 1, 256])?;
        let conv_buf_out = Tensor::<f32>::zeros(&[1usize, 7, 481])?;
        
        // Bind inputs
        binding.bind_input("feat", &feat_in)?;
        binding.bind_input("h_in", &h_in)?;
        binding.bind_input("conv_buf", &conv_buf_in)?;
        
        // Bind outputs to pre-allocated tensors
        binding.bind_output("output", feat_out)?;
        binding.bind_output("h_out", h_out)?;
        binding.bind_output("conv_buf_out", conv_buf_out)?;
        
        Ok(Self {
            session,
            binding,
            feat_in,
            h_in,
            conv_buf_in,
            feat_out,
            h_out,
            conv_buf_out,
        })
    }
    
    pub fn process_frame(&mut self, features: &[f32]) -> Result<(), ort::Error> {
        // 1. Write input features into pre-allocated tensor (memcpy, no alloc)
        {
            let arr = self.feat_in.extract_array_mut();
            for (i, &f) in features.iter().enumerate() {
                arr[[0, 0, i]] = f;
            }
        }
        
        // 2. Run with IO binding — outputs go directly into pre-allocated tensors
        let _outputs = self.session.run_binding(&self.binding)?;
        
        // 3. Copy output state into input state for next frame
        //    (this is a memcpy between pre-allocated buffers, zero alloc)
        self.h_out.copy_into(&mut self.h_in)?;
        self.conv_buf_out.copy_into(&mut self.conv_buf_in)?;
        
        // 4. Read output (e.g., denoised mask, TSE output)
        let output_arr = self.feat_out.extract_array();
        // ... process output ...
        
        Ok(())
    }
}
```

**Key IO Binding benefits for streaming:**
1. **Inputs**: bound once (or re-bound when values change) — no per-frame allocation
2. **Outputs**: pre-allocated tensors reused across runs — no per-frame `Vec` creation
3. **State feedback**: `copy_into` between pre-allocated buffers (memcpy, no alloc)
4. **Zero heap allocations in hot path** — satisfies ClearCore's invariant

### 5.5 RunOptions Output Pre-allocation

Alternative to IO Binding — use `RunOptions` with `OutputSelector`:

```rust
use ort::session::{RunOptions, OutputSelector};
use ort::memory::Allocator;
use ort::value::Tensor;

fn create_run_options(session: &Session) -> Result<RunOptions, ort::Error> {
    let output0_name = session.outputs()[0].name();
    let h_out_name = session.outputs()[1].name();
    
    let options = RunOptions::new()?
        .with_outputs(
            OutputSelector::no_default()
                .with(output0_name)
                .preallocate(output0_name,
                    Tensor::<f32>::zeros(&[1usize, 1, 1])?)
                .with(h_out_name)
                .preallocate(h_out_name,
                    Tensor::<f32>::zeros(&[2usize, 1, 256])?),
        );
    Ok(options)
}

// Then use with session.run_with_options()
// Pre-allocated outputs are overwritten each run (zero alloc)
```

### 5.6 DeepFilter-rt: Production Reference

The [deepfilter-rt](https://github.com/shimondoodkin/deepfilter-rt) repository (MIT/Apache-2.0) demonstrates production-quality streaming inference with `ort`:

**Measured performance (34s audio, Windows, ONNX Runtime CPU):**

| Model | Mode | RTF | Frame Time (avg) | Frame Time (max) |
|-------|------|-----|-------------------|-------------------|
| dfn3 | Combined streaming | 0.11x | 1.0 ms | ~3 ms |
| dfn3 | Split streaming | 0.34x | 1.3 ms | ~5 ms |
| dfn3_ll | Split streaming | 0.82x | 3.3 ms | 8.5 ms |

**Key design patterns from deepfilter-rt:**
1. **Split streaming** (recommended): separate ONNX files for conv and GRU parts
2. **Flat ring buffers** for causal conv context (no heap allocation)
3. **Pre-allocated I/O tensors** reused across frames
4. **2 intra-op threads** default for realtime
5. **State auto-detection**: checks for `h0` input in ONNX graph
6. **Warmup call** before streaming to avoid cold-start latency

---

## 6. Zero-Allocation Streaming Pattern in Rust

### 6.1 Complete Working Example

```rust
//! TSE Streaming Inference with zero heap allocation
//! Uses ort (ONNX Runtime Rust wrapper) with IO Binding
//!
//! ClearCore constraints: #![forbid(unsafe_code)], no alloc in RT callback

use ort::session::{Session, IoBinding, builder::GraphOptimizationLevel};
use ort::value::Tensor;

/// Maximum frame size for 10ms at 16kHz = 160 samples
const FRAME_SIZE: usize = 160;
/// Feature dimension (e.g., 481 for STFT, or 80 for MFCC)
const FEATURE_DIM: usize = 481;
/// GRU hidden dimension
const HIDDEN_DIM: usize = 256;
/// Number of GRU layers
const NUM_LAYERS: usize = 2;
/// Causal conv kernel size - 1 (history frames)
const CONV_HISTORY: usize = 7;

pub struct TSEStream {
    session: Session,
    binding: IoBinding,
    
    // Pre-allocated input tensors (written each frame, no alloc)
    input_frame: Tensor<f32>,      // [1, 1, FEATURE_DIM]
    gru_h_in: Tensor<f32>,         // [NUM_LAYERS, 1, HIDDEN_DIM]
    conv_history: Tensor<f32>,     // [1, CONV_HISTORY, FEATURE_DIM]
    
    // Pre-allocated output tensors (reused each frame, no alloc)
    output_mask: Tensor<f32>,      // [1, 1, FEATURE_DIM]
    gru_h_out: Tensor<f32>,        // [NUM_LAYERS, 1, HIDDEN_DIM]
    conv_history_out: Tensor<f32>, // [1, CONV_HISTORY, FEATURE_DIM]
}

impl TSEStream {
    /// Initialize — call ONCE during setup, NOT in RT callback
    pub fn new(model_path: &str) -> Result<Self, ort::Error> {
        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(2)?
            .with_inter_threads(1)?
            .commit_from_file(model_path)?;
        
        let mut binding = session.create_binding()?;
        
        // ---- Pre-allocate ALL tensors ----
        let input_frame = Tensor::<f32>::zeros(
            &[1usize, 1, FEATURE_DIM])?;
        let gru_h_in = Tensor::<f32>::zeros(
            &[NUM_LAYERS, 1, HIDDEN_DIM])?;
        let conv_history = Tensor::<f32>::zeros(
            &[1usize, CONV_HISTORY, FEATURE_DIM])?;
        let output_mask = Tensor::<f32>::zeros(
            &[1usize, 1, FEATURE_DIM])?;
        let gru_h_out = Tensor::<f32>::zeros(
            &[NUM_LAYERS, 1, HIDDEN_DIM])?;
        let conv_history_out = Tensor::<f32>::zeros(
            &[1usize, CONV_HISTORY, FEATURE_DIM])?;
        
        // ---- Bind inputs ----
        binding.bind_input("frame", &input_frame)?;
        binding.bind_input("h_in", &gru_h_in)?;
        binding.bind_input("conv_hist", &conv_history)?;
        
        // ---- Bind outputs to pre-allocated tensors ----
        binding.bind_output("mask", &output_mask)?;
        binding.bind_output("h_out", &gru_h_out)?;
        binding.bind_output("conv_hist_out", &conv_history_out)?;
        
        Ok(Self {
            session,
            binding,
            input_frame,
            gru_h_in,
            conv_history,
            output_mask,
            gru_h_out,
            conv_history_out,
        })
    }
    
    /// Process one frame — called in RT callback
    /// ZERO heap allocations in this function
    pub fn process_frame(
        &mut self,
        features: &[f32],  // slice from pre-allocated ring buffer
    ) -> Result<&Tensor<f32>, ort::Error> {
        // 1. Write features into pre-allocated input tensor (memcpy)
        {
            let arr = self.input_frame.extract_array_mut();
            for (i, &val) in features.iter().enumerate() {
                if i < FEATURE_DIM {
                    arr[[0, 0, i]] = val;
                }
            }
        }
        
        // 2. Run inference — outputs go into pre-allocated tensors
        self.session.run_binding(&self.binding)?;
        
        // 3. Copy output state → input state for next frame
        //    (memcpy between pre-allocated buffers, no allocation)
        self.gru_h_out.copy_into(&mut self.gru_h_in)?;
        self.conv_history_out.copy_into(&mut self.conv_history)?;
        
        // 4. Return reference to pre-allocated output
        Ok(&self.output_mask)
    }
    
    /// Reset state between streams — call between sessions, NOT in RT callback
    pub fn reset(&mut self) -> Result<(), ort::Error> {
        let zero_h = Tensor::<f32>::zeros(
            &[NUM_LAYERS, 1, HIDDEN_DIM])?;
        zero_h.copy_into(&mut self.gru_h_in)?;
        
        let zero_conv = Tensor::<f32>::zeros(
            &[1usize, CONV_HISTORY, FEATURE_DIM])?;
        zero_conv.copy_into(&mut self.conv_history)?;
        
        Ok(())
    }
    
    /// Warmup — run one dummy frame to avoid cold-start latency
    pub fn warmup(&mut self) -> Result<(), ort::Error> {
        let dummy = [0.0f32; FEATURE_DIM];
        self.process_frame(&dummy)?;
        self.reset()?;
        Ok(())
    }
}
```

### 6.2 Integration with ClearCore's Realtime Callback

```rust
// In ClearCore's audio callback (zero allocation enforced):
fn process_audio_frame(
    stream: &mut TSEStream,
    input_samples: &[f32],        // 160 samples @ 16kHz (10ms)
    output_samples: &mut [f32],   // pre-allocated output buffer
    feature_ring: &mut RingBuffer<FEATURE_DIM>,  // pre-allocated
) {
    // 1. Compute STFT features (writes into pre-allocated ring buffer)
    let features = feature_ring.update_and_view(input_samples);
    
    // 2. Run TSE inference (zero alloc — all tensors pre-allocated)
    let mask = stream.process_frame(features).expect("inference");
    
    // 3. Apply mask to audio (element-wise multiply)
    for (out, &sample) in output_samples.iter_mut().zip(input_samples) {
        *out = sample; // simplified; real impl applies mask
    }
}
```

### 6.3 Why IO Binding Over Plain `session.run()`

| Aspect | Plain `session.run()` | IO Binding |
|--------|----------------------|------------|
| Input allocation | New `Tensor` each call OR borrow | Pre-allocated, bound once |
| Output allocation | New `Tensor` each run | Pre-allocated, reused |
| State feedback | Extract → copy → new Tensor | `copy_into` between pre-allocated |
| Heap allocs per frame | 0 (if using `TensorRef`) to N | **0 guaranteed** |
| Copy overhead | Input copied into ORT buffers | Can bind directly to ORT memory |
| Thread safety | `&mut self` required | `&mut self` required |

**Important:** Even plain `session.run()` can be zero-alloc on the Rust side (v2.0.0-rc.10+) when using `TensorRef` for inputs. However, IO Binding additionally avoids copies between Rust and ORT memory spaces.

---

## 7. Quantization & CPU Acceleration

### 7.1 ONNX Runtime CPU EP Quantization Support

From [ONNX Runtime quantization docs](https://onnxruntime.ai/docs/performance/model-optimizations/quantization.html):

| Quantization | CPU Support | Notes |
|-------------|-------------|-------|
| **INT8 (S8S8 QDQ)** | ✅ Recommended default | Best balance of perf/accuracy |
| **INT8 (U8S8 QDQ)** | ✅ Fallback if accuracy drops | For AVX2/AVX512 without VNNI |
| **INT8 (U8U8)** | ✅ Avoid saturation | Only if accuracy issues with U8S8 |
| **INT8 (S8S8 QOperator)** | ⚠️ **Slow on x86-64** | **Avoid on x86** |
| **FP16** | ⚠️ Limited on CPU | Primarily GPU; some AVX512_FP16 ops |
| **BF16** | ⚠️ Via oneDNN EP only | Default CPU EP does NOT support BF16 |
| **FP32** | ✅ Baseline | Always works |

**Key findings:**

1. **INT8 S8S8 QDQ is the recommended default** for x86-64 CPU
2. **BF16 is NOT supported by the default CPU EP** — only via oneDNN execution provider (requires `USE_DNNL` build)
3. **FP16 on CPU** is primarily useful for weight-only quantization (MatMulNBits), not general computation
4. On x86 without VNNI, U8S8 may suffer from `VPMADDUBSW` saturation — use `reduce_range` (7-bit weights) as mitigation

### 7.2 Hardware-Specific Acceleration

**AVX2 (no VNNI):**
- Uses `VPMADDUBSW` for U8S8 — may saturate (16-bit overflow)
- Mitigation: `reduce_range` (quantize weights to 7 bits) or U8U8
- INT8 speedup over FP32: **~1.5-2x** for GEMM-heavy workloads

**AVX-512 VNNI (Cascade Lake, Ice Lake, Zen 4+):**
- Native `VPDPBUSD` instruction — no saturation issues
- INT8 speedup over FP32: **~2-4x** for suitable models
- The "sweet spot" for INT8 quantization

**AVX-512 BF16 (Cooper Lake, Zen 4):**
- Available only via oneDNN EP, not default CPU EP
- Useful if BF16 accuracy is sufficient (avoids quantization calibration)

### 7.3 Measured Speedups (from ONNX Runtime benchmarks)

**BERT model (12-layer) on 11th Gen Intel Core with VNNI:**
- INT8 quantized: **2.9x** speedup over FP32
- Distilled BERT INT8: **3.38x** speedup

**MLAS GEMM benchmarks (Intel Xeon Platinum 8370C, AVX-512+VNNI):**

| Operation | Quantization | Speedup over FP32 |
|-----------|-------------|-------------------|
| Small GEMM (M=1, decode) | INT8 per-tensor | ~1.3-1.4x |
| Medium GEMM (M=128) | INT8 per-tensor | ~1.15-1.36x |
| Q4 GEMM (MatMulNBits) | 4-bit weights | **1.5-6x** (weight memory bound) |

**Phi-3-mini 4-bit quantization (AVX512-VNNI, sym, blklen128):**
- Prompt: **+37.5%** throughput vs baseline
- Token generation: **+12.6%** throughput vs baseline

**For ClearCore's TSE model (small, streaming):**
- The model processes ~160 samples per 10ms frame
- Compute budget: **<10ms per frame** (ClearCore's realtime constraint)
- INT8 QDQ quantization should provide **1.5-3x speedup** over FP32
- Calibration set: use representative audio frames (500-1000 frames)

### 7.4 Quantization Workflow

```python
# Python: Quantize TSE model for ONNX Runtime CPU
from onnxruntime.quantization import quantize_dynamic, QuantType

# Dynamic quantization (weights only, fast — good for streaming)
quantize_dynamic(
    model_input="tse_model.onnx",
    model_output="tse_model_int8.onnx",
    weight_type=QuantType.QInt8,  # S8S8 QDQ (recommended)
)

# Static quantization (weights + activations — better accuracy)
# Requires calibration dataset
from onnxruntime.quantization import quantize_static, CalibrationDataReader

class TSECalibrator(CalibrationDataReader):
    def __init__(self, frames):
        self.frames = frames
        self.idx = 0
    def get_next(self):
        if self.idx >= len(self.frames):
            return None
        frame = self.frames[self.idx]
        self.idx += 1
        return {"frame": frame, "h_in": zero_state, "conv_hist": zero_hist}

quantize_static(
    model_input="tse_model.onnx",
    model_output="tse_model_int8_static.onnx",
    calibration_data_reader=TSECalibrator(calibration_frames),
    quant_format="QDQ",  # QDQ format (recommended for CPU)
    per_channel=True,     # per-channel quantization for better accuracy
    weight_type=QuantType.QInt8,
)
```

**Rust side: nothing changes** — ONNX Runtime automatically detects quantized models and uses INT8 kernels.

---

## 8. Recommended Starting Point for ClearCore

### 8.1 Architecture Recommendation

Given ClearCore's constraints (`#![forbid(unsafe_code)]`, zero-allocation RT callback, `ort` crate, 48 kHz pipeline), the recommended TSE architecture is:

```
                    ┌─────────────────────────────────┐
                    │     Enrollment (offline/setup)   │
  Reference audio → │  STFT → Speaker Encoder → emb   │ → speaker_emb [192]
                    │  (CAM++ or ECAPA-TDNN, ONNX)     │
                    └─────────────────────────────────┘
                                    │
                                    ▼ (stored as tensor)
┌──────────────────────────────────────────────────────────────┐
│              TSE Extraction (realtime, per 10ms frame)       │
│                                                              │
│  Input 16kHz frame (160 samples)                             │
│      │                                                       │
│      ▼                                                       │
│  STFT → features [481]                                       │
│      │                                                       │
│      ├──→ Causal Conv (history buffer) ──→ GRU (state h) ──→ │
│      │                                                       │
│      ├──→ Cross-attention with speaker_emb ──→ mask          │
│      │                                                       │
│      ▼                                                       │
│  Apply mask → ISTFT → output frame (160 samples)             │
│      │                                                       │
│      ▼                                                       │
│  Upsample 16kHz → 48kHz (for ClearCore pipeline)             │
└──────────────────────────────────────────────────────────────┘
```

### 8.2 Recommended Open-Source Starting Points

**Priority 1 (best fit): SpeechBrain + custom training**

| Aspect | Recommendation |
|--------|----------------|
| **Enrollment encoder** | ECAPA-TDNN (SpeechBrain, Apache 2.0, 20.8M params, 16 kHz) |
| **TSE extraction model** | Custom: causal conv + GRU + cross-attention (train from scratch) |
| **Speaker verification baseline** | CAM++ (7.2M params, 16 kHz, Apache 2.0) |
| **Training framework** | SpeechBrain recipes (Apache 2.0) or ESPnet (Apache 2.0) |
| **Data pipeline** | Dynamic mixing (SpeechBrain pattern) + Libri2Mix + VoxCeleb2 |

**Priority 2 (reference implementations):**
- **ESPnet `espnet_model_tse.py`**: enrollment-conditioned extraction architecture
- **deepfilter-rt**: production Rust streaming ONNX patterns (MIT/Apache-2.0)
- **DeepFilterNet**: streaming model design + ONNX export tooling (MIT/Apache-2.0)

**Priority 3 (avoid):**
- SpEx+ original (GPL-3.0)
- ClearerVoice audio-visual models (requires video input)
- TF-GridNet (non-causal, full-sequence attention)

### 8.3 ONNX Export Strategy for ClearCore

1. **Train a causal TSE model** in PyTorch with explicit state inputs/outputs
   - GRU hidden state as `h_in` / `h_out`
   - Causal conv history as `conv_hist_in` / `conv_hist_out`
   - Cross-attention KV cache as `kv_cache_in` / `kv_cache_out`

2. **Export to ONNX** with `torch.onnx.export(dynamo=True)`:
   ```python
   torch.onnx.export(
       model,
       (frame, h_in, conv_hist, kv_cache, speaker_emb),
       "tse_streaming.onnx",
       input_names=["frame", "h_in", "conv_hist", "kv_cache", "speaker_emb"],
       output_names=["mask", "h_out", "conv_hist_out", "kv_cache_out"],
       opset_version=17,
       dynamo=True,
   )
   ```

3. **Optional: split model** into encoder (conv+GRU) and decoder (mask+attention) for better ORT optimization (deepfilter-rt pattern)

4. **Quantize** with INT8 QDQ static quantization using 500+ representative audio frames

5. **Deploy** in Rust with `ort` + IO Binding (zero allocation)

### 8.4 Key Performance Targets

| Metric | Target | Basis |
|--------|--------|-------|
| Frame size | 160 samples (10ms @ 16kHz) | ClearCore realtime budget |
| Inference time | <5ms per frame | 10ms budget, leave headroom |
| RTF | <0.5x | DeepFilterNet achieves 0.11x |
| Model size | <20M params | CPU realtime feasibility |
| Quantized size | <10MB (INT8) | Distribution size |
| Heap allocs per frame | **0** | ClearCore invariant |
| Latency (end-to-end) | <30ms | 10ms frame + STFT + compute |

### 8.5 Summary of Key Findings

1. **No turnkey open-source streaming TSE model exists** — all available models are non-causal or audio-visual. ClearCore must train a custom causal TSE model.

2. **ECAPA-TDNN or CAM++** are the best enrollment encoders (small, well-tested, 16 kHz, permissively licensed).

3. **ONNX Runtime requires explicit state management** — RNN states, conv buffers, and KV caches must be exported as graph inputs/outputs and fed back between frames.

4. **`ort` crate supports zero-allocation streaming** via IO Binding with pre-allocated `Tensor<f32>` buffers — satisfies ClearCore's invariant.

5. **INT8 QDQ quantization is the recommended default** for x86-64 CPU (AVX2 and AVX-512 VNNI), providing 1.5-3x speedup. BF16 requires oneDNN EP (not default). FP16 is not useful on CPU.

6. **deepfilter-rt** (MIT/Apache-2.0) is the best reference implementation for production Rust streaming ONNX inference — it achieves RTF 0.11x with zero heap allocations.

7. **License landscape is favorable**: SpeechBrain (Apache 2.0), ESPnet (Apache 2.0), ClearerVoice (Apache 2.0), FunASR (MIT), Asteroid (MIT) are all permissively licensed. Avoid SpEx+ original (GPL-3.0).

---

## References

1. ClearerVoice-Studio: https://github.com/modelscope/ClearerVoice-Studio (Apache 2.0)
2. 3D-Speaker: https://github.com/modelscope/3D-Speaker (Apache 2.0)
3. FunASR: https://github.com/modelscope/FunASR (MIT)
4. SpeechBrain: https://github.com/speechbrain/speechbrain (Apache 2.0)
5. Asteroid: https://github.com/asteroid-team/asteroid (MIT)
6. ESPnet: https://github.com/espnet/espnet (Apache 2.0)
7. ESPnet TSE: https://github.com/espnet/espnet/blob/master/espnet2/enh/espnet_model_tse.py
8. SpEx+ original: https://github.com/xuchenglin28/speaker_extraction_SpEx (GPL-3.0)
9. deepfilter-rt: https://github.com/shimondoodkin/deepfilter-rt (MIT/Apache-2.0)
10. DeepFilterNet: https://github.com/Rikorose/DeepFilterNet (MIT/Apache-2.0)
11. ort crate: https://github.com/pykeio/ort | https://ort.pyke.io
12. ONNX Runtime quantization: https://onnxruntime.ai/docs/performance/model-optimizations/quantization.html
13. ONNX Runtime stateful RNN issue: https://github.com/microsoft/onnxruntime/issues/11085
14. ONNX LSTM spec: https://onnx.ai/onnx/operators/onnx__LSTM.html
15. ONNX graph surgery gist: https://gist.github.com/skottmckay/ff23c03752dfb9873eb15888e5892c78
16. Intel DL Boost + ONNX Runtime BERT benchmark: https://opensource.microsoft.com/blog/2021/03/01/optimizing-bert-model-for-intel-cpu-cores-using-onnx-runtime-default-execution-provider/
17. MLAS quantized GEMM benchmarks: https://github.com/microsoft/onnxruntime/pull/28606
18. X-TF-GridNet: https://github.com/HaoFengyuan/X-TF-GridNet
19. SpeakerBeam (Asteroid-based): https://github.com/BUTSpeechFIT/speakerbeam
20. PyTorch ONNX export: https://docs.pytorch.org/docs/stable/onnx_export.md
