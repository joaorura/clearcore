#define _GNU_SOURCE
#include "pipewire_helper.h"

#include <stdio.h>
#include <stdlib.h>
#include <signal.h>
#include <unistd.h>
#include <fcntl.h>
#include <sys/file.h>
#include <errno.h>

static pipewire_helper_context_t *g_ctx = NULL;
static int g_lock_fd = -1;

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
        /* Valid frame received from engine: sanitize against IEEE 754 clipping */
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
            break;
        case PW_STREAM_STATE_PAUSED:
            ctx->node_id = pw_stream_get_node_id(ctx->stream);
            atomic_store_explicit(&ctx->node_ready, true, memory_order_release);
            fprintf(stderr, "[pipewire_helper] Stream paused (Node ID: %u)\n", ctx->node_id);
            break;
        case PW_STREAM_STATE_UNCONNECTED:
            atomic_store_explicit(&ctx->node_ready, false, memory_order_release);
            fprintf(stderr, "[pipewire_helper] Stream unconnected\n");
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

    g_lock_fd = open(lock_path, O_CREAT | O_RDWR, 0600);
    if (g_lock_fd < 0) {
        return -errno;
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

int pipewire_helper_init(pipewire_helper_context_t *ctx) {
    if (!ctx) return -1;
    memset(ctx, 0, sizeof(*ctx));

    atomic_init(&ctx->running, false);
    atomic_init(&ctx->node_ready, false);
    atomic_init(&ctx->process_count, 0);
    atomic_init(&ctx->alloc_violations, 0);
    atomic_init(&ctx->blocking_violations, 0);

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

    return 0;
}

int pipewire_helper_start(pipewire_helper_context_t *ctx) {
    if (!ctx || !ctx->core) return -1;

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
        PW_KEY_NODE_ALWAYS_PROCESS, "true",
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

    atomic_store_explicit(&ctx->running, true, memory_order_release);
    return 0;
}

void pipewire_helper_stop(pipewire_helper_context_t *ctx) {
    if (!ctx) return;
    atomic_store_explicit(&ctx->running, false, memory_order_release);
    if (ctx->stream) {
        pw_stream_disconnect(ctx->stream);
        pw_stream_destroy(ctx->stream);
        ctx->stream = NULL;
    }
}

void pipewire_helper_destroy(pipewire_helper_context_t *ctx) {
    if (!ctx) return;
    pipewire_helper_stop(ctx);
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
}

int main(int argc, char *argv[]) {
    (void)argc;
    (void)argv;

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
