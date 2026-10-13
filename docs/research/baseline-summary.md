# Blank layer-shell baseline

Measured 2026-10-13; three complete 60-second samples per candidate.
The final campaign supersedes interrupted, short and incompletely isolated trials.

| Candidate | First-run PSS / USS MiB | Later PSS MiB (runs 2 / 3) | Median PSS / USS MiB | Idle CPU (one core) |
| --- | ---: | ---: | ---: | ---: |
| tiny-skia | 1.86 / 1.81 | 1.90 / 1.88 | 1.88 / 1.83 | 0.000% |
| vello | 177.48 / 173.04 | 32.05 / 32.39 | 32.39 / 27.98 | 0.000% |
| femtovg | 26.79 / 22.36 | 19.80 / 19.88 | 19.88 / 15.46 | 0.000% |
| qml | 59.51 / 47.31 | 71.85 / 59.57 | 59.57 / 47.31 | 0.000% |

All samples recorded zero CPU ticks and zero SwapPss. At CLK_TCK=100, one tick
over 60 s is about 0.0167% of one core: zero is below that measurement resolution.
PSS excludes unmapped GPU memory. The raw JSON includes deduplicated driver
resident accounting; do not add it to PSS or call it total GPU RAM.

Qt used OpenGL; Rust GPU modes used Vulkan on Intel ARL/Mesa 26.2.4. One nested
niri output is 1278×692 logical pixels at scale 1, 60 Hz; each client surface is
512×32 with no exclusive zone or keyboard focus. No full Nacre UI/services loaded.

Vello first-run memory is a concern: 177.48 MiB PSS versus 32–33 MiB later.
Shader-cache/allocator behavior is not fully attributed. OS page cache and all
driver cache overrides were not controlled; first-run is not synonymous with cold. QML run 2 is also higher
(71.85 MiB PSS / 65.12 MiB USS) than the other two; the cause is not established.
All three rows are retained, so the median must not hide these costs.

## Startup markers (different boundaries; no speedup claim)

| Candidate | First marker ms | Later markers ms | Boundary |
| --- | ---: | ---: | --- |
| tiny-skia | 25.6 | 16.6 / 15.8 | wl_surface frame callback; not presentation timestamp |
| vello | 675.6 | 77.3 / 159.9 | wl_surface frame callback; not presentation timestamp |
| femtovg | 79.7 | 57.2 / 44.5 | wl_surface frame callback; not presentation timestamp |
| qml | 79.5 | 70.1 / 74.4 | QML component completion |

These are launch-to-log-observation monotonic wall times. They include scheduling
and pipe delivery, and are not input-to-photon or common-boundary startup times.
Hot reload and animated frame pacing are N/A: neither is implemented by the spike.

## Reproduce in the prepared checkout and inspect

From the Bead folder: `./spikes/run.sh`. Default is three runs / 60 s per client,
roughly 13–15 minutes plus compilation. Look at printed PSS/USS and idle CPU, then
spikes/results/baseline.json. Runs are offscreen; nothing replaces daily Nacre.
Screenshots/raw logs go to ignored spikes/results/raw/. The script terminates
only its own clients/compositors; Ctrl+C stops the foreground measurement.

The existing task checkout has isolated Rust 1.99.0/rustfmt/clippy and extracted
labwc 0.20.2 + wlroots 0.20.2 + libsfdo 0.1.4 in `.tools/`. No pacman packages
were installed and no system/shell configuration changed. Cargo downloaded locked
dependencies. Other checkouts need Rust, Wayland development libraries/pkg-config,
niri 26.04, quickshell 0.3.2, grim, wayland-info, dbus-run-session, Python and the
isolated labwc parent setup. See the runner for exact environment/lifecycle.
The tool setup is task-local and is not distributed as a release. For a fresh
checkout, follow [the isolated setup recipe](spike-setup.md).

## Verdict

**Concern / bounded proceed.** CPU blank cost is promising. GPU first-run costs,
Qt variability, text/effects, animation bandwidth and full service-rich parity
remain unproven. The next authorized action is to review ADR 0001; Feature 1
requires the owner’s OK.
