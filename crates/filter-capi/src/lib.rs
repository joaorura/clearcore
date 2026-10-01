#![allow(unsafe_code)]
#![allow(clippy::missing_safety_doc)]

use std::ffi::CStr;
use std::os::raw::c_char;
use std::path::Path;

use realtime_noise_contracts::AudioFrame;
use realtime_noise_model::{ApprovedAssetManifest, CpuProfile, InferenceBackend, TractBackend};

pub struct ClearcoreFilter {
    backend: TractBackend,
}

/// Create a new neural filter instance backed by Tract and the approved DeepFilterNet3 model asset.
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

    let filter = Box::new(ClearcoreFilter { backend });
    Box::into_raw(filter)
}

/// Process a single 480-sample (10 ms @ 48 kHz) audio frame through the DeepFilterNet3 neural network.
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
