# Modelo de cadastro de voz (enrollment v3)

- **Arquivo**: `enrollment.onnx` (84 012 231 bytes), SHA-256
  `82a292b95121dd7d7e82a2f4515497512693d499f06bafa0d3d627bed34ad3c1`, fixado em
  `crates/model/src/pdfnet3_bundle.rs` (`ENROLLMENT_V3_SHA256`) e conferido pelo `package-app.mjs` e pelo
  workflow de release.
- **Contrato**: entrada `audio` (`f32`, `[1, N]`, 16 kHz mono, `N` em 96 000..=192 000); saída
  `speaker_embedding` (`[1, 192]`, ECAPA-TDNN SpeechBrain `spkrec-ecapa-voxceleb`, normalizado em L2).
  Uma chamada por perfil sobre as amostras concatenadas (250 ms de silêncio entre elas, truncado em 12 s).
- **Par**: o embedding condiciona o pDFNet3 Pro v3 completo (`models/pdfnet3/`), no daemon e no filtro.
- **Origem**: `clearcore-train`, `scripts/export_enrollment_v3.py`, checkpoint
  `runs/m2/pro-v2-mhca-k16-tse/checkpoints/ckpt-00002000.pt`; paridade PyTorch × ONNX Runtime ≥ 117,5 dB,
  tract 0.19.16 × ONNX Runtime 103,5 dB.
- Carregado pelo serviço via tract (`crates/model/src/enrollment_v3.rs`); um arquivo com outro SHA-256 é
  recusado (`ENROLL_MODEL_NOT_CONFIGURED`), salvo com `CLEARCORE_DEV_ENROLLMENT_SHA256` explícito.
