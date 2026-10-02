//! Native macOS CoreML and Apple Neural Engine (ANE) / Metal GPU FFI abstraction.
//!
//! Communicates with the Apple `CoreML.framework` and `Foundation.framework`
//! runtimes using the Objective-C runtime ABI.

use std::ffi::{CStr, CString};
use std::path::{Path, PathBuf};

use crate::error::CoreMlError;
use crate::tensor::CoreMlTensor;
use crate::types::ComputeUnit;

#[link(name = "CoreML", kind = "framework")]
#[link(name = "Foundation", kind = "framework")]
#[link(name = "objc")]
unsafe extern "C" {
    fn objc_getClass(name: *const libc::c_char) -> *mut libc::c_void;
    fn sel_registerName(name: *const libc::c_char) -> *mut libc::c_void;
    fn objc_msgSend();
}

type MsgSend0 = unsafe extern "C" fn(*mut libc::c_void, *mut libc::c_void) -> *mut libc::c_void;
type MsgSend1Ptr = unsafe extern "C" fn(
    *mut libc::c_void,
    *mut libc::c_void,
    *mut libc::c_void,
) -> *mut libc::c_void;
type MsgSend1I64 = unsafe extern "C" fn(*mut libc::c_void, *mut libc::c_void, i64);
type MsgSend2Ptr = unsafe extern "C" fn(
    *mut libc::c_void,
    *mut libc::c_void,
    *mut libc::c_void,
    *mut libc::c_void,
) -> *mut libc::c_void;
type MsgSend3Ptr = unsafe extern "C" fn(
    *mut libc::c_void,
    *mut libc::c_void,
    *mut libc::c_void,
    *mut libc::c_void,
    *mut libc::c_void,
) -> *mut libc::c_void;
type MsgSendShape = unsafe extern "C" fn(
    *mut libc::c_void,
    *mut libc::c_void,
    *mut libc::c_void,
    i64,
    *mut *mut libc::c_void,
) -> *mut libc::c_void;
type MsgSendDataPtr = unsafe extern "C" fn(*mut libc::c_void, *mut libc::c_void) -> *mut f32;
type MsgSendUtf8 =
    unsafe extern "C" fn(*mut libc::c_void, *mut libc::c_void) -> *const libc::c_char;

unsafe fn get_class(name: &str) -> Result<*mut libc::c_void, CoreMlError> {
    let c_name = CString::new(name)
        .map_err(|_| CoreMlError::ConfigurationFailed(format!("invalid class name: {name}")))?;
    // SAFETY: Calling system Objective-C runtime objc_getClass with valid null-terminated string.
    let cls = unsafe { objc_getClass(c_name.as_ptr()) };
    if cls.is_null() {
        Err(CoreMlError::PlatformNotSupported(format!(
            "Objective-C class '{name}' not found in CoreML/Foundation framework"
        )))
    } else {
        Ok(cls)
    }
}

unsafe fn get_sel(name: &str) -> *mut libc::c_void {
    let c_name = CString::new(name).unwrap_or_default();
    // SAFETY: Calling system Objective-C runtime sel_registerName with valid null-terminated string.
    unsafe { sel_registerName(c_name.as_ptr()) }
}

unsafe fn create_nsstring(text: &str) -> Result<*mut libc::c_void, CoreMlError> {
    let cls_nsstring = unsafe { get_class("NSString")? };
    let sel_string_with_utf8 = unsafe { get_sel("stringWithUTF8String:") };
    let c_str = CString::new(text).map_err(|_| {
        CoreMlError::ConfigurationFailed("invalid UTF-8 string for NSString".to_owned())
    })?;
    // SAFETY: Casting objc_msgSend to MsgSend1Ptr to invoke [NSString stringWithUTF8String:].
    let func: MsgSend1Ptr = unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    let nsstring = unsafe {
        func(
            cls_nsstring,
            sel_string_with_utf8,
            c_str.as_ptr() as *mut libc::c_void,
        )
    };
    if nsstring.is_null() {
        Err(CoreMlError::ConfigurationFailed(
            "failed to allocate NSString".to_owned(),
        ))
    } else {
        Ok(nsstring)
    }
}

