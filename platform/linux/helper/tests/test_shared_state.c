#define _GNU_SOURCE
#include "clearcore_state.h"

#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <sys/stat.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>

/* Bytes exactly as the Electron app writes them: mode, targetNodeId and generation as 32-bit LE. */
static void put_u32(unsigned char *buf, size_t offset, uint32_t value) {
    buf[offset + 0] = (unsigned char)(value & 0xFF);
    buf[offset + 1] = (unsigned char)((value >> 8) & 0xFF);
    buf[offset + 2] = (unsigned char)((value >> 16) & 0xFF);
    buf[offset + 3] = (unsigned char)((value >> 24) & 0xFF);
}

static void test_layout(void) {
    printf("[TEST] test_layout\n");
    assert(sizeof(clearcore_shared_state_t) == 16);
    assert(offsetof(clearcore_shared_state_t, mode) == 0);
    assert(offsetof(clearcore_shared_state_t, target_node_id) == 4);
    assert(offsetof(clearcore_shared_state_t, generation) == 8);
    assert(offsetof(clearcore_shared_state_t, preset) == 12);
    /* Mute must keep its value: an old helper compares `mode == 2` to silence the microphone. */
    assert(CLEARCORE_MODE_ACTIVE == 0);
    assert(CLEARCORE_MODE_BYPASS == 1);
    assert(CLEARCORE_MODE_MUTE == 2);
}

static void test_old_writer_leaves_preset_off(void) {
    printf("[TEST] test_old_writer_leaves_preset_off\n");
    unsigned char file[16];
    memset(file, 0, sizeof(file));
    put_u32(file, 0, CLEARCORE_MODE_MUTE);
    put_u32(file, 4, 77);
    put_u32(file, 8, 5);

    clearcore_shared_state_t state;
    memcpy(&state, file, sizeof(state));
    assert(atomic_load(&state.mode) == CLEARCORE_MODE_MUTE);
    assert(atomic_load(&state.target_node_id) == 77);
    assert(atomic_load(&state.generation) == 5);
    assert(clearcore_preset_decode(atomic_load(&state.preset)) == CLEARCORE_PRESET_OFF);
}

static void test_old_64bit_generation_leaves_preset_off(void) {
    printf("[TEST] test_old_64bit_generation_leaves_preset_off\n");
    /* A writer that stored the old u64 generation = 5 little-endian: bytes 12..15 are zero. */
    unsigned char file[16];
    memset(file, 0, sizeof(file));
    uint64_t generation = 5;
    memcpy(file + 8, &generation, sizeof(generation));

    clearcore_shared_state_t state;
    memcpy(&state, file, sizeof(state));
    assert(atomic_load(&state.generation) == 5);
    assert(clearcore_preset_decode(atomic_load(&state.preset)) == CLEARCORE_PRESET_OFF);
}

static void test_new_writer_preset_does_not_touch_the_mode(void) {
    printf("[TEST] test_new_writer_preset_does_not_touch_the_mode\n");
    unsigned char file[16];
    memset(file, 0, sizeof(file));
    put_u32(file, 0, CLEARCORE_MODE_BYPASS);
    put_u32(file, 12, CLEARCORE_PRESET_PODCAST);

    clearcore_shared_state_t state;
    memcpy(&state, file, sizeof(state));
    assert(atomic_load(&state.mode) == CLEARCORE_MODE_BYPASS);
    assert(clearcore_preset_decode(atomic_load(&state.preset)) == CLEARCORE_PRESET_PODCAST);
}

static void test_decode_rejects_out_of_range(void) {
    printf("[TEST] test_decode_rejects_out_of_range\n");
    for (uint32_t raw = 0; raw <= 3; ++raw) {
        assert(clearcore_preset_decode(raw) == (int)raw);
    }
    assert(clearcore_preset_decode(4) == CLEARCORE_PRESET_INVALID);
    assert(clearcore_preset_decode(255) == CLEARCORE_PRESET_INVALID);
    assert(clearcore_preset_decode(0xFFFFFFFFu) == CLEARCORE_PRESET_INVALID);
}

