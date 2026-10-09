#define _GNU_SOURCE
#include "pipewire_helper.h"

#include <stdio.h>
#include <stdlib.h>
#include <signal.h>
#include <unistd.h>
#include <fcntl.h>
#include <sys/file.h>
#include <sys/stat.h>
#include <sys/mman.h>
#include <errno.h>
#include <dlfcn.h>
#include <libgen.h>
#include <limits.h>

static pipewire_helper_context_t *g_ctx = NULL;
static int g_lock_fd = -1;

/* Sample Accumulator Helpers (Bounded, Zero Allocations) */
static void accumulator_init(sample_accumulator_t *acc) {
    if (!acc) return;
    memset(acc, 0, sizeof(*acc));
}

static void accumulator_push(sample_accumulator_t *acc, const float *samples, size_t n) {
    if (!acc || !samples || n == 0) return;

    /* If accumulator would overflow 4096 samples, drop stale head samples to maintain low latency */
    if (acc->len + n > ACCUMULATOR_MAX_SAMPLES) {
        size_t excess = (acc->len + n) - ACCUMULATOR_MAX_SAMPLES;
        if (excess > acc->len) excess = acc->len;
        acc->head = (acc->head + excess) % ACCUMULATOR_MAX_SAMPLES;
        acc->len -= excess;
    }

    size_t to_copy = n;
    if (to_copy > (ACCUMULATOR_MAX_SAMPLES - acc->len)) {
        to_copy = ACCUMULATOR_MAX_SAMPLES - acc->len;
    }

    for (size_t i = 0; i < to_copy; ++i) {
        size_t idx = (acc->head + acc->len) % ACCUMULATOR_MAX_SAMPLES;
        acc->buffer[idx] = samples[i];
        acc->len++;
    }
}

static bool accumulator_pop_frame(sample_accumulator_t *acc, float *dst) {
    if (!acc || !dst || acc->len < HOP_SAMPLES) {
        return false;
    }

    for (size_t i = 0; i < HOP_SAMPLES; ++i) {
        size_t idx = (acc->head + i) % ACCUMULATOR_MAX_SAMPLES;
        dst[i] = acc->buffer[idx];
    }

    acc->head = (acc->head + HOP_SAMPLES) % ACCUMULATOR_MAX_SAMPLES;
    acc->len -= HOP_SAMPLES;
    return true;
}