unsafe fn release_object(obj: *mut libc::c_void) {
    if !obj.is_null() {
        let sel_release = unsafe { get_sel("release") };
        // SAFETY: Casting objc_msgSend to MsgSend0 to invoke [obj release].
        let func: MsgSend0 = unsafe { std::mem::transmute(objc_msgSend as *const ()) };
        unsafe { func(obj, sel_release) };
    }
}

unsafe fn retain_object(obj: *mut libc::c_void) -> *mut libc::c_void {
    if !obj.is_null() {
        let sel_retain = unsafe { get_sel("retain") };
        // SAFETY: Casting objc_msgSend to MsgSend0 to invoke [obj retain].
        let func: MsgSend0 = unsafe { std::mem::transmute(objc_msgSend as *const ()) };
        unsafe { func(obj, sel_retain) }
    } else {
        std::ptr::null_mut()
    }
}

unsafe fn parse_nserror(error_ptr: *mut libc::c_void) -> String {
    if error_ptr.is_null() {
        return "unknown CoreML error".to_owned();
    }
    let sel_desc = unsafe { get_sel("localizedDescription") };
    let sel_utf8 = unsafe { get_sel("UTF8String") };
    // SAFETY: Invoking [error localizedDescription] then [str UTF8String].
    let func_desc: MsgSend0 = unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    let ns_desc = unsafe { func_desc(error_ptr, sel_desc) };
    if ns_desc.is_null() {
        return "unspecified NSError".to_owned();
    }
    let func_utf8: MsgSendUtf8 = unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    let c_str_ptr = unsafe { func_utf8(ns_desc, sel_utf8) };
    if c_str_ptr.is_null() {
        "unreadable NSError message".to_owned()
    } else {
        // SAFETY: Pointer is non-null valid C-string from UTF8String.
        unsafe { CStr::from_ptr(c_str_ptr) }
            .to_string_lossy()
            .into_owned()
    }
}

/// Native macOS CoreML engine handle managing compiled `.mlmodelc` bundles,
/// compute units on Apple Silicon (ANE/GPU Metal), and pre-allocated I/O tensors.
#[derive(Debug)]
pub struct CoreMlEngineHandle {
    model: *mut libc::c_void,
    model_path: PathBuf,
    compute_unit: ComputeUnit,
    input_array: *mut libc::c_void,
    input_data_ptr: *mut f32,
    output_array: *mut libc::c_void,
    output_data_ptr: *mut f32,
    prediction_options: *mut libc::c_void,
    feature_provider_cls: *mut libc::c_void,
    input_feature_name: *mut libc::c_void,
    output_feature_name: *mut libc::c_void,
    simulated_failure: bool,
}

// SAFETY: All CoreML Objective-C object instances and tensors can be sent across threads.
unsafe impl Send for CoreMlEngineHandle {}

