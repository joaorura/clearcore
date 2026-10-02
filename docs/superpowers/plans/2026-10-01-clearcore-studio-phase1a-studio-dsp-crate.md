# Clearcore Studio — Fase 1a: crate `studio-dsp` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Criar o crate `crates/studio-dsp` (lib `studio_dsp`): a cadeia DSP de acabamento "estúdio" (high-pass, EQ fixo, de-esser, compressor, AGC de loudness BS.1770, limiter com lookahead) em Rust puro, sem `unsafe` e sem alocação no caminho de áudio.

**Architecture:** Cada bloco é um módulo pequeno e testável isoladamente (`biquad`, `loudness`, `deesser`, `compressor`, `limiter`, `agc`), todos calculando em `f64` por amostra com coeficientes computados fora do laço. `chain::StudioChain` monta os blocos na ordem do spec, seleciona os parâmetros a partir de uma tabela `const` de presets (`params`) e troca de preset com crossfade linear de 1 hop (480 amostras, ~10 ms) entre dois `Path` com estado clonado, sem clique. `Off` é identidade bit a bit (retorno antecipado). `StudioControl` é um `AtomicU8` para o controle entre threads.

**Tech Stack:** Rust 1.90.0 (edition 2024, toolchain fixado em `rust-toolchain.toml`), somente `std`. `stats_alloc =0.1.10` apenas como `[dev-dependencies]` do teste de zero alocação (ver "Decisões e divergências").

**Spec:** `docs/superpowers/specs/2026-10-01-clearcore-studio-pipeline-design.md` (seções 1, 4 e 9). Este plano cobre só o crate; `StudioBackend`, IPC e UI são planos separados que dependem das interfaces públicas abaixo.

## Global Constraints

Copiadas verbatim do spec (seções 1, 4 e 9):

- `crates/studio-dsp`, Rust puro, **sem dependências externas**, `#![forbid(unsafe_code)]`, em conformidade com a política do workspace (lints `unwrap/expect/panic = deny`).
- Contrato: `StudioChain::new(Preset)`, `process(&mut self, &mut [f32; 480])` sem alocação e sem panic, `latency_samples()` (só o lookahead do limiter, ~96 amostras / 2 ms).
- Ordem: high-pass 80 Hz → EQ fixo (2 biquads) → de-esser (passa-banda 5–9 kHz com envelope) → compressor (attack/release, razão, limiar, makeup) → AGC (ITU-R BS.1770, ganho lento, alvo −16 LUFS) → limiter com lookahead e teto de −1 dBFS.
- Presets: `Off` (**passagem bit a bit idêntica**), `Natural`, `Podcast`, `Broadcast`, em tabela constante. Os valores iniciais são ponto de partida e se ajustam por escuta. Padrão do produto: `Off`.
- Segurança numérica: nunca produzir NaN/infinito (`ProcessedFrame::checked` rejeita); tratar denormais.
- Testes (sem áudio commitado): sinais sintéticos. `Off` idêntico; limiter respeita o teto; ganho do compressor bate com a fórmula; tom calibrado mede a loudness esperada (±0,1 LU); zero alocações por hop (`stats_alloc`, como o benchmark).
- Cada fase só fecha com `cargo fmt --check`, `clippy -D warnings`, `cargo test --locked --offline` e `scripts/check-offline.sh` passando, mais revisão de código.
- Execução autônoma: **sem commit e sem push** (não foram pedidos); cada tarefa termina com um checkpoint (`git status --short`).
- Em `Bypass` o áudio passa cru; em `Mute` sai zero; todo o resto só roda em `Active` (a integração é de outro plano; o crate só processa quando chamado).

Restrições do repositório que valem para toda tarefa (verificadas em `Cargo.toml`, `rust-toolchain.toml`, `.github/workflows/ci.yml`):

- `[workspace.lints]`: `unsafe_code = "forbid"`, `clippy::all = deny`, `pedantic`/`nursery = warn` (o CI usa `-D warnings`, então viram erro), `unwrap_used`/`expect_used`/`panic`/`todo`/`unimplemented = deny`. Valem também para `tests/` e `#[cfg(test)]` (não existe `clippy.toml` com exceção): teste usa `assert!`/`assert_eq!`, nunca `unwrap()`/`expect()`/`panic!`.
- `crates/workspace-policy` (teste `workspace_policy_accepts_all_registered_repository_manifests`) lê todo membro de `[workspace] members` (sem glob) e rejeita dependência de terceiros sem `default-features = false` e `[features] default` não vazio. Convenção do repo: deps pinadas com `=` e `default-features = false`; herança de lints por `[lints] workspace = true`.
- Interfaces públicas fixas (outros planos dependem delas): `HOP_SAMPLES: usize = 480`, `SAMPLE_RATE_HZ: u32 = 48_000`, `Preset { Off = 0, Natural = 1, Podcast = 2, Broadcast = 3 }` (`#[repr(u8)]`, `from_u8`, `as_u8`), `StudioChain { new, set_preset, process, latency_samples, reset }`, `StudioControl { new, set_preset, preset }`.

## Decisões e divergências

1. **Dev-dependency `stats_alloc`.** O spec manda "sem dependências externas" e, na seção de testes, `stats_alloc`. Um `#[global_allocator]` contador escrito à mão exige `unsafe impl GlobalAlloc`, e o workspace usa `unsafe_code = "forbid"` (um `forbid` de linha de comando não admite `#[allow]` no arquivo de teste). A única forma de contar alocações sem `unsafe` é `stats_alloc`, que já está no `Cargo.lock` e em cache offline (usado por `crates/tools`). Decisão: `[dependencies]` do crate fica vazio; `stats_alloc = { version = "=0.1.10", default-features = false }` entra só em `[dev-dependencies]` na Task 8.
2. **`Off` e NaN.** `Off` é identidade bit a bit, então NaN/inf na entrada passam intactos em `Off` estacionário (quem rejeita é `ProcessedFrame::checked`). Em qualquer preset ativo e durante o crossfade de/para `Off`, a entrada é sanitizada (não finito → 0, denormal → 0) e a saída nunca é NaN/inf.
3. **Latência.** `latency_samples()` retorna sempre 96 (lookahead do limiter), inclusive em `Off` (o valor é uma constante de contrato; o chamador decide se compensa).
4. **Compressor.** O detector é RMS de constante de tempo fixa (5 ms, equivalente de pico: um seno de amplitude A lê A) e o attack/release atua sobre o ganho em dB (`static_gain_db` é a fórmula L→T+(L−T)/R). Isso torna o ganho estacionário previsível para o teste da fórmula.
5. **Cargo.lock.** Criar o crate muda `[workspace] members` (raiz) e `Cargo.lock`. Como o CI usa `--locked`, o lock precisa ser atualizado uma vez, offline, por `cargo metadata --offline --format-version 1 > /dev/null` (verificado: funciona sem rede para pacote novo; `cargo update -p studio-dsp --offline` **falha** com "did not match any packages" enquanto o pacote não está no lock). Ele muda de novo ao adicionar o dev-dependency na Task 8.

## Mapa de arquivos

| Arquivo | Responsabilidade |
|---|---|
| `Cargo.toml` (raiz) | adiciona `"crates/studio-dsp"` em `members` |
| `Cargo.lock` | entrada nova `studio-dsp` (gerada, nunca à mão) |
| `crates/studio-dsp/Cargo.toml` | manifesto, herda lints e metadados do workspace |
| `crates/studio-dsp/src/lib.rs` | constantes, reexports, `#![forbid(unsafe_code)]` |
| `crates/studio-dsp/src/preset.rs` | `Preset`, `StudioControl` |
| `crates/studio-dsp/src/units.rs` | conversões dB/linear, coeficiente de constante de tempo, `flush` de denormais |
| `crates/studio-dsp/src/test_util.rs` | geradores e medidas sintéticas para testes unitários (`cfg(test)`) |
| `crates/studio-dsp/src/biquad.rs` | `Biquad` (RBJ, DF2T) |
| `crates/studio-dsp/src/loudness.rs` | K-weighting, `LoudnessMeter` (400 ms), `integrated_lufs` (offline, com gating) |
| `crates/studio-dsp/src/deesser.rs` | de-esser |
| `crates/studio-dsp/src/compressor.rs` | compressor |
| `crates/studio-dsp/src/limiter.rs` | limiter com lookahead de 96 amostras |
| `crates/studio-dsp/src/agc.rs` | AGC de loudness |
| `crates/studio-dsp/src/params.rs` | tabela `const` de presets |
| `crates/studio-dsp/src/chain.rs` | `StudioChain`, `Path`, crossfade |
| `crates/studio-dsp/tests/chain.rs` | testes de integração: Off, NaN, determinismo, troca de preset, teto |
| `crates/studio-dsp/tests/no_alloc.rs` | zero alocações por hop (`stats_alloc`, só `dev-dependencies`) |

---

### Task 1: Scaffolding do crate, `Preset` e `StudioControl`

**Files:**
- Modify: `Cargo.toml` (raiz, bloco `[workspace] members`)
- Modify: `Cargo.lock` (gerado)
- Create: `crates/studio-dsp/Cargo.toml`
- Create: `crates/studio-dsp/src/lib.rs`
- Create: `crates/studio-dsp/src/preset.rs`

**Interfaces:**
- Consumes: nada.
- Produces: `studio_dsp::{HOP_SAMPLES, SAMPLE_RATE_HZ, Preset, StudioControl}` exatamente como em "Interfaces públicas fixas". `Preset::from_u8(u8) -> Option<Preset>` e `Preset::as_u8(self) -> u8` são `const fn`; `StudioControl::new` é `const fn`.

- [ ] **Step 1: Registrar o membro no workspace**

Em `Cargo.toml` (raiz), acrescente uma linha ao final da lista `members` de crates (logo após `"crates/accelerators",`):

```toml
    "crates/accelerators",
    "crates/studio-dsp",
    "crates/app-tauri/src-tauri",
```

- [ ] **Step 2: Criar o manifesto do crate**

<!-- write: crates/studio-dsp/Cargo.toml -->
```toml
[package]
name = "studio-dsp"
version = "0.1.0"
edition = "2024"
license.workspace = true
rust-version.workspace = true
publish = false

[lints]
workspace = true
```

Não há `[dependencies]`: a política de deps (`=` e `default-features = false`) é vacuamente satisfeita.

- [ ] **Step 3: Escrever o teste que falha (somente os testes, sem implementação)**

<!-- write: crates/studio-dsp/src/lib.rs -->
```rust
#![forbid(unsafe_code)]
//! Studio DSP chain: high-pass, fixed EQ, de-esser, compressor, loudness AGC and limiter.
//!
//! Pure Rust, no external runtime dependencies, no `unsafe`, and no allocation on the audio path.
//! One hop is [`HOP_SAMPLES`] samples of mono 48 kHz audio.

mod preset;

pub use preset::{Preset, StudioControl};

/// Samples per processing hop (10 ms at 48 kHz). Matches `realtime_noise_contracts::HOP_SAMPLES`.
pub const HOP_SAMPLES: usize = 480;

/// Sample rate in hertz. Matches `realtime_noise_contracts::SAMPLE_RATE_HZ`.
pub const SAMPLE_RATE_HZ: u32 = 48_000;
```

<!-- write: crates/studio-dsp/src/preset.rs -->
```rust
#[cfg(test)]
mod tests {
    use super::{Preset, StudioControl};

    #[test]
    fn preset_round_trips_through_u8() {
        for preset in [
            Preset::Off,
            Preset::Natural,
            Preset::Podcast,
            Preset::Broadcast,
        ] {
            assert_eq!(Preset::from_u8(preset.as_u8()), Some(preset));
        }
    }

    #[test]
    fn preset_discriminants_are_stable_wire_values() {
        assert_eq!(Preset::Off.as_u8(), 0);
        assert_eq!(Preset::Natural.as_u8(), 1);
        assert_eq!(Preset::Podcast.as_u8(), 2);
        assert_eq!(Preset::Broadcast.as_u8(), 3);
    }

    #[test]
    fn preset_rejects_unknown_values() {
        assert_eq!(Preset::from_u8(4), None);
        assert_eq!(Preset::from_u8(255), None);
    }

    #[test]
    fn control_stores_and_returns_the_selected_preset() {
        let control = StudioControl::new(Preset::Off);
        assert_eq!(control.preset(), Preset::Off);
        control.set_preset(Preset::Broadcast);
        assert_eq!(control.preset(), Preset::Broadcast);
    }
}
```

- [ ] **Step 4: Atualizar o Cargo.lock offline e ver o teste falhar**

Run: `cargo metadata --offline --format-version 1 > /dev/null && git diff --stat Cargo.lock`
Expected: `Cargo.lock | 4 ++++` (um bloco `[[package]] name = "studio-dsp" version = "0.1.0"`, nada mais).

Run: `cargo test -p studio-dsp --locked --offline`
Expected: FAIL de compilação, `unresolved import preset::Preset` / `cannot find type Preset`.

- [ ] **Step 5: Implementação mínima (inserir ACIMA do `#[cfg(test)]` de `preset.rs`)**

<!-- prepend: crates/studio-dsp/src/preset.rs -->
```rust
//! Preset identifiers and the lock-free control handle shared between threads.

use std::sync::atomic::{AtomicU8, Ordering};

/// Studio finishing preset. `Off` is a bit-exact pass-through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Preset {
    /// Bit-exact pass-through; the chain does not touch the samples.
    Off = 0,
    /// Light compression, no presence boost.
    Natural = 1,
    /// 3:1 compression and +2 dB presence.
    Podcast = 2,
    /// 4:1 compression and a stronger de-esser.
    Broadcast = 3,
}

impl Preset {
    /// Decodes a wire/atomic value; `None` for unknown values.
    #[must_use]
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Off),
            1 => Some(Self::Natural),
            2 => Some(Self::Podcast),
            3 => Some(Self::Broadcast),
            _ => None,
        }
    }

    /// Encodes the preset as its `repr(u8)` discriminant.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Thread-safe preset selector: the control thread writes, the audio thread reads once per hop.
#[derive(Debug)]
pub struct StudioControl {
    preset: AtomicU8,
}

impl StudioControl {
    /// Creates a control initialised to `preset`.
    #[must_use]
    pub const fn new(preset: Preset) -> Self {
        Self {
            preset: AtomicU8::new(preset.as_u8()),
        }
    }

    /// Selects a preset; visible to the audio thread on its next [`StudioControl::preset`] call.
    pub fn set_preset(&self, preset: Preset) {
        self.preset.store(preset.as_u8(), Ordering::Release);
    }

    /// Returns the selected preset. An unrepresentable stored value falls back to `Off`.
    #[must_use]
    pub fn preset(&self) -> Preset {
        Preset::from_u8(self.preset.load(Ordering::Acquire)).unwrap_or(Preset::Off)
    }
}

```

- [ ] **Step 6: Rodar os testes e a política do workspace**

Run: `cargo test -p studio-dsp --locked --offline`
Expected: PASS (4 testes).

Run: `cargo test -p workspace-policy --locked --offline`
Expected: PASS (inclui `workspace_policy_accepts_all_registered_repository_manifests`, que agora lê o manifesto novo).

- [ ] **Step 7: Checkpoint**

Run: `git status --short`
Expected: ` M Cargo.toml`, ` M Cargo.lock`, `?? crates/studio-dsp/` (mais os arquivos de docs já não rastreados). NÃO commitar.

### Task 2: Utilitários numéricos, helpers de teste e `Biquad`

**Files:**
- Create: `crates/studio-dsp/src/units.rs`
- Create: `crates/studio-dsp/src/test_util.rs`
- Create: `crates/studio-dsp/src/biquad.rs`
- Modify: `crates/studio-dsp/src/lib.rs` (declara os módulos e o `allow` de `suboptimal_flops`)

**Interfaces:**
- Consumes: `crate::SAMPLE_RATE_HZ` (Task 1).
- Produces (todos `pub` dentro do crate, `pub(crate)` na prática porque os módulos são privados):
  - `units::{DENORMAL_FLOOR: f64, sample_rate() -> f64, db_to_linear(f64) -> f64, linear_to_db(f64) -> f64, time_constant_coefficient(ms: f64) -> f64, flush(f64) -> f64}`
  - `test_util::{sine(freq_hz: f64, amplitude: f64, len: usize) -> Vec<f64>, rms(&[f64]) -> f64, db(f64) -> f64}` (só `cfg(test)`)
  - `Biquad::{from_coefficients(b0,b1,b2,a1,a2), high_pass(freq_hz, q), band_pass(freq_hz, q), peaking(freq_hz, q, gain_db), process(&mut self, f64) -> f64, reset(&mut self), set_coefficients_from(&mut self, &Biquad)}` e, só em `cfg(test)`, `identity() -> Biquad` e `magnitude_db(&self, freq_hz) -> f64`.

