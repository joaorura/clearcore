use std::{
    io::Write,
    sync::{Arc, mpsc},
    thread::{self, JoinHandle},
};

use deep_filter::tract::{DfParams, DfTract, FilmVectors, RuntimeParams};
use ndarray::{Array2, ArrayView2};
use realtime_noise_contracts::{AudioFrame, CHANNELS, HOP_SAMPLES, SAMPLE_RATE_HZ};
use tempfile::NamedTempFile;

use crate::{
    ALGORITHM_LATENCY_SAMPLES, ApprovedAssetManifest, BackendDescriptor, BandGains, CpuProfile,
    FILM_HIDDEN_DIM, FiLMVectors, InferenceBackend, InferenceError, ModelRole, ProcessedFrame,
    VerifiedAsset, VoiceProfile, spectral_eq,
};

const SPEAKER_CONDITIONING_UNSUPPORTED: &str =
    "backend model does not support speaker conditioning";

enum WorkerCommand {
    Process(Box<AudioFrame>),
    /// Swaps the conditioning between two frames without touching the recurrent state.
    Condition(Box<Conditioning>),
    Shutdown,
}

struct Conditioning {
    /// `None` leaves the `FiLM` inputs untouched (models without `FiLM` inputs).
    film: Option<FilmVectors>,
    /// `None` clears the spectral EQ.
    eq_factors: Option<Vec<f32>>,
}

enum WorkerReply {
    Frame(Box<AudioFrame>),
    Conditioned,
}

/// What the loaded graphs declare, reported once at startup.
struct ModelCapabilities {
    film: bool,
    erb_band_widths: Vec<usize>,
}

type WorkerResult<T> = Result<T, InferenceError>;

pub struct TractBackend {
    requests: mpsc::SyncSender<WorkerCommand>,
    responses: mpsc::Receiver<WorkerResult<WorkerReply>>,
    worker: Option<JoinHandle<()>>,
    descriptor: BackendDescriptor,
    film_supported: bool,
    erb_band_widths: Vec<usize>,
    active_profile: Option<VoiceProfile>,
}

impl TractBackend {
    pub fn new(
        manifest: &ApprovedAssetManifest,
        profile: CpuProfile,
    ) -> Result<Self, InferenceError> {
        profile.ensure_supported()?;
        let manifest = manifest.revalidate()?;
        let descriptor = BackendDescriptor {
            backend: "tract",
            backend_version: "deep_filter-v0.5.6",
            runtime: "tract",
            runtime_version: "0.19.16",
            asset_id: manifest.asset_id().to_owned(),
            asset_sha256: manifest.asset_sha256().to_owned(),
            cpu_profile: profile.name(),
        };
        Self::from_archive(manifest.archive_snapshot(), descriptor)
    }

    /// Builds the backend from an asset verified by the [`crate::ModelAssetRegistry`]
    /// (signature, provenance, size, digest and member allowlist already checked).
    ///
    /// Only denoising roles are accepted; enrollment and EQ assets are not loaded here.
    pub fn from_verified_asset(
        asset: &VerifiedAsset,
        profile: CpuProfile,
    ) -> Result<Self, InferenceError> {
        profile.ensure_supported()?;
        if !matches!(
            asset.role(),
            ModelRole::DenoisingBase | ModelRole::DenoisingPersonalized
        ) {
            return Err(InferenceError::AssetNotApproved(format!(
                "asset {} has role {}, which is not a denoising model",
                asset.asset_id(),
                asset.role().as_str()
            )));
        }
        let descriptor = BackendDescriptor {
            backend: "tract",
            backend_version: "deep_filter-v0.5.6",
            runtime: "tract",
            runtime_version: "0.19.16",
            asset_id: asset.asset_id().to_owned(),
            asset_sha256: asset.sha256().to_owned(),
            cpu_profile: profile.name(),
        };
        Self::from_archive(asset.archive_snapshot(), descriptor)
    }

