# Platform Support Matrix

**Product:** Clearcore Realtime Noise Suppression  
**Version:** 0.1.0 (GA)  

---

## Supported Operating Systems

| Operating System | Architecture | Audio Architecture | Driver / Virtual Endpoint | Minimum Version |
|---|---|---|---|---|
| **Windows 11** | x86_64 | PortCls / WaveRT | SysVAD Kernel-Mode WaveRT Driver | 22H2 (Build 22621+) |
| **macOS** | arm64, x86_64 | CoreAudio HAL | AudioServerPlugIn (`RealtimeNoiseHAL.driver`) | macOS 13 (Ventura)+ |
| **Ubuntu Linux** | x86_64 | PipeWire / WirePlumber | `pipewire_helper` (Audio/Source node) | Ubuntu 24.04 LTS (PW >= 0.3.0) |
| **Fedora Linux** | x86_64 | PipeWire / WirePlumber | `pipewire_helper` (Audio/Source node) | Fedora 42+ (PW 1.6+) |

---

## Audio Pipeline Specifications

- **Sampling Rate:** 48,000 Hz canonical (internal 48 kHz mono Float32).
- **Hop Size:** 480 samples (10.0 ms).
- **Algorithmic Latency:** 30.0 ms (1,440 samples @ 48 kHz).
- **Total End-to-End Latency Target:** <= 70.0 ms (budgeted <= 80.0 ms max product p95).
- **Inference Budget:** <= 7.0 ms p99 per 10 ms hop on supported reference CPU (<= i5-10210U).
- **Failsafe Contract:** Digital silence (zeros) on crash, underrun, or supervisor restart. Zero raw audio leakage.
