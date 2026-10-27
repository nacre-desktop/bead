/* SPDX-License-Identifier: GPL-3.0-only */
#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <wayland-client.h>
#include <linux/input-event-codes.h>
#include "wlr-virtual-pointer-unstable-v1-client-protocol.h"

struct globals {
    struct wl_seat *seat;
    struct wl_output *output;
    struct zwlr_virtual_pointer_manager_v1 *manager;
    uint32_t manager_version;
};

static void global_add(void *data, struct wl_registry *registry, uint32_t name,
                       const char *interface, uint32_t version) {
    struct globals *g = data;
    if (!strcmp(interface, wl_seat_interface.name) && !g->seat)
        g->seat = wl_registry_bind(registry, name, &wl_seat_interface, version < 7 ? version : 7);
    else if (!strcmp(interface, wl_output_interface.name) && !g->output)
        g->output = wl_registry_bind(registry, name, &wl_output_interface, version < 4 ? version : 4);
    else if (!strcmp(interface, zwlr_virtual_pointer_manager_v1_interface.name) && !g->manager) {
        g->manager_version = version < 2 ? version : 2;
        g->manager = wl_registry_bind(registry, name,
            &zwlr_virtual_pointer_manager_v1_interface, g->manager_version);
    }
}

static void global_remove(void *data, struct wl_registry *registry, uint32_t name) {
    (void)data; (void)registry; (void)name;
}

static uint32_t now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint32_t)(ts.tv_sec * 1000u + ts.tv_nsec / 1000000u);
}

static void die(const char *message) {
    fprintf(stderr, "daily-acceptance-pointer: %s\n", message);
    exit(2);
}

static long parse(const char *text) {
    errno = 0;
    char *end = NULL;
    long value = strtol(text, &end, 10);
    if (errno || !end || *end || value < 0 || value > UINT32_MAX)
        die("coordinates and output extents must be nonnegative integers");
    return value;
}

int main(int argc, char **argv) {
    if (argc != 3) die("usage: pointer WIDTH HEIGHT; then write 'move X Y', 'click X Y', or 'quit'");
    long width = parse(argv[1]), height = parse(argv[2]);
    if (!width || !height) die("output extents must be positive");
    struct wl_display *display = wl_display_connect(NULL);
    if (!display) die("cannot connect to WAYLAND_DISPLAY");
    struct globals g = {0};
    struct wl_registry *registry = wl_display_get_registry(display);
    static const struct wl_registry_listener listener = { global_add, global_remove };
    wl_registry_add_listener(registry, &listener, &g);
    if (wl_display_roundtrip(display) < 0) die("registry roundtrip failed");
    if (!g.manager) die("parent compositor does not expose zwlr_virtual_pointer_manager_v1");
    struct zwlr_virtual_pointer_v1 *pointer = g.manager_version >= 2
        ? zwlr_virtual_pointer_manager_v1_create_virtual_pointer_with_output(g.manager, g.seat, g.output)
        : zwlr_virtual_pointer_manager_v1_create_virtual_pointer(g.manager, g.seat);
    if (!pointer) die("could not create virtual pointer");
    printf("POINTER_READY\n");
    fflush(stdout);
    char line[256];
    while (fgets(line, sizeof(line), stdin)) {
        char op[16], extra;
        long x, y;
        if (sscanf(line, "%15s %ld %ld %c", op, &x, &y, &extra) == 1 && !strcmp(op, "quit")) break;
        if (sscanf(line, "%15s %ld %ld %c", op, &x, &y, &extra) != 3 ||
            (strcmp(op, "move") && strcmp(op, "click") && strcmp(op, "down") && strcmp(op, "up")))
            die("malformed pointer command");
        if (x < 0 || y < 0 || x >= width || y >= height) die("point outside parent output");
        uint32_t t = now_ms();
        zwlr_virtual_pointer_v1_motion_absolute(pointer, t, (uint32_t)x, (uint32_t)y,
                                                (uint32_t)width, (uint32_t)height);
        zwlr_virtual_pointer_v1_frame(pointer);
        if (!strcmp(op, "click") || !strcmp(op, "down") || !strcmp(op, "up")) {
            if (wl_display_roundtrip(display) < 0) die("motion roundtrip failed");
            t = now_ms();
            if (!strcmp(op, "click") || !strcmp(op, "down")) {
                zwlr_virtual_pointer_v1_button(pointer, t, BTN_LEFT, WL_POINTER_BUTTON_STATE_PRESSED);
                zwlr_virtual_pointer_v1_frame(pointer);
            }
            if (!strcmp(op, "click") || !strcmp(op, "up")) {
                t = now_ms();
                zwlr_virtual_pointer_v1_button(pointer, t, BTN_LEFT, WL_POINTER_BUTTON_STATE_RELEASED);
            }
            zwlr_virtual_pointer_v1_frame(pointer);
        }
        if (wl_display_roundtrip(display) < 0) die("input roundtrip failed");
        puts("POINTER_ACK");
        fflush(stdout);
    }
    zwlr_virtual_pointer_v1_destroy(pointer);
    if (g.manager) zwlr_virtual_pointer_manager_v1_destroy(g.manager);
    if (g.output) wl_output_destroy(g.output);
    if (g.seat) wl_seat_destroy(g.seat);
    wl_registry_destroy(registry);
    wl_display_disconnect(display);
    return 0;
}