Nota: enquanto os blocos não são usados pela cadeia (Task 6), o compilador emite avisos `dead_code` nas tarefas 2 a 5. Eles são esperados, não afetam `cargo test` e somem na Task 6; o gate de clippy `-D warnings` só vale a partir da Task 6 e é exigido na Task 9.

- [ ] **Step 1: Criar `units.rs` e `test_util.rs` (helpers com seus próprios testes) e declarar os módulos**

<!-- write: crates/studio-dsp/src/units.rs -->
```rust
//! Small numeric helpers shared by the DSP blocks.

use crate::SAMPLE_RATE_HZ;

/// Values below this magnitude are flushed to zero to avoid denormal slowdowns.
pub const DENORMAL_FLOOR: f64 = 1.0e-30;

/// Sample rate as `f64`.
pub fn sample_rate() -> f64 {
    f64::from(SAMPLE_RATE_HZ)
}

/// Converts decibels to a linear amplitude factor.
pub fn db_to_linear(db: f64) -> f64 {
    10.0_f64.powf(db / 20.0)
}

/// Converts a linear amplitude to decibels, flooring at -400 dB so the result is always finite.
pub fn linear_to_db(linear: f64) -> f64 {
    20.0 * linear.max(1.0e-20).log10()
}

/// One-pole smoothing coefficient `exp(-1 / (tau * fs))` for a time constant in milliseconds.
pub fn time_constant_coefficient(milliseconds: f64) -> f64 {
    (-1.0 / (milliseconds * 1.0e-3 * sample_rate())).exp()
}

/// Flushes denormal and non-finite values to zero.
pub fn flush(value: f64) -> f64 {
    if value.is_finite() && value.abs() >= DENORMAL_FLOOR {
        value
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::{db_to_linear, flush, linear_to_db, time_constant_coefficient};

    #[test]
    fn decibel_conversions_round_trip() {
        for db in [-60.0, -20.0, -1.0, 0.0, 6.0] {
            assert!((linear_to_db(db_to_linear(db)) - db).abs() < 1.0e-9);
        }
        assert!((db_to_linear(-6.0206) - 0.5).abs() < 1.0e-4);
    }

    #[test]
    fn linear_to_db_is_finite_for_zero() {
        assert!(linear_to_db(0.0).is_finite());
    }

    #[test]
    fn time_constant_reaches_one_over_e_after_tau() {
        let coefficient = time_constant_coefficient(10.0);
        let after_tau = coefficient.powi(480);
        assert!((after_tau - (-1.0_f64).exp()).abs() < 1.0e-9);
    }

    #[test]
    fn flush_removes_denormals_and_non_finite_values() {
        assert!(flush(f64::NAN).abs() < f64::MIN_POSITIVE);
        assert!(flush(f64::INFINITY).abs() < f64::MIN_POSITIVE);
        assert!(flush(1.0e-310).abs() < f64::MIN_POSITIVE);
        assert!((flush(0.5) - 0.5).abs() < f64::EPSILON);
    }
}
```

<!-- write: crates/studio-dsp/src/test_util.rs -->
```rust
//! Synthetic-signal helpers for unit tests (compiled only under `cfg(test)`).

use crate::units::sample_rate;
use std::f64::consts::TAU;

/// Generates `len` samples of a sine with the given peak amplitude.
#[allow(clippy::cast_precision_loss)]
pub fn sine(freq_hz: f64, amplitude: f64, len: usize) -> Vec<f64> {
    let step = TAU * freq_hz / sample_rate();
    (0..len)
        .map(|index| amplitude * (step * index as f64).sin())
        .collect()
}

/// Root-mean-square of a slice (0 for an empty slice).
#[allow(clippy::cast_precision_loss)]
pub fn rms(samples: &[f64]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let energy: f64 = samples.iter().map(|sample| sample * sample).sum();
    (energy / samples.len() as f64).sqrt()
}

/// Converts a linear amplitude to dB.
pub fn db(linear: f64) -> f64 {
    crate::units::linear_to_db(linear)
}

#[cfg(test)]
mod tests {
    use super::{db, rms, sine};

    #[test]
    fn sine_rms_is_amplitude_over_root_two() {
        let signal = sine(1_000.0, 0.5, 48_000);
        assert!((rms(&signal) - 0.5 / 2.0_f64.sqrt()).abs() < 1.0e-6);
    }

    #[test]
    fn db_of_one_is_zero() {
        assert!(db(1.0).abs() < 1.0e-12);
    }
}
```

<!-- write: crates/studio-dsp/src/lib.rs -->
```rust
#![forbid(unsafe_code)]
// Plain multiply/add keeps results bit-identical on every target; `mul_add` would depend on
// hardware FMA availability and fall back to a slow software routine without it.
#![allow(clippy::suboptimal_flops)]
//! Studio DSP chain: high-pass, fixed EQ, de-esser, compressor, loudness AGC and limiter.
//!
//! Pure Rust, no external runtime dependencies, no `unsafe`, and no allocation on the audio path.
//! One hop is [`HOP_SAMPLES`] samples of mono 48 kHz audio.

mod biquad;
mod preset;
#[cfg(test)]
mod test_util;
mod units;

pub use preset::{Preset, StudioControl};

/// Samples per processing hop (10 ms at 48 kHz). Matches `realtime_noise_contracts::HOP_SAMPLES`.
pub const HOP_SAMPLES: usize = 480;

/// Sample rate in hertz. Matches `realtime_noise_contracts::SAMPLE_RATE_HZ`.
pub const SAMPLE_RATE_HZ: u32 = 48_000;
```

- [ ] **Step 2: Escrever os testes do `Biquad` (arquivo só com testes)**

<!-- write: crates/studio-dsp/src/biquad.rs -->
```rust
#[cfg(test)]
mod tests {
    use super::Biquad;
    use crate::test_util::{db, rms, sine};

    const Q_BUTTERWORTH: f64 = std::f64::consts::FRAC_1_SQRT_2;

    #[test]
    fn high_pass_80_hz_is_minus_3_db_at_cutoff_and_flat_above() {
        let filter = Biquad::high_pass(80.0, Q_BUTTERWORTH);
        assert!((filter.magnitude_db(80.0) + 3.0103).abs() < 0.05);
        assert!(filter.magnitude_db(8_000.0).abs() < 0.01);
        assert!(filter.magnitude_db(20.0) < -22.0);
    }

    #[test]
    fn peaking_hits_the_requested_gain_at_centre_and_is_flat_far_away() {
        let cut = Biquad::peaking(250.0, 1.0, -3.0);
        assert!((cut.magnitude_db(250.0) + 3.0).abs() < 0.01);
        assert!(cut.magnitude_db(8_000.0).abs() < 0.1);
        let boost = Biquad::peaking(4_000.0, 0.9, 2.0);
        assert!((boost.magnitude_db(4_000.0) - 2.0).abs() < 0.01);
        assert!(boost.magnitude_db(100.0).abs() < 0.1);
    }

    #[test]
    fn band_pass_has_unity_gain_at_centre_and_rejects_low_frequencies() {
        let filter = Biquad::band_pass(6_708.0, 1.68);
        assert!(filter.magnitude_db(6_708.0).abs() < 0.01);
        assert!(filter.magnitude_db(1_000.0) < -15.0);
    }

    #[test]
    fn time_domain_output_matches_the_analytic_response() {
        let mut filter = Biquad::high_pass(80.0, Q_BUTTERWORTH);
        let expected_db = filter.magnitude_db(100.0);
        let input = sine(100.0, 0.5, 48_000);
        let output: Vec<f64> = input.iter().map(|&x| filter.process(x)).collect();
        let measured_db = db(rms(&output[24_000..])) - db(rms(&input[24_000..]));
        assert!((measured_db - expected_db).abs() < 0.01);
    }

    #[test]
    fn identity_passes_samples_unchanged_and_reset_clears_state() {
        let mut identity = Biquad::identity();
        assert!((identity.process(0.25) - 0.25).abs() < f64::EPSILON);
        let mut filter = Biquad::high_pass(80.0, Q_BUTTERWORTH);
        let first = filter.process(1.0);
        filter.process(0.5);
        filter.reset();
        assert!((filter.process(1.0) - first).abs() < f64::EPSILON);
    }

    #[test]
    fn state_decays_to_exact_zero_instead_of_denormals() {
        let mut filter = Biquad::high_pass(80.0, Q_BUTTERWORTH);
        filter.process(1.0);
        let mut last = 1.0;
        for _ in 0..2_000_000 {
            last = filter.process(0.0);
        }
        assert!(last == 0.0 || last.abs() >= f64::MIN_POSITIVE);
    }
}
```

- [ ] **Step 3: Rodar e ver falhar**

Run: `cargo test -p studio-dsp --locked --offline biquad`
Expected: FAIL de compilação, `unresolved import super::Biquad` / `no function high_pass`.

- [ ] **Step 4: Implementar o `Biquad` (inserir ACIMA do `#[cfg(test)]` de `biquad.rs`)**

<!-- prepend: crates/studio-dsp/src/biquad.rs -->
```rust
//! Second-order IIR sections from the RBJ Audio EQ Cookbook, Direct Form II Transposed.
//!
//! Coefficients are computed once at construction (never inside the sample loop). State is `f64`
//! and denormals are flushed after every update.

use crate::units::{DENORMAL_FLOOR, db_to_linear, sample_rate};
use std::f64::consts::TAU;

/// One biquad section with normalised coefficients (`a0 == 1`) and its two state variables.
#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    /// Builds a section from already-normalised coefficients (`a0 == 1`).
    pub const fn from_coefficients(b0: f64, b1: f64, b2: f64, a1: f64, a2: f64) -> Self {
        Self {
            b0,
            b1,
            b2,
            a1,
            a2,
            z1: 0.0,
            z2: 0.0,
        }
    }

    /// A section that passes the signal unchanged.
    #[cfg(test)]
    pub const fn identity() -> Self {
        Self::from_coefficients(1.0, 0.0, 0.0, 0.0, 0.0)
    }

    fn normalised(b: [f64; 3], a: [f64; 3]) -> Self {
        Self::from_coefficients(
            b[0] / a[0],
            b[1] / a[0],
            b[2] / a[0],
            a[1] / a[0],
            a[2] / a[0],
        )
    }

    /// Returns `(cos w0, alpha)` for a centre frequency and Q.
    fn trig(freq_hz: f64, q: f64) -> (f64, f64) {
        let w0 = TAU * freq_hz / sample_rate();
        (w0.cos(), w0.sin() / (2.0 * q))
    }

    /// Second-order high-pass.
    pub fn high_pass(freq_hz: f64, q: f64) -> Self {
        let (cos_w0, alpha) = Self::trig(freq_hz, q);
        let half = f64::midpoint(1.0, cos_w0);
        Self::normalised(
            [half, -2.0 * half, half],
            [1.0 + alpha, -2.0 * cos_w0, 1.0 - alpha],
        )
    }

    /// Band-pass with 0 dB gain at the centre frequency (constant peak gain form).
    pub fn band_pass(freq_hz: f64, q: f64) -> Self {
        let (cos_w0, alpha) = Self::trig(freq_hz, q);
        Self::normalised(
            [alpha, 0.0, -alpha],
            [1.0 + alpha, -2.0 * cos_w0, 1.0 - alpha],
        )
    }

    /// Peaking (bell) EQ with `gain_db` at the centre frequency.
    pub fn peaking(freq_hz: f64, q: f64, gain_db: f64) -> Self {
        let amplitude = db_to_linear(gain_db / 2.0);
        let (cos_w0, alpha) = Self::trig(freq_hz, q);
        Self::normalised(
            [
                1.0 + alpha * amplitude,
                -2.0 * cos_w0,
                1.0 - alpha * amplitude,
            ],
            [
                1.0 + alpha / amplitude,
                -2.0 * cos_w0,
                1.0 - alpha / amplitude,
            ],
        )
    }

    /// Filters one sample.
    pub fn process(&mut self, input: f64) -> f64 {
        let output = self.b0 * input + self.z1;
        self.z1 = self.b1 * input - self.a1 * output + self.z2;
        self.z2 = self.b2 * input - self.a2 * output;
        if self.z1.abs() < DENORMAL_FLOOR {
            self.z1 = 0.0;
        }
        if self.z2.abs() < DENORMAL_FLOOR {
            self.z2 = 0.0;
        }
        output
    }

    /// Clears the filter state, keeping the coefficients.
    pub const fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    /// Replaces the coefficients with those of `other`, keeping this section's state.
    pub const fn set_coefficients_from(&mut self, other: &Self) {
        self.b0 = other.b0;
        self.b1 = other.b1;
        self.b2 = other.b2;
        self.a1 = other.a1;
        self.a2 = other.a2;
    }

    /// Analytic magnitude response in dB at `freq_hz`.
    #[cfg(test)]
    pub fn magnitude_db(&self, freq_hz: f64) -> f64 {
        let w = TAU * freq_hz / sample_rate();
        let (c1, s1, c2, s2) = (w.cos(), -w.sin(), (2.0 * w).cos(), -(2.0 * w).sin());
        let num_re = self.b0 + self.b1 * c1 + self.b2 * c2;
        let num_im = self.b1 * s1 + self.b2 * s2;
        let den_re = 1.0 + self.a1 * c1 + self.a2 * c2;
        let den_im = self.a1 * s1 + self.a2 * s2;
        10.0 * ((num_re * num_re + num_im * num_im) / (den_re * den_re + den_im * den_im)).log10()
    }
}
```

- [ ] **Step 5: Rodar os testes**

Run: `cargo test -p studio-dsp --locked --offline`
Expected: PASS (16 testes: 4 de `preset`, 4 de `units`, 2 de `test_util`, 6 de `biquad`).

- [ ] **Step 6: Checkpoint**

Run: `git status --short`
Expected: apenas `Cargo.toml`, `Cargo.lock` e `crates/studio-dsp/` entre os itens novos/modificados. NÃO commitar.

### Task 3: Medidor de loudness BS.1770 (K-weighting, janela de 400 ms, medida integrada com gating)

**Files:**
- Create: `crates/studio-dsp/src/loudness.rs`
- Modify: `crates/studio-dsp/src/lib.rs` (adiciona `mod loudness;`)

**Interfaces:**
- Consumes: `Biquad::{from_coefficients, process, reset}` (Task 2), `crate::HOP_SAMPLES`.
- Produces:
  - `loudness::SILENCE_LUFS: f64` (-120.0) e `loudness::WINDOW_HOPS: usize` (40, janela de 400 ms em hops de 10 ms).
  - `loudness::KWeighting::{new() -> Self, process(&mut self, f64) -> f64, reset(&mut self)}`: filtro K-weighting do BS.1770 para 48 kHz (shelf + RLB).
  - `loudness::LoudnessMeter::{new() -> Self, push_hop(&mut self, hop: &[f64]), momentary_lufs(&self) -> f64, reset(&mut self)}`: medidor momentâneo de 400 ms, tamanho fixo, sem alocação. Antes de encher a janela, mede sobre os hops já vistos.
  - O módulo é `pub mod loudness` (os testes de integração das Tasks 7 e 8 o usam como medidor de referência); `KWeighting` e `LoudnessMeter` implementam `Default` (exigência de `clippy::new_without_default` em tipo público).
  - `loudness::integrated_lufs(signal: &[f32]) -> f64`: medida integrada com gating (absoluto -70 LUFS, relativo -10 LU). **Offline** (aloca); serve a testes e diagnóstico, nunca ao caminho de áudio.

Os coeficientes do K-weighting são os publicados na ITU-R BS.1770 para 48 kHz. A conformidade é verificada pelo teste do seno de escala plena a 997 Hz, que deve ler -3,01 LUFS (valor de referência da própria norma).

- [ ] **Step 1: Escrever os testes (arquivo só com testes)**

