# Bead daily acceptance

This directory contains the private, nested-compositor acceptance runner. Run
`./tools/daily-acceptance/run.sh` on the desktop test host. The runner
requires `niri`, `labwc`, `wlr-randr`, `grim`, `wtype`, Python
3, ImageMagick (`compare`), `flock`, `dbus-run-session`, `systemd-run`, a C
compiler, `pkg-config`, `wayland-client`, and `wayland-scanner`.

Use the default `target/release/nacre-shell`, `/usr/bin/nacre-helper`, and
`~/nacre-lead/daily-acceptance/staged-data` paths, or pass `--bead`, `--daemon`,
and `--data` explicitly. The staged-data directory must contain `wallpapers/`,
`applications/`, `icons/`, and `state/nacre/presentation.json`. The one-command
wrapper takes the desktop heavy lock for the full acceptance arm; it never
uses the host Wayland socket. Data is copied to a mode-0700
temporary HOME (`Pictures/Wallpapers`, `XDG_DATA_HOME/applications`,
`XDG_DATA_HOME/icons`, and private Nacre state) and removed on exit. Run artifacts and failure screenshots are
written to the selected private evidence directory. The wrapper caps the run
with a user systemd scope (`MemoryMax=12G`, no swap) and holds the desktop heavy
lock for the arm.

Input is sent to the *parent* Labwc compositor through the `wlr-virtual-pointer`
protocol, so the host pointer is never moved. Keyboard input uses `wtype` on the
nested niri display, following the established input path in
`spikes/measure.py`. `pointer.c` is compiled at runtime against the host's
Wayland development packages; no generated protocol sources are vendored.

The acceptance profile is JSON because pixel coordinates depend on the staged
user's own bar layout. `default-profile.json` provides a standard layout. Its
`[x, y]` points are raw parent-output pixels in a verified 2880×1800 Labwc
headless output (the child niri output is separately 2880×1800 physical at
scale 1.5). An optional JSON override supports custom widget placement. The
runner rejects missing or out-of-bounds points before input begins.
