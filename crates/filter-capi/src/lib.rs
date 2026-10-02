#![allow(unsafe_code)]
#![allow(clippy::missing_safety_doc)]

use std::ffi::CStr;
use std::os::raw::c_char;
use std::path::Path;
use std::sync::Arc;

use realtime_noise_contracts::AudioFrame;
use realtime_noise_model::{
    ApprovedAssetManifest, CpuProfile, InferenceBackend, StudioBackend, TractBackend,
};
use studio_dsp::{Preset, StudioControl};

pub struct ClearcoreFilter {
    backend: StudioBackend<TractBackend>,
    control: Arc<StudioControl>,
}

/// Create a new neural filter instance backed by Tract and the approved DeepFilterNet3 model asset,
/// followed by the studio finishing chain. The studio preset starts at `Off` (bit-exact passthrough
/// of the neural output); select another one with `clearcore_filter_set_preset`.
///
/// # Safety
/// If `repo_root_path` is non-null, it must be a valid null-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clearcore_filter_create(
    repo_root_path: *const c_char,
) -> *mut ClearcoreFilter {
    let repo_root = if repo_root_path.is_null() {
        Path::new(".")
    } else {
        let c_str = unsafe { CStr::from_ptr(repo_root_path) };
        match c_str.to_str() {
            Ok(s) => Path::new(s),
            Err(_) => return std::ptr::null_mut(),
        }
    };

    let manifest = match ApprovedAssetManifest::verify(repo_root) {
        Ok(m) => m,
        Err(_) => return std::ptr::null_mut(),
    };

    let backend = match TractBackend::new(&manifest, CpuProfile::Avx2Minimum) {
        Ok(b) => b,
        Err(_) => return std::ptr::null_mut(),
    };

    let control = Arc::new(StudioControl::new(Preset::Off));
    let filter = Box::new(ClearcoreFilter {
        backend: StudioBackend::new(backend, Arc::clone(&control)),
        control,
    });
    Box::into_raw(filter)
}

/// Process a single 480-sample (10 ms @ 48 kHz) audio frame through the DeepFilterNet3 neural network and the studio chain.
///
/// # Safety
/// `filter` must be a valid pointer returned by `clearcore_filter_create`.
/// `in_samples` and `out_samples` must each point to at least 480 contiguous `f32` samples.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clearcore_filter_process(
    filter: *mut ClearcoreFilter,
    in_samples: *const f32,
    out_samples: *mut f32,
) -> i32 {
    if filter.is_null() || in_samples.is_null() || out_samples.is_null() {
        return -1;
    }
    let filter_ref = unsafe { &mut *filter };
    let in_slice = unsafe { std::slice::from_raw_parts(in_samples, 480) };
    let mut in_frame: AudioFrame = [0.0; 480];
    in_frame.copy_from_slice(in_slice);

    match filter_ref.backend.process(&in_frame) {
        Ok(processed) => {
            let out_slice = unsafe { std::slice::from_raw_parts_mut(out_samples, 480) };
            out_slice.copy_from_slice(&processed.samples);
            0
        }
        Err(_) => -2,
    }
}

/// Select the studio finishing preset applied after the neural network.
///
/// `preset` is `0` = Off, `1` = Natural, `2` = Podcast, `3` = Broadcast. The change takes effect at
/// the next 480-sample frame boundary.
///
/// Returns `0` on success, `-1` if `filter` is NULL and `-3` if `preset` is not one of the values
/// above (the current preset is left untouched). `-2` is reserved for `clearcore_filter_process`.
///
/// # Safety
/// `filter` must be a valid pointer returned by `clearcore_filter_create`, or NULL. It must not be
/// called concurrently with `clearcore_filter_process` or `clearcore_filter_free` on the same
/// handle: call it from the thread that calls `clearcore_filter_process` (the C helper does).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clearcore_filter_set_preset(
    filter: *mut ClearcoreFilter,
    preset: u8,
) -> i32 {
    if filter.is_null() {
        return -1;
    }
    let Some(preset) = Preset::from_u8(preset) else {
        return -3;
    };
    let filter_ref = unsafe { &*filter };
    filter_ref.control.set_preset(preset);
    0
}

