//! Paridade do fork vendorizado do libDF (`vendor/crates/deep_filter`, `FiLM`) contra o `DFNet3`
//! upstream, reproduzindo os casos B, C, D, E e G do relatório do spike da Fase 3
//! (`docs/superpowers/plans/2026-10-01-clearcore-studio-phase3-libdf-spike-report.md`, seção 5).
//!
//! Referência upstream: saída do libDF 0.5.6 `978576aa` **sem patch** (`harness-pristine`) para o
//! sinal sintético determinístico abaixo, 1000 quadros, asset aprovado. Ela está congelada em
//! `fixtures/film/upstream-golden-1000f.f32` (SHA-256 `UPSTREAM_GOLDEN_SHA256`, o mesmo
//! `golden-pristine.f32` do relatório, seção 3). Os assets com `FiLM` saem de
//! `tools/accelerators/gen_film_onnx.py` (ver `fixtures/film/README.md`).
#![cfg(feature = "tract")]
// O gerador do sinal sintético e a tolerância do gate precisam reproduzir a aritmética do harness
// do spike bit a bit; fundir multiplicação e soma (`mul_add`) mudaria o sinal e o SHA-256 fixado.
#![allow(clippy::suboptimal_flops)]

use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};

use deep_filter::tract::{DfParams, DfTract, FilmVectors, RuntimeParams};
use ndarray::{Array2, ArrayView2};
use realtime_noise_model::{FILM_HIDDEN_DIM, FiLMVectors};
use sha2::{Digest, Sha256};

const HOP: usize = 480;
const UPSTREAM_GOLDEN_SHA256: &str =
    "c041ee1fa4b2ce4bdad4f07e28b8fdcb901c56bab20380d17abd254d5c9736aa";
const FILM_IDENTITY_ASSET_SHA256: &str =
    "8783baff55cad837953d5420c0f96edfbe40c01e5e2ac715cb4f3fef2b6cb45d";
const FILM_BAKED_ASSET_SHA256: &str =
    "aa977fafbd81bf6df729661d76ef41e49ae5e6d5ea225d1d8f89f7bca6f18342";
const GOLDEN_FRAMES: usize = 1000;
/// Sensibilidade mínima exigida do caso E (o spike mediu 1,030e-1).
const SENSITIVITY_FLOOR: f32 = 0.05;
/// Tolerância do gate do produto (`golden_reference`), usada só quando o SHA-256 não confere.
const GATE_ABS_TOL: f64 = 1e-4;
const GATE_REL_TOL: f64 = 1e-4;

type TestResult = Result<(), Box<dyn Error>>;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Falha com bloqueio explícito (nunca passa em silêncio) se o fixture faltar ou não conferir.
fn pinned_fixture(relative: &str, sha256: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = repository_root().join(relative);
    let bytes = fs::read(&path).map_err(|error| {
        format!(
            "BLOCKED_FIXTURE_MISSING: {} ({error}); regenere com tools/accelerators/gen_film_onnx.py",
            path.display()
        )
    })?;
    let actual = sha256_hex(&bytes);
    if actual != sha256 {
        return Err(format!("fixture {relative} sha256 {actual} != esperado {sha256}").into());
    }
    Ok(path)
}

/// Sinal sintético idêntico ao do harness do spike: pilha harmônica de 140 Hz, envelope de 3 Hz
/// e ruído LCG. Determinístico.
fn synthetic(frames: usize) -> Vec<f32> {
    let mut state: u32 = 0x1234_5678;
    (0..frames * HOP)
        .map(|index| {
            let t = f64::from(u32::try_from(index).unwrap_or(u32::MAX)) / 48_000.0;
            let envelope = 0.5 * (1.0 + (2.0 * std::f64::consts::PI * 3.0 * t).sin());
            let voiced: f64 = (1..=12)
                .map(|h| {
                    (2.0 * std::f64::consts::PI * 140.0 * f64::from(h) * t).sin() / f64::from(h)
                })
                .sum();
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = f64::from(state >> 8) / 8_388_608.0 - 1.0;
            #[allow(clippy::cast_possible_truncation)]
            let sample = (0.08 * envelope * voiced + 0.02 * noise).clamp(-0.99, 0.99) as f32;
            sample
        })
        .collect()
}

