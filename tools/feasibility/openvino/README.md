# OpenVINO Feasibility & Benchmark Tools

Ferramentas criadas para investigação de viabilidade de execução do DeepFilterNet3 via OpenVINO em dispositivos de hardware (CPU, GPU, NPU).

## Arquivos

1. **`inspect_onnx.py`**: Inspeciona os grafos ONNX (`enc.onnx`, `erb_dec.onnx`, `df_dec.onnx`) do modelo aprovado:
   - Detecta entradas, saídas, opset e tipos de nós.
   - Identifica os nós GRU (5 no total) e confirma que estados ocultos (`initial_h` / `Y_h`) e contextos causais de convolução não estão expostos como I/O.
2. **`bench_ov.py <DEVICE>`**: Compila e mede latência p50 e p99 para cada um dos 3 grafos no dispositivo alvo (`CPU`, `GPU` ou `NPU`).
3. **`state_check.py`**: Prova empírica de divergência temporal quando os grafos são executados frame a frame ($S=1$) sem persistência de estado em comparação à sequência contínua ($S=8$).

## Como Executar

Os scripts localizam automaticamente os grafos ONNX extraindo `vendor/approved/df-compatible-release-asset-v1.bin` ou lendo `CLEARCORE_EXPORT_DIR`.

```bash
# Inspecionar grafos
python3 tools/feasibility/openvino/inspect_onnx.py

# Benchmark por dispositivo (CPU, GPU, NPU)
python3 tools/feasibility/openvino/bench_ov.py CPU
python3 tools/feasibility/openvino/bench_ov.py GPU
python3 tools/feasibility/openvino/bench_ov.py NPU

# Validação de divergência temporal frame-a-frame
python3 tools/feasibility/openvino/state_check.py
```

## Logs das Medições Locais (Host: Intel Core Ultra 7 265H)

### CPU (OpenVINO)
```text
CPU enc      OK compile=54ms p50=157us p99=463us min=128us
CPU erb_dec  OK compile=42ms p50=198us p99=332us min=161us
CPU df_dec   OK compile=32ms p50=98us p99=163us min=88us
CPU SUM(3 graphs) p50=453us p99=958us (soma dos percentis, estimativa)
```

### GPU (NVIDIA RTX PRO 1000 Blackwell dGPU)
```text
GPU enc      OK compile=403ms p50=295us p99=817us min=263us
GPU erb_dec  OK compile=88ms p50=219us p99=653us min=198us
GPU df_dec   OK compile=68ms p50=172us p99=539us min=135us
GPU SUM(3 graphs) p50=685us p99=2010us (soma dos percentis, estimativa)
```

### NPU (Intel AI Boost)
```text
NPU enc      OK compile=67ms p50=770us p99=2444us min=458us
NPU erb_dec  OK compile=24ms p50=742us p99=1555us min=400us
NPU df_dec   OK compile=21ms p50=400us p99=1084us min=369us
NPU SUM(3 graphs) p50=1911us p99=5083us (soma dos percentis, estimativa)
```
