# B0192: workspace-switch cadence and OSD first frame

Status: **Candidate.** Two fixes from the B0188 open items, measured on the desktop TEST host (private labwc
2880x1800@120 > niri scale 1.5, release build of `nacre-shell`, default renderer, fake system daemon,
`tools/daily-acceptance/animation_audit.py`; one `workspace`/`osd-a` run per arm, repeats noted). The owner's laptop
live audit stays the acceptance.

## Findings
1. **Workspace indicator at 60 Hz.** The bar session got `output_frame_interval` = the advertised `wl_output` mode.
   The nested niri window advertises a fixed 60 Hz mode (`OUTPUT_FRAME_INTERVAL origin=mode ns=16666666`) while its
   clients are paced by the 120 Hz parent, and its presentation feedback carries `refresh=0`, so `output_presented`
   never corrected it: `ui.animate` deadlines were 16.6 ms apart (`UI_COMMIT` every 16.5 ms, 13 commits per switch).
   Fix: until presentation feedback names the real refresh, the mode is not trusted below 120 Hz
   (`UNCONFIRMED_TICK_CAP`, the same 120 Hz opportunity `scroll::tick` keeps for unknown refresh). Every slot
   (bar sessions, panel, overlay, canvas, window) shares `output_frame_interval`, so all get it; a confirmed
   presentation refresh still sets the exact interval.
2. **1 -> 3 direction.** Not reproduced on current main: both directions committed 13 frames before and 24 after
   (3 repeats each). The B0188 "2 commits" run was most likely the harness' first switch after a cold start.
3. **OSD first frame.** `OVERLAY_FIRST_FRAME view= create_ms scene_ms configure_ms commit_ms render_ms` is now logged
   once per show (offer to first content commit). Cold, before: create 0.1, first worker scene 38.6, commit 40.0 ms.
   The first prepare ran at scale 2: the overlay started from the integer `wl_output` factor (2 at 1.5x) and only a
   later fractional-scale event corrected it, so the whole OSD (including the 30 ms shadow template, `shadow_ms=29.5`
   in `WORKER_PREPARE total_ms=36.8`) was rastered at the wrong scale first. The overlay now starts at the fractional
   scale its output's own surfaces already use. After: scene 8.4, commit 10.1 ms (two runs: 10.1/10.2), with no
   prewarm, so no resident memory. This also explains the 26-28 ms stall B0188's parked overlay saw: its resume
   reset the scale to the integer factor and paid the scale-2 shadow template again (hypothesis consistent with the
   numbers; the parked variant was not rebuilt).
4. The harness' "first=162 ms" for OSD show is unchanged: it includes the fake daemon's per-event `wpctl` spawn. The
   host side is the `OVERLAY_FIRST_FRAME` line; the real-key first frame needs the lead's live journal.

## Results
| measurement | before (base a425b615) | after |
|---|---|---|
| workspace switch commits / p50 / max gap, 120 Hz, 3 repeats, both directions | 13 / 16.6-16.7 / 17.5-18.0 ms | 24 / 8.3 / 8.8-9.2 ms |
| OSD cold offer -> first content commit | 40.0 ms | 10.1 ms |
| OSD show frames / max gap (osd-a, osd-b) | 22-23 / 38.6 ms | 25 / 9.3 ms |
| `UI_ERROR` in the real-shell runs (real `shell/` tree) | 0 | 0 |

Target check: OSD first frame <= 17 ms after first use and <= 50 ms cold are both met by the cold path (10 ms), so
no warm/parked overlay was added (memory cost 0).

## Trade-off (recorded)
On a genuine 60 Hz output whose compositor reports `refresh=0`, the cap stages twice per frame. Measured with a
60 Hz parent: workspace commits p50 13.8 ms, one 28 ms gap (before 16.7 / 17.6). Compositors that report refresh
(niri on DRM does) get the exact interval. Open item: a frame-callback-driven tick for view animations (host sends
the worker a tick on each callback while the last packet reported an animation deadline) would need no refresh
knowledge at all; not built in this unit's time box.

## Tests
- `service_views_tests::workspace_switch_ticks_every_refresh_at_120_hz_in_both_directions` (real `bar/main.luau`
  workspaces widget, 8.33 ms session interval, 3->1 and 1->3: >= 16 frames, scheduled gaps <= 1 refresh).
- `runtime::palette_barrier_tests::output_refresh_millihertz_maps_to_deadline_period` extended with the cap.

## Not done / follow-ups
- Panel/window/canvas surfaces also start from the integer scale factor on fractional outputs; only the overlay was
  changed here (the OSD/toast path).
- Memory: none added. No temporary duplicates.