<!-- write: crates/studio-dsp/src/loudness.rs -->
```rust
#[cfg(test)]
mod tests {
    use super::{LoudnessMeter, SILENCE_LUFS, integrated_lufs};
    use crate::HOP_SAMPLES;
    use crate::test_util::sine;
    use crate::units::db_to_linear;

    /// Peak level in dBFS of a sine whose BS.1770 loudness at 1 kHz is `lufs`
    /// (a sine's mean square is half the squared peak: -3.0103 dB).
    fn tone_peak_db_for(lufs: f64) -> f64 {
        lufs + 3.0103
    }

    fn meter_reading(signal: &[f64]) -> f64 {
        let mut meter = LoudnessMeter::new();
        for hop in signal.chunks_exact(HOP_SAMPLES) {
            meter.push_hop(hop);
        }
        meter.momentary_lufs()
    }

    fn to_f32(signal: &[f64]) -> Vec<f32> {
        #[allow(clippy::cast_possible_truncation)]
        signal.iter().map(|&sample| sample as f32).collect()
    }

    #[test]
    fn calibrated_tone_at_minus_23_lufs_measures_minus_23_on_the_meter() {
        let amplitude = db_to_linear(tone_peak_db_for(-23.0));
        let signal = sine(1_000.0, amplitude, 48_000 * 2);
        assert!((meter_reading(&signal) + 23.0).abs() < 0.1);
    }

    #[test]
    fn calibrated_tone_at_minus_23_lufs_measures_minus_23_integrated() {
        let amplitude = db_to_linear(tone_peak_db_for(-23.0));
        let signal = to_f32(&sine(1_000.0, amplitude, 48_000 * 5));
        assert!((integrated_lufs(&signal) + 23.0).abs() < 0.1);
    }

    #[test]
    fn full_scale_997_hz_sine_reads_minus_3_01_lufs() {
        let signal = sine(997.0, 1.0, 48_000 * 2);
        assert!((meter_reading(&signal) + 3.01).abs() < 0.05);
    }

    #[test]
    fn k_weighting_attenuates_deep_bass_and_lifts_the_top_octave() {
        let amplitude = 0.1;
        let reference = meter_reading(&sine(1_000.0, amplitude, 48_000 * 2));
        let bass = meter_reading(&sine(20.0, amplitude, 48_000 * 4));
        let treble = meter_reading(&sine(10_000.0, amplitude, 48_000 * 2));
        assert!(bass < reference - 10.0, "bass {bass} reference {reference}");
        assert!(
            treble > reference + 3.0,
            "treble {treble} reference {reference}"
        );
    }

    #[test]
    fn empty_and_silent_meters_report_the_silence_floor() {
        let meter = LoudnessMeter::new();
        assert!((meter.momentary_lufs() - SILENCE_LUFS).abs() < f64::EPSILON);
        let silence = vec![0.0; HOP_SAMPLES * 10];
        assert!((meter_reading(&silence) - SILENCE_LUFS).abs() < f64::EPSILON);
    }

    #[test]
    fn window_forgets_audio_older_than_400_ms() {
        let mut meter = LoudnessMeter::new();
        let loud = sine(1_000.0, 0.5, HOP_SAMPLES * 40);
        for hop in loud.chunks_exact(HOP_SAMPLES) {
            meter.push_hop(hop);
        }
        assert!(meter.momentary_lufs() > -20.0);
        let silence = [0.0; HOP_SAMPLES];
        for _ in 0..40 {
            meter.push_hop(&silence);
        }
        // The K-weighting tail of the first silent hop is the only energy left in the window.
        assert!(meter.momentary_lufs() < -45.0);
    }

    #[test]
    fn reset_returns_the_meter_to_its_initial_state() {
        let mut meter = LoudnessMeter::new();
        meter.push_hop(&[0.5; HOP_SAMPLES]);
        meter.reset();
        assert!((meter.momentary_lufs() - SILENCE_LUFS).abs() < f64::EPSILON);
    }

    #[test]
    fn integrated_measurement_ignores_blocks_below_the_gates() {
        let loud = sine(1_000.0, db_to_linear(tone_peak_db_for(-23.0)), 48_000 * 10);
        let quiet = sine(1_000.0, db_to_linear(-100.0), 48_000 * 10);
        let mut signal = to_f32(&loud);
        signal.extend(to_f32(&quiet));
        assert!((integrated_lufs(&signal) + 23.0).abs() < 0.1);
        assert!((integrated_lufs(&vec![0.0; 48_000]) - SILENCE_LUFS).abs() < f64::EPSILON);
    }
}
```

- [ ] **Step 2: Declarar o módulo e ver falhar**

<!-- write: crates/studio-dsp/src/lib.rs -->
```rust
#![forbid(unsafe_code)]
// Plain multiply/add keeps results bit-identical on every target; `mul_add` would depend on
// hardware FMA availability and fall back to a slow software routine without it.
#![allow(clippy::suboptimal_flops)]
//! Studio DSP chain: high-pass, fixed EQ, de-esser, compressor, loudness AGC and limiter.
//!
//! Pure Rust, no external runtime dependencies, no `unsafe`, and no allocation on the audio path.
//! One hop is [`HOP_SAMPLES`] samples of mono 48 kHz audio.

mod biquad;
pub mod loudness;
mod preset;
#[cfg(test)]
mod test_util;
mod units;

pub use preset::{Preset, StudioControl};

/// Samples per processing hop (10 ms at 48 kHz). Matches `realtime_noise_contracts::HOP_SAMPLES`.
pub const HOP_SAMPLES: usize = 480;

/// Sample rate in hertz. Matches `realtime_noise_contracts::SAMPLE_RATE_HZ`.
pub const SAMPLE_RATE_HZ: u32 = 48_000;
```

Run: `cargo test -p studio-dsp --locked --offline loudness`
Expected: FAIL de compilação, `unresolved import super::LoudnessMeter` e `integrated_lufs`.

- [ ] **Step 3: Implementar (inserir ACIMA do `#[cfg(test)]` de `loudness.rs`)**

<!-- prepend: crates/studio-dsp/src/loudness.rs -->
```rust
//! ITU-R BS.1770 loudness: K-weighting, a 400 ms momentary meter and an offline gated measurement.
//!
//! The K-weighting coefficients are the ones published in BS.1770 for a 48 kHz sample rate, which
//! is the only rate this crate supports.

use crate::{HOP_SAMPLES, biquad::Biquad};

/// Loudness reported for silence (and for an empty meter), in LUFS.
pub const SILENCE_LUFS: f64 = -120.0;

/// Number of 10 ms hops in the 400 ms momentary window.
pub const WINDOW_HOPS: usize = 40;

/// Constant of the BS.1770 loudness formula: `LUFS = -0.691 + 10 * log10(mean square)`.
const LOUDNESS_OFFSET_DB: f64 = -0.691;

/// Gating block length (400 ms) and step (100 ms, 75 % overlap) of the offline measurement.
const BLOCK_SAMPLES: usize = WINDOW_HOPS * HOP_SAMPLES;
const STEP_SAMPLES: usize = BLOCK_SAMPLES / 4;

/// Absolute gate of BS.1770, in LUFS, and the relative gate offset, in LU.
const ABSOLUTE_GATE_LUFS: f64 = -70.0;
const RELATIVE_GATE_LU: f64 = -10.0;

/// Converts a mean square of K-weighted samples to LUFS.
fn mean_square_to_lufs(mean_square: f64) -> f64 {
    if mean_square <= 0.0 || !mean_square.is_finite() {
        return SILENCE_LUFS;
    }
    (LOUDNESS_OFFSET_DB + 10.0 * mean_square.log10()).max(SILENCE_LUFS)
}

/// Converts LUFS back to the mean square that produces it.
fn lufs_to_mean_square(lufs: f64) -> f64 {
    10.0_f64.powf((lufs - LOUDNESS_OFFSET_DB) / 10.0)
}

/// BS.1770 K-weighting: a high-shelf pre-filter followed by the RLB high-pass (48 kHz).
#[derive(Clone, Copy, Debug)]
pub struct KWeighting {
    shelf: Biquad,
    high_pass: Biquad,
}

impl KWeighting {
    /// Creates a K-weighting filter with cleared state.
    pub const fn new() -> Self {
        Self {
            shelf: Biquad::from_coefficients(
                1.535_124_859_586_97,
                -2.691_696_189_406_38,
                1.198_392_810_852_85,
                -1.690_659_293_182_41,
                0.732_480_774_215_85,
            ),
            high_pass: Biquad::from_coefficients(
                1.0,
                -2.0,
                1.0,
                -1.990_047_454_833_98,
                0.990_072_250_366_21,
            ),
        }
    }

    /// Weights one sample.
    pub fn process(&mut self, input: f64) -> f64 {
        self.high_pass.process(self.shelf.process(input))
    }

    /// Clears the filter state.
    pub const fn reset(&mut self) {
        self.shelf.reset();
        self.high_pass.reset();
    }
}

impl Default for KWeighting {
    fn default() -> Self {
        Self::new()
    }
}

/// Streaming momentary loudness over the last 400 ms, updated once per hop. Fixed size, no
/// allocation.
#[derive(Clone, Copy, Debug)]
pub struct LoudnessMeter {
    filter: KWeighting,
    hop_energy: [f64; WINDOW_HOPS],
    next: usize,
    filled: usize,
}

impl LoudnessMeter {
    /// Creates an empty meter.
    pub const fn new() -> Self {
        Self {
            filter: KWeighting::new(),
            hop_energy: [0.0; WINDOW_HOPS],
            next: 0,
            filled: 0,
        }
    }

    /// Feeds one hop (any non-zero length) and slides the 400 ms window.
    #[allow(clippy::cast_precision_loss)]
    pub fn push_hop(&mut self, hop: &[f64]) {
        if hop.is_empty() {
            return;
        }
        let mut energy = 0.0;
        for &sample in hop {
            let weighted = self.filter.process(sample);
            energy += weighted * weighted;
        }
        self.hop_energy[self.next] = energy / hop.len() as f64;
        self.next = (self.next + 1) % WINDOW_HOPS;
        self.filled = (self.filled + 1).min(WINDOW_HOPS);
    }

    /// Momentary loudness in LUFS over the hops seen so far (at most 400 ms);
    /// [`SILENCE_LUFS`] when empty or silent.
    #[allow(clippy::cast_precision_loss)]
    pub fn momentary_lufs(&self) -> f64 {
        if self.filled == 0 {
            return SILENCE_LUFS;
        }
        let total: f64 = self.hop_energy.iter().take(self.filled).sum();
        mean_square_to_lufs(total / self.filled as f64)
    }

    /// Clears the window and the filter state.
    pub const fn reset(&mut self) {
        self.filter.reset();
        self.hop_energy = [0.0; WINDOW_HOPS];
        self.next = 0;
        self.filled = 0;
    }
}

impl Default for LoudnessMeter {
    fn default() -> Self {
        Self::new()
    }
}

/// Integrated loudness of a whole signal with the BS.1770 two-stage gating (absolute -70 LUFS,
/// relative -10 LU). **Offline only**: it allocates, so it must never run on the audio thread.
#[allow(clippy::cast_precision_loss)]
pub fn integrated_lufs(signal: &[f32]) -> f64 {
    let mut filter = KWeighting::new();
    let energy: Vec<f64> = signal
        .iter()
        .map(|&sample| {
            let weighted = filter.process(f64::from(sample));
            weighted * weighted
        })
        .collect();

    let mut blocks = Vec::new();
    let mut start = 0;
    while start + BLOCK_SAMPLES <= energy.len() {
        let block = &energy[start..start + BLOCK_SAMPLES];
        blocks.push(block.iter().sum::<f64>() / BLOCK_SAMPLES as f64);
        start += STEP_SAMPLES;
    }

    let mean_above = |threshold: f64| -> Option<f64> {
        let kept: Vec<f64> = blocks
            .iter()
            .copied()
            .filter(|&block| block > threshold)
            .collect();
        if kept.is_empty() {
            None
        } else {
            Some(kept.iter().sum::<f64>() / kept.len() as f64)
        }
    };

    let Some(after_absolute) = mean_above(lufs_to_mean_square(ABSOLUTE_GATE_LUFS)) else {
        return SILENCE_LUFS;
    };
    let relative_gate = mean_square_to_lufs(after_absolute) + RELATIVE_GATE_LU;
    mean_above(lufs_to_mean_square(relative_gate)).map_or(SILENCE_LUFS, mean_square_to_lufs)
}
```

- [ ] **Step 4: Rodar os testes**

Run: `cargo test -p studio-dsp --locked --offline loudness`
Expected: PASS (8 testes), incluindo `calibrated_tone_at_minus_23_lufs_measures_minus_23_on_the_meter` e `..._integrated` (±0,1 LU) e `full_scale_997_hz_sine_reads_minus_3_01_lufs` (±0,05).

- [ ] **Step 5: Checkpoint**

Run: `git status --short`
Expected: sem arquivos fora de `crates/studio-dsp/`, `Cargo.toml` e `Cargo.lock`. NÃO commitar.

### Task 4: De-esser e compressor

**Files:**
- Create: `crates/studio-dsp/src/deesser.rs`
- Create: `crates/studio-dsp/src/compressor.rs`
- Modify: `crates/studio-dsp/src/lib.rs` (adiciona `mod compressor;` e `mod deesser;`)

**Interfaces:**
- Consumes: `Biquad::{band_pass, process, reset}` (Task 2); `units::{db_to_linear, linear_to_db, flush, time_constant_coefficient}` (Task 2); `test_util::{sine, rms, db}` (Task 2).
- Produces:
  - `deesser::DeEsserParams { threshold_ratio: f64, ratio: f64, max_reduction_db: f64 }` (`Clone, Copy, Debug, PartialEq`; `max_reduction_db == 0.0` desliga o efeito com bypass exato).
  - `deesser::DeEsser::{new(DeEsserParams) -> Self, set_params(&mut self, DeEsserParams), reset(&mut self), process(&mut self, f64) -> f64}`; constantes `BAND_LOW_HZ = 5_000.0`, `BAND_HIGH_HZ = 9_000.0`.
  - `compressor::CompressorParams { threshold_db: f64, ratio: f64, attack_ms: f64, release_ms: f64, makeup_db: f64 }` (`Clone, Copy, Debug, PartialEq`).
  - `compressor::static_gain_db(level_db: f64, threshold_db: f64, ratio: f64) -> f64` (a fórmula `T + (L - T) / R - L` acima do limiar, 0 abaixo).
  - `compressor::Compressor::{new(CompressorParams) -> Self, set_params(&mut self, CompressorParams), reset(&mut self), process(&mut self, f64) -> f64}`; `#[cfg(test)] gain_db(&self) -> f64`.
  - `set_params` de ambos preserva o estado (filtros, envelopes, ganho suavizado): é o que permite a troca de preset sem salto (Task 6).

Como funciona o de-esser: um passa-banda 5–9 kHz (centro geométrico 6708 Hz, Q 1,68) alimenta um seguidor de envelope de pico; um segundo seguidor acompanha o sinal inteiro. Quando `envelope_banda / envelope_total` passa de `threshold_ratio`, só o componente da banda é atenuado (`saída = entrada - (1 - ganho) * banda`), com a lei de compressor até `max_reduction_db`. Abaixo de -80 dBFS de envelope total ele não age.

Como funciona o compressor: detector de média quadrática (5 ms) lido como nível de pico equivalente de um seno; computador de ganho estático em dB; attack/release suavizam o ganho; makeup entra depois.

- [ ] **Step 1: Escrever os testes (arquivos só com testes)**

<!-- write: crates/studio-dsp/src/deesser.rs -->
```rust
#[cfg(test)]
mod tests {
    use super::{DeEsser, DeEsserParams};
    use crate::test_util::{db, rms, sine};

    const STRONG: DeEsserParams = DeEsserParams {
        threshold_ratio: 0.5,
        ratio: 4.0,
        max_reduction_db: 9.0,
    };

    fn run(params: DeEsserParams, input: &[f64]) -> Vec<f64> {
        let mut deesser = DeEsser::new(params);
        input.iter().map(|&x| deesser.process(x)).collect()
    }

    fn gain_db_between(input: &[f64], output: &[f64]) -> f64 {
        let tail = input.len() / 2;
        db(rms(&output[tail..])) - db(rms(&input[tail..]))
    }

    #[test]
    fn attenuates_a_tone_inside_the_sibilance_band() {
        let input = sine(7_000.0, 0.1, 48_000);
        let output = run(STRONG, &input);
        // ratio ~ 1 against a 0.5 threshold: 6.02 dB over, 4:1 -> about -4.5 dB.
        let gain = gain_db_between(&input, &output);
        assert!((-5.5..=-3.5).contains(&gain), "gain {gain}");
    }

    #[test]
    fn leaves_a_low_frequency_tone_untouched() {
        let input = sine(1_000.0, 0.3, 48_000);
        let output = run(STRONG, &input);
        assert!(gain_db_between(&input, &output).abs() < 0.1);
    }

    #[test]
    fn leaves_weak_sibilance_under_a_loud_vowel_untouched() {
        let vowel = sine(200.0, 0.3, 48_000);
        let hiss = sine(7_000.0, 0.05, 48_000);
        let input: Vec<f64> = vowel.iter().zip(&hiss).map(|(a, b)| a + b).collect();
        let output = run(STRONG, &input);
        assert!(gain_db_between(&input, &output).abs() < 0.3);
    }

    #[test]
    fn reduction_never_exceeds_the_configured_maximum() {
        let params = DeEsserParams {
            max_reduction_db: 2.0,
            ..STRONG
        };
        let input = sine(7_000.0, 0.1, 48_000);
        let gain = gain_db_between(&input, &run(params, &input));
        assert!(gain >= -2.6, "gain {gain}");
    }

    #[test]
    fn zero_max_reduction_is_an_exact_bypass() {
        let params = DeEsserParams {
            max_reduction_db: 0.0,
            ..STRONG
        };
        let input = sine(7_000.0, 0.1, 4_800);
        let output = run(params, &input);
        assert!(
            input
                .iter()
                .zip(&output)
                .all(|(a, b)| a.to_bits() == b.to_bits())
        );
    }

    #[test]
    fn silence_stays_silent_and_finite() {
        let output = run(STRONG, &vec![0.0; 4_800]);
        assert!(output.iter().all(|sample| sample.abs() < f64::MIN_POSITIVE));
    }
}
```

