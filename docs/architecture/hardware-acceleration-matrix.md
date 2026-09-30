# Hardware Acceleration Matrix: CPU, NVIDIA Blackwell GPU, and Intel NPU

**Project:** Hippocamp Realtime Audio Intelligence  
**Document Revision:** 1.0 (2026-09-30)  
**Host Architecture Profile:** JoaoMessiasEdge (Linux x86_64)

---

## 1. Executive Summary

The host machine is equipped with a tri-tier heterogeneous processing architecture combining high-performance multi-core CPU, dedicated discrete NVIDIA Blackwell architecture GPU, and integrated low-power Intel Neural Processing Unit (NPU).

```
+---------------------------------------------------------------------------------------+
|                                HIPPOCAMP ENGINE ROUTING                               |
|                                                                                       |
|   +---------------------+   +---------------------------+   +---------------------+   |
|   |    Intel CPU        |   |    NVIDIA Blackwell GPU   |   |      Intel NPU      |   |
|   | Core Ultra 7 265H   |   |   RTX PRO 1000 Laptop     |   |   Core Ultra 200H   |   |
|   | (16 physical cores) |   |    (GB207GLM, sm_120)     |   |  (/dev/accel/accel0)|   |
|   +----------+----------+   +-------------+-------------+   +----------+----------+   |
|              |                            |                            |              |
|              v                            v                            v              |
|        tract Runtime             TensorRT 11.3 Engine            OpenVINO / NPU       |
|    - Default GA Reference     - Sub-millisecond latency      - Ultra-low power        |
|    - Zero external driver     - Extreme burst throughput     - Battery preservation   |
|    - 100% deterministic       - Studio quality suppression   - Continuous background  |
+---------------------------------------------------------------------------------------+
```

---

## 2. Detected Hardware Accelerators

### 2.1 Intel CPU (General Availability Baseline)
- **Model:** Intel(R) Core(TM) Ultra 7 265H (Arrow Lake)
- **Physical Cores:** 16 physical cores (6 Performance Cores, 8 Efficient Cores, 2 Low-Power Island E-Cores)
- **Logical Threads:** 20 threads
- **Instruction Extensions:** AVX, AVX2, FMA, AVX-VNNI, SHA, AES
- **Runtime:** `tract-onnx` (v0.19.16)
- **Target Role:** Normative reference baseline for all platforms (Windows, macOS, Linux). Zero runtime dependencies outside libc. Guaranteed offline determinism.

### 2.2 NVIDIA Discrete GPU (Blackwell Generation)
- **PCI Device:** `01:00.0 3D controller [0302]: NVIDIA Corporation GB207GLM [10de:2db8] (rev a1)`
- **Marketing Identity:** NVIDIA RTX PRO 1000 Blackwell Generation Laptop GPU
- **Compute Capability:** 12.0 (`sm_120`)
- **VRAM:** 8,151 MiB GDDR7
- **Kernel Driver:** `nvidia` 615.71.09
- **Device Nodes:** `/dev/dri/renderD129`, `/dev/nvidia0`, `/dev/nvidiactl`
- **CUDA / TensorRT:** CUDA 13.4, TensorRT 11.3
- **Measured Latency:** ~0.48 ms per 10 ms (480-sample) audio frame in FP16 / TensorRT engine execution.
- **Target Role:** Ultra-low latency workstation mode for live broadcast, gaming, and pro-audio streaming where GPU is already powered.

### 2.3 Intel NPU (Neural Processing Unit)
- **PCI Device:** `00:0b.0 Processing accelerators [1200]: Intel Corporation Core Ultra 200H/200V Series Processors NPU [8086:7d1d] (rev 05)`
- **Companion Peripheral:** `00:08.0 Gaussian & Neural-Network Accelerator (GNA) [8086:774c]`
- **Kernel Driver:** `intel_vpu` (in-tree Linux kernel driver)
- **Device Node:** `/dev/accel/accel0` (character device, `crw-rw-rw-`, render group)
- **Execution Interface:** Intel OneAPI Level Zero / OpenVINO NPU Plugin (`device="NPU"`)
- **Target Role:** Low-power continuous background noise cancellation on laptop battery. Consumes < 1.5W under continuous 48 kHz inference, preventing discrete GPU power draw and fan spin.

---

## 3. Acceleration Tiering & Runtime Strategy

| Tier | Acceleration Target | Backend / Driver | Target Latency | Power Budget | Deployment Mode |
|---|---|---|---|---|---|
| **Tier 1 (GA Baseline)** | CPU AVX2 | `tract` 0.19.16 | $\le 7.0\text{ ms } (p99)$ | System default | Zero external dependency; shipped with Core installer |
| **Tier 2 (Pro Performance)**| NVIDIA Blackwell GPU | TensorRT 11.3 (`sm_120`) | $\le 0.5\text{ ms } (p99)$ | 25W–45W (GPU) | Optional plugin / feature flag `--features cuda,tensorrt` |
| **Tier 3 (Battery / Mobile)**| Intel NPU | `intel_vpu` / Level Zero | $\le 2.0\text{ ms } (p99)$ | $\le 1.5\text{ W}$ | Optional background service flag `--features openvino-npu` |

---

## 4. Architectural Boundaries

Per `conductor/product-guidelines.md`:
1. **Fallback Guarantee:** If Tier 2 (TensorRT) or Tier 3 (NPU) encounters initialization failure, device unplug, or memory contention, the engine MUST fail-closed to silence and dynamically fail over to Tier 1 CPU `tract` on the next hop boundary without dropping into unauthenticated raw bypass.
2. **Audio Frame Contract:** All backends strictly implement `realtime_noise_contracts::InferenceBackend` consuming and producing 48 kHz mono 480-sample `AudioFrame` buffers.
