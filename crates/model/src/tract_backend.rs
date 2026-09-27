use std::{
    io::Write,
    sync::{Arc, mpsc},
    thread::{self, JoinHandle},
};

use deep_filter::tract::{DfParams, DfTract, RuntimeParams};
use ndarray::{Array2, ArrayView2};
use realtime_noise_contracts::{AudioFrame, CHANNELS, HOP_SAMPLES, SAMPLE_RATE_HZ};
use tempfile::NamedTempFile;

use crate::{
    ALGORITHM_LATENCY_SAMPLES, ApprovedAssetManifest, BackendDescriptor, CpuProfile,
    InferenceBackend, InferenceError, ProcessedFrame,
};

enum WorkerCommand {
    Process(Box<AudioFrame>),
    Shutdown,
}

pub struct TractBackend {
    requests: mpsc::SyncSender<WorkerCommand>,
    responses: mpsc::Receiver<Result<AudioFrame, InferenceError>>,
    worker: Option<JoinHandle<()>>,
    descriptor: BackendDescriptor,
}

impl TractBackend {
    pub fn new(
        manifest: &ApprovedAssetManifest,
        profile: CpuProfile,
    ) -> Result<Self, InferenceError> {
        profile.ensure_supported()?;
        let descriptor = BackendDescriptor {
            backend: "tract",
            backend_version: "deep_filter-v0.5.6",
            runtime: "tract",
            runtime_version: "0.19.16",
            asset_id: manifest.asset_id().to_owned(),
            asset_sha256: manifest.asset_sha256().to_owned(),
            cpu_profile: profile.name(),
        };
        let archive = manifest.archive_snapshot();
        let (requests, request_receiver) = mpsc::sync_channel(1);
        let (responses, response_receiver) = mpsc::sync_channel(1);
        let (startup_sender, startup_receiver) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("tract-reference-backend".to_owned())
            .spawn(move || run_worker(&archive, &request_receiver, &responses, &startup_sender))
            .map_err(InferenceError::Io)?;
        startup_receiver
            .recv()
            .map_err(|error| InferenceError::InferenceExecution(error.to_string()))??;
        Ok(Self {
            requests,
            responses: response_receiver,
            worker: Some(worker),
            descriptor,
        })
    }
}

impl InferenceBackend for TractBackend {
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
        let samples = self
            .responses
            .recv()
            .map_err(|error| InferenceError::InferenceExecution(error.to_string()))??;
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
    responses: &mpsc::SyncSender<Result<AudioFrame, InferenceError>>,
    startup: &mpsc::SyncSender<Result<(), InferenceError>>,
) {
    let mut model = match load_model(archive) {
        Ok(model) => {
            let _ = startup.send(Ok(()));
            model
        }
        Err(error) => {
            let _ = startup.send(Err(error));
            return;
        }
    };
    while let Ok(command) = requests.recv() {
        match command {
            WorkerCommand::Process(input) => {
                let result = process_frame(&mut model, &input);
                if responses.send(result).is_err() {
                    return;
                }
            }
            WorkerCommand::Shutdown => return,
        }
    }
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
    Ok(())
}
