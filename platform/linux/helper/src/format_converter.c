#include "pipewire_helper.h"
#include <math.h>

/**
 * Emits fail-closed digital silence by writing pure zeros to destination buffer.
 * Zero allocation, zero blocking, zero raw audio leakage.
 */
void format_converter_zero_silence(float *dest, size_t n_samples) {
    if (!dest || n_samples == 0) {
        return;
    }
    memset(dest, 0, n_samples * sizeof(float));
}

/**
 * Sanitizes Float32 samples:
 * - Replaces non-finite values (NaN, +/-Inf) with 0.0f
 * - Clamps values to the legitimate range [-1.0f, +1.0f]
 * - Guarantees predictable, bounded Float32 LE output without clipping distortion.
 */
void format_converter_f32_sanitize(const float *src, float *dest, size_t n_samples) {
    if (!dest || n_samples == 0) {
        return;
    }
    if (!src) {
        format_converter_zero_silence(dest, n_samples);
        return;
    }

    for (size_t i = 0; i < n_samples; ++i) {
        float val = src[i];
        if (isnan(val) || isinf(val)) {
            val = 0.0f;
        } else if (val > 1.0f) {
            val = 1.0f;
        } else if (val < -1.0f) {
            val = -1.0f;
        }
        dest[i] = val;
    }
}

/**
 * Fast direct copy when format is already pre-validated.
 */
void format_converter_copy(const float *src, float *dest, size_t n_samples) {
    if (!dest || n_samples == 0) {
        return;
    }
    if (!src) {
        format_converter_zero_silence(dest, n_samples);
        return;
    }
    memcpy(dest, src, n_samples * sizeof(float));
}
