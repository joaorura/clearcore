//! Valida o `DspPipeline` desacoplado (STFT, ERB, DF e iSTFT sem rede neural) contra o caminho
//! monolítico do libDF (`DfTract::process`), usando o próprio `DfTract` como backend espectral:
//! o adaptador alimenta `process_raw` com o espectro ruidoso e devolve lsnr, ganhos ERB e
//! coeficientes DF. Se o pipeline desacoplado divergir do upstream em qualquer etapa (alinhamento
//! de quadros, DF, post-filter, limite de atenuação), a saída diverge.
#![cfg(feature = "tract")]
// O sinal sintético reproduz a aritmética do harness do spike sem fusão multiplicar-somar.
#![allow(clippy::suboptimal_flops)]

use std::{error::Error, path::PathBuf, sync::mpsc, thread};

use deep_filter::{
    Complex32,
    tract::{DfParams, DfTract, RuntimeParams},
};
use ndarray::{Array2, ArrayView2};
use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES};
use realtime_noise_model::{
    BackendDescriptor, DspPipeline, DspPipelineConfig, ExtractedFeatures, InferenceError,
    ModelFrameOutput, SpectralModelBackend,
};

const NB_ERB: usize = 32;
const NB_DF: usize = 96;
const DF_ORDER: usize = 5;

type TestResult = Result<(), Box<dyn Error>>;

fn approved_asset() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor/approved/df-compatible-release-asset-v1.bin")
}

fn load(params: &RuntimeParams) -> Result<DfTract, Box<dyn Error>> {
    Ok(DfTract::new(DfParams::new(approved_asset())?, params)?)
}

/// `SpectralModelBackend` cujo modelo é o `DfTract` do libDF (os mesmos 3 grafos ONNX).
///
/// `DfTract` não é `Send` (estado do tract), então ele vive numa thread própria, como no
/// `TractBackend`; o adaptador conversa com ela por canais.
struct DfTractSpectralModel {
    requests: mpsc::SyncSender<Vec<Complex32>>,
    responses: mpsc::Receiver<Result<ModelFrameOutput, String>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl DfTractSpectralModel {
    fn spawn(params: fn() -> RuntimeParams) -> Self {
        let (requests, request_receiver) = mpsc::sync_channel::<Vec<Complex32>>(1);
        let (response_sender, responses) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            let Ok(mut model) = load(&params()) else {
                return;
            };
            while let Ok(spec_noisy) = request_receiver.recv() {
                if response_sender
                    .send(raw_prediction(&mut model, &spec_noisy))
                    .is_err()
                {
                    return;
                }
            }
        });
        Self {
            requests,
            responses,
            worker: Some(worker),
        }
    }
}