static void test_to_apply_only_on_change(void) {
    printf("[TEST] test_to_apply_only_on_change\n");
    assert(clearcore_preset_to_apply(2, CLEARCORE_PRESET_OFF) == 2);
    assert(clearcore_preset_to_apply(2, 2) == CLEARCORE_PRESET_INVALID);
    assert(clearcore_preset_to_apply(0, 2) == 0);
    assert(clearcore_preset_to_apply(9, 2) == CLEARCORE_PRESET_INVALID);
    assert(clearcore_preset_to_apply(0, CLEARCORE_PRESET_OFF) == CLEARCORE_PRESET_INVALID);
}

static int g_set_calls = 0;
static int g_last_preset = -1;
static int g_set_result = 0;

static int fake_set_preset(void *filter, uint8_t preset) {
    (void)filter;
    g_set_calls++;
    g_last_preset = preset;
    return g_set_result;
}

static void test_sync_applies_only_on_change(void) {
    printf("[TEST] test_sync_applies_only_on_change\n");
    clearcore_shared_state_t state;
    memset(&state, 0, sizeof(state));
    int filter_handle = 0;
    int applied = CLEARCORE_PRESET_OFF;
    g_set_calls = 0;
    g_set_result = 0;

    clearcore_state_sync_preset(&state, &filter_handle, fake_set_preset, &applied);
    assert(g_set_calls == 0); /* file says Off and the filter starts Off */

    atomic_store(&state.preset, CLEARCORE_PRESET_PODCAST);
    clearcore_state_sync_preset(&state, &filter_handle, fake_set_preset, &applied);
    assert(g_set_calls == 1 && g_last_preset == CLEARCORE_PRESET_PODCAST);
    assert(applied == CLEARCORE_PRESET_PODCAST);

    clearcore_state_sync_preset(&state, &filter_handle, fake_set_preset, &applied);
    assert(g_set_calls == 1); /* unchanged: no second call */

    atomic_store(&state.preset, 9); /* garbage in the file is ignored */
    clearcore_state_sync_preset(&state, &filter_handle, fake_set_preset, &applied);
    assert(g_set_calls == 1 && applied == CLEARCORE_PRESET_PODCAST);

    atomic_store(&state.preset, CLEARCORE_PRESET_OFF);
    clearcore_state_sync_preset(&state, &filter_handle, fake_set_preset, &applied);
    assert(g_set_calls == 2 && g_last_preset == CLEARCORE_PRESET_OFF);
}

static void test_sync_retries_after_a_failed_call(void) {
    printf("[TEST] test_sync_retries_after_a_failed_call\n");
    clearcore_shared_state_t state;
    memset(&state, 0, sizeof(state));
    atomic_store(&state.preset, CLEARCORE_PRESET_BROADCAST);
    int filter_handle = 0;
    int applied = CLEARCORE_PRESET_OFF;
    g_set_calls = 0;
    g_set_result = -3;

    clearcore_state_sync_preset(&state, &filter_handle, fake_set_preset, &applied);
    assert(g_set_calls == 1 && applied == CLEARCORE_PRESET_OFF);
    g_set_result = 0;
    clearcore_state_sync_preset(&state, &filter_handle, fake_set_preset, &applied);
    assert(g_set_calls == 2 && applied == CLEARCORE_PRESET_BROADCAST);
}

static void test_sync_tolerates_a_library_without_set_preset(void) {
    printf("[TEST] test_sync_tolerates_a_library_without_set_preset\n");
    clearcore_shared_state_t state;
    memset(&state, 0, sizeof(state));
    atomic_store(&state.preset, CLEARCORE_PRESET_NATURAL);
    int filter_handle = 0;
    int applied = CLEARCORE_PRESET_OFF;

    clearcore_state_sync_preset(&state, &filter_handle, NULL, &applied);
    clearcore_state_sync_preset(NULL, &filter_handle, fake_set_preset, &applied);
    clearcore_state_sync_preset(&state, NULL, fake_set_preset, &applied);
    assert(applied == CLEARCORE_PRESET_OFF);
}