<!-- write: crates/studio-dsp/src/compressor.rs -->
```rust
#[cfg(test)]
mod tests {
    use super::{Compressor, CompressorParams, static_gain_db};
    use crate::test_util::{db, rms, sine};
    use crate::units::db_to_linear;

    const PARAMS: CompressorParams = CompressorParams {
        threshold_db: -20.0,
        ratio: 4.0,
        attack_ms: 10.0,
        release_ms: 100.0,
        makeup_db: 0.0,
    };

    fn run(params: CompressorParams, input: &[f64]) -> Vec<f64> {
        let mut compressor = Compressor::new(params);
        input.iter().map(|&x| compressor.process(x)).collect()
    }

    fn settled_gain_db(input: &[f64], output: &[f64]) -> f64 {
        let tail = input.len() * 3 / 4;
        db(rms(&output[tail..])) - db(rms(&input[tail..]))
    }

    #[test]
    fn static_curve_matches_the_formula() {
        // L = -10, T = -20, R = 4: T + (L - T) / R = -17.5 -> gain -7.5 dB.
        assert!((static_gain_db(-10.0, -20.0, 4.0) + 7.5).abs() < 1.0e-12);
        assert!(static_gain_db(-30.0, -20.0, 4.0).abs() < 1.0e-12);
        assert!(static_gain_db(-20.0, -20.0, 4.0).abs() < 1.0e-12);
        assert!(static_gain_db(-5.0, -20.0, 1.0).abs() < 1.0e-12);
    }

    #[test]
    fn steady_tone_gain_matches_the_static_curve() {
        let input = sine(1_000.0, db_to_linear(-10.0), 48_000);
        let output = run(PARAMS, &input);
        let gain = settled_gain_db(&input, &output);
        assert!((gain + 7.5).abs() < 0.25, "gain {gain}");
    }

    #[test]
    fn tone_below_threshold_is_not_compressed() {
        let input = sine(1_000.0, db_to_linear(-40.0), 48_000);
        let gain = settled_gain_db(&input, &run(PARAMS, &input));
        assert!(gain.abs() < 0.01, "gain {gain}");
    }

    #[test]
    fn makeup_gain_is_added_after_compression() {
        let params = CompressorParams {
            makeup_db: 3.0,
            ..PARAMS
        };
        let input = sine(1_000.0, db_to_linear(-10.0), 48_000);
        let gain = settled_gain_db(&input, &run(params, &input));
        assert!((gain + 4.5).abs() < 0.25, "gain {gain}");
    }

    #[test]
    fn attack_is_gradual_and_release_returns_to_unity() {
        let mut compressor = Compressor::new(PARAMS);
        let loud = sine(1_000.0, db_to_linear(-10.0), 48_000);
        let mut at_1ms = 0.0;
        let mut at_100ms = 0.0;
        for (index, &sample) in loud.iter().enumerate() {
            compressor.process(sample);
            if index == 47 {
                at_1ms = compressor.gain_db();
            }
            if index == 4_799 {
                at_100ms = compressor.gain_db();
            }
        }
        let settled = compressor.gain_db();
        assert!(at_1ms > at_100ms && at_100ms > settled - 0.5);
        assert!(at_1ms > -1.0, "1 ms gain {at_1ms}");
        for _ in 0..48_000 {
            compressor.process(0.0);
        }
        assert!(compressor.gain_db() > -0.1);
    }

    #[test]
    fn changing_parameters_keeps_the_smoothed_gain() {
        let mut compressor = Compressor::new(PARAMS);
        for &sample in &sine(1_000.0, db_to_linear(-10.0), 48_000) {
            compressor.process(sample);
        }
        let before = compressor.gain_db();
        compressor.set_params(CompressorParams {
            ratio: 3.0,
            ..PARAMS
        });
        assert!((compressor.gain_db() - before).abs() < f64::EPSILON);
    }
}
```

- [ ] **Step 2: Declarar os módulos e ver falhar**

<!-- write: crates/studio-dsp/src/lib.rs -->
```rust
#![forbid(unsafe_code)]
// Plain multiply/add keeps results bit-identical on every target; `mul_add` would depend on
// hardware FMA availability and fall back to a slow software routine without it.
#![allow(clippy::suboptimal_flops)]
//! Studio DSP chain: high-pass, fixed EQ, de-esser, compressor, loudness AGC and limiter.
//!
//! Pure Rust, no external runtime dependencies, no `unsafe`, and no allocation on the audio path.
//! One hop is [`HOP_SAMPLES`] samples of mono 48 kHz audio.

mod biquad;
mod compressor;
mod deesser;
pub mod loudness;
mod preset;
#[cfg(test)]
mod test_util;
mod units;

pub use preset::{Preset, StudioControl};

/// Samples per processing hop (10 ms at 48 kHz). Matches `realtime_noise_contracts::HOP_SAMPLES`.
pub const HOP_SAMPLES: usize = 480;

/// Sample rate in hertz. Matches `realtime_noise_contracts::SAMPLE_RATE_HZ`.
pub const SAMPLE_RATE_HZ: u32 = 48_000;
```

Run: `cargo test -p studio-dsp --locked --offline`
Expected: FAIL de compilação (`unresolved import super::DeEsser`, `super::Compressor`, `super::static_gain_db`).

- [ ] **Step 3: Implementar o de-esser (inserir ACIMA do `#[cfg(test)]` de `deesser.rs`)**

<!-- prepend: crates/studio-dsp/src/deesser.rs -->
```rust
//! De-esser: attenuates only the 5-9 kHz band, and only when that band dominates the signal.
//!
//! A band-pass (5-9 kHz) feeds a peak envelope follower; a second follower tracks the broadband
//! signal. When `band_envelope / broadband_envelope` exceeds a relative threshold, the band
//! component (not the whole signal) is attenuated with a compressor-style law, up to
//! `max_reduction_db`. The output is `input - (1 - gain) * band`.

use crate::biquad::Biquad;
use crate::units::{db_to_linear, flush, linear_to_db, time_constant_coefficient};

/// Lower and upper edge of the sibilance band, in hertz.
pub const BAND_LOW_HZ: f64 = 5_000.0;
pub const BAND_HIGH_HZ: f64 = 9_000.0;

/// Envelope attack and release, in milliseconds (shared by the band and broadband followers so a
/// pure in-band tone reads a ratio of 1).
const ATTACK_MS: f64 = 1.0;
const RELEASE_MS: f64 = 20.0;

/// Below this broadband envelope (about -80 dBFS) the de-esser does nothing.
const ACTIVITY_FLOOR: f64 = 1.0e-4;

/// De-esser settings. `max_reduction_db == 0.0` disables the effect (exact bypass).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeEsserParams {
    /// Band-to-broadband envelope ratio above which attenuation starts (0..1).
    pub threshold_ratio: f64,
    /// Compression ratio applied to the excess (1 = none).
    pub ratio: f64,
    /// Largest attenuation of the band, in dB (>= 0).
    pub max_reduction_db: f64,
}

/// Band-limited de-esser.
#[derive(Clone, Copy, Debug)]
pub struct DeEsser {
    band: Biquad,
    params: DeEsserParams,
    band_envelope: f64,
    broadband_envelope: f64,
    attack: f64,
    release: f64,
}

/// One-pole peak follower with separate attack and release coefficients.
fn follow(envelope: f64, level: f64, attack: f64, release: f64) -> f64 {
    let coefficient = if level > envelope { attack } else { release };
    flush(level + coefficient * (envelope - level))
}

impl DeEsser {
    /// Creates a de-esser with cleared state.
    pub fn new(params: DeEsserParams) -> Self {
        let centre = (BAND_LOW_HZ * BAND_HIGH_HZ).sqrt();
        Self {
            band: Biquad::band_pass(centre, centre / (BAND_HIGH_HZ - BAND_LOW_HZ)),
            params,
            band_envelope: 0.0,
            broadband_envelope: 0.0,
            attack: time_constant_coefficient(ATTACK_MS),
            release: time_constant_coefficient(RELEASE_MS),
        }
    }

    /// Replaces the settings, keeping filter and envelope state (used for preset changes).
    pub const fn set_params(&mut self, params: DeEsserParams) {
        self.params = params;
    }

    /// Clears the filter and envelope state.
    pub const fn reset(&mut self) {
        self.band.reset();
        self.band_envelope = 0.0;
        self.broadband_envelope = 0.0;
    }

    /// Processes one sample.
    pub fn process(&mut self, input: f64) -> f64 {
        let band = self.band.process(input);
        self.band_envelope = follow(self.band_envelope, band.abs(), self.attack, self.release);
        self.broadband_envelope = follow(
            self.broadband_envelope,
            input.abs(),
            self.attack,
            self.release,
        );
        input - (1.0 - self.band_gain()) * band
    }

    /// Linear gain to apply to the band component for the current envelopes.
    fn band_gain(&self) -> f64 {
        let params = &self.params;
        if params.max_reduction_db <= 0.0 || self.broadband_envelope < ACTIVITY_FLOOR {
            return 1.0;
        }
        let ratio = self.band_envelope / self.broadband_envelope;
        if ratio <= params.threshold_ratio {
            return 1.0;
        }
        let over_db = linear_to_db(ratio / params.threshold_ratio);
        let reduction_db =
            (over_db * (1.0 - 1.0 / params.ratio.max(1.0))).min(params.max_reduction_db);
        db_to_linear(-reduction_db)
    }
}
```

- [ ] **Step 4: Implementar o compressor (inserir ACIMA do `#[cfg(test)]` de `compressor.rs`)**

<!-- prepend: crates/studio-dsp/src/compressor.rs -->
```rust
//! Feed-forward compressor.
//!
//! The detector is a mean-square follower with a fixed 5 ms time constant, reported as the
//! peak-equivalent level of a sine (`10 log10(ms) + 3.0103`), so a sine of amplitude A reads A.
//! The gain computer is the textbook static curve `L -> T + (L - T) / R` above the threshold `T`,
//! and attack/release smooth the resulting gain (in dB). Makeup is added after smoothing.

use crate::units::{db_to_linear, flush, time_constant_coefficient};

/// Detector time constant, in milliseconds.
const DETECTOR_MS: f64 = 5.0;

/// Peak-equivalent offset of a sine: `10 * log10(2)`.
const SINE_PEAK_OFFSET_DB: f64 = 3.010_299_956_639_812;

/// Compressor settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompressorParams {
    /// Level above which compression starts, in dB (peak-equivalent).
    pub threshold_db: f64,
    /// Compression ratio R in `R:1` (>= 1).
    pub ratio: f64,
    /// Time to bite into rising level, in milliseconds.
    pub attack_ms: f64,
    /// Time to let go after the level falls, in milliseconds.
    pub release_ms: f64,
    /// Gain added after compression, in dB.
    pub makeup_db: f64,
}

/// Static gain, in dB, applied to a signal at `level_db`: `T + (L - T) / R - L` above the
/// threshold, `0` at or below it.
pub fn static_gain_db(level_db: f64, threshold_db: f64, ratio: f64) -> f64 {
    if level_db > threshold_db {
        threshold_db + (level_db - threshold_db) / ratio.max(1.0) - level_db
    } else {
        0.0
    }
}

/// Mono compressor.
#[derive(Clone, Copy, Debug)]
pub struct Compressor {
    params: CompressorParams,
    detector: f64,
    attack: f64,
    release: f64,
    mean_square: f64,
    gain_db: f64,
}

impl Compressor {
    /// Creates a compressor with cleared state.
    pub fn new(params: CompressorParams) -> Self {
        Self {
            params,
            detector: time_constant_coefficient(DETECTOR_MS),
            attack: time_constant_coefficient(params.attack_ms),
            release: time_constant_coefficient(params.release_ms),
            mean_square: 0.0,
            gain_db: 0.0,
        }
    }

    /// Replaces the settings, keeping detector and gain state (used for preset changes).
    pub fn set_params(&mut self, params: CompressorParams) {
        self.params = params;
        self.attack = time_constant_coefficient(params.attack_ms);
        self.release = time_constant_coefficient(params.release_ms);
    }

    /// Clears the detector and gain state.
    pub const fn reset(&mut self) {
        self.mean_square = 0.0;
        self.gain_db = 0.0;
    }

    /// Current smoothed gain reduction in dB, without makeup (<= 0).
    #[cfg(test)]
    pub const fn gain_db(&self) -> f64 {
        self.gain_db
    }

    /// Processes one sample.
    pub fn process(&mut self, input: f64) -> f64 {
        self.mean_square =
            flush(self.detector * self.mean_square + (1.0 - self.detector) * input * input);
        let level_db = 10.0 * self.mean_square.max(1.0e-20).log10() + SINE_PEAK_OFFSET_DB;
        let target_db = static_gain_db(level_db, self.params.threshold_db, self.params.ratio);
        let coefficient = if target_db < self.gain_db {
            self.attack
        } else {
            self.release
        };
        self.gain_db = flush(coefficient * self.gain_db + (1.0 - coefficient) * target_db);
        input * db_to_linear(self.gain_db + self.params.makeup_db)
    }
}
```

- [ ] **Step 5: Rodar os testes**

Run: `cargo test -p studio-dsp --locked --offline`
Expected: PASS (36 testes no total: 24 anteriores + 6 do de-esser + 6 do compressor). Os testes-chave: `attenuates_a_tone_inside_the_sibilance_band` (ganho entre -5,5 e -3,5 dB), `leaves_a_low_frequency_tone_untouched` (< 0,1 dB), `steady_tone_gain_matches_the_static_curve` (-7,5 dB ± 0,25 para tom de 1 kHz a -10 dBFS, T = -20, R = 4).

- [ ] **Step 6: Checkpoint**

Run: `git status --short`
Expected: apenas `Cargo.toml`, `Cargo.lock` e `crates/studio-dsp/`. NÃO commitar.

### Task 5: Limiter com lookahead e AGC de loudness

**Files:**
- Create: `crates/studio-dsp/src/limiter.rs`
- Create: `crates/studio-dsp/src/agc.rs`
- Modify: `crates/studio-dsp/src/lib.rs` (adiciona `mod agc;` e `mod limiter;`)

**Interfaces:**
- Consumes: `units::{db_to_linear, linear_to_db, flush, time_constant_coefficient, sample_rate}` e `test_util::sine` (Task 2); `loudness::LoudnessMeter::{new, push_hop, momentary_lufs, reset}` (Task 3); `crate::HOP_SAMPLES`.
- Produces:
  - `limiter::{LOOKAHEAD_SAMPLES: usize = 96, LATENCY_SAMPLES: u32 = 96, CEILING_DBFS: f64 = -1.0, CEILING_LINEAR_F32: f32}`.
  - `limiter::Limiter::{new() -> Self, process(&mut self, input: f64) -> f32, reset(&mut self)}`: `process` recebe uma amostra e devolve a amostra de 96 amostras atrás, já limitada. Entrada não finita vira silêncio. A saída **nunca** passa de `CEILING_LINEAR_F32`.
  - `agc::{TARGET_LUFS = -16.0, MAX_GAIN_DB = 12.0, MIN_GAIN_DB = -12.0, FREEZE_BELOW_LUFS = -50.0, TAU_UP_S = 3.0, TAU_DOWN_S = 1.0}`.
  - `agc::Agc::{new() -> Self, process_hop(&mut self, hop: &mut [f64; HOP_SAMPLES]), reset(&mut self)}` e, só em `cfg(test)`, `gain_db(&self) -> f64`. `process_hop` mede a loudness momentânea do hop (já incluído na janela de 400 ms), atualiza o ganho lento e o aplica no hop com rampa linear.

