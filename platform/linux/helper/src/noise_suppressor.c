#include "noise_suppressor.h"
#include <string.h>
#include <math.h>

/* Butterworth 2nd-Order Filter Coefficients at 48000 Hz Sample Rate */

/* 90 Hz High-Pass (Q = 0.7071) - Removes DC, mechanical rumble, and 50/60 Hz mains hum */
#define HP90_B0  (0.9917041956f)
#define HP90_B1 (-1.9834083912f)
#define HP90_B2  (0.9917041956f)
#define HP90_A1 (-1.9833395696f)
#define HP90_A2  (0.9834772127f)

/* 350 Hz Low-Pass (Q = 0.7071) - Separates low fan harmonics and motor vibration */
#define LP350_B0  (0.0005082014f)
#define LP350_B1  (0.0010164028f)
#define LP350_B2  (0.0005082014f)
#define LP350_A1 (-1.9352295547f)
#define LP350_A2  (0.9372623602f)

/* 4500 Hz High-Pass (Q = 0.7071) - Separates fan air hiss and electronic white noise */
#define HP4500_B0  (0.6574551915f)
#define HP4500_B1 (-1.3149103830f)
#define HP4500_B2  (0.6574551915f)
#define HP4500_A1 (-1.1939133677f)
#define HP4500_A2  (0.4359073982f)

/* Dynamic thresholding bounds */
#define EPSILON 1e-7f
#define GATE_FLOOR 0.015f /* -36.5 dB attenuation during pauses */

static inline void biquad_init(biquad_filter_t *f, float b0, float b1, float b2, float a1, float a2) {
    if (!f) return;
    f->b0 = b0; f->b1 = b1; f->b2 = b2;
    f->a1 = a1; f->a2 = a2;
    f->s1 = 0.0f; f->s2 = 0.0f;
}

static inline float biquad_process(biquad_filter_t *f, float x) {
    float y = f->b0 * x + f->s1;
    f->s1 = f->b1 * x - f->a1 * y + f->s2;
    f->s2 = f->b2 * x - f->a2 * y;
    return y;
}

static inline void biquad_flush_denormals(biquad_filter_t *f) {
    if (fabsf(f->s1) < 1e-15f) f->s1 = 0.0f;
    if (fabsf(f->s2) < 1e-15f) f->s2 = 0.0f;
}

void noise_suppressor_init(noise_suppressor_t *ns) {
    if (!ns) return;
    memset(ns, 0, sizeof(*ns));

    biquad_init(&ns->hp90, HP90_B0, HP90_B1, HP90_B2, HP90_A1, HP90_A2);
    biquad_init(&ns->lp350, LP350_B0, LP350_B1, LP350_B2, LP350_A1, LP350_A2);
    biquad_init(&ns->hp4500, HP4500_B0, HP4500_B1, HP4500_B2, HP4500_A1, HP4500_A2);

    noise_suppressor_reset(ns);
}

void noise_suppressor_reset(noise_suppressor_t *ns) {
    if (!ns) return;

    ns->hp90.s1 = 0.0f; ns->hp90.s2 = 0.0f;
    ns->lp350.s1 = 0.0f; ns->lp350.s2 = 0.0f;
    ns->hp4500.s1 = 0.0f; ns->hp4500.s2 = 0.0f;

    ns->noise_floor[0] = 1e-4f;
    ns->noise_floor[1] = 1e-4f;
    ns->noise_floor[2] = 1e-4f;

    ns->prev_band_gain[0] = 0.20f;
    ns->prev_band_gain[1] = 0.50f;
    ns->prev_band_gain[2] = 0.20f;
    ns->prev_gate = GATE_FLOOR;

    ns->speech_activity = 0.0f;
    ns->hangover_frames = 0;
    ns->frames_processed = 0;
    ns->current_snr_db = 0.0f;
    ns->current_attenuation_db = 0.0f;
    ns->is_speech_active = false;
}

