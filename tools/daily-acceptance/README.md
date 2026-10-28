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
uses the host Wayland socket. Applications, icons, and private Nacre state are
staged into a mode-0700 private HOME/XDG tree. By default it is temporary and
removed on exit. For a normal-day cache prime followed by fresh shell/daemon
measurement, set `BEAD_DAILY_HOME_ROOT` to a task-private directory; config,
cache, data, state, and HOME persist between runs while compositor/runtime
sockets and every shell/daemon process are recreated. `BEAD_DAILY_RUNTIME_ROOT`
can select a short ephemeral runtime path when the task directory would make
UNIX socket paths too long. The wallpaper directory stays at the stable
task-private `--data/wallpapers` path and is selected through that arm's private
`config/nacre/cli.json`; this preserves wallpaper file identity across runs
without using host user data. Run artifacts and failure screenshots are
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

When `--tools` is supplied, the runner points `NACRE_HELPER_BINARY` at the
matching `--daemon` executable so private `cli-bridge.sh` calls use that same
task-local helper rather than expecting an installed helper under the temporary
HOME. It sets the daemon/shell `PATH` to the task-local binary directory plus
`/usr/bin:/bin`, ensuring asynchronous helper children launched by name resolve
deterministically. Each evidence arm records HOME, XDG roots, PATH, and helper
paths in `acceptance-environment.json`.
`wallpaper-library-identities.json` records the stable staged input paths and
file identity (size, mtime, inode, device) used by display-raster cache keys.
The wallpaper-live arm also snapshots the private wallpaper selection-generation
file per cycle; this distinguishes a queued RPC ACK from a child selection that
actually ran. Selection-to-presentation samples use the first presentation
feedback per output revision, so a repaint of one wallpaper is not counted as a
second selection. It reports per-cycle counts/p95/max and gates the combined
three-cycle p95, with at least five distinct presented revisions in each cycle.

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
task-owned directory. The runner seeds the arm's XDG cache from that directory
and saves the resulting cache on exit. Use this with `BEAD_DAILY_HOME_ROOT` to
prime matching preview/palette keys and active wallpaper state, then measure
with a fresh shell and daemon against the same private roots. Omit the snapshot
and use a new HOME root (or the default temporary HOME) for a fully cold-cache
arm. `--wallpaper-live` always runs three selection cycles and a two-minute idle
sample, even when `--quick` is supplied. It samples process and per-thread
`/proc/<pid>/stat` CPU ticks through three seconds after close, with 20 ms samples
around the two-second deadline (the kernel clock is 100 Hz), plus `Pss_Anon` at
2.5 seconds. It reports the last nonzero sample interval and one-second rolling
windows so sub-10 ms tick resolution and any residual tick are explicit. Each
cycle also captures a five-second pre-open thread baseline and the shell log
after close to correlate late link or wallpaper events with CPU ticks. A
post-two-second thread tick is reported raw and compared by TID/name with that
cycle's baseline; only ticks beyond the observed baseline count as continuing
picker work.

Use `--wallpaper-open-only --latency-limit-ms 300` with a fresh HOME root and no
cache snapshot to measure one fully cold picker first frame without selections,
the general dropdown suite, image-diff conversion, or the two-minute idle. It
opens with the actual Super+W virtual key chord, captures the first presented
frame, closes with Escape, and records the closed layer inventory.
`--wallpaper-live` remains the warm-cache selection and post-close CPU gate.
