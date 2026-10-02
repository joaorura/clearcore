# Fixtures de paridade do FiLM (Fase 4)

Usados por `crates/model/tests/parity_film.rs` e `crates/model/src/tract_backend.rs` (testes).
Não são assets de produto: não passam pelo gate de assinatura e nunca devem ser carregados fora de teste.

| Arquivo | SHA-256 | Origem |
|---|---|---|
| `film-identity-asset.tar.gz` | `8783baff55cad837953d5420c0f96edfbe40c01e5e2ac715cb4f3fef2b6cb45d` | `tools/accelerators/gen_film_onnx.py` sobre `vendor/approved/df-compatible-release-asset-v1.bin`: `enc.onnx` e `df_dec.onnx` ganham as entradas `gamma`/`beta` (FiLM em identidade é bit-exato). |
| `film-baked-asset.tar.gz` | `aa977fafbd81bf6df729661d76ef41e49ae5e6d5ea225d1d8f89f7bca6f18342` | Mesmo script: `gamma=1.5`, `beta=0.1` embutidos como constantes (caso G). |
| `upstream-golden-1000f.f32` | `c041ee1fa4b2ce4bdad4f07e28b8fdcb901c56bab20380d17abd254d5c9736aa` | Saída (f32 little-endian, 1000 quadros) do libDF 0.5.6 `978576aa` **sem patch**, asset aprovado, sinal sintético do relatório do spike (`docs/superpowers/plans/2026-10-01-clearcore-studio-phase3-libdf-spike-report.md`, seção 3). |

Regerar os dois assets (precisa de `onnx` e `onnxruntime`):

```
tar -xzf vendor/approved/df-compatible-release-asset-v1.bin -C /tmp/asset
python3 tools/accelerators/gen_film_onnx.py /tmp/asset/tmp/export /tmp/film-out
```

Os testes conferem o SHA-256 de cada arquivo e falham com `BLOCKED_FIXTURE_MISSING` se algum faltar.
