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
repeat. `--quick --focus-dropdown updates` runs the three dismissal paths for
one status panel for regression bisection. A normal run uses three rounds, the
two-minute idle, and one final round.
The scope uses Bead's existing `spikes/capture_safety.py` guard for its memory
rail, cgroup containment, and watchdog. The runner also gates host swap-in,
process `SwapPss`, full-window screenshots, and the shell's `Pss_Anon` delta.
Open latency is measured from `PanelVisibility(true)` to the first Wayland
presentation-feedback callback. Legacy comparison builds receive only a
measurement overlay for this same boundary; the overlay does not alter rendering.
The nested 60 Hz TEST host uses a 65 ms limit, calibrated against stable baseline
900369fb (60.1–63.9 ms on the common marker). This is a TEST-host comparison
limit, not a claim that the physical two-frame target is 65 ms.
The runner enables `BEAD_ACCEPTANCE_PANEL_LATENCY=1`; current Bead records the
request at the layer-panel open transition and the first matching presentation
callback, while 900369fb receives the same two-point overlay at build time.
Per-widget focused arms run Escape and outside before click, so the click-to-close
path is exercised after the lazy view has been exercised once. Each method still
has its own open, latency, close, and pixel-diff assertion.

Pointer input is sent to the *parent* Labwc compositor through
`wlr-virtual-pointer`, so the host pointer is never moved. Modifier chords use
`wtype` on the parent seat to reach nested Niri; typed text and Escape use the
nested Niri display to reach the focused Bead surface. `pointer.c` is compiled
at runtime against the host's Wayland development packages; no generated
protocol sources are vendored.

The acceptance profile is JSON because pixel coordinates depend on the staged
user's own bar layout. `default-profile.json` provides a standard layout. Its
`[x, y]` points are raw parent-output pixels in a verified 2880×1800 Labwc
headless output (the child niri output is separately 2880×1800 physical at
scale 1.5). An optional JSON override supports custom widget placement. The
runner rejects missing or out-of-bounds points before input begins.

Closed-state screenshot comparisons mask only the changing clock digits in the
profile's `volatile_rectangles`; neighboring status icons and the dropdown area
remain pixel-checked. The standard profile also checks the battery gap/re-arm
pointer path. Niri may report the blank `bead-daily-acceptance` Background canvas,
the Overlay widget-drag canvas, or one duplicate Top layer beyond startup. The
harness records only those exact identities when the closed screenshot matches
pixel-for-pixel, then permits previously verified identities at idle; changed
pixels, an unknown identity, or duplicate extras fail.

`--quick --interaction[=title,switch,pill]` runs only the pointer regressions from the 2026-10-27 audit: hovering the
bar title opens the dashboard, clicking a second bar icon while a dropdown is open opens its
dropdown at once, and a pointer sweep along the status pill (`pill_sweep` = `[x0, x1, y, step]` in
the profile) shows a hover background at every step, so the pill's hit areas have no gaps.

For cache-state comparisons, set `BEAD_DAILY_CACHE_SNAPSHOT` to a private
task-owned directory. The runner seeds its fresh per-arm XDG cache from that
directory and saves the arm's resulting cache before deleting the private HOME.
The shell and daemon remain fresh on every run; omit or clear the snapshot for a
fully cold-cache arm. `--wallpaper-live` also samples the shell process's
`/proc/<pid>/stat` CPU ticks for two seconds and `Pss_Anon` at 2.5 seconds
after each picker close.