/// Destroy and free a neural filter instance.
///
/// # Safety
/// `filter` must be a valid pointer returned by `clearcore_filter_create` or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clearcore_filter_free(filter: *mut ClearcoreFilter) {
    if !filter.is_null() {
        unsafe {
            drop(Box::from_raw(filter));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    fn repo_root() -> CString {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        CString::new(root.to_string_lossy().as_bytes()).unwrap_or_default()
    }

    fn create_filter() -> *mut ClearcoreFilter {
        let root = repo_root();
        unsafe { clearcore_filter_create(root.as_ptr()) }
    }

    /// Voiced-speech-like frame: 120 Hz fundamental with 20 harmonics and a 4 Hz syllabic envelope.
    fn voiced_frame(index: u32) -> [f32; 480] {
        let mut frame = [0.0_f32; 480];
        for (offset, sample) in (0_u32..).zip(frame.iter_mut()) {
            let t = (f64::from(index) * 480.0 + f64::from(offset)) / 48_000.0;
            let envelope = 0.5 + 0.5 * (std::f64::consts::TAU * 4.0 * t).sin();
            let voiced: f64 = (1..=20_u32)
                .map(|h| {
                    let h = f64::from(h);
                    (std::f64::consts::TAU * 120.0 * h * t).sin() / h
                })
                .sum();
            *sample = (0.25 * envelope * voiced) as f32;
        }
        frame
    }

    fn process(filter: *mut ClearcoreFilter, input: &[f32; 480]) -> ([f32; 480], i32) {
        let mut output = [0.0_f32; 480];
        let rc = unsafe { clearcore_filter_process(filter, input.as_ptr(), output.as_mut_ptr()) };
        (output, rc)
    }

    #[test]
    fn set_preset_rejects_a_null_handle() {
        assert_eq!(
            unsafe { clearcore_filter_set_preset(std::ptr::null_mut(), 0) },
            -1
        );
    }

    #[test]
    fn set_preset_rejects_unknown_values_and_keeps_the_filter_usable() {
        let filter = create_filter();
        assert!(!filter.is_null());
        assert_eq!(unsafe { clearcore_filter_set_preset(filter, 4) }, -3);
        assert_eq!(unsafe { clearcore_filter_set_preset(filter, 255) }, -3);
        assert_eq!(process(filter, &voiced_frame(0)).1, 0);
        unsafe { clearcore_filter_free(filter) };
    }

    #[test]
    fn set_preset_accepts_every_preset() {
        let filter = create_filter();
        assert!(!filter.is_null());
        for value in 0..=3_u8 {
            assert_eq!(unsafe { clearcore_filter_set_preset(filter, value) }, 0);
        }
        unsafe { clearcore_filter_free(filter) };
    }

    #[test]
    fn new_filter_reports_the_model_latency_plus_the_chain() {
        let filter = create_filter();
        assert!(!filter.is_null());
        let latency = unsafe { (*filter).backend.algorithmic_latency_samples() };
        assert_eq!(latency, 1_440 + 96);
        unsafe { clearcore_filter_free(filter) };
    }

    #[test]
    fn off_is_bit_exact_with_the_raw_neural_backend_and_a_preset_changes_it() {
        let root = repo_root();
        let root_path = Path::new(root.to_str().unwrap_or("."));
        let manifest = ApprovedAssetManifest::verify(root_path).ok();
        assert!(manifest.is_some());
        let Some(manifest) = manifest else { return };
        let reference = TractBackend::new(&manifest, CpuProfile::Avx2Minimum).ok();
        assert!(reference.is_some());
        let Some(mut reference) = reference else {
            return;
        };

        let filter = create_filter();
        assert!(!filter.is_null());

        for index in 0..30 {
            let input = voiced_frame(index);
            let expected = reference.process(&input).map(|frame| frame.samples).ok();
            let (output, rc) = process(filter, &input);
            assert_eq!(rc, 0);
            assert_eq!(Some(output), expected, "Off must not alter frame {index}");
        }

        assert_eq!(unsafe { clearcore_filter_set_preset(filter, 3) }, 0);
        let mut changed = false;
        for index in 30..130 {
            let input = voiced_frame(index);
            let expected = reference.process(&input).map(|frame| frame.samples).ok();
            let (output, rc) = process(filter, &input);
            assert_eq!(rc, 0);
            if Some(output) != expected {
                changed = true;
                break;
            }
        }
        assert!(changed, "Broadcast never altered the neural output");
        unsafe { clearcore_filter_free(filter) };
    }
}
