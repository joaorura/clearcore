#include "noise_suppressor.h"
#include <string.h>
#include <math.h>

/* Min noise floor baseline to avoid division by zero */
#define EPSILON 1e-7f
/* Minimum gain floor during active noise reduction (-24 dB) */
#define MIN_GAIN_FLOOR 0.063f
/* Maximum suppression during silence / speech pause (-40 dB) */
#define GATE_FLOOR 0.01f

void noise_suppressor_init(noise_suppressor_t *ns) {
    if (!ns) return;
    memset(ns, 0, sizeof(*ns));
    noise_suppressor_reset(ns);
}

void noise_suppressor_reset(noise_suppressor_t *ns) {
    if (!ns) return;
    for (size_t i = 0; i < NS_NUM_BANDS; ++i) {
        ns->band_energy[i] = 1e-4f;
        ns->noise_floor[i] = 1e-4f;
        ns->band_gain[i] = 1.0f;
    }
    ns->hp_x1 = 0.0f;
    ns->hp_y1 = 0.0f;
    ns->speech_activity = 0.5f;
    ns->hangover_samples = 0;
    ns->frames_processed = 0;
}

void noise_suppressor_process(noise_suppressor_t *ns, const float *src, float *dst, size_t n_samples) {
    if (!dst || n_samples == 0) return;
    if (!src || !ns) {
        if (dst) memset(dst, 0, n_samples * sizeof(float));
        return;
    }

    /* 1. High-Pass Filter (DC + low rumble cutoff ~70 Hz @ 48 kHz) */
    /* Transfer function: y[n] = 0.9908 * (y[n-1] + x[n] - x[n-1]) */
    const float hp_coeff = 0.9908f;
    float temp_filtered[NS_MAX_FRAME_SAMPLES];
    size_t proc_count = (n_samples > NS_MAX_FRAME_SAMPLES) ? NS_MAX_FRAME_SAMPLES : n_samples;

    float x1 = ns->hp_x1;
    float y1 = ns->hp_y1;
    float total_frame_energy = 0.0f;

    for (size_t i = 0; i < proc_count; ++i) {
        float x = src[i];
        if (isnan(x) || isinf(x)) x = 0.0f;
        float y = hp_coeff * (y1 + x - x1);
        x1 = x;
        y1 = y;
        temp_filtered[i] = y;
        total_frame_energy += y * y;
    }
    ns->hp_x1 = x1;
    ns->hp_y1 = y1;

    total_frame_energy /= (float)proc_count;

    /* 2. Sub-band Energy Estimation (8 critical bands approximation) */
    /* Using temporal derivative and curvature energy distribution */
    float local_bands[NS_NUM_BANDS] = {0};
    for (size_t i = 0; i < proc_count; ++i) {
        float s0 = temp_filtered[i];
        float s1 = (i > 0) ? temp_filtered[i - 1] : s0;
        float s2 = (i > 1) ? temp_filtered[i - 2] : s1;

        float diff1 = s0 - s1;
        float diff2 = s0 - 2.0f * s1 + s2;

        float d0 = s0 * s0;
        float d1 = diff1 * diff1;
        float d2 = diff2 * diff2;

        local_bands[0] += d0;                   /* Low bass (0 - 300 Hz) */
        local_bands[1] += d0 * 0.7f + d1 * 0.3f;/* Mid bass (300 - 600 Hz) */
        local_bands[2] += d0 * 0.5f + d1 * 0.5f;/* Fundamental voice (600 - 1.2 kHz) */
        local_bands[3] += d0 * 0.3f + d1 * 0.7f;/* Formants (1.2 - 2.4 kHz) */
        local_bands[4] += d1;                   /* Clarity (2.4 - 4.8 kHz) */
        local_bands[5] += d1 * 0.5f + d2 * 0.5f;/* Sibilance (4.8 - 8 kHz) */
        local_bands[6] += d2;                   /* Air (8 - 14 kHz) */
        local_bands[7] += d2 * 1.5f;            /* High hiss (14 - 24 kHz) */
    }

    for (size_t b = 0; b < NS_NUM_BANDS; ++b) {
        local_bands[b] /= (float)proc_count;
        /* Exponential moving average of band energy */
        ns->band_energy[b] = 0.8f * ns->band_energy[b] + 0.2f * local_bands[b];
    }

    /* 3. Noise Floor Tracking with Asymmetric Smoothing */
    /* Downward track (when energy drops): fast alpha ~ 0.1 */
    /* Upward track (when energy rises): very slow beta ~ 0.001 to prevent voice from raising noise floor */
    float voice_snr_sum = 0.0f;
    for (size_t b = 0; b < NS_NUM_BANDS; ++b) {
        float energy = ns->band_energy[b];
        float noise = ns->noise_floor[b];

        if (energy < noise) {
            ns->noise_floor[b] = 0.85f * noise + 0.15f * energy;
        } else {
            ns->noise_floor[b] = 0.998f * noise + 0.002f * energy;
        }

        /* Ensure noise floor doesn't drop below absolute minimum */
        if (ns->noise_floor[b] < EPSILON) {
            ns->noise_floor[b] = EPSILON;
        }

        /* Wiener spectral gain computation */
        float snr = energy / ns->noise_floor[b];
        float gain = snr / (snr + 1.6f);
        if (gain < MIN_GAIN_FLOOR) gain = MIN_GAIN_FLOOR;
        if (gain > 1.0f) gain = 1.0f;

        /* Smooth gain changes */
        ns->band_gain[b] = 0.75f * ns->band_gain[b] + 0.25f * gain;

        /* Voice bands are bands 1, 2, 3, 4 (300 Hz - 4.8 kHz) */
        if (b >= 1 && b <= 4) {
            voice_snr_sum += snr;
        }
    }

    /* 4. Voice Activity Detection & Downward Expander (Noise Gate) */
    float avg_voice_snr = voice_snr_sum / 4.0f;
    bool is_speech = (avg_voice_snr > 2.2f && total_frame_energy > 5e-5f);

    if (is_speech) {
        /* Instant voice attack (open within ~3 ms) */
        ns->speech_activity = 0.6f * ns->speech_activity + 0.4f * 1.0f;
        /* Set 150 ms hangover time (7200 samples @ 48 kHz) to preserve natural word endings */
        ns->hangover_samples = 7200;
    } else {
        if (ns->hangover_samples > proc_count) {
            ns->hangover_samples -= proc_count;
            /* In hangover period, maintain full open speech gate */
            ns->speech_activity = 0.92f * ns->speech_activity + 0.08f * 1.0f;
        } else {
            ns->hangover_samples = 0;
            /* Smooth release down to gate floor */
            ns->speech_activity = 0.94f * ns->speech_activity + 0.06f * GATE_FLOOR;
        }
    }

    float gate = ns->speech_activity;
    if (gate < GATE_FLOOR) gate = GATE_FLOOR;
    if (gate > 1.0f) gate = 1.0f;

    /* Compute effective overall spectral gain from voice bands */
    float effective_spectral_gain = (ns->band_gain[1] + ns->band_gain[2] + ns->band_gain[3] + ns->band_gain[4]) * 0.25f;
    float overall_gain = gate * effective_spectral_gain;

    /* 5. Synthesis & Soft Saturation Limiter */
    for (size_t i = 0; i < proc_count; ++i) {
        float out_sample = temp_filtered[i] * overall_gain;

        /* Soft-knee limiter (polynomial saturation to avoid hard digital clipping) */
        if (out_sample > 1.0f) {
            out_sample = 1.0f;
        } else if (out_sample < -1.0f) {
            out_sample = -1.0f;
        } else if (out_sample > 0.8f) {
            float excess = out_sample - 0.8f;
            out_sample = 0.8f + excess / (1.0f + excess * excess);
        } else if (out_sample < -0.8f) {
            float excess = -out_sample - 0.8f;
            out_sample = -(0.8f + excess / (1.0f + excess * excess));
        }

        dst[i] = out_sample;
    }

    /* Zero any remaining samples if n_samples > proc_count */
    for (size_t i = proc_count; i < n_samples; ++i) {
        dst[i] = 0.0f;
    }

    ns->frames_processed++;
}