impl Drop for DfTractSpectralModel {
    fn drop(&mut self) {
        // Fecha o canal de pedidos (encerra a thread) e espera por ela.
        let (closed, _) = mpsc::sync_channel(1);
        drop(std::mem::replace(&mut self.requests, closed));
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn raw_prediction(
    model: &mut DfTract,
    spec_noisy: &[Complex32],
) -> Result<ModelFrameOutput, String> {
    {
        let mut spec = model.get_mut_spec_enh();
        for (dst, src) in spec.row_mut(0).iter_mut().zip(spec_noisy) {
            *dst = *src;
        }
    }
    let (lsnr, gains, coefs) = model.process_raw().map_err(|e| e.to_string())?;
    let erb_gains = match gains {
        Some(tensor) => {
            let slice = tensor.as_slice::<f32>().map_err(|e| e.to_string())?;
            Some(<[f32; NB_ERB]>::try_from(slice).map_err(|e| e.to_string())?)
        }
        None => None,
    };
    let df_coefs = match coefs {
        Some(tensor) => {
            let slice = tensor.as_slice::<f32>().map_err(|e| e.to_string())?;
            assert_eq!(slice.len(), NB_DF * DF_ORDER * 2);
            Some(
                slice
                    .chunks_exact(DF_ORDER * 2)
                    .map(|bin| {
                        std::array::from_fn(|tap| Complex32::new(bin[tap * 2], bin[tap * 2 + 1]))
                    })
                    .collect(),
            )
        }
        None => None,
    };
    Ok(ModelFrameOutput {
        lsnr,
        erb_gains,
        df_coefs,
    })
}

impl SpectralModelBackend for DfTractSpectralModel {
    fn descriptor(&self) -> BackendDescriptor {
        BackendDescriptor {
            backend: "df-tract-adapter",
            backend_version: "test",
            runtime: "tract",
            runtime_version: "0.19.16",
            asset_id: "df-compatible-release-asset-v1".to_owned(),
            asset_sha256: "0".repeat(64),
            cpu_profile: "test",
        }
    }

    fn process_spectrum(
        &mut self,
        features: &ExtractedFeatures,
    ) -> Result<ModelFrameOutput, InferenceError> {
        let fail = |message: String| InferenceError::InferenceExecution(message);
        self.requests
            .send(features.spec_noisy.clone())
            .map_err(|e| fail(e.to_string()))?;
        self.responses
            .recv()
            .map_err(|e| fail(e.to_string()))?
            .map_err(fail)
    }

    fn reset(&mut self) -> Result<(), InferenceError> {
        Ok(())
    }
}

fn lcg_noise(state: &mut u32) -> f64 {
    *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    f64::from(*state >> 8) / 8_388_608.0 - 1.0
}

/// Mesma fala sintética do spike da Fase 3, sem silêncio digital (o libDF pula quadros com
/// rms < 1e-7 sem avançar o estado; o pipeline desacoplado não tem esse atalho).
fn speech_like(frames: usize) -> Vec<f32> {
    let mut state: u32 = 0x1234_5678;
    (0..frames * HOP_SAMPLES)
        .map(|index| {
            let t = f64::from(u32::try_from(index).unwrap_or(u32::MAX)) / 48_000.0;
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

fn reference_output(
    params: fn() -> RuntimeParams,
    signal: &[f32],
) -> Result<Vec<f32>, Box<dyn Error>> {
    let mut model = load(&params())?;
    let mut out = Vec::with_capacity(signal.len());
    for frame in signal.chunks_exact(HOP_SAMPLES) {
        let input = ArrayView2::from_shape((1, HOP_SAMPLES), frame)?;
        let mut output = Array2::<f32>::zeros((1, HOP_SAMPLES));
        model.process(input, output.view_mut())?;
        out.extend(output.iter());
    }
    Ok(out)
}

fn pipeline_output(
    config: DspPipelineConfig,
    params: fn() -> RuntimeParams,
    signal: &[f32],
) -> Result<Vec<f32>, Box<dyn Error>> {
    let mut pipeline = DspPipeline::new(config)?;
    let mut backend = DfTractSpectralModel::spawn(params);
    let mut out = Vec::with_capacity(signal.len());
    for frame in signal.chunks_exact(HOP_SAMPLES) {
        let input: AudioFrame = frame.try_into()?;
        out.extend(pipeline.process_frame(&input, &mut backend)?);
    }
    Ok(out)
}

fn max_abs_diff(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b)
        .fold(0.0_f32, |acc, (x, y)| acc.max((x - y).abs()))
}

fn assert_parity(reference: &[f32], actual: &[f32], label: &str) {
    let peak = reference.iter().fold(0.0_f32, |acc, s| acc.max(s.abs()));
    assert!(
        peak > 1e-3,
        "{label}: a referência não pode ser silenciosa (pico {peak})"
    );
    let diff = max_abs_diff(reference, actual);
    assert!(
        diff <= 1e-6,
        "{label}: pipeline desacoplado diverge do libDF (max_abs_diff={diff:e}, pico={peak})"
    );
}

#[test]
fn default_pipeline_matches_default_libdf_runtime() -> TestResult {
    // Padrões do produto: RuntimeParams::default() (sem post-filter, sem limite de atenuação).
    let signal = speech_like(200);
    let reference = reference_output(RuntimeParams::default, &signal)?;
    let actual = pipeline_output(
        DspPipelineConfig::default(),
        RuntimeParams::default,
        &signal,
    )?;
    assert_parity(&reference, &actual, "padrão");
    Ok(())
}

#[test]
fn post_filter_matches_libdf() -> TestResult {
    let signal = speech_like(200);
    let params: fn() -> RuntimeParams = || RuntimeParams::default().with_post_filter(0.02);
    let reference = reference_output(params, &signal)?;
    let config = DspPipelineConfig {
        post_filter_beta: 0.02,
        ..DspPipelineConfig::default()
    };
    let actual = pipeline_output(config, params, &signal)?;
    assert_parity(&reference, &actual, "post-filter");
    Ok(())
}

#[test]
fn attenuation_limit_matches_libdf() -> TestResult {
    let signal = speech_like(200);
    let params: fn() -> RuntimeParams = || RuntimeParams::default().with_atten_lim(12.0);
    let reference = reference_output(params, &signal)?;
    let config = DspPipelineConfig {
        atten_lim_db: Some(12.0),
        ..DspPipelineConfig::default()
    };
    let actual = pipeline_output(config, params, &signal)?;
    assert_parity(&reference, &actual, "limite de atenuação");
    Ok(())
}
