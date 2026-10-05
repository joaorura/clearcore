# Experimento: Estabilidade de Vetor vs. Duração do Enrollment

**O que é:** Medição da estabilidade dos vetores FiLM (gamma_enc, beta_enc, gamma_df, beta_df) do modelo `enrollment.onnx` M3 (desenvolvimento) em função da duração do áudio de enrollment.

**Como reproduzir:**
```bash
python run.py
```

O script usa:
- Modelo: `/home/joaorura/orca/projects/clearcore-train/runs/m3/enrollment.onnx`
- Corpus: `/home/joaorura/orca/projects/clearcore-train/data/raw/cml-tts-pt/`
- Durações testadas: 2–90 segundos

**Saída:** `metrics.csv` (cosine e L2 relativo entre durações), `sanity.csv` (estatísticas por vetor), `latency.json` (tempo de inferência).

**Ressalva:** O experimento mede estabilidade do vetor, não qualidade do denoiser condicionado.