Por que o limiter garante o teto (prova curta, também no comentário do módulo): para cada amostra `x[k]` o ganho necessário é `g[k] = min(1, teto / |x[k]|)`. Seja `m[j]` o mínimo de `g` nas últimas 97 amostras, e a saída em `n` seja `x[n-96]` vezes a média de `m[n-96..=n]`. Cada `m[j]` nesse intervalo inclui `g[n-96]`, logo a média é no máximo `g[n-96]` e `|y| <= teto`. A rampa de ganho é linear ao longo das 96 amostras antes do pico. O release de 50 ms só pode baixar o ganho abaixo dessa média, então a garantia se mantém.

- [ ] **Step 1: Escrever os testes (arquivos só com testes)**

<!-- write: crates/studio-dsp/src/limiter.rs -->
```rust
#[cfg(test)]
mod tests {
    use super::{CEILING_DBFS, CEILING_LINEAR_F32, LATENCY_SAMPLES, LOOKAHEAD_SAMPLES, Limiter};
    use crate::test_util::sine;
    use crate::units::{db_to_linear, linear_to_db};

    fn run(input: &[f64]) -> Vec<f32> {
        let mut limiter = Limiter::new();
        input.iter().map(|&x| limiter.process(x)).collect()
    }

    fn peak_db(output: &[f32]) -> f64 {
        let peak = output.iter().fold(0.0_f32, |acc, &x| acc.max(x.abs()));
        linear_to_db(f64::from(peak))
    }

    /// Deterministic pseudo-random samples in `[-amplitude, amplitude]` (xorshift64).
    #[allow(clippy::cast_precision_loss)]
    fn noise(len: usize, amplitude: f64) -> Vec<f64> {
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        (0..len)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                amplitude * ((state >> 11) as f64 / (1_u64 << 53) as f64 * 2.0 - 1.0)
            })
            .collect()
    }

    #[test]
    fn ceiling_constant_is_at_or_below_minus_one_dbfs() {
        assert!(linear_to_db(f64::from(CEILING_LINEAR_F32)) <= CEILING_DBFS);
    }

    #[test]
    fn latency_is_the_lookahead() {
        assert_eq!(LATENCY_SAMPLES, 96);
        assert_eq!(LOOKAHEAD_SAMPLES, 96);
    }

    #[test]
    fn plus_6_db_sine_never_exceeds_the_ceiling() {
        let output = run(&sine(1_000.0, db_to_linear(6.0), 48_000));
        assert!(
            peak_db(&output) <= CEILING_DBFS,
            "peak {}",
            peak_db(&output)
        );
        assert!(peak_db(&output) > -1.5, "limiter over-attenuates");
    }

    #[test]
    fn square_wave_noise_and_impulses_never_exceed_the_ceiling() {
        let square: Vec<f64> = (0..48_000)
            .map(|i| if (i / 24) % 2 == 0 { 2.0 } else { -2.0 })
            .collect();
        let impulses: Vec<f64> = (0..48_000)
            .map(|i| if i % 1_000 == 0 { 4.0 } else { 0.0 })
            .collect();
        for signal in [square, impulses, noise(96_000, 4.0)] {
            let output = run(&signal);
            assert!(
                peak_db(&output) <= CEILING_DBFS,
                "peak {}",
                peak_db(&output)
            );
        }
    }

    #[test]
    fn signal_below_the_ceiling_is_only_delayed() {
        let input = sine(1_000.0, 0.5, 9_600);
        let output = run(&input);
        for index in LOOKAHEAD_SAMPLES..input.len() {
            #[allow(clippy::cast_possible_truncation)]
            let expected = input[index - LOOKAHEAD_SAMPLES] as f32;
            assert!((output[index] - expected).abs() <= f32::EPSILON);
        }
    }

    #[test]
    fn an_impulse_emerges_exactly_one_lookahead_later() {
        let mut input = vec![0.0; 1_000];
        input[10] = 0.5;
        let output = run(&input);
        assert!((output[10 + LOOKAHEAD_SAMPLES] - 0.5).abs() <= f32::EPSILON);
        assert!(output[10 + LOOKAHEAD_SAMPLES - 1].abs() <= f32::EPSILON);
    }

    #[test]
    fn gain_recovers_after_a_loud_burst() {
        let mut input = sine(1_000.0, 2.0, 4_800);
        input.extend(sine(1_000.0, 0.1, 24_000));
        let output = run(&input);
        let tail_peak = output[output.len() - 2_000..]
            .iter()
            .fold(0.0_f32, |acc, &x| acc.max(x.abs()));
        assert!((tail_peak - 0.1).abs() < 0.002, "tail peak {tail_peak}");
    }

    #[test]
    fn gain_reduction_has_no_audible_steps() {
        let output = run(&sine(1_000.0, db_to_linear(6.0), 48_000));
        let largest_step = output
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .fold(0.0_f32, f32::max);
        // A full-scale 1 kHz sine at the ceiling moves at most 0.89 * 2 * pi * 1000 / 48000.
        assert!(largest_step < 0.13, "largest step {largest_step}");
    }

    #[test]
    fn non_finite_input_yields_finite_output_under_the_ceiling() {
        let input = [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.3, 1.0e300];
        let output = run(&input.repeat(100));
        assert!(output.iter().all(|x| x.is_finite()));
        assert!(peak_db(&output) <= CEILING_DBFS);
    }
}
```

<!-- write: crates/studio-dsp/src/agc.rs -->
```rust
#[cfg(test)]
mod tests {
    use super::{Agc, MAX_GAIN_DB, MIN_GAIN_DB, TARGET_LUFS};
    use crate::HOP_SAMPLES;
    use crate::loudness::LoudnessMeter;
    use crate::test_util::sine;
    use crate::units::db_to_linear;

    /// Sine amplitude whose BS.1770 loudness at 1 kHz is `lufs` (peak = rms + 3.0103 dB).
    fn amplitude_for(lufs: f64) -> f64 {
        db_to_linear(lufs + 3.0103)
    }

    /// Runs `seconds` of a 1 kHz tone at `lufs` through the AGC and returns the output.
    fn run_tone(agc: &mut Agc, lufs: f64, seconds: usize) -> Vec<f64> {
        let input = sine(1_000.0, amplitude_for(lufs), 48_000 * seconds);
        run(agc, &input)
    }

    fn run(agc: &mut Agc, input: &[f64]) -> Vec<f64> {
        let mut output = Vec::with_capacity(input.len());
        for chunk in input.chunks_exact(HOP_SAMPLES) {
            let mut hop = [0.0; HOP_SAMPLES];
            hop.copy_from_slice(chunk);
            agc.process_hop(&mut hop);
            output.extend_from_slice(&hop);
        }
        output
    }

    fn lufs_of_tail(signal: &[f64]) -> f64 {
        let mut meter = LoudnessMeter::new();
        for hop in signal[signal.len() - 48_000..].chunks_exact(HOP_SAMPLES) {
            meter.push_hop(hop);
        }
        meter.momentary_lufs()
    }

    #[test]
    fn quiet_tone_is_raised_to_the_target() {
        let mut agc = Agc::new();
        let output = run_tone(&mut agc, -26.0, 40);
        assert!((lufs_of_tail(&output) - TARGET_LUFS).abs() < 0.5);
        assert!((agc.gain_db() - 10.0).abs() < 0.5);
    }

    #[test]
    fn loud_tone_is_lowered_to_the_target() {
        let mut agc = Agc::new();
        let output = run_tone(&mut agc, -8.0, 30);
        assert!((lufs_of_tail(&output) - TARGET_LUFS).abs() < 0.5);
    }

    #[test]
    fn boost_is_capped_at_the_maximum_gain() {
        let mut agc = Agc::new();
        run_tone(&mut agc, -40.0, 60);
        assert!(
            (agc.gain_db() - MAX_GAIN_DB).abs() < 0.05,
            "{}",
            agc.gain_db()
        );
    }

    #[test]
    fn cut_is_capped_at_the_minimum_gain() {
        let mut agc = Agc::new();
        run_tone(&mut agc, -2.0, 40);
        assert!(
            (agc.gain_db() - MIN_GAIN_DB).abs() < 0.05,
            "{}",
            agc.gain_db()
        );
    }

    #[test]
    fn gain_moves_slowly() {
        let mut agc = Agc::new();
        run_tone(&mut agc, -26.0, 1);
        // One second into a 10 dB correction with a 3 s time constant: well under 4 dB.
        assert!(
            agc.gain_db() > 0.0 && agc.gain_db() < 4.0,
            "{}",
            agc.gain_db()
        );
    }

    #[test]
    fn gain_is_frozen_during_silence() {
        let mut agc = Agc::new();
        run_tone(&mut agc, -26.0, 20);
        let silence = vec![0.0; 48_000];
        run(&mut agc, &silence);
        let after_one_second = agc.gain_db();
        for _ in 0..4 {
            run(&mut agc, &silence);
        }
        assert!((agc.gain_db() - after_one_second).abs() < 1.0e-12);
        assert!((agc.gain_db() - 10.0).abs() < 0.5, "{}", agc.gain_db());
    }

    #[test]
    fn first_hop_does_not_jump_in_level() {
        let mut fresh = Agc::new();
        let input = sine(1_000.0, amplitude_for(-26.0), HOP_SAMPLES);
        let output = run(&mut fresh, &input);
        let in_peak = input.iter().fold(0.0_f64, |acc, x| acc.max(x.abs()));
        let out_peak = output.iter().fold(0.0_f64, |acc, x| acc.max(x.abs()));
        assert!(out_peak / in_peak < db_to_linear(0.1));
    }

    #[test]
    fn reset_restores_unity_gain() {
        let mut agc = Agc::new();
        run_tone(&mut agc, -26.0, 5);
        agc.reset();
        assert!(agc.gain_db().abs() < f64::EPSILON);
    }
}
```

- [ ] **Step 2: Declarar os módulos e ver falhar**

<!-- write: crates/studio-dsp/src/lib.rs -->
```rust
#![forbid(unsafe_code)]
// Plain multiply/add keeps results bit-identical on every target; `mul_add` would depend on
// hardware FMA availability and fall back to a slow software routine without it.
#![allow(clippy::suboptimal_flops)]
//! Studio DSP chain: high-pass, fixed EQ, de-esser, compressor, loudness AGC and limiter.
//!
//! Pure Rust, no external runtime dependencies, no `unsafe`, and no allocation on the audio path.
//! One hop is [`HOP_SAMPLES`] samples of mono 48 kHz audio.

mod agc;
mod biquad;
mod compressor;
mod deesser;
mod limiter;
pub mod loudness;
mod preset;
#[cfg(test)]
mod test_util;
mod units;

pub use preset::{Preset, StudioControl};

/// Samples per processing hop (10 ms at 48 kHz). Matches `realtime_noise_contracts::HOP_SAMPLES`.
pub const HOP_SAMPLES: usize = 480;

/// Sample rate in hertz. Matches `realtime_noise_contracts::SAMPLE_RATE_HZ`.
pub const SAMPLE_RATE_HZ: u32 = 48_000;
```

Run: `cargo test -p studio-dsp --locked --offline`
Expected: FAIL de compilação (`unresolved import super::Limiter`, `super::Agc`, `super::MAX_GAIN_DB`).

- [ ] **Step 3: Implementar o limiter (inserir ACIMA do `#[cfg(test)]` de `limiter.rs`)**

<!-- prepend: crates/studio-dsp/src/limiter.rs -->
```rust
//! Lookahead peak limiter with a -1 dBFS ceiling.
//!
//! Let `D = LOOKAHEAD_SAMPLES`. For every input sample `x[k]` the limiter computes the gain
//! `g[k] = min(1, ceiling / |x[k]|)` needed to keep that sample under the ceiling, takes the
//! running minimum `m[j]` of `g` over the last `D + 1` samples, and averages `m` over the last
//! `D + 1` values. The output at time `n` is `x[n - D]` times that average (`s[n - D]`). Because
//! every `m[j]` with `n - D <= j <= n` is at most `g[n - D]`, the average is at most `g[n - D]`,
//! so `|y| <= ceiling` holds by construction, while the gain reaches its minimum linearly over the
//! `D` samples before the peak (no distortion-inducing step). A one-pole release then slows the
//! gain recovery; it only ever lowers the gain below that average, so the guarantee holds.

use crate::units::{db_to_linear, flush, time_constant_coefficient};

/// Lookahead, and therefore latency, in samples (2 ms at 48 kHz).
pub const LOOKAHEAD_SAMPLES: usize = 96;

/// Same value as [`LOOKAHEAD_SAMPLES`], typed `u32` for `StudioChain::latency_samples`.
pub const LATENCY_SAMPLES: u32 = 96;

const _: () = assert!(LOOKAHEAD_SAMPLES == LATENCY_SAMPLES as usize);

/// Output ceiling in dBFS.
pub const CEILING_DBFS: f64 = -1.0;

/// Ceiling as a linear amplitude in `f32`, rounded down so that `20 * log10(ceiling) <= -1.0`.
pub const CEILING_LINEAR_F32: f32 = 0.891_250_9;

/// Gain recovery time constant after a peak, in milliseconds.
const RELEASE_MS: f64 = 50.0;

const WINDOW: usize = LOOKAHEAD_SAMPLES + 1;

/// Mono lookahead limiter. Fixed size, no allocation.
#[derive(Clone, Copy, Debug)]
pub struct Limiter {
    ceiling: f64,
    release: f64,
    audio: [f64; WINDOW],
    required: [f64; WINDOW],
    minimum: [f64; WINDOW],
    position: usize,
    gain: f64,
}

impl Limiter {
    /// Creates a limiter with an empty (silent) delay line.
    pub fn new() -> Self {
        Self {
            ceiling: db_to_linear(CEILING_DBFS).min(f64::from(CEILING_LINEAR_F32)),
            release: time_constant_coefficient(RELEASE_MS),
            audio: [0.0; WINDOW],
            required: [1.0; WINDOW],
            minimum: [1.0; WINDOW],
            position: 0,
            gain: 1.0,
        }
    }

    /// Clears the delay line and the gain.
    pub const fn reset(&mut self) {
        self.audio = [0.0; WINDOW];
        self.required = [1.0; WINDOW];
        self.minimum = [1.0; WINDOW];
        self.position = 0;
        self.gain = 1.0;
    }

    /// Pushes one sample and returns the limited sample from `LOOKAHEAD_SAMPLES` ago. Non-finite
    /// input is treated as silence.
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    pub fn process(&mut self, input: f64) -> f32 {
        let input = if input.is_finite() { input } else { 0.0 };
        let magnitude = input.abs();
        let required = if magnitude > self.ceiling {
            self.ceiling / magnitude
        } else {
            1.0
        };
        self.audio[self.position] = input;
        self.required[self.position] = required;
        let minimum = self.required.iter().copied().fold(1.0, f64::min);
        self.minimum[self.position] = minimum;
        let average = self.minimum.iter().sum::<f64>() / WINDOW as f64;

        self.gain = if average < self.gain {
            average
        } else {
            flush(self.gain + (average - self.gain) * (1.0 - self.release))
        };

        let oldest = (self.position + 1) % WINDOW;
        self.position = oldest;
        let delayed = self.audio[oldest];
        ((delayed * self.gain) as f32).clamp(-CEILING_LINEAR_F32, CEILING_LINEAR_F32)
    }
}
```

- [ ] **Step 4: Implementar o AGC (inserir ACIMA do `#[cfg(test)]` de `agc.rs`)**

