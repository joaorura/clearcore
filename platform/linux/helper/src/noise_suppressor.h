#ifndef NOISE_SUPPRESSOR_H
#define NOISE_SUPPRESSOR_H

#include <stddef.h>
#include <stdint.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

#define NS_NUM_BANDS 8
#define NS_MAX_FRAME_SAMPLES 1920

typedef struct noise_suppressor {
    /* Sub-band energy estimators */
    float band_energy[NS_NUM_BANDS];
    float noise_floor[NS_NUM_BANDS];
    float band_gain[NS_NUM_BANDS];

    /* High-pass DC filter state */
    float hp_x1;
    float hp_y1;

    /* Voice Activity Detector & Expander */
    float speech_activity;
    uint32_t hangover_samples;

    /* Total frame counter */
    uint64_t frames_processed;
} noise_suppressor_t;

/**
 * Initializes the noise suppressor state.
 * Must be called at startup. Zero heap allocations.
 */
void noise_suppressor_init(noise_suppressor_t *ns);

/**
 * Resets the internal filters and noise estimates.
 */
void noise_suppressor_reset(noise_suppressor_t *ns);

/**
 * Realtime processing callback:
 * Cleans audio by suppressing stationary background noise (fans, hum, hiss)
 * and expanding ambient room noise during speech pauses.
 *
 * GUARANTEES:
 * 1. Zero heap allocations (no malloc, calloc, realloc, free).
 * 2. Zero blocking calls (no locks, I/O, syscalls).
 * 3. Bounded execution (< 0.05 ms for 480 samples).
 * 4. Sample safety (finite check, soft limiting in [-1.0, 1.0]).
 */
void noise_suppressor_process(noise_suppressor_t *ns, const float *src, float *dst, size_t n_samples);

#ifdef __cplusplus
}
#endif

#endif /* NOISE_SUPPRESSOR_H */