impl CoreMlEngineHandle {
    /// Loads a compiled `.mlmodelc` bundle and binds to Apple Neural Engine / Metal GPU.
    pub fn load(model_path: &Path, units: ComputeUnit) -> Result<Self, CoreMlError> {
        let abs_path = model_path
            .canonicalize()
            .unwrap_or_else(|_| model_path.to_path_buf());

        if !abs_path.exists() {
            return Err(CoreMlError::ModelNotFound(abs_path));
        }

        // SAFETY: Initialize Objective-C classes and CoreML configuration.
        unsafe {
            let cls_config = get_class("MLModelConfiguration")?;
            let cls_model = get_class("MLModel")?;
            let cls_url = get_class("NSURL")?;
            let cls_options = get_class("MLPredictionOptions")?;
            let cls_feature_provider = get_class("MLDictionaryFeatureProvider")?;
            let cls_multi_array = get_class("MLMultiArray")?;
            let cls_number = get_class("NSNumber")?;

            // 1. Create [MLModelConfiguration new]
            let sel_new = get_sel("new");
            let func_new: MsgSend0 = std::mem::transmute(objc_msgSend as *const ());
            let config = func_new(cls_config, sel_new);
            if config.is_null() {
                return Err(CoreMlError::ConfigurationFailed(
                    "failed to allocate MLModelConfiguration".to_owned(),
                ));
            }

            // 2. [config setComputeUnits:units]
            let sel_set_units = get_sel("setComputeUnits:");
            let func_set_units: MsgSend1I64 = std::mem::transmute(objc_msgSend as *const ());
            func_set_units(config, sel_set_units, units.to_ml_compute_units());

            // 3. NSURL from path
            let path_str = abs_path.to_string_lossy();
            let ns_path = create_nsstring(&path_str)?;
            let sel_file_url = get_sel("fileURLWithPath:");
            let func_url: MsgSend1Ptr = std::mem::transmute(objc_msgSend as *const ());
            let model_url = func_url(cls_url, sel_file_url, ns_path);
            if model_url.is_null() {
                release_object(config);
                return Err(CoreMlError::ModelLoadFailed(format!(
                    "failed to create NSURL for {}",
                    abs_path.display()
                )));
            }

            // 4. [MLModel modelWithContentsOfURL:configuration:error:]
            let sel_load_model = get_sel("modelWithContentsOfURL:configuration:error:");
            let func_load_model: MsgSend3Ptr = std::mem::transmute(objc_msgSend as *const ());
            let mut error: *mut libc::c_void = std::ptr::null_mut();
            let model = func_load_model(
                cls_model,
                sel_load_model,
                model_url,
                config,
                &mut error as *mut _ as *mut libc::c_void,
            );

            if model.is_null() {
                let err_msg = parse_nserror(error);
                release_object(config);
                return Err(CoreMlError::ModelLoadFailed(format!(
                    "CoreML model load error for {}: {err_msg}",
                    abs_path.display()
                )));
            }
            retain_object(model);

            // 5. Create MLPredictionOptions
            let pred_opts = func_new(cls_options, sel_new);
            retain_object(pred_opts);

            // 6. Pre-allocate MLMultiArray tensors for 480 samples @ 48 kHz mono: shape [1, 1, 480]
            let sel_number_int = get_sel("numberWithInteger:");
            let func_number: unsafe extern "C" fn(
                *mut libc::c_void,
                *mut libc::c_void,
                i64,
            ) -> *mut libc::c_void = std::mem::transmute(objc_msgSend as *const ());
            let dim0 = func_number(cls_number, sel_number_int, 1);
            let dim1 = func_number(cls_number, sel_number_int, 1);
            let dim2 = func_number(cls_number, sel_number_int, 480);

            let cls_array = get_class("NSArray")?;
            let sel_array_with_objs = get_sel("arrayWithObjects:count:");
            let func_array: unsafe extern "C" fn(
                *mut libc::c_void,
                *mut libc::c_void,
                *const *mut libc::c_void,
                usize,
            ) -> *mut libc::c_void = std::mem::transmute(objc_msgSend as *const ());
            let dims = [dim0, dim1, dim2];
            let shape_array = func_array(cls_array, sel_array_with_objs, dims.as_ptr(), 3);

            let sel_alloc = get_sel("alloc");
            let sel_init_shape = get_sel("initWithShape:dataType:error:");
            let func_alloc: MsgSend0 = std::mem::transmute(objc_msgSend as *const ());
            let func_init_shape: MsgSendShape = std::mem::transmute(objc_msgSend as *const ());
            let sel_data_ptr = get_sel("dataPointer");
            let func_data_ptr: MsgSendDataPtr = std::mem::transmute(objc_msgSend as *const ());

            // MLMultiArrayDataTypeFloat32 = 65568 (0x10000 | 32)
            const ML_MULTI_ARRAY_FLOAT32: i64 = 65568;

            let in_alloc = func_alloc(cls_multi_array, sel_alloc);
            let mut in_err: *mut libc::c_void = std::ptr::null_mut();
            let input_array = func_init_shape(
                in_alloc,
                sel_init_shape,
                shape_array,
                ML_MULTI_ARRAY_FLOAT32,
                &mut in_err,
            );
            if input_array.is_null() {
                let err_msg = parse_nserror(in_err);
                release_object(model);
                release_object(config);
                return Err(CoreMlError::TensorAllocationFailed(err_msg));
            }
            retain_object(input_array);
            let input_data_ptr = func_data_ptr(input_array, sel_data_ptr);

            let out_alloc = func_alloc(cls_multi_array, sel_alloc);
            let mut out_err: *mut libc::c_void = std::ptr::null_mut();
            let output_array = func_init_shape(
                out_alloc,
                sel_init_shape,
                shape_array,
                ML_MULTI_ARRAY_FLOAT32,
                &mut out_err,
            );
            if output_array.is_null() {
                let err_msg = parse_nserror(out_err);
                release_object(input_array);
                release_object(model);
                release_object(config);
                return Err(CoreMlError::TensorAllocationFailed(err_msg));
            }
            retain_object(output_array);
            let output_data_ptr = func_data_ptr(output_array, sel_data_ptr);

            let input_name = create_nsstring("input")?;
            retain_object(input_name);
            let output_name = create_nsstring("output")?;
            retain_object(output_name);

            release_object(config);

            Ok(Self {
                model,
                model_path: abs_path,
                compute_unit: units,
                input_array,
                input_data_ptr,
                output_array,
                output_data_ptr,
                prediction_options: pred_opts,
                feature_provider_cls: cls_feature_provider,
                input_feature_name: input_name,
                output_feature_name: output_name,
                simulated_failure: false,
            })
        }
    }

