#ifndef CLEARCORE_STATE_H
#define CLEARCORE_STATE_H

/*
 * Layout of the control file shared between the Electron app (writer) and the PipeWire helper
 * (reader): $XDG_RUNTIME_DIR/clearcore_state, mapped MAP_SHARED by the helper.
 *
 * This header deliberately includes nothing from PipeWire so the layout and the preset decoding
 * can be tested without the PipeWire development files.
 *
 * Byte layout (16 bytes, little-endian, never changes size):
 *
 *   offset  size  field           written by
 *   0       4     mode            Electron (and `pipewire_helper --mode`)
 *   4       4     target_node_id  Electron
 *   8       4     generation      Electron (unused by the helper)
 *   12      4     preset          Electron (studio finishing preset, see CLEARCORE_PRESET_*)
 *
 * History: bytes 8..15 used to be a single `_Atomic uint64_t generation`. The Electron app only ever
 * wrote 32 bits at offset 8 and the helper never read the field, so bytes 12..15 were always zero.
 * `preset` reuses them. Compatibility:
 *   - old helper + new Electron: the helper ignores offset 12; `mode` is untouched (the preset is
 *     NOT packed into `mode`, because an old helper compares `mode == MUTE` and would leak audio);
 *   - new helper + old Electron or an older file: offset 12 is zero, which is `Off`;
 *   - the file never grows, so a writer that rewrites 16 bytes cannot shrink the helper's mapping.
 * Writers must update only the 4-byte field they own (pwrite at its offset), never rewrite all 16
 * bytes: another process may be changing `mode` (for example Mute) at the same moment.
 * A `preset` outside CLEARCORE_PRESET_OFF..CLEARCORE_PRESET_BROADCAST is ignored by the helper.
 *
 * Degraded mode: the studio chain runs inside libclearcore_filter.so (StudioBackend). When that
 * library is missing, fails to load, or a frame returns an error, the helper falls back to
 * noise_suppressor.c, which does NOT apply any studio preset. The preset in this file is then
 * simply not used.
 */

#include <errno.h>
#include <fcntl.h>
#include <stdatomic.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <unistd.h>

#if !defined(__BYTE_ORDER__) || __BYTE_ORDER__ != __ORDER_LITTLE_ENDIAN__
#error "clearcore_state.h: the shared state file is little-endian; this host is not"
#endif

/* ClearCore Operating Modes */
#define CLEARCORE_MODE_ACTIVE   0
#define CLEARCORE_MODE_BYPASS   1
#define CLEARCORE_MODE_MUTE     2
#define CLEARCORE_STATE_FILE    "clearcore_state"
#define CLEARCORE_STATE_SIZE    16

/* Studio finishing presets (mirror studio_dsp::Preset::as_u8 and clearcore_filter_set_preset) */
#define CLEARCORE_PRESET_OFF        0
#define CLEARCORE_PRESET_NATURAL    1
#define CLEARCORE_PRESET_PODCAST    2
#define CLEARCORE_PRESET_BROADCAST  3
#define CLEARCORE_PRESET_INVALID    (-1)

/**
 * Shared memory / control state for mode, physical mic binding and studio preset.
 */
typedef struct clearcore_shared_state {
    _Atomic uint32_t mode;             /* 0=Active, 1=Bypass, 2=Mute */
    _Atomic uint32_t target_node_id;   /* Physical microphone node ID (0=auto) */
    _Atomic uint32_t generation;       /* Engine generation counter (unused by the helper) */
    _Atomic uint32_t preset;           /* 0=Off, 1=Natural, 2=Podcast, 3=Broadcast */
} clearcore_shared_state_t;

_Static_assert(sizeof(clearcore_shared_state_t) == CLEARCORE_STATE_SIZE,
               "clearcore_state layout must stay 16 bytes");
_Static_assert(offsetof(clearcore_shared_state_t, mode) == 0, "mode must be at offset 0");
_Static_assert(offsetof(clearcore_shared_state_t, target_node_id) == 4,
               "target_node_id must be at offset 4");
_Static_assert(offsetof(clearcore_shared_state_t, generation) == 8,
               "generation must be at offset 8");
_Static_assert(offsetof(clearcore_shared_state_t, preset) == 12, "preset must be at offset 12");

/**
 * Opens (creating it if needed) the shared state file at `path`, read/write, close-on-exec.
 *
 * The file may live in a world-writable directory (`/tmp` when XDG_RUNTIME_DIR is unset), so a
 * hostile user could pre-plant a symlink there and make the helper `ftruncate` the victim's file.
 * The file is created `0600` (owner only; the umask can only tighten it): only this user's helper
 * and UI ever touch it. Hence O_NOFOLLOW, and a check on the open descriptor (no TOCTOU) that it is a regular file owned
 * by the effective user. Returns the fd, or -1 with errno set (ELOOP for a symlink, EPERM/EINVAL
 * for a foreign or non-regular file).
 */
static inline int clearcore_state_open(const char *path) {
    int fd = open(path, O_RDWR | O_CREAT | O_NOFOLLOW | O_CLOEXEC, 0600);
    if (fd < 0) {
        return -1;
    }
    struct stat st;
    if (fstat(fd, &st) != 0) {
        int saved = errno;
        close(fd);
        errno = saved;
        return -1;
    }
    if (!S_ISREG(st.st_mode)) {
        close(fd);
        errno = EINVAL;
        return -1;
    }
    if (st.st_uid != geteuid()) {
        close(fd);
        errno = EPERM;
        return -1;
    }
    return fd;
}

/** Returns `raw` as a preset in 0..=3, or CLEARCORE_PRESET_INVALID when it is out of range. */
static inline int clearcore_preset_decode(uint32_t raw) {
    return raw <= CLEARCORE_PRESET_BROADCAST ? (int)raw : CLEARCORE_PRESET_INVALID;
}

/**
 * Decides what the helper must do about the preset written in the state file.
 * Returns the preset to apply, or CLEARCORE_PRESET_INVALID when nothing must change (the stored
 * value is invalid or equals the preset already applied to the filter).
 */
static inline int clearcore_preset_to_apply(uint32_t raw, int applied) {
    int wanted = clearcore_preset_decode(raw);
    if (wanted == CLEARCORE_PRESET_INVALID || wanted == applied) {
        return CLEARCORE_PRESET_INVALID;
    }
    return wanted;
}

/** Signature of `clearcore_filter_set_preset` in libclearcore_filter.so (0 on success). */
typedef int (*clearcore_set_preset_fn_t)(void *filter, uint8_t preset);

/**
 * Called once per hop from the realtime callback, in Active mode only. Reads the preset from the
 * shared state (one relaxed atomic load, no syscall) and calls `set_fn` only when it differs from
 * `*applied`. `*applied` is updated only when `set_fn` reports success, so a failed call is retried
 * on the next hop. Does nothing if the library has no `set_fn` (NULL) or there is no filter/state.
 */
static inline void clearcore_state_sync_preset(const clearcore_shared_state_t *state, void *filter,
                                               clearcore_set_preset_fn_t set_fn, int *applied) {
    if (!state || !filter || !set_fn || !applied) {
        return;
    }
    int wanted = clearcore_preset_to_apply(
        atomic_load_explicit(&state->preset, memory_order_relaxed), *applied);
    if (wanted == CLEARCORE_PRESET_INVALID) {
        return;
    }
    if (set_fn(filter, (uint8_t)wanted) == 0) {
        *applied = wanted;
    }
}

#endif /* CLEARCORE_STATE_H */
