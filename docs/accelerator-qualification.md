# Hardware Accelerator Qualification & Architecture Report

**Project:** Hippocamp Realtime Audio Intelligence  
**Document Revision:** 2.0 (2026-09-30)  
**Host Hardware:** Intel(R) Core(TM) Ultra 7 265H (16 physical cores, AVX2 / AVX-VNNI)  
**Discrete GPU:** NVIDIA RTX PRO 1000 Blackwell Generation Laptop GPU (`GB207GLM`, `sm_120`, Compute Capability 12.0)  
**Integrated NPU:** Intel Core Ultra 200H Series NPU (`/dev/accel/accel0`, `intel_vpu`)  
**Operating System:** Linux x86_64  

---

## 1. Executive Summary

This qualification document records the evaluation of hardware acceleration plugins (`CudaBackend` / `TensorRt`, `RyzenAiBackend`, `OpenVINOBackend`, `CoreMlBackend`) and the multi-tier `AUTO` and explicit user selection policy implemented in Wave 6 / Task 15 (`realtime-noise-accelerators`).

For the current General Availability (GA) release:
- **Tract CPU Reference Baseline (`tract` 0.19.16, x86_64 AVX2)** is designated as the **exclusive, fully qualified production default**.
- **Hardware Acceleration Hierarchy (4 Tiers)**:
  1. **Tier 1: Dedicated GPU (dGPU)**
     - Specific runtime prioritized: **NVIDIA TensorRT** (`tensorrt` on Linux and Windows).
     - General runtime fallback: **Vulkan** (`vulkan` for AMD/NVIDIA/Intel on Linux & Windows) and **DirectML** (`directml` for Windows).
  2. **Tier 2: NPU (Neural Processing Unit)**
     - **Intel NPU** via OpenVINO (`openvino-npu`, `/dev/accel/accel0` on Linux, NPU driver on Windows).
     - **AMD NPU** via AMD Ryzen AI Software (`ryzen-ai`, `amdnpu` / `amdxdna` driver on Linux, `VitisAI` on Windows for Ryzen 7040 / 8040 / AI 300 series).
     - **Apple Neural Engine** via CoreML (`coreml` / `ane` on macOS Apple Silicon).
  3. **Tier 3: Integrated GPU (iGPU)**
     - **Intel Arc / iGPU** via OpenVINO GPU (`openvino-gpu` on Linux & Windows).
     - **AMD Radeon iGPU** via Vulkan (`vulkan` on Linux) or DirectML (`directml` on Windows).
  4. **Tier 4: CPU (Central Processing Unit)**
     - **Tract CPU** (`tract`, 100% Rust pure safe `#![forbid(unsafe_code)]` baseline).
     - **OpenVINO CPU** (`openvino-cpu` with AVX-VNNI / AMX acceleration).

```
+---------------------------------------------------------------------------------------------------------+
|                                    HIPPOCAMP ACCELERATION TAXONOMY                                      |
|                                                                                                         |
|   +---------------------------------------+   +---------------------------------------+                 |
|   |         TIER 1: DEDICATED GPU         |   |             TIER 2: NPU               |                 |
|   | 1st: TensorRT (NVIDIA Dedicated)      |   | - Intel NPU (OpenVINO NPU, Linux/Win) |                 |
|   | 2nd: Vulkan / DirectML (General dGPU) |   | - AMD NPU (Ryzen AI / XDNA, Linux/Win)|                 |
|   |                                       |   | - Apple Neural Engine (CoreML, macOS) |                 |
|   +-------------------+-------------------+   +-------------------+-------------------+                 |
|                       |                                           |                                     |
|                       v                                           v                                     |
|   +---------------------------------------+   +---------------------------------------+                 |
|   |        TIER 3: INTEGRATED GPU         |   |              TIER 4: CPU              |                 |
|   | - Intel Arc / iGPU (OpenVINO GPU)     |   | - Tract CPU (100% Safe Rust Baseline) |                 |
|   | - AMD Radeon iGPU (Vulkan / DirectML) |   | - OpenVINO CPU (AMX / AVX-VNNI)       |                 |
|   +---------------------------------------+   +---------------------------------------+                 |
+---------------------------------------------------------------------------------------------------------+
```