    /// Spawns the inference worker for an already-verified archive. Callers are responsible for
    /// having passed the asset gate; this is deliberately not public.
    pub(crate) fn from_archive(
        archive: Arc<[u8]>,
        descriptor: BackendDescriptor,
    ) -> Result<Self, InferenceError> {
        let (requests, request_receiver) = mpsc::sync_channel(1);
        let (responses, response_receiver) = mpsc::sync_channel(1);
        let (startup_sender, startup_receiver) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("tract-reference-backend".to_owned())
            .spawn(move || run_worker(&archive, &request_receiver, &responses, &startup_sender))
            .map_err(InferenceError::Io)?;
        let capabilities = startup_receiver
            .recv()
            .map_err(|error| InferenceError::InferenceExecution(error.to_string()))??;
        Ok(Self {
            requests,
            responses: response_receiver,
            worker: Some(worker),
            descriptor,
            film_supported: capabilities.film,
            erb_band_widths: capabilities.erb_band_widths,
            active_profile: None,
        })
    }

    /// Whether the loaded model declares the `gamma`/`beta` `FiLM` inputs (pDFNet3-style models).
    #[must_use]
    pub const fn supports_speaker_conditioning(&self) -> bool {
        self.film_supported
    }

    #[must_use]
    pub const fn active_voice_profile(&self) -> Option<&VoiceProfile> {
        self.active_profile.as_ref()
    }

    /// Activates `profile` (or removes conditioning with `None`) between two frames.
    ///
    /// Everything is validated before anything is applied: on error the previous profile stays
    /// active. The recurrent state (GRUs, delay rings) is preserved, so there is no re-warm-up.
    /// A profile that is not the identity needs a model with `FiLM` inputs; a neutral profile is
    /// accepted by every model. The profile's EQ, if any, replaces the current spectral EQ.
    pub fn set_voice_profile(
        &mut self,
        profile: Option<&VoiceProfile>,
    ) -> Result<(), InferenceError> {
        let (film, eq_gains) = match profile {
            Some(profile) => {
                profile.verify_integrity()?;
                (Some(&profile.film), profile.eq.as_ref())
            }
            None => (None, None),
        };
        let film_update = match film {
            _ if !self.film_supported => {
                if film.is_some_and(|vectors| !vectors.is_identity()) {
                    return Err(InferenceError::InputContract(
                        SPEAKER_CONDITIONING_UNSUPPORTED.to_owned(),
                    ));
                }
                None
            }
            Some(vectors) => Some(to_film_vectors(vectors)),
            None => Some(to_film_vectors(&FiLMVectors::identity())),
        };
        let eq_factors = self.eq_factors(eq_gains)?;
        self.apply(Conditioning {
            film: film_update,
            eq_factors,
        })?;
        self.active_profile = profile.cloned();
        Ok(())
    }

    /// Sets (or clears with `None`) the spectral EQ applied to the enhanced spectrum right before
    /// the inverse STFT: zero added algorithmic latency. Direct override: it does not change
    /// [`Self::active_voice_profile`], and the next [`Self::set_voice_profile`] replaces it.
    pub fn set_spectral_eq(&mut self, eq: Option<&BandGains>) -> Result<(), InferenceError> {
        let eq_factors = self.eq_factors(eq)?;
        self.apply(Conditioning {
            film: None,
            eq_factors,
        })
    }

    fn eq_factors(&self, eq: Option<&BandGains>) -> Result<Option<Vec<f32>>, InferenceError> {
        match eq {
            Some(gains) if !gains.is_neutral() => Ok(Some(spectral_eq::bin_factors(
                gains,
                &self.erb_band_widths,
            )?)),
            Some(gains) => {
                gains.validate()?;
                Ok(None)
            }
            None => Ok(None),
        }
    }

    fn apply(&self, conditioning: Conditioning) -> Result<(), InferenceError> {
        self.requests
            .send(WorkerCommand::Condition(Box::new(conditioning)))
            .map_err(|error| InferenceError::InferenceExecution(error.to_string()))?;
        match self
            .responses
            .recv()
            .map_err(|error| InferenceError::InferenceExecution(error.to_string()))??
        {
            WorkerReply::Conditioned => Ok(()),
            WorkerReply::Frame(_) => Err(InferenceError::InferenceExecution(
                "unexpected frame reply to a conditioning command".to_owned(),
            )),
        }
    }
}

fn to_film_vectors(vectors: &FiLMVectors) -> FilmVectors {
    FilmVectors {
        gamma_enc: vectors.gamma_enc.clone(),
        beta_enc: vectors.beta_enc.clone(),
        gamma_df: vectors.gamma_df.clone(),
        beta_df: vectors.beta_df.clone(),
    }
}

