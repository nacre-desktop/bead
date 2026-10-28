# 0170 Live panel slides (2026-10-28)

Status: PARTIAL. The slide clock rule is fixed and measured in a nested 120 Hz laptop-mode session; the live
GL panel path (the laptop promotes panels from CPU to GL after the first GL panel opens) is not reproduced by the
nested harness and is the open item.

## Cause
- Panel reveals started their clock at the open **request** (`start_panel` admission, B0024 `002364b3`) and
  scene motions at the first draw **attempt**. Worker preparation (`CpuResourcesPending` retries), configure and slow
  first frames then spent the curve before anything was on screen. The old `rebase_fresh` only rescued a motion
  whose very first draw committed and took more than 4 ms; it never applied to panel reveals (their clock began at
  admission, not at a draw) or to draws that did not commit. Live dashboard first content frames are 153-191 ms
  (journal), so the user saw the motion from its middle.
- Overlay views (notification toast, OSD) targeted `1` on their first render with no prior state, so they started
  fully open: the toast never slid (B0168 ranked problem 2).

## Fix (one rule, in `bead_core::animation::Animations`)
- `open_to(id, open, ...)` drives every closed (0) .. open (1) motion: a never-seen motion starts closed, and a
  motion it starts is **held** at its first pose. `committed(id, drawn, now)` is called after every committed frame
  of a surface showing a motion; the first commit after a request starts the clocks of all held motions in that
  set, so each slide plays its whole curve from the first presented frame. A motion replaced before it was drawn
  (one dropdown switching to another) starts with the next commit and finishes normally.
- Used by the panel admission and content-ready paths (`panel.rs`), the draw path (`rendering.rs`) and the
  overlay (`overlay.rs`). Removed: `Animations::rebase_fresh`, the >4 ms special case, the separate "seed 0" blocks
  in `rendering.rs`/`panel.rs`.
- Journal marker, one per finished slide: `PANEL_SLIDE name=<motion id> dir=open|close first_frame_ms=..
  frames=.. duration_ms=.. max_gap_ms=..`. `first_frame_ms` counts from the motion's (last) start request to its first
  committed frame; `frames` are committed frames from first frame to final pose. `PANEL_OPEN_LATENCY` keeps the
  request-to-first-presented number.
- Memory: none (a few floats per motion id).

## Evidence (desktop TEST host, private labwc headless 2880x1800@120 Hz > niri scale 1.5, default renderer flags as
live: `--bar --core-views --ui-provider --notices`, no `--cpu`; pacing harness `animation_audit.py`, 3 repeats)
| surface | PANEL_SLIDE open (frames / duration ms / max gap ms) | close |
|---|---|---|
| dashboard | 29-30 / 378-383 / 45-89 | 38-40 / 361-366 / 19 |
| launcher | 39-40 / 309-314 / 11-12 | 40 / 302-307 / 11-13 |
| network dropdown (status-popout) | 26-31 / 233-236 / 15-52 | 32 / 234 / 14-16 |
| audio dropdown (sound) | 31 / 240 / 31-32 | 30 / 236-238 / 15 |
| control centre | 24-36 / 288 / 18-108 | 37 / 282-284 / 13 |
| power menu and calendar (`popout`, `status-popout`) | 30-31 / 233-236 / 12-19 | 32-33 / 234-239 / 14-17 |
| updates dropdown | 27-31 / 232-233 / 15-51 | 32 / 234-235 / 11-15 |
| notification toast | 20-25 / 163-206 / 9 (base: 2-3 commits, no slide) | 20 / 162 / 9 |
All listed opens and closes have >= 20 committed frames at 120 Hz (acceptance: >= 6). The base arm (39ec77be)
also commits 26-40 frames per slide in this nested CPU session (PANEL_OPEN_LATENCY dashboard 20-112 ms), i.e. the
nested CPU path does not reproduce the live "appears" symptom; the clock rule removes the request-time dependence
that grows with live first-frame latency (153-191 ms) and the toast now slides.
Idle CPU after settle: 0.0-0.13 % (15 s windows; base 0.0-0.13 %). 0 protocol errors in shell/daemon logs.
A first candidate kept stale held motions (a dropdown replaced before it drew) active, so control centre, updates
and audio committed 140-176 frames over 1.1-1.4 s per window; releasing every held motion on the next commit
(final code) brings them back to 31-37 frames, equal to base.

## Open
- Live renderer: after the wallpaper picker, the laptop logs `RENDER_POLICY companion=true transition=cpu-to-gl` and
  panels draw through femtovg GL; the nested harness never promotes (no `RENDER_POLICY` lines), so the GL slide path
  is unverified. Verify on the laptop with the `PANEL_SLIDE` lines (frames >= 6 per open) before and after opening
  the wallpaper picker.
- Dashboard first content frame stays 20-112 ms nested / 153-191 ms live (target < 60 ms); not changed here.
- Dashboard and launcher opens have one 45-89 ms gap (first heavy frame after progress 0).
