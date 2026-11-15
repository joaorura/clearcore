#ifndef CONSUMER_CLASSIFIER_H
#define CONSUMER_CLASSIFIER_H

/*
 * Which links reading from the virtual microphone count as a consumer (someone recording).
 *
 * A link only keeps the physical capture and the neural model running when neither the link nor
 * the node reading from it marks itself as monitoring or passive. The props that matter are not
 * in the registry "global" events (impl-link.c and impl-node.c only publish a fixed key list
 * there), so the helper binds the link and its input node and feeds the info props here.
 *
 * Passive or monitoring markers (PipeWire 1.6, pipewire/keys.h):
 *   link.passive=true        the link does not make the graph runnable (impl-link.c sets it
 *                            itself when either side is passive);
 *   node.passive=true|in     the node wants passive links on its input ports (impl-node.c);
 *   stream.monitor=true      monitoring stream; pipewire-pulse sets it on record streams created
 *                            with PA_STREAM_PEAK_DETECT, i.e. volume meters such as pavucontrol
 *                            (module-protocol-pulse/pulse-server.c, do_create_record_stream:
 *                            "if (peak_detect) pw_properties_set(props, PW_KEY_STREAM_MONITOR,
 *                            "true")"), together with node.passive=true when the client asks for
 *                            PA_STREAM_DONT_INHIBIT_AUTO_SUSPEND;
 *   stream.capture.sink=true the stream records a sink monitor, not a microphone;
 *   media.category=Monitor   monitoring client.
 *
 * Pure functions on a spa_dict (header-only SPA, no libpipewire call): unit tested in
 * tests/test_consumer_classifier.c.
 */

#include <stdbool.h>
#include <string.h>
#include <strings.h>

#include <spa/utils/dict.h>

/* Classification of one side (link or node) of a consumer, or of the whole consumer. */
typedef enum consumer_class {
    CONSUMER_UNKNOWN = 0, /* props not received yet: not counted until they arrive */
    CONSUMER_ACTIVE = 1,  /* records from the virtual mic: keeps the capture running */
    CONSUMER_PASSIVE = 2, /* monitor / peak meter / passive link: ignored */
} consumer_class_t;

static inline bool consumer_prop_is_true(const char *value) {
    return value != NULL && (strcasecmp(value, "true") == 0 || strcmp(value, "1") == 0);
}

/* Props of the link itself (link info). */
static inline consumer_class_t consumer_classify_link_props(const struct spa_dict *props) {
    if (props == NULL) {
        return CONSUMER_ACTIVE;
    }
    return consumer_prop_is_true(spa_dict_lookup(props, "link.passive")) ? CONSUMER_PASSIVE
                                                                           : CONSUMER_ACTIVE;
}

/* Props of the node on the input side of the link (node info, or a global dict). */
static inline consumer_class_t consumer_classify_node_props(const struct spa_dict *props) {
    if (props == NULL) {
        return CONSUMER_ACTIVE;
    }
    const char *passive = spa_dict_lookup(props, "node.passive");
    if (passive != NULL && (consumer_prop_is_true(passive) || strcmp(passive, "in") == 0)) {
        return CONSUMER_PASSIVE;
    }
    if (consumer_prop_is_true(spa_dict_lookup(props, "stream.monitor")) ||
        consumer_prop_is_true(spa_dict_lookup(props, "stream.capture.sink"))) {
        return CONSUMER_PASSIVE;
    }
    const char *category = spa_dict_lookup(props, "media.category");
    if (category != NULL && strcasecmp(category, "Monitor") == 0) {
        return CONSUMER_PASSIVE;
    }
    return CONSUMER_ACTIVE;
}

/* Both sides: passive as soon as either side is passive, active only when both are known. */
static inline consumer_class_t consumer_combine(consumer_class_t link, consumer_class_t node) {
    if (link == CONSUMER_PASSIVE || node == CONSUMER_PASSIVE) {
        return CONSUMER_PASSIVE;
    }
    if (link == CONSUMER_ACTIVE && node == CONSUMER_ACTIVE) {
        return CONSUMER_ACTIVE;
    }
    return CONSUMER_UNKNOWN;
}

#endif /* CONSUMER_CLASSIFIER_H */
