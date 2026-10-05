//! Decorator that runs the `studio-dsp` finishing chain after a neural backend.
//!
//! `StudioBackend` implements the same [`InferenceBackend`] trait as the backend it wraps, so the
//! engine and the C ABI only wrap the backend they already build. The trait does not change.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use realtime_noise_contracts::AudioFrame;
use studio_dsp::{Preset, StudioChain, StudioControl};

use crate::{BackendDescriptor, InferenceBackend, InferenceError, ProcessedFrame, VoiceProfile};

/// Cloneable handle that asks a [`StudioBackend`] to reset its DSP state before the next frame.
///
/// The engine only sees a `Box<dyn InferenceBackend>`, so it cannot reach the chain directly.
/// It keeps this handle and calls [`StudioResetHandle::request`] whenever it opens a new
/// generation; the backend consumes the request at the start of the next `process` call.
#[derive(Debug, Clone, Default)]
pub struct StudioResetHandle(Arc<AtomicBool>);

impl StudioResetHandle {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests a DSP state reset before the next processed frame.
    pub fn request(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Reports whether a reset is pending, without consuming it.
    #[must_use]
    pub fn is_requested(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    fn take(&self) -> bool {
        self.0.swap(false, Ordering::AcqRel)
    }
}

/// Runs `inner`, then applies the studio chain to the processed frame.
pub struct StudioBackend<B: InferenceBackend> {
    inner: B,
    chain: StudioChain,
    control: Arc<StudioControl>,
    applied: Preset,
    reset: StudioResetHandle,
}

impl<B: InferenceBackend> StudioBackend<B> {
    /// Wraps `inner`; the chain starts on the preset currently held by `control`.
    pub fn new(inner: B, control: Arc<StudioControl>) -> Self {
        Self::with_reset_handle(inner, control, StudioResetHandle::new())
    }

    /// Like [`StudioBackend::new`], but shares an existing reset handle (used by the engine so the
    /// handle survives a backend swap).
    pub fn with_reset_handle(
        inner: B,
        control: Arc<StudioControl>,
        reset: StudioResetHandle,
    ) -> Self {
        let applied = control.preset();
        Self {
            inner,
            chain: StudioChain::new(applied),
            control,
            applied,
            reset,
        }
    }

    /// Returns a handle that resets the DSP state before the next frame.
    pub fn reset_handle(&self) -> StudioResetHandle {
        self.reset.clone()
    }
}

impl<B: InferenceBackend> InferenceBackend for StudioBackend<B> {
    fn descriptor(&self) -> BackendDescriptor {
        self.inner.descriptor()
    }

    fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError> {
        let mut processed = self.inner.process(input)?;

        if self.reset.take() {
            self.chain.reset();
        }
        let wanted = self.control.preset();
        if wanted != self.applied {
            self.chain.set_preset(wanted);
            self.applied = wanted;
        }

        self.chain.process(&mut processed.samples);
        ProcessedFrame::checked(
            processed.samples,
            processed
                .algorithmic_latency_samples
                .saturating_add(self.chain.latency_samples()),
            processed.provenance,
        )
    }

    fn algorithmic_latency_samples(&self) -> u32 {
        self.inner
            .algorithmic_latency_samples()
            .saturating_add(self.chain.latency_samples())
    }

    fn set_voice_profile(&mut self, profile: Option<&VoiceProfile>) -> Result<(), InferenceError> {
        self.inner.set_voice_profile(profile)
    }

    fn supports_voice_profile(&self) -> bool {
        self.inner.supports_voice_profile()
    }
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]
mod tests {
    use super::*;
    use realtime_noise_contracts::HOP_SAMPLES;

    const INNER_LATENCY: u32 = 1_440;

    fn descriptor() -> BackendDescriptor {
        BackendDescriptor {
            backend: "fake",
            backend_version: "1",
            runtime: "test",
            runtime_version: "1",
            asset_id: "fake-asset".to_owned(),
            asset_sha256: "0".repeat(64),
            cpu_profile: "test",
        }
    }

    /// Returns the input unchanged, or fails when `fail` is set.
    struct FakeBackend {
        fail: bool,
        fail_profile: bool,
        profile_calls: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl FakeBackend {
        fn new(fail: bool) -> Self {
            Self {
                fail,
                fail_profile: false,
                profile_calls: Arc::default(),
            }
        }
    }

    impl InferenceBackend for FakeBackend {
        fn descriptor(&self) -> BackendDescriptor {
            descriptor()
        }

        fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError> {
            if self.fail {
                return Err(InferenceError::UnsupportedCpuProfile(
                    "fake failure".to_owned(),
                ));
            }
            ProcessedFrame::checked(*input, INNER_LATENCY, descriptor())
        }

        fn algorithmic_latency_samples(&self) -> u32 {
            INNER_LATENCY
        }

        fn set_voice_profile(
            &mut self,
            profile: Option<&VoiceProfile>,
        ) -> Result<(), InferenceError> {
            self.profile_calls
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if self.fail_profile {
                return crate::reject_unsupported_voice_profile(profile);
            }
            Ok(())
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

    fn backend_with(preset: Preset) -> (StudioBackend<FakeBackend>, Arc<StudioControl>) {
        let control = Arc::new(StudioControl::new(preset));
        let backend = StudioBackend::new(FakeBackend::new(false), Arc::clone(&control));
        (backend, control)
    }

    #[test]
    fn off_preset_returns_the_inner_output_bit_for_bit() {
        let (mut backend, _control) = backend_with(Preset::Off);
        for index in 0..20 {
            let input = tone(index);
            let output = backend.process(&input).map(|frame| frame.samples);
            assert_eq!(output.ok(), Some(input), "frame {index}");
        }
    }

    #[test]
    fn latency_is_inner_plus_chain_in_both_places() {
        let (mut backend, _control) = backend_with(Preset::Off);
        assert_eq!(backend.algorithmic_latency_samples(), INNER_LATENCY + 96);
        let reported = backend
            .process(&tone(0))
            .map(|frame| frame.algorithmic_latency_samples);
        assert_eq!(reported.ok(), Some(INNER_LATENCY + 96));
    }

    #[test]
    fn descriptor_is_delegated_to_the_inner_backend() {
        let (backend, _control) = backend_with(Preset::Off);
        assert_eq!(backend.descriptor(), descriptor());
    }

    #[test]
    fn inner_error_is_propagated() {
        let control = Arc::new(StudioControl::new(Preset::Off));
        let mut backend = StudioBackend::new(FakeBackend::new(true), control);
        assert!(matches!(
            backend.process(&tone(0)),
            Err(InferenceError::UnsupportedCpuProfile(_))
        ));
    }

    #[test]
    fn preset_change_on_the_control_reaches_the_chain() {
        let (mut backend, control) = backend_with(Preset::Off);
        let off_output = backend.process(&tone(0)).map(|frame| frame.samples);
        assert_eq!(off_output.ok(), Some(tone(0)));

        control.set_preset(Preset::Broadcast);
        let mut changed = false;
        for index in 1..200 {
            let input = tone(index);
            let output = backend.process(&input).map(|frame| frame.samples);
            if output.ok() != Some(input) {
                changed = true;
                break;
            }
        }
        assert!(changed, "Broadcast never altered the signal");
    }

    #[test]
    fn reset_request_restores_the_initial_chain_state() {
        let (mut warmed, _warmed_control) = backend_with(Preset::Broadcast);
        let (mut fresh, _fresh_control) = backend_with(Preset::Broadcast);
        for index in 0..30 {
            assert!(warmed.process(&tone(index)).is_ok());
        }

        warmed.reset_handle().request();
        let probe = tone(1_000);
        let after_reset = warmed.process(&probe).map(|frame| frame.samples);
        let first_frame = fresh.process(&probe).map(|frame| frame.samples);
        assert!(after_reset.is_ok());
        assert_eq!(after_reset.ok(), first_frame.ok());
    }

    #[test]
    fn reset_handle_is_shared_between_clones_and_consumed_once() {
        let handle = StudioResetHandle::new();
        let clone = handle.clone();
        assert!(!handle.is_requested());
        clone.request();
        assert!(handle.is_requested());
        assert!(handle.take());
        assert!(!handle.is_requested());
        assert!(!handle.take());
    }

    #[test]
    fn studio_backend_delegates_profile_to_inner() {
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let inner = FakeBackend {
            profile_calls: Arc::clone(&calls),
            ..FakeBackend::new(false)
        };
        let mut s = StudioBackend::new(inner, Arc::new(StudioControl::new(Preset::Off)));
        let p = VoiceProfile::identity("id", "n", "2026-10-05T00:00:00Z").unwrap();
        s.set_voice_profile(Some(&p)).unwrap();
        s.set_voice_profile(None).unwrap();
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[test]
    fn studio_backend_propagates_inner_profile_error() {
        let inner = FakeBackend {
            fail_profile: true,
            ..FakeBackend::new(false)
        };
        let mut s = StudioBackend::new(inner, Arc::new(StudioControl::new(Preset::Off)));
        let p = VoiceProfile::identity("id", "n", "2026-10-05T00:00:00Z").unwrap();
        let err = s.set_voice_profile(Some(&p)).unwrap_err();
        assert!(matches!(err, InferenceError::UnsupportedFeature(_)));
    }

    #[test]
    fn boxed_dyn_backend_can_be_wrapped() {
        let control = Arc::new(StudioControl::new(Preset::Off));
        let boxed: Box<dyn InferenceBackend> = Box::new(FakeBackend::new(false));
        let mut backend = StudioBackend::new(boxed, control);
        assert_eq!(backend.algorithmic_latency_samples(), INNER_LATENCY + 96);
        let output = backend.process(&tone(0)).map(|frame| frame.samples);
        assert_eq!(output.ok(), Some(tone(0)));
    }
}
