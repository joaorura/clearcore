#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use realtime_noise_contracts::{
    AudioFrame, Discontinuity, FrameEnvelope, HOP_SAMPLES, RealtimeTransport,
};
use realtime_noise_engine::{BoundedQueueTransport, DenoiseEngine, VoiceProfileUpdate};
use realtime_noise_model::{
    BackendDescriptor, InferenceBackend, InferenceError, ProcessedFrame, VoiceProfile,
};

type Calls = Arc<Mutex<Vec<Option<String>>>>;

struct ProfileBackend {
    calls: Calls,
    supports: Arc<AtomicBool>,
    delay: Duration,
}

impl ProfileBackend {
    fn descriptor() -> BackendDescriptor {
        BackendDescriptor {
            backend: "profile",
            backend_version: "1",
            runtime: "test",
            runtime_version: "1",
            asset_id: "test".to_owned(),
            asset_sha256: "0".repeat(64),
            cpu_profile: "test",
        }
    }
}

impl InferenceBackend for ProfileBackend {
    fn descriptor(&self) -> BackendDescriptor {
        Self::descriptor()
    }

    fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError> {
        ProcessedFrame::checked(*input, 0, Self::descriptor())
    }

    fn algorithmic_latency_samples(&self) -> u32 {
        0
    }

    fn set_voice_profile(&mut self, profile: Option<&VoiceProfile>) -> Result<(), InferenceError> {
        std::thread::sleep(self.delay);
        if !self.supports.load(Ordering::SeqCst) {
            return Err(InferenceError::UnsupportedFeature(
                "voice profile conditioning".to_owned(),
            ));
        }
        self.calls
            .lock()
            .unwrap()
            .push(profile.map(|p| p.id.clone()));
        Ok(())
    }
}

fn profile(id: &str) -> VoiceProfile {
    VoiceProfile::identity(id, "name", "2026-10-05T00:00:00Z").unwrap()
}

struct Rig {
    engine: DenoiseEngine,
    input: Arc<BoundedQueueTransport>,
    output: Arc<BoundedQueueTransport>,
    calls: Calls,
    supports: Arc<AtomicBool>,
}

fn rig() -> Rig {
    rig_with_delay(Duration::ZERO)
}

fn rig_with_delay(delay: Duration) -> Rig {
    let calls: Calls = Arc::new(Mutex::new(Vec::new()));
    let supports = Arc::new(AtomicBool::new(true));
    let input = Arc::new(BoundedQueueTransport::new());
    let output = Arc::new(BoundedQueueTransport::new());
    let engine = DenoiseEngine::new(
        input.clone(),
        output.clone(),
        Box::new(ProfileBackend {
            calls: Arc::clone(&calls),
            supports: Arc::clone(&supports),
            delay,
        }),
    );
    Rig {
        engine,
        input,
        output,
        calls,
        supports,
    }
}

