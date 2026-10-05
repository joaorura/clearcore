# Piloto de Qualidade de Enrollment — 2026-10-05

## O que é
Avalia o efeito da duração do áudio de enrollment sobre a qualidade do pDFNet3 M2 (t3/ckpt-00022500) em misturas sintéticas: 24 locutores (LibriSpeech dev-clean, VCTK, CML-TTS pt), inferência em CPU.

## Condições
**Durações**: 6, 12, 30, 60, 90 s; **K=3** streams de enrollment por locutor.
**Misturas**: TARGET-only + INTERFERER (SIR 0/5 dB, 5 cada) + INTERFERER_NOISE (SIR 0/5 dB, SNR 10 dB, 4 cada) + TARGET_NOISE (SNR 0/5/10 dB, 2x3). Enrollment com fala limpa do alvo.

## Colunas de results_long.csv
`speaker`, `corpus`, `mix` (índice), `kind` (INT/INTN/NOISE), `sir`, `snr`, `wrong_spk` (locutor errado para robustez), `cond` (noisy/neutral/wrong/d{6,12,30,60,90}_k{0,1,2}), `sisdr`, `sisdri` (melhoria), `stoi`, `stoi_in`, `sisdr_in`.

## Reproduzir
```bash
cd /path/to/hippocamp
python docs/superpowers/research/2026-10-05-enrollment-quality/pilot.py select  # seleciona locutores
python docs/superpowers/research/2026-10-05-enrollment-quality/pilot.py run      # executa (4 workers)
```
Requer `/home/joaorura/orca/projects/clearcore-train` e variáveis de ambiente (`Paths.from_env()`).

## Ressalvas
Modelo M2 é NO-GO e fraco. Enrollment com fala limpa do alvo não testa qualidade de denoisamento. Resultados de CPU.
