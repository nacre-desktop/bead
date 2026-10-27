# Bead daily acceptance

This directory contains the private, nested-compositor acceptance runner. Run
`./tools/daily-acceptance/run.sh` on the desktop test host. The runner
requires `niri`, `labwc`, `wlr-randr`, `grim`, `wtype`, Python
3, ImageMagick (`compare`, `identify`), `flock`, `dbus-run-session`, `systemd-run`, a C
compiler, `pkg-config`, `wayland-client`, and `wayland-scanner`.

Use the default `target/release/nacre-shell`, `/usr/bin/nacre-helper`, and
`~/nacre-lead/b0162-daily-acceptance/staged-data` paths, or pass `--bead`, `--daemon`,
and `--data` explicitly. Pass `--shell` as the project root containing `shell/`
when using an arm build. The daemon executable must be the current Nacre
`nacre-helper` multi-call binary; the runner launches its `daemon` command with
the private `NACRE_DAEMON_SOCKET` environment. The control executable must be
`nacrectl` (a symlink to that same multi-call binary). The staged-data directory must contain `wallpapers/`,
`applications/`, `icons/`, and `state/nacre/presentation.json`. The one-command
wrapper takes the desktop heavy lock for the full acceptance arm; it never
uses the host Wayland socket. Data is copied to a mode-0700
temporary HOME (`Pictures/Wallpapers`, `XDG_DATA_HOME/applications`,
`XDG_DATA_HOME/icons`, and private Nacre state) and removed on exit. Run artifacts and failure screenshots are
written to the selected private evidence directory. The wrapper caps the run
with a user systemd scope (`MemoryMax=12G`, no swap) and holds the desktop heavy
lock for the arm.

`--quick` runs one full interaction round without the two-minute idle or final
repeat. A normal run uses three rounds, the two-minute idle, and one final round.
The scope uses Bead's existing `spikes/capture_safety.py` guard for its memory
rail, cgroup containment, and watchdog. The runner also gates host swap-in,
process `SwapPss`, full-window screenshots, and the shell's `Pss_Anon` delta.

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

Closed-state screenshot comparisons mask only the changing clock digits in the
profile's `volatile_rectangles`; neighboring status icons and the dropdown area
remain pixel-checked. The standard profile also checks the battery gap/re-arm
pointer path.
