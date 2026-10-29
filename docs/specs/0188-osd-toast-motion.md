# B0188: OSD, toast and workspace-switch motion at 120 Hz

Status: **Measured; one small fix; the first-frame and workspace findings are open.** The 2026-10-27 audit (stable)
reported no OSD/toast slide, a 160 ms OSD first frame and a 60 Hz-looking workspace indicator. Current main
(`6f6bde99`) already slides both overlays at the display rate; the remaining misses are listed below with the
evidence that bounds them. A prewarm/ready-gate/frame-callback attempt was built, measured, and **not kept**.

## Method
Desktop TEST host, private labwc (headless 2880x1800@120 Hz) > niri scale 1.5 (laptop geometry), default renderer
flags as live, release build (`cargo build -p nacre-shell --release`, ubuntu:24.04 on prod), fake system daemon.
Harness: `tools/daily-acceptance/animation_audit.py` (new anims: `osd-a/b/c`, `osd-step`, `notify-a/b/c`,
`workspace`; `PARITY_NAMED_WORKSPACES=1`, new `niri-action` step, level-change stamps `level_marks_ns` in
`pacing.json`). Numbers are overlay `WINDOW_COMMIT` times against the trigger. Each show starts hidden
(prepare sleeps outlast the 2 s OSD / 5 s toast timers). One run per arm; `UI_ERROR`: 0.

## Results on current main (baseline arm, 3 shows each)
| motion | first frame | frames | max gap | note |
|---|---|---|---|---|
| OSD show | 160-163 ms (fake-audio path, see below) | 25-26 | 8.9-10.6 ms (one 39 ms gap in 1 of 4) | 120 Hz cadence (8.5 ms p50), `PANEL_SLIDE` 201-208 ms |
| OSD hide (accelerated, B0181) | 68-71 ms after the 2 s hide trigger | 13-14 | 8.6-8.9 ms | 111 ms |
| Toast show | 4.7-5.1 ms (first commit) | 25 | 11-12 ms | slide 205 ms; B0170 holds |
| Toast hide | not triggered by the harness close step | | | |
| OSD repeated steps (60 ms apart, visible) | 5.4-7.2 ms to next commit | no restart: one `OVERLAY_OPEN`, one `PANEL_SLIDE` | | level fill retargets over 130 ms |
| Workspace indicator, 3 -> 1 | first commit 22 ms | 13 | 20 ms, **p50 16.6 ms** | commits at 60 Hz cadence on a 120 Hz output |
| Workspace indicator, 1 -> 3 | 22 ms | 2 | | no visible indicator animation in the nested run |

The OSD "first frame" here is dominated by the harness: the fake audio daemon reads levels by spawning a stand-in
`wpctl` after each event, so 160 ms is a floor that a cold or a prewarmed overlay both meet (below). The real key
path (`nacrectl key-step audio.step`) answers `Unavailable` in the private session (no PipeWire), so it could not be
driven; its first-frame number must come from the lead's live audit.

## Fix kept
- Overlay views (OSD, notices) never received the output refresh interval, so their in-view animations (`ui.animate`
  OSD fill, 130 ms) ticked at the 16 ms default (60 Hz) while panels/bar/windows/canvases already use the output
  rate. `open_overlay` now calls `frame_interval(output_frame_interval)` and an output mode change updates the shown
  overlay (`Overlay::output_mode_changed`). Not separately measurable in the nested run (the slide itself is
  `PANEL_SLIDE`, drawn by the overlay timer at `scroll::tick()`); logically it moves the fill tween to 120 Hz.
- Test: `overlay::tests::osd_and_toast_motion_commit_every_refresh_at_120_hz` (B0178 style): the shared motion
  selector gives exits a shorter accelerating curve; show >= 17 and hide >= 11 commits, max gap <= 9 ms, marker emitted.

## Attempt not kept (numbers, so nobody repeats it blindly)
Kept the overlay worker, renderer and last scene resident while closed (`Parked`, prewarmed 1.5 s after start,
`OVERLAY_OPEN ... warm=1`), gated the first draw on a scene built for the show, routed events to parked views, and
paced overlay frames by `wl_surface.frame` callbacks. Result against the baseline above: OSD first frame unchanged
(162-165 ms), but show frames fell from 25 to 17-21 and the max gap rose to 16-28 ms (toasts 16-27 ms). With callbacks
removed (timer cadence kept) the gap was still 26-28 ms, so the regression comes from the warm resume path (likely
the extra configure after the stale-sized surface or the late worker packet), not from the callbacks. Diff kept at
`.lead` only; the code was dropped. Prewarm memory was not measured (not kept).

## Open items for the lead
1. **Workspace indicator at 60 Hz cadence** (identical on base and candidate): the bar's `ui.animate` workspace
   capsule commits every ~16.6 ms on a 120 Hz output. The bar session's frame interval or the view-animation wake
   path (`Runtime::wakeup_at` / `animation_wakeup_after_panel_hold`, `bead-ui` session `preview_deadline`) is the
   suspect; the 1 -> 3 direction produced only 2 commits. Needs a trace of `UI_FRAME` on the bar surface.
2. **OSD first frame** on real input: measure live with `PANEL_SLIDE`/`OVERLAY_OPEN` journal markers before changing
   anything; the nested fake pipeline cannot separate session-build cost from daemon latency.
3. Warm overlay resume (above) needs the first-frame stall (26-28 ms) explained before it is worth its memory.

## Checks
- Final candidate (frame-interval fix only) against the baseline in the same harness: OSD show 161-162 ms / 25-26
  frames / 9-10.7 ms max gap, toast 4.6-4.8 ms / 25 frames / 11-12 ms, repeated OSD steps 4.8-6.7 ms, no regression;
  0 `UI_ERROR` in the real-shell run (real `shell/` tree, notice routes, `PANEL_SLIDE` markers above).
- Full Bead CI (ubuntu:24.04 recipe, prod): fmt, clippy `-D warnings`, boundary, workspace tests (35 suites ok),
  trace/feature checks, Luau analyze, spike/packaging tests, wallpaper smoke, services, private tray/clock buses,
  `cargo deny`: `FULL_CI_PASS`.
- Memory: none added (a test, one `frame_interval` call per shown overlay). No temporary duplicates.
