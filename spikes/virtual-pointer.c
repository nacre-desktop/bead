// SPDX-License-Identifier: GPL-3.0-only
// B0067 parity harness: move (and optionally press) a virtual pointer on a PRIVATE parent compositor.
//   virtual-pointer X Y W H [press|click]     positions are in the parent output's pixels, W H its size
// It connects to $WAYLAND_DISPLAY (the harness points it at its own headless labwc, never the live session),
// creates a wlr virtual pointer, sends one absolute motion and, with `press`, holds the left button until killed.
// Build (needs wayland-scanner and wlr-protocols):
//   P=/usr/share/wlr-protocols/unstable/wlr-virtual-pointer-unstable-v1.xml
//   wayland-scanner client-header $P wlr-vp.h && wayland-scanner private-code $P wlr-vp.c
//   cc -o virtual-pointer virtual-pointer.c wlr-vp.c -lwayland-client
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <linux/input-event-codes.h>
#include <wayland-client.h>
#include "wlr-vp.h"

static volatile sig_atomic_t done;
static void stop(int sig) { (void)sig; done = 1; }
static struct zwlr_virtual_pointer_manager_v1 *manager;
static struct wl_seat *seat;

static void global(void *data, struct wl_registry *registry, uint32_t name, const char *interface, uint32_t version) {
    (void)data;
    if (!strcmp(interface, zwlr_virtual_pointer_manager_v1_interface.name))
        manager = wl_registry_bind(registry, name, &zwlr_virtual_pointer_manager_v1_interface, 1);
    else if (!strcmp(interface, "wl_seat") && !seat)
        seat = wl_registry_bind(registry, name, &wl_seat_interface, 1);
}
static void global_remove(void *data, struct wl_registry *registry, uint32_t name) { (void)data; (void)registry; (void)name; }
static const struct wl_registry_listener listener = {global, global_remove};

int main(int argc, char **argv) {
    if (argc < 5) { fprintf(stderr, "usage: %s X Y W H [press|click]\n", argv[0]); return 2; }
    struct wl_display *display = wl_display_connect(NULL);
    if (!display) { fprintf(stderr, "no wayland display\n"); return 1; }
    struct wl_registry *registry = wl_display_get_registry(display);
    wl_registry_add_listener(registry, &listener, NULL);
    wl_display_roundtrip(display);
    if (!manager || !seat) { fprintf(stderr, "no virtual pointer manager\n"); return 1; }
    struct zwlr_virtual_pointer_v1 *pointer = zwlr_virtual_pointer_manager_v1_create_virtual_pointer(manager, seat);
    unsigned x = (unsigned)atoi(argv[1]), y = (unsigned)atoi(argv[2]), w = (unsigned)atoi(argv[3]), h = (unsigned)atoi(argv[4]);
    zwlr_virtual_pointer_v1_motion_absolute(pointer, 0, x, y, w, h);
    zwlr_virtual_pointer_v1_frame(pointer);
    wl_display_roundtrip(display);
    if (argc > 5) {
        zwlr_virtual_pointer_v1_button(pointer, 1, BTN_LEFT, WL_POINTER_BUTTON_STATE_PRESSED);
        zwlr_virtual_pointer_v1_frame(pointer);
        wl_display_roundtrip(display);
        if (!strcmp(argv[5], "click")) {
            usleep(60000);
            zwlr_virtual_pointer_v1_button(pointer, 2, BTN_LEFT, WL_POINTER_BUTTON_STATE_RELEASED);
            zwlr_virtual_pointer_v1_frame(pointer);
            wl_display_roundtrip(display);
        } else {
            // `press`: hold the button until SIGTERM, then release it.
            signal(SIGTERM, stop);
            while (!done) usleep(20000);
            zwlr_virtual_pointer_v1_button(pointer, 3, BTN_LEFT, WL_POINTER_BUTTON_STATE_RELEASED);
            zwlr_virtual_pointer_v1_frame(pointer);
            wl_display_roundtrip(display);
        }
    } else {
        // Keep the virtual pointer alive a moment so the compositor delivers the motion.
        usleep(150000);
    }
    return 0;
}
