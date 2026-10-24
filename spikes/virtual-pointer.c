// SPDX-License-Identifier: GPL-3.0-only
// B0067 parity harness: move (and optionally press) a virtual pointer on a PRIVATE parent compositor.
//   virtual-pointer X Y W H [hover|press|click]     positions are in the parent output's pixels, W H its size
// It connects to $WAYLAND_DISPLAY (the harness points it at its own headless labwc, never the live session),
// creates a wlr virtual pointer, sends one absolute motion and, with `press`, holds the left button until killed.
// Build (needs wayland-scanner and wlr-protocols):
//   P=/usr/share/wlr-protocols/unstable/wlr-virtual-pointer-unstable-v1.xml
//   wayland-scanner client-header $P wlr-vp.h && wayland-scanner private-code $P wlr-vp.c
//   cc -o virtual-pointer virtual-pointer.c wlr-vp.c -lwayland-client -lm
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <time.h>
#include <linux/input-event-codes.h>
#include <wayland-client.h>
#include "wlr-vp.h"

static uint32_t now_ms(void) {
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (uint32_t)(now.tv_sec * 1000 + now.tv_nsec / 1000000);
}
static volatile sig_atomic_t done;
static void stop(int sig) { (void)sig; done = 1; }
static struct zwlr_virtual_pointer_manager_v1 *manager;
static struct wl_seat *seat;

static void global(void *data, struct wl_registry *registry, uint32_t name, const char *interface, uint32_t version) {
    (void)version;
    (void)data;
    if (!strcmp(interface, zwlr_virtual_pointer_manager_v1_interface.name))
        manager = wl_registry_bind(registry, name, &zwlr_virtual_pointer_manager_v1_interface, 1);
    else if (!strcmp(interface, "wl_seat") && !seat)
        seat = wl_registry_bind(registry, name, &wl_seat_interface, 1);
}
static void global_remove(void *data, struct wl_registry *registry, uint32_t name) { (void)data; (void)registry; (void)name; }
static const struct wl_registry_listener listener = {global, global_remove};

static void trace_motion(unsigned x, unsigned y) {
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    fprintf(stdout, "POINTER_SENT monotonic_ns=%llu x=%u y=%u\n",
            (unsigned long long)now.tv_sec * 1000000000ULL + now.tv_nsec, x, y);
    fflush(stdout);
}

int main(int argc, char **argv) {
    if (argc < 5) { fprintf(stderr, "usage: %s X Y W H [hover|press|click|drag X2 Y2 MS]\n", argv[0]); return 2; }
    struct wl_display *display = wl_display_connect(NULL);
    if (!display) { fprintf(stderr, "no wayland display\n"); return 1; }
    struct wl_registry *registry = wl_display_get_registry(display);
    wl_registry_add_listener(registry, &listener, NULL);
    wl_display_roundtrip(display);
    if (!manager || !seat) { fprintf(stderr, "no virtual pointer manager\n"); return 1; }
    struct zwlr_virtual_pointer_v1 *pointer = zwlr_virtual_pointer_manager_v1_create_virtual_pointer(manager, seat);
    unsigned x = (unsigned)atoi(argv[1]), y = (unsigned)atoi(argv[2]), w = (unsigned)atoi(argv[3]), h = (unsigned)atoi(argv[4]);
    trace_motion(x, y);
    zwlr_virtual_pointer_v1_motion_absolute(pointer, now_ms(), x, y, w, h);
    zwlr_virtual_pointer_v1_frame(pointer);
    wl_display_roundtrip(display);
    usleep(100000); // Let the compositor establish pointer focus before the gesture.
    if (argc > 6 && (!strcmp(argv[5], "wheel") || !strcmp(argv[5], "pixel"))) {
        int wheel = !strcmp(argv[5], "wheel");
        double delta = atof(argv[6]);
        zwlr_virtual_pointer_v1_axis_source(pointer, wheel ? WL_POINTER_AXIS_SOURCE_WHEEL : WL_POINTER_AXIS_SOURCE_FINGER);
        if (wheel)
            zwlr_virtual_pointer_v1_axis_discrete(pointer, now_ms(), WL_POINTER_AXIS_VERTICAL_SCROLL, wl_fixed_from_double(delta * 15), (int)delta);
        else
            zwlr_virtual_pointer_v1_axis(pointer, now_ms(), WL_POINTER_AXIS_VERTICAL_SCROLL, wl_fixed_from_double(delta));
        zwlr_virtual_pointer_v1_frame(pointer);
        wl_display_roundtrip(display);
        usleep(300000);
        return 0;
    }
    if (argc > 5 && !strcmp(argv[5], "hover")) {
        signal(SIGTERM, stop);
        while (!done) usleep(20000);
        return 0;
    }
    if (argc > 5) {
        zwlr_virtual_pointer_v1_button(pointer, now_ms(), BTN_LEFT, WL_POINTER_BUTTON_STATE_PRESSED);
        zwlr_virtual_pointer_v1_frame(pointer);
        wl_display_roundtrip(display);
        if (!strcmp(argv[5], "drag")) {
            if (argc != 9) { fprintf(stderr, "drag requires X2 Y2 MS\n"); return 2; }
            unsigned end_x = (unsigned)atoi(argv[6]), end_y = (unsigned)atoi(argv[7]);
            unsigned ms = (unsigned)atoi(argv[8]);
            if (!ms || ms > 10000) { fprintf(stderr, "drag duration must be 1..10000 ms\n"); return 2; }
            // 250 Hz input deliberately exceeds the display rate to expose queues.
            unsigned steps = ms / 4 + 1;
            for (unsigned i = 1; i <= steps; i++) {
                usleep(ms * 1000 / steps);
                unsigned px = (unsigned)((double)x + ((double)end_x - x) * i / steps);
                unsigned py = (unsigned)((double)y + ((double)end_y - y) * i / steps);
                trace_motion(px, py);
                zwlr_virtual_pointer_v1_motion_absolute(pointer, now_ms(), px, py, w, h);
                zwlr_virtual_pointer_v1_frame(pointer);
                wl_display_roundtrip(display);
            }
            zwlr_virtual_pointer_v1_button(pointer, now_ms(), BTN_LEFT, WL_POINTER_BUTTON_STATE_RELEASED);
            zwlr_virtual_pointer_v1_frame(pointer);
            wl_display_roundtrip(display);
        } else if (!strcmp(argv[5], "click")) {
            usleep(120000);
            zwlr_virtual_pointer_v1_button(pointer, now_ms(), BTN_LEFT, WL_POINTER_BUTTON_STATE_RELEASED);
            zwlr_virtual_pointer_v1_frame(pointer);
            wl_display_roundtrip(display);
            usleep(300000); // Keep the device alive until the release has been dispatched.
        } else {
            // `press`: hold the button until SIGTERM, then release it.
            signal(SIGTERM, stop);
            while (!done) usleep(20000);
            zwlr_virtual_pointer_v1_button(pointer, now_ms(), BTN_LEFT, WL_POINTER_BUTTON_STATE_RELEASED);
            zwlr_virtual_pointer_v1_frame(pointer);
            wl_display_roundtrip(display);
        }
    } else {
        // Keep the virtual pointer alive a moment so the compositor delivers the motion.
        usleep(150000);
    }
    return 0;
}
