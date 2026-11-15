//! Headless smoke of the daemon's real path with the pDFNet3 Pro v7: the supervisor configured as
//! in production (bundled Pro v3 full + bundled v7), a v7 profile in a profile store, isolation
//! turned on over IPC, the profile delivered by `attach_profile_store`, then a vector WAV (CC0
//! voices, no user audio) hop by hop through the supervisor (studio chain included), compared
//! with the exporter's expected output.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout,
    clippy::cast_precision_loss
)]

use std::path::{Path, PathBuf};

use std::io::Cursor;

use realtime_noise_ipc::{IpcCommand, IpcRequest, IpcResponse};
use realtime_noise_model::pdfnet3_bundle;
use realtime_noise_model::{ProfileStore, SpeakerEmbedding, VoiceProfileV2};
use realtime_noise_service::ServiceDaemon;
use realtime_noise_supervisor::EngineSupervisor;

fn send(daemon: &mut ServiceDaemon, command: IpcCommand) -> IpcResponse {
    let request = IpcRequest::new(command, serde_json::json!({}));
    let mut reader = Cursor::new(format!("{}\n", request.to_json().unwrap()).into_bytes());
    let mut writer = Cursor::new(Vec::new());
    daemon.serve_client(&mut reader, &mut writer).unwrap();
    let raw = String::from_utf8(writer.into_inner()).unwrap();
    IpcResponse::from_json(raw.trim()).unwrap()
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn case(file: &str) -> PathBuf {
    root()
        .join("crates/model/tests/fixtures/pdfnet3_v7/alvo_interf_sir0_snr10")
        .join(file)
}

fn f32s(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn wav(path: &Path) -> Vec<f32> {
    let bytes = std::fs::read(path).unwrap();
    let at = bytes.windows(4).position(|w| w == b"data").unwrap();
    let size = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
    f32s(&bytes[at + 8..at + 8 + size])
}

#[test]
fn daemon_applies_a_v7_profile_and_processes_a_vector_wav() {
    let home = tempfile::tempdir().unwrap();
    let embedding = SpeakerEmbedding::from_raw_normalizing(&f32s(
        &std::fs::read(case("speaker_embedding.f32")).unwrap(),
    ))
    .unwrap();
    let tokens = f32s(&std::fs::read(case("tokens.f32")).unwrap());
    let profile = VoiceProfileV2::new_v7(
        "p7",
        "Voz",
        "2026-11-15T12:00:00Z",
        &embedding,
        &tokens,
        None,
    )
    .unwrap();
    let store = ProfileStore::new(home.path().join("profiles"));
    store.save_active_v2(&profile).unwrap();

    // The supervisor exactly as `ServiceDaemon::new` configures it in production: the bundled
    // Pro v3 full as the required isolation model plus the bundled v7, `auto` backend.
    let models = root().join("models");
    let mut supervisor = EngineSupervisor::default();
    supervisor.require_isolation_model(Ok(pdfnet3_bundle::load_isolation_archive_from(
        std::slice::from_ref(&models),
    )
    .unwrap()));
    supervisor.set_v7_asset(Some(pdfnet3_bundle::load_v7_asset_from(&[models]).unwrap()));
    let _ = supervisor.select_backend("auto", None, None);
    let mut daemon = ServiceDaemon::with_supervisor(supervisor);
    send(&mut daemon, IpcCommand::SetVoiceIsolation { enabled: true });
    daemon.attach_profile_store(store);
    assert_eq!(daemon.active_voice_profile_id(), Some("p7"));
    let status = send(&mut daemon, IpcCommand::GetStatus).payload;
    println!(
        "status: isolation_model={} variant={} backend={} profile_model={}",
        status["isolation_model"],
        status["isolation_model_variant"],
        status["active_backend"],
        status["voice_profile_model_id"]
    );
    assert_eq!(status["isolation_model"], "pdfnet3-pro");
    assert_eq!(status["isolation_model_variant"], "pdfnet3-pro-v7");
    assert_eq!(status["voice_profile_state"], "active");

    let input = wav(&case("input.wav"));
    let expected = wav(&case("expected_out_gateC.wav"));
    let mut output = Vec::with_capacity(input.len());
    let start = std::time::Instant::now();
    for chunk in input.chunks_exact(480) {
        let frame: [f32; 480] = chunk.try_into().unwrap();
        output.extend_from_slice(&daemon.supervisor_mut().process_frame(&frame).unwrap());
    }
    let elapsed = start.elapsed().as_secs_f64();
    // Output is 21 frames behind the expected (which already has the base latency); no flush in
    // a live stream, so the last 21 expected frames are not compared.
    let delay = 21 * 480;
    let got = &output[delay..];
    let reference = &expected[..got.len()];
    let signal: f64 = reference.iter().map(|v| f64::from(*v).powi(2)).sum();
    let noise: f64 = reference
        .iter()
        .zip(got)
        .map(|(a, b)| (f64::from(*a) - f64::from(*b)).powi(2))
        .sum();
    let snr = 10.0 * (signal / noise.max(1e-30)).log10();
    println!(
        "daemon smoke: {} hops in {:.1} ms ({:.1} ms per s of audio), SNR vs expected_out_gateC = {snr:.1} dB",
        input.len() / 480,
        elapsed * 1000.0,
        elapsed * 1000.0 / (input.len() as f64 / 48_000.0)
    );
    assert!(snr >= 60.0, "SNR {snr}");
}