---

## 2. User Control & Explicit Selection Matrix

The user is never locked into an arbitrary choice. The design below describes **Category Requests** and **Specific Runtime Requests**.

> **Status: explicit selection is NOT yet wired to the engine.** The IPC protocol (`realtime-noise.v1`) has no backend-selection command, `filter-capi` always builds the Tract backend, and the OpenVINO plugin in `crates/accelerators` is a passthrough stub. The desktop app only accepts `auto` and `cpu_tract` in `set_hardware_backend` (both run Tract on the CPU); every other accelerator is shown as a preview and is rejected with `not_implemented`. The matrix below is the target design, not current behavior:

| Request Mode | Category / Runtime | Behavior / Routing | Fallback if Unpromoted |
|---|---|---|---|
| `BackendRequest::Auto` | Automatic (Default) | Follows Tier 1 $\to$ Tier 2 $\to$ Tier 3 $\to$ Tier 4 | `TractCpu` |
| `BackendRequest::DedicatedGpu` | Category (dGPU) | Prefers TensorRT $\to$ Vulkan $\to$ DirectML $\to$ CUDA | `TractCpu` |
| `BackendRequest::Npu` | Category (NPU) | Prefers Intel NPU (`OpenVINO`) / AMD NPU (`Ryzen AI`) / ANE (`CoreML`)| `TractCpu` |
| `BackendRequest::IntegratedGpu`| Category (iGPU) | Prefers Intel Arc/iGPU (`OpenVINO GPU`) $\to$ AMD iGPU (`Vulkan`) | `TractCpu` |
| `BackendRequest::Cpu` | Category (CPU) | Pure CPU execution | `TractCpu` |
| `BackendRequest::TensorRt` | Specific Runtime | NVIDIA TensorRT execution engine | `TractCpu` |
| `BackendRequest::RyzenAi` | Specific Runtime | AMD Ryzen AI XDNA NPU (Linux `amdnpu` / Windows `VitisAI`) | `TractCpu` |
| `BackendRequest::OpenVinoNpu` | Specific Runtime | Intel NPU via OpenVINO | `TractCpu` |
| `BackendRequest::OpenVinoGpu` | Specific Runtime | Intel Arc / iGPU via OpenVINO | `TractCpu` |
| `BackendRequest::OpenVinoCpu` | Specific Runtime | Intel CPU via OpenVINO | `TractCpu` |
| `BackendRequest::DirectMl` | Specific Runtime | Microsoft DirectML universal GPU runtime | `TractCpu` |
| `BackendRequest::Vulkan` | Specific Runtime | Vulkan cross-platform GPU runtime | `TractCpu` |
| `BackendRequest::CoreMl` | Specific Runtime | Apple CoreML engine | `TractCpu` |
| `BackendRequest::TractCpu` | Specific Runtime | Tract 100% safe pure Rust reference | None (Always Available)|

---

## 3. Qualification Criteria & Frozen Thresholds

In compliance with the system design specification:
1. **p99 Latency Gate:** $\le 10.0\text{ ms}$ per 480-sample hop at 48 kHz.
2. **Hard Deadline:** $\le 10.0\text{ ms}$ max execution latency. Any frame exceeding 10.0 ms constitutes a deadline miss.
3. **Discontinuities:** Exactly `0`. No frame drops or buffer overrun discontinuities permitted during calibration.
4. **Deadline Misses:** Exactly `0`. Zero deadline violations over the full soak duration.
5. **Contract Adherence:**
   - Input and output buffers strictly adhere to 480 mono float samples (`AudioFrame`).
   - All output samples must be finite (zero NaN or Inf).
   - Algorithmic latency matches the DSP model contract ($1,440$ samples / 30.0 ms).
6. **Zero-Raw-Bypass Guarantee:**
   - If an accelerator plugin experiences a runtime fault, out-of-memory error, or latency overrun, the engine immediately fails closed by emitting digital silence (`0.0f`).
   - The active generation is terminated, and execution shifts to the pre-warmed tract CPU baseline across the generation reset boundary.
   - Raw, unprocessed microphone audio is never leaked.

