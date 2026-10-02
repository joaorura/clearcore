/*
 * Contract test: the physical microphone must be handed to PipeWire as a
 * stable node.name, never as a bare node id.
 *
 * Root cause covered here: pipewire_helper.c used to put "%u" of a node id into
 * PW_KEY_TARGET_OBJECT. WirePlumber (find-defined-target.lua) resolves a numeric
 * target.object against object.serial, so an id never matched and the capture
 * stream fell back to the default source (the virtual mic itself) -> silence.
 */
#define _GNU_SOURCE
#include <assert.h>
#include <stdio.h>
#include <string.h>

#include "target_resolver.h"

static void test_numeric_id_resolves_to_name(void) {
    printf("[TEST] numeric id -> node.name\n");
    known_source_table_t t;
    memset(&t, 0, sizeof(t));
    known_sources_add(&t, 67, "alsa_input.pci-0000_00_1f.3-platform-sof_sdw.HiFi__Mic__source");
    known_sources_add(&t, 143, "bluez_input.38:FB:A0:38:F7:EB");

    char out[TARGET_NAME_MAX];
    assert(target_resolve(&t, 143, NULL, out, sizeof(out)));
    assert(strcmp(out, "bluez_input.38:FB:A0:38:F7:EB") == 0);
    /* never a number */
    assert(strcmp(out, "143") != 0);
}

static void test_explicit_name_wins_over_id(void) {
    printf("[TEST] explicit name wins\n");
    known_source_table_t t;
    memset(&t, 0, sizeof(t));
    known_sources_add(&t, 67, "alsa_input.mic");

    char out[TARGET_NAME_MAX];
    assert(target_resolve(&t, 67, "bluez_input.AA:BB", out, sizeof(out)));
    assert(strcmp(out, "bluez_input.AA:BB") == 0);
}

static void test_unknown_id_is_not_forwarded_as_number(void) {
    printf("[TEST] unknown id yields no target (never a numeric string)\n");
    known_source_table_t t;
    memset(&t, 0, sizeof(t));
    known_sources_add(&t, 67, "alsa_input.mic");

    char out[TARGET_NAME_MAX] = "sentinel";
    assert(!target_resolve(&t, 999, NULL, out, sizeof(out)));
    assert(out[0] == '\0');
}

static void test_zero_id_without_name(void) {
    printf("[TEST] id 0 and no name -> auto (no target)\n");
    known_source_table_t t;
    memset(&t, 0, sizeof(t));
    char out[TARGET_NAME_MAX];
    assert(!target_resolve(&t, 0, NULL, out, sizeof(out)));
    assert(!target_resolve(&t, 0, "", out, sizeof(out)));
}

static void test_spec_parsing(void) {
    printf("[TEST] --target spec: digits are ids, everything else is a name\n");
    assert(target_spec_is_numeric("143"));
    assert(target_spec_is_numeric("0"));
    assert(!target_spec_is_numeric(""));
    assert(!target_spec_is_numeric(NULL));
    assert(!target_spec_is_numeric("bluez_input.38:FB:A0:38:F7:EB"));
    assert(!target_spec_is_numeric("12abc"));
    assert(!target_spec_is_numeric("alsa_input.pci-0000_00_1f.3"));
}

static void test_table_bounds_and_dedupe(void) {
    printf("[TEST] table is bounded and de-duplicates by id\n");
    known_source_table_t t;
    memset(&t, 0, sizeof(t));
    for (uint32_t i = 1; i <= TARGET_MAX_SOURCES + 10; ++i) {
        char name[32];
        snprintf(name, sizeof(name), "src_%u", i);
        known_sources_add(&t, i, name);
    }
    assert(t.count == TARGET_MAX_SOURCES);

    known_sources_add(&t, 1, "renamed");
    assert(t.count == TARGET_MAX_SOURCES);
    char out[TARGET_NAME_MAX];
    assert(target_resolve(&t, 1, NULL, out, sizeof(out)));
    assert(strcmp(out, "renamed") == 0);

    /* long names are truncated, never overflow */
    char longname[512];
    memset(longname, 'x', sizeof(longname) - 1);
    longname[sizeof(longname) - 1] = '\0';
    known_source_table_t t2;
    memset(&t2, 0, sizeof(t2));
    const char *volatile longname_rt = longname; /* hide the length: truncation is intended */
    known_sources_add(&t2, 5, longname_rt);
    assert(target_resolve(&t2, 5, NULL, out, sizeof(out)));
    assert(strlen(out) == TARGET_NAME_MAX - 1);
}

static void test_env_mic_never_overrides_explicit_name(void) {
    printf("[TEST] CLEARCORE_PHYSICAL_MIC does not override an explicit --target name\n");
    /* --target NOME: id stays 0 but the name is set -> the env must be ignored */
    assert(target_env_mic_id(0, "alsa_input.mic", "77") == 0);
    /* an explicit numeric --target also wins over the env */
    assert(target_env_mic_id(143, "", "77") == 143);
    /* nothing explicit: the env supplies the id */
    assert(target_env_mic_id(0, "", "77") == 77);
    assert(target_env_mic_id(0, NULL, "77") == 77);
    /* no env: unchanged */
    assert(target_env_mic_id(0, "", NULL) == 0);
    assert(target_env_mic_id(0, "", "") == 0);
}

static void test_snprintf_truncation_terminates(void) {
    printf("[TEST] copies are always NUL-terminated when truncated\n");
    char longname[512];
    memset(longname, 'y', sizeof(longname) - 1);
    longname[sizeof(longname) - 1] = '\0';
    char out[16];
    volatile size_t n = sizeof(out); /* runtime size: truncation is the point here */
    known_source_table_t t;
    memset(&t, 0, sizeof(t));
    const char *volatile longname_rt = longname;
    assert(target_resolve(&t, 0, longname_rt, out, n));
    assert(strlen(out) == n - 1);
    known_sources_add(&t, 9, longname_rt);
    assert(target_resolve(&t, 9, NULL, out, n));
    assert(strlen(out) == n - 1);
}

int main(void) {
    test_env_mic_never_overrides_explicit_name();
    test_snprintf_truncation_terminates();
    test_numeric_id_resolves_to_name();
    test_explicit_name_wins_over_id();
    test_unknown_id_is_not_forwarded_as_number();
    test_zero_id_without_name();
    test_spec_parsing();
    test_table_bounds_and_dedupe();
    printf("[TEST] all target resolver tests passed.\n");
    return 0;
}