void noise_suppressor_process(noise_suppressor_t *ns, const float *src, float *dst, size_t n_samples) {
    if (!dst || n_samples == 0) return;
    if (!src || !ns) {
        memset(dst, 0, n_samples * sizeof(float));
        return;
    }

    size_t proc_count = (n_samples > NS_MAX_FRAME_SAMPLES) ? NS_MAX_FRAME_SAMPLES : n_samples;

    float low_buf[NS_MAX_FRAME_SAMPLES];
    float mid_buf[NS_MAX_FRAME_SAMPLES];
    float high_buf[NS_MAX_FRAME_SAMPLES];

    float sum_low = 0.0f;
    float sum_mid = 0.0f;
    float sum_high = 0.0f;

    /* 1. Multi-band Crossover Decomposition (Exact Bit-Perfect Summation) */
    for (size_t i = 0; i < proc_count; ++i) {
        float x = src[i];
        if (isnan(x) || isinf(x)) {
            x = 0.0f;
        } else if (x > 1.0f) {
            x = 1.0f;
        } else if (x < -1.0f) {
            x = -1.0f;
        }

        /* 90 Hz High-Pass Filter: strips DC offset and 50/60 Hz electrical mains hum */
        float hp = biquad_process(&ns->hp90, x);

        /* Complementary Crossover Filters */
        float low = biquad_process(&ns->lp350, hp);
        float high = biquad_process(&ns->hp4500, hp);
        /* Mid band contains voice formants; exact identity: low + mid + high == hp */
        float mid = hp - low - high;

        low_buf[i] = low;
        mid_buf[i] = mid;
        high_buf[i] = high;

        sum_low += low * low;
        sum_mid += mid * mid;
        sum_high += high * high;
    }

    /* Denormal prevention on internal filter state */
    biquad_flush_denormals(&ns->hp90);
    biquad_flush_denormals(&ns->lp350);
    biquad_flush_denormals(&ns->hp4500);

    /* 2. Band Energy & Adaptive Noise Floor Tracking */
    float e_low = sum_low / (float)proc_count;
    float e_mid = sum_mid / (float)proc_count;
    float e_high = sum_high / (float)proc_count;

    float energies[NS_NUM_BANDS] = { e_low, e_mid, e_high };

    for (size_t b = 0; b < NS_NUM_BANDS; ++b) {
        float energy = energies[b];
        float noise = ns->noise_floor[b];

        /* Fast tracking when energy drops; slow upward adaptation to avoid speech corruption */
        if (energy < noise) {
            ns->noise_floor[b] = 0.82f * noise + 0.18f * energy;
        } else {
            ns->noise_floor[b] = 0.997f * noise + 0.003f * energy;
        }

        if (ns->noise_floor[b] < EPSILON) {
            ns->noise_floor[b] = EPSILON;
        }
    }

    float snr_low = e_low / ns->noise_floor[0];
    float snr_mid = e_mid / ns->noise_floor[1];
    float snr_high = e_high / ns->noise_floor[2];

    /* 3. Voice Activity Detection (VAD) Pegged to Mid-Band Noise Floor */
    float mid_rms = sqrtf(e_mid);
    float mid_noise_rms = sqrtf(ns->noise_floor[1]);
    float vad_thresh = 2.2f * mid_noise_rms;
    if (vad_thresh < 0.008f) {
        vad_thresh = 0.008f;
    }

    bool is_speech = (snr_mid > 2.5f && mid_rms > vad_thresh);
    ns->is_speech_active = is_speech;

    /* Attack, Hangover (200 ms), and Release Dynamics */
    if (is_speech) {
        ns->speech_activity = 0.70f * ns->speech_activity + 0.30f * 1.0f;
        ns->hangover_frames = 20; /* 200 ms @ 10 ms/frame (480 samples) */
    } else {
        if (ns->hangover_frames > 0) {
            ns->hangover_frames--;
            /* Maintain open gate during word endings and pauses */
            ns->speech_activity = 0.93f * ns->speech_activity + 0.07f * 1.0f;
        } else {
            /* Smooth release down to downward expander floor (-36.5 dB) */
            ns->speech_activity = 0.88f * ns->speech_activity + 0.12f * GATE_FLOOR;
        }
    }

    float target_gate = ns->speech_activity;
    if (target_gate < GATE_FLOOR) target_gate = GATE_FLOOR;
    if (target_gate > 1.0f) target_gate = 1.0f;

    /* 4. Multi-Band Wiener Suppression Targets */
    /* Low band: suppress persistent fan rumble by up to -28 dB while preserving speech body */
    float g_low = snr_low / (snr_low + 5.0f);
    if (g_low > 0.70f) g_low = 0.70f;
    if (g_low < 0.04f) g_low = 0.04f;

    /* Mid band: preserve primary voice formants with minimal attenuation during speech */
    float g_mid = snr_mid / (snr_mid + 1.0f);
    if (g_mid > 1.00f) g_mid = 1.00f;
    if (g_mid < 0.05f) g_mid = 0.05f;

    /* High band: suppress air turbulence and hiss, opening for speech fricatives/sibilance */
    float g_high = snr_high / (snr_high + 3.0f);
    if (g_high > 0.85f) g_high = 0.85f;
    if (g_high < 0.03f) g_high = 0.03f;

    /* 5. C1 Continuous Gain Interpolation & Soft Polynomial Limiting */
    for (size_t i = 0; i < proc_count; ++i) {
        float alpha = (float)(i + 1) / (float)proc_count;
        float gl = (1.0f - alpha) * ns->prev_band_gain[0] + alpha * g_low;
        float gm = (1.0f - alpha) * ns->prev_band_gain[1] + alpha * g_mid;
        float gh = (1.0f - alpha) * ns->prev_band_gain[2] + alpha * g_high;
        float gt = (1.0f - alpha) * ns->prev_gate + alpha * target_gate;

        float sample_out = (low_buf[i] * gl + mid_buf[i] * gm + high_buf[i] * gh) * gt;

        /* Soft-knee polynomial limiter preventing harsh digital clipping */
        if (sample_out > 0.8f) {
            float excess = sample_out - 0.8f;
            sample_out = 0.8f + excess / (1.0f + excess * excess);
        } else if (sample_out < -0.8f) {
            float excess = -sample_out - 0.8f;
            sample_out = -(0.8f + excess / (1.0f + excess * excess));
        }

        if (sample_out > 1.0f) sample_out = 1.0f;
        if (sample_out < -1.0f) sample_out = -1.0f;

        dst[i] = sample_out;
    }

    /* Save gain state for seamless interpolation in next frame */
    ns->prev_band_gain[0] = g_low;
    ns->prev_band_gain[1] = g_mid;
    ns->prev_band_gain[2] = g_high;
    ns->prev_gate = target_gate;

    /* Zero any remaining samples beyond proc_count */
    for (size_t i = proc_count; i < n_samples; ++i) {
        dst[i] = 0.0f;
    }

    /* Telemetry updates */
    ns->frames_processed++;
    ns->current_snr_db = 10.0f * log10f((snr_mid > 1.0f) ? snr_mid : 1.0f);
    ns->current_attenuation_db = -20.0f * log10f((target_gate > 1e-4f) ? target_gate : 1e-4f);
}
