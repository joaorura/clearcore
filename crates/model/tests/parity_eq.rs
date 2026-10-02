//! Gancho de EQ espectral do fork vendorizado: ganho exato e latência algorítmica zero
//! (Fase 3, pergunta Q6, e plano da Fase 4, Step 6.4). O EQ multiplica o espectro realçado logo
//! antes do iSTFT do próprio modelo, então `eq_none` e `eq_flat6` têm o mesmo atraso.
#![cfg(feature = "tract")]
// O sinal sintético reproduz a aritmética do harness do spike sem fusão multiplicar-somar.
#![allow(clippy::suboptimal_flops)]

use std::{error::Error, path::PathBuf};

use deep_filter::{
    Complex32, DFState,
    tract::{DfParams, DfTract, RuntimeParams},
};
use ndarray::{Array2, ArrayView2};
use realtime_noise_model::{BandGains, NUM_ERB_BANDS, spectral_eq};

const HOP: usize = 480;
const FFT_SIZE: usize = 960;
const SAMPLE_RATE: usize = 48_000;
const N_FREQS: usize = FFT_SIZE / 2 + 1;
/// Quadros iniciais descartados nas medidas (aquecimento das GRUs e do overlap-add).
const WARMUP_FRAMES: usize = 30;
const MAX_LAG: usize = 64;

/// Largura (em bins) de cada banda ERB do modelo de 48 kHz, N=960, 32 bandas (soma 481).
const MODEL_ERB_WIDTHS: [usize; NUM_ERB_BANDS] = [
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 5, 5, 7, 7, 8, 10, 12, 13, 15, 18, 20, 24, 28, 31, 37,
    42, 50, 56, 67,
];

type TestResult = Result<(), Box<dyn Error>>;

fn approved_asset() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor/approved/df-compatible-release-asset-v1.bin")
}

fn load() -> Result<DfTract, Box<dyn Error>> {
    let params = DfParams::new(approved_asset())?;
    Ok(DfTract::new(params, &RuntimeParams::default())?)
}

fn run(model: &mut DfTract, signal: &[f32]) -> Result<Vec<f32>, Box<dyn Error>> {
    let mut out = Vec::with_capacity(signal.len());
    for frame in signal.chunks_exact(HOP) {
        let input = ArrayView2::from_shape((1, HOP), frame)?;
        let mut output = Array2::<f32>::zeros((1, HOP));
        model.process(input, output.view_mut())?;
        out.extend(output.iter());
    }
    Ok(out)
}

fn lcg_noise(state: &mut u32) -> f64 {
    *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    f64::from(*state >> 8) / 8_388_608.0 - 1.0
}

fn time_of(index: usize) -> f64 {
    f64::from(u32::try_from(index).unwrap_or(u32::MAX)) / 48_000.0
}

/// Fala sintética do spike (pilha harmônica de 140 Hz + ruído).
fn speech_like(frames: usize) -> Vec<f32> {
    let mut state: u32 = 0x1234_5678;
    (0..frames * HOP)
        .map(|index| {
            let t = time_of(index);
            let envelope = 0.5 * (1.0 + (2.0 * std::f64::consts::PI * 3.0 * t).sin());
            let voiced: f64 = (1..=12)
                .map(|h| {
                    (2.0 * std::f64::consts::PI * 140.0 * f64::from(h) * t).sin() / f64::from(h)
                })
                .sum();
            #[allow(clippy::cast_possible_truncation)]
            let sample =
                (0.08 * envelope * voiced + 0.02 * lcg_noise(&mut state)).clamp(-0.99, 0.99) as f32;
            sample
        })
        .collect()
}

fn sine(frames: usize, hz: f64) -> Vec<f32> {
    (0..frames * HOP)
        .map(|index| {
            #[allow(clippy::cast_possible_truncation)]
            let sample = (0.3 * (2.0 * std::f64::consts::PI * hz * time_of(index)).sin()) as f32;
            sample
        })
        .collect()
}

