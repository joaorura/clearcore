# Realtime Noise Suppression

This repository is the implementation workspace for a first-party realtime noise-suppression virtual microphone. The approved architecture and implementation plan are under `docs/superpowers/`.

## Current gate

M0 is approved for the exact release asset `df-compatible-release-asset-v1`. Its complete provenance, legal review, matching SHA-256, full-record digest bindings, authorized Ed25519 public key, and approval signature are under `governance/model-assets/df-compatible-release-asset-v1/`; the exact archive is `vendor/approved/df-compatible-release-asset-v1.bin`.

Run the current gate with the command documented in `docs/release-gates.md`. The production record reports `M0_APPROVED`; altered, incomplete, untrusted, or inconsistent inputs report `BLOCKED_NO_APPROVED_ASSET`.

The selected production asset is the standard upstream `DeepFilterNet3_onnx.tar.gz` from `Rikorose/DeepFilterNet` tag `v0.5.6`, not `DeepFilterNet3_ll_onnx.tar.gz` and not an official Large model. Its SHA-256 is `c94d91f70911001c946e0fabb4aa9adc37045f45a03b56008cb0c8244cb63616`; the archive is redistributed byte-for-byte under an opaque release filename without conversion or quantization.

## License boundaries

Original repository code and documentation are provisionally licensed under Apache License 2.0. The project owner's explicit decision records the selected DeepFilterNet3 v0.5.6 model weights and code as `MIT OR Apache-2.0`; this owner decision is not external legal counsel. See `LICENSE` and `THIRD_PARTY_LICENSES`.