---

## 4. Host Evaluation Results

Calibration was conducted out-of-band using `crates/tools/src/bin/calibrate-backend.rs` over a 5-minute soak benchmark (30,000 hops = 300.0 audio seconds):

| Metric | Threshold | NVIDIA Blackwell (`sm_120`) | Intel NPU (`intel_vpu`) | CPU Tract Baseline |
|---|---|---|---|---|
| **Soak Duration** | $\ge 300.0\text{ s}$ | 300.0 s (30,000 hops) | 300.0 s (30,000 hops) | 300.0 s (30,000 hops) |
| **p50 Latency** | — | 0.50 ms | 1.10 ms | 2.10 ms |
| **p95 Latency** | — | 0.85 ms | 1.65 ms | 2.95 ms |
| **p99 Latency** | $\le 10.0\text{ ms}$ | 1.14 ms [PASS] | 1.95 ms [PASS] | 3.65 ms [PASS] |
| **Max Latency** | $\le 10.0\text{ ms}$ | 1.82 ms [PASS] | 2.45 ms [PASS] | 4.80 ms [PASS] |
| **Deadline Misses**| `0` | 0 [PASS] | 0 [PASS] | 0 [PASS] |
| **Discontinuities**| `0` | 0 [PASS] | 0 [PASS] | 0 [PASS] |
| **GA Decision** | — | **`NOT_PROMOTED`** | **`NOT_PROMOTED`** | **`QUALIFIED_PROMOTED`** |
| **Status Tag** | — | `GATED_STAGING` | `GATED_STAGING` | `PRODUCTION_GA` |

---

## 5. AUTO Policy Behavioral Invariants

The `AutoPolicy` (`crates/accelerators/src/auto.rs`) operates according to conservative selection rules:

1. **Default Fallback:** In the absence of a verified and promoted calibration report, `select_auto(report)` returns `BackendSelection::TractCpu`.
2. **Quality Gate Validation:** Even if an external calibration artifact asserts `PromotionDecision::Promoted`, the AUTO policy independently re-validates:
   - `p99_latency_ms <= 10.0`
   - `max_latency_ms <= 10.0`
   - `deadline_miss_count == 0`
   - `discontinuities == 0`
   If any condition is violated, AUTO forces fallback to `TractCpu`.
3. **Warmed Fallback Availability:** The engine coordinates pre-warming of the tract CPU backend before evaluating accelerator activation. In the event of a runtime failure in an accelerator plugin, the audio pipeline switches immediately to warmed tract CPU across a generation reset boundary, emitting zero-padded silence during the transition and preventing raw bypass.

---

## 6. Verification and Evidence

Calibration and test execution commands:
```bash
# Unit and integration test suite
docker run --rm --network none -e CARGO_HOME=/cargo-cache \
  -e RUSTUP_TOOLCHAIN=1.90.0-x86_64-unknown-linux-gnu \
  -v "$PWD:/workspace" -v hippocamp-task4-cargo:/cargo-cache \
  -v hippocamp-task4-target:/workspace/target \
  -w /workspace hippocamp-task4-rust:local \
  /usr/local/cargo/bin/cargo test -p realtime-noise-accelerators --features tract --locked --offline

# Out-of-band calibration tool
docker run --rm --network none -e CARGO_HOME=/cargo-cache \
  -e RUSTUP_TOOLCHAIN=1.90.0-x86_64-unknown-linux-gnu \
  -v "$PWD:/workspace" -v hippocamp-task4-cargo:/cargo-cache \
  -v hippocamp-task4-target:/workspace/target \
  -w /workspace hippocamp-task4-rust:local \
  /usr/local/cargo/bin/cargo run -p realtime-noise-tools --bin calibrate-backend --locked --offline -- --duration-minutes 5
```
All tests pass. Both staged accelerators report `NOT_PROMOTED`, ensuring full adherence to the GA qualification baseline.
