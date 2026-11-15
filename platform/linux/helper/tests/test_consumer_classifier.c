/* Classification of the links reading from the virtual microphone (consumer_classifier.h). */
#include <assert.h>
#include <stdio.h>

#include "consumer_classifier.h"

#define ITEMS(...) ((const struct spa_dict_item[]){__VA_ARGS__})
#define DICT(...)                                                                         \
    (&(const struct spa_dict){.flags = 0,                                                 \
                              .n_items = sizeof(ITEMS(__VA_ARGS__)) / sizeof(struct spa_dict_item), \
                              .items = ITEMS(__VA_ARGS__)})

static int failures = 0;

static void expect(int cond, const char *what) {
    if (!cond) {
        fprintf(stderr, "FAIL: %s\n", what);
        failures++;
    }
}

int main(void) {
    /* A recorder (pw-record, a browser, a VoIP app): active. */
    expect(consumer_classify_node_props(DICT(SPA_DICT_ITEM_INIT("media.class", "Stream/Input/Audio"),
                                             SPA_DICT_ITEM_INIT("media.category", "Capture"),
                                             SPA_DICT_ITEM_INIT("node.passive", "false"))) == CONSUMER_ACTIVE,
           "a regular capture stream is a consumer");
    expect(consumer_classify_link_props(DICT(SPA_DICT_ITEM_INIT("link.output.node", "40"),
                                             SPA_DICT_ITEM_INIT("link.passive", "false"))) == CONSUMER_ACTIVE,
           "a non-passive link is a consumer");

    /* pipewire-pulse peak detect (pavucontrol): stream.monitor=true (+ node.passive=true). */
    expect(consumer_classify_node_props(DICT(SPA_DICT_ITEM_INIT("stream.monitor", "true"))) == CONSUMER_PASSIVE,
           "stream.monitor=true is a monitor");
    expect(consumer_classify_node_props(DICT(SPA_DICT_ITEM_INIT("stream.monitor", "1"))) == CONSUMER_PASSIVE,
           "stream.monitor=1 is a monitor");
    expect(consumer_classify_node_props(DICT(SPA_DICT_ITEM_INIT("node.passive", "true"))) == CONSUMER_PASSIVE,
           "node.passive=true is passive");
    expect(consumer_classify_node_props(DICT(SPA_DICT_ITEM_INIT("node.passive", "in"))) == CONSUMER_PASSIVE,
           "node.passive=in is passive on the input side");
    expect(consumer_classify_node_props(DICT(SPA_DICT_ITEM_INIT("node.passive", "out"))) == CONSUMER_ACTIVE,
           "node.passive=out only concerns its output ports");
    expect(consumer_classify_node_props(DICT(SPA_DICT_ITEM_INIT("stream.capture.sink", "true"))) == CONSUMER_PASSIVE,
           "stream.capture.sink=true is a monitor");
    expect(consumer_classify_node_props(DICT(SPA_DICT_ITEM_INIT("media.category", "Monitor"))) == CONSUMER_PASSIVE,
           "media.category=Monitor is a monitor");
    expect(consumer_classify_link_props(DICT(SPA_DICT_ITEM_INIT("link.passive", "true"))) == CONSUMER_PASSIVE,
           "link.passive=true is passive");
    expect(consumer_classify_node_props(DICT(SPA_DICT_ITEM_INIT("stream.monitor", "false"))) == CONSUMER_ACTIVE,
           "stream.monitor=false is a consumer");

    /* Order of events: counted only once both sides are known; passive wins at once. */
    expect(consumer_combine(CONSUMER_UNKNOWN, CONSUMER_UNKNOWN) == CONSUMER_UNKNOWN, "nothing known yet");
    expect(consumer_combine(CONSUMER_ACTIVE, CONSUMER_UNKNOWN) == CONSUMER_UNKNOWN, "link before node props");
    expect(consumer_combine(CONSUMER_UNKNOWN, CONSUMER_ACTIVE) == CONSUMER_UNKNOWN, "node before link props");
    expect(consumer_combine(CONSUMER_ACTIVE, CONSUMER_ACTIVE) == CONSUMER_ACTIVE, "both active");
    expect(consumer_combine(CONSUMER_UNKNOWN, CONSUMER_PASSIVE) == CONSUMER_PASSIVE, "monitor node, link pending");
    expect(consumer_combine(CONSUMER_PASSIVE, CONSUMER_UNKNOWN) == CONSUMER_PASSIVE, "passive link, node pending");
    expect(consumer_combine(CONSUMER_PASSIVE, CONSUMER_ACTIVE) == CONSUMER_PASSIVE, "passive link");

    /* Missing props are not a reason to cut a recorder off. */
    expect(consumer_classify_node_props(NULL) == CONSUMER_ACTIVE, "no node props: consumer");
    expect(consumer_classify_link_props(NULL) == CONSUMER_ACTIVE, "no link props: consumer");

    if (failures) return 1;
    printf("consumer_classifier: all checks passed\n");
    return 0;
}
