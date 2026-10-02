#ifndef TARGET_RESOLVER_H
#define TARGET_RESOLVER_H

/*
 * Pure (PipeWire-free, header-only) helpers that decide what goes into the
 * capture stream's PW_KEY_TARGET_OBJECT.
 *
 * WirePlumber treats a numeric target.object as an object.serial, NOT a node id
 * (scripts/linking/find-defined-target.lua), so a node id must never be forwarded
 * as a number. The stable reference is node.name, which is the same on every
 * PipeWire/WirePlumber version.
 */

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define TARGET_NAME_MAX      128
#define TARGET_MAX_SOURCES   64

typedef struct known_source {
    uint32_t id;
    char name[TARGET_NAME_MAX];
} known_source_t;

typedef struct known_source_table {
    known_source_t entries[TARGET_MAX_SOURCES];
    size_t count;
} known_source_table_t;

/* True when spec is non-empty and made only of decimal digits (a node id). */
static inline bool target_spec_is_numeric(const char *spec) {
    if (!spec || spec[0] == '\0') return false;
    for (const char *p = spec; *p; ++p) {
        if (*p < '0' || *p > '9') return false;
    }
    return true;
}

/* Remember an Audio/Source node (id -> node.name). Replaces an existing id; bounded. */
static inline void known_sources_add(known_source_table_t *t, uint32_t id, const char *name) {
    if (!t || !name || name[0] == '\0') return;
    for (size_t i = 0; i < t->count; ++i) {
        if (t->entries[i].id == id) {
            snprintf(t->entries[i].name, TARGET_NAME_MAX, "%s", name);
            return;
        }
    }
    if (t->count >= TARGET_MAX_SOURCES) return;
    t->entries[t->count].id = id;
    snprintf(t->entries[t->count].name, TARGET_NAME_MAX, "%s", name);
    t->count++;
}

/*
 * Pick the node.name to use as target.object.
 *   1. explicit_name (user passed a name) wins;
 *   2. otherwise a known node id is translated to its node.name;
 *   3. otherwise nothing: returns false with out[0] == '\0', and the caller must
 *      NOT fall back to the numeric id.
 */
static inline bool target_resolve(const known_source_table_t *t, uint32_t id,
                                  const char *explicit_name, char *out, size_t out_size) {
    if (!out || out_size == 0) return false;
    out[0] = '\0';
    if (explicit_name && explicit_name[0] != '\0') {
        snprintf(out, out_size, "%s", explicit_name);
        return true;
    }
    if (id > 0 && t) {
        for (size_t i = 0; i < t->count; ++i) {
            if (t->entries[i].id == id) {
                snprintf(out, out_size, "%s", t->entries[i].name);
                return true;
            }
        }
    }
    return false;
}

/*
 * Apply the CLEARCORE_PHYSICAL_MIC fallback to the node id chosen on the command
 * line. The env var is a numeric id and is only a default: it never overrides an
 * explicit numeric --target (id != 0) nor an explicit --target NAME (name set,
 * id stays 0).
 */
static inline uint32_t target_env_mic_id(uint32_t id, const char *explicit_name,
                                         const char *env_mic) {
    if (id != 0) return id;
    if (explicit_name && explicit_name[0] != '\0') return id;
    if (!env_mic || env_mic[0] == '\0') return id;
    return (uint32_t)strtoul(env_mic, NULL, 10);
}

#endif /* TARGET_RESOLVER_H */
