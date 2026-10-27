# 0168 Smoothness and resource audit (2026-10-27)

Status: PARTIAL. Stable `900369fb` and the QML shell are measured completely; `origin/main` (`cfefb63c`) is
**inconclusive** (see "Main arm"). Owner time budget (2 h) ended before a clean main re-run could get the shared desktop lock.

## Method
- Tools (new): `tools/daily-acceptance/animation_audit.py` (wraps `spikes/animation-pacing.py`) and
  `animation_report.py`. Per surface open and close, 3 repeats: wp/WAYLAND_DEBUG commit cadence at 60 Hz (p50/p99,
  dropped frames = intervals above 1.5 vsync, longest gap, first-frame latency), per-thread CPU of the shell
  process from `/proc/<pid>/task/*/stat` at 50 ms sampling, idle CPU over 15 s, idle `Pss_Anon`/`Pss_Shmem`/DRM, and a
  separate motion pass (back-to-back grim frames every 25 ms at scale 0.25; "slides" = mid-progress frames toward
  the final image; format `mid frames / visible ms`; 0 / 0 means the surface appears without moving).
- Host: desktop TEST machine, private labwc > niri 2880x1800 scale 1.5, 60 Hz parent, CPU renderer (`--cpu`),
  owner presentation record + wallpaper from the laptop, nacre-helper from the B0167 build, QML from the B0114 Nacre
  archive. Apps/icons were NOT copied from the laptop (private HOME uses the desktop's /usr/share; the dropdowns use
  fixtures), so icon-heavy views are not owner-real. One run per build.