static void test_mapped_file_round_trip(void) {
    printf("[TEST] test_mapped_file_round_trip\n");
    const char *tmp_dir = getenv("TMPDIR");
    char path[512];
    snprintf(path, sizeof(path), "%s/clearcore_state_test_XXXXXX", tmp_dir ? tmp_dir : "/tmp");
    int fd = mkstemp(path);
    assert(fd >= 0);
    assert(ftruncate(fd, CLEARCORE_STATE_SIZE) == 0);

    clearcore_shared_state_t *mapped = mmap(NULL, sizeof(clearcore_shared_state_t),
                                            PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    assert(mapped != MAP_FAILED);

    /* A writer outside the mapping (as the Electron app is) changes only the preset bytes. */
    unsigned char preset_bytes[4];
    put_u32(preset_bytes, 0, CLEARCORE_PRESET_BROADCAST);
    assert(pwrite(fd, preset_bytes, sizeof(preset_bytes), 12) == (ssize_t)sizeof(preset_bytes));
    assert(atomic_load(&mapped->preset) == CLEARCORE_PRESET_BROADCAST);
    assert(atomic_load(&mapped->mode) == CLEARCORE_MODE_ACTIVE);

    munmap(mapped, sizeof(clearcore_shared_state_t));
    close(fd);
    unlink(path);
}

static void test_open_refuses_symlink_and_non_regular_files(void) {
    printf("[TEST] test_open_refuses_symlink_and_non_regular_files\n");
    const char *tmp_dir = getenv("TMPDIR");
    char dir[512];
    snprintf(dir, sizeof(dir), "%s/clearcore_open_test_XXXXXX", tmp_dir ? tmp_dir : "/tmp");
    assert(mkdtemp(dir) != NULL);

    char victim[600], link_path[600], file_path[600], dir_path[600];
    snprintf(victim, sizeof(victim), "%s/victim", dir);
    snprintf(link_path, sizeof(link_path), "%s/link", dir);
    snprintf(file_path, sizeof(file_path), "%s/state", dir);
    snprintf(dir_path, sizeof(dir_path), "%s/subdir", dir);

    /* A symlink planted at the state path must not be followed (ftruncate would hit the victim). */
    int vfd = open(victim, O_WRONLY | O_CREAT, 0600);
    assert(vfd >= 0);
    assert(write(vfd, "precious-data", 13) == 13);
    close(vfd);
    assert(symlink(victim, link_path) == 0);
    assert(clearcore_state_open(link_path) == -1);
    assert(errno == ELOOP);
    struct stat st;
    assert(stat(victim, &st) == 0 && st.st_size == 13);

    /* A directory is not a regular file. */
    assert(mkdir(dir_path, 0700) == 0);
    assert(clearcore_state_open(dir_path) == -1);

    /* A regular file of ours opens (created on demand) and is close-on-exec. */
    int fd = clearcore_state_open(file_path);
    assert(fd >= 0);
    assert((fcntl(fd, F_GETFD) & FD_CLOEXEC) != 0);
    close(fd);
    fd = clearcore_state_open(file_path);
    assert(fd >= 0);
    close(fd);

    /* Created owner-only even with a permissive umask: the file is private to this user. */
    char private_path[600];
    snprintf(private_path, sizeof(private_path), "%s/private_state", dir);
    mode_t old_umask = umask(0);
    fd = clearcore_state_open(private_path);
    umask(old_umask);
    assert(fd >= 0);
    assert(fstat(fd, &st) == 0);
    assert((st.st_mode & 0777) == 0600);
    close(fd);
    unlink(private_path);

    unlink(file_path);
    unlink(link_path);
    unlink(victim);
    rmdir(dir_path);
    rmdir(dir);
}

int main(void) {
    test_layout();
    test_old_writer_leaves_preset_off();
    test_old_64bit_generation_leaves_preset_off();
    test_new_writer_preset_does_not_touch_the_mode();
    test_decode_rejects_out_of_range();
    test_to_apply_only_on_change();
    test_sync_applies_only_on_change();
    test_sync_retries_after_a_failed_call();
    test_sync_tolerates_a_library_without_set_preset();
    test_mapped_file_round_trip();
    test_open_refuses_symlink_and_non_regular_files();
    printf("[TEST] all shared state tests passed\n");
    return 0;
}