    /// Creates a simulated handle instance for unit tests and offline testing.
    #[must_use]
    pub fn new_simulated(model_name: &str, units: ComputeUnit) -> Self {
        Self {
            model: std::ptr::null_mut(),
            model_path: PathBuf::from(format!("{model_name}.mlmodelc")),
            compute_unit: units,
            input_array: std::ptr::null_mut(),
            input_data_ptr: std::ptr::null_mut(),
            output_array: std::ptr::null_mut(),
            output_data_ptr: std::ptr::null_mut(),
            prediction_options: std::ptr::null_mut(),
            feature_provider_cls: std::ptr::null_mut(),
            input_feature_name: std::ptr::null_mut(),
            output_feature_name: std::ptr::null_mut(),
            simulated_failure: false,
        }
    }

    /// Whether `predict` runs a loaded CoreML model. `false` for simulated handles, whose
    /// `predict` copies the input tensor to the output tensor.
    #[must_use]
    pub fn executes_inference(&self) -> bool {
        !self.model.is_null()
    }

    /// Evaluates inference on the pre-allocated tensors.
    pub fn predict(
        &mut self,
        input_tensor: &CoreMlTensor,
        output_tensor: &mut CoreMlTensor,
    ) -> Result<(), CoreMlError> {
        if self.simulated_failure {
            return Err(CoreMlError::SimulatedFailure);
        }

        input_tensor.verify_finite()?;

        if self.model.is_null() {
            output_tensor
                .as_mut_slice()
                .copy_from_slice(input_tensor.as_slice());
            output_tensor.verify_finite()?;
            return Ok(());
        }

        // SAFETY: Copy input into the pre-allocated CoreML MLMultiArray memory buffer
        unsafe {
            if !self.input_data_ptr.is_null() {
                std::ptr::copy_nonoverlapping(
                    input_tensor.as_slice().as_ptr(),
                    self.input_data_ptr,
                    input_tensor.len().min(480),
                );
            }

            // Execute prediction via [model predictionFromFeatures:options:error:]
            let sel_dict_with_obj = get_sel("dictionaryWithObject:forKey:");
            let cls_nsdict = get_class("NSDictionary")?;
            let func_dict: unsafe extern "C" fn(
                *mut libc::c_void,
                *mut libc::c_void,
                *mut libc::c_void,
                *mut libc::c_void,
            ) -> *mut libc::c_void = std::mem::transmute(objc_msgSend as *const ());
            let dict = func_dict(
                cls_nsdict,
                sel_dict_with_obj,
                self.input_array,
                self.input_feature_name,
            );

            let sel_init_dict = get_sel("initWithDictionary:error:");
            let sel_alloc = get_sel("alloc");
            let func_alloc: MsgSend0 = std::mem::transmute(objc_msgSend as *const ());
            let func_init_dict: MsgSend2Ptr = std::mem::transmute(objc_msgSend as *const ());
            let provider_alloc = func_alloc(self.feature_provider_cls, sel_alloc);
            let mut prov_err: *mut libc::c_void = std::ptr::null_mut();
            let provider = func_init_dict(
                provider_alloc,
                sel_init_dict,
                dict,
                &mut prov_err as *mut _ as *mut libc::c_void,
            );

            let sel_predict = get_sel("predictionFromFeatures:options:error:");
            let func_predict: MsgSend3Ptr = std::mem::transmute(objc_msgSend as *const ());
            let mut pred_err: *mut libc::c_void = std::ptr::null_mut();
            let output_features = func_predict(
                self.model,
                sel_predict,
                provider,
                self.prediction_options,
                &mut pred_err as *mut _ as *mut libc::c_void,
            );

            if output_features.is_null() {
                let msg = parse_nserror(pred_err);
                release_object(provider);
                return Err(CoreMlError::InferenceExecution(msg));
            }

            // Extract output feature multi-array and copy back
            let sel_feature_value = get_sel("featureValueForName:");
            let func_feature_val: MsgSend1Ptr = std::mem::transmute(objc_msgSend as *const ());
            let feat_val =
                func_feature_val(output_features, sel_feature_value, self.output_feature_name);

            if !feat_val.is_null() {
                let sel_multiarray_val = get_sel("multiArrayValue");
                let func_multi: MsgSend0 = std::mem::transmute(objc_msgSend as *const ());
                let out_arr = func_multi(feat_val, sel_multiarray_val);
                if !out_arr.is_null() {
                    let sel_data_ptr = get_sel("dataPointer");
                    let func_data: MsgSendDataPtr = std::mem::transmute(objc_msgSend as *const ());
                    let ptr = func_data(out_arr, sel_data_ptr);
                    if !ptr.is_null() {
                        std::ptr::copy_nonoverlapping(
                            ptr,
                            output_tensor.as_mut_slice().as_mut_ptr(),
                            output_tensor.len().min(480),
                        );
                    }
                }
            } else if !self.output_data_ptr.is_null() {
                std::ptr::copy_nonoverlapping(
                    self.output_data_ptr,
                    output_tensor.as_mut_slice().as_mut_ptr(),
                    output_tensor.len().min(480),
                );
            }

            release_object(provider);
        }

        output_tensor.verify_finite()?;
        Ok(())
    }

