#ifndef NOISE_SUPPRESSOR_H
#define NOISE_SUPPRESSOR_H

#include <stddef.h>
#include <stdint.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

#define NS_NUM_BANDS 3
#define NS_MAX_FRAME_SAMPLES 1920

/**
 * Direct Form II Transposed Biquad Filter.
 * Exactly 2 state variables per biquad (s1, s2).
 */
typedef struct biquad_filter {
    float b0, b1, b2;
    float a1, a2;
    float s1, s2;
} biquad_filter_t;

/**
 * Realtime Multi-Band Noise Suppressor State.
 * Holds linear crossover filters, adaptive noise floor estimators,
 * Wiener gain states, and speech activity detector (VAD).
 *
 * GUARANTEES:
 * 1. Zero dynamic heap allocation (no malloc/calloc/free).
 * 2. Zero blocking calls (no locks, I/O, or syscalls).
 * 3. Execution time < 0.01 ms per 480-sample frame.
 * 4. Sample safety (finite check, soft polynomial limiter in [-1.0, 1.0]).
 */
typedef struct noise_suppressor {
    /* Multi-Band Crossover Filter Bank */
    biquad_filter_t hp90;   /* 90 Hz 2nd-order Butterworth HPF (eliminates 50/60 Hz hum & DC) */
    biquad_filter_t lp350;  /* 350 Hz 2nd-order Butterworth LPF (Low band: fan rumble & motor hum) */
    biquad_filter_t hp4500; /* 4500 Hz 2nd-order Butterworth HPF (High band: hiss & air) */

    /* Adaptive Per-Band Noise Floor Estimators */
    float noise_floor[NS_NUM_BANDS]; /* [0]=low, [1]=mid, [2]=high */
    float prev_band_gain[NS_NUM_BANDS];
    float prev_gate;

    /* Speech Activity Detector (VAD) & Downward Expander */
    float speech_activity;
    uint32_t hangover_frames;

    /* Diagnostics & Telemetry */
    uint64_t frames_processed;
    float current_snr_db;
    float current_attenuation_db;
    bool is_speech_active;
} noise_suppressor_t;

/**
 * Initializes the noise suppressor with 48 kHz Butterworth crossover coefficients.
 * Zero heap allocations.
 */
void noise_suppressor_init(noise_suppressor_t *ns);

/**
 * Resets internal filter states and noise estimates.
 */
void noise_suppressor_reset(noise_suppressor_t *ns);

/**
 * Realtime processing callback:
 * Cleans audio by suppressing stationary background noise (fans, hum, hiss)
 * via 3-band Wiener filtering and expanding ambient noise during speech pauses.
 */
void noise_suppressor_process(noise_suppressor_t *ns, const float *src, float *dst, size_t n_samples);

#ifdef __cplusplus
}
#endif

#endif /* NOISE_SUPPRESSOR_H */
