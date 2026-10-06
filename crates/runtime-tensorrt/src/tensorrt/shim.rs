use std::ffi::{c_char, c_int, c_void};

pub type CreateInferRuntimeFn =
    Option<unsafe extern "C" fn(logger: *mut c_void, version: c_int) -> *mut c_void>;

#[cfg(has_trt_shim)]
unsafe extern "C" {
    pub fn trt_runtime_create(create_fn: CreateInferRuntimeFn, version: i32) -> *mut c_void;
    pub fn trt_runtime_destroy(runtime: *mut c_void);
    pub fn trt_engine_deserialize(
        runtime: *mut c_void,
        blob: *const c_void,
        size: usize,
    ) -> *mut c_void;
    pub fn trt_engine_destroy(engine: *mut c_void);
    pub fn trt_engine_get_nb_io_tensors(engine: *mut c_void) -> i32;
    pub fn trt_engine_get_io_tensor_name(engine: *mut c_void, index: i32) -> *const c_char;
    pub fn trt_engine_get_tensor_mode(engine: *mut c_void, name: *const c_char) -> i32;
    pub fn trt_engine_get_tensor_shape(
        engine: *mut c_void,
        name: *const c_char,
        out_nb_dims: *mut i32,
        out_dims: *mut i64,
    ) -> bool;
    pub fn trt_context_create(engine: *mut c_void) -> *mut c_void;
    pub fn trt_context_destroy(context: *mut c_void);
    pub fn trt_context_set_input_shape(
        context: *mut c_void,
        name: *const c_char,
        nb_dims: i32,
        dims: *const i64,
    ) -> bool;
    pub fn trt_context_set_tensor_address(
        context: *mut c_void,
        name: *const c_char,
        device_ptr: *mut c_void,
    ) -> bool;
    pub fn trt_context_enqueue_v3(context: *mut c_void, stream: *mut c_void) -> bool;
}

#[cfg(not(has_trt_shim))]
#[allow(unused_variables)]
pub unsafe fn trt_runtime_create(create_fn: CreateInferRuntimeFn, version: i32) -> *mut c_void {
    std::ptr::null_mut()
}
#[cfg(not(has_trt_shim))]
#[allow(unused_variables)]
pub unsafe fn trt_runtime_destroy(runtime: *mut c_void) {}
#[cfg(not(has_trt_shim))]
#[allow(unused_variables)]
pub unsafe fn trt_engine_deserialize(
    runtime: *mut c_void,
    blob: *const c_void,
    size: usize,
) -> *mut c_void {
    std::ptr::null_mut()
}
#[cfg(not(has_trt_shim))]
#[allow(unused_variables)]
pub unsafe fn trt_engine_destroy(engine: *mut c_void) {}
#[cfg(not(has_trt_shim))]
#[allow(unused_variables)]
pub unsafe fn trt_engine_get_nb_io_tensors(engine: *mut c_void) -> i32 {
    0
}
#[cfg(not(has_trt_shim))]
#[allow(unused_variables)]
pub unsafe fn trt_engine_get_io_tensor_name(engine: *mut c_void, index: i32) -> *const c_char {
    std::ptr::null()
}
#[cfg(not(has_trt_shim))]
#[allow(unused_variables)]
pub unsafe fn trt_engine_get_tensor_mode(engine: *mut c_void, name: *const c_char) -> i32 {
    -1
}
#[cfg(not(has_trt_shim))]
#[allow(unused_variables)]
pub unsafe fn trt_engine_get_tensor_shape(
    engine: *mut c_void,
    name: *const c_char,
    out_nb_dims: *mut i32,
    out_dims: *mut i64,
) -> bool {
    false
}
#[cfg(not(has_trt_shim))]
#[allow(unused_variables)]
pub unsafe fn trt_context_create(engine: *mut c_void) -> *mut c_void {
    std::ptr::null_mut()
}
#[cfg(not(has_trt_shim))]
#[allow(unused_variables)]
pub unsafe fn trt_context_destroy(context: *mut c_void) {}
#[cfg(not(has_trt_shim))]
#[allow(unused_variables)]
pub unsafe fn trt_context_set_input_shape(
    context: *mut c_void,
    name: *const c_char,
    nb_dims: i32,
    dims: *const i64,
) -> bool {
    false
}
#[cfg(not(has_trt_shim))]
#[allow(unused_variables)]
pub unsafe fn trt_context_set_tensor_address(
    context: *mut c_void,
    name: *const c_char,
    device_ptr: *mut c_void,
) -> bool {
    false
}
#[cfg(not(has_trt_shim))]
#[allow(unused_variables)]
pub unsafe fn trt_context_enqueue_v3(context: *mut c_void, stream: *mut c_void) -> bool {
    false
}

#[must_use]
pub fn is_trt_shim_compiled() -> bool {
    cfg!(has_trt_shim)
}
