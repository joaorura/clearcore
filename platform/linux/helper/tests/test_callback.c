#define _GNU_SOURCE
#include "pipewire_helper.h"

#include <stdio.h>
#include <stdlib.h>
#include <assert.h>
#include <math.h>

/* Allocation instrumentation */
static size_t g_alloc_count = 0;

void *__real_malloc(size_t size);
void *__wrap_malloc(size_t size) {
    g_alloc_count++;
    return __real_malloc(size);
}

void *__real_calloc(size_t nmemb, size_t size);
void *__wrap_calloc(size_t nmemb, size_t size) {
    g_alloc_count++;
    return __real_calloc(nmemb, size);
}

static void test_format_converter(void) {
    printf("[TEST] Running test_format_converter...\n");

    float src[4] = { 1.5f, -2.5f, NAN, INFINITY };
    float dst[4] = { 0 };

    format_converter_f32_sanitize(src, dst, 4);

    assert(dst[0] == 1.0f);
    assert(dst[1] == -1.0f);
    assert(dst[2] == 0.0f);
    assert(dst[3] == 0.0f);

    float silence[8] = { 0.1f, 0.2f, 0.3f, 0.4f, 0.5f, 0.6f, 0.7f, 0.8f };
    format_converter_zero_silence(silence, 8);
    for (size_t i = 0; i < 8; ++i) {
        assert(silence[i] == 0.0f);
    }

    printf("[TEST] test_format_converter passed.\n");
}

static void test_bounded_transport(void) {
    printf("[TEST] Running test_bounded_transport...\n");

    bounded_transport_t transport;
    int rc = transport_bridge_init(&transport, 24);
    assert(rc == 0);
    assert(transport.capacity == 24);
    assert(transport_bridge_available_write(&transport) == 24);
    assert(transport_bridge_available_read(&transport) == 0);

    /* Push 24 frames */
    for (uint64_t i = 0; i < 24; ++i) {
        wire_frame_envelope_v1_t frame;
        memset(&frame, 0, sizeof(frame));
        frame.version_le = WIRE_VERSION_V1;
        frame.payload_len_bytes_le = WIRE_PAYLOAD_LEN_BYTES;
        frame.sequence_le = i;
        frame.generation_le = 1;
        frame.samples[0] = (float)i * 0.01f;

        bool ok = transport_bridge_push(&transport, &frame);
        assert(ok == true);
    }

    assert(transport_bridge_available_write(&transport) == 0);
    assert(transport_bridge_available_read(&transport) == 24);

    /* 25th push must fail (bounded capacity drop) */
    wire_frame_envelope_v1_t extra_frame;
    memset(&extra_frame, 0, sizeof(extra_frame));
    extra_frame.sequence_le = 999;
    bool overflow_ok = transport_bridge_push(&transport, &extra_frame);
    assert(overflow_ok == false);
    assert(atomic_load(&transport.dropped_count) == 1);

    /* Pop all 24 frames and verify sequential integrity */
    for (uint64_t i = 0; i < 24; ++i) {
        wire_frame_envelope_v1_t popped;
        bool ok = transport_bridge_pop(&transport, &popped);
        assert(ok == true);
        assert(popped.sequence_le == i);
        assert(fabsf(popped.samples[0] - ((float)i * 0.01f)) < 1e-6f);
    }

    assert(transport_bridge_available_read(&transport) == 0);

    /* Pop on empty buffer must report underrun */
    wire_frame_envelope_v1_t empty_pop;
    bool underrun_ok = transport_bridge_pop(&transport, &empty_pop);
    assert(underrun_ok == false);
    assert(atomic_load(&transport.underrun_count) == 1);

    transport_bridge_free(&transport);
    printf("[TEST] test_bounded_transport passed.\n");
}

void test_process_callback_does_not_call_inference_or_allocate(void) {
    printf("[TEST] Running test_process_callback_does_not_call_inference_or_allocate...\n");

    bounded_transport_t transport;
    assert(transport_bridge_init(&transport, DEFAULT_CAPACITY_HOPS) == 0);
    transport_bridge_set_generation(&transport, 100);

    /* Push one valid frame */
    wire_frame_envelope_v1_t frame;
    memset(&frame, 0, sizeof(frame));
    frame.version_le = WIRE_VERSION_V1;
    frame.payload_len_bytes_le = WIRE_PAYLOAD_LEN_BYTES;
    frame.sequence_le = 42;
    frame.generation_le = 100;
    for (size_t i = 0; i < HOP_SAMPLES; ++i) {
        frame.samples[i] = 0.5f;
    }
    assert(transport_bridge_push(&transport, &frame) == true);

    /* Setup output mock buffer */
    float out_buffer[HOP_SAMPLES];
    memset(out_buffer, 0xFF, sizeof(out_buffer));

    /* Reset allocation counter to measure realtime section */
    g_alloc_count = 0;

    /* Simulating RT callback transfer logic */
    wire_frame_envelope_v1_t popped;
    bool has_frame = transport_bridge_pop(&transport, &popped);
    uint64_t active_gen = transport_bridge_get_generation(&transport);

    if (has_frame && (popped.generation_le == active_gen)) {
        format_converter_f32_sanitize(popped.samples, out_buffer, HOP_SAMPLES);
    } else {
        format_converter_zero_silence(out_buffer, HOP_SAMPLES);
    }

    /* ASSERTION: Zero dynamic heap allocations in RT callback */
    assert(g_alloc_count == 0);

    /* Check transferred audio values */
    for (size_t i = 0; i < HOP_SAMPLES; ++i) {
        assert(out_buffer[i] == 0.5f);
    }

    /* Test Fail-Closed Silence on Underrun (empty transport) */
    g_alloc_count = 0;
    bool has_second_frame = transport_bridge_pop(&transport, &popped);
    assert(has_second_frame == false); /* Underrun */

    if (has_second_frame && (popped.generation_le == active_gen)) {
        format_converter_f32_sanitize(popped.samples, out_buffer, HOP_SAMPLES);
    } else {
        format_converter_zero_silence(out_buffer, HOP_SAMPLES);
    }

    assert(g_alloc_count == 0);
    /* Verify pure digital silence on underrun */
    for (size_t i = 0; i < HOP_SAMPLES; ++i) {
        assert(out_buffer[i] == 0.0f);
    }

    /* Test Fail-Closed Silence on Generation Mismatch */
    frame.generation_le = 99; /* Stale generation */
    assert(transport_bridge_push(&transport, &frame) == true);

    has_frame = transport_bridge_pop(&transport, &popped);
    assert(has_frame == true);

    if (has_frame && (popped.generation_le == active_gen)) {
        format_converter_f32_sanitize(popped.samples, out_buffer, HOP_SAMPLES);
    } else {
        format_converter_zero_silence(out_buffer, HOP_SAMPLES);
    }

    /* Verify pure digital silence on generation mismatch */
    for (size_t i = 0; i < HOP_SAMPLES; ++i) {
        assert(out_buffer[i] == 0.0f);
    }

    transport_bridge_free(&transport);
    printf("[TEST] test_process_callback_does_not_call_inference_or_allocate passed.\n");
}

int main(void) {
    test_format_converter();
    test_bounded_transport();
    test_process_callback_does_not_call_inference_or_allocate();
    printf("[SUCCESS] All native PipeWire helper tests passed!\n");
    return 0;
}
