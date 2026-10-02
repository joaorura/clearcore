#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use std::sync::Arc;

use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES};
use realtime_noise_engine::{DenoiseEngine, DenoiseMode, ResetReason};
use realtime_noise_model::{BackendDescriptor, InferenceBackend, InferenceError, ProcessedFrame};
use studio_dsp::{Preset, StudioControl};

struct Passthrough;

impl Passthrough {
    fn descriptor() -> BackendDescriptor {
        BackendDescriptor {
            backend: "passthrough",
            backend_version: "1",
            runtime: "test",
            runtime_version: "1",
            asset_id: "test".to_owned(),
            asset_sha256: "0".repeat(64),
            cpu_profile: "test",
        }
    }
}

impl InferenceBackend for Passthrough {
    fn descriptor(&self) -> BackendDescriptor {
        Self::descriptor()
    }

    fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError> {
        ProcessedFrame::checked(*input, 1_440, Self::descriptor())
    }

    fn algorithmic_latency_samples(&self) -> u32 {
        1_440
    }
}

fn tone(frame_index: u16) -> AudioFrame {
    let mut frame = [0.0_f32; HOP_SAMPLES];
    for (i, sample) in (0_u16..).zip(frame.iter_mut()) {
        let n = f32::from(frame_index).mul_add(480.0, f32::from(i));
        *sample = 0.9 * (n * 0.13).sin();
    }
    frame
}

fn started_engine(control: &Arc<StudioControl>, mode: DenoiseMode) -> DenoiseEngine {
    let mut engine =
        DenoiseEngine::new_standalone(Box::new(Passthrough), mode).with_studio(Arc::clone(control));
    engine.start().unwrap();
    engine
}

/// Feeds tones until the engine output differs from the input; returns whether it ever did.
fn output_ever_differs(engine: &mut DenoiseEngine) -> bool {
    (0..200).any(|index| {
        let input = tone(index);
        engine.process_frame(&input).unwrap() != input
    })
}

#[test]
fn off_preset_leaves_the_active_output_identical() {
    let control = Arc::new(StudioControl::new(Preset::Off));
    let mut engine = started_engine(&control, DenoiseMode::Active);
    for index in 0..20 {
        let input = tone(index);
        assert_eq!(
            engine.process_frame(&input).unwrap(),
            input,
            "frame {index}"
        );
    }
}

#[test]
fn a_studio_preset_changes_the_active_output() {
    let control = Arc::new(StudioControl::new(Preset::Broadcast));
    let mut engine = started_engine(&control, DenoiseMode::Active);
    assert!(output_ever_differs(&mut engine));
}

#[test]
fn preset_switched_while_running_reaches_the_audio() {
    let control = Arc::new(StudioControl::new(Preset::Off));
    let mut engine = started_engine(&control, DenoiseMode::Active);
    let first = tone(0);
    assert_eq!(engine.process_frame(&first).unwrap(), first);

    control.set_preset(Preset::Broadcast);
    assert!(output_ever_differs(&mut engine));
}

#[test]
fn bypass_returns_the_raw_input_even_with_a_studio_preset() {
    let control = Arc::new(StudioControl::new(Preset::Broadcast));
    let mut engine = started_engine(&control, DenoiseMode::Bypass);
    for index in 0..50 {
        let input = tone(index);
        assert_eq!(
            engine.process_frame(&input).unwrap(),
            input,
            "frame {index}"
        );
    }
}

#[test]
fn mute_returns_silence_even_with_a_studio_preset() {
    let control = Arc::new(StudioControl::new(Preset::Broadcast));
    let mut engine = started_engine(&control, DenoiseMode::Mute);
    for index in 0..50 {
        let output = engine.process_frame(&tone(index)).unwrap();
        assert_eq!(output, [0.0; HOP_SAMPLES], "frame {index}");
    }
}

#[test]
fn generation_restart_resets_the_dsp_state() {
    let control = Arc::new(StudioControl::new(Preset::Broadcast));
    let mut warmed = started_engine(&control, DenoiseMode::Active);
    let mut fresh = started_engine(&control, DenoiseMode::Active);
    for index in 0..30 {
        warmed.process_frame(&tone(index)).unwrap();
    }

    warmed
        .begin_generation_restart(ResetReason::UserRequested)
        .unwrap();
    let probe = tone(1_000);
    assert_eq!(
        warmed.process_frame(&probe).unwrap(),
        fresh.process_frame(&probe).unwrap()
    );
}

#[test]
fn a_backend_set_later_keeps_the_studio_chain() {
    let control = Arc::new(StudioControl::new(Preset::Broadcast));
    let mut engine = DenoiseEngine::new_standalone(Box::new(Passthrough), DenoiseMode::Active)
        .with_studio(Arc::clone(&control));
    engine.set_backend(Box::new(Passthrough)).unwrap();
    engine.start().unwrap();
    assert!(output_ever_differs(&mut engine));
}

#[test]
fn an_engine_without_studio_behaves_as_before() {
    let mut engine = DenoiseEngine::new_standalone(Box::new(Passthrough), DenoiseMode::Active);
    engine.start().unwrap();
    for index in 0..20 {
        let input = tone(index);
        assert_eq!(
            engine.process_frame(&input).unwrap(),
            input,
            "frame {index}"
        );
    }
}