fn load(archive: &Path) -> Result<DfTract, Box<dyn Error>> {
    let params = DfParams::new(archive.to_path_buf())?;
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

fn to_le_bytes(samples: &[f32]) -> Vec<u8> {
    samples
        .iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect()
}

fn max_abs_diff(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "sequences must have equal length");
    a.iter()
        .zip(b)
        .fold(0.0_f32, |acc, (x, y)| acc.max((x - y).abs()))
}

fn film_from_contract(vectors: &FiLMVectors) -> FilmVectors {
    FilmVectors {
        gamma_enc: vectors.gamma_enc.clone(),
        beta_enc: vectors.beta_enc.clone(),
        gamma_df: vectors.gamma_df.clone(),
        beta_df: vectors.beta_df.clone(),
    }
}

fn constant_film(gamma: f32, beta: f32) -> FilmVectors {
    FilmVectors {
        gamma_enc: vec![gamma; FILM_HIDDEN_DIM],
        beta_enc: vec![beta; FILM_HIDDEN_DIM],
        gamma_df: vec![gamma; FILM_HIDDEN_DIM],
        beta_df: vec![beta; FILM_HIDDEN_DIM],
    }
}

fn approved_asset() -> PathBuf {
    repository_root().join("vendor/approved/df-compatible-release-asset-v1.bin")
}

fn film_identity_asset() -> Result<PathBuf, Box<dyn Error>> {
    pinned_fixture(
        "fixtures/film/film-identity-asset.tar.gz",
        FILM_IDENTITY_ASSET_SHA256,
    )
}

/// Saída do upstream sem patch: bit-exata (SHA-256 do relatório) ou, se o kernel de ponto
/// flutuante desta CPU diferir da máquina de referência, dentro da tolerância do gate.
fn assert_matches_upstream(output: &[f32], label: &str) -> TestResult {
    assert_eq!(output.len(), GOLDEN_FRAMES * HOP);
    if sha256_hex(&to_le_bytes(output)) == UPSTREAM_GOLDEN_SHA256 {
        return Ok(());
    }
    let golden_path = pinned_fixture(
        "fixtures/film/upstream-golden-1000f.f32",
        UPSTREAM_GOLDEN_SHA256,
    )?;
    let golden: Vec<f32> = fs::read(golden_path)?
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();
    assert_eq!(golden.len(), output.len());
    let worst_excess = output
        .iter()
        .zip(&golden)
        .fold(f64::MIN, |acc, (got, want)| {
            let want = f64::from(*want);
            acc.max((f64::from(*got) - want).abs() - (GATE_ABS_TOL + GATE_REL_TOL * want.abs()))
        });
    eprintln!(
        "{label}: SHA-256 difere do upstream (CPU diferente?); comparado com a tolerância do gate"
    );
    assert!(
        worst_excess <= 0.0,
        "{label}: excede a tolerância do gate em {worst_excess:e}"
    );
    Ok(())
}

#[test]
fn caso_b_fork_with_approved_asset_matches_upstream() -> TestResult {
    let mut model = load(&approved_asset())?;
    assert!(
        model.film_hidden().is_none(),
        "asset aprovado não declara FiLM"
    );
    let output = run(&mut model, &synthetic(GOLDEN_FRAMES))?;
    assert_matches_upstream(&output, "caso B")
}

#[test]
fn caso_c_film_asset_without_set_film_is_bit_exact_with_the_unconditioned_model() -> TestResult {
    let signal = synthetic(GOLDEN_FRAMES);
    let baseline = run(&mut load(&approved_asset())?, &signal)?;
    let mut film = load(&film_identity_asset()?)?;
    assert_eq!(film.film_hidden(), Some(FILM_HIDDEN_DIM));
    let output = run(&mut film, &signal)?;
    assert_eq!(
        max_abs_diff(&baseline, &output).to_bits(),
        0.0_f32.to_bits()
    );
    assert_matches_upstream(&output, "caso C")
}

#[test]
fn caso_d_explicit_identity_film_is_bit_exact_with_the_unconditioned_model() -> TestResult {
    let signal = synthetic(GOLDEN_FRAMES);
    let baseline = run(&mut load(&approved_asset())?, &signal)?;
    let mut film = load(&film_identity_asset()?)?;
    film.set_film(&film_from_contract(&FiLMVectors::identity()))?;
    let output = run(&mut film, &signal)?;
    assert_eq!(
        max_abs_diff(&baseline, &output).to_bits(),
        0.0_f32.to_bits()
    );
    assert_matches_upstream(&output, "caso D")
}