    /// Sets whether simulated failures should be triggered.
    pub fn set_simulated_failure(&mut self, fail: bool) {
        self.simulated_failure = fail;
    }

    /// Associated model path.
    #[must_use]
    pub fn model_path(&self) -> &Path {
        &self.model_path
    }

    /// Configured compute unit.
    #[must_use]
    pub const fn compute_unit(&self) -> ComputeUnit {
        self.compute_unit
    }
}

impl Drop for CoreMlEngineHandle {
    fn drop(&mut self) {
        // SAFETY: Release all allocated Objective-C objects.
        unsafe {
            release_object(self.input_feature_name);
            release_object(self.output_feature_name);
            release_object(self.prediction_options);
            release_object(self.input_array);
            release_object(self.output_array);
            release_object(self.model);
        }
    }
}

impl Clone for CoreMlEngineHandle {
    fn clone(&self) -> Self {
        // SAFETY: Retain all active Objective-C objects.
        unsafe {
            retain_object(self.input_feature_name);
            retain_object(self.output_feature_name);
            retain_object(self.prediction_options);
            retain_object(self.input_array);
            retain_object(self.output_array);
            retain_object(self.model);
        }
        Self {
            model: self.model,
            model_path: self.model_path.clone(),
            compute_unit: self.compute_unit,
            input_array: self.input_array,
            input_data_ptr: self.input_data_ptr,
            output_array: self.output_array,
            output_data_ptr: self.output_data_ptr,
            prediction_options: self.prediction_options,
            feature_provider_cls: self.feature_provider_cls,
            input_feature_name: self.input_feature_name,
            output_feature_name: self.output_feature_name,
            simulated_failure: self.simulated_failure,
        }
    }
}
