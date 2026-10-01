#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <dlfcn.h>
#include <math.h>

typedef void* (*filter_create_fn)(const char* repo_root);
typedef int (*filter_process_fn)(void* filter, const float* in_samples, float* out_samples);
typedef void (*filter_free_fn)(void* filter);

static double get_time_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (double)ts.tv_sec * 1000.0 + (double)ts.tv_nsec / 1000000.0;
}

int main(void) {
    printf("[test_neural_filter] Opening libclearcore_filter.so...\n");
    void *lib = dlopen("./platform/linux/helper/lib/libclearcore_filter.so", RTLD_NOW | RTLD_GLOBAL);
    if (!lib) {
        fprintf(stderr, "dlopen failed: %s\n", dlerror());
        return 1;
    }

    filter_create_fn create_fn = (filter_create_fn)dlsym(lib, "clearcore_filter_create");
    filter_process_fn process_fn = (filter_process_fn)dlsym(lib, "clearcore_filter_process");
    filter_free_fn free_fn = (filter_free_fn)dlsym(lib, "clearcore_filter_free");

    if (!create_fn || !process_fn || !free_fn) {
        fprintf(stderr, "dlsym failed to locate symbols\n");
        return 1;
    }

    printf("[test_neural_filter] Initializing ClearCore DeepFilterNet3 neural model...\n");
    double t0 = get_time_ms();
    void *filter = create_fn(".");
    double init_ms = get_time_ms() - t0;
    if (!filter) {
        fprintf(stderr, "clearcore_filter_create failed!\n");
        return 1;
    }
    printf("[test_neural_filter] Model loaded and validated in %.2f ms!\n", init_ms);

    float in_frame[480];
    float out_frame[480];
    for (int i = 0; i < 480; i++) {
        in_frame[i] = ((float)rand() / (float)RAND_MAX - 0.5f) * 0.1f; /* White noise */
    }

    printf("[test_neural_filter] Warming up model with 10 frames...\n");
    for (int i = 0; i < 10; i++) {
        int rc = process_fn(filter, in_frame, out_frame);
        if (rc != 0) {
            fprintf(stderr, "Warmup frame %d failed with code %d\n", i, rc);
            return 1;
        }
    }

    printf("[test_neural_filter] Benchmarking 100 frames (1 second of real-time 48kHz audio)...\n");
    double total_ms = 0.0;
    double max_ms = 0.0;
    for (int i = 0; i < 100; i++) {
        double frame_t0 = get_time_ms();
        int rc = process_fn(filter, in_frame, out_frame);
        double frame_ms = get_time_ms() - frame_t0;
        if (rc != 0) {
            fprintf(stderr, "Frame %d failed with code %d\n", i, rc);
            return 1;
        }
        total_ms += frame_ms;
        if (frame_ms > max_ms) max_ms = frame_ms;
    }

    double avg_ms = total_ms / 100.0;
    printf("[test_neural_filter] 100 frames (1000ms audio) processed in %.2f ms total!\n", total_ms);
    printf("[test_neural_filter] Average inference latency: %.3f ms per 10ms frame (Budget: 10.0 ms)\n", avg_ms);
    printf("[test_neural_filter] Max inference latency: %.3f ms\n", max_ms);
    printf("[test_neural_filter] Realtime factor: %.1fx faster than real-time!\n", 10.0 / avg_ms);

    float in_energy = 0.0f;
    float out_energy = 0.0f;
    for (int i = 0; i < 480; i++) {
        in_energy += in_frame[i] * in_frame[i];
        out_energy += out_frame[i] * out_frame[i];
    }
    float snr_reduction_db = 10.0f * log10f((in_energy + 1e-12f) / (out_energy + 1e-12f));
    printf("[test_neural_filter] Noise attenuation: %.2f dB\n", snr_reduction_db);

    free_fn(filter);
    dlclose(lib);
    printf("[test_neural_filter] PASSED!\n");
    return 0;
}