/// Fala sintética com um impulso (clique de 0,9) no meio. Um impulso sozinho sobre silêncio é
/// tratado como ruído pelo modelo e sai zerado, o que não mediria nada.
fn speech_with_impulse(frames: usize) -> Vec<f32> {
    let mut signal = speech_like(frames);
    signal[(frames / 2) * HOP + 7] = 0.9;
    signal
}

fn eq_factors(model: &DfTract, db: impl Fn(usize) -> f32) -> Result<Vec<f32>, Box<dyn Error>> {
    let mut gains = [0.0_f32; NUM_ERB_BANDS];
    for (band, gain) in gains.iter_mut().enumerate() {
        *gain = db(band);
    }
    let widths = &model.df_states[0].erb;
    Ok(spectral_eq::bin_factors(
        &BandGains::from_array(gains)?,
        widths,
    )?)
}

fn rms(samples: &[f32]) -> f64 {
    let sum: f64 = samples.iter().map(|s| f64::from(*s).powi(2)).sum();
    (sum / f64::from(u32::try_from(samples.len()).unwrap_or(u32::MAX))).sqrt()
}

fn ratio_db(a: f64, b: f64) -> f64 {
    20.0 * (b / a).log10()
}

/// Atraso (em amostras) que maximiza a correlação cruzada entre `reference` e `shifted`.
fn lag_of_max_correlation(reference: &[f32], shifted: &[f32]) -> isize {
    let n = reference.len().min(shifted.len());
    let max_lag = isize::try_from(MAX_LAG).unwrap_or(0);
    let mut best = (f64::MIN, 0_isize);
    for lag in -max_lag..=max_lag {
        let start = MAX_LAG;
        let end = n - MAX_LAG;
        let shifted_start = start.checked_add_signed(lag).unwrap_or(start);
        let sum: f64 = reference[start..end]
            .iter()
            .zip(&shifted[shifted_start..])
            .map(|(a, b)| f64::from(*a) * f64::from(*b))
            .sum();
        if sum > best.0 {
            best = (sum, lag);
        }
    }
    best.1
}

/// Energia por bin do espectro de curto prazo (mesmo STFT do modelo), somada nos quadros.
fn bin_energy(signal: &[f32]) -> Vec<f64> {
    let mut state = DFState::new(SAMPLE_RATE, FFT_SIZE, HOP, NUM_ERB_BANDS, 2);
    let mut energy = vec![0.0_f64; N_FREQS];
    let mut spec = vec![Complex32::default(); N_FREQS];
    for (index, frame) in signal.chunks_exact(HOP).enumerate() {
        state.analysis(frame, &mut spec);
        if index >= WARMUP_FRAMES {
            for (acc, bin) in energy.iter_mut().zip(&spec) {
                *acc += f64::from(bin.norm_sqr());
            }
        }
    }
    energy
}

fn band_delta_db(baseline: &[f64], eq: &[f64], low_hz: usize, high_hz: usize) -> f64 {
    let bin_hz = SAMPLE_RATE / FFT_SIZE;
    let range = (low_hz / bin_hz)..(high_hz / bin_hz).min(N_FREQS);
    let a: f64 = baseline[range.clone()].iter().sum();
    let b: f64 = eq[range].iter().sum();
    10.0 * (b / a).log10()
}

fn stimuli() -> Vec<(&'static str, Vec<f32>)> {
    vec![
        ("fala sintética", speech_like(200)),
        ("seno 1 kHz", sine(120, 1_000.0)),
        ("fala com impulso", speech_with_impulse(200)),
    ]
}

#[test]
fn model_erb_layout_matches_the_frozen_widths() -> TestResult {
    let model = load()?;
    assert_eq!(model.df_states[0].erb, MODEL_ERB_WIDTHS);
    assert_eq!(MODEL_ERB_WIDTHS.iter().sum::<usize>(), N_FREQS);
    assert_eq!(model.n_freqs, N_FREQS);
    Ok(())
}