impl InferenceBackend for TractBackend {
    fn set_voice_profile(&mut self, profile: Option<&VoiceProfile>) -> Result<(), InferenceError> {
        Self::set_voice_profile(self, profile)
    }

    fn descriptor(&self) -> BackendDescriptor {
        self.descriptor.clone()
    }

    fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError> {
        if input.iter().any(|sample| !sample.is_finite()) {
            return Err(InferenceError::InputContract(
                "input frame contains a non-finite sample".to_owned(),
            ));
        }
        self.requests
            .send(WorkerCommand::Process(Box::new(*input)))
            .map_err(|error| InferenceError::InferenceExecution(error.to_string()))?;
        let samples = match self
            .responses
            .recv()
            .map_err(|error| InferenceError::InferenceExecution(error.to_string()))??
        {
            WorkerReply::Frame(samples) => *samples,
            WorkerReply::Conditioned => {
                return Err(InferenceError::InferenceExecution(
                    "unexpected conditioning reply to a frame".to_owned(),
                ));
            }
        };
        ProcessedFrame::checked(samples, ALGORITHM_LATENCY_SAMPLES, self.descriptor())
    }

    fn algorithmic_latency_samples(&self) -> u32 {
        ALGORITHM_LATENCY_SAMPLES
    }
}

