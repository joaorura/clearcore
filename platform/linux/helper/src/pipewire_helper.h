#ifndef PIPEWIRE_HELPER_H
#define PIPEWIRE_HELPER_H

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdatomic.h>
#include <string.h>

#include <pipewire/pipewire.h>
#include <spa/param/audio/format-utils.h>
#include <spa/param/audio/raw.h>
#include <spa/utils/result.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Audio Format Constants */
#define SAMPLE_RATE_HZ          48000
#define AUDIO_CHANNELS          1
#define HOP_SAMPLES             480
#define BYTES_PER_SAMPLE        ((uint32_t)sizeof(float))
#define HOP_BYTES               (HOP_SAMPLES * BYTES_PER_SAMPLE) /* 1920 bytes */
#define DEFAULT_CAPACITY_HOPS   24

/* Node Configuration Constants */
#define NODE_NAME_DEFAULT       "realtime-noise-source"
#define NODE_DESC_DEFAULT       "Realtime Noise Virtual Microphone"
#define MEDIA_CLASS_SOURCE      "Audio/Source"

/* Discontinuity Flags (mirroring crates/contracts/src/audio.rs) */
#define DISCONTINUITY_NONE                    0x00
#define DISCONTINUITY_CAPTURE_DROP            0x01
#define DISCONTINUITY_DEVICE_CHANGE           0x02
#define DISCONTINUITY_GENERATION_CHANGE       0x04
#define DISCONTINUITY_INFERENCE_DEADLINE_MISS 0x08

/* Wire Protocol Version 1 (mirroring crates/contracts/src/wire.rs) */
#define WIRE_VERSION_V1         1
#define WIRE_PAYLOAD_LEN_BYTES  1920
#define WIRE_ENVELOPE_SIZE      1960

/**
 * Binary wire envelope matching Rust WireFrameEnvelopeV1 exactly.
 * Total size: 1960 bytes, 8-byte aligned.
 */
typedef struct __attribute__((aligned(8))) wire_frame_envelope_v1 {
    uint32_t version_le;               /* Version identifier, must be 1 */
    uint32_t payload_len_bytes_le;     /* Length in bytes of samples array (1920) */
    uint32_t flags_le;                 /* Discontinuity bit flags */
    uint32_t reserved_le;              /* Reserved field, must be 0 */
    uint64_t sequence_le;              /* Monotonic frame sequence */
    uint64_t capture_monotonic_ns_le;  /* Capture timestamp in nanoseconds */
    uint64_t generation_le;            /* Audio engine generation counter */
    float samples[HOP_SAMPLES];        /* 480 Float32 LE samples */
} wire_frame_envelope_v1_t;

/**
 * Bounded lock-free single-producer single-consumer ring transport.
 * Operates strictly with bounded preallocated memory and atomic counters.
 */
typedef struct bounded_transport {
    wire_frame_envelope_v1_t *slots;
    size_t capacity;
    _Atomic uint64_t write_idx;
    _Atomic uint64_t read_idx;
    _Atomic uint64_t active_generation;
    _Atomic uint64_t pushed_count;
    _Atomic uint64_t dropped_count;
    _Atomic uint64_t underrun_count;
    _Atomic uint64_t silence_count;
    _Atomic bool active;
} bounded_transport_t;

/**
 * PipeWire Native Helper Context.
 */
typedef struct pipewire_helper_context {
    struct pw_main_loop *loop;
    struct pw_context *context;
    struct pw_core *core;
    struct pw_stream *stream;
    struct spa_hook stream_listener;
    struct spa_hook core_listener;
    bounded_transport_t transport;
    _Atomic bool running;
    _Atomic bool node_ready;
    uint32_t node_id;
    _Atomic uint64_t process_count;
    _Atomic uint64_t alloc_violations;
    _Atomic uint64_t blocking_violations;
} pipewire_helper_context_t;

/* Format Converter APIs */
void format_converter_zero_silence(float *dest, size_t n_samples);
void format_converter_f32_sanitize(const float *src, float *dest, size_t n_samples);
void format_converter_copy(const float *src, float *dest, size_t n_samples);

/* Transport Bridge APIs */
int transport_bridge_init(bounded_transport_t *transport, size_t capacity);
void transport_bridge_free(bounded_transport_t *transport);
bool transport_bridge_push(bounded_transport_t *transport, const wire_frame_envelope_v1_t *frame);
bool transport_bridge_pop(bounded_transport_t *transport, wire_frame_envelope_v1_t *out_frame);
size_t transport_bridge_available_read(const bounded_transport_t *transport);
size_t transport_bridge_available_write(const bounded_transport_t *transport);
void transport_bridge_set_generation(bounded_transport_t *transport, uint64_t generation);
uint64_t transport_bridge_get_generation(const bounded_transport_t *transport);
void transport_bridge_set_active(bounded_transport_t *transport, bool active);
bool transport_bridge_is_active(const bounded_transport_t *transport);

/* Realtime Processing Callback */
void transfer_bounded_buffers(void *userdata);

/* PipeWire Helper Lifecycle */
int pipewire_helper_init(pipewire_helper_context_t *ctx);
int pipewire_helper_start(pipewire_helper_context_t *ctx);
void pipewire_helper_stop(pipewire_helper_context_t *ctx);
void pipewire_helper_destroy(pipewire_helper_context_t *ctx);

#ifdef __cplusplus
}
#endif

#endif /* PIPEWIRE_HELPER_H */