<!-- prepend: crates/studio-dsp/src/agc.rs -->
```rust
//! Slow loudness AGC steering the signal towards -16 LUFS.
//!
//! Once per hop the 400 ms momentary loudness (BS.1770 K-weighting) of the incoming signal is
//! measured, the desired gain `target - loudness` is clamped to `[MIN_GAIN_DB, MAX_GAIN_DB]`, and
//! the applied gain moves towards it with a slow one-pole (`TAU_UP_S` when raising, `TAU_DOWN_S`
//! when lowering). Below `FREEZE_BELOW_LUFS` the gain is frozen, so pauses and noise floor are not
//! amplified. Inside a hop the gain ramps linearly from the previous to the new value, so there
//! are no steps. All values are documented starting points to be tuned by ear.

use crate::HOP_SAMPLES;
use crate::loudness::LoudnessMeter;
use crate::units::{db_to_linear, sample_rate};

/// Loudness target, in LUFS.
pub const TARGET_LUFS: f64 = -16.0;
/// Largest boost and largest cut, in dB.
pub const MAX_GAIN_DB: f64 = 12.0;
pub const MIN_GAIN_DB: f64 = -12.0;
/// Momentary loudness below which the gain is frozen, in LUFS.
pub const FREEZE_BELOW_LUFS: f64 = -50.0;
/// Time constants of the gain, in seconds.
pub const TAU_UP_S: f64 = 3.0;
pub const TAU_DOWN_S: f64 = 1.0;

/// Per-hop smoothing coefficient for a time constant in seconds.
#[allow(clippy::cast_precision_loss)]
fn hop_coefficient(tau_seconds: f64) -> f64 {
    (-(HOP_SAMPLES as f64) / (tau_seconds * sample_rate())).exp()
}

/// Loudness AGC. Fixed size, no allocation.
#[derive(Clone, Copy, Debug)]
pub struct Agc {
    meter: LoudnessMeter,
    gain_db: f64,
    up: f64,
    down: f64,
}

impl Agc {
    /// Creates an AGC with unity gain and an empty loudness window.
    pub fn new() -> Self {
        Self {
            meter: LoudnessMeter::new(),
            gain_db: 0.0,
            up: hop_coefficient(TAU_UP_S),
            down: hop_coefficient(TAU_DOWN_S),
        }
    }

    /// Resets the gain to unity and clears the loudness window.
    pub const fn reset(&mut self) {
        self.meter.reset();
        self.gain_db = 0.0;
    }

    /// Current gain in dB.
    #[cfg(test)]
    pub const fn gain_db(&self) -> f64 {
        self.gain_db
    }

    /// Measures `hop`, updates the gain and applies it in place with a linear ramp.
    #[allow(clippy::cast_precision_loss)]
    pub fn process_hop(&mut self, hop: &mut [f64; HOP_SAMPLES]) {
        self.meter.push_hop(hop);
        let loudness = self.meter.momentary_lufs();
        let previous_gain = db_to_linear(self.gain_db);
        if loudness >= FREEZE_BELOW_LUFS {
            let desired = (TARGET_LUFS - loudness).clamp(MIN_GAIN_DB, MAX_GAIN_DB);
            let coefficient = if desired < self.gain_db {
                self.down
            } else {
                self.up
            };
            self.gain_db = coefficient * self.gain_db + (1.0 - coefficient) * desired;
        }
        let next_gain = db_to_linear(self.gain_db);
        for (index, sample) in hop.iter_mut().enumerate() {
            let fraction = (index + 1) as f64 / HOP_SAMPLES as f64;
            *sample *= previous_gain + (next_gain - previous_gain) * fraction;
        }
    }
}
```

- [ ] **Step 5: Rodar os testes**

Run: `cargo test -p studio-dsp --locked --offline`
Expected: PASS (53 testes: 36 anteriores + 9 do limiter + 8 do AGC). Os testes-chave: `plus_6_db_sine_never_exceeds_the_ceiling` e `square_wave_noise_and_impulses_never_exceed_the_ceiling` (pico ≤ -1 dBFS), `quiet_tone_is_raised_to_the_target` (-16 LUFS ± 0,5 após 40 s), `boost_is_capped_at_the_maximum_gain`, `gain_is_frozen_during_silence`.

- [ ] **Step 6: Checkpoint**

Run: `git status --short`
Expected: apenas `Cargo.toml`, `Cargo.lock` e `crates/studio-dsp/`. NÃO commitar.

### Task 6: Tabela de presets e `StudioChain` (cadeia completa, crossfade, `Off` bit-exato)

**Files:**
- Create: `crates/studio-dsp/src/params.rs`
- Create: `crates/studio-dsp/src/chain.rs`
- Modify: `crates/studio-dsp/src/lib.rs` (adiciona `mod chain;`, `mod params;` e `pub use chain::StudioChain;`)

**Interfaces:**
- Consumes: `Biquad::{high_pass, peaking, process, reset, set_coefficients_from}` (Task 2); `DeEsser::{new, set_params, reset, process}` e `DeEsserParams`, `Compressor::{new, set_params, reset, process}` e `CompressorParams` (Task 4); `Limiter::{new, process, reset}`, `LATENCY_SAMPLES`, `LOOKAHEAD_SAMPLES`, `Agc::{new, process_hop, reset}` (Task 5); `Preset` (Task 1).
- Produces:
  - `params::{EqBand { freq_hz, q, gain_db }, PresetParams { low_mid_cut: EqBand, presence: EqBand, deesser: DeEsserParams, compressor: CompressorParams }}`, as constantes `NATURAL`, `PODCAST`, `BROADCAST` e `params::for_preset(Preset) -> Option<&'static PresetParams>` (`None` para `Off`). O módulo é privado: a tabela não faz parte da API pública.
  - **API pública** (contrato fixo), reexportada na raiz: `StudioChain::new(Preset) -> Self`, `set_preset(&mut self, Preset)`, `process(&mut self, &mut [f32; HOP_SAMPLES])`, `latency_samples(&self) -> u32` (sempre 96), `reset(&mut self)`. `StudioChain` é `Clone + Copy + Debug`.

Decisões de projeto desta tarefa:

1. **Valores dos presets** (ponto de partida, a ajustar por escuta; comentário no código): `Natural` = compressão 2:1 (limiar -24 dB), corte de 1 dB em 250 Hz, sem presença, de-esser leve (3 dB máx.); `Podcast` = 3:1, corte de 2 dB em 250 Hz, +2 dB em 4 kHz, de-esser até 6 dB; `Broadcast` = 4:1 (limiar -26 dB), corte de 3 dB, +2 dB em 4 kHz, de-esser mais forte (até 9 dB, limiar relativo 0,4).
2. **Troca de preset sem clique.** Dois `Path` rodam lado a lado por exatamente um hop. O `Path` antigo mantém o estado; o novo parte de uma cópia desse estado com os parâmetros novos (`with_params`: copia filtros, envelopes, ganho do AGC e linha de atraso do limiter e só troca coeficientes e parâmetros), ou do zero quando vem de `Off`. As primeiras 96 amostras do hop ficam no sinal antigo (a linha de atraso de um `Path` novo ainda está vazia ali) e depois a saída rampa linearmente até o sinal novo, que é atingido na última amostra (rampa de 384 amostras = 8 ms dentro do hop de 10 ms). Sinal e peso são contínuos, logo não há descontinuidade.
3. **Segurança numérica.** `sanitize` zera NaN, infinito e denormal na entrada sempre que a cadeia está ativa ou em crossfade. Em `Off` estacionário, `process` retorna antes de tocar nas amostras (bit a bit idêntico, inclusive para NaN).
4. **Sem alocação e sem panic.** `Path` é `Copy` e tem tamanho fixo (os buffers do limiter e do medidor são arrays); o hop intermediário em `f64` fica na pilha; não há indexação com índice calculado fora de laços por iterador.

- [ ] **Step 1: Escrever os testes (arquivos só com testes)**

<!-- write: crates/studio-dsp/src/params.rs -->
```rust
#[cfg(test)]
mod tests {
    use super::{BROADCAST, NATURAL, PODCAST, for_preset};
    use crate::preset::Preset;

    #[test]
    fn off_has_no_parameters() {
        assert!(for_preset(Preset::Off).is_none());
    }

    #[test]
    fn each_active_preset_maps_to_its_table_entry() {
        assert_eq!(for_preset(Preset::Natural), Some(&NATURAL));
        assert_eq!(for_preset(Preset::Podcast), Some(&PODCAST));
        assert_eq!(for_preset(Preset::Broadcast), Some(&BROADCAST));
    }

    #[test]
    fn presets_follow_the_spec_character() {
        assert!(NATURAL.presence.gain_db.abs() < f64::EPSILON);
        assert!((PODCAST.compressor.ratio - 3.0).abs() < f64::EPSILON);
        assert!((PODCAST.presence.gain_db - 2.0).abs() < f64::EPSILON);
        assert!((BROADCAST.compressor.ratio - 4.0).abs() < f64::EPSILON);
        let ratios = [NATURAL, PODCAST, BROADCAST].map(|params| params.compressor.ratio);
        assert!(ratios.is_sorted());
        let de_essing = [NATURAL, PODCAST, BROADCAST].map(|params| params.deesser.max_reduction_db);
        assert!(de_essing.is_sorted());
        assert!(de_essing[2] > de_essing[1] && de_essing[1] > de_essing[0]);
    }

    #[test]
    fn every_value_is_in_a_sane_range() {
        for params in [NATURAL, PODCAST, BROADCAST] {
            assert!((200.0..=300.0).contains(&params.low_mid_cut.freq_hz));
            assert!(params.low_mid_cut.gain_db <= 0.0);
            assert!((3_000.0..=5_000.0).contains(&params.presence.freq_hz));
            assert!(params.presence.gain_db >= 0.0);
            assert!((0.0..1.0).contains(&params.deesser.threshold_ratio));
            assert!(params.deesser.ratio >= 1.0 && params.deesser.max_reduction_db >= 0.0);
            assert!(params.compressor.ratio >= 1.0);
            assert!(params.compressor.threshold_db < 0.0);
            assert!(params.compressor.attack_ms > 0.0 && params.compressor.release_ms > 0.0);
        }
    }
}
```

<!-- write: crates/studio-dsp/src/chain.rs -->
```rust
#[cfg(test)]
mod tests {
    use super::StudioChain;
    use crate::HOP_SAMPLES;
    use crate::preset::Preset;

    fn tone_hop(start: usize, amplitude: f32) -> [f32; HOP_SAMPLES] {
        let mut hop = [0.0_f32; HOP_SAMPLES];
        for (index, sample) in hop.iter_mut().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let time = (start + index) as f32 / 48_000.0;
            *sample = amplitude * (std::f32::consts::TAU * 1_000.0 * time).sin();
        }
        hop
    }

    #[test]
    fn off_leaves_every_bit_untouched_even_for_non_finite_input() {
        let mut chain = StudioChain::new(Preset::Off);
        let mut hop = tone_hop(0, 0.5);
        hop[3] = f32::NAN;
        hop[4] = f32::INFINITY;
        let before = hop.map(f32::to_bits);
        chain.process(&mut hop);
        assert_eq!(hop.map(f32::to_bits), before);
    }

    #[test]
    fn active_preset_changes_the_signal_and_keeps_it_finite() {
        let mut chain = StudioChain::new(Preset::Podcast);
        let mut changed = false;
        for hop_index in 0..20 {
            let original = tone_hop(hop_index * HOP_SAMPLES, 0.5);
            let mut hop = original;
            chain.process(&mut hop);
            assert!(hop.iter().all(|sample| sample.is_finite()));
            changed |= hop.map(f32::to_bits) != original.map(f32::to_bits);
        }
        assert!(changed);
    }

    #[test]
    fn latency_is_the_limiter_lookahead() {
        assert_eq!(StudioChain::new(Preset::Off).latency_samples(), 96);
        assert_eq!(StudioChain::new(Preset::Broadcast).latency_samples(), 96);
    }

    #[test]
    fn crossfade_to_off_completes_in_one_hop_and_then_passes_bits_through() {
        let mut chain = StudioChain::new(Preset::Podcast);
        for hop_index in 0..10 {
            chain.process(&mut tone_hop(hop_index * HOP_SAMPLES, 0.3));
        }
        chain.set_preset(Preset::Off);
        chain.process(&mut tone_hop(10 * HOP_SAMPLES, 0.3));
        let original = tone_hop(11 * HOP_SAMPLES, 0.3);
        let mut hop = original;
        chain.process(&mut hop);
        assert_eq!(hop.map(f32::to_bits), original.map(f32::to_bits));
    }

    #[test]
    fn selecting_the_current_preset_is_a_no_op() {
        let mut chain = StudioChain::new(Preset::Off);
        chain.set_preset(Preset::Off);
        let original = tone_hop(0, 0.3);
        let mut hop = original;
        chain.process(&mut hop);
        assert_eq!(hop.map(f32::to_bits), original.map(f32::to_bits));
    }

    #[test]
    fn reset_restores_the_cold_state() {
        let mut fresh = StudioChain::new(Preset::Podcast);
        let mut reused = StudioChain::new(Preset::Podcast);
        for hop_index in 0..30 {
            reused.process(&mut tone_hop(hop_index * HOP_SAMPLES, 0.7));
        }
        reused.reset();
        let mut a = tone_hop(0, 0.2);
        let mut b = a;
        fresh.process(&mut a);
        reused.process(&mut b);
        assert_eq!(a.map(f32::to_bits), b.map(f32::to_bits));
    }
}
```

- [ ] **Step 2: Declarar os módulos e ver falhar**

<!-- write: crates/studio-dsp/src/lib.rs -->
```rust
#![forbid(unsafe_code)]
// Plain multiply/add keeps results bit-identical on every target; `mul_add` would depend on
// hardware FMA availability and fall back to a slow software routine without it.
#![allow(clippy::suboptimal_flops)]
//! Studio DSP chain: high-pass, fixed EQ, de-esser, compressor, loudness AGC and limiter.
//!
//! Pure Rust, no external runtime dependencies, no `unsafe`, and no allocation on the audio path.
//! One hop is [`HOP_SAMPLES`] samples of mono 48 kHz audio.

mod agc;
mod biquad;
mod chain;
mod compressor;
mod deesser;
mod limiter;
pub mod loudness;
mod params;
mod preset;
#[cfg(test)]
mod test_util;
mod units;

pub use chain::StudioChain;
pub use preset::{Preset, StudioControl};

/// Samples per processing hop (10 ms at 48 kHz). Matches `realtime_noise_contracts::HOP_SAMPLES`.
pub const HOP_SAMPLES: usize = 480;

/// Sample rate in hertz. Matches `realtime_noise_contracts::SAMPLE_RATE_HZ`.
pub const SAMPLE_RATE_HZ: u32 = 48_000;
```

Run: `cargo test -p studio-dsp --locked --offline`
Expected: FAIL de compilação (`unresolved import super::StudioChain`, `super::for_preset`).

- [ ] **Step 3: Implementar a tabela de presets (inserir ACIMA do `#[cfg(test)]` de `params.rs`)**

<!-- prepend: crates/studio-dsp/src/params.rs -->
```rust
//! Preset parameter table.
//!
//! Every value below is a documented **starting point** to be tuned by ear, not a measured
//! optimum. `Off` has no entry: it bypasses the chain entirely.

use crate::compressor::CompressorParams;
use crate::deesser::DeEsserParams;
use crate::preset::Preset;

/// One bell filter of the fixed EQ.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EqBand {
    pub freq_hz: f64,
    pub q: f64,
    /// Gain at the centre frequency in dB (negative cuts, positive boosts).
    pub gain_db: f64,
}

/// Everything a preset configures.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PresetParams {
    /// Bell around 250 Hz that cuts low-mid boxiness.
    pub low_mid_cut: EqBand,
    /// Bell around 4 kHz that adds presence.
    pub presence: EqBand,
    pub deesser: DeEsserParams,
    pub compressor: CompressorParams,
}

/// Light compression, mild low-mid cut, no presence boost, gentle de-esser.
pub const NATURAL: PresetParams = PresetParams {
    low_mid_cut: EqBand {
        freq_hz: 250.0,
        q: 0.9,
        gain_db: -1.0,
    },
    presence: EqBand {
        freq_hz: 4_000.0,
        q: 0.9,
        gain_db: 0.0,
    },
    deesser: DeEsserParams {
        threshold_ratio: 0.6,
        ratio: 2.0,
        max_reduction_db: 3.0,
    },
    compressor: CompressorParams {
        threshold_db: -24.0,
        ratio: 2.0,
        attack_ms: 15.0,
        release_ms: 150.0,
        makeup_db: 1.0,
    },
};

/// 3:1 compression and +2 dB of presence.
pub const PODCAST: PresetParams = PresetParams {
    low_mid_cut: EqBand {
        freq_hz: 250.0,
        q: 0.9,
        gain_db: -2.0,
    },
    presence: EqBand {
        freq_hz: 4_000.0,
        q: 0.9,
        gain_db: 2.0,
    },
    deesser: DeEsserParams {
        threshold_ratio: 0.5,
        ratio: 3.0,
        max_reduction_db: 6.0,
    },
    compressor: CompressorParams {
        threshold_db: -24.0,
        ratio: 3.0,
        attack_ms: 10.0,
        release_ms: 120.0,
        makeup_db: 2.0,
    },
};

/// 4:1 compression and the strongest de-esser.
pub const BROADCAST: PresetParams = PresetParams {
    low_mid_cut: EqBand {
        freq_hz: 250.0,
        q: 0.9,
        gain_db: -3.0,
    },
    presence: EqBand {
        freq_hz: 4_000.0,
        q: 0.9,
        gain_db: 2.0,
    },
    deesser: DeEsserParams {
        threshold_ratio: 0.4,
        ratio: 4.0,
        max_reduction_db: 9.0,
    },
    compressor: CompressorParams {
        threshold_db: -26.0,
        ratio: 4.0,
        attack_ms: 8.0,
        release_ms: 100.0,
        makeup_db: 3.0,
    },
};

/// Looks up a preset's parameters; `None` for `Off`.
pub const fn for_preset(preset: Preset) -> Option<&'static PresetParams> {
    match preset {
        Preset::Off => None,
        Preset::Natural => Some(&NATURAL),
        Preset::Podcast => Some(&PODCAST),
        Preset::Broadcast => Some(&BROADCAST),
    }
}
```