fn push_hop_and_wait(rig: &Rig, sequence: u64) {
    let frame = FrameEnvelope {
        samples: [0.1; HOP_SAMPLES],
        sequence,
        capture_monotonic_ns: sequence * 10_000_000,
        generation: 1,
        discontinuity: Discontinuity::NONE,
    };
    rig.input.try_push(frame).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while rig.output.try_pop().is_none() {
        assert!(Instant::now() < deadline, "no output within 5 s");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn stopped_engine_applies_synchronously() {
    let mut rig = rig();
    rig.engine
        .set_voice_profile(VoiceProfileUpdate::Set(profile("a")))
        .unwrap();
    assert_eq!(rig.engine.applied_voice_profile_id(), Some("a".to_owned()));
    rig.engine
        .set_voice_profile(VoiceProfileUpdate::Clear)
        .unwrap();
    assert_eq!(rig.engine.applied_voice_profile_id(), None);
    assert_eq!(*rig.calls.lock().unwrap(), vec![Some("a".to_owned()), None]);
}

#[test]
fn stopped_engine_reports_backend_failure() {
    let mut rig = rig();
    rig.supports.store(false, Ordering::SeqCst);
    let result = rig
        .engine
        .set_voice_profile(VoiceProfileUpdate::Set(profile("a")));
    assert!(result.is_err());
    assert_eq!(rig.engine.applied_voice_profile_id(), None);
    assert!(rig.engine.voice_profile_error().is_some());
}

#[test]
fn running_engine_applies_between_frames_not_inside_set() {
    let mut rig = rig();
    rig.engine.start().unwrap();
    rig.engine
        .set_voice_profile(VoiceProfileUpdate::Set(profile("a")))
        .unwrap();
    assert!(rig.calls.lock().unwrap().is_empty());
    push_hop_and_wait(&rig, 1);
    assert_eq!(*rig.calls.lock().unwrap(), vec![Some("a".to_owned())]);
    assert_eq!(rig.engine.applied_voice_profile_id(), Some("a".to_owned()));
    rig.engine.stop().unwrap();
}

#[test]
fn failed_update_keeps_previous_profile_id() {
    let mut rig = rig();
    rig.engine
        .set_voice_profile(VoiceProfileUpdate::Set(profile("a")))
        .unwrap();
    rig.engine.start().unwrap();
    rig.supports.store(false, Ordering::SeqCst);
    rig.engine
        .set_voice_profile(VoiceProfileUpdate::Set(profile("b")))
        .unwrap();
    push_hop_and_wait(&rig, 1);
    assert_eq!(rig.engine.applied_voice_profile_id(), Some("a".to_owned()));
    assert!(rig.engine.voice_profile_error().is_some());
    rig.engine.stop().unwrap();
}

fn fresh_backend(rig: &Rig) -> Box<dyn InferenceBackend> {
    Box::new(ProfileBackend {
        calls: Arc::clone(&rig.calls),
        supports: Arc::clone(&rig.supports),
        delay: Duration::ZERO,
    })
}

#[test]
fn stopped_backend_swap_clears_applied_profile_id() {
    let mut rig = rig();
    rig.engine
        .set_voice_profile(VoiceProfileUpdate::Set(profile("a")))
        .unwrap();
    rig.supports.store(false, Ordering::SeqCst);
    let _ = rig
        .engine
        .set_voice_profile(VoiceProfileUpdate::Set(profile("b")));
    assert!(rig.engine.voice_profile_error().is_some());
    rig.engine.set_backend(fresh_backend(&rig)).unwrap();
    assert_eq!(rig.engine.applied_voice_profile_id(), None);
    assert_eq!(rig.engine.voice_profile_error(), None);
}

#[test]
fn running_backend_swap_clears_applied_profile_id() {
    let mut rig = rig();
    rig.engine.start().unwrap();
    rig.engine
        .set_voice_profile(VoiceProfileUpdate::Set(profile("a")))
        .unwrap();
    push_hop_and_wait(&rig, 1);
    assert_eq!(rig.engine.applied_voice_profile_id(), Some("a".to_owned()));
    rig.engine.set_backend(fresh_backend(&rig)).unwrap();
    push_hop_and_wait(&rig, 2);
    assert_eq!(rig.engine.applied_voice_profile_id(), None);
    rig.engine.stop().unwrap();
}

#[test]
fn backend_swap_with_pending_update_ends_with_the_pending_id() {
    let mut rig = rig();
    rig.engine.start().unwrap();
    rig.engine
        .set_voice_profile(VoiceProfileUpdate::Set(profile("a")))
        .unwrap();
    push_hop_and_wait(&rig, 1);
    rig.engine.set_backend(fresh_backend(&rig)).unwrap();
    rig.engine
        .set_voice_profile(VoiceProfileUpdate::Set(profile("b")))
        .unwrap();
    push_hop_and_wait(&rig, 2);
    assert_eq!(rig.engine.applied_voice_profile_id(), Some("b".to_owned()));
    rig.engine.stop().unwrap();
}

#[test]
fn slow_profile_update_does_not_count_toward_the_inference_deadline() {
    let mut rig = rig_with_delay(Duration::from_millis(15));
    rig.engine.start().unwrap();
    rig.engine
        .set_voice_profile(VoiceProfileUpdate::Set(profile("a")))
        .unwrap();
    push_hop_and_wait(&rig, 1);
    assert_eq!(rig.engine.status().deadline_miss_count, 0);
    assert_eq!(rig.engine.applied_voice_profile_id(), Some("a".to_owned()));
    rig.engine.stop().unwrap();
}

#[test]
fn successful_update_after_failure_clears_the_error() {
    let mut rig = rig();
    rig.engine.start().unwrap();
    rig.supports.store(false, Ordering::SeqCst);
    rig.engine
        .set_voice_profile(VoiceProfileUpdate::Set(profile("b")))
        .unwrap();
    push_hop_and_wait(&rig, 1);
    assert!(rig.engine.voice_profile_error().is_some());
    rig.supports.store(true, Ordering::SeqCst);
    rig.engine
        .set_voice_profile(VoiceProfileUpdate::Set(profile("c")))
        .unwrap();
    push_hop_and_wait(&rig, 2);
    assert_eq!(rig.engine.voice_profile_error(), None);
    assert_eq!(rig.engine.applied_voice_profile_id(), Some("c".to_owned()));
    rig.engine.stop().unwrap();
}

#[test]
fn latest_pending_request_wins() {
    let mut rig = rig();
    rig.engine.start().unwrap();
    rig.engine
        .set_voice_profile(VoiceProfileUpdate::Set(profile("a")))
        .unwrap();
    rig.engine
        .set_voice_profile(VoiceProfileUpdate::Clear)
        .unwrap();
    push_hop_and_wait(&rig, 1);
    assert_eq!(*rig.calls.lock().unwrap(), vec![None]);
    rig.engine.stop().unwrap();
}
