# B0189: Desktop widgets cost

Status: candidate. The release idle numbers below come from private nested sessions; the lead's live audit remains the acceptance.

## Cause

With a digital clock, an analog clock and a battery ring on the desktop canvas, the shell used about one full core at idle:

1. The analog clock animated three hands with `ui.animate(..., 1 s, "linear")`. Every tick started a new one-second animation, so the canvas view rebuilt and redrew at about 30 Hz forever. The second hand needs seconds ticks, so the clock-seconds lease was always held.
2. `canvas_draw` (bead-wayland) widened every buffer's damage to the whole surface on every draw. The canvas surface is the bounding box of all placed widgets, so each redraw repainted and re-damaged that whole box (1.94 Mpx here).
3. Each hand `Polyline` had the dial-sized box as its rect, so even with semantic damage a moving hand would damage the whole dial.

## Change

- `crates/bead-wayland/src/canvas.rs`: the canvas uses the same `FrameDamage` diff as bars and panels. Per-slot history carries the changed region (buffer-age safe, new buffers start full). A redraw with no pixel change commits nothing. A failed attempt that needs image preparation resets the diff so the retry repaints. `--trace-state` prints `CANVAS_DAMAGE rects= pixels= surface=`.
- `shell/widgets/clock/widget.luau`:
  - `analog` has hour and minute hands only and needs no seconds ticks. The hands ease for 0.4 s once per minute (about 24 frames), then nothing is scheduled.
  - New `analog-seconds` ("B2 · Analog + seconds") adds the second hand, which eases 0.3 s once per second and leases clock seconds. This is the only analog style with an always-on rate.
  - Hand angles are unwrapped (days since epoch, minutes of day, 12 h meridiem handled), so hands never sweep backwards at 12 or at minute 59. Animation keys are per instance (they were a shared global key, so two analog clocks fought).
  - Each hand's box hugs the hand, so a moving hand damages only its own pixels.
  - The dial glass, ticks SVG (cached by B0183) and date text are static items and are never repainted by hand motion. The hand strokes are two points each; caching rotated hands would cost more than redrawing them.
- `spikes/desktop-widgets-idle.py` + `spikes/desktop-widgets-layout.json`: the idle harness (below).
- The digital HH:MM clock and the battery ring already redraw only on data change (once per minute and on a battery change).

Removed/changed: the old global `wall-clock-*` animation keys; the old `analog` seconds hand moved to `analog-seconds`. A stored `analog` instance now shows no second hand (owner-visible; switch the style to `analog-seconds` to get it back).

## Evidence

Release build, CPU renderer, private nested niri on the desktop host, 2880x1800 scale 1.5, `spikes/desktop-widgets-idle.py` (60 s, `WIDGET_IDLE_QUIET=1` for CPU; the traced run gives the redraw columns and costs the same). Layout: digital clock, analog clock, battery ring. CPU is per mille of one core for the whole shell process; wakeups are context switches per second over all threads.

| Run | CPU ‰ core | ctx sw/s | canvas redraws/s | damage per redraw |
|---|---|---|---|---|
| Before (origin/main 6f6bde99, analog with seconds) | 1012 | 302 | 31.2 | 1 940 814 px (whole surface) |
| After, `analog` (default) | 46.5 | 24.8 | 1.2 (all zero-pixel: skipped) | 4.8 k px average |
| After, `analog-seconds` | 341 | 106 | 12.1 | 10.9 k px |
| Floor: shell with no desktop widgets | 16.5 | 14.8 | – | – |

Redraw cadence per widget: digital clock once per minute; analog once per minute (0.4 s ease); analog-seconds about 12 redraws/s (0.3 s ease per second, 40 fps effective); battery on change only.

Screenshots (`docs/specs/evidence/0189/`): `analog-before.png` and `analog-seconds-dark.png` (same hands geometry before/after; inspected: 08:26 and 08:39 hands read correctly), `bright-wallpaper-before-scrim.png` (see gaps).

## Tests

- `analog_clock_redraws_only_on_ticks_and_hands_hug_their_pixels` (nacre-shell, real shell tree and canvas entry): `analog` holds no seconds lease and schedules no frames when settled; a minute tick schedules an animation deadline within 450 ms; every hand box is under half the dial; `analog-seconds` leases seconds.
- Full workspace CI recipe on dev-2: see the report.

## Known gaps (not done)

- Adaptive scrim: not implemented. On a light wallpaper the digital clock and date are unreadable (see the screenshot); the analog dial is fine because of its glass. It needs a mean luminance of the active wallpaper once per change (computed in the decode helper, exposed as a store such as `bead.desktop.wallpaper.luma`), then tokens for text colour and shadow. Plumbing from `nacre-wallpaper-decode` to the widget stores was out of the time box.
- `pause_on` is not enforced anywhere (`budget.pause_on` is only validated by the registry). Without continuous animations, hidden widgets cost nothing except `analog-seconds`, which still takes 1 Hz seconds ticks and its tick animation while covered. Enforcing it needs the fullscreen/lock state in the canvas runtime (`wallpaper_motion` already derives it per output). Follow-up.
- About 1.2 canvas rebuilds/s remain with zero pixel change (about 30 ‰ over the floor); the store churn that triggers them was not identified. `analog-seconds` costs about 28 ms CPU per eased frame in the nested software renderer; a shorter tick ease or a hard tick (1 redraw/s) would cut it. Live audit decides.
- `widgets/registry.luau` was not touched (B0186).

No temporary duplicates were added.