- [ ] **Step 4: Implementar a cadeia (inserir ACIMA do `#[cfg(test)]` de `chain.rs`)**

<!-- prepend: crates/studio-dsp/src/chain.rs -->
```rust
//! The studio chain: high-pass, fixed EQ, de-esser, compressor, AGC and limiter, with a click-free
//! preset switch.
//!
//! Numeric safety: whenever the chain is active (or fading to/from `Off`) the input is sanitised
//! first: non-finite samples and denormals become `0.0`. In steady `Off` the chain returns before
//! touching a single sample, so `Off` is bit-exact even for NaN or infinity; rejecting those is the
//! caller's job (`ProcessedFrame::checked`).
//!
//! Preset switch: two [`Path`]s run side by side for exactly one hop. The old path keeps its state;
//! the new path starts from a copy of that state with the new parameters (or from scratch when
//! coming from `Off`). The first [`LATENCY_SAMPLES`] samples of the hop stay on the old signal
//! (a fresh path's delay line is still empty there), then the output ramps linearly to the new
//! signal, reaching it on the last sample. Both signals are continuous and the weight is
//! continuous, so there is no click.

use crate::HOP_SAMPLES;
use crate::agc::Agc;
use crate::biquad::Biquad;
use crate::compressor::Compressor;
use crate::deesser::DeEsser;
use crate::limiter::{LATENCY_SAMPLES, LOOKAHEAD_SAMPLES, Limiter};
use crate::params::{EqBand, PresetParams, for_preset};
use crate::preset::Preset;
use std::f64::consts::FRAC_1_SQRT_2;

/// High-pass corner in hertz (second-order Butterworth).
const HIGH_PASS_HZ: f64 = 80.0;

/// Length of the part of the crossfade that actually ramps, in samples.
#[allow(clippy::cast_precision_loss)]
const RAMP_SAMPLES: f32 = (HOP_SAMPLES - LOOKAHEAD_SAMPLES) as f32;

fn bell(band: &EqBand) -> Biquad {
    Biquad::peaking(band.freq_hz, band.q, band.gain_db)
}

/// Replaces non-finite and denormal samples with zero.
fn sanitize(hop: &mut [f32; HOP_SAMPLES]) {
    for sample in hop.iter_mut() {
        if !(sample.is_finite() && sample.abs() >= f32::MIN_POSITIVE) {
            *sample = 0.0;
        }
    }
}

/// One complete signal path for one preset.
#[derive(Clone, Copy, Debug)]
struct Path {
    high_pass: Biquad,
    low_mid_cut: Biquad,
    presence: Biquad,
    deesser: DeEsser,
    compressor: Compressor,
    agc: Agc,
    limiter: Limiter,
}

impl Path {
    fn new(params: &PresetParams) -> Self {
        Self {
            high_pass: Biquad::high_pass(HIGH_PASS_HZ, FRAC_1_SQRT_2),
            low_mid_cut: bell(&params.low_mid_cut),
            presence: bell(&params.presence),
            deesser: DeEsser::new(params.deesser),
            compressor: Compressor::new(params.compressor),
            agc: Agc::new(),
            limiter: Limiter::new(),
        }
    }

    /// A copy of this path (same filter, envelope, AGC and limiter state) with new parameters.
    fn with_params(&self, params: &PresetParams) -> Self {
        let mut path = *self;
        path.low_mid_cut
            .set_coefficients_from(&bell(&params.low_mid_cut));
        path.presence.set_coefficients_from(&bell(&params.presence));
        path.deesser.set_params(params.deesser);
        path.compressor.set_params(params.compressor);
        path
    }

    const fn reset(&mut self) {
        self.high_pass.reset();
        self.low_mid_cut.reset();
        self.presence.reset();
        self.deesser.reset();
        self.compressor.reset();
        self.agc.reset();
        self.limiter.reset();
    }

    /// Processes one sanitised hop in place.
    fn process(&mut self, hop: &mut [f32; HOP_SAMPLES]) {
        let mut scratch = [0.0_f64; HOP_SAMPLES];
        for (slot, &sample) in scratch.iter_mut().zip(hop.iter()) {
            let mut value = self.high_pass.process(f64::from(sample));
            value = self.low_mid_cut.process(value);
            value = self.presence.process(value);
            value = self.deesser.process(value);
            *slot = self.compressor.process(value);
        }
        self.agc.process_hop(&mut scratch);
        for (out, &value) in hop.iter_mut().zip(scratch.iter()) {
            *out = self.limiter.process(value);
        }
    }
}

/// The studio DSP chain for one mono 48 kHz stream.
#[derive(Clone, Copy, Debug)]
pub struct StudioChain {
    target: Preset,
    main: Option<Path>,
    outgoing: Option<Path>,
    fading: bool,
}

impl StudioChain {
    /// Creates a chain running `preset` from the first hop.
    #[must_use]
    pub fn new(preset: Preset) -> Self {
        Self {
            target: preset,
            main: for_preset(preset).map(Path::new),
            outgoing: None,
            fading: false,
        }
    }

    /// Switches preset. The change is rendered with a one-hop crossfade (about 10 ms) on the next
    /// [`StudioChain::process`] call. Selecting the current preset is a no-op.
    pub fn set_preset(&mut self, preset: Preset) {
        if !self.fading && preset == self.target {
            return;
        }
        if !self.fading {
            self.outgoing = self.main.take();
            self.fading = true;
        }
        self.target = preset;
        self.main = for_preset(preset).map(|params| {
            self.outgoing
                .as_ref()
                .map_or_else(|| Path::new(params), |old| old.with_params(params))
        });
    }

    /// Processes one hop in place: no allocation, no panic.
    pub fn process(&mut self, hop: &mut [f32; HOP_SAMPLES]) {
        if !self.fading && self.main.is_none() {
            return;
        }
        sanitize(hop);
        if !self.fading {
            if let Some(path) = self.main.as_mut() {
                path.process(hop);
            }
            return;
        }

        let dry = *hop;
        let mut old = dry;
        if let Some(path) = self.outgoing.as_mut() {
            path.process(&mut old);
        }
        let mut new = dry;
        if let Some(path) = self.main.as_mut() {
            path.process(&mut new);
        }
        self.outgoing = None;
        self.fading = false;

        for (index, ((out, &old_sample), &new_sample)) in
            hop.iter_mut().zip(old.iter()).zip(new.iter()).enumerate()
        {
            let ramp = index.saturating_sub(LOOKAHEAD_SAMPLES - 1);
            #[allow(clippy::cast_precision_loss)]
            let weight = (ramp as f32 / RAMP_SAMPLES).min(1.0);
            *out = old_sample * (1.0 - weight) + new_sample * weight;
        }
    }

    /// Latency in samples: the limiter lookahead (96 = 2 ms). It is a constant of the contract and
    /// is reported even while the preset is `Off`.
    #[must_use]
    pub const fn latency_samples(&self) -> u32 {
        LATENCY_SAMPLES
    }

    /// Clears all filter, envelope, AGC and limiter state; the selected preset is kept and any
    /// pending crossfade is dropped.
    pub const fn reset(&mut self) {
        self.outgoing = None;
        self.fading = false;
        if let Some(path) = self.main.as_mut() {
            path.reset();
        }
    }
}
```

- [ ] **Step 5: Rodar testes e clippy (os avisos `dead_code` das tarefas 2 a 5 devem ter sumido)**

Run: `cargo test -p studio-dsp --locked --offline`
Expected: PASS (63 testes: 53 anteriores + 4 de `params` + 6 de `chain`).

Run: `cargo clippy -p studio-dsp --all-targets --locked --offline -- -D warnings`
Expected: sem erros nem avisos. Se aparecer `dead_code`, o item é de uso exclusivo de teste e deve ganhar `#[cfg(test)]` (é o caso de `Biquad::identity` e `Agc::gain_db`, já marcados nos blocos acima).

- [ ] **Step 6: Checkpoint**

Run: `git status --short`
Expected: apenas `Cargo.toml`, `Cargo.lock` e `crates/studio-dsp/`. NÃO commitar.

### Task 7: Testes de integração da API pública (Off, teto, loudness, troca de preset, NaN, determinismo)

**Files:**
- Create: `crates/studio-dsp/tests/chain.rs`

**Interfaces:**
- Consumes: apenas a API pública: `studio_dsp::{HOP_SAMPLES, Preset, StudioChain, StudioControl}` e `studio_dsp::loudness::integrated_lufs` (Tasks 1, 3 e 6).
- Produces: nada (só testes).

Mapa dos requisitos de teste do spec para os testes do projeto:

| Requisito | Onde |
|---|---|
| `Off` bit a bit idêntico | `off_is_bit_exact_for_every_kind_of_input` (inclui NaN, infinito, denormal, `-0.0`) e `chain::tests::off_leaves_every_bit_untouched_even_for_non_finite_input` |
| Limiter respeita o teto -1 dBFS com pico +6 dB | `output_never_exceeds_the_ceiling_with_plus_6_db_input` (3 presets x seno 1 kHz, seno 60 Hz, ruído), `ceiling_holds_across_preset_switches_between_active_presets`; unitários em `limiter::tests` |
| Ganho do compressor bate com a fórmula | `compressor::tests::static_curve_matches_the_formula` e `steady_tone_gain_matches_the_static_curve` (Task 4) |
| Tom calibrado a -23 LUFS mede -23 LUFS (±0,1 LU) | `loudness::tests::calibrated_tone_at_minus_23_lufs_*` (Task 3); no nível da cadeia, `agc_steers_a_quiet_tone_to_minus_16_lufs_through_the_whole_chain` |
| Troca de preset sem descontinuidade | `preset_switches_never_create_a_discontinuity` (as 16 trocas de/para, `Off` incluído; limiar = 1,25 x o maior degrau estacionário + 0,002) |
| Saída nunca NaN/inf (entrada NaN/inf/denormal) | `non_finite_and_denormal_input_never_reaches_the_output`, `non_finite_input_is_sanitised_during_a_crossfade_to_and_from_off` |
| Determinismo | `two_runs_produce_identical_bits` (60 hops com trocas de preset) |
| Zero alocações por hop | Task 8 |

Validação de sensibilidade feita ao escrever o plano: trocar a rampa do crossfade por uma troca seca (peso 1,0) faz `preset_switches_never_create_a_discontinuity` falhar; os demais continuam verdes.

- [ ] **Step 1: Escrever os testes**

<!-- write: crates/studio-dsp/tests/chain.rs -->
```rust
//! Integration tests of the public `studio_dsp` API with synthetic signals only.

#![forbid(unsafe_code)]
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::suboptimal_flops
)]

use std::f32::consts::TAU;

use studio_dsp::loudness::integrated_lufs;
use studio_dsp::{HOP_SAMPLES, Preset, StudioChain, StudioControl};

const ALL_PRESETS: [Preset; 4] = [
    Preset::Off,
    Preset::Natural,
    Preset::Podcast,
    Preset::Broadcast,
];
const ACTIVE_PRESETS: [Preset; 3] = [Preset::Natural, Preset::Podcast, Preset::Broadcast];

/// `10^(-1/20)` rounded down: the -1 dBFS ceiling.
const CEILING: f32 = 0.891_250_9;

fn tone(freq_hz: f32, amplitude: f32, start: usize, len: usize) -> Vec<f32> {
    (start..start + len)
        .map(|index| amplitude * (TAU * freq_hz * index as f32 / 48_000.0).sin())
        .collect()
}

/// Deterministic pseudo-random samples in `[-amplitude, amplitude]` (xorshift64).
fn noise(len: usize, amplitude: f32) -> Vec<f32> {
    let mut state: u64 = 0x2545_F491_4F6C_DD1D;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            amplitude * ((state >> 40) as f32 / (1_u64 << 24) as f32 * 2.0 - 1.0)
        })
        .collect()
}

/// Runs whole hops through the chain in place.
fn process(chain: &mut StudioChain, signal: &mut [f32]) {
    for chunk in signal.chunks_exact_mut(HOP_SAMPLES) {
        let mut hop = [0.0_f32; HOP_SAMPLES];
        hop.copy_from_slice(chunk);
        chain.process(&mut hop);
        chunk.copy_from_slice(&hop);
    }
}

fn bits(signal: &[f32]) -> Vec<u32> {
    signal.iter().map(|sample| sample.to_bits()).collect()
}

fn max_step(signal: &[f32]) -> f32 {
    signal
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .fold(0.0, f32::max)
}

fn peak(signal: &[f32]) -> f32 {
    signal
        .iter()
        .fold(0.0, |acc, &sample| acc.max(sample.abs()))
}

#[test]
fn off_is_bit_exact_for_every_kind_of_input() {
    let mut signal = noise(HOP_SAMPLES * 50, 1.5);
    signal[7] = f32::NAN;
    signal[8] = f32::INFINITY;
    signal[9] = f32::NEG_INFINITY;
    signal[10] = 1.0e-40;
    signal[11] = -0.0;
    let expected = bits(&signal);
    let mut chain = StudioChain::new(Preset::Off);
    process(&mut chain, &mut signal);
    assert_eq!(bits(&signal), expected);
}

#[test]
fn output_never_exceeds_the_ceiling_with_plus_6_db_input() {
    let loud = 2.0;
    for preset in ACTIVE_PRESETS {
        for mut signal in [
            tone(1_000.0, loud, 0, 48_000),
            tone(60.0, loud, 0, 48_000),
            noise(48_000, loud),
        ] {
            let mut chain = StudioChain::new(preset);
            process(&mut chain, &mut signal);
            assert!(
                peak(&signal) <= CEILING,
                "{preset:?} peak {}",
                peak(&signal)
            );
        }
    }
}

#[test]
fn ceiling_holds_across_preset_switches_between_active_presets() {
    let mut chain = StudioChain::new(Preset::Natural);
    let mut signal = tone(800.0, 2.0, 0, HOP_SAMPLES * 40);
    for (index, chunk) in signal.chunks_exact_mut(HOP_SAMPLES).enumerate() {
        chain.set_preset(ACTIVE_PRESETS[index % ACTIVE_PRESETS.len()]);
        process(&mut chain, chunk);
    }
    assert!(peak(&signal) <= CEILING, "peak {}", peak(&signal));
}

#[test]
fn agc_steers_a_quiet_tone_to_minus_16_lufs_through_the_whole_chain() {
    // A 1 kHz sine of peak P dBFS reads P - 3.01 LUFS: -26 LUFS is a peak of about -22.99 dBFS.
    let amplitude = 10.0_f32.powf((-26.0 + 3.0103) / 20.0);
    let mut signal = tone(1_000.0, amplitude, 0, 48_000 * 60);
    let mut chain = StudioChain::new(Preset::Podcast);
    process(&mut chain, &mut signal);
    let measured = integrated_lufs(&signal[signal.len() - 48_000 * 5..]);
    assert!((measured + 16.0).abs() < 0.3, "measured {measured}");
}

#[test]
fn preset_switches_never_create_a_discontinuity() {
    for from in ALL_PRESETS {
        for to in ALL_PRESETS {
            let mut chain = StudioChain::new(from);
            let mut before = tone(1_000.0, 0.3, 0, HOP_SAMPLES * 30);
            process(&mut chain, &mut before);
            chain.set_preset(to);
            let mut during = tone(1_000.0, 0.3, HOP_SAMPLES * 30, HOP_SAMPLES * 4);
            process(&mut chain, &mut during);

            let steady_before = max_step(&before[HOP_SAMPLES * 20..]);
            let steady_after = max_step(&during[HOP_SAMPLES * 2..]);
            let across_switch = max_step(&during);
            let limit = 1.25 * steady_before.max(steady_after) + 0.002;
            assert!(
                across_switch <= limit,
                "{from:?} -> {to:?}: step {across_switch} limit {limit}"
            );
            let join = [before[before.len() - 1], during[0]];
            assert!(max_step(&join) <= limit, "{from:?} -> {to:?} at the join");
        }
    }
}

#[test]
fn non_finite_and_denormal_input_never_reaches_the_output() {
    let poison = [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        1.0e-40,
        -1.0e-40,
        f32::MAX,
        f32::MIN,
        f32::MIN_POSITIVE / 4.0,
    ];
    for preset in ACTIVE_PRESETS {
        let mut chain = StudioChain::new(preset);
        let mut signal = tone(500.0, 0.3, 0, HOP_SAMPLES * 20);
        for (index, sample) in signal.iter_mut().enumerate() {
            if index % 7 == 0 {
                *sample = poison[(index / 7) % poison.len()];
            }
        }
        process(&mut chain, &mut signal);
        assert!(signal.iter().all(|sample| sample.is_finite()), "{preset:?}");
        assert!(peak(&signal) <= CEILING, "{preset:?}");
    }
}

#[test]
fn non_finite_input_is_sanitised_during_a_crossfade_to_and_from_off() {
    let mut chain = StudioChain::new(Preset::Off);
    chain.set_preset(Preset::Podcast);
    let mut hop = [0.2_f32; HOP_SAMPLES];
    hop[5] = f32::NAN;
    hop[300] = f32::INFINITY;
    chain.process(&mut hop);
    assert!(hop.iter().all(|sample| sample.is_finite()));

    chain.set_preset(Preset::Off);
    let mut hop = [0.2_f32; HOP_SAMPLES];
    hop[17] = f32::NEG_INFINITY;
    chain.process(&mut hop);
    assert!(hop.iter().all(|sample| sample.is_finite()));
}

fn scripted_run() -> Vec<f32> {
    let mut chain = StudioChain::new(Preset::Natural);
    let mut signal = noise(HOP_SAMPLES * 60, 0.8);
    for (index, chunk) in signal.chunks_exact_mut(HOP_SAMPLES).enumerate() {
        if index % 7 == 3 {
            chain.set_preset(ALL_PRESETS[(index / 7) % ALL_PRESETS.len()]);
        }
        process(&mut chain, chunk);
    }
    signal
}

#[test]
fn two_runs_produce_identical_bits() {
    assert_eq!(bits(&scripted_run()), bits(&scripted_run()));
}

#[test]
fn switching_twice_before_a_hop_still_crossfades_from_the_audible_preset() {
    let mut chain = StudioChain::new(Preset::Podcast);
    let mut warm = tone(1_000.0, 0.3, 0, HOP_SAMPLES * 20);
    process(&mut chain, &mut warm);
    chain.set_preset(Preset::Off);
    chain.set_preset(Preset::Broadcast);
    let mut hop = tone(1_000.0, 0.3, HOP_SAMPLES * 20, HOP_SAMPLES);
    process(&mut chain, &mut hop);
    assert!(hop.iter().all(|sample| sample.is_finite()));
    let join = [warm[warm.len() - 1], hop[0]];
    assert!(max_step(&join) < 0.05);
    assert!(max_step(&hop) < 0.05);
}

#[test]
fn latency_is_96_samples_for_every_preset() {
    for preset in ALL_PRESETS {
        assert_eq!(StudioChain::new(preset).latency_samples(), 96);
    }
}

#[test]
fn control_is_shared_across_threads() {
    let control = StudioControl::new(Preset::Off);
    std::thread::scope(|scope| {
        scope.spawn(|| control.set_preset(Preset::Broadcast));
    });
    assert_eq!(control.preset(), Preset::Broadcast);
}
```