#[test]
fn caso_e_non_trivial_film_changes_the_output_and_stays_finite() -> TestResult {
    let signal = synthetic(GOLDEN_FRAMES);
    let baseline = run(&mut load(&approved_asset())?, &signal)?;
    let mut film = load(&film_identity_asset()?)?;
    film.set_film(&constant_film(1.5, 0.1))?;
    let output = run(&mut film, &signal)?;
    assert!(
        output.iter().all(|sample| sample.is_finite()),
        "saída não finita"
    );
    let diff = max_abs_diff(&baseline, &output);
    assert!(
        diff > SENSITIVITY_FLOOR,
        "FiLM 1.5/0.1 deveria divergir mais que {SENSITIVITY_FLOOR}; diff={diff}"
    );
    Ok(())
}

#[test]
fn caso_g_film_inputs_equal_constants_baked_into_the_graph() -> TestResult {
    let signal = synthetic(GOLDEN_FRAMES);
    let mut with_inputs = load(&film_identity_asset()?)?;
    with_inputs.set_film(&constant_film(1.5, 0.1))?;
    let by_inputs = run(&mut with_inputs, &signal)?;

    let baked_path = pinned_fixture(
        "fixtures/film/film-baked-asset.tar.gz",
        FILM_BAKED_ASSET_SHA256,
    )?;
    let mut baked = load(&baked_path)?;
    assert!(
        baked.film_hidden().is_none(),
        "asset baked não declara entradas FiLM"
    );
    let by_constants = run(&mut baked, &signal)?;

    assert_eq!(
        max_abs_diff(&by_inputs, &by_constants).to_bits(),
        0.0_f32.to_bits()
    );
    Ok(())
}

#[test]
fn set_film_on_a_model_without_film_inputs_is_refused() -> TestResult {
    let mut model = load(&approved_asset())?;
    assert!(
        model
            .set_film(&film_from_contract(&FiLMVectors::identity()))
            .is_err()
    );
    Ok(())
}

#[test]
fn set_film_with_wrong_dimensions_is_refused_and_keeps_the_previous_conditioning() -> TestResult {
    let signal = synthetic(40);
    let mut reference = load(&film_identity_asset()?)?;
    let expected = run(&mut reference, &signal)?;

    let mut model = load(&film_identity_asset()?)?;
    let mut bad = film_from_contract(&FiLMVectors::identity());
    bad.gamma_enc.pop();
    assert!(model.set_film(&bad).is_err());
    let output = run(&mut model, &signal)?;
    assert_eq!(
        max_abs_diff(&expected, &output).to_bits(),
        0.0_f32.to_bits()
    );
    Ok(())
}

#[test]
fn switching_film_mid_stream_keeps_gru_state() -> TestResult {
    // Identidade -> identidade no meio do fluxo não pode alterar um único bit: set_film não
    // reinicializa as GRUs. Depois, trocar para um perfil não trivial muda a saída sem NaN.
    let signal = synthetic(120);
    let (first, second) = signal.split_at(60 * HOP);

    let mut uninterrupted = load(&film_identity_asset()?)?;
    let expected = run(&mut uninterrupted, &signal)?;

    let mut switched = load(&film_identity_asset()?)?;
    let mut output = run(&mut switched, first)?;
    switched.set_film(&film_from_contract(&FiLMVectors::identity()))?;
    output.extend(run(&mut switched, second)?);
    assert_eq!(
        max_abs_diff(&expected, &output).to_bits(),
        0.0_f32.to_bits()
    );

    let mut conditioned = load(&film_identity_asset()?)?;
    let mut changed = run(&mut conditioned, first)?;
    conditioned.set_film(&constant_film(1.5, 0.1))?;
    changed.extend(run(&mut conditioned, second)?);
    assert!(changed.iter().all(|sample| sample.is_finite()));
    assert_eq!(
        max_abs_diff(&expected[..first.len()], &changed[..first.len()]).to_bits(),
        0.0_f32.to_bits()
    );
    assert!(max_abs_diff(&expected[first.len()..], &changed[first.len()..]) > 0.0);
    Ok(())
}
