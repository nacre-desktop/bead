// SPDX-License-Identifier: GPL-3.0-only
// Harness-only: one xdg toplevel with a solid colour and a bar that moves twice a second, drawn through wl_shm.
// Build: see spikes/b0090-previews.py (wayland-scanner client-header/private-code on xdg-shell.xml).
// Usage: shm-window TITLE APP_ID RRGGBB
#define _GNU_SOURCE
#include <poll.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
#include <wayland-client.h>
#include "xdg-shell-client-protocol.h"

enum { W = 640, H = 400 };
static struct wl_compositor *compositor;
static struct wl_shm *shm;
static struct xdg_wm_base *wm;
static int closing, configured;

static void ping(void *d, struct xdg_wm_base *b, uint32_t s) { xdg_wm_base_pong(b, s); }
static const struct xdg_wm_base_listener wm_l = {ping};
static void global(void *d, struct wl_registry *r, uint32_t n, const char *i, uint32_t v) {
	if (!strcmp(i, "wl_compositor")) compositor = wl_registry_bind(r, n, &wl_compositor_interface, 4);
	else if (!strcmp(i, "wl_shm")) shm = wl_registry_bind(r, n, &wl_shm_interface, 1);
	else if (!strcmp(i, "xdg_wm_base")) { wm = wl_registry_bind(r, n, &xdg_wm_base_interface, 1); xdg_wm_base_add_listener(wm, &wm_l, 0); }
}
static void global_remove(void *d, struct wl_registry *r, uint32_t n) {}
static const struct wl_registry_listener reg_l = {global, global_remove};
static void sconf(void *d, struct xdg_surface *s, uint32_t serial) { xdg_surface_ack_configure(s, serial); configured = 1; }
static const struct xdg_surface_listener surf_l = {sconf};
static void tconf(void *d, struct xdg_toplevel *t, int32_t w, int32_t h, struct wl_array *s) {}
static void tclose(void *d, struct xdg_toplevel *t) { closing = 1; }
static const struct xdg_toplevel_listener top_l = {tconf, tclose};

int main(int argc, char **argv) {
	if (argc < 4) return 2;
	uint32_t colour = strtoul(argv[3], 0, 16);
	struct wl_display *dpy = wl_display_connect(0);
	if (!dpy) return 1;
	wl_registry_add_listener(wl_display_get_registry(dpy), &reg_l, 0);
	wl_display_roundtrip(dpy);
	if (!compositor || !shm || !wm) return 1;
	struct wl_surface *surface = wl_compositor_create_surface(compositor);
	struct xdg_surface *xs = xdg_wm_base_get_xdg_surface(wm, surface);
	xdg_surface_add_listener(xs, &surf_l, 0);
	struct xdg_toplevel *top = xdg_surface_get_toplevel(xs);
	xdg_toplevel_add_listener(top, &top_l, 0);
	xdg_toplevel_set_title(top, argv[1]);
	xdg_toplevel_set_app_id(top, argv[2]);
	wl_surface_commit(surface);
	while (!configured) wl_display_dispatch(dpy);
	int fd = memfd_create("shm-window", 0);
	ftruncate(fd, W * H * 4);
	uint32_t *px = mmap(0, W * H * 4, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
	struct wl_shm_pool *pool = wl_shm_create_pool(shm, fd, W * H * 4);
	struct wl_buffer *buf = wl_shm_pool_create_buffer(pool, 0, W, H, W * 4, WL_SHM_FORMAT_XRGB8888);
	struct timespec next;
	clock_gettime(CLOCK_MONOTONIC, &next);
	for (int tick = 0; !closing; tick++) {
		int bar = (tick * 40) % (W - 80);
		for (int y = 0; y < H; y++)
			for (int x = 0; x < W; x++) px[y * W + x] = (x >= bar && x < bar + 80 && y > H / 3 && y < 2 * H / 3) ? 0xffffff : colour;
		wl_surface_attach(surface, buf, 0, 0);
		wl_surface_damage_buffer(surface, 0, 0, W, H);
		wl_surface_commit(surface);
		wl_display_flush(dpy);
		next.tv_nsec += 500000000;
		if (next.tv_nsec >= 1000000000) { next.tv_nsec -= 1000000000; next.tv_sec++; }
		for (;;) {
			struct timespec now;
			clock_gettime(CLOCK_MONOTONIC, &now);
			long ms = (next.tv_sec - now.tv_sec) * 1000 + (next.tv_nsec - now.tv_nsec) / 1000000;
			if (ms <= 0 || closing) break;
			while (wl_display_prepare_read(dpy) != 0) wl_display_dispatch_pending(dpy);
			wl_display_flush(dpy);
			struct pollfd p = {wl_display_get_fd(dpy), POLLIN, 0};
			if (poll(&p, 1, (int)ms) > 0) wl_display_read_events(dpy); else wl_display_cancel_read(dpy);
			if (wl_display_dispatch_pending(dpy) < 0) return 0;
		}
	}
	return 0;
}
