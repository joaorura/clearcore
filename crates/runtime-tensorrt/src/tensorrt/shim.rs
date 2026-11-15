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

/// Creates a TensorRT `IRuntime`; returns null on failure or when `create_fn` is `None`.
///
/// # Safety
/// - `create_fn`, when `Some`, must be the `createInferRuntime_INTERNAL` entry point of a loaded
///   TensorRT library that stays loaded for as long as the returned runtime (and anything created
///   from it) is alive, and it must accept `(ILogger*, i32)` as declared by
///   [`CreateInferRuntimeFn`].
/// - `version` must be a TensorRT version accepted by that library (`<= 0` selects the version the
///   shim was compiled against).
/// - A non-null result is owned by the caller and must be released exactly once with
///   [`trt_runtime_destroy`], after every engine deserialized from it has been destroyed.
#[cfg(not(has_trt_shim))]
#[allow(unused_variables)]
pub unsafe fn trt_runtime_create(create_fn: CreateInferRuntimeFn, version: i32) -> *mut c_void {
    std::ptr::null_mut()
}
/// Destroys a runtime created by [`trt_runtime_create`].
///
/// # Safety
/// - `runtime` must be null, or a pointer returned by [`trt_runtime_create`] that has not been
///   destroyed yet; it must not be used after this call (no double free).
/// - Every engine deserialized from `runtime` must already have been destroyed.
/// - The TensorRT library that created it must still be loaded.
#[cfg(not(has_trt_shim))]
#[allow(unused_variables)]
pub unsafe fn trt_runtime_destroy(runtime: *mut c_void) {}
/// Deserializes a serialized TensorRT engine; returns null on failure.
///
/// # Safety
/// - `runtime` must be a live pointer from [`trt_runtime_create`] (null is tolerated and yields
///   null), and no other thread may use that runtime concurrently with this call.
/// - `blob` must be valid for reads of `size` bytes (it may be null only together with a null
///   result) and must not be mutated for the duration of the call; TensorRT does not retain it
///   after the call returns.
/// - A non-null result is owned by the caller and must be released exactly once with
///   [`trt_engine_destroy`] before `runtime` is destroyed.
#[cfg(not(has_trt_shim))]
#[allow(unused_variables)]
pub unsafe fn trt_engine_deserialize(
    runtime: *mut c_void,
    blob: *const c_void,
    size: usize,
) -> *mut c_void {
    std::ptr::null_mut()
}
/// Destroys an engine returned by [`trt_engine_deserialize`].
///
/// # Safety
/// - `engine` must be null, or a pointer returned by [`trt_engine_deserialize`] that has not been
///   destroyed yet; it must not be used after this call.
/// - Every execution context created from `engine` must already have been destroyed.
/// - The runtime that produced it must not have been destroyed yet.
#[cfg(not(has_trt_shim))]
#[allow(unused_variables)]
pub unsafe fn trt_engine_destroy(engine: *mut c_void) {}
/// Returns the number of I/O tensors of the engine (0 for a null engine).
///
/// # Safety
/// - `engine` must be null, or a live pointer returned by [`trt_engine_deserialize`] that is not
///   being destroyed concurrently.
#[cfg(not(has_trt_shim))]
#[allow(unused_variables)]
pub unsafe fn trt_engine_get_nb_io_tensors(engine: *mut c_void) -> i32 {
    0
}
/// Returns the name of the I/O tensor at `index`, or null for a null engine.
///
/// # Safety
/// - `engine` must be null, or a live pointer returned by [`trt_engine_deserialize`].
/// - `index` must be in `0..trt_engine_get_nb_io_tensors(engine)`; otherwise TensorRT returns null.
/// - The returned pointer is a NUL-terminated string owned by the engine. It must not be freed or
///   written through, and must not be used after the engine is destroyed; copy it if it must outlive
///   the engine.
#[cfg(not(has_trt_shim))]
#[allow(unused_variables)]
pub unsafe fn trt_engine_get_io_tensor_name(engine: *mut c_void, index: i32) -> *const c_char {
    std::ptr::null()
}
/// Returns the I/O mode of tensor `name` as an `nvinfer1::TensorIOMode` value (-1 if `engine` or
/// `name` is null).
///
/// # Safety
/// - `engine` must be null, or a live pointer returned by [`trt_engine_deserialize`].
/// - `name` must be null, or point to a valid NUL-terminated string that stays valid for the
///   duration of the call.
#[cfg(not(has_trt_shim))]
#[allow(unused_variables)]
pub unsafe fn trt_engine_get_tensor_mode(engine: *mut c_void, name: *const c_char) -> i32 {
    -1
}
/// Writes the shape of tensor `name` to `out_nb_dims`/`out_dims`; returns `false` (writing
/// nothing) if any pointer is null.
///
/// # Safety
/// - `engine` must be a live pointer returned by [`trt_engine_deserialize`].
/// - `name` must point to a valid NUL-terminated string that stays valid for the duration of the
///   call, and name an existing tensor of the engine.
/// - `out_nb_dims` must be valid for a write of one `i32`.
/// - `out_dims` must be valid for writes of `nvinfer1::Dims::MAX_DIMS` (8) `i64` values; at most the
///   first `*out_nb_dims` entries are written. Unwritten entries are left untouched.
/// - Neither output may alias the other or `name`, and no other thread may access them during the
///   call.
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
/// Creates an execution context for `engine`; returns null on failure or for a null engine.
///
/// # Safety
/// - `engine` must be null, or a live pointer returned by [`trt_engine_deserialize`].
/// - A non-null result is owned by the caller and must be released exactly once with
///   [`trt_context_destroy`] before `engine` is destroyed.
#[cfg(not(has_trt_shim))]
#[allow(unused_variables)]
pub unsafe fn trt_context_create(engine: *mut c_void) -> *mut c_void {
    std::ptr::null_mut()
}
/// Destroys a context returned by [`trt_context_create`].
///
/// # Safety
/// - `context` must be null, or a pointer returned by [`trt_context_create`] that has not been
///   destroyed yet; it must not be used after this call.
/// - Work enqueued on it via [`trt_context_enqueue_v3`] must have completed (synchronize the CUDA
///   stream first), and the device buffers bound to it must still be alive until then.
/// - Its engine must not have been destroyed yet.
#[cfg(not(has_trt_shim))]
#[allow(unused_variables)]
pub unsafe fn trt_context_destroy(context: *mut c_void) {}
/// Sets the shape of input tensor `name`; returns `false` on invalid arguments or if TensorRT
/// rejects the shape.
///
/// # Safety
/// - `context` must be a live pointer returned by [`trt_context_create`]; a context is not
///   thread-safe, so no other thread may use it during the call.
/// - `name` must point to a valid NUL-terminated string that stays valid for the duration of the
///   call, and name an input tensor of the context's engine.
/// - `dims` must be valid for reads of `nb_dims` `i64` values, with
///   `0 <= nb_dims <= nvinfer1::Dims::MAX_DIMS` (8).
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
/// Binds the device buffer `device_ptr` to tensor `name`; returns `false` on invalid arguments or
/// if TensorRT rejects the binding.
///
/// # Safety
/// - `context` must be a live pointer returned by [`trt_context_create`]; a context is not
///   thread-safe, so no other thread may use it during the call.
/// - `name` must point to a valid NUL-terminated string that stays valid for the duration of the
///   call, and name an I/O tensor of the context's engine.
/// - `device_ptr` must be a CUDA device pointer (not a host pointer) with enough room for the
///   tensor's full shape and element type, aligned as TensorRT requires for that type. It must
///   stay allocated, on the context's CUDA device, until the context is destroyed or the tensor is
///   rebound, including while work is enqueued.
#[cfg(not(has_trt_shim))]
#[allow(unused_variables)]
pub unsafe fn trt_context_set_tensor_address(
    context: *mut c_void,
    name: *const c_char,
    device_ptr: *mut c_void,
) -> bool {
    false
}
/// Enqueues inference of the context on `stream`; returns `false` if `context` is null or TensorRT
/// rejects the call. The call is asynchronous: results are only complete after the stream is
/// synchronized.
///
/// # Safety
/// - `context` must be a live pointer returned by [`trt_context_create`], with every I/O tensor
///   address bound via [`trt_context_set_tensor_address`] and every dynamic input shape set. A
///   context is not thread-safe, so concurrent enqueues on the same context are forbidden.
/// - `stream` must be a valid `CUstream` created on the same CUDA device, with that device's CUDA
///   context current on the calling thread, and must outlive all work enqueued on it (null selects
///   the default stream).
/// - The device buffers bound to the context must stay alive, and must not be accessed in a
///   conflicting way, until the work completes on `stream`.
#[cfg(not(has_trt_shim))]
#[allow(unused_variables)]
pub unsafe fn trt_context_enqueue_v3(context: *mut c_void, stream: *mut c_void) -> bool {
    false
}

#[must_use]
pub fn is_trt_shim_compiled() -> bool {
    cfg!(has_trt_shim)
}