/* Shared Memory State Initialization */
static void init_shared_state(pipewire_helper_context_t *ctx) {
    if (!ctx) return;
    ctx->shared_state = NULL;
    ctx->shared_state_fd = -1;

    char state_path[256];
    const char *runtime_dir = getenv("XDG_RUNTIME_DIR");
    if (!runtime_dir) {
        runtime_dir = "/tmp";
    }
    snprintf(state_path, sizeof(state_path), "%s/%s", runtime_dir, CLEARCORE_STATE_FILE);

    /* O_NOFOLLOW + owner/regular-file check: see clearcore_state_open(). */
    int fd = clearcore_state_open(state_path);
    if (fd < 0) {
        return;
    }

    /* The file is exactly CLEARCORE_STATE_SIZE (16) bytes, as it has always been. An older file
     * keeps working: offset 12 (preset) is zero there, which means Off. See clearcore_state.h. */
    if (ftruncate(fd, sizeof(clearcore_shared_state_t)) != 0) {
        close(fd);
        return;
    }

    void *mapped = mmap(NULL, sizeof(clearcore_shared_state_t), PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    if (mapped == MAP_FAILED) {
        close(fd);
        return;
    }

    ctx->shared_state = (clearcore_shared_state_t *)mapped;
    ctx->shared_state_fd = fd;
}

/* Studio finishing preset: copy the value written by the Electron app into the filter. Runs in the
 * realtime callback, in Active mode only; costs one relaxed atomic load per hop. */
static void apply_studio_preset(pipewire_helper_context_t *ctx) {
    clearcore_state_sync_preset(ctx->shared_state, ctx->neural_filter, ctx->neural_set_preset_fn,
                                &ctx->applied_preset);
}

/* Core Events Listener for Synchronous Discovery */
static void core_event_done(void *data, uint32_t id, int seq) {
    pipewire_helper_context_t *ctx = (pipewire_helper_context_t *)data;
    if (ctx && id == PW_ID_CORE && seq == ctx->sync_seq) {
        ctx->sync_done = true;
    }
}

static const struct pw_core_events core_events = {
    .version = PW_VERSION_CORE_EVENTS,
    .done = core_event_done,
};

static void bluetooth_switch_profile(pipewire_helper_context_t *ctx, bool enable_headset);

/* Registry Listener for Auto-detecting Physical Microphone and Severing Self-Loops */
static void registry_event_global(void *data, uint32_t id, uint32_t permissions,
                                  const char *type, uint32_t version,
                                  const struct spa_dict *props) {
    pipewire_helper_context_t *ctx = (pipewire_helper_context_t *)data;
    (void)permissions;
    (void)version;
    if (!ctx || !props || !type) return;

    if (strcmp(type, PW_TYPE_INTERFACE_Node) == 0) {
        const char *media_class = spa_dict_lookup(props, PW_KEY_MEDIA_CLASS);
        const char *node_name = spa_dict_lookup(props, PW_KEY_NODE_NAME);

        if (media_class && strcmp(media_class, "Audio/Source") == 0) {
            if (node_name && strcmp(node_name, NODE_NAME_DEFAULT) != 0 && strstr(node_name, "realtime-noise") == NULL) {
                known_sources_add(&ctx->known_sources, id, node_name);
                if (ctx->target_device_id == 0 || ctx->target_device_id == id) {
                    ctx->target_device_id = id;
                    snprintf(ctx->target_device_name, sizeof(ctx->target_device_name), "%s", node_name);
                    fprintf(stderr, "[pipewire_helper] Auto-detected physical microphone: %s (Node ID: %u)\n", node_name, id);
                }
            }
        }
    } else if (strcmp(type, PW_TYPE_INTERFACE_Device) == 0) {
        const char *device_bus = spa_dict_lookup(props, PW_KEY_DEVICE_BUS);
        const char *device_name = spa_dict_lookup(props, PW_KEY_DEVICE_NAME);
        if ((device_bus && strcmp(device_bus, "bluetooth") == 0) ||
            (device_name && strstr(device_name, "bluez_card") != NULL)) {
            ctx->bluetooth_card_id = id;
            if (device_name) {
                snprintf(ctx->bluetooth_card_name, sizeof(ctx->bluetooth_card_name), "%s", device_name);
            }
            fprintf(stderr, "[pipewire_helper] Detected Bluetooth Audio Card: %s (Card ID: %u)\n",
                    device_name ? device_name : "bluez_card", id);
        }
    } else if (strcmp(type, PW_TYPE_INTERFACE_Link) == 0) {
        const char *out_node = spa_dict_lookup(props, PW_KEY_LINK_OUTPUT_NODE);
        const char *in_node = spa_dict_lookup(props, PW_KEY_LINK_INPUT_NODE);

        /* Fail-safe: Detect and sever any self-referential loop created by WirePlumber */
        if (out_node && in_node && ctx->node_id > 0 && ctx->capture_node_id > 0) {
            uint32_t out_id = (uint32_t)strtoul(out_node, NULL, 10);
            uint32_t in_id = (uint32_t)strtoul(in_node, NULL, 10);
            if (out_id == ctx->node_id && in_id == ctx->capture_node_id) {
                fprintf(stderr, "[pipewire_helper] Severed self-loop link %u -> %u (Link ID: %u)\n", out_id, in_id, id);
                if (ctx->registry) {
                    pw_registry_destroy(ctx->registry, id);
                }
                return;
            }
        }

        /* Track active consumer links reading from the virtual microphone */
        if (out_node && ctx->node_id > 0) {
            uint32_t out_id = (uint32_t)strtoul(out_node, NULL, 10);
            if (out_id == ctx->node_id) {
                /* New consumer connected to virtual microphone */
                bool exists = false;
                for (size_t i = 0; i < ctx->consumer_link_count; i++) {
                    if (ctx->consumer_link_ids[i] == id) {
                        exists = true;
                        break;
                    }
                }
                if (!exists && ctx->consumer_link_count < sizeof(ctx->consumer_link_ids) / sizeof(ctx->consumer_link_ids[0])) {
                    ctx->consumer_link_ids[ctx->consumer_link_count++] = id;
                    ctx->active_consumer_links++;
                    fprintf(stderr, "[pipewire_helper] [LINK] Consumer attached to virtual mic (Link ID: %u, Total active: %u)\n",
                            id, ctx->active_consumer_links);

                    /* Disarm idle timer */
                    if (ctx->bt_release_timer && ctx->loop) {
                        struct timespec value = {0, 0};
                        pw_loop_update_timer(pw_main_loop_get_loop(ctx->loop), ctx->bt_release_timer, &value, NULL, false);
                    }

                    /* Ensure physical capture stream is active */
                    if (ctx->capture_stream) {
                        pw_stream_set_active(ctx->capture_stream, true);
                    }
                    bluetooth_switch_profile(ctx, true);
                }
            }
        }
    }
}

static void registry_event_global_remove(void *data, uint32_t id) {
    pipewire_helper_context_t *ctx = (pipewire_helper_context_t *)data;
    if (!ctx) return;
    if (ctx->bluetooth_card_id == id) {
        ctx->bluetooth_card_id = 0;
        ctx->bluetooth_is_headset = false;
        fprintf(stderr, "[pipewire_helper] Bluetooth Audio Card removed (ID: %u)\n", id);
    }

    /* Check if a tracked consumer link was removed */
    for (size_t i = 0; i < ctx->consumer_link_count; i++) {
        if (ctx->consumer_link_ids[i] == id) {
            ctx->consumer_link_ids[i] = ctx->consumer_link_ids[ctx->consumer_link_count - 1];
            ctx->consumer_link_count--;
            if (ctx->active_consumer_links > 0) {
                ctx->active_consumer_links--;
            }
            fprintf(stderr, "[pipewire_helper] [LINK] Consumer disconnected from virtual mic (Link ID: %u, Remaining active: %u)\n",
                    id, ctx->active_consumer_links);

            if (ctx->active_consumer_links == 0) {
                fprintf(stderr, "[pipewire_helper] [LINK] No active consumers left on virtual mic. Arming 3.5s release timer.\n");
                if (ctx->bt_release_timer && ctx->loop) {
                    struct timespec value = {3, 500000000}; /* 3.5 seconds */
                    pw_loop_update_timer(pw_main_loop_get_loop(ctx->loop), ctx->bt_release_timer, &value, NULL, false);
                } else {
                    bluetooth_switch_profile(ctx, false);
                }
            }
            break;
        }
    }
}

static const struct pw_registry_events registry_events = {
    .version = PW_VERSION_REGISTRY_EVENTS,
    .global = registry_event_global,
    .global_remove = registry_event_global_remove,
};

/**
 * Realtime callback for physical microphone capture.
 * Dequeues captured samples, filters through noise suppressor according to mode,
 * and pushes envelopes into bounded transport ring.
 *
 * Order of decisions for every hop (the mode is read from the shared state first):
 *   1. Mute   -> digital silence; neither the neural model nor the studio chain runs.
 *   2. Bypass -> sanitized raw frame; neither the neural model nor the studio chain runs.
 *   3. Active -> the studio preset is synchronized, then neural_process_fn runs the model and the
 *                studio chain together (StudioBackend inside libclearcore_filter.so).
 *      Fallback (no library, or rc != 0): noise_suppressor_process, which has NO studio chain
 *      (documented degraded mode, see clearcore_state.h).
 */
void on_capture_process(void *userdata) {
    pipewire_helper_context_t *ctx = (pipewire_helper_context_t *)userdata;
    if (!ctx || !ctx->capture_stream) return;

    struct pw_buffer *b = pw_stream_dequeue_buffer(ctx->capture_stream);
    if (!b) return;

    struct spa_buffer *buf = b->buffer;
    if (!buf || buf->n_datas == 0 || !buf->datas[0].data) {
        pw_stream_queue_buffer(ctx->capture_stream, b);
        return;
    }

    const float *src = (const float *)buf->datas[0].data;
    uint32_t size_bytes = buf->datas[0].chunk->size;
    uint32_t n_samples = size_bytes / sizeof(float);

    if (n_samples > 0) {
        accumulator_push(&ctx->accumulator, src, n_samples);

        float raw_frame[HOP_SAMPLES];
        float processed_frame[HOP_SAMPLES];

        while (accumulator_pop_frame(&ctx->accumulator, raw_frame)) {
            uint32_t mode = CLEARCORE_MODE_ACTIVE;
            if (ctx->shared_state) {
                mode = atomic_load_explicit(&ctx->shared_state->mode, memory_order_relaxed);
            }

            if (mode == CLEARCORE_MODE_MUTE) {
                format_converter_zero_silence(processed_frame, HOP_SAMPLES);
            } else if (mode == CLEARCORE_MODE_BYPASS) {
                format_converter_f32_sanitize(raw_frame, processed_frame, HOP_SAMPLES);
            } else {
                /* Active: Real-time neural noise suppression (DeepFilterNet3) */
                if (ctx->neural_filter && ctx->neural_process_fn) {
                    apply_studio_preset(ctx);
                    int rc = ctx->neural_process_fn(ctx->neural_filter, raw_frame, processed_frame);
                    if (rc != 0) {
                        /* Fallback to DSP suppressor if neural inference reports an issue */
                        noise_suppressor_process(&ctx->suppressor, raw_frame, processed_frame, HOP_SAMPLES);
                    }
                } else {
                    noise_suppressor_process(&ctx->suppressor, raw_frame, processed_frame, HOP_SAMPLES);
                }
            }

            wire_frame_envelope_v1_t env;
            memset(&env, 0, sizeof(env));
            env.version_le = WIRE_VERSION_V1;
            env.payload_len_bytes_le = WIRE_PAYLOAD_LEN_BYTES;
            env.sequence_le = atomic_fetch_add_explicit(&ctx->capture_sequence, 1, memory_order_relaxed);
            env.generation_le = transport_bridge_get_generation(&ctx->transport);
            memcpy(env.samples, processed_frame, sizeof(processed_frame));

            transport_bridge_push(&ctx->transport, &env);
        }
    }

    pw_stream_queue_buffer(ctx->capture_stream, b);
}

static void on_capture_stream_state_changed(void *data, enum pw_stream_state old,
                                            enum pw_stream_state state, const char *error) {
    pipewire_helper_context_t *ctx = (pipewire_helper_context_t *)data;
    (void)old;

    switch (state) {
        case PW_STREAM_STATE_ERROR:
            fprintf(stderr, "[pipewire_helper] Capture stream error: %s\n", error ? error : "unspecified");
            atomic_store_explicit(&ctx->capture_ready, false, memory_order_release);
            break;
        case PW_STREAM_STATE_STREAMING:
            ctx->capture_node_id = pw_stream_get_node_id(ctx->capture_stream);
            atomic_store_explicit(&ctx->capture_ready, true, memory_order_release);
            fprintf(stderr, "[pipewire_helper] Capture stream streaming (Node ID: %u)\n", ctx->capture_node_id);
            break;
        case PW_STREAM_STATE_PAUSED:
            ctx->capture_node_id = pw_stream_get_node_id(ctx->capture_stream);
            atomic_store_explicit(&ctx->capture_ready, true, memory_order_release);
            fprintf(stderr, "[pipewire_helper] Capture stream paused (Node ID: %u)\n", ctx->capture_node_id);
            break;
        case PW_STREAM_STATE_UNCONNECTED:
            atomic_store_explicit(&ctx->capture_ready, false, memory_order_release);
            fprintf(stderr, "[pipewire_helper] Capture stream unconnected\n");
            break;
        default:
            break;
    }
}

static const struct pw_stream_events capture_stream_events = {
    .version = PW_VERSION_STREAM_EVENTS,
    .state_changed = on_capture_stream_state_changed,
    .process = on_capture_process,
};

/**
 * Realtime callback invoked by PipeWire audio thread when a buffer is available.
 * STRICT CONTRACT:
 * 1. Zero inference: No DSP, no ML/neural network model calls.
 * 2. Zero dynamic heap allocation: No malloc, calloc, realloc, free.
 * 3. Zero blocking calls: No locks, semaphores, file I/O, or printf.
 * 4. Fail-closed digital silence on engine underrun, absence, or generation mismatch.
 */
void transfer_bounded_buffers(void *userdata) {
    pipewire_helper_context_t *ctx = (pipewire_helper_context_t *)userdata;
    if (!ctx || !ctx->stream) {
        return;
    }

    struct pw_buffer *b = pw_stream_dequeue_buffer(ctx->stream);
    if (!b) {
        return;
    }

    struct spa_buffer *buf = b->buffer;
    if (!buf || buf->n_datas == 0 || !buf->datas[0].data) {
        pw_stream_queue_buffer(ctx->stream, b);
        return;
    }

    float *dst = (float *)buf->datas[0].data;
    uint32_t max_bytes = buf->datas[0].maxsize;
    uint32_t req_bytes = HOP_BYTES;

    if (max_bytes < req_bytes) {
        req_bytes = max_bytes;
    }
    uint32_t n_samples = req_bytes / BYTES_PER_SAMPLE;

    wire_frame_envelope_v1_t frame;
    bool has_frame = transport_bridge_pop(&ctx->transport, &frame);
    uint64_t active_gen = transport_bridge_get_generation(&ctx->transport);

    if (has_frame && (frame.generation_le == active_gen)) {
        /* Valid frame received: sanitize against IEEE 754 clipping */
        format_converter_f32_sanitize(frame.samples, dst, n_samples);
    } else {
        /* Fail-closed digital silence policy: emit pure zeros */
        format_converter_zero_silence(dst, n_samples);
        atomic_fetch_add_explicit(&ctx->transport.silence_count, 1, memory_order_relaxed);
    }

    buf->datas[0].chunk->offset = 0;
    buf->datas[0].chunk->size = req_bytes;
    buf->datas[0].chunk->stride = (int32_t)BYTES_PER_SAMPLE;

    pw_stream_queue_buffer(ctx->stream, b);
    atomic_fetch_add_explicit(&ctx->process_count, 1, memory_order_relaxed);
}

static void bluetooth_switch_profile(pipewire_helper_context_t *ctx, bool enable_headset) {
    if (!ctx || ctx->bluetooth_card_id == 0) return;

    if (enable_headset) {
        if (ctx->bluetooth_is_headset) return;
        ctx->bluetooth_is_headset = true;
        fprintf(stderr, "[pipewire_helper] [BT-SWITCH] Microphone in use: activating headset capture on Card %u (%s)\n",
                ctx->bluetooth_card_id, ctx->bluetooth_card_name);

        /* Connect and activate capture stream */
        if (ctx->capture_stream) {
            pw_stream_set_active(ctx->capture_stream, true);
        }
    } else {
        if (!ctx->bluetooth_is_headset) return;
        ctx->bluetooth_is_headset = false;
        fprintf(stderr, "[pipewire_helper] [BT-SWITCH] Microphone idle: releasing Bluetooth capture stream. OS / WirePlumber will restore high-fidelity A2DP automatically.\n");

        /* Deactivate capture stream completely so BlueZ knows microphone is not in use */
        if (ctx->capture_stream) {
            pw_stream_set_active(ctx->capture_stream, false);
        }
    }
}

static void on_bt_release_timer(void *data, uint64_t expirations) {
    (void)expirations;
    pipewire_helper_context_t *ctx = (pipewire_helper_context_t *)data;
    if (!ctx) return;

    fprintf(stderr, "[pipewire_helper] [BT-TIMER] Idle timer expired (3.5s without mic usage). Triggering release.\n");

    // Disarm timer
    if (ctx->bt_release_timer && ctx->loop) {
        struct timespec value = {0, 0};
        pw_loop_update_timer(pw_main_loop_get_loop(ctx->loop), ctx->bt_release_timer, &value, NULL, false);
    }

    /* Pause physical capture stream so PipeWire knows the mic is completely dormant */
    if (ctx->capture_stream) {
        fprintf(stderr, "[pipewire_helper] [BT-TIMER] Pausing physical capture stream\n");
        pw_stream_set_active(ctx->capture_stream, false);
    }

    bluetooth_switch_profile(ctx, false);
}

static void on_process(void *userdata) {
    transfer_bounded_buffers(userdata);
}

static void on_stream_state_changed(void *data, enum pw_stream_state old,
                                    enum pw_stream_state state, const char *error) {
    pipewire_helper_context_t *ctx = (pipewire_helper_context_t *)data;
    (void)old;

    switch (state) {
        case PW_STREAM_STATE_ERROR:
            fprintf(stderr, "[pipewire_helper] Stream error: %s\n", error ? error : "unspecified");
            atomic_store_explicit(&ctx->node_ready, false, memory_order_release);
            break;
        case PW_STREAM_STATE_STREAMING:
            ctx->node_id = pw_stream_get_node_id(ctx->stream);
            atomic_store_explicit(&ctx->node_ready, true, memory_order_release);
            fprintf(stderr, "[pipewire_helper] Stream streaming (Node ID: %u)\n", ctx->node_id);

            // Disarm idle release timer if active
            if (ctx->bt_release_timer && ctx->loop) {
                struct timespec value = {0, 0};
                pw_loop_update_timer(pw_main_loop_get_loop(ctx->loop), ctx->bt_release_timer, &value, NULL, false);
            }

            // Reactivate physical microphone capture stream
            if (ctx->capture_stream) {
                pw_stream_set_active(ctx->capture_stream, true);
            }

            // Stream is actively recording: acquire Bluetooth headset profile
            bluetooth_switch_profile(ctx, true);
            break;
        case PW_STREAM_STATE_PAUSED:
            ctx->node_id = pw_stream_get_node_id(ctx->stream);
            atomic_store_explicit(&ctx->node_ready, true, memory_order_release);
            fprintf(stderr, "[pipewire_helper] Stream paused (Node ID: %u)\n", ctx->node_id);

            // Arm 3.5s release timer to return to A2DP cleanly without thrashing
            if (ctx->bt_release_timer && ctx->loop) {
                struct timespec value = {3, 500000000}; // 3.5 seconds
                pw_loop_update_timer(pw_main_loop_get_loop(ctx->loop), ctx->bt_release_timer, &value, NULL, false);
            } else {
                if (ctx->capture_stream) {
                    pw_stream_set_active(ctx->capture_stream, false);
                }
                bluetooth_switch_profile(ctx, false);
            }
            break;
        case PW_STREAM_STATE_UNCONNECTED:
            atomic_store_explicit(&ctx->node_ready, false, memory_order_release);
            fprintf(stderr, "[pipewire_helper] Stream unconnected\n");
            if (ctx->capture_stream) {
                pw_stream_set_active(ctx->capture_stream, false);
            }
            bluetooth_switch_profile(ctx, false);
            break;
        default:
            break;
    }
}

static const struct pw_stream_events stream_events = {
    .version = PW_VERSION_STREAM_EVENTS,
    .state_changed = on_stream_state_changed,
    .process = on_process,
};

static void signal_handler(int sig) {
    (void)sig;
    if (g_ctx && g_ctx->loop) {
        atomic_store_explicit(&g_ctx->running, false, memory_order_release);
        pw_main_loop_quit(g_ctx->loop);
    }
}

/**
 * Acquires per-user exclusive lock to detect device contention.
 * If another helper instance holds the lock, returns -EBUSY (UnavailableBusy).
 */
static int acquire_instance_lock(void) {
    char lock_path[256];
    const char *runtime_dir = getenv("XDG_RUNTIME_DIR");
    if (!runtime_dir) {
        runtime_dir = "/tmp";
    }
    snprintf(lock_path, sizeof(lock_path), "%s/hippocamp_pipewire_helper.lock", runtime_dir);

    g_lock_fd = open(lock_path, O_CREAT | O_RDWR | O_NOFOLLOW | O_CLOEXEC, 0600);
    if (g_lock_fd < 0) {
        return -errno;
    }

    /* The lock may live in a world-writable directory (/tmp without XDG_RUNTIME_DIR): O_NOFOLLOW
     * stops a planted symlink, and this check on the open descriptor stops a planted foreign or
     * special file from being locked (or used to make the helper report "busy"). */
    struct stat lock_st;
    int lock_err = 0;
    if (fstat(g_lock_fd, &lock_st) != 0) {
        lock_err = errno;
    } else if (!S_ISREG(lock_st.st_mode)) {
        lock_err = EINVAL;
    } else if (lock_st.st_uid != geteuid()) {
        lock_err = EPERM;
    }
    if (lock_err != 0) {
        close(g_lock_fd);
        g_lock_fd = -1;
        return -lock_err;
    }

    if (flock(g_lock_fd, LOCK_EX | LOCK_NB) < 0) {
        if (errno == EWOULDBLOCK || errno == EAGAIN) {
            close(g_lock_fd);
            g_lock_fd = -1;
            return -EBUSY; /* UnavailableBusy */
        }
        close(g_lock_fd);
        g_lock_fd = -1;
        return -errno;
    }

    return 0;
}

static void release_instance_lock(void) {
    if (g_lock_fd >= 0) {
        flock(g_lock_fd, LOCK_UN);
        close(g_lock_fd);
        g_lock_fd = -1;
    }
}

static void neural_filter_init(pipewire_helper_context_t *ctx) {
    if (!ctx) return;
    ctx->neural_lib_handle = NULL;
    ctx->neural_filter = NULL;
    ctx->neural_process_fn = NULL;
    ctx->neural_free_fn = NULL;
    ctx->neural_set_preset_fn = NULL;
    ctx->applied_preset = CLEARCORE_PRESET_OFF;
    ctx->neural_set_voice_profile_fn = NULL;
    ctx->neural_clear_voice_profile_fn = NULL;
    ctx->neural_reload_active_profile_fn = NULL;
    ctx->neural_supports_voice_profile_fn = NULL;
    ctx->neural_is_voice_profile_active_fn = NULL;
    atomic_store_explicit(&ctx->applied_generation, 0, memory_order_relaxed);

    char exe_buf[PATH_MAX] = {0};
    char exe_dir[PATH_MAX] = {0};
    char lib_same_dir[PATH_MAX + 128] = {0};
    char lib_parent_dir[PATH_MAX + 128] = {0};
    char lib_grandparent_dir[PATH_MAX + 128] = {0};
    char repo_same_dir[PATH_MAX + 128] = {0};
    char repo_grandparent_dir[PATH_MAX + 128] = {0};

    ssize_t len = readlink("/proc/self/exe", exe_buf, sizeof(exe_buf) - 1);
    if (len > 0) {
        exe_buf[len] = '\0';
        char *d = dirname(exe_buf);
        if (d) {
            snprintf(exe_dir, sizeof(exe_dir), "%s", d);
            snprintf(lib_same_dir, sizeof(lib_same_dir), "%s/libclearcore_filter.so", exe_dir);
            snprintf(lib_parent_dir, sizeof(lib_parent_dir), "%s/../libclearcore_filter.so", exe_dir);
            snprintf(lib_grandparent_dir, sizeof(lib_grandparent_dir), "%s/../../libclearcore_filter.so", exe_dir);
            snprintf(repo_same_dir, sizeof(repo_same_dir), "%s", exe_dir);
            snprintf(repo_grandparent_dir, sizeof(repo_grandparent_dir), "%s/../..", exe_dir);
        }
    }

    const char *env_lib = getenv("CLEARCORE_FILTER_LIB");
    const char *home = getenv("HOME");
    char user_lib[PATH_MAX] = {0};
    char user_bin_lib[PATH_MAX] = {0};
    if (home) {
        snprintf(user_lib, sizeof(user_lib), "%s/.local/share/clearcore/libclearcore_filter.so", home);
        snprintf(user_bin_lib, sizeof(user_bin_lib), "%s/.local/share/clearcore/resources/bin/libclearcore_filter.so", home);
    }

    const char *lib_candidates[] = {
        env_lib ? env_lib : "",
        lib_same_dir[0] ? lib_same_dir : "",
        lib_parent_dir[0] ? lib_parent_dir : "",
        lib_grandparent_dir[0] ? lib_grandparent_dir : "",
        "platform/linux/helper/lib/libclearcore_filter.so",
        "./libclearcore_filter.so",
        "target/release/libclearcore_filter.so",
        "/opt/clearcore/libclearcore_filter.so",
        "/opt/clearcore/resources/bin/libclearcore_filter.so",
        user_lib[0] ? user_lib : "",
        user_bin_lib[0] ? user_bin_lib : "",
        "libclearcore_filter.so",
        NULL
    };

    void *lib = NULL;
    for (int i = 0; lib_candidates[i] != NULL; i++) {
        if (lib_candidates[i][0] == '\0') continue;
        lib = dlopen(lib_candidates[i], RTLD_NOW | RTLD_GLOBAL);
        if (lib) {
            fprintf(stderr, "[pipewire_helper] Loaded neural filter library: %s\n", lib_candidates[i]);
            break;
        }
    }

    if (!lib) {
        fprintf(stderr, "[pipewire_helper] Neural filter library not loaded: %s. Using DSP suppressor.\n", dlerror());
        return;
    }

    typedef void* (*create_fn_t)(const char*);
    typedef int (*process_fn_t)(void*, const float*, float*);
    typedef void (*free_fn_t)(void*);

    create_fn_t create_fn = (create_fn_t)dlsym(lib, "clearcore_filter_create");
    process_fn_t process_fn = (process_fn_t)dlsym(lib, "clearcore_filter_process");
    free_fn_t free_fn = (free_fn_t)dlsym(lib, "clearcore_filter_free");

    /* Optional: libraries built before the studio chain have no preset symbol. */
    clearcore_set_preset_fn_t set_preset_fn =
        (clearcore_set_preset_fn_t)dlsym(lib, "clearcore_filter_set_preset");
    if (!set_preset_fn) {
        fprintf(stderr, "[pipewire_helper] clearcore_filter_set_preset not found; studio presets disabled (Off).\n");
    }

    /* Optional: libraries built before voice-profile support have none of these symbols. Left NULL
     * so older libclearcore_filter.so builds keep working; the reload hooks below no-op on NULL. */
    clearcore_set_voice_profile_fn_t set_voice_profile_fn =
        (clearcore_set_voice_profile_fn_t)dlsym(lib, "clearcore_filter_set_voice_profile");
    clearcore_clear_voice_profile_fn_t clear_voice_profile_fn =
        (clearcore_clear_voice_profile_fn_t)dlsym(lib, "clearcore_filter_clear_voice_profile");
    clearcore_reload_active_profile_fn_t reload_active_profile_fn =
        (clearcore_reload_active_profile_fn_t)dlsym(lib, "clearcore_filter_reload_active_profile");
    clearcore_supports_voice_profile_fn_t supports_voice_profile_fn =
        (clearcore_supports_voice_profile_fn_t)dlsym(lib, "clearcore_filter_supports_voice_profile");
    clearcore_is_voice_profile_active_fn_t is_voice_profile_active_fn =
        (clearcore_is_voice_profile_active_fn_t)dlsym(lib, "clearcore_filter_is_voice_profile_active");
    if (!reload_active_profile_fn || !supports_voice_profile_fn || !is_voice_profile_active_fn) {
        fprintf(stderr, "[pipewire_helper] Voice-profile C-ABI absent; running unconditioned denoiser.\n");
    }

    if (!create_fn || !process_fn || !free_fn) {
        fprintf(stderr, "[pipewire_helper] Failed to resolve clearcore_filter symbols: %s\n", dlerror());
        dlclose(lib);
        return;
    }

    const char *env_repo = getenv("CLEARCORE_REPO_ROOT");
    char user_share[PATH_MAX] = {0};
    if (home) {
        snprintf(user_share, sizeof(user_share), "%s/.local/share/clearcore", home);
    }

    const char *repo_candidates[] = {
        env_repo ? env_repo : "",
        repo_grandparent_dir[0] ? repo_grandparent_dir : "",
        repo_same_dir[0] ? repo_same_dir : "",
        "/opt/clearcore",
        user_share[0] ? user_share : "",
        ".",
        "..",
        NULL
    };

    void *filter = NULL;
    for (int i = 0; repo_candidates[i] != NULL; i++) {
        if (repo_candidates[i][0] == '\0') continue;
        filter = create_fn(repo_candidates[i]);
        if (filter) {
            fprintf(stderr, "[pipewire_helper] Initialized DeepFilterNet3 neural model from: %s\n", repo_candidates[i]);
            break;
        }
    }

    if (!filter) {
        fprintf(stderr, "[pipewire_helper] Failed to instantiate DeepFilterNet3 neural model; fallback to DSP.\n");
        dlclose(lib);
        return;
    }

    /* Reload persisted active profile from disk BEFORE warmup so the warmup hops exercise the
     * personalized conditioning path (rc: 1 loaded, 0 none/neutral, <0 error). Never on the audio
     * thread — this runs in neural_filter_init on the control thread. */
    int profile_rc = 0;
    bool profile_loaded = false;
    if (reload_active_profile_fn && ctx->neural_filter) {
        ctx->neural_reload_active_profile_fn = reload_active_profile_fn;
        profile_rc = reload_active_profile_fn(filter);
        profile_loaded = (profile_rc >= 0);
    }

    /* Pre-heat neural inference kernels with 5 frames. */
    float warmup_in[HOP_SAMPLES] = {0};
    float warmup_out[HOP_SAMPLES] = {0};
    for (int i = 0; i < 5; i++) {
        process_fn(filter, warmup_in, warmup_out);
    }

    ctx->neural_lib_handle = lib;
    ctx->neural_filter = filter;
    ctx->neural_process_fn = process_fn;
    ctx->neural_free_fn = free_fn;
    ctx->neural_set_preset_fn = set_preset_fn;
    ctx->neural_set_voice_profile_fn = set_voice_profile_fn;
    ctx->neural_clear_voice_profile_fn = clear_voice_profile_fn;
    ctx->neural_supports_voice_profile_fn = supports_voice_profile_fn;
    ctx->neural_is_voice_profile_active_fn = is_voice_profile_active_fn;

    int supports_profile = 0;
    if (supports_voice_profile_fn) {
        supports_profile = supports_voice_profile_fn(filter);
    }
    int profile_active = 0;
    if (is_voice_profile_active_fn && profile_loaded) {
        profile_active = is_voice_profile_active_fn(filter);
    }
    if (supports_profile) {
        fprintf(stderr, "[pipewire_helper] Neural model supports voice profiles (pDFNet3 Pro). "
                        "Active profile at startup: %s (rc=%d)\n",
                profile_active ? "YES" : "no", profile_rc);
    } else {
        fprintf(stderr, "[pipewire_helper] Neural model is unconditioned base DFNet3 (no voice-profile support). "
                        "Active profile at startup: %s (rc=%d)\n",
                profile_active ? "YES" : "no", profile_rc);
    }

    /* Seed the generation poller from the current shared-state generation: only later *increments*
     * by the writer should trigger a reload. shared_state may be NULL (no daemon/UI yet). */
    if (ctx->shared_state) {
        atomic_store_explicit(&ctx->applied_generation,
                              atomic_load_explicit(&ctx->shared_state->generation, memory_order_relaxed),
                              memory_order_relaxed);
    }
    fprintf(stderr, "[pipewire_helper] ClearCore DeepFilterNet3 neural suppressor ACTIVE!\n");
}

static void neural_filter_free(pipewire_helper_context_t *ctx) {
    if (!ctx) return;
    if (ctx->neural_filter && ctx->neural_free_fn) {
        ctx->neural_free_fn(ctx->neural_filter);
        ctx->neural_filter = NULL;
    }
    if (ctx->neural_lib_handle) {
        dlclose(ctx->neural_lib_handle);
        ctx->neural_lib_handle = NULL;
    }
    ctx->neural_process_fn = NULL;
    ctx->neural_free_fn = NULL;
    ctx->neural_set_preset_fn = NULL;
    ctx->neural_set_voice_profile_fn = NULL;
    ctx->neural_clear_voice_profile_fn = NULL;
    ctx->neural_reload_active_profile_fn = NULL;
    ctx->neural_supports_voice_profile_fn = NULL;
    ctx->neural_is_voice_profile_active_fn = NULL;
    ctx->applied_preset = CLEARCORE_PRESET_OFF;
}

/**
 * Main-loop hook: reload the active voice profile if the shared-state `generation` changed since
 * the last reload. Invoked from the 250 ms poll timer and the SIGUSR1 signal callback, i.e. only
 * from the PipeWire main thread — never from on_capture_process. The reload C-ABI itself performs
 * disk I/O on this control thread and is a no-op when the library lacks voice-profile support.
 */
static void profile_check_and_reload(pipewire_helper_context_t *ctx) {
    if (!ctx || !ctx->shared_state) {
        return;
    }
    uint32_t current_gen = atomic_load_explicit(&ctx->shared_state->generation, memory_order_relaxed);
    uint32_t applied_gen = atomic_load_explicit(&ctx->applied_generation, memory_order_relaxed);
    if (current_gen == applied_gen) {
        return;
    }

    if (ctx->neural_filter && ctx->neural_reload_active_profile_fn) {
        int rc = ctx->neural_reload_active_profile_fn(ctx->neural_filter);
        const char *kind = "profile";
        if (rc == 0) {
            kind = "cleared to neutral";
        } else if (rc == 1) {
            kind = "loaded";
        } else if (rc < 0) {
            kind = "reload error";
        }
        fprintf(stderr, "[pipewire_helper] [PROFILE] generation %u -> %u: %s (rc=%d)\n",
                applied_gen, current_gen, kind, rc);
    }
    /* Accept the generation regardless of reload outcome: a failed reload (e.g. a profile still
     * being written) is retried on the next increment rather than re-attempted forever. */
    atomic_store_explicit(&ctx->applied_generation, current_gen, memory_order_relaxed);
}

/** 250 ms poll timer on the PipeWire main loop: detects shared-state generation increments. */
static void on_profile_poll_timer(void *data, uint64_t expirations) {
    (void)expirations;
    profile_check_and_reload((pipewire_helper_context_t *)data);
}

/** SIGUSR1 handler on the PipeWire main loop: reloads the voice profile immediately. */
static void on_profile_signal(void *data, int signal_number) {
    (void)signal_number;
    profile_check_and_reload((pipewire_helper_context_t *)data);
}

int pipewire_helper_init(pipewire_helper_context_t *ctx) {
    if (!ctx) return -1;
    memset(ctx, 0, sizeof(*ctx));

    atomic_init(&ctx->running, false);
    atomic_init(&ctx->node_ready, false);
    atomic_init(&ctx->capture_ready, false);
    atomic_init(&ctx->capture_sequence, 0);
    atomic_init(&ctx->process_count, 0);
    atomic_init(&ctx->alloc_violations, 0);
    atomic_init(&ctx->blocking_violations, 0);

    accumulator_init(&ctx->accumulator);
    noise_suppressor_init(&ctx->suppressor);
    init_shared_state(ctx);
    neural_filter_init(ctx);

    if (transport_bridge_init(&ctx->transport, DEFAULT_CAPACITY_HOPS) != 0) {
        return -1;
    }

    ctx->loop = pw_main_loop_new(NULL);
    if (!ctx->loop) {
        transport_bridge_free(&ctx->transport);
        return -1;
    }

    ctx->context = pw_context_new(pw_main_loop_get_loop(ctx->loop), NULL, 0);
    if (!ctx->context) {
        pw_main_loop_destroy(ctx->loop);
        transport_bridge_free(&ctx->transport);
        return -1;
    }

    ctx->core = pw_context_connect(ctx->context, NULL, 0);
    if (!ctx->core) {
        pw_context_destroy(ctx->context);
        pw_main_loop_destroy(ctx->loop);
        transport_bridge_free(&ctx->transport);
        return -1;
    }

    pw_core_add_listener(ctx->core, &ctx->core_events_listener, &core_events, ctx);

    /* Timer for releasing Bluetooth headset mode back to high-fidelity A2DP */
    ctx->bt_release_timer = pw_loop_add_timer(pw_main_loop_get_loop(ctx->loop), on_bt_release_timer, ctx);

    /* 250 ms poll timer (main thread only) for shared-state `generation` changes. Reloads the voice
     * profile between hops; never touches the realtime on_capture_process path. */
    if (ctx->neural_reload_active_profile_fn) {
        ctx->profile_timer_source = pw_loop_add_timer(pw_main_loop_get_loop(ctx->loop),
                                                      on_profile_poll_timer, ctx);
        if (ctx->profile_timer_source) {
            struct timespec interval = {0, 250 * 1000000L};
            pw_loop_update_timer(pw_main_loop_get_loop(ctx->loop), ctx->profile_timer_source,
                                 &interval, NULL, false);
        }

        /* SIGUSR1: instantaneous reload trigger on the main loop thread (the Electron app signals
         * this PID after writing active_profile.json). Registered via pw_loop_add_signal so the
         * async signal is marshalled onto the main loop, never onto the audio thread. */
        ctx->profile_signal_source = pw_loop_add_signal(pw_main_loop_get_loop(ctx->loop),
                                                        SIGUSR1, on_profile_signal, ctx);
    }

    /* Listen for available physical microphones via registry */
    ctx->registry = pw_core_get_registry(ctx->core, PW_VERSION_REGISTRY, 0);
    if (ctx->registry) {
        pw_registry_add_listener(ctx->registry, &ctx->core_listener, &registry_events, ctx);
    }

    /* Perform initial synchronous discovery so physical microphones are known BEFORE streams start */
    ctx->sync_done = false;
    ctx->sync_seq = pw_core_sync(ctx->core, PW_ID_CORE, 0);
    for (int iter = 0; iter < 100 && !ctx->sync_done; iter++) {
        pw_loop_iterate(pw_main_loop_get_loop(ctx->loop), 10);
    }

    if (!ctx->shared_state) {
        atomic_store_explicit(&ctx->applied_generation, 0, memory_order_relaxed);
    }

    return 0;
}

int pipewire_helper_start(pipewire_helper_context_t *ctx) {
    if (!ctx || !ctx->core) return -1;

    /* 1. Initialize Virtual Output/Source Stream (realtime-noise-source) */
    struct pw_properties *props = pw_properties_new(
        PW_KEY_MEDIA_TYPE, "Audio",
        PW_KEY_MEDIA_CATEGORY, "Source",
        PW_KEY_MEDIA_ROLE, "Communication",
        PW_KEY_MEDIA_CLASS, MEDIA_CLASS_SOURCE,
        PW_KEY_NODE_NAME, NODE_NAME_DEFAULT,
        PW_KEY_NODE_DESCRIPTION, NODE_DESC_DEFAULT,
        PW_KEY_NODE_VIRTUAL, "true",
        PW_KEY_AUDIO_RATE, "48000",
        PW_KEY_AUDIO_CHANNELS, "1",
        PW_KEY_AUDIO_FORMAT, "F32LE",
        PW_KEY_NODE_LATENCY, "480/48000",
        PW_KEY_NODE_PAUSE_ON_IDLE, "true",
        NULL
    );

    if (!props) {
        return -1;
    }

    ctx->stream = pw_stream_new(ctx->core, NODE_DESC_DEFAULT, props);
    if (!ctx->stream) {
        return -1;
    }

    pw_stream_add_listener(ctx->stream, &ctx->stream_listener, &stream_events, ctx);

    uint8_t buffer[1024];
    struct spa_pod_builder b = SPA_POD_BUILDER_INIT(buffer, sizeof(buffer));

    struct spa_audio_info_raw info = SPA_AUDIO_INFO_RAW_INIT(
        .format = SPA_AUDIO_FORMAT_F32_LE,
        .rate = SAMPLE_RATE_HZ,
        .channels = AUDIO_CHANNELS,
        .position = { SPA_AUDIO_CHANNEL_MONO }
    );

    const struct spa_pod *params[1];
    params[0] = spa_format_audio_raw_build(&b, SPA_PARAM_EnumFormat, &info);

    int res = pw_stream_connect(
        ctx->stream,
        PW_DIRECTION_OUTPUT,
        PW_ID_ANY,
        PW_STREAM_FLAG_MAP_BUFFERS | PW_STREAM_FLAG_RT_PROCESS,
        params, 1
    );

    if (res < 0) {
        fprintf(stderr, "[pipewire_helper] Failed to connect stream: %s\n", spa_strerror(res));
        pw_stream_destroy(ctx->stream);
        ctx->stream = NULL;
        return res;
    }

    /* 2. Initialize Physical Microphone Capture Stream */
    struct pw_properties *cap_props = pw_properties_new(
        PW_KEY_MEDIA_TYPE, "Audio",
        PW_KEY_MEDIA_CATEGORY, "Capture",
        PW_KEY_MEDIA_ROLE, "Communication",
        PW_KEY_NODE_NAME, "realtime-noise-capture",
        PW_KEY_NODE_DESCRIPTION, "Realtime Noise Physical Capture",
        PW_KEY_AUDIO_RATE, "48000",
        PW_KEY_AUDIO_CHANNELS, "1",
        PW_KEY_AUDIO_FORMAT, "F32LE",
        PW_KEY_NODE_LATENCY, "480/48000",
        PW_KEY_NODE_PAUSE_ON_IDLE, "true",
        PW_KEY_NODE_AUTOCONNECT, "true",
        PW_KEY_NODE_DONT_RECONNECT, "true",
        NULL
    );

    /* Bind to target physical microphone if specified or detected */
    uint32_t target_id = ctx->target_device_id;
    if (ctx->shared_state) {
        uint32_t state_target = atomic_load_explicit(&ctx->shared_state->target_node_id, memory_order_relaxed);
        if (state_target > 0) {
            target_id = state_target;
        }
    }

    /*
     * target.object must carry the stable node.name: WirePlumber reads a numeric
     * value as object.serial, which never equals a node id, so the stream would
     * fall back to the default source (this virtual mic) and only silence came out.
     */
    char target_name[TARGET_NAME_MAX];
    bool have_target = target_resolve(&ctx->known_sources, target_id,
                                      target_id == 0 ? ctx->target_device_name : NULL,
                                      target_name, sizeof(target_name));
    if (!have_target && target_id > 0) {
        fprintf(stderr, "[pipewire_helper] Warning: node id %u is not a known Audio/Source; "
                        "falling back to the auto-detected microphone\n", target_id);
        if (ctx->target_device_name[0] != '\0') {
            snprintf(target_name, sizeof(target_name), "%s", ctx->target_device_name);
            have_target = true;
        }
    }
    if (have_target) {
        pw_properties_set(cap_props, PW_KEY_TARGET_OBJECT, target_name);
        fprintf(stderr, "[pipewire_helper] Capture stream targeting physical microphone: %s\n", target_name);
    }

    ctx->capture_stream = pw_stream_new(ctx->core, "Realtime Noise Physical Capture", cap_props);
    if (ctx->capture_stream) {
        pw_stream_add_listener(ctx->capture_stream, &ctx->capture_listener, &capture_stream_events, ctx);
        int cap_res = pw_stream_connect(
            ctx->capture_stream,
            PW_DIRECTION_INPUT,
            PW_ID_ANY,
            PW_STREAM_FLAG_MAP_BUFFERS | PW_STREAM_FLAG_RT_PROCESS | PW_STREAM_FLAG_AUTOCONNECT,
            params, 1
        );
        if (cap_res < 0) {
            fprintf(stderr, "[pipewire_helper] Warning: Failed to connect capture stream: %s\n", spa_strerror(cap_res));
            pw_stream_destroy(ctx->capture_stream);
            ctx->capture_stream = NULL;
        }
    }

    atomic_store_explicit(&ctx->running, true, memory_order_release);
    return 0;
}

void pipewire_helper_stop(pipewire_helper_context_t *ctx) {
    if (!ctx) return;
    atomic_store_explicit(&ctx->running, false, memory_order_release);
    if (ctx->capture_stream) {
        pw_stream_disconnect(ctx->capture_stream);
        pw_stream_destroy(ctx->capture_stream);
        ctx->capture_stream = NULL;
    }
    if (ctx->stream) {
        pw_stream_disconnect(ctx->stream);
        pw_stream_destroy(ctx->stream);
        ctx->stream = NULL;
    }
}

void pipewire_helper_destroy(pipewire_helper_context_t *ctx) {
    if (!ctx) return;
    pipewire_helper_stop(ctx);
    if (ctx->bt_release_timer && ctx->loop) {
        pw_loop_destroy_source(pw_main_loop_get_loop(ctx->loop), ctx->bt_release_timer);
        ctx->bt_release_timer = NULL;
    }
    if (ctx->profile_timer_source && ctx->loop) {
        pw_loop_destroy_source(pw_main_loop_get_loop(ctx->loop), ctx->profile_timer_source);
        ctx->profile_timer_source = NULL;
    }
    if (ctx->profile_signal_source && ctx->loop) {
        pw_loop_destroy_source(pw_main_loop_get_loop(ctx->loop), ctx->profile_signal_source);
        ctx->profile_signal_source = NULL;
    }
    if (ctx->registry) {
        spa_hook_remove(&ctx->core_listener);
        pw_proxy_destroy((struct pw_proxy *)ctx->registry);
        ctx->registry = NULL;
    }
    spa_hook_remove(&ctx->core_events_listener);
    if (ctx->core) {
        pw_core_disconnect(ctx->core);
        ctx->core = NULL;
    }
    if (ctx->context) {
        pw_context_destroy(ctx->context);
        ctx->context = NULL;
    }
    if (ctx->loop) {
        pw_main_loop_destroy(ctx->loop);
        ctx->loop = NULL;
    }
    transport_bridge_free(&ctx->transport);
    neural_filter_free(ctx);
    if (ctx->shared_state) {
        munmap(ctx->shared_state, sizeof(clearcore_shared_state_t));
        ctx->shared_state = NULL;
    }
    if (ctx->shared_state_fd >= 0) {
        close(ctx->shared_state_fd);
        ctx->shared_state_fd = -1;
    }
}

int main(int argc, char *argv[]) {
    pw_init(NULL, NULL);

    int lock_res = acquire_instance_lock();
    if (lock_res == -EBUSY) {
        fprintf(stderr, "[pipewire_helper] Device contention: %s already active (UnavailableBusy)\n", NODE_NAME_DEFAULT);
        pw_deinit();
        return 2; /* 2 indicates UnavailableBusy */
    } else if (lock_res < 0) {
        fprintf(stderr, "[pipewire_helper] Failed to acquire lock: %s\n", strerror(-lock_res));
        pw_deinit();
        return 1;
    }

    pipewire_helper_context_t ctx;
    g_ctx = &ctx;

    if (pipewire_helper_init(&ctx) != 0) {
        fprintf(stderr, "[pipewire_helper] Failed to initialize PipeWire context\n");
        release_instance_lock();
        pw_deinit();
        return 1;
    }

    /* Parse command line arguments */
    for (int i = 1; i < argc; ++i) {
        if ((strcmp(argv[i], "--target") == 0 || strcmp(argv[i], "-t") == 0) && i + 1 < argc) {
            /* --target accepts a node.name (preferred) or, for compatibility, a node id */
            const char *spec = argv[++i];
            if (target_spec_is_numeric(spec)) {
                ctx.target_device_id = (uint32_t)strtoul(spec, NULL, 10);
            } else {
                ctx.target_device_id = 0;
                snprintf(ctx.target_device_name, sizeof(ctx.target_device_name), "%s", spec);
            }
        } else if (strcmp(argv[i], "--mode") == 0 && i + 1 < argc) {
            const char *m = argv[++i];
            uint32_t mode_val = CLEARCORE_MODE_ACTIVE;
            if (strcmp(m, "bypass") == 0) mode_val = CLEARCORE_MODE_BYPASS;
            else if (strcmp(m, "mute") == 0) mode_val = CLEARCORE_MODE_MUTE;
            if (ctx.shared_state) {
                atomic_store_explicit(&ctx.shared_state->mode, mode_val, memory_order_relaxed);
            }
        }
    }

    /* Environment variable fallback for physical microphone */
    const char *env_mic = getenv("CLEARCORE_PHYSICAL_MIC");
    ctx.target_device_id = target_env_mic_id(ctx.target_device_id, ctx.target_device_name, env_mic);

    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);
    signal(SIGHUP, signal_handler);

    if (pipewire_helper_start(&ctx) != 0) {
        fprintf(stderr, "[pipewire_helper] Failed to start PipeWire virtual source\n");
        pipewire_helper_destroy(&ctx);
        release_instance_lock();
        pw_deinit();
        return 1;
    }

    fprintf(stderr, "[pipewire_helper] Running event loop for '%s'...\n", NODE_NAME_DEFAULT);
    pw_main_loop_run(ctx.loop);

    fprintf(stderr, "[pipewire_helper] Shutting down cleanly...\n");
    pipewire_helper_destroy(&ctx);
    release_instance_lock();
    pw_deinit();

    return 0;
}
