// SPDX-License-Identifier: GPL-3.0-only
// B0067 parity harness: move (and optionally press) a virtual pointer on a PRIVATE parent compositor.
//   virtual-pointer X Y W H [hover|press|click|click-clock]  positions are in the parent output's pixels, W H its size
//   virtual-pointer X Y W H drag|drag-stream X2 Y2 MS
//   virtual-pointer X Y W H relative-drag X2 Y2 MS [SCALE]
//     drags hold left while moving; relative-drag sends relative deltas and optional SCALE divides travel
//   virtual-pointer X Y W H stream wheel|pixel DELTA COUNT MS   COUNT axis events, one every MS ms (B0110)
//   virtual-pointer X Y W H stream-reverse wheel|pixel DELTA COUNT MS
//     sends DELTA through the first half of the stream, then -DELTA without lifting (B0135)
//   virtual-pointer X Y W H stream-oscillate wheel|pixel DELTA COUNT MS PERIOD
//     alternates direction every PERIOD samples without lifting (active-scroll proof)
// It connects to $WAYLAND_DISPLAY (private labwc or nested niri, never the live session),
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
#include <math.h>
#include <errno.h>
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

static int flush_stream_event(struct wl_display *display, unsigned index) {
    if (wl_display_flush(display) >= 0) return 0;
    int error = errno;
    if (!error) error = wl_display_get_error(display);
    if (!error) error = EIO;
    fprintf(stderr, "stream flush failed at event %u: %s%s\n", index,
            strerror(error), error == EAGAIN ? " (Wayland socket backpressure)" : "");
    return 1;
}