impl Drop for TractBackend {
    fn drop(&mut self) {
        let _ = self.requests.send(WorkerCommand::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run_worker(
    archive: &Arc<[u8]>,
    requests: &mpsc::Receiver<WorkerCommand>,
    responses: &mpsc::SyncSender<WorkerResult<WorkerReply>>,
    startup: &mpsc::SyncSender<WorkerResult<ModelCapabilities>>,
) {
    let mut model = match load_model(archive) {
        Ok(model) => {
            let _ = startup.send(Ok(capabilities(&model)));
            model
        }
        Err(error) => {
            let _ = startup.send(Err(error));
            return;
        }
    };
    while let Ok(command) = requests.recv() {
        let reply = match command {
            WorkerCommand::Process(input) => process_frame(&mut model, &input)
                .map(|samples| WorkerReply::Frame(Box::new(samples))),
            WorkerCommand::Condition(conditioning) => {
                condition(&mut model, *conditioning).map(|()| WorkerReply::Conditioned)
            }
            WorkerCommand::Shutdown => return,
        };
        if responses.send(reply).is_err() {
            return;
        }
    }
}

fn capabilities(model: &DfTract) -> ModelCapabilities {
    ModelCapabilities {
        film: model.film_hidden().is_some(),
        erb_band_widths: model
            .df_states
            .first()
            .map(|state| state.erb.clone())
            .unwrap_or_default(),
    }
}

fn condition(model: &mut DfTract, conditioning: Conditioning) -> Result<(), InferenceError> {
    if let Some(film) = &conditioning.film {
        model
            .set_film(film)
            .map_err(|error| InferenceError::InputContract(error.to_string()))?;
    }
    model
        .set_spectral_eq_factors(conditioning.eq_factors)
        .map_err(|error| InferenceError::InputContract(error.to_string()))
}

fn load_model(archive_bytes: &[u8]) -> Result<DfTract, InferenceError> {
    let mut archive = NamedTempFile::new().map_err(InferenceError::Io)?;
    archive
        .write_all(archive_bytes)
        .map_err(InferenceError::Io)?;
    archive.flush().map_err(InferenceError::Io)?;
    let parameters = DfParams::new(archive.path().to_path_buf())
        .map_err(|error| InferenceError::ModelCorruption(error.to_string()))?;
    let model = DfTract::new(parameters, &RuntimeParams::default())
        .map_err(|error| InferenceError::ModelCorruption(error.to_string()))?;
    validate_model_contract(&model)?;
    Ok(model)
}

fn process_frame(model: &mut DfTract, input: &AudioFrame) -> Result<AudioFrame, InferenceError> {
    let input_view = ArrayView2::from_shape((CHANNELS, HOP_SAMPLES), input.as_slice())
        .map_err(|error| InferenceError::InputContract(error.to_string()))?;
    let mut output = Array2::<f32>::zeros((CHANNELS, HOP_SAMPLES));
    model
        .process(input_view, output.view_mut())
        .map_err(|error| InferenceError::InferenceExecution(error.to_string()))?;
    output
        .into_raw_vec()
        .try_into()
        .map_err(|_| InferenceError::InferenceExecution("unexpected output shape".to_owned()))
}

fn validate_model_contract(model: &DfTract) -> Result<(), InferenceError> {
    let latency = (model.fft_size - model.hop_size) + model.lookahead * model.hop_size;
    if model.sr != SAMPLE_RATE_HZ as usize
        || model.ch != CHANNELS
        || model.hop_size != HOP_SAMPLES
        || model.fft_size != 960
        || model.nb_erb != 32
        || model.nb_df != 96
        || model.df_order != 5
        || model.df_lookahead != 2
        || model.conv_lookahead != 2
        || latency != ALGORITHM_LATENCY_SAMPLES as usize
    {
        return Err(InferenceError::InputContract(
            "approved model does not match the frozen DSP contract".to_owned(),
        ));
    }
    if model
        .film_hidden()
        .is_some_and(|hidden| hidden != FILM_HIDDEN_DIM)
    {
        return Err(InferenceError::InputContract(
            "FiLM hidden size does not match the voice profile contract".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::{fs, path::PathBuf, sync::Arc};

    use super::*;
    use crate::{BandGains, FILM_HIDDEN_DIM, FiLMVectors, NUM_ERB_BANDS, VoiceProfile};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn repository_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn test_descriptor(asset_id: &str) -> BackendDescriptor {
        BackendDescriptor {
            backend: "tract",
            backend_version: "deep_filter-v0.5.6",
            runtime: "tract",
            runtime_version: "0.19.16",
            asset_id: asset_id.to_owned(),
            asset_sha256: "0".repeat(64),
            cpu_profile: "avx2-minimum",
        }
    }

    /// Approved base asset (no `FiLM` inputs), loaded through the real asset gate.
    fn base_backend() -> Result<TractBackend, Box<dyn std::error::Error>> {
        let manifest = ApprovedAssetManifest::verify(&repository_root())?;
        Ok(TractBackend::new(&manifest, CpuProfile::Avx2Minimum)?)
    }

    /// FiLM-capable fixture (`tools/accelerators/gen_film_onnx.py`), loaded without the asset
    /// gate (it is a test fixture, not an approved release asset).
    fn film_backend() -> Result<TractBackend, Box<dyn std::error::Error>> {
        let bytes = fs::read(repository_root().join("fixtures/film/film-identity-asset.tar.gz"))?;
        Ok(TractBackend::from_archive(
            Arc::from(bytes),
            test_descriptor("film-identity-test-fixture"),
        )?)
    }

    fn frames(count: usize) -> Vec<AudioFrame> {
        let mut state: u32 = 0x1234_5678;
        (0..count)
            .map(|frame| {
                std::array::from_fn(|index| {
                    let n =
                        f32::from(u16::try_from(frame * HOP_SAMPLES + index).unwrap_or(u16::MAX));
                    state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    let noise = f32::from(u16::try_from(state >> 16).unwrap_or(0)) / 65_536.0 - 0.5;
                    (n * 0.0291).sin().mul_add(0.3, noise * 0.04)
                })
            })
            .collect()
    }

    fn run(backend: &mut TractBackend, input: &[AudioFrame]) -> Result<Vec<f32>, InferenceError> {
        let mut out = Vec::new();
        for frame in input {
            out.extend(backend.process(frame)?.samples);
        }
        Ok(out)
    }

    fn max_abs_diff(a: &[f32], b: &[f32]) -> f32 {
        assert_eq!(a.len(), b.len());
        a.iter()
            .zip(b)
            .fold(0.0, |acc, (x, y)| acc.max((x - y).abs()))
    }

    fn conditioned_profile(gamma: f32, beta: f32) -> VoiceProfile {
        let film = FiLMVectors::new(
            vec![gamma; FILM_HIDDEN_DIM],
            vec![beta; FILM_HIDDEN_DIM],
            vec![gamma; FILM_HIDDEN_DIM],
            vec![beta; FILM_HIDDEN_DIM],
        )
        .expect("valid film");
        VoiceProfile::new("spk", "Speaker", "2026-10-02T12:00:00Z", film, None).expect("profile")
    }

    fn flat_eq(db: f32) -> BandGains {
        BandGains::from_array([db; NUM_ERB_BANDS]).expect("valid eq")
    }

    #[test]
    fn registry_verified_base_asset_loads_and_matches_the_manifest_path() -> TestResult {
        use crate::ModelAssetRegistry;
        let input = frames(20);
        let expected = run(&mut base_backend()?, &input)?;
        let verified = ModelAssetRegistry::default().verify_role(
            &repository_root(),
            "df-compatible-release-asset-v1",
            ModelRole::DenoisingBase,
        )?;
        let mut backend = TractBackend::from_verified_asset(&verified, CpuProfile::Avx2Minimum)?;
        assert_eq!(
            backend.descriptor().asset_id,
            "df-compatible-release-asset-v1"
        );
        assert_eq!(
            max_abs_diff(&expected, &run(&mut backend, &input)?).to_bits(),
            0.0_f32.to_bits()
        );
        Ok(())
    }

    #[test]
    fn non_denoising_roles_are_not_loadable_as_a_backend() -> TestResult {
        use crate::ModelAssetRegistry;
        let verified = ModelAssetRegistry::default().verify_role(
            &repository_root(),
            "df-compatible-release-asset-v1",
            ModelRole::DenoisingBase,
        )?;
        for role in [ModelRole::SpeakerEnrollment, ModelRole::NeuralEq] {
            let retagged = verified.clone().with_role_for_test(role);
            assert!(matches!(
                TractBackend::from_verified_asset(&retagged, CpuProfile::Avx2Minimum),
                Err(InferenceError::AssetNotApproved(_))
            ));
        }
        Ok(())
    }

    #[test]
    fn base_model_does_not_support_speaker_conditioning() -> TestResult {
        assert!(!base_backend()?.supports_speaker_conditioning());
        assert!(film_backend()?.supports_speaker_conditioning());
        Ok(())
    }

    #[test]
    fn base_model_accepts_neutral_profile_and_refuses_conditioning_without_side_effects()
    -> TestResult {
        let input = frames(30);
        let expected = run(&mut base_backend()?, &input)?;

        let mut backend = base_backend()?;
        let neutral = VoiceProfile::identity("spk", "Speaker", "2026-10-02T12:00:00Z")?;
        backend.set_voice_profile(Some(&neutral))?;
        assert_eq!(backend.active_voice_profile(), Some(&neutral));

        let error = backend
            .set_voice_profile(Some(&conditioned_profile(1.5, 0.1)))
            .expect_err("a non-neutral profile needs a FiLM model");
        assert!(
            matches!(&error, InferenceError::InputContract(message)
                if message == "backend model does not support speaker conditioning"),
            "unexpected error: {error:?}"
        );
        assert_eq!(
            backend.active_voice_profile(),
            Some(&neutral),
            "failed call must not change state"
        );
        assert_eq!(
            max_abs_diff(&expected, &run(&mut backend, &input)?).to_bits(),
            0.0_f32.to_bits()
        );
        Ok(())
    }

    #[test]
    fn film_model_identity_profile_is_bit_exact_with_unconditioned_output() -> TestResult {
        let input = frames(60);
        let expected = run(&mut base_backend()?, &input)?;
        let mut backend = film_backend()?;
        let neutral = VoiceProfile::identity("spk", "Speaker", "2026-10-02T12:00:00Z")?;
        backend.set_voice_profile(Some(&neutral))?;
        assert_eq!(
            max_abs_diff(&expected, &run(&mut backend, &input)?).to_bits(),
            0.0_f32.to_bits()
        );
        Ok(())
    }

    #[test]
    fn profile_switch_mid_stream_does_not_reset_the_recurrent_state() -> TestResult {
        let input = frames(60);
        let (first, second) = input.split_at(30);
        let expected = run(&mut film_backend()?, &input)?;

        let mut backend = film_backend()?;
        let mut out = run(&mut backend, first)?;
        backend.set_voice_profile(Some(&VoiceProfile::identity(
            "spk",
            "Speaker",
            "2026-10-02T12:00:00Z",
        )?))?;
        out.extend(run(&mut backend, second)?);
        assert_eq!(max_abs_diff(&expected, &out).to_bits(), 0.0_f32.to_bits());

        let mut conditioned = film_backend()?;
        let mut changed = run(&mut conditioned, first)?;
        conditioned.set_voice_profile(Some(&conditioned_profile(1.5, 0.1)))?;
        changed.extend(run(&mut conditioned, second)?);
        let boundary = first.len() * HOP_SAMPLES;
        assert!(changed.iter().all(|sample| sample.is_finite()));
        assert_eq!(
            max_abs_diff(&expected[..boundary], &changed[..boundary]).to_bits(),
            0.0_f32.to_bits()
        );
        assert!(max_abs_diff(&expected[boundary..], &changed[boundary..]) > 0.0);

        // Back to no profile: conditioning is removed, state is still continuous (finite).
        conditioned.set_voice_profile(None)?;
        assert!(conditioned.active_voice_profile().is_none());
        assert!(
            run(&mut conditioned, second)?
                .iter()
                .all(|sample| sample.is_finite())
        );
        Ok(())
    }

    #[test]
    fn tampered_profile_is_refused_and_keeps_the_previous_one() -> TestResult {
        let mut backend = film_backend()?;
        let good = conditioned_profile(1.2, 0.05);
        backend.set_voice_profile(Some(&good))?;
        let mut tampered = conditioned_profile(1.5, 0.1);
        tampered.name = "Imposter".to_owned();
        assert!(backend.set_voice_profile(Some(&tampered)).is_err());
        assert_eq!(backend.active_voice_profile(), Some(&good));
        Ok(())
    }

    #[test]
    fn spectral_eq_flat_6_db_scales_the_output_with_zero_lag() -> TestResult {
        let input = frames(60);
        let baseline = run(&mut base_backend()?, &input)?;
        let mut backend = base_backend()?;
        backend.set_spectral_eq(Some(&flat_eq(6.0)))?;
        let boosted = run(&mut backend, &input)?;
        let gain = 10.0_f32.powf(6.0 / 20.0);
        let peak = baseline.iter().fold(0.0_f32, |acc, s| acc.max(s.abs()));
        assert!(peak > 1e-3, "baseline must not be silent (peak {peak})");
        let worst = baseline
            .iter()
            .zip(&boosted)
            .fold(0.0_f32, |acc, (b, e)| acc.max((e - b * gain).abs()));
        assert!(
            worst < peak * 1e-5,
            "output must be baseline * 10^(6/20); worst deviation {worst}"
        );

        // Clearing the EQ restores the unmodified signal bit for bit (the EQ never feeds the state).
        backend.set_spectral_eq(None)?;
        let more = frames(20);
        let after = run(&mut backend, &more)?;
        let mut reference = base_backend()?;
        run(&mut reference, &input)?;
        assert_eq!(
            max_abs_diff(&run(&mut reference, &more)?, &after).to_bits(),
            0.0_f32.to_bits()
        );
        Ok(())
    }

    #[test]
    fn neutral_eq_is_a_bit_exact_no_op() -> TestResult {
        let input = frames(40);
        let baseline = run(&mut base_backend()?, &input)?;
        let mut backend = base_backend()?;
        backend.set_spectral_eq(Some(&BandGains::neutral()))?;
        assert_eq!(
            max_abs_diff(&baseline, &run(&mut backend, &input)?).to_bits(),
            0.0_f32.to_bits()
        );
        Ok(())
    }

    #[test]
    fn eq_out_of_range_is_refused() -> TestResult {
        let mut backend = base_backend()?;
        let invalid = BandGains {
            gains_db: [30.0; NUM_ERB_BANDS],
        };
        assert!(backend.set_spectral_eq(Some(&invalid)).is_err());
        Ok(())
    }

    #[test]
    fn profile_eq_is_applied_with_the_profile() -> TestResult {
        let input = frames(40);
        let baseline = run(&mut base_backend()?, &input)?;
        let film = FiLMVectors::identity();
        let profile = VoiceProfile::new(
            "spk",
            "Speaker",
            "2026-10-02T12:00:00Z",
            film,
            Some(flat_eq(6.0)),
        )?;
        let mut backend = base_backend()?;
        backend.set_voice_profile(Some(&profile))?;
        let boosted = run(&mut backend, &input)?;
        assert!(max_abs_diff(&baseline, &boosted) > 0.0);
        Ok(())
    }
}