- Builds: release builds in ubuntu:24.04 on prod (`cargo build -p nacre-shell --release --locked -j4`).
- Not done: the audit was not added to `acceptance.py` itself (it needs the pacing harness's WAYLAND_DEBUG topology);
  the QML arm used the same harness. Presentation-feedback stamps are not reported: stable has no `UI_PRESENTED`
  trace and the client-trace fallback mixes surfaces, so commit cadence is the pacing metric.

## Results: stable vs QML (per surface, median of 3)
| surface/phase | build | slides (mid frames / visible ms) | commits | commit interval p50/p99 ms | dropped frames (60 Hz) | longest gap ms | first ms | CPU ms (top thread) |
|---|---|---|---|---|---|---|---|---|
| dashboard/open | stable | 8 / 200 | 27 | 15.8/31.7 | 1 | 32 | 15 | 110 (nacre-shell) |
| dashboard/open | qml | 9 / 226 | 24 | 16.0/28.9 | 1 | 29 | 16 | 50 (quickshell) |
| dashboard/close | stable | 9 / 226 | 25 | 16.0/32.1 | 1 | 32 | 10 | 50 (nacre-shell) |
| dashboard/close | qml | 8 / 200 | 23 | 16.0/25.6 | 1 | 26 | 16 | 40 (quickshell) |
| control-centre/open | stable | 10 / 251 | 20 | 16.1/23.6 | 0 | 24 | 13 | 90 (nacre-shell) |
| control-centre/open | qml | 9 / 226 | 18 | 16.0/32.0 | 1 | 32 | 27 | 50 (quickshell) |
| control-centre/close | stable | 7 / 176 | 19 | 15.7/30.4 | 1 | 30 | 11 | 60 (nacre-shell) |
| control-centre/close | qml | 6 / 151 | 18 | 16.0/20.4 | 0 | 20 | 11 | 40 (quickshell) |
| sidebar/open | stable | 9 / 226 | 27 | 16.1/28.1 | 1 | 28 | 13 | 0 () |
| sidebar/open | qml | 9 / 225 | 25 | 16.0/29.9 | 1 | 30 | 10 | 30 (quickshell) |
| sidebar/close | stable | 9 / 226 | 27 | 16.1/28.9 | 1 | 29 | 9 | 10 (nacre-shell) |
| sidebar/close | qml | 9 / 226 | 27 | 16.0/17.2 | 0 | 17 | 10 | 30 (quickshell) |
| launcher/open | stable | 10 / 251 | 20 | 16.1/33.3 | 1 | 33 | 14 | 50 () |
| launcher/open | qml | 9 / 225 | 24 | 16.0/134.4 | 8 | 134 | 14 | 20 (quickshell) |
| launcher/close | stable | 8 / 201 | 20 | 15.8/31.2 | 1 | 31 | 10 | 40 (nacre-shell) |
| launcher/close | qml | 5 / 126 | 21 | 16.0/17.8 | 0 | 18 | 11 | 20 (QSGRenderThread) |
| popout-network/open | stable | 5 / 126 | 16 | 16.0/18.2 | 0 | 18 | 13 | 30 () |
| popout-network/open | qml | 5 / 126 | 16 | 16.0/20.1 | 0 | 20 | 13 | 40 (quickshell) |
| popout-network/close | stable | 6 / 151 | 17 | 16.0/30.2 | 1 | 30 | 10 | 20 (nacre-shell) |
| popout-network/close | qml | 5 / 126 | 15 | 16.0/17.8 | 0 | 18 | 10 | 10 (quickshell) |
| popout-audio/open | stable | 5 / 125 | 18 | 16.0/18.2 | 0 | 18 | 62 | 60 (nacre-shell) |
| popout-audio/open | qml | 6 / 150 | 17 | 16.0/19.8 | 0 | 20 | 15 | 40 (quickshell) |
| popout-audio/close | stable | 4 / 101 | 17 | 15.9/32.3 | 1 | 32 | 10 | 40 (nacre-shell) |
| popout-audio/close | qml | 5 / 126 | 15 | 16.0/17.6 | 0 | 18 | 10 | 10 (quickshell) |
| popout-bluetooth/open | stable | 5 / 126 | 17 | 16.0/17.9 | 0 | 18 | 14 | 20 (nacre-shell) |
| popout-bluetooth/open | qml | 5 / 126 | 15 | 16.0/17.9 | 0 | 18 | 12 | 10 (quickshell) |
| popout-bluetooth/close | stable | 6 / 151 | 17 | 16.0/30.0 | 1 | 30 | 10 | 30 (nacre-shell) |
| popout-bluetooth/close | qml | 6 / 151 | 15 | 16.0/17.8 | 0 | 18 | 9 | 20 (quickshell) |
| popout-battery/open | stable | 7 / 176 | 17 | 16.1/31.4 | 1 | 31 | 12 | 20 (nacre-link) |
| popout-battery/open | qml | 5 / 126 | 15 | 16.0/17.9 | 0 | 18 | 11 | 40 (quickshell) |
| popout-battery/close | stable | 6 / 151 | 17 | 16.1/18.4 | 0 | 18 | 10 | 20 (nacre-shell) |
| popout-battery/close | qml | 5 / 126 | 15 | 16.1/17.7 | 0 | 18 | 10 | 20 (quickshell) |
| popout-calendar/open | stable | 6 / 151 | 17 | 16.0/17.6 | 0 | 18 | 12 | 20 (nacre-link) |
| popout-calendar/open | qml | 6 / 151 | 15 | 16.0/17.3 | 0 | 17 | 19 | 30 (quickshell) |
| popout-calendar/close | stable | 6 / 151 | 17 | 15.9/18.0 | 0 | 18 | 10 | 20 (nacre-shell) |
| popout-calendar/close | qml | 5 / 126 | 15 | 16.0/17.7 | 0 | 18 | 10 | 20 (quickshell) |
| menu-power/open | stable | 10 / 251 | 19 | 16.2/33.0 | 3 | 33 | 14 | 210 (nacre-shell) |
| menu-power/open | qml | 9 / 226 | 24 | 16.0/156.1 | 10 | 156 | 13 | 40 (QSGRenderThread) |
| menu-power/close | stable | 8 / 201 | 21 | 15.9/31.5 | 1 | 32 | 10 | 90 (nacre-shell) |
| menu-power/close | qml | 5 / 126 | 21 | 16.0/30.6 | 1 | 31 | 15 | 10 () |
| notification/open | stable | 0 / 0 | 2 | 24.2/24.2 | 0 | 24 | 8 | 40 (nacre-shell) |
| notification/open | qml | 5 / 0 | 29 | 16.0/32.5 | 1 | 32 | 8 | 70 (quickshell) |
| notification/close | stable | 0 / 0 | 2 | -/- | 0 | - | 9 | 0 () |
| notification/close | qml | 0 / 0 | - | -/- | - | - | - | 10 (quickshell) |
| wallpaper-carousel/open | stable | 0 / 0 | - | -/- | - | - | - | 10 (nacre-shell) |
| wallpaper-carousel/open | qml | 0 / 0 | - | -/- | - | - | - | 10 () |
| wallpaper-carousel/close | stable | 7 / 176 | 22 | 16.1/18.6 | 0 | 19 | 10 | 30 (nacre-shell) |
| wallpaper-carousel/close | qml | 7 / 176 | 21 | 16.1/27.0 | 1 | 27 | 11 | 30 (quickshell) |
| osd/open | stable | 0 / 0 | 3 | 52.3/52.3 | 2 | 52 | 160 | 0 (nacre-shell) |
| osd/open | qml | 3 / 75 | 9 | 16.0/16.2 | 0 | 16 | 50 | 10 (quickshell) |
| osd/close | stable | 0 / 0 | 2 | 16.8/16.8 | 0 | 17 | 9 | 10 (nacre-link) |
| osd/close | qml | 0 / 0 | 4 | 16.0/16.0 | 0 | 16 | 8 | 10 (quickshell) |

| build | idle CPU % | idle Pss_Anon MiB | idle Pss_Shmem MiB | idle DRM MiB | SwapPss KiB | idle top thread ms/15s |
|---|---|---|---|---|---|---|
| stable | 0.0 | 27.19 | 0.69 | 0.00 | 0 | {} |
| qml | 0.13 | 374.18 | 0.09 | 303.38 | 0 | {'quickshell': 20.0} |


Stable total shell CPU for the 78 windows: 3.4 s; QML 2.1 s (QML time is the quickshell process only).

## Main arm (`cfefb63c`): inconclusive
- First attempts died: the desktop's /tmp is a 7.3 GiB tmpfs shared by all workers, and WAYLAND_DEBUG logs of the
  main arm reached 2.4 GiB per run; the kernel OOM killer ended the capture scope four times (journal:
  `bead-capture-*.scope: The kernel OOM killer killed some processes`). Split runs finished only for
  dashboard/control-centre/sidebar/launcher; the other chunks did not run (shared-lock queue).
- Those four surfaces gave **invalid-looking data**: the grim frames show only the wallpaper (no bar, no panel), panels
  recorded 0 intermediate frames, several windows recorded no commits, and the shell thread burned about 1.4-1.6 s CPU in
  each 1.4 s window (about 100 percent of a core; stable: 0.05-0.11 s). Idle: 0.07 percent CPU, Pss_Anon 31.2 MiB
  (stable 27.2 MiB, +4 MiB) with a 10 ms `bead-wallpaper-` thread blip.
- Cannot yet tell harness mismatch under OOM pressure from a real main regression (bar and panels not visible,
  100 percent CPU burn). It must be re-run first, on a quiet desktop with /tmp headroom (keep per-run WAYLAND_DEBUG
  logs below about 1 GiB: 4 surfaces per run), with the same command as `run-arm.sh`.

## Ranked problems and gaps
1. **Main: unverified possible regression** (no bar/panel in frames, about 100 percent CPU during animations, 0 slides in
   dashboard/control-centre/sidebar/launcher). Suspect area: the panel/layer changes since stable, e.g. `e8b764ec`
   (unmap closed panels), `be031320` (reuse stable content for panel reveals), `e190ec56`/`44277f68`/`b10e0955`
   (panel sizing and anchors), `e875cbb4` (CPU snapshot work). Needs the re-run above before any claim.
2. **Notification toast and OSD do not slide** on stable (0 intermediate frames, 2-3 commits); QML OSD and toast have
   3-5 intermediate frames. OSD first frame 160 ms (QML 50 ms). Area: toast/OSD reveal in the notices and osd views.
3. **Wallpaper picker (carousel) open** produced no commits on stable and QML in this driver (the open step is key
   presses on an already-open launcher); its animation is NOT covered. Close slides (176 ms stable).
4. **CPU per animation above QML**: stable 20-110 ms CPU per panel open (QML 20-50 ms); dashboard/open 110 vs 50,
   menu-power/open 210 vs 40, close 90 vs 10, launcher 50 vs 20. CPU renderer cost, no GPU; area: panel
   rasterisation during reveal ticks (see `be031320`, `ec6cd311` notes in 0162).
5. **Frame pacing**: stable commits at 16 ms p50 but p99 28-33 ms (one dropped frame per window) on dashboard, launcher,
   sidebar, menu-power; QML p99 17-30 ms with the same single drop; QML launcher/open and menu-power/open each have one
   134-156 ms stall that stable lacks. Stable popouts are within 2 ms of QML.
6. **popout-audio first frame 62 ms** on stable vs 15 ms QML (lazy audio view construction on first open).
7. **Memory**: stable idle Pss_Anon 27.2 MiB vs QML 374 MiB (plus 303 MiB DRM); idle CPU 0.0 vs 0.13 percent. Bead is far
   lighter; main adds about 4 MiB (unverified, one sample).

## Cleanup
Remote dirs, containers and the copied poster were removed (see report). No temporary duplicates introduced; the
audit tools replace nothing.
