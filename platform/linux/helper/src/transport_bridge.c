#include "pipewire_helper.h"
#include <stdlib.h>

/**
 * Initializes bounded transport ring with preallocated slots.
 * Allocation is strictly performed at startup, never inside RT callback.
 */
int transport_bridge_init(bounded_transport_t *transport, size_t capacity) {
    if (!transport || capacity == 0) {
        return -1;
    }

    transport->slots = (wire_frame_envelope_v1_t *)calloc(capacity, sizeof(wire_frame_envelope_v1_t));
    if (!transport->slots) {
        return -1;
    }

    transport->capacity = capacity;
    atomic_init(&transport->write_idx, 0);
    atomic_init(&transport->read_idx, 0);
    atomic_init(&transport->active_generation, 0);
    atomic_init(&transport->pushed_count, 0);
    atomic_init(&transport->dropped_count, 0);
    atomic_init(&transport->underrun_count, 0);
    atomic_init(&transport->silence_count, 0);
    atomic_init(&transport->active, true);

    return 0;
}

/**
 * Frees transport resources upon helper shutdown.
 */
void transport_bridge_free(bounded_transport_t *transport) {
    if (!transport) {
        return;
    }
    if (transport->slots) {
        free(transport->slots);
        transport->slots = NULL;
    }
    transport->capacity = 0;
}

/**
 * Returns number of ready frames available for reading.
 */
size_t transport_bridge_available_read(const bounded_transport_t *transport) {
    if (!transport) return 0;
    uint64_t w = atomic_load_explicit(&transport->write_idx, memory_order_acquire);
    uint64_t r = atomic_load_explicit(&transport->read_idx, memory_order_relaxed);
    return (w >= r) ? (size_t)(w - r) : 0;
}

/**
 * Returns number of free slots available for writing.
 */
size_t transport_bridge_available_write(const bounded_transport_t *transport) {
    if (!transport) return 0;
    uint64_t w = atomic_load_explicit(&transport->write_idx, memory_order_relaxed);
    uint64_t r = atomic_load_explicit(&transport->read_idx, memory_order_acquire);
    uint64_t backlog = (w >= r) ? (w - r) : 0;
    return (backlog < transport->capacity) ? (transport->capacity - (size_t)backlog) : 0;
}

/**
 * Pushes a frame into the bounded ring buffer.
 * If buffer is full, drops frame and returns false (bounded drop policy).
 * Lock-free, zero allocation, zero blocking calls.
 */
bool transport_bridge_push(bounded_transport_t *transport, const wire_frame_envelope_v1_t *frame) {
    if (!transport || !transport->slots || !frame) {
        return false;
    }

    uint64_t w = atomic_load_explicit(&transport->write_idx, memory_order_relaxed);
    uint64_t r = atomic_load_explicit(&transport->read_idx, memory_order_acquire);

    if ((w - r) >= transport->capacity) {
        atomic_fetch_add_explicit(&transport->dropped_count, 1, memory_order_relaxed);
        return false;
    }

    size_t slot_index = (size_t)(w % transport->capacity);
    memcpy(&transport->slots[slot_index], frame, sizeof(wire_frame_envelope_v1_t));

    atomic_store_explicit(&transport->write_idx, w + 1, memory_order_release);
    atomic_fetch_add_explicit(&transport->pushed_count, 1, memory_order_relaxed);
    return true;
}

/**
 * Pops a frame from the bounded ring buffer.
 * If empty or inactive, returns false (underrun condition triggering digital silence).
 * Lock-free, zero allocation, zero blocking calls.
 */
bool transport_bridge_pop(bounded_transport_t *transport, wire_frame_envelope_v1_t *out_frame) {
    if (!transport || !transport->slots || !out_frame) {
        return false;
    }

    if (!atomic_load_explicit(&transport->active, memory_order_acquire)) {
        atomic_fetch_add_explicit(&transport->underrun_count, 1, memory_order_relaxed);
        return false;
    }

    uint64_t r = atomic_load_explicit(&transport->read_idx, memory_order_relaxed);
    uint64_t w = atomic_load_explicit(&transport->write_idx, memory_order_acquire);

    if (r >= w) {
        atomic_fetch_add_explicit(&transport->underrun_count, 1, memory_order_relaxed);
        return false;
    }

    size_t slot_index = (size_t)(r % transport->capacity);
    memcpy(out_frame, &transport->slots[slot_index], sizeof(wire_frame_envelope_v1_t));

    atomic_store_explicit(&transport->read_idx, r + 1, memory_order_release);
    return true;
}

/**
 * Sets current engine active generation.
 */
void transport_bridge_set_generation(bounded_transport_t *transport, uint64_t generation) {
    if (!transport) return;
    atomic_store_explicit(&transport->active_generation, generation, memory_order_release);
}

/**
 * Gets current engine active generation.
 */
uint64_t transport_bridge_get_generation(const bounded_transport_t *transport) {
    if (!transport) return 0;
    return atomic_load_explicit(&transport->active_generation, memory_order_acquire);
}

/**
 * Activates or deactivates transport bridge.
 */
void transport_bridge_set_active(bounded_transport_t *transport, bool active) {
    if (!transport) return;
    atomic_store_explicit(&transport->active, active, memory_order_release);
}

/**
 * Checks whether transport bridge is active.
 */
bool transport_bridge_is_active(const bounded_transport_t *transport) {
    if (!transport) return false;
    return atomic_load_explicit(&transport->active, memory_order_acquire);
}