- [ ] **Step 2: Rodar (a implementação já existe; os testes devem passar de primeira)**

Run: `cargo test -p studio-dsp --locked --offline --test chain`
Expected: PASS (11 testes; em `debug` leva cerca de 4 s, dominado pelos 60 s de áudio sintético do teste de AGC).

Se algum teste falhar, o defeito é do bloco indicado (ver tabela acima), não do teste: leia a mensagem de asserção (os testes de troca de preset imprimem `from -> to`, o degrau e o limite) e corrija o módulo correspondente.

- [ ] **Step 3: Checkpoint**

Run: `git status --short`
Expected: apenas `Cargo.toml`, `Cargo.lock` e `crates/studio-dsp/`. NÃO commitar.

### Task 8: Teste de zero alocações por hop (`stats_alloc` como dev-dependency)

**Files:**
- Modify: `crates/studio-dsp/Cargo.toml` (acrescenta `[dev-dependencies]`)
- Modify: `Cargo.lock` (gerado: a entrada `studio-dsp` ganha `dependencies = ["stats_alloc"]`)
- Create: `crates/studio-dsp/tests/no_alloc.rs`

**Interfaces:**
- Consumes: `studio_dsp::{HOP_SAMPLES, Preset, StudioChain}` (Tasks 1 e 6); `stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc}` (mesmo mecanismo de `crates/tools/src/bin/benchmark.rs`).
- Produces: nada (só o teste).

Por que `stats_alloc` e não um allocator escrito à mão: `unsafe impl GlobalAlloc` exige `unsafe`, e o workspace usa `unsafe_code = "forbid"` por linha de comando; um `#![allow(unsafe_code)]` no arquivo de teste é rejeitado (verificado com `rustc -F unsafe_code`: `error[E0453]: allow(unsafe_code) incompatible with previous forbid`). `stats_alloc 0.1.10` já está no `Cargo.lock` e no cache offline (é usado por `crates/tools`), então não entra nenhuma crate nova no grafo. Ele fica só em `[dev-dependencies]`, e `[dependencies]` do crate continua vazio.

Pontos de projeto do teste: o allocator é global ao processo, então o arquivo tem **um único** `#[test]` que mede (outro teste em paralelo poluiria os contadores); a entrada é gerada antes da medição; cada hop é medido separadamente, incluindo `set_preset` (a cada 5 hops, passando por `Off`, `Natural`, `Podcast`, `Broadcast`); um controle (`Vec::with_capacity` deve contar) prova que o contador está instalado, evitando um falso verde.

- [ ] **Step 1: Acrescentar o dev-dependency ao manifesto**

<!-- append: crates/studio-dsp/Cargo.toml -->
```toml

[dev-dependencies]
stats_alloc = { version = "=0.1.10", default-features = false }
```

Run: `cargo metadata --offline --format-version 1 > /dev/null && git diff Cargo.lock`
Expected: o bloco `studio-dsp` do `Cargo.lock` ganha exatamente `dependencies = [ "stats_alloc", ]` e nenhuma outra linha muda (o bloco `stats_alloc` já existe).

Run: `cargo test -p workspace-policy --locked --offline`
Expected: PASS (a política aceita `=0.1.10` com `default-features = false` em `[dev-dependencies]`).

- [ ] **Step 2: Escrever o teste**

<!-- write: crates/studio-dsp/tests/no_alloc.rs -->
```rust
//! Proves that `StudioChain::process` and `StudioChain::set_preset` never allocate.
//!
//! The allocator is process-wide, so this file must keep exactly one `#[test]` that measures: any
//! other test running in parallel would pollute the counters.

#![forbid(unsafe_code)]

use std::hint::black_box;

use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};
use studio_dsp::{HOP_SAMPLES, Preset, StudioChain};

#[global_allocator]
static GLOBAL: &StatsAlloc<std::alloc::System> = &INSTRUMENTED_SYSTEM;

const HOPS: usize = 600;
const PRESETS: [Preset; 4] = [
    Preset::Off,
    Preset::Natural,
    Preset::Podcast,
    Preset::Broadcast,
];

/// Deterministic pseudo-random hops in `[-1.0, 1.0]` (xorshift64), built before measuring.
fn input_hops() -> Vec<[f32; HOP_SAMPLES]> {
    let mut state: u64 = 0x1234_5678_9ABC_DEF1;
    (0..HOPS)
        .map(|_| {
            let mut hop = [0.0_f32; HOP_SAMPLES];
            for sample in &mut hop {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                *sample = f32::from((state >> 48) as u16) / 32_768.0 - 1.0;
            }
            hop
        })
        .collect()
}

fn allocations_in(work: impl FnOnce()) -> usize {
    let region = Region::new(GLOBAL);
    work();
    let change = region.change();
    change.allocations + change.reallocations
}

#[test]
fn processing_and_switching_presets_never_allocate() {
    // Control: the instrumented allocator really counts.
    let control = allocations_in(|| {
        black_box(Vec::<u8>::with_capacity(64));
    });
    assert!(control >= 1, "the counting allocator is not installed");

    let input = input_hops();
    let mut chain = StudioChain::new(Preset::Natural);
    let mut allocating_hops = 0_usize;
    let mut checksum = 0_u32;
    for (index, source) in input.iter().enumerate() {
        let mut hop = *source;
        let count = allocations_in(|| {
            if index % 5 == 0 {
                chain.set_preset(PRESETS[(index / 5) % PRESETS.len()]);
            }
            chain.process(&mut hop);
        });
        allocating_hops += usize::from(count > 0);
        checksum ^= hop[0].to_bits();
    }
    black_box(checksum);
    assert_eq!(allocating_hops, 0, "hops that allocated");
}
```

- [ ] **Step 3: Rodar**

Run: `cargo test -p studio-dsp --locked --offline --test no_alloc`
Expected: PASS (1 teste, cerca de 0,4 s). Validado ao escrever o plano: 40 execuções seguidas passaram sem oscilação, e inserir `black_box(Vec::<u8>::with_capacity(8))` em `Path::process` faz o teste falhar com `hops that allocated`.

- [ ] **Step 4: Checkpoint**

Run: `git status --short`
Expected: apenas `Cargo.toml`, `Cargo.lock` e `crates/studio-dsp/`. NÃO commitar.

### Task 9: Fechamento da fase 1a (fmt, clippy, testes, loudness, política, Cargo.lock)

**Files:**
- Modify (somente se um gate pedir): qualquer arquivo de `crates/studio-dsp/`
- Nenhum arquivo novo.

**Interfaces:**
- Consumes: tudo das Tasks 1 a 8.
- Produces: o crate `studio-dsp` verde em todos os gates da seção 9 do spec. Os planos seguintes (`StudioBackend`, IPC de preset, UI) dependem apenas da API pública: `HOP_SAMPLES`, `SAMPLE_RATE_HZ`, `Preset`, `StudioChain`, `StudioControl` (e, opcionalmente, `studio_dsp::loudness`).

Resumo exato do que esta fase muda fora de `crates/studio-dsp/`:

- `Cargo.toml` (raiz): **uma linha** em `[workspace] members`, `"crates/studio-dsp",`.
- `Cargo.lock`: **um bloco** novo, inserido em ordem alfabética, e nenhuma outra linha alterada:

```toml
[[package]]
name = "studio-dsp"
version = "0.1.0"
dependencies = [
 "stats_alloc",
]
```

O CI roda com `--locked`, que recusa um lock desatualizado. Como o ambiente é offline, o lock é atualizado uma única vez com `cargo metadata --offline --format-version 1 > /dev/null` (Tasks 1 e 8). `cargo update -p studio-dsp --offline` **não** serve: falha com "package ID specification `studio-dsp` did not match any packages" enquanto o pacote não está no lock.

- [ ] **Step 1: Formatação**

Run: `cargo fmt --all -- --check`
Expected: sem saída e código de saída 0. Se houver diferença, rode `cargo fmt -p studio-dsp` e confira com `git diff --stat` que só `crates/studio-dsp/` mudou.

- [ ] **Step 2: Clippy, no comando exato do CI restrito ao crate**

O CI (`.github/workflows/ci.yml`) roda `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings`.

Run: `cargo clippy -p studio-dsp --all-targets --locked --offline -- -D warnings`
Expected: `Finished` sem nenhum `warning:` ou `error:` (inclui `tests/` e `#[cfg(test)]`; `unwrap`/`expect`/`panic`/`unsafe` não aparecem em nenhum arquivo do crate).

Run: `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings`
Expected: o mesmo resultado para o workspace inteiro (garante que o novo membro não quebrou nada).

- [ ] **Step 3: Testes do crate**

Run: `cargo test -p studio-dsp --locked --offline`
Expected: PASS em quatro alvos: `lib` (63 testes), `tests/chain.rs` (11), `tests/no_alloc.rs` (1) e doc-tests (0). Total: 75 testes.

- [ ] **Step 4: Teste de loudness (o crate não tem binário, então no lugar de `cargo run` rode os testes de medição com saída visível)**

Run: `cargo test -p studio-dsp --locked --offline loudness -- --nocapture`
Expected: PASS nos 8 testes de `loudness::tests`, em especial `calibrated_tone_at_minus_23_lufs_measures_minus_23_on_the_meter`, `calibrated_tone_at_minus_23_lufs_measures_minus_23_integrated` (±0,1 LU) e `full_scale_997_hz_sine_reads_minus_3_01_lufs` (referência da própria BS.1770).

Run: `cargo test -p studio-dsp --locked --offline --test chain agc_steers`
Expected: PASS (1 teste): um tom de -26 LUFS atravessa o preset `Podcast` por 60 s e os últimos 5 s medem -16 LUFS ± 0,3 pelo medidor integrado do crate.

- [ ] **Step 5: Política do workspace e build do workspace**

Run: `cargo test -p workspace-policy --locked --offline`
Expected: PASS (inclui `workspace_policy_accepts_all_registered_repository_manifests`, que lê `crates/studio-dsp/Cargo.toml`).

Run: `cargo build --workspace --locked --offline`
Expected: `Finished`.

- [ ] **Step 6: Gate completo da fase (seção 9 do spec)**

Run: `cargo test --workspace --locked --offline`
Expected: PASS em todo o workspace.

Run: `./scripts/check-offline.sh`
Expected: termina com sucesso (código 0). Se imprimir `BLOCKED_OFFLINE_DEPENDENCY`, a lista de crates faltantes deve estar vazia de `studio-dsp` e de `stats_alloc`; qualquer outra crate faltante é problema do ambiente, não desta fase.

- [ ] **Step 7: Revisão de código e conferência final**

1. Despachar a revisão de código da fase (Sonnet ou Haiku, conforme a seção 9 do spec) sobre `crates/studio-dsp/`, com este plano e o spec como referência. Pontos a conferir: ordem dos blocos em `Path::process` (high-pass, EQ de 2 biquads, de-esser, compressor, AGC, limiter), ausência de alocação e de `panic` no caminho de áudio, `Off` bit-exato.
2. Conferir que `[dependencies]` de `crates/studio-dsp/Cargo.toml` não existe e que `[dev-dependencies]` tem somente `stats_alloc`.

- [ ] **Step 8: Checkpoint final**

Run: `git status --short`
Expected: `M Cargo.toml`, `M Cargo.lock`, `?? crates/studio-dsp/`, mais os arquivos de `docs/superpowers/` já não rastreados. NÃO commitar e NÃO fazer push (decisão do usuário registrada no spec, seção 9).

## Auto-revisão do plano

**Cobertura do spec (seção 4 e pedido):**

| Requisito | Tarefa |
|---|---|
| Crate `crates/studio-dsp`, Rust puro, sem deps externas (runtime), `forbid(unsafe_code)`, política do workspace | 1, 8 (dev-dep justificada), 9 |
| `HOP_SAMPLES`, `SAMPLE_RATE_HZ`, `Preset` (`from_u8`/`as_u8`), `StudioControl` | 1 |
| Biquad RBJ, DF2T, coeficientes fora do laço | 2 |
| High-pass 80 Hz, EQ fixo de 2 biquads, ordem da cadeia | 6 (`Path`) |
| De-esser (passa-banda 5-9 kHz, envelope, atenua só a banda) | 4 |
| Compressor (L→T+(L−T)/R, attack/release, makeup) | 4 |
| AGC BS.1770 (K-weighting, 400 ms, ganho lento, -16 LUFS, limites, congelamento em silêncio) | 3, 5 |
| Limiter com lookahead de 96 e teto de -1 dBFS | 5 |
| Presets em tabela `const`, `Off` bit-exato, crossfade sem clique de/para `Off` | 6 |
| `StudioChain::{new, set_preset, process, latency_samples, reset}` | 6 |
| Testes: Off, teto, fórmula do compressor, -23 LUFS ±0,1, troca de preset, NaN/inf/denormal, determinismo, zero alocações | 3, 4, 5, 6, 7, 8 |
| Fechamento: fmt, clippy `-D warnings`, test `--locked --offline`, Cargo.lock e `members` | 1, 8, 9 |

**Varredura de placeholders:** nenhum marcador de pendência no texto; todo passo de código traz o código completo, e o código dos blocos é o mesmo que compilou e passou (fmt, clippy `-D warnings`, 75 testes) num crate descartável com as lints do workspace.

**Consistência de tipos entre tarefas:** `Biquad` (Task 2) é consumido por `loudness` (3), `deesser` (4) e `chain` (6) com as mesmas assinaturas; `LoudnessMeter::push_hop(&[f64])` (3) é usado por `Agc::process_hop` (5); `Limiter::process(f64) -> f32` e `LATENCY_SAMPLES: u32` (5) são usados por `Path::process` e `StudioChain::latency_samples` (6); `DeEsserParams`/`CompressorParams` (4) compõem `PresetParams` (6); `for_preset(Preset) -> Option<&'static PresetParams>` (6) só é chamado por `StudioChain::{new, set_preset}` (6).