int main(int argc, char **argv) {
    if (argc < 5) { fprintf(stderr, "usage: %s X Y W H [hover|press|click|click-clock|drag|drag-stream X2 Y2 MS|relative-drag X2 Y2 MS [SCALE]]\n", argv[0]); return 2; }
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
    int oscillate = argc == 11 && !strcmp(argv[5], "stream-oscillate");
    if ((argc == 10 && (!strcmp(argv[5], "stream") || !strcmp(argv[5], "stream-reverse"))) || oscillate) {
        // One virtual pointer, absolute deadlines: scroll gestures at a steady device rate (B0110).
        int reverse = !strcmp(argv[5], "stream-reverse");
        int wheel = !strcmp(argv[6], "wheel");
        double delta = atof(argv[7]);
        unsigned count = (unsigned)atoi(argv[8]), ms = (unsigned)atoi(argv[9]);
        unsigned period = oscillate ? (unsigned)atoi(argv[10]) : 0;
        if (!count || count > 5000 || !ms || ms > 2000 || (reverse && count < 2) ||
            (oscillate && (!period || period > count))) {
            fprintf(stderr, "stream needs COUNT 1..5000, MS 1..2000; reverse needs at least 2\n"); return 2;
        }
        struct timespec started;
        clock_gettime(CLOCK_MONOTONIC, &started);
        for (unsigned i = 0; i < count; i++) {
            unsigned long long ns = (unsigned long long)started.tv_nsec + (unsigned long long)ms * 1000000ULL * i;
            struct timespec deadline = {started.tv_sec + ns / 1000000000ULL, ns % 1000000000ULL};
            clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME, &deadline, NULL);
            double event_delta = reverse && i >= count / 2 ? -delta :
                                 oscillate && (i / period) % 2 ? -delta : delta;
            zwlr_virtual_pointer_v1_axis_source(pointer, wheel ? WL_POINTER_AXIS_SOURCE_WHEEL : WL_POINTER_AXIS_SOURCE_FINGER);
            if (wheel)
                zwlr_virtual_pointer_v1_axis_discrete(pointer, now_ms(), WL_POINTER_AXIS_VERTICAL_SCROLL, wl_fixed_from_double(event_delta * 15), (int)event_delta);
            else
                zwlr_virtual_pointer_v1_axis(pointer, now_ms(), WL_POINTER_AXIS_VERTICAL_SCROLL, wl_fixed_from_double(event_delta));
            zwlr_virtual_pointer_v1_frame(pointer);
            // Deadlines govern injection cadence; flush queues immediately without
            // waiting for a compositor roundtrip on every sample.
            if (flush_stream_event(display, i)) return 1;
            struct timespec sent;
            clock_gettime(CLOCK_MONOTONIC, &sent);
            fprintf(stdout, "AXIS_SENT monotonic_ns=%llu source=%s direction=%s index=%u delta=%.3f\n",
                    (unsigned long long)sent.tv_sec * 1000000000ULL + sent.tv_nsec,
                    wheel ? "wheel" : "finger", event_delta < 0 ? "up" : "down", i, event_delta);
            fflush(stdout);
        }
        if (!wheel) {  // a finger lifting ends the gesture, which is what starts a fling
            struct timespec stopped;
            clock_gettime(CLOCK_MONOTONIC, &stopped);
            fprintf(stdout, "AXIS_STOP monotonic_ns=%llu source=finger\n",
                    (unsigned long long)stopped.tv_sec * 1000000000ULL + stopped.tv_nsec);
            fflush(stdout);
            zwlr_virtual_pointer_v1_axis_stop(pointer, now_ms(), WL_POINTER_AXIS_VERTICAL_SCROLL);
            zwlr_virtual_pointer_v1_frame(pointer);
            wl_display_roundtrip(display);
        }
        usleep(300000);
        return 0;
    }
    if (argc > 6 && (!strcmp(argv[5], "wheel") || !strcmp(argv[5], "wheel-angle") || !strcmp(argv[5], "pixel"))) {
        int wheel = strncmp(argv[5], "wheel", 5) == 0;
        double delta = atof(argv[6]);
        zwlr_virtual_pointer_v1_axis_source(pointer, wheel ? WL_POINTER_AXIS_SOURCE_WHEEL : WL_POINTER_AXIS_SOURCE_FINGER);
        if (wheel)
            zwlr_virtual_pointer_v1_axis_discrete(pointer, now_ms(), WL_POINTER_AXIS_VERTICAL_SCROLL, wl_fixed_from_double(!strcmp(argv[5], "wheel-angle") ? 0 : delta * 15), (int)delta);
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
        if (!strcmp(argv[5], "drag") || !strcmp(argv[5], "drag-stream") || !strcmp(argv[5], "relative-drag")) {
            int relative = !strcmp(argv[5], "relative-drag");
            if (argc != 9 && !(relative && argc == 10)) {
                fprintf(stderr, "%s requires X2 Y2 MS%s\n", argv[5], relative ? " [SCALE]" : ""); return 2;
            }
            unsigned end_x = (unsigned)atoi(argv[6]), end_y = (unsigned)atoi(argv[7]);
            unsigned ms = (unsigned)atoi(argv[8]);
            char *scale_end = NULL;
            double scale = argc == 10 ? strtod(argv[9], &scale_end) : 1.0;
            if (!ms || ms > 10000) { fprintf(stderr, "drag duration must be 1..10000 ms\n"); return 2; }
            if (relative && (!isfinite(scale) || scale <= 0.0 || (argc == 10 && (!scale_end || *scale_end)))) {
                fprintf(stderr, "relative-drag scale must be finite and positive\n"); return 2;
            }
            // 250 Hz input deliberately exceeds the display rate to expose queues.
            unsigned steps = ms / 4 + 1;
            struct timespec started;
            clock_gettime(CLOCK_MONOTONIC, &started);
            for (unsigned i = 1; i <= steps; i++) {
                // Roundtrip time must not accumulate into a slower-than-requested
                // gesture. Absolute deadlines keep the requested pixels/second.
                unsigned long long ns = (unsigned long long)started.tv_nsec +
                    (unsigned long long)ms * 1000000ULL * i / steps;
                struct timespec deadline = {started.tv_sec + ns / 1000000000ULL,
                                            ns % 1000000000ULL};
                clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME, &deadline, NULL);
                unsigned px = (unsigned)((double)x + ((double)end_x - x) * i / steps);
                unsigned py = (unsigned)((double)y + ((double)end_y - y) * i / steps);
                trace_motion(px, py);
                if (relative) {
                    unsigned prev_x = (unsigned)((double)x + ((double)end_x - x) * (i - 1) / steps);
                    unsigned prev_y = (unsigned)((double)y + ((double)end_y - y) * (i - 1) / steps);
                    zwlr_virtual_pointer_v1_motion(pointer, now_ms(),
                        wl_fixed_from_double(((double)px - prev_x) / scale),
                        wl_fixed_from_double(((double)py - prev_y) / scale));
                } else {
                    zwlr_virtual_pointer_v1_motion_absolute(pointer, now_ms(), px, py, w, h);
                }
                zwlr_virtual_pointer_v1_frame(pointer);
                // Relative input is scheduled by the monotonic deadlines above;
                // a compositor roundtrip per step stretches it under capture load.
                if (relative || !strcmp(argv[5], "drag-stream")) wl_display_flush(display);
                else wl_display_roundtrip(display);
            }
            if (relative) {
                struct timespec released;
                clock_gettime(CLOCK_MONOTONIC, &released);
                fprintf(stdout, "POINTER_RELEASE monotonic_ns=%llu\n",
                        (unsigned long long)released.tv_sec * 1000000000ULL + released.tv_nsec);
                fflush(stdout);
            }
            zwlr_virtual_pointer_v1_button(pointer, now_ms(), BTN_LEFT, WL_POINTER_BUTTON_STATE_RELEASED);
            zwlr_virtual_pointer_v1_frame(pointer);
            wl_display_roundtrip(display);
        } else if (!strcmp(argv[5], "click") || !strcmp(argv[5], "click-clock")) {
            usleep(120000);
            int report_release = !strcmp(argv[5], "click-clock");
            struct timespec released;
            if (report_release) clock_gettime(CLOCK_MONOTONIC, &released);
            zwlr_virtual_pointer_v1_button(pointer, now_ms(), BTN_LEFT, WL_POINTER_BUTTON_STATE_RELEASED);
            zwlr_virtual_pointer_v1_frame(pointer);
            if (report_release) {
                fprintf(stdout, "POINTER_RELEASED monotonic_ns=%llu\n",
                        (unsigned long long)released.tv_sec * 1000000000ULL + released.tv_nsec);
                fflush(stdout);
            }
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