#[test]
fn empty_eq_hook_is_bit_exact_with_no_hook() -> TestResult {
    let signal = speech_like(100);
    let baseline = run(&mut load()?, &signal)?;
    let mut model = load()?;
    model.set_spectral_eq_factors(None)?;
    assert_eq!(baseline, run(&mut model, &signal)?);
    Ok(())
}

#[test]
fn flat_6_db_is_exactly_6_db_with_zero_lag_for_every_stimulus() -> TestResult {
    for (name, signal) in stimuli() {
        let baseline = run(&mut load()?, &signal)?;
        let mut model = load()?;
        let factors = eq_factors(&model, |_| 6.0)?;
        model.set_spectral_eq_factors(Some(factors))?;
        let boosted = run(&mut model, &signal)?;

        let peak = baseline.iter().fold(0.0_f32, |acc, s| acc.max(s.abs()));
        assert!(
            peak > 1e-4,
            "{name}: o baseline não pode ser silencioso (pico {peak})"
        );
        assert!(
            boosted.iter().all(|s| s.is_finite()),
            "{name}: saída não finita"
        );

        let delta = ratio_db(rms(&baseline), rms(&boosted));
        assert!(
            (delta - 6.0).abs() < 1e-3,
            "{name}: ganho {delta:+.4} dB, esperado +6,0000 dB"
        );
        assert_eq!(
            lag_of_max_correlation(&baseline, &boosted),
            0,
            "{name}: o EQ não pode adicionar atraso"
        );
        // Proporcionalidade amostra a amostra: uma versão deslocada não satisfaria isto.
        let gain = 10.0_f32.powf(6.0 / 20.0);
        let worst = baseline
            .iter()
            .zip(&boosted)
            .fold(0.0_f32, |acc, (b, e)| acc.max((e - b * gain).abs()));
        assert!(
            worst < peak * 1e-5,
            "{name}: saída != baseline * 10^(6/20); desvio {worst}"
        );
    }
    Ok(())
}

#[test]
fn top_bands_12_db_boosts_only_the_high_frequencies_with_zero_lag() -> TestResult {
    let signal = speech_like(300);
    let baseline = run(&mut load()?, &signal)?;
    let mut model = load()?;
    // +12 dB nas 8 últimas bandas ERB (a partir de ~7,3 kHz), 0 dB no resto.
    let factors = eq_factors(
        &model,
        |band| if band >= NUM_ERB_BANDS - 8 { 12.0 } else { 0.0 },
    )?;
    model.set_spectral_eq_factors(Some(factors))?;
    let shaped = run(&mut model, &signal)?;

    let (e_base, e_shaped) = (bin_energy(&baseline), bin_energy(&shaped));
    let low = band_delta_db(&e_base, &e_shaped, 100, 1_000);
    let mid = band_delta_db(&e_base, &e_shaped, 1_000, 4_000);
    let high = band_delta_db(&e_base, &e_shaped, 12_000, 20_000);
    let top = band_delta_db(&e_base, &e_shaped, 20_000, 24_000);
    assert!(
        low.abs() < 0.05,
        "100-1000 Hz deve ficar em 0 dB, ficou {low:+.3}"
    );
    assert!(
        mid.abs() < 0.05,
        "1000-4000 Hz deve ficar em 0 dB, ficou {mid:+.3}"
    );
    assert!(
        (high - 12.0).abs() < 0.05,
        "12-20 kHz deve subir 12 dB, subiu {high:+.3}"
    );
    assert!(
        (top - 12.0).abs() < 0.05,
        "20-24 kHz deve subir 12 dB, subiu {top:+.3}"
    );
    assert_eq!(lag_of_max_correlation(&baseline, &shaped), 0);
    Ok(())
}

#[test]
fn eq_factors_with_wrong_length_are_refused() -> TestResult {
    let mut model = load()?;
    assert!(
        model
            .set_spectral_eq_factors(Some(vec![1.0; N_FREQS - 1]))
            .is_err()
    );
    Ok(())
}
