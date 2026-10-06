#include <NvInfer.h>
#include <cuda.h>
#include <cstdint>
#include <cstddef>

class TrtShimLogger : public nvinfer1::ILogger {
public:
    void log(Severity severity, const char* msg) noexcept override {
        // Silently drop verbose messages to avoid polluting production logs.
        (void)severity;
        (void)msg;
    }
};

static TrtShimLogger gTrtLogger;

typedef void* (*CreateInferRuntimeFn)(void*, int32_t);

extern "C" {

void* trt_runtime_create(CreateInferRuntimeFn create_fn, int32_t version) {
    if (!create_fn) {
        return nullptr;
    }
    if (version <= 0) {
        version = NV_TENSORRT_VERSION;
    }
    return create_fn(&gTrtLogger, version);
}

void trt_runtime_destroy(void* runtime) {
    if (runtime) {
        delete static_cast<nvinfer1::IRuntime*>(runtime);
    }
}

void* trt_engine_deserialize(void* runtime, const void* blob, size_t size) {
    if (!runtime || !blob || size == 0) {
        return nullptr;
    }
    return static_cast<nvinfer1::IRuntime*>(runtime)->deserializeCudaEngine(blob, size);
}

void trt_engine_destroy(void* engine) {
    if (engine) {
        delete static_cast<nvinfer1::ICudaEngine*>(engine);
    }
}

int32_t trt_engine_get_nb_io_tensors(void* engine) {
    if (!engine) {
        return 0;
    }
    return static_cast<nvinfer1::ICudaEngine*>(engine)->getNbIOTensors();
}

const char* trt_engine_get_io_tensor_name(void* engine, int32_t index) {
    if (!engine) {
        return nullptr;
    }
    return static_cast<nvinfer1::ICudaEngine*>(engine)->getIOTensorName(index);
}

int32_t trt_engine_get_tensor_mode(void* engine, const char* name) {
    if (!engine || !name) {
        return -1;
    }
    auto mode = static_cast<nvinfer1::ICudaEngine*>(engine)->getTensorIOMode(name);
    return static_cast<int32_t>(mode);
}

bool trt_engine_get_tensor_shape(
    void* engine,
    const char* name,
    int32_t* out_nb_dims,
    int64_t* out_dims
) {
    if (!engine || !name || !out_nb_dims || !out_dims) {
        return false;
    }
    nvinfer1::Dims dims = static_cast<nvinfer1::ICudaEngine*>(engine)->getTensorShape(name);
    *out_nb_dims = dims.nbDims;
    for (int32_t i = 0; i < dims.nbDims && i < nvinfer1::Dims::MAX_DIMS; ++i) {
        out_dims[i] = dims.d[i];
    }
    return true;
}

void* trt_context_create(void* engine) {
    if (!engine) {
        return nullptr;
    }
    return static_cast<nvinfer1::ICudaEngine*>(engine)->createExecutionContext();
}

void trt_context_destroy(void* context) {
    if (context) {
        delete static_cast<nvinfer1::IExecutionContext*>(context);
    }
}

bool trt_context_set_input_shape(
    void* context,
    const char* name,
    int32_t nb_dims,
    const int64_t* dims
) {
    if (!context || !name || nb_dims < 0 || nb_dims > nvinfer1::Dims::MAX_DIMS || !dims) {
        return false;
    }
    nvinfer1::Dims d{};
    d.nbDims = nb_dims;
    for (int32_t i = 0; i < nb_dims; ++i) {
        d.d[i] = dims[i];
    }
    return static_cast<nvinfer1::IExecutionContext*>(context)->setInputShape(name, d);
}

bool trt_context_set_tensor_address(void* context, const char* name, void* device_ptr) {
    if (!context || !name) {
        return false;
    }
    return static_cast<nvinfer1::IExecutionContext*>(context)->setTensorAddress(name, device_ptr);
}

bool trt_context_enqueue_v3(void* context, void* stream) {
    if (!context) {
        return false;
    }
    return static_cast<nvinfer1::IExecutionContext*>(context)->enqueueV3(
        reinterpret_cast<cudaStream_t>(stream)
    );
}

}
